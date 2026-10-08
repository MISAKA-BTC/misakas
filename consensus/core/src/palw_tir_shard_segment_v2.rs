//! **RFC-0006 per-segment pricing (agent SHARD, `docs/design/palw/shard-rfc6-10.md` §4)** — the dormant fence
//! `Params::palw_tir_shard_segment_v2` (`None` on every preset, **refused when armed** until the full-activation release names its
//! height) and the pure rules it puts in force.
//!
//! The armed rules (`palw_tir_shard_v1`, testnet-12 DAA 5,300) price a cell's *work*: a seat locks and is paid
//! `full × share(mask) / 1,000` with the share the work of its cells. They do not price what a cell must *hold*: a late segment's
//! cell needs its layers' whole `Hist` history up to the segment's end, an early one only the rows before it. Past this fence:
//!
//! * **the resident table** ([`palw_tir_shard_cell_resident_permille_v2`]): per cell `(i, j)` the shard's weight (its layers'
//!   params and `Fixed` state, `pre`/`post`/globals as RFC §1.1) plus its layers' `Hist` rows up to segment `j`'s end on the
//!   class's canonical job — as permille of the sum over cells, summing to exactly 1,000. A pure function of the registered program
//!   and the plan, derived when it is read: no new rooted state, so the armed records' encodings are untouched;
//! * **the price share** ([`palw_tir_shard_price_share_v2`]): `max(Σ work, Σ resident)` over the seat's cells — the lock a counted
//!   signer posts and the pay weight the bind fixes (`drawn_permille`) both read it, keyed on the claim's acceptance (immutable)
//!   so the two agree. The floor and the slash term are unchanged: the deterrent does not shrink with the cell;
//! * **readiness is the heaviest cell** (node policy): a node files a shard's possession proof only when its host can hold
//!   [`palw_tir_shard_seat_need_bytes_v2`] — the shard's heaviest cell — so every ready seat can host every cell of its shard and the
//!   draw needs no tier (a tier is a capacity claim the chain cannot prove, `shard-rfc6-10.md` §4.4).

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_tir_shard_v1::{
    PALW_TIR_SHARD_SEAT_FLOOR_PERMILLE_V1, PalwTirShardError, PalwTirShardPlanV1, palw_largest_remainder_permille_v1,
    palw_tir_cells_share_permille_v1, palw_tir_shard_assignment_v1, palw_tir_shard_canonical_facts_v1,
    palw_tir_shard_outsider_mask_v1, palw_tir_shard_partition_v1, palw_tir_shard_weights_v1,
};
use crate::palw_verification_v2::PalwSegmentMaskV2;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::program::{Ref, StateKind};

/// A lane is 4 bytes held whatever its dtype (PALW-TIR-5), as `palw_tir_shard_weights_v1` counts it.
const LANE_BYTES: u128 = 4;

/// **What each cell of a plan must hold**, in bytes, shard-major then segment (`s_l × s_p` entries): the shard's weight with its
/// layers' per-layer `Hist` state counted only up to the segment's end on the class's canonical job
/// ([`palw_tir_shard_canonical_facts_v1`]), every other term as `palw_tir_shard_weights_v1` counts it. A global `Hist` state a
/// layer reads stays counted whole (it is no layer's to shard).
pub fn palw_tir_shard_cell_resident_bytes_v2(
    p: &TirProgramV1,
    max_context: u32,
    s_l: u16,
    s_p: u16,
) -> Result<Vec<u128>, PalwTirShardError> {
    if s_p == 0 {
        return Err(PalwTirShardError::PositionSegmentsNotOffered(s_p));
    }
    let weights = palw_tir_shard_weights_v1(p, max_context);
    let layers = weights.layer_bytes.len();
    let parts = palw_tir_shard_partition_v1(&weights, s_l).ok_or(PalwTirShardError::MoreShardsThanLayers { shards: s_l, layers })?;
    // Per layer: every per-layer `Hist` state its block touches, as (elements a row, window), each once — the terms of
    // `palw_tir_shard_weights_v1`'s `layer_bytes` that grow with the position.
    let mut hist: Vec<Vec<(u128, u32)>> = vec![Vec::new(); layers];
    for (l, bi) in p.schedule.layers.iter().enumerate() {
        let mut seen: Vec<u16> = Vec::new();
        for n in &p.blocks[*bi as usize].nodes {
            let mut touch = |j: u16| {
                if !seen.contains(&j) {
                    seen.push(j);
                }
            };
            for r in &n.inputs {
                if let Ref::State(j) = r {
                    touch(*j);
                }
            }
            if let misaka_palw_tir::Prim::StateWrite { state } | misaka_palw_tir::Prim::HistAppend { state } = n.prim {
                touch(state);
            }
        }
        for j in seen {
            let d = &p.states[j as usize];
            if d.per_layer
                && let StateKind::Hist { window } = d.kind
            {
                let elements: u128 = d.shape.iter().map(|x| *x as u128).product::<u128>().max(1);
                hist[l].push((elements, window));
            }
        }
    }
    let rows_at = |window: u32, positions: u128| -> u128 { (window as u128).min(max_context as u128).min(positions).max(1) };
    let facts = palw_tir_shard_canonical_facts_v1(max_context);
    let positions = u128::from(facts.prefill_tokens) + u128::from(facts.generated_tokens) - 1;
    let mut out = Vec::with_capacity(usize::from(s_l) * usize::from(s_p));
    for range in &parts {
        let whole = weights.of_range(range.start, range.end);
        let full_hist: u128 = hist[range.clone()]
            .iter()
            .flatten()
            .map(|(e, w)| e.saturating_mul(rows_at(*w, positions)).saturating_mul(LANE_BYTES))
            .fold(0, u128::saturating_add);
        let fixed = whole.saturating_sub(full_hist);
        for j in 0..u128::from(s_p) {
            let end = positions * (j + 1) / u128::from(s_p);
            let held: u128 = hist[range.clone()]
                .iter()
                .flatten()
                .map(|(e, w)| e.saturating_mul(rows_at(*w, end)).saturating_mul(LANE_BYTES))
                .fold(0, u128::saturating_add);
            out.push(fixed.saturating_add(held));
        }
    }
    Ok(out)
}

/// **The resident table** — each cell's share of what the plan's cells hold, permille, summing to exactly 1,000 (largest
/// remainder), in the plan's own cell order (`PalwTirShardPlanV1::cell_permille`'s).
pub fn palw_tir_shard_cell_resident_permille_v2(
    p: &TirProgramV1,
    max_context: u32,
    s_l: u16,
    s_p: u16,
) -> Result<Vec<u16>, PalwTirShardError> {
    Ok(palw_largest_remainder_permille_v1(&palw_tir_shard_cell_resident_bytes_v2(p, max_context, s_l, s_p)?))
}

/// **A seat's price share** past the fence: `max(Σ work, Σ resident)` over the cells of `shard` its `mask` names (permille of the
/// claim), what its lock (`palw_tir_shard_lock_v1`, floored there) and its pay weight are scaled by.
pub fn palw_tir_shard_price_share_v2(work: &[u16], resident: &[u16], s_p: u16, shard: u16, mask: PalwSegmentMaskV2) -> u32 {
    palw_tir_cells_share_permille_v1(work, s_p, shard, mask).max(palw_tir_cells_share_permille_v1(resident, s_p, shard, mask))
}

/// **Each stored seat's price share**, in stored panel order (`[outsider?] ++ class seats` a shard) — the pay weights a bind past the
/// fence fixes in `PalwTirShardClaimV1::drawn_permille` (the field the armed record has; only its values change).
pub fn palw_tir_shard_drawn_price_permille_v2(
    plan: &PalwTirShardPlanV1,
    resident: &[u16],
    seed: &Hash64,
    claim_id: &Hash64,
    outsider: bool,
) -> Vec<u32> {
    let mut out = Vec::new();
    for shard in 0..plan.s_l {
        if outsider {
            out.push(palw_tir_shard_price_share_v2(
                &plan.cell_permille,
                resident,
                plan.s_p,
                shard,
                palw_tir_shard_outsider_mask_v1(plan.s_p),
            ));
        }
        for mask in palw_tir_shard_assignment_v1(seed, claim_id, shard, plan.s_p) {
            out.push(palw_tir_shard_price_share_v2(&plan.cell_permille, resident, plan.s_p, shard, mask));
        }
    }
    out
}

/// **What a seat of `shard` must be able to hold** (node policy past the fence): the shard's heaviest cell, in bytes — the last
/// segment's, whose history reaches the canonical job's end. A node files the shard's possession proof only when its host can.
pub fn palw_tir_shard_seat_need_bytes_v2(
    p: &TirProgramV1,
    max_context: u32,
    s_l: u16,
    s_p: u16,
    shard: u16,
) -> Result<u128, PalwTirShardError> {
    let cells = palw_tir_shard_cell_resident_bytes_v2(p, max_context, s_l, s_p)?;
    let from = usize::from(shard) * usize::from(s_p);
    cells
        .get(from..from + usize::from(s_p))
        .and_then(|row| row.iter().copied().max())
        .ok_or(PalwTirShardError::ShardCountOutOfRange(s_l))
}

/// The floor every price share is lifted to before it scales a lock (`palw_tir_shard_lock_v1`): unchanged by this fence.
pub const PALW_TIR_SHARD_SEGMENT_SEAT_FLOOR_PERMILLE_V2: u32 = PALW_TIR_SHARD_SEAT_FLOOR_PERMILLE_V1;

// ---------------------------------------------------------------------------------------------
// The fence: None on every preset, refused when armed (the full-activation release names its height)
// ---------------------------------------------------------------------------------------------

impl Params {
    /// `palw_tir_shard_segment_v2`, resolved: `Some` only on a `ConsensusV2` network with a real height (a `never()` value is dormant).
    pub fn palw_tir_shard_segment_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_tir_shard_segment_v2.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Is per-segment pricing in force at `daa_score`?** `false` on every preset.
    pub fn palw_tir_shard_segment_active_at(&self, daa_score: u64) -> bool {
        self.palw_tir_shard_segment_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`tir_shard_segment_from_daa`), which the fold reads. Written here and
    /// nothing else; `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_tir_shard_segment_v2(&mut self) {
        let from_daa = self.palw_tir_shard_segment_v2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_tir_shard_segment_from_daa(from_daa);
        }
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`]: a V2 bundle whose mirror is not the fence's; arming on a ruleset
    /// that is not `ConsensusV2`; arming without `palw_tir_shard_v1` in force at or below it; and — until the full-activation release
    /// names its height (the Lead's registry) — **any arming at all**. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_tir_shard_segment_v2(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.tir_shard_segment_from_daa(),
            _ => None,
        };
        let armed = self.palw_tir_shard_segment_v2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_segment_v2 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_tir_shard_segment_v2 \
                 after the bundle is assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_tir_shard_segment_v2 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if !self.palw_tir_shard_v1.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_segment_v2 needs palw_tir_shard_v1 in force at or below it: it prices the cells of claims drawn per shard",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_tir_shard_segment_v2 cannot be armed yet: per-segment pricing changes the lock and the pay of every sharded cell, and \
             its height is the full-activation release's to name (agent SHARD, docs/design/palw/shard-rfc6-10.md §4)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_price_share_is_the_larger_of_work_and_residency_over_the_mask() {
        // Two shards, two segments: work rises with the segment (attention), residency rises faster on the late segment.
        let work = [200, 300, 200, 300];
        let resident = [150, 350, 150, 350];
        let full = PalwSegmentMaskV2::full(2);
        assert_eq!(palw_tir_shard_price_share_v2(&work, &resident, 2, 0, full), 500);
        assert_eq!(palw_tir_shard_price_share_v2(&work, &resident, 2, 0, PalwSegmentMaskV2::single(0)), 200);
        assert_eq!(palw_tir_shard_price_share_v2(&work, &resident, 2, 1, PalwSegmentMaskV2::single(1)), 350);
    }
}
