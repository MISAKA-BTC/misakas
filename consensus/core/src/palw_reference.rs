//! PALW canonical reference arithmetic v1 — the adjudication arithmetic of ADR-0027 §2.
//!
//! Normative sources: `docs/adr/0027-palw-slash-unilateral-fraud-proofs.md` §2 ("one step,
//! canonical reference arithmetic, reproduced by every node") and the v0.1 slash spec §5.2's
//! prohibition on hashing or comparing raw runtime floats.
//!
//! # What this module is — and is not
//!
//! This is the **arithmetic** a future one-step check is defined in: IEEE-754 binary32 add /
//! sub / mul / negate, a dot product with a pinned reduction order, and a GEMM tile defined over
//! that dot product — implemented in **pure integer arithmetic** (soft-float). No value in the
//! normative path ever touches a hardware float register, so the result cannot depend on MXCSR /
//! FPCR state, compiler contraction, or target quirks. The hardware-float twin used to verify
//! this implementation exists **only inside the test module**, where the test runner's clean FP
//! environment makes it an IEEE-754 oracle.
//!
//! It is deliberately **not** the step function. Which operator, which tile shape, which layer —
//! that is `shape_profile_id` pinning work (ADR-0027 consequences) and does not exist yet.
//! Transcendental functions (exp, sqrt, division — needed for softmax/norm challenges) are also
//! deliberately absent: IEEE-754 does not require correctly-rounded libm, so hardware `exp`
//! differs across platforms, and pinning canonical implementations of them is its own
//! ADR-grade decision. Nothing here may be extended with a transcendental "for convenience".
//!
//! # The rules (frozen; [`reference_arithmetic_ruleset_id_v1`] is their identity)
//!
//! * binary32 throughout; every input and output is a raw little-endian bit pattern (`u32`) —
//!   the same bytes the v2 trace commits, so adjudication never converts representations.
//! * round-to-nearest-ties-to-even, the only IEEE-754 mode the v2 canonical policy admits.
//! * subnormals are exact: no FTZ, no DAZ — by construction, since there is no FPU involved.
//! * no fused multiply-add anywhere: every multiply rounds, then every add rounds.
//! * **every NaN operand or NaN result canonicalizes to [`PALW_REFERENCE_CANONICAL_NAN_V1`]**
//!   (`0x7FC00000`). NaN payloads are the one place IEEE-754 hardware is *legitimately*
//!   nondeterministic across vendors, so no payload may ever reach a committed byte.
//! * signed zeros follow IEEE-754 exactly: `(+0) + (−0) = +0`, exact cancellation yields `+0`,
//!   multiplication signs are XOR — RNE semantics, bit-frozen by the golden tests.
//! * `dot`: the accumulator starts at `+0.0` and folds strictly k-ascending —
//!   `acc = add(acc, mul(a[k], b[k]))`. No pairwise trees, no partial sums, no reassociation.
//!   The order-witness test pins a vector whose ascending and descending sums differ, so an
//!   implementation that reorders cannot pass.
//! * `gemm`: `C[i][j] = dot(row_i(A), col_j(B))`, `C` row-major. Each output element is an
//!   independent dot; the iteration order is pinned for streaming only and cannot change values.
//!
//! Arithmetic here is **total**: ±Inf and the canonical NaN are ordinary results (IEEE-754
//! defines them), and [`ref_is_finite_v1`] is provided for the *policy* layers — the v2
//! fail-closed rule (non-finite ⇒ no receipt) lives above the arithmetic, not inside it.
//!
//! # Verification stance (ADR-0027: "independently implemented twice")
//!
//! The tests cross-check every operation against the hardware FPU under a clean environment —
//! a full ±38-value special matrix (zeros, subnormal boundaries, hidden-bit boundaries, tie
//! neighborhoods, carry/borrow edges, overflow edge, infinities, NaN payloads) plus wide
//! deterministic random sweeps, including a tie-hunting sweep with sparse low bits. That makes
//! the hardware a *test oracle*, not a second implementation in the v0.1 §29 gate-1 sense: the
//! gate's independent second implementation (e.g. a Berkeley-SoftFloat cross-build) remains
//! open work and MUST NOT be marked satisfied by this module's own tests.

use kaspa_hashes::Hash64;
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------------------------

/// The frozen human-readable rule text. Changing ANY behavior of this module is a new ruleset
/// string, a new id, and a new module version — never an in-place edit.
pub const PALW_REFERENCE_ARITHMETIC_RULESET_V1: &str = "IEEE-754 binary32; round-to-nearest-ties-to-even; subnormals exact (no FTZ/DAZ); no fused multiply-add; every NaN operand or NaN result is the canonical quiet NaN 0x7FC00000; negation flips the sign bit; sub(a,b) = add(a, neg(b)); dot(a,b): acc = +0.0, then for k ascending acc = add(acc, mul(a[k], b[k])); gemm: C[i][j] = dot(row i of A, column j of B), C row-major; v1";

/// Keyed-BLAKE2b-512 domain of [`reference_arithmetic_ruleset_id_v1`].
pub const PALW_REFERENCE_DOMAIN_RULESET: &[u8] = b"misaka-palw/reference-arithmetic-ruleset/v1";

/// The **additive** ruleset v2 (ADR-0030 §4): everything in v1, unchanged, plus the operations
/// the pinned class kernels are actually built from and v1 deliberately excluded — fused
/// multiply-add (the repack gemv seam, Fact 3/11), correctly-rounded division and square root
/// (IEEE-754 basic operations; softmax/norm scale paths), binary64 add/mul/div (the `double`
/// accumulators of rms_norm / l2_norm / soft_max, Fact 6), exact f32→f64 widening, RNE f64→f32
/// narrowing, and binary16 conversions (the F16 KV-cache seam). v1's string, id and behavior do
/// not move; a profile that needs fma binds THIS id.
pub const PALW_REFERENCE_ARITHMETIC_RULESET_V2: &str = "extends misaka-palw reference arithmetic v1 unchanged (IEEE-754 binary32; RNE; subnormals exact; canonical NaN 0x7FC00000; pinned k-ascending dot; gemm over dot) with: fma(a,b,c) = RNE(a*b + c) fused, one rounding; div(a,b) and sqrt(a) correctly rounded; binary64 add/sub/mul/div, RNE, subnormals exact, canonical NaN 0x7FF8000000000000, negation flips the sign bit, sub64(a,b) = add64(a, neg64(b)); widen f32->f64 exact; narrow f64->f32 RNE; binary16: f16->f32 exact, f32->f16 RNE, canonical NaN 0x7E00; every NaN operand or NaN result canonicalizes in its own width; v2";

/// Keyed-BLAKE2b-512 domain of [`reference_arithmetic_ruleset_id_v2`].
pub const PALW_REFERENCE_DOMAIN_RULESET_V2: &[u8] = b"misaka-palw/reference-arithmetic-ruleset/v2";

/// Every domain this module introduces (uniqueness-tested against the v2 and PALW-S lists).
pub const PALW_REFERENCE_ALL_DOMAINS: &[&[u8]] = &[PALW_REFERENCE_DOMAIN_RULESET, PALW_REFERENCE_DOMAIN_RULESET_V2];

/// The identity a shape profile binds to say "steps under this profile are adjudicated in this
/// arithmetic".
pub fn reference_arithmetic_ruleset_id_v1() -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_REFERENCE_DOMAIN_RULESET).to_state();
    h.update(PALW_REFERENCE_ARITHMETIC_RULESET_V1.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The ruleset-v2 identity (see [`PALW_REFERENCE_ARITHMETIC_RULESET_V2`]).
pub fn reference_arithmetic_ruleset_id_v2() -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_REFERENCE_DOMAIN_RULESET_V2).to_state();
    h.update(PALW_REFERENCE_ARITHMETIC_RULESET_V2.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The canonical quiet NaN. The only NaN bit pattern this arithmetic can ever emit.
pub const PALW_REFERENCE_CANONICAL_NAN_V1: u32 = 0x7FC0_0000;

/// The canonical binary64 quiet NaN (ruleset v2).
pub const PALW_REFERENCE_CANONICAL_NAN64_V2: u64 = 0x7FF8_0000_0000_0000;

/// The canonical binary16 quiet NaN (ruleset v2).
pub const PALW_REFERENCE_CANONICAL_NAN16_V2: u16 = 0x7E00;

// ---------------------------------------------------------------------------------------------
// Caps and errors
// ---------------------------------------------------------------------------------------------

/// Longest vector [`ref_dot_v1`] accepts. Generous against any plausible challenge tile
/// (the measured full logit row is 248 320), tight against adversarial allocation.
pub const PALW_REFERENCE_MAX_DOT_LEN: usize = 1 << 20;
/// Largest single GEMM dimension [`ref_gemm_v1`] accepts.
pub const PALW_REFERENCE_MAX_GEMM_DIM: usize = 4096;
/// Largest GEMM output element count (m·n).
pub const PALW_REFERENCE_MAX_GEMM_OUT: usize = 1 << 20;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwReferenceError {
    #[error("operand vectors are empty")]
    EmptyOperand,
    #[error("operand lengths differ: {a} vs {b}")]
    LengthMismatch { a: usize, b: usize },
    #[error("vector length {got} exceeds the {max}-element cap")]
    VectorTooLong { got: usize, max: usize },
    #[error("gemm dimension is zero")]
    DimensionZero,
    #[error("gemm dimension {got} exceeds the {max} cap")]
    DimensionTooLarge { got: usize, max: usize },
    #[error("matrix a has {got} elements, expected m*k = {expected}")]
    MatrixALengthMismatch { got: usize, expected: usize },
    #[error("matrix b has {got} elements, expected k*n = {expected}")]
    MatrixBLengthMismatch { got: usize, expected: usize },
    #[error("gemm output m*n = {got} exceeds the {max}-element cap")]
    OutputTooLarge { got: usize, max: usize },
}

// ---------------------------------------------------------------------------------------------
// binary32 soft-float core (pure integer; no `f32` appears anywhere below this line outside
// the test module)
// ---------------------------------------------------------------------------------------------

const SIGN_MASK: u32 = 0x8000_0000;
const ABS_MASK: u32 = 0x7FFF_FFFF;
const EXP_MASK: u32 = 0x7F80_0000;
const FRAC_MASK: u32 = 0x007F_FFFF;
const HIDDEN_BIT: u32 = 0x0080_0000; // 2^23

#[inline]
fn exp_field(bits: u32) -> u32 {
    (bits & EXP_MASK) >> 23
}

#[inline]
fn is_nan_bits(bits: u32) -> bool {
    (bits & ABS_MASK) > EXP_MASK
}

#[inline]
fn is_inf_bits(bits: u32) -> bool {
    (bits & ABS_MASK) == EXP_MASK
}

#[inline]
fn is_zero_bits(bits: u32) -> bool {
    (bits & ABS_MASK) == 0
}

/// True iff `bits` is neither NaN nor ±Inf. For the fail-closed policy layers.
pub fn ref_is_finite_v1(bits: u32) -> bool {
    (bits & EXP_MASK) != EXP_MASK
}

/// Right shift that ORs every shifted-out bit into the result's lowest bit ("jamming"). With
/// three rounding bits kept, jamming preserves round-to-nearest-even exactly: a rounding tie
/// requires the sticky region to be all-zero, which a jam can never fake.
#[inline]
fn shift_right_jam(value: u64, shift: u32) -> u64 {
    if shift == 0 {
        value
    } else if shift < 64 {
        let kept = value >> shift;
        let lost = value & ((1u64 << shift) - 1);
        kept | u64::from(lost != 0)
    } else {
        u64::from(value != 0)
    }
}

/// Decomposes non-zero finite `bits` into `(significand, biased_exponent)` with
/// `value = significand · 2^(biased_exponent − 150)` and `significand ∈ [2^23, 2^24)`.
/// Subnormal inputs are normalized (their exponent goes ≤ 0), so every caller sees one frame.
#[inline]
fn decompose_finite_nonzero(bits: u32) -> (u32, i32) {
    let exp = exp_field(bits);
    let frac = bits & FRAC_MASK;
    if exp == 0 {
        // Subnormal: value = frac · 2^(1−150). Normalize into the hidden-bit frame.
        let shift = frac.leading_zeros() - 8; // brings the top set bit to bit 23
        ((frac << shift), 1 - shift as i32)
    } else {
        (frac | HIDDEN_BIT, exp as i32)
    }
}

/// Rounds and packs `(sign, biased_exponent, sig_grs)` where
/// `value = (sig_grs / 8) · 2^(biased_exponent − 150)` and `sig_grs ∈ [2^26, 2^27)` for normal
/// results (callers may pass smaller magnitudes only via the subnormal path, i.e. exponent ≤ 0).
/// The low three bits of `sig_grs` are guard/round/sticky. Handles gradual underflow, round-up
/// into the normal range, and RNE overflow to ±Inf.
fn round_pack(sign: u32, mut exp: i32, mut sig_grs: u64) -> u32 {
    debug_assert!(sig_grs < (1 << 28));
    if exp <= 0 {
        // Denormalize: bring the exponent up to 1 (the subnormal frame), jamming what falls off.
        sig_grs = shift_right_jam(sig_grs, (1 - exp) as u32);
        exp = 1;
    }
    // RNE on the three low bits: round up iff guard && (sticky || lsb-odd).
    let round_bits = (sig_grs & 7) as u32;
    let mut sig = (sig_grs >> 3) as u32;
    if round_bits > 4 || (round_bits == 4 && (sig & 1) == 1) {
        sig += 1;
        if sig == 1 << 24 {
            // Rounding carried out of the significand: exact power of two, one binade up.
            sig = 1 << 23;
            exp += 1;
        }
    }
    if sig < HIDDEN_BIT {
        // Still below the hidden bit after rounding: a true subnormal (or zero). exp is 1 here,
        // and the encoding for subnormals is exponent field 0 with the raw fraction.
        return sign | sig;
    }
    if exp >= 255 {
        return sign | EXP_MASK; // ±Inf (RNE overflow)
    }
    sign | ((exp as u32) << 23) | (sig & FRAC_MASK)
}

/// Canonical negate: flips the sign bit; any NaN becomes THE canonical NaN.
pub fn ref_neg_v1(bits: u32) -> u32 {
    if is_nan_bits(bits) {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    bits ^ SIGN_MASK
}

/// Canonical binary32 addition (RNE, subnormals exact, canonical NaN).
pub fn ref_add_v1(a: u32, b: u32) -> u32 {
    if is_nan_bits(a) || is_nan_bits(b) {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    match (is_inf_bits(a), is_inf_bits(b)) {
        (true, true) => {
            return if (a ^ b) & SIGN_MASK == 0 { a } else { PALW_REFERENCE_CANONICAL_NAN_V1 };
        }
        (true, false) => return a,
        (false, true) => return b,
        (false, false) => {}
    }
    match (is_zero_bits(a), is_zero_bits(b)) {
        // IEEE-754 RNE: (−0)+(−0) = −0; every other zero pairing is +0.
        (true, true) => return if a == b { a } else { 0 },
        (true, false) => return b,
        (false, true) => return a,
        (false, false) => {}
    }

    // Order by magnitude; for same-format floats, |x| order is the integer order of abs bits.
    let (big, small) = if (a & ABS_MASK) >= (b & ABS_MASK) { (a, b) } else { (b, a) };
    let (sig_b, exp_b) = decompose_finite_nonzero(big);
    let (sig_s, exp_s) = decompose_finite_nonzero(small);
    let sign_big = big & SIGN_MASK;
    let delta = (exp_b - exp_s) as u32; // ≥ 0 by the magnitude ordering

    let big_grs = (sig_b as u64) << 3;
    let small_grs = shift_right_jam((sig_s as u64) << 3, delta);

    if (big ^ small) & SIGN_MASK == 0 {
        // Same sign: magnitudes add. Sum ∈ [2^26, 2^28); fold a carry back into the frame.
        let mut sum = big_grs + small_grs;
        let mut exp = exp_b;
        if sum >= 1 << 27 {
            sum = shift_right_jam(sum, 1);
            exp += 1;
        }
        round_pack(sign_big, exp, sum)
    } else {
        // Opposite signs: magnitudes subtract; the sign of the larger magnitude wins.
        let mut diff = big_grs - small_grs;
        if diff == 0 {
            // Exact cancellation is +0 under RNE.
            return 0;
        }
        let mut exp = exp_b;
        // Normalize back into [2^26, 2^27). When delta ≤ 1 the alignment was exact, so the
        // left shift pulls in true zeros; when delta ≥ 2 the result is ≥ 2^25 and shifts at
        // most once, keeping the jammed sticky in place.
        let shift = 26u32.saturating_sub(63 - diff.leading_zeros());
        diff <<= shift;
        exp -= shift as i32;
        round_pack(sign_big, exp, diff)
    }
}

/// Canonical binary32 subtraction: exactly `add(a, neg(b))` (that identity IS the IEEE-754
/// definition, and keeping it literal means there is one rounding path to audit, not two).
pub fn ref_sub_v1(a: u32, b: u32) -> u32 {
    ref_add_v1(a, ref_neg_v1(b))
}

/// Canonical binary32 multiplication (RNE, subnormals exact, canonical NaN, no FMA — this
/// rounds; whatever consumes the product rounds again).
pub fn ref_mul_v1(a: u32, b: u32) -> u32 {
    if is_nan_bits(a) || is_nan_bits(b) {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    let sign = (a ^ b) & SIGN_MASK;
    match (is_inf_bits(a), is_inf_bits(b)) {
        (true, true) => return sign | EXP_MASK,
        (true, false) => return if is_zero_bits(b) { PALW_REFERENCE_CANONICAL_NAN_V1 } else { sign | EXP_MASK },
        (false, true) => return if is_zero_bits(a) { PALW_REFERENCE_CANONICAL_NAN_V1 } else { sign | EXP_MASK },
        (false, false) => {}
    }
    if is_zero_bits(a) || is_zero_bits(b) {
        return sign;
    }

    let (sig_a, exp_a) = decompose_finite_nonzero(a);
    let (sig_b, exp_b) = decompose_finite_nonzero(b);
    // Exact 48-bit product: (sig_a·sig_b) ∈ [2^46, 2^48), value = product · 2^(exp_a+exp_b−300).
    let mut product = (sig_a as u64) * (sig_b as u64);
    // Target frame is value = (sig_grs/8) · 2^(exp−150) with sig_grs ∈ [2^26, 2^27):
    // product · 2^(E−300) = (sig_grs/8) · 2^(exp−150) with sig_grs = product >> 20 requires
    // exp = E − 127; one extra shift if the product crossed 2^47.
    let mut exp = exp_a + exp_b - 127;
    if product >= 1 << 47 {
        product = shift_right_jam(product, 21);
        exp += 1;
    } else {
        product = shift_right_jam(product, 20);
    }
    round_pack(sign, exp, product)
}

// ---------------------------------------------------------------------------------------------
// Pinned-order reductions
// ---------------------------------------------------------------------------------------------

/// Canonical dot product: `acc = +0.0; for k ascending: acc = add(acc, mul(a[k], b[k]))`.
/// Total like the scalar ops — an overflowing accumulation is ±Inf (or canonical NaN after
/// opposing infinities), exactly as the rule text says; policy layers decide what a non-finite
/// canonical result *means*.
pub fn ref_dot_v1(a: &[u32], b: &[u32]) -> Result<u32, PalwReferenceError> {
    if a.is_empty() {
        return Err(PalwReferenceError::EmptyOperand);
    }
    if a.len() != b.len() {
        return Err(PalwReferenceError::LengthMismatch { a: a.len(), b: b.len() });
    }
    if a.len() > PALW_REFERENCE_MAX_DOT_LEN {
        return Err(PalwReferenceError::VectorTooLong { got: a.len(), max: PALW_REFERENCE_MAX_DOT_LEN });
    }
    let mut acc = 0u32; // +0.0
    for (x, y) in a.iter().zip(b.iter()) {
        acc = ref_add_v1(acc, ref_mul_v1(*x, *y));
    }
    Ok(acc)
}

/// Canonical GEMM tile: `C[i][j] = dot(row_i(A), col_j(B))`, `A` row-major `m×k`, `B` row-major
/// `k×n`, `C` row-major `m×n`. Every element is an independent [`ref_dot_v1`]; iteration order
/// is pinned (i-major, then j) for streaming only and cannot affect any value.
pub fn ref_gemm_v1(a: &[u32], b: &[u32], m: usize, n: usize, k: usize) -> Result<Vec<u32>, PalwReferenceError> {
    if m == 0 || n == 0 || k == 0 {
        return Err(PalwReferenceError::DimensionZero);
    }
    for dim in [m, n, k] {
        if dim > PALW_REFERENCE_MAX_GEMM_DIM {
            return Err(PalwReferenceError::DimensionTooLarge { got: dim, max: PALW_REFERENCE_MAX_GEMM_DIM });
        }
    }
    let a_len = m * k; // no overflow: both ≤ 4096 ⇒ product ≤ 2^24
    let b_len = k * n;
    let out_len = m * n;
    if a.len() != a_len {
        return Err(PalwReferenceError::MatrixALengthMismatch { got: a.len(), expected: a_len });
    }
    if b.len() != b_len {
        return Err(PalwReferenceError::MatrixBLengthMismatch { got: b.len(), expected: b_len });
    }
    if out_len > PALW_REFERENCE_MAX_GEMM_OUT {
        return Err(PalwReferenceError::OutputTooLarge { got: out_len, max: PALW_REFERENCE_MAX_GEMM_OUT });
    }

    let mut out = Vec::with_capacity(out_len);
    let mut column = vec![0u32; k];
    for i in 0..m {
        let row = &a[i * k..(i + 1) * k];
        for j in 0..n {
            for (kk, slot) in column.iter_mut().enumerate() {
                *slot = b[kk * n + j];
            }
            out.push(ref_dot_v1(row, &column)?);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Ruleset v2 (ADR-0030 §4) — additive operations. Nothing above this line moved.
//
// Frame conventions (shared with v1's `round_pack`):
//   binary32: value = (sig_grs / 8) · 2^(exp − 150),  normal sig_grs ∈ [2^26, 2^27)
//   binary64: value = (sig_grs / 8) · 2^(exp − 1075), normal sig_grs ∈ [2^55, 2^56)
//   binary16: value = (sig_grs / 8) · 2^(exp − 25),   normal sig_grs ∈ [2^13, 2^14)
// The low three bits of every `sig_grs` are guard/round/sticky, exactly as in v1.
// ---------------------------------------------------------------------------------------------

const SIGN_MASK64: u64 = 0x8000_0000_0000_0000;
const ABS_MASK64: u64 = 0x7FFF_FFFF_FFFF_FFFF;
const EXP_MASK64: u64 = 0x7FF0_0000_0000_0000;
const FRAC_MASK64: u64 = 0x000F_FFFF_FFFF_FFFF;
const HIDDEN_BIT64: u64 = 0x0010_0000_0000_0000; // 2^52

#[inline]
fn is_nan_bits64(bits: u64) -> bool {
    (bits & ABS_MASK64) > EXP_MASK64
}

#[inline]
fn is_inf_bits64(bits: u64) -> bool {
    (bits & ABS_MASK64) == EXP_MASK64
}

#[inline]
fn is_zero_bits64(bits: u64) -> bool {
    (bits & ABS_MASK64) == 0
}

/// True iff `bits` is neither NaN nor ±Inf (binary64 twin of [`ref_is_finite_v1`]).
pub fn ref64_is_finite_v2(bits: u64) -> bool {
    (bits & EXP_MASK64) != EXP_MASK64
}

/// 128-bit twin of [`shift_right_jam`].
#[inline]
fn shift_right_jam128(value: u128, shift: u32) -> u128 {
    if shift == 0 {
        value
    } else if shift < 128 {
        let kept = value >> shift;
        let lost = value & ((1u128 << shift) - 1);
        kept | u128::from(lost != 0)
    } else {
        u128::from(value != 0)
    }
}

/// Binary64 twin of [`decompose_finite_nonzero`]: `value = significand · 2^(exp − 1075)`,
/// `significand ∈ [2^52, 2^53)`.
#[inline]
fn decompose_finite_nonzero64(bits: u64) -> (u64, i32) {
    let exp = ((bits & EXP_MASK64) >> 52) as i32;
    let frac = bits & FRAC_MASK64;
    if exp == 0 {
        let shift = frac.leading_zeros() - 11; // top set bit → bit 52
        ((frac << shift), 1 - shift as i32)
    } else {
        (frac | HIDDEN_BIT64, exp)
    }
}

/// Binary64 twin of [`round_pack`]: RNE on three GRS bits, gradual underflow, overflow to ±Inf.
fn round_pack64(sign: u64, mut exp: i32, mut sig_grs: u64) -> u64 {
    debug_assert!(sig_grs < (1 << 57));
    if exp <= 0 {
        sig_grs = shift_right_jam(sig_grs, (1 - exp) as u32);
        exp = 1;
    }
    let round_bits = (sig_grs & 7) as u32;
    let mut sig = sig_grs >> 3;
    if round_bits > 4 || (round_bits == 4 && (sig & 1) == 1) {
        sig += 1;
        if sig == 1 << 53 {
            sig = 1 << 52;
            exp += 1;
        }
    }
    if sig < HIDDEN_BIT64 {
        return sign | sig;
    }
    if exp >= 2047 {
        return sign | EXP_MASK64;
    }
    sign | ((exp as u64) << 52) | (sig & FRAC_MASK64)
}

/// Binary16 twin of [`round_pack`] (bias 15, 10 fraction bits, overflow at exponent 31).
fn round_pack16(sign: u16, mut exp: i32, mut sig_grs: u32) -> u16 {
    debug_assert!(sig_grs < (1 << 15));
    if exp <= 0 {
        sig_grs = shift_right_jam(sig_grs as u64, (1 - exp) as u32) as u32;
        exp = 1;
    }
    let round_bits = sig_grs & 7;
    let mut sig = sig_grs >> 3;
    if round_bits > 4 || (round_bits == 4 && (sig & 1) == 1) {
        sig += 1;
        if sig == 1 << 11 {
            sig = 1 << 10;
            exp += 1;
        }
    }
    if sig < 1 << 10 {
        return sign | sig as u16;
    }
    if exp >= 31 {
        return sign | 0x7C00;
    }
    sign | ((exp as u16) << 10) | (sig as u16 & 0x03FF)
}

/// Normalizes an exact wide magnitude `value = w · 2^(exp − 153)` (any width up to 128 bits,
/// zero allowed) into the binary32 round frame and packs. `w`'s bits are exact — jamming here
/// is the FIRST information loss, which is what makes the fma below correctly rounded.
fn round_wide32(sign: u32, exp: i32, w: u128) -> u32 {
    if w == 0 {
        return 0; // exact cancellation is +0 under RNE (the sign argument does not survive)
    }
    let bit_len = 128 - w.leading_zeros();
    if bit_len > 27 {
        let s = bit_len - 27;
        round_pack(sign, exp + s as i32, shift_right_jam128(w, s) as u64)
    } else {
        let s = 27 - bit_len;
        round_pack(sign, exp - s as i32, (w as u64) << s)
    }
}

/// Canonical fused multiply-add: `RNE(a·b + c)` with ONE rounding (ruleset v2). The product is
/// kept exact (48 bits); alignment against `c` is by exact left shift inside u128, so a massive
/// cancellation never meets a jammed bit — the only jam paths are the two "other operand is
/// below the sticky horizon" fast paths, where cancellation is impossible.
pub fn ref_fma_v2(a: u32, b: u32, c: u32) -> u32 {
    if is_nan_bits(a) || is_nan_bits(b) || is_nan_bits(c) {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    let psign = (a ^ b) & SIGN_MASK;
    let (a_inf, b_inf) = (is_inf_bits(a), is_inf_bits(b));
    let (a_zero, b_zero) = (is_zero_bits(a), is_zero_bits(b));
    if (a_inf && b_zero) || (b_inf && a_zero) {
        return PALW_REFERENCE_CANONICAL_NAN_V1; // 0 × Inf is invalid regardless of c
    }
    if a_inf || b_inf {
        if is_inf_bits(c) && ((c ^ psign) & SIGN_MASK) != 0 {
            return PALW_REFERENCE_CANONICAL_NAN_V1; // Inf − Inf
        }
        return psign | EXP_MASK;
    }
    if is_inf_bits(c) {
        return c;
    }
    if a_zero || b_zero {
        // Exact ±0 product: IEEE addition-of-zeros sign rules against c.
        if is_zero_bits(c) {
            return if ((c ^ psign) & SIGN_MASK) == 0 { c } else { 0 };
        }
        return c;
    }

    let (sa, ea) = decompose_finite_nonzero(a);
    let (sb, eb) = decompose_finite_nonzero(b);
    // Exact product P ∈ [2^46, 2^48); in the round frame: value = ((P<<3)/8) · 2^(ep − 150)
    // with ep = ea + eb − 150.
    let wp = ((sa as u64 * sb as u64) as u128) << 3;
    let ep = ea + eb - 150;
    if is_zero_bits(c) {
        return round_wide32(psign, ep, wp);
    }
    let (sc, ec) = decompose_finite_nonzero(c);
    let csign = c & SIGN_MASK;
    let wc = (sc as u128) << 3;
    // Align both to one common frame by exact LEFT shifts (u128 has the room: the product is
    // ≤ 51 bits, c is ≤ 27); fall to a sticky fold only when the other operand lies entirely
    // below the guard/round/sticky horizon — where cancellation is impossible. This is what
    // makes a massive cancellation never meet a jammed bit.
    let (e0, prod_aligned, c_aligned) = match ep - ec {
        d if d >= 0 => {
            if d > 72 {
                return round_wide32_folding(psign, ep, wp, csign); // c is sticky-only
            }
            (ec, wp << d as u32, wc)
        }
        d => {
            let d = (-d) as u32;
            if d > 96 {
                return round_wide32_folding(csign, ec, wc, psign); // the product is sticky-only
            }
            (ep, wp, wc << d)
        }
    };
    if psign == csign {
        round_wide32(psign, e0, prod_aligned + c_aligned)
    } else if prod_aligned >= c_aligned {
        round_wide32(psign, e0, prod_aligned - c_aligned)
    } else {
        round_wide32(csign, e0, c_aligned - prod_aligned)
    }
}

/// `round_wide32` of exact `w · 2^(exp−153)` with an operand of sign `other_sign` folded in
/// from strictly below the sticky horizon. Same sign: the fold is one sticky bit. Opposite
/// sign: the true value is "just below w" — encoded as `w·2 − 1` one frame lower, which sets
/// sticky and borrows one out of w's trailing-zero run, exactly the RNE-visible effect of
/// subtracting an ε.
fn round_wide32_folding(sign: u32, exp: i32, w: u128, other_sign: u32) -> u32 {
    if sign == other_sign {
        round_wide32(sign, exp, w | 1)
    } else {
        round_wide32(sign, exp - 1, (w << 1) - 1)
    }
}

/// Canonical binary32 division, correctly rounded (ruleset v2).
pub fn ref_div_v2(a: u32, b: u32) -> u32 {
    if is_nan_bits(a) || is_nan_bits(b) {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    let sign = (a ^ b) & SIGN_MASK;
    match (is_inf_bits(a), is_inf_bits(b)) {
        (true, true) => return PALW_REFERENCE_CANONICAL_NAN_V1,
        (true, false) => return sign | EXP_MASK,
        (false, true) => return sign,
        (false, false) => {}
    }
    match (is_zero_bits(a), is_zero_bits(b)) {
        (true, true) => return PALW_REFERENCE_CANONICAL_NAN_V1,
        (false, true) => return sign | EXP_MASK,
        (true, false) => return sign,
        (false, false) => {}
    }
    let (sa, ea) = decompose_finite_nonzero(a);
    let (sb, eb) = decompose_finite_nonzero(b);
    // Q = (sa << 27) / sb ∈ (2^26, 2^28); the infinite tail is nonzero iff the remainder is.
    let n = (sa as u64) << 27;
    let mut q = n / sb as u64;
    let r = n % sb as u64;
    let mut exp = ea - eb + 126;
    q |= u64::from(r != 0); // sticky, valid while the low 3 bits are GRS
    if q >= 1 << 27 {
        q = shift_right_jam(q, 1);
        exp += 1;
    }
    round_pack(sign, exp, q)
}

/// Floor integer square root (binary digit-by-digit; exact for all u64).
fn isqrt_u64(x: u64) -> u64 {
    let mut root: u64 = 0;
    let mut rem: u64 = 0;
    for i in (0..32).rev() {
        let two = (x >> (2 * i)) & 3;
        rem = (rem << 2) | two;
        let trial = (root << 2) | 1;
        root <<= 1;
        if trial <= rem {
            rem -= trial;
            root |= 1;
        }
    }
    root
}

/// Canonical binary32 square root, correctly rounded (ruleset v2). `sqrt(−0) = −0`;
/// any negative non-zero operand (including −Inf) is the canonical NaN.
pub fn ref_sqrt_v2(a: u32) -> u32 {
    if is_nan_bits(a) {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    if is_zero_bits(a) {
        return a;
    }
    if a & SIGN_MASK != 0 {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    if is_inf_bits(a) {
        return a;
    }
    let (sig, e) = decompose_finite_nonzero(a);
    // value = sig · 2^(e−150) = M · 2^(2h): absorb odd exponents into the significand.
    let (m, two_h) = if (e - 150) & 1 != 0 { ((sig as u64) << 1, e - 151) } else { (sig as u64, e - 150) };
    let h = two_h / 2;
    // Q = floor(sqrt(M · 2^32)) ∈ [2^27, 2^29); value = (Q + frac) · 2^(h−16), frac > 0 iff
    // the remainder is nonzero.
    let x = m << 32;
    let mut q = isqrt_u64(x);
    debug_assert!(q * q <= x && (q + 1) * (q + 1) > x);
    q |= u64::from(q * q != x); // sticky
    // Round frame: value = (grs/8)·2^(exp−150) ⟹ exp = h − 16 + 153 + shift.
    let bit_len = 64 - q.leading_zeros();
    let s = bit_len - 27;
    let grs = shift_right_jam(q, s);
    round_pack(0, h - 16 + 153 + s as i32, grs)
}

/// Canonical binary64 negate (ruleset v2).
pub fn ref64_neg_v2(bits: u64) -> u64 {
    if is_nan_bits64(bits) {
        return PALW_REFERENCE_CANONICAL_NAN64_V2;
    }
    bits ^ SIGN_MASK64
}

/// Canonical binary64 addition (ruleset v2) — the v1 binary32 algorithm, widened.
pub fn ref64_add_v2(a: u64, b: u64) -> u64 {
    if is_nan_bits64(a) || is_nan_bits64(b) {
        return PALW_REFERENCE_CANONICAL_NAN64_V2;
    }
    match (is_inf_bits64(a), is_inf_bits64(b)) {
        (true, true) => {
            return if (a ^ b) & SIGN_MASK64 == 0 { a } else { PALW_REFERENCE_CANONICAL_NAN64_V2 };
        }
        (true, false) => return a,
        (false, true) => return b,
        (false, false) => {}
    }
    match (is_zero_bits64(a), is_zero_bits64(b)) {
        (true, true) => return if a == b { a } else { 0 },
        (true, false) => return b,
        (false, true) => return a,
        (false, false) => {}
    }
    let (big, small) = if (a & ABS_MASK64) >= (b & ABS_MASK64) { (a, b) } else { (b, a) };
    let (sig_b, exp_b) = decompose_finite_nonzero64(big);
    let (sig_s, exp_s) = decompose_finite_nonzero64(small);
    let sign_big = big & SIGN_MASK64;
    let delta = (exp_b - exp_s) as u32;
    let big_grs = sig_b << 3;
    let small_grs = shift_right_jam(sig_s << 3, delta);
    if (big ^ small) & SIGN_MASK64 == 0 {
        let mut sum = big_grs + small_grs;
        let mut exp = exp_b;
        if sum >= 1 << 56 {
            sum = shift_right_jam(sum, 1);
            exp += 1;
        }
        round_pack64(sign_big, exp, sum)
    } else {
        let mut diff = big_grs - small_grs;
        if diff == 0 {
            return 0;
        }
        let mut exp = exp_b;
        let shift = 55u32.saturating_sub(63 - diff.leading_zeros());
        diff <<= shift;
        exp -= shift as i32;
        round_pack64(sign_big, exp, diff)
    }
}

/// Canonical binary64 subtraction: literally `add64(a, neg64(b))` (the v1 discipline).
pub fn ref64_sub_v2(a: u64, b: u64) -> u64 {
    ref64_add_v2(a, ref64_neg_v2(b))
}

/// Canonical binary64 multiplication (ruleset v2).
pub fn ref64_mul_v2(a: u64, b: u64) -> u64 {
    if is_nan_bits64(a) || is_nan_bits64(b) {
        return PALW_REFERENCE_CANONICAL_NAN64_V2;
    }
    let sign = (a ^ b) & SIGN_MASK64;
    match (is_inf_bits64(a), is_inf_bits64(b)) {
        (true, true) => return sign | EXP_MASK64,
        (true, false) => return if is_zero_bits64(b) { PALW_REFERENCE_CANONICAL_NAN64_V2 } else { sign | EXP_MASK64 },
        (false, true) => return if is_zero_bits64(a) { PALW_REFERENCE_CANONICAL_NAN64_V2 } else { sign | EXP_MASK64 },
        (false, false) => {}
    }
    if is_zero_bits64(a) || is_zero_bits64(b) {
        return sign;
    }
    let (sig_a, exp_a) = decompose_finite_nonzero64(a);
    let (sig_b, exp_b) = decompose_finite_nonzero64(b);
    // Exact product ∈ [2^104, 2^106); value = P · 2^(exp_a + exp_b − 2150). In the round frame
    // (grs/8)·2^(e−1075) with grs = P >> s (jam): e = exp_a + exp_b − 1072 + s. Anchor:
    // 1.0 × 1.0 → P = 2^104, s = 49, e = 1023.
    let product = (sig_a as u128) * (sig_b as u128);
    let (grs, e) = if product >= 1 << 105 {
        (shift_right_jam128(product, 50) as u64, exp_a + exp_b - 1022)
    } else {
        (shift_right_jam128(product, 49) as u64, exp_a + exp_b - 1023)
    };
    round_pack64(sign, e, grs)
}

/// Canonical binary64 division, correctly rounded (ruleset v2).
pub fn ref64_div_v2(a: u64, b: u64) -> u64 {
    if is_nan_bits64(a) || is_nan_bits64(b) {
        return PALW_REFERENCE_CANONICAL_NAN64_V2;
    }
    let sign = (a ^ b) & SIGN_MASK64;
    match (is_inf_bits64(a), is_inf_bits64(b)) {
        (true, true) => return PALW_REFERENCE_CANONICAL_NAN64_V2,
        (true, false) => return sign | EXP_MASK64,
        (false, true) => return sign,
        (false, false) => {}
    }
    match (is_zero_bits64(a), is_zero_bits64(b)) {
        (true, true) => return PALW_REFERENCE_CANONICAL_NAN64_V2,
        (false, true) => return sign | EXP_MASK64,
        (true, false) => return sign,
        (false, false) => {}
    }
    let (sa, ea) = decompose_finite_nonzero64(a);
    let (sb, eb) = decompose_finite_nonzero64(b);
    let n = (sa as u128) << 56;
    let mut q = (n / sb as u128) as u64; // ∈ (2^55, 2^57)
    let r = n % sb as u128;
    let mut exp = ea - eb + 1022;
    q |= u64::from(r != 0);
    if q >= 1 << 56 {
        q = shift_right_jam(q, 1);
        exp += 1;
    }
    round_pack64(sign, exp, q)
}

/// Exact widening f32 → f64 (ruleset v2). Every binary32 value is normal in binary64.
pub fn ref_widen_f32_to_f64_v2(bits: u32) -> u64 {
    if is_nan_bits(bits) {
        return PALW_REFERENCE_CANONICAL_NAN64_V2;
    }
    let sign = (bits as u64 & SIGN_MASK as u64) << 32;
    if is_inf_bits(bits) {
        return sign | EXP_MASK64;
    }
    if is_zero_bits(bits) {
        return sign;
    }
    let (sig, e32) = decompose_finite_nonzero(bits);
    let sig64 = (sig as u64) << 29; // ∈ [2^52, 2^53)
    let e64 = e32 + 896;
    sign | ((e64 as u64) << 52) | (sig64 & FRAC_MASK64)
}

/// RNE narrowing f64 → f32 (ruleset v2).
pub fn ref_narrow_f64_to_f32_v2(bits: u64) -> u32 {
    if is_nan_bits64(bits) {
        return PALW_REFERENCE_CANONICAL_NAN_V1;
    }
    let sign = ((bits >> 32) as u32) & SIGN_MASK;
    if is_inf_bits64(bits) {
        return sign | EXP_MASK;
    }
    if is_zero_bits64(bits) {
        return sign;
    }
    let (sig, e64) = decompose_finite_nonzero64(bits);
    // grs = (sig<<3) >> 29 (jam) ∈ [2^26, 2^27); e32 = e64 − 896 (anchor: 1.0 → 1.0).
    round_pack(sign, e64 - 896, shift_right_jam(sig << 3, 29))
}

/// Exact widening f16 → f32 (ruleset v2). Every binary16 value is exactly a binary32 value.
pub fn ref_f16_to_f32_v2(bits: u16) -> u32 {
    let sign = (bits as u32 & 0x8000) << 16;
    let exp = (bits >> 10) & 0x1F;
    let frac = (bits & 0x03FF) as u32;
    match exp {
        31 => {
            if frac != 0 {
                PALW_REFERENCE_CANONICAL_NAN_V1
            } else {
                sign | EXP_MASK
            }
        }
        0 => {
            if frac == 0 {
                return sign;
            }
            // Subnormal: value = frac · 2^(−24); normalize into the f32 hidden-bit frame.
            let k = frac.leading_zeros() - 8; // top set bit → bit 23
            let sig = frac << k;
            let e32 = 126 - k as i32;
            sign | ((e32 as u32) << 23) | (sig & FRAC_MASK)
        }
        e => {
            let sig = (frac | 0x0400) << 13; // ∈ [2^23, 2^24)
            let e32 = e as i32 + 112;
            sign | ((e32 as u32) << 23) | (sig & FRAC_MASK)
        }
    }
}

/// RNE narrowing f32 → f16 (ruleset v2) — the KV-cache write seam.
pub fn ref_f32_to_f16_v2(bits: u32) -> u16 {
    if is_nan_bits(bits) {
        return PALW_REFERENCE_CANONICAL_NAN16_V2;
    }
    let sign = ((bits >> 16) & 0x8000) as u16;
    if is_inf_bits(bits) {
        return sign | 0x7C00;
    }
    if is_zero_bits(bits) {
        return sign;
    }
    let (sig, e32) = decompose_finite_nonzero(bits);
    // grs = (sig<<3) >> 13 (jam) ∈ [2^13, 2^14); e16 = e32 − 112 (inverse of the widening).
    round_pack16(sign, e32 - 112, shift_right_jam((sig as u64) << 3, 13) as u32)
}

// =============================================================================================
// Tests — the hardware FPU under the test runner's clean environment is the IEEE-754 oracle.
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;

    /// Hardware oracle with the same canonical-NaN rule applied on top. Valid as an oracle only
    /// under the test runner's default FP environment (RNE, no FTZ/DAZ) — which is exactly what
    /// the normative soft path must reproduce without needing that environment.
    fn hw_canon(x: f32) -> u32 {
        if x.is_nan() {
            PALW_REFERENCE_CANONICAL_NAN_V1
        } else {
            x.to_bits()
        }
    }

    fn hw_add(a: u32, b: u32) -> u32 {
        hw_canon(f32::from_bits(a) + f32::from_bits(b))
    }

    fn hw_mul(a: u32, b: u32) -> u32 {
        hw_canon(f32::from_bits(a) * f32::from_bits(b))
    }

    fn hw_sub(a: u32, b: u32) -> u32 {
        hw_canon(f32::from_bits(a) - f32::from_bits(b))
    }

    /// Deterministic xorshift64* — no clock, no OS randomness, same sequence every run.
    struct DetRng(u64);
    impl DetRng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn next_u32(&mut self) -> u32 {
            (self.next() >> 32) as u32
        }
    }

    /// The special-value matrix: every boundary the rounding paths branch on.
    fn special_values() -> Vec<u32> {
        let mut magnitudes: Vec<u32> = vec![
            0x0000_0000, // +0
            0x0000_0001, // min subnormal
            0x0000_0002,
            0x0000_0003,
            0x003F_FFFF, // mid subnormal, all-ones tail
            0x007F_FFFF, // max subnormal
            0x0080_0000, // min normal
            0x0080_0001,
            0x00FF_FFFF,
            0x3F00_0000, // 0.5
            0x3F40_0000, // 0.75
            0x3F7F_FFFF, // just under 1
            0x3F80_0000, // 1.0
            0x3F80_0001, // 1 + ulp
            0x3FC0_0000, // 1.5
            0x4000_0000, // 2.0
            0x3FFF_FFFF, // just under 2
            0x4B80_0000, // 2^24
            0x4B80_0001,
            0x4B00_0000, // 2^23
            0x7F00_0000, // 2^127
            0x7F7F_FFFE,
            0x7F7F_FFFF, // max finite
            0x7F80_0000, // +Inf
        ];
        let negatives: Vec<u32> = magnitudes.iter().map(|m| m | SIGN_MASK).collect();
        magnitudes.extend(negatives);
        magnitudes.push(0x7FC0_0000); // canonical qNaN
        magnitudes.push(0x7F80_0001); // signaling-NaN pattern
        magnitudes.push(0xFFC0_1234); // negative NaN with payload
        magnitudes
    }

    // -----------------------------------------------------------------------------------------
    // Identity and domains
    // -----------------------------------------------------------------------------------------

    #[test]
    fn ruleset_id_golden_vector() {
        // Frozen 2026-08-15. A change here is a semantics change: new ruleset, new id, new
        // module version — never an in-place edit.
        assert_eq!(
            reference_arithmetic_ruleset_id_v1().to_string(),
            "dd45aec197cc664e3fa64965775b37e290a1e8c9368057ff61cde34ce10d7e61\
             77bb92cfeacce4762641103a614e2848f058de8aefc370a610a53ffb5c65a7bb"
        );
    }

    #[test]
    fn reference_domains_are_unique_across_all_palw_modules() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_REFERENCE_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate reference domain");
            assert!(d.len() <= 64, "blake2b key cap exceeded");
        }
        for d in PALW_V2_ALL_DOMAINS.iter().chain(PALW_S_ALL_DOMAINS.iter()) {
            assert!(!seen.contains(d), "reference module reuses a foreign domain: {}", String::from_utf8_lossy(d));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Scalar ops vs the hardware oracle
    // -----------------------------------------------------------------------------------------

    #[test]
    fn special_matrix_matches_hardware_exactly() {
        let values = special_values();
        for &a in &values {
            for &b in &values {
                assert_eq!(ref_add_v1(a, b), hw_add(a, b), "add {a:08x} {b:08x}");
                assert_eq!(ref_mul_v1(a, b), hw_mul(a, b), "mul {a:08x} {b:08x}");
                assert_eq!(ref_sub_v1(a, b), hw_sub(a, b), "sub {a:08x} {b:08x}");
            }
        }
    }

    #[test]
    fn random_sweep_matches_hardware_exactly() {
        let mut rng = DetRng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..200_000 {
            let a = rng.next_u32();
            let b = rng.next_u32();
            assert_eq!(ref_add_v1(a, b), hw_add(a, b), "add {a:08x} {b:08x}");
            assert_eq!(ref_mul_v1(a, b), hw_mul(a, b), "mul {a:08x} {b:08x}");
        }
    }

    /// Ties need the sticky region exactly zero, so uniform random essentially never exercises
    /// them. Force the neighborhood: nearby exponents, sparse low bits.
    #[test]
    fn tie_neighborhood_sweep_matches_hardware_exactly() {
        let mut rng = DetRng(0xD1B5_4A32_D192_ED03);
        for _ in 0..100_000 {
            let exp = 110 + (rng.next_u32() % 40); // exponents in a ±20 band around 1.0
            let exp_b = exp.wrapping_add(rng.next_u32() % 5).clamp(1, 254);
            let sparse_mask = !((1u32 << (rng.next_u32() % 12)) - 1); // clear up to 11 low bits
            let a = ((rng.next_u32() & SIGN_MASK) | (exp << 23) | (rng.next_u32() & FRAC_MASK)) & (SIGN_MASK | EXP_MASK | sparse_mask);
            let b = ((rng.next_u32() & SIGN_MASK) | (exp_b << 23) | (rng.next_u32() & FRAC_MASK)) & (SIGN_MASK | EXP_MASK | sparse_mask);
            assert_eq!(ref_add_v1(a, b), hw_add(a, b), "add {a:08x} {b:08x}");
            assert_eq!(ref_mul_v1(a, b), hw_mul(a, b), "mul {a:08x} {b:08x}");
        }
    }

    /// The subnormal range end-to-end: uniform bits rarely land there, so drive it directly.
    #[test]
    fn subnormal_sweep_matches_hardware_exactly() {
        let mut rng = DetRng(0xA076_1D64_78BD_642F);
        for _ in 0..100_000 {
            // Both operands drawn from {subnormal, smallest-normal} binades, either sign.
            let a = (rng.next_u32() & SIGN_MASK) | (rng.next_u32() % (3 * HIDDEN_BIT));
            let b = (rng.next_u32() & SIGN_MASK) | (rng.next_u32() % (3 * HIDDEN_BIT));
            assert_eq!(ref_add_v1(a, b), hw_add(a, b), "add {a:08x} {b:08x}");
            assert_eq!(ref_mul_v1(a, b), hw_mul(a, b), "mul {a:08x} {b:08x}");
            // Products of a subnormal/small-normal with a mid-range value underflow gradually.
            let c = (rng.next_u32() & SIGN_MASK) | ((60 + (rng.next_u32() % 80)) << 23) | (rng.next_u32() & FRAC_MASK);
            assert_eq!(ref_mul_v1(a, c), hw_mul(a, c), "mul {a:08x} {c:08x}");
        }
    }

    // -----------------------------------------------------------------------------------------
    // Behaviors the rule text names, frozen as explicit bits (not just oracle agreement)
    // -----------------------------------------------------------------------------------------

    #[test]
    fn signed_zero_and_cancellation_rules_are_pinned() {
        let pos = 0x0000_0000;
        let neg = 0x8000_0000;
        assert_eq!(ref_add_v1(pos, pos), pos);
        assert_eq!(ref_add_v1(neg, neg), neg); // (−0)+(−0) = −0
        assert_eq!(ref_add_v1(pos, neg), pos); // mixed zeros = +0
        assert_eq!(ref_add_v1(neg, pos), pos);
        let one = 0x3F80_0000;
        assert_eq!(ref_add_v1(one, one | SIGN_MASK), pos); // exact cancellation = +0
        assert_eq!(ref_sub_v1(one, one), pos);
        assert_eq!(ref_mul_v1(one, neg), neg); // sign xor through zero
        assert_eq!(ref_mul_v1(one | SIGN_MASK, neg), pos);
    }

    #[test]
    fn nan_payloads_never_survive() {
        for nan in [0x7FC0_0000u32, 0x7F80_0001, 0xFFC0_1234, 0x7FFF_FFFF] {
            for other in [0x3F80_0000u32, 0x7F80_0000, 0x0000_0000] {
                for result in [
                    ref_add_v1(nan, other),
                    ref_add_v1(other, nan),
                    ref_mul_v1(nan, other),
                    ref_mul_v1(other, nan),
                    ref_sub_v1(nan, other),
                    ref_neg_v1(nan),
                ] {
                    assert_eq!(result, PALW_REFERENCE_CANONICAL_NAN_V1, "payload leaked from {nan:08x}");
                }
            }
        }
        // Invalid operations mint ONLY the canonical NaN.
        let inf = 0x7F80_0000;
        assert_eq!(ref_add_v1(inf, inf | SIGN_MASK), PALW_REFERENCE_CANONICAL_NAN_V1);
        assert_eq!(ref_mul_v1(inf, 0), PALW_REFERENCE_CANONICAL_NAN_V1);
        assert_eq!(ref_mul_v1(SIGN_MASK, inf), PALW_REFERENCE_CANONICAL_NAN_V1);
    }

    #[test]
    fn overflow_underflow_and_tie_edges_are_pinned() {
        let max_finite = 0x7F7F_FFFF;
        let inf = 0x7F80_0000;
        assert_eq!(ref_add_v1(max_finite, max_finite), inf); // RNE overflow
        assert_eq!(ref_mul_v1(max_finite, 0x4000_0000), inf);
        // 2^24 + 1 is a tie to even → stays 2^24; 2^24 + 2 is exact.
        let two_24 = 0x4B80_0000;
        assert_eq!(ref_add_v1(two_24, 0x3F80_0000), two_24);
        assert_eq!(ref_add_v1(two_24, 0x4000_0000), 0x4B80_0001);
        // min-normal − min-subnormal borrows into the subnormal range (gradual underflow).
        assert_eq!(ref_sub_v1(0x0080_0000, 0x0000_0001), 0x007F_FFFF);
        // Product of two tiny values underflows to zero with the correct sign.
        assert_eq!(ref_mul_v1(0x0000_0001, 0x0000_0001), 0);
        assert_eq!(ref_mul_v1(0x0000_0001, 0x8000_0001), SIGN_MASK);
    }

    /// A multiply-then-add differs from a fused multiply-add on witness inputs; the reference
    /// must produce the two-rounding result. This test proves the witness actually
    /// discriminates (hardware FMA disagrees) — otherwise it would pin nothing.
    #[test]
    fn no_fma_witness() {
        let a = 1.0f32 + f32::from_bits(0x3480_0000); // 1 + 2^-22-ish: product needs > 24 bits
        let product_rounded = ref_mul_v1(a.to_bits(), a.to_bits());
        let two_step = ref_add_v1(product_rounded, product_rounded | SIGN_MASK);
        assert_eq!(two_step, 0, "two-step self-cancellation must be exact");
        let residual = f32::from_bits(a.to_bits()).mul_add(a, -f32::from_bits(product_rounded));
        assert_ne!(residual, 0.0, "witness does not discriminate FMA from two-step");
        // And the reference two-step result equals hardware two-step, not hardware FMA.
        let hw_two_step = hw_canon(a * a);
        assert_eq!(product_rounded, hw_two_step);
    }

    // -----------------------------------------------------------------------------------------
    // Dot and GEMM
    // -----------------------------------------------------------------------------------------

    #[test]
    fn dot_order_witness_ascending_differs_from_descending() {
        // [2^24, 1, 1] · [1, 1, 1]: ascending gives 2^24 (both +1 ties stay, to even);
        // descending gives 2^24 + 2 (1+1 = 2 first, then exact). The pinned order is ascending.
        let a = [0x4B80_0000u32, 0x3F80_0000, 0x3F80_0000];
        let ones = [0x3F80_0000u32; 3];
        let ascending = ref_dot_v1(&a, &ones).unwrap();
        assert_eq!(ascending, 0x4B80_0000, "pinned ascending result moved");
        let mut reversed = a;
        reversed.reverse();
        let descending = ref_dot_v1(&reversed, &ones).unwrap();
        assert_eq!(descending, 0x4B80_0001);
        assert_ne!(ascending, descending, "order witness lost its power");
    }

    #[test]
    fn dot_matches_sequential_hardware_on_random_vectors() {
        let mut rng = DetRng(0x1234_5678_9ABC_DEF1);
        for round in 0..200 {
            let len = 1 + (rng.next_u32() as usize % 64);
            // Mixed magnitudes force heavy cancellation — the order-sensitive regime.
            let a: Vec<u32> = (0..len)
                .map(|_| (rng.next_u32() & SIGN_MASK) | ((64 + (rng.next_u32() % 128)) << 23) | (rng.next_u32() & FRAC_MASK))
                .collect();
            let b: Vec<u32> = (0..len)
                .map(|_| (rng.next_u32() & SIGN_MASK) | ((64 + (rng.next_u32() % 128)) << 23) | (rng.next_u32() & FRAC_MASK))
                .collect();
            let mut acc = 0.0f32;
            for k in 0..len {
                acc += f32::from_bits(a[k]) * f32::from_bits(b[k]);
            }
            assert_eq!(ref_dot_v1(&a, &b).unwrap(), hw_canon(acc), "round {round}");
        }
    }

    #[test]
    fn dot_shape_errors_are_closed() {
        assert_eq!(ref_dot_v1(&[], &[]), Err(PalwReferenceError::EmptyOperand));
        assert_eq!(ref_dot_v1(&[0, 0], &[0]), Err(PalwReferenceError::LengthMismatch { a: 2, b: 1 }));
        let long = vec![0u32; PALW_REFERENCE_MAX_DOT_LEN + 1];
        assert!(matches!(ref_dot_v1(&long, &long), Err(PalwReferenceError::VectorTooLong { .. })));
    }

    #[test]
    fn gemm_golden_and_layout() {
        // A = [[1, 2, 3], [0.5, -1, 4]] (2×3), B = [[1, 0], [0, 1], [1, 1]] (3×2).
        let one = 0x3F80_0000u32;
        let two = 0x4000_0000u32;
        let three = 0x4040_0000u32;
        let four = 0x4080_0000u32;
        let half = 0x3F00_0000u32;
        let a = [one, two, three, half, one | SIGN_MASK, four];
        let b = [one, 0, 0, one, one, one];
        let c = ref_gemm_v1(&a, &b, 2, 2, 3).unwrap();
        // Row-major C: [1+3, 2+3, 0.5+4, -1+4] = [4, 5, 4.5, 3].
        assert_eq!(c, vec![0x4080_0000, 0x40A0_0000, 0x4090_0000, 0x4040_0000]);
    }

    #[test]
    fn gemm_matches_hardware_on_random_tiles() {
        let mut rng = DetRng(0xFACE_FEED_0BAD_F00D);
        for _ in 0..20 {
            let m = 1 + (rng.next_u32() as usize % 5);
            let n = 1 + (rng.next_u32() as usize % 5);
            let k = 1 + (rng.next_u32() as usize % 9);
            let a: Vec<u32> = (0..m * k)
                .map(|_| (rng.next_u32() & SIGN_MASK) | ((100 + (rng.next_u32() % 56)) << 23) | (rng.next_u32() & FRAC_MASK))
                .collect();
            let b: Vec<u32> = (0..k * n)
                .map(|_| (rng.next_u32() & SIGN_MASK) | ((100 + (rng.next_u32() % 56)) << 23) | (rng.next_u32() & FRAC_MASK))
                .collect();
            let c = ref_gemm_v1(&a, &b, m, n, k).unwrap();
            for i in 0..m {
                for j in 0..n {
                    let mut acc = 0.0f32;
                    for kk in 0..k {
                        acc += f32::from_bits(a[i * k + kk]) * f32::from_bits(b[kk * n + j]);
                    }
                    assert_eq!(c[i * n + j], hw_canon(acc), "m={m} n={n} k={k} i={i} j={j}");
                }
            }
        }
    }

    #[test]
    fn gemm_shape_errors_are_closed() {
        assert_eq!(ref_gemm_v1(&[], &[], 0, 1, 1), Err(PalwReferenceError::DimensionZero));
        assert!(matches!(
            ref_gemm_v1(&[0; 4], &[0; 4], PALW_REFERENCE_MAX_GEMM_DIM + 1, 2, 2),
            Err(PalwReferenceError::DimensionTooLarge { .. })
        ));
        assert_eq!(ref_gemm_v1(&[0; 5], &[0; 6], 2, 2, 3), Err(PalwReferenceError::MatrixALengthMismatch { got: 5, expected: 6 }));
        assert_eq!(ref_gemm_v1(&[0; 6], &[0; 5], 2, 2, 3), Err(PalwReferenceError::MatrixBLengthMismatch { got: 5, expected: 6 }));
        // m·n over the output cap, with dims individually legal.
        assert!(matches!(
            ref_gemm_v1(&[0; 4096], &[0; 4096], 2048, 2048, 2),
            Err(PalwReferenceError::OutputTooLarge { .. })
        ));
    }

    /// Non-finite results are total, deterministic values — the policy layer sees exactly the
    /// canonical bits, never a platform artifact.
    #[test]
    fn dot_is_total_through_overflow_and_finiteness_helper_reports_it() {
        let max_finite = 0x7F7F_FFFFu32;
        let one = 0x3F80_0000u32;
        let overflowing = ref_dot_v1(&[max_finite, max_finite], &[one, one]).unwrap();
        assert_eq!(overflowing, 0x7F80_0000);
        assert!(!ref_is_finite_v1(overflowing));
        // Pinned order matters even for overflow reachability: acc = max, then
        // add(max, −max) = +0 — the sum never visits ±Inf.
        let cancelling = ref_dot_v1(&[max_finite, max_finite], &[one, one | SIGN_MASK]);
        assert_eq!(cancelling.unwrap(), 0);
        let nan_path = ref_dot_v1(&[max_finite, max_finite, max_finite], &[one, one, one | SIGN_MASK]).unwrap();
        // acc: max → +Inf (overflow) → Inf + (−max) = +Inf still → stays +Inf.
        assert_eq!(nan_path, 0x7F80_0000);
        assert!(ref_is_finite_v1(0x3F80_0000));
        assert!(!ref_is_finite_v1(PALW_REFERENCE_CANONICAL_NAN_V1));
    }

    // =========================================================================================
    // Ruleset v2 — hardware oracles: aarch64 fmadd (fused), fdiv, fsqrt, native binary64, and
    // the RNE `as` casts. Same stance as v1: the oracle validates the soft path; the soft path
    // is normative without needing the oracle's environment.
    // =========================================================================================

    fn hw_canon64(x: f64) -> u64 {
        if x.is_nan() {
            PALW_REFERENCE_CANONICAL_NAN64_V2
        } else {
            x.to_bits()
        }
    }

    fn hw_fma(a: u32, b: u32, c: u32) -> u32 {
        hw_canon(f32::from_bits(a).mul_add(f32::from_bits(b), f32::from_bits(c)))
    }

    fn hw_div(a: u32, b: u32) -> u32 {
        hw_canon(f32::from_bits(a) / f32::from_bits(b))
    }

    #[test]
    fn ruleset_v2_id_golden_vector_and_distinct_from_v1() {
        // Frozen 2026-08-16. Any change to the v2 rule text is a v3, never an edit.
        assert_eq!(
            reference_arithmetic_ruleset_id_v2().to_string(),
            "669e08064d664738508e00bfb30e9458350de8261d83a063e236cfcfba34dc9d\
             3a634a36b5c166951b1d524bd52a3ee0f84298571ead1640e9d2e10b5b435785"
        );
        assert_ne!(reference_arithmetic_ruleset_id_v2(), reference_arithmetic_ruleset_id_v1());
    }

    #[test]
    fn fma_special_matrix_matches_hardware_exactly() {
        // The full triple product of the special matrix is ~110k combinations — exhaustive over
        // every zero/inf/nan/subnormal/tie boundary interaction, including 0×Inf+NaN.
        let values = special_values();
        for &a in &values {
            for &b in &values {
                for &c in &values {
                    assert_eq!(ref_fma_v2(a, b, c), hw_fma(a, b, c), "fma {a:08x} {b:08x} {c:08x}");
                }
            }
        }
    }

    #[test]
    fn fma_random_sweep_matches_hardware_exactly() {
        let mut rng = DetRng(0x5851_F42D_4C95_7F2D);
        for _ in 0..300_000 {
            let (a, b, c) = (rng.next_u32(), rng.next_u32(), rng.next_u32());
            assert_eq!(ref_fma_v2(a, b, c), hw_fma(a, b, c), "fma {a:08x} {b:08x} {c:08x}");
        }
    }

    /// The regime that discriminates a real fma from mul-then-add: c ≈ −(a·b), so the result is
    /// the sub-ulp residual of the exact product. Also drives the negligible-operand folds.
    #[test]
    fn fma_cancellation_and_magnitude_gap_sweeps_match_hardware() {
        let mut rng = DetRng(0x0DDB_1A5E_5BAD_5EED);
        for _ in 0..200_000 {
            let exp_a = 64 + (rng.next_u32() % 128);
            let exp_b = 64 + (rng.next_u32() % 128);
            let a = (rng.next_u32() & SIGN_MASK) | (exp_a << 23) | (rng.next_u32() & FRAC_MASK);
            let b = (rng.next_u32() & SIGN_MASK) | (exp_b << 23) | (rng.next_u32() & FRAC_MASK);
            // c = −round(a·b) and its ±1-ulp neighbors: maximal cancellation against the exact
            // product, where only the fused path has the answer.
            let p = ref_mul_v1(a, b);
            if ref_is_finite_v1(p) && !is_zero_bits(p) {
                let neg_p = p ^ SIGN_MASK;
                for c in [neg_p, neg_p.wrapping_add(1), neg_p.wrapping_sub(1)] {
                    assert_eq!(ref_fma_v2(a, b, c), hw_fma(a, b, c), "fma-cancel {a:08x} {b:08x} {c:08x}");
                }
            }
            // Magnitude-gap folds: c astronomically smaller / larger than a·b.
            let tiny = (rng.next_u32() & SIGN_MASK) | (1 + (rng.next_u32() % 8)) << 23 | (rng.next_u32() & FRAC_MASK);
            let huge = (rng.next_u32() & SIGN_MASK) | (240 + (rng.next_u32() % 14)) << 23 | (rng.next_u32() & FRAC_MASK);
            assert_eq!(ref_fma_v2(a, b, tiny), hw_fma(a, b, tiny), "fma-tinyc {a:08x} {b:08x} {tiny:08x}");
            assert_eq!(ref_fma_v2(tiny, tiny, huge), hw_fma(tiny, tiny, huge), "fma-hugec {tiny:08x} {huge:08x}");
            assert_eq!(ref_fma_v2(tiny, tiny, tiny), hw_fma(tiny, tiny, tiny), "fma-subn {tiny:08x}");
        }
    }

    #[test]
    fn div_matches_hardware_exactly() {
        let values = special_values();
        for &a in &values {
            for &b in &values {
                assert_eq!(ref_div_v2(a, b), hw_div(a, b), "div {a:08x} {b:08x}");
            }
        }
        let mut rng = DetRng(0xD1F1_5100_0000_0001);
        for _ in 0..300_000 {
            let (a, b) = (rng.next_u32(), rng.next_u32());
            assert_eq!(ref_div_v2(a, b), hw_div(a, b), "div {a:08x} {b:08x}");
        }
        // Subnormal results and gradual underflow.
        for _ in 0..100_000 {
            let a = (rng.next_u32() & SIGN_MASK) | (1 + (rng.next_u32() % 40)) << 23 | (rng.next_u32() & FRAC_MASK);
            let b = (rng.next_u32() & SIGN_MASK) | (150 + (rng.next_u32() % 100)) << 23 | (rng.next_u32() & FRAC_MASK);
            assert_eq!(ref_div_v2(a, b), hw_div(a, b), "div-underflow {a:08x} {b:08x}");
        }
        // Anchor: 1/3 is the classic RNE witness.
        assert_eq!(ref_div_v2(0x3F80_0000, 0x4040_0000), 0x3EAA_AAAB);
    }

    /// NaN-INPUT rows do not consult the hardware: in release builds LLVM's transformations
    /// around the sqrt intrinsic make NaN *payload* observation through `is_nan`/`to_bits`
    /// unreliable (observed 2026-08-16: `hw_canon∘sqrt` returned a payload NaN for an sNaN
    /// input, and the failing row moved when an eprintln changed inlining). IEEE-754's whole
    /// claim for a NaN operand is "the result is a NaN" — which the canonicalization rule is
    /// the answer to — so those rows assert the rule itself. Non-NaN inputs are unaffected:
    /// any NaN the hardware mints there (negative operands) is the payload-free default NaN,
    /// which compares identically whether or not the canonicalizing branch runs.
    #[test]
    fn sqrt_matches_hardware_exactly() {
        let check = |a: u32| {
            if is_nan_bits(a) {
                assert_eq!(ref_sqrt_v2(a), PALW_REFERENCE_CANONICAL_NAN_V1, "sqrt nan-in {a:08x}");
            } else {
                assert_eq!(ref_sqrt_v2(a), hw_canon(f32::from_bits(a).sqrt()), "sqrt {a:08x}");
            }
        };
        for &a in &special_values() {
            check(a);
        }
        // sqrt(−0) = −0 is IEEE; pin it explicitly.
        assert_eq!(ref_sqrt_v2(0x8000_0000), 0x8000_0000);
        let mut rng = DetRng(0x50B7_0000_0000_0001);
        for _ in 0..400_000 {
            check(rng.next_u32() & ABS_MASK); // nonnegative half; negatives are covered by specials
        }
        // Dense significand sweep at fixed exponents (both parities + subnormal range).
        for exp in [0u32, 1, 126, 127, 128, 254] {
            for step in 0..40_000u32 {
                let frac = step.wrapping_mul(209) & FRAC_MASK;
                let a = (exp << 23) | frac;
                assert_eq!(ref_sqrt_v2(a), hw_canon(f32::from_bits(a).sqrt()), "sqrt {a:08x}");
            }
        }
        assert_eq!(ref_sqrt_v2(0x4000_0000), 0x3FB5_04F3); // sqrt(2), the RNE anchor
    }

    fn special_values64() -> Vec<u64> {
        let mut m: Vec<u64> = vec![
            0x0000_0000_0000_0000,
            0x0000_0000_0000_0001, // min subnormal
            0x000F_FFFF_FFFF_FFFF, // max subnormal
            0x0010_0000_0000_0000, // min normal
            0x3FE0_0000_0000_0000, // 0.5
            0x3FEF_FFFF_FFFF_FFFF,
            0x3FF0_0000_0000_0000, // 1.0
            0x3FF0_0000_0000_0001,
            0x4000_0000_0000_0000, // 2.0
            0x4330_0000_0000_0000, // 2^52
            0x4340_0000_0000_0000, // 2^53
            0x7FE0_0000_0000_0000, // 2^1023
            0x7FEF_FFFF_FFFF_FFFF, // max finite
            0x7FF0_0000_0000_0000, // +Inf
            // f32-boundary neighborhood (the narrowing seams)
            0x47EF_FFFF_E000_0000, // f32::MAX exactly
            0x47EF_FFFF_F000_0000, // halfway to f32 overflow
            0x47EF_FFFF_F000_0001,
            0x3690_0000_0000_0000, // near f32 min subnormal
            0x36A0_0000_0000_0000,
        ];
        let negs: Vec<u64> = m.iter().map(|v| v | SIGN_MASK64).collect();
        m.extend(negs);
        m.push(0x7FF8_0000_0000_0000); // canonical qNaN
        m.push(0x7FF0_0000_0000_0001); // sNaN pattern
        m.push(0xFFF8_0000_0000_1234); // negative payload NaN
        m
    }

    #[test]
    fn binary64_ops_match_hardware_exactly() {
        let values = special_values64();
        for &a in &values {
            for &b in &values {
                let (fa, fb) = (f64::from_bits(a), f64::from_bits(b));
                assert_eq!(ref64_add_v2(a, b), hw_canon64(fa + fb), "add64 {a:016x} {b:016x}");
                assert_eq!(ref64_mul_v2(a, b), hw_canon64(fa * fb), "mul64 {a:016x} {b:016x}");
                assert_eq!(ref64_sub_v2(a, b), hw_canon64(fa - fb), "sub64 {a:016x} {b:016x}");
                assert_eq!(ref64_div_v2(a, b), hw_canon64(fa / fb), "div64 {a:016x} {b:016x}");
            }
        }
        let mut rng = DetRng(0xB16F_1047_5000_0001);
        for _ in 0..200_000 {
            let a = rng.next();
            let b = rng.next();
            let (fa, fb) = (f64::from_bits(a), f64::from_bits(b));
            assert_eq!(ref64_add_v2(a, b), hw_canon64(fa + fb), "add64 {a:016x} {b:016x}");
            assert_eq!(ref64_mul_v2(a, b), hw_canon64(fa * fb), "mul64 {a:016x} {b:016x}");
            assert_eq!(ref64_div_v2(a, b), hw_canon64(fa / fb), "div64 {a:016x} {b:016x}");
        }
        // Tie/sparse-low-bit neighborhood, the regime uniform bits never visit.
        for _ in 0..100_000 {
            let exp = 990 + (rng.next() % 70);
            let exp_b = (exp + rng.next() % 5).clamp(1, 2046);
            let sparse = !((1u64 << (rng.next() % 24)) - 1);
            let a = (rng.next() & SIGN_MASK64) | (exp << 52) | (rng.next() & FRAC_MASK64 & sparse);
            let b = (rng.next() & SIGN_MASK64) | (exp_b << 52) | (rng.next() & FRAC_MASK64 & sparse);
            let (fa, fb) = (f64::from_bits(a), f64::from_bits(b));
            assert_eq!(ref64_add_v2(a, b), hw_canon64(fa + fb), "add64-tie {a:016x} {b:016x}");
            assert_eq!(ref64_mul_v2(a, b), hw_canon64(fa * fb), "mul64-tie {a:016x} {b:016x}");
        }
    }

    #[test]
    fn widen_and_narrow_match_hardware_exactly() {
        for &a in &special_values() {
            assert_eq!(ref_widen_f32_to_f64_v2(a), hw_canon64(f32::from_bits(a) as f64), "widen {a:08x}");
        }
        for &a in &special_values64() {
            assert_eq!(ref_narrow_f64_to_f32_v2(a), hw_canon(f64::from_bits(a) as f32), "narrow {a:016x}");
        }
        let mut rng = DetRng(0xCA57_0000_0000_0001);
        for _ in 0..300_000 {
            let a32 = rng.next_u32();
            assert_eq!(ref_widen_f32_to_f64_v2(a32), hw_canon64(f32::from_bits(a32) as f64), "widen {a32:08x}");
            let a64 = rng.next();
            assert_eq!(ref_narrow_f64_to_f32_v2(a64), hw_canon(f64::from_bits(a64) as f32), "narrow {a64:016x}");
            // Round-trip: widen is exact, so narrow∘widen must be the identity on non-NaN.
            if !f32::from_bits(a32).is_nan() {
                assert_eq!(ref_narrow_f64_to_f32_v2(ref_widen_f32_to_f64_v2(a32)), a32);
            }
        }
        // Narrowing tie region: doubles between adjacent f32 values with sparse tails.
        for _ in 0..200_000 {
            let base = (rng.next_u32() & (SIGN_MASK | EXP_MASK)) | (rng.next_u32() & FRAC_MASK);
            if !ref_is_finite_v1(base) || (base & EXP_MASK) == 0 {
                continue;
            }
            let wide = ref_widen_f32_to_f64_v2(base);
            // Perturb the 29 dropped bits around the exact halfway point.
            for tail in [0x1000_0000u64 << 1, (0x1000_0000u64 << 1) - 1, (0x1000_0000u64 << 1) + 1, 1, (1u64 << 29) - 1] {
                let a64 = wide | tail >> 1;
                assert_eq!(ref_narrow_f64_to_f32_v2(a64), hw_canon(f64::from_bits(a64) as f32), "narrow-tie {a64:016x}");
            }
        }
    }

    #[test]
    fn f16_conversions_are_exact_and_rne() {
        // f16 → f32 is exact and injective away from NaN; round-tripping every one of the
        // 65 536 patterns is the exhaustive proof.
        for h in 0..=u16::MAX {
            let f = ref_f16_to_f32_v2(h);
            let exp = (h >> 10) & 0x1F;
            let frac = h & 0x3FF;
            if exp == 31 && frac != 0 {
                assert_eq!(f, PALW_REFERENCE_CANONICAL_NAN_V1, "f16 nan {h:04x}");
                assert_eq!(ref_f32_to_f16_v2(f), PALW_REFERENCE_CANONICAL_NAN16_V2);
            } else {
                assert_eq!(ref_f32_to_f16_v2(f), h, "f16 roundtrip {h:04x}");
                // Cross-check the widening against the hardware f64 value of the f16.
                let via_f64 = f64::from_bits(ref_widen_f32_to_f64_v2(f));
                let manual = manual_f16_value(h);
                assert_eq!(via_f64, manual, "f16 value {h:04x}");
            }
        }
        // RNE narrowing: for random f32, the result must be the nearest f16 (ties to even),
        // decided in exact f64 arithmetic (f64 represents every f32 and f16 exactly).
        let mut rng = DetRng(0xF16C_0000_0000_0001);
        let mut checked = 0u32;
        while checked < 200_000 {
            let bits = rng.next_u32();
            let x = f32::from_bits(bits);
            if x.is_nan() {
                continue;
            }
            checked += 1;
            let h = ref_f32_to_f16_v2(bits);
            let xv = x as f64;
            if xv.abs() >= 65520.0 {
                assert_eq!(h & 0x7FFF, 0x7C00, "overflow to inf {bits:08x}");
                assert_eq!(h & 0x8000 != 0, x.is_sign_negative());
                continue;
            }
            let hv = manual_f16_value(h);
            // Neighbors in encoding order (h is finite, magnitude < max here).
            let (lo, hi) = f16_neighbors(h);
            let (dv, dl, dh) = ((xv - hv).abs(), (xv - lo).abs(), (xv - hi).abs());
            assert!(dv <= dl && dv <= dh, "not nearest: {bits:08x} -> {h:04x} (|d|={dv}, lo={dl}, hi={dh})");
            if dv == dl || dv == dh {
                assert_eq!(h & 1, 0, "tie not to even: {bits:08x} -> {h:04x}");
            }
        }
        // Fixed anchors: max finite, first overflow, subnormal ties.
        assert_eq!(ref_f32_to_f16_v2(0x477F_E000), 0x7BFF); // 65504
        assert_eq!(ref_f32_to_f16_v2(0x477F_EFFF), 0x7BFF); // just under halfway stays
        assert_eq!(ref_f32_to_f16_v2(0x477F_F000), 0x7C00); // halfway: 65520 → Inf (tie toward even=Inf side)
        assert_eq!(ref_f32_to_f16_v2(0x3380_0000), 0x0001); // 2^-24 = min subnormal
        assert_eq!(ref_f32_to_f16_v2(0x3300_0000), 0x0000); // 2^-25: tie with zero → even (0)
        assert_eq!(ref_f32_to_f16_v2(0xB300_0000), 0x8000); // −2^-25 → −0
    }

    /// The mathematically exact value of a finite f16, computed without the code under test.
    fn manual_f16_value(h: u16) -> f64 {
        let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
        let exp = ((h >> 10) & 0x1F) as i32;
        let frac = (h & 0x3FF) as f64;
        if exp == 31 {
            return sign * f64::INFINITY;
        }
        if exp == 0 {
            sign * frac * (-24f64).exp2()
        } else {
            sign * (1024.0 + frac) * ((exp - 25) as f64).exp2()
        }
    }

    /// Exact values of the two finite-encoding neighbors of finite h (saturating at the ends).
    fn f16_neighbors(h: u16) -> (f64, f64) {
        let mag = h & 0x7FFF;
        let sign = h & 0x8000;
        let below = if mag == 0 { sign ^ 0x8000 | 1 } else { sign | (mag - 1) }; // crossing zero
        let above = if mag >= 0x7BFF { sign | 0x7BFF } else { sign | (mag + 1) };
        let (a, b) = (manual_f16_value(below), manual_f16_value(above));
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    /// fma really is fused (not mul-then-add): the v1 no-FMA witness, inverted.
    #[test]
    fn fma_is_fused_witness() {
        let a = (1.0f32 + f32::from_bits(0x3480_0000)).to_bits();
        let product_two_step = ref_mul_v1(a, a);
        let residual = ref_fma_v2(a, a, product_two_step ^ SIGN_MASK);
        assert_ne!(residual, 0, "fma failed to see the sub-ulp residual — it is not fused");
        assert_eq!(residual, hw_fma(a, a, product_two_step ^ SIGN_MASK));
        // And with a true zero addend it reduces to the single-rounded product.
        assert_eq!(ref_fma_v2(a, a, 0), product_two_step);
    }
}
