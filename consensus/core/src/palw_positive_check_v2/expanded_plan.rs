//! Pure expanded geometry for a later exact-checking path. This module produces
//! neither checked computation nor native admission/credit authority. It keeps
//! the registered program, dtype, context and legacy leaf numbering unchanged.
//!
//! The initial checker remains separate: this plan also enumerates every lane
//! of uncommitted nodes, including i64/i128 intermediates. No wide value is
//! reinterpreted as a four-byte legacy step leaf. Prefix metadata is O(positions),
//! not O(relations); pure derivation refuses a job before its position loops if
//! its complete structural visit/metadata bounds cannot be reserved. No sampling
//! is used; producing a plan does not admit a native claim.

use crate::{
    Hash64,
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_step_v1::{PalwTirJobShapeV1, PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepSpaceV1},
    palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};
use misaka_palw_tir::{
    DType, Dim,
    admit::{CostV1, node_cost},
    demand::DemandContext,
    program::{MAX_BLOCKS, MAX_NODES_PER_BLOCK, MAX_PROGRAM_BYTES, MAX_STATES, TIR_PROGRAM_VERSION_V1},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpandedPlanLimits {
    pub max_positions: u32,
    pub max_geometry_visits: u64,
    /// Logical backing allocation for position-prefix metadata only; excludes
    /// the separately admitted registered program/step space and physical RSS.
    pub max_plan_bytes: u64,
    pub max_legacy_leaves: u64,
    pub max_relations: u64,
    /// Aggregate lane targets, including state/checkpoint/history relations.
    pub max_target_lanes: u64,
    /// One result vector uses i128 storage even for a narrow target dtype.
    /// This excludes source, decoder, evaluator and program-analysis workspace.
    pub max_peak_target_bytes: u64,
    pub uncommitted_tile_lanes: u32,
    pub max_elements_per_relation: u64,
    pub max_terms_per_relation: u64,
    pub max_total_elements: u64,
    pub max_total_terms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpandedPlanError {
    Binding,
    UnsupportedProfile,
    Budget,
    Overflow,
    Geometry,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExpandedSummary {
    pub positions: u32,
    pub geometry_visits: u64,
    pub geometry_visit_bound: u64,
    pub plan_metadata_bytes: u64,
    pub node_instances: u64,
    pub relations: u64,
    pub legacy_relations: u64,
    pub uncommitted_relations: u64,
    pub target_lanes: u64,
    pub node_lanes: u64,
    pub state_lanes: u64,
    pub hist_lanes: u64,
    pub peak_target_bytes: u64,
    pub reserved_elements: u64,
    pub reserved_terms: u64,
    /// Existing structural node_cost summed once per executed node/position.
    /// Diagnostic work only: not reward accounting or exact future check cost.
    pub structural_work: CostV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpandedRelation {
    pub index: u64,
    /// Number of legacy leaves preceding this relation in the geometry. This
    /// number does not assert that any leaf has actually been checked. A source
    /// factory must rederive it from the plan and require Node-checked progress;
    /// peer-constructed relation DTOs are not authentication authority.
    pub legacy_frontier: u64,
    pub kind: ExpandedRelationKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpandedRelationKind {
    Legacy(PalwTirLeafV1),
    RecomputeNode {
        ctx: DemandContext,
        block: u8,
        layer: Option<u16>,
        node: u16,
        dtype: DType,
        shape: Vec<usize>,
        first_lane: u64,
        lane_count: u32,
    },
}

#[derive(Clone, Copy, Debug, Default)]
struct PositionPrefix {
    relations: u64,
    legacy: u64,
}

/// Node-derived geometry only. There is deliberately no Deserialize, peer
/// schedule constructor, mutable cursor or successful-computation constructor.
pub struct ExpandedPlan {
    space: PalwTirStepSpaceV1,
    job: PalwJobContextV2,
    shape: PalwTirJobShapeV1,
    limits: ExpandedPlanLimits,
    prefixes: Vec<PositionPrefix>,
    summary: ExpandedSummary,
}

impl ExpandedPlan {
    /// Original geometry for read-only builders. This reference is not a
    /// checked-prefix, arithmetic-success or native-admission capability.
    pub fn step_space(&self) -> &PalwTirStepSpaceV1 {
        &self.space
    }
    /// Exact original job used to derive this geometry, without substitution.
    pub fn job_context(&self) -> &PalwJobContextV2 {
        &self.job
    }
    pub fn derive(
        registered: &PalwTirClassV1,
        artifact: Hash64,
        job: &PalwJobContextV2,
        limits: ExpandedPlanLimits,
    ) -> Result<Self, ExpandedPlanError> {
        if job.version != PALW_TRACE_COMMITMENT_VERSION_V2 || registered.version != PALW_TIR_CLASS_VERSION_V1 {
            return Err(ExpandedPlanError::UnsupportedProfile);
        }
        // Format-level bounds precede class_id's program hashing/layout Borsh
        // serialization, so foreign typed DTOs cannot force unbounded extra work.
        // The registered native class still needs independent rooted resolution.
        if registered.program.len() > MAX_PROGRAM_BYTES
            || registered.layout.commit_tiles.len() > MAX_BLOCKS * MAX_NODES_PER_BLOCK
            || registered.layout.state_tiles.len() > MAX_STATES
        {
            return Err(ExpandedPlanError::Binding);
        }
        if job.network_id.is_empty()
            || job.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES
            || job.declared_prefill_tokens == 0
            || job.exact_decode_tokens == 0
            || job.declared_prefill_tokens.checked_add(job.exact_decode_tokens).is_none_or(|n| n > job.max_context_tokens)
            || registered.class_id(&artifact) != job.shape_profile_id
        {
            return Err(ExpandedPlanError::Binding);
        }
        // The registered tokenizer remains part of class identity. The job's
        // tokenizer is a distinct execution field (canonical jobs may use 0).
        // FullRelationPreparation binds the actual job/context and its tokenizer
        // to the independently rooted, signed Result; never rewrite either DTO.
        let positions = job.declared_prefill_tokens.checked_add(job.exact_decode_tokens - 1).ok_or(ExpandedPlanError::Overflow)?;
        if positions > limits.max_positions
            || limits.uncommitted_tile_lanes == 0
            || limits.max_relations == 0
            || limits.max_target_lanes == 0
            || limits.max_peak_target_bytes == 0
            || limits.max_elements_per_relation == 0
            || limits.max_terms_per_relation == 0
        {
            return Err(ExpandedPlanError::Budget);
        }
        let space = PalwTirStepSpaceV1::new(registered).map_err(|_| ExpandedPlanError::Binding)?;
        let program = &space.program;
        if program.version != TIR_PROGRAM_VERSION_V1
            || Hash64::from_bytes(program.logits_scheme_id) != tiled_logits_scheme_id_v1()
            || job.trace_scheme_id != tiled_logits_scheme_id_v1()
            || program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.dtype != DType::I32
        {
            return Err(ExpandedPlanError::UnsupportedProfile);
        }
        let shape = space.job_shape(job).map_err(|_| ExpandedPlanError::Binding)?;
        let legacy_count = space.leaf_count_capped(job, limits.max_legacy_leaves).map_err(|_| ExpandedPlanError::Budget)?;
        if legacy_count == 0 {
            return Err(ExpandedPlanError::Geometry);
        }
        // Bound every upcoming occurrence/node/state visit before entering a
        // position loop or allocating its prefix vector. The post-skip makes
        // actual visits no larger; skipped state intervals also cost no visits.
        let mut per_position = usize_u64(space.occurrences().len())?;
        for (block, _) in space.occurrences() {
            per_position = add(per_position, usize_u64(program.blocks[*block as usize].nodes.len())?)?;
        }
        per_position = add(per_position, usize_u64(space.fixed_instances().len())?)?;
        per_position = add(per_position, usize_u64(space.hist_instances().len())?)?;
        let bound = mul(u64::from(positions), per_position)?;
        if bound > limits.max_geometry_visits {
            return Err(ExpandedPlanError::Budget);
        }
        let prefix_count = add(u64::from(positions), 1)?;
        let plan_metadata_bytes = mul(prefix_count, std::mem::size_of::<PositionPrefix>() as u64)?;
        if plan_metadata_bytes > limits.max_plan_bytes {
            return Err(ExpandedPlanError::Budget);
        }
        let prefix_len = usize::try_from(prefix_count).map_err(|_| ExpandedPlanError::Overflow)?;
        let mut prefixes = Vec::new();
        prefixes.try_reserve_exact(prefix_len).map_err(|_| ExpandedPlanError::Budget)?;
        let plan_metadata_bytes = mul(usize_u64(prefixes.capacity())?, std::mem::size_of::<PositionPrefix>() as u64)?;
        if plan_metadata_bytes > limits.max_plan_bytes {
            return Err(ExpandedPlanError::Budget);
        }
        prefixes.push(PositionPrefix::default());
        let mut summary = ExpandedSummary { positions, geometry_visit_bound: bound, plan_metadata_bytes, ..Default::default() };
        for position in 0..positions {
            let legacy_before = summary.legacy_relations;
            for (occurrence, (block, _)) in space.occurrences().iter().enumerate() {
                if occurrence + 1 == space.occurrences().len() && !shape.runs_post(position) {
                    continue;
                }
                summary.geometry_visits = add(summary.geometry_visits, 1)?;
                let h = h_at(&space, *block, position);
                for (node_index, node) in program.blocks[*block as usize].nodes.iter().enumerate() {
                    summary.geometry_visits = add(summary.geometry_visits, 1)?;
                    summary.node_instances = add(summary.node_instances, 1)?;
                    let lanes = elements(&node.out.shape, h)?;
                    let tile = if node.commit {
                        u64::from(space.commit_tile_len(*block, node_index as u16).ok_or(ExpandedPlanError::Geometry)?)
                    } else {
                        u64::from(limits.uncommitted_tile_lanes)
                    };
                    add_targets(&mut summary, lanes, tile, node.commit, &limits)?;
                    summary.node_lanes = add(summary.node_lanes, lanes)?;
                    let cost = node_cost(program, usize::from(*block), node_index, h);
                    checked_cost_add(&mut summary.structural_work, cost)?;
                }
            }
            if (position + 1).is_multiple_of(space.layout.checkpoint_interval) {
                for state in space.fixed_instances() {
                    summary.geometry_visits = add(summary.geometry_visits, 1)?;
                    add_targets(&mut summary, state.elements, u64::from(state.tile_lanes), true, &limits)?;
                    summary.state_lanes = add(summary.state_lanes, state.elements)?;
                }
            }
            if (position + 1).is_multiple_of(space.layout.h_tile) {
                for state in space.hist_instances() {
                    summary.geometry_visits = add(summary.geometry_visits, 1)?;
                    // Hist leaves tile lanes WITHIN a row, then concatenate
                    // h_tile rows; do not tile their combined flattened size.
                    let tiles = state.elements.div_ceil(u64::from(state.tile_lanes));
                    let lanes = mul(state.elements, u64::from(space.layout.h_tile))?;
                    let widest = mul(state.elements.min(u64::from(state.tile_lanes)), u64::from(space.layout.h_tile))?;
                    add_relation_counts(&mut summary, tiles, lanes, widest, true, &limits)?;
                    summary.hist_lanes = add(summary.hist_lanes, lanes)?;
                }
            }
            if summary.legacy_relations.checked_sub(legacy_before) != Some(space.leaves_at_position(&shape, position)) {
                return Err(ExpandedPlanError::Geometry);
            }
            prefixes.push(PositionPrefix { relations: summary.relations, legacy: summary.legacy_relations });
        }
        if summary.legacy_relations != legacy_count || summary.geometry_visits > bound {
            return Err(ExpandedPlanError::Geometry);
        }
        summary.reserved_elements = mul(summary.relations, limits.max_elements_per_relation)?;
        summary.reserved_terms = mul(summary.relations, limits.max_terms_per_relation)?;
        if summary.reserved_elements > limits.max_total_elements || summary.reserved_terms > limits.max_total_terms {
            return Err(ExpandedPlanError::Budget);
        }
        Ok(Self { space, job: job.clone(), shape, limits, prefixes, summary })
    }

    pub fn summary(&self) -> ExpandedSummary {
        self.summary
    }

    /// Lookup retains no previous relation or tensor value. Its node/state
    /// traversal is bounded by the same admitted per-position geometry.
    pub fn relation_at(&self, index: u64) -> Option<ExpandedRelation> {
        if index >= self.summary.relations {
            return None;
        }
        let position = self.prefixes.partition_point(|prefix| prefix.relations <= index).checked_sub(1)?;
        let prefix = *self.prefixes.get(position)?;
        let position = u32::try_from(position).ok()?;
        let mut offset = index.checked_sub(prefix.relations)?;
        let mut legacy = prefix.legacy;
        for (occurrence, (block, layer)) in self.space.occurrences().iter().enumerate() {
            if occurrence + 1 == self.space.occurrences().len() && !self.shape.runs_post(position) {
                continue;
            }
            let h = h_at(&self.space, *block, position);
            for (node_index, node) in self.space.program.blocks[*block as usize].nodes.iter().enumerate() {
                let lanes = elements(&node.out.shape, h).ok()?;
                let tile = if node.commit {
                    u64::from(self.space.commit_tile_len(*block, node_index as u16)?)
                } else {
                    u64::from(self.limits.uncommitted_tile_lanes)
                };
                let tiles = lanes.div_ceil(tile);
                if offset < tiles {
                    if node.commit {
                        let old_index = legacy.checked_add(offset)?;
                        let leaf = self.space.leaf_at(&self.job, old_index)?;
                        if leaf.position != position
                            || !matches!(leaf.kind, PalwTirLeafKindV1::Commit { occurrence: o, node: n, .. }
                                if o as usize == occurrence && n as usize == node_index)
                        {
                            return None;
                        }
                        return Some(ExpandedRelation { index, legacy_frontier: old_index, kind: ExpandedRelationKind::Legacy(leaf) });
                    }
                    let first_lane = offset.checked_mul(tile)?;
                    let lane_count = u32::try_from(lanes.checked_sub(first_lane)?.min(tile)).ok()?;
                    let shape = node
                        .out
                        .shape
                        .iter()
                        .map(|dim| match dim {
                            Dim::Fixed(n) => usize::try_from(*n).ok(),
                            Dim::H => usize::try_from(h).ok(),
                        })
                        .collect::<Option<Vec<_>>>()?;
                    return Some(ExpandedRelation {
                        index,
                        legacy_frontier: legacy,
                        kind: ExpandedRelationKind::RecomputeNode {
                            ctx: DemandContext { pos: position, occurrence: u16::try_from(occurrence).ok()? },
                            block: *block,
                            layer: *layer,
                            node: u16::try_from(node_index).ok()?,
                            dtype: node.out.dtype,
                            shape,
                            first_lane,
                            lane_count,
                        },
                    });
                }
                offset = offset.checked_sub(tiles)?;
                if node.commit {
                    legacy = legacy.checked_add(tiles)?;
                }
            }
        }
        let old_index = legacy.checked_add(offset)?;
        let leaf = self.space.leaf_at(&self.job, old_index)?;
        if leaf.position != position || matches!(leaf.kind, PalwTirLeafKindV1::Commit { .. }) {
            return None;
        }
        Some(ExpandedRelation { index, legacy_frontier: old_index, kind: ExpandedRelationKind::Legacy(leaf) })
    }

    /// Iterate the immutable, successfully derived geometry. An internal gap
    /// would violate the checked prefix/count invariant and is asserted rather
    /// than silently truncating coverage; no peer-supplied indices are iterated.
    pub fn iter(&self) -> impl Iterator<Item = ExpandedRelation> + '_ {
        (0..self.summary.relations).map(|index| self.relation_at(index).expect("validated expanded relation geometry"))
    }
}

fn h_at(space: &PalwTirStepSpaceV1, block: u8, position: u32) -> u64 {
    space.info.blocks[usize::from(block)].window.map_or(1, |window| (u64::from(position) + 1).min(u64::from(window)))
}

fn elements(shape: &[Dim], h: u64) -> Result<u64, ExpandedPlanError> {
    shape.iter().try_fold(1u64, |count, dim| {
        mul(
            count,
            match dim {
                Dim::Fixed(n) => u64::from(*n),
                Dim::H => h,
            },
        )
    })
}

fn add_targets(
    summary: &mut ExpandedSummary,
    lanes: u64,
    tile: u64,
    legacy: bool,
    limits: &ExpandedPlanLimits,
) -> Result<(), ExpandedPlanError> {
    if lanes == 0 || tile == 0 {
        return Err(ExpandedPlanError::Geometry);
    }
    add_relation_counts(summary, lanes.div_ceil(tile), lanes, lanes.min(tile), legacy, limits)
}

fn add_relation_counts(
    summary: &mut ExpandedSummary,
    relations: u64,
    lanes: u64,
    widest: u64,
    legacy: bool,
    limits: &ExpandedPlanLimits,
) -> Result<(), ExpandedPlanError> {
    summary.relations = add(summary.relations, relations)?;
    summary.target_lanes = add(summary.target_lanes, lanes)?;
    if legacy {
        summary.legacy_relations = add(summary.legacy_relations, relations)?;
    } else {
        summary.uncommitted_relations = add(summary.uncommitted_relations, relations)?;
    }
    summary.peak_target_bytes = summary.peak_target_bytes.max(mul(widest, std::mem::size_of::<i128>() as u64)?);
    if summary.relations > limits.max_relations
        || summary.target_lanes > limits.max_target_lanes
        || summary.legacy_relations > limits.max_legacy_leaves
        || summary.peak_target_bytes > limits.max_peak_target_bytes
    {
        return Err(ExpandedPlanError::Budget);
    }
    Ok(())
}

fn checked_cost_add(total: &mut CostV1, cost: CostV1) -> Result<(), ExpandedPlanError> {
    // Existing node_cost saturates internal products. Refuse a saturated
    // component rather than accepting it as an exact structural diagnostic.
    if [cost.macs, cost.elementwise, cost.transcendentals, cost.bytes_read, cost.bytes_written].contains(&u64::MAX) {
        return Err(ExpandedPlanError::Overflow);
    }
    total.macs = add(total.macs, cost.macs)?;
    total.elementwise = add(total.elementwise, cost.elementwise)?;
    total.transcendentals = add(total.transcendentals, cost.transcendentals)?;
    total.bytes_read = add(total.bytes_read, cost.bytes_read)?;
    total.bytes_written = add(total.bytes_written, cost.bytes_written)?;
    Ok(())
}
fn add(a: u64, b: u64) -> Result<u64, ExpandedPlanError> {
    a.checked_add(b).ok_or(ExpandedPlanError::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, ExpandedPlanError> {
    a.checked_mul(b).ok_or(ExpandedPlanError::Overflow)
}
fn usize_u64(value: usize) -> Result<u64, ExpandedPlanError> {
    u64::try_from(value).map_err(|_| ExpandedPlanError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_and_structural_overflows_refuse() {
        assert_eq!(add(u64::MAX, 1), Err(ExpandedPlanError::Overflow));
        assert_eq!(mul(u64::MAX, 2), Err(ExpandedPlanError::Overflow));
        assert_eq!(elements(&[Dim::Fixed(u32::MAX), Dim::Fixed(u32::MAX), Dim::H], 2), Err(ExpandedPlanError::Overflow));
        let mut total = CostV1 { macs: u64::MAX - 1, ..Default::default() };
        assert_eq!(checked_cost_add(&mut total, CostV1 { macs: 2, ..Default::default() }), Err(ExpandedPlanError::Overflow));
        assert_eq!(
            checked_cost_add(&mut CostV1::default(), CostV1 { elementwise: u64::MAX, ..Default::default() }),
            Err(ExpandedPlanError::Overflow)
        );
    }
}
