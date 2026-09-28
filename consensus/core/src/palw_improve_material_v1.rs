//! **RFC-0004 §5, §9, §10: the training and evaluation material's wire payloads** — hard cases, the
//! data-use opt-in, setter sets, registered datasets, teaching artifacts and teacher licences (tags
//! 71–79).
//!
//! Skeleton (step 0) of the candidates lane's module (`rfc4/cand` owns it from the skeleton's sha on):
//! the payloads' shapes. Their validation and admission are the lane's (A4).

use crate::Hash64;
use crate::palw_improve_state_v1::{PalwTeacherClassV1, PalwTeachingArtifactKindV1, PalwVerificationTypeV1};
use borsh::{BorshDeserialize, BorshSerialize};

/// What a hard case is scored against (RFC-0004 §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwCaseReferenceV1 {
    /// An exact-match task: the committed answer span.
    ExactKey { commitment: Hash64 },
    /// A likelihood task: the committed reference continuation.
    Continuation { commitment: Hash64 },
    /// Judged only.
    None,
}

/// Where a hard case came from (RFC-0004 §5.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwCaseSourceV1 {
    /// Real usage, under the job's `DataUseOptIn`.
    UsageOptIn { job_pin: Hash64 },
    /// A bonded problem setter.
    Setter,
    /// A `SyntheticProblem` or `HardCaseVariant` artifact.
    Artifact { artifact_id: Hash64 },
}

/// **`HardCaseSubmitted` (tag 71).**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHardCaseV1 {
    pub line_id: Hash64,
    pub case_id: Hash64,
    pub domain: u16,
    /// Token ids under the line's tokenizer.
    pub prompt_ids: Vec<u32>,
    pub reference: PalwCaseReferenceV1,
    pub source: PalwCaseSourceV1,
    /// A final evaluation-kind claim of the head that fails the case's reference, when the case claims hardness.
    pub head_evidence: Option<Hash64>,
}

/// **`DataUseOptIn` (tag 72)** (RFC-0004 §10): signed by the job's committer.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDataUseOptInV1 {
    pub job_pin: Hash64,
}

/// **`SetterSetCommitted` (tag 73)** (RFC-0004 §7.1): before `t_close`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSetterSetCommitmentV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub set_id: Hash64,
    pub items: u32,
    pub prompts_commitment: Hash64,
    pub keys_commitment: Hash64,
}

/// **`SetterSetRevealed` (tag 74)**: the prompts, at `Drawn`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSetterSetRevealV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub set_id: Hash64,
    pub prompts: Vec<Vec<u32>>,
    pub salt: Hash64,
}

/// **`SetterKeysRevealed` (tag 75)**: the keys and references, after every subject's outputs are final.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSetterKeysRevealV1 {
    pub line_id: Hash64,
    pub epoch: u64,
    pub set_id: Hash64,
    pub keys: Vec<Vec<u32>>,
    pub salt: Hash64,
}

/// **`DatasetRegistered` (tag 76)** (RFC-0004 §5.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDatasetV1 {
    pub line_id: Hash64,
    pub dataset_id: Hash64,
    pub content_root: Hash64,
    pub items: u64,
    pub license_classes: Vec<Hash64>,
    /// A mask of [`PalwTeacherClassV1::bit`].
    pub teacher_classes: u8,
    pub provenance_commitment: Hash64,
}

/// **`TeachingArtifactCommitted` (tag 77)**: `commit = H(artifact ‖ salt)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeachingArtifactCommitV1 {
    pub line_id: Hash64,
    pub commit: Hash64,
}

/// **`TeachingArtifactRevealed` (tag 78)** (RFC-0004 §5.3).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeachingArtifactV1 {
    pub line_id: Hash64,
    pub kind: PalwTeachingArtifactKindV1,
    pub task_id: Hash64,
    pub teacher_type: PalwTeacherClassV1,
    pub teacher_id: Hash64,
    pub license_class: Hash64,
    pub provenance_commitment: Hash64,
    /// The content's hash; the content lives off chain, content-addressed.
    pub output_hash: Hash64,
    pub verification_type: PalwVerificationTypeV1,
    /// For an EXACT-verified `Answer`: the answer span the fold compares with the key.
    pub answer_span: Vec<u32>,
    pub salt: Hash64,
}

/// **`TeacherLicenceRegistered` (tag 79)** (RFC-0004 §9): a rights holder's licence for `LICENSED_DISTILL`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTeacherLicenceV1 {
    pub licence_id: Hash64,
    /// The rights holder's ML-DSA-87 public key.
    pub rights_holder_key: Vec<u8>,
    pub model_family: Hash64,
    pub domains: Vec<u16>,
    /// A mask of permitted uses (bit 0: training data).
    pub uses: u8,
    pub per_use_fee: u64,
    pub expiry_daa: u64,
}
