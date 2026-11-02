//! Result-free source/prover declarations authenticated by the common bundle.
//! Initial Assignment mode starts neither data nor prover service before the
//! atomic assigned-Result/Case admission. These DTOs cannot issue receipts.
use super::{AssignmentAdmissionBundleV2, AssignmentId};
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{Digest, hash},
    messages::AggregateBudget,
    rows::NativeOutpoint,
};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentSourceConsentV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: AssignmentId,
    pub worker: NativeOutpoint,
    pub source_catalog: Digest,
    pub source_policy: Digest,
    pub budget: AggregateBudget,
    pub service_before_daa: u64,
}
impl AssignmentSourceConsentV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-source/v2/r1", self)
    }
    pub fn check(&self, bundle: &AssignmentAdmissionBundleV2) -> Result<(), ContractError> {
        bundle.validate_shape()?;
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if self.assignment != bundle.terms.id()
            || self.worker != bundle.terms.worker
            || self.source_catalog != bundle.source_catalog
            || self.source_policy != bundle.terms.source_policy
            || self.id() != bundle.source_consent
        {
            return Err(ContractError::Identity);
        }
        if [
            self.budget.rounds,
            self.budget.unique_source_bytes,
            self.budget.transmitted_bytes,
            self.budget.additional_work,
            self.budget.retained_bytes,
        ]
        .into_iter()
        .any(|n| n == 0 || n == u64::MAX)
            || self.budget.fee == u64::MAX
            || self.budget.witness_attempts == 0
            || self.budget.witness_attempts == u32::MAX
            || self.budget.unique_source_bytes > self.budget.transmitted_bytes
            || self.budget.fee > bundle.future_case.max_case_fee
        {
            return Err(ContractError::Budget);
        }
        if self.service_before_daa <= bundle.future_case.case_admission_before_daa
            || self.service_before_daa > bundle.future_case.service_before_daa
            || self.budget.retention_until < self.service_before_daa
            || self.budget.retention_until > bundle.terms.reserve_until_daa
        {
            return Err(ContractError::Range);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentProverConsentV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: AssignmentId,
    pub prover: NativeOutpoint,
    pub positive_policy: Digest,
    pub max_attempts: u32,
    pub max_build_work: u64,
    pub max_submitted_bytes: u64,
    pub max_retained_bytes: u64,
    pub retention_until_daa: u64,
    pub build_before_daa: u64,
    pub max_service_fee: u64,
    pub cancellation_policy: Digest,
}
impl AssignmentProverConsentV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-prover-consent/v2/r1", self)
    }
    pub fn check(&self, bundle: &AssignmentAdmissionBundleV2, positive_policy: Digest) -> Result<(), ContractError> {
        bundle.validate_shape()?;
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if self.assignment != bundle.terms.id()
            || self.prover != bundle.terms.prover
            || self.positive_policy != positive_policy
            || self.cancellation_policy != bundle.cancellation_policy
            || self.id() != bundle.prover_consent
        {
            return Err(ContractError::Identity);
        }
        if self.max_attempts == 0
            || self.max_attempts == u32::MAX
            || [self.max_build_work, self.max_submitted_bytes, self.max_retained_bytes].into_iter().any(|n| n == 0 || n == u64::MAX)
            || self.max_service_fee > bundle.future_case.max_case_fee
        {
            return Err(ContractError::Budget);
        }
        if self.build_before_daa <= bundle.future_case.case_admission_before_daa
            || self.build_before_daa > bundle.future_case.service_before_daa
            || self.retention_until_daa < self.build_before_daa
            || self.retention_until_daa > bundle.terms.reserve_until_daa
        {
            return Err(ContractError::Range);
        }
        Ok(())
    }
}
