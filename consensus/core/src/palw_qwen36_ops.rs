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

use crate::palw_base0::{K, LN2_Q, ONE, int_exp, int_recip, int_rsqrt, rounding_shift_right_64};
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

// -------------------------------------------------------------------------------------------
// Q6. `IntLn` — the fourth transcendental, and the one the recurrence's decay needs
// -------------------------------------------------------------------------------------------

/// **Op Q6: `IntLn` — natural log of a positive Q[`K`] value.**
///
/// ADR-0040 Decision F gives the class three integer transcendentals: `IntExp`, `IntRsqrt` and
/// `IntRecip`. Gated DeltaNet needs a fourth, and the reason is worth stating because it is not
/// obvious from the architecture diagram.
///
/// The decay gate is `exp(−exp(A_log) · softplus(a))`. Writing `c = exp(A_log)` — a learned
/// per-head constant, registered — and `u = sigmoid(−a)`, the identity
/// `exp(−c·softplus(a)) = (1 + e^a)^(−c) = u^c` turns it into a power. At `c = 1` that is
/// `int_sigmoid` and nothing new is needed; at any other `c` it is `exp(c · ln u)`, and there is
/// no logarithm in the catalog.
///
/// # The algorithm, and the Newton step that was removed because it made things worse
///
/// `x = M · 2^s` with `M in [1, 2)`, so `ln x = ln M + s*ln2`. `ln M` uses the atanh series with
/// `t = (M-1)/(M+1)` in `[0, 1/3]`:
///
/// ```text
/// ln M = 2*(t + t^3/3 + t^5/5 + t^7/7 + t^9/9 + t^11/11)
/// ```
///
/// Truncating there leaves at most `2*t^13/13`, about 9.6e-8, which is under two units of Q[`K`].
/// `t` itself is an exact `i128` division rather than `int_recip`, because `int_recip`'s three
/// Newton steps would be the dominant error in a quantity that is the series' whole argument.
///
/// **The first draft ended with a Newton step on `f(y) = e^y - x`** — `y <- y - 1 + x*e^(-y)`,
/// built from `int_exp` and `int_recip`, which looked elegant: no new arithmetic concept, and the
/// truncation point becomes a performance choice rather than a precision one. Measured, it made
/// the answer **fourteen times worse**. At `x` near 0.0045 the series lands 494 Q[`K`] units from
/// the true value and the refined result lands 7,202 away, because the refinement's accuracy is
/// `int_exp`'s (about 4e-4 relative) and the series' is 5e-6. A Newton step only squares an error
/// when the function it evaluates is more accurate than the estimate it is correcting.
///
/// So the series stands alone, and the two extra terms cost less than the step did.
///
/// Refuses `x ≤ 0` with `None` rather than returning a sentinel: `ln 0` is not a number and a
/// class that answered anyway would be defining one.
pub fn q36_int_ln(x: i64) -> Option<i64> {
    if x <= 0 {
        return None;
    }
    // Normalize into `[ONE, 2·ONE)`. `s` is the power of two removed.
    let mut m = x;
    let mut s: i64 = 0;
    while m >= 2 * ONE {
        m >>= 1;
        s += 1;
    }
    while m < ONE {
        m <<= 1;
        s -= 1;
    }
    // t = (m − ONE) / (m + ONE) in Q[K]. The division is exact-width in i128 rather than through
    // `int_recip`, because `int_recip`'s three Newton steps are the dominant error here and this
    // quotient is the series' whole argument.
    let t = (((m - ONE) as i128) << K) / ((m + ONE) as i128);
    let t = t as i64;
    let t2 = ((t as i128 * t as i128) >> K) as i64;
    let mut term = t;
    let mut sum = t;
    for odd in [3i64, 5, 7, 9, 11] {
        term = ((term as i128 * t2 as i128) >> K) as i64;
        sum += term / odd;
    }
    Some(2 * sum + s * LN2_Q as i64)
}

/// **Op Q7: `PowQ` — `u^c` for `u ∈ (0, 1]` and a registered non-negative `c`, in Q[`K`].**
///
/// `exp(c · ln u)`, which is the decay gate once the identity above is applied. `u = ONE` returns
/// `ONE` without touching either transcendental: the identity is the common case (a head whose
/// `A_log` is zero) and routing it through a log and an exp would put two roundings on a value
/// that has none.
pub fn q36_pow_q(u: i64, c: i64) -> i64 {
    if u <= 0 {
        return 0;
    }
    if u >= ONE || c == 0 {
        return ONE;
    }
    let Some(ln_u) = q36_int_ln(u) else { return 0 };
    // `ln u ≤ 0` for `u ≤ 1`, and `c ≥ 0`, so the product is non-positive and `int_exp` is defined
    // on it. A negative `c` would be an inverted gate and is refused by clamping to zero.
    if c < 0 {
        return ONE;
    }
    let arg = ((c as i128 * ln_u as i128) >> K).clamp(i32::MIN as i128, 0) as i32;
    // **The clamp is load-bearing, not defensive.** `u^c` for `u` in (0, 1] and `c >= 0` is a
    // value in (0, 1] by definition, and the recurrence's gate refuses anything outside `[0, ONE]`
    // because a "decay" above one is an amplifier. But the frozen `int_exp` OVERSHOOTS at zero —
    // `int_exp(0)` is 6.8e-5 of `ONE` above it — and `arg` reaches zero whenever `u` is close
    // enough to one that `c * ln u` rounds away, which on real weights is a head whose `dt` is
    // strongly negative. The first run of this on a real checkpoint refused at the very first
    // GatedDeltaNet head for exactly that reason.
    (int_exp(arg) as i64).clamp(0, ONE)
}

// -------------------------------------------------------------------------------------------
// Q8. `L2Norm` — the key vector the delta rule needs on the unit sphere
// -------------------------------------------------------------------------------------------

/// **Op Q8: `L2Norm` — `x / ‖x‖`, A16 codes in, Q15 codes out.**
///
/// The delta rule's stability argument is that `k` is a unit vector: `S(I − β k kᵀ)` is a
/// contraction only when `‖k‖ = 1`, and the state's bound in [`q36_gdn_step`] is derived from it.
/// So this is not a normalization for conditioning, it is part of the recurrence's definition.
///
/// The class's step vocabulary already names `L2Norm` as "`1/max(sqrt(sum), eps)` — a different
/// composition than RmsNorm". Here it is `int_rsqrt` of the exact sum of squares, then one
/// multiply per lane. `sum` is exact in `i64`: 128 lanes of `32767²` is `1.4e11`.
///
/// A zero vector returns zero rather than dividing: it has no direction, and inventing one would
/// make the recurrence's next state depend on which implementation invented it.
pub fn q36_l2_norm(x: &[i32]) -> Result<Vec<i32>, PalwQwen36OpError> {
    check_a16(x)?;
    let sum: i64 = x.iter().map(|v| *v as i64 * *v as i64).sum();
    if sum <= 0 {
        return Ok(vec![0; x.len()]);
    }
    // **Two things about `int_rsqrt` that a caller has to know, both learned the hard way.**
    //
    // 1. It takes a Q[`K`] value, not a plain integer — `rms_norm` feeds it
    //    `(sum_squares << K) / n` for exactly this reason. Passing the raw sum asks for
    //    `1/sqrt(sum/2^24)`, which is 2^12 too large; every lane then saturates at the code rail
    //    and the "unit" vector has norm `sqrt(n)`. The first draft did that.
    //
    // 2. **Its relative accuracy depends on the MAGNITUDE of its argument**, because the answer
    //    is returned in Q[`K`] and a small answer has few significant bits there. Measured:
    //    2.4e-5 relative at an argument near `2^25`, but 1.0e-3 at `1e11`, where `1/sqrt(x)` is
    //    3.2e-6 and one Q[`K`] unit is 1.9 % of it. The iterations are not the limit; the output
    //    grid is.
    //
    // So the exponent is taken out here rather than inside `int_rsqrt`: `S = m * 4^e` with `m` in
    // `[1, 4)` keeps the reciprocal square root near 1 where Q[`K`] has all of its resolution, and
    // the `2^-e` is applied to the PRODUCT, where it is a shift on a number that has bits to
    // spare. That is the difference between a 0.22 % norm and a 0.02 % one.
    let bit = 63 - sum.leading_zeros() as i32;
    let e = bit.div_euclid(2);
    let two_e = 2 * e;
    let m_q = if two_e >= K as i32 { sum >> (two_e - K as i32) } else { sum << (K as i32 - two_e) };
    let rsqrt = int_rsqrt(m_q);
    // `x * rsqrt` is `(x / sqrt(m)) * 2^K`; the wanted code is `(x / sqrt(S)) * 2^15`, and
    // `sqrt(S) = sqrt(m) * 2^e`.
    let shift = K as i32 - 15 + e;
    Ok(x.iter()
        .map(|v| {
            let product = *v as i128 * rsqrt as i128;
            let scaled = if shift >= 0 { product >> shift } else { product << (-shift) };
            scaled.clamp(-(A16_CODE_MAX as i128), A16_CODE_MAX as i128) as i32
        })
        .collect())
}

// -------------------------------------------------------------------------------------------
// Q9. `SsmConv` — the four-tap causal convolution over the qkv channels
// -------------------------------------------------------------------------------------------

/// **Op Q9: `SsmConv` — a depthwise causal convolution, kernel 4.**
///
/// `linear_conv_kernel_dim: 4`. Each channel is convolved with its own four taps over the last
/// four positions, oldest first, with positions before the start treated as zero. Depthwise, so
/// there is no reduction across channels and no accumulator to bound beyond one lane's four
/// products.
///
/// `history` is the channel-major window `[t−3, t−2, t−1, t]` — the caller keeps it, because the
/// runtime's cache is the runtime's problem and an op that owned a buffer could not be replayed
/// from an oracle.
pub fn q36_ssm_conv(window: &[i32], taps: &[i32], channels: usize, params: &[A16QuantParams]) -> Result<Vec<i32>, PalwQwen36OpError> {
    if channels == 0 || window.len() != 4 * channels || taps.len() != 4 * channels {
        return Err(PalwQwen36OpError::LengthMismatch { a: window.len(), b: 4 * channels });
    }
    if params.len() != channels {
        return Err(PalwQwen36OpError::LengthMismatch { a: params.len(), b: channels });
    }
    check_a16(window)?;
    // **Q[`K`] out, and per channel.** The convolution's consumer is `Silu`, whose input scale is
    // part of the function rather than a convention, so this narrowing targets Q[`K`] and the
    // `i32` rail rather than the A16 code range — the same shape `MatMulRescale` has for the
    // FFN gate. Per channel because the taps are quantized per channel like every other weight;
    // one shared scale would give the quiet channels the loud ones' resolution.
    Ok((0..channels)
        .map(|c| {
            let acc: i64 = (0..4).map(|t| window[t * channels + c] as i64 * taps[t * channels + c] as i64).sum();
            let p = params[c];
            a16_scale_round(acc, p.multiplier, p.shift).saturating_add(p.zero).clamp(i32::MIN as i64, i32::MAX as i64) as i32
        })
        .collect())
}

// -------------------------------------------------------------------------------------------
// Q10. The gated delta rule — the recurrence, and the one op with STATE
// -------------------------------------------------------------------------------------------

/// The largest state magnitude the bounds below are proved for. The state is `i32`, so this is
/// the type's own limit named as a premise rather than assumed.
pub const QWEN36_STATE_MAX: i64 = i32::MAX as i64;

/// The largest multiplier a state-scale narrowing may carry. Every product formed against the
/// state is `state · multiplier` in `i64`, so `2^31 · 2^31 = 2^62` is the bound and this is one
/// bit under it.
pub const QWEN36_STATE_MULT_MAX: i64 = 1 << 30;

const _: () = assert!(
    QWEN36_STATE_MAX * QWEN36_STATE_MULT_MAX < i64::MAX / 2,
    "every state product is formed in i64; past this the recurrence wraps and the wrap is silent"
);

/// `round(x · m / 2^shift)`, half away from zero, in `i64` rather than `i128`.
///
/// [`a16_scale_round`] widens to `i128` and is the right thing everywhere it is used — but a
/// state decay touches `d_v · d_k` lanes per head, which at Qwen3.6's geometry is 16,384 lanes
/// × 32 heads × 30 layers = **15.7 million narrowings per token**. At `i128` that is the whole
/// token. So the state's two narrowings are `i64` with a bound proved at the op's entry instead
/// of a width that makes the bound unnecessary.
#[inline]
fn state_scale(x: i64, m: i64, shift: u8) -> i64 {
    rounding_shift_right_64(x.wrapping_mul(m), shift.min(62))
}

/// One head's recurrent state: `d_v × d_k`, row-major, in the class's registered state scale.
///
/// Held by the caller rather than by the op. A runtime keeps this in an arena and a court
/// reconstructs it from an opening, and an op that owned the buffer could do neither.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qwen36GdnStateV1 {
    pub d_v: usize,
    pub d_k: usize,
    pub s: Vec<i32>,
}

impl Qwen36GdnStateV1 {
    pub fn zeros(d_v: usize, d_k: usize) -> Self {
        Self { d_v, d_k, s: vec![0; d_v * d_k] }
    }
    pub fn is_empty(&self) -> bool {
        self.s.is_empty()
    }
    pub fn len(&self) -> usize {
        self.s.len()
    }
}

/// The registered narrowings one GatedDeltaNet head needs. All three are class data, frozen at
/// registration like every other scale in this tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Qwen36GdnParamsV1 {
    /// `S·k` (state scale × Q15) → the value code scale, so the delta is formed in `v`'s units.
    pub read: A16QuantParams,
    /// `u ⊗ k` (code × Q15) → the state scale.
    pub write: A16QuantParams,
    /// `S·q` (state scale × Q15) → the output code scale.
    pub out: A16QuantParams,
}

/// **Op Q10: `GatedDeltaNet` — one position of the recurrence, plus its output.**
///
/// ```text
/// S ← decay · S                       (the gate)
/// w  = S k                            (what the state already predicts for this key)
/// u  = β · (v − w)                    (the delta rule's correction, in v's units)
/// S ← S + u kᵀ                        (rank-1 write)
/// o  = S q
/// ```
///
/// # Why an integer state is stable, and where the argument actually rests
///
/// The worry with a recurrence in fixed point is that rounding compounds: every step's error
/// enters the state and is carried forward. Here it is carried forward **through the decay**, and
/// `decay < 1`, so an error injected at step `t` is worth `decay^(T−t)` by step `T`. The
/// recurrence is a contraction and its fixed point is not the errors' fixed point — they
/// geometrically vanish rather than accumulating. That is the whole stability argument, and it is
/// why the gate being a real multiply (rather than a shift) matters: a shift-only decay would
/// quantize the contraction rate and, at `decay` near 1, quantize it to 1.
///
/// The magnitude bound comes from `‖k‖ = 1` ([`q36_l2_norm`] is part of the definition, not a
/// conditioning step): `‖S‖` is bounded by `max‖v‖ · β / (1 − decay)`, so the state scale has to
/// carry roughly `log2(1/(1−decay))` bits above the value scale. That is a registration decision
/// the converter makes from calibration, and it is why [`Qwen36GdnParamsV1`] is class data.
///
/// # What this op does NOT do
///
/// No convolution (that is [`q36_ssm_conv`], a separate node), no normalization of `k` (that is
/// [`q36_l2_norm`]), and no output gate (that is [`q36_gate_apply`]). Each is its own step in the
/// court's space, because a fused node is a node a bisection cannot land inside.
#[allow(clippy::too_many_arguments)]
pub fn q36_gdn_step(
    state: &mut Qwen36GdnStateV1,
    k: &[i32],
    v: &[i32],
    q: &[i32],
    decay_q: i64,
    beta_q: i64,
    params: Qwen36GdnParamsV1,
) -> Result<Vec<i32>, PalwQwen36OpError> {
    let (d_v, d_k) = (state.d_v, state.d_k);
    if d_v == 0 || d_k == 0 || state.s.len() != d_v * d_k {
        return Err(PalwQwen36OpError::Empty);
    }
    if k.len() != d_k || q.len() != d_k {
        return Err(PalwQwen36OpError::LengthMismatch { a: k.len(), b: d_k });
    }
    if v.len() != d_v {
        return Err(PalwQwen36OpError::LengthMismatch { a: v.len(), b: d_v });
    }
    check_a16(k)?;
    check_a16(v)?;
    check_a16(q)?;
    if !(0..=ONE).contains(&decay_q) || !(0..=ONE).contains(&beta_q) {
        return Err(PalwQwen36OpError::Empty);
    }
    for m in [params.read.multiplier, params.write.multiplier, params.out.multiplier] {
        if m.abs() > QWEN36_STATE_MULT_MAX {
            return Err(PalwQwen36OpError::Empty);
        }
    }

    // 1. The gate. `|s| ≤ 2^31` and `decay ≤ 2^24`, so the product is at most `2^55`.
    for slot in state.s.iter_mut() {
        *slot = rounding_shift_right_64(*slot as i64 * decay_q, K as u8).clamp(-QWEN36_STATE_MAX, QWEN36_STATE_MAX) as i32;
    }

    // 2. `w = S k`, narrowed into the value code scale. Each product is at most `2^31 · 2^15` and
    //    the sum is over `d_k`, so `d_k ≤ 2^17` keeps it exact in `i64`.
    if d_k > 1 << 17 || d_v > 1 << 17 {
        return Err(PalwQwen36OpError::BadK { k: d_k, experts: d_v });
    }
    let mut u = Vec::with_capacity(d_v);
    for (row, vi) in state.s.chunks_exact(d_k).zip(v) {
        let acc: i64 = row.iter().zip(k).map(|(a, b)| *a as i64 * *b as i64).sum();
        let w = state_scale(acc, params.read.multiplier, params.read.shift).saturating_add(params.read.zero);
        // 3. `u = beta * (v - w)`, in v's units and on the code grid.
        let delta = (*vi as i64).saturating_sub(w);
        let scaled = rounding_shift_right_64(delta.saturating_mul(beta_q), K as u8);
        u.push(scaled.clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32);
    }

    // 4. The rank-one write. `u · k` is at most `2^30`, and the multiplier is bounded above, so
    //    the product stays inside `i64`.
    for (row, ui) in state.s.chunks_exact_mut(d_k).zip(&u) {
        let ui = *ui as i64;
        if ui == 0 {
            continue;
        }
        for (slot, kj) in row.iter_mut().zip(k) {
            let write = state_scale(ui * *kj as i64, params.write.multiplier, params.write.shift).saturating_add(params.write.zero);
            *slot = (*slot as i64).saturating_add(write).clamp(-QWEN36_STATE_MAX, QWEN36_STATE_MAX) as i32;
        }
    }

    // 5. `o = S q`, narrowed to the output code scale.
    Ok(state
        .s
        .chunks_exact(d_k)
        .map(|row| {
            let acc: i64 = row.iter().zip(q).map(|(a, b)| *a as i64 * *b as i64).sum();
            state_scale(acc, params.out.multiplier, params.out.shift)
                .saturating_add(params.out.zero)
                .clamp(-A16_CODE_MAX, A16_CODE_MAX) as i32
        })
        .collect())
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

    /// `IntLn` against `f64::ln` across the whole domain a decay gate uses.
    ///
    /// The tolerance is absolute in Q[`K`] units and it is tight on purpose: the Newton step is
    /// what earns it, and a truncated series without it lands two orders worse. If this loosens,
    /// the refinement stopped working.
    #[test]
    fn the_logarithm_agrees_with_the_real_one() {
        let mut worst = 0i64;
        let mut worst_at = 0i64;
        for x in [1i64, 2, 100, ONE / 1000, ONE / 100, ONE / 7, ONE / 2, ONE - 1, ONE, ONE + 1, 2 * ONE, 100 * ONE, 1 << 40] {
            let got = q36_int_ln(x).expect("positive");
            let want = ((x as f64 / ONE as f64).ln() * ONE as f64) as i64;
            let error = (got - want).abs();
            if error > worst {
                worst = error;
                worst_at = x;
            }
        }
        // Sweep the (0, 1] band the decay gate actually lives in.
        for step in 1..=2000i64 {
            let x = step * ONE / 2000;
            let got = q36_int_ln(x).expect("positive");
            let want = ((x as f64 / ONE as f64).ln() * ONE as f64) as i64;
            let error = (got - want).abs();
            if error > worst {
                worst = error;
                worst_at = x;
            }
        }
        assert!(worst < ONE / 10_000, "IntLn is off by {worst} Q[K] units at x={worst_at} (ONE={ONE})");
        assert_eq!(q36_int_ln(0), None, "ln 0 is not a number and must not be given one");
        assert_eq!(q36_int_ln(-5), None);
        // ln 1 = 0 exactly, which the series gives without help.
        assert_eq!(q36_int_ln(ONE), Some(0));
    }

    /// The power the decay gate is. `c = 0` and `u = 1` are the identities and must be exact.
    #[test]
    fn the_power_matches_the_real_one() {
        assert_eq!(q36_pow_q(ONE, 3 * ONE), ONE, "1^c is 1");
        // The case that refused on a real checkpoint: a `u` just under one, where `c * ln u`
        // rounds to zero and the frozen `int_exp(0)` sits above `ONE`.
        for u in [ONE - 1, ONE - 2, ONE - 16] {
            let got = q36_pow_q(u, ONE);
            assert!((0..=ONE).contains(&got), "u^c must be a gate, got {got} for u={u}");
        }
        for c in [1i64, 2, ONE / 1000] {
            let got = q36_pow_q(ONE - 1, c);
            assert!((0..=ONE).contains(&got), "u^c must be a gate, got {got} for c={c}");
        }
        assert_eq!(q36_pow_q(ONE / 2, 0), ONE, "u^0 is 1");
        assert_eq!(q36_pow_q(0, ONE), 0);
        for (u, c) in [(ONE / 2, ONE), (ONE / 2, 2 * ONE), (ONE / 4, ONE / 2), (ONE * 9 / 10, 5 * ONE), (ONE / 100, ONE / 3)] {
            let got = q36_pow_q(u, c) as f64 / ONE as f64;
            let want = (u as f64 / ONE as f64).powf(c as f64 / ONE as f64);
            // **The tolerance is `int_exp`'s, not this op's.** Measured, the frozen exponential
            // is up to 3.0e-3 relative (worst near x = -0.53, where its quadratic is weakest).
            // Every op built on it inherits that, and it is class data — `int_exp` is in the KAT
            // set and its values are the class id — so this is recorded rather than tightened.
            assert!((got - want).abs() < 5e-3, "({u},{c}): got {got}, want {want}");
        }
    }

    /// `L2Norm` puts the vector on the unit sphere at Q15, which is what the delta rule's
    /// contraction argument rests on.
    #[test]
    fn l2_norm_lands_on_the_unit_sphere() {
        let mut rng = Lcg(0x36_0012);
        for n in [4usize, 64, 128] {
            let x: Vec<i32> = (0..n).map(|_| rng.code() / 4).collect();
            let unit = q36_l2_norm(&x).expect("well-formed");
            let norm2: i64 = unit.iter().map(|v| *v as i64 * *v as i64).sum();
            let one2 = (A16_CODE_MAX * A16_CODE_MAX) as f64;
            let ratio = norm2 as f64 / one2;
            // 2e-4 rather than the 2e-3 the first version needed: taking the exponent out of
            // `int_rsqrt` is what buys the order of magnitude.
            assert!((ratio - 1.0).abs() < 4e-4, "norm-squared = {ratio} at n={n}");
        }
        // A zero vector has no direction and is not given one.
        assert_eq!(q36_l2_norm(&[0, 0, 0]).expect("well-formed"), vec![0, 0, 0]);
    }

    /// **The feasibility question for the whole architecture: does the recurrence drift?**
    ///
    /// A float reference of the same delta rule is run alongside the integer one for a long
    /// sequence, and the relative error of the OUTPUT is measured at the end rather than at the
    /// start. If fixed-point error compounded, this is where it would show — and it does not,
    /// because the decay makes the recurrence a contraction and an error injected at step `t` is
    /// worth `decay^(T−t)` by step `T`.
    ///
    /// The comparison is statistical on purpose. The integer op is exact and reproducible; what
    /// is being measured is how far the class's arithmetic sits from the model it quantizes,
    /// which is a fidelity question and not a determinism one.
    ///
    /// **Measured, and this is the answer the architecture needed**: worst relative output error
    /// 9.1e-4 at 128 steps, 8.8e-4 at 512, 1.1e-3 at 2048. Flat in the sequence length — a
    /// sixteen-fold longer run costs 30 % more error, not sixteen times more. `MISAKA_GDN_STEPS`
    /// re-runs it at another length.
    #[test]
    fn the_recurrence_does_not_drift_over_a_long_sequence() {
        let (d_v, d_k) = (32usize, 32usize);
        let steps: usize = std::env::var("MISAKA_GDN_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(512);
        let code = A16_CODE_MAX as f64;
        // The state carries eight bits above the value scale — `β/(1−decay)` at the numbers below
        // is about 20, so eight bits is a comfortable margin and it is what a converter would pick.
        let state_bits = 8u8;
        let params = Qwen36GdnParamsV1 {
            // S·k is (code · 2^8) × Q15 → code: divide by 2^(15+8).
            read: A16QuantParams { multiplier: 1, shift: 15 + state_bits, zero: 0 },
            // u ⊗ k is code × Q15 → code · 2^8: divide by 2^(15−8).
            write: A16QuantParams { multiplier: 1, shift: 15 - state_bits, zero: 0 },
            out: A16QuantParams { multiplier: 1, shift: 15 + state_bits, zero: 0 },
        };

        let mut rng = Lcg(0x36_0DEF);
        let mut state = Qwen36GdnStateV1::zeros(d_v, d_k);
        let mut float_state = vec![0f64; d_v * d_k];
        let mut worst_relative = 0f64;

        for step in 0..steps {
            let raw: Vec<i32> = (0..d_k).map(|_| rng.code() / 4).collect();
            let k = q36_l2_norm(&raw).expect("well-formed");
            let raw_q: Vec<i32> = (0..d_k).map(|_| rng.code() / 4).collect();
            let q = q36_l2_norm(&raw_q).expect("well-formed");
            let v: Vec<i32> = (0..d_v).map(|_| rng.code() / 8).collect();
            // A decay and a beta in the range a trained gate produces.
            let decay_q = ONE * 95 / 100 + (rng.next_u64() % (ONE as u64 / 30)) as i64;
            let beta_q = ONE / 4 + (rng.next_u64() % (ONE as u64 / 2)) as i64;

            let out = q36_gdn_step(&mut state, &k, &v, &q, decay_q, beta_q, params).expect("well-formed");

            // The same recurrence in f64, on the same numbers.
            let kf: Vec<f64> = k.iter().map(|c| *c as f64 / code).collect();
            let qf: Vec<f64> = q.iter().map(|c| *c as f64 / code).collect();
            let vf: Vec<f64> = v.iter().map(|c| *c as f64 / code).collect();
            let (df, bf) = (decay_q as f64 / ONE as f64, beta_q as f64 / ONE as f64);
            for slot in float_state.iter_mut() {
                *slot *= df;
            }
            let mut uf = vec![0f64; d_v];
            for i in 0..d_v {
                let w: f64 = (0..d_k).map(|j| float_state[i * d_k + j] * kf[j]).sum();
                uf[i] = bf * (vf[i] - w);
            }
            for i in 0..d_v {
                for j in 0..d_k {
                    float_state[i * d_k + j] += uf[i] * kf[j];
                }
            }
            let of: Vec<f64> = (0..d_v).map(|i| (0..d_k).map(|j| float_state[i * d_k + j] * qf[j]).sum()).collect();

            // Compare only once the recurrence has filled: the first few steps are a state of
            // zeros against a state of zeros and would flatter the result.
            if step > steps / 2 {
                let scale: f64 = of.iter().map(|x| x.abs()).fold(0.0, f64::max).max(1e-9);
                for (got, want) in out.iter().zip(&of) {
                    let relative = ((*got as f64 / code) - want).abs() / scale;
                    worst_relative = worst_relative.max(relative);
                }
            }
        }
        eprintln!("MEASURED worst relative output error over {steps} steps: {worst_relative}");
        // Five times the measured worst (1.1e-3 at 2,048 steps). A threshold at the measurement
        // would fail on a different machine's rounding of the f64 REFERENCE; one at 5 % would not
        // notice the recurrence going unstable.
        assert!(worst_relative < 5e-3, "the recurrence drifted: worst relative output error {worst_relative}");
        // And the state is still inside its type with room, rather than riding the clamp.
        let peak = state.s.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
        assert!(peak > 0, "a recurrence that ends at zero measured nothing");
        assert!((peak as i64) < QWEN36_STATE_MAX / 2, "the state is riding its clamp: peak {peak}");
    }

    /// Determinism and the refusals, which the fidelity test above cannot see.
    #[test]
    fn the_recurrence_is_deterministic_and_total() {
        let (d_v, d_k) = (8usize, 8usize);
        let params = Qwen36GdnParamsV1 {
            read: A16QuantParams { multiplier: 1, shift: 23, zero: 0 },
            write: A16QuantParams { multiplier: 1, shift: 7, zero: 0 },
            out: A16QuantParams { multiplier: 1, shift: 23, zero: 0 },
        };
        let k = q36_l2_norm(&(0..d_k).map(|i| (i as i32 + 1) * 900).collect::<Vec<_>>()).expect("well-formed");
        let v: Vec<i32> = (0..d_v).map(|i| (i as i32 - 4) * 1000).collect();
        let q = k.clone();

        let run = || {
            let mut state = Qwen36GdnStateV1::zeros(d_v, d_k);
            let mut last = Vec::new();
            for _ in 0..16 {
                last = q36_gdn_step(&mut state, &k, &v, &q, ONE * 9 / 10, ONE / 2, params).expect("well-formed");
            }
            (state, last)
        };
        assert_eq!(run(), run(), "the same inputs must produce the same state and output");

        let mut state = Qwen36GdnStateV1::zeros(d_v, d_k);
        // A gate outside [0, ONE] is refused rather than clamped: it is not a gate.
        assert!(q36_gdn_step(&mut state, &k, &v, &q, ONE + 1, ONE / 2, params).is_err());
        assert!(q36_gdn_step(&mut state, &k, &v, &q, -1, ONE / 2, params).is_err());
        // A wrong width is refused.
        assert!(q36_gdn_step(&mut state, &k[..d_k - 1], &v, &q, ONE, ONE, params).is_err());
        // A multiplier past the i64 bound is refused, because the bound is the premise.
        let bad = Qwen36GdnParamsV1 { read: A16QuantParams { multiplier: QWEN36_STATE_MULT_MAX + 1, shift: 23, zero: 0 }, ..params };
        assert!(q36_gdn_step(&mut state, &k, &v, &q, ONE, ONE, bad).is_err());
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
