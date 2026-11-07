//! **The committed subject** (RFC-0007 §VI.2, §VI.5): what a challenge is about, fixed before any source window opens.
//!
//! Every subject kind has its own seed (the kind is inside the seed preimage), so model conformance, claim verification, a work
//! slice, public prosecution and kernel conformance never share a seed domain. Roots that do not apply are [`RootV1::Absent`] —
//! typed absence, never a zero wildcard that a present all-zero root could collide with.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::{DOMAIN_SUBJECT, Digest, object_id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum SubjectKindV1 {
    KernelConformance = 1,
    ModelConformance = 2,
    ClaimVerification = 3,
    WorkSlice = 4,
    /// Only explicitly committed supplemental checks: never a favorable reroll of a claim's own checks, and never a prerequisite
    /// for filing an already authenticated exact fraud proof.
    PublicProsecution = 5,
    /// RFC-0010 Panel binding entropy. Its sources must be Panel-independent Finals ([`crate::beacon::FinalPathV1`]).
    PanelAssignment = 6,
}

impl SubjectKindV1 {
    pub const ALL: [Self; 6] = [
        Self::KernelConformance,
        Self::ModelConformance,
        Self::ClaimVerification,
        Self::WorkSlice,
        Self::PublicProsecution,
        Self::PanelAssignment,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::KernelConformance => "KERNEL_CONFORMANCE",
            Self::ModelConformance => "MODEL_CONFORMANCE",
            Self::ClaimVerification => "CLAIM_VERIFICATION",
            Self::WorkSlice => "WORK_SLICE",
            Self::PublicProsecution => "PUBLIC_PROSECUTION",
            Self::PanelAssignment => "PANEL_ASSIGNMENT",
        }
    }
}

/// A root that applies, or explicit absence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum RootV1 {
    Absent,
    Present(Digest),
}

impl From<Digest> for RootV1 {
    fn from(d: Digest) -> Self {
        Self::Present(d)
    }
}

/// The seed-bearing subject (RFC-0007 §VI.5's preimage, minus the anchor and the beacon output).
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ChallengeSubjectV1 {
    pub chain_genesis: Digest,
    pub ruleset_id: Digest,
    pub challenge_policy_id: Digest,
    pub subject_kind: SubjectKindV1,
    pub subject_id: Digest,
    pub kernel_id: RootV1,
    pub verification_plan_root: RootV1,
    pub program_root: RootV1,
    pub artifact_root: RootV1,
    pub tokenizer_or_schema_root: RootV1,
    pub layout_root: RootV1,
    pub input_root: RootV1,
    pub state_root: RootV1,
    pub constraint_root: RootV1,
    /// The pre-beacon commitment that binds every further subject-specific root (RFC-0013 §9's statement digest, a claim's
    /// commitment root, a slice's binding).
    pub commitment_root: Digest,
}

impl ChallengeSubjectV1 {
    pub fn id(&self) -> Digest {
        object_id(DOMAIN_SUBJECT, self)
    }
}
