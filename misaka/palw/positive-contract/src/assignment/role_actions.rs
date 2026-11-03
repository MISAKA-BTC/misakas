//! Assignment-purpose cancel/join declarations. Their signatures do not
//! release rights, replenish global budgets, lock funds or create native rows.
use super::{AssignmentAdmissionBundleV2, AssignmentId};
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{CaseId, Digest, PUBKEY_BYTES, PrincipalId, ResultId, SIGNATURE_BYTES, hash, principal, verify_signature},
    rows::NativeOutpoint,
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const ASSIGNMENT_CANCEL_POSTER_CONTEXT_V2: &[u8] = b"misaka/positive/assignment-cancel-poster/v2/r1";
pub const ASSIGNMENT_CANCEL_WORKER_CONTEXT_V2: &[u8] = b"misaka/positive/assignment-cancel-worker/v2/r1";
pub const ASSIGNMENT_PROVER_JOIN_CONTEXT_V2: &[u8] = b"misaka/positive/assignment-prover-join/v2/r1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentCancellationV2 {
    pub assignment: AssignmentId,
    pub original_bundle: Digest,
    pub cancellation_policy: Digest,
    pub cancel_before_daa: u64,
}
impl AssignmentCancellationV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-cancel/v2/r1", self)
    }
    pub fn check(&self, bundle: &AssignmentAdmissionBundleV2, now: u64) -> Result<(), ContractError> {
        bundle.validate_shape()?;
        if self.assignment != bundle.terms.id()
            || self.original_bundle != bundle.id()
            || self.cancellation_policy != bundle.cancellation_policy
        {
            return Err(ContractError::Identity);
        }
        if now >= self.cancel_before_daa || self.cancel_before_daa > bundle.terms.result_before_daa {
            return Err(ContractError::Range);
        }
        // Native must still require Open, initial+ALL joined prover consent to
        // these exact finite conditions, and preservation of every earned due,
        // capital, lease and unfinished duty. Consumed never returns to Open.
        Ok(())
    }
    pub fn authenticate_poster(
        &self,
        bundle: &AssignmentAdmissionBundleV2,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        now: u64,
    ) -> Result<(), ContractError> {
        self.check(bundle, now)?;
        if principal(key) != bundle.terms.payer {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id(), signature, ASSIGNMENT_CANCEL_POSTER_CONTEXT_V2) {
            return Err(ContractError::Signature);
        }
        Ok(())
    }
    /// The native reader resolves this key from the current worker bond. The
    /// DTO alone cannot authenticate a registered worker or release its duties.
    pub fn authenticate_worker(
        &self,
        bundle: &AssignmentAdmissionBundleV2,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        now: u64,
    ) -> Result<(), ContractError> {
        self.check(bundle, now)?;
        if !verify_signature(key, self.id(), signature, ASSIGNMENT_CANCEL_WORKER_CONTEXT_V2) {
            return Err(ContractError::Signature);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentProverJoinV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: AssignmentId,
    pub original_bundle: Digest,
    pub case: Option<(CaseId, ResultId)>,
    pub prover: NativeOutpoint,
    pub payer: PrincipalId,
    pub funding_output_index: u32,
    pub max_fee: u64,
    pub max_capital: u64,
    pub max_locked: u64,
    pub max_attempts: u32,
    pub max_build_work: u64,
    pub max_submitted_bytes: u64,
    pub max_retained_bytes: u64,
    pub admission_before_daa: u64,
    pub service_before_daa: u64,
    pub retention_until_daa: u64,
    pub cancellation_policy: Digest,
}
impl AssignmentProverJoinV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-prover-join/v2/r1", self)
    }
    pub fn check(&self, bundle: &AssignmentAdmissionBundleV2, now: u64) -> Result<(), ContractError> {
        bundle.validate_shape()?;
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if self.assignment != bundle.terms.id()
            || self.original_bundle != bundle.id()
            || self.cancellation_policy != bundle.cancellation_policy
        {
            return Err(ContractError::Identity);
        }
        if self.max_attempts == 0
            || self.max_attempts == u32::MAX
            || [self.max_locked, self.max_build_work, self.max_submitted_bytes, self.max_retained_bytes]
                .into_iter()
                .any(|n| n == 0 || n == u64::MAX)
            || self.max_fee.checked_add(self.max_capital).is_none_or(|n| n > self.max_locked)
        {
            return Err(ContractError::Budget);
        }
        if now >= self.admission_before_daa
            || (self.case.is_none() && self.admission_before_daa > bundle.terms.result_before_daa)
            || self.admission_before_daa > self.service_before_daa
            || self.service_before_daa > bundle.future_case.service_before_daa
            || self.retention_until_daa < self.service_before_daa
            || self.retention_until_daa > bundle.terms.reserve_until_daa
        {
            return Err(ContractError::Range);
        }
        Ok(())
    }
    pub fn authenticate(
        &self,
        bundle: &AssignmentAdmissionBundleV2,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        now: u64,
    ) -> Result<(), ContractError> {
        self.check(bundle, now)?;
        if principal(key) != self.payer {
            return Err(ContractError::Identity);
        }
        if !verify_signature(key, self.id(), signature, ASSIGNMENT_PROVER_JOIN_CONTEXT_V2) {
            return Err(ContractError::Signature);
        }
        // Native additionally requires payer == the registered prover key,
        // actual eligibility/outpoint/lock/capacity, exact current Case if any,
        // total candidate/join/remaining-attempt caps and atomic journal state.
        Ok(())
    }
}
