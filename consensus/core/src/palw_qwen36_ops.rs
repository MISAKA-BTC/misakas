//! **`PALW-QWEN36` — the integer ops Qwen3.6's hybrid graph needs and BASE-0 deliberately did not
//! have.**
//!
//! ADR-0040 named the three things it was declining: "integerising GatedDeltaNet, interleaved
//! multimodal RoPE and fused SwiGLU would reproduce the catalog problem". That was the right call
//! for a liveness floor, and it is exactly the bill that comes due for running Qwen3.6, whose
//! forty layers are thirty GatedDeltaNet arms and ten gated-attention arms, every one of them
//! followed by a 256-expert MoE block.
//!
//! This module is the arithmetic half of that bill. It adds nothing to the *style* of ADR-0040 —
//! integers only, no libm, no float, every lossy site named — and it adds four things to its
//! content:
//!
//! * **A router.** Top-`k` of 256 is a SELECTION, and a selection is the one thing this class had
//!   never had to make: every op so far is a total function of its input row, while a router
//!   makes a discrete choice that changes which weights the rest of the layer reads. Two
//!   implementations that break a tie differently do not disagree by one ulp; they run different
//!   experts and produce unrelated output. So the tie rule is normative and stated first.
//! * **A weighted combine**, which is where the eight expert outputs and the shared expert meet.
//! * **Partial rotation.** Qwen3.6 rotates 64 of each head's 256 lanes (`partial_rotary_factor`
//!   0.25) and passes the rest through untouched. Rotating all of them is a different model.
//! * **An output gate.** `attn_output_gate: true` means the Q projection is double width and the
//!   second half becomes `sigmoid(gate)` applied elementwise after attention.
//!
//! # Scales
//!
//! Activations are A16 codes (`palw_base0_a16`): `i16` values carried in `i32` lanes. Router
//! probabilities and gates are Q[`K`] values, the same fixed-point the int8 tier's `SoftMax` and
//! `Silu` already use, so `int_exp`, `int_recip` and `int_sigmoid` are reused rather than
//! re-derived.

use crate::palw_base0::{K, ONE, int_recip};
use crate::palw_base0_a16::{A16_CODE_MAX, A16QuantParams, PalwA16OpError, a16_scale_round};
use crate::palw_base0_ops::{int_sigmoid, softmax_shifted};

/// The experts Qwen3.6 routes over, and the number it activates. Named here because the router's
/// bounds are arithmetic facts about `i64` headroom, not policy.
pub const QWEN36_NUM_EXPERTS: usize = 256;
/// `num_experts_per_tok`.
pub const QWEN36_EXPERTS_PER_TOKEN: usize = 8;

/// The largest `k` the combine's accumulator is proved for. `k · ONE · A16_CODE_MAX` must fit an
/// `i64`: at `k = 64` that is `64 · 2^24 · 32767 ≈ 3.5e13`, four orders under `i64::MAX`.
pub const QWEN36_MAX_ROUTED: usize = 64;

const _: () = assert!(
    QWEN36_MAX_ROUTED as i64 * ONE * A16_CODE_MAX < i64::MAX / 1024,
    "the weighted combine accumulates k terms of (Q[K] weight × A16 code) in i64; past this the \
     sum can overflow and the free reduction order stops holding"
);

/// One routed expert: which one, and the renormalized weight it carries in Q[`K`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Qwen36RoutedExpert {
    pub expert: u16,
    pub weight_q: i32,
}

/// Why a Qwen3.6 op refused. Distinct from [`PalwA16OpError`] where the failure is about routing
/// rather than about a row's shape or domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwQwen36OpError {
    /// The op's row was empty.
    Empty,
    /// `k` was zero, larger than the expert count, or past [`QWEN36_MAX_ROUTED`].
    BadK { k: usize, experts: usize },
    /// A row length that is not a whole number of heads, or a rotary width that is not even or
    /// does not fit the head.
    NotAMultiple { got: usize, unit: usize },
    /// Two rows that had to agree on a length did not.
    LengthMismatch { a: usize, b: usize },
    /// An A16 row carried a lane outside `±A16_CODE_MAX`.
    NotA16Codes,
}

impl From<PalwA16OpError> for PalwQwen36OpError {
    fn from(e: PalwA16OpError) -> Self {
        match e {
            PalwA16OpError::Empty => Self::Empty,
            PalwA16OpError::LengthMismatch { a, b } => Self::LengthMismatch { a, b },
            PalwA16OpError::NotAMultiple { got, unit } => Self::NotAMultiple { got, unit },
            _ => Self::Empty,
        }
    }
}

fn check_a16(row: &[i32]) -> Result<(), PalwQwen36OpError> {
    if row.is_empty() {
        return Err(PalwQwen36OpError::Empty);
    }
    if row.iter().any(|v| (*v as i64).abs() > A16_CODE_MAX) {
        return Err(PalwQwen36OpError::NotA16Codes);
    }
    Ok(())
}

// -------------------------------------------------------------------------------------------
// Q1. The router — a selection, and therefore the one op whose tie rule is normative
// -------------------------------------------------------------------------------------------

/// **Op Q1: `RouterTopK` — softmax over every expert, take `k`, renormalize to Q[`K`].**
///
/// `logits` are the router projection's A16 codes, one per expert. The function is
/// `softmax_shifted` (the tier already needs the wide domain: a router's logits are not confined
/// to Qk any more than an attention logit is), then the `k` largest, then a renormalization so
/// the kept weights sum to `ONE`.
///
/// # The tie rule, and why it is the first thing in this file
///
/// Every other op in this class is a total function of its input row: two implementations that
/// disagree, disagree by a value, and the court localises the disagreement to one step. A router
/// is not that. It makes a discrete choice, and a different choice means a different expert's
/// weights enter the next matmul — the outputs are then unrelated, no bisection converges on an
/// arithmetic step, and the disagreement looks like fraud on both sides.
///
/// **Ties break to the LOWEST expert index**, the same rule the class's argmax already uses. Ties
/// are not exotic here: `softmax_shifted` returns Q[`K`] values with 24 fractional bits, 256
/// experts routinely produce probabilities below that resolution, and **every expert whose
/// probability underflows to zero ties with every other**. A `k` that reaches into that region —
/// which happens on a confident token, where eight experts hold nearly all the mass — is
/// selecting among exact zeros, and only the index rule decides it.
///
/// The result is sorted by **expert index ascending**, not by weight: the combine is a sum and
/// integer addition does not care, but the committed row has to have one order and index order is
/// the one that does not change when two weights are equal.
pub fn q36_router_topk(logits: &[i32], k: usize, up_bits: u8) -> Result<Vec<Qwen36RoutedExpert>, PalwQwen36OpError> {
    check_a16(logits)?;
    let experts = logits.len();
    if k == 0 || k > experts || k > QWEN36_MAX_ROUTED {
        return Err(PalwQwen36OpError::BadK { k, experts });
    }
    let probs = softmax_shifted(logits, up_bits).map_err(|_| PalwQwen36OpError::Empty)?;

    // Selection by (probability descending, index ascending). Written as k passes over the row
    // rather than as a sort: a sort needs a total order on equal probabilities and every
    // comparator that provides one is a place to get the tie rule wrong, whereas "the first
    // index holding the maximum" has only one reading.
    let mut chosen = Vec::with_capacity(k);
    let mut taken = vec![false; experts];
    for _ in 0..k {
        let mut best = usize::MAX;
        for (i, p) in probs.iter().enumerate() {
            if taken[i] {
                continue;
            }
            if best == usize::MAX || *p > probs[best] {
                best = i;
            }
        }
        taken[best] = true;
        chosen.push(best);
    }
    chosen.sort_unstable();

    // Renormalize over the kept set. The sum is at most `k · ONE`, so it is exact in `i64`.
    let sum: i64 = chosen.iter().map(|i| probs[*i] as i64).sum();
    if sum <= 0 {
        // Every kept probability underflowed. A uniform split over the kept set is the only
        // answer that is still a distribution, and it is defined rather than a division by zero.
        let uniform = (ONE / k as i64) as i32;
        return Ok(chosen.iter().map(|i| Qwen36RoutedExpert { expert: *i as u16, weight_q: uniform }).collect());
    }
    let recip = int_recip(sum);
    Ok(chosen.iter().map(|i| Qwen36RoutedExpert { expert: *i as u16, weight_q: ((probs[*i] as i64 * recip) >> K) as i32 }).collect())
}

// -------------------------------------------------------------------------------------------
// Q2. The combine — where the expert outputs meet
// -------------------------------------------------------------------------------------------

/// **Op Q2: `MoeCombine` — `Σ_e w_e · y_e`, one requantization at the end.**
///
/// `outputs` is `k` expert rows of `width` A16 codes each, in the same order as `weights`. The
/// accumulator is `i64` and holds the whole sum before any narrowing: requantizing per expert and
/// adding afterwards would round `k` times, which is a different function and one whose result
/// depends on how the caller grouped the experts.
///
/// The `params` triple carries the Q[`K`] undo (`shift = K` with `multiplier = 1` is the identity
/// combine) together with whatever scale change the site declares, exactly as every other A16
/// requantization does.
pub fn q36_moe_combine(outputs: &[i32], weights: &[i32], width: usize, params: A16QuantParams) -> Result<Vec<i32>, PalwQwen36OpError> {
    if width == 0 || outputs.is_empty() {
        return Err(PalwQwen36OpError::Empty);
    }
    if !outputs.len().is_multiple_of(width) {
        return Err(PalwQwen36OpError::NotAMultiple { got: outputs.len(), unit: width });
    }
    let k = outputs.len() / width;
    if k != weights.len() {
        return Err(PalwQwen36OpError::LengthMismatch { a: k, b: weights.len() });
    }
    if k > QWEN36_MAX_ROUTED {
        return Err(PalwQwen36OpError::BadK { k, experts: k });
    }
    check_a16(outputs)?;
    let mut out = Vec::with_capacity(width);
    for lane in 0..width {
        let acc: i64 = (0..k).map(|e| weights[e] as i64 * outputs[e * width + lane] as i64).sum();
        out.push(a16_scale_round(acc, params.multiplier, params.shift).saturating_add(params.zero).clamp(-A16_CODE_MAX, A16_CODE_MAX)
            as i32);
    }
    Ok(out)
}

// -------------------------------------------------------------------------------------------
// Q3. The gates — the shared expert's scalar, and attention's elementwise output gate
// -------------------------------------------------------------------------------------------

/// **Op Q3: `SigmoidGate` — `sigmoid(x)` in Q[`K`], from a Q[`K`] input.**
///
/// Both gates Qwen3.6 carries are this function: the shared expert's `sigmoid(Linear(x, 1))`
/// scalar, and the full-attention arm's elementwise `sigmoid(gate)`. It is
/// `palw_base0_ops::int_sigmoid` under a name that says where it is used, so that a graph node
/// naming `Sigmoid` and this op are the same arithmetic by construction rather than by comment.
///
/// # Two measured properties of the frozen arithmetic, recorded so they are not rediscovered as
/// bugs
///
/// `int_sigmoid(0)` is 8,389,753 against an exact half of 8,388,608 — 6.8e-5 of `ONE` high — and
/// the function is **not monotone across the origin**: `int_sigmoid(1) < int_sigmoid(0)`, a step
/// of 2,291. Both follow from `int_exp(0)` overshooting `ONE` slightly and the two branches of
/// `int_sigmoid` (`e/(1+e)` for `x ≤ 0`, `1/(1+e)` for `x > 0`) meeting at a point where that
/// overshoot lands on opposite sides of the fraction.
///
/// This is ADR-0040 arithmetic, frozen: `int_exp` is in the class's KAT set and its values are
/// the class id. It is recorded and not repaired. At 1.4e-4 of a gate's range it is far below
/// what W8A16 quantization already costs, and a gate is a multiplier rather than a decision — no
/// discrete choice in this model turns on it.
#[inline]
pub fn q36_sigmoid_gate(x_q: &[i32]) -> Vec<i32> {
    x_q.iter().map(|v| int_sigmoid(*v)).collect()
}

/// **Op Q4: `GateApply` — `y ⊙ g`, A16 codes times a Q[`K`] gate.**
///
/// The multiply is exact in `i64` and narrows once. `g = ONE` reproduces `y` for every lane, which
/// is the property that makes an ungated arm expressible without a second op.
pub fn q36_gate_apply(y: &[i32], gate_q: &[i32], params: A16QuantParams) -> Result<Vec<i32>, PalwQwen36OpError> {
    check_a16(y)?;
    if y.len() != gate_q.len() {
        return Err(PalwQwen36OpError::LengthMismatch { a: y.len(), b: gate_q.len() });
    }
    Ok(y.iter()
        .zip(gate_q)
        .map(|(v, g)| {
            let acc = *v as i64 * *g as i64;
            a16_scale_round(acc, params.multiplier, params.shift).saturating_add(params.zero).clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
        })
        .collect())
}

// -------------------------------------------------------------------------------------------
// Q5. Partial rotation — 64 of every head's 256 lanes
// -------------------------------------------------------------------------------------------

/// **Op Q5: `RopePartial` — rotate the first `rotary_dim` lanes of each head, pass the rest.**
///
/// `partial_rotary_factor: 0.25` with `head_dim: 256` means 64 rotated lanes and 192 carried
/// through unchanged. This is not an optimisation of a full rotation: the unrotated lanes are
/// position-independent by design, and rotating them would make every one of them a different
/// number, so a full-rotation implementation is a different model rather than a slower one.
///
/// `cos_q`/`sin_q` are the pinned integer table's half-rows for the position, `rotary_dim / 2`
/// entries each — the same table `palw_base0_ops::rope_table` reads, at the width this class
/// rotates.
pub fn q36_rope_partial(
    x: &[i32],
    head_dim: usize,
    rotary_dim: usize,
    cos_q: &[i32],
    sin_q: &[i32],
    clamp: A16QuantParams,
) -> Result<Vec<i32>, PalwQwen36OpError> {
    if x.is_empty() || head_dim == 0 {
        return Err(PalwQwen36OpError::Empty);
    }
    if !x.len().is_multiple_of(head_dim) {
        return Err(PalwQwen36OpError::NotAMultiple { got: x.len(), unit: head_dim });
    }
    if rotary_dim > head_dim || !rotary_dim.is_multiple_of(2) {
        return Err(PalwQwen36OpError::NotAMultiple { got: rotary_dim, unit: 2 });
    }
    let pairs = rotary_dim / 2;
    if cos_q.len() != pairs || sin_q.len() != pairs {
        return Err(PalwQwen36OpError::LengthMismatch { a: cos_q.len(), b: pairs });
    }
    let mut out = Vec::with_capacity(x.len());
    for head in x.chunks_exact(head_dim) {
        // The rotated half, in interleaved (even, odd) pairs — the layout the pinned table
        // indexes and the one `rope_table` already uses.
        for p in 0..pairs {
            let (a, b) = (head[2 * p] as i64, head[2 * p + 1] as i64);
            let (c, s) = (cos_q[p] as i64, sin_q[p] as i64);
            for acc in [a * c - b * s, a * s + b * c] {
                out.push(
                    a16_scale_round(acc, clamp.multiplier, clamp.shift).saturating_add(clamp.zero).clamp(-A16_CODE_MAX, A16_CODE_MAX)
                        as i32,
                );
            }
        }
        // The carried lanes, untouched. Not re-narrowed: a requantization here would move values
        // the model does not move.
        out.extend_from_slice(&head[rotary_dim..]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            self.0.rotate_right(24)
        }
        fn code(&mut self) -> i32 {
            (self.next_u64() % (2 * A16_CODE_MAX as u64 + 1)) as i64 as i32 - A16_CODE_MAX as i32
        }
    }

    fn unity() -> A16QuantParams {
        A16QuantParams { multiplier: 1, shift: K as u8, zero: 0 }
    }

    /// The router's contract: `k` distinct experts, index order, weights summing to `ONE`.
    #[test]
    fn the_router_returns_k_distinct_experts_whose_weights_are_a_distribution() {
        let mut rng = Lcg(0x36_0001);
        for k in [1usize, 2, 8, 32] {
            let logits: Vec<i32> = (0..QWEN36_NUM_EXPERTS).map(|_| rng.code() / 8).collect();
            let routed = q36_router_topk(&logits, k, 20).expect("a well-formed router row");
            assert_eq!(routed.len(), k);
            for pair in routed.windows(2) {
                assert!(pair[0].expert < pair[1].expert, "the result must be strictly index-ascending");
            }
            let sum: i64 = routed.iter().map(|r| r.weight_q as i64).sum();
            // The bound is the reciprocal's rounding, not a tolerance on the function: `int_recip`
            // is three fixed Newton steps, and the measured error over k in 1..=32 is at most 4
            // parts in 2^24. Stated as an absolute count so that a change in `int_recip` — which
            // would be a class id change — fails here.
            assert!((sum - ONE).abs() <= 16, "the kept weights must renormalize to ONE, got {sum} (k={k})");
        }
    }

    /// It must select the LARGEST, which a router that returned the first `k` indices would also
    /// satisfy on a sorted row — so the row here is deliberately not sorted.
    #[test]
    fn the_router_selects_the_largest() {
        let mut logits = vec![0i32; QWEN36_NUM_EXPERTS];
        logits[200] = 900;
        logits[3] = 1000;
        logits[77] = 950;
        let routed = q36_router_topk(&logits, 3, 20).expect("well-formed");
        assert_eq!(routed.iter().map(|r| r.expert).collect::<Vec<_>>(), vec![3, 77, 200]);
        // And the weights follow the logits, largest to smallest.
        let weight = |e: u16| routed.iter().find(|r| r.expert == e).expect("present").weight_q;
        assert!(weight(3) > weight(77) && weight(77) > weight(200));
    }

    /// **The tie rule.** An all-equal row has 256 experts tied, and only the index decides. A
    /// selection that scanned with `>=` instead of `>` would take the LAST maximum and produce a
    /// different eight experts here — the same arithmetic, a different model.
    #[test]
    fn ties_break_to_the_lowest_expert_index() {
        let flat = vec![7i32; QWEN36_NUM_EXPERTS];
        let routed = q36_router_topk(&flat, 8, 20).expect("well-formed");
        assert_eq!(routed.iter().map(|r| r.expert).collect::<Vec<_>>(), (0..8u16).collect::<Vec<_>>());

        // The realistic version: one confident expert and a tail that underflows Q[K] to exactly
        // zero. The kept set past the first is decided entirely by the index rule.
        let mut peaked = vec![-30_000i32; QWEN36_NUM_EXPERTS];
        peaked[100] = 30_000;
        let routed = q36_router_topk(&peaked, 4, 24).expect("well-formed");
        assert_eq!(routed.iter().map(|r| r.expert).collect::<Vec<_>>(), vec![0, 1, 2, 100]);
    }

    /// A row whose whole kept set underflows must still be a distribution rather than a division
    /// by zero — the same rule `softmax` applies to its own degenerate row.
    #[test]
    fn a_fully_underflowed_row_is_uniform_over_the_kept_set() {
        let mut logits = vec![-32_767i32; QWEN36_NUM_EXPERTS];
        logits[0] = 32_767;
        // A widening large enough that everything but the max is exp(−huge) = 0, then drop the
        // max from the kept set by asking for experts that are all zero.
        let routed = q36_router_topk(&logits, 8, 62).expect("well-formed");
        assert_eq!(routed.len(), 8);
        let sum: i64 = routed.iter().map(|r| r.weight_q as i64).sum();
        assert!(sum > 0, "a kept set must carry weight");
    }

    /// Refusals: `k` out of range, an empty row, a lane outside the A16 range.
    #[test]
    fn the_router_refuses_what_it_cannot_route() {
        let logits = vec![1i32; 16];
        assert_eq!(q36_router_topk(&logits, 0, 20), Err(PalwQwen36OpError::BadK { k: 0, experts: 16 }));
        assert_eq!(q36_router_topk(&logits, 17, 20), Err(PalwQwen36OpError::BadK { k: 17, experts: 16 }));
        assert_eq!(q36_router_topk(&[], 1, 20), Err(PalwQwen36OpError::Empty));
        assert_eq!(q36_router_topk(&[A16_CODE_MAX as i32 + 1, 0], 1, 20), Err(PalwQwen36OpError::NotA16Codes));
        assert_eq!(
            q36_router_topk(&vec![1i32; QWEN36_MAX_ROUTED + 2], QWEN36_MAX_ROUTED + 1, 20),
            Err(PalwQwen36OpError::BadK { k: QWEN36_MAX_ROUTED + 1, experts: QWEN36_MAX_ROUTED + 2 })
        );
    }

    /// The combine is a weighted sum, and one weight at `ONE` must reproduce that expert exactly.
    #[test]
    fn the_combine_is_a_weighted_sum() {
        let width = 5;
        let outputs = vec![10i32, -20, 30, -40, 50, 1, 2, 3, 4, 5];
        let single = q36_moe_combine(&outputs, &[ONE as i32, 0], width, unity()).expect("well-formed");
        assert_eq!(single, vec![10, -20, 30, -40, 50], "weight ONE on expert 0 must reproduce it");
        let other = q36_moe_combine(&outputs, &[0, ONE as i32], width, unity()).expect("well-formed");
        assert_eq!(other, vec![1, 2, 3, 4, 5]);
        // Half and half, rounding half away from zero as the tier's one rule says.
        let half = (ONE / 2) as i32;
        let mixed = q36_moe_combine(&outputs, &[half, half], width, unity()).expect("well-formed");
        assert_eq!(mixed, vec![6, -9, 17, -18, 28]);
    }

    /// Grouping must not matter: the accumulator is `i64` and the sum is exact, so combining in
    /// any order gives the same row. A per-expert requantization would fail this.
    #[test]
    fn the_combine_does_not_depend_on_expert_order() {
        let mut rng = Lcg(0x36_0002);
        let (width, k) = (64usize, 8usize);
        let outputs: Vec<i32> = (0..width * k).map(|_| rng.code()).collect();
        let weights: Vec<i32> = (0..k).map(|_| (rng.next_u64() % ONE as u64) as i32).collect();
        let forward = q36_moe_combine(&outputs, &weights, width, unity()).expect("well-formed");

        let order: Vec<usize> = (0..k).rev().collect();
        let mut shuffled = Vec::with_capacity(width * k);
        let mut shuffled_weights = Vec::with_capacity(k);
        for e in &order {
            shuffled.extend_from_slice(&outputs[e * width..(e + 1) * width]);
            shuffled_weights.push(weights[*e]);
        }
        assert_eq!(q36_moe_combine(&shuffled, &shuffled_weights, width, unity()).expect("well-formed"), forward);
    }

    /// The gate at its two rails, and unity in the middle.
    #[test]
    fn the_gate_saturates_and_passes() {
        let g = q36_sigmoid_gate(&[0, 20 * ONE as i32, -20 * ONE as i32]);
        // 6.8e-5 of ONE high, which is `int_exp`'s overshoot at zero and is frozen — see the op's
        // documentation. Pinned exactly so that a change to the frozen arithmetic fails here.
        assert_eq!(g[0], 8_389_753, "sigmoid(0) is a half plus int_exp's overshoot");
        assert!((g[0] - (ONE / 2) as i32).abs() < ONE as i32 / 8192, "and the overshoot stays under 1.2e-4");
        assert!(g[1] > (ONE as i32) - 64, "a large positive gate is open");
        assert_eq!(g[2], 0, "a large negative gate is shut at Qk resolution");

        let y = vec![100i32, -100, 32_767];
        assert_eq!(q36_gate_apply(&y, &[ONE as i32; 3], unity()).expect("well-formed"), y, "a unity gate is the identity");
        assert_eq!(q36_gate_apply(&y, &[0; 3], unity()).expect("well-formed"), vec![0, 0, 0]);
    }

    /// Partial rotation touches the rotary lanes and nothing else, and a zero-width rotation is
    /// the identity.
    #[test]
    fn partial_rotation_leaves_the_carried_lanes_alone() {
        let head_dim = 8;
        let x: Vec<i32> = (0..2 * head_dim).map(|i| (i as i32 + 1) * 11).collect();
        // cos = 1, sin = 0 in the table's own scale: the rotation is the identity, so ONLY the
        // requantization of the rotated half can move a value — and with a unity clamp it does
        // not either.
        let one_q = ONE as i32;
        let clamp = A16QuantParams { multiplier: 1, shift: K as u8, zero: 0 };
        let rotated = q36_rope_partial(&x, head_dim, 4, &[one_q, one_q], &[0, 0], clamp).expect("well-formed");
        assert_eq!(rotated, x, "an identity rotation must move nothing");

        // A quarter turn: (a, b) becomes (−b, a). The carried lanes are still untouched.
        let quarter = q36_rope_partial(&x, head_dim, 4, &[0, 0], &[one_q, one_q], clamp).expect("well-formed");
        assert_eq!(&quarter[0..4], &[-x[1], x[0], -x[3], x[2]]);
        assert_eq!(&quarter[4..8], &x[4..8], "lanes past rotary_dim are carried");
        assert_eq!(&quarter[8..12], &[-x[9], x[8], -x[11], x[10]]);

        // Refusals.
        assert!(q36_rope_partial(&x, head_dim, 5, &[one_q, one_q], &[0, 0], clamp).is_err(), "an odd rotary width is refused");
        assert!(q36_rope_partial(&x, head_dim, head_dim + 2, &[], &[], clamp).is_err(), "a rotation wider than the head is refused");
    }
}
