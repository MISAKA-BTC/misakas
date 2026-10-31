//! Public TIR arithmetic localization. Only accepted, original-root-authenticated
//! frontiers advance the search; agreement of this tree is not an Honest verdict.
#![allow(dead_code)]
use kaspa_consensus_core::{
    Hash64,
    palw_backend::PalwTirOwnStepSourceV1,
    palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1},
    palw_legacy_public_filer_v1::{
        PALW_DISPUTE_RESERVATION_VERSION_V1, PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1, PalwDisputeReacquireV1,
    },
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwBondKeyV2, PalwLegacyDisputeViewV1},
    palw_tir_court_v1::{
        PalwTirStepNodeDisclosureV1, palw_tir_step_node_frontier_v1, palw_tir_step_node_reaches_v1, palw_tir_step_tree_height_v1,
    },
};
use std::sync::Arc;

use kaspa_consensus_core::{
    palw_backend::{PalwExecutionBackendV1, PalwTirPublicConeNeedV1, PalwTirPublicConeProbeOwnedV2},
    palw_court_v2::PalwCourtVerdictProofV2,
    palw_tir_court_v1::{PalwTirBuilderMeterV1, PalwTirBuilderWorkLimitsV1, PalwTirCourtRulesV1, PalwTirStepRunDisclosureV1},
    palw_tir_public_source_v1::{
        PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1, PalwTirPublicAuthenticatedIdsV1, PalwTirPublicAuthenticatedUnitV1, PalwTirPublicRequestV1,
        PalwTirPublicResponseV1, PalwTirPublicSourceV1,
    },
};

/// Literal native scope supplied by the future paid-sidecar reader. This local
/// DTO is NOT an admission, a paid ticket or permission to choose a later clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConeScopeV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub source_id: Hash64,
    pub leaf_index: u64,
    pub acquisition_until_daa: u64,
}

/// Operational limits, not native class limits. Runtime integration must obtain
/// these and all spent totals from authenticated paid state, including restart.
#[derive(Clone, Copy, Debug)]
pub(super) struct ConeContinuationCapsV1 {
    pub total_builder: PalwTirBuilderWorkLimitsV1,
    pub maximum_probes: u64,
    pub maximum_requests: u64,
    pub maximum_response_bytes: u64,
    pub maximum_copy_bytes: u64,
    pub maximum_planning_steps: u64,
    pub maximum_material_units: usize,
    pub maximum_preparation_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ConeSpentV1 {
    pub probes: u64,
    pub elements: u64,
    pub terms: u64,
    pub targets: u64,
    pub source_reads: u64,
    pub requests: u64,
    pub response_bytes: u64,
    pub copy_bytes: u64,
    pub planning_steps: u64,
}

pub(super) struct ConeContinuationBudgetV1 {
    scope: ConeScopeV1,
    caps: ConeContinuationCapsV1,
    spent: ConeSpentV1,
    pending: Option<PalwTirPublicRequestV1>,
}

fn charged_v1(old: u64, amount: u64, maximum: u64) -> Result<u64, String> {
    old.checked_add(amount).filter(|n| *n <= maximum).ok_or_else(|| "TIR cone continuation allocation is exhausted".into())
}

impl ConeContinuationBudgetV1 {
    /// Restore, never silently renew. Until a paid native reader restores these
    /// values, this helper must not be activated as a runtime paid source actor.
    pub(super) fn restore_v1(
        scope: ConeScopeV1,
        caps: ConeContinuationCapsV1,
        spent: ConeSpentV1,
        pending: Option<PalwTirPublicRequestV1>,
    ) -> Result<Self, String> {
        let budget = Self { scope, caps, spent, pending };
        budget.check_spent_v1(&spent)?;
        if scope.reserved_daa >= scope.acquisition_until_daa
            || scope.claim == Hash64::default()
            || scope.bundle_id == Hash64::default()
            || scope.source_id == Hash64::default()
            || caps.maximum_preparation_bytes == 0
            || (pending.is_some() && spent.requests == 0)
        {
            return Err("TIR cone continuation lacks an original clock or host allocation".into());
        }
        Ok(budget)
    }

    pub(super) fn spent_v1(&self) -> ConeSpentV1 {
        self.spent
    }

    fn check_spent_v1(&self, spent: &ConeSpentV1) -> Result<(), String> {
        let b = self.caps.total_builder;
        for (used, maximum) in [
            (spent.probes, self.caps.maximum_probes),
            (spent.elements, b.evaluation.max_elements),
            (spent.terms, b.evaluation.max_terms),
            (spent.targets, b.max_target_requests),
            (spent.source_reads, b.max_source_reads),
            (spent.requests, self.caps.maximum_requests),
            (spent.response_bytes, self.caps.maximum_response_bytes),
            (spent.copy_bytes, self.caps.maximum_copy_bytes),
            (spent.planning_steps, self.caps.maximum_planning_steps),
        ] {
            charged_v1(used, 0, maximum)?;
        }
        Ok(())
    }

    fn charge_probe_v1(&mut self, limits: PalwTirBuilderWorkLimitsV1, copy_budget_bytes: u64) -> Result<(), String> {
        let cap = self.caps.total_builder;
        let mut next = self.spent;
        next.probes = charged_v1(next.probes, 1, self.caps.maximum_probes)?;
        next.elements = charged_v1(next.elements, limits.evaluation.max_elements, cap.evaluation.max_elements)?;
        next.terms = charged_v1(next.terms, limits.evaluation.max_terms, cap.evaluation.max_terms)?;
        next.targets = charged_v1(next.targets, limits.max_target_requests, cap.max_target_requests)?;
        next.source_reads = charged_v1(next.source_reads, limits.max_source_reads, cap.max_source_reads)?;
        next.copy_bytes = charged_v1(next.copy_bytes, copy_budget_bytes, self.caps.maximum_copy_bytes)?;
        self.spent = next;
        Ok(())
    }

    fn charge_plan_v1(&mut self, steps: u64) -> Result<(), String> {
        self.spent.planning_steps = charged_v1(self.spent.planning_steps, steps, self.caps.maximum_planning_steps)?;
        Ok(())
    }

    fn acquire_v1(&mut self, source: &PalwTirPublicSourceV1, request: PalwTirPublicRequestV1) -> Result<ConeContinuationV1, String> {
        let bound = source.unit_bound_v1(&request)?;
        let requests = charged_v1(self.spent.requests, 1, self.caps.maximum_requests)?;
        let bytes = charged_v1(self.spent.response_bytes, bound.maximum_response_bytes, self.caps.maximum_response_bytes)?;
        self.spent.requests = requests;
        self.spent.response_bytes = bytes;
        self.pending = Some(request);
        Ok(ConeContinuationV1::Acquire(request))
    }
}

pub(super) struct PreparedConeV1 {
    pub proof: PalwCourtVerdictProofV2,
    pub meter: PalwTirBuilderMeterV1,
    // The actual proof remains accounted after the blocking wave returns.
    pub _reservation: crate::palw_memory_ledger::PalwMemoryReservationV1,
    _native: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
    _host: Vec<Box<dyn Send + Sync>>,
}

pub(super) enum ConeContinuationV1 {
    Acquire(PalwTirPublicRequestV1),
    AwaitOriginalPrompt {
        tile: Option<u32>,
    },
    AwaitAuthenticatedIds,
    AwaitRegisteredArtifact,
    /// All named leaves exist, but a canonical combined path is still absent.
    AwaitRangePath {
        first: u64,
        count: u64,
    },
    NeedMoreBudget {
        reason: String,
    },
    Refused {
        reason: String,
    },
    /// Proof construction only. Native terminal carriage, rehearsal, original
    /// attribution and clock admission remain independent mandatory checks.
    Ready(PreparedConeV1),
}

fn interval_v1(unit: &PalwTirPublicAuthenticatedUnitV1) -> Option<(u64, u64)> {
    match unit.request_v1() {
        PalwTirPublicRequestV1::StepLeaf { index } => Some((index, index.checked_add(1)?)),
        PalwTirPublicRequestV1::StepRun { first, count } => Some((first, first.checked_add(u64::from(count))?)),
        _ => None,
    }
}

/// Sorted authenticated intervals, not an iteration over every requested leaf.
fn first_gap_v1(first: u64, count: u64, intervals: &[(u64, u64)]) -> Result<Option<(u64, u64)>, String> {
    let end = first.checked_add(count).ok_or("TIR operand interval overflow")?;
    if count == 0 {
        return Err("TIR operand interval is empty".into());
    }
    let mut cursor = first;
    for &(lo, hi) in intervals {
        if hi <= cursor {
            continue;
        }
        if lo > cursor {
            return Ok(Some((cursor, lo.min(end) - cursor)));
        }
        cursor = hi.min(end);
        if cursor == end {
            return Ok(None);
        }
    }
    Ok((cursor < end).then_some((cursor, end - cursor)))
}

fn lower_cone_need_v1(
    budget: &mut ConeContinuationBudgetV1,
    source: &PalwTirPublicSourceV1,
    intervals: &[(u64, u64)],
    need: PalwTirPublicConeNeedV1,
) -> Result<ConeContinuationV1, String> {
    use PalwTirPublicConeNeedV1 as N;
    let (first, count) = match need {
        N::StepPreimage { index } | N::StepOpening { index } => (index, 1),
        N::StepRange { first, count } => (first, count),
        N::Prompt { tile } => return Ok(ConeContinuationV1::AwaitOriginalPrompt { tile }),
        N::DecodePin => return Ok(ConeContinuationV1::AwaitAuthenticatedIds),
        N::RegisteredArtifact => return Ok(ConeContinuationV1::AwaitRegisteredArtifact),
    };
    if first.checked_add(count).is_none_or(|end| end > source.leaf_count_v1()) {
        return Err("canonical TIR evaluator requested a range outside the original execution".into());
    }
    let Some((missing, length)) = first_gap_v1(first, count, intervals)? else {
        return Ok(match need {
            N::StepRange { .. } => ConeContinuationV1::AwaitRangePath { first, count },
            _ => ConeContinuationV1::NeedMoreBudget {
                reason: "the canonical credential is already authenticated; another DA demand cannot repair a reader/copy refusal"
                    .into(),
            },
        });
    };
    let count = length.min(u64::from(PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1)) as u32;
    let request = if count == 1 {
        PalwTirPublicRequestV1::StepLeaf { index: missing }
    } else {
        PalwTirPublicRequestV1::StepRun { first: missing, count }
    };
    budget.acquire_v1(source, request)
}

/// One canonical dependency wave, under a caller's already-funded original scope.
/// Only native original-root-authenticated public units enter the sparse builder.
/// The operational guard is NOT a replacement for a native paid reader/ticket;
/// no runtime activation is wired until that reader also restores spent totals.
#[allow(clippy::too_many_arguments)]
pub(super) fn continue_cone_v1(
    native: &super::tir_acquisition_driver::BorrowedPaidSnapshotV1<'_>,
    native_work: &mut super::tir_acquisition_driver::OperationalLedgerV1,
    budget: &mut ConeContinuationBudgetV1,
    source: &PalwTirPublicSourceV1,
    registered: &kaspa_consensus_core::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1,
    material: &[Arc<PalwTirPublicAuthenticatedUnitV1>],
    ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    prompt: &[u32],
    backend: &dyn PalwExecutionBackendV1,
    rules: &PalwTirCourtRulesV1,
    limits: PalwTirBuilderWorkLimitsV1,
    copy_budget_bytes: u64,
    now_daa: u64,
) -> Result<ConeContinuationV1, String> {
    let scope = budget.scope;
    native_work.require_current_v1(
        native,
        source,
        kaspa_consensus_core::palw_tir_public_owned_v1::PalwTirPublicWorkPhaseV1::ConeProbe,
        PalwTirPublicRequestV1::StepLeaf { index: scope.leaf_index },
    )?;
    let paid = *native_work.scope_v1()?;
    let (count, root) = native.material_revision_v1();
    if paid.claim != scope.claim
        || paid.owner != scope.owner
        || paid.reserved_daa != scope.reserved_daa
        || paid.bundle_id != scope.bundle_id
        || paid.input_head != root
        || paid.input_offset != count
        || native.original_clock_v1() != (scope.reserved_daa, scope.acquisition_until_daa, now_daa)
    {
        return Err("TIR cone work changes native original paid scope/clock/material revision".into());
    }
    if scope.source_id != source.source_id_v1()
        || scope.leaf_index >= source.leaf_count_v1()
        || now_daa < scope.reserved_daa
        || now_daa >= scope.acquisition_until_daa
        || material.len() > budget.caps.maximum_material_units
        || ids.is_some_and(|ids| ids.source_id_v1() != source.source_id_v1())
    {
        return Err("TIR cone wave changes its original source, clock or material allocation".into());
    }
    native_work.debit_v1(
        source.source_id_v1(),
        kaspa_consensus_core::palw_tir_public_owned_v1::PalwTirPublicWorkV1 {
            attempts: 1,
            parser_steps: (material.len() as u64).checked_mul(16).ok_or("TIR native material scan overflow")?,
            ..Default::default()
        },
    )?;
    if let Some(pending) = budget.pending {
        source.check_request_v1(&pending)?;
        if !material.iter().any(|unit| unit.source_id_v1() == source.source_id_v1() && unit.request_v1() == pending) {
            // Polling alone spends no new probe/plan quota or hydration. An
            // existing signed native demand keeps its literal clock and identity.
            return Ok(ConeContinuationV1::Acquire(pending));
        }
    }
    let logical_bytes = material.iter().try_fold(0u64, |bytes, unit| {
        let bound = source.unit_bound_v1(&unit.request_v1())?;
        bytes.checked_add(bound.maximum_response_bytes).ok_or_else(|| "TIR native material byte quote overflow".to_string())
    })?;
    let preparation_bytes = logical_bytes
        .checked_add(registered.program.len() as u64)
        .and_then(|n| n.checked_add(u64::from(source.decode_count_v1()) * 4))
        .ok_or("TIR native context preparation overflow")?;
    // Full pre-allocation debit, including preparation and every failed probe.
    // A restored local continuation budget cannot renew this accepted permit.
    native_work.debit_v1(
        source.source_id_v1(),
        kaspa_consensus_core::palw_tir_public_owned_v1::PalwTirPublicWorkV1 {
            attempts: 1,
            parser_steps: preparation_bytes.checked_mul(16).ok_or("TIR native preparation work overflow")?,
            hash_bytes: preparation_bytes.checked_mul(16).ok_or("TIR native preparation hash work overflow")?,
            copy_bytes: copy_budget_bytes
                .checked_mul(2)
                .and_then(|n| n.checked_add(preparation_bytes.checked_mul(8)?))
                .ok_or("TIR native probe copy work overflow")?,
            eval_elements: limits.evaluation.max_elements,
            eval_terms: limits.evaluation.max_terms,
            source_reads: limits.max_source_reads,
            target_requests: limits.max_target_requests,
            ..Default::default()
        },
    )?;
    let units = material.len() as u64;
    let sort_levels = u64::from(64 - units.max(1).leading_zeros());
    budget.charge_plan_v1(units.checked_mul(sort_levels + 8).ok_or("TIR planning work overflow")?)?;
    // Reserve before resolving/cloning the native program/layout, any run or IDs.
    // This estimate is operational, not a consensus resource certificate.
    let mut bytes = (registered.program.len() as u64)
        .checked_add(borsh::object_length(layout).map_err(|e| e.to_string())? as u64)
        .and_then(|n| n.checked_add(borsh::object_length(source.header_v1()).ok()? as u64))
        .ok_or("TIR original context preparation overflow")?;
    let binding_bytes = bytes;
    for unit in material {
        if unit.source_id_v1() != source.source_id_v1() {
            return Err("TIR cone material belongs to another original source".into());
        }
        if interval_v1(unit).is_some() {
            bytes = bytes
                .checked_add(binding_bytes)
                .and_then(|n| n.checked_add(borsh::object_length(unit.response_v1()).ok()? as u64))
                .ok_or("TIR public run preparation overflow")?;
        }
    }
    let evaluator_units = limits
        .evaluation
        .max_elements
        .checked_add(limits.max_source_reads)
        .and_then(|n| n.checked_add(limits.max_target_requests))
        .ok_or("TIR evaluator scratch overflow")?;
    bytes = bytes
        .checked_add(u64::from(source.decode_count_v1()) * 4)
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(evaluator_units.checked_mul(128)?))
        .and_then(|n| n.checked_add(copy_budget_bytes.checked_mul(2)?))
        .and_then(|n| n.checked_add(65_536))
        .ok_or("TIR cone host allocation overflow")?;
    if copy_budget_bytes == 0 || bytes > budget.caps.maximum_preparation_bytes {
        return Ok(ConeContinuationV1::NeedMoreBudget { reason: "TIR cone wave exceeds its held preparation/copy allocation".into() });
    }
    let reservation = crate::palw_memory_ledger::host_ledger_v1()
        .reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "public-tir-cone-wave",
                class_id: registered.facts.class_id,
                job: scope.claim,
            },
            bytes,
        )
        .map_err(|e| e.to_string())?;
    let binding = source.header_v1().resolve_registered_layout_v1(
        registered,
        layout,
        source.header_v1().committed_execution_root,
        rules.max_step_leaf_count,
    )?;
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&binding.class).map_err(|e| e.to_string())?;
    // Enumerate only through the native program's actual canonical leaf space.
    space.leaf_at(&binding.job_context, scope.leaf_index).ok_or("TIR cone output is not a canonical original leaf")?;
    let mut compact = binding;
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut compact);
    let mut intervals: Vec<_> = material.iter().filter_map(|unit| interval_v1(unit)).collect();
    intervals.sort_unstable();
    if let Some(pending) = budget.pending {
        let (first, count) = match pending {
            PalwTirPublicRequestV1::StepLeaf { index } => (index, 1),
            PalwTirPublicRequestV1::StepRun { first, count } => (first, u64::from(count)),
            _ => return Err("TIR cone pending request has another source family".into()),
        };
        if first_gap_v1(first, count, &intervals)?.is_some() {
            return Ok(ConeContinuationV1::Acquire(pending));
        }
        budget.pending = None;
    }
    if first_gap_v1(scope.leaf_index, 1, &intervals)?.is_some() {
        return budget.acquire_v1(source, PalwTirPublicRequestV1::StepLeaf { index: scope.leaf_index });
    }
    let mut runs = Vec::with_capacity(intervals.len());
    for unit in material {
        let (preimages, range) = match unit.response_v1() {
            PalwTirPublicResponseV1::StepLeaf { preimage, opening } => (
                vec![preimage.clone()],
                kaspa_consensus_core::palw_step_leg::PalwStepRangeOpeningV1 {
                    first_leaf_index: opening.leaf_index,
                    leaf_hashes: vec![opening.leaf_hash],
                    siblings: opening.siblings.clone(),
                },
            ),
            PalwTirPublicResponseV1::StepRun { preimages, range } => (preimages.clone(), range.clone()),
            _ => continue,
        };
        // Verify singleton-opening conversion too; no own leaf/root is used to
        // repair an absent path or to manufacture a range credential.
        if kaspa_consensus_core::palw_step_leg::step_range_opening_root_capped_v1(
            compact.step_leaf_count,
            &range,
            rules.max_step_leaf_count,
        )
        .map_err(|e| e.to_string())?
            != compact.step_merkle_root
        {
            return Err("TIR public range conversion changes the original step root".into());
        }
        runs.push(PalwTirStepRunDisclosureV1 { binding: compact.clone(), preimages, range });
    }
    let trace = ids.map(|ids| kaspa_consensus_core::palw_step_refute::PalwTiledDecodeTokensV1 {
        rows_root: ids.rows_root_v1(),
        generated_token_ids: ids.ids_v1().to_vec(),
    });
    // Charge the FULL allocated probe work/copy budget even on Need, Refused or
    // Err. Re-evaluating the already-read prefix never resets total paid work.
    budget.charge_probe_v1(limits, copy_budget_bytes)?;
    match backend
        .probe_tir_public_cone_owned_v2(
            source.header_v1(),
            prompt,
            trace.as_ref(),
            &runs,
            scope.leaf_index,
            rules,
            limits,
            copy_budget_bytes,
        )
        .map_err(|why| why.to_string())?
    {
        PalwTirPublicConeProbeOwnedV2::Need(need) => lower_cone_need_v1(budget, source, &intervals, need),
        PalwTirPublicConeProbeOwnedV2::Refused { reason } => Ok(ConeContinuationV1::Refused { reason }),
        PalwTirPublicConeProbeOwnedV2::Ready(ready) => {
            let (proof, native, host, meter) = ready.into_parts_v2();
            if meter.work.elements > limits.evaluation.max_elements
                || meter.work.terms > limits.evaluation.max_terms
                || meter.target_requests > limits.max_target_requests
                || meter.source_reads > limits.max_source_reads
            {
                return Ok(ConeContinuationV1::Refused { reason: "TIR cone builder exceeded this wave's allocation".into() });
            }
            Ok(ConeContinuationV1::Ready(PreparedConeV1 { proof, meter, _reservation: reservation, _native: native, _host: host }))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ArithmeticStepV1 {
    Need(PalwDaUnitV1),
    /// Localization has selected a leaf. Its preimage, decode pin and operands
    /// still have to be authenticated and judged by the native TIR court.
    Leaf {
        index: u64,
    },
    /// The independent steps agree. Trace, output and all required semantic
    /// families remain separate obligations; this never certifies arithmetic.
    StepAgreement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PublicArithmeticStepV1 {
    Acquire(PalwTirPublicRequestV1),
    Leaf { index: u64 },
    StepAgreement,
}

fn public_step_v1(step: ArithmeticStepV1) -> Result<PublicArithmeticStepV1, String> {
    Ok(match step {
        ArithmeticStepV1::Need(PalwDaUnitV1::TirStepNode { level, index }) => {
            PublicArithmeticStepV1::Acquire(PalwTirPublicRequestV1::StepNode { level, index })
        }
        ArithmeticStepV1::Leaf { index } => PublicArithmeticStepV1::Leaf { index },
        ArithmeticStepV1::StepAgreement => PublicArithmeticStepV1::StepAgreement,
        _ => return Err("TIR public arithmetic localization selected a legacy acquisition unit".into()),
    })
}

/// Prospective paid-source entry. Authentication markers are produced only by
/// the original native source; an unsigned packet or an old DA answered bit is
/// not a frontier. This returns a request, never a legacy DA accusation object.
pub(super) fn next_public_from_source_v1<'a>(
    source: &PalwTirPublicSourceV1,
    own: &dyn PalwTirOwnStepSourceV1,
    mut unit: impl FnMut(PalwTirPublicRequestV1) -> Option<&'a PalwTirPublicAuthenticatedUnitV1>,
) -> Result<PublicArithmeticStepV1, String> {
    let original = source.header_v1();
    let info = own.info_v1();
    if info.header.version != original.version
        || info.header.job_context != original.job_context
        || info.header.artifact_root != original.artifact_root
        || info.generated_ids.len() as u64 != u64::from(original.job_context.exact_decode_tokens)
        || own.node_v1(palw_tir_step_tree_height_v1(info.header.step_leaf_count), 0)? != info.header.step_merkle_root
    {
        return Err("independent TIR source is not the original registered job's rooted replay".into());
    }
    public_step_v1(next_tree_inner_v2(
        source.leaf_count_v1(),
        original.step_merkle_root,
        info.header.step_leaf_count,
        |level, index| own.node_v1(level, index),
        |level, index| {
            let request = PalwTirPublicRequestV1::StepNode { level, index };
            let Some(unit) = unit(request) else { return Ok(None) };
            if unit.source_id_v1() != source.source_id_v1() || unit.request_v1() != request {
                return Err("TIR arithmetic frontier marker belongs to another source or request".into());
            }
            match unit.response_v1() {
                PalwTirPublicResponseV1::StepNode { frontier, .. } => Ok(Some(frontier.as_slice())),
                _ => Err("TIR arithmetic frontier marker has another response kind".into()),
            }
        },
    )?)
}

fn first_difference_v1(
    count: u64,
    level: u8,
    index: u64,
    frontier: &[Hash64],
    own_node: &mut impl FnMut(u8, u64) -> Result<Hash64, String>,
) -> Result<Option<(u8, u64)>, String> {
    let (below, lo, hi) =
        palw_tir_step_node_frontier_v1(count, level, index).ok_or("TIR frontier geometry is outside the original step tree")?;
    if frontier.len() as u64 != hi - lo {
        return Err("TIR frontier has a noncanonical length".into());
    }
    for (offset, alleged) in frontier.iter().enumerate() {
        let child = lo.checked_add(offset as u64).ok_or("TIR frontier index overflow")?;
        if own_node(below, child)? != *alleged {
            return Ok(Some((below, child)));
        }
    }
    Ok(None)
}

fn next_tree_v1(
    count: u64,
    original_step_root: Hash64,
    own_count: u64,
    own_node: impl FnMut(u8, u64) -> Result<Hash64, String>,
    authenticated_frontier: impl FnMut(u8, u64) -> Result<Option<Vec<Hash64>>, String>,
) -> Result<ArithmeticStepV1, String> {
    next_tree_inner_v2(count, original_step_root, own_count, own_node, authenticated_frontier)
}

fn next_tree_inner_v2<F: AsRef<[Hash64]>>(
    count: u64,
    original_step_root: Hash64,
    own_count: u64,
    mut own_node: impl FnMut(u8, u64) -> Result<Hash64, String>,
    mut authenticated_frontier: impl FnMut(u8, u64) -> Result<Option<F>, String>,
) -> Result<ArithmeticStepV1, String> {
    if count == 0 || own_count != count {
        return Err("independent TIR execution has another step geometry; an identity proof is required".into());
    }
    let height = palw_tir_step_tree_height_v1(count);
    if own_node(height, 0)? == original_step_root {
        return Ok(ArithmeticStepV1::StepAgreement);
    }
    let (mut level, mut index) = (height, 0);
    // Every accepted frontier strictly lowers the level. No count-sized search,
    // unbounded response history or caller-provided iteration cap is needed.
    while level != 0 {
        let Some(frontier) = authenticated_frontier(level, index)? else {
            return Ok(ArithmeticStepV1::Need(PalwDaUnitV1::TirStepNode { level, index }));
        };
        let Some((below, child)) = first_difference_v1(count, level, index, frontier.as_ref(), &mut own_node)? else {
            return Err("authenticated divergent TIR node has no independently divergent child".into());
        };
        if below >= level {
            return Err("TIR frontier does not advance the search".into());
        }
        (level, index) = (below, child);
    }
    Ok(ArithmeticStepV1::Leaf { index })
}

fn check_node_v1(
    original: &PalwPublicTirBindingHeaderV1,
    level: u8,
    index: u64,
    disclosure: &PalwTirStepNodeDisclosureV1,
) -> Result<(), String> {
    // The original header comes from the native accepted claim, not this answer.
    // Node authentication needs only the original tree geometry and hashes. The
    // program is rehydrated from native registration later for semantic judging.
    if !disclosure.binding.class.program.is_empty() || PalwPublicTirBindingHeaderV1::of_binding_v1(&disclosure.binding) != *original {
        return Err("TIR arithmetic frontier changes the original public binding".into());
    }
    palw_tir_step_node_reaches_v1(
        original.step_leaf_count,
        &original.step_merkle_root,
        level,
        index,
        &disclosure.frontier,
        &disclosure.siblings,
    )
    .map_err(str::to_string)
}

/// Read-only scheduling over native accepted answers. Callers must retain the
/// independent source's resource ticket and the original paid acquisition clock.
/// This function does not authorize a new DA session or extend that clock.
pub(super) fn next_v1(
    original: &PalwPublicTirBindingHeaderV1,
    maximum_leaves: u64,
    own_count: u64,
    own_node: impl FnMut(u8, u64) -> Result<Hash64, String>,
    mut answer: impl FnMut(PalwDaUnitV1) -> Option<Arc<PalwDaAnswerV1>>,
) -> Result<ArithmeticStepV1, String> {
    if original.step_leaf_count > maximum_leaves {
        return Err("TIR arithmetic header exceeds the native class ladder".into());
    }
    next_tree_v1(original.step_leaf_count, original.step_merkle_root, own_count, own_node, |level, index| {
        match answer(PalwDaUnitV1::TirStepNode { level, index }).as_deref() {
            None => Ok(None),
            Some(PalwDaAnswerV1::TirStepNode(disclosure)) => {
                check_node_v1(original, level, index, disclosure)?;
                Ok(Some(disclosure.frontier.clone()))
            }
            Some(_) => Err("accepted TIR node key contains another answer kind".into()),
        }
    })
}

/// The runtime entry uses the backend's independent, resource-held replay, not
/// arbitrary own-tree hashes supplied alongside the accused binding.
pub(super) fn next_from_source_v1(
    original: &PalwPublicTirBindingHeaderV1,
    maximum_leaves: u64,
    own: &dyn PalwTirOwnStepSourceV1,
    answer: impl FnMut(PalwDaUnitV1) -> Option<Arc<PalwDaAnswerV1>>,
) -> Result<ArithmeticStepV1, String> {
    let info = own.info_v1();
    if info.header.version != original.version
        || info.header.job_context != original.job_context
        || info.header.artifact_root != original.artifact_root
        || info.generated_ids.len() as u64 != u64::from(original.job_context.exact_decode_tokens)
        || info.header.step_leaf_count > maximum_leaves
    {
        return Err("independent TIR source is not the original registered job's replay".into());
    }
    let own_height = palw_tir_step_tree_height_v1(info.header.step_leaf_count);
    if own.node_v1(own_height, 0)? != info.header.step_merkle_root {
        return Err("independent TIR source no longer reaches its computed step root".into());
    }
    next_v1(original, maximum_leaves, info.header.step_leaf_count, |level, index| own.node_v1(level, index), answer)
}

/// Material loss does not erase the native answered bit. Reopen only with this
/// owner's original, still-live reservation and its next accepted nonce. This is
/// the existing finite DA budget, not the prospective maximum-size paid envelope.
pub(super) fn reacquisition_v1(
    view: &PalwLegacyDisputeViewV1,
    owner: PalwBondKeyV2,
    unit: PalwDaUnitV1,
    now_daa: u64,
) -> Option<PalwDisputeReacquireV1> {
    if !matches!(
        unit,
        PalwDaUnitV1::TirStepLeaf { .. }
            | PalwDaUnitV1::TirStepNode { .. }
            | PalwDaUnitV1::TirStepRun { .. }
            | PalwDaUnitV1::TirRowNode { .. }
    ) || !view.unit_was_answered_v1(&unit)
        || view.sessions.iter().any(|(bond, _)| *bond == owner)
        || view.executor_refuted
        || view.court_convicted
        || view.da_defaulted
        || now_daa > view.hard_deadline_daa
    {
        return None;
    }
    let record = view.record.as_ref()?;
    let row = record.live.get(&owner)?;
    let session_number = row.sessions_opened.checked_add(1)?;
    if record.hard_deadline_daa != view.hard_deadline_daa
        || row.reserved_daa > now_daa
        || session_number > PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1
    {
        return None;
    }
    Some(PalwDisputeReacquireV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: view.claim_id,
        reserver: owner,
        reserved_daa: row.reserved_daa,
        session_number,
        valid_until_daa: view.hard_deadline_daa,
        unit,
        binding: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> PalwBondKeyV2 {
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
            transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(7),
            index: 0,
        })
    }

    fn view() -> PalwLegacyDisputeViewV1 {
        use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwDisputeClaimV1, PalwDisputeReservationRowV1};
        let mut record = PalwDisputeClaimV1 { opened_daa: 20, hard_deadline_daa: 80, ..Default::default() };
        record.live.insert(owner(), PalwDisputeReservationRowV1 { reserved_daa: 20, deposit: 9, sessions_opened: 3, lanes_joined: 0 });
        // A policy fixture only; production scheduling reads this view from native state.
        PalwLegacyDisputeViewV1 {
            claim_id: hash(1),
            class_id: hash(2),
            producer: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
                transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(8),
                index: 0,
            }),
            phase: kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Provisional,
            execution_root: hash(3),
            trace_root: hash(4),
            accepted_daa: 10,
            trace_retention_daa: 100,
            job_identity: hash(5),
            public_fp_job: None,
            registered_semantics: None,
            work_leaves: 64,
            deadline_daa: None,
            hard_deadline_daa: 80,
            record: Some(record),
            sessions: Vec::new(),
            answered: vec![PalwDaUnitV1::TirStepNode { level: 6, index: 0 }],
            flat_answered_rows: 0,
            open_courts: 0,
            executor_refuted: false,
            court_convicted: false,
            da_defaulted: false,
        }
    }

    fn hash(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn cone_budget() -> (ConeContinuationBudgetV1, PalwTirBuilderWorkLimitsV1) {
        let per_wave = PalwTirBuilderWorkLimitsV1 {
            evaluation: misaka_palw_tir::demand::DemandLimits { max_elements: 10, max_terms: 20 },
            max_target_requests: 5,
            max_source_reads: 30,
        };
        let caps = ConeContinuationCapsV1 {
            total_builder: PalwTirBuilderWorkLimitsV1 {
                evaluation: misaka_palw_tir::demand::DemandLimits { max_elements: 20, max_terms: 40 },
                max_target_requests: 10,
                max_source_reads: 60,
            },
            maximum_probes: 2,
            maximum_requests: 10,
            maximum_response_bytes: 1_000_000,
            maximum_copy_bytes: 100,
            maximum_planning_steps: 1_000,
            maximum_material_units: 10,
            maximum_preparation_bytes: 1_000_000,
        };
        let scope = ConeScopeV1 {
            claim: hash(1),
            owner: owner(),
            reserved_daa: 10,
            bundle_id: hash(2),
            source_id: hash(3),
            leaf_index: 7,
            acquisition_until_daa: 100,
        };
        (ConeContinuationBudgetV1::restore_v1(scope, caps, ConeSpentV1::default(), None).unwrap(), per_wave)
    }

    #[test]
    fn missing_and_refused_probe_waves_cannot_reset_cumulative_work_or_copy_budget() {
        let (mut budget, per_wave) = cone_budget();
        // Charged BEFORE the call; Need, Refused and Err receive no refund.
        budget.charge_probe_v1(per_wave, 50).unwrap();
        budget.charge_probe_v1(per_wave, 50).unwrap();
        let spent = budget.spent_v1();
        assert_eq!((spent.probes, spent.elements, spent.terms, spent.source_reads, spent.copy_bytes), (2, 20, 40, 60, 100));
        assert!(budget.charge_probe_v1(per_wave, 1).is_err());
        assert_eq!(budget.spent_v1(), spent, "a rejected charge is atomic");
        let mut restored = ConeContinuationBudgetV1::restore_v1(budget.scope, budget.caps, spent, None).unwrap();
        assert!(restored.charge_probe_v1(per_wave, 1).is_err(), "restore does not buy another probe");
        assert_eq!(restored.scope.acquisition_until_daa, 100);
    }

    #[test]
    fn canonical_range_over_one_source_unit_advances_past_authenticated_prefixes() {
        assert_eq!(first_gap_v1(0, 1_024, &[(0, 256), (256, 512), (768, 1_024)]).unwrap(), Some((512, 256)));
        assert_eq!(first_gap_v1(0, 1_024, &[(0, 256), (256, 512), (512, 768), (768, 1_024)]).unwrap(), None);
        assert_eq!(first_gap_v1(500, 600, &[(0, 768), (768, 1_024)]).unwrap(), Some((1_024, 76)));
        assert!(first_gap_v1(u64::MAX, 1, &[]).is_err());
        assert!(first_gap_v1(0, 0, &[]).is_err());
    }

    #[test]
    fn malformed_restored_spending_and_counter_overflow_do_not_create_authority() {
        let (budget, _) = cone_budget();
        let spent = ConeSpentV1 { probes: 3, ..Default::default() };
        assert!(ConeContinuationBudgetV1::restore_v1(budget.scope, budget.caps, spent, None).is_err());
        assert!(
            ConeContinuationBudgetV1::restore_v1(
                budget.scope,
                budget.caps,
                ConeSpentV1::default(),
                Some(PalwTirPublicRequestV1::StepLeaf { index: 7 }),
            )
            .is_err()
        );
        assert!(charged_v1(u64::MAX, 1, u64::MAX).is_err());
    }

    #[test]
    fn maximum_ladder_descent_uses_typed_nodes_without_seat_reach_cap() {
        // Geometry fixture, not a cryptographic or native admission credential.
        let count = 1u64 << 40;
        let target = count - 1;
        let mut requests = Vec::new();
        let step = next_tree_v1(
            count,
            hash(2),
            count,
            |_, _| Ok(hash(1)),
            |level, index| {
                requests.push((level, index));
                let (below, lo, hi) = palw_tir_step_node_frontier_v1(count, level, index).unwrap();
                let mut frontier = vec![hash(1); (hi - lo) as usize];
                frontier[((target >> below) - lo) as usize] = hash(2);
                Ok(Some(frontier))
            },
        )
        .unwrap();
        assert_eq!(step, ArithmeticStepV1::Leaf { index: target });
        assert_eq!(requests, vec![(40, 0), (30, 1023), (20, (1 << 20) - 1), (10, (1 << 30) - 1)]);
    }

    #[test]
    fn public_descent_uses_new_request_domain_and_does_not_accept_legacy_units() {
        assert_eq!(
            public_step_v1(ArithmeticStepV1::Need(PalwDaUnitV1::TirStepNode { level: 40, index: 0 })).unwrap(),
            PublicArithmeticStepV1::Acquire(PalwTirPublicRequestV1::StepNode { level: 40, index: 0 })
        );
        assert_eq!(public_step_v1(ArithmeticStepV1::StepAgreement).unwrap(), PublicArithmeticStepV1::StepAgreement);
        assert!(public_step_v1(ArithmeticStepV1::Need(PalwDaUnitV1::Event { row: 0, tile: 0 })).is_err());
    }

    #[test]
    fn missing_accepted_frontier_does_not_advance_from_unsigned_or_old_progress() {
        let count = 1u64 << 40;
        assert_eq!(
            next_tree_v1(count, hash(2), count, |_, _| Ok(hash(1)), |_, _| Ok(None)).unwrap(),
            ArithmeticStepV1::Need(PalwDaUnitV1::TirStepNode { level: 40, index: 0 })
        );
    }

    #[test]
    fn step_agreement_is_distinct_from_honest_and_does_not_request_a_leaf() {
        assert_eq!(
            next_tree_v1(1, hash(1), 1, |_, _| Ok(hash(1)), |_, _| panic!("no frontier needed")).unwrap(),
            ArithmeticStepV1::StepAgreement
        );
        assert_eq!(
            next_tree_v1(1, hash(2), 1, |_, _| Ok(hash(1)), |_, _| panic!("a singleton has no interior node")).unwrap(),
            ArithmeticStepV1::Leaf { index: 0 }
        );
    }

    #[test]
    fn noncanonical_frontier_and_changed_geometry_are_not_localization() {
        assert!(next_tree_v1(5, hash(2), 5, |_, _| Ok(hash(1)), |_, _| Ok(Some(vec![hash(2); 4]))).is_err());
        assert!(next_tree_v1(5, hash(2), 6, |_, _| Ok(hash(1)), |_, _| panic!("geometry checked first")).is_err());
        assert!(next_tree_v1(0, hash(2), 0, |_, _| Ok(hash(1)), |_, _| panic!("empty tree refused")).is_err());
    }

    #[test]
    fn own_source_failure_or_inconsistent_children_cannot_become_agreement() {
        assert!(next_tree_v1(8, hash(2), 8, |_, _| Err("own disk read failed".into()), |_, _| Ok(None)).is_err());
        assert!(next_tree_v1(8, hash(2), 8, |_, _| Ok(hash(1)), |_, _| Ok(Some(vec![hash(1); 8]))).is_err());
    }

    #[test]
    fn reacquisition_uses_original_native_reservation_nonce_and_absolute_clock() {
        let view = view();
        let unit = view.answered[0];
        let request = reacquisition_v1(&view, owner(), unit, 30).unwrap();
        assert_eq!(request.claim, view.claim_id);
        assert_eq!(request.reserved_daa, 20);
        assert_eq!(request.session_number, 4);
        assert_eq!(request.valid_until_daa, 80);
        assert_eq!(request.binding, None);
        assert_eq!(request.unit, unit);
        assert!(reacquisition_v1(&view, owner(), unit, 19).is_none());
        assert!(reacquisition_v1(&view, owner(), unit, 81).is_none());
        assert!(reacquisition_v1(&view, owner(), PalwDaUnitV1::TirStepLeaf { index: 0 }, 30).is_none());
    }

    #[test]
    fn paid_semantic_row_or_exhausted_da_budget_cannot_authorize_a_fresh_reacquisition() {
        let mut view = view();
        let unit = view.answered[0];
        view.record.as_mut().unwrap().live.get_mut(&owner()).unwrap().sessions_opened = PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
        view.record = None;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
        let mut view = self::view();
        view.record.as_mut().unwrap().hard_deadline_daa += 1;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
        view.record.as_mut().unwrap().hard_deadline_daa -= 1;
        view.da_defaulted = true;
        assert!(reacquisition_v1(&view, owner(), unit, 30).is_none());
    }
}
