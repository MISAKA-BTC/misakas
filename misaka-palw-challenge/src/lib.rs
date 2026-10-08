//! **The single post-commit challenge contract** (RFC-0007 Part VI; RFC-0011 §17; RFC-0013 §9) — shared by kernel conformance,
//! model onboarding, per-claim verification, EXEC work slices and public prosecution. No other crate derives a beacon or a seed.
//!
//! ```text
//! STATIC SEMANTIC ADMISSION → COMMIT FIRST → FUTURE PALW WORK BEACON → INDEPENDENT PROBABILISTIC CHECK → mismatch → EXACT PUBLIC COURT
//! ```
//!
//! The beacon authorizes nothing: it selects unpredictable checks of an already committed statement. Semantic admission, constraint
//! coverage, exact courts, public material and resource bounds come first (the kernel crate's code-derived
//! `PUBLIC_PROSECUTION_COMPLETE` gate among them); a beacon mismatch is never a conviction — it localizes, an authenticated opening
//! goes to the exact court, and withheld material is DA/default, not arithmetic fraud.
//!
//! **Dormant**: no consensus code reads this crate; it has no fence, wire id, state tag or activation. Its domains and algorithm
//! ids are proposed values awaiting the release review of RFC-0007 §VI.8 (codec vectors, source-cost/bias/grinding analysis,
//! k/D/delay selection). Nothing here is BFT, DNS, validator- or committee-derived.
//!
//! [`composition`] is not part of the contract: a pure calculator of the composed false-accept bound for the external soundness
//! review dossier (`docs/design/palw/soundness-review-dossier/`), called by no consensus or policy code.

pub mod beacon;
pub mod composition;
pub mod conformance;
pub mod hash;
pub mod lifecycle;
pub mod policy;
pub mod sealed;
pub mod seed;
pub mod soundness;
pub mod subject;

pub use beacon::{
    AttributedWorkV1, BeaconContextV1, BeaconSourceV1, FinalPathV1, IneligibleV1, SourceAttributionV1, WorkBeaconStateV1,
    WorkBeaconV1, WorkFinalEventV1, WorkSourceKindV1, collect_attributed_work_beacon_v1, collect_work_beacon_v1,
    lock_evidence_root_v1, verify_attributed_work_beacon_v1, verify_work_beacon_v1,
};
pub use conformance::{BeaconConformanceEvidenceV1, ConformanceCommitmentV1, ConformanceStatusV1, verify_conformance_evidence_v1};
pub use hash::Digest;
pub use lifecycle::{OnboardingFailureV1, OnboardingRecordV1, OnboardingStateV1, OnboardingStepV1};
pub use policy::{
    ApprovedTupleV1, InteractiveModeV1, PostCommitChallengePolicyV1, SourceRuleV1, approved_v1, complete_check_policy_v1,
    reference_policy_v1, sealed_source_policy_v3, shipped_registry_v1,
};
pub use sealed::{
    SealRevealV3, SealedBeaconStateV3, SealedSourceV3, SourceFateV3, collect_sealed_work_beacon_v3, combine_failure_bits_v1,
    sealed_beacon_grinding_choices_v3, sealed_source_censorship_bits_v3, verify_sealed_work_beacon_v3,
};
pub use seed::{ChallengeStreamV1, FiatShamirTranscriptV1, StreamKindV1, StreamLabelV1, challenge_seed_v1, staged_round_challenge_v1};
pub use soundness::{
    APPROVAL_MIN_TARGET_BITS_V1, EffectiveBitsV1, EffectiveSoundnessInputV1, RelationSoundnessV1, SoundnessRefusalV1,
    beacon_grinding_choices_bound_v1, competing_works_bound_v1, effective_false_accept_bits_v1,
};
pub use subject::{ChallengeSubjectV1, RootV1, SubjectKindV1};
