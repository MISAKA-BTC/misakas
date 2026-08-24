//! Differential tests for the ruleset-v2 surface: Berkeley-SoftFloat-backed `ref2_*` vs the
//! normative `palw_reference` `_v2` functions, exact bit equality on every single case.
//!
//! Same discipline as `differential.rs` (which covers the v1 surface): deterministic
//! xorshift64* sweeps, the special-value matrices the normative rounding paths branch on,
//! and an immediate bit-precise stop on any disagreement. Any single disagreement is a
//! CRITICAL finding against the §29 gate-1 claim and must stop the line — never widen a
//! tolerance here.
//!
//! New in v2 and pinned here: fused multiply-add (one rounding, both widths), correctly
//! rounded division and square root, the binary64 family, exact f32→f64 / f16→f32 widening,
//! RNE f64→f32 / f32→f16 narrowing, and per-width canonical NaNs (`0x7FC00000` /
//! `0x7FF8000000000000` / `0x7E00`). The f16→f32 direction is EXHAUSTIVE (all 65 536 inputs).

use kaspa_consensus_core::palw_reference::{
    PALW_REFERENCE_CANONICAL_NAN16_V2, PALW_REFERENCE_CANONICAL_NAN64_V2, ref_div_v2, ref_f16_to_f32_v2, ref_f32_to_f16_v2,
    ref_fma_v2, ref_narrow_f64_to_f32_v2, ref_sqrt_v2, ref_widen_f32_to_f64_v2, ref64_add_v2, ref64_div_v2, ref64_fma_v2,
    ref64_mul_v2, ref64_neg_v2, ref64_sub_v2,
};
use misaka_palw_reference2::{
    REF2_CANONICAL_NAN16, REF2_CANONICAL_NAN64, ref2_add64, ref2_div, ref2_div64, ref2_f16_to_f32, ref2_f32_to_f16, ref2_fma,
    ref2_fma64, ref2_mul64, ref2_narrow_f64_to_f32, ref2_neg64, ref2_sqrt, ref2_sub64, ref2_sub64_direct, ref2_widen_f32_to_f64,
};

const SIGN_MASK: u32 = 0x8000_0000;
const SIGN_MASK64: u64 = 0x8000_0000_0000_0000;

/// Deterministic xorshift64* — the same generator as the v1 differential sweeps; distinct
/// seeds per test keep the streams independent without any OS randomness.
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

/// The binary32 special-value matrix — replicated verbatim from `differential.rs`.
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

/// The binary64 twin of the special-value matrix: the same boundary structure in the wider
/// format (subnormal edges, hidden-bit edges, powers straddling the 2^52/2^53 integer window,
/// max finite, infinities, three NaN shapes).
fn special_values64() -> Vec<u64> {
    let mut magnitudes: Vec<u64> = vec![
        0x0000_0000_0000_0000, // +0
        0x0000_0000_0000_0001, // min subnormal
        0x0000_0000_0000_0002,
        0x0000_0000_0000_0003,
        0x0008_0000_0000_0000, // mid subnormal
        0x000F_FFFF_FFFF_FFFF, // max subnormal
        0x0010_0000_0000_0000, // min normal
        0x0010_0000_0000_0001,
        0x001F_FFFF_FFFF_FFFF,
        0x3FE0_0000_0000_0000, // 0.5
        0x3FE8_0000_0000_0000, // 0.75
        0x3FEF_FFFF_FFFF_FFFF, // just under 1
        0x3FF0_0000_0000_0000, // 1.0
        0x3FF0_0000_0000_0001, // 1 + ulp
        0x3FF8_0000_0000_0000, // 1.5
        0x4000_0000_0000_0000, // 2.0
        0x3FFF_FFFF_FFFF_FFFF, // just under 2
        0x4340_0000_0000_0000, // 2^53
        0x4340_0000_0000_0001,
        0x4330_0000_0000_0000, // 2^52
        0x7FE0_0000_0000_0000, // 2^1023
        0x7FEF_FFFF_FFFF_FFFE,
        0x7FEF_FFFF_FFFF_FFFF, // max finite
        0x7FF0_0000_0000_0000, // +Inf
    ];
    let negatives: Vec<u64> = magnitudes.iter().map(|m| m | SIGN_MASK64).collect();
    magnitudes.extend(negatives);
    magnitudes.push(0x7FF8_0000_0000_0000); // canonical qNaN
    magnitudes.push(0x7FF0_0000_0000_0001); // signaling-NaN pattern
    magnitudes.push(0xFFF8_0000_0000_1234); // negative NaN with payload
    magnitudes
}

/// One binary32 pair through division. Exact u32 equality or an immediate stop.
fn check_div(a: u32, b: u32, ctx: &str) {
    assert_eq!(ref2_div(a, b), ref_div_v2(a, b), "CRITICAL div disagreement {ctx} a={a:08x} b={b:08x}");
}

/// One binary32 triple through fused multiply-add.
fn check_fma(a: u32, b: u32, c: u32, ctx: &str) {
    assert_eq!(ref2_fma(a, b, c), ref_fma_v2(a, b, c), "CRITICAL fma disagreement {ctx} a={a:08x} b={b:08x} c={c:08x}");
}

/// One binary64 pair through the whole two-operand v2 family (add, the pinned sub identity,
/// SoftFloat's own sub, mul, div).
fn check_pair64(a: u64, b: u64, ctx: &str) {
    assert_eq!(ref2_add64(a, b), ref64_add_v2(a, b), "CRITICAL add64 disagreement {ctx} a={a:016x} b={b:016x}");
    assert_eq!(ref2_sub64(a, b), ref64_sub_v2(a, b), "CRITICAL sub64 disagreement {ctx} a={a:016x} b={b:016x}");
    assert_eq!(ref2_sub64_direct(a, b), ref64_sub_v2(a, b), "CRITICAL direct-sub64 disagreement {ctx} a={a:016x} b={b:016x}");
    assert_eq!(ref2_mul64(a, b), ref64_mul_v2(a, b), "CRITICAL mul64 disagreement {ctx} a={a:016x} b={b:016x}");
    assert_eq!(ref2_div64(a, b), ref64_div_v2(a, b), "CRITICAL div64 disagreement {ctx} a={a:016x} b={b:016x}");
}

/// One binary64 triple through fused multiply-add.
fn check_fma64(a: u64, b: u64, c: u64, ctx: &str) {
    assert_eq!(ref2_fma64(a, b, c), ref64_fma_v2(a, b, c), "CRITICAL fma64 disagreement {ctx} a={a:016x} b={b:016x} c={c:016x}");
}

/// The unary v2 surface for one binary32 value: square root, exact widening, and the RNE
/// narrowing to binary16.
fn check_unary32(a: u32, ctx: &str) {
    assert_eq!(ref2_sqrt(a), ref_sqrt_v2(a), "CRITICAL sqrt disagreement {ctx} a={a:08x}");
    assert_eq!(ref2_widen_f32_to_f64(a), ref_widen_f32_to_f64_v2(a), "CRITICAL widen disagreement {ctx} a={a:08x}");
    assert_eq!(ref2_f32_to_f16(a), ref_f32_to_f16_v2(a), "CRITICAL f32->f16 disagreement {ctx} a={a:08x}");
}

// ---------------------------------------------------------------------------------------------
// Contract pins
// ---------------------------------------------------------------------------------------------

#[test]
fn canonical_nan_constants_are_bit_identical_v2() {
    assert_eq!(REF2_CANONICAL_NAN64, PALW_REFERENCE_CANONICAL_NAN64_V2);
    assert_eq!(REF2_CANONICAL_NAN16, PALW_REFERENCE_CANONICAL_NAN16_V2);
}

/// No NaN payload — quiet, signaling, negative, all-ones — survives any v2 operation in
/// either implementation, in any width, and the result is always the canonical NaN of the
/// RESULT width.
#[test]
fn nan_operands_always_canonicalize_in_both_implementations_v2() {
    let one = 0x3F80_0000u32;
    for nan in [0x7FC0_0000u32, 0x7F80_0001, 0xFFC0_1234, 0x7FFF_FFFF, 0xFF80_0001] {
        for (r2, r1) in [
            (ref2_div(nan, one), ref_div_v2(nan, one)),
            (ref2_div(one, nan), ref_div_v2(one, nan)),
            (ref2_sqrt(nan), ref_sqrt_v2(nan)),
            (ref2_fma(nan, one, one), ref_fma_v2(nan, one, one)),
            (ref2_fma(one, nan, one), ref_fma_v2(one, nan, one)),
            (ref2_fma(one, one, nan), ref_fma_v2(one, one, nan)),
            (ref2_f16_to_f32(0x7E01), ref_f16_to_f32_v2(0x7E01)),
            (ref2_f16_to_f32(0xFDAB), ref_f16_to_f32_v2(0xFDAB)),
        ] {
            assert_eq!(r2, 0x7FC0_0000, "a NaN operand must canonicalize to the binary32 canonical NaN");
            assert_eq!(r2, r1);
        }
        assert_eq!(ref2_widen_f32_to_f64(nan), REF2_CANONICAL_NAN64, "widening a NaN canonicalizes in the RESULT width");
        assert_eq!(ref2_widen_f32_to_f64(nan), ref_widen_f32_to_f64_v2(nan));
        assert_eq!(ref2_f32_to_f16(nan), REF2_CANONICAL_NAN16);
        assert_eq!(ref2_f32_to_f16(nan), ref_f32_to_f16_v2(nan));
    }
    let one64 = 0x3FF0_0000_0000_0000u64;
    for nan in [0x7FF8_0000_0000_0000u64, 0x7FF0_0000_0000_0001, 0xFFF8_0000_0000_1234, 0x7FFF_FFFF_FFFF_FFFF] {
        for (r2, r1) in [
            (ref2_add64(nan, one64), ref64_add_v2(nan, one64)),
            (ref2_sub64(one64, nan), ref64_sub_v2(one64, nan)),
            (ref2_mul64(nan, one64), ref64_mul_v2(nan, one64)),
            (ref2_div64(one64, nan), ref64_div_v2(one64, nan)),
            (ref2_fma64(one64, nan, one64), ref64_fma_v2(one64, nan, one64)),
            (ref2_neg64(nan), ref64_neg_v2(nan)),
        ] {
            assert_eq!(r2, REF2_CANONICAL_NAN64, "a NaN operand must canonicalize to the binary64 canonical NaN");
            assert_eq!(r2, r1);
        }
        assert_eq!(ref2_narrow_f64_to_f32(nan), 0x7FC0_0000, "narrowing a NaN canonicalizes in the RESULT width");
        assert_eq!(ref2_narrow_f64_to_f32(nan), ref_narrow_f64_to_f32_v2(nan));
    }
}

/// The v2 invalid operations mint only the canonical NaN of their width, in both
/// implementations: `0/0`, `Inf/Inf`, `0 × Inf` (through fma), `Inf − Inf` (through fma's
/// addend), and the square root of any negative non-zero value.
#[test]
fn invalid_operations_mint_only_the_canonical_nan_v2() {
    let zero = 0x0000_0000u32;
    let nzero = SIGN_MASK;
    let inf = 0x7F80_0000u32;
    let ninf = 0xFF80_0000u32;
    let one = 0x3F80_0000u32;
    for (a, b) in [(zero, zero), (zero, nzero), (nzero, nzero), (inf, inf), (inf, ninf), (ninf, ninf)] {
        assert_eq!(ref2_div(a, b), 0x7FC0_0000, "div invalid {a:08x}/{b:08x}");
        check_div(a, b, "invalid");
    }
    for (a, b, c) in [
        (zero, inf, one),
        (inf, zero, one),
        (nzero, inf, zero),
        (inf, one, ninf), // Inf − Inf via the addend
        (ninf, one, inf),
    ] {
        assert_eq!(ref2_fma(a, b, c), 0x7FC0_0000, "fma invalid {a:08x}·{b:08x}+{c:08x}");
        check_fma(a, b, c, "invalid");
    }
    for a in [0xBF80_0000u32, 0x8000_0001, 0xFF80_0000, 0xFF7F_FFFF] {
        assert_eq!(ref2_sqrt(a), 0x7FC0_0000, "sqrt of negative {a:08x}");
        check_unary32(a, "invalid-sqrt");
    }
    // sqrt(±0) = ±0 — NOT an invalid operation; the sign survives.
    assert_eq!(ref2_sqrt(zero), ref_sqrt_v2(zero));
    assert_eq!(ref2_sqrt(nzero), ref_sqrt_v2(nzero));
    assert_eq!(ref2_sqrt(nzero), nzero);
    let zero64 = 0u64;
    let inf64 = 0x7FF0_0000_0000_0000u64;
    let ninf64 = 0xFFF0_0000_0000_0000u64;
    let one64 = 0x3FF0_0000_0000_0000u64;
    for (a, b) in [(zero64, zero64), (inf64, inf64), (inf64, ninf64)] {
        assert_eq!(ref2_div64(a, b), REF2_CANONICAL_NAN64, "div64 invalid");
        check_pair64(a, b, "invalid");
    }
    assert_eq!(ref2_add64(inf64, ninf64), REF2_CANONICAL_NAN64, "Inf − Inf through add64");
    for (a, b, c) in [(zero64, inf64, one64), (inf64, zero64, one64), (inf64, one64, ninf64)] {
        assert_eq!(ref2_fma64(a, b, c), REF2_CANONICAL_NAN64, "fma64 invalid");
        check_fma64(a, b, c, "invalid");
    }
}

// ---------------------------------------------------------------------------------------------
// Exhaustive and special-matrix sweeps
// ---------------------------------------------------------------------------------------------

/// binary16 → binary32 widening is checked on EVERY possible input — all 65 536 of them —
/// and the widening of every non-NaN f16 must round-trip exactly through the RNE narrowing
/// (every binary16 value is exactly representable in binary32).
#[test]
fn f16_to_f32_exhaustive_and_roundtrip() {
    for bits in 0..=u16::MAX {
        let widened2 = ref2_f16_to_f32(bits);
        let widened1 = ref_f16_to_f32_v2(bits);
        assert_eq!(widened2, widened1, "CRITICAL f16->f32 disagreement bits={bits:04x}");
        let is_nan = (bits & 0x7FFF) > 0x7C00;
        if is_nan {
            assert_eq!(widened2, 0x7FC0_0000, "NaN widens to the binary32 canonical NaN");
        } else {
            let back2 = ref2_f32_to_f16(widened2);
            assert_eq!(back2, ref_f32_to_f16_v2(widened2), "CRITICAL f32->f16 disagreement bits={widened2:08x}");
            assert_eq!(back2, bits, "f16 -> f32 -> f16 must be the identity for {bits:04x}");
        }
    }
}

/// Every ordered pair from the binary32 special-value matrix through division, and every
/// value through the unary surface (sqrt / widen / narrow-to-f16).
#[test]
fn special_matrix_div_and_unary_agree_exactly() {
    let values = special_values();
    for &a in &values {
        check_unary32(a, "special");
        for &b in &values {
            check_div(a, b, "special");
        }
    }
}

/// Every ordered TRIPLE from the binary32 special-value matrix through fused multiply-add —
/// the fma corner space (0 × Inf against every addend, massive cancellation against the
/// sticky horizon, subnormal products) is three-dimensional, so the matrix is cubed.
#[test]
fn special_matrix_fma_all_ordered_triples_agree_exactly() {
    let values = special_values();
    for &a in &values {
        for &b in &values {
            for &c in &values {
                check_fma(a, b, c, "special-cube");
            }
        }
    }
}

/// Every ordered pair from the binary64 special-value matrix through the two-operand family,
/// and negation over the whole matrix.
#[test]
fn special_matrix64_all_ordered_pairs_agree_exactly() {
    let values = special_values64();
    for &a in &values {
        assert_eq!(ref2_neg64(a), ref64_neg_v2(a), "CRITICAL neg64 disagreement a={a:016x}");
        assert_eq!(ref2_narrow_f64_to_f32(a), ref_narrow_f64_to_f32_v2(a), "CRITICAL narrow disagreement a={a:016x}");
        for &b in &values {
            check_pair64(a, b, "special64");
        }
    }
}

/// Every ordered triple from the binary64 special-value matrix through fma64.
#[test]
fn special_matrix64_fma_all_ordered_triples_agree_exactly() {
    let values = special_values64();
    for &a in &values {
        for &b in &values {
            for &c in &values {
                check_fma64(a, b, c, "special64-cube");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Randomized wide sweeps
// ---------------------------------------------------------------------------------------------

/// 1M uniformly random binary32 pairs through division, interleaved with the unary surface
/// on each operand.
#[test]
fn random_sweep_1m_div_and_unary_agree_exactly() {
    let mut rng = DetRng(0x5EED_0002_D1FF_0001);
    for i in 0..1_000_000u32 {
        let a = rng.next_u32();
        let b = rng.next_u32();
        check_div(a, b, &format!("random #{i}"));
        if i % 4 == 0 {
            check_unary32(a, &format!("random #{i}"));
        }
    }
}

/// 1M uniformly random binary32 triples through fused multiply-add.
#[test]
fn random_sweep_1m_fma_triples_agree_exactly() {
    let mut rng = DetRng(0x5EED_0002_D1FF_0002);
    for i in 0..1_000_000u32 {
        let (a, b, c) = (rng.next_u32(), rng.next_u32(), rng.next_u32());
        check_fma(a, b, c, &format!("random #{i}"));
    }
}

/// 1M uniformly random binary64 pairs through the two-operand family, narrowing each first
/// operand as it goes.
#[test]
fn random_sweep_1m_pairs64_agree_exactly() {
    let mut rng = DetRng(0x5EED_0002_D1FF_0003);
    for i in 0..1_000_000u32 {
        let a = rng.next();
        let b = rng.next();
        check_pair64(a, b, &format!("random #{i}"));
        if i % 4 == 0 {
            assert_eq!(ref2_narrow_f64_to_f32(a), ref_narrow_f64_to_f32_v2(a), "CRITICAL narrow disagreement a={a:016x}");
        }
    }
}

/// 500k uniformly random binary64 triples through fma64.
#[test]
fn random_sweep_500k_fma64_triples_agree_exactly() {
    let mut rng = DetRng(0x5EED_0002_D1FF_0004);
    for i in 0..500_000u32 {
        let (a, b, c) = (rng.next(), rng.next(), rng.next());
        check_fma64(a, b, c, &format!("random #{i}"));
    }
}

/// 500k division pairs constrained to the subnormal / near-subnormal band, where the
/// quotient's normalization and jam paths live.
#[test]
fn subnormal_band_500k_div_pairs_agree_exactly() {
    let mut rng = DetRng(0x5EED_0002_D1FF_0005);
    for i in 0..500_000u32 {
        // Exponent fields 0..=2: subnormals and the first two normal binades, both signs.
        let a = (rng.next_u32() & 0x807F_FFFF) | ((rng.next_u32() % 3) << 23);
        let b = (rng.next_u32() & 0x807F_FFFF) | ((rng.next_u32() % 3) << 23);
        check_div(a, b, &format!("subnormal #{i}"));
        check_fma(a, b, rng.next_u32(), &format!("subnormal-fma #{i}"));
    }
}

/// 500k widen/narrow round trips: `f32 → f64` widening is exact, so narrowing back must be
/// the identity for every non-NaN input; NaNs canonicalize in the result width.
#[test]
fn widen_narrow_roundtrip_500k_agree_exactly() {
    let mut rng = DetRng(0x5EED_0002_D1FF_0006);
    for i in 0..500_000u32 {
        let a = rng.next_u32();
        let wide2 = ref2_widen_f32_to_f64(a);
        assert_eq!(wide2, ref_widen_f32_to_f64_v2(a), "CRITICAL widen disagreement #{i} a={a:08x}");
        let back = ref2_narrow_f64_to_f32(wide2);
        assert_eq!(back, ref_narrow_f64_to_f32_v2(wide2), "CRITICAL narrow disagreement #{i} a={a:08x}");
        let is_nan = (a & 0x7FFF_FFFF) > 0x7F80_0000;
        if is_nan {
            assert_eq!(back, 0x7FC0_0000);
        } else {
            assert_eq!(back, a, "widen∘narrow must be the identity for non-NaN {a:08x}");
        }
    }
}

/// 500k random f32 → f16 narrowings (RNE, overflow to infinity, subnormal underflow) — the
/// KV-cache write seam.
#[test]
fn f32_to_f16_random_500k_agree_exactly() {
    let mut rng = DetRng(0x5EED_0002_D1FF_0007);
    for i in 0..500_000u32 {
        let a = rng.next_u32();
        assert_eq!(ref2_f32_to_f16(a), ref_f32_to_f16_v2(a), "CRITICAL f32->f16 disagreement #{i} a={a:08x}");
        // And in the f16-representable neighborhood, where rounding actually decides:
        // squash the exponent into [104, 143] (2^-24 .. 2^16).
        let squashed = (a & 0x807F_FFFF) | ((104 + (rng.next_u32() % 40)) << 23);
        assert_eq!(
            ref2_f32_to_f16(squashed),
            ref_f32_to_f16_v2(squashed),
            "CRITICAL f32->f16 disagreement #{i} squashed={squashed:08x}"
        );
    }
}
