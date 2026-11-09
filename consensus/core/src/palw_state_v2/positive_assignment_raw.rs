//! Native raw replay inputs. These retain actual transport material, never a
//! prepared admission, saved success or caller-supplied computation verdict.
use super::*;
use crate::tx::{Transaction, UtxoEntry};
use crate::{
    block::BlockTransactionsV3,
    config::params::{
        AssignmentFundingRelayProfileV2, AssignmentNativePreimagesV2, AssignmentServiceCapacityV2, Params,
        PositiveAssignmentRuleScopeV2,
    },
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1},
    palw_positive_assignment_certification_v2::{AssignmentGateCClockErrorV2, AssignmentGateCClockV2},
    palw_positive_assignment_material_v2::{AssignmentAdmissionMaterialV2, AssignmentRegisteredDescriptorV2},
    palw_positive_court_v2::coordinator::assignment_body::AssignmentBodyLimitsV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId, funding_relay_policy::FundingRelayPhysicalProfileV2, funding_relay_rows::FundingRelayCarrierFeesV2,
        policy::AssignmentPolicyV2,
    },
    identity::Digest,
    rows::{AcceptedMaterial, AcceptedOrigin},
};
use std::sync::Arc;

/// Historical material replayed by the native selected-parent reader. The
/// final fold must still match the current pending row and its actual charge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativePendingRelayReplayV2 {
    pub selected_parent: BlockHash,
    pub reservation: AcceptedMaterial,
    pub fees: FundingRelayCarrierFeesV2,
    pub physical: FundingRelayPhysicalProfileV2,
}

/// Immutable policy data from the native intake Params or the rooted original
/// material of a continuation. The fold checks the current prefix before use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeAssignmentRulesV2 {
    scope: PositiveAssignmentRuleScopeV2,
    policy: AssignmentPolicyV2,
    positive: crate::config::params::PositiveCourtPolicyV2,
    preimages: Option<AssignmentNativePreimagesV2>,
    funding_relay_profile: AssignmentFundingRelayProfileV2,
    original_material: Option<(AcceptedMaterial, u64)>,
    network: Digest,
    prompt_form: PalwPromptIdsFormV1,
    gate_c: Option<crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1>,
    service_capacity: Option<(u64, AssignmentServiceCapacityV2)>,
}
impl NativeAssignmentRulesV2 {
    pub fn resolve(params: &Params, daa: u64, network: Hash64, prompt_form: PalwPromptIdsFormV1) -> Option<Self> {
        let fence = params.palw_positive_assignment_v2_at(daa)?;
        let scope = params.palw_positive_assignment_rule_scope_v2_at(daa)?;
        let positive = params.palw_positive_court_v2_at(daa)?.policy.clone();
        let gate_c = params.palw_rfc0018_expected_mirror_v1().filter(|m| m.gate_c_at(daa)).map(|m| m.certification);
        Some(Self {
            scope,
            policy: fence.policy,
            positive,
            preimages: fence.native_preimages.clone(),
            funding_relay_profile: AssignmentFundingRelayProfileV2::capture_at_intake(params, daa).ok()?,
            original_material: None,
            network: Digest(network.as_bytes()),
            prompt_form,
            gate_c,
            service_capacity: fence.service_capacity.map(|capacity| (daa, capacity)),
        })
    }
    /// Reconstruct original decode rules from a checked rooted material pair.
    /// Native still owns accepted ancestry and the current-prefix CAS.
    pub fn from_original_material_data_v2(
        row: &misaka_palw_positive_contract::assignment::rows::AssignmentRow,
        material: &crate::palw_positive_assignment_material_v2::AssignmentAdmissionMaterialV2,
    ) -> Result<Self, misaka_palw_positive_contract::ContractError> {
        use misaka_palw_positive_contract::ContractError;
        material.check_row(row)?;
        let scope = PositiveAssignmentRuleScopeV2::from_retained_data(material.fired_rules, &material.assignment_policy)
            .filter(|scope| scope.ruleset().as_bytes() == material.ruleset.0)
            .ok_or(ContractError::Scope)?;
        Ok(Self {
            scope,
            policy: material.assignment_policy,
            positive: material.positive_policy.clone(),
            preimages: Some(material.native_preimages.clone()),
            funding_relay_profile: material.funding_relay,
            original_material: Some((row.admission_material, material.native_assignment_journal_storage_limit_v2())),
            network: material.network,
            prompt_form: material.network_prompt_form,
            gate_c: Some(material.certification_policy),
            service_capacity: None,
        })
    }
    /// New joins use today's shared supply while preserving the original
    /// contract and rules that govern already accepted obligations.
    pub fn with_current_service_capacity_v2(mut self, params: &Params, daa: u64) -> Self {
        self.service_capacity =
            params.palw_positive_assignment_v2_at(daa).and_then(|fence| fence.service_capacity).map(|capacity| (daa, capacity));
        self
    }
    pub(super) fn service_capacity_at_v2(&self, daa: u64) -> Option<AssignmentServiceCapacityV2> {
        self.service_capacity.filter(|(at, _)| *at == daa).map(|(_, capacity)| capacity)
    }
    pub(super) fn original_material(&self) -> Option<AcceptedMaterial> {
        self.original_material.map(|(material, _)| material)
    }
    pub(super) fn original_history_storage_limit_v2(&self) -> Option<u64> {
        self.original_material.map(|(_, bytes)| bytes)
    }
    pub fn rule_scope(&self) -> PositiveAssignmentRuleScopeV2 {
        self.scope
    }
    pub fn assignment_policy(&self) -> &AssignmentPolicyV2 {
        &self.policy
    }
    pub fn positive_policy(&self) -> &crate::config::params::PositiveCourtPolicyV2 {
        &self.positive
    }
    pub fn native_preimages(&self) -> Option<&AssignmentNativePreimagesV2> {
        self.preimages.as_ref()
    }
    /// Selected at new intake or reconstructed from that contract's original
    /// material for a continuation.
    pub fn funding_relay_profile(&self) -> &AssignmentFundingRelayProfileV2 {
        &self.funding_relay_profile
    }
    pub fn network(&self) -> Digest {
        self.network
    }
    pub fn prompt_form(&self) -> PalwPromptIdsFormV1 {
        self.prompt_form
    }
    pub fn body_limits(
        &self,
    ) -> Result<AssignmentBodyLimitsV2, crate::palw_positive_court_v2::coordinator::assignment_body::AssignmentBodyErrorV2> {
        AssignmentBodyLimitsV2::from_policy(&self.policy)
    }
    pub(crate) fn certification_policy(&self) -> Option<&crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1> {
        self.gate_c.as_ref()
    }
    pub fn gate_c_clock(&self, quoted_result_daa: u64) -> Result<AssignmentGateCClockV2, AssignmentGateCClockErrorV2> {
        AssignmentGateCClockV2::from_existing_policy(
            self.gate_c.as_ref().ok_or(AssignmentGateCClockErrorV2::MissingActualPolicy)?,
            quoted_result_daa,
        )
    }
}
/// Actual accepted transport bytes/inputs and native origin. This remains DATA.
/// The only production producer is the private Full-validator companion; final
/// execution still scans and resolves against its current TransitionBuilder.
#[derive(Clone)]
pub struct PositiveAssignmentRawActionV2(Arc<PositiveAssignmentRawDataV2>);
struct PositiveAssignmentRawDataV2 {
    body: BlockTransactionsV3,
    index: u32,
    entries: Vec<UtxoEntry>,
    fee: u64,
    origin: AcceptedOrigin,
    selected_parent: BlockHash,
    current_output: Option<UtxoEntry>,
    rules: NativeAssignmentRulesV2,
    case_backing: Option<Arc<NativeAssignedCaseBackingV2>>,
    original_open: Option<Arc<NativeAcceptedCaseCarriageV2>>,
    original_admission: Option<Arc<AssignmentAdmissionMaterialV2>>,
    signed_anchor: Option<(BlockHash, u64)>,
    registered_class: Option<AssignmentRegisteredDescriptorV2>,
    relay_carrier_fees: Option<FundingRelayCarrierFeesV2>,
    pending_relay_replay: Option<NativePendingRelayReplayV2>,
    case_relay_contract: Option<(AssignmentId, AcceptedMaterial)>,
    _transport: Arc<NativeDemandLeaseV1>,
    _decode: Arc<NativeDemandLeaseV1>,
}
impl PositiveAssignmentRawActionV2 {
    /// Checked transport assembly only. No accepted-state capability is issued.
    #[allow(clippy::too_many_arguments)]
    pub fn from_native_transport_v2(
        body: BlockTransactionsV3,
        index: u32,
        entries: Vec<UtxoEntry>,
        fee: u64,
        origin: AcceptedOrigin,
        selected_parent: BlockHash,
        current_output: Option<UtxoEntry>,
        rules: NativeAssignmentRulesV2,
        transport: NativeDemandLeaseV1,
        decode: NativeDemandLeaseV1,
    ) -> Result<Self, NativeCoverageV1> {
        let tx = body
            .get(index as usize)
            .ok_or_else(|| NativeCoverageV1::unsupported::<Self>("Assignment raw action", "missing actual carrier index"))?;
        if tx.is_coinbase() || tx.id().as_bytes() != origin.carrier_transaction.0 || tx.inputs.len() != entries.len() {
            return Err(NativeCoverageV1::unsupported::<Self>("Assignment raw action", "actual transaction/origin/input mismatch"));
        }
        Ok(Self(Arc::new(PositiveAssignmentRawDataV2 {
            body,
            index,
            entries,
            fee,
            origin,
            selected_parent,
            current_output,
            rules,
            case_backing: None,
            original_open: None,
            original_admission: None,
            signed_anchor: None,
            registered_class: None,
            relay_carrier_fees: None,
            pending_relay_replay: None,
            case_relay_contract: None,
            _transport: Arc::new(transport),
            _decode: Arc::new(decode),
        })))
    }
    pub fn retained_owner_bytes_v2() -> u64 {
        let arc_data = std::mem::size_of::<PositiveAssignmentRawDataV2>()
            + std::mem::size_of::<[usize; 2]>()
            + std::mem::align_of::<PositiveAssignmentRawDataV2>();
        let arc_ticket = std::mem::size_of::<NativeDemandLeaseV1>()
            + std::mem::size_of::<[usize; 2]>()
            + std::mem::align_of::<NativeDemandLeaseV1>();
        (arc_data + 2 * arc_ticket) as u64
    }
    /// Attach the signed ancestor's clock after native reachability/header
    /// lookup. The fold binds it to the literal op8/op14 and accepting point.
    pub fn with_signed_anchor_clock_v2(mut self, block: BlockHash, daa: u64) -> Result<Self, NativeCoverageV1> {
        let error = || NativeCoverageV1::unsupported::<Self>("Assignment signed ancestor", "wrong operation, clock or shared owner");
        let data = Arc::get_mut(&mut self.0).ok_or_else(error)?;
        if data.signed_anchor.is_some()
            || !matches!(data.body[data.index as usize].payload.get(7), Some(8 | 14))
            || daa > data.origin.accepted_daa
        {
            return Err(error());
        }
        data.signed_anchor = Some((block, daa));
        Ok(self)
    }
    pub(super) fn signed_anchor_clock_v2(&self) -> Option<(BlockHash, u64)> {
        self.0.signed_anchor
    }
    /// Carry the selected-parent archive's registration data to the exact
    /// accepted prefix, where the fold checks the current rooted class again.
    pub fn with_registered_class_v2(mut self, descriptor: AssignmentRegisteredDescriptorV2) -> Result<Self, NativeCoverageV1> {
        let invalid = || NativeCoverageV1::unsupported::<Self>("Assignment class carriage", "wrong operation, clock or shared owner");
        let data = Arc::get_mut(&mut self.0).ok_or_else(invalid)?;
        if data.registered_class.is_some()
            || data.body[data.index as usize].payload.get(7) != Some(&8)
            || descriptor.registration.origin.accepted_daa > data.origin.accepted_daa
            || descriptor.registration.origin.accepting_block == data.origin.accepting_block
        {
            return Err(invalid());
        }
        data.registered_class = Some(descriptor);
        Ok(self)
    }

    pub(super) fn registered_class_v2(&self) -> Option<AssignmentRegisteredDescriptorV2> {
        self.0.registered_class
    }

    /// Transport the fee partition observed at the actual UTXO fee sink.
    pub fn with_relay_carrier_fees_v2(mut self, fees: FundingRelayCarrierFeesV2) -> Result<Self, NativeCoverageV1> {
        let invalid = || NativeCoverageV1::unsupported::<Self>("relay carrier fees", "wrong operation, gross fee or shared owner");
        let data = Arc::get_mut(&mut self.0).ok_or_else(invalid)?;
        if data.relay_carrier_fees.is_some()
            || data.body[data.index as usize].payload.get(7) != Some(&14)
            || fees.gross_fee() != data.fee
        {
            return Err(invalid());
        }
        data.relay_carrier_fees = Some(fees);
        Ok(self)
    }

    pub fn relay_carrier_fees_v2(&self) -> Option<FundingRelayCarrierFeesV2> {
        self.0.relay_carrier_fees
    }

    pub fn with_pending_relay_replay_v2(mut self, replay: NativePendingRelayReplayV2) -> Result<Self, NativeCoverageV1> {
        let invalid = || NativeCoverageV1::unsupported::<Self>("pending relay replay", "wrong operation, selected parent or owner");
        let data = Arc::get_mut(&mut self.0).ok_or_else(invalid)?;
        if data.pending_relay_replay.is_some()
            || !matches!(data.body[data.index as usize].payload.get(7), Some(8 | 9 | 14))
            || replay.selected_parent != data.selected_parent
            || replay.reservation.origin.accepting_block.0 == data.origin.accepting_block.0
        {
            return Err(invalid());
        }
        data.pending_relay_replay = Some(replay);
        Ok(self)
    }

    pub(super) fn pending_relay_replay_v2(&self) -> Option<NativePendingRelayReplayV2> {
        self.0.pending_relay_replay
    }

    pub fn with_case_relay_contract_v2(
        mut self,
        assignment: AssignmentId,
        material: AcceptedMaterial,
    ) -> Result<Self, NativeCoverageV1> {
        let invalid = || NativeCoverageV1::unsupported::<Self>("Case relay contract", "wrong operation, clock or owner");
        let data = Arc::get_mut(&mut self.0).ok_or_else(invalid)?;
        if data.case_relay_contract.is_some()
            || data.body[data.index as usize].payload.get(7) != Some(&14)
            || material.origin.accepted_blue_score >= data.origin.accepted_blue_score
        {
            return Err(invalid());
        }
        data.case_relay_contract = Some((assignment, material));
        Ok(self)
    }

    pub(super) fn case_relay_contract_v2(&self) -> Option<(AssignmentId, AcceptedMaterial)> {
        self.0.case_relay_contract
    }

    /// Retain the original accepted op8 and executed prefix for op9 replay.
    pub fn with_original_open_carriage_v2(
        mut self,
        carriage: NativeAcceptedCaseCarriageV2,
        admission: AssignmentAdmissionMaterialV2,
    ) -> Result<Self, NativeCoverageV1> {
        let error =
            || NativeCoverageV1::unsupported::<Self>("original Assignment carriage", "wrong operation, origin or shared owner");
        let data = Arc::get_mut(&mut self.0).ok_or_else(error)?;
        if data.original_open.is_some()
            || data.original_admission.is_some()
            || data.rules.original_material().map(|material| material.origin) != Some(carriage.origin())
            || data.body[data.index as usize].payload.get(7) != Some(&9)
        {
            return Err(error());
        }
        let bytes = (std::mem::size_of::<NativeAcceptedCaseCarriageV2>()
            + std::mem::size_of::<[usize; 2]>()
            + std::mem::align_of::<NativeAcceptedCaseCarriageV2>()) as u64;
        let coverage = admission.native_heap_v1("original Assignment admission");
        let heap = coverage.bytes().ok_or(coverage)?;
        let bytes = bytes
            .checked_add(heap)
            .and_then(|n| {
                n.checked_add((std::mem::size_of::<AssignmentAdmissionMaterialV2>() + 2 * std::mem::size_of::<usize>()) as u64)
            })
            .ok_or_else(error)?;
        Arc::get_mut(&mut data._decode).ok_or_else(error)?.grow_memory_v1(bytes)?;
        data.original_admission = Some(Arc::new(admission));
        data.original_open = Some(Arc::new(carriage));
        Ok(self)
    }
    pub(super) fn original_open_carriage_v2(&self) -> Option<&NativeAcceptedCaseCarriageV2> {
        self.0.original_open.as_deref()
    }
    pub(super) fn original_admission_v2(&self) -> Option<&AssignmentAdmissionMaterialV2> {
        self.0.original_admission.as_deref()
    }
    pub fn with_native_case_backing_v2(
        mut self,
        backing: NativeAssignedCaseBackingV2,
    ) -> Result<Self, NativeAssignedCaseBackingErrorV2> {
        let data = Arc::get_mut(&mut self.0).ok_or(NativeAssignedCaseBackingErrorV2::Shape)?;
        if data.case_backing.is_some() {
            return Err(NativeAssignedCaseBackingErrorV2::Shape);
        }
        // Backing's native archive owner already retains its payload. Reserve
        // this additional Arc allocation before creating the shared owner.
        let bytes = (std::mem::size_of::<NativeAssignedCaseBackingV2>()
            + std::mem::size_of::<[usize; 2]>()
            + std::mem::align_of::<NativeAssignedCaseBackingV2>()) as u64;
        Arc::get_mut(&mut data._decode)
            .ok_or(NativeAssignedCaseBackingErrorV2::Shape)?
            .grow_memory_v1(bytes)
            .map_err(NativeAssignedCaseBackingErrorV2::NativeResource)?;
        data.case_backing = Some(Arc::new(backing));
        Ok(self)
    }
    pub fn case_backing(&self) -> Result<&NativeAssignedCaseBackingV2, NativeAssignedCaseBackingErrorV2> {
        self.0.case_backing.as_deref().ok_or(NativeAssignedCaseBackingErrorV2::Unavailable)
    }
    pub fn transaction(&self) -> &Transaction {
        &self.0.body[self.0.index as usize]
    }
    pub fn entries(&self) -> &[UtxoEntry] {
        &self.0.entries
    }
    pub fn fee(&self) -> u64 {
        self.0.fee
    }
    pub fn origin(&self) -> AcceptedOrigin {
        self.0.origin
    }
    pub fn selected_parent(&self) -> BlockHash {
        self.0.selected_parent
    }
    pub fn current_output(&self) -> Option<&UtxoEntry> {
        self.0.current_output.as_ref()
    }
    pub fn rules(&self) -> &NativeAssignmentRulesV2 {
        &self.0.rules
    }
}
impl std::fmt::Debug for PositiveAssignmentRawActionV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PositiveAssignmentRawActionV2")
            .field("transaction", &self.transaction().id())
            .field("origin", &self.origin())
            .finish()
    }
}
impl PartialEq for PositiveAssignmentRawActionV2 {
    fn eq(&self, other: &Self) -> bool {
        self.transaction() == other.transaction()
            && self.entries() == other.entries()
            && self.fee() == other.fee()
            && self.origin() == other.origin()
            && self.selected_parent() == other.selected_parent()
            && self.current_output() == other.current_output()
            && self.rules() == other.rules()
            && self.signed_anchor_clock_v2() == other.signed_anchor_clock_v2()
            && self.registered_class_v2() == other.registered_class_v2()
            && self.relay_carrier_fees_v2() == other.relay_carrier_fees_v2()
            && self.pending_relay_replay_v2() == other.pending_relay_replay_v2()
            && match (&self.0.original_admission, &other.0.original_admission) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
            && match (&self.0.original_open, &other.0.original_open) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
            && match (&self.0.case_backing, &other.0.case_backing) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}
impl Eq for PositiveAssignmentRawActionV2 {}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositiveAssignmentScheduledActionV2 {
    pub before_object: u32,
    pub action: PositiveAssignmentRawActionV2,
}
impl NativeHeapV1 for NativeAssignmentRulesV2 {
    fn native_heap_v1(&self, _path: &str) -> NativeCoverageV1 {
        // All fields are bounded fixed policy/preimage values. No Params or
        // peer variable collection is hidden behind this immutable descriptor.
        let Self {
            scope,
            policy,
            positive,
            preimages,
            funding_relay_profile,
            original_material,
            network,
            prompt_form,
            gate_c,
            service_capacity,
        } = self;
        fn require_fixed_copy<T: Copy>(_: &T) {}
        require_fixed_copy(scope);
        require_fixed_copy(funding_relay_profile);
        require_fixed_copy(original_material);
        require_fixed_copy(policy);
        require_fixed_copy(network);
        require_fixed_copy(prompt_form);
        require_fixed_copy(gate_c);
        require_fixed_copy(service_capacity);
        if let Some(AssignmentNativePreimagesV2 { assignment_fees, physical, source_policy, case_fees, cancellation }) = preimages {
            require_fixed_copy(assignment_fees);
            require_fixed_copy(physical);
            require_fixed_copy(source_policy);
            require_fixed_copy(case_fees);
            require_fixed_copy(cancellation);
        }
        let crate::config::params::PositiveCourtPolicyV2 {
            version,
            revision,
            profile,
            derivation_id,
            selector_id,
            service_profile_id,
            funding_schedule_id,
            max_frame_bytes,
            max_part_heap,
            max_relations,
            max_attempts,
            max_all_attempt_bytes,
            max_all_attempt_elements,
            max_all_attempt_terms,
            max_case_heap_bytes,
            max_case_journal_bytes,
            max_case_work,
            max_retention_daa,
        } = positive;
        require_fixed_copy(version);
        require_fixed_copy(revision);
        require_fixed_copy(profile);
        require_fixed_copy(derivation_id);
        require_fixed_copy(selector_id);
        require_fixed_copy(service_profile_id);
        require_fixed_copy(funding_schedule_id);
        require_fixed_copy(max_frame_bytes);
        require_fixed_copy(max_part_heap);
        require_fixed_copy(max_relations);
        require_fixed_copy(max_attempts);
        require_fixed_copy(max_all_attempt_bytes);
        require_fixed_copy(max_all_attempt_elements);
        require_fixed_copy(max_all_attempt_terms);
        require_fixed_copy(max_case_heap_bytes);
        require_fixed_copy(max_case_journal_bytes);
        require_fixed_copy(max_case_work);
        require_fixed_copy(max_retention_daa);
        NativeCoverageV1::zero()
    }
}
impl NativeHeapV1 for PositiveAssignmentRawActionV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let PositiveAssignmentRawDataV2 {
            body,
            index: _,
            entries,
            fee: _,
            origin: _,
            selected_parent: _,
            current_output,
            rules,
            case_backing,
            original_open,
            original_admission,
            signed_anchor: _,
            registered_class: _,
            relay_carrier_fees: _,
            pending_relay_replay: _,
            case_relay_contract: _,
            _transport: _,
            _decode: _,
        } = &*self.0;
        NativeCoverageV1::Bounded { bytes: Self::retained_owner_bytes_v2() }
            .add::<Self>(body.native_heap_v1(path), path)
            .add::<Self>(entries.native_heap_v1(path), path)
            .add::<Self>(current_output.native_heap_v1(path), path)
            .add::<Self>(rules.native_heap_v1(path), path)
            .add::<Self>(case_backing.native_heap_v1(path), path)
            .add::<Self>(original_open.native_heap_v1(path), path)
            .add::<Self>(original_admission.native_heap_v1(path), path)
    }
}
impl NativeHeapV1 for PositiveAssignmentScheduledActionV2 {
    fn native_heap_v1(&self, path: &str) -> NativeCoverageV1 {
        let Self { before_object: _, action } = self;
        action.native_heap_v1(path)
    }
}

#[cfg(test)]
mod tests;
