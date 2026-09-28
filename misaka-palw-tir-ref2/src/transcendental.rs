//! `Log2Floor` and the three Q24 transcendentals (04b §6.4, §6.5), re-derived from the text.
//!
//! Structural choices: every `>> k` of the text is written as floor division by `2^k` through
//! [`floor_div`], which is defined from truncating division plus an explicit correction (never a
//! shift operator); every constant is re-declared here from the text, not imported; `Log2Floor` is
//! the largest `k` with `2^k ≤ x`, searched upward.

use crate::types::pow2;

pub const ONE: i128 = 16_777_216;
pub const LN2_Q: i128 = 11_629_080;
pub const POLY2_A: i128 = 6_014_632;
pub const POLY2_B: i128 = 22_699_573;
pub const POLY2_C: i128 = 5_771_362;
pub const RSQRT_SEED: [i128; 16] = [
    15_395_829, 14_307_657, 13_421_772, 12_682_383, 12_053_107, 11_509_075, 11_032_629, 10_610_843, 10_234_005, 9_894_662,
    9_586_980, 9_306_325, 9_048_957, 8_811_825, 8_592_409, 8_388_608,
];

/// `(⌊a / b⌋, a mod b)` for `b ≥ 1`: the `q` with `b·q ≤ a < b·(q + 1)` and `r = a − b·q ∈ [0, b)`.
/// Built from truncating division, whose remainder `a − q_t·b` never overflows (`|q_t·b| ≤ |a|`),
/// then corrected by one step when that remainder is negative.
pub fn floor_divmod(a: i128, b: i128) -> (i128, i128) {
    debug_assert!(b >= 1);
    let q = a / b; // truncates toward zero
    let r = a - q * b; // in (−b, b), same sign as a
    if r < 0 { (q - 1, r + b) } else { (q, r) }
}

/// `⌊a / b⌋` for `b ≥ 1`.
pub fn floor_div(a: i128, b: i128) -> i128 {
    floor_divmod(a, b).0
}

/// `a mod b ∈ [0, b)` for `b ≥ 1`.
pub fn floor_mod(a: i128, b: i128) -> i128 {
    floor_divmod(a, b).1
}

/// §6.4 `Log2Floor`: `⌊log2 x⌋` for `x ≥ 1`, `−1` for `x ≤ 0`.
pub fn log2_floor(x: i128) -> i128 {
    if x <= 0 {
        return -1;
    }
    let mut k: u32 = 0;
    // x ≤ 2^127 − 1, so the answer is at most 126 and 2^(k+1) is always representable here.
    while k < 126 && pow2(k + 1) <= x {
        k += 1;
    }
    k as i128
}

/// `Div_HalfAwayFromZero(x, d)` for `x` of these algorithms (small, `d = 2^z ≥ 1`).
fn half_away(x: i128, d: i128) -> i128 {
    let m = x.abs();
    let q = floor_div(m, d) + if 2 * floor_mod(m, d) >= d { 1 } else { 0 };
    if x < 0 { -q } else { q }
}

/// §6.5 `IntExp` for an input of at most 64 bits.
pub fn int_exp(x: i128) -> i128 {
    let xp = x.min(0);
    if xp <= -31 * LN2_Q {
        return 0;
    }
    let z = floor_div(-xp, LN2_Q); // 0 ≤ z ≤ 30
    let p = xp + z * LN2_Q; // −LN2_Q < p ≤ 0
    let t = p + POLY2_B;
    let poly = floor_div(POLY2_A * floor_div(t * t, ONE), ONE) + POLY2_C;
    half_away(poly, pow2(z as u32))
}

/// §6.5 `IntRsqrt` for an input of at most 64 bits.
pub fn int_rsqrt(v: i128) -> i128 {
    if v <= 0 {
        return 0;
    }
    let e = floor_div(log2_floor(v) - 24, 2);
    let m = if e >= 0 { floor_div(v, pow2(2 * e as u32)) } else { v * pow2((-2 * e) as u32) };
    let i = floor_div((m - ONE) * 16, 3 * ONE).clamp(0, 15);
    let mut y = RSQRT_SEED[i as usize];
    for _ in 0..3 {
        let y2 = floor_div(y * y, ONE);
        let my2 = floor_div(m * y2, ONE);
        y = floor_div(y * (3 * ONE - my2), 2 * ONE);
        if y <= 0 {
            y = 1;
        }
    }
    if e >= 0 { floor_div(y, pow2(e as u32)) } else { y * pow2((-e) as u32) }
}

/// §6.5 `IntLn` for an input of at most 64 bits.
pub fn int_ln(x: i128) -> i128 {
    if x <= 0 {
        return 0;
    }
    let s = log2_floor(x) - 24;
    let m = if s >= 0 { floor_div(x, pow2(s as u32)) } else { x * pow2((-s) as u32) };
    let t = floor_div((m - ONE) * ONE, m + ONE);
    let t2 = floor_div(t * t, ONE);
    let mut term = t;
    let mut sum = t;
    for d in [3, 5, 7, 9, 11] {
        term = floor_div(term * t2, ONE);
        sum += floor_div(term, d);
    }
    2 * sum + s * LN2_Q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stated_values() {
        // §6.5: IntExp(0) = 16,781,800 is the maximum; not monotone across the first bucket edge.
        assert_eq!(int_exp(0), 16_781_800);
        assert!(int_exp(-LN2_Q + 1) < int_exp(-LN2_Q));
        assert_eq!(int_exp(-31 * LN2_Q), 0);
        assert_eq!(int_exp(i64::MIN as i128), 0);
        assert_eq!(int_exp(i64::MAX as i128), 16_781_800);
        // §6.5: IntRsqrt(1) = 2^36 − 2^12 is the maximum.
        assert_eq!(int_rsqrt(1), (1i128 << 36) - (1 << 12));
        assert_eq!(int_rsqrt(0), 0);
        assert_eq!(int_rsqrt(-5), 0);
        // IntLn ranges: [s·LN2_Q, (s+1)·LN2_Q) for x ≥ 1, over i64 in [−24·LN2_Q, 39·LN2_Q).
        assert_eq!(int_ln(ONE), 0);
        for x in [1i128, 2, 3, 1000, ONE - 1, ONE + 1, 1 << 40, i64::MAX as i128] {
            let s = log2_floor(x) - 24;
            let v = int_ln(x);
            assert!(v >= s * LN2_Q && v < (s + 1) * LN2_Q, "x={x} v={v}");
        }
        assert!(int_ln(i64::MAX as i128) < 39 * LN2_Q);
        assert_eq!(int_ln(1), -24 * LN2_Q + int_ln(ONE)); // m = ONE → series 0
    }

    #[test]
    fn log2_by_definition() {
        assert_eq!(log2_floor(0), -1);
        assert_eq!(log2_floor(i128::MIN), -1);
        assert_eq!(log2_floor(1), 0);
        assert_eq!(log2_floor(2), 1);
        assert_eq!(log2_floor(3), 1);
        assert_eq!(log2_floor(i128::MAX), 126);
        for k in 0..126u32 {
            assert_eq!(log2_floor(pow2(k)), k as i128);
            assert_eq!(log2_floor(pow2(k + 1) - 1), k as i128);
        }
    }

    #[test]
    fn rsqrt_output_bound_over_every_exponent() {
        for k in 0..63u32 {
            for v in [pow2(k), pow2(k) + 1, pow2(k + 1) - 1, 3 * pow2(k) / 2 + 1] {
                if v <= i64::MAX as i128 {
                    let y = int_rsqrt(v);
                    assert!((0..=68_719_472_640).contains(&y), "v={v} y={y}");
                }
            }
        }
    }
}
