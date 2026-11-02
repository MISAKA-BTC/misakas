//! Native raw replay inputs. These retain actual transport material, never a
//! prepared admission, saved success or caller-supplied computation verdict.
use super::*;
use crate::tx::{Transaction, UtxoEntry};
use crate::{
    block::BlockTransactionsV3,
    config::params::{AssignmentFundingRelayProfileV2, AssignmentNativePreimagesV2, Params, PositiveAssignmentRuleScopeV2},
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1},
    palw_positive_assignment_certification_v2::{AssignmentGateCClockErrorV2, AssignmentGateCClockV2},
    palw_positive_court_v2::coordinator::assignment_body::AssignmentBodyLimitsV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
};
use misaka_palw_positive_contract::{assignment::policy::AssignmentPolicyV2, identity::Digest, rows::AcceptedOrigin};
use std::sync::Arc;

/// Immutable policy DATA selected from the actual native Params snapshot at
/// this accepting clock. Possession does not authorize a row mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeAssignmentRulesV2 {
    scope: PositiveAssignmentRuleScopeV2,
    policy: AssignmentPolicyV2,
    positive: crate::config::params::PositiveCourtPolicyV2,
    preimages: Option<AssignmentNativePreimagesV2>,
    intake_funding_relay: AssignmentFundingRelayProfileV2,
    network: Digest,
    prompt_form: PalwPromptIdsFormV1,
    gate_c: Option<crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1>,
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
            intake_funding_relay: AssignmentFundingRelayProfileV2::capture_at_intake(params, daa).ok()?,
            network: Digest(network.as_bytes()),
            prompt_form,
            gate_c,
        })
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
    /// NEW intake selection only. Continuing contracts use their retained
    /// original material, including when current intake is later changed.
    pub fn intake_funding_relay(&self) -> &AssignmentFundingRelayProfileV2 {
        &self.intake_funding_relay
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
        let Self { scope, policy, positive, preimages, intake_funding_relay, network, prompt_form, gate_c } = self;
        fn require_fixed_copy<T: Copy>(_: &T) {}
        require_fixed_copy(scope);
        require_fixed_copy(intake_funding_relay);
        require_fixed_copy(policy);
        require_fixed_copy(network);
        require_fixed_copy(prompt_form);
        require_fixed_copy(gate_c);
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
            _transport: _,
            _decode: _,
        } = &*self.0;
        NativeCoverageV1::Bounded { bytes: Self::retained_owner_bytes_v2() }
            .add::<Self>(body.native_heap_v1(path), path)
            .add::<Self>(entries.native_heap_v1(path), path)
            .add::<Self>(current_output.native_heap_v1(path), path)
            .add::<Self>(rules.native_heap_v1(path), path)
            .add::<Self>(case_backing.native_heap_v1(path), path)
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
