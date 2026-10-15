//! **RFC-0017 — model eligibility: registration is free, economic and consensus influence is examined, bounded and renewed**
//! (`docs/rfc/0017-model-eligibility-examination-and-renewal.md`).
//!
//! The DORMANT fence `palw_model_eligibility_v1` — `None` on every preset and in no flag-day list, hashed Some-only in both
//! fingerprints, collapsed from `Some(never())`, refused when armed by [`Params::validate_palw_model_eligibility_v1`] until its rule is
//! built, verified and its values are approved. Nothing here is production approval of any value.
//!
//! ```text
//! REQ-ELIG-0  registration stays free and immutable (ADR-0175); the chain never inspects model availability (ADR-0177)
//! REQ-ELIG-1  a class registered at/after the fence starts `Registered`: user-paid jobs complete and settle (escrow, B = 0 / F = 0
//!             kernel claims untouched), but it earns NO coinbase allocation, NO reward-bearing block right and NO Final weight
//! REQ-ELIG-2  `Probation`: subsidy and block rights only inside min(per-principal common cap [ADR-0176, not edited here],
//!             per-stage model cap, network-wide probation budget); the network budget is a fixed number per epoch that never grows
//!             with the model count; Final weight off (initial proposal: job completion is separated from consensus credit)
//! REQ-ELIG-3  promotion: the model, the examined claim set and the participating stake are committed FIRST (inner 74); the draw's
//!             randomness is the first execution-lane seed anchor recorded strictly after the commit (ADR-0130 `round_seed_anchor`),
//!             so the registrant cannot nominate examiners or keep a favourable draw; m stake-proportional draws, k positive
//!             authenticated attestations (inner 75) by the deadline promote; anything else is "not promoted" — never a slash
//! REQ-ELIG-4  attempts are counted per model lineage (artifact root: re-registration keeps them), per principal (operator) and
//!             network-wide per epoch; a commit consumes an attempt whether or not the draw is later liked (no withdrawal refund)
//! REQ-ELIG-5  `Eligible` lasts until `next_renewal_daa`; a renewal exam passed inside the renewal window extends it; otherwise
//!             the class is `Lapsed` after the grace. One pass is never permanent
//! REQ-ELIG-6  loss of eligibility never cancels existing claims' collateral, challenge or settlement liability; per-claim faults go
//!             to the claim's own court and never decide the model-wide verdict (a single Miner cannot halt a public model)
//! REQ-ELIG-7  below the fence (every shipped network: the mirror is `None`) every predicate answers the old rule; a claim accepted
//!             below the fence keeps its old rights; inner 74/75 ride unjudged below the fence and the fold writes nothing (A-2)
//! ```
//!
//! **Allocations (grepped 2026-10-15 on `integ/rfc02-rfc06-12-g14`):** kernel-route inner kinds **74** (`CommitEligibilityExamV1`)
//! and **75** (`AttestEligibilityExamV1`); kernel-route aux table **131** ([`PALW_MODEL_ELIGIBILITY_TABLE_V1`]) with five 65-byte
//! prefixed key namespaces. No object tag, RPC op, messages.proto field, p2p version or funded action is allocated; the RPC exposes
//! the stage through appended fields of the existing model-registry row.
//!
//! **Not decided here (named, never invented):** every value of [`PalwModelEligibilityPolicyV1`]. The `*_DESIGN_V1` constants are the
//! RFC's illustrative design values (RFC-0017 §11) so tests and the simulation driver have something to read; validation refuses every
//! activation, so none of them is a production value.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// A 64-byte identifier (the kernel route's `Digest`): class ids, artifact roots, bond ids, operator ids, exam ids.
pub type EligDigestV1 = [u8; 64];

pub const PALW_MODEL_ELIGIBILITY_VERSION_V1: u16 = 1;

/// **Kernel-route aux table 131** — every eligibility row, under five 65-byte key namespaces (prefix ‖ 64-byte id).
pub const PALW_MODEL_ELIGIBILITY_TABLE_V1: u8 = 131;
/// `'M' ‖ class id` → [`PalwEligibilityModelRowV1`].
pub const ELIGIBILITY_MODEL_PREFIX_V1: u8 = 0x4D;
/// `'L' ‖ artifact root` → [`PalwEligibilityLineageRowV1`] (survives re-registration of the same weights).
pub const ELIGIBILITY_LINEAGE_PREFIX_V1: u8 = 0x4C;
/// `'P' ‖ operator id` → [`PalwEligibilityPrincipalRowV1`].
pub const ELIGIBILITY_PRINCIPAL_PREFIX_V1: u8 = 0x50;
/// `'E' ‖ exam id` → [`PalwEligibilityExamRowV1`].
pub const ELIGIBILITY_EXAM_PREFIX_V1: u8 = 0x45;
/// `'N' ‖ 64 zero bytes` → [`PalwEligibilityNetworkRowV1`] (one row).
pub const ELIGIBILITY_NETWORK_PREFIX_V1: u8 = 0x4E;
/// `'C' ‖ claim id` → [`PalwEligibilityProbationClaimRowV1`]: a claim admitted under `Probation` rights (pinned at acceptance, retired
/// at its Final, void or retirement), so its Final reads the rights it was accepted with.
pub const ELIGIBILITY_PROBATION_CLAIM_PREFIX_V1: u8 = 0x43;

/// Kernel-route inner kind 74: commit an eligibility exam (model, claim set, purpose; the stake cut is the commit's DAA).
pub const PALW_KERNEL_INNER_COMMIT_ELIGIBILITY_EXAM_V1: u8 = 74;
/// Kernel-route inner kind 75: a drawn verifier's authenticated attestation for one exam.
pub const PALW_KERNEL_INNER_ATTEST_ELIGIBILITY_EXAM_V1: u8 = 75;

pub const PALW_ELIGIBILITY_EXAM_ID_DOMAIN_V1: &[u8] = b"misaka-palw/model-eligibility/exam-id/v1";
pub const PALW_ELIGIBILITY_EXAM_SEED_DOMAIN_V1: &[u8] = b"misaka-palw/model-eligibility/exam-seed/v1";
pub const PALW_ELIGIBILITY_CLAIM_PICK_DOMAIN_V1: &[u8] = b"misaka-palw/model-eligibility/claim-pick/v1";
pub const PALW_ELIGIBILITY_ATTESTATION_DOMAIN_V1: &[u8] = b"misaka-palw/model-eligibility/attestation/v1";
pub const PALW_ELIGIBILITY_ATTESTATION_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/model-eligibility/attestation/mldsa87/v1";

// =================================================================================================================================
// Design values (RFC-0017 §11) — illustrative, unapproved; validation refuses every activation.
// =================================================================================================================================

/// `m`: stake-proportional draws per exam (RFC-0017 §5 uses 4-of-7 as its worked example).
pub const PALW_ELIG_EXAM_M_DESIGN_V1: u16 = 7;
/// `k`: positive attestations an exam needs. Must be a strict majority of `m` (validation).
pub const PALW_ELIG_EXAM_K_DESIGN_V1: u16 = 4;
/// Claims of the committed set each drawn verifier re-executes.
pub const PALW_ELIG_EXAM_CLAIMS_DESIGN_V1: u16 = 4;
/// Spans between the commit's span and the earliest seed anchor that may fix its draw (strictly later: at least 1).
pub const PALW_ELIG_ANCHOR_DELAY_SPANS_DESIGN_V1: u64 = 1;
/// Days a drawn verifier has to attest after the draw.
pub const PALW_ELIG_ATTEST_WINDOW_DAYS_DESIGN_V1: u64 = 2;
/// Days a class stays in `Probation` before its first promotion exam may commit.
pub const PALW_ELIG_PROBATION_MIN_DAYS_DESIGN_V1: u64 = 7;
/// Committed probation claims a promotion exam must name at least.
pub const PALW_ELIG_PROBATION_MIN_CLAIMS_DESIGN_V1: u32 = 16;
/// Days of a probation budget epoch.
pub const PALW_ELIG_PROBATION_EPOCH_DAYS_DESIGN_V1: u64 = 1;
/// Network-wide probation budget per epoch, in permille of the epoch's PALW block opportunity (fixed; never × model count).
pub const PALW_ELIG_PROBATION_NETWORK_PERMILLE_DESIGN_V1: u16 = 20;
/// Per-model probation cap per epoch, in permille of the epoch's PALW block opportunity.
pub const PALW_ELIG_PROBATION_MODEL_PERMILLE_DESIGN_V1: u16 = 2;
/// Lifetime exam attempts of one model lineage (artifact root), promotion and renewal together.
pub const PALW_ELIG_ATTEMPTS_PER_LINEAGE_DESIGN_V1: u32 = 24;
/// Exam attempts one principal (operator) may commit per attempt epoch.
pub const PALW_ELIG_ATTEMPTS_PER_PRINCIPAL_EPOCH_DESIGN_V1: u32 = 4;
/// Exam attempts the whole network admits per attempt epoch (fixed; bounds Σ attempts × P_pass).
pub const PALW_ELIG_ATTEMPTS_NETWORK_EPOCH_DESIGN_V1: u32 = 64;
/// Days of an attempt epoch.
pub const PALW_ELIG_ATTEMPT_EPOCH_DAYS_DESIGN_V1: u64 = 30;
/// Days between two exam commits of the same lineage.
pub const PALW_ELIG_ATTEMPT_COOLDOWN_DAYS_DESIGN_V1: u64 = 3;
/// Days an `Eligible` class keeps its rights without a passed renewal.
pub const PALW_ELIG_RENEWAL_EPOCH_DAYS_DESIGN_V1: u64 = 30;
/// Days before `next_renewal_daa` a renewal exam may commit.
pub const PALW_ELIG_RENEWAL_WINDOW_DAYS_DESIGN_V1: u64 = 7;
/// Days after `next_renewal_daa` before a class without a passed renewal is `Lapsed`.
pub const PALW_ELIG_LAPSE_GRACE_DAYS_DESIGN_V1: u64 = 2;
/// Days a bond must have been registered before an exam's commit to be in its verifier population.
pub const PALW_ELIG_VERIFIER_MATURITY_DAYS_DESIGN_V1: u64 = 7;
/// **The non-refundable exam-attempt fee** (RFC-0017 §5, GAP-E3; the VALUE is user decision U-4): burned from the committing bond's
/// free collateral at the commit — whatever the draw, the outcome or a later withdrawal — so filling the network's attempt budget
/// costs `attempts_network_per_epoch × fee` per attempt epoch ([`palw_eligibility_griefing_cost_v1`]). Burned, never paid out:
/// no examiner is paid from new issuance. 100,000 BILI (in sompi) is a PLACEHOLDER so the mechanism has a non-zero value to test.
pub const PALW_ELIG_ATTEMPT_FEE_DESIGN_V1: u64 = 100_000 * 100_000_000;

// =================================================================================================================================
// Stage, rights
// =================================================================================================================================

/// **The eligibility stage of a model** — an overlay on the ADR-0135 registry lifecycle ([`crate::palw_model_registry_v1::
/// PalwModelLifecycleV1`]), which keeps deciding serviceability (whether claims are admitted at all and at what cadence). This stage
/// decides only economic and consensus rights. Borsh: the order is chain bytes; append only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PalwEligibilityStageV1 {
    /// Exists; user-paid jobs complete. No subsidy, no reward-bearing block right, no Final weight.
    Registered,
    /// Bounded subsidy and block rights; no Final weight (initial proposal).
    Probation,
    /// Normal rights until `next_renewal_daa` (+ grace).
    Eligible,
    /// A renewal was not passed: back to `Registered`'s rights. Existing liabilities survive.
    Lapsed,
}

impl PalwEligibilityStageV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::Probation => "probation",
            Self::Eligible => "eligible",
            Self::Lapsed => "lapsed",
        }
    }
}

/// **The three outcomes RFC-0017 §6 gates separately** (plus job completion, which no stage removes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEligibilityRightsV1 {
    /// A user-paid job's claim may complete and settle its own escrow (no issuance, no consensus credit). Always true.
    pub job_completion: bool,
    /// Coinbase subsidy eligibility and payment (the model-allocation `R_m` leg and every subsidy-carved PALW reward).
    pub subsidy: bool,
    /// Reward-bearing block rights: issuance slots, admission of reward-bearing claims and the ADR-0176 `B` cap's consumption.
    pub block_right: bool,
    /// Chain choice: provisional credit and Final weight (the `F` leg). `false` = the claim's Final is job completion only.
    pub final_weight: bool,
    /// `subsidy`/`block_right` draw on the probation budgets ([`palw_probation_grant_v1`]) rather than on the normal allocation.
    pub probation_bounded: bool,
}

impl PalwEligibilityRightsV1 {
    /// What every class has below the fence and every genesis class has always: the existing rules decide, nothing is added.
    pub const OLD_RULE: Self =
        Self { job_completion: true, subsidy: true, block_right: true, final_weight: true, probation_bounded: false };
    /// Job completion only.
    pub const JOB_ONLY: Self =
        Self { job_completion: true, subsidy: false, block_right: false, final_weight: false, probation_bounded: false };
}

/// The rights a stage carries under `policy` (RFC-0017 §4 A/B/D).
pub fn palw_eligibility_rights_of_stage_v1(
    stage: PalwEligibilityStageV1,
    policy: &PalwModelEligibilityPolicyV1,
) -> PalwEligibilityRightsV1 {
    match stage {
        PalwEligibilityStageV1::Registered | PalwEligibilityStageV1::Lapsed => PalwEligibilityRightsV1::JOB_ONLY,
        PalwEligibilityStageV1::Probation => PalwEligibilityRightsV1 {
            job_completion: true,
            subsidy: true,
            block_right: true,
            final_weight: policy.probation_final_weight,
            probation_bounded: true,
        },
        PalwEligibilityStageV1::Eligible => PalwEligibilityRightsV1 { probation_bounded: false, ..PalwEligibilityRightsV1::OLD_RULE },
    }
}

// =================================================================================================================================
// Policy
// =================================================================================================================================

pub const PALW_MODEL_ELIGIBILITY_POLICY_VERSION_V1: u16 = 1;

/// **RFC-0017's terms.** Every value is policy with no approved production value; a ruleset carries it only through a fence
/// validation refuses. Durations are in DAA; [`Self::design_unapproved_v1`] derives them from a network's `day_daa`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwModelEligibilityPolicyV1 {
    pub version: u16,
    /// The network's DAA per day (the unit every `*_DAYS` design value is converted with).
    pub day_daa: u64,
    // ---- B. probation --------------------------------------------------------------------------------------------------------
    pub probation_epoch_daa: u64,
    /// Network-wide probation budget per epoch, permille of the epoch's PALW block opportunity. Fixed; never multiplied by the
    /// number of models or principals.
    pub probation_network_permille: u16,
    /// Per-model (per-stage) probation cap per epoch, permille of the epoch's PALW block opportunity.
    pub probation_model_permille: u16,
    /// Whether a probation claim's Final creates consensus credit (initial proposal: `false`).
    pub probation_final_weight: bool,
    pub probation_min_daa: u64,
    pub probation_min_claims: u32,
    // ---- C. examination ------------------------------------------------------------------------------------------------------
    pub exam_m: u16,
    pub exam_k: u16,
    pub exam_claims: u16,
    pub exam_anchor_delay_spans: u64,
    pub exam_attest_window_daa: u64,
    /// A bond is in an exam's verifier population when Active, registered at least this long before the commit, and holding at
    /// least `verifier_min_collateral_sompi` (0 = the network's bond floor already applies).
    pub verifier_maturity_daa: u64,
    pub verifier_min_collateral_sompi: u64,
    pub attempts_per_lineage_max: u32,
    pub attempts_per_principal_per_epoch: u32,
    pub attempts_network_per_epoch: u32,
    pub attempt_epoch_daa: u64,
    pub attempt_cooldown_daa: u64,
    /// The non-refundable attempt fee, burned at commit (sompi; U-4).
    pub attempt_fee_sompi: u64,
    // ---- D. renewal ----------------------------------------------------------------------------------------------------------
    pub renewal_epoch_daa: u64,
    pub renewal_window_daa: u64,
    pub lapse_grace_daa: u64,
}

impl PalwModelEligibilityPolicyV1 {
    /// **The RFC's design values at a network's `day_daa`** — illustrative and UNAPPROVED (RFC-0017 §11). Tests and the
    /// simulation driver read them; no preset carries them and validation refuses an armed fence.
    pub const fn design_unapproved_v1(day_daa: u64) -> Self {
        Self {
            version: PALW_MODEL_ELIGIBILITY_POLICY_VERSION_V1,
            day_daa,
            probation_epoch_daa: PALW_ELIG_PROBATION_EPOCH_DAYS_DESIGN_V1 * day_daa,
            probation_network_permille: PALW_ELIG_PROBATION_NETWORK_PERMILLE_DESIGN_V1,
            probation_model_permille: PALW_ELIG_PROBATION_MODEL_PERMILLE_DESIGN_V1,
            probation_final_weight: false,
            probation_min_daa: PALW_ELIG_PROBATION_MIN_DAYS_DESIGN_V1 * day_daa,
            probation_min_claims: PALW_ELIG_PROBATION_MIN_CLAIMS_DESIGN_V1,
            exam_m: PALW_ELIG_EXAM_M_DESIGN_V1,
            exam_k: PALW_ELIG_EXAM_K_DESIGN_V1,
            exam_claims: PALW_ELIG_EXAM_CLAIMS_DESIGN_V1,
            exam_anchor_delay_spans: PALW_ELIG_ANCHOR_DELAY_SPANS_DESIGN_V1,
            exam_attest_window_daa: PALW_ELIG_ATTEST_WINDOW_DAYS_DESIGN_V1 * day_daa,
            verifier_maturity_daa: PALW_ELIG_VERIFIER_MATURITY_DAYS_DESIGN_V1 * day_daa,
            verifier_min_collateral_sompi: 0,
            attempts_per_lineage_max: PALW_ELIG_ATTEMPTS_PER_LINEAGE_DESIGN_V1,
            attempts_per_principal_per_epoch: PALW_ELIG_ATTEMPTS_PER_PRINCIPAL_EPOCH_DESIGN_V1,
            attempts_network_per_epoch: PALW_ELIG_ATTEMPTS_NETWORK_EPOCH_DESIGN_V1,
            attempt_epoch_daa: PALW_ELIG_ATTEMPT_EPOCH_DAYS_DESIGN_V1 * day_daa,
            attempt_cooldown_daa: PALW_ELIG_ATTEMPT_COOLDOWN_DAYS_DESIGN_V1 * day_daa,
            attempt_fee_sompi: PALW_ELIG_ATTEMPT_FEE_DESIGN_V1,
            renewal_epoch_daa: PALW_ELIG_RENEWAL_EPOCH_DAYS_DESIGN_V1 * day_daa,
            renewal_window_daa: PALW_ELIG_RENEWAL_WINDOW_DAYS_DESIGN_V1 * day_daa,
            lapse_grace_daa: PALW_ELIG_LAPSE_GRACE_DAYS_DESIGN_V1 * day_daa,
        }
    }

    /// Structural validation (never an approval of the values).
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PALW_MODEL_ELIGIBILITY_POLICY_VERSION_V1 {
            return Err(format!("eligibility policy version {} is not {}", self.version, PALW_MODEL_ELIGIBILITY_POLICY_VERSION_V1));
        }
        if self.day_daa == 0 || self.probation_epoch_daa == 0 || self.attempt_epoch_daa == 0 || self.renewal_epoch_daa == 0 {
            return Err("eligibility epochs and the day must be positive".into());
        }
        if self.exam_m == 0 || self.exam_k == 0 || self.exam_k > self.exam_m || 2 * (self.exam_k as u32) <= self.exam_m as u32 {
            return Err("an exam needs 0 < k <= m with k a strict majority of m".into());
        }
        if self.exam_claims == 0 || (self.exam_claims as u32) > self.probation_min_claims.max(1) {
            return Err("an exam examines at least one claim and no more than the committed minimum".into());
        }
        if self.exam_anchor_delay_spans == 0 {
            return Err("the draw's anchor must be strictly later than the commit (delay >= 1 span)".into());
        }
        if self.exam_attest_window_daa == 0 {
            return Err("the attestation window must be positive".into());
        }
        if self.probation_model_permille > self.probation_network_permille || self.probation_network_permille > 1_000 {
            return Err("the per-model probation cap cannot exceed the network's, nor the network's 1000 permille".into());
        }
        if self.attempts_per_lineage_max == 0
            || self.attempts_per_principal_per_epoch == 0
            || self.attempts_network_per_epoch == 0
            || self.attempts_per_principal_per_epoch > self.attempts_network_per_epoch
        {
            return Err("attempt bounds must be positive and the principal's within the network's".into());
        }
        if self.attempt_fee_sompi == 0 {
            return Err(
                "an exam attempt must cost a non-refundable fee (GAP-E3): a free attempt makes the network budget free to grief"
                    .into(),
            );
        }
        if self.renewal_window_daa == 0 || self.renewal_window_daa > self.renewal_epoch_daa {
            return Err("the renewal window must be positive and inside the renewal epoch".into());
        }
        Ok(())
    }

    /// Policy id (hashed into the fingerprint beside the height when a fence ever carries one).
    pub fn id(&self) -> EligDigestV1 {
        hash64(PALW_ELIGIBILITY_EXAM_ID_DOMAIN_V1, &[b"policy", &borsh::to_vec(self).expect("a policy serializes")])
    }
}

// =================================================================================================================================
// Rows (aux 131)
// =================================================================================================================================

/// `'M' ‖ class id`: one model's eligibility.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEligibilityModelRowV1 {
    pub class_id: EligDigestV1,
    /// The weights' identity (the class's artifact root): re-registering the same weights joins this lineage.
    pub lineage: EligDigestV1,
    /// The registrant's operator id (all-zero for a class without a registrant).
    pub principal: EligDigestV1,
    pub stage: PalwEligibilityStageV1,
    pub registered_daa: u64,
    pub stage_since_daa: u64,
    /// Probation units this class has consumed (display; the per-epoch cap counts on the lineage row).
    pub probation_units_total: u64,
    /// `0` when never eligible.
    pub next_renewal_daa: u64,
    pub open_exam: Option<EligDigestV1>,
    pub exams_passed: u32,
}

/// `'L' ‖ artifact root`: a lineage's lifetime counters — they never decrease and survive re-registration.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEligibilityLineageRowV1 {
    pub attempts_total: u32,
    pub last_commit_daa: u64,
    /// The probation epoch `probation_units_used` counts in — the per-stage model cap is charged HERE, so a re-registration of the
    /// same weights under another class id shares it.
    pub probation_epoch: u64,
    pub probation_units_used: u64,
    pub probation_units_total: u64,
    /// Attempt fees burned for this lineage's exams (GAP-E3).
    pub fees_burned_sompi: u128,
}

/// `'P' ‖ operator id`: a principal's counters. A Sybil principal is a new key with its own bond floor; the network budgets
/// ([`PalwEligibilityNetworkRowV1`]) are what bound a split, and the lineage counters what bound a re-registration.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEligibilityPrincipalRowV1 {
    pub attempt_epoch: u64,
    pub attempts_in_epoch: u32,
    pub attempts_total: u32,
    pub probation_units_total: u64,
    /// Attempt fees this principal's bonds burned (GAP-E3).
    pub fees_burned_sompi: u128,
}

/// `'N' ‖ 0^64`: the network-wide budgets.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEligibilityNetworkRowV1 {
    pub probation_epoch: u64,
    pub probation_units_used: u64,
    pub attempt_epoch: u64,
    pub attempts_in_epoch: u32,
    pub attempts_total: u64,
    /// Attempt fees burned network-wide (GAP-E3).
    pub fees_burned_sompi: u128,
}

/// `'C' ‖ claim id`: the pin of a probation-admitted claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEligibilityProbationClaimRowV1 {
    pub class_id: EligDigestV1,
    pub accepted_daa: u64,
    /// The probation units (block units) charged at acceptance.
    pub units: u64,
    /// Whether its Final creates consensus credit (the policy's `probation_final_weight` at acceptance).
    pub final_weight: bool,
}

/// The wire terms (kernel crate, inner 74 / 75), under this module's names.
pub use misaka_palw_kernel::model_eligibility_v1::{
    EligibilityAttestationV1 as PalwEligibilityAttestationV1, EligibilityExamCommitV1 as PalwEligibilityExamCommitV1,
    EligibilityExamPurposeV1 as PalwEligibilityExamPurposeV1, EligibilityVerdictV1 as PalwEligibilityVerdictV1,
    MAX_ELIGIBILITY_MISMATCHES_V1 as PALW_ELIGIBILITY_MAX_MISMATCHES_V1,
};

/// One recorded attestation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEligibilityAttestationRowV1 {
    pub verifier: EligDigestV1,
    pub verdict: misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1,
    pub transcript: EligDigestV1,
    pub attested_daa: u64,
}

/// How an exam ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwEligibilityExamOutcomeV1 {
    /// `k` or more positive draws by the deadline.
    Passed,
    /// Fewer than `k` positive draws by the deadline (unavailability, silence, or model-level non-reproduction). Not a slash.
    NotPromoted,
    /// The population could not fill `m` draws (fewer eligible bonds than the draw needs, or zero stake). Counted as an attempt.
    NoPopulation,
}

/// `'E' ‖ exam id`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwEligibilityExamRowV1 {
    /// The commit as carried (its `committer` is the signing bond's kernel id).
    pub commit: misaka_palw_kernel::model_eligibility_v1::EligibilityExamCommitV1,
    /// The committing bond's operator id (the principal its attempt is counted against).
    pub committer_principal: EligDigestV1,
    pub committed_daa: u64,
    pub committed_span: u64,
    /// `(span, execution key)` of the seed anchor that fixed the draw; `None` until one strictly later than the commit exists.
    pub anchor: Option<(u64, EligDigestV1)>,
    /// The `m` drawn bond ids, in draw order, with multiplicity (a bond drawn twice holds two of the `m` positions).
    pub draws: Vec<EligDigestV1>,
    /// Indices into the committed claim list each verifier re-executes.
    pub examined: Vec<u32>,
    pub drawn_daa: u64,
    pub deadline_daa: u64,
    pub attestations: Vec<PalwEligibilityAttestationRowV1>,
    pub outcome: Option<(PalwEligibilityExamOutcomeV1, u64)>,
}

impl PalwEligibilityExamRowV1 {
    /// Positive draw positions: each draw position whose bond attested `Reproduced` counts once (multiplicity kept).
    pub fn positive_positions(&self) -> u32 {
        self.draws
            .iter()
            .filter(|bond| {
                self.attestations.iter().any(|a| &a.verifier == *bond && matches!(a.verdict, PalwEligibilityVerdictV1::Reproduced))
            })
            .count() as u32
    }
}

// =================================================================================================================================
// Keys
// =================================================================================================================================

fn prefixed(prefix: u8, id: &EligDigestV1) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(prefix);
    key.extend_from_slice(id);
    key
}
pub fn eligibility_model_key_v1(class_id: &EligDigestV1) -> Vec<u8> {
    prefixed(ELIGIBILITY_MODEL_PREFIX_V1, class_id)
}
pub fn eligibility_lineage_key_v1(lineage: &EligDigestV1) -> Vec<u8> {
    prefixed(ELIGIBILITY_LINEAGE_PREFIX_V1, lineage)
}
pub fn eligibility_principal_key_v1(principal: &EligDigestV1) -> Vec<u8> {
    prefixed(ELIGIBILITY_PRINCIPAL_PREFIX_V1, principal)
}
pub fn eligibility_exam_key_v1(exam: &EligDigestV1) -> Vec<u8> {
    prefixed(ELIGIBILITY_EXAM_PREFIX_V1, exam)
}
pub fn eligibility_network_key_v1() -> Vec<u8> {
    prefixed(ELIGIBILITY_NETWORK_PREFIX_V1, &[0u8; 64])
}
pub fn eligibility_probation_claim_key_v1(claim: &EligDigestV1) -> Vec<u8> {
    prefixed(ELIGIBILITY_PROBATION_CLAIM_PREFIX_V1, claim)
}

// =================================================================================================================================
// The fence and the effective stage (A-2)
// =================================================================================================================================

/// The fence's mirror on the V2 bundle's state params: its height and policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwModelEligibilityMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwModelEligibilityPolicyV1,
}

impl PalwModelEligibilityMirrorV1 {
    pub fn governs(&self, daa: u64) -> bool {
        daa >= self.from_daa
    }
}

/// **What the class looks like to the gate** — the facts every caller already holds about the class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEligibilityClassFactsV1 {
    /// `registrant_bond.is_none()`: a class the genesis assembly registered.
    pub genesis: bool,
    pub registered_daa: u64,
    /// The ADR-0135 registry lifecycle admits claims for the class ([`palw_eligibility_registry_admits_v1`]).
    pub registry_admits: bool,
}

/// **The stage in force for a class at `daa`**:
///
/// * no mirror, or `daa` below the fence → `None` (the old rule: the caller grants what it granted before);
/// * a genesis class → `None` (outside RFC-0017; user decision U-6 in the RFC);
/// * a row → the row's stage, with an `Eligible` row past `next_renewal_daa + lapse_grace_daa` read as `Lapsed` (the sweep writes it;
///   the read never waits for the sweep);
/// * no row and registered below the fence → grandfathered `Eligible` until `from_daa + renewal_epoch_daa + lapse_grace_daa`, then
///   `Lapsed` (one pass — here, the pre-fence history — is never permanent; user decision U-5);
/// * no row and registered at/after the fence → `Probation` once the registry admits claims (the fold writes the row at the first
///   probation claim it admits), `Registered` before; a missing row is never read as eligible.
pub fn palw_eligibility_effective_stage_v1(
    mirror: Option<&PalwModelEligibilityMirrorV1>,
    class: &PalwEligibilityClassFactsV1,
    row: Option<&PalwEligibilityModelRowV1>,
    daa: u64,
) -> Option<PalwEligibilityStageV1> {
    let mirror = mirror.filter(|m| m.governs(daa))?;
    if class.genesis {
        return None;
    }
    let p = &mirror.policy;
    Some(match row {
        Some(row) => match row.stage {
            PalwEligibilityStageV1::Eligible if daa > row.next_renewal_daa.saturating_add(p.lapse_grace_daa) => {
                PalwEligibilityStageV1::Lapsed
            }
            stage => stage,
        },
        None if class.registered_daa < mirror.from_daa => {
            if daa > mirror.from_daa.saturating_add(p.renewal_epoch_daa).saturating_add(p.lapse_grace_daa) {
                PalwEligibilityStageV1::Lapsed
            } else {
                PalwEligibilityStageV1::Eligible
            }
        }
        None if class.registry_admits => PalwEligibilityStageV1::Probation,
        None => PalwEligibilityStageV1::Registered,
    })
}

/// **The one gate every path reads** (RFC-0017 §6): the rights of the class for a claim accepted at `accepted_daa`. Keyed on the
/// CLAIM's acceptance DAA, so a claim accepted below the fence, or while the class was eligible, keeps the rights it was accepted
/// with (no retroactivity); the caller passes the class's row as of the acceptance (the fold reads it at acceptance and pins
/// the result into the claim's reservation — the same pattern ADR-0176's budget uses).
pub fn palw_eligibility_rights_v1(
    mirror: Option<&PalwModelEligibilityMirrorV1>,
    class: &PalwEligibilityClassFactsV1,
    row: Option<&PalwEligibilityModelRowV1>,
    accepted_daa: u64,
) -> PalwEligibilityRightsV1 {
    match (mirror, palw_eligibility_effective_stage_v1(mirror, class, row, accepted_daa)) {
        (Some(mirror), Some(stage)) => palw_eligibility_rights_of_stage_v1(stage, &mirror.policy),
        _ => PalwEligibilityRightsV1::OLD_RULE,
    }
}

// =================================================================================================================================
// B. Probation budgets
// =================================================================================================================================

/// The probation epoch of `daa`.
pub fn palw_probation_epoch_v1(policy: &PalwModelEligibilityPolicyV1, daa: u64) -> u64 {
    daa / policy.probation_epoch_daa.max(1)
}

/// **An epoch's PALW block opportunity, in the bond budget's block units** (`PALW_BUDGET_BLOCK_UNIT_V1` = one block): one block per
/// DAA of the epoch — the upper bound, a named derivation and not a measured PALW share. Probation influence is counted in the
/// same unit as ADR-0176's `B` (a claim's block-unit ask), so the two minimums compose.
pub fn palw_probation_epoch_block_units_v1(policy: &PalwModelEligibilityPolicyV1) -> u64 {
    policy.probation_epoch_daa.saturating_mul(crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1)
}

/// The network's and one lineage's (per-stage model cap) probation budget for one epoch.
pub fn palw_probation_budgets_v1(policy: &PalwModelEligibilityPolicyV1) -> (u64, u64) {
    let units = palw_probation_epoch_block_units_v1(policy) as u128;
    let of = |permille: u16| (units * (permille as u128) / 1_000) as u64;
    (of(policy.probation_network_permille), of(policy.probation_model_permille))
}

/// **REQ-ELIG-2: the grant** — `min(request, principal_remaining, model_remaining, network_remaining)`. `principal_remaining` is the
/// ADR-0176 common cap's remaining `B` of the claim's bond (enforced by `palw_bond_budget_v1`'s own reservation, which this module
/// does not edit; callers that cannot read it pass `u64::MAX` and rely on that reservation refusing); the other two are this
/// module's. Never more than any of the four.
pub fn palw_probation_grant_v1(request: u64, principal_remaining: u64, model_remaining: u64, network_remaining: u64) -> u64 {
    request.min(principal_remaining).min(model_remaining).min(network_remaining)
}

/// The lineage's (per-stage model cap) and the network's remaining probation units at `daa`. An epoch roll resets the in-epoch
/// counters only; lifetime totals never reset.
pub fn palw_probation_remaining_v1(
    policy: &PalwModelEligibilityPolicyV1,
    lineage: &PalwEligibilityLineageRowV1,
    network: &PalwEligibilityNetworkRowV1,
    daa: u64,
) -> (u64, u64) {
    let epoch = palw_probation_epoch_v1(policy, daa);
    let (network_budget, model_budget) = palw_probation_budgets_v1(policy);
    let lineage_used = if lineage.probation_epoch == epoch { lineage.probation_units_used } else { 0 };
    let network_used = if network.probation_epoch == epoch { network.probation_units_used } else { 0 };
    (model_budget.saturating_sub(lineage_used), network_budget.saturating_sub(network_used))
}

/// **Admit a probation claim's `request` units or refuse it whole** (a claim's block right is not divisible: a partial grant is a
/// refusal, so the budget never pays a fraction the coinbase cannot carry). `Ok(units)` = granted in full.
pub fn palw_probation_admit_v1(
    policy: &PalwModelEligibilityPolicyV1,
    lineage: &PalwEligibilityLineageRowV1,
    network: &PalwEligibilityNetworkRowV1,
    daa: u64,
    request: u64,
) -> Result<u64, (u64, u64)> {
    let (model_left, network_left) = palw_probation_remaining_v1(policy, lineage, network, daa);
    let granted = palw_probation_grant_v1(request, u64::MAX, model_left, network_left);
    if granted < request { Err((model_left, network_left)) } else { Ok(granted) }
}

/// Charge `units` (already granted) to the model, its lineage, its principal and the network, rolling epochs.
pub fn palw_probation_charge_v1(
    policy: &PalwModelEligibilityPolicyV1,
    daa: u64,
    units: u64,
    model: &mut PalwEligibilityModelRowV1,
    lineage: &mut PalwEligibilityLineageRowV1,
    principal: &mut PalwEligibilityPrincipalRowV1,
    network: &mut PalwEligibilityNetworkRowV1,
) {
    let epoch = palw_probation_epoch_v1(policy, daa);
    if lineage.probation_epoch != epoch {
        lineage.probation_epoch = epoch;
        lineage.probation_units_used = 0;
    }
    if network.probation_epoch != epoch {
        network.probation_epoch = epoch;
        network.probation_units_used = 0;
    }
    lineage.probation_units_used = lineage.probation_units_used.saturating_add(units);
    network.probation_units_used = network.probation_units_used.saturating_add(units);
    lineage.probation_units_total = lineage.probation_units_total.saturating_add(units);
    model.probation_units_total = model.probation_units_total.saturating_add(units);
    principal.probation_units_total = principal.probation_units_total.saturating_add(units);
}

// =================================================================================================================================
// C. Attempts, commit-then-draw, verdict
// =================================================================================================================================

/// Why a commit is refused (each a deterministic refusal; a refusal writes nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEligibilityCommitRefusalV1 {
    WrongStageForPurpose,
    ProbationTooShort,
    TooFewClaims,
    ExamAlreadyOpen,
    AttemptIndexMismatch,
    LineageAttemptsExhausted,
    PrincipalAttemptsExhausted,
    NetworkAttemptsExhausted,
    Cooldown,
    OutsideRenewalWindow,
}

/// **REQ-ELIG-4: may this commit consume an attempt now?** Pure; the fold calls it and then [`palw_eligibility_count_attempt_v1`].
#[allow(clippy::too_many_arguments)]
pub fn palw_eligibility_commit_admits_v1(
    policy: &PalwModelEligibilityPolicyV1,
    commit: &PalwEligibilityExamCommitV1,
    model: &PalwEligibilityModelRowV1,
    effective_stage: PalwEligibilityStageV1,
    lineage: &PalwEligibilityLineageRowV1,
    principal: &PalwEligibilityPrincipalRowV1,
    network: &PalwEligibilityNetworkRowV1,
    daa: u64,
) -> Result<(), PalwEligibilityCommitRefusalV1> {
    use PalwEligibilityCommitRefusalV1 as R;
    match (commit.purpose, effective_stage) {
        (PalwEligibilityExamPurposeV1::Promotion, PalwEligibilityStageV1::Probation) => {
            if daa < model.stage_since_daa.saturating_add(policy.probation_min_daa) {
                return Err(R::ProbationTooShort);
            }
        }
        (PalwEligibilityExamPurposeV1::Renewal, PalwEligibilityStageV1::Eligible) => {
            if daa.saturating_add(policy.renewal_window_daa) < model.next_renewal_daa {
                return Err(R::OutsideRenewalWindow);
            }
        }
        (PalwEligibilityExamPurposeV1::Renewal, PalwEligibilityStageV1::Lapsed) => {}
        _ => return Err(R::WrongStageForPurpose),
    }
    // A promotion examines the probation's own claims (at least the committed minimum); a renewal any claims of the model (at least
    // as many as one exam examines — a lapsed model has no new reward-bearing claims to offer).
    let min_claims = match commit.purpose {
        PalwEligibilityExamPurposeV1::Promotion => policy.probation_min_claims,
        PalwEligibilityExamPurposeV1::Renewal => policy.exam_claims as u32,
    };
    if commit.claims_count < min_claims {
        return Err(R::TooFewClaims);
    }
    if model.open_exam.is_some() {
        return Err(R::ExamAlreadyOpen);
    }
    if commit.attempt_index != lineage.attempts_total {
        return Err(R::AttemptIndexMismatch);
    }
    if lineage.attempts_total >= policy.attempts_per_lineage_max {
        return Err(R::LineageAttemptsExhausted);
    }
    if lineage.attempts_total > 0 && daa < lineage.last_commit_daa.saturating_add(policy.attempt_cooldown_daa) {
        return Err(R::Cooldown);
    }
    let epoch = daa / policy.attempt_epoch_daa.max(1);
    let principal_in_epoch = if principal.attempt_epoch == epoch { principal.attempts_in_epoch } else { 0 };
    if principal_in_epoch >= policy.attempts_per_principal_per_epoch {
        return Err(R::PrincipalAttemptsExhausted);
    }
    let network_in_epoch = if network.attempt_epoch == epoch { network.attempts_in_epoch } else { 0 };
    if network_in_epoch >= policy.attempts_network_per_epoch {
        return Err(R::NetworkAttemptsExhausted);
    }
    Ok(())
}

/// **GAP-E3: may `free` collateral pay the attempt fee?** The fee is due whole; a bond that cannot pay it commits nothing.
pub fn palw_eligibility_fee_affordable_v1(free_collateral: u128, fee: u64) -> bool {
    free_collateral >= fee as u128
}

/// **What filling the network's attempt budget costs** over `epochs` attempt epochs: `attempts_network_per_epoch × fee × epochs`,
/// burned — the griefing price RFC-0017 §5 reports beside the honest models' waiting time.
pub fn palw_eligibility_griefing_cost_v1(policy: &PalwModelEligibilityPolicyV1, epochs: u64) -> u128 {
    (policy.attempts_network_per_epoch as u128) * (policy.attempt_fee_sompi as u128) * (epochs as u128)
}

/// Record a burned attempt fee on the lineage, the principal and the network (the burn itself is the fold's bond write).
pub fn palw_eligibility_record_fee_v1(
    fee: u64,
    lineage: &mut PalwEligibilityLineageRowV1,
    principal: &mut PalwEligibilityPrincipalRowV1,
    network: &mut PalwEligibilityNetworkRowV1,
) {
    lineage.fees_burned_sompi = lineage.fees_burned_sompi.saturating_add(fee as u128);
    principal.fees_burned_sompi = principal.fees_burned_sompi.saturating_add(fee as u128);
    network.fees_burned_sompi = network.fees_burned_sompi.saturating_add(fee as u128);
}

/// Count one attempt at commit time (never refunded: a withdrawal after seeing the draw is a spent attempt).
pub fn palw_eligibility_count_attempt_v1(
    policy: &PalwModelEligibilityPolicyV1,
    daa: u64,
    lineage: &mut PalwEligibilityLineageRowV1,
    principal: &mut PalwEligibilityPrincipalRowV1,
    network: &mut PalwEligibilityNetworkRowV1,
) {
    let epoch = daa / policy.attempt_epoch_daa.max(1);
    lineage.attempts_total = lineage.attempts_total.saturating_add(1);
    lineage.last_commit_daa = daa;
    if principal.attempt_epoch != epoch {
        principal.attempt_epoch = epoch;
        principal.attempts_in_epoch = 0;
    }
    principal.attempts_in_epoch = principal.attempts_in_epoch.saturating_add(1);
    principal.attempts_total = principal.attempts_total.saturating_add(1);
    if network.attempt_epoch != epoch {
        network.attempt_epoch = epoch;
        network.attempts_in_epoch = 0;
    }
    network.attempts_in_epoch = network.attempts_in_epoch.saturating_add(1);
    network.attempts_total = network.attempts_total.saturating_add(1);
}

/// **Whether a seed anchor may fix an exam's draw**: its span is at least `exam_anchor_delay_spans` after the commit's span, so
/// the anchor's execution did not exist when the model, the claim set and the stake cut were committed. The anchor is ADR-0130's
/// execution-lane `round_seed_anchor` (`PalwExecSeedAnchorV1`), the randomness ADR-0147's admission jury already uses
/// (`palw_admission_jury_seed_v2`): re-rolling it costs a winning inference, not a header.
pub fn palw_eligibility_anchor_admissible_v1(policy: &PalwModelEligibilityPolicyV1, committed_span: u64, anchor_span: u64) -> bool {
    anchor_span >= committed_span.saturating_add(policy.exam_anchor_delay_spans.max(1))
}

/// `H(domain ‖ exam id ‖ anchor span ‖ anchor execution key)`.
pub fn palw_eligibility_exam_seed_v1(exam: &EligDigestV1, anchor_span: u64, anchor_execution_key: &EligDigestV1) -> EligDigestV1 {
    hash64(PALW_ELIGIBILITY_EXAM_SEED_DOMAIN_V1, &[exam, &anchor_span.to_le_bytes(), anchor_execution_key])
}

/// One member of an exam's verifier population: its kernel bond id and its stake (net collateral).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEligibilityVerifierV1 {
    pub bond: EligDigestV1,
    pub operator: EligDigestV1,
    pub stake: u64,
}

/// **The population predicate** (what the fold filters the bond registry with): Active, registered at least
/// `verifier_maturity_daa` before the commit (no bond created after the commit can be drawn), at least the policy's collateral, and
/// not the committing principal's operator. Excluding the committer is cosmetic against Sybil keys — the analysis (RFC-0017 §5)
/// counts every attacker bond in `f` regardless.
pub fn palw_eligibility_population_admits_v1(
    policy: &PalwModelEligibilityPolicyV1,
    committed_daa: u64,
    committer_principal: &EligDigestV1,
    active: bool,
    registered_daa: u64,
    collateral: u64,
    operator: &EligDigestV1,
) -> bool {
    active
        && registered_daa.saturating_add(policy.verifier_maturity_daa) <= committed_daa
        && collateral >= policy.verifier_min_collateral_sompi
        && collateral > 0
        && operator != committer_principal
}

/// **`m` independent stake-proportional draws with replacement** over `population`, through the R7-ECON gate's draw
/// ([`crate::palw_econ_contracts_v1::palw_model_verification_draw_v1`]: `u = H(domain ‖ beacon ‖ tally ‖ i) mod total`, the bond
/// whose cumulative-stake interval holds `u`) with this exam's seed as the beacon and its id as the tally. The population is made
/// canonical here (ascending distinct bond ids, positive stakes), so the caller's order never matters. Splitting a stake over many
/// bonds neither raises nor lowers its expected positions (Sybil-neutral). `None` when the population is empty or has zero stake.
pub fn palw_eligibility_draw_v1(
    seed: &EligDigestV1,
    exam: &EligDigestV1,
    population: &[PalwEligibilityVerifierV1],
    m: u16,
) -> Option<Vec<EligDigestV1>> {
    let mut pool: Vec<(EligDigestV1, u64)> = population.iter().filter(|v| v.stake > 0).map(|v| (v.bond, v.stake)).collect();
    pool.sort_by(|a, b| a.0.cmp(&b.0));
    pool.dedup_by(|a, b| a.0 == b.0);
    crate::palw_econ_contracts_v1::palw_model_verification_draw_v1(seed, exam, &pool, m).ok()
}

/// **Which committed claims are examined**: `count` distinct indices in `0..claims_count`, from the same seed (so the registrant
/// cannot choose them either). Fewer claims than `count` examines all of them.
pub fn palw_eligibility_examined_claims_v1(seed: &EligDigestV1, claims_count: u32, count: u16) -> Vec<u32> {
    let want = (count as u32).min(claims_count) as usize;
    let mut picked: Vec<u32> = Vec::with_capacity(want);
    let mut i: u32 = 0;
    while picked.len() < want {
        let h = hash64(PALW_ELIGIBILITY_CLAIM_PICK_DOMAIN_V1, &[seed, &i.to_le_bytes()]);
        let mut word = [0u8; 8];
        word.copy_from_slice(&h[..8]);
        let index = (u64::from_le_bytes(word) % claims_count as u64) as u32;
        if !picked.contains(&index) {
            picked.push(index);
        }
        i = i.wrapping_add(1);
    }
    picked.sort_unstable();
    picked
}

/// **The verdict at `daa`**: `Passed` as soon as `k` draw positions attested `Reproduced`; at the deadline, `NotPromoted`
/// otherwise; `None` while open. `Unavailable`, `NotReproducible` and silence all mean "promotion conditions not met"; none of them
/// is a fraud verdict and none slashes (per-claim findings travel separately).
pub fn palw_eligibility_exam_verdict_v1(
    policy: &PalwModelEligibilityPolicyV1,
    exam: &PalwEligibilityExamRowV1,
    daa: u64,
) -> Option<PalwEligibilityExamOutcomeV1> {
    if exam.outcome.is_some() || exam.anchor.is_none() {
        return None;
    }
    if exam.positive_positions() >= policy.exam_k as u32 {
        return Some(PalwEligibilityExamOutcomeV1::Passed);
    }
    if daa > exam.deadline_daa {
        return Some(PalwEligibilityExamOutcomeV1::NotPromoted);
    }
    None
}

/// **Apply an exam's outcome to its model** (REQ-ELIG-3/5): a passed promotion or renewal makes the model `Eligible` until
/// `daa + renewal_epoch_daa` (a renewal passed early extends from the later of `daa` and the current `next_renewal_daa`, never
/// beyond one epoch past it); anything else leaves the stage as the sweep finds it.
pub fn palw_eligibility_apply_outcome_v1(
    policy: &PalwModelEligibilityPolicyV1,
    model: &mut PalwEligibilityModelRowV1,
    purpose: PalwEligibilityExamPurposeV1,
    outcome: PalwEligibilityExamOutcomeV1,
    daa: u64,
) {
    model.open_exam = None;
    if outcome != PalwEligibilityExamOutcomeV1::Passed {
        return;
    }
    let base = match purpose {
        PalwEligibilityExamPurposeV1::Renewal if model.stage == PalwEligibilityStageV1::Eligible => model.next_renewal_daa.max(daa),
        _ => daa,
    };
    if model.stage != PalwEligibilityStageV1::Eligible {
        model.stage = PalwEligibilityStageV1::Eligible;
        model.stage_since_daa = daa;
    }
    model.next_renewal_daa = base.saturating_add(policy.renewal_epoch_daa);
    model.exams_passed = model.exams_passed.saturating_add(1);
}

/// **D. Lapse** (the sweep): an `Eligible` row past `next_renewal_daa + lapse_grace_daa` with no passed renewal becomes `Lapsed`.
/// Returns whether it changed. Existing claims' liabilities are untouched (they live in the claim and budget rows).
pub fn palw_eligibility_lapse_v1(policy: &PalwModelEligibilityPolicyV1, model: &mut PalwEligibilityModelRowV1, daa: u64) -> bool {
    if model.stage == PalwEligibilityStageV1::Eligible && daa > model.next_renewal_daa.saturating_add(policy.lapse_grace_daa) {
        model.stage = PalwEligibilityStageV1::Lapsed;
        model.stage_since_daa = daa;
        return true;
    }
    false
}

/// **A. → B.** — reusing the ADR-0135/ADR-0147 registry: a class leaves `Registered` for `Probation` when its registry lifecycle
/// admits claims (`PalwModelLifecycleV1::admits_claims`: the network-drawn admission jury seated it past `Candidate`, and ready
/// seats beyond its registrant hold it). Registration alone, or the registrant's own Panel being Ready, never does.
pub fn palw_eligibility_registry_admits_v1(lifecycle: Option<crate::palw_model_registry_v1::PalwModelLifecycleV1>) -> bool {
    lifecycle.is_some_and(|state| state.admits_claims())
}

/// A fresh `Probation` row, written by the fold at the first probation claim it admits for the class.
pub fn palw_eligibility_new_probation_row_v1(
    class_id: EligDigestV1,
    lineage: EligDigestV1,
    principal: EligDigestV1,
    registered_daa: u64,
    daa: u64,
) -> PalwEligibilityModelRowV1 {
    PalwEligibilityModelRowV1 {
        class_id,
        lineage,
        principal,
        stage: PalwEligibilityStageV1::Probation,
        registered_daa,
        stage_since_daa: daa,
        probation_units_total: 0,
        next_renewal_daa: 0,
        open_exam: None,
        exams_passed: 0,
    }
}

/// The row a pre-fence (grandfathered) class is read as, written when it first commits a renewal.
pub fn palw_eligibility_grandfathered_row_v1(
    mirror: &PalwModelEligibilityMirrorV1,
    class_id: EligDigestV1,
    lineage: EligDigestV1,
    principal: EligDigestV1,
    registered_daa: u64,
) -> PalwEligibilityModelRowV1 {
    PalwEligibilityModelRowV1 {
        class_id,
        lineage,
        principal,
        stage: PalwEligibilityStageV1::Eligible,
        registered_daa,
        stage_since_daa: mirror.from_daa,
        probation_units_total: 0,
        next_renewal_daa: mirror.from_daa.saturating_add(mirror.policy.renewal_epoch_daa),
        open_exam: None,
        exams_passed: 0,
    }
}

/// The status a reader shows: `(stage name, next renewal DAA or 0, subsidy, block right, final weight)`.
pub fn palw_eligibility_status_v1(
    mirror: Option<&PalwModelEligibilityMirrorV1>,
    class: &PalwEligibilityClassFactsV1,
    row: Option<&PalwEligibilityModelRowV1>,
    daa: u64,
) -> (&'static str, u64, bool, bool, bool) {
    let rights = palw_eligibility_rights_v1(mirror, class, row, daa);
    match palw_eligibility_effective_stage_v1(mirror, class, row, daa) {
        None => ("not-governed", 0, rights.subsidy, rights.block_right, rights.final_weight),
        Some(stage) => {
            let next = match (row, stage) {
                (Some(row), _) => row.next_renewal_daa,
                (None, PalwEligibilityStageV1::Eligible) => {
                    mirror.map(|m| m.from_daa.saturating_add(m.policy.renewal_epoch_daa)).unwrap_or(0)
                }
                _ => 0,
            };
            (stage.name(), next, rights.subsidy, rights.block_right, rights.final_weight)
        }
    }
}

/// **The model-allocation input a stage admits** (RFC-0017 §6, the coinbase leg): the capital the ADR-0177 V3 allocation
/// (`palw_allocation_snapshot_v3` / `roll_epoch_v3` in `palw_bond_budget_v1`, owned by the ADR-0176/0177 lane and NOT edited here)
/// should count for a model in its denominator and its numerator. `Registered` / `Lapsed` count 0 (no allocation, and no dilution of other models by parked capital); `Probation` counts
/// its capital but its payment is bounded by the probation grant; `Eligible` and ungoverned models count all of it. The allocation's
/// call site must pass every model's snapshot `S_m(k)` through this function before the `α` curve — the hook this module offers;
/// wiring it into `roll_epoch_v3` (`palw_bond_budget_fold_v1.rs`) is the coordination item RFC-0017 §9 GAP-E1 records.
pub fn palw_eligibility_allocation_capital_v1(stage: Option<PalwEligibilityStageV1>, capital: u128) -> u128 {
    match stage {
        Some(PalwEligibilityStageV1::Registered) | Some(PalwEligibilityStageV1::Lapsed) => 0,
        _ => capital,
    }
}

/// **What the registry RPC shows of a class's eligibility** (`GetPalwModelRegistry`, appended row fields, wire version 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEligibilityStatusReadV1 {
    /// `not-governed` (below the fence / genesis), `registered`, `probation`, `eligible`, `lapsed`.
    pub stage: String,
    /// The DAA the current eligibility lapses at without a passed renewal (before the grace); 0 = none.
    pub next_renewal_daa: u64,
    pub subsidy: bool,
    pub block_right: bool,
    pub final_weight: bool,
    pub exams_passed: u32,
    /// Exam attempts the class's lineage (artifact root) has consumed, all registrations of the same weights together.
    pub lineage_attempts: u32,
}

impl Default for PalwEligibilityStatusReadV1 {
    fn default() -> Self {
        Self {
            stage: "not-governed".to_string(),
            next_renewal_daa: 0,
            subsidy: true,
            block_right: true,
            final_weight: true,
            exams_passed: 0,
            lineage_attempts: 0,
        }
    }
}

// =================================================================================================================================
// Analysis helpers (never read by the fold): the gate math of RFC-0017 §5
// =================================================================================================================================

/// `P_pass = Σ_{i=k..m} C(m,i) f^i (1−f)^(m−i)` — the probability that an attacker holding stake fraction `f` of the DRAWN
/// population fills `k` of `m` independent stake-proportional draws (and so can pass without any honest check). Analysis only:
/// it is not computation soundness, not a conviction probability, and it ignores randomness bias, examiner attacks,
/// approve-without-checking and missed sparse fraud (RFC-0017 §5).
pub fn palw_eligibility_pass_probability_f64(f: f64, m: u32, k: u32) -> f64 {
    let mut total = 0.0;
    for i in k..=m {
        total += binomial(m, i) * f.powi(i as i32) * (1.0 - f).powi((m - i) as i32);
    }
    total
}

/// `1 − (1 − p)^A`: at least one pass in `A` independent attempts.
pub fn palw_eligibility_any_pass_f64(p_pass: f64, attempts: u64) -> f64 {
    1.0 - (1.0 - p_pass).powf(attempts as f64)
}

/// The α allocation share of a concentrated attacker (RFC-0017 §2): `a^α / (a^α + h^α / n^(α−1))` with honest capital `h` spread
/// evenly over `n` models.
pub fn palw_eligibility_alpha_share_f64(attacker: f64, honest: f64, n_models: u32, alpha: f64) -> f64 {
    let a = attacker.powf(alpha);
    let h = (n_models as f64) * (honest / n_models as f64).powf(alpha);
    a / (a + h)
}

fn binomial(n: u32, k: u32) -> f64 {
    let mut r = 1.0;
    for j in 0..k {
        r = r * (n - j) as f64 / (j + 1) as f64;
    }
    r
}

fn hash64(domain: &[u8], parts: &[&[u8]]) -> EligDigestV1 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        state.update(&(part.len() as u64).to_le_bytes());
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// The message a drawn verifier signs (ML-DSA-87 under [`PALW_ELIGIBILITY_ATTESTATION_MLDSA87_CONTEXT_V1`] with its bond key).
pub fn palw_eligibility_attestation_message_v1(attestation: &PalwEligibilityAttestationV1) -> EligDigestV1 {
    hash64(PALW_ELIGIBILITY_ATTESTATION_DOMAIN_V1, &[&borsh::to_vec(attestation).expect("an attestation serializes")])
}

// =================================================================================================================================
// Params: the fence
// =================================================================================================================================

impl Params {
    /// `palw_model_eligibility_v1`'s height, `Some` only for a real one (`never()` is `None`).
    pub fn palw_model_eligibility_v1_from_daa(&self) -> Option<u64> {
        self.palw_model_eligibility_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// Whether the fence is in force at `daa`.
    pub fn palw_model_eligibility_v1_active_at(&self, daa: u64) -> bool {
        self.palw_model_eligibility_v1_from_daa().is_some_and(|from| daa >= from)
    }

    /// **The mirror** on the V2 bundle's state params (`model_eligibility_v1`), which the fold, the gates and the RPC read. Written
    /// here and nothing else; `None` where the fence is not armed (or is `never()`). The policy a mirror carries is the RFC's
    /// unapproved design at the bundle's day length: an armed fence is refused by validation, so it is never read on a network.
    pub fn sync_palw_model_eligibility_v1(&mut self) {
        let from = self.palw_model_eligibility_v1_from_daa();
        let day_daa = self.palw_model_eligibility_day_daa_v1();
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            let mirror = from.map(|from_daa| PalwModelEligibilityMirrorV1 {
                from_daa,
                policy: PalwModelEligibilityPolicyV1::design_unapproved_v1(day_daa),
            });
            bundle.state = bundle.state.clone().with_model_eligibility_v1(mirror);
        }
    }

    /// The network's DAA per day as this fence reads it: `86_400 / target_time_per_block_secs` at the network's DAA rate. A named
    /// derivation, not a value: no network arms the fence.
    pub fn palw_model_eligibility_day_daa_v1(&self) -> u64 {
        // One DAA score step per block at the network's target block time (milliseconds); at least one.
        (86_400_000u64 / self.target_time_per_block().max(1)).max(1)
    }

    /// **Refused when armed at a real height** until RFC-0017's rule is built, verified and its values approved; `None` and
    /// `Some(never())` pass. The mirror must agree with the fence.
    pub fn validate_palw_model_eligibility_v1(&self) -> Result<(), PalwModeV2Error> {
        if self.palw_model_eligibility_v1_from_daa().is_some() {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_eligibility_v1 cannot be armed: RFC-0017's rule is not built and verified and its values are not approved",
            ));
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.model_eligibility_v1().map(|m| m.from_daa) != self.palw_model_eligibility_v1_from_daa()
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_eligibility_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_model_eligibility_v1",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
