//! **The single post-commit challenge contract** (RFC-0007 Part VI; RFC-0011 §17; RFC-0013 §9) — shared by kernel conformance,
//! per-claim verification, EXEC work slices and public prosecution. Historical model-onboarding contracts do not authorize
//! new model reviews: registered models follow the frozen two-tier policy in PRINCIPLES §9.
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
//! Individual consumers have versioned activation and policy contracts; importing a helper does not activate a source.
//! The new [`bootstrap_epoch_v1`] mechanics are dormant: native epoch authority, selected approval receipts and the source
//! lineage consumer remain unconnected. Its domains and parameters are not release approval. Consensus credit requires
//! randomly selected verifier approval under explicit honest-fraction, participation and material-availability assumptions.
//!
//! [`composition`] is not part of the contract: a pure calculator of the composed false-accept bound for the external soundness
//! review dossier (`docs/design/palw/soundness-review-dossier/`), called by no consensus or policy code.

pub mod approved_source_v1;
pub mod beacon;
pub mod bootstrap_epoch_v1;
pub mod composition;
pub mod conformance;
pub mod hash;
pub mod lifecycle;
pub mod opt_in;
pub mod paid_opt_in;
pub mod policy;
pub mod sealed;
pub mod seed;
pub mod soundness;
pub mod subject;

pub use beacon::{
    AttributedWorkV1, BeaconContextV1, BeaconSourceV1, FinalPathV1, IneligibleV1, SourceAttributionV1, VerifiedWorkBeaconV1,
    WorkBeaconStateV1, WorkBeaconV1, WorkFinalEventV1, WorkSourceKindV1, collect_attributed_work_beacon_v1, collect_work_beacon_v1,
    lock_evidence_root_v1, verify_attributed_work_beacon_v1, verify_work_beacon_v1,
};
pub use conformance::{BeaconConformanceEvidenceV1, ConformanceCommitmentV1, ConformanceStatusV1, verify_conformance_evidence_v1};
pub use hash::Digest;
pub use lifecycle::{OnboardingFailureV1, OnboardingRecordV1, OnboardingStateV1, OnboardingStepV1};
pub use policy::{
    ApprovedTupleV1, ImplementedV1, InteractiveModeV1, PostCommitChallengePolicyV1, ReviewedFailureBoundV2, ReviewedTupleV2,
    SourceRuleV1, approved_v1, approved_v2, complete_check_policy_v1, reference_policy_v1, sealed_source_policy_v3,
    shipped_registry_v1, shipped_registry_v2,
};
pub use sealed::{
    SealRevealV3, SealedBeaconStateV3, SealedSourceV3, SourceFateV3, collect_sealed_work_beacon_v3, combine_failure_bits_v1,
    sealed_beacon_grinding_choices_v3, sealed_source_censorship_bits_v3, verify_sealed_work_beacon_v3,
};
pub use seed::{ChallengeStreamV1, FiatShamirTranscriptV1, StreamKindV1, StreamLabelV1, challenge_seed_v1, staged_round_challenge_v1};
pub use soundness::{
    APPROVAL_MIN_TARGET_BITS_V1, CumulativeFailureTermV1, EffectiveBitsV1, EffectiveSoundnessInputV1, RelationSoundnessV1,
    SoundnessRefusalV1, beacon_grinding_choices_bound_v1, competing_works_bound_v1, cumulative_failure_bits_v1,
    effective_false_accept_bits_v1,
};
pub use subject::{ChallengeSubjectV1, RootV1, SubjectKindV1};

pub use opt_in::{OptInContractV4, OptInSourceV4, collect_opt_in_work_beacon_v4, verify_opt_in_work_beacon_v4};
pub use paid_opt_in::{
    PaidOptInContractV5, PaidSourcePromiseV5, collect_paid_opt_in_work_beacon_v5, verify_paid_opt_in_work_beacon_v5,
};
pub use policy::opt_in_source_policy_v4;
pub use policy::paid_opt_in_source_policy_v5;

pub mod approval_scope;
