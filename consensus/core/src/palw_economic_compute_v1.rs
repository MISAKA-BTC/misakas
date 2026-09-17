//! **ADR-0131 — economic compute, in shadow: what a job COSTS, derived from the graph it runs.**
//!
//! A class is paid per `pwu_per_inference` (ADR-0124 Decision 6), the STEP-LEAF count of its
//! canonical job. A leaf is a tile of a node's committed output — `elements / tile_len` — which is a
//! court fact (what one dispute opens), not a cost: the dense tier tiles at 128 and the hybrid at
//! 512, a routed-expert row commits eight experts' worth of multiply–accumulates as one row, and a
//! softmax row over the kv length commits the same tile count as a matmul row a hundred times its
//! cost. Two classes with equal leaves can therefore run very different amounts of arithmetic, and
//! a price in leaves pays them the same.
//!
//! This module derives, from the SAME profile a class registers and nothing else, the
//! multiply–accumulate count of every node the step enumeration visits — the dense matmuls by their
//! input and output widths, the routed experts by the experts the row concatenates, attention by
//! heads × head dimension × the true kv length, the gated-delta recurrence by its state, the LM head
//! where logits are computed — and sums them over exactly the positions [`crate::palw_step::step_leaf_count`]
//! counts leaves over. The result is integer, deterministic and hardware-free: wall-clock time on a
//! reference host CALIBRATES the cost table and never enters it. **Nothing on the block path reads
//! this module** (ADR-0131 Decision 2): it is a measurement, published beside the leaf basis so the two
//! can be compared before any reward moves.
//!
//! Units: a **MAC-equivalent** — one 8-bit-weight × integer-activation multiply–accumulate, which
//! at the integer engine's batch of one is also one weight byte streamed, so the count is a proxy
//! for both arithmetic and memory traffic on the kernels every shipped class runs. Every other
//! operation is priced in that unit by [`PALW_ECONOMIC_COST_TABLE_V1`], and the table is versioned:
//! a change to any entry is a new version, never an edit.

use crate::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwStateParamsV2};
use crate::palw_step::{
    PALW_STEP_INPUT_CHECKPOINT_STATE, PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN,
    PALW_STEP_INPUT_SENTINEL_MIN, PalwLayerKindV1, PalwShapeProfileV3, PalwStepError, PalwStepNodeV1, PalwStepOpKindV1,
    PalwStepOutLenV1,
};
use crate::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// The cost table's version. A new table is a new version; an entry never moves in place.
pub const PALW_ECONOMIC_COMPUTE_VERSION_V1: u16 = 1;

/// **The cost of each operation the step graph can contain, in MAC-equivalents.**
///
/// Weights are per output element unless the field says otherwise. Chosen from the arithmetic each
/// kernel performs at the integer engine's batch of one, not from any host's timing:
///
/// * `matmul_mac` — one 8-bit weight × integer activation multiply–accumulate: the unit. A matmul
///   costs `input width × output width` of them (a routed expert row, the active experts' only).
/// * `attention_mac` — one multiply–accumulate over the cache: a query row against one cached key
///   (or one probability against one cached value). An attention site costs
///   `heads × head_dim × kv_len` of them for the scores and as many again for the values.
/// * `gdn_state_element` — per element of the recurrence state `k_dim × v_dim`, per head, per
///   position: the delta rule's decay, outer-product update and readout, four multiply–accumulates
///   per state element (the hybrid profile's own accounting: `n_ctx × 128 × 128 × 4` a head).
/// * `elementwise` — a multiply, an add, a scale, a copy, an embedding row read: one per element.
/// * `norm` — an RMS or L2 norm: a square-accumulate and a scale per element.
/// * `rope` — a rotation: two multiplies and an add over a pair, per element.
/// * `transcendental` — a softmax, sigmoid, SiLU or softplus element: an exponential through the
///   engine's fixed table, a sum and a normalisation.
/// * `glu` — a fused `silu(gate) · up`: a transcendental and a multiply.
/// * `conv_tap` — one tap of the causal convolution, per channel; a node costs
///   `channels × kernel taps` of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEconomicCostTableV1 {
    pub matmul_mac: u64,
    pub attention_mac: u64,
    pub gdn_state_element: u64,
    pub elementwise: u64,
    pub norm: u64,
    pub rope: u64,
    pub transcendental: u64,
    pub glu: u64,
    pub conv_tap: u64,
}

pub const PALW_ECONOMIC_COST_TABLE_V1: PalwEconomicCostTableV1 = PalwEconomicCostTableV1 {
    matmul_mac: 1,
    attention_mac: 1,
    gdn_state_element: 4,
    elementwise: 1,
    norm: 2,
    rope: 2,
    transcendental: 4,
    glu: 5,
    conv_tap: 1,
};

/// **What a weight element of this GGML dtype costs per multiply–accumulate**, relative to the 8-bit
/// unit: its byte width, because at the integer engine's batch of one a matmul streams every weight
/// element it touches once, so a 16-bit weight is two bytes of traffic per MAC and a 32-bit one four.
/// The block-quantised 2–6-bit types are priced at the unit: their vec-dot kernels dequantise per
/// block into the same 8-bit lane and read less, not more. Every class testnet-11 registers carries
/// 8-bit weights (`QWEN36_WEIGHT_DTYPE_I8` = 24 on every node), so this table separates no shipped
/// class from another; it exists so a class that registered wider weights is not priced as if it
/// had not.
pub fn palw_weight_dtype_cost_v1(dtype: u8) -> u64 {
    match dtype {
        // GGML_TYPE_F32
        0 => 4,
        // GGML_TYPE_F16, GGML_TYPE_BF16
        1 | 30 => 2,
        // GGML_TYPE_I16
        25 => 2,
        // GGML_TYPE_I32
        26 => 4,
        // Q4_0 … Q8_K, I8 and every other 8-bit-or-narrower block type
        _ => 1,
    }
}

/// A cost that is affine in the kv length: `constant + per_kv × kv_len`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwAffineCostV1 {
    pub constant: u128,
    pub per_kv: u128,
}

impl PalwAffineCostV1 {
    fn add(self, other: Self) -> Self {
        Self { constant: self.constant.saturating_add(other.constant), per_kv: self.per_kv.saturating_add(other.per_kv) }
    }

    fn scale(self, by: u128) -> Self {
        Self { constant: self.constant.saturating_mul(by), per_kv: self.per_kv.saturating_mul(by) }
    }

    /// The cost at one kv length.
    pub fn at(self, kv_len: u128) -> u128 {
        self.constant.saturating_add(self.per_kv.saturating_mul(kv_len))
    }

    /// `Σ_{L = from}^{to} (constant + per_kv × L)`, inclusive, zero for an empty range.
    fn sum_over(self, from: u128, to: u128) -> u128 {
        if from > to {
            return 0;
        }
        let count = to - from + 1;
        // Σ L over [from, to] = (from + to) × count / 2, exact because one factor is even.
        let (a, b) = (from.saturating_add(to), count);
        let sum_l = if a % 2 == 0 { (a / 2).saturating_mul(b) } else { a.saturating_mul(b / 2) };
        self.constant.saturating_mul(count).saturating_add(self.per_kv.saturating_mul(sum_l))
    }
}

/// **A job's compute, by the kind of work** — so a price gap between two classes can be read as
/// "the hybrid's routed experts" or "the dense tier's 512-wide prefill" rather than as a number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwEconomicBreakdownV1 {
    /// Dense weight matmuls: projections, the dense FFN, the router, the shared expert.
    pub dense_matmul: u128,
    /// The active experts' matmuls of a mixture layer, the concatenated row's only.
    pub routed_experts: u128,
    /// Scores and values over the cache (the fused site included).
    pub attention: u128,
    /// The gated-delta recurrence.
    pub recurrence: u128,
    /// The LM head, where logits are computed.
    pub logits: u128,
    /// Norms, rotations, transcendentals, the convolution, residuals, requantisation, the embedding.
    pub elementwise: u128,
}

impl PalwEconomicBreakdownV1 {
    pub fn total(&self) -> u128 {
        self.dense_matmul
            .saturating_add(self.routed_experts)
            .saturating_add(self.attention)
            .saturating_add(self.recurrence)
            .saturating_add(self.logits)
            .saturating_add(self.elementwise)
    }
}

/// The same six kinds, each affine in the kv length — a profile reduced to what one position costs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AffineBreakdown {
    dense_matmul: PalwAffineCostV1,
    routed_experts: PalwAffineCostV1,
    attention: PalwAffineCostV1,
    recurrence: PalwAffineCostV1,
    logits: PalwAffineCostV1,
    elementwise: PalwAffineCostV1,
}

impl AffineBreakdown {
    fn add(self, other: Self) -> Self {
        Self {
            dense_matmul: self.dense_matmul.add(other.dense_matmul),
            routed_experts: self.routed_experts.add(other.routed_experts),
            attention: self.attention.add(other.attention),
            recurrence: self.recurrence.add(other.recurrence),
            logits: self.logits.add(other.logits),
            elementwise: self.elementwise.add(other.elementwise),
        }
    }

    fn sum_over(self, from: u128, to: u128) -> PalwEconomicBreakdownV1 {
        PalwEconomicBreakdownV1 {
            dense_matmul: self.dense_matmul.sum_over(from, to),
            routed_experts: self.routed_experts.sum_over(from, to),
            attention: self.attention.sum_over(from, to),
            recurrence: self.recurrence.sum_over(from, to),
            logits: self.logits.sum_over(from, to),
            elementwise: self.elementwise.sum_over(from, to),
        }
    }
}

fn add_breakdown(a: PalwEconomicBreakdownV1, b: PalwEconomicBreakdownV1) -> PalwEconomicBreakdownV1 {
    PalwEconomicBreakdownV1 {
        dense_matmul: a.dense_matmul.saturating_add(b.dense_matmul),
        routed_experts: a.routed_experts.saturating_add(b.routed_experts),
        attention: a.attention.saturating_add(b.attention),
        recurrence: a.recurrence.saturating_add(b.recurrence),
        logits: a.logits.saturating_add(b.logits),
        elementwise: a.elementwise.saturating_add(b.elementwise),
    }
}

/// **A profile reduced to per-position costs**: the body (the pre table and every layer) and the
/// logits (the post table), each affine in the kv length. Derived once per profile; a job is then a
/// closed-form sum over its positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEconomicShapeV1 {
    body: AffineBreakdown,
    logits: AffineBreakdown,
}

impl PalwEconomicShapeV1 {
    /// One position of the body at `kv_len`, by kind.
    pub fn body_at(&self, kv_len: u128) -> PalwEconomicBreakdownV1 {
        self.body.sum_over(kv_len, kv_len)
    }

    /// The logits pass at `kv_len`, by kind.
    pub fn logits_at(&self, kv_len: u128) -> PalwEconomicBreakdownV1 {
        self.logits.sum_over(kv_len, kv_len)
    }
}

/// Which table a node lives in, for the LM-head rule.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Table {
    Pre,
    Layer,
    Post,
}

/// The width of a node's output, affine in the kv length.
fn out_width(node: &PalwStepNodeV1) -> PalwAffineCostV1 {
    match node.out_len {
        PalwStepOutLenV1::Fixed { elements } => PalwAffineCostV1 { constant: elements as u128, per_kv: 0 },
        PalwStepOutLenV1::KvScaled { multiplier } => PalwAffineCostV1 { constant: 0, per_kv: multiplier as u128 },
    }
}

/// The width of what a node reads through one input reference.
fn input_width(profile: &PalwShapeProfileV3, table: &[PalwStepNodeV1], r: u16) -> Result<PalwAffineCostV1, PalwStepError> {
    let fixed = |n: u128| PalwAffineCostV1 { constant: n, per_kv: 0 };
    if r < PALW_STEP_INPUT_SENTINEL_MIN {
        return table
            .get(r as usize)
            .map(out_width)
            .ok_or(PalwStepError::ProfileNotCanonical("a node reads a step outside its table"));
    }
    Ok(match r {
        PALW_STEP_INPUT_LAYER_IN => fixed(profile.hidden_dim as u128),
        PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V => {
            PalwAffineCostV1 { constant: 0, per_kv: (profile.attn_kv_heads as u128).saturating_mul(profile.attn_head_dim as u128) }
        }
        PALW_STEP_INPUT_CHECKPOINT_STATE => fixed(gdn_state_elements(profile)),
        _ => return Err(PalwStepError::ProfileNotCanonical("a node reads an input sentinel this module does not know")),
    })
}

fn gdn_state_elements(profile: &PalwShapeProfileV3) -> u128 {
    (profile.gdn_heads as u128).saturating_mul(profile.gdn_head_k_dim as u128).saturating_mul(profile.gdn_head_v_dim as u128)
}

/// **How many experts a mixture table routes a token to**, read off its router: the top-k softmax
/// node commits `2 × k` values (an index and a probability per chosen expert). `1` for a table
/// without a router — a dense FFN, or a one-expert mixture whose router is the identity.
fn routed_group_count(table: &[PalwStepNodeV1]) -> u128 {
    table
        .iter()
        .find(|node| node.op_kind == PalwStepOpKindV1::SoftMax && node.weight_name.contains("router"))
        .and_then(|node| match node.out_len {
            PalwStepOutLenV1::Fixed { elements } if elements >= 2 => Some((elements / 2) as u128),
            _ => None,
        })
        .unwrap_or(1)
}

/// **Is the row a node reads a concatenation of routed experts' rows?** Follows the node's first
/// input back through elementwise nodes to the matmul that produced the row; a routed-expert matmul
/// (`…_exps.routed`) makes it one, so a matmul consuming it is block-diagonal: each expert's slice is
/// read by that expert's weights only.
fn reads_routed_row(table: &[PalwStepNodeV1], index: usize) -> bool {
    let mut at = index;
    for _ in 0..table.len() {
        let Some(&r) = table[at].input_refs.first() else { return false };
        if r >= PALW_STEP_INPUT_SENTINEL_MIN || r as usize >= table.len() || r as usize >= at {
            return false;
        }
        at = r as usize;
        let node = &table[at];
        if matches!(node.op_kind, PalwStepOpKindV1::MatMulQuant | PalwStepOpKindV1::MatMulF16) {
            return node.weight_name.ends_with(".routed");
        }
    }
    false
}

/// The dtype weight of a node's weight operand for one layer of its table.
fn dtype_cost(node: &PalwStepNodeV1, layer_index: usize) -> u128 {
    node.weight_dtypes.get(layer_index).or(node.weight_dtypes.first()).map(|d| palw_weight_dtype_cost_v1(*d)).unwrap_or(1) as u128
}

/// One node's cost at every kv length, as an affine breakdown.
fn node_cost(
    profile: &PalwShapeProfileV3,
    table: &[PalwStepNodeV1],
    index: usize,
    which: Table,
    layer_index: usize,
    cost: &PalwEconomicCostTableV1,
) -> Result<AffineBreakdown, PalwStepError> {
    let node = &table[index];
    let out = out_width(node);
    let mut b = AffineBreakdown::default();
    let per_element = |weight: u64| out.scale(weight as u128);
    match node.op_kind {
        PalwStepOpKindV1::MatMulQuant | PalwStepOpKindV1::MatMulF16 => {
            let over_cache = node.input_refs.iter().any(|&r| r == PALW_STEP_INPUT_KV_K || r == PALW_STEP_INPUT_KV_V);
            if over_cache {
                // Scores: `heads × kv_len` outputs, a `head_dim` dot each. Values: `heads × head_dim`
                // outputs, a `kv_len` dot each. Both are `heads × head_dim × kv_len`.
                let per_kv = (profile.attn_heads as u128)
                    .saturating_mul(profile.attn_head_dim as u128)
                    .saturating_mul(cost.attention_mac as u128);
                b.attention = PalwAffineCostV1 { constant: 0, per_kv };
                return Ok(b);
            }
            let Some(&first) = node.input_refs.first() else {
                return Err(PalwStepError::ProfileNotCanonical("a weight matmul reads no input"));
            };
            let input = input_width(profile, table, first)?;
            if input.per_kv != 0 && out.per_kv != 0 {
                return Err(PalwStepError::ProfileNotCanonical("a matmul quadratic in the kv length has no cost here"));
            }
            let routed = node.weight_name.ends_with(".routed");
            // A matmul over a concatenated routed row reads each expert's slice with that expert's
            // weights only: block-diagonal, one group's worth per group.
            let groups = if routed && reads_routed_row(table, index) { routed_group_count(table) } else { 1 };
            let dtype = dtype_cost(node, layer_index);
            let macs = PalwAffineCostV1 {
                constant: input.constant.saturating_mul(out.constant),
                per_kv: input.per_kv.saturating_mul(out.constant).saturating_add(input.constant.saturating_mul(out.per_kv)),
            }
            .scale(dtype.saturating_mul(cost.matmul_mac as u128));
            let macs = PalwAffineCostV1 { constant: macs.constant / groups.max(1), per_kv: macs.per_kv / groups.max(1) };
            if routed {
                b.routed_experts = macs;
            } else if which == Table::Post {
                b.logits = macs;
            } else {
                b.dense_matmul = macs;
            }
        }
        PalwStepOpKindV1::AttnFused => {
            // Scores and values over the cache, the softmax row and its requantisation, per head.
            let heads = profile.attn_heads as u128;
            let per_kv = heads.saturating_mul(
                (2u128)
                    .saturating_mul(profile.attn_head_dim as u128)
                    .saturating_mul(cost.attention_mac as u128)
                    .saturating_add(cost.transcendental as u128)
                    .saturating_add(cost.elementwise as u128),
            );
            b.attention = PalwAffineCostV1 { constant: 0, per_kv };
        }
        PalwStepOpKindV1::GatedDeltaNet => {
            b.recurrence =
                PalwAffineCostV1 { constant: gdn_state_elements(profile).saturating_mul(cost.gdn_state_element as u128), per_kv: 0 };
        }
        PalwStepOpKindV1::SsmConv => {
            b.elementwise = per_element((profile.gdn_conv_kernel.max(1) as u64).saturating_mul(cost.conv_tap));
        }
        PalwStepOpKindV1::RmsNorm | PalwStepOpKindV1::L2Norm => b.elementwise = per_element(cost.norm),
        PalwStepOpKindV1::RopeImrope => b.elementwise = per_element(cost.rope),
        PalwStepOpKindV1::SoftMax | PalwStepOpKindV1::Sigmoid | PalwStepOpKindV1::Softplus | PalwStepOpKindV1::Silu => {
            b.elementwise = per_element(cost.transcendental)
        }
        PalwStepOpKindV1::Glu => b.elementwise = per_element(cost.glu),
        PalwStepOpKindV1::MulElem | PalwStepOpKindV1::AddElem | PalwStepOpKindV1::Scale | PalwStepOpKindV1::CpyF32F16 => {
            // A combine that folds a wide row into a narrow one works the wide row: the widest of
            // what the node reads and what it writes.
            let mut widest = out;
            for &r in &node.input_refs {
                if r < PALW_STEP_INPUT_SENTINEL_MIN {
                    let w = input_width(profile, table, r)?;
                    if w.constant > widest.constant || w.per_kv > widest.per_kv {
                        widest = PalwAffineCostV1 { constant: w.constant.max(widest.constant), per_kv: w.per_kv.max(widest.per_kv) };
                    }
                }
            }
            b.elementwise = widest.scale(cost.elementwise as u128);
        }
        PalwStepOpKindV1::EmbedLookup => b.elementwise = per_element(cost.elementwise),
    }
    Ok(b)
}

fn table_cost(
    profile: &PalwShapeProfileV3,
    table: &[PalwStepNodeV1],
    which: Table,
    layer_index: usize,
    cost: &PalwEconomicCostTableV1,
) -> Result<AffineBreakdown, PalwStepError> {
    let mut sum = AffineBreakdown::default();
    for index in 0..table.len() {
        sum = sum.add(node_cost(profile, table, index, which, layer_index, cost)?);
    }
    Ok(sum)
}

/// **The profile's per-position costs** — the body (pre table, then every layer through the table
/// its kind selects, each layer at its own weight dtype) and the logits (post table).
pub fn palw_economic_shape_v1(
    profile: &PalwShapeProfileV3,
    cost: &PalwEconomicCostTableV1,
) -> Result<PalwEconomicShapeV1, PalwStepError> {
    profile.validate_shape()?;
    let mut body = table_cost(profile, &profile.pre_nodes, Table::Pre, 0, cost)?;
    let (mut gdn_seen, mut attn_seen) = (0usize, 0usize);
    for layer in 0..profile.layer_count {
        let layer_cost = match profile.layer_kind(layer) {
            PalwLayerKindV1::Attention => {
                let c = table_cost(profile, &profile.attn_nodes, Table::Layer, attn_seen, cost)?;
                attn_seen += 1;
                c
            }
            PalwLayerKindV1::GatedDeltaNet => {
                let c = table_cost(profile, &profile.gdn_nodes, Table::Layer, gdn_seen, cost)?;
                gdn_seen += 1;
                c
            }
        };
        body = body.add(layer_cost);
    }
    let logits = table_cost(profile, &profile.post_nodes, Table::Post, 0, cost)?;
    Ok(PalwEconomicShapeV1 { body, logits })
}

/// **A job's economic compute, by kind** — over exactly the positions the leaf enumeration visits
/// ([`crate::palw_step::step_leaf_count_capped_v1`]'s closed form): prefill position `p` runs the
/// body at kv length `p` for `p = 1..P`; the last prefill position adds the logits at `P`; decode
/// call `c = 1..D` (`D = exact_decode_tokens − 1`) runs the body and the logits at `P + c`.
pub fn palw_job_economic_breakdown_v1(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    cost: &PalwEconomicCostTableV1,
) -> Result<PalwEconomicBreakdownV1, PalwStepError> {
    let shape = palw_economic_shape_v1(profile, cost)?;
    Ok(palw_job_breakdown_from_shape_v1(&shape, context.declared_prefill_tokens, context.exact_decode_tokens))
}

/// [`palw_job_economic_breakdown_v1`] from a shape already derived.
pub fn palw_job_breakdown_from_shape_v1(shape: &PalwEconomicShapeV1, prefill: u32, exact_decode: u32) -> PalwEconomicBreakdownV1 {
    let prefill = prefill as u128;
    let decode_calls = exact_decode.saturating_sub(1) as u128;
    let mut total = shape.body.sum_over(1, prefill);
    if prefill >= 1 {
        total = add_breakdown(total, shape.logits.sum_over(prefill, prefill));
    }
    total = add_breakdown(total, shape.body.sum_over(prefill + 1, prefill + decode_calls));
    total = add_breakdown(total, shape.logits.sum_over(prefill + 1, prefill + decode_calls));
    total
}

/// A job's economic compute, total.
pub fn palw_job_economic_compute_v1(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    cost: &PalwEconomicCostTableV1,
) -> Result<u128, PalwStepError> {
    Ok(palw_job_economic_breakdown_v1(profile, context, cost)?.total())
}

/// **The compute an attempt of the class actually runs**: its canonical job under ADR-0117's rule
/// at the attempt's height (`palw_attempt_job_v1` — past `palw_prefill_draw` the prefill and one
/// generated token, no decode calls). Since ADR-0072 a draw IS an execution, so this is one draw's
/// cost, and a claim's producer ran [`palw_attempted_compute_per_claim_v1`] of it in expectation.
pub fn palw_attempt_economic_compute_v1(
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    prefill_draw: bool,
    cost: &PalwEconomicCostTableV1,
) -> Result<u128, PalwStepError> {
    let job = crate::palw_attempt_v2::palw_attempt_job_v1(canonical.clone(), prefill_draw);
    palw_job_economic_compute_v1(profile, &job, cost)
}

/// The compute a claim cost its producer in expectation: `expected_attempts × one draw`, each draw
/// an execution of the attempt's job. Saturating.
pub fn palw_attempted_compute_per_claim_v1(expected_attempts: u64, draw_compute: u128) -> u128 {
    draw_compute.saturating_mul(expected_attempts.max(1) as u128)
}

/// One draw, in the Q32 fixed point of [`palw_expected_attempts_q32_v1`].
pub const PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1: u128 = 1u128 << 32;

/// **The draws a claim costs in expectation at a target, in Q32 fixed point**: `⌊2¹⁶⁰ / (target + 1)⌋`.
/// Its integer part is [`crate::palw_pwu::palw_expected_attempts_v1`] exactly; the fraction is what
/// that consensus factor floors away. The fork-choice factor is an integer by design (`claim.pwu` is
/// work in whole inferences), but a price of attempts cannot floor it: a class at 0.67 × MAX draws
/// 1.5 forwards a claim in expectation and would be paid for one. Saturates at `u128::MAX` for
/// targets under 2³² (some 2⁹⁶ draws a claim — no class mines there). Deterministic integer
/// arithmetic; nothing on the block path reads it.
pub fn palw_expected_attempts_q32_v1(class_target: u128) -> u128 {
    if class_target == u128::MAX {
        return PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
    }
    let d = class_target + 1;
    // ⌊2¹²⁸ / d⌋ and 2¹²⁸ mod d, from `2¹²⁸ − d = u128::MAX − target` as `palw_expected_attempts_v1`
    // derives its quotient.
    let q = (u128::MAX - class_target) / d + 1;
    let r = (u128::MAX - class_target) % d;
    if q >= 1u128 << 96 {
        return u128::MAX;
    }
    // ⌊r · 2³² / d⌋ by shift-subtract; the remainder stays below d ≤ 2¹²⁸, so a carry out of the
    // shift means the doubled remainder is at least d.
    let (mut rem, mut frac) = (r, 0u128);
    for _ in 0..32 {
        let carry = rem >> 127;
        rem <<= 1;
        frac <<= 1;
        if carry == 1 || rem >= d {
            rem = rem.wrapping_sub(d);
            frac |= 1;
        }
    }
    (q << 32) | frac
}

/// **The network draws a class win costs in expectation, in Q32** (ADR-0132 §1.1–1.2): a forward
/// that won its class ticket still faces the Layer-0 digest against the header's `bits`, and a lost
/// network draw is a lost inference (ADR-0072: nothing is searched). `⌊2²⁵⁶ · 2³² / (target₂₅₆ + 1)⌋`
/// from the compact `bits`, the target the difficulty lift compares against (`target₅₁₂ = target₂₅₆
/// ≪ 256`, so the probability is the 256-bit one). At the difficulty floor `0x207fffff` this is 2.0:
/// half of every class's winning forwards are discarded even on a starved chain. One `bits` prices
/// every class at a moment, so this factor never moves a cross-model ratio; it multiplies every
/// class's attempted compute. Saturates at `u128::MAX` for a target under 2⁹⁶ draws a win (mantissa
/// zero, or a negative compact mantissa, decodes to a zero target). Nothing on the block path reads it.
pub fn palw_network_expected_attempts_q32_v1(bits: u32) -> u128 {
    use kaspa_math::Uint256;
    let target = Uint256::from_compact_target_bits(bits);
    if target == Uint256::MAX {
        return PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
    }
    if target.is_zero() {
        return u128::MAX;
    }
    let one = Uint256::from_u64(1);
    let d = target + one;
    // ⌊2²⁵⁶ / d⌋ and 2²⁵⁶ mod d from `2²⁵⁶ − d = MAX − target`, as the 128-bit reading does.
    let (q, r) = (Uint256::MAX - target).div_rem(d);
    let q = q + one;
    if q.bits() > 96 {
        return u128::MAX;
    }
    let (mut rem, mut frac) = (r, 0u128);
    for _ in 0..32 {
        let (shifted, carry) = rem.overflowing_shl(1);
        rem = shifted;
        frac <<= 1;
        if carry || rem >= d {
            rem = rem.overflowing_sub(d).0;
            frac |= 1;
        }
    }
    (q.as_u128() << 32) | frac
}

/// [`palw_attempted_compute_per_claim_v1`] with the draws in Q32: `⌊expected_attempts_q32 × draw / 2³²⌋`,
/// never below one draw. Saturating.
pub fn palw_attempted_compute_q32_per_claim_v1(expected_attempts_q32: u128, draw_compute: u128) -> u128 {
    let ea = expected_attempts_q32.max(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1);
    let whole = (ea >> 32).saturating_mul(draw_compute);
    let frac = (ea & (PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 - 1)).saturating_mul(draw_compute) >> 32;
    whole.saturating_add(frac)
}

/// **A price of work in `u128` measures** — [`crate::palw_panel_economy_v1::palw_work_priced_reward_v1`]'s
/// rule over any compute measure: the escrow whole at or above the unit (or under a unit of zero),
/// proportionally less below it, never more than the escrow.
pub fn palw_priced_reward_u128_v1(escrow: u64, measure: u128, unit: u128) -> u64 {
    if unit == 0 || measure >= unit {
        return escrow;
    }
    // `escrow × measure / unit` with the product kept inside u128: `measure < unit`, so the quotient
    // is below the escrow; a measure past 2⁶³ (a job of nine quintillion MAC-equivalents, which no
    // profile reaches) is scaled down with its unit, keeping 63 bits of the ratio.
    let (mut measure, mut unit) = (measure, unit);
    while measure >= 1u128 << 63 {
        measure >>= 1;
        unit >>= 1;
    }
    ((escrow as u128).saturating_mul(measure) / unit.max(1)) as u64
}

/// **The bases a claim's pay can be read in, for the shadow comparison** (ADR-0131 Decision 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRewardBasisV1 {
    /// Today's rule: the class's static `pwu_per_inference`, the canonical job's leaves.
    CurrentLeaves,
    /// The economic compute of the job an attempt runs, once.
    EconomicJob,
    /// The economic compute a claim cost in expectation: `class draws × network draws × the draw's
    /// compute` (ADR-0132: both lotteries, both from chain facts).
    EconomicAttempted,
}

/// What one class measures under every basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassMeasureV1 {
    /// The class's `pwu_per_inference`: its canonical job's leaves.
    pub leaves: u64,
    /// The economic compute of the job an attempt runs (ADR-0117's rule at the height).
    pub draw_compute: u128,
    /// `palw_expected_attempts_q32_v1` of the class target: the draws a claim costs in expectation,
    /// fraction included — not the integer fork-choice factor.
    pub expected_attempts_q32: u128,
    /// `palw_network_expected_attempts_q32_v1` of the header's `bits`: the network draws each class
    /// win costs in expectation (ADR-0132). One value for every class at a moment.
    pub network_expected_attempts_q32: u128,
    /// Whether the class is a weight-bearing model class — the only classes that can set the unit.
    pub sets_unit: bool,
    /// Whether the class is priced at all: the liveness floor is not (ADR-0124 Decision 6, "the
    /// floor is not a model") and is paid its escrow whole on every basis.
    pub priced: bool,
}

impl PalwClassMeasureV1 {
    pub fn measure(&self, basis: PalwRewardBasisV1) -> u128 {
        match basis {
            PalwRewardBasisV1::CurrentLeaves => self.leaves as u128,
            PalwRewardBasisV1::EconomicJob => self.draw_compute,
            PalwRewardBasisV1::EconomicAttempted => palw_attempted_compute_q32_per_claim_v1(
                self.network_expected_attempts_q32,
                palw_attempted_compute_q32_per_claim_v1(self.expected_attempts_q32, self.draw_compute),
            ),
        }
    }
}

/// **The unit a basis prices against**: the largest measure among the classes that set it —
/// ADR-0124 Decision 6's rule, read in the basis's own measure. Zero when no class sets it (every
/// claim is then paid whole).
pub fn palw_basis_unit_v1(basis: PalwRewardBasisV1, classes: &[PalwClassMeasureV1]) -> u128 {
    classes.iter().filter(|c| c.sets_unit).map(|c| c.measure(basis)).max().unwrap_or(0)
}

/// **What the chain holds for one class at the sink** (ADR-0131 Decision 1): its registration's
/// numbers, its target, and a census of the attempt-lane claims still in the state — every phase,
/// the redraws, and the escrows. Retired claims are gone from the state and from this count, so a
/// window is at most the retention the network keeps; the compute the class's job costs is not here
/// (the state holds no profile), the node adds it from the registration's carriage or its build.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwClassCensusV1 {
    pub class_id: Hash64,
    pub is_base_class: bool,
    pub status: String,
    pub share_permille: Option<u16>,
    /// The class's `pwu_per_inference`: its canonical job's leaves, the price basis today.
    pub pwu_per_inference: u64,
    pub class_target: u128,
    /// `palw_expected_attempts_v1` of the target — the draws a claim costs in expectation, as the
    /// fork-choice factor floors them — and `palw_expected_attempts_q32_v1`, the same with its fraction.
    pub expected_attempts: u64,
    pub expected_attempts_q32: u128,
    /// Attempt-lane claims in the state, whatever their phase.
    pub claims_accepted: u64,
    pub claims_provisional: u64,
    pub claims_panel_bound: u64,
    pub claims_licensed: u64,
    pub claims_final: u64,
    pub claims_voided: u64,
    /// Claims whose first panel timed out and were drawn a second one.
    pub claims_redrawn: u64,
    /// Σ `escrowed_reward` over every accepted claim, and over the `Final` ones.
    pub escrow_accepted_sompi: u128,
    pub escrow_final_sompi: u128,
    /// ADR-0133: the panel's capacity facts — `Active` bonds that may judge the class (the draw's
    /// own rule, capability by declaration), the seats on duty over the class's bound claims, the
    /// seat exposure those duties hold, and the free collateral the eligible bonds have left after
    /// every duty and reservation they hold.
    pub eligible_seats: u32,
    pub duty_seats_inflight: u64,
    pub seat_exposure_inflight_sompi: u128,
    pub free_collateral_sompi: u128,
}

/// The census of every class, with the DAA the state was read at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwClassCensusReadV1 {
    pub tip_daa: u64,
    /// The tip's compact `bits` and the network draws a class win costs at it (ADR-0132).
    pub network_bits: u32,
    pub network_expected_attempts_q32: u128,
    pub classes: Vec<PalwClassCensusV1>,
}

/// **The census, from the state** — one pass over the classes and one over the claims.
pub fn palw_class_census_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, network_bits: u32) -> PalwClassCensusReadV1 {
    use std::collections::BTreeMap;
    let mut rows: BTreeMap<Hash64, PalwClassCensusV1> = state
        .classes_iter()
        .map(|(id, record)| {
            let class_target = state.class_target(id).map(|t| t.target).unwrap_or(0);
            (
                *id,
                PalwClassCensusV1 {
                    class_id: *id,
                    is_base_class: *id == params.base_class_id(),
                    status: format!("{:?}", record.status),
                    share_permille: state.class_share_permille(id),
                    pwu_per_inference: record.pwu_rule.canonical_leaves_v1(),
                    class_target,
                    expected_attempts: if class_target == 0 { 0 } else { crate::palw_pwu::palw_expected_attempts_v1(class_target) },
                    expected_attempts_q32: if class_target == 0 { 0 } else { palw_expected_attempts_q32_v1(class_target) },
                    ..Default::default()
                },
            )
        })
        .collect();
    // ADR-0133: what every bond holds — its seat exposure over the duty rows, and its producer
    // reservations over its live claims — for the free collateral of a class's eligible seats.
    let seat_exposure = crate::palw_panel_economy_v1::palw_seat_exposure_ledger_v1(state);
    let mut producer_reserved: std::collections::BTreeMap<crate::palw_state_v2::PalwBondKeyV2, u128> =
        std::collections::BTreeMap::new();
    for (_, claim) in state.claims_iter() {
        if !matches!(claim.phase, PalwClaimPhaseV2::Final { .. } | PalwClaimPhaseV2::Voided { .. }) {
            let held = producer_reserved.entry(claim.bond).or_insert(0);
            *held = held.saturating_add(claim.reserved);
        }
    }
    for (class_id, row) in rows.iter_mut() {
        for (key, bond) in state.bonds_iter() {
            if !matches!(bond.status, crate::palw_state_v2::PalwBondStatusV2::Active)
                || !crate::palw_state_v2::palw_bond_may_judge_class_v2(bond, class_id)
            {
                continue;
            }
            row.eligible_seats += 1;
            let held = seat_exposure.get(key).copied().unwrap_or(0).saturating_add(producer_reserved.get(key).copied().unwrap_or(0));
            let free = (bond.collateral as u128).saturating_sub(bond.slashed as u128).saturating_sub(held);
            row.free_collateral_sompi = row.free_collateral_sompi.saturating_add(free);
        }
    }
    for (claim_id, duty) in state.panel_duty_rows_iter() {
        let Some(claim) = state.claim(claim_id) else { continue };
        let Some(row) = rows.get_mut(&claim.class_id) else { continue };
        row.duty_seats_inflight += duty.seats.len() as u64;
        row.seat_exposure_inflight_sompi =
            row.seat_exposure_inflight_sompi.saturating_add(duty.seat_exposure.saturating_mul(duty.seats.len() as u128));
    }
    for (_, claim) in state.claims_iter() {
        if !matches!(claim.source, PalwClaimSourceV2::Attempt) {
            continue;
        }
        let Some(row) = rows.get_mut(&claim.class_id) else { continue };
        row.claims_accepted += 1;
        row.escrow_accepted_sompi = row.escrow_accepted_sompi.saturating_add(claim.escrowed_reward as u128);
        if claim.rebound_daa.is_some() {
            row.claims_redrawn += 1;
        }
        match claim.phase {
            PalwClaimPhaseV2::Provisional => row.claims_provisional += 1,
            PalwClaimPhaseV2::PanelBound { .. } => row.claims_panel_bound += 1,
            PalwClaimPhaseV2::ReceiptLicensed { .. } => row.claims_licensed += 1,
            PalwClaimPhaseV2::Final { .. } => {
                row.claims_final += 1;
                row.escrow_final_sompi = row.escrow_final_sompi.saturating_add(claim.escrowed_reward as u128);
            }
            PalwClaimPhaseV2::Voided { .. } => row.claims_voided += 1,
            _ => {}
        }
    }
    PalwClassCensusReadV1 {
        tip_daa: state.last_point().map(|p| p.daa_score).unwrap_or(0),
        network_bits,
        network_expected_attempts_q32: if network_bits == 0 {
            PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1
        } else {
            palw_network_expected_attempts_q32_v1(network_bits)
        },
        classes: rows.into_values().collect(),
    }
}

/// **What one class did over a window, as the shadow reads it.** Claims are the attempt lane's;
/// every count is the chain's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassEconomicsInputV1 {
    pub measure: PalwClassMeasureV1,
    /// Σ escrow over the class's `Final` claims in the window — what the schedule withheld for them.
    pub escrow_final_sompi: u128,
    /// Claims the chain accepted (each a draw that won) and, of them, the claims that reached `Final`.
    pub claims_accepted: u64,
    pub claims_final: u64,
    /// Seats that replay the attempt's job to license it (the panel's size).
    pub seats_replaying: u64,
}

/// **One class's economics on one basis**, in sompi and MAC-equivalents. `producer + panel_pool +
/// burned == escrow_final_sompi` on every basis: a basis moves the burned share, never the withheld
/// amount, which is what "no new issuance" means here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClassEconomicsV1 {
    /// The unit the basis priced against.
    pub unit: u128,
    /// The class's price on the basis, in permille of its escrow.
    pub price_permille: u32,
    /// One `Final` claim's priced reward (the window's average), and its producer's and its panel's
    /// shares of it.
    pub reward_per_final_sompi: u64,
    pub producer_per_final_sompi: u64,
    pub panel_pool_per_final_sompi: u64,
    /// Over the window.
    pub producer_paid_sompi: u128,
    pub panel_pool_sompi: u128,
    pub burned_sompi: u128,
    /// The compute the `Final` claims certified (one attempt's job each), the compute every accepted
    /// claim cost its producer in expectation (`expected_attempts` draws each, voided claims included),
    /// and the compute the panels spent replaying the accepted claims.
    pub final_compute: u128,
    pub attempted_compute: u128,
    pub panel_compute: u128,
}

/// Sompi per 10⁹ MAC-equivalents — the ratio the comparison reads, kept integer.
pub const PALW_ECONOMICS_RATE_SCALE_V1: u128 = 1_000_000_000;

impl PalwClassEconomicsV1 {
    fn rate(paid: u128, compute: u128) -> u128 {
        if compute == 0 { 0 } else { paid.saturating_mul(PALW_ECONOMICS_RATE_SCALE_V1) / compute }
    }

    /// `F_m` for the producer: sompi per 10⁹ units of `Final` compute.
    pub fn producer_per_final_compute(&self) -> u128 {
        Self::rate(self.producer_paid_sompi, self.final_compute)
    }

    /// `F_m` for the panel pool.
    pub fn panel_per_final_compute(&self) -> u128 {
        Self::rate(self.panel_pool_sompi, self.final_compute)
    }

    /// `F_m` for the whole priced reward.
    pub fn total_per_final_compute(&self) -> u128 {
        Self::rate(self.producer_paid_sompi.saturating_add(self.panel_pool_sompi), self.final_compute)
    }

    /// `A_m` for the producer: sompi per 10⁹ units of attempted compute.
    pub fn producer_per_attempted_compute(&self) -> u128 {
        Self::rate(self.producer_paid_sompi, self.attempted_compute)
    }

    pub fn panel_per_attempted_compute(&self) -> u128 {
        Self::rate(self.panel_pool_sompi, self.attempted_compute)
    }

    pub fn total_per_attempted_compute(&self) -> u128 {
        Self::rate(self.producer_paid_sompi.saturating_add(self.panel_pool_sompi), self.attempted_compute)
    }

    /// The panel pool per unit of the compute the panels spent replaying: what a seat's work is paid.
    pub fn panel_per_verification_compute(&self) -> u128 {
        Self::rate(self.panel_pool_sompi, self.panel_compute)
    }
}

/// **One class's economics on `basis` against `unit`** — the price ADR-0124 Decision 6 applies
/// (`palw_priced_reward_u128_v1`), the 80 / 20 split (`palw_panel_split_v1`, the pool whole; which
/// seats were credited is a panel-liveness fact the state does not retain past `Final`), and the
/// compute on both sides.
pub fn palw_class_economics_v1(basis: PalwRewardBasisV1, input: &PalwClassEconomicsInputV1, unit: u128) -> PalwClassEconomicsV1 {
    let measure = input.measure.measure(basis);
    let unit = if input.measure.priced { unit } else { 0 };
    let price_permille = if unit == 0 || measure >= unit { 1000 } else { (measure.saturating_mul(1000) / unit) as u32 };
    // The price is linear in the escrow, so pricing the window's total is pricing each claim.
    let reward_total = palw_priced_total_v1(input.escrow_final_sompi, measure, unit);
    let pool_total = reward_total.saturating_mul(crate::palw_panel_economy_v1::PALW_PANEL_POOL_PERMILLE_V1 as u128) / 1000;
    let producer_total = reward_total - pool_total;
    let finals = input.claims_final as u128;
    let per_final = |total: u128| if finals == 0 { 0 } else { (total / finals).min(u64::MAX as u128) as u64 };
    let draw = input.measure.draw_compute;
    PalwClassEconomicsV1 {
        unit,
        price_permille,
        reward_per_final_sompi: per_final(reward_total),
        producer_per_final_sompi: per_final(producer_total),
        panel_pool_per_final_sompi: per_final(pool_total),
        producer_paid_sompi: producer_total,
        panel_pool_sompi: pool_total,
        burned_sompi: input.escrow_final_sompi - reward_total,
        final_compute: draw.saturating_mul(finals),
        attempted_compute: palw_attempted_compute_q32_per_claim_v1(input.measure.expected_attempts_q32, draw)
            .saturating_mul(input.claims_accepted as u128),
        panel_compute: draw.saturating_mul(input.seats_replaying as u128).saturating_mul(input.claims_accepted as u128),
    }
}

/// [`palw_priced_reward_u128_v1`] over a window's escrow total (a `u128`): the same rule, the same
/// scale-down past 2⁶³ on either side.
pub fn palw_priced_total_v1(escrow_total: u128, measure: u128, unit: u128) -> u128 {
    if unit == 0 || measure >= unit {
        return escrow_total;
    }
    let (mut escrow, mut measure, mut unit) = (escrow_total, measure, unit);
    while measure >= 1u128 << 63 {
        measure >>= 1;
        unit >>= 1;
    }
    let mut scale = 0u32;
    while escrow >= 1u128 << 64 {
        escrow >>= 1;
        scale += 1;
    }
    (escrow.saturating_mul(measure) / unit.max(1)) << scale
}

/// `max(a, b) / min(a, b) − 1`, in permille, saturating; `None` when either is zero (a class that
/// was paid nothing per unit has no ratio, and the comparison must say so rather than print 0).
pub fn palw_gap_permille_v1(a: u128, b: u128) -> Option<u128> {
    if a == 0 || b == 0 {
        return None;
    }
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    Some(hi.saturating_mul(1000) / lo - 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
    use crate::palw_context_ladder::palw_a16_context_row_profile_v5;
    use crate::palw_qwen25_profile::{QWEN25_A16_GRAPH_V5_N_CTX, qwen25_a16_graph_v5_canonical_v1};
    use crate::palw_qwen36_profile::{
        QWEN36_35B_A3B, QWEN36_RC_CANONICAL, QWEN38_27B, qwen36_geometry_artifact_eps, qwen36_profile_v2,
    };
    use crate::palw_step::step_leaf_count_capped_v1;

    const T: &PalwEconomicCostTableV1 = &PALW_ECONOMIC_COST_TABLE_V1;

    /// The four classes testnet-11 registers, built exactly as the chain registered them, with the
    /// first sixteen hex of the class ids the live node reports (`getPalwClasses`, 2026-09-17).
    fn live_classes() -> Vec<(&'static str, &'static str, PalwShapeProfileV3, PalwJobContextV2)> {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        let floor_job = rc_job_context(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let qwen36 = qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN36_35B_A3B)).unwrap();
        let qwen36_job = rc_job_context(&qwen36, QWEN36_RC_CANONICAL.0, QWEN36_RC_CANONICAL.1);
        let qwen25 = palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).unwrap();
        let (p, d) = qwen25_a16_graph_v5_canonical_v1();
        let qwen25_job = rc_job_context(&qwen25, p, d);
        let qwen38 = qwen36_profile_v2(qwen36_geometry_artifact_eps(QWEN38_27B)).unwrap();
        let qwen38_job = rc_job_context(&qwen38, QWEN36_RC_CANONICAL.0, QWEN36_RC_CANONICAL.1);
        vec![
            ("PALW-BASE-0/rc", "f1c5635c6e47e96e", floor, floor_job),
            ("Qwen3.6-35B-A3B/graph-v3", "5bd9ae3d91df8065", qwen36, qwen36_job),
            ("Qwen2.5-1.5B/graph-v5@512", "4277d84f7d91528c", qwen25, qwen25_job),
            ("Qwen3.8-27B/graph-v3", "2705b8f65f7ba54a", qwen38, qwen38_job),
        ]
    }

    /// The loop the closed form replaces: one position at a time, at its true kv length.
    fn job_by_loop(shape: &PalwEconomicShapeV1, prefill: u32, exact_decode: u32) -> u128 {
        let mut total = 0u128;
        for p in 1..=prefill as u128 {
            total += shape.body_at(p).total();
        }
        if prefill >= 1 {
            total += shape.logits_at(prefill as u128).total();
        }
        for c in 1..=(exact_decode.saturating_sub(1) as u128) {
            total += shape.body_at(prefill as u128 + c).total() + shape.logits_at(prefill as u128 + c).total();
        }
        total
    }

    /// **The closed form is the loop**, on every live class and a sweep of job shapes, including
    /// the empty job and a prefill of one.
    #[test]
    fn adr0131_the_closed_form_is_the_position_loop() {
        for (name, _, profile, canonical) in live_classes() {
            let shape = palw_economic_shape_v1(&profile, T).unwrap();
            for (p, d) in [(0u32, 0u32), (0, 3), (1, 1), (1, 2), (7, 2), (8, 4), (63, 2), (63, 1), (200, 5), (511, 2)] {
                let closed = palw_job_breakdown_from_shape_v1(&shape, p, d).total();
                assert_eq!(closed, job_by_loop(&shape, p, d), "{name} at ({p}, {d})");
            }
            let job = palw_job_economic_compute_v1(&profile, &canonical, T).unwrap();
            assert_eq!(job, job_by_loop(&shape, canonical.declared_prefill_tokens, canonical.exact_decode_tokens), "{name} canonical");
        }
    }

    /// **The live classes, priced: leaves against economic compute.** The numbers are pinned so a
    /// change to the cost table or to a profile is visible here first. Read with
    /// `cargo test -p kaspa-consensus-core adr0131_the_live -- --nocapture`.
    #[test]
    fn adr0131_the_live_classes_leaves_and_economic_compute_are_pinned() {
        let mut rows = Vec::new();
        for (name, id_prefix, profile, canonical) in live_classes() {
            assert!(
                profile.shape_profile_id().to_string().starts_with(id_prefix),
                "{name}: this is not the class testnet-11 registered"
            );
            let leaves = step_leaf_count_capped_v1(&profile, &canonical, u64::MAX).unwrap();
            let draw_job = crate::palw_attempt_v2::palw_attempt_job_v1(canonical.clone(), true);
            let draw_leaves = step_leaf_count_capped_v1(&profile, &draw_job, u64::MAX).unwrap();
            let canonical_b = palw_job_economic_breakdown_v1(&profile, &canonical, T).unwrap();
            let draw_b = palw_job_economic_breakdown_v1(&profile, &draw_job, T).unwrap();
            let tokens = canonical.declared_prefill_tokens as u128 + canonical.exact_decode_tokens as u128;
            eprintln!(
                "CCU {name}: canonical ({}+{}) leaves={leaves} ccu={} (per token {}, per leaf {}) | draw job leaves={draw_leaves} ccu={} | dense={} routed={} attn={} gdn={} logits={} elem={}",
                canonical.declared_prefill_tokens,
                canonical.exact_decode_tokens,
                canonical_b.total(),
                canonical_b.total() / tokens,
                canonical_b.total() / leaves as u128,
                draw_b.total(),
                canonical_b.dense_matmul,
                canonical_b.routed_experts,
                canonical_b.attention,
                canonical_b.recurrence,
                canonical_b.logits,
                canonical_b.elementwise,
            );
            rows.push((name, leaves, canonical_b.total(), draw_b.total(), draw_leaves));
        }
        // Every class costs more per leaf than the floor's integer graph, and the two model tiers
        // are not priced alike per leaf — which is the whole finding.
        let per_leaf: Vec<u128> = rows.iter().map(|(_, leaves, ccu, _, _)| ccu / *leaves as u128).collect();
        assert!(per_leaf.iter().skip(1).all(|&x| x > per_leaf[0]), "{per_leaf:?}");
        assert_ne!(per_leaf[1], per_leaf[2], "the hybrid and the dense tier do not cost the same per leaf: {per_leaf:?}");
        // Pinned. The leaves are the chain's registrations; the compute is this table's.
        let expected: Vec<(&str, u64, u128, u128, u64)> = vec![
            ("PALW-BASE-0/rc", 7_708, PIN_FLOOR_CCU, PIN_FLOOR_DRAW_CCU, PIN_FLOOR_DRAW_LEAVES),
            ("Qwen3.6-35B-A3B/graph-v3", 2_685_360, PIN_QWEN36_CCU, PIN_QWEN36_DRAW_CCU, PIN_QWEN36_DRAW_LEAVES),
            ("Qwen2.5-1.5B/graph-v5@512", 6_630_544, PIN_QWEN25_CCU, PIN_QWEN25_DRAW_CCU, PIN_QWEN25_DRAW_LEAVES),
            ("Qwen3.8-27B/graph-v3", 9_000_776, PIN_QWEN38_CCU, PIN_QWEN38_DRAW_CCU, PIN_QWEN38_DRAW_LEAVES),
        ];
        assert_eq!(rows, expected);
    }

    // The pins. Filled from the first measured run and moved only with a versioned table change.
    const PIN_FLOOR_CCU: u128 = 30_504_896;
    const PIN_FLOOR_DRAW_CCU: u128 = 21_657_728;
    const PIN_FLOOR_DRAW_LEAVES: u64 = 5_560;
    const PIN_QWEN36_CCU: u128 = 21_070_759_296;
    const PIN_QWEN36_DRAW_CCU: u128 = 18_055_200_736;
    const PIN_QWEN36_DRAW_LEAVES: u64 = 2_326_264;
    const PIN_QWEN25_CCU: u128 = 84_653_733_376;
    const PIN_QWEN25_DRAW_CCU: u128 = 83_102_171_136;
    const PIN_QWEN25_DRAW_LEAVES: u64 = 6_508_520;
    const PIN_QWEN38_CCU: u128 = 198_712_432_640;
    const PIN_QWEN38_DRAW_CCU: u128 = 172_919_123_392;
    const PIN_QWEN38_DRAW_LEAVES: u64 = 7_828_768;

    /// **The routed experts are priced as the active experts only**, and the block-diagonal down
    /// projection is not overcounted: on the hybrid, the mixture's three matmuls cost
    /// `3 × k × hidden × moe_dim` a layer, and the whole is what the geometry says.
    #[test]
    fn adr0131_a_mixture_layer_costs_its_active_experts_only() {
        let g = QWEN36_35B_A3B;
        let profile = qwen36_profile_v2(qwen36_geometry_artifact_eps(g)).unwrap();
        let shape = palw_economic_shape_v1(&profile, T).unwrap();
        let one_position = shape.body_at(1);
        let per_layer_routed = 3u128 * g.experts_per_token as u128 * g.hidden_dim as u128 * g.moe_dim as u128 * T.matmul_mac as u128;
        assert_eq!(
            one_position.routed_experts,
            per_layer_routed * g.layer_count as u128,
            "eight experts of 2048 × 512, three matmuls, forty layers"
        );
        assert_eq!(routed_group_count(&profile.gdn_nodes), g.experts_per_token as u128);
        assert_eq!(routed_group_count(&profile.attn_nodes), g.experts_per_token as u128);
        // A dense graph routes nothing.
        let dense = palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).unwrap();
        assert_eq!(routed_group_count(&dense.attn_nodes), 1);
        assert_eq!(palw_economic_shape_v1(&dense, T).unwrap().body_at(1).routed_experts, 0);
    }

    /// **Attention is priced by the true kv length and nothing else grows with it**: the dense
    /// tier's per-position cost at kv length `L` is `constant + L × heads × (2 × head_dim × attention
    /// + transcendental + elementwise)` (the fused site), and the hybrid's attention layers likewise.
    #[test]
    fn adr0131_only_attention_grows_with_the_kv_length() {
        let dense = palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).unwrap();
        let shape = palw_economic_shape_v1(&dense, T).unwrap();
        let (a, b) = (shape.body_at(1), shape.body_at(2));
        let heads = dense.attn_heads as u128;
        let per_kv =
            heads * (2 * dense.attn_head_dim as u128 * T.attention_mac as u128 + T.transcendental as u128 + T.elementwise as u128);
        assert_eq!(b.attention - a.attention, per_kv * dense.layer_count as u128);
        assert_eq!(
            (b.dense_matmul, b.routed_experts, b.recurrence, b.logits, b.elementwise),
            (a.dense_matmul, a.routed_experts, a.recurrence, a.logits, a.elementwise)
        );
        assert_eq!(shape.logits_at(5), shape.logits_at(500), "the LM head does not read the cache");
    }

    /// **The price of work in u128, the priced basis and the unit.** The reward is never above the
    /// escrow, a unit of zero pays whole, and the unit is the largest measure among the classes that
    /// set it, in the basis's own measure.
    #[test]
    fn adr0131_the_priced_reward_and_the_unit() {
        assert_eq!(palw_priced_reward_u128_v1(1_000, 0, 0), 1_000);
        assert_eq!(palw_priced_reward_u128_v1(1_000, 5, 10), 500);
        assert_eq!(palw_priced_reward_u128_v1(1_000, 30, 10), 1_000);
        assert_eq!(palw_priced_reward_u128_v1(u64::MAX, u128::MAX / 2, u128::MAX), u64::MAX / 2);
        assert_eq!(
            palw_priced_reward_u128_v1(320_084_650_080, 18_055_200_736, 83_102_171_136),
            69_543_220_480,
            "the hybrid's job against the dense tier's: 21.73 %"
        );
        // A window's total is priced once, exactly; a hundred claims priced one by one lose a hundred
        // floors, so the total is within the claim count of their sum.
        let total = palw_priced_total_v1(320_084_650_080 * 100, 18_055_200_736, 83_102_171_136);
        assert!(total >= 69_543_220_480 * 100 && total - 69_543_220_480 * 100 < 100, "{total}");
        assert_eq!(palw_priced_total_v1(1u128 << 100, 1, 2), 1u128 << 99, "a window total past 2⁶⁴ keeps its scale");
        let huge = palw_priced_total_v1(u128::MAX / 4, 1, 2);
        assert!(huge <= u128::MAX / 8 && u128::MAX / 8 - huge < 1u128 << 62, "…to the bits the scale-down drops: {huge}");
        let classes = [
            PalwClassMeasureV1 {
                leaves: 7_708,
                draw_compute: 100,
                expected_attempts_q32: 26_404 << 32,
                network_expected_attempts_q32: NET_ONE,
                sets_unit: false,
                priced: false,
            },
            PalwClassMeasureV1 {
                leaves: 2_685_360,
                draw_compute: 16_000,
                expected_attempts_q32: 1 << 32,
                network_expected_attempts_q32: NET_ONE,
                sets_unit: true,
                priced: true,
            },
            PalwClassMeasureV1 {
                leaves: 6_630_544,
                draw_compute: 80_000,
                expected_attempts_q32: 1 << 32,
                network_expected_attempts_q32: NET_ONE,
                sets_unit: true,
                priced: true,
            },
        ];
        assert_eq!(palw_basis_unit_v1(PalwRewardBasisV1::CurrentLeaves, &classes), 6_630_544);
        assert_eq!(palw_basis_unit_v1(PalwRewardBasisV1::EconomicJob, &classes), 80_000);
        assert_eq!(palw_basis_unit_v1(PalwRewardBasisV1::EconomicAttempted, &classes), 80_000);
        assert_eq!(classes[0].measure(PalwRewardBasisV1::EconomicAttempted), 2_640_400, "the floor's 26,404 draws of 100");
        assert_eq!(palw_basis_unit_v1(PalwRewardBasisV1::CurrentLeaves, &classes[..1]), 0, "no unit without a model class");
    }

    /// testnet-11 past DAA 7,001: the escrow a claim holds (72 % of the 4,445.62 MSK block) and the
    /// two model classes as this table measures them at the live targets (both at one expected
    /// attempt, `getPalwProducerFacts` 2026-09-17).
    const T11_ESCROW_7001: u64 = 320_084_650_080;
    /// One network draw a class win: the factor the measures below hold fixed, so every pin above
    /// reads as it did before ADR-0132 added the factor.
    const NET_ONE: u128 = PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
    const SEATS: u64 = 5;
    fn qwen36(expected_attempts: u64) -> PalwClassMeasureV1 {
        let expected_attempts_q32 = (expected_attempts as u128) << 32;
        PalwClassMeasureV1 {
            leaves: 2_685_360,
            draw_compute: PIN_QWEN36_DRAW_CCU,
            expected_attempts_q32,
            network_expected_attempts_q32: NET_ONE,
            sets_unit: true,
            priced: true,
        }
    }
    fn qwen25(expected_attempts: u64) -> PalwClassMeasureV1 {
        let expected_attempts_q32 = (expected_attempts as u128) << 32;
        PalwClassMeasureV1 {
            leaves: 6_630_544,
            draw_compute: PIN_QWEN25_DRAW_CCU,
            expected_attempts_q32,
            network_expected_attempts_q32: NET_ONE,
            sets_unit: true,
            priced: true,
        }
    }
    fn input(measure: PalwClassMeasureV1, accepted: u64, finals: u64) -> PalwClassEconomicsInputV1 {
        PalwClassEconomicsInputV1 {
            measure,
            escrow_final_sompi: T11_ESCROW_7001 as u128 * finals as u128,
            claims_accepted: accepted,
            claims_final: finals,
            seats_replaying: SEATS,
        }
    }
    fn gap(a: u128, b: u128) -> u128 {
        palw_gap_permille_v1(a, b).expect("both classes were paid")
    }
    const ALL_BASES: [PalwRewardBasisV1; 3] =
        [PalwRewardBasisV1::CurrentLeaves, PalwRewardBasisV1::EconomicJob, PalwRewardBasisV1::EconomicAttempted];

    /// **The two live classes, every mix, every basis.** Per-claim pricing makes a class's rate
    /// per unit of compute independent of how many claims it or its neighbour made — only one class,
    /// half and half, nine to one either way — so the gap is a property of the basis: on today's
    /// leaf basis the dense tier is paid 62.7 % less per unit of compute than the hybrid (a clear
    /// distortion by the operator's 20 % line); on the compute bases the gap is 0.
    #[test]
    fn adr0131_the_gap_between_qwen25_and_qwen36_is_the_basis_not_the_mix() {
        let classes = [qwen36(1), qwen25(1)];
        for (n36, n25) in [(1_000u64, 0u64), (0, 1_000), (500, 500), (900, 100), (100, 900)] {
            for basis in ALL_BASES {
                let unit = palw_basis_unit_v1(basis, &classes);
                let e36 = palw_class_economics_v1(basis, &input(classes[0], n36, n36), unit);
                let e25 = palw_class_economics_v1(basis, &input(classes[1], n25, n25), unit);
                // A class with no claims has no rate; the other's rate is what it is with any mix.
                let rate = |e: &PalwClassEconomicsV1| (e.total_per_final_compute(), e.total_per_attempted_compute());
                let reference36 = palw_class_economics_v1(basis, &input(classes[0], 7, 7), unit);
                let reference25 = palw_class_economics_v1(basis, &input(classes[1], 7, 7), unit);
                if n36 > 0 {
                    assert_eq!(
                        rate(&e36),
                        rate(&reference36),
                        "{basis:?} at ({n36}, {n25}): the hybrid's rate does not depend on the mix"
                    );
                }
                if n25 > 0 {
                    assert_eq!(
                        rate(&e25),
                        rate(&reference25),
                        "{basis:?} at ({n36}, {n25}): the dense tier's rate does not depend on the mix"
                    );
                }
                let gap_final = gap(reference36.total_per_final_compute(), reference25.total_per_final_compute());
                let gap_attempted = gap(reference36.total_per_attempted_compute(), reference25.total_per_attempted_compute());
                match basis {
                    PalwRewardBasisV1::CurrentLeaves => {
                        // The price is the canonical job's leaves (2.469× the hybrid's) and the compute
                        // is the job an attempt runs (4.603×): the leaf basis pays the hybrid 1.864× per
                        // unit of the compute it ran. (Canonical against canonical, 12,767 MAC-eq a
                        // leaf against 7,846, is 1.627×; the executed job loses the hybrid two of its
                        // nine tokens' decode calls and the dense tier two of sixty-five.)
                        let expected = gap(PIN_QWEN36_DRAW_CCU * 1_000_000 / 2_685_360, PIN_QWEN25_DRAW_CCU * 1_000_000 / 6_630_544);
                        assert!(gap_final.abs_diff(expected) <= 2, "leaf basis, gap_final {gap_final} ‰ against {expected} ‰");
                        assert!((855..=875).contains(&gap_final), "leaf basis, gap_final {gap_final} ‰");
                        assert!(gap_attempted.abs_diff(expected) <= 2, "leaf basis, gap_attempted {gap_attempted} ‰");
                        assert!(reference36.total_per_final_compute() > reference25.total_per_final_compute());
                    }
                    _ => {
                        assert!(gap_final <= 1, "{basis:?}: gap_final {gap_final} ‰ (integer rounding only)");
                        assert!(gap_attempted <= 1, "{basis:?}: gap_attempted {gap_attempted} ‰");
                    }
                }
            }
        }
    }

    /// **A difference in `Final` rate shows in `A_m` and not in `F_m`, on every basis.** The hybrid's
    /// panels license one claim in five, the dense tier's four in five: what each `Final` claim is
    /// paid per unit it certified is unchanged, and the pay per unit attempted falls with the rate.
    #[test]
    fn adr0131_a_final_rate_gap_is_an_attempted_gap_not_a_final_gap() {
        let classes = [qwen36(1), qwen25(1)];
        for basis in ALL_BASES {
            let unit = palw_basis_unit_v1(basis, &classes);
            let full36 = palw_class_economics_v1(basis, &input(classes[0], 100, 100), unit);
            let half36 = palw_class_economics_v1(basis, &input(classes[0], 100, 20), unit);
            let full25 = palw_class_economics_v1(basis, &input(classes[1], 100, 100), unit);
            let most25 = palw_class_economics_v1(basis, &input(classes[1], 100, 80), unit);
            assert_eq!(half36.total_per_final_compute(), full36.total_per_final_compute(), "{basis:?}: F_m does not read the rate");
            assert_eq!(half36.total_per_attempted_compute(), full36.total_per_attempted_compute() / 5, "{basis:?}: A_m is one fifth");
            assert_eq!(
                most25.total_per_attempted_compute(),
                full25.total_per_attempted_compute() * 4 / 5,
                "{basis:?}: A_m is four fifths"
            );
            let gap_attempted = gap(half36.total_per_attempted_compute(), most25.total_per_attempted_compute());
            if basis != PalwRewardBasisV1::CurrentLeaves {
                assert!(
                    (2_995..=3_005).contains(&gap_attempted),
                    "{basis:?}: the attempted gap is the rate ratio, 4 → {gap_attempted} ‰"
                );
            }
            // A class whose panels license nothing has no rate, and the comparison says so.
            let none36 = palw_class_economics_v1(basis, &input(classes[0], 100, 0), unit);
            assert_eq!(palw_gap_permille_v1(none36.total_per_attempted_compute(), most25.total_per_attempted_compute()), None);
        }
    }

    /// **A class target that moves is paid for on the attempted basis only.** At three expected
    /// attempts a hybrid claim cost three draws: the job basis and the leaf basis pay it as one, so
    /// its pay per attempted unit is a third of the dense tier's; the attempted basis pays the draws,
    /// so per attempted unit the two are equal and per `Final` unit the hybrid is paid three times —
    /// and no basis pays more than the escrow.
    #[test]
    fn adr0131_a_moving_class_target_is_paid_for_on_the_attempted_basis_only() {
        let classes = [qwen36(3), qwen25(1)];
        for basis in ALL_BASES {
            let unit = palw_basis_unit_v1(basis, &classes);
            let e36 = palw_class_economics_v1(basis, &input(classes[0], 100, 100), unit);
            let e25 = palw_class_economics_v1(basis, &input(classes[1], 100, 100), unit);
            assert!(e36.reward_per_final_sompi <= T11_ESCROW_7001 && e25.reward_per_final_sompi <= T11_ESCROW_7001);
            let gap_attempted = gap(e36.total_per_attempted_compute(), e25.total_per_attempted_compute());
            let gap_final = gap(e36.total_per_final_compute(), e25.total_per_final_compute());
            match basis {
                PalwRewardBasisV1::EconomicAttempted => {
                    assert!(gap_attempted <= 1, "attempted basis: A_m equal, got {gap_attempted} ‰");
                    assert!((1_995..=2_005).contains(&gap_final), "attempted basis: the hybrid's F_m is 3×, got {gap_final} ‰");
                    assert!(e36.total_per_final_compute() > e25.total_per_final_compute());
                }
                PalwRewardBasisV1::EconomicJob => {
                    assert!((1_995..=2_005).contains(&gap_attempted), "job basis: the hybrid's A_m is a third, got {gap_attempted} ‰");
                    assert!(e36.total_per_attempted_compute() < e25.total_per_attempted_compute());
                }
                PalwRewardBasisV1::CurrentLeaves => {
                    // The leaf premium (1.627×) against three draws: the hybrid nets 0.54× per attempted unit.
                    assert!(
                        e36.total_per_attempted_compute() < e25.total_per_attempted_compute(),
                        "leaf basis: three draws outweigh the leaf premium"
                    );
                }
            }
        }
    }

    /// **Panels: the pool per unit of verification compute is equal on the job basis and not on the
    /// attempted one.** A seat replays one job whatever the producer's draws cost, so a basis that
    /// pays the producer's draws overpays the panels of a hard-target class by the same factor —
    /// ADR-0131 Decision 4's reason to measure the panel's compute apart.
    #[test]
    fn adr0131_panel_pay_per_verification_compute_follows_the_job_not_the_draws() {
        let classes = [qwen36(3), qwen25(1)];
        for basis in ALL_BASES {
            let unit = palw_basis_unit_v1(basis, &classes);
            let e36 = palw_class_economics_v1(basis, &input(classes[0], 100, 100), unit);
            let e25 = palw_class_economics_v1(basis, &input(classes[1], 100, 100), unit);
            assert_eq!(e36.panel_compute, PIN_QWEN36_DRAW_CCU * SEATS as u128 * 100, "five seats replay every accepted claim's job");
            let gap_panel = gap(e36.panel_per_verification_compute(), e25.panel_per_verification_compute());
            match basis {
                PalwRewardBasisV1::EconomicJob => assert!(gap_panel <= 1, "job basis: {gap_panel} ‰"),
                PalwRewardBasisV1::EconomicAttempted => {
                    assert!((1_995..=2_005).contains(&gap_panel), "attempted basis: {gap_panel} ‰")
                }
                PalwRewardBasisV1::CurrentLeaves => assert!((855..=875).contains(&gap_panel), "leaf basis: {gap_panel} ‰"),
            }
            // And the pool is a fifth of the priced reward, to the division's dust, on every basis.
            let reward = e36.reward_per_final_sompi as u128;
            let pool = e36.panel_pool_per_final_sompi as u128;
            assert!(pool * 5 <= reward && reward < pool * 5 + 5, "{basis:?}: pool {pool} of reward {reward}");
        }
    }

    /// **No basis mints: `producer + pool + burned == escrow × finals` on every basis, every class,
    /// every mix**, the reward never exceeds the escrow, and moving between bases moves only the
    /// burned share. A class that sets the unit is paid whole; a heavier class registered at any
    /// share lowers every other class's pay and never raises the total withheld.
    #[test]
    fn adr0131_no_basis_changes_the_total_withheld() {
        let with_qwen38 = [
            qwen36(1),
            qwen25(1),
            PalwClassMeasureV1 {
                leaves: 9_000_776,
                draw_compute: PIN_QWEN38_DRAW_CCU,
                expected_attempts_q32: 3_165 << 32,
                network_expected_attempts_q32: NET_ONE,
                sets_unit: true,
                priced: true,
            },
        ];
        for classes in [&with_qwen38[..2], &with_qwen38[..]] {
            for (n36, n25) in [(1_000u64, 0u64), (500, 500), (900, 100), (100, 900), (0, 1_000)] {
                for basis in ALL_BASES {
                    let unit = palw_basis_unit_v1(basis, classes);
                    for (class, accepted, finals) in [(classes[0], n36, n36 / 2), (classes[1], n25, n25 * 4 / 5)] {
                        let e = palw_class_economics_v1(basis, &input(class, accepted, finals), unit);
                        assert!(e.reward_per_final_sompi <= T11_ESCROW_7001);
                        assert_eq!(
                            e.producer_paid_sompi + e.panel_pool_sompi + e.burned_sompi,
                            T11_ESCROW_7001 as u128 * finals as u128,
                            "{basis:?} with {} classes at ({n36}, {n25})",
                            classes.len()
                        );
                        // The per-claim averages are the totals divided separately: within a sompi.
                        assert!(
                            (e.producer_per_final_sompi as u128 + e.panel_pool_per_final_sompi as u128)
                                .abs_diff(e.reward_per_final_sompi as u128)
                                <= 1,
                            "{:?}",
                            e
                        );
                    }
                    // Whoever sets the unit is paid whole.
                    let top = classes.iter().filter(|c| c.sets_unit).max_by_key(|c| c.measure(basis)).unwrap();
                    assert_eq!(palw_class_economics_v1(basis, &input(*top, 1, 1), unit).reward_per_final_sompi, T11_ESCROW_7001);
                }
            }
        }
        // The heavier class's registration: the same two classes are paid less against it, and the
        // gap between them is unchanged.
        for basis in ALL_BASES {
            let pair = palw_basis_unit_v1(basis, &with_qwen38[..2]);
            let three = palw_basis_unit_v1(basis, &with_qwen38);
            assert!(three > pair, "{basis:?}: the 27B sets the unit");
            let (a2, b2) = (
                palw_class_economics_v1(basis, &input(with_qwen38[0], 10, 10), pair),
                palw_class_economics_v1(basis, &input(with_qwen38[1], 10, 10), pair),
            );
            let (a3, b3) = (
                palw_class_economics_v1(basis, &input(with_qwen38[0], 10, 10), three),
                palw_class_economics_v1(basis, &input(with_qwen38[1], 10, 10), three),
            );
            assert!(a3.reward_per_final_sompi < a2.reward_per_final_sompi && b3.reward_per_final_sompi < b2.reward_per_final_sompi);
            let gap2 = palw_gap_permille_v1(a2.total_per_final_compute(), b2.total_per_final_compute());
            let gap3 = palw_gap_permille_v1(a3.total_per_final_compute(), b3.total_per_final_compute());
            assert!(gap2.unwrap().abs_diff(gap3.unwrap()) <= 2, "{basis:?}: {gap2:?} vs {gap3:?}");
        }
    }

    /// **The draws a claim costs are read with their fraction.** The fork-choice factor floors
    /// `2¹²⁸ / (target + 1)` to whole inferences; a price of attempts must not, or a class drawn at
    /// two thirds of MAX (1.5 draws a claim) is priced as one. The Q32 reading's integer part is the
    /// consensus factor at every target, its fraction is exact to 2⁻³², and on the attempted basis
    /// the class drawn 1.5 times is measured 1.5 times — the job basis, which prices one execution,
    /// under-reads it by half.
    #[test]
    fn adr0131_expected_attempts_keep_their_fraction_in_the_shadow() {
        use crate::palw_pwu::palw_expected_attempts_v1;
        let one = PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
        assert_eq!(palw_expected_attempts_q32_v1(u128::MAX), one, "every ticket admits: one draw");
        assert_eq!(palw_expected_attempts_q32_v1((1u128 << 127) - 1), 2 * one, "half of MAX: two draws exactly");
        assert_eq!(palw_expected_attempts_q32_v1((1u128 << 100) - 1), one << 28, "a power-of-two divisor divides exactly");
        assert_eq!(palw_expected_attempts_q32_v1(1u128 << 100), (one << 28) - 1, "…and one past it floors a bit below");
        let two_thirds = u128::MAX / 3 * 2;
        let q = palw_expected_attempts_q32_v1(two_thirds);
        assert!(q.abs_diff(one * 3 / 2) <= 2, "1.5 draws a claim in Q32: {q}");
        for target in [u128::MAX, u128::MAX / 2, two_thirds, u128::MAX / 12_665, u128::MAX / 1_000_000, 1u128 << 100, 1u128 << 40] {
            let q = palw_expected_attempts_q32_v1(target);
            // …to where the consensus factor saturates into u64 (a target of 2⁴⁰ is 2⁸⁸ draws).
            assert_eq!(
                (q >> 32).min(u64::MAX as u128),
                palw_expected_attempts_v1(target) as u128,
                "integer part is the consensus factor at {target}"
            );
        }
        assert_eq!(palw_expected_attempts_q32_v1(5), u128::MAX, "a target under 2³² saturates");
        assert_eq!(palw_attempted_compute_q32_per_claim_v1(q, 1_000_000), 1_499_999, "⌊1.5 × 10⁶⌋ to the bit the Q32 floor drops");
        assert_eq!(palw_attempted_compute_q32_per_claim_v1(0, 7), 7, "never below one draw");
        assert_eq!(palw_attempted_compute_q32_per_claim_v1(26_404 << 32, 100), 2_640_400);
        // Two classes of equal draw compute, one drawn at MAX and one at two thirds of it, over one
        // window: the attempted basis reads the second at 1.5 × the first (its A rates agree); the
        // job basis reads both as one execution and its A rates differ by that half.
        let at_max = PalwClassMeasureV1 {
            leaves: 1,
            draw_compute: 1_000_000,
            expected_attempts_q32: one,
            network_expected_attempts_q32: one,
            sets_unit: true,
            priced: true,
        };
        let at_two_thirds = PalwClassMeasureV1 { expected_attempts_q32: q, ..at_max };
        assert_eq!(at_two_thirds.measure(PalwRewardBasisV1::EconomicAttempted), 1_499_999);
        assert_eq!(at_two_thirds.measure(PalwRewardBasisV1::EconomicJob), 1_000_000);
        let classes = [at_max, at_two_thirds];
        let window = |m: PalwClassMeasureV1| PalwClassEconomicsInputV1 {
            measure: m,
            escrow_final_sompi: 320_084_650_080u128 * 100,
            claims_accepted: 100,
            claims_final: 100,
            seats_replaying: 5,
        };
        for (basis, expected_gap) in [(PalwRewardBasisV1::EconomicAttempted, 0..=1), (PalwRewardBasisV1::EconomicJob, 498..=502)] {
            let unit = palw_basis_unit_v1(basis, &classes);
            let a: Vec<u128> =
                classes.iter().map(|c| palw_class_economics_v1(basis, &window(*c), unit).total_per_attempted_compute()).collect();
            let gap = palw_gap_permille_v1(a[0], a[1]).expect("both paid");
            assert!(expected_gap.contains(&gap), "{basis:?}: A rates {a:?}, gap {gap}‰");
        }
    }

    /// **The floor is paid whole on every basis** (ADR-0124 Decision 6: not a model, not priced),
    /// and never sets the unit.
    #[test]
    fn adr0131_the_floor_is_unpriced_on_every_basis() {
        let floor = PalwClassMeasureV1 {
            leaves: 7_708,
            draw_compute: PIN_FLOOR_DRAW_CCU,
            expected_attempts_q32: 26_404 << 32,
            network_expected_attempts_q32: NET_ONE,
            sets_unit: false,
            priced: false,
        };
        let classes = [qwen36(1), qwen25(1), floor];
        for basis in ALL_BASES {
            let unit = palw_basis_unit_v1(basis, &classes);
            let e = palw_class_economics_v1(basis, &input(floor, 14, 6), unit);
            assert_eq!((e.price_permille, e.reward_per_final_sompi), (1000, T11_ESCROW_7001), "{basis:?}");
            assert_eq!(e.burned_sompi, 0);
        }
    }

    /// The gap helper: symmetric, zero on equality, none on a zero.
    #[test]
    fn adr0131_the_gap_is_symmetric_and_refuses_a_zero() {
        assert_eq!(palw_gap_permille_v1(100, 100), Some(0));
        assert_eq!(palw_gap_permille_v1(150, 100), Some(500));
        assert_eq!(palw_gap_permille_v1(100, 150), Some(500));
        assert_eq!(palw_gap_permille_v1(0, 150), None);
        assert_eq!(palw_gap_permille_v1(150, 0), None);
    }

    /// **The network draw is the second lottery, and it is priced from `bits`** (ADR-0132). At the
    /// difficulty floor `0x207fffff` a class win faces a coin flip (2.0 draws a win, to the 2⁻²³ the
    /// mantissa's top bit leaves); a tighter target costs more; a zero target saturates; and the
    /// attempted basis multiplies the class draws by it, so a class's attempted compute reads
    /// `class × network × draw` while every cross-model ratio is unchanged (one `bits` for all).
    #[test]
    fn adr0132_the_network_draw_is_priced_from_bits() {
        let one = PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;
        let floor = palw_network_expected_attempts_q32_v1(0x207f_ffff);
        assert!(floor >= 2 * one && floor <= 2 * one + 2_048, "the difficulty floor is a coin flip: {floor}");
        // Bitcoin's genesis bits, `0xffff << 208`: 2⁴⁸ / 0xffff draws a win, to the one the `+ 1` drops.
        let genesis = palw_network_expected_attempts_q32_v1(0x1d00_ffff);
        let expected = (1u128 << 48) / 0xffff;
        assert!((genesis >> 32).abs_diff(expected) <= 1, "{}", genesis >> 32);
        assert!(genesis > floor, "a tighter target costs more draws");
        assert!(palw_network_expected_attempts_q32_v1(0x1c00_ffff) > genesis, "and a tighter one still, monotone in the target");
        assert_eq!(palw_network_expected_attempts_q32_v1(0x0100_0000), u128::MAX, "a zero target saturates");
        assert_eq!(palw_network_expected_attempts_q32_v1(0x2080_0000), u128::MAX, "a negative compact mantissa is a zero target");
        let both = PalwClassMeasureV1 {
            leaves: 1,
            draw_compute: 1_000_000,
            expected_attempts_q32: one * 3 / 2,
            network_expected_attempts_q32: floor,
            sets_unit: true,
            priced: true,
        };
        let attempted = both.measure(PalwRewardBasisV1::EconomicAttempted);
        assert!((2_999_990..=3_001_500).contains(&attempted), "1.5 class draws × 2.0 network draws × 10⁶: {attempted}");
        let net_only = PalwClassMeasureV1 { expected_attempts_q32: one, ..both };
        assert!(net_only.measure(PalwRewardBasisV1::EconomicAttempted) >= 1_999_990);
        // The factor is common to every class at a moment: the ratio between two classes on the
        // attempted basis is the same at the floor and at genesis bits.
        let (a, b) = (both, PalwClassMeasureV1 { draw_compute: 250_000, ..both });
        let ratio_at = |bits: u32| {
            let n = palw_network_expected_attempts_q32_v1(bits);
            let ma = PalwClassMeasureV1 { network_expected_attempts_q32: n, ..a }.measure(PalwRewardBasisV1::EconomicAttempted);
            let mb = PalwClassMeasureV1 { network_expected_attempts_q32: n, ..b }.measure(PalwRewardBasisV1::EconomicAttempted);
            ma * 1000 / mb
        };
        assert_eq!(ratio_at(0x207f_ffff) / 10, ratio_at(0x1d00_ffff) / 10, "one bits for all: the ratio does not move");
    }
}
