//! **The cross-lane G14 gates** (`docs/audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md` §4.1, the cross-lane lane): the arming rules
//! and the claim / reward gates that decide whether a reward-bearing claim can exist without a G14 public-conviction route.
//!
//! ```text
//! G-10  palw_staged_public_court_v1 / _tir_route_v1 / _float_constraint_v1 / palw_tir_public_conviction_v1
//!       refused when armed until an arm-readiness record (V1–V7) for this network and height is pinned   (T-N17)
//! G-18  palw_staged_public_court_v1 needs palw_legacy_public_filer_v1 in force at or below it               (T-N16)
//! N4    palw_public_evidence_reward_v1 refused when armed until its arm-readiness record is pinned          (T-N9)
//! G-01  palw_g14_route_required_v1 (option (b), user decision 2026-10-16): past it a newly admitted claim of a family with no
//!  (b)  ARMED G14 route is refused at the claim gate — no reward, weight or consensus credit, the carrier's ordinary fee only —
//! G-05  and the job versions no single honest seat can judge (FP selection, constraint, prefix, tokenizer) are refused at
//!       admission until their route is armed. The route is read from the fence state (the mirror), so a family regains reward
//!       the moment its route is armed.                                                                     (T-N8, T-N10)
//! G-08  palw_reward_liability_horizon_v1: a legacy free-prompt claim's Final floor reaches its liability horizon
//! G-15  (`licence + window_challenge + window_court`), and a Gate C certified reward is named only after `final + window_court` or at
//!       the claim's retirement, whichever is first (nothing can convict a retired claim)                    (T-N15)
//! ```
//!
//! **No measured value is invented here.** Every arming record list is EMPTY: arming any gated fence on a real network is refused until a
//! reviewed change pins a record whose evidence digests are the named tests and drills. The G14 fences of option (b), N4 and the
//! liability horizon only REMOVE or DELAY reward, so their records name tests and drills, never an economic measurement.
//!
//! **G14 from genesis** (user decision 2026-10-16): [`palw_g14_genesis_test_params_v1`] is a test-only ruleset whose genesis is an
//! isolated derivation ([`palw_isolated_g14_genesis_v1`], a marker no shipped genesis carries) with every G14 fence at DAA 0. Only
//! such a genesis skips the record pins ([`Params::has_isolated_g14_genesis_v1`]); no shipped preset can satisfy it.
//!
//! **A-2.** No new object kind, wire form or state row. Every claim rule keys on the CLAIM's acceptance DAA, every mirror is
//! `#[borsh(skip)]` and `None` on every shipped preset, so below each fence — on every live state — every verdict, write and root is the
//! live build's. Nothing here reads pre-fence history: at fence 0 "a claim admitted before the fence" is an empty set.
//!
//! No cryptographic computation proof of any kind is used (PRINCIPLES §8): the gates read fence state and the claim's own public job.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PalwFpJobTailV1, PalwFreePromptCommitmentV3, PalwFreePromptJobV3};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwStateParamsV2};

/// ADR-0190: the gather / model-parameter operand bound of the bounded-round door is derived. `false` until the closure-bound lane lands; 
/// while `false` the bounded door cannot be armed on any network.
pub const PALW_TIR_BOUNDED_DOOR_GATHER_BOUND_DERIVED_V1: bool = false;

// =================================================================================================================================
// 1. The arm-readiness records (G-10, N4, option (b), the liability horizon)
// =================================================================================================================================

/// **The fences this lane gates on a pinned arm-readiness record.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwG14GatedFenceV1 {
    StagedPublicCourtV1,
    StagedTirRouteV1,
    StagedFloatConstraintV1,
    TirPublicConvictionV1,
    PublicEvidenceRewardV1,
    G14RouteRequiredV1,
    RewardLiabilityHorizonV1,
    /// LG14-A `palw_legacy_public_filer_v1` (lane g14-close-legacy, 2026-10-16): its unconditional refusal is replaced by this gate.
    LegacyPublicFilerV1,
    /// LG14-B `palw_legacy_held_da_v2` (lane g14-close-legacy, 2026-10-16): likewise, and it needs LG14-A at or below it.
    LegacyHeldDaV2,
    /// `palw_court_capacity_v1` (lane g14-court-capacity, audit G-07): the court budget, time price and deadline-aware queue.
    CourtCapacityV1,
    /// `palw_court_model_scope_v1` (register D2, ruling 2026-10-12; lane d2-x3-finish, 2026-10-18): its unconditional refusal is replaced by
    /// this gate. Its requirements are tests and drills (no economic value is measured).
    CourtModelScopeV1,
    /// `palw_claim_hard_deadline_v1` (register X3, ruling 2026-10-12; lane d2-x3-finish, 2026-10-18): its unconditional refusal is replaced
    /// by this gate, and its policy terms are measurements ([`Self::needs_measured_value`]).
    ClaimHardDeadlineV1,
}

/// **D2's arm-readiness requirements** (register §3.1.1, ADR-0177 D2): the replacement routes' own tests, the node-level full-collusion
/// conviction past the fence, and the fence-crossing drill. No economic value is measured.
pub const PALW_D2_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "T-D2 fold: palw_court_model_scope_routes_v1 suite; a D2 claim's restricted units refused, the replacement routes admitted, old claims unaffected",
    "node E2E: past the fence a new claim's full collusion (producer and every seat, outside bond) is convicted by the replacement route; the block stands",
    "drill: the shipped binary crosses the fence; the four model-byte routes are closed for new claims only and every older claim settles",
];

/// **X3's arm-readiness requirements** (register §3.1, REQ-X3-1..6): the fold tests, the MEASURED policy terms (every `B_*` term with no
/// existing source and `c_retain`; 0 is refused), the node-level flow, and the fence-crossing drill.
pub const PALW_X3_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, regressions and boundary decode (palw_ruling_fences_v1 X3 suite, REQ-X3-1..6)",
    "measured: B_cold_fetch, B_check, B_localize, B_carrier, B_reorg_slack and c_retain on the slowest supported node and the real retention \
     price, as the approved policy (none of them 0)",
    "node E2E: acceptance fixes the deadline, effective = min(proposed, hard), no extension or regrant for a new claim, a pursuit starts only \
     inside its budget, and reaching the deadline alone never Finals an unresolved claim",
    "drill: the shipped binary crosses the fence; restart and reorg replay identical across it; claims accepted before it keep the old rules",
];

/// The G14 validation steps V1–V7 (`g14-objective-conviction-remaining-2026-10-15.md` §5), one evidence digest each.
pub const PALW_G14_VALIDATION_STEPS_V1: &[&str] = &[
    "V1 build, existing regressions and boundary decode of the change set",
    "V2 one correct seat, every other seat false: native conviction, real slash, block / reward refusal",
    "V3 producer + every Panel seat colluding: one outside bond, its own model, public RPC / authenticated material only",
    "V4 every violation family and its honest twin",
    "V5 maximum profile, resources and maximum legal delay under capacity contention",
    "V6 real network and economic accounting (node / mempool / block acceptance)",
    "V7 restart / IBD / pruned / reorg at varied interruption points",
];

/// N4's arm-readiness requirements (it removes reward; no economic value is measured).
pub const PALW_N4_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "T-N9 fold: n4_v5_image_and_panel_da_claims_take_no_new_reward",
    "T-N9 node E2E: the shipped binary's mempool, template and fold drop the V5 / PanelDa commitment and the block stands",
    "D-17 drill: the shipped binary crosses the fence; no V5 / PanelDa claim admitted past it, every older claim settles",
];

/// Option (b)'s arm-readiness requirements (user decision 2026-10-16: its own test plus node E2E; it removes reward, so no economic
/// value is measured) and the fence-crossing drill the release plan runs with the shipped binary.
pub const PALW_G14_ROUTE_REQUIRED_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "T-N8 fold: legacy_route_claim_without_a_g14_path_earns_nothing_past_14000",
    "T-N10 fold: legacy_fp_selection_and_constraint_jobs_are_refused_until_their_route_is_armed",
    "node E2E: past the fence the producer pre-check and the template refuse a route-less attempt and commitment; the block stands",
    "drill: the shipped binary crosses the fence; claims admitted before it settle under the old rules",
];

/// The liability horizon's arm-readiness requirements (it delays a payout; no economic value is measured).
pub const PALW_REWARD_LIABILITY_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "T-N15 fold: certified_and_final_rewards_are_held_until_the_liability_horizon",
    "node E2E: a legacy FP claim accepted past the fence spends no receipt before its liability horizon",
    "drill: the shipped binary crosses the fence; Panel capacity under the longer pre-Final hold measured and reported",
];

/// **LG14-A's arm-readiness requirements** (the legacy route's common filer, reservation, Final hold, direct proofs over open
/// sessions, reserved further dissection and the Valid lock held to the liability horizon). It changes WHEN a V2 claim may end and
/// how long a Valid seat's collateral stays locked, so V5 is a measurement: the hard-deadline envelope per class and the Panel
/// capacity under the longer lock. Nothing here is a value this binary pins.
pub const PALW_LEGACY_FILER_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, existing regressions and boundary decode (lg14a_legacy_dispute_fold, g14_close_legacy fold suite)",
    "V2/V3 node E2E legacy_v2_attempt_full_collusion_outside_bond_convicts: producer and every seat Valid on a forged leaf, a bond \
     registered after genesis convicts from public material, producer and every Valid seat slashed, coinbase / weight / safe weight \
     refused, Final held while the pursuit is open",
    "V4 every legacy V2 Attempt violation family and its honest twin (lg14a_* node E2E: before and after Final, dismissal, Sybil \
     reservation, decoy court)",
    "V5 measured: the hard-deadline envelope (start + B_cold_fetch + B_check + B_localize + B_disclose + B_court + B_carrier + \
     B_reorg_slack <= trace_retention - W_disclose) per class at its maximum profile (GAP-07), and Panel capacity under the Valid lock \
     held to final + window_court (G-13)",
    "V6 real network and economic accounting with the shipped binary (node / mempool / block acceptance)",
    "V7 restart / IBD / pruned / reorg mid-pursuit (lg14a_the_pursuit_survives_a_node_restart, g14r_resume)",
    "drill D-02 / D-13 / D-16: the shipped binary crosses the fence; claims admitted before it settle under the old rules",
];

/// **LG14-B's arm-readiness requirements** (the legacy route's public descent, CKW, held units and the leaf recompute).
pub const PALW_LEGACY_HELD_DA_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, regressions and boundary decode (tags 157-159 dropped by name below the fence)",
    "V2/V3 node E2E legacy_v2_attempt_held_da_withholding_ends_in_da_default (withheld held unit: DA default, never a computation \
     fault) and the fused-tile lie convicted by a fresh outsider",
    "V4 every held unit family and its honest twin (lg14b_* suite)",
    "V5 measured: descent rounds, answer bytes and the 34-session reservation budget at each class's maximum profile, and the court \
     session cap under contention (G-07 residual)",
    "V6 real network and economic accounting with the shipped binary",
    "V7 restart / reorg mid-descent (lg14b_a_restarted_outsider_reacquires_answered_units_without_replaying_old_accusations)",
    "drill D-02 / D-11: the shipped binary crosses the fence; a withheld unit defaults on the real network",
];

/// **The court-capacity fence's arm-readiness requirements** (audit G-07; user direction 2026-10-16 §1, §3). Every value of its
/// policy is a measurement: the per-operation work units and the block budget on the slowest supported node, ρ_court, and the drills
/// that show an honest prosecution reaches a verdict under a saturating and a continuously refilling coalition.
pub const PALW_COURT_CAPACITY_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, regressions and boundary decode (g14_court_capacity fold suite; the dispute record's appended court rows)",
    "measured: each court operation class's verification time, evidence bytes and working set on the slowest supported node \
     (palw_court_op_timing_harness and the node E2E's [court-op timing]), with the block allotments, as a PalwCourtMeasurementV1 \
     whose digest is THIS evidence entry and from which the armed policy derives exactly (validated)",
    "measured: rho_court (sompi per occupied DAA per weight unit) and the session weight, with the occupancy economics reported",
    "V3 drill: a coalition holds every court slot; a further honest independent verifier reaches its verdict inside its claim's \
     hard deadline",
    "V3 drill: a coalition refills every slot as it frees; the queued honest prosecution is granted first and convicts in time",
    "V5 drill: maximum load — every block's court operations at the budget; node verification time and memory under the bound",
    "V7 restart / IBD / pruned / reorg mid-queue: identical reservations, deadlines, fees and releases",
];

impl PalwG14GatedFenceV1 {
    pub const ALL: [Self; 12] = [
        Self::StagedPublicCourtV1,
        Self::StagedTirRouteV1,
        Self::StagedFloatConstraintV1,
        Self::TirPublicConvictionV1,
        Self::PublicEvidenceRewardV1,
        Self::G14RouteRequiredV1,
        Self::RewardLiabilityHorizonV1,
        Self::LegacyPublicFilerV1,
        Self::LegacyHeldDaV2,
        Self::CourtCapacityV1,
        Self::CourtModelScopeV1,
        Self::ClaimHardDeadlineV1,
    ];

    /// The `Params` field's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::StagedPublicCourtV1 => "palw_staged_public_court_v1",
            Self::StagedTirRouteV1 => "palw_staged_tir_route_v1",
            Self::StagedFloatConstraintV1 => "palw_staged_float_constraint_v1",
            Self::TirPublicConvictionV1 => "palw_tir_public_conviction_v1",
            Self::PublicEvidenceRewardV1 => "palw_public_evidence_reward_v1",
            Self::G14RouteRequiredV1 => "palw_g14_route_required_v1",
            Self::RewardLiabilityHorizonV1 => "palw_reward_liability_horizon_v1",
            Self::LegacyPublicFilerV1 => "palw_legacy_public_filer_v1",
            Self::LegacyHeldDaV2 => "palw_legacy_held_da_v2",
            Self::CourtCapacityV1 => "palw_court_capacity_v1",
            Self::CourtModelScopeV1 => "palw_court_model_scope_v1",
            Self::ClaimHardDeadlineV1 => "palw_claim_hard_deadline_v1",
        }
    }

    /// The evidence a record must carry, one digest per entry, in this order.
    pub fn requirements(self) -> &'static [&'static str] {
        match self {
            Self::StagedPublicCourtV1 | Self::StagedTirRouteV1 | Self::StagedFloatConstraintV1 | Self::TirPublicConvictionV1 => {
                PALW_G14_VALIDATION_STEPS_V1
            }
            Self::PublicEvidenceRewardV1 => PALW_N4_ARMING_REQUIREMENTS_V1,
            Self::G14RouteRequiredV1 => PALW_G14_ROUTE_REQUIRED_ARMING_REQUIREMENTS_V1,
            Self::RewardLiabilityHorizonV1 => PALW_REWARD_LIABILITY_ARMING_REQUIREMENTS_V1,
            Self::LegacyPublicFilerV1 => PALW_LEGACY_FILER_ARMING_REQUIREMENTS_V1,
            Self::LegacyHeldDaV2 => PALW_LEGACY_HELD_DA_ARMING_REQUIREMENTS_V1,
            Self::CourtCapacityV1 => PALW_COURT_CAPACITY_ARMING_REQUIREMENTS_V1,
            Self::CourtModelScopeV1 => PALW_D2_ARMING_REQUIREMENTS_V1,
            Self::ClaimHardDeadlineV1 => PALW_X3_ARMING_REQUIREMENTS_V1,
        }
    }

    /// **Does arming need a measured value?** The four G-10 fences do (V5's maximum profile, deadlines and resources), and so do
    /// LG14-A / LG14-B (the hard-deadline envelope, the descent's budget, Panel capacity under the longer lock). Option (b), N4 and
    /// the liability horizon only remove or delay reward: their records are tests and drills.
    pub fn needs_measured_value(self) -> bool {
        matches!(
            self,
            Self::StagedPublicCourtV1
                | Self::StagedTirRouteV1
                | Self::StagedFloatConstraintV1
                | Self::TirPublicConvictionV1
                | Self::LegacyPublicFilerV1
                | Self::LegacyHeldDaV2
                | Self::CourtCapacityV1
                | Self::ClaimHardDeadlineV1
        )
    }

    /// The fence's raw height on `p` (`Some(never())` reads as `None`).
    pub fn armed(self, p: &Params) -> Option<ForkActivation> {
        let raw = match self {
            Self::StagedPublicCourtV1 => p.palw_staged_public_court_v1,
            Self::StagedTirRouteV1 => p.palw_staged_tir_route_v1,
            Self::StagedFloatConstraintV1 => p.palw_staged_float_constraint_v1,
            Self::TirPublicConvictionV1 => p.palw_tir_public_conviction_v1,
            Self::PublicEvidenceRewardV1 => p.palw_public_evidence_reward_v1,
            Self::G14RouteRequiredV1 => p.palw_g14_route_required_v1,
            Self::RewardLiabilityHorizonV1 => p.palw_reward_liability_horizon_v1,
            Self::LegacyPublicFilerV1 => p.palw_legacy_public_filer_v1,
            Self::LegacyHeldDaV2 => p.palw_legacy_held_da_v2,
            Self::CourtCapacityV1 => p.palw_court_capacity_v1.map(|f| f.activation),
            Self::CourtModelScopeV1 => p.palw_court_model_scope_v1,
            Self::ClaimHardDeadlineV1 => p.palw_claim_hard_deadline_v1.map(|f| f.activation),
        };
        raw.filter(|f| *f != ForkActivation::never())
    }

    /// The refusal a real network gets while no record for it is pinned.
    pub fn no_record_refusal(self) -> &'static str {
        match self {
            Self::StagedPublicCourtV1 => {
                "palw_staged_public_court_v1 cannot be armed: no arm-readiness record (G14 validation V1–V7) is pinned for this network \
                 and height (PALW_G14_ARMING_RECORDS_V1 is empty; audit G-10)"
            }
            Self::StagedTirRouteV1 => {
                "palw_staged_tir_route_v1 cannot be armed: no arm-readiness record (G14 validation V1–V7) is pinned for this network and \
                 height (PALW_G14_ARMING_RECORDS_V1 is empty; audit G-10)"
            }
            Self::StagedFloatConstraintV1 => {
                "palw_staged_float_constraint_v1 cannot be armed: no arm-readiness record (G14 validation V1–V7) is pinned for this \
                 network and height (PALW_G14_ARMING_RECORDS_V1 is empty; audit G-10)"
            }
            Self::TirPublicConvictionV1 => {
                "palw_tir_public_conviction_v1 cannot be armed: no arm-readiness record (G14 validation V1–V7) is pinned for this network \
                 and height (PALW_G14_ARMING_RECORDS_V1 is empty; audit G-10)"
            }
            Self::PublicEvidenceRewardV1 => {
                "palw_public_evidence_reward_v1 cannot be armed: no arm-readiness record (T-N9 fold and node E2E, D-17 drill) is pinned \
                 for this network and height (PALW_G14_ARMING_RECORDS_V1 is empty; register N4, ruling 2026-10-12)"
            }
            Self::G14RouteRequiredV1 => {
                "palw_g14_route_required_v1 cannot be armed: no arm-readiness record (T-N8 / T-N10 fold, node E2E, fence-crossing \
                 drill) is pinned for this network and height (PALW_G14_ARMING_RECORDS_V1 is empty; audit G-01 option (b))"
            }
            Self::RewardLiabilityHorizonV1 => {
                "palw_reward_liability_horizon_v1 cannot be armed: no arm-readiness record (T-N15 fold, node E2E, fence-crossing \
                 drill) is pinned for this network and height (PALW_G14_ARMING_RECORDS_V1 is empty; audit G-08 / G-15)"
            }
            Self::LegacyPublicFilerV1 => {
                "palw_legacy_public_filer_v1 cannot be armed: no arm-readiness record (LG14-A: fold suite, node E2E, measured hard-deadline \
                 envelope and lock-life capacity, fence-crossing drill) is pinned for this network and height (PALW_G14_ARMING_RECORDS_V1 \
                 is empty; audit G-01 option (a) / G-06 / G-07 / G-13)"
            }
            Self::LegacyHeldDaV2 => {
                "palw_legacy_held_da_v2 cannot be armed: no arm-readiness record (LG14-B: held-DA node E2E, measured descent budget, \
                 fence-crossing drill) is pinned for this network and height (PALW_G14_ARMING_RECORDS_V1 is empty; audit G-01 option (a))"
            }
            Self::CourtCapacityV1 => {
                "palw_court_capacity_v1 cannot be armed: no arm-readiness record (measured court operation costs and block budget, \
                 rho_court, saturation / refill / maximum-load drills) is pinned for this network and height (PALW_G14_ARMING_RECORDS_V1 \
                 is empty; audit G-07)"
            }
            Self::CourtModelScopeV1 => {
                "palw_court_model_scope_v1 cannot be armed: no arm-readiness record (D2 replacement-route suite, node full-collusion E2E, \
                 fence-crossing drill) is pinned for this network and height (PALW_G14_ARMING_RECORDS_V1 is empty; register D2)"
            }
            Self::ClaimHardDeadlineV1 => {
                "palw_claim_hard_deadline_v1 cannot be armed: no arm-readiness record (X3 fold suite, measured B_* terms and c_retain, node \
                 E2E, fence-crossing drill) is pinned for this network and height (PALW_G14_ARMING_RECORDS_V1 is empty; register X3)"
            }
        }
    }
}

/// The record format's version.
pub const PALW_G14_ARMING_RECORD_VERSION_V1: u16 = 1;

/// **An arm-readiness record** — a fence, the network (genesis) and the exact height it authorizes, the source commit of the binary
/// that ran the evidence, one evidence digest per [`PalwG14GatedFenceV1::requirements`] entry (in order, each non-zero and distinct) and
/// the review's digest. Pinned only by a reviewed change; the binary's list is [`PALW_G14_ARMING_RECORDS_V1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwG14ArmingRecordV1 {
    pub version: u16,
    pub fence: PalwG14GatedFenceV1,
    pub genesis: Hash64,
    pub activation_daa: u64,
    pub source_commit: [u8; 20],
    pub evidence: &'static [[u8; 32]],
    pub review: [u8; 32],
}

/// **The pinned records — EMPTY.** Every gated fence armed at a real height on a real network is refused.
pub const PALW_G14_ARMING_RECORDS_V1: &[PalwG14ArmingRecordV1] = &[];

/// Why a record does not authorize an arming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwG14ArmingRefusalV1 {
    /// No pinned record names this fence, network and height.
    NoRecord,
    /// The record's version is not this binary's.
    Version,
    /// The record names a zero source commit.
    SourceCommit,
    /// The evidence list does not have one digest per requirement.
    EvidenceCount { expected: usize, got: usize },
    /// Evidence `i` is zero, repeated, or equal to the review.
    Evidence(usize),
    /// The review digest is zero.
    Review,
}

impl PalwG14ArmingRefusalV1 {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoRecord => "no pinned arm-readiness record names this fence, network and height",
            Self::Version => "the arm-readiness record's version is not this binary's",
            Self::SourceCommit => "the arm-readiness record names no source commit",
            Self::EvidenceCount { .. } => "the arm-readiness record does not carry one evidence digest per requirement",
            Self::Evidence(_) => "an arm-readiness evidence digest is zero, repeated, or equal to the review",
            Self::Review => "the arm-readiness record carries no review digest",
        }
    }
}

/// A record's own form (not whether it is pinned).
pub fn palw_g14_arming_record_form_v1(record: &PalwG14ArmingRecordV1) -> Result<(), PalwG14ArmingRefusalV1> {
    if record.version != PALW_G14_ARMING_RECORD_VERSION_V1 {
        return Err(PalwG14ArmingRefusalV1::Version);
    }
    if record.source_commit == [0; 20] {
        return Err(PalwG14ArmingRefusalV1::SourceCommit);
    }
    let expected = record.fence.requirements().len();
    if record.evidence.len() != expected {
        return Err(PalwG14ArmingRefusalV1::EvidenceCount { expected, got: record.evidence.len() });
    }
    if record.review == [0; 32] {
        return Err(PalwG14ArmingRefusalV1::Review);
    }
    for (i, digest) in record.evidence.iter().enumerate() {
        if *digest == [0; 32] || *digest == record.review || record.evidence[..i].contains(digest) {
            return Err(PalwG14ArmingRefusalV1::Evidence(i));
        }
    }
    Ok(())
}

/// **Does `records` authorize arming `fence` at `activation_daa` on the network whose genesis is `genesis`?** The first record that
/// names exactly this (fence, network, height) decides; none is [`PalwG14ArmingRefusalV1::NoRecord`].
pub fn palw_g14_arming_record_admits_v1(
    fence: PalwG14GatedFenceV1,
    genesis: Hash64,
    activation_daa: u64,
    records: &[PalwG14ArmingRecordV1],
) -> Result<(), PalwG14ArmingRefusalV1> {
    let mut named = records.iter().filter(|r| r.fence == fence && r.genesis == genesis && r.activation_daa == activation_daa);
    let Some(first) = named.next() else { return Err(PalwG14ArmingRefusalV1::NoRecord) };
    palw_g14_arming_record_form_v1(first)
}

// =================================================================================================================================
// 2. The isolated G14 test genesis (G14 from genesis, test-only)
// =================================================================================================================================

/// The marker an isolated G14 test genesis appends to its base genesis payload (before the 64-byte isolation digest).
pub const PALW_G14_TEST_GENESIS_MARKER_V1: &[u8] = b"MISAKA/PALW/G14-TEST-GENESIS/V1";

/// **An isolated G14 test genesis** derived from `base`: its coinbase payload is `base ‖ marker ‖ isolation`, its merkle root and hash
/// recomputed. A different genesis is a different network (it is in `consensus_identity_id`), so such a ruleset never joins a real one.
pub fn palw_isolated_g14_genesis_v1(
    base: &crate::config::genesis::GenesisBlock,
    isolation: Hash64,
) -> Result<crate::config::genesis::GenesisBlock, &'static str> {
    if isolation == Hash64::default() || base.coinbase_payload.len() > 4096 {
        return Err("an isolated G14 genesis needs a non-zero isolation digest and a bounded base payload");
    }
    let mut raw = base.coinbase_payload.to_vec();
    raw.extend_from_slice(PALW_G14_TEST_GENESIS_MARKER_V1);
    raw.extend_from_slice(isolation.as_bytes().as_slice());
    let mut g = base.clone();
    // GenesisBlock owns a static payload by design (as `palw_native_calibration_v34::isolated_native_genesis_v34`): one bounded
    // allocation per test configuration; no protocol input calls this.
    g.coinbase_payload = Box::leak(raw.into_boxed_slice());
    g.hash_merkle_root = crate::merkle::calc_hash_merkle_root(g.build_genesis_transactions().iter());
    g.hash = crate::header::Header::from(&g).hash;
    Ok(g)
}

impl Params {
    /// **Is this ruleset on an isolated G14 test genesis?** Its genesis payload ends with the marker and a non-zero isolation digest, its
    /// merkle root and hash recompute, and it is not a mainnet. No shipped preset's genesis carries the marker.
    pub fn has_isolated_g14_genesis_v1(&self) -> bool {
        let payload = self.genesis.coinbase_payload;
        let marker = PALW_G14_TEST_GENESIS_MARKER_V1;
        if matches!(self.net.network_type(), crate::network::NetworkType::Mainnet) || payload.len() < marker.len() + 64 {
            return false;
        }
        let (head, isolation) = payload.split_at(payload.len() - 64);
        head.ends_with(marker)
            && isolation.iter().any(|b| *b != 0)
            && self.genesis.hash_merkle_root == crate::merkle::calc_hash_merkle_root(self.genesis.build_genesis_transactions().iter())
            && crate::header::Header::from(&self.genesis).hash == self.genesis.hash
    }

    /// **One gated fence's record gate**: `Ok` when the fence is not armed (`None`, `never()`), on an isolated G14 test genesis, or when
    /// a pinned record authorizes exactly this network and height AND the court policy is complete;
    /// otherwise the fence's own refusal. A readiness digest cannot close RFC-0026 F02/F03.
    pub fn validate_palw_g14_fence_record_v1(&self, fence: PalwG14GatedFenceV1) -> Result<(), PalwModeV2Error> {
        self.validate_palw_g14_fence_record_in_v1(fence, PALW_G14_ARMING_RECORDS_V1)
    }

    /// [`Self::validate_palw_g14_fence_record_v1`] against `records` in place of the binary's pinned list — the one path
    /// [`Params::validate_palw_v2_with_arming_records_v1`] takes, so a test (or arm-readiness tooling) can show that a ruleset passes
    /// the WHOLE validation once a record for its exact (fence, network, height) exists. The node never calls it: `ConfigBuilder`
    /// validates with [`PALW_G14_ARMING_RECORDS_V1`], so a record only arms a real network once a reviewed change pins it.
    pub fn validate_palw_g14_fence_record_in_v1(
        &self,
        fence: PalwG14GatedFenceV1,
        records: &[PalwG14ArmingRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        let Some(at) = fence.armed(self) else { return Ok(()) };
        if self.has_isolated_g14_genesis_v1() {
            return Ok(());
        }
        match palw_g14_arming_record_admits_v1(fence, self.genesis.hash, at.daa_score(), records) {
            Ok(()) => self.validate_palw_court_policy_completeness_v1(fence),
            Err(PalwG14ArmingRefusalV1::NoRecord) => Err(PalwModeV2Error::Invalid(fence.no_record_refusal())),
            Err(other) => Err(PalwModeV2Error::Invalid(other.reason())),
        }
    }

    /// RFC-0026 §4–5: readiness digests cannot substitute for a defined service
    /// opportunity or an outsider's access to localization. No production completion
    /// record exists. The isolated test genesis may exercise unfinished routes.
    pub fn validate_palw_court_policy_completeness_v1(&self, fence: PalwG14GatedFenceV1) -> Result<(), PalwModeV2Error> {
        if fence.armed(self).is_none() || self.has_isolated_g14_genesis_v1() {
            return Ok(());
        }
        if matches!(fence,
            PalwG14GatedFenceV1::StagedPublicCourtV1
            | PalwG14GatedFenceV1::StagedTirRouteV1
            | PalwG14GatedFenceV1::StagedFloatConstraintV1
            | PalwG14GatedFenceV1::TirPublicConvictionV1
            | PalwG14GatedFenceV1::LegacyPublicFilerV1
            | PalwG14GatedFenceV1::LegacyHeldDaV2
        ) {
            // G14 lane court-f02: each half clears only on its own evidence. F02 needs the attribution, the admission binding, the
            // shared lifecycle dispatch and native notice intake (all present) AND a pinned service profile (typed-missing, so it
            // still refuses). F03's outsider capacity clears only when the court-owner outsider lane is in force at or below this
            // fence.
            let service = crate::palw_court_service_binding_v1::palw_court_service_opportunity_complete_v1();
            let outsider = fence.armed(self).zip(self.palw_court_outsider_lane_v1_fence())
                .is_some_and(|(at, lane)| lane.daa_score() <= at.daa_score());
            match (outsider, service) {
                (true, true) => {}
                (false, false) => return Err(PalwModeV2Error::Invalid(
                    "OneOutsiderCapacityIncomplete: RFC-0026 F03 shared localization under colluding owner occupancy is incomplete; \
                     ServiceOpportunityIncomplete: F02 deterministic service opportunities and inclusion assumptions are not adopted",
                )),
                (false, true) => return Err(PalwModeV2Error::Invalid(
                    "OneOutsiderCapacityIncomplete: RFC-0026 F03 needs palw_court_outsider_lane_v1 in force at or below this fence",
                )),
                (true, false) => return Err(PalwModeV2Error::Invalid(
                    "ServiceOpportunityIncomplete: RFC-0026 F02 has no pinned court service profile (PALW_COURT_SERVICE_PROFILE_V1)",
                )),
            }
        }
        Ok(())
    }

    /// **The cross-lane G14 gates' refusals**, asked by `validate_palw_v2`:
    ///
    /// * the mirrors the fold reads agree with the fences (`sync_palw_g14_gates_v1`);
    /// * option (b) and the liability horizon are `ConsensusV2` rules;
    /// * **G-18**: `palw_staged_public_court_v1` needs `palw_legacy_public_filer_v1` in force at or below it — a new staged seed and a
    ///   `BundleOpen` each need the filer's own live dispute reservation (tag 154), which only that fence opens. Asked before the record
    ///   gate so the ordering is already pinned when a record is;
    /// * **G-10 / N4 / option (b) / liability**: every gated fence armed at a real height needs its pinned record.
    pub fn validate_palw_g14_gates_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_g14_gates_in_v1(PALW_G14_ARMING_RECORDS_V1)
    }

    /// [`Self::validate_palw_g14_gates_v1`] against `records` (see [`Self::validate_palw_g14_fence_record_in_v1`]).
    pub fn validate_palw_g14_gates_in_v1(&self, records: &[PalwG14ArmingRecordV1]) -> Result<(), PalwModeV2Error> {
        if self.palw_g14_route_required_v1_mirror() != self.palw_bundle_g14_route_mirror_v1() {
            return Err(PalwModeV2Error::Invalid(
                "palw_g14_route_required_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_g14_gates_v1",
            ));
        }
        let liability = PalwG14GatedFenceV1::RewardLiabilityHorizonV1.armed(self).map(|f| f.daa_score());
        let v2 = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        };
        if v2.map(|b| b.state.reward_liability_horizon_from_daa()).unwrap_or(None) != liability && v2.is_some() {
            return Err(PalwModeV2Error::Invalid(
                "palw_reward_liability_horizon_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_g14_gates_v1",
            ));
        }
        for fence in [PalwG14GatedFenceV1::G14RouteRequiredV1, PalwG14GatedFenceV1::RewardLiabilityHorizonV1] {
            if fence.armed(self).is_some() && v2.is_none() {
                return Err(PalwModeV2Error::Invalid(
                    "palw_g14_route_required_v1 and palw_reward_liability_horizon_v1 are ConsensusV2 rules: this ruleset has no V2 bundle",
                ));
            }
        }
        palw_g14_staged_court_needs_legacy_filer_v1(self)?;
        for fence in PalwG14GatedFenceV1::ALL {
            self.validate_palw_g14_fence_record_in_v1(fence, records)?;
        }
        Ok(())
    }

    /// **ADR-0191 step B gate (DORMANT).** `palw_tir_designated_answers_v1` needs the TIR public conviction fence, because its rules
    /// only answer a conviction path. It needs no record: nothing consumes it on any shipped preset.
    pub fn validate_palw_tir_designated_answers_in_v1(&self) -> Result<(), PalwModeV2Error> {
        if self.palw_tir_designated_answers_v1_fence().is_none() {
            return Ok(());
        }
        if self.palw_tir_public_conviction_v1_fence().is_none() {
            return Err(PalwModeV2Error::Invalid("palw_tir_designated_answers_v1 needs palw_tir_public_conviction_v1"));
        }
        Ok(())
    }

    /// **G14 F03 outsider court lane gate (DORMANT, lane g14p/court-owner).** `palw_court_outsider_lane_v1` needs
    /// `palw_legacy_public_filer_v1` in force at or below it (the lane reads the LG14-A dispute record and its reservations, and that
    /// fence carries the arming record), and the fold's mirror must agree (`sync_palw_g14_gates_v1`).
    pub fn validate_palw_court_outsider_lane_in_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(lane) = self.palw_court_outsider_lane_v1_fence() else { return Ok(()) };
        if !self.palw_legacy_public_filer_v1.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= lane.daa_score()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_outsider_lane_v1 needs palw_legacy_public_filer_v1 in force at or below it (G14 F03 outsider court lane)",
            ));
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            if bundle.state.court_outsider_lane_from_daa() != Some(lane.daa_score()) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_court_outsider_lane_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_g14_gates_v1",
                ));
            }
        }
        Ok(())
    }

    /// **G14 U06/U07 court clock v2 gate (DORMANT, lane g14p/court-clock).** `palw_court_clock_v2` needs `palw_legacy_public_filer_v1`
    /// in force at or below it (the extension IS the LG14-A dispute record, and that fence carries the arming record), and the fold's
    /// mirror must agree (`sync_palw_g14_gates_v1`).
    pub fn validate_palw_court_clock_v2(&self) -> Result<(), PalwModeV2Error> {
        let Some(clock) = self.palw_court_clock_v2_fence() else { return Ok(()) };
        if !self.palw_legacy_public_filer_v1.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= clock.daa_score()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_court_clock_v2 needs palw_legacy_public_filer_v1 in force at or below it (G14 U06/U07 court clock)",
            ));
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            if bundle.state.court_clock_v2_from_daa() != Some(clock.daa_score()) {
                return Err(PalwModeV2Error::Invalid("palw_court_clock_v2 disagrees with the V2 bundle's mirror: Params::sync_palw_g14_gates_v1"));
            }
        }
        Ok(())
    }

    /// **The TIR frame cost gate** (audit §10): `Ok` when `palw_tir_public_conviction_v1` is not armed; otherwise the ruleset must
    /// carry a well-formed `palw_tir_frame_cost_v1` and — unless its genesis is an isolated G14 test genesis — a record in `records`
    /// must name this network, the fence's height and exactly that model.
    /// **ADR-0190 bounded-round door gate (DORMANT).** `Ok` when `palw_tir_bounded_door_v1` is not armed. Arming needs the frame cost
    /// model, the TIR public conviction fence, a pinned record (unless isolated genesis), and the gather/param operand bound, which is
    /// not derived yet ([`PALW_TIR_BOUNDED_DOOR_GATHER_BOUND_DERIVED_V1`] is `false`), so arming is refused everywhere.
    pub fn validate_palw_tir_bounded_door_in_v1(
        &self,
        records: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_tir_bounded_door_v1_fence() else { return Ok(()) };
        if !PALW_TIR_BOUNDED_DOOR_GATHER_BOUND_DERIVED_V1 {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_bounded_door_v1 cannot be armed: the gather / model-parameter operand bound (ADR-0190) is not derived",
            ));
        }
        if self.palw_tir_frame_cost_v1.is_none() || self.palw_tir_public_conviction_v1_fence().is_none() {
            return Err(PalwModeV2Error::Invalid("palw_tir_bounded_door_v1 needs palw_tir_frame_cost_v1 and palw_tir_public_conviction_v1"));
        }
        if self.has_isolated_g14_genesis_v1() {
            return Ok(());
        }
        let cost = self.palw_tir_frame_cost_v1.as_ref().expect("checked above");
        crate::palw_tir_frame_cost_v1::palw_tir_frame_cost_record_admits_v1(self.genesis.hash, at.daa_score(), Some(cost), records)
            .map_err(|_| PalwModeV2Error::Invalid("palw_tir_bounded_door_v1 has no pinned frame cost record for this network and height"))
    }

    pub fn validate_palw_tir_frame_cost_in_v1(
        &self,
        records: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        let Some(at) = PalwG14GatedFenceV1::TirPublicConvictionV1.armed(self) else { return Ok(()) };
        let Some(cost) = self.palw_tir_frame_cost_v1.as_ref() else {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_public_conviction_v1 cannot be armed without a measured TIR frame cost model (palw_tir_frame_cost_v1; audit §10)",
            ));
        };
        if cost.check_v1().is_err() {
            return Err(PalwModeV2Error::Invalid("palw_tir_frame_cost_v1 is malformed (zero window, price, unit or capacity)"));
        }
        // External audit 10-17 §7 P0(3): the longest accepted pursuit ends inside the liability horizon Final and payment are held
        // to (`window_challenge_at(fence) + window_court`), on every network including an isolated test genesis.
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            let horizon =
                bundle.state.window_challenge_at(at.daa_score()).saturating_add(bundle.state.window_court());
            if cost.clock.maximum_door_daa > horizon {
                return Err(PalwModeV2Error::Invalid(
                    "palw_tir_frame_cost_v1's longest accepted pursuit (maximum_door_daa) exceeds the network's liability horizon \
                     window_challenge + window_court",
                ));
            }
        }
        if self.has_isolated_g14_genesis_v1() {
            return Ok(());
        }
        crate::palw_tir_frame_cost_v1::palw_tir_frame_cost_record_admits_v1(self.genesis.hash, at.daa_score(), Some(cost), records)
            .map_err(|why| PalwModeV2Error::Invalid(why.reason()))
    }

    /// `palw_g14_route_required_v1` in force at `daa_score`.
    pub fn palw_g14_route_required_v1_active_at(&self, daa_score: u64) -> bool {
        PalwG14GatedFenceV1::G14RouteRequiredV1.armed(self).is_some_and(|f| f.is_active(daa_score))
    }

    /// `palw_reward_liability_horizon_v1` in force at `daa_score`.
    pub fn palw_reward_liability_horizon_v1_active_at(&self, daa_score: u64) -> bool {
        PalwG14GatedFenceV1::RewardLiabilityHorizonV1.armed(self).is_some_and(|f| f.is_active(daa_score))
    }

    /// **Option (b)'s mirror**: its height and the resolved heights of every route fence it reads (`None` where not armed).
    pub fn palw_g14_route_required_v1_mirror(&self) -> Option<PalwG14RouteMirrorV1> {
        let from_daa = PalwG14GatedFenceV1::G14RouteRequiredV1.armed(self)?.daa_score();
        let height = |f: Option<ForkActivation>| f.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        Some(PalwG14RouteMirrorV1 {
            from_daa,
            legacy_filer_from: height(self.palw_legacy_public_filer_v1),
            legacy_held_da_from: height(self.palw_legacy_held_da_v2),
            staged_court_from: height(self.palw_staged_public_court_v1_fence()),
            tir_conviction_from: height(self.palw_tir_public_conviction_v1_fence()),
            court_capacity_from: self.palw_court_capacity_v1_fence().map(|f| f.activation.daa_score()),
        })
    }

    fn palw_bundle_g14_route_mirror_v1(&self) -> Option<PalwG14RouteMirrorV1> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.g14_route_required_v1(),
            _ => self.palw_g14_route_required_v1_mirror(),
        }
    }

    /// **The cross-lane G14 gates' mirrors** on the V2 bundle's state params, which the fold reads. Written here and nothing else; `None`
    /// where a fence is not armed. Called after every route fence is set (it reads their resolved heights).
    pub fn sync_palw_g14_gates_v1(&mut self) {
        let route = self.palw_g14_route_required_v1_mirror();
        let liability = PalwG14GatedFenceV1::RewardLiabilityHorizonV1.armed(self).map(|f| f.daa_score());
        let outsider_lane = self.palw_court_outsider_lane_v1_fence().map(|f| f.daa_score());
        let panel_one = self.palw_kernel_route_panel_one_v1_fence().map(|f| f.daa_score());
        let court_clock = self.palw_court_clock_v2_fence().map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle
                .state
                .clone()
                .with_g14_route_required_v1(route)
                .with_reward_liability_horizon_from_daa(liability)
                .with_court_outsider_lane_from_daa(outsider_lane)
                .with_kernel_route_panel_one_from_daa(panel_one)
                .with_court_clock_v2_from_daa(court_clock);
        }
    }
}

/// **G-18 (T-N16): the staged public court needs the legacy public filer in force at or below it.** The alternative — decoupling the
/// staged court from tag 154 — is NOT taken: the reservation is what holds a claim's Final while a non-seat pursues it (R3) and what
/// bounds a reserver to one staged row (G-07, `lg14a_coalition_reservations_cannot_exclude_a_new_bond`); a staged court without it
/// could be outrun (G-06) and filled (G-13) exactly as the live legacy route is. Requiring the prerequisite changes no fold rule.
pub fn palw_g14_staged_court_needs_legacy_filer_v1(p: &Params) -> Result<(), PalwModeV2Error> {
    let Some(court) = PalwG14GatedFenceV1::StagedPublicCourtV1.armed(p) else { return Ok(()) };
    if p.palw_legacy_public_filer_v1.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= court.daa_score()) {
        return Ok(());
    }
    Err(PalwModeV2Error::Invalid(
        "palw_staged_public_court_v1 needs palw_legacy_public_filer_v1 in force at or below it: a staged seed and BundleOpen each need \
         the filer's own live dispute reservation (tag 154) (audit G-18)",
    ))
}

// =================================================================================================================================
// 3. G-01 option (b) and G-05: the route-required claim gate
// =================================================================================================================================

/// **Option (b)'s mirror** (`PalwStateParamsV2::g14_route_required_v1`): the fence's height and the resolved height of every route
/// fence the rule reads. A route is armed for a claim accepted at `d` when every fence it needs is in force at `d`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwG14RouteMirrorV1 {
    pub from_daa: u64,
    /// `palw_legacy_public_filer_v1` (LG14-A: the common filer, reservation, Final hold, FP direct proofs).
    pub legacy_filer_from: Option<u64>,
    /// `palw_legacy_held_da_v2` (LG14-B: the public descent and held DA of the legacy route).
    pub legacy_held_da_from: Option<u64>,
    /// `palw_staged_public_court_v1`, resolved (ConsensusV2).
    pub staged_court_from: Option<u64>,
    /// `palw_tir_public_conviction_v1`, resolved (the later of its prerequisites).
    pub tir_conviction_from: Option<u64>,
    /// `palw_court_capacity_v1` (audit G-07): the court-capacity guarantee the fused-leaf adjudication route depends on.
    pub court_capacity_from: Option<u64>,
}

fn in_force(height: Option<u64>, daa: u64) -> bool {
    height.is_some_and(|h| daa >= h)
}

impl PalwG14RouteMirrorV1 {
    /// Does option (b) govern a claim accepted at `accepted_daa`? (At fence 0 every claim: nothing is pre-fence.)
    pub fn governs(&self, accepted_daa: u64) -> bool {
        accepted_daa >= self.from_daa
    }

    /// Is `family`'s G14 public-conviction route armed for a claim accepted at `accepted_daa`? `framed`: the commitment is paired with a
    /// TIR public frame (TFP1) in its own transaction.
    pub fn route_armed(&self, family: PalwG14RouteFamilyV1, accepted_daa: u64, framed: bool) -> bool {
        let d = accepted_daa;
        match family {
            PalwG14RouteFamilyV1::LegacyPanelAttempt => in_force(self.legacy_filer_from, d) && in_force(self.legacy_held_da_from, d),
            // User direction 2026-10-16 (§1): the fused-leaf route (KernelWitness → ShardCourtAccused → held dissection) needs a court
            // slot, so it is not a G14 route until the court-capacity guarantee is armed as well.
            PalwG14RouteFamilyV1::LegacyFusedLeaf => {
                self.route_armed(PalwG14RouteFamilyV1::LegacyPanelAttempt, d, framed) && in_force(self.court_capacity_from, d)
            }
            PalwG14RouteFamilyV1::FpSelection => framed && in_force(self.tir_conviction_from, d),
            PalwG14RouteFamilyV1::FpConstraint | PalwG14RouteFamilyV1::FpPrefix | PalwG14RouteFamilyV1::FpTokenizer => {
                in_force(self.staged_court_from, d) && in_force(self.legacy_filer_from, d)
            }
            PalwG14RouteFamilyV1::NonPublicEvidence => false,
        }
    }
}

/// **The G14 route families** a newly admitted legacy-route claim needs (audit §3.2 / §3.3, G-01, G-05). Lead default 2026-10-16:
///
/// | family | claims | G14 public-conviction route (every fence in force at the claim's acceptance) |
/// |---|---|---|
/// | `LegacyPanelAttempt` | every V2 Attempt (floor, model, TIR v1, gen v1, shard classes) | LG14-A `palw_legacy_public_filer_v1` + LG14-B `palw_legacy_held_da_v2` |
/// | `LegacyFusedLeaf` | a V2 Attempt whose class has fused-attention leaves (their lies are adjudicated only through a court session) | `LegacyPanelAttempt`'s + `palw_court_capacity_v1` (user direction 2026-10-16) |
/// | `FpSelection` | every FP job decoded under a selection rule (V3, V4, constraint, prefix) | the TIR public frame: `palw_tir_public_conviction_v1` AND the commitment carries its TFP1 frame |
/// | `FpConstraint` | FP Job V6 (constraint) | the staged Constraint family: `palw_staged_public_court_v1` + LG14-A |
/// | `FpPrefix` | prefix state / inherit (11 / 12) | the staged PublicPrefix family: `palw_staged_public_court_v1` + LG14-A |
/// | `FpTokenizer` | every FP job (its ids ↔ text through its tokenizer) | the staged raw-tokenizer family: `palw_staged_public_court_v1` + LG14-A |
/// | `NonPublicEvidence` | FP Job V5 (images / source ids), PanelDa | none can exist (the material never reaches the chain) |
///
/// The legacy FP lane's selection proofs are refused on FP claims and contradiction 14–18 sit under LG14-A (R5, R6), so even LG14-A +
/// LG14-B give an unframed FP claim no selection route: a plain FP claim regains reward only through the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwG14RouteFamilyV1 {
    LegacyPanelAttempt,
    LegacyFusedLeaf,
    FpSelection,
    FpConstraint,
    FpPrefix,
    FpTokenizer,
    NonPublicEvidence,
}

/// The refusal code for a claim whose family has no armed G14 route (no reward, weight or credit; `ClassNotRewardable`'s `code`).
pub const PALW_G14_ROUTE_ABSENT_CODE_V1: &str = "G14_ROUTE_ABSENT";
/// The refusal code for a G-05 job version (selection, constraint, prefix, tokenizer) admitted before its route is armed.
pub const PALW_G14_JOB_UNJUDGEABLE_CODE_V1: &str = "G14_JOB_VERSION_UNJUDGEABLE";

/// **The route families an FP job needs** (a G-05 job version names its own; every FP job needs selection and tokenizer). A V5 job or a
/// PanelDa job is [`PalwG14RouteFamilyV1::NonPublicEvidence`] alone.
pub fn palw_g14_fp_route_families_v1(job: &PalwFreePromptJobV3) -> Vec<PalwG14RouteFamilyV1> {
    if job.privacy_mode == PALW_FP_PRIVACY_PANEL_DA || job.is_v5() || matches!(job.tail, Some(PalwFpJobTailV1::V5(_))) {
        return vec![PalwG14RouteFamilyV1::NonPublicEvidence];
    }
    let mut families = vec![PalwG14RouteFamilyV1::FpSelection];
    if job.is_constraint() {
        families.push(PalwG14RouteFamilyV1::FpConstraint);
    }
    if job.is_prefix_state() {
        families.push(PalwG14RouteFamilyV1::FpPrefix);
    }
    families.push(PalwG14RouteFamilyV1::FpTokenizer);
    families
}

/// **Option (b) on a V2 Attempt of a class WITH fused-attention leaves** (lane g14-court-capacity, user direction 2026-10-16 §1):
/// [`palw_g14_route_attempt_refusal_v1`] and, past it, the fused-leaf route — whose adjudication needs a court session — counts only
/// once `palw_court_capacity_v1` is in force at the claim's acceptance. `fused` is `PalwClassStateV2::fused_attention`. This is the
/// one the class gate asks; below option (b)'s fence `None`, byte for byte.
pub fn palw_g14_route_attempt_refusal_for_class_v1(params: &PalwStateParamsV2, accepted_daa: u64, fused: bool) -> Option<String> {
    if let Some(why) = palw_g14_route_attempt_refusal_v1(params, accepted_daa) {
        return Some(why);
    }
    let mirror = params.g14_route_required_v1()?;
    if !fused || !mirror.governs(accepted_daa) || mirror.route_armed(PalwG14RouteFamilyV1::LegacyFusedLeaf, accepted_daa, false) {
        return None;
    }
    Some(
        "palw_g14_route_required_v1: this V2 Attempt's class has fused-attention leaves, whose lies are adjudicated only through a \
         court session (KernelWitness, ShardCourtAccused, held dissection); that route counts as a G14 route only once \
         palw_court_capacity_v1 guarantees an honest prosecution court capacity before its deadline: no reward, weight or consensus \
         credit"
            .to_string(),
    )
}

/// **Option (b) on a V2 Attempt accepted at `accepted_daa`** (the class-agnostic half: a class with no fused-attention leaf): `None`
/// below the fence (or where no network arms it), or when the legacy Panel route's G14 route is armed; else `Some(why)`, refused at
/// the claim gate with [`PALW_G14_ROUTE_ABSENT_CODE_V1`].
pub fn palw_g14_route_attempt_refusal_v1(params: &PalwStateParamsV2, accepted_daa: u64) -> Option<String> {
    let mirror = params.g14_route_required_v1()?;
    if !mirror.governs(accepted_daa) || mirror.route_armed(PalwG14RouteFamilyV1::LegacyPanelAttempt, accepted_daa, false) {
        return None;
    }
    Some(
        "palw_g14_route_required_v1: a V2 Attempt's family (the legacy Panel route) has no armed G14 public-conviction route \
         (LG14-A palw_legacy_public_filer_v1 + LG14-B palw_legacy_held_da_v2): no reward, weight or consensus credit"
            .to_string(),
    )
}

/// **Option (b) and G-05 on a free-prompt commitment accepted at `accepted_daa`**: `None` below the fence or when every family the
/// job needs has its route armed; else `Some((code, why))`. A G-05 job version (constraint, prefix) is refused with
/// [`PALW_G14_JOB_UNJUDGEABLE_CODE_V1`]; a job whose only gap is the lane-wide selection / tokenizer route, or whose evidence is
/// non-public, with [`PALW_G14_ROUTE_ABSENT_CODE_V1`]. `commitment = None` (nothing shows the job) is refused (fail closed).
pub fn palw_g14_route_fp_refusal_v1(
    params: &PalwStateParamsV2,
    accepted_daa: u64,
    commitment: Option<&PalwFreePromptCommitmentV3>,
    framed: bool,
) -> Option<(&'static str, String)> {
    let mirror = params.g14_route_required_v1()?;
    if !mirror.governs(accepted_daa) {
        return None;
    }
    let Some(commitment) = commitment else {
        return Some((PALW_G14_ROUTE_ABSENT_CODE_V1, "palw_g14_route_required_v1: no signed commitment shows this claim's job".into()));
    };
    let families = palw_g14_fp_route_families_v1(&commitment.job);
    let missing: Vec<PalwG14RouteFamilyV1> =
        families.iter().copied().filter(|f| !mirror.route_armed(*f, accepted_daa, framed)).collect();
    if missing.is_empty() {
        return None;
    }
    let g05 = missing.iter().any(|f| matches!(f, PalwG14RouteFamilyV1::FpConstraint | PalwG14RouteFamilyV1::FpPrefix));
    let code = if g05 { PALW_G14_JOB_UNJUDGEABLE_CODE_V1 } else { PALW_G14_ROUTE_ABSENT_CODE_V1 };
    Some((
        code,
        format!(
            "palw_g14_route_required_v1: FP job version {} needs G14 routes {missing:?} that are not armed at DAA {accepted_daa} \
             (framed: {framed}): refused at admission, no reward, weight or consensus credit",
            commitment.job.version
        ),
    ))
}

// =================================================================================================================================
// 4. G-08 / G-15: the reward liability horizon
// =================================================================================================================================

/// **G-08: a legacy free-prompt claim's liability horizon, as a Final floor** — `licensed + window_challenge_at(licensed) +
/// window_court`, the end of the window in which the old rule could still reverse its Final (old Final + `window_court`). Past
/// `palw_reward_liability_horizon_v1` (keyed on the claim's acceptance) the claim stays licensed — voidable, no receipt spend, no
/// coinbase leg — until that horizon, so every conviction the old rule allowed after Final now voids it before any payout.
/// `None` for an attempt (its escrow already vests to `final + window_court`), an evaluation claim (user-paid, off the reward path),
/// or below the fence. Read only by `palw_claim_final_floor_v1` (the one Final floor).
pub fn palw_legacy_fp_liability_floor_v1(
    params: &PalwStateParamsV2,
    claim: &PalwClaimStateV2,
    is_eval: bool,
    licensed_daa: u64,
) -> Option<u64> {
    if is_eval || !params.reward_liability_horizon_governs_v1(claim.accepted_daa) {
        return None;
    }
    if !matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }) {
        return None;
    }
    Some(licensed_daa.saturating_add(params.window_challenge_at(licensed_daa)).saturating_add(params.window_court()))
}

/// **G-15: until when a Gate C certified reward is held** — `final_daa + window_court` (the liability expiry the attempt lane's vesting
/// row uses, `palw_panel_liability_expiry_v1`), for a claim accepted past `palw_reward_liability_horizon_v1`; `None` below it or for a
/// claim that is not `Final`. The Gate C sweep names the reward at the first block strictly past it, or the deadline sweep at the claim's
/// retirement if that comes first (a retired claim can no longer be convicted) — once, and only if the claim is still certified.
pub fn palw_certified_reward_held_until_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<u64> {
    if !params.reward_liability_horizon_governs_v1(claim.accepted_daa) {
        return None;
    }
    match claim.phase {
        PalwClaimPhaseV2::Final { final_daa } => Some(final_daa.saturating_add(params.window_court())),
        _ => None,
    }
}

// =================================================================================================================================
// 5. G14 from genesis: the test-only ruleset
// =================================================================================================================================

/// The isolation digest of [`palw_g14_genesis_test_params_v1`].
pub const PALW_G14_GENESIS_TEST_ISOLATION_V1: u64 = 0x0614_6E45_5E15_0001;

/// **"g14-genesis": a TEST-ONLY ruleset with every G14 fence in force from block 0** (user decision 2026-10-16) — testnet-12's ruleset on
/// an isolated G14 genesis ([`palw_isolated_g14_genesis_v1`]), with option (b), the reward liability horizon, N4, the staged public
/// court with its legacy-filer prerequisite, the staged TIR and Float32 routes, the TIR public conviction route and — since lane
/// g14-close-legacy (2026-10-16) made it armable behind its record — LG14-B (`palw_legacy_held_da_v2`) at DAA 0, so the legacy Panel
/// route's G14 route (LG14-A + LG14-B) is complete from block 0 and V2 Attempt pays there. NOT armed: X3 (`palw_claim_hard_deadline_v1`:
/// no approved value exists, only an unmeasured candidate), and every fence whose own lane refuses it (Gate C, kernel route, EXEC,
/// native, rule E). Never a network: its genesis is isolated, and only such a genesis skips the record pins.
pub fn palw_g14_genesis_test_params_v1() -> Params {
    let mut p = crate::config::params::palw_t12_shipped_params();
    p.genesis = palw_isolated_g14_genesis_v1(&p.genesis, Hash64::from_u64_word(PALW_G14_GENESIS_TEST_ISOLATION_V1))
        .expect("the t12 genesis payload is bounded");
    let at = Some(ForkActivation::new(0));
    p.palw_legacy_public_filer_v1 = at;
    p.sync_palw_legacy_public_filer_v1();
    p.palw_legacy_held_da_v2 = at;
    // Lane g14-court-capacity: the court-capacity guarantee at 0 too (its placeholder policy is admissible only on an isolated genesis).
    p.palw_court_capacity_v1 = Some(crate::palw_court_capacity_v1::PalwCourtCapacityFenceV1::placeholder_at(ForkActivation::new(0)));
    p.sync_palw_court_capacity_v1();
    p.palw_staged_public_court_v1 = at;
    p.palw_staged_tir_route_v1 = at;
    p.palw_staged_float_constraint_v1 = at;
    p.palw_tir_public_conviction_v1 = at;
    // The frame's cost model: the TEST-ONLY config (an isolated genesis skips the record pin, never the model itself).
    // Its longest accepted pursuit is this genesis's own liability horizon at the fence (validation refuses a longer one).
    let mut cost = crate::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1();
    if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
        cost.clock.maximum_door_daa = bundle.state.window_challenge_at(0).saturating_add(bundle.state.window_court());
    }
    p.palw_tir_frame_cost_v1 = Some(cost);
    p.palw_public_evidence_reward_v1 = at;
    p.sync_palw_2026_10_12_ruling_fences_v1();
    p.palw_g14_route_required_v1 = at;
    p.palw_reward_liability_horizon_v1 = at;
    p.sync_palw_g14_gates_v1();
    p
}
