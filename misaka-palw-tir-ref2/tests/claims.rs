//! The numeric statements 04b §6.5 makes about its own algorithms (and §7 relies on for soundness),
//! checked by exhaustive sweeps with this crate's transcendentals:
//!   - `IntExp(0) = 16,781,800` is the maximum over all inputs; `IntExp` is non-decreasing inside a
//!     range-reduction bucket and not across the first bucket edge;
//!   - `IntRsqrt`'s output lies in `[0, 68,719,472,640]`, the maximum is `IntRsqrt(1)`, and (§7) the
//!     Newton value `y` before the final scaling is in `[1, ONE]` for every input;
//!   - `IntLn(x) ∈ [s·LN2_Q, (s+1)·LN2_Q)` for `x ≥ 1`, and over `i64` inputs in
//!     `[−24·LN2_Q, 39·LN2_Q)`.
//! The full sweeps take a few seconds in release (`cargo test --release -p misaka-palw-tir-ref2
//! --test claims -- --ignored`); the default run samples.

use misaka_palw_tir_ref2::transcendental::{LN2_Q, ONE, RSQRT_SEED, floor_div, int_exp, int_ln, int_rsqrt, log2_floor};

fn exp_sweep(step: usize) {
    let lo = -31 * LN2_Q - 1000;
    let mut max = i128::MIN;
    let mut prev: Option<(i128, i128)> = None; // (z, value)
    let mut x = lo;
    while x <= 1000 {
        let v = int_exp(x);
        max = max.max(v);
        let xp = x.min(0);
        if xp > -31 * LN2_Q {
            let z = floor_div(-xp, LN2_Q);
            if let Some((pz, pv)) = prev {
                if pz == z && x <= 0 {
                    assert!(v >= pv, "IntExp decreases inside bucket z = {z} at x = {x}");
                }
            }
            prev = Some((z, v));
        } else {
            assert_eq!(v, 0);
        }
        x += step as i128;
    }
    assert_eq!(max, 16_781_800);
    assert_eq!(int_exp(0), 16_781_800);
    assert!(int_exp(-LN2_Q + 1) < int_exp(-LN2_Q));
}

/// The Newton value `y` of IntRsqrt for a normalised mantissa `m ∈ [2^24, 2^26)`.
fn newton_y(m: i128) -> i128 {
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
    y
}

fn rsqrt_sweep(step: usize) {
    let mut ymin = i128::MAX;
    let mut ymax = i128::MIN;
    let mut m = 1i128 << 24;
    while m < 1 << 26 {
        let y = newton_y(m);
        ymin = ymin.min(y);
        ymax = ymax.max(y);
        m += step as i128;
    }
    println!("IntRsqrt Newton y over the mantissas: [{ymin}, {ymax}] (ONE = {ONE})");
    assert!(ymin >= 1 && ymax <= ONE);
    // Outputs over every exponent, at the bucket starts and a sample inside.
    let mut best = 0i128;
    for k in 0..63u32 {
        let base = 1i128 << k;
        for off in [0i128, 1, base / 3, base / 2, base - 1] {
            let v = base + off;
            if v <= i64::MAX as i128 {
                let r = int_rsqrt(v);
                assert!((0..=68_719_472_640).contains(&r), "v = {v}");
                best = best.max(r);
            }
        }
    }
    assert_eq!(best, int_rsqrt(1));
    assert_eq!(int_rsqrt(1), (1i128 << 36) - (1 << 12));
}

fn ln_sweep(step: usize) {
    // IntLn(x) = 2·sum(m) + s·LN2_Q with m = the mantissa of x in [2^24, 2^25): the claim is
    // 0 ≤ 2·sum(m) < LN2_Q for every mantissa.
    let mut m = 1i128 << 24;
    let mut lo = i128::MAX;
    let mut hi = i128::MIN;
    while m < 1 << 25 {
        let series = int_ln(m); // s = 0 for these m
        lo = lo.min(series);
        hi = hi.max(series);
        m += step as i128;
    }
    println!("IntLn series part over the mantissas: [{lo}, {hi}] (LN2_Q = {LN2_Q})");
    assert!(lo >= 0 && hi < LN2_Q);
    for x in [1i128, 2, 3, (1 << 24) - 1, 1 << 24, i64::MAX as i128, (1i128 << 62) + 12345] {
        let s = log2_floor(x) - 24;
        let v = int_ln(x);
        assert!(v >= s * LN2_Q && v < (s + 1) * LN2_Q);
        assert!(v >= -24 * LN2_Q && v < 39 * LN2_Q);
    }
}

#[test]
fn claims_sampled() {
    exp_sweep(997);
    rsqrt_sweep(101);
    ln_sweep(53);
}

#[test]
#[ignore]
fn claims_exhaustive() {
    exp_sweep(1);
    rsqrt_sweep(1);
    ln_sweep(1);
}
