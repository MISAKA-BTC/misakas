//! RFC-0045: result-free canonical Assignment contracts.
//! Authentication here establishes role consent only. Native registration,
//! ancestry, funds, service capacity and atomic accepted state are separate.
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{hash, *},
    rows::{NativeHash64, NativeOutpoint},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub mod consents;
pub mod control_material;
pub mod funding_relay;
pub mod funding_relay_policy;
pub mod funding_relay_rows;
pub mod journal;
pub mod open_material;
pub mod policy;
pub mod role_actions;
pub mod rows;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct AssignmentId(pub Digest);

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentModeV2 {
    CanonicalTirAssignmentV1,
}

/// Field order is RFC-0045 §3. No Result/output, derived catalog/quote,
/// signature, carrier, accepted origin or lease is an identity input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentTermsV2 {
    pub version: u16,
    pub revision: u16,
    pub network: Digest,
    pub ruleset: Digest,
    pub branch_anchor: NativeHash64,
    pub mode: AssignmentModeV2,
    pub registered_class: Digest,
    pub poster: NativeOutpoint,
    pub worker: NativeOutpoint,
    pub prover: NativeOutpoint,
    pub payer: PrincipalId,
    pub refund_payload: Digest,
    pub poster_nonce: u64,
    pub derivation_policy: Digest,
    pub source_policy: Digest,
    pub reserve_schedule: Digest,
    pub worker_reward: u64,
    pub max_assignment_fee: u64,
    pub max_assignment_capital: u64,
    pub max_assignment_locked: u64,
    pub accept_before_daa: u64,
    pub result_before_daa: u64,
    pub reserve_until_daa: u64,
}
impl AssignmentTermsV2 {
    pub fn id(&self) -> AssignmentId {
        AssignmentId(hash(b"misaka/positive/assignment-terms/v2/r1", self))
    }
    /// Canonical authenticated-empty dependency preimage for the initial
    /// assignment mode. Persist the preimage in accepted native material; a
    /// missing history must never be replaced by an assumed empty list.
    pub fn empty_dependencies_preimage(&self) -> [u8; 72] {
        let mut bytes = [0; 72];
        bytes[..2].copy_from_slice(&CONTRACT_VERSION.to_le_bytes());
        bytes[2..4].copy_from_slice(&CONTRACT_REVISION.to_le_bytes());
        bytes[4..68].copy_from_slice(&self.id().0.0);
        // Final four bytes are the canonical Borsh empty-list count, u32 zero.
        bytes
    }
    pub fn empty_dependencies_id(&self) -> Digest {
        hash(b"misaka/positive/assignment-dependencies/v2/r1", &self.empty_dependencies_preimage())
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if !(self.accept_before_daa < self.result_before_daa && self.result_before_daa < self.reserve_until_daa)
            || self.reserve_until_daa == u64::MAX
        {
            return Err(ContractError::Range);
        }
        if self.max_assignment_locked == 0
            || self.max_assignment_locked == u64::MAX
            || self.worker_reward > self.max_assignment_locked
            || self.max_assignment_fee > self.max_assignment_locked
            || self.max_assignment_capital > self.max_assignment_locked
        {
            return Err(ContractError::Budget);
        }
        if [self.derivation_policy, self.source_policy, self.reserve_schedule].contains(&Digest([0; 64])) {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}

/// Result-free signed limits for the later, separately consented Case account.
/// These do not authorize a spend, replace r1 consent or identify its output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FutureCaseFundingBoundsV2 {
    pub assignment: AssignmentId,
    pub payer: PrincipalId,
    pub refund_payload: Digest,
    pub worker: NativeOutpoint,
    pub max_case_fee: u64,
    pub max_case_capital: u64,
    pub max_case_locked: u64,
    pub case_admission_before_daa: u64,
    pub service_before_daa: u64,
}
impl FutureCaseFundingBoundsV2 {
    pub fn validate(&self, terms: &AssignmentTermsV2) -> Result<(), ContractError> {
        if self.assignment != terms.id()
            || self.payer != terms.payer
            || self.refund_payload != terms.refund_payload
            || self.worker != terms.worker
        {
            return Err(ContractError::Identity);
        }
        if self.max_case_locked == 0
            || self.max_case_locked == u64::MAX
            || self.max_case_fee.checked_add(self.max_case_capital).is_none_or(|n| n > self.max_case_locked)
        {
            return Err(ContractError::Budget);
        }
        if self.case_admission_before_daa == 0
            || self.case_admission_before_daa > terms.result_before_daa
            || self.service_before_daa < self.case_admission_before_daa
            || self.service_before_daa > terms.reserve_until_daa
        {
            return Err(ContractError::Range);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentFundingAuthorizationV2 {
    pub assignment: AssignmentId,
    pub quote: Digest,
    pub payer: PrincipalId,
    pub funding_output_index: u32,
    pub refund_payload: Digest,
    pub max_fee: u64,
    pub max_capital: u64,
    pub max_reward: u64,
    pub max_locked: u64,
    pub admission_expiry: u64,
}

/// All three roles sign the same unsigned bundle under distinct contexts.
/// The native reader independently re-derives the job/quote/catalog digests;
/// a submitted bundle is never a job, reservation or funding authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentAdmissionBundleV2 {
    pub terms: AssignmentTermsV2,
    pub job_context: Digest,
    pub quote: Digest,
    pub source_catalog: Digest,
    pub funding: AssignmentFundingAuthorizationV2,
    pub future_case: FutureCaseFundingBoundsV2,
    /// Canonical bounded declaration/preimage references; these must be
    /// resolved, quoted and retained by native admission, never trusted alone.
    pub source_consent: Digest,
    pub prover_consent: Digest,
    pub cancellation_policy: Digest,
}
impl AssignmentAdmissionBundleV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-bundle/v2/r1", self)
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        self.terms.validate_shape()?;
        self.future_case.validate(&self.terms)?;
        let a = &self.funding;
        let t = &self.terms;
        if a.assignment != t.id() || a.quote != self.quote || a.payer != t.payer || a.refund_payload != t.refund_payload {
            return Err(ContractError::Identity);
        }
        if a.admission_expiry == 0 || a.admission_expiry > t.accept_before_daa {
            return Err(ContractError::Range);
        }
        if a.max_fee > t.max_assignment_fee
            || a.max_capital > t.max_assignment_capital
            || a.max_reward != t.worker_reward
            || a.max_locked > t.max_assignment_locked
            || a.max_locked < t.worker_reward
        {
            return Err(ContractError::Budget);
        }
        if [self.job_context, self.quote, self.source_catalog, self.source_consent, self.prover_consent, self.cancellation_policy]
            .contains(&Digest([0; 64]))
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
    /// Arithmetic preflight only. `actual_locked` must come from the validated
    /// carrier and current composed UTXO, whose native lock is still required.
    pub fn check_quoted_amounts(&self, required_fee: u64, required_capital: u64, actual_locked: u64) -> Result<(), ContractError> {
        self.validate_shape()?;
        let a = &self.funding;
        let required = self
            .terms
            .worker_reward
            .checked_add(required_fee)
            .and_then(|n| n.checked_add(required_capital))
            .ok_or(ContractError::Budget)?;
        if required_fee > a.max_fee || required_capital > a.max_capital || actual_locked < required || actual_locked > a.max_locked {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
    pub fn authenticate_role(
        &self,
        role: AssignmentRoleV2,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<(), ContractError> {
        self.validate_shape()?;
        // Only native can bind this key to the declared current bond. For the
        // payer we can additionally check the literal consent principal here.
        if role == AssignmentRoleV2::Poster && principal(key) != self.terms.payer {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id(), signature, role.signing_context()) {
            return Err(ContractError::Signature);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentRoleV2 {
    Poster,
    Worker,
    Prover,
}
impl AssignmentRoleV2 {
    pub fn signing_context(self) -> &'static [u8] {
        match self {
            Self::Poster => b"misaka/positive/assignment-poster/v2/r1",
            Self::Worker => b"misaka/positive/assignment-worker/v2/r1",
            Self::Prover => b"misaka/positive/assignment-prover/v2/r1",
        }
    }
}

/// Permanent canonical-lineage counter, separate from retained payloads.
/// Native commits/undoes it atomically with assignment intake. This DTO alone
/// cannot consume a nonce or authorize an idempotent replay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentNonceV2 {
    pub next: u64,
    pub exhausted: bool,
}
impl AssignmentNonceV2 {
    pub fn after_accepting(self, nonce: u64) -> Result<Self, ContractError> {
        if self.exhausted || nonce != self.next {
            return Err(ContractError::NonCanonical);
        }
        Ok(match nonce.checked_add(1) {
            Some(next) => Self { next, exhausted: false },
            None => Self { next: nonce, exhausted: true },
        })
    }
}
