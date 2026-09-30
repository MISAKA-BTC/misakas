//! Scalar integer arithmetic: the division rules and the three transcendentals (spec 04b §5–6).
//!
//! Every function here is total on the domain its primitive's type rule admits and is written the
//! plain way: one branch per case, no reassociation, no cleverness. The transcendental algorithms
//! are ADR-0040 F1/F2 and ADR-0052 D exactly as `kaspa-consensus-core` implements them
//! (`palw_base0::int_exp`, `palw_base0::int_rsqrt`, `palw_qwen36_ops::q36_int_ln`), including the
//! pinned constants; `tests/kat_base0.rs` reproduces the frozen BASE-0 KAT digest through them.
//!
//! No float, no libm, anywhere (PALW-TIR-2).

use crate::prim::Rounding;

/// Fraction bits of the Q format every transcendental reads and writes.
pub const K: u32 = 24;
/// `1.0` in Q24.
pub const ONE: i128 = 1 << K;
/// `round(ln 2 · 2^24)`.
pub const LN2_Q: i128 = 11_629_080;
/// `IntExp`'s shifted-square coefficients: `Poly2(p) = A·(p + B)² + C` (NOT Horner).
pub const POLY2_A: i128 = 6_014_632;
pub const POLY2_B: i128 = 22_699_573;
pub const POLY2_C: i128 = 5_771_362;
/// `IntExp`'s largest range-reduction shift; at and beyond it the result is 0.
pub const Z_MAX: i128 = 31;
/// Newton iterations of `IntRsqrt`, fixed.
pub const RSQRT_ITERS: u32 = 3;
/// `IntRsqrt`'s seed table: `2^24/√(1 + 3(i+1)/16)`, the reciprocal square root of each bucket's
/// UPPER end (a seed above the basin diverges to zero).
pub const RSQRT_SEED: [i128; 16] = [
    15_395_829, 14_307_657, 13_421_772, 12_682_383, 12_053_107, 11_509_075, 11_032_629, 10_610_843, 10_234_005, 9_894_662, 9_586_980,
    9_306_325, 9_048_957, 8_811_825, 8_592_409, 8_388_608,
];

/// `IntExp(x) = 0` for every `x ≤ EXP_ZERO_AT` (`−31·LN2_Q`).
pub const EXP_ZERO_AT: i128 = -Z_MAX * LN2_Q;

/// `round_rule(x / d)` for `d ≥ 1`; `None` for `d < 1`.
///
/// Written on the quotient and remainder, never on a nudged shift: the nudged-shift forms
/// `(x ± 2^(s−1)) >> s` are the two defects 04a C1 records. `HalfUp` and `HalfAwayFromZero` round
/// up exactly when `2r ≥ d`, tested as `r ≥ d − r` so nothing overflows.
pub fn div_round(x: i128, d: i128, rule: Rounding) -> Option<i128> {
    if d < 1 {
        return None;
    }
    match rule {
        Rounding::Floor => Some(x.div_euclid(d)),
        Rounding::HalfUp => {
            let q = x.div_euclid(d);
            let r = x.rem_euclid(d);
            if r >= d - r { q.checked_add(1) } else { Some(q) }
        }
        Rounding::HalfAwayFromZero => {
            let magnitude = x.unsigned_abs();
            let du = d as u128;
            let mut q = magnitude / du;
            let r = magnitude % du;
            if r >= du - r {
                q += 1;
            }
            if x < 0 {
                // `q ≤ 2^127` because `|x| ≤ 2^127`; `−2^127` is `i128::MIN`.
                if q == 1u128 << 127 { Some(i128::MIN) } else { Some(-(q as i128)) }
            } else {
                i128::try_from(q).ok()
            }
        }
    }
}

/// `floor(log2 x)` for `x ≥ 1`, `−1` for `x ≤ 0`.
pub fn log2_floor(x: i128) -> i128 {
    if x <= 0 { -1 } else { 127 - x.leading_zeros() as i128 }
}

/// `Poly2(p) = ((A · ((p + B)² >> 24)) >> 24) + C`, both shifts floor, for `p ∈ (−LN2_Q, 0]`.
fn poly2(p: i128) -> i128 {
    let t = p + POLY2_B;
    let square = (t * t).div_euclid(ONE);
    (POLY2_A * square).div_euclid(ONE) + POLY2_C
}

/// `IntExp` (04a F1) on the mathematical integer `x`, Q24 in and out.
///
/// ```text
/// x' = min(x, 0)
/// if x' ≤ −31·LN2_Q: 0
/// z  = floor(−x' / LN2_Q)            ∈ [0, 30]
/// p  = x' + z·LN2_Q                  ∈ (−LN2_Q, 0]
/// IntExp(x) = RoundingShiftRight(Poly2(p), z)    (half away from zero; the value is positive)
/// ```
///
/// Identical to `palw_base0::int_exp` on every `i32`; on wider inputs it is the same formula (the
/// legacy `i32` argument clamps below `−31·LN2_Q` to the same 0).
pub fn int_exp(x: i128) -> i128 {
    let x = x.min(0);
    if x <= EXP_ZERO_AT {
        return 0;
    }
    let z = (-x) / LN2_Q; // non-negative operands: truncation is floor
    let p = x + z * LN2_Q;
    let poly = poly2(p);
    div_round(poly, 1i128 << z, Rounding::HalfAwayFromZero).expect("2^z ≥ 1")
}

/// `IntRsqrt` (04a F2) on `v`, Q24 in and out; `v ≤ 0` gives 0. The primitive's type rule keeps
/// `v` inside `i64`.
///
/// ```text
/// b = floor(log2 v);  e = floor((b − 24) / 2)
/// m = v >> 2e  (e ≥ 0)  |  v << −2e  (e < 0)        m ∈ [2^24, 2^26)
/// i = clamp(floor((m − 2^24)·16 / (3·2^24)), 0, 15);  y = SEED[i]
/// 3 times: y2 = (y·y) >> 24;  my2 = (m·y2) >> 24;  y = (y·(3·2^24 − my2)) >> 25;  if y ≤ 0: y = 1
/// IntRsqrt(v) = y >> e  (e ≥ 0)  |  y << −e  (e < 0)
/// ```
/// Every `>>` floors. The two normalisation loops of the legacy text never execute (the first
/// shift already lands `m` in `[2^24, 2^26)`); they are kept below so the transcription is literal.
pub fn int_rsqrt(v: i128) -> i128 {
    if v <= 0 {
        return 0;
    }
    let bit = log2_floor(v);
    let mut e = (bit - K as i128).div_euclid(2);
    let mut m = if 2 * e >= 0 { v >> (2 * e) } else { v << (-2 * e) };
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

/// `IntLn` (ADR-0052 D, `palw_qwen36_ops::q36_int_ln`) on `x`, Q24 in and out; `x ≤ 0` gives 0
/// (the legacy function refuses it; every legacy caller either proves `x > 0` or maps the refusal
/// to 0, `q36_softplus`).
///
/// ```text
/// s = floor(log2 x) − 24;   m = x >> s (s ≥ 0) | x << −s (s < 0)      m ∈ [2^24, 2^25)
/// t  = floor(((m − 2^24)·2^24) / (m + 2^24))                        t ∈ [0, 2^24/3)
/// t2 = (t·t) >> 24
/// term = t; sum = t
/// for d in 3, 5, 7, 9, 11:  term = (term·t2) >> 24;  sum += floor(term / d)
/// IntLn(x) = 2·sum + s·LN2_Q
/// ```
pub fn int_ln(x: i128) -> i128 {
    if x <= 0 {
        return 0;
    }
    let s = log2_floor(x) - K as i128;
    let m = if s >= 0 { x >> s } else { x << (-s) };
    let t = ((m - ONE) << K) / (m + ONE);
    let t2 = (t * t) >> K;
    let mut term = t;
    let mut sum = t;
    for odd in [3i128, 5, 7, 9, 11] {
        term = (term * t2) >> K;
        sum += term / odd;
    }
    2 * sum + s * LN2_Q
}

/// The value `IntExp` takes at 0 and its maximum over every input (spec 04b §6.1).
pub fn int_exp_max() -> i128 {
    int_exp(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn division_rules_at_every_edge() {
        use Rounding::*;
        // Exact halves, both signs, three rules.
        assert_eq!(div_round(3, 2, Floor), Some(1));
        assert_eq!(div_round(-3, 2, Floor), Some(-2));
        assert_eq!(div_round(3, 2, HalfUp), Some(2));
        assert_eq!(div_round(-3, 2, HalfUp), Some(-1));
        assert_eq!(div_round(3, 2, HalfAwayFromZero), Some(2));
        assert_eq!(div_round(-3, 2, HalfAwayFromZero), Some(-2));
        // 04a C1's regression: a negative exact quotient is not rounded at all.
        assert_eq!(div_round(-64, 2, HalfAwayFromZero), Some(-32));
        assert_eq!(div_round(-63, 2, HalfAwayFromZero), Some(-32));
        // SRDHM's negative half: -1/2 rounds UP to 0.
        assert_eq!(div_round(-(1 << 30), 1 << 31, HalfUp), Some(0));
        // The ends of i128.
        assert_eq!(div_round(i128::MIN, 1, HalfAwayFromZero), Some(i128::MIN));
        assert_eq!(div_round(i128::MIN, 2, HalfAwayFromZero), Some(i128::MIN / 2));
        assert_eq!(div_round(i128::MAX, 1, HalfUp), Some(i128::MAX));
        assert_eq!(div_round(i128::MIN, i128::MAX, Floor), Some(-2));
        assert_eq!(div_round(5, 0, Floor), None);
        assert_eq!(div_round(5, -1, Floor), None);
    }

    #[test]
    fn transcendental_anchor_values() {
        // 04a/ADR-0052 anchors, as the legacy module states them.
        assert_eq!(int_exp(0), 16_781_800, "Poly2(0): 3e-4 above ONE, the value the legacy tests quote");
        assert_eq!(int_exp(1_000_000), int_exp(0), "positive inputs clamp");
        assert_eq!(int_exp(i32::MIN as i128), 0);
        assert_eq!(int_exp(EXP_ZERO_AT), 0);
        assert!(int_exp(EXP_ZERO_AT + 1) >= 0);
        assert_eq!(int_rsqrt(0), 0);
        assert_eq!(int_rsqrt(-5), 0);
        assert!(int_rsqrt(3 * ONE) > 0, "the seed-basin regression");
        assert_eq!(int_ln(ONE), 0, "ln 1 = 0 exactly");
        assert_eq!(int_ln(0), 0);
        assert_eq!(log2_floor(1), 0);
        assert_eq!(log2_floor(i128::MAX), 126);
        assert_eq!(log2_floor(0), -1);
        assert_eq!(log2_floor(i128::MIN), -1);
    }
}
