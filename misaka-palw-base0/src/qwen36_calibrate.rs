//! **Choosing the A16 triples for Qwen3.6, from what the model actually does.**
//!
//! A quantization scale is a statement about a range. Three things go into every triple the
//! converter writes, and the first version of the converter had only the third:
//!
//! 1. **The weight row's own scale.** `w_ci = W_ci / s_c` with `s_c = absmax(W_c)/127`, so the
//!    accumulator is `(2^e_in / s_c) · Out_c` and the narrowing has to put `s_c` back. A converter
//!    that omits it computes every output channel scaled by a different, wrong factor — not a
//!    coarse answer, a different function.
//! 2. **The norm gain γ.** `RmsNorm` in this class is `a16_rms_norm` followed by a per-channel
//!    requantization, and γ rides in that requantization's multiplier. Leaving it at unity drops
//!    the model's learned per-channel scaling entirely.
//! 3. **The site's range**, which sets the exponent the codes are held at.
//!
//! # The exponents come from a measured forward pass, layer by layer
//!
//! The reference in `qwen36_reference` runs the graph in `f32`, and it is driven LAYER-MAJOR here:
//! all prompt positions advance through layer `i` while layer `i`'s weights are in hand, then the
//! weights are dropped. That is what lets one streaming read of a 24 GiB checkpoint both quantize
//! the weights and measure the activations — the alternative is two passes over the network.
//!
//! # Offline
//!
//! Float, and never on the block-validation path. ADR-0040 pins the class's scales at
//! registration; this is what produces them.

use crate::qwen36_reference::SiteRangeV1;
use kaspa_consensus_core::palw_base0::{K, ONE};
use kaspa_consensus_core::palw_base0_a16::{A16_CODE_MAX, A16QuantParams};

/// How many bits of headroom a site keeps above the largest magnitude it was seen at.
///
/// One bit. A calibration prompt does not visit every activation the class will ever see, and a
/// site that saturates is a site whose largest values all become the same number — which is the
/// failure that made the int8 tier's attention flat. One bit costs half the resolution and buys a
/// factor of two in range, which is the right trade at sixteen bits.
pub const HEADROOM_BITS: i32 = 1;

/// The exponent that puts `absmax` at the top of the A16 code range, with headroom.
///
/// `code = value · 2^e`, so `e = floor(log2(A16_CODE_MAX / absmax)) − HEADROOM`.
pub fn site_exponent(absmax: f64) -> i32 {
    // NaN and non-positive both take this branch, spelled so that neither reaches the logarithm.
    if absmax.is_nan() || absmax <= 0.0 {
        // A site that never moved gets a scale that cannot overflow rather than one derived from
        // a zero: the alternative is an exponent of infinity.
        return 0;
    }
    (A16_CODE_MAX as f64 / absmax).log2().floor() as i32 - HEADROOM_BITS
}

/// `(multiplier, shift)` with `x ≈ multiplier / 2^shift`, the mantissa normalized into the top
/// bits of an `i64`.
///
/// The A16 wire's multiplier is `i64` and its shift is at most 62, so this keeps about 62
/// significant bits — far more than any scale needs, and the excess is what stops a chain of
/// narrowings from accumulating a bias.
pub fn mul_shift(x: f64) -> (i64, u8) {
    // NaN is not finite, so one test covers it and the infinities alike.
    if !x.is_finite() || x == 0.0 {
        return (0, 0);
    }
    let mut shift = 0i32;
    let mut v = x.abs();
    while v < (1u64 << 62) as f64 && shift < 62 {
        v *= 2.0;
        shift += 1;
    }
    while v >= (1u64 << 62) as f64 && shift > 0 {
        v /= 2.0;
        shift -= 1;
    }
    let m = v.round() as i64;
    if x < 0.0 { (-m, shift as u8) } else { (m, shift as u8) }
}

/// A triple for a gain `x`, with no additive term.
pub fn triple(x: f64) -> A16QuantParams {
    let (multiplier, shift) = mul_shift(x);
    A16QuantParams { multiplier, shift, zero: 0 }
}

/// `(multiplier, shift)` with the mantissa normalized into `bits` rather than into all 62.
///
/// **The recurrence's narrowings need this and the rest do not.** Every other site narrows through
/// `a16_scale_round`, which widens to `i128` and does not care how large the multiplier is. The
/// state's two narrowings are `i64` — a decay touches `d_v · d_k` lanes per head and `i128` there
/// is the whole token — so their multiplier is bounded by `QWEN36_STATE_MULT_MAX`, and a triple
/// built by [`mul_shift`] carries about 2^62 and is refused.
///
/// Found by running a calibrated four-layer conversion: the engine refused at the first
/// GatedDeltaNet head with the bound's own error, which is the refusal doing its job.
pub fn mul_shift_bounded(x: f64, bits: u32) -> (i64, u8) {
    if !x.is_finite() || x == 0.0 {
        return (0, 0);
    }
    let cap = (1u64 << bits) as f64;
    let mut shift = 0i32;
    let mut v = x.abs();
    while v < cap / 2.0 && shift < 62 {
        v *= 2.0;
        shift += 1;
    }
    while v >= cap && shift > 0 {
        v /= 2.0;
        shift -= 1;
    }
    let m = v.round() as i64;
    if x < 0.0 { (-m, shift as u8) } else { (m, shift as u8) }
}

/// A triple whose multiplier fits the recurrence's `i64` bound.
pub fn state_triple(x: f64) -> A16QuantParams {
    let (multiplier, shift) = mul_shift_bounded(x, 30);
    A16QuantParams { multiplier, shift, zero: 0 }
}

/// **A projection's per-channel narrowing.**
///
/// `acc_c = Σ (W_ci / s_c) · (X_i · 2^e_in) = (2^e_in / s_c) · Out_c`, and the wanted code is
/// `Out_c · 2^e_out`, so the gain is `s_c · 2^(e_out − e_in)`.
pub fn projection_params(row_scales: &[f64], e_in: i32, e_out: i32) -> Vec<A16QuantParams> {
    let shift = (e_out - e_in) as f64;
    row_scales.iter().map(|s| triple(s * 2f64.powf(shift))).collect()
}

/// **A norm site's per-channel narrowing, with γ folded in.**
///
/// `a16_rms_norm` returns `x_i · r` where `r = 1/rms` in Q[`K`], so the lane is the normalized
/// activation at Q[`K`]. The wanted code is `(x_i/rms) · γ_i · 2^e_out`, so the gain is
/// `γ_i · 2^(e_out − K)`.
pub fn norm_params(gamma: &[f32], e_out: i32) -> Vec<A16QuantParams> {
    let shift = (e_out - K as i32) as f64;
    gamma.iter().map(|g| triple(*g as f64 * 2f64.powf(shift))).collect()
}

/// **A rescale into Q[`K`]** — what `Silu` and the gates read, since a nonlinearity's input scale
/// is part of the function rather than a convention.
pub fn to_qk_params(row_scales: &[f64], e_in: i32) -> Vec<A16QuantParams> {
    let shift = (K as i32 - e_in) as f64;
    row_scales.iter().map(|s| triple(s * 2f64.powf(shift))).collect()
}

/// **A plain rescale between two code exponents**, for the residual alignments and the elementwise
/// sites where no weight scale is involved.
pub fn rescale_params(e_in: i32, e_out: i32) -> A16QuantParams {
    triple(2f64.powi(e_out - e_in))
}

/// **An elementwise product of two code rows** — `MulElem` returns `a·b`, so the gain undoes both
/// input exponents and applies the output's.
pub fn product_params(e_a: i32, e_b: i32, e_out: i32) -> A16QuantParams {
    triple(2f64.powi(e_out - e_a - e_b))
}

/// **The attention logit narrowing**, including the `1/√d_head` the reference applies.
///
/// `acc = Σ q·k` at `2^(e_q + e_k)`, the wanted logit code is `(q·k/√d) · 2^e_logit`.
pub fn attn_logit_params(e_q: i32, e_k: i32, e_logit: i32, d_head: usize) -> A16QuantParams {
    triple(2f64.powi(e_logit - e_q - e_k) / (d_head as f64).sqrt())
}

/// **How far below Q[`K`] the logit codes sit**, which is what `softmax_shifted` widens by.
///
/// The softmax's max subtraction happens before the widening, in `i64`, so this can be large
/// without clamping anything. Clamped to the op's domain.
pub fn softmax_up_bits(e_logit: i32) -> u8 {
    (K as i32 - e_logit).clamp(0, 62) as u8
}

/// **The router's narrowing and widening.** The selection is made on the committed code row, so
/// the codes have to hold the logits' spread: a router whose codes saturate ties experts the
/// reference separates, and the tie rule then decides what the model should have.
pub fn router_params(row_scales: &[f64], e_in: i32, absmax: f64) -> (Vec<A16QuantParams>, A16QuantParams, u8) {
    let e_router = site_exponent(absmax);
    (projection_params(row_scales, e_in, e_router), rescale_params(e_router, e_router), softmax_up_bits(e_router))
}

/// **The gated delta rule's three narrowings, from the state's MEASURED exponent.**
///
/// The first version derived the state's width from ADR-0052's contraction bound,
/// `‖S‖ ≤ max‖v‖ · β / (1 − decay)`. That bound is true and it is the wrong number to build a
/// scale from: a single head whose gate sits at `decay ≈ 1` sends it to a million, the state's
/// grid is then a million times finer than it needs to be, and the values that actually occur —
/// measured at 2.6 where the bound says 10^6 — saturate the `i32` rail on the first token. The
/// bound says the recurrence is stable. What the scale needs is what the state actually reaches.
///
/// * `read`: `S·k` is `(value · 2^e_state) × Q15` → the value code scale, so `2^(e_value − e_state − 15)`.
/// * `write`: `u ⊗ k` is `(code at e_value) × Q15` → the state scale, so `2^(e_state − e_value − 15)`.
/// * `out`: `S·q` likewise, into the output site's exponent.
pub fn gdn_params(e_state: i32, e_value: i32, e_out: i32) -> (A16QuantParams, A16QuantParams, A16QuantParams) {
    (
        state_triple(2f64.powi(e_value - e_state - 15)),
        state_triple(2f64.powi(e_state - e_value - 15)),
        state_triple(2f64.powi(e_out - e_state - 15)),
    )
}

/// The decay exponent `c = exp(A_log)`, carried in a triple's `zero` at Q[`K`].
pub fn decay_exponent(a_log: f32) -> A16QuantParams {
    let c = (a_log as f64).exp() * ONE as f64;
    A16QuantParams { multiplier: 1, shift: 0, zero: c.clamp(0.0, i64::MAX as f64) as i64 }
}

/// A site's exponent from its measured range, or a fallback when the site was never observed.
///
/// The fallback is deliberately conservative rather than clever: a site the calibration prompt did
/// not reach is a site nothing is known about, and a scale invented for it should not be tighter
/// than one that was measured.
pub fn exponent_of(range: Option<&SiteRangeV1>, fallback_absmax: f64) -> i32 {
    site_exponent(range.map(|r| r.absmax).filter(|a| *a > 0.0).unwrap_or(fallback_absmax))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mul_shift` must reproduce the gain it was given, across the range a scale takes.
    #[test]
    fn a_gain_round_trips_through_the_wire_form() {
        for x in [1.0f64, 0.5, 2.0, 1e-6, 1e6, 127.0 / 32767.0, 3.25e-4, -2.5] {
            let (m, s) = mul_shift(x);
            let back = m as f64 / 2f64.powi(s as i32);
            let relative = (back - x).abs() / x.abs();
            assert!(relative < 1e-12, "{x} came back as {back} (relative {relative})");
        }
        assert_eq!(mul_shift(0.0), (0, 0));
        assert_eq!(mul_shift(f64::NAN), (0, 0));
    }

    /// The exponent puts the largest observed value near the top of the code range, one bit down.
    #[test]
    fn the_exponent_leaves_one_bit_of_headroom() {
        for absmax in [1.0f64, 0.01, 37.5, 1e-4] {
            let e = site_exponent(absmax);
            let code = absmax * 2f64.powi(e);
            assert!(code <= A16_CODE_MAX as f64, "absmax {absmax} maps to {code}, past the rail");
            assert!(code > A16_CODE_MAX as f64 / 8.0, "absmax {absmax} maps to {code}, wasting the range");
        }
        // A site that never moved does not produce an infinite exponent.
        assert_eq!(site_exponent(0.0), 0);
    }

    /// **A projection's narrowing must undo the weight row's scale.** This is the one that was
    /// missing: without `s_c` every output channel comes out scaled by a different wrong factor.
    #[test]
    fn a_projection_undoes_the_weight_row_scale() {
        // Two channels whose weights differ in magnitude by 100×.
        let scales = vec![1.0e-2, 1.0e-4];
        let (e_in, e_out) = (10, 12);
        let params = projection_params(&scales, e_in, e_out);
        for (p, s) in params.iter().zip(&scales) {
            let gain = p.multiplier as f64 / 2f64.powi(p.shift as i32);
            let want = s * 2f64.powi(e_out - e_in);
            assert!((gain - want).abs() / want < 1e-12, "gain {gain} should be {want}");
        }
        // The channels' narrowings differ by the same 100×, which is exactly what a per-channel
        // table is for.
        let g = |p: &A16QuantParams| p.multiplier as f64 / 2f64.powi(p.shift as i32);
        assert!((g(&params[0]) / g(&params[1]) - 100.0).abs() < 1e-9);
    }

    /// **A norm site carries γ.** Unity here drops the model's learned per-channel scaling.
    #[test]
    fn a_norm_site_carries_the_learned_gain() {
        let gamma = vec![1.0f32, 2.0, 0.25];
        let params = norm_params(&gamma, K as i32);
        for (p, g) in params.iter().zip(&gamma) {
            let gain = p.multiplier as f64 / 2f64.powi(p.shift as i32);
            assert!((gain - *g as f64).abs() < 1e-9, "at e_out = K the gain IS γ, got {gain} for {g}");
        }
        // And a different output exponent scales all of them together.
        let shifted = norm_params(&gamma, K as i32 + 3);
        for (a, b) in shifted.iter().zip(&params) {
            let ratio = (a.multiplier as f64 / 2f64.powi(a.shift as i32)) / (b.multiplier as f64 / 2f64.powi(b.shift as i32));
            assert!((ratio - 8.0).abs() < 1e-9);
        }
    }

    /// **The recurrence's multipliers must fit its `i64` bound.** `mul_shift` normalizes into all
    /// 62 bits, which the state's narrowings refuse — and the refusal is what caught it, at the
    /// first GatedDeltaNet head of the first calibrated conversion.
    #[test]
    fn the_state_triples_fit_the_recurrence_bound() {
        let bound = kaspa_consensus_core::palw_qwen36_ops::QWEN36_STATE_MULT_MAX;
        for (e_state, e_value, e_out) in [(0i32, 0i32, 0i32), (12, 10, 14), (20, -5, 30), (9, 20, -3)] {
            let (read, write, out) = gdn_params(e_state, e_value, e_out);
            for p in [read, write, out] {
                assert!(p.multiplier.abs() <= bound, "multiplier {} is past the bound {bound}", p.multiplier);
                assert!(p.shift <= 62);
            }
        }
        // And the bounded form still reproduces the gain it was given.
        for x in [1.0f64, 2f64.powi(-23), 2f64.powi(-7), 3.5e-9] {
            let (m, s) = mul_shift_bounded(x, 30);
            let back = m as f64 / 2f64.powi(s as i32);
            assert!((back - x).abs() / x < 1e-8, "{x} came back as {back}");
        }
    }

    /// **The state's exponent is measured, not derived from the bound.** A head whose gate sits at
    /// `decay ≈ 1` sends the contraction bound to a million; the state's grid becomes a million
    /// times finer than it needs to be and the values that actually occur saturate on the first
    /// token. This is what that looked like and what fixed it.
    #[test]
    fn the_state_exponent_holds_what_the_state_reaches() {
        // A state that reaches 2.6 with values at 3.4: the read/write pair must be inverses up to
        // the Q15 the key carries.
        let (e_state, e_value, e_out) = (site_exponent(2.6), site_exponent(3.4), site_exponent(0.58));
        let (read, write, _) = gdn_params(e_state, e_value, e_out);
        let g = |p: &A16QuantParams| p.multiplier as f64 / 2f64.powi(p.shift as i32);
        // read · write = 2^-30 — the two Q15 the key contributes on the way in and on the way out.
        assert!((g(&read) * g(&write) - 2f64.powi(-30)).abs() / 2f64.powi(-30) < 1e-6);
        // And the state's rail holds the value it was measured at, with the headroom the exponent
        // was chosen for.
        assert!(2.6 * 2f64.powi(e_state) < i32::MAX as f64);
    }

    /// The softmax widening is the distance from the logit codes up to Q[K], clamped to the op's
    /// domain.
    #[test]
    fn the_softmax_widening_is_the_distance_to_qk() {
        assert_eq!(softmax_up_bits(K as i32), 0);
        assert_eq!(softmax_up_bits(K as i32 - 10), 10);
        assert_eq!(softmax_up_bits(K as i32 + 5), 0, "a logit already past Qk is not widened");
        assert_eq!(softmax_up_bits(-100), 62, "and the widening stays inside the op's domain");
    }
}
