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
        parameter_cover_v1::{OriginalRootInputCoverV1, StreamingParameterCoverBudgetV1},
    },
};
use misaka_palw_tir::program::TirProgramV1;
use std::collections::{BTreeMap, BTreeSet};

type E = NativeDecodeErrorV1<String>;
type RootRanges = NativeRetainedV1<BTreeMap<Hash64, (MaterialTargetV1, u64)>>;
pub(crate) const MAX_BOOKED_NUMERICAL_RANGES_V1: usize = 4_096;

/// Maximum byte cap per canonical range position across all numerical roots.
/// This covers one selected root's inputs, including the root leaf. It does
/// not price later turns, native verdicts, or simultaneous independent roots.
pub(crate) struct NumericalBatchEnvelopeV1 {
    pub(super) claim: Hash64,
    pub(super) profile: Hash64,
    scope: Hash64,
    maximums: Vec<u64>,
}

impl NumericalBatchEnvelopeV1 {
    #[cfg(test)]
    pub(crate) fn test_only_v1(claim: Hash64, profile: Hash64, scope: Hash64, maximums: Vec<u64>) -> Self {
        Self { claim, profile, scope, maximums }
    }

    pub(crate) fn scope_v1(&self) -> Hash64 {
        self.scope
    }

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

    pub(crate) fn check_signed_ranges_v1(&self, ranges: &[BatchRangeV1]) -> Result<(), String> {
        check_declared_ranges_v1(
            self.ranges.iter().map(|(target, bound)| Ok((target.id_v1(self.claim)?, *bound))),
            ranges,
        )
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

fn check_declared_ranges_v1(
    planned: impl ExactSizeIterator<Item = Result<(Hash64, u64), String>>,
    ranges: &[BatchRangeV1],
) -> Result<(), String> {
    if planned.len() != ranges.len() {
        return Err("numerical Batch Open must name every booked target".into());
    }
    for (expected, range) in planned.zip(ranges) {
        let (id, maximum) = expected?;
        if id != range.id || range.bytes == 0 || u64::from(range.bytes) > maximum
            || range.authentication != BatchRangeAuthenticationV1::WholeRange
        {
            return Err("numerical Batch Open substitutes a target, length or authentication".into());
        }
    }
    Ok(())
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
    use crate::palw_batch_court_v1::{
        carriage::batch_open_wire_bytes_v1,
        rooted::{checked_initial_notice_target_v1, reserved_open_slot_v1},
    };
    use crate::palw_court_shared_tir_state_v1::BatchNoticeTargetV1;
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    let context = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(E::Decode)?;
    let notice_key = Key::CourtSharedQuery(notice);
    let Some(Row::CourtSharedQuery(notice_row)) = state.staged_court_row_v1(&notice_key) else {
        return Err(E::Decode("numerical Batch has no accepted notice".into()));
    };
    let target = checked_initial_notice_target_v1(state, context, notice_row, notice).map_err(E::Decode)?;
    if target.is_some_and(|target| target != (BatchNoticeTargetV1::Numerical { leaf })) {
        return Err(E::Decode("numerical Batch leaf differs from its initial notice".into()));
    }
    if notice_row.claim != claim
        || notice_row.scope_id != context.scope_id
        || notice_row.material_until_daa != context.material_until_daa
        || state.staged_court_row_v1(&Key::CourtBatchDemand(claim, demand_index as u64)).is_some()
    {
        return Err(E::Decode("numerical Batch notice or opening differs from the accepted case".into()));
    }
    let wire_bytes = batch_open_wire_bytes_v1(lengths.len()).map_err(E::Decode)?;
    let (_, binding, opened_daa) =
        reserved_open_slot_v1(state, claim, context.scope_id, demand_index as u64, wire_bytes).map_err(E::Decode)?;
    if state.last_point().is_some_and(|point| point.daa_score > opened_daa) {
        return Err(E::Decode("numerical Batch opening slot has passed".into()));
    }

    let demand = numerical_demand_from_source_v1(state, context, &binding.packet_ids, leaf, demand_index, maximum_ranges)?;
    demand.quote_open_v1(state, notice, lengths)
}

pub(crate) fn numerical_demand_from_source_v1(
    state: &PalwChainStateV2,
    context: &crate::palw_court_shared_tir_state_v1::SharedTirContextV1,
    owned: &BTreeSet<[u8; 64]>,
    leaf: u64,
    demand_index: usize,
    maximum_ranges: usize,
) -> Result<NativeRetainedV1<NumericalBatchRootDemandV1>, E> {
    let claim = context.claim;
    with_numerical_source_v1(state, claim, |frame, source, record, layout, program, streaming| {
        if context.batch_notice_only_v1() {
            let ranges = booked_root_ranges_v1(context, frame, source, record, layout, program, streaming, leaf, maximum_ranges)?;
            let span = booked_range_span_v1(ranges.len(), demand_index, |ordinal, index| {
                owned_begin_v1(claim, context.scope_id, owned, ordinal, index)
            }).map_err(E::Decode)?;
            retain_root_demand_v1(claim, context.scope_id, &ranges, demand_index, span)
        } else {
            numerical_batch_streaming_demand_v1(
                frame, source, record, layout, program, context.profile_id, streaming, maximum_ranges, leaf, demand_index,
            )
        }
    })
}

fn with_numerical_source_v1<T>(
    state: &PalwChainStateV2,
    claim: Hash64,
    use_source: impl FnOnce(
        &PalwTirPublicClaimV1,
        &NativeRetainedV1<PalwTirPublicSourceV1>,
        &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        &crate::palw_tir_class_v1::PalwTirLayoutV1,
        &TirProgramV1,
        StreamingParameterCoverBudgetV1,
    ) -> Result<T, E>,
) -> Result<T, E> {
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
    // This estimate still supplies only aggregate limits. The funded
    // candidate can supply independently qualified per-root limits below.
    let streaming = StreamingParameterCoverBudgetV1 {
        total: budget,
        per_root_edges: budget.max_total_edges,
        per_root_coordinates: budget.max_total_coordinates,
        per_root_memberships: budget.max_root_memberships,
        // The walker reserves its quoted workspace in the native budget.
        max_workspace_bytes: u64::MAX,
    };
    let program =
        crate::palw_native_resource_v1::decode_native_tir_program_v1(record.program.as_slice()).map_err(|err| match err {
            NativeDecodeErrorV1::Decode(why) => E::Decode(why.to_string()),
            NativeDecodeErrorV1::Resource(why) => E::Resource(why),
        })?;
    use_source(frame, &source, record, layout, &program, streaming)
}

#[allow(clippy::too_many_arguments)]
fn booked_root_ranges_v1(
    context: &crate::palw_court_shared_tir_state_v1::SharedTirContextV1,
    frame: &PalwTirPublicClaimV1,
    source: &NativeRetainedV1<PalwTirPublicSourceV1>,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    program: &TirProgramV1,
    streaming: StreamingParameterCoverBudgetV1,
    leaf: u64,
    maximum_ranges: usize,
) -> Result<RootRanges, E> {
    let set = TirSharedRequestSetV1::new(frame, source, context.profile_id).map_err(E::Decode)?;
    if set.scope_id().map_err(E::Decode)? != context.scope_id {
        return Err(E::Decode("numerical Batch source differs from its reserved scope".into()));
    }
    let maximum_ranges = maximum_ranges.min(MAX_BOOKED_NUMERICAL_RANGES_V1);
    selected_root_ranges_v1(frame, source, &set, record, layout, program, streaming, leaf, maximum_ranges)
}

fn owned_begin_v1(claim: Hash64, scope: Hash64, owned: &BTreeSet<[u8; 64]>, ordinal: usize, index: usize) -> Result<bool, String> {
    let position = super::BatchResponsePositionV1 { demand_ordinal: ordinal as u64, range_index: index as u32 };
    Ok(owned.contains(&position.packet_id(claim, scope, super::Stage::Begin)?))
}

/// Complete authenticated input for the numerical verification stage.
/// Preparing it does not execute the leaf or grant Court credit.
pub(crate) struct PreparedNumericalBatchMaterialV1 {
    pub leaf: u64,
    pub scope: Hash64,
    pub notice: Hash64,
    pub material: NativeRetainedV1<super::super::RootedMaterialV1>,
}

pub(crate) struct NumericalBatchFindingV1 {
    pub claim: Hash64,
    pub leaf: u64,
    pub scope: Hash64,
    pub notice: Hash64,
    pub at: crate::palw_state_v2::PalwBlockContextV2,
    pub fault: Option<(Hash64, NativeRetainedV1<Vec<u8>>)>,
}

/// Read-only native replay at the booked verification point. The terminal
/// carrier must separately authenticate its filer and retain the finding.
pub(crate) fn replay_numerical_batch_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
    budget: super::numerical_verification::NumericalBatchReplayBudgetV1,
) -> Result<NumericalBatchFindingV1, E> {
    use crate::palw_tir_court_v1::{PalwTirCourtRulesV1, replay_tir_cone_from_store_v1};
    rules.check().map_err(E::Decode)?;
    if !rules.tir_active || budget.workspace_bytes == 0 {
        return Err(E::Decode("numerical replay has no active TIR rules or admitted workspace".into()));
    }
    let prepared = prepare_numerical_batch_material_v1(state, claim, rules.at.daa_score)?;
    let frame = super::super::frame_of(state, claim).map_err(E::Decode)?;
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or_else(|| E::Decode("numerical replay has no registered class".into()))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
        .ok_or_else(|| E::Decode("numerical replay has no registered layout".into()))?;
    let prompt_form = if rules.prompt_ids_merkle { PalwPromptIdsFormV1::MerkleV1 } else { PalwPromptIdsFormV1::Flat };
    if prompt_form != frame.plan_v1().owner_work.rules.prompt_form {
        return Err(E::Decode("numerical replay prompt form differs from the original".into()));
    }
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(PalwTirPublicSourceV1::native_construction_requests_v1(record, layout, frame.header_v1())?)
        .map_err(E::Resource)?;
    q.add_v1(budget.workspace_bytes).map_err(E::Resource)?;
    let _workspace = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
    let binding = frame.header_v1().resolve_registered_layout_v1(
        record, layout, frame.original_v1().execution_root, frame.descriptor_v1().maximum_leaves.min(rules.step_ladder),
    ).map_err(E::Decode)?;
    let store = prepared.material.store_v1(state, claim, prepared.scope).map_err(E::Decode)?;
    let court = PalwTirCourtRulesV1 {
        max_step_leaf_count: rules.step_ladder,
        prompt_form,
        limits: crate::palw_court_v2::palw_tir_court_limits_v1(&rules.court),
    };
    let finding = replay_tir_cone_from_store_v1(
        &binding, prepared.leaf, &store, &court, budget.work, rules.court.max_close_bytes(),
    ).map_err(E::Decode)?;
    let fault = if let Some((fault, mut cone)) = finding {
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut cone.binding);
        let len = borsh::object_length(&cone).map_err(|why| E::Decode(why.to_string()))?;
        if len == 0 || len as u64 > budget.max_fault_bytes
            || len > crate::palw_batch_court_v1::rooted::MAX_NUMERICAL_FAULT_BYTES_V1
        {
            return Err(E::Decode("numerical Batch fault exceeds its reserved retention".into()));
        }
        let lease = reserve_native_requests_v1(len as u64).map_err(E::Resource)?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(len).map_err(|why| E::Resource(
            NativeCoverageV1::unsupported::<Vec<u8>>("retain numerical Batch fault", &why.to_string()),
        ))?;
        borsh::BorshSerialize::serialize(&cone, &mut bytes).map_err(|why| E::Decode(why.to_string()))?;
        Some((fault.evidence_id, NativeRetainedV1::from_owner_reserved_v1(bytes, lease)))
    } else {
        None
    };
    Ok(NumericalBatchFindingV1 { claim, leaf: prepared.leaf, scope: prepared.scope, notice: prepared.notice, at: rules.at, fault })
}

/// Authenticate the archived operands at the reserved terminal point. A
/// matching leaf has no fault capability and grants no claim-level credit.
pub(crate) fn recheck_numerical_terminal_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    receipt_digest: Hash64,
    rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
    budget: super::numerical_verification::NumericalBatchReplayBudgetV1,
) -> Result<Option<(crate::palw_state_v2::PalwBondKeyV2, NativeRetainedV1<crate::palw_tir_court_v1::PalwTirConeRefutationV1>)>, E> {
    use crate::palw_tir_court_v1::{PalwTirConeRefutationV1, PalwTirCourtRulesV1, recheck_retained_tir_cone_v1};
    let invalid = |why: &str| E::Decode(why.into());
    rules.check().map_err(E::Decode)?;
    let verification_cost = budget.service_cost_v1().filter(|_| rules.tir_active)
        .ok_or_else(|| invalid("numerical terminal has no active TIR rules or admitted budget"))?;
    let receipt = crate::palw_batch_court_v1::rooted::reserved_numerical_terminal_v1(state, claim, receipt_digest, &rules.at)
        .map_err(E::Decode)?;
    let (verification, terminal) = super::admission::batch_numerical_service_ids_v1(claim, receipt.scope)
        .map_err(|_| invalid("numerical terminal packet identities are invalid"))?;
    let book = &state.court_service_state_v1().ok_or_else(|| invalid("numerical terminal has no service book"))?.book;
    if !book.packet(&verification).is_some_and(|packet| packet.cost == verification_cost)
        || !book.packet(&terminal).is_some_and(|packet| budget.covers_terminal_recheck_v1(packet.cost))
    {
        return Err(invalid("numerical terminal lacks booked work or IO"));
    }
    receipt.check_context(state, &rules.at).map_err(E::Decode)?;
    let Some(expected) = receipt.fault_evidence else { return Ok(None) };
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&Key::CourtSharedQuery(receipt.notice)) else {
        return Err(invalid("numerical terminal lost its accepted notice issuer"));
    };
    let bytes = receipt.fault_bytes.as_ref().ok_or_else(|| invalid("numerical terminal is missing retained fault material"))?;
    if bytes.len() as u64 > budget.max_fault_bytes || bytes.len() as u64 >= rules.court.max_close_bytes() {
        return Err(invalid("numerical terminal fault exceeds its admitted bytes"));
    }
    let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<PalwTirConeRefutationV1>(
        bytes.as_slice(), bytes.len() as u64,
    ).map_err(|why| E::Decode(why.to_string()))?;
    if shape.logical_requests_v1() > budget.workspace_bytes {
        return Err(invalid("numerical terminal fault exceeds its admitted workspace"));
    }
    let frame = super::super::frame_of(state, claim).map_err(E::Decode)?;
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or_else(|| invalid("numerical terminal has no registered class"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
        .ok_or_else(|| invalid("numerical terminal has no registered layout"))?;
    let prompt_form = if rules.prompt_ids_merkle { PalwPromptIdsFormV1::MerkleV1 } else { PalwPromptIdsFormV1::Flat };
    if prompt_form != frame.plan_v1().owner_work.rules.prompt_form {
        return Err(invalid("numerical terminal prompt form differs from the original"));
    }
    let mut quote = TirNativeRequestsV1::new();
    quote.add_v1(PalwTirPublicSourceV1::native_construction_requests_v1(record, layout, frame.header_v1())?).map_err(E::Resource)?;
    quote.add_v1(shape.logical_requests_v1()).map_err(E::Resource)?;
    quote.add_v1(budget.workspace_bytes).map_err(E::Resource)?;
    let lease = reserve_native_requests_v1(quote.bytes_v1()).map_err(E::Resource)?;
    let original = frame.header_v1().resolve_registered_layout_v1(
        record, layout, frame.original_v1().execution_root, frame.descriptor_v1().maximum_leaves.min(rules.step_ladder),
    ).map_err(E::Decode)?;
    let cone: PalwTirConeRefutationV1 = borsh::from_slice(bytes.as_slice()).map_err(|why| E::Decode(why.to_string()))?;
    let court = PalwTirCourtRulesV1 {
        max_step_leaf_count: rules.step_ladder,
        prompt_form,
        limits: crate::palw_court_v2::palw_tir_court_limits_v1(&rules.court),
    };
    let cone = recheck_retained_tir_cone_v1(cone, original, receipt.leaf, expected, &court, budget.work).map_err(E::Decode)?;
    Ok(Some((notice.opened_by, NativeRetainedV1::from_owner_reserved_v1(cone, lease))))
}

pub(crate) fn prepare_numerical_batch_material_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    verify_daa: u64,
) -> Result<PreparedNumericalBatchMaterialV1, E> {
    use super::{BatchResponsePositionV1, Stage, admission::batch_numerical_service_ids_v1};
    use crate::palw_batch_court_v1::rooted::{batch_open_packet_v1, checked_initial_notice_target_v1};
    use crate::palw_court_shared_tir_state_v1::{self as shared, BatchNoticeTargetV1};
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    let invalid = |why: &str| E::Decode(why.into());
    let context = shared::context(state, claim).map_err(E::Decode)?;
    let scope = context.scope_id;
    let Some((_, key)) = state.court_shared_initial_notice_v1(claim) else {
        return Err(invalid("numerical verification has no accepted notice"));
    };
    let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(key) else {
        return Err(invalid("numerical verification lost its accepted notice"));
    };
    let notice_id = notice.request_id;
    let Some(BatchNoticeTargetV1::Numerical { leaf }) =
        checked_initial_notice_target_v1(state, context, notice, notice_id).map_err(E::Decode)?
    else {
        return Err(invalid("Batch notice does not select numerical verification"));
    };
    let service = state.court_service_row_v3_v1().ok_or_else(|| invalid("numerical verification has no causal service row"))?;
    context.check_binding(&service.state).map_err(E::Decode)?;
    let owned = &service.state.claims.get(&claim).ok_or_else(|| invalid("numerical verification has no original booking"))?.packet_ids;
    let book = &service.state.book;
    let (verification, terminal) = batch_numerical_service_ids_v1(claim, scope)
        .map_err(|_| invalid("numerical verification packet identities are invalid"))?;
    let predecessors = service.causal.get(&verification).ok_or_else(|| invalid("numerical verification has no reserved inputs"))?;
    if !owned.contains(&verification) || !owned.contains(&terminal)
        || book.assigned_daa(&verification) != Some(verify_daa)
        || verify_daa >= context.hard_daa
        || book.assigned_daa(&terminal).is_none_or(|daa| daa <= verify_daa || daa >= context.hard_daa)
        || !service.causal.get(&terminal).is_some_and(|ids| ids.binary_search(&verification).is_ok())
    {
        return Err(invalid("numerical verification differs from its reserved stage"));
    }
    let ranges = with_numerical_source_v1(state, claim, |frame, source, record, layout, program, streaming| {
        booked_root_ranges_v1(context, frame, source, record, layout, program, streaming, leaf, MAX_BOOKED_NUMERICAL_RANGES_V1)
    })?;
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<MaterialTargetV1>(ranges.len() as u64).map_err(E::Resource)?;
    let _targets = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
    let mut targets = Vec::with_capacity(ranges.len());
    let mut expected = ranges.iter();
    visit_booked_range_groups_v1(ranges.len(), |ordinal, index| owned_begin_v1(claim, scope, owned, ordinal, index), |ordinal, count| {
        let ordinal = ordinal as u64;
        let key = Key::CourtBatchDemand(claim, ordinal);
        let Some(Row::CourtBatchDemand(open)) = state.staged_court_row_v1(&key) else {
            return Err("numerical verification is missing a required Open".into());
        };
        open.check(&key)?;
        let open_packet = batch_open_packet_v1(claim, scope, ordinal)?.as_bytes();
        if open.scope_id != scope || open.notice_id != notice_id || !owned.contains(&open_packet)
            || book.assigned_daa(&open_packet) != Some(open.demand.opened_daa)
        {
            return Err("numerical verification Open changes its original reservation or notice".into());
        }
        let group = expected.clone().take(count);
        check_declared_ranges_v1(group.map(|(id, (_, maximum))| Ok((*id, *maximum))), &open.demand.ranges)?;
        for (index, ((_, (target, _)), declared)) in expected.by_ref().take(count).zip(&open.demand.ranges).enumerate() {
            let position = BatchResponsePositionV1 { demand_ordinal: ordinal, range_index: index as u32 };
            let finish = position.packet_id(claim, scope, Stage::Finish)?;
            let daa = book.assigned_daa(&finish).ok_or("numerical verification has an unbooked Finish")?;
            if !owned.contains(&finish) || predecessors.binary_search(&finish).is_err() || daa >= verify_daa
                || daa >= open.demand.response_deadline_daa
            {
                return Err("numerical verification Finish falls outside its causal reservation".into());
            }
            super::super::check_selected_batch_delivery_v1(state, context, *target, declared.bytes, daa)?;
            targets.push(*target);
        }
        Ok(())
    }).map_err(E::Decode)?;
    let material = super::super::RootedMaterialV1::load_selected_batch_v1(state, claim, &targets, verify_daa)?;
    Ok(PreparedNumericalBatchMaterialV1 { leaf, scope, notice: notice_id, material })
}

fn visit_booked_range_groups_v1(
    mut remaining: usize,
    mut has_begin: impl FnMut(usize, usize) -> Result<bool, String>,
    mut visit: impl FnMut(usize, usize) -> Result<(), String>,
) -> Result<(), String> {
    let mut ordinal = 0;
    while remaining != 0 {
        let count = booked_group_capacity_v1(ordinal, &mut has_begin)?.min(remaining);
        if count == 0 {
            return Err("numerical verification has unbooked material targets".into());
        }
        visit(ordinal, count)?;
        remaining -= count;
        ordinal += 1;
    }
    Ok(())
}

fn booked_group_capacity_v1(
    ordinal: usize,
    has_begin: &mut impl FnMut(usize, usize) -> Result<bool, String>,
) -> Result<usize, String> {
    let mut count = 0;
    while count < MAX_BATCH_RANGES_V1 && has_begin(ordinal, count)? {
        count += 1;
    }
    Ok(count)
}

#[allow(clippy::too_many_arguments)]
fn selected_root_ranges_v1(
    frame: &PalwTirPublicClaimV1,
    source: &NativeRetainedV1<PalwTirPublicSourceV1>,
    set: &TirSharedRequestSetV1<'_>,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    program: &TirProgramV1,
    streaming: StreamingParameterCoverBudgetV1,
    leaf: u64,
    maximum_ranges: usize,
) -> Result<RootRanges, E> {
    let mut result = None;
    let walked = PalwTirPublicSourceV1::visit_selected_root_inputs_v1(source, record, layout, streaming, leaf, |params, inputs| {
        result = Some(root_ranges_v1(frame, source, set, leaf, inputs.iter().copied(), params, program, maximum_ranges, &mut None));
        Ok(())
    });
    walked?;
    result.ok_or_else(|| E::Decode("numerical Batch root is absent from the original".into()))?
}

/// The native reservation fixes positional capacities before the notice.
/// A shorter root consumes a prefix of the same groups. Stop's special
/// ordinal is never visited, and each earlier group must have a Begin slot.
fn booked_range_span_v1(
    count: usize,
    demand_index: usize,
    mut has_begin: impl FnMut(usize, usize) -> Result<bool, String>,
) -> Result<std::ops::Range<usize>, String> {
    if demand_index >= count {
        return Err("numerical Batch root has no material at the reserved demand index".into());
    }
    let mut start = 0;
    for ordinal in 0..=demand_index {
        let capacity = booked_group_capacity_v1(ordinal, &mut has_begin)?;
        if capacity == 0 || start >= count {
            break;
        }
        let end = start + capacity;
        if ordinal == demand_index {
            return Ok(start..end.min(count));
        }
        start = end;
    }
    Err("numerical Batch root has no material at the reserved demand index".into())
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
    streaming: StreamingParameterCoverBudgetV1,
    maximum_ranges: usize,
    leaf: u64,
    demand_index: usize,
) -> Result<NativeRetainedV1<NumericalBatchRootDemandV1>, E> {
    let (envelope, selected) = project_numerical_batch_streaming_v1(
        frame, source, record, layout, program, profile, streaming, maximum_ranges, Some(leaf),
    )?;
    let selected = selected.ok_or_else(|| E::Decode("numerical Batch root is absent from the original".into()))?;
    finish_root_demand_v1(frame.plan_v1().original_claim, envelope.scope, &envelope.maximums, &selected, demand_index)
}

/// Preclaim envelope without retaining the per-root input cover or deriving
/// the legacy shared frontier. The streaming limits belong to the caller's
/// native profile; an envelope alone does not admit a claim.
#[allow(clippy::too_many_arguments)]
pub(crate) fn numerical_batch_streaming_envelope_v1(
    frame: &PalwTirPublicClaimV1,
    source: &NativeRetainedV1<PalwTirPublicSourceV1>,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    program: &TirProgramV1,
    profile: Hash64,
    streaming: StreamingParameterCoverBudgetV1,
    maximum_ranges: usize,
) -> Result<NativeRetainedV1<NumericalBatchEnvelopeV1>, E> {
    project_numerical_batch_streaming_v1(frame, source, record, layout, program, profile, streaming, maximum_ranges, None)
        .map(|(envelope, _)| envelope)
}

#[allow(clippy::too_many_arguments)]
fn project_numerical_batch_streaming_v1(
    frame: &PalwTirPublicClaimV1,
    source: &NativeRetainedV1<PalwTirPublicSourceV1>,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
    program: &TirProgramV1,
    profile: Hash64,
    streaming: StreamingParameterCoverBudgetV1,
    maximum_ranges: usize,
    leaf: Option<u64>,
) -> Result<(NativeRetainedV1<NumericalBatchEnvelopeV1>, Option<RootRanges>), E> {
    let set = TirSharedRequestSetV1::new(frame, source, profile).map_err(E::Decode)?;
    let mut q = TirNativeRequestsV1::new();
    q.vec_v1::<u64>(maximum_ranges as u64).map_err(E::Resource)?;
    q.exact_v1::<NumericalBatchEnvelopeV1>(1).map_err(E::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
    let mut maximums = Vec::with_capacity(maximum_ranges);
    let mut selected = None;
    let mut callback_error = None;
    let mut ids_bytes = None;
    let summary = PalwTirPublicSourceV1::root_input_cover_streaming_size_v1(
        source, record, layout, streaming, |root, params, inputs| {
            let ranges = root_ranges_v1(
                frame, source, &set, root, inputs.iter().copied(), params, program, maximum_ranges, &mut ids_bytes,
            ).map_err(|error| {
                callback_error = Some(error);
                "numerical Batch range projection failed".to_string()
            })?;
            fold_maximums(&mut maximums, ranges.values().map(|(_, bytes)| *bytes), maximum_ranges)?;
            if Some(root) == leaf {
                selected = Some(ranges);
            }
            Ok(0)
        },
    );
    if let Some(error) = callback_error {
        return Err(error);
    }
    if summary?.source_id != set.source_id() || maximums.is_empty() {
        return Err(E::Decode("streamed Batch cover differs from the original".into()));
    }
    let scope = set.scope_id().map_err(E::Decode)?;
    let claim = frame.plan_v1().original_claim;
    let envelope = NumericalBatchEnvelopeV1 { claim, profile, scope, maximums };
    Ok((NativeRetainedV1::from_owner_reserved_v1(envelope, lease), selected))
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
    let claim = frame.plan_v1().original_claim;
    Ok(NativeRetainedV1::from_owner_reserved_v1(NumericalBatchEnvelopeV1 { claim, profile, scope, maximums }, lease))
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
    retain_root_demand_v1(claim, scope, ranges, demand_index, span)
}

fn retain_root_demand_v1(
    claim: Hash64,
    scope: Hash64,
    ranges: &BTreeMap<Hash64, (MaterialTargetV1, u64)>,
    demand_index: usize,
    span: std::ops::Range<usize>,
) -> Result<NativeRetainedV1<NumericalBatchRootDemandV1>, E> {
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
    use super::{booked_range_span_v1, check_declared_ranges_v1, check_material_lengths_v1, fold_maximums, partition_maximums_v1, root_demand_span_v1};

    #[test]
    fn signed_ranges_bind_the_target_identity_size_and_authentication() {
        use crate::{Hash64, palw_batch_court_v1::{BatchRangeAuthenticationV1, BatchRangeV1}};
        let id = Hash64::from_u64_word(1);
        let good = BatchRangeV1 { id, bytes: 7, authentication: BatchRangeAuthenticationV1::WholeRange };
        let check = |ranges: &[BatchRangeV1]| check_declared_ranges_v1([Ok((id, 8))].into_iter(), ranges);
        assert!(check(&[good]).is_ok());
        assert!(check(&[]).is_err());
        for bad in [
            BatchRangeV1 { id: Hash64::from_u64_word(2), ..good },
            BatchRangeV1 { bytes: 0, ..good },
            BatchRangeV1 { bytes: 9, ..good },
            BatchRangeV1 { authentication: BatchRangeAuthenticationV1::IndependentParts, ..good },
        ] {
            assert!(check(&[bad]).is_err());
        }
    }

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
    fn begin_slots_reconstruct_the_reserved_partition() {
        use crate::palw_batch_court_v1::{MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1};
        let full = MAX_BATCH_PARTS_V1 as u64 * u64::from(MAX_BATCH_PART_BYTES_V1);
        let maximums = [full, 2, 3, full, 2, 3];
        let groups = partition_maximums_v1(&maximums).collect::<Vec<_>>();
        for count in 0..=maximums.len() {
            for ordinal in 0..=maximums.len() {
                let booked = booked_range_span_v1(count, ordinal, |d, r| Ok(groups.get(d).is_some_and(|g| r < g.len())));
                let expected = root_demand_span_v1(&maximums, (0..count).map(|_| 1), ordinal);
                assert_eq!(booked, expected);
            }
        }
        assert!(booked_range_span_v1(3, 1, |ordinal, _| Ok(ordinal == 1)).is_err());
    }

    #[test]
    fn verification_visits_every_required_group_and_ignores_the_reserved_tail() {
        use super::visit_booked_range_groups_v1;
        let mut visited = Vec::new();
        visit_booked_range_groups_v1(5, |ordinal, index| Ok(index < [2, 4, 8][ordinal]), |ordinal, count| {
            visited.push((ordinal, count));
            Ok(())
        }).unwrap();
        assert_eq!(visited, [(0, 2), (1, 3)]);
        assert!(visit_booked_range_groups_v1(5, |ordinal, index| Ok(ordinal == 0 && index < 2), |_, _| Ok(())).is_err());
        assert_eq!(visit_booked_range_groups_v1(5, |_, index| Ok(index < 2), |ordinal, _| {
            if ordinal == 1 { Err("required Open missing".into()) } else { Ok(()) }
        }), Err("required Open missing".into()));
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
