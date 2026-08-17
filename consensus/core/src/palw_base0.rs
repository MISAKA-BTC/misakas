//! ADR-0040: `PALW-BASE-0`'s integer-only arithmetic — the six primitives, normatively.
//!
//! Everything a conforming `PALW-BASE-0` implementation must reproduce bit-for-bit reduces to the
//! functions here:
//!
//! ```text
//! integer add, integer multiply    (native, exact — not wrapped in a function)
//! rounding_shift_right             the ONLY site where a value loses information
//! srdhm                            the ONE fixed-point multiply
//! int_exp                          softmax, and (through sigmoid) SiLU
//! int_rsqrt                        RMS norm
//! ```
//!
//! All are total functions on `i32`/`i64`. There is no rounding mode, no denormal, no `errno`, no
//! libm version and no contraction flag anywhere in this module, which is the entire reason
//! ADR-0039 orders this class first: its kernel catalog can actually close, and until a class's
//! catalog closes it may not carry fork-choice weight.
//!
//! # No floating point, at all
//!
//! Not one `f32`/`f64` appears below, including in the constants — those are integer literals,
//! derived once and pinned by [`tests::constants_are_the_pinned_derivations`] rather than
//! recomputed from a float expression at runtime. ADR-0040 Decision A: a build that links `libm`
//! for this class is not a conforming implementation.
//!
//! # The associativity that makes this class worth building
//!
//! Integer addition is exactly associative and commutative, so the order a dot product, a norm
//! sum or a softmax denominator is accumulated in **cannot change the result** — across thread
//! counts, SIMD widths, tile shapes, compilers or vendors (ADR-0040 Decision E). That holds only
//! while accumulation cannot overflow, because saturating addition is *not* associative at the
//! boundary — which is why [`MAX_DOT_LEN`] is a premise of the property rather than a safety
//! nicety.

/// Fixed-point fraction bits. Every `Qk` value below is an integer scaled by `2^K`.
pub const K: u32 = 24;
/// `1.0` in Qk.
pub const ONE: i64 = 1i64 << K;

/// `round(ln 2 · 2^K)` — the range-reduction step for [`int_exp`].
pub const LN2_Q: i32 = 11_629_080;

/// [`int_exp`]'s shifted-square coefficients: `Poly2(p) = A·(p + B)² + C`, all Qk.
///
/// **These are not Horner coefficients.** `A(p+B)² + C` and `Ap² + Bp + C` read the same three
/// numbers and are different algorithms: the shifted-square form hits `Poly2(0) ≈ 1.0` and
/// `Poly2(−ln2) ≈ 0.5`, the two endpoints `exp` must reproduce, while the Horner reading gives
/// `Poly2(0) = 0.344` — every result low by a factor of three, uniformly enough to look like a
/// scale bug rather than a wrong algorithm. ADR-0040 F1 records the error because it was made.
pub const POLY2_A: i64 = 6_014_632;
pub const POLY2_B: i64 = 22_699_573;
pub const POLY2_C: i64 = 5_771_362;

/// Largest range-reduction shift. Beyond it `exp` is below the Qk floor and the result is 0
/// exactly; capping here also keeps the shift a legal one for `i32`.
pub const Z_MAX: i32 = 31;

/// Newton iterations in [`int_rsqrt`]. Pinned, not convergence-tested: a loop that stops when it
/// converges stops at different times on different inputs, and "different times" is a divergence.
/// Measured at 3; 4 and 5 are not more accurate (ADR-0040 F2).
pub const RSQRT_ITERS: u32 = 3;

/// [`int_rsqrt`]'s seed table, indexed by the top four bits of the normalized mantissa.
///
/// **A correctness requirement, not an optimisation.** Newton for `1/√v` converges only from
/// `y₀ ≤ √(3/m)`; a seed above that basin diverges *to zero* rather than oscillating, so the
/// failure is silent and total. Every entry is the reciprocal square root of its bucket's UPPER
/// end, which is conservative by construction. A first draft seeded from the leading bit alone
/// landed exactly on the boundary at `m = 3` and returned 0 (ADR-0040 F2).
pub const RSQRT_SEED: [i64; 16] = [
    15_395_829, 14_307_657, 13_421_772, 12_682_383, 12_053_107, 11_509_075, 11_032_629, 10_610_843, 10_234_005, 9_894_662,
    9_586_980, 9_306_325, 9_048_957, 8_811_825, 8_592_409, 8_388_608,
];

/// The longest `int8 × int8` dot product that provably cannot overflow an `i32` accumulator:
/// `|product| ≤ 127 × 127 = 16_129`, so `MAX_DOT_LEN × 16_129 ≤ i32::MAX`.
///
/// ADR-0040 C3. This bound is what licenses Decision E's free reduction order, so a graph that
/// exceeds it does not merely risk overflow — it loses the property the class exists for, and
/// must accumulate in `i64` and say so.
pub const MAX_DOT_LEN: usize = 133_144;

/// Round-half-away-from-zero shift — the ONE site in the whole class where a value loses
/// information (ADR-0040 C1). Every other operation here is exact.
#[inline]
pub fn rounding_shift_right(x: i32, s: u8) -> i32 {
    debug_assert!(s <= 31);
    if s == 0 {
        return x;
    }
    let round = 1i32 << (s - 1);
    // The add cannot overflow for any `x` an in-range accumulator produces, and `wrapping_add`
    // states that rather than leaving a debug-only panic on the consensus path.
    (x.wrapping_add(if x >= 0 { round } else { -round })) >> s
}

/// Saturating rounding doubling high multiply — the ONE fixed-point multiply (ADR-0040 C2).
///
/// gemmlowp's primitive verbatim, deliberately: it is already implemented identically in several
/// independent codebases, which is exactly the property a second implementation needs.
#[inline]
pub fn srdhm(a: i32, b: i32) -> i32 {
    // The single saturating case: -2^31 × -2^31 doubled is 2^63, one past `i64`'s range.
    if a == i32::MIN && b == i32::MIN {
        return i32::MAX;
    }
    // `a · b`, NOT `2 · a · b`. The "doubling" in the name describes the relationship to the
    // hardware VQRDMULH — `(a·b) >> 31` IS `(2·a·b) >> 32` — it is not an extra factor to apply.
    // Writing the 2 explicitly and still shifting by 31 doubles every product and overflows `i32`
    // at the top of the range: 0.5 × 0.5 returns 0.5 instead of 0.25.
    let product = (a as i64) * (b as i64);
    let nudge: i64 = if product >= 0 { 1 << 30 } else { 1 - (1 << 30) };
    ((product + nudge) >> 31) as i32
}

/// `i32` accumulator → `int8`, the explicit narrowing (ADR-0040 D op 2). Saturates; nothing in
/// this class wraps.
#[inline]
pub fn requantize(acc: i32, multiplier: i32, shift: u8) -> i8 {
    rounding_shift_right(srdhm(acc, multiplier), shift).clamp(-128, 127) as i8
}

/// `Poly2(p) = A·(p + B)² + C` on `p ∈ (−LN2_Q, 0]`, Qk in and out.
#[inline]
fn poly2(p: i32) -> i32 {
    let t = p as i64 + POLY2_B;
    let square = (t * t) >> K;
    (((POLY2_A * square) >> K) + POLY2_C) as i32
}

/// `exp(x)` for `x ≤ 0`, Qk in and out (ADR-0040 F1).
///
/// Softmax subtracts the row max first, so `x ≤ 0` always holds — that subtraction is part of the
/// op, not an optimisation. A positive input is clamped to 0 rather than being undefined, because
/// an undefined case is a place two implementations can differ.
#[inline]
pub fn int_exp(x: i32) -> i32 {
    let x = x.min(0);
    let z = (-(x as i64) / LN2_Q as i64).min(Z_MAX as i64) as i32;
    if z >= Z_MAX {
        // Below the Qk floor: exp(−31·ln2) ≈ 4.6e−10 against a resolution of 2^−24 ≈ 6e−8.
        return 0;
    }
    rounding_shift_right(poly2(x + z * LN2_Q), z as u8)
}

/// `1/√v` for `v > 0`, Qk in and out (ADR-0040 F2). Returns 0 for `v ≤ 0` — the norm paths add
/// `eps` before calling, so a non-positive input is already a caller error rather than a value to
/// invent an answer for.
pub fn int_rsqrt(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    // Normalize v = m · 2^(2e) with m ∈ [1, 4), so the seed table indexes a fixed range.
    let bit = 63 - v.leading_zeros() as i32;
    let mut e = (bit - K as i32).div_euclid(2);
    // `div_euclid` floors toward negative infinity, which is what keeps m in [1,4) for v < 1.
    let mut m = if 2 * e >= 0 { v >> (2 * e) } else { v << (-2 * e) };
    // Guard the boundary: rounding can leave m one step outside [1,4) for extreme inputs.
    while m >= 4 * ONE {
        m >>= 2;
        e += 1;
    }
    while m < ONE {
        m <<= 2;
        e -= 1;
    }
    let index = (((m - ONE) * 16) / (3 * ONE)).clamp(0, 15) as usize;
    let mut y = RSQRT_SEED[index];
    for _ in 0..RSQRT_ITERS {
        let y2 = (y * y) >> K;
        let my2 = (m * y2) >> K;
        y = (y * (3 * ONE - my2)) >> (K + 1);
        if y <= 0 {
            y = 1;
        }
    }
    if e >= 0 { y >> e } else { y << (-e) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Qk → a rational check without floats: assert `value` is within `tol_num/tol_den` of
    /// `want_num/want_den`, in integer arithmetic only.
    fn close(value: i64, want_num: i64, want_den: i64, tol_ppm: i64) -> bool {
        // |value·want_den − want_num·ONE| · 1e6 ≤ tol_ppm · |want_num·ONE|
        let lhs = (value * want_den - want_num * ONE).abs();
        let rhs = want_num.abs() * ONE;
        lhs * 1_000_000 <= tol_ppm * rhs
    }

    /// The constants are the pinned derivations, checked in integers so the module never needs a
    /// float expression to justify itself.
    #[test]
    fn constants_are_the_pinned_derivations() {
        assert_eq!(ONE, 16_777_216);
        assert_eq!(MAX_DOT_LEN as i64, (i32::MAX as i64) / 16_129);
        // ln2 in Qk, bracketed: 0.693147 < LN2_Q/ONE < 0.693148
        assert!((LN2_Q as i64) * 1_000_000 > 693_147 * ONE);
        assert!((LN2_Q as i64) * 1_000_000 < 693_148 * ONE);
        assert_eq!(RSQRT_SEED.len(), 16);
        // The seed table is monotonically decreasing: bucket upper ends increase.
        for w in RSQRT_SEED.windows(2) {
            assert!(w[0] > w[1], "seeds must decrease across buckets");
        }
    }

    /// **The endpoints that distinguish the shifted-square form from the Horner reading.**
    ///
    /// `Poly2(0) ≈ 1.0` and `Poly2(−ln2) ≈ 0.5` are the two values `exp` must hit. Reading
    /// `A, B, C` as Horner coefficients gives `Poly2(0) = 0.344` — this test is what catches that,
    /// and it is the regression for the error ADR-0040 F1 records.
    #[test]
    fn poly2_hits_the_two_endpoints_exp_must() {
        assert!(close(poly2(0) as i64, 1, 1, 1_000), "poly2(0) must be 1.0, not 0.344");
        assert!(close(poly2(-LN2_Q) as i64, 1, 2, 1_000), "poly2(-ln2) must be 0.5");
    }

    /// `int_exp` against known values, to a tolerance ADR-0040 F1 measured (0.5 %).
    #[test]
    fn int_exp_matches_known_values() {
        assert!(close(int_exp(0) as i64, 1, 1, 5_000), "exp(0) = 1");
        assert!(close(int_exp(-LN2_Q) as i64, 1, 2, 5_000), "exp(-ln2) = 1/2");
        assert!(close(int_exp(-2 * LN2_Q) as i64, 1, 4, 5_000), "exp(-2ln2) = 1/4");
        assert!(close(int_exp(-10 * LN2_Q) as i64, 1, 1024, 5_000), "exp(-10ln2) = 1/1024");
        // Monotone non-increasing across the whole domain, and never negative.
        let mut previous = int_exp(0);
        for step in 1..4_000i64 {
            // i64 intermediate: `step · LN2_Q` passes i32::MAX well before the sweep ends.
            let value = int_exp((-(step * LN2_Q as i64) / 64) as i32);
            assert!(value <= previous, "exp must not increase as x decreases (step {step})");
            assert!(value >= 0, "exp is never negative");
            previous = value;
        }
        // Far below the Qk floor is exactly zero, not garbage.
        assert_eq!(int_exp(i32::MIN), 0);
        // A positive input is clamped, never undefined.
        assert_eq!(int_exp(1_000_000), int_exp(0));
    }

    /// **The seed-basin regression.** Newton for `1/√v` diverges to ZERO from a seed above
    /// `√(3/m)`, and `m = 3` is exactly where a leading-bit-only seed lands. ADR-0040 F2.
    #[test]
    fn int_rsqrt_does_not_collapse_at_the_basin_boundary() {
        // v = 3.0 — the input that returned 0 before the seed table existed.
        let y = int_rsqrt(3 * ONE);
        assert!(y > 0, "rsqrt(3) must not be zero — that is the divergence-to-zero failure");
        assert!(close(y, 57_735, 100_000, 10_000), "1/sqrt(3) ≈ 0.57735");
    }

    /// Accuracy and range: ADR-0040 F2 measured 6.4e−6 at N = 3.
    #[test]
    fn int_rsqrt_is_accurate_across_the_range() {
        // 1/sqrt(1) = 1, 1/sqrt(4) = 1/2, 1/sqrt(16) = 1/4, 1/sqrt(1/4) = 2.
        assert!(close(int_rsqrt(ONE), 1, 1, 1_000));
        assert!(close(int_rsqrt(4 * ONE), 1, 2, 1_000));
        assert!(close(int_rsqrt(16 * ONE), 1, 4, 1_000));
        assert!(close(int_rsqrt(ONE / 4), 2, 1, 1_000));
        // Never zero and monotonically non-increasing over a wide sweep.
        let mut previous = i64::MAX;
        for step in 1..20_000i64 {
            let v = step * ONE / 100;
            let y = int_rsqrt(v);
            assert!(y > 0, "rsqrt must never collapse to zero (v = {v})");
            assert!(y <= previous, "rsqrt must not increase as v increases (step {step})");
            previous = y;
        }
    }

    /// ADR-0040 C1/C2: the two lossy/fixed-point primitives, at their defining cases.
    #[test]
    fn shift_and_multiply_round_and_saturate_as_specified() {
        // Round half AWAY from zero, symmetric about zero.
        assert_eq!(rounding_shift_right(3, 1), 2);
        assert_eq!(rounding_shift_right(-3, 1), -2);
        assert_eq!(rounding_shift_right(1, 1), 1);
        assert_eq!(rounding_shift_right(-1, 1), -1);
        assert_eq!(rounding_shift_right(7, 0), 7);
        // SRDHM is `(a·b) >> 31`. These two identities pin the factor: an extra 2 makes the
        // first return 0.5 and the second overflow i32 (the bug this test caught).
        assert_eq!(srdhm(1 << 30, 1 << 30), 1 << 29, "0.5 x 0.5 = 0.25 in Q31");
        let one_q31 = i32::MAX;
        assert!(srdhm(one_q31, one_q31) >= one_q31 - 2, "~1.0 x ~1.0 stays in range");
        assert_eq!(srdhm(i32::MIN, i32::MIN), i32::MAX);
        assert_eq!(srdhm(0, 12345), 0);
        // Requantize saturates rather than wrapping.
        assert_eq!(requantize(i32::MAX, i32::MAX, 0), 127);
        assert_eq!(requantize(i32::MIN, i32::MAX, 0), -128);
    }

    /// **ADR-0040 Decision E, pinned: reduction order cannot change a result.**
    ///
    /// The property the whole class is built for. Summed forward, backward, and in interleaved
    /// halves — the three shapes a threaded or SIMD kernel actually produces — every order gives
    /// the identical accumulator, because integer addition is exactly associative within the
    /// no-overflow bound.
    #[test]
    fn reduction_order_cannot_change_an_accumulator() {
        let a: Vec<i32> = (0..4_096).map(|i| ((i * 37) % 255) as i32 - 127).collect();
        let b: Vec<i32> = (0..4_096).map(|i| ((i * 101) % 255) as i32 - 127).collect();

        let forward: i32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        let backward: i32 = a.iter().zip(&b).rev().map(|(x, y)| x * y).sum();
        let products: Vec<i32> = a.iter().zip(&b).map(|(x, y)| x * y).collect();
        let even: i32 = products.iter().step_by(2).sum();
        let odd: i32 = products.iter().skip(1).step_by(2).sum();
        let interleaved: i32 = even + odd;

        assert_eq!(forward, backward, "reversal must not change the sum");
        assert_eq!(forward, interleaved, "splitting into lanes must not change the sum");
    }

    /// The bound that licenses the property above: `MAX_DOT_LEN` worst-case products still fit an
    /// `i32`, and one more does not. Decision E is conditional on this, so it is pinned.
    #[test]
    fn the_no_overflow_bound_is_exactly_at_the_edge() {
        let worst = 127i64 * 127;
        assert!(MAX_DOT_LEN as i64 * worst <= i32::MAX as i64, "the bound must fit");
        assert!((MAX_DOT_LEN as i64 + 1) * worst > i32::MAX as i64, "and must be the largest that does");
    }
}
