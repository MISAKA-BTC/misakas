//! Bounded private G14/Positive role coordinator. There is one Miner-result
//! scope, one all-Panel service ledger, and at most one active witness candidate.
//! This does not create native coins, slash authority, or a credit capability.
use super::{
    AdmissionScopeV2, ClaimStatusV2, ErrorV2, ExactCaseV2, ExactPolicyV2, MinerResultV2, PanelWitnessV2, RESULT_CONTEXT, SignatureV2,
    WITNESS_CONTEXT, WitnessStatusV2, digest,
    service::{RequestEffectV2, ServiceLedgerV2, ServiceMeterV2, ServicePolicyV2, WorkRequestV2},
};
use crate::{
    Hash64,
    mldsa87_primitives::{MLDSA87_PUBKEY_LEN, mldsa87_key_id},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_leg::PalwStepOpeningV1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use std::collections::BTreeMap;

pub mod admission;
pub mod assignment_body;
pub mod capacity;
pub mod carrier;
pub mod contract;
pub mod funding;
pub mod native_body;
pub mod record_ingress;
#[path = "coordinator_recovery.rs"]
pub mod recovery;
pub mod tir_checker;
pub mod transport;

pub const SOURCE_REQUEST_CONTEXT: &[u8] = b"misaka/positive-court/source-request/v2";

/// Constructed by the trusted private admission adapter, never decoded from a
/// witness. Native integration must resolve the drawn identities from its state.
pub struct PanelRosterV2(BTreeMap<Hash64, Vec<u8>>);
impl PanelRosterV2 {
    pub fn new(keys: Vec<Vec<u8>>, max_roster_bytes: u64) -> Result<Self, ErrorV2> {
        let bytes = (keys.len() as u64).checked_mul(MLDSA87_PUBKEY_LEN as u64).ok_or(ErrorV2::Budget)?;
        if keys.is_empty() || bytes > max_roster_bytes {
            return Err(ErrorV2::Budget);
        }
        let mut rows = BTreeMap::new();
        for key in keys {
            if key.len() != MLDSA87_PUBKEY_LEN {
                return Err(ErrorV2::Authentication);
            }
            if rows.insert(mldsa87_key_id(&key), key).is_some() {
                return Err(ErrorV2::Binding);
            }
        }
        Ok(Self(rows))
    }
    fn key(&self, panel: &Hash64) -> Result<&[u8], ErrorV2> {
        self.0.get(panel).map(Vec::as_slice).ok_or(ErrorV2::Authentication)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoordinatorLimitsV2 {
    pub max_attempts: u32,
    pub max_reserved_elements: u64,
    pub max_reserved_terms: u64,
    pub max_reserved_witness_bytes: u64,
    pub max_history_bytes: u64,
    pub max_batch_parts: u64,
    pub max_batch_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttemptMeterV2 {
    pub attempts: u32,
    pub reserved_elements: u64,
    pub reserved_terms: u64,
    pub reserved_witness_bytes: u64,
    pub received_witness_bytes: u64,
    pub reserved_history_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataResponseStatusV2 {
    NotRequested,
    Pending,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionV2 {
    pub miner_data: DataResponseStatusV2,
    pub witness: Option<WitnessStatusV2>,
    pub claim: ClaimStatusV2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateEffectV2 {
    Opened,
    AlreadyActive,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttemptRecordV2 {
    pub attempt: u32,
    pub panel: Hash64,
    pub witness: Hash64,
    pub status: WitnessStatusV2,
    pub checked_leaves: u64,
    pub received_bytes: u64,
}
pub struct EncodedPartV2<'a> {
    pub cone: &'a [u8],
    pub trace: Option<&'a [u8]>,
}

/// This object is deliberately not deserializable. Restart must replay signed
/// inputs/service proofs/witness parts, not trust a saved Pass or ACK flag.
pub struct PositiveCoordinatorV2 {
    scope: AdmissionScopeV2,
    result: MinerResultV2,
    miner_signature: SignatureV2,
    miner_key: Vec<u8>,
    registered: PalwTirClassV1,
    policy: ExactPolicyV2,
    prompt_form: PalwPromptIdsFormV1,
    roster: PanelRosterV2,
    service: ServiceLedgerV2,
    limits: CoordinatorLimitsV2,
    meter: AttemptMeterV2,
    leaves: u64,
    current: Option<(Hash64, ExactCaseV2)>,
    history: Vec<AttemptRecordV2>,
    claim: ClaimStatusV2,
    transport: Option<transport::MaterialTransportV2>,
}
impl PositiveCoordinatorV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        scope: AdmissionScopeV2,
        result: MinerResultV2,
        miner_signature: SignatureV2,
        miner_key: Vec<u8>,
        registered: PalwTirClassV1,
        policy: ExactPolicyV2,
        prompt_form: PalwPromptIdsFormV1,
        roster: PanelRosterV2,
        service_policy: ServicePolicyV2,
        limits: CoordinatorLimitsV2,
        now: u64,
    ) -> Result<Self, ErrorV2> {
        if scope.check_result(&result).is_err()
            || result.version != 2
            || service_policy.case != result.claim
            || service_policy.deadline != policy.deadline
        {
            return Err(ErrorV2::Binding);
        }
        if now >= policy.deadline {
            return Err(ErrorV2::Deadline);
        }
        if borsh::object_length(&registered).map_err(|_| ErrorV2::Wire)? as u64 > policy.max_commit_bytes
            || borsh::object_length(&result).map_err(|_| ErrorV2::Wire)? as u64 > policy.max_commit_bytes
        {
            return Err(ErrorV2::Budget);
        }
        if miner_key.len() != MLDSA87_PUBKEY_LEN || !miner_signature.verifies(&miner_key, &result.id(), RESULT_CONTEXT) {
            return Err(ErrorV2::Authentication);
        }
        if !result.sampling.is_greedy() {
            return Err(ErrorV2::UnsupportedProfile);
        }
        if registered.class_id(&result.artifact) != result.job.shape_profile_id {
            return Err(ErrorV2::Binding);
        }
        let space = PalwTirStepSpaceV1::new(&registered).map_err(|_| ErrorV2::Binding)?;
        let leaves = space.leaf_count_capped(&result.job, policy.max_leaves).map_err(|_| ErrorV2::Budget)?;
        if leaves == 0
            || limits.max_attempts == 0
            || limits.max_batch_parts == 0
            || limits.max_batch_bytes == 0
            || policy.max_part_bytes == 0
            || policy.max_part_heap == 0
            || policy.max_total_bytes == 0
            || policy.max_elements_per_leaf == 0
            || policy.max_terms_per_leaf == 0
        {
            return Err(ErrorV2::Budget);
        }
        // Reserve for EVERY permitted candidate up front. A rejected Panel
        // cannot create a new per-claim CPU/byte allowance by changing its key.
        let count = limits.max_attempts as u64;
        let elements = leaves.checked_mul(policy.max_elements_per_leaf).and_then(|x| x.checked_mul(count)).ok_or(ErrorV2::Budget)?;
        let terms = leaves.checked_mul(policy.max_terms_per_leaf).and_then(|x| x.checked_mul(count)).ok_or(ErrorV2::Budget)?;
        let bytes = policy.max_total_bytes.checked_mul(count).ok_or(ErrorV2::Budget)?;
        // Include Vec growth slack; records contain only inline fixed fields.
        let history =
            count.checked_mul(std::mem::size_of::<AttemptRecordV2>() as u64).and_then(|x| x.checked_mul(2)).ok_or(ErrorV2::Budget)?;
        if elements > limits.max_reserved_elements
            || terms > limits.max_reserved_terms
            || bytes > limits.max_reserved_witness_bytes
            || history > limits.max_history_bytes
        {
            return Err(ErrorV2::Budget);
        }
        let service = ServiceLedgerV2::new(service_policy, result.service_policy, now)?;
        Ok(Self {
            scope,
            result,
            miner_signature,
            miner_key,
            registered,
            policy,
            prompt_form,
            roster,
            service,
            limits,
            meter: AttemptMeterV2 {
                reserved_elements: elements,
                reserved_terms: terms,
                reserved_witness_bytes: bytes,
                reserved_history_bytes: history,
                ..Default::default()
            },
            leaves,
            current: None,
            history: Vec::new(),
            claim: ClaimStatusV2::Checking,
            transport: None,
        })
    }

    pub fn result(&self) -> &MinerResultV2 {
        &self.result
    }
    pub fn current(&self) -> Option<&ExactCaseV2> {
        self.current.as_ref().map(|(_, c)| c)
    }
    pub fn attempts(&self) -> AttemptMeterV2 {
        self.meter
    }
    pub fn history(&self) -> &[AttemptRecordV2] {
        &self.history
    }
    pub fn service_meter(&self) -> ServiceMeterV2 {
        self.service.meter()
    }
    pub fn completion(&self) -> CompletionV2 {
        let miner_data = if self.service.meter().unique_bytes == 0 {
            DataResponseStatusV2::NotRequested
        } else if self.service.data_complete() {
            DataResponseStatusV2::Complete
        } else {
            DataResponseStatusV2::Pending
        };
        CompletionV2 { miner_data, witness: self.current().map(|x| x.statuses().1), claim: self.claim }
    }
    pub fn request_id(&self, request: WorkRequestV2) -> Result<Hash64, ErrorV2> {
        let WorkRequestV2::Source { first, count } = request else {
            return Err(ErrorV2::UnsupportedProfile);
        };
        Ok(digest(b"misaka/positive-source-request/v2", &(self.result.id(), self.result.service_policy, first, count)))
    }
    pub fn request_source(
        &mut self,
        panel: Hash64,
        request: WorkRequestV2,
        signature: &SignatureV2,
        now: u64,
    ) -> Result<RequestEffectV2, ErrorV2> {
        self.ensure_open(now)?;
        let id = self.request_id(request)?;
        if !signature.verifies(self.roster.key(&panel)?, &id, SOURCE_REQUEST_CONTEXT) {
            return Err(ErrorV2::Authentication);
        }
        self.service.request(request, now)
    }
    /// Any relay may deliver already authenticated source bytes directly to the
    /// Node. No Panel receipt or acknowledgement is needed to complete service.
    pub fn respond_source(&mut self, index: u64, bytes: &[u8], opening: &PalwStepOpeningV1, now: u64) -> Result<(), ErrorV2> {
        self.expire(now);
        // Completing a computation does not erase a preaccepted source duty.
        // The Miner can deliver to this Node even after a Panel finished.
        self.service.respond(index, bytes, opening, now)?;
        if self.claim != ClaimStatusV2::Checking && self.service.data_complete() {
            self.service.close();
        }
        Ok(())
    }
    pub fn open_candidate(
        &mut self,
        panel: Hash64,
        witness: PanelWitnessV2,
        signature: &SignatureV2,
        now: u64,
    ) -> Result<CandidateEffectV2, ErrorV2> {
        self.ensure_open(now)?;
        if witness.result != self.result.id() || witness.policy != self.policy.id() {
            return Err(ErrorV2::Binding);
        }
        if borsh::object_length(&witness).map_err(|_| ErrorV2::Wire)? as u64 > self.policy.max_commit_bytes {
            return Err(ErrorV2::Budget);
        }
        let key = self.roster.key(&panel)?;
        if !signature.verifies(key, &witness.id(), WITNESS_CONTEXT) {
            return Err(ErrorV2::Authentication);
        }
        if let Some((author, current)) = &self.current
            && current.statuses().1 == WitnessStatusV2::Checking
        {
            return if *author == panel && current.witness() == &witness {
                Ok(CandidateEffectV2::AlreadyActive)
            } else {
                Err(ErrorV2::Order)
            };
        }
        if self.meter.attempts >= self.limits.max_attempts {
            return Err(ErrorV2::Budget);
        }
        if witness.attempt != self.meter.attempts {
            return Err(ErrorV2::Order);
        }
        let attempt = self.meter.attempts;
        self.meter.attempts += 1;
        // Authentication preceded this charge. A validly signed but malformed
        // candidate consumes its attempt and cannot retry the same ordinal.
        let candidate = ExactCaseV2::admit(
            self.result.clone(),
            &self.miner_signature,
            &self.miner_key,
            witness.clone(),
            signature,
            key,
            &self.registered,
            self.policy,
            self.prompt_form,
            now,
            self.scope,
        );
        self.archive_current();
        match candidate {
            Ok(case) => {
                self.current = Some((panel, case));
                Ok(CandidateEffectV2::Opened)
            }
            Err(error) => {
                self.history.push(AttemptRecordV2 {
                    attempt,
                    panel,
                    witness: witness.id(),
                    status: WitnessStatusV2::Rejected,
                    checked_leaves: 0,
                    received_bytes: 0,
                });
                Err(error)
            }
        }
    }
    fn archive_current(&mut self) {
        self.discard_material_buffer();
        if let Some((panel, case)) = self.current.take() {
            self.history.push(AttemptRecordV2 {
                attempt: case.witness().attempt,
                panel,
                witness: case.witness().id(),
                status: case.statuses().1,
                checked_leaves: case.cursor(),
                received_bytes: case.charged_bytes(),
            });
        }
    }
    /// Batched private transport over the SAME G14 TIR checker. A batch failure
    /// retains accepted-prefix progress and all consumed budgets; it never rolls
    /// back service completion or opens a second active candidate.
    pub fn submit_batch(&mut self, witness: Hash64, first: u64, parts: &[EncodedPartV2<'_>], now: u64) -> Result<u64, ErrorV2> {
        self.ensure_open(now)?;
        if parts.is_empty() || parts.len() as u64 > self.limits.max_batch_parts {
            return Err(ErrorV2::Budget);
        }
        let size = parts
            .iter()
            .try_fold(0u64, |n, p| n.checked_add(p.cone.len() as u64)?.checked_add(p.trace.map_or(0, |x| x.len() as u64)))
            .ok_or(ErrorV2::Budget)?;
        if size > self.limits.max_batch_bytes {
            return Err(ErrorV2::Budget);
        }
        let (_, case) = self.current.as_mut().ok_or(ErrorV2::Order)?;
        if case.witness().id() != witness {
            return Err(ErrorV2::Binding);
        }
        if case.cursor() != first || first.checked_add(parts.len() as u64).filter(|x| *x <= self.leaves).is_none() {
            return Err(ErrorV2::Order);
        }
        let before = case.charged_bytes();
        let mut outcome = Ok(0);
        for (i, part) in parts.iter().enumerate() {
            if let Err(error) = case.submit_encoded(part.cone, part.trace, now) {
                outcome = Err(error);
                break;
            }
            outcome = Ok(i as u64 + 1);
        }
        self.meter.received_witness_bytes =
            self.meter.received_witness_bytes.checked_add(case.charged_bytes() - before).ok_or(ErrorV2::Budget)?;
        debug_assert!(self.meter.received_witness_bytes <= self.meter.reserved_witness_bytes);
        outcome
    }
    pub fn finish(&mut self, now: u64) -> Result<ClaimStatusV2, ErrorV2> {
        self.ensure_open(now)?;
        let (_, case) = self.current.as_mut().ok_or(ErrorV2::Order)?;
        let status = case.finish(now)?;
        self.claim = status;
        self.discard_material_buffer();
        if self.service.meter().unique_bytes == 0 || self.service.data_complete() {
            self.service.close();
        }
        Ok(status)
    }
    fn ensure_open(&mut self, now: u64) -> Result<(), ErrorV2> {
        self.expire(now);
        if self.claim != ClaimStatusV2::Checking {
            return Err(ErrorV2::Closed);
        }
        Ok(())
    }
    pub fn expire(&mut self, now: u64) {
        if now >= self.policy.deadline {
            self.discard_material_buffer();
            if self.claim == ClaimStatusV2::Checking {
                self.claim = ClaimStatusV2::EndedUnverified;
                if let Some((_, case)) = self.current.as_mut() {
                    case.expire(now);
                }
            }
            self.service.close();
        }
    }
    pub fn revoke(&mut self) {
        self.discard_material_buffer();
        self.claim = ClaimStatusV2::Revoked;
        if let Some((_, case)) = self.current.as_mut() {
            case.revoke();
        }
        self.service.close();
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
mod tests;
