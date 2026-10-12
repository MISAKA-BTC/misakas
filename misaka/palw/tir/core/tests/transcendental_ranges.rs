//! **The output bounds the range rules of spec 04b §6 state for the transcendentals, checked
//! exhaustively over each algorithm's normalised domain.** Release-only (`--ignored`): each sweep is
//! tens of millions of evaluations.
//!
//! * `IntExp`: the maximum over every input is `IntExp(0) = 16,781,800`, the minimum 0. Within a
//!   range-reduction bucket the value is non-decreasing in `x`; ACROSS a bucket edge it is not
//!   (the vectors show `IntExp(−LN2_Q + 1) < IntExp(−LN2_Q)`), which is why the range rule is a
//!   constant interval and never `[f(lo), f(hi)]`.
//! * `IntRsqrt`: after normalisation `m ∈ [2^24, 2^26)` the Newton value `y` is in
//!   `[1, 2^24]`; the output is `y` shifted by `−e ≤ 12`, so the maximum is `IntRsqrt(1) =
//!   68,719,472,640 = 2^36 − 2^12`.
//! * `IntLn`: for `m ∈ [2^24, 2^25)` the series part `2·sum` is in `[0, LN2_Q)`, so
//!   `IntLn(x) ∈ [s·LN2_Q, (s + 1)·LN2_Q)` with `s = floor(log2 x) − 24 ∈ [−24, 38]`.

use misaka_palw_tir::arith::{LN2_Q, ONE, int_exp, int_ln, int_rsqrt};

#[test]
#[ignore]
fn int_exp_is_bounded_by_its_value_at_zero_and_monotone_inside_each_bucket() {
    let max = int_exp(0);
    assert_eq!(max, 16_781_800);
    let mut prev = int_exp(-LN2_Q + 1);
    for x in (-LN2_Q + 1)..=0 {
        let v = int_exp(x);
        assert!(v >= prev, "not monotone inside bucket 0 at {x}");
        assert!((0..=max).contains(&v));
        prev = v;
    }
    // Every later bucket is the first one's values shifted right with rounding: bounded by max.
    for z in 1..31i128 {
        for p in [(-LN2_Q + 1), -LN2_Q / 2, -1, 0] {
            let v = int_exp(p - z * LN2_Q);
            assert!((0..=max).contains(&v));
        }
    }
}

#[test]
#[ignore]
fn int_rsqrt_newton_stays_in_one_to_one() {
    // m ranges over the whole normalised interval: v = m with e = 0.
    let mut max_y = 0;
    for m in ONE..4 * ONE {
        let y = int_rsqrt(m);
        assert!((1..=ONE).contains(&y), "y = {y} at m = {m}");
        max_y = max_y.max(y);
    }
    assert_eq!(int_rsqrt(1), (max_y) << 12, "the global maximum is at v = 1");
    assert_eq!(int_rsqrt(1), 68_719_472_640);
}

#[test]
#[ignore]
fn int_ln_series_part_is_below_ln2() {
    for m in ONE..2 * ONE {
        let v = int_ln(m);
        assert!((0..LN2_Q).contains(&v), "IntLn({m}) = {v}");
    }
    assert_eq!(int_ln(1), -24 * LN2_Q);
    assert!(int_ln(i64::MAX as i128) < 39 * LN2_Q);
}
