//! **ADR-0099 — a seat holds a shard, not the model: the plan, derived.**
//!
//! A class's execution is one row per node per position, the nodes in one pinned order —
//! `pre ‖ layer 0 ‖ … ‖ layer L−1 ‖ post` — and every row is a committed step leaf. A **shard**
//! is a contiguous run of layers; shard 0 also holds `pre` (the embedding) and the last shard
//! holds `post` (the logits). Two facts of the shipped tree make a shard a unit a seat can verify
//! with nothing but its own layers:
//!
//! * **A shard's input is another shard's committed output.** The layer input of the shard's
//!   first layer is the output row of the previous shard's last node, at the same position — a
//!   step leaf, opened against the claim's step root like any other. Nothing new is committed.
//! * **A shard's leaves are contiguous at every position.** Main leaves are ordered
//!   position-major then by global node slot ([`crate::palw_step::canonical_step_coordinates`]),
//!   so a layer range is a slot range and a slot range is one run of leaves per position
//!   ([`palw_shard_leaf_run_v1`]). A shard seat's interval opening is therefore the same object a
//!   whole-model seat opens today, cut to its run.
//!
//! What this module decides is the PLAN — which layers go together, and what each shard costs a
//! seat: its artifact bytes (from the family's geometry, one byte a weight), its attention cache
//! at the class's context, its recurrent state, and the rows that cross its boundary. The plan is
//! derived from the class and a shard count (or a seat's budget), never chosen: two nodes given
//! the same class and the same number produce the same shards, which is what lets a shard be named
//! on a chain (ADR-0099 Decision 3's capability id) without carrying the plan.
//!
//! Every number here is a generated artifact (ADR-0092 §5); `misaka-palw-base0 --bin
//! palw-shard-plan` prints them. The artifact estimate counts the projections a layer holds and
//! not its norms or biases, so it is a floor, and the card's own total is the other bracket.

use crate::palw_qwen25_profile::PalwQwen25GeometryV1;
use crate::palw_qwen36_profile::PalwQwen36GeometryV1;
use crate::palw_state_chunk_map::gdn_delta_head_slice_bytes_v1;
use crate::palw_step::PalwStepCoordinateV1;
use crate::palw_step::{PalwLayerKindV1, PalwShapeProfileV3, PalwStepOutLenV1, PalwStepTableV1, canonical_step_leaf_index};
use crate::palw_v2::PalwJobContextV2;

// =================================================================================================
// What an artifact holds, per layer
// =================================================================================================

/// The bytes an artifact holds, split the way a shard splits them: the embedding, each layer, the
/// unembedding. One byte a weight — the integer family's dtype — and no norms, biases or
/// per-row triples, so every entry is a floor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwArtifactBytesV1 {
    pub pre: u64,
    pub post: u64,
    /// One entry per layer, in layer order.
    pub layers: Vec<u64>,
    /// Where the numbers came from — the family formula, or a card's total the formula was scaled to.
    pub basis: &'static str,
}

impl PalwArtifactBytesV1 {
    pub fn total(&self) -> u64 {
        self.layers.iter().fold(self.pre.saturating_add(self.post), |acc, l| acc.saturating_add(*l))
    }

    /// The same split, scaled so that the total is `total` — for a model whose card states a
    /// parameter count the formula does not reach (or exceeds). The proportions are the formula's;
    /// the total is the card's; the basis says so.
    pub fn scaled_to_total(&self, total: u64) -> Self {
        let own = self.total().max(1) as u128;
        let scale = |v: u64| ((v as u128) * (total as u128) / own) as u64;
        PalwArtifactBytesV1 {
            pre: scale(self.pre),
            post: scale(self.post),
            layers: self.layers.iter().map(|l| scale(*l)).collect(),
            basis: "the family formula's proportions, scaled to a stated total",
        }
    }
}

/// The dense family (Qwen2.5): per layer, the four attention projections and the three MLP
/// projections; the embedding and the unembedding once each.
pub fn palw_qwen25_artifact_bytes_v1(g: &PalwQwen25GeometryV1) -> PalwArtifactBytesV1 {
    let h = g.hidden_dim as u64;
    let q = (g.attn_heads as u64) * (g.attn_head_dim as u64);
    let kv = (g.attn_kv_heads as u64) * (g.attn_head_dim as u64);
    let attention = h * q + 2 * h * kv + q * h;
    let mlp = 3 * h * (g.ffn_dim as u64);
    let vocab = (g.vocab_size as u64) * h;
    PalwArtifactBytesV1 {
        pre: vocab,
        post: vocab,
        layers: vec![attention + mlp; g.layer_count as usize],
        basis: "dense: q,k,v,o + gate,up,down per layer; embedding and unembedding; one byte a weight",
    }
}

/// The hybrid family (Qwen3.6 / the qwen3moe members / the K3 stand-in): per attention layer the
/// q (doubled under the output gate), k, v and o projections; per recurrent layer the q, k, v,
/// gate and output projections of the delta rule; and per layer the mixture — every expert's
/// three projections, the shared expert's three, and the router.
pub fn palw_qwen36_artifact_bytes_v1(g: &PalwQwen36GeometryV1) -> PalwArtifactBytesV1 {
    let h = g.hidden_dim as u64;
    let q = (g.attn_heads as u64) * (g.attn_head_dim as u64);
    let kv = (g.attn_kv_heads as u64) * (g.attn_head_dim as u64);
    let attention = h * q * if g.attn_output_gate != 0 { 2 } else { 1 } + 2 * h * kv + q * h;
    let gk = (g.gdn_k_heads as u64) * (g.gdn_head_dim as u64);
    let gv = (g.gdn_v_heads as u64) * (g.gdn_head_dim as u64);
    let recurrent = 2 * h * gk + h * gv + h * gv + gv * h;
    let moe = (g.n_experts as u64) * 3 * h * (g.moe_dim as u64) + 3 * h * (g.shared_dim as u64) + h * (g.n_experts as u64);
    let vocab = (g.vocab_size as u64) * h;
    let layers = (0..g.layer_count)
        .map(|i| {
            let is_attention =
                g.full_attention_interval != 0 && (u32::from(i) + 1).is_multiple_of(u32::from(g.full_attention_interval));
            if is_attention { attention + moe } else { recurrent + moe }
        })
        .collect();
    PalwArtifactBytesV1 {
        pre: vocab,
        post: vocab,
        layers,
        basis: "hybrid: attention (q[,gate],k,v,o) or delta-rule (q,k,v,gate,o) + every expert's gate,up,down + shared + router per layer; one byte a weight",
    }
}

// =================================================================================================
// The plan
// =================================================================================================

/// One shard of a plan: a contiguous layer range, with what a seat holding it must hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwShardV1 {
    pub index: u32,
    pub first_layer: u16,
    pub layer_count: u16,
    /// Shard 0 holds the embedding.
    pub holds_pre: bool,
    /// The last shard holds the logits.
    pub holds_post: bool,
    /// The artifact bytes of the shard's layers (and `pre` / `post` where held).
    pub artifact_bytes: u64,
    pub attention_layers: u32,
    pub recurrent_layers: u32,
    /// `attention_layers × 2 × kv_row × n_ctx` — the i32 cache the shard's attention layers
    /// write at the class's context.
    pub kv_cache_bytes: u64,
    /// `recurrent_layers × gdn_heads × (k × v × 4)` — constant in the context.
    pub recurrent_state_bytes: u64,
    /// The shard's first global node slot, and how many slots it holds.
    pub first_slot: u32,
    pub slot_count: u32,
}

impl PalwShardV1 {
    /// What a seat holding this shard must have resident to replay one job at the class's
    /// context: the artifact, the cache, the state.
    pub fn seat_bytes(&self) -> u64 {
        self.artifact_bytes.saturating_add(self.kv_cache_bytes).saturating_add(self.recurrent_state_bytes)
    }

    /// **What a seat of this shard is served to resume an interval, by the two forms a seat can
    /// take** (ADR-0099 Decision 2), for a job of `positions` positions:
    ///
    /// * `recompute` — the boundary rows of every position before the interval (the previous
    ///   shard's committed outputs, `positions × hidden × 4`), from which the seat recomputes its
    ///   own layers' state, as a whole-model seat recomputes from the prompt (ADR-0082 D9). Shard
    ///   0 needs none: its input is the prompt ids it holds.
    /// * `resume` — the checkpoint chunks of the shard's own layers at the interval's start (the
    ///   cache and the recurrent state, ADR-0077 D8's original form, cut to the shard), which is
    ///   at most the shard's state at the class's context.
    ///
    /// Both are committed material (step leaves; state chunks against the checkpoint root), so
    /// either is verifiable; which is cheaper is a property of the shard — many attention layers
    /// and a narrow residual favour recompute, few and a wide one favour resume — and the plan
    /// prints both rather than choosing.
    pub fn resume_transfer_bytes(&self, positions: u64, boundary_row_bytes: u64) -> (u64, u64) {
        let recompute = if self.holds_pre { 0 } else { positions.saturating_mul(boundary_row_bytes) };
        let resume = self.kv_cache_bytes.saturating_add(self.recurrent_state_bytes);
        (recompute, resume)
    }
}

/// A plan: the shards, and what crosses between them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwShardPlanV1 {
    /// The context the state terms were sized at — the profile's `n_ctx`.
    pub n_ctx: u32,
    pub shard_count: u32,
    pub shards: Vec<PalwShardV1>,
    /// One boundary row: the residual stream at one position, `hidden_dim × 4` bytes.
    pub boundary_row_bytes: u64,
    /// `max` over shards of [`PalwShardV1::seat_bytes`].
    pub widest_seat_bytes: u64,
}

impl PalwShardPlanV1 {
    /// The rows that cross shard boundaries for one job of `positions` positions:
    /// `(shards − 1) × positions × boundary_row_bytes`. Every one of them is a committed step
    /// leaf, so this is also what the boundary openings carry, and what the seats of two
    /// adjacent shards each need served to replay a job whole.
    pub fn boundary_bytes_per_job(&self, positions: u64) -> u64 {
        u64::from(self.shard_count.saturating_sub(1)).saturating_mul(positions).saturating_mul(self.boundary_row_bytes)
    }

    pub fn shard(&self, index: u32) -> Option<&PalwShardV1> {
        self.shards.get(index as usize)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwShardPlanError {
    #[error("a plan needs at least one shard")]
    ZeroShards,
    #[error("the class has no layers")]
    NoLayers,
    #[error("the artifact estimate has {got} layer entries and the profile has {expected} layers")]
    LayerCountMismatch { got: usize, expected: u16 },
    #[error("{shards} shards asked of {layers} layers: a shard holds at least one layer")]
    TooManyShards { shards: u32, layers: u16 },
    #[error("no plan of up to {max_shards} shards keeps a seat under {budget} bytes: the smallest widest seat is {smallest_widest}")]
    BudgetTooSmall { budget: u64, max_shards: u32, smallest_widest: u64 },
}

/// The per-layer weight the partition balances: the layer's artifact bytes plus the state its
/// kind holds at the class's context. `pre` rides layer 0 and `post` rides the last layer,
/// because they are pinned to the ends.
fn layer_weights_v1(profile: &PalwShapeProfileV3, artifact: &PalwArtifactBytesV1) -> Result<Vec<u64>, PalwShardPlanError> {
    let layers = profile.layer_count;
    if layers == 0 {
        return Err(PalwShardPlanError::NoLayers);
    }
    if artifact.layers.len() != layers as usize {
        return Err(PalwShardPlanError::LayerCountMismatch { got: artifact.layers.len(), expected: layers });
    }
    let kv_layer = kv_layer_bytes_v1(profile);
    let gdn_layer = gdn_layer_bytes_v1(profile);
    let mut weights: Vec<u64> = (0..layers)
        .map(|l| {
            let state = match profile.layer_kind(l) {
                PalwLayerKindV1::Attention => kv_layer,
                PalwLayerKindV1::GatedDeltaNet => gdn_layer,
            };
            artifact.layers[l as usize].saturating_add(state)
        })
        .collect();
    weights[0] = weights[0].saturating_add(artifact.pre);
    let last = weights.len() - 1;
    weights[last] = weights[last].saturating_add(artifact.post);
    Ok(weights)
}

/// One attention layer's i32 cache at the class's context: `2 × kv_row × n_ctx`.
fn kv_layer_bytes_v1(profile: &PalwShapeProfileV3) -> u64 {
    let row = (profile.attn_kv_heads as u64).saturating_mul(profile.attn_head_dim as u64).saturating_mul(4);
    2u64.saturating_mul(row).saturating_mul(profile.n_ctx as u64)
}

/// One recurrent layer's delta state: every head's `k × v × 4`.
fn gdn_layer_bytes_v1(profile: &PalwShapeProfileV3) -> u64 {
    (profile.gdn_heads as u64).saturating_mul(gdn_delta_head_slice_bytes_v1(profile).unwrap_or(0))
}

/// Can `weights` be cut into at most `parts` contiguous runs, each summing to at most `cap`?
fn fits_under_cap_v1(weights: &[u64], parts: u32, cap: u64) -> bool {
    let mut runs = 1u32;
    let mut sum = 0u64;
    for w in weights {
        if *w > cap {
            return false;
        }
        if sum.saturating_add(*w) > cap {
            runs += 1;
            sum = *w;
        } else {
            sum += *w;
        }
    }
    runs <= parts
}

/// The smallest cap under which `weights` cuts into at most `parts` runs — the min-max contiguous
/// partition, by bisection on the cap.
fn min_max_cap_v1(weights: &[u64], parts: u32) -> u64 {
    let (mut lo, mut hi) = (weights.iter().copied().max().unwrap_or(0), weights.iter().fold(0u64, |a, w| a.saturating_add(*w)));
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if fits_under_cap_v1(weights, parts, mid) { hi = mid } else { lo = mid + 1 }
    }
    lo
}

/// Cut `weights` into exactly `parts` contiguous non-empty runs under `cap` (which
/// [`min_max_cap_v1`] found feasible): greedily as few runs as possible, then split the largest
/// runs at their widest point until there are `parts` of them — a run of two or more layers can
/// always be split without raising the maximum.
fn cut_v1(weights: &[u64], parts: u32, cap: u64) -> Vec<(u16, u16)> {
    let mut runs: Vec<(u16, u16)> = Vec::new();
    let mut start = 0u16;
    let mut sum = 0u64;
    for (i, w) in weights.iter().enumerate() {
        if sum.saturating_add(*w) > cap && i as u16 > start {
            runs.push((start, i as u16 - start));
            start = i as u16;
            sum = *w;
        } else {
            sum += *w;
        }
    }
    runs.push((start, weights.len() as u16 - start));
    while (runs.len() as u32) < parts {
        // Split the heaviest run that has two or more layers.
        let (at, _) = runs
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| *n >= 2)
            .map(|(i, (f, n))| (i, weights[*f as usize..(*f + *n) as usize].iter().sum::<u64>()))
            .max_by_key(|(_, s)| *s)
            .expect("parts <= layers, so some run has two layers");
        let (f, n) = runs[at];
        // The cut that balances the two halves best.
        let mut best = (1u16, u64::MAX);
        let mut left = 0u64;
        let total: u64 = weights[f as usize..(f + n) as usize].iter().sum();
        for k in 1..n {
            left += weights[(f + k - 1) as usize];
            let widest = left.max(total - left);
            if widest < best.1 {
                best = (k, widest);
            }
        }
        runs[at] = (f, best.0);
        runs.insert(at + 1, (f + best.0, n - best.0));
    }
    runs
}

/// **The plan for `shard_count` shards**: the contiguous partition of the layers that minimises
/// the widest shard's weight (artifact plus state at the class's context, with `pre` and `post`
/// pinned to the ends), then each shard's own accounting.
pub fn palw_shard_plan_v1(
    profile: &PalwShapeProfileV3,
    artifact: &PalwArtifactBytesV1,
    shard_count: u32,
) -> Result<PalwShardPlanV1, PalwShardPlanError> {
    if shard_count == 0 {
        return Err(PalwShardPlanError::ZeroShards);
    }
    let weights = layer_weights_v1(profile, artifact)?;
    if shard_count as usize > weights.len() {
        return Err(PalwShardPlanError::TooManyShards { shards: shard_count, layers: profile.layer_count });
    }
    let cap = min_max_cap_v1(&weights, shard_count);
    let runs = cut_v1(&weights, shard_count, cap);
    debug_assert_eq!(runs.len(), shard_count as usize);

    let pre_slots = profile.pre_nodes.len() as u32;
    let post_slots = profile.post_nodes.len() as u32;
    let kv_layer = kv_layer_bytes_v1(profile);
    let gdn_layer = gdn_layer_bytes_v1(profile);
    let mut shards = Vec::with_capacity(runs.len());
    let mut slot = pre_slots;
    for (index, (first_layer, layer_count)) in runs.iter().copied().enumerate() {
        let holds_pre = index == 0;
        let holds_post = index + 1 == runs.len();
        let mut attention_layers = 0u32;
        let mut recurrent_layers = 0u32;
        let mut artifact_bytes = if holds_pre { artifact.pre } else { 0 };
        let mut slot_count = if holds_pre { pre_slots } else { 0 };
        for l in first_layer..first_layer + layer_count {
            match profile.layer_kind(l) {
                PalwLayerKindV1::Attention => attention_layers += 1,
                PalwLayerKindV1::GatedDeltaNet => recurrent_layers += 1,
            }
            artifact_bytes = artifact_bytes.saturating_add(artifact.layers[l as usize]);
            slot_count += profile.layer_table(l).len() as u32;
        }
        if holds_post {
            artifact_bytes = artifact_bytes.saturating_add(artifact.post);
            slot_count += post_slots;
        }
        let first_slot = if holds_pre { 0 } else { slot };
        slot = first_slot + slot_count - if holds_post { post_slots } else { 0 };
        shards.push(PalwShardV1 {
            index: index as u32,
            first_layer,
            layer_count,
            holds_pre,
            holds_post,
            artifact_bytes,
            attention_layers,
            recurrent_layers,
            kv_cache_bytes: u64::from(attention_layers).saturating_mul(kv_layer),
            recurrent_state_bytes: u64::from(recurrent_layers).saturating_mul(gdn_layer),
            first_slot,
            slot_count,
        });
    }
    let widest_seat_bytes = shards.iter().map(PalwShardV1::seat_bytes).max().unwrap_or(0);
    Ok(PalwShardPlanV1 {
        n_ctx: profile.n_ctx,
        shard_count,
        shards,
        boundary_row_bytes: (profile.hidden_dim as u64).saturating_mul(4),
        widest_seat_bytes,
    })
}

/// **The fewest shards a seat of `seat_budget_bytes` can hold one of** — the smallest shard
/// count, up to `max_shards`, whose widest seat fits the budget. The widest seat never grows with
/// the shard count (a shard can always be split), so the first fit is the answer.
pub fn palw_shard_plan_for_seat_v1(
    profile: &PalwShapeProfileV3,
    artifact: &PalwArtifactBytesV1,
    seat_budget_bytes: u64,
    max_shards: u32,
) -> Result<PalwShardPlanV1, PalwShardPlanError> {
    let most = max_shards.min(u32::from(profile.layer_count)).max(1);
    let mut smallest_widest = u64::MAX;
    for shards in 1..=most {
        let plan = palw_shard_plan_v1(profile, artifact, shards)?;
        if plan.widest_seat_bytes <= seat_budget_bytes {
            return Ok(plan);
        }
        smallest_widest = smallest_widest.min(plan.widest_seat_bytes);
    }
    Err(PalwShardPlanError::BudgetTooSmall { budget: seat_budget_bytes, max_shards: most, smallest_widest })
}

// =================================================================================================
// A shard's leaves
// =================================================================================================

/// **The run of main leaves a shard commits at one step** — `(first_leaf_index, count)`, or
/// `None` for a step the job does not have. Contiguous by the enumeration's own order
/// (position-major, then global node slot, then tile), so a shard's interval opening is a range
/// opening over exactly this run at each of the interval's positions.
///
/// `call_index` 0 is the prefill call and `position` its position; a decode call has one
/// position, 0. Post slots exist only at a logits step (the last prefill position and every
/// decode call), so the last shard's run is shorter at the other prefill positions.
pub fn palw_shard_leaf_run_v1(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    shard: &PalwShardV1,
    call_index: u32,
    position: u32,
) -> Option<(u64, u64)> {
    let prefill = context.declared_prefill_tokens as u64;
    let decode_calls = context.exact_decode_tokens.saturating_sub(1) as u64;
    if u64::from(call_index) > decode_calls {
        return None;
    }
    let positions = if call_index == 0 { prefill } else { 1 };
    if u64::from(position) >= positions {
        return None;
    }
    let kv_len = if call_index == 0 { u64::from(position) + 1 } else { prefill + u64::from(call_index) };
    let with_logits = if call_index == 0 { u64::from(position) + 1 == prefill } else { true };
    let slot_count = profile.global_node_count();
    let post_first = slot_count - profile.post_nodes.len() as u32;
    let mut count = 0u64;
    let mut first_slot: Option<u32> = None;
    for slot in shard.first_slot..shard.first_slot + shard.slot_count {
        if slot >= post_first && !with_logits {
            continue;
        }
        let (node, _) = profile.resolve_node_slot(slot)?;
        let len = match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => u64::from(elements),
            PalwStepOutLenV1::KvScaled { multiplier } => u64::from(multiplier).saturating_mul(kv_len),
        };
        let tile = u64::from(node.tile_len.max(1));
        count = count.saturating_add(len.div_ceil(tile));
        first_slot.get_or_insert(slot);
    }
    let Some(first_slot) = first_slot else { return Some((0, 0)) };
    let coord = PalwStepCoordinateV1 { call_index, position, node_slot: first_slot, tile_index: 0 };
    let first = canonical_step_leaf_index(profile, context, &coord)?;
    Some((first, count))
}

/// The table a layer's slots belong to — for a caller that wants to name a shard's first node by
/// `(table, layer, index)` rather than by slot.
pub fn palw_layer_table_v1(profile: &PalwShapeProfileV3, layer: u16) -> PalwStepTableV1 {
    match profile.layer_kind(layer) {
        PalwLayerKindV1::Attention => PalwStepTableV1::Attn,
        PalwLayerKindV1::GatedDeltaNet => PalwStepTableV1::Gdn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_partition_helpers_balance_and_cover() {
        let weights = [5u64, 1, 1, 1, 5, 1, 1, 1, 5];
        assert_eq!(min_max_cap_v1(&weights, 1), 21);
        assert_eq!(min_max_cap_v1(&weights, 3), 7, "5+1+1 | 1+5+1 | 1+1+5 — the cut is not greedy");
        assert_eq!(min_max_cap_v1(&weights, 9), 5);
        for parts in 1..=9u32 {
            let cap = min_max_cap_v1(&weights, parts);
            let runs = cut_v1(&weights, parts, cap);
            assert_eq!(runs.len(), parts as usize);
            assert_eq!(runs[0].0, 0);
            for w in runs.windows(2) {
                assert_eq!(w[0].0 + w[0].1, w[1].0, "contiguous");
            }
            let (f, n) = runs[runs.len() - 1];
            assert_eq!(f + n, weights.len() as u16, "covers the last layer");
            assert!(runs.iter().all(|(_, n)| *n >= 1));
            let widest = runs.iter().map(|(f, n)| weights[*f as usize..(*f + *n) as usize].iter().sum::<u64>()).max().unwrap();
            assert_eq!(widest, cap, "the cut realises the cap it was found for");
        }
    }

    #[test]
    fn scaling_keeps_the_proportions_and_meets_the_total() {
        let a = PalwArtifactBytesV1 { pre: 100, post: 100, layers: vec![300, 500], basis: "t" };
        let s = a.scaled_to_total(2_000);
        assert_eq!(s.total() / 10 * 10, 2_000, "within rounding");
        assert_eq!(s.layers[1] / s.layers[0], 500 / 300, "the proportions are the formula's");
    }
}
