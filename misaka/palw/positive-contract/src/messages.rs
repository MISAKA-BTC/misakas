use crate::identity::hash;
use crate::{CONTRACT_REVISION, CONTRACT_VERSION, ContractError, identity::*};
use borsh::{BorshDeserialize, BorshSerialize};

/// A2 signs this result. It deliberately contains no step tree/trace root or
/// Positive witness. Registering a model is not a model-review procedure.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MinerResultCommit {
    pub version: u16,
    pub revision: u16,
    pub scope: ChainScope,
    pub job: Digest,
    pub execution: Digest,
    pub miner: PrincipalId,
    pub registered_class: Digest,
    pub artifact: Digest,
    pub program: Digest,
    pub input: Digest,
    pub tokenizer: Digest,
    pub execution_rules: Digest,
    pub selector: Digest,
    pub generation_seed: [u8; 32],
    pub temperature_q24: u32,
    pub prefill: u32,
    pub decode: u32,
    pub max_context: u32,
    pub output: Digest,
    pub dependencies: Digest,
    pub service_contract: Digest,
}
impl MinerResultCommit {
    pub fn id(&self) -> ResultId {
        ResultId(hash(b"misaka/positive/result-id/v2/r1", self))
    }
    pub fn claim_id(&self) -> ClaimId {
        ClaimId(hash(b"misaka/positive/claim-id/v2/r1", &self.id()))
    }
    /// Output/witness/Panel/relay changes cannot purchase a fresh case budget.
    /// A4 also enforces one admitted result for this execution identity.
    pub fn case_id(&self) -> CaseId {
        CaseId(hash(b"misaka/positive/case-id/v2/r1", &(self.scope, self.job, self.execution, self.miner)))
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        version(self.version, self.revision)?;
        let positions =
            self.prefill.checked_add(self.decode.checked_sub(1).ok_or(ContractError::Range)?).ok_or(ContractError::Range)?;
        if self.prefill == 0 || positions > self.max_context {
            return Err(ContractError::Range);
        }
        Ok(())
    }
    pub fn authenticate(
        &self,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<AuthenticatedResult<'_>, ContractError> {
        self.validate_shape()?;
        if principal(key) != self.miner {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id().0, signature, RESULT_SIGN_CONTEXT) {
            return Err(ContractError::Signature);
        }
        Ok(AuthenticatedResult(self))
    }
}
/// Signature checked, not native-admitted, available, computed, or creditable.
pub struct AuthenticatedResult<'a>(&'a MinerResultCommit);
impl<'a> AuthenticatedResult<'a> {
    pub fn commit(&self) -> &'a MinerResultCommit {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CheckProfile {
    ExactStreamV1,
    FullRelationDirectV1,
}

/// A3 owns the record and its errors. This does not acquire a Miner signature.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PanelWitnessCommit {
    pub version: u16,
    pub revision: u16,
    pub scope: ChainScope,
    pub case: CaseId,
    pub result: ResultId,
    pub author: PrincipalId,
    pub attempt: u32,
    pub profile: CheckProfile,
    pub plan: PlanId,
    pub execution_record: Digest,
    pub provenance: Digest,
    pub candidate_output: Digest,
}
impl PanelWitnessCommit {
    pub fn id(&self) -> WitnessId {
        WitnessId(hash(b"misaka/positive/witness-id/v2/r1", self))
    }
    pub fn validate_binding(&self, result: &MinerResultCommit) -> Result<(), ContractError> {
        self.validate_shape()?;
        if self.scope != result.scope || self.case != result.case_id() || self.result != result.id() {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        version(self.version, self.revision)?;
        if self.profile != CheckProfile::ExactStreamV1 {
            return Err(ContractError::UnsupportedProfile);
        }
        Ok(())
    }
    pub fn authenticate(
        &self,
        result: &MinerResultCommit,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        self.validate_binding(result)?;
        if principal(key) != self.author {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id().0, signature, WITNESS_SIGN_CONTEXT) {
            return Err(ContractError::Signature);
        }
        Ok(())
    }
}

/// A4 resolves this from registered native state. A peer-supplied copy is only
/// data; it cannot authorize admission. Class ID pins program AND weight root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmissionContext {
    pub scope: ChainScope,
    pub expected_job: Digest,
    pub expected_execution: Digest,
    pub expected_miner: PrincipalId,
    pub expected_input: Digest,
    pub registered_class: Digest,
    pub artifact: Digest,
    pub program: Digest,
    pub tokenizer: Digest,
    pub execution_rules: Digest,
    pub selector: Digest,
    pub required_form: ClaimForm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimForm {
    LegacyTrace,
    PositiveResultV2,
}
impl AdmissionContext {
    pub fn check(&self, form: ClaimForm, result: &MinerResultCommit) -> Result<(), ContractError> {
        if form != self.required_form || form != ClaimForm::PositiveResultV2 {
            return Err(ContractError::Version);
        }
        if self.scope != result.scope
            || self.expected_job != result.job
            || self.expected_execution != result.execution
            || self.expected_miner != result.miner
            || self.expected_input != result.input
            || self.registered_class != result.registered_class
            || self.artifact != result.artifact
            || self.program != result.program
            || self.tokenizer != result.tokenizer
            || self.execution_rules != result.execution_rules
            || self.selector != result.selector
        {
            return Err(ContractError::Scope);
        }
        result.validate_shape()
    }
}

/// Existing G14 original inventory/prompt identities are referenced here; these
/// fields do not define a second Merkle tree or a substitute source protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum SourceKind {
    ArtifactInventory,
    Input,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceRequest {
    pub version: u16,
    pub revision: u16,
    pub case: CaseId,
    pub contract: Digest,
    pub source: Digest,
    pub kind: SourceKind,
    /// Half-open range in canonical G14 source UNITS, not an arbitrary byte slice.
    pub first: u64,
    pub end: u64,
}
impl SourceRequest {
    /// Author, relay and request nonce are deliberately absent. A4 meters the
    /// union of overlapping units across ALL Panel identities and attempts.
    pub fn id(&self) -> RequestId {
        RequestId(hash(b"misaka/positive/source-request/v2/r1", self))
    }
    pub fn validate(&self, source_units: u64) -> Result<(), ContractError> {
        version(self.version, self.revision)?;
        if self.first >= self.end || self.end > source_units {
            return Err(ContractError::Range);
        }
        Ok(())
    }
    pub fn signing_id(&self, requester: PrincipalId) -> Digest {
        hash(b"misaka/positive/request-signing/v2/r1", &(self.id(), requester))
    }
    pub fn authenticate(
        &self,
        source_units: u64,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        self.validate(source_units)?;
        if !verify_signature(key, self.signing_id(principal(key)), signature, REQUEST_SIGN_CONTEXT) {
            return Err(ContractError::Signature);
        }
        // Authentication only. A4 separately binds drawn Panel membership,
        // exact case/contract, permitted source catalog/kind/units, clocks,
        // reservations and all-Panel union budgets to the native duty.
        Ok(())
    }
}

/// Every dimension is finite and admitted before creating an obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AggregateBudget {
    pub rounds: u64,
    pub unique_source_bytes: u64,
    pub transmitted_bytes: u64,
    pub additional_work: u64,
    pub retained_bytes: u64,
    pub retention_until: u64,
    pub fee: u64,
    pub witness_attempts: u32,
}

/// Signed indirectly by MinerResultCommit.service_contract. This is a finite
/// consent, not a claim that native funds/capacity have actually been reserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MinerServiceContract {
    pub version: u16,
    pub revision: u16,
    pub case: CaseId,
    pub source_catalog: Digest,
    pub provider_delegations: Digest,
    pub budget: AggregateBudget,
    pub funding_consent: Digest,
    pub deadline: u64,
}
impl MinerServiceContract {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/miner-service/v2/r1", self)
    }
    pub fn validate(&self, result: &MinerResultCommit, now: u64) -> Result<(), ContractError> {
        version(self.version, self.revision)?;
        if self.case != result.case_id() || self.id() != result.service_contract {
            return Err(ContractError::Identity);
        }
        if now >= self.deadline || self.budget.retention_until < self.deadline || self.budget.witness_attempts == 0 {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MinerDataStatus {
    NotRequested,
    Pending,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum WitnessStatus {
    Building,
    Committed,
    Checking,
    Complete,
    Rejected,
    Incomplete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ClaimStatus {
    ResultCommitted,
    Pending,
    PositivePassed,
    ClaimRefuted,
    EndedUnverified,
    Revoked,
}

/// A1 derives this from the registered program; A3 cannot nominate a smaller plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CoverageDescriptor {
    pub scope: ChainScope,
    pub result: ResultId,
    pub plan: PlanId,
    pub profile: CheckProfile,
    pub mandatory_relations: u64,
    pub reserved_elements: u64,
    pub reserved_terms: u64,
    pub max_part_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageCursor {
    pub plan: PlanId,
    pub next_relation: u64,
}

/// Canonical identity for A1's independently derived relation enumeration.
/// `derivation` names the exact TIR ordering/rules revision, never a Panel list.
pub fn coverage_plan_id(
    result: &MinerResultCommit,
    profile: CheckProfile,
    derivation: Digest,
    relations: u64,
) -> Result<PlanId, ContractError> {
    if profile != CheckProfile::ExactStreamV1 {
        return Err(ContractError::UnsupportedProfile);
    }
    if relations == 0 {
        return Err(ContractError::Range);
    }
    Ok(PlanId(hash(b"misaka/positive/coverage-plan/v2/r1", &(result.id(), profile, derivation, relations))))
}

pub(crate) fn version(v: u16, r: u16) -> Result<(), ContractError> {
    if v != CONTRACT_VERSION || r != CONTRACT_REVISION { Err(ContractError::Version) } else { Ok(()) }
}
