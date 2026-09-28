//! **RFC-0004 §6: the candidate's wire payload** (`CandidateSubmitted`, tag 80).
//!
//! Skeleton (step 0) of the candidates lane's module (`rfc4/cand` owns it from the skeleton's sha on):
//! the payload's shape. Its validation (the family rule, the composite rule, the provenance policy's
//! form and references) is the lane's.

use crate::Hash64;
use crate::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use borsh::{BorshDeserialize, BorshSerialize};

/// Key of [`palw_candidate_declarations_digest_v1`].
pub const PALW_IMPROVE_DECLARATIONS_DOMAIN_V1: &[u8] = b"misaka-palw/improve/candidate-declarations/v1";

/// **A candidate's declarations** (RFC-0004 §2.1 check 2, §8.2, §9): what the provenance policy checks
/// in form and references (never in truth).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCandidateDeclarationsV1 {
    /// Registered datasets the trainer declares it used, each with its declared weight in permille (S2).
    pub datasets: Vec<(Hash64, u16)>,
    /// Registered `TeacherLicence` ids the candidate's training material relied on.
    pub licences: Vec<Hash64>,
    /// A mask of [`crate::palw_improve_state_v1::PalwTeacherClassV1::bit`].
    pub teacher_classes: u8,
}

/// **The `CandidateSubmitted` payload** (tag 80): an admitted IR class entered in an epoch.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCandidateSubmissionV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    /// The candidate's IR class id (Phase F's `tir_class_id_v1` over its artifact root).
    pub class_id: Hash64,
    pub artifact: PalwTirArtifactRefV1,
    /// The candidate class's layout, carried because the `tir_classes` record keeps only its digest:
    /// admission rebuilds the class over the composite root to recheck `class_id`, and sizes the
    /// composite close's step space from it.
    pub layout: crate::palw_tir_class_v1::PalwTirLayoutV1,
    pub declarations: PalwCandidateDeclarationsV1,
}

/// **The declarations' digest**, as the epoch keeps it ([`crate::palw_improve_state_v1::PalwEpochCandidateV1`]).
pub fn palw_candidate_declarations_digest_v1(declarations: &PalwCandidateDeclarationsV1) -> Hash64 {
    let bytes = borsh::to_vec(declarations).expect("declarations are borsh-serializable");
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_IMPROVE_DECLARATIONS_DOMAIN_V1).to_state();
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}
