//! The §29 gate-1 **independent second implementation** of the PALW canonical reference
//! arithmetic (`kaspa_consensus_core::palw_reference`, ADR-0027 §2).
//!
//! # Why this crate exists
//!
//! The PALW activation gate requires the reference arithmetic to be "independently implemented
//! twice", and `palw_reference` itself documents that its in-crate hardware-FPU oracle does
//! **not** count as the second implementation. This crate is that second implementation: the
//! same frozen IEEE-754 binary32 semantics, computed by **Berkeley SoftFloat Release 3e** by
//! John R. Hauser — an implementation of independent authorship, written years before this
//! project, with a completely different internal structure (magnitude-dispatch add/sub, a
//! table-driven CLZ, 7-bit round/sticky frames) from `palw_reference`'s 3-bit GRS soft-float.
//! Agreement between the two is therefore evidence about IEEE-754 conformance, not about a
//! shared bug. The vendored C sources are byte-identical to upstream (see `vendor/softfloat/`
//! and the crate README for the exact commit); like `palw_reference`, SoftFloat is pure integer
//! code — no value ever touches a hardware float register in either implementation.
//!
//! # The canonicalization contract imposed on top
//!
//! Raw SoftFloat implements the 8086-SSE NaN rules (payload propagation, default NaN
//! `0xFFC00000`). The frozen PALW ruleset instead demands: **every NaN operand or NaN result
//! canonicalizes to `0x7FC00000`**, and negation flips the sign bit with any NaN input becoming
//! the canonical NaN. The wrappers here impose exactly that contract:
//!
//! * `ref2_add` / `ref2_mul`: if either **operand** is any NaN, return `0x7FC00000` *before*
//!   SoftFloat is called (so SoftFloat's payload-propagation rules can never be observed);
//!   otherwise call SoftFloat and canonicalize a NaN **result** (invalid operations such as
//!   `+Inf + -Inf` or `0 × Inf`) to `0x7FC00000`.
//! * `ref2_neg`: pure Rust sign-bit flip; any NaN input returns the canonical NaN.
//! * `ref2_sub(a, b) = ref2_add(a, ref2_neg(b))` — the same literal identity the normative
//!   module uses. (`ref2_sub_direct` additionally exposes SoftFloat's own `f32_sub` under the
//!   same contract, so tests can confirm the identity holds inside SoftFloat too.)
//! * `ref2_dot` / `ref2_gemm`: the same pinned reduction — accumulator starts at `+0.0` and
//!   folds strictly k-ascending, `acc = add(acc, mul(a[k], b[k]))`; GEMM is
//!   `C[i][j] = dot(row_i(A), col_j(B))` with `C` row-major.
//!
//! # Thread safety
//!
//! SoftFloat keeps process-global state: `softfloat_roundingMode`, `softfloat_detectTininess`
//! and `softfloat_exceptionFlags` (`softfloat_state.c`; we build without `THREAD_LOCAL`).
//! Every wrapper serializes access through one global mutex and pins
//! `softfloat_roundingMode = softfloat_round_near_even` (0) on entry, so concurrent test
//! threads can never race the mode or the `exceptionFlags |=` read-modify-write inside
//! `softfloat_raiseFlags`. Exception flags are deliberately ignored — only result bit patterns
//! are compared; tininess detection (`afterRounding`, the 8086-SSE default) affects only the
//! underflow *flag*, never a result value.
//!
//! # This crate is test/verification-only
//!
//! It exists to differentially test `palw_reference` and for nothing else. It must **NEVER**
//! be linked into consensus, mining, validation, or any other production path: the normative
//! implementation of the frozen ruleset is `kaspa_consensus_core::palw_reference` alone, and
//! a second arithmetic in a consensus binary is a fork risk, not a feature. Enforced
//! structurally: no workspace crate may depend on this one (it depends on
//! `kaspa-consensus-core` only as a dev-dependency, and nothing depends on it).

use std::sync::Mutex;

/// The canonical quiet NaN — must stay bit-identical to
/// `palw_reference::PALW_REFERENCE_CANONICAL_NAN_V1` (asserted by the differential tests).
pub const REF2_CANONICAL_NAN: u32 = 0x7FC0_0000;

const SIGN_MASK: u32 = 0x8000_0000;
const ABS_MASK: u32 = 0x7FFF_FFFF;
const EXP_MASK: u32 = 0x7F80_0000;

/// `softfloat_round_near_even` from softfloat.h — the only mode this crate ever sets.
const SOFTFLOAT_ROUND_NEAR_EVEN: u8 = 0;

/// SoftFloat's `float32_t`: a single-member struct wrapping the raw bit pattern
/// (`typedef struct { uint32_t v; } float32_t;` in softfloat_types.h). Passed and returned
/// by value; `#[repr(C)]` on a 4-byte struct matches the C ABI on all supported targets.
#[repr(C)]
#[derive(Clone, Copy)]
struct Float32T {
    v: u32,
}

unsafe extern "C" {
    fn f32_add(a: Float32T, b: Float32T) -> Float32T;
    fn f32_sub(a: Float32T, b: Float32T) -> Float32T;
    fn f32_mul(a: Float32T, b: Float32T) -> Float32T;
    /// `uint_fast8_t` in C — one byte on every supported target (Darwin, glibc, musl all
    /// typedef it to `unsigned char`/`uint8_t`). We only ever store 0 (round_near_even),
    /// which is also the static initializer in softfloat_state.c, so even a hypothetical
    /// width mismatch could not corrupt the mode.
    static mut softfloat_roundingMode: u8;
}

/// Serializes every touch of the SoftFloat globals (see "Thread safety" above).
static SOFTFLOAT_LOCK: Mutex<()> = Mutex::new(());

#[inline]
fn is_nan_bits(bits: u32) -> bool {
    (bits & ABS_MASK) > EXP_MASK
}

#[inline]
fn canon_result(bits: u32) -> u32 {
    if is_nan_bits(bits) { REF2_CANONICAL_NAN } else { bits }
}

/// Runs `f` with the SoftFloat global state locked and the rounding mode pinned to RNE.
fn with_softfloat<R>(f: impl FnOnce() -> R) -> R {
    let _guard = SOFTFLOAT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // Pin RNE on every entry: a single byte store, and it makes the wrapper immune to any
    // other in-process code that might have touched the global.
    unsafe { softfloat_roundingMode = SOFTFLOAT_ROUND_NEAR_EVEN };
    f()
}

/// Canonical-contract add over SoftFloat. Must only run while `SOFTFLOAT_LOCK` is held.
#[inline]
fn add_locked(a: u32, b: u32) -> u32 {
    if is_nan_bits(a) || is_nan_bits(b) {
        return REF2_CANONICAL_NAN;
    }
    canon_result(unsafe { f32_add(Float32T { v: a }, Float32T { v: b }) }.v)
}

/// Canonical-contract mul over SoftFloat. Must only run while `SOFTFLOAT_LOCK` is held.
#[inline]
fn mul_locked(a: u32, b: u32) -> u32 {
    if is_nan_bits(a) || is_nan_bits(b) {
        return REF2_CANONICAL_NAN;
    }
    canon_result(unsafe { f32_mul(Float32T { v: a }, Float32T { v: b }) }.v)
}

/// binary32 addition under the canonical contract (second implementation of `ref_add_v1`).
pub fn ref2_add(a: u32, b: u32) -> u32 {
    with_softfloat(|| add_locked(a, b))
}

/// binary32 multiplication under the canonical contract (second implementation of `ref_mul_v1`).
pub fn ref2_mul(a: u32, b: u32) -> u32 {
    with_softfloat(|| mul_locked(a, b))
}

/// Canonical negate: sign-bit flip; any NaN becomes the canonical NaN (second implementation
/// of `ref_neg_v1`). Pure Rust — negation is a bit operation, not arithmetic.
pub fn ref2_neg(bits: u32) -> u32 {
    if is_nan_bits(bits) {
        return REF2_CANONICAL_NAN;
    }
    bits ^ SIGN_MASK
}

/// binary32 subtraction as the literal identity `add(a, neg(b))` — the same definition
/// `ref_sub_v1` pins, so both implementations share one rounding path for subtraction.
pub fn ref2_sub(a: u32, b: u32) -> u32 {
    ref2_add(a, ref2_neg(b))
}

/// Subtraction through SoftFloat's own `f32_sub` under the same canonical contract.
/// Not part of the mirrored API surface — it exists so the differential tests can verify
/// that the `sub = add ∘ neg` identity also holds inside the independent implementation.
pub fn ref2_sub_direct(a: u32, b: u32) -> u32 {
    with_softfloat(|| {
        if is_nan_bits(a) || is_nan_bits(b) {
            return REF2_CANONICAL_NAN;
        }
        canon_result(unsafe { f32_sub(Float32T { v: a }, Float32T { v: b }) }.v)
    })
}

/// Canonical dot product (second implementation of `ref_dot_v1`): the accumulator starts at
/// `+0.0` and folds strictly k-ascending, `acc = add(acc, mul(a[k], b[k]))`.
///
/// Shape discipline is by `assert!` — this crate is test-only, and the differential tests
/// only ever present the shapes the normative API accepts (its `Result` error surface is not
/// part of the arithmetic under test).
///
/// # Panics
/// If the vectors are empty or their lengths differ.
pub fn ref2_dot(a: &[u32], b: &[u32]) -> u32 {
    assert!(!a.is_empty(), "empty operand");
    assert_eq!(a.len(), b.len(), "length mismatch");
    with_softfloat(|| {
        let mut acc = 0u32; // +0.0
        for (&x, &y) in a.iter().zip(b.iter()) {
            acc = add_locked(acc, mul_locked(x, y));
        }
        acc
    })
}

/// Canonical GEMM tile (second implementation of `ref_gemm_v1`): `A` row-major `m×k`,
/// `B` row-major `k×n`, `C[i][j] = dot(row_i(A), col_j(B))`, `C` row-major, iteration
/// i-major then j — the same pinned structure as the normative implementation.
///
/// # Panics
/// If any dimension is zero or an input slice length does not match its `m·k` / `k·n` shape.
pub fn ref2_gemm(a: &[u32], b: &[u32], m: usize, n: usize, k: usize) -> Vec<u32> {
    assert!(m != 0 && n != 0 && k != 0, "gemm dimension is zero");
    assert_eq!(a.len(), m * k, "matrix a length");
    assert_eq!(b.len(), k * n, "matrix b length");
    with_softfloat(|| {
        let mut out = Vec::with_capacity(m * n);
        let mut column = vec![0u32; k];
        for i in 0..m {
            let row = &a[i * k..(i + 1) * k];
            for j in 0..n {
                for (kk, slot) in column.iter_mut().enumerate() {
                    *slot = b[kk * n + j];
                }
                let mut acc = 0u32; // +0.0
                for (&x, &y) in row.iter().zip(column.iter()) {
                    acc = add_locked(acc, mul_locked(x, y));
                }
                out.push(acc);
            }
        }
        out
    })
}

#[cfg(test)]
mod linkage_smoke {
    //! A tiny canary so a broken C build fails loudly and readably before the
    //! multi-million-case differential sweeps (tests/differential.rs) run.
    use super::*;

    #[test]
    fn softfloat_links_and_computes() {
        let one = 0x3F80_0000;
        let two = 0x4000_0000;
        assert_eq!(ref2_add(one, one), two);
        assert_eq!(ref2_mul(two, two), 0x4080_0000);
        assert_eq!(ref2_sub(one, one), 0);
        assert_eq!(ref2_sub_direct(one, one), 0);
        assert_eq!(ref2_neg(one), one | SIGN_MASK);
        assert_eq!(ref2_dot(&[one, one], &[one, one]), two);
        assert_eq!(ref2_gemm(&[one], &[one], 1, 1, 1), vec![one]);
        // Invalid operations mint ONLY the canonical NaN (8086-SSE default NaN is
        // 0xFFC00000 — it must never escape the wrapper).
        let inf = 0x7F80_0000;
        assert_eq!(ref2_add(inf, inf | SIGN_MASK), REF2_CANONICAL_NAN);
        assert_eq!(ref2_mul(inf, 0), REF2_CANONICAL_NAN);
    }
}
