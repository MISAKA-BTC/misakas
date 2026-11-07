//! Differential tests: Berkeley-SoftFloat-backed `ref2_*` vs the normative
//! `palw_reference::ref_*_v1`, exact `u32` equality on every single case.
//!
//! The sweep constructions deliberately replicate the ones in
//! `consensus/core/src/palw_reference.rs`'s own test module (same special-value matrix, same
//! xorshift64* generator, same tie/subnormal neighborhoods), then run them 10x wider: what the
//! normative module checks against the hardware FPU, this crate checks against an independently
//! authored soft-float. Any single disagreement is a CRITICAL finding against the §29 gate-1
//! claim and must stop the line — never widen a tolerance here.

use kaspa_consensus_core::palw_reference::{
    PALW_REFERENCE_CANONICAL_NAN_V1, ref_add_v1, ref_dot_v1, ref_gemm_v1, ref_mul_v1, ref_neg_v1, ref_sub_v1,
};
use misaka_palw_reference2::{REF2_CANONICAL_NAN, ref2_add, ref2_dot, ref2_gemm, ref2_mul, ref2_neg, ref2_sub, ref2_sub_direct};

const SIGN_MASK: u32 = 0x8000_0000;
const EXP_MASK: u32 = 0x7F80_0000;
const FRAC_MASK: u32 = 0x007F_FFFF;
const HIDDEN_BIT: u32 = 0x0080_0000;

/// Deterministic xorshift64* — the same generator (and seeds) as the palw_reference tests:
/// no clock, no OS randomness, identical sequence every run on every machine.
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

/// The special-value matrix — replicated verbatim from the palw_reference test module:
/// every boundary its rounding paths branch on, ± both signs, plus three NaN shapes.
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

/// One scalar case, all four binary ops. Exact u32 equality or an immediate, bit-precise stop.
fn check_pair(a: u32, b: u32, ctx: &str) {
    assert_eq!(ref2_add(a, b), ref_add_v1(a, b), "CRITICAL add disagreement {ctx} a={a:08x} b={b:08x}");
    assert_eq!(ref2_mul(a, b), ref_mul_v1(a, b), "CRITICAL mul disagreement {ctx} a={a:08x} b={b:08x}");
    assert_eq!(ref2_sub(a, b), ref_sub_v1(a, b), "CRITICAL sub disagreement {ctx} a={a:08x} b={b:08x}");
    // f32_sub must agree with the pinned identity sub(a,b) = add(a, neg(b)) as well: the
    // identity is IEEE-754's own definition, so a divergence here indicts an implementation.
    assert_eq!(ref2_sub_direct(a, b), ref_sub_v1(a, b), "CRITICAL direct-sub disagreement {ctx} a={a:08x} b={b:08x}");
}

// ---------------------------------------------------------------------------------------------
// Contract pins
// ---------------------------------------------------------------------------------------------

#[test]
fn canonical_nan_constants_are_bit_identical() {
    assert_eq!(REF2_CANONICAL_NAN, PALW_REFERENCE_CANONICAL_NAN_V1);
}

/// The wrapper subtlety the crate exists to get right: SoftFloat's 8086-SSE rules PROPAGATE
/// NaN payloads (quieting them via `| 0x00400000`), while the frozen ruleset canonicalizes
/// every NaN operand. Pin that no payload — quiet, signaling, negative, all-ones — survives
/// any operation in either implementation.
#[test]
fn nan_operands_always_canonicalize_in_both_implementations() {
    for nan in [0x7FC0_0000u32, 0x7F80_0001, 0xFFC0_1234, 0x7FFF_FFFF, 0xFF80_0001] {
        for other in special_values() {
            for (result, name) in [
                (ref2_add(nan, other), "add(nan, x)"),
                (ref2_add(other, nan), "add(x, nan)"),
                (ref2_mul(nan, other), "mul(nan, x)"),
                (ref2_mul(other, nan), "mul(x, nan)"),
                (ref2_sub(nan, other), "sub(nan, x)"),
                (ref2_sub(other, nan), "sub(x, nan)"),
                (ref2_sub_direct(nan, other), "sub_direct(nan, x)"),
                (ref2_sub_direct(other, nan), "sub_direct(x, nan)"),
                (ref2_neg(nan), "neg(nan)"),
            ] {
                assert_eq!(result, REF2_CANONICAL_NAN, "payload leaked from {nan:08x} via {name}, other={other:08x}");
            }
        }
    }
}

/// Invalid operations reach SoftFloat itself (no NaN operand to pre-canonicalize) and produce
/// its 8086 default NaN 0xFFC00000 — the result-side canonicalization must convert it. Pinned
/// against the normative implementation AND as literal bits.
#[test]
fn invalid_operations_mint_only_the_canonical_nan() {
    let inf = 0x7F80_0000u32;
    for (a, b, op) in [
        (inf, inf | SIGN_MASK, "add"),
        (inf | SIGN_MASK, inf, "add"),
        (inf, 0, "mul"),
        (0, inf, "mul"),
        (SIGN_MASK, inf, "mul"),
        (inf | SIGN_MASK, SIGN_MASK, "mul"),
    ] {
        let (r2, r1) = match op {
            "add" => (ref2_add(a, b), ref_add_v1(a, b)),
            _ => (ref2_mul(a, b), ref_mul_v1(a, b)),
        };
        assert_eq!(r2, REF2_CANONICAL_NAN, "{op}({a:08x}, {b:08x})");
        assert_eq!(r1, REF2_CANONICAL_NAN, "{op}({a:08x}, {b:08x}) normative");
    }
    // Same-signed infinities are NOT invalid; both must agree there too.
    assert_eq!(ref2_add(inf, inf), ref_add_v1(inf, inf));
    assert_eq!(ref2_sub(inf, inf | SIGN_MASK), ref_sub_v1(inf, inf | SIGN_MASK));
}

/// The dot order witness from the normative tests: ascending and descending sums of the same
/// vector differ by one ulp, so agreement on BOTH proves ref2_dot pins the same order, not
/// merely the same scalar arithmetic.
#[test]
fn dot_order_witness_pins_the_same_reduction_order() {
    let a = [0x4B80_0000u32, 0x3F80_0000, 0x3F80_0000];
    let ones = [0x3F80_0000u32; 3];
    assert_eq!(ref2_dot(&a, &ones), 0x4B80_0000);
    assert_eq!(ref2_dot(&a, &ones), ref_dot_v1(&a, &ones).unwrap());
    let mut reversed = a;
    reversed.reverse();
    assert_eq!(ref2_dot(&reversed, &ones), 0x4B80_0001);
    assert_eq!(ref2_dot(&reversed, &ones), ref_dot_v1(&reversed, &ones).unwrap());
}

// ---------------------------------------------------------------------------------------------
// (a) Full special-value matrix — all ordered pairs, add + mul + sub (+ direct sub) + neg
// ---------------------------------------------------------------------------------------------

#[test]
fn special_matrix_all_ordered_pairs_agree_exactly() {
    let values = special_values();
    for &a in &values {
        assert_eq!(ref2_neg(a), ref_neg_v1(a), "CRITICAL neg disagreement a={a:08x}");
        for &b in &values {
            check_pair(a, b, "special-matrix");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// (b) 2,000,000 deterministic-xorshift random pairs
// ---------------------------------------------------------------------------------------------

#[test]
fn random_sweep_2m_pairs_agree_exactly() {
    let mut rng = DetRng(0x9E37_79B9_7F4A_7C15);
    for i in 0..2_000_000 {
        let a = rng.next_u32();
        let b = rng.next_u32();
        check_pair(a, b, &format!("random i={i}"));
    }
}

// ---------------------------------------------------------------------------------------------
// (c) 500,000 tie-neighborhood pairs (sparse low bits, close exponents — the construction
//     replicated from the palw_reference tests, where RNE ties actually live)
// ---------------------------------------------------------------------------------------------

#[test]
fn tie_neighborhood_500k_pairs_agree_exactly() {
    let mut rng = DetRng(0xD1B5_4A32_D192_ED03);
    for i in 0..500_000 {
        let exp = 110 + (rng.next_u32() % 40); // exponents in a ±20 band around 1.0
        let exp_b = exp.wrapping_add(rng.next_u32() % 5).clamp(1, 254);
        let sparse_mask = !((1u32 << (rng.next_u32() % 12)) - 1); // clear up to 11 low bits
        let a = ((rng.next_u32() & SIGN_MASK) | (exp << 23) | (rng.next_u32() & FRAC_MASK)) & (SIGN_MASK | EXP_MASK | sparse_mask);
        let b = ((rng.next_u32() & SIGN_MASK) | (exp_b << 23) | (rng.next_u32() & FRAC_MASK)) & (SIGN_MASK | EXP_MASK | sparse_mask);
        check_pair(a, b, &format!("tie i={i}"));
    }
}

// ---------------------------------------------------------------------------------------------
// (d) 500,000 subnormal-range pairs, including gradual-underflow products
// ---------------------------------------------------------------------------------------------

#[test]
fn subnormal_500k_pairs_agree_exactly() {
    let mut rng = DetRng(0xA076_1D64_78BD_642F);
    for i in 0..500_000 {
        // Both operands drawn from {subnormal, smallest-normal} binades, either sign.
        let a = (rng.next_u32() & SIGN_MASK) | (rng.next_u32() % (3 * HIDDEN_BIT));
        let b = (rng.next_u32() & SIGN_MASK) | (rng.next_u32() % (3 * HIDDEN_BIT));
        check_pair(a, b, &format!("subnormal i={i}"));
        // Products of a subnormal/small-normal with a mid-range value underflow gradually.
        let c = (rng.next_u32() & SIGN_MASK) | ((60 + (rng.next_u32() % 80)) << 23) | (rng.next_u32() & FRAC_MASK);
        assert_eq!(ref2_mul(a, c), ref_mul_v1(a, c), "CRITICAL gradual-underflow mul i={i} a={a:08x} c={c:08x}");
    }
}

// ---------------------------------------------------------------------------------------------
// (e) Random dot vectors and GEMM tiles — element-exact equality
// ---------------------------------------------------------------------------------------------

#[test]
fn dot_500_rounds_mixed_magnitude_agree_exactly() {
    let mut rng = DetRng(0x1234_5678_9ABC_DEF1);
    for round in 0..500 {
        let len = 1 + (rng.next_u32() as usize % 64);
        // Mixed magnitudes force heavy cancellation — the order-sensitive regime.
        let a: Vec<u32> = (0..len)
            .map(|_| (rng.next_u32() & SIGN_MASK) | ((64 + (rng.next_u32() % 128)) << 23) | (rng.next_u32() & FRAC_MASK))
            .collect();
        let b: Vec<u32> = (0..len)
            .map(|_| (rng.next_u32() & SIGN_MASK) | ((64 + (rng.next_u32() % 128)) << 23) | (rng.next_u32() & FRAC_MASK))
            .collect();
        assert_eq!(ref2_dot(&a, &b), ref_dot_v1(&a, &b).unwrap(), "CRITICAL dot disagreement round={round} a={a:08x?} b={b:08x?}");
    }
}

/// Dot with NaN / Inf / subnormal elements injected: both arithmetics are total, and the
/// canonical-NaN accumulator rule must flow identically through the whole reduction.
#[test]
fn dot_100_rounds_with_special_elements_agree_exactly() {
    let specials = special_values();
    let mut rng = DetRng(0x5EED_5EED_5EED_5EED);
    let draw = |rng: &mut DetRng| {
        if rng.next_u32().is_multiple_of(4) { specials[rng.next_u32() as usize % specials.len()] } else { rng.next_u32() }
    };
    for round in 0..100 {
        let len = 1 + (rng.next_u32() as usize % 32);
        let a: Vec<u32> = (0..len).map(|_| draw(&mut rng)).collect();
        let b: Vec<u32> = (0..len).map(|_| draw(&mut rng)).collect();
        assert_eq!(
            ref2_dot(&a, &b),
            ref_dot_v1(&a, &b).unwrap(),
            "CRITICAL special-dot disagreement round={round} a={a:08x?} b={b:08x?}"
        );
    }
}

#[test]
fn gemm_20_random_tiles_agree_element_exactly() {
    let mut rng = DetRng(0xFACE_FEED_0BAD_F00D);
    for round in 0..20 {
        let m = 1 + (rng.next_u32() as usize % 5);
        let n = 1 + (rng.next_u32() as usize % 5);
        let k = 1 + (rng.next_u32() as usize % 9);
        let a: Vec<u32> = (0..m * k)
            .map(|_| (rng.next_u32() & SIGN_MASK) | ((100 + (rng.next_u32() % 56)) << 23) | (rng.next_u32() & FRAC_MASK))
            .collect();
        let b: Vec<u32> = (0..k * n)
            .map(|_| (rng.next_u32() & SIGN_MASK) | ((100 + (rng.next_u32() % 56)) << 23) | (rng.next_u32() & FRAC_MASK))
            .collect();
        let c2 = ref2_gemm(&a, &b, m, n, k);
        let c1 = ref_gemm_v1(&a, &b, m, n, k).unwrap();
        assert_eq!(c2, c1, "CRITICAL gemm disagreement round={round} m={m} n={n} k={k}");
    }
}
