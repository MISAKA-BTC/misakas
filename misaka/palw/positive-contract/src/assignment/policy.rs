//! Fixed finite policy preimage for RFC-0045. A valid shape is not readiness,
//! a service supply record, a lease, accepted origin or activation permission.
use super::{AssignmentModeV2, AssignmentTermsV2};
use crate::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{Digest, hash},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub fn canonical_assignment_derivation_id() -> Digest {
    hash(
        b"misaka/positive/assignment-derivation/v2/r1",
        &"registered-TIR/full-canonical-pair/AssignmentId-anchor/original-prompt-form-and-profile/no-prefill-draw",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentLiveCapsV2 {
    pub global: u32,
    pub per_poster: u32,
    pub per_worker: u32,
    pub per_prover: u32,
    pub per_class: u32,
}
impl AssignmentLiveCapsV2 {
    fn validate(&self) -> bool {
        [self.global, self.per_poster, self.per_worker, self.per_prover, self.per_class]
            .into_iter()
            .all(|n| n > 0 && n < u32::MAX && n <= self.global)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentPolicyV2 {
    pub version: u16,
    pub revision: u16,
    pub mode: AssignmentModeV2,
    pub positive_policy: Digest,
    pub derivation_policy: Digest,
    pub source_policy: Digest,
    pub assignment_fee_schedule: Digest,
    pub case_fee_schedule: Digest,
    pub reserve_schedule: Digest,
    pub physical_service_profile: Digest,
    pub cancellation_policy: Digest,
    pub live: AssignmentLiveCapsV2,
    pub max_frame_bytes: u64,
    pub max_program_bytes: u32,
    pub max_layout_entries: u32,
    pub max_source_catalog_entries: u32,
    pub max_role_candidates: u32,
    pub max_prover_joins: u32,
    pub max_history_frames: u32,
    pub max_history_bytes: u64,
    pub max_retained_bytes: u64,
    pub max_replay_bytes: u64,
    pub max_replay_work: u64,
    pub max_heap_bytes: u64,
    pub accept_window_daa: u64,
    pub result_window_daa: u64,
    pub build_window_daa: u64,
    pub check_window_daa: u64,
    pub fraud_tail_daa: u64,
    pub refund_window_daa: u64,
    pub retention_window_daa: u64,
}
impl AssignmentPolicyV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-policy/v2/r1", self)
    }
    pub fn validate_shape(&self) -> Result<(), ContractError> {
        if self.version != CONTRACT_VERSION || self.revision != CONTRACT_REVISION {
            return Err(ContractError::Version);
        }
        if self.derivation_policy != canonical_assignment_derivation_id() {
            return Err(ContractError::UnsupportedProfile);
        }
        if [
            self.positive_policy,
            self.derivation_policy,
            self.source_policy,
            self.assignment_fee_schedule,
            self.case_fee_schedule,
            self.reserve_schedule,
            self.physical_service_profile,
            self.cancellation_policy,
        ]
        .contains(&Digest([0; 64]))
        {
            return Err(ContractError::Identity);
        }
        if !self.live.validate()
            || [
                self.max_program_bytes,
                self.max_layout_entries,
                self.max_source_catalog_entries,
                self.max_role_candidates,
                self.max_prover_joins,
                self.max_history_frames,
            ]
            .into_iter()
            .any(|n| n == 0 || n == u32::MAX)
            || [
                self.max_frame_bytes,
                self.max_history_bytes,
                self.max_retained_bytes,
                self.max_replay_bytes,
                self.max_replay_work,
                self.max_heap_bytes,
                self.accept_window_daa,
                self.result_window_daa,
                self.build_window_daa,
                self.check_window_daa,
                self.fraud_tail_daa,
                self.refund_window_daa,
                self.retention_window_daa,
            ]
            .into_iter()
            .any(|n| n == 0 || n == u64::MAX)
            || self.max_role_candidates < 3
            || self.max_prover_joins > self.max_role_candidates
            || self.max_frame_bytes > self.max_history_bytes
            || self.max_history_bytes > self.max_retained_bytes
            || self.max_retained_bytes > self.max_replay_bytes
        {
            return Err(ContractError::Budget);
        }
        let local_windows = self
            .accept_window_daa
            .checked_add(self.result_window_daa)
            .and_then(|n| n.checked_add(self.build_window_daa))
            .and_then(|n| n.checked_add(self.check_window_daa))
            .and_then(|n| n.checked_add(self.fraud_tail_daa))
            .and_then(|n| n.checked_add(self.refund_window_daa));
        if local_windows.is_none_or(|n| n > self.retention_window_daa) {
            return Err(ContractError::Budget);
        }
        // No caller-supplied Gate C duration substitutes for resolving every
        // existing draw/attestation/fraud/failure branch from its actual policy.
        Ok(())
    }
    pub fn check_terms(&self, terms: &AssignmentTermsV2, now: u64) -> Result<(), ContractError> {
        self.validate_shape()?;
        terms.validate_shape()?;
        if terms.mode != self.mode
            || terms.derivation_policy != self.derivation_policy
            || terms.source_policy != self.source_policy
            || terms.reserve_schedule != self.reserve_schedule
        {
            return Err(ContractError::Scope);
        }
        if now >= terms.accept_before_daa
            || terms.accept_before_daa.checked_sub(now).is_none_or(|n| n > self.accept_window_daa)
            || terms.result_before_daa.checked_sub(terms.accept_before_daa).is_none_or(|n| n > self.result_window_daa)
            || terms.reserve_until_daa.checked_sub(now).is_none_or(|n| n > self.retention_window_daa)
        {
            return Err(ContractError::Range);
        }
        Ok(())
    }
}

/// Separate recipe; the existing r1 native::ruleset_id bytes/meaning stay exact.
pub fn assignment_ruleset_id(fired_fork: Digest, positive_policy: Digest, assignment_policy: Digest) -> Digest {
    hash(b"misaka/positive/assignment-ruleset/v2/r1", &(fired_fork, positive_policy, assignment_policy))
}
