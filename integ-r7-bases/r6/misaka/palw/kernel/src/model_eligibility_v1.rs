//! **RFC-0017 — model eligibility exams: the wire terms of kernel-route inner kinds 74 and 75** (2026-10-15; dormant).
//!
//! * inner 74 [`EligibilityExamCommitV1`]: commit an exam of one registered model — the model, the root of the examined claim set and
//!   the purpose — BEFORE any randomness that draws its verifiers exists. The verifier population is cut by the commit's own DAA.
//! * inner 75 [`EligibilityAttestationV1`]: one drawn verifier's authenticated attestation for one exam: a model-level verdict
//!   (reproduced / could not obtain / not reproducible) and, separately, the indices of examined claims whose committed answers did
//!   not match (per-claim findings, which the consumer routes to each claim's own court and never folds into the model's verdict).
//!
//! The bare ledger refuses both kinds: only the authenticated native consumer, past its own dormant fence
//! (`palw_model_eligibility_v1`), interprets them. Every fact the exam is judged by (the draw, the examined claims, the deadline,
//! the population) is derived by the consumer from chain state; none is asserted here. An exam earns no PALW reward, block right or
//! Final weight.

use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};

pub const ELIGIBILITY_EXAM_COMMIT_VERSION_V1: u16 = 1;
pub const ELIGIBILITY_ATTESTATION_VERSION_V1: u16 = 1;
/// The longest per-claim mismatch list an attestation may carry.
pub const MAX_ELIGIBILITY_MISMATCHES_V1: usize = 64;
/// Every inner 74/75 object is bounded by this (version byte and tag included), with headroom over the largest attestation.
pub const MAX_ELIGIBILITY_OBJECT_BYTES_V1: usize = 1024;

/// Why an exam is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum EligibilityExamPurposeV1 {
    /// `Probation` → `Eligible`.
    Promotion = 0,
    /// `Eligible` → `Eligible` with a later renewal; also `Lapsed` → `Eligible`.
    Renewal = 1,
}

/// **Inner 74.** Signed by the committing bond (any Active bond: the attempt is counted against the model's lineage, the committer's
/// principal and the network, whoever commits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EligibilityExamCommitV1 {
    pub version: u16,
    pub network: Digest,
    /// The committing bond's kernel bond id; must be the object's signer.
    pub committer: Digest,
    /// The registered class (model) examined.
    pub class_id: Digest,
    pub purpose: EligibilityExamPurposeV1,
    /// Root of the examined claim ids (sorted ascending), each a claim of `class_id` accepted before the commit.
    pub claims_root: Digest,
    pub claims_count: u32,
    /// The lineage's attempt number this commit consumes (= its attempts so far): replay and reorder protection.
    pub attempt_index: u32,
}

impl EligibilityExamCommitV1 {
    /// The exam id: `H("MISAKA/PALW/ELIGIBILITY-EXAM/V1"; borsh(commit))`.
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/ELIGIBILITY-EXAM/V1", self)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != ELIGIBILITY_EXAM_COMMIT_VERSION_V1 {
            return Err("an eligibility exam commit of an unknown version");
        }
        if self.claims_count == 0 {
            return Err("an eligibility exam commits at least one claim");
        }
        Ok(())
    }
}

/// A drawn verifier's model-level verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum EligibilityVerdictV1 {
    /// Obtained the registered weights (authenticated against the registered root) and reproduced the examined jobs under the
    /// registered specification.
    Reproduced = 0,
    /// Could not obtain the model, or not in time. Not a fraud finding; nobody is penalized (ADR-0177 D1).
    Unavailable = 1,
    /// Obtained the model; the registered specification does not reproduce deterministically (a model-level fault).
    NotReproducible = 2,
}

/// **Inner 75.** Signed by `verifier`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EligibilityAttestationV1 {
    pub version: u16,
    pub network: Digest,
    pub exam: Digest,
    /// The attesting bond's kernel bond id; must be the object's signer and among the exam's draws.
    pub verifier: Digest,
    pub verdict: EligibilityVerdictV1,
    /// Digest of the verifier's public re-execution transcript of the examined claims (never judged by the consumer).
    pub transcript: Digest,
    /// Examined-claim indices whose committed answers did not match: per-claim findings only.
    pub claim_mismatches: Vec<u32>,
}

impl EligibilityAttestationV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != ELIGIBILITY_ATTESTATION_VERSION_V1 {
            return Err("an eligibility attestation of an unknown version");
        }
        if self.claim_mismatches.len() > MAX_ELIGIBILITY_MISMATCHES_V1 {
            return Err("an eligibility attestation names too many mismatching claims");
        }
        if self.claim_mismatches.windows(2).any(|w| w[0] >= w[1]) {
            return Err("an eligibility attestation's mismatches are not strictly ascending");
        }
        if self.verdict != EligibilityVerdictV1::Reproduced && !self.claim_mismatches.is_empty() {
            return Err("only a verifier that reproduced the model can name mismatching claims");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_ids_bind_every_field_and_attestations_validate_their_shape() {
        let commit = EligibilityExamCommitV1 {
            version: ELIGIBILITY_EXAM_COMMIT_VERSION_V1,
            network: [1; 64],
            committer: [2; 64],
            class_id: [3; 64],
            purpose: EligibilityExamPurposeV1::Promotion,
            claims_root: [4; 64],
            claims_count: 16,
            attempt_index: 0,
        };
        commit.validate().unwrap();
        assert_ne!(commit.id(), EligibilityExamCommitV1 { attempt_index: 1, ..commit }.id(), "the attempt index is in the id");
        assert_ne!(commit.id(), EligibilityExamCommitV1 { claims_root: [5; 64], ..commit }.id(), "the claim set is in the id");
        assert!(EligibilityExamCommitV1 { claims_count: 0, ..commit }.validate().is_err());
        let mut a = EligibilityAttestationV1 {
            version: ELIGIBILITY_ATTESTATION_VERSION_V1,
            network: [1; 64],
            exam: commit.id(),
            verifier: [6; 64],
            verdict: EligibilityVerdictV1::Reproduced,
            transcript: [7; 64],
            claim_mismatches: vec![0, 3],
        };
        a.validate().unwrap();
        a.claim_mismatches = vec![3, 0];
        assert!(a.validate().is_err(), "unsorted");
        a.claim_mismatches = vec![1];
        a.verdict = EligibilityVerdictV1::Unavailable;
        assert!(a.validate().is_err(), "a verifier without the model cannot accuse a claim");
        let bytes = borsh::to_vec(&a).unwrap();
        assert!(bytes.len() + 2 <= MAX_ELIGIBILITY_OBJECT_BYTES_V1);
    }
}
