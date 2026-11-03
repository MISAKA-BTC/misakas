//! Native numerical input geometry for Batch admission.
use crate::{
    Hash64,
    palw_batch_court_v1::{
        BatchRangeAuthenticationV1, BatchRangeV1, MAX_BATCH_RANGES_V1,
        carriage::{SignedBatchDemandV1, quote_batch_open_v1},
    },
    palw_court_shared_material_intake_v1::{
        MATERIAL_PIPELINED_MAX_BYTES, MaterialTargetV1, parameter_body_bound_v1, prompt_tile_bound,
    },
    palw_court_tir_requests_v1::TirSharedRequestSetV1,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
        reserve_native_requests_v1,
    },
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1},
    palw_state_v2::PalwChainStateV2,
    palw_tir_court_v1::parameter_cover::RootInputBoundaryV1,
    palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    palw_tir_public_source_v1::{
        PalwTirPublicRequestV1, PalwTirPublicSourceV1,
        parameter_cover_v1::{OriginalParameterCoverBudgetV1, OriginalRootInputCoverV1, StreamingParameterCoverBudgetV1},
    },
};
use misaka_palw_tir::program::TirProgramV1;
use std::collections::{BTreeMap, BTreeSet};

type E = NativeDecodeErrorV1<String>;

/// Maximum byte cap per canonical range position across all numerical roots.
/// This covers one selected root's inputs, including the root leaf. It does
/// not price later turns, native verdicts, or simultaneous independent roots.
pub(crate) struct NumericalBatchEnvelopeV1 {
    pub scope: Hash64,
    maximums: Vec<u64>,
}

impl NumericalBatchEnvelopeV1 {
    pub(crate) fn maximums_v1(&self) -> &[u64] {
        &self.maximums
    }

    /// The same position boundaries apply to every selected root, even when
    /// individual payloads are smaller. A shorter root uses a prefix.
    pub(crate) fn demand_ranges_v1(&self) -> impl Iterator<Item = &[u64]> {
        partition_maximums_v1(&self.maximums)
    }
}

/// A selected numerical root's targets in one originally reserved demand.
pub(crate) struct NumericalBatchRootDemandV1 {
    claim: Hash64,
    scope: Hash64,
    ordinal: u64,
    ranges: Vec<(MaterialTargetV1, u64)>,
}

impl NumericalBatchRootDemandV1 {
    pub(crate) fn ranges_v1(&self) -> &[(MaterialTargetV1, u64)] {
        &self.ranges
    }

    /// Build the unsigned carrier for the accepted notice. Actual encoded
    /// body lengths come from the responder and must match the selected targets.
    pub(crate) fn quote_open_v1(
        &self,
        state: &PalwChainStateV2,
        notice: Hash64,
        lengths: &[(MaterialTargetV1, u64)],
    ) -> Result<NativeRetainedV1<SignedBatchDemandV1>, E> {
        let context = crate::palw_court_shared_tir_state_v1::context(state, self.claim).map_err(E::Decode)?;
        if context.scope_id != self.scope {
            return Err(E::Decode("numerical Batch demand belongs to another scope".into()));
        }
        check_material_lengths_v1(&self.ranges, lengths).map_err(E::Decode)?;
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<BatchRangeV1>(lengths.len() as u64).map_err(E::Resource)?;
        q.exact_v1::<SignedBatchDemandV1>(1).map_err(E::Resource)?;
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        let mut ranges = Vec::with_capacity(lengths.len());
        for &(target, bytes) in lengths {
            ranges.push(BatchRangeV1 {
                id: target.id_v1(self.claim).map_err(E::Decode)?,
                bytes: bytes as u32,
                authentication: BatchRangeAuthenticationV1::WholeRange,
            });
        }
        let open = quote_batch_open_v1(state, self.claim, notice, self.ordinal, ranges).map_err(E::Decode)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(open, lease))
    }
}

fn check_material_lengths_v1<T: Eq>(planned: &[(T, u64)], actual: &[(T, u64)]) -> Result<(), String> {
    if !(1..=MAX_BATCH_RANGES_V1).contains(&planned.len()) || planned.len() != actual.len() {
        return Err("numerical Batch response must name every selected target".into());
    }
    if planned
        .iter()
        .zip(actual)
        .any(|((expected, cap), (target, bytes))| expected != target || *bytes == 0 || *bytes > *cap || *bytes > u64::from(u32::MAX))
    {
        return Err("numerical Batch target or exact body length differs from its plan".into());
    }
    Ok(())
}

/// Build an unsigned numerical Open against one accepted branch snapshot.
/// The caller supplies its profile's enumeration cap and actual encoded body
/// lengths in canonical target order. The result carries no signing authority.
pub fn quote_numerical_batch_open_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    notice: Hash64,
    leaf: u64,
    demand_index: usize,
    maximum_ranges: usize,
    lengths: &[(MaterialTargetV1, u64)],
) -> Result<NativeRetainedV1<SignedBatchDemandV1>, NativeDecodeErrorV1<String>> {
    use crate::palw_batch_court_v1::{carriage::batch_open_wire_bytes_v1, rooted::reserved_open_slot_v1};
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    let context = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(E::Decode)?;
    let notice_key = Key::CourtSharedQuery(notice);
    let Some(Row::CourtSharedQuery(notice_row)) = state.staged_court_row_v1(&notice_key) else {
        return Err(E::Decode("numerical Batch has no accepted notice".into()));
    };
    notice_row.check(&notice_key).map_err(E::Decode)?;
    if notice_row.claim != claim
        || notice_row.scope_id != context.scope_id
        || notice_row.material_until_daa != context.material_until_daa
        || state.court_shared_initial_notice_daa_v1(claim) != Some(notice_row.opened_daa)
        || state.staged_court_row_v1(&Key::CourtBatchDemand(claim, demand_index as u64)).is_some()
    {
        return Err(E::Decode("numerical Batch notice or opening differs from the accepted case".into()));
    }
    let wire_bytes = batch_open_wire_bytes_v1(lengths.len()).map_err(E::Decode)?;
    let (_, _, opened_daa) =
        reserved_open_slot_v1(state, claim, context.scope_id, demand_index as u64, wire_bytes).map_err(E::Decode)?;
    if state.last_point().is_some_and(|point| point.daa_score > opened_daa) {
        return Err(E::Decode("numerical Batch opening slot has passed".into()));
    }

    let frame = super::super::frame_of(state, claim).map_err(E::Decode)?;
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or_else(|| E::Decode("numerical Batch class is absent".into()))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
        .ok_or_else(|| E::Decode("numerical Batch layout is absent".into()))?;
    let source = PalwTirPublicSourceV1::new_native_v1(
        record,
        layout,
        frame.header_v1(),
        frame.original_v1().execution_root,
        frame.original_v1().trace_root,
        frame.descriptor_v1().maximum_leaves,
    )?;
    let budget = super::super::material_intake_schedule_v1()
        .map_err(|_| E::Decode("numerical Batch material schedule is absent".into()))?
        .cover;
    let program =
        crate::palw_native_resource_v1::decode_native_tir_program_v1(record.program.as_slice()).map_err(|err| match err {
            NativeDecodeErrorV1::Decode(why) => E::Decode(why.to_string()),
            NativeDecodeErrorV1::Resource(why) => E::Resource(why),
        })?;
    let demand = numerical_batch_streaming_demand_v1(
        frame, &source, record, layout, &program, context.profile_id, budget, maximum_ranges, leaf, demand_index,
    )?;
    demand.quote_open_v1(state, notice, lengths)
}

/// Fold the positional maxima while visiting one authenticated root at a
/// time. Retain only the selected root's targets for the quote.
#[allow(clippy::too_many_arguments)]
pub(crate) fn numerical_batch_streaming_demand_v1(
    frame: &PalwTirPublicClaimV1,
    source: &NativeRetainedV1<PalwTirPublicSourceV1>,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    program: &TirProgramV1,
    profile: Hash64,
    budget: OriginalParameterCoverBudgetV1,
    maximum_ranges: usize,
    leaf: u64,
    demand_index: usize,
) -> Result<NativeRetainedV1<NumericalBatchRootDemandV1>, E> {
    let set = TirSharedRequestSetV1::new(frame, source, profile).map_err(E::Decode)?;
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<u64>(maximum_ranges as u64).map_err(E::Resource)?;
    let _maxima_lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
    let mut maximums = Vec::with_capacity(maximum_ranges);
    let mut selected = None;
    let mut callback_error = None;
    let mut ids_bytes = None;
    let streaming = StreamingParameterCoverBudgetV1 {
        total: budget,
        per_root_edges: budget.max_total_edges,
        per_root_coordinates: budget.max_total_coordinates,
        per_root_memberships: budget.max_root_memberships,
        // The walker quotes one root's bounded scratch and reserves it
        // against the native process budget before exploring any root.
        max_workspace_bytes: u64::MAX,
    };
    let summary = PalwTirPublicSourceV1::root_input_cover_streaming_size_v1(
        source, record, layout, streaming, |root, params, inputs| {
            let ranges = root_ranges_v1(
                frame, source, &set, root, inputs.iter().copied(), params, program, maximum_ranges, &mut ids_bytes,
            ).map_err(|error| {
                callback_error = Some(error);
                "numerical Batch range projection failed".to_string()
            })?;
            fold_maximums(&mut maximums, ranges.values().map(|(_, bytes)| *bytes), maximum_ranges)?;
            if root == leaf {
                selected = Some(ranges);
            }
            Ok(0)
        },
    );
    if let Some(error) = callback_error {
        return Err(error);
    }
    if summary?.source_id != set.source_id() {
        return Err(E::Decode("streamed Batch cover differs from the original".into()));
    }
    let selected = selected.ok_or_else(|| E::Decode("numerical Batch root is absent from the original".into()))?;
    finish_root_demand_v1(frame.plan_v1().original_claim, set.scope_id().map_err(E::Decode)?, &maximums, &selected, demand_index)
}

fn partition_maximums_v1(mut remaining: &[u64]) -> impl Iterator<Item = &[u64]> {
    use crate::palw_batch_court_v1::{MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1, MAX_BATCH_RANGES_V1};
    std::iter::from_fn(move || {
        if remaining.is_empty() {
            return None;
        }
        let mut parts = 0;
        let count = remaining
            .iter()
            .take(MAX_BATCH_RANGES_V1)
            .take_while(|&&bytes| {
                parts += bytes.div_ceil(u64::from(MAX_BATCH_PART_BYTES_V1));
                parts <= MAX_BATCH_PARTS_V1 as u64
            })
            .count();
        // Every retained range passed the individual native codec bound.
        debug_assert!(count > 0);
        let (next, rest) = remaining.split_at(count);
        remaining = rest;
        Some(next)
    })
}

/// Use canonical target ordering for both the demand and its admission quote.
fn fold_maximums(maximums: &mut Vec<u64>, ranges: impl ExactSizeIterator<Item = u64>, cap: usize) -> Result<(), String> {
    if ranges.len() > cap {
        return Err("numerical Batch range cap exhausted".into());
    }
    maximums.resize(maximums.len().max(ranges.len()), 0);
    for (reserved, actual) in maximums.iter_mut().zip(ranges) {
        *reserved = (*reserved).max(actual);
    }
    Ok(())
}

pub(crate) fn numerical_batch_envelope_v1(
    frame: &PalwTirPublicClaimV1,
    source: &PalwTirPublicSourceV1,
    cover: &OriginalRootInputCoverV1,
    program: &TirProgramV1,
    profile: Hash64,
    maximum_ranges: usize,
) -> Result<NativeRetainedV1<NumericalBatchEnvelopeV1>, E> {
    let set = TirSharedRequestSetV1::new(frame, source, profile).map_err(E::Decode)?;
    let parameters = cover.parameters_v1();
    let roots = cover.roots_v1();
    if parameters.source_id != set.source_id()
        || roots.is_empty()
        || roots.len() != parameters.roots.len()
        || roots.iter().zip(&parameters.roots).any(|(root, params)| root.leaf != params.leaf)
    {
        return Err(E::Decode("numerical Batch cover differs from the original".into()));
    }
    let maximum_ranges = maximum_ranges.min(
        roots
            .iter()
            .zip(&parameters.roots)
            .map(|(root, params)| root.inputs.len().saturating_add(params.parameters.len()).saturating_add(1))
            .max()
            .unwrap_or(0),
    );
    let mut retained = TirNativeRequestsV1::new();
    retained.vec_v1::<u64>(maximum_ranges as u64).map_err(E::Resource)?;
    retained.exact_v1::<NumericalBatchEnvelopeV1>(1).map_err(E::Resource)?;
    let lease = reserve_native_requests_v1(retained.bytes_v1()).map_err(E::Resource)?;
    let mut maximums = Vec::with_capacity(maximum_ranges);
    let mut ids_bytes = None;

    for (root, params) in roots.iter().zip(&parameters.roots) {
        let ranges = root_ranges_v1(
            frame, source, &set, root.leaf, root.inputs.iter().copied(), &params.parameters, program, maximum_ranges, &mut ids_bytes,
        )?;
        fold_maximums(&mut maximums, ranges.values().map(|(_, bytes)| *bytes), maximum_ranges).map_err(E::Decode)?;
    }

    let scope = set.scope_id().map_err(E::Decode)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(NumericalBatchEnvelopeV1 { scope, maximums }, lease))
}

/// Recover the canonical ranges for one selected root using the same
/// target enumeration and position caps used to reserve its original.
/// Returned sizes are upper bounds; the sender must know the exact body
/// size before committing it in a signed Open.
pub(crate) fn numerical_batch_root_demand_v1(
    frame: &PalwTirPublicClaimV1,
    source: &PalwTirPublicSourceV1,
    cover: &OriginalRootInputCoverV1,
    program: &TirProgramV1,
    profile: Hash64,
    envelope: &NumericalBatchEnvelopeV1,
    leaf: u64,
    demand_index: usize,
) -> Result<NativeRetainedV1<NumericalBatchRootDemandV1>, E> {
    let set = TirSharedRequestSetV1::new(frame, source, profile).map_err(E::Decode)?;
    let parameters = cover.parameters_v1();
    if parameters.source_id != set.source_id() || envelope.scope != set.scope_id().map_err(E::Decode)? {
        return Err(E::Decode("numerical Batch demand differs from the original".into()));
    }
    let roots = cover.roots_v1();
    let index = roots
        .binary_search_by_key(&leaf, |root| root.leaf)
        .map_err(|_| E::Decode("numerical Batch root is absent from the original".into()))?;
    let params = parameters
        .roots
        .get(index)
        .filter(|params| params.leaf == leaf)
        .ok_or_else(|| E::Decode("numerical Batch parameter cover differs from the original".into()))?;
    let ranges = root_ranges_v1(
        frame, source, &set, leaf, roots[index].inputs.iter().copied(), &params.parameters, program, envelope.maximums.len(), &mut None,
    )?;
    finish_root_demand_v1(frame.plan_v1().original_claim, envelope.scope, &envelope.maximums, &ranges, demand_index)
}

fn finish_root_demand_v1(
    claim: Hash64,
    scope: Hash64,
    maximums: &[u64],
    ranges: &BTreeMap<Hash64, (MaterialTargetV1, u64)>,
    demand_index: usize,
) -> Result<NativeRetainedV1<NumericalBatchRootDemandV1>, E> {
    let span = root_demand_span_v1(maximums, ranges.values().map(|(_, bytes)| *bytes), demand_index).map_err(E::Decode)?;
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<(MaterialTargetV1, u64)>(span.len() as u64).map_err(E::Resource)?;
    q.exact_v1::<NumericalBatchRootDemandV1>(1).map_err(E::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
    let mut demand = Vec::with_capacity(span.len());
    demand.extend(ranges.values().skip(span.start).take(span.len()).copied());
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        NumericalBatchRootDemandV1 {
            claim,
            scope,
            ordinal: demand_index as u64,
            ranges: demand,
        },
        lease,
    ))
}

fn root_demand_span_v1(
    maximums: &[u64],
    ranges: impl ExactSizeIterator<Item = u64>,
    demand_index: usize,
) -> Result<std::ops::Range<usize>, String> {
    let len = ranges.len();
    if len > maximums.len() || ranges.zip(maximums).any(|(actual, &cap)| actual == 0 || actual > cap) {
        return Err("numerical Batch root exceeds its reserved positions".into());
    }
    let mut start = 0;
    for (index, group) in partition_maximums_v1(maximums).enumerate() {
        let end = start + group.len();
        if index == demand_index && start < len {
            return Ok(start..end.min(len));
        }
        if end >= len {
            break;
        }
        start = end;
    }
    Err("numerical Batch root has no material at the reserved demand index".into())
}

#[allow(clippy::too_many_arguments)]
fn root_ranges_v1(
    frame: &PalwTirPublicClaimV1,
    source: &PalwTirPublicSourceV1,
    set: &TirSharedRequestSetV1<'_>,
    leaf: u64,
    inputs: impl ExactSizeIterator<Item = RootInputBoundaryV1>,
    parameters: &[u32],
    program: &TirProgramV1,
    maximum_ranges: usize,
    ids_bytes: &mut Option<u64>,
) -> Result<NativeRetainedV1<BTreeMap<Hash64, (MaterialTargetV1, u64)>>, E> {
    let claim = frame.plan_v1().original_claim;
    let count = source.header_v1().job_context.declared_prefill_tokens;
    let prompt = frame.plan_v1().owner_work.rules.prompt_form;
    let full_ids = |source| {
        crate::palw_tir_public_owned_v1::request_body_bound_v1(source, PalwTirPublicRequestV1::Ids { offset: 0 })
            .map(|(bytes, _)| bytes)
    };
    use RootInputBoundaryV1::{CommittedStepLeaf, GeneratedId, PromptPosition};
    let boundaries = inputs.len() as u64 + 1;
    let overflow = || E::Resource(NativeCoverageV1::overflow::<NumericalBatchEnvelopeV1>("Batch range enumeration"));
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(boundaries.checked_mul(btree_node_envelope_bytes_v1::<RootInputBoundaryV1, ()>() as u64).ok_or_else(overflow)?)
        .map_err(E::Resource)?;
    q.add_v1(
        (maximum_ranges as u64)
            .min(boundaries.saturating_add(parameters.len() as u64))
            .checked_mul(btree_node_envelope_bytes_v1::<Hash64, (MaterialTargetV1, u64)>() as u64)
            .ok_or_else(overflow)?,
    )
    .map_err(E::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
    let mut ranges = BTreeMap::new();
    let mut add = |target: MaterialTargetV1, bytes: u64| -> Result<(), String> {
        if !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&bytes) {
            return Err("numerical Batch native range exceeds the codec capacity".into());
        }
        let id = target.id_v1(claim)?;
        if let Some(old) = ranges.get(&id) {
            return if *old == (target, bytes) { Ok(()) } else { Err("numerical Batch range identity collision".into()) };
        }
        if ranges.len() >= maximum_ranges {
            return Err("numerical Batch range cap exhausted".into());
        }
        ranges.insert(id, (target, bytes));
        Ok(())
    };
    let boundaries = inputs.chain([CommittedStepLeaf(leaf)]).collect::<BTreeSet<_>>();
    set.visit_batch_step_targets_v1(
        &boundaries,
        crate::palw_batch_court_v1::MAX_BATCH_PARTS_V1,
        maximum_ranges,
        |target, _, bound| add(target, bound.maximum_response_bytes),
    )
    .map_err(E::Decode)?;
    for &leaf in parameters {
        let bytes = parameter_body_bound_v1(program, leaf).map_err(E::Decode)?;
        add(MaterialTargetV1::Parameter { leaf }, bytes).map_err(E::Decode)?;
    }
    for boundary in &boundaries {
        match *boundary {
            PromptPosition(position) => {
                if position >= count {
                    return Err(E::Decode("numerical Batch prompt position outside original".into()));
                }
                let (target, bytes) = match prompt {
                    PalwPromptIdsFormV1::Flat => (MaterialTargetV1::PromptFlat, 4 + 4 * u64::from(count)),
                    PalwPromptIdsFormV1::MerkleV1 => {
                        let tile = position / PALW_PROMPT_IDS_TILE_LEN;
                        (MaterialTargetV1::PromptTile { tile }, prompt_tile_bound(count, tile).map_err(E::Decode)?)
                    }
                };
                add(target, bytes).map_err(E::Decode)?;
            }
            GeneratedId(index) => {
                if index >= source.decode_count_v1() {
                    return Err(E::Decode("numerical Batch generated ID outside original".into()));
                }
                let bytes = match *ids_bytes {
                    Some(bytes) => bytes,
                    None => {
                        let bytes = full_ids(source).map_err(E::Decode)?;
                        *ids_bytes = Some(bytes);
                        bytes
                    }
                };
                add(MaterialTargetV1::FullIds, bytes).map_err(E::Decode)?;
            }
            _ => {}
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(ranges, lease))
}

#[cfg(test)]
mod tests {
    use super::{check_material_lengths_v1, fold_maximums, partition_maximums_v1, root_demand_span_v1};

    #[test]
    fn exact_lengths_cannot_substitute_or_reorder_selected_targets() {
        let plan = [(1, 7), (2, 9)];
        assert!(check_material_lengths_v1(&plan, &[(1, 3), (2, 9)]).is_ok());
        assert!(check_material_lengths_v1(&plan, &[(2, 3), (1, 9)]).is_err());
        assert!(check_material_lengths_v1(&plan, &[(1, 3), (1, 9)]).is_err());
        assert!(check_material_lengths_v1(&plan, &[(1, 3), (2, 10)]).is_err());
        assert!(check_material_lengths_v1(&plan, &[(1, 0), (2, 9)]).is_err());
        assert!(check_material_lengths_v1(&plan, &[(1, 3)]).is_err());
    }

    #[test]
    fn envelope_preserves_every_position_without_combining_root_material() {
        let mut maximums = Vec::new();
        fold_maximums(&mut maximums, [7, 11, 2].into_iter(), 4).unwrap();
        fold_maximums(&mut maximums, [13, 4].into_iter(), 4).unwrap();
        fold_maximums(&mut maximums, [1, 3, 7, 9].into_iter(), 4).unwrap();
        assert_eq!(maximums, [13, 11, 7, 9]);
        assert!(fold_maximums(&mut maximums, [1; 5].into_iter(), 4).is_err());
        assert_eq!(maximums, [13, 11, 7, 9]);
    }

    #[test]
    fn demand_partitions_fit_the_wire_caps_without_reordering_positions() {
        use crate::palw_batch_court_v1::{MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1, MAX_BATCH_RANGES_V1};
        let cap = MAX_BATCH_PARTS_V1 as u64 * u64::from(MAX_BATCH_PART_BYTES_V1);
        let mut maximums = vec![1; MAX_BATCH_RANGES_V1 + 1];
        maximums.push(cap);
        maximums.push(1);
        let groups = partition_maximums_v1(&maximums).collect::<Vec<_>>();
        assert_eq!(groups.iter().map(|r| r.len()).collect::<Vec<_>>(), [MAX_BATCH_RANGES_V1, 1, 1, 1]);
        assert_eq!(groups.into_iter().flatten().copied().collect::<Vec<_>>(), maximums);
    }

    #[test]
    fn selected_root_uses_booked_partitions_even_when_its_ranges_are_smaller() {
        use crate::palw_batch_court_v1::{MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1};
        let cap = MAX_BATCH_PARTS_V1 as u64 * u64::from(MAX_BATCH_PART_BYTES_V1);
        let maximums = [cap, 2, 3];
        let actual = [1, 2];
        assert_eq!(root_demand_span_v1(&maximums, actual.into_iter(), 0).unwrap(), 0..1);
        assert_eq!(root_demand_span_v1(&maximums, actual.into_iter(), 1).unwrap(), 1..2);
        assert!(root_demand_span_v1(&maximums, actual.into_iter(), 2).is_err());
        assert!(root_demand_span_v1(&maximums, [1, 3].into_iter(), 0).is_err());
    }

    #[test]
    fn later_roots_can_change_the_partition_of_an_earlier_selection() {
        use crate::palw_batch_court_v1::{MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1};
        let cap = MAX_BATCH_PARTS_V1 as u64 * u64::from(MAX_BATCH_PART_BYTES_V1);
        let selected = [2, 3];
        let mut maximums = Vec::new();
        fold_maximums(&mut maximums, selected.into_iter(), 2).unwrap();
        assert_eq!(root_demand_span_v1(&maximums, selected.into_iter(), 0).unwrap(), 0..2);
        fold_maximums(&mut maximums, [cap, 2].into_iter(), 2).unwrap();
        assert_eq!(root_demand_span_v1(&maximums, selected.into_iter(), 0).unwrap(), 0..1);
        assert_eq!(root_demand_span_v1(&maximums, selected.into_iter(), 1).unwrap(), 1..2);
    }
}
