//! Scalar integer functions at native width, each equal to its `misaka_palw_tir::arith` original
//! on every input the primitive's type rule admits (the tests below compare them exhaustively on
//! the edges and on dense samples, and the differential runs them against the reference).
//!
//! The reference computes everything in `i128`; its intermediates provably fit `i64` for every
//! `i64` input of `IntExp`/`IntRsqrt`/`IntLn` (bounds stated per function), so these run in one
//! machine word. Division by a power of two — every `>> s` of spec 04b §6.4 — is a shift here,
//! written from the rule's definition on the quotient and the remainder bit, never from a nudged
//! `(x ± 2^(s−1)) >> s`.

use misaka_palw_tir::Rounding;

const K: u32 = 24;
const ONE: i64 = 1 << K;
const LN2_Q: i64 = 11_629_080;
const POLY2_A: i64 = 6_014_632;
const POLY2_B: i64 = 22_699_573;
const POLY2_C: i64 = 5_771_362;
const EXP_ZERO_AT: i64 = -31 * LN2_Q;
const RSQRT_SEED: [i64; 16] = [
    15_395_829, 14_307_657, 13_421_772, 12_682_383, 12_053_107, 11_509_075, 11_032_629, 10_610_843, 10_234_005, 9_894_662, 9_586_980,
    9_306_325, 9_048_957, 8_811_825, 8_592_409, 8_388_608,
];

#[inline]
pub fn log2_floor_i64(x: i64) -> i64 {
    if x <= 0 { -1 } else { 63 - x.leading_zeros() as i64 }
}

/// `IntExp` (spec 04b §6.5). `z ≤ 30`, `t ∈ (B − LN2_Q, B]` so `t² < 2^49`, `A·⌊t²/2^24⌋ < 2^48`.
#[inline]
pub fn int_exp_i64(x: i64) -> i64 {
    let x = x.min(0);
    if x <= EXP_ZERO_AT {
        return 0;
    }
    let z = (-x) / LN2_Q;
    let p = x + z * LN2_Q;
    let t = p + POLY2_B;
    let square = (t * t) >> K;
    let poly = ((POLY2_A * square) >> K) + POLY2_C;
    // Half away from zero of a positive value by 2^z.
    if z == 0 { poly } else { (poly >> z) + ((poly >> (z - 1)) & 1) }
}

/// `IntRsqrt` (spec 04b §6.5). `m < 2^26`; the Newton value stays below `2^26` (it grows at most
/// ×1.5 per step from a seed below `2^24`, or resets to 1), so every product is below `2^56`.
#[inline]
pub fn int_rsqrt_i64(v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let bit = log2_floor_i64(v);
    let mut e = (bit - K as i64).div_euclid(2);
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
    for _ in 0..3 {
        let y2 = (y * y) >> K;
        let my2 = (m * y2) >> K;
        y = (y * (3 * ONE - my2)) >> (K + 1);
        if y <= 0 {
            y = 1;
        }
    }
    if e >= 0 { y >> e } else { y << (-e) }
}

/// `IntLn` (spec 04b §6.5). `m < 2^25`, `t < 2^24/3`: every product is below `2^48`.
#[inline]
pub fn int_ln_i64(x: i64) -> i64 {
    if x <= 0 {
        return 0;
    }
    let s = log2_floor_i64(x) - K as i64;
    let m = if s >= 0 { x >> s } else { x << (-s) };
    let t = ((m - ONE) << K) / (m + ONE);
    let t2 = (t * t) >> K;
    let mut term = t;
    let mut sum = t;
    for odd in [3i64, 5, 7, 9, 11] {
        term = (term * t2) >> K;
        sum += term / odd;
    }
    2 * sum + s * LN2_Q
}

/// `round_rule(x / 2^s)` for `0 ≤ s ≤ 62`.
#[inline(always)]
pub fn shr_round_i64(x: i64, s: u32, rule: Rounding) -> i64 {
    if s == 0 {
        return x;
    }
    match rule {
        Rounding::Floor => x >> s,
        // ⌊x/2^s⌋ plus the remainder's top bit (2r ≥ d ⟺ bit s−1 of r, which is bit s−1 of x).
        Rounding::HalfUp => (x >> s) + ((x >> (s - 1)) & 1),
        Rounding::HalfAwayFromZero => {
            let m = x.unsigned_abs();
            let q = (m >> s) + ((m >> (s - 1)) & 1);
            if x < 0 { (q as i64).wrapping_neg() } else { q as i64 }
        }
    }
}

/// `round_rule(x / d)` for `d ≥ 1` (the caller has checked the divisor).
#[inline(always)]
pub fn div_round_i64(x: i64, d: i64, rule: Rounding) -> i64 {
    if d & (d - 1) == 0 {
        return shr_round_i64(x, d.trailing_zeros(), rule);
    }
    match rule {
        Rounding::Floor => x.div_euclid(d),
        Rounding::HalfUp => {
            let q = x.div_euclid(d);
            let r = x.rem_euclid(d);
            if r >= d - r { q + 1 } else { q }
        }
        Rounding::HalfAwayFromZero => {
            let m = x.unsigned_abs();
            let du = d as u64;
            let mut q = m / du;
            let r = m % du;
            if r >= du - r {
                q += 1;
            }
            // q = 2^63 only for x = i64::MIN, d = 1 (handled as a power of two above).
            if x < 0 { (q as i64).wrapping_neg() } else { q as i64 }
        }
    }
}

/// `round_rule(x / 2^s)` for `0 ≤ s ≤ 126`.
#[inline(always)]
pub fn shr_round_i128(x: i128, s: u32, rule: Rounding) -> i128 {
    if s == 0 {
        return x;
    }
    match rule {
        Rounding::Floor => x >> s,
        Rounding::HalfUp => (x >> s) + ((x >> (s - 1)) & 1),
        Rounding::HalfAwayFromZero => {
            let m = x.unsigned_abs();
            let q = (m >> s) + ((m >> (s - 1)) & 1);
            if x < 0 { (q as i128).wrapping_neg() } else { q as i128 }
        }
    }
}

/// `round_rule(x / d)` for `d ≥ 1`.
#[inline(always)]
pub fn div_round_i128(x: i128, d: i128, rule: Rounding) -> i128 {
    if d & (d - 1) == 0 {
        return shr_round_i128(x, d.trailing_zeros(), rule);
    }
    // Both fit a machine word: the hardware divider.
    if let (Ok(x64), Ok(d64)) = (i64::try_from(x), i64::try_from(d)) {
        return div_round_i64(x64, d64, rule) as i128;
    }
    misaka_palw_tir::arith::div_round(x, d, rule).expect("d ≥ 1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::arith;

    fn samples() -> Vec<i64> {
        let mut v: Vec<i64> = vec![i64::MIN, i64::MIN + 1, -1, 0, 1, 2, 3, i64::MAX, i64::MAX - 1];
        for b in 0..63 {
            for d in [-2i64, -1, 0, 1, 2] {
                let x = (1i64 << b).wrapping_add(d);
                v.push(x);
                v.push(x.wrapping_neg());
            }
        }
        for z in 0..33i64 {
            for d in -3..=3 {
                v.push(-z * LN2_Q + d);
            }
        }
        let mut s = 0x9E37_79B9_7F4A_7C15u64;
        for _ in 0..200_000 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            let shift = (s >> 58) as u32;
            v.push((s as i64) >> shift);
        }
        v
    }

    #[test]
    fn the_transcendentals_equal_the_reference_on_every_sample() {
        for x in samples() {
            let r = x as i128;
            assert_eq!(int_exp_i64(x) as i128, arith::int_exp(r), "IntExp({x})");
            assert_eq!(int_rsqrt_i64(x) as i128, arith::int_rsqrt(r), "IntRsqrt({x})");
            assert_eq!(int_ln_i64(x) as i128, arith::int_ln(r), "IntLn({x})");
            assert_eq!(log2_floor_i64(x) as i128, arith::log2_floor(r), "Log2Floor({x})");
        }
    }

    #[test]
    fn int_exp_equals_the_reference_on_every_i32() {
        // IntExp's whole non-trivial domain is (−31·LN2_Q, 0], inside i32.
        let mut x = EXP_ZERO_AT - 2;
        while x <= 2 {
            assert_eq!(int_exp_i64(x) as i128, arith::int_exp(x as i128), "IntExp({x})");
            x += 1;
        }
    }

    #[test]
    fn division_equals_the_reference_on_every_sample() {
        let xs = samples();
        let mut ds: Vec<i64> = vec![1, 2, 3, 4, 5, 7, 8, 1 << 31, (1 << 31) + 1, 1 << 62, i64::MAX, (1 << 62) + 1];
        ds.extend((0..63).map(|b| 1i64 << b));
        for rule in Rounding::ALL {
            for (i, x) in xs.iter().enumerate().step_by(7) {
                for d in &ds {
                    let want = arith::div_round(*x as i128, *d as i128, rule).unwrap();
                    assert_eq!(div_round_i64(*x, *d, rule) as i128, want, "{x} / {d} {rule:?}");
                    let wide = (*x as i128) << ((i % 60) as u32);
                    let want = arith::div_round(wide, *d as i128, rule).unwrap();
                    assert_eq!(div_round_i128(wide, *d as i128, rule), want, "{wide} / {d} {rule:?}");
                }
            }
            for x in [i128::MIN, i128::MIN + 1, i128::MAX, -1, 0, 1] {
                for d in [1i128, 2, 3, 1 << 126, i128::MAX, (1 << 64) + 1] {
                    assert_eq!(div_round_i128(x, d, rule), arith::div_round(x, d, rule).unwrap(), "{x} / {d} {rule:?}");
                }
            }
        }
    }
}
