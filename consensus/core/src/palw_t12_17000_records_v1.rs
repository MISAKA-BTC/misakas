//! **GENERATED** by `scripts/release/gen_t12_17000_records.py` (lane ACT2) — do not edit by hand.
//! testnet-12 G14 flag-day arm-readiness records and waivers (F1 kinds only: drill skipped, E2E not run, provisional
//! estimate with its value, Claude-agent review, user decision). A record is emitted only when EVERY slot is a permitted
//! waiver or a run; leftover slots (no permitted kind) are listed below and keep their fence refused. A waiver lifts no
//! ordering, value, measurement, wiring or open-blocker refusal.
//!
//! Leftover slots:
use crate::palw_arming_waiver_v1::{PALW_ARMING_WAIVER_REVIEW_V1, PalwArmingWaiverKindV1, PalwArmingWaiverV1};

/// Every pinned waiver.
pub const PALW_T12_17000_WAIVERS_V1: &[PalwArmingWaiverV1] = &[
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "hard-deadline envelope = the X3 decided policy (Delta 2,016 DAA, inside trace retention minus the disclosure window); Panel capacity under the Valid lock = the lane VAL Panel=1 schedule (2 packets per 833-DAA interval)",
        reason: "estimate from the pinned X3 terms and the lane VAL V1 schedule (user 2026-10-22 estimates); not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "reservation budget 34 sessions (design constant), court session cap 42 (V11), answer bytes within the 34,000 B one-move budget; descent rounds bounded by the same budget",
        reason: "estimate from design constants and the user-approved V11 estimate (2026-10-26); not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "maximum legal delay = the X3 hard deadline (Delta 2,016 DAA); capacity under contention = V11 cap 42 sessions with the 42-DAA turn deadline",
        reason: "estimate from the pinned X3 decided terms and the user-approved V11 estimate (user 2026-10-22 / 2026-10-26); not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run. No node E2E of a lying FpConstraint claim on a TIR class with a TFP1 frame across a restart or reorg (CI-TRIAGE / lane FPCP addendum 3): in-process the same lie without restart/reorg is convicted (6 fresh_public_bond_service_*constraint* tests pass); the restart/reorg harness failure is its recovery mode not answering input; in the release an A16 unframed job is refused at admission",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "maximum legal delay = the X3 hard deadline (Delta 2,016 DAA); capacity under contention = V11 cap 42 sessions with the 42-DAA turn deadline",
        reason: "estimate from the pinned X3 decided terms and the user-approved V11 estimate (user 2026-10-22 / 2026-10-26); not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "maximum legal delay = the X3 hard deadline (Delta 2,016 DAA); capacity under contention = V11 cap 42 sessions with the 42-DAA turn deadline",
        reason: "estimate from the pinned X3 decided terms and the user-approved V11 estimate (user 2026-10-22 / 2026-10-26); not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1: cap 42, session weight 13, op_work [5,13,1,11], rho_court 3,125 sompi/DAA/unit, block budget work 13, one-move budget work 5 / 34,000 B",
        reason: "user approval 2026-10-26 of the V11 estimate as the value; only the one-move class was measured (M-CAP arm-legacy-2026-10-27, p95 141.7 us under load ~11); the all-class match and the no-load re-measurement are waived",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "rho_court 3,125 sompi/DAA/unit, session weight 13",
        reason: "lane VAL V11 estimate approved by the user 2026-10-26",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "PALW_X3_DECIDED_DELTA_TERMS_V1 (Delta 2,016 DAA) and c_retain 36,058 sompi/DAA (PALW_X3_RETENTION_COST_PER_DAA_SOMPI_V1)",
        reason: "lane P3 V10 decided 2026-10-22; c_retain = ceil(2,250,000 B x 50 sompi/B / 3,120 DAA); estimates, user 2026-10-22",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "9B-8k: producer exec ~440 s / ~56 GB, verifier ~41 GB, court ~710 s / ~19 GB (x4.5 of the Qwen3.5-2B measurement); 2M: not estimated - in this release only the wide128 class is attested, so no 9B class can be registered",
        reason: "provisional value derived from KCONV measurements on OTHER models (claude/kconv 0d98e8e6c, kconv-report.md): Dolphin 8k exec 115 s / 6.47 GB, verifier 4.24 GB, court 132 s / 2.22 GB; Qwen3.5-2B exec 97.6 s / 12.4 GB, verifier 9.19 GB, court 158 s / 4.22 GB; scaled linearly to parameter count",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "a late single fault is localized within the V11 court budgets (cap 42, 34,000 B one-move) and the X3 hard deadline (Delta 2,016 DAA); concurrent disputes bounded by cap 42",
        reason: "estimate from the user-approved V11 estimate and the pinned X3 terms; not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "lease reservation >= 100 MSK, challenge bond 10 MSK, challenge fee 1 MSK, response window 20 DAA, >= 2 providers, <= 8 open challenges (palw_provider_court_v1 interim constants)",
        reason: "the interim constants in palw_provider_court_v1, pinned as estimates (user 2026-10-22); not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "Gate C activates at testnet-12 DAA 17,400 with the G14 list",
        reason: "user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "certification m/k = 75/41; the drawn verifiers bear the re-executions",
        reason: "user approval 2026-10-26",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "U-6: no veto (m/k 75/41, veto 0)",
        reason: "user ruling 2026-10-27 (\"推奨どおり\", relayed by the coordinator)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "U-2/U-3: the issuance-leg hold is paid only after SecurityCertified; free-prompt gets no coinbase carve before certification (code: hold_issuance_reward_at_final_v1, palw_rfc0018_v1 free-prompt refusal)",
        reason: "user ruling 2026-10-27 (\"推奨どおり\", relayed by the coordinator)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "c = 300 permille, kappa = 1000 permille from DAA 17,400; catalog edition 0 empty",
        reason: "user decision 2026-10-27; c is the frozen premise (PRINCIPLES 9)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "lane V-REVIEW (read-only Claude agent) at fbd1998dd6e495562cbcf79500872e5bd334d7f2: PASS - the preset is genesis_only(Vec::new()) (full_activation_v1.rs:464-466), catalog-tip refusal palw_model_tier_v1.rs:465-470, KCAT 1792ea6aa not in the release",
        reason: "review: edition 0 only, none adds a model at 1.0 / new local LLM after the fence (independent review, user-approved reviewer)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "lane V-REVIEW (read-only Claude agent) at fbd1998dd6e495562cbcf79500872e5bd334d7f2: PASS - the preset is genesis_only(Vec::new()) (full_activation_v1.rs:464-466), catalog-tip refusal palw_model_tier_v1.rs:465-470, KCAT 1792ea6aa not in the release",
        reason: "review: edition 0 only, none adds a model at 1.0 / new local LLM after the fence (independent review, user-approved reviewer)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "readiness M1-M5: satisfied by the ACT2 report (edition 0 only, nothing added after the fence)",
        reason: "user ruling 2026-10-27 (\"推奨どおり\", relayed by the coordinator)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 (inclusion margin 4 DAA, rents 1, acquisition 1 DAA, collection 8 DAA, reorg retention 360 DAA)",
        reason: "lane VAL estimate, user approval 2026-10-26 of the estimates as values; not measured",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
];

/// The g14 records (complete ones only).
pub const PALW_T12_17000_G14_RECORDS_V1: &[crate::palw_g14_gates_v1::PalwG14ArmingRecordV1] = &[
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyPublicFilerV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0x51, 0x0b, 0x8c, 0x8d, 0xb3, 0x50, 0xc6, 0x0c, 0xab, 0xd8, 0xe5, 0x69, 0xac, 0x39, 0x6f, 0x2c, 0x44, 0xd9, 0x16,
                0x98, 0xc3, 0xa4, 0xa4, 0x57, 0xf6, 0xea, 0x26, 0x5f, 0x3e, 0xce, 0xff, 0x85,
            ], // slot 0 run
            [
                0x4c, 0x92, 0x34, 0xed, 0x00, 0x48, 0xcc, 0xd5, 0xbd, 0xa9, 0xfb, 0x22, 0x5f, 0xaa, 0x22, 0xac, 0x0f, 0x1e, 0xc6,
                0x7e, 0x20, 0x7a, 0x6b, 0xb2, 0x3f, 0xee, 0x86, 0xe4, 0x93, 0xb7, 0xb6, 0x32,
            ], // slot 1 waiver
            [
                0xa2, 0xda, 0x70, 0xf9, 0xb6, 0xdc, 0x3c, 0x37, 0x20, 0x41, 0xb6, 0xf9, 0xd0, 0x4b, 0xcd, 0x5e, 0xb5, 0xdd, 0x8b,
                0xfb, 0x07, 0xf9, 0xef, 0x22, 0x57, 0x8b, 0xc1, 0x4f, 0x45, 0xec, 0xde, 0x43,
            ], // slot 2 waiver
            [
                0x2e, 0x5c, 0x98, 0x26, 0x48, 0x86, 0x3b, 0xe9, 0x92, 0x43, 0x86, 0x42, 0x18, 0xc3, 0xa2, 0x0b, 0xd5, 0x93, 0x37,
                0x7e, 0x7b, 0x08, 0x7b, 0x4a, 0xf0, 0x3f, 0x1a, 0x79, 0x94, 0xd6, 0x5a, 0x47,
            ], // slot 3 waiver
            [
                0xcb, 0xa5, 0x97, 0xb7, 0x14, 0x87, 0x8f, 0xe1, 0xc5, 0x84, 0x42, 0xd3, 0x3f, 0x7d, 0xcb, 0x46, 0xc2, 0xa3, 0xf7,
                0x63, 0x31, 0x87, 0x5c, 0x4e, 0xd8, 0x63, 0xf7, 0x1b, 0x4d, 0x1c, 0x3f, 0xaa,
            ], // slot 4 waiver
            [
                0x09, 0x2f, 0xd2, 0x78, 0xc8, 0xd1, 0x94, 0x53, 0x12, 0xcd, 0xce, 0x6c, 0x50, 0xa9, 0x04, 0x94, 0xc4, 0x31, 0x77,
                0xb5, 0xb9, 0xd4, 0x12, 0x3e, 0x6d, 0x30, 0x89, 0xbc, 0x8c, 0x23, 0x11, 0xb1,
            ], // slot 5 run
            [
                0xea, 0x29, 0xcc, 0xa8, 0x1c, 0xd4, 0x25, 0x3f, 0x44, 0xd5, 0x76, 0x0c, 0x30, 0x64, 0xa4, 0x84, 0x9d, 0xbe, 0x0f,
                0x62, 0x02, 0x44, 0xb6, 0xf1, 0xb0, 0x6a, 0x16, 0xf8, 0x82, 0x15, 0xa3, 0x30,
            ], // slot 6 waiver
        ],
        review: [
            0x0f, 0x5b, 0xa9, 0x6c, 0x4d, 0xe0, 0xc9, 0x12, 0xcf, 0xba, 0x57, 0xf0, 0xf7, 0xe4, 0x31, 0x69, 0x55, 0xbd, 0xbc, 0x55,
            0x60, 0x95, 0xc3, 0x27, 0xc4, 0xd5, 0x7f, 0x60, 0x79, 0x5c, 0x60, 0x33,
        ],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyHeldDaV2,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0x51, 0x0b, 0x8c, 0x8d, 0xb3, 0x50, 0xc6, 0x0c, 0xab, 0xd8, 0xe5, 0x69, 0xac, 0x39, 0x6f, 0x2c, 0x44, 0xd9, 0x16,
                0x98, 0xc3, 0xa4, 0xa4, 0x57, 0xf6, 0xea, 0x26, 0x5f, 0x3e, 0xce, 0xff, 0x85,
            ], // slot 0 run
            [
                0xd0, 0x67, 0xc4, 0x0b, 0x55, 0xa0, 0x46, 0x42, 0x52, 0xde, 0xb0, 0x63, 0x61, 0x42, 0x96, 0x81, 0xbb, 0xc1, 0xb8,
                0xd2, 0x64, 0xdd, 0x3e, 0xb5, 0xab, 0xe3, 0x6f, 0x40, 0xe3, 0xe6, 0x6b, 0xd4,
            ], // slot 1 waiver
            [
                0x69, 0xc8, 0x7e, 0xec, 0xc5, 0xb8, 0xfe, 0xb1, 0xa1, 0x27, 0x98, 0x89, 0xa0, 0x1a, 0x58, 0x49, 0x18, 0xe4, 0x27,
                0x88, 0xed, 0xe5, 0x99, 0xe2, 0x8f, 0xf6, 0xde, 0xe5, 0x0b, 0x6b, 0x72, 0xdd,
            ], // slot 2 run
            [
                0x0e, 0xf9, 0xfc, 0x52, 0x70, 0x08, 0x0b, 0x77, 0x8d, 0x06, 0xa8, 0x26, 0x86, 0xc4, 0xc2, 0xc2, 0xfb, 0x55, 0xc1,
                0x5e, 0xa4, 0x8e, 0x03, 0xf0, 0xde, 0xd1, 0x23, 0x91, 0x25, 0x94, 0x18, 0x89,
            ], // slot 3 waiver
            [
                0x87, 0x5d, 0x11, 0x91, 0x79, 0xcd, 0xd2, 0x18, 0x90, 0x84, 0x9f, 0x22, 0x9f, 0xdf, 0x52, 0x38, 0x75, 0x69, 0x3d,
                0x27, 0x78, 0xc3, 0x2c, 0xcc, 0xe3, 0xc4, 0x2a, 0xfd, 0x29, 0xa4, 0x01, 0xa2,
            ], // slot 4 waiver
            [
                0x65, 0x50, 0xba, 0x94, 0xa2, 0x75, 0x32, 0xe6, 0xde, 0xe7, 0xd3, 0x7c, 0x32, 0x9f, 0x30, 0x23, 0x0e, 0x19, 0x88,
                0x64, 0x24, 0xc6, 0xb2, 0x2c, 0x0a, 0xc6, 0x9e, 0x9b, 0x80, 0x0e, 0x4d, 0x18,
            ], // slot 5 run
            [
                0x83, 0x21, 0xfb, 0xb4, 0xfa, 0x2b, 0x2b, 0x1b, 0x4d, 0x12, 0x33, 0x4b, 0xd0, 0x66, 0x82, 0x73, 0x0e, 0x3f, 0x97,
                0x8d, 0x39, 0x59, 0xe8, 0x9c, 0xb5, 0x7c, 0xb4, 0x22, 0xeb, 0xc0, 0x6a, 0x86,
            ], // slot 6 waiver
        ],
        review: [
            0xf4, 0x52, 0x39, 0xae, 0xc2, 0xaf, 0x4e, 0x22, 0xd9, 0xc3, 0x57, 0xbf, 0xe9, 0xe3, 0xb3, 0xf9, 0x1e, 0xad, 0x08, 0x3b,
            0x44, 0x6d, 0x8b, 0x0e, 0xd3, 0x2c, 0xe1, 0x5d, 0x63, 0xb0, 0xc7, 0x80,
        ],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedPublicCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0xa6, 0x09, 0x91, 0x32, 0xba, 0xd2, 0xf1, 0x0f, 0xb8, 0x74, 0x34, 0x7f, 0xe9, 0x4c, 0x26, 0xe3, 0xe7, 0xfc, 0x07,
                0xa6, 0xe1, 0x02, 0x66, 0x52, 0x23, 0x85, 0xd9, 0x20, 0x95, 0x83, 0x0f, 0x15,
            ], // slot 0 run
            [
                0xeb, 0x4c, 0x62, 0xba, 0x69, 0x7e, 0xc4, 0x98, 0x96, 0xc1, 0x96, 0xd8, 0x16, 0xf6, 0xa9, 0xd2, 0xe5, 0xc7, 0x77,
                0xc9, 0xb0, 0xd6, 0xc2, 0x51, 0x6e, 0xe0, 0xeb, 0x29, 0x04, 0x3d, 0x8f, 0xd3,
            ], // slot 1 waiver
            [
                0xac, 0x55, 0x38, 0x28, 0x01, 0xd8, 0xe6, 0x2d, 0x96, 0x39, 0x3f, 0xa9, 0xcd, 0xe3, 0x3b, 0x6a, 0xf7, 0x36, 0x6b,
                0x60, 0xaa, 0x03, 0xd7, 0x88, 0xa9, 0x17, 0x6d, 0x24, 0x9f, 0xd4, 0x9b, 0x78,
            ], // slot 2 waiver
            [
                0xb8, 0x95, 0x90, 0x95, 0xb7, 0x10, 0x25, 0x41, 0x8b, 0xcd, 0x0e, 0x58, 0x10, 0xc3, 0xea, 0x6b, 0x23, 0x80, 0xf5,
                0x94, 0xd7, 0x01, 0xe8, 0xd6, 0xae, 0x2c, 0x45, 0xfe, 0x55, 0xef, 0xd9, 0x3b,
            ], // slot 3 waiver
            [
                0x9d, 0x86, 0x18, 0xfe, 0x35, 0x0c, 0x73, 0x4a, 0xdf, 0x18, 0x24, 0x2e, 0x1b, 0xc5, 0xa3, 0x33, 0xb8, 0x79, 0x62,
                0x35, 0x66, 0xb0, 0xaa, 0x10, 0xe9, 0x66, 0x65, 0x54, 0x8a, 0xf4, 0x6a, 0xda,
            ], // slot 4 waiver
            [
                0x48, 0xd1, 0x80, 0x9d, 0x9e, 0x1f, 0xf8, 0x58, 0x9e, 0xd1, 0xe5, 0xae, 0xe6, 0xaa, 0x3f, 0x70, 0x25, 0x86, 0x26,
                0x30, 0x27, 0xf0, 0xfd, 0x4f, 0xfc, 0x4c, 0x0e, 0xb1, 0xb7, 0x1f, 0x9b, 0x07,
            ], // slot 5 waiver
            [
                0x2f, 0x98, 0xfa, 0x1d, 0x70, 0xba, 0x1d, 0x08, 0xb7, 0x6c, 0x64, 0x57, 0xe3, 0x7a, 0x21, 0x54, 0x4f, 0xd1, 0x6b,
                0xd0, 0xf6, 0x09, 0xd3, 0x32, 0xbf, 0xda, 0xfa, 0x5d, 0x0b, 0x38, 0x1f, 0x2b,
            ], // slot 6 waiver
        ],
        review: [
            0xd0, 0x41, 0x67, 0x80, 0xd1, 0x5d, 0xb3, 0xca, 0x88, 0x42, 0xb6, 0x6c, 0xf1, 0xfb, 0xa8, 0xe1, 0xce, 0xe4, 0xe4, 0xf9,
            0x02, 0x3f, 0x3c, 0xf8, 0x11, 0xfd, 0xba, 0x05, 0x6a, 0x6b, 0x6d, 0xdc,
        ],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedTirRouteV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0xc7, 0xa6, 0xcf, 0x2a, 0xee, 0x8a, 0xfa, 0x57, 0x37, 0x01, 0x95, 0x72, 0x6d, 0xa9, 0x69, 0xac, 0xa1, 0x80, 0xac,
                0xdb, 0x46, 0x97, 0xad, 0x93, 0x83, 0x20, 0x50, 0x28, 0xc3, 0xb2, 0x7b, 0x70,
            ], // slot 0 run
            [
                0x4b, 0xfc, 0x01, 0x6b, 0xc8, 0xff, 0xd9, 0x20, 0xc0, 0x89, 0x61, 0x2b, 0x73, 0x10, 0xe1, 0x69, 0x3d, 0x6a, 0xaa,
                0x94, 0xf9, 0x4e, 0xd5, 0xad, 0x3a, 0xaf, 0x7d, 0x35, 0x91, 0x4d, 0x28, 0xf6,
            ], // slot 1 waiver
            [
                0x01, 0xae, 0x14, 0x17, 0x9a, 0x43, 0xa9, 0x8d, 0x65, 0x3b, 0xd2, 0x08, 0x45, 0x21, 0x2d, 0x87, 0x52, 0xd1, 0x31,
                0xec, 0x50, 0x2b, 0x60, 0x87, 0x4c, 0x8f, 0xdf, 0x7e, 0x9a, 0x42, 0xa1, 0xbc,
            ], // slot 2 waiver
            [
                0x74, 0xb5, 0x28, 0x3c, 0x28, 0x13, 0x57, 0x0f, 0xbf, 0x6f, 0x50, 0xe9, 0x0e, 0x48, 0xa3, 0x96, 0x05, 0x5a, 0x1d,
                0xc6, 0xb1, 0xe2, 0x49, 0x32, 0x3e, 0xbe, 0x99, 0xbb, 0xe5, 0x87, 0x6d, 0x0c,
            ], // slot 3 waiver
            [
                0x52, 0xac, 0x98, 0xb4, 0x8a, 0x16, 0xec, 0xee, 0x59, 0xa2, 0x9e, 0x82, 0x10, 0x24, 0xfe, 0x3d, 0x1f, 0x32, 0xc3,
                0x4d, 0x0c, 0xe4, 0xcf, 0xde, 0x72, 0x29, 0xc5, 0x60, 0x28, 0x8e, 0xa8, 0xbf,
            ], // slot 4 waiver
            [
                0xb0, 0x0f, 0xda, 0x2c, 0x86, 0xa6, 0x87, 0x2a, 0xb0, 0x34, 0xd4, 0xd0, 0x78, 0x58, 0xbc, 0x06, 0x1f, 0x5f, 0xbe,
                0xfa, 0xec, 0xc8, 0x7f, 0xa3, 0xa5, 0x90, 0x28, 0x7a, 0xd0, 0xb6, 0xa1, 0x2b,
            ], // slot 5 waiver
            [
                0xd0, 0xe3, 0xa0, 0x3d, 0xf7, 0x1c, 0x3d, 0xe0, 0x64, 0x39, 0x65, 0xcd, 0x34, 0x86, 0x7a, 0x9d, 0xf2, 0x24, 0x75,
                0x4b, 0x6c, 0x64, 0x54, 0x08, 0xeb, 0xca, 0x85, 0x4b, 0x52, 0xec, 0xd2, 0x14,
            ], // slot 6 waiver
        ],
        review: [
            0x00, 0x38, 0x49, 0x31, 0xdb, 0xc6, 0x14, 0xf7, 0xd8, 0x02, 0x39, 0xcd, 0xc3, 0x9b, 0x61, 0x16, 0x9f, 0x46, 0x0e, 0x5e,
            0xcd, 0xd5, 0x53, 0x0d, 0x0f, 0x29, 0x72, 0x85, 0x62, 0x9e, 0x43, 0xcf,
        ],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::TirPublicConvictionV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0x8a, 0xe8, 0x17, 0xbf, 0xe7, 0x96, 0x72, 0x82, 0x1a, 0x7f, 0xcd, 0xac, 0x7c, 0x5f, 0xc2, 0x3a, 0x77, 0x6c, 0x16,
                0xb4, 0xc5, 0x75, 0x69, 0x35, 0x82, 0x69, 0x88, 0x5a, 0x31, 0x0e, 0xd3, 0x19,
            ], // slot 0 run
            [
                0x9b, 0xcb, 0x01, 0xaa, 0xe0, 0x5f, 0x17, 0x76, 0xbd, 0x77, 0x9e, 0xb2, 0x32, 0x86, 0x81, 0x44, 0x9a, 0xcb, 0x04,
                0xb1, 0x1a, 0x85, 0xbf, 0xf6, 0x56, 0xbc, 0x7a, 0x89, 0xe3, 0xe4, 0xd7, 0xb1,
            ], // slot 1 waiver
            [
                0x90, 0xb9, 0xee, 0xdd, 0x05, 0x85, 0x25, 0x4e, 0xb5, 0xc6, 0x22, 0x68, 0xcf, 0x3d, 0x7f, 0xa3, 0x42, 0xd8, 0x73,
                0x0e, 0xd9, 0x7f, 0xd9, 0xe8, 0x42, 0x83, 0x3c, 0xc3, 0xd7, 0xb7, 0x78, 0x96,
            ], // slot 2 waiver
            [
                0x67, 0xd4, 0xed, 0xfe, 0xfb, 0x01, 0x98, 0x65, 0xbf, 0x8f, 0xc5, 0x3d, 0x1f, 0x29, 0x37, 0x3e, 0x46, 0xdd, 0xc0,
                0xb6, 0x80, 0x2c, 0x61, 0x5b, 0x23, 0x69, 0x28, 0x1f, 0x65, 0xfd, 0x0b, 0xe0,
            ], // slot 3 waiver
            [
                0x2a, 0xe2, 0xb7, 0x29, 0x00, 0xc7, 0x1b, 0x33, 0x9f, 0x32, 0x5b, 0xdc, 0x95, 0x0a, 0x31, 0x6f, 0xa0, 0x61, 0x04,
                0x18, 0x4f, 0x59, 0x02, 0x4f, 0x10, 0x32, 0x58, 0x7e, 0x95, 0x20, 0x58, 0x90,
            ], // slot 4 waiver
            [
                0xc4, 0x37, 0x60, 0x39, 0x97, 0x03, 0x62, 0x46, 0x6c, 0xea, 0xcc, 0x21, 0xb2, 0x6e, 0x20, 0x74, 0x8d, 0x72, 0x57,
                0xa6, 0x97, 0x2d, 0x41, 0x1f, 0x0f, 0xc7, 0x8b, 0x0e, 0xb6, 0xcd, 0x09, 0x35,
            ], // slot 5 waiver
            [
                0xbc, 0x39, 0xed, 0x60, 0x73, 0xac, 0x75, 0x38, 0xbd, 0x00, 0x67, 0x00, 0xec, 0x33, 0xed, 0xa2, 0x65, 0x80, 0x29,
                0x84, 0x68, 0xb2, 0x75, 0x09, 0xa7, 0xba, 0x3f, 0x25, 0xc2, 0xd0, 0x13, 0x86,
            ], // slot 6 waiver
        ],
        review: [
            0xe0, 0x6d, 0xb7, 0x89, 0x46, 0xe6, 0x95, 0x5a, 0x18, 0x73, 0x86, 0xe4, 0xc4, 0xc9, 0x5e, 0x93, 0x47, 0x5b, 0xc5, 0x10,
            0xef, 0xa5, 0x73, 0x8e, 0x5b, 0x05, 0x3e, 0x52, 0xc3, 0xf4, 0x99, 0xb8,
        ],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::CourtCapacityV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0xfc, 0x62, 0x1b, 0xc4, 0xbb, 0x4a, 0x68, 0x8c, 0x18, 0x49, 0xeb, 0x5d, 0x2f, 0x3c, 0xc1, 0x61, 0x05, 0x3c, 0xe6,
                0xfb, 0x0f, 0xbf, 0x31, 0xcc, 0x70, 0x01, 0x2a, 0xde, 0xf5, 0x6f, 0xd6, 0xd9,
            ], // slot 0 run
            [
                0x8b, 0x89, 0xa2, 0xf3, 0xf8, 0x66, 0xe0, 0x63, 0x96, 0x8f, 0x59, 0xce, 0xbc, 0x35, 0x46, 0x08, 0x43, 0x97, 0x11,
                0xd6, 0x42, 0xbe, 0x2e, 0x51, 0x38, 0xb2, 0xf4, 0x46, 0x7d, 0x58, 0x0b, 0xb1,
            ], // slot 1 waiver
            [
                0x9f, 0x6b, 0xfa, 0x92, 0xa7, 0x8e, 0x20, 0xa4, 0x90, 0x38, 0x52, 0x93, 0xdf, 0x01, 0x42, 0xfc, 0x00, 0xe1, 0x77,
                0x27, 0x5b, 0x90, 0x53, 0xd8, 0x3b, 0x31, 0x32, 0x6f, 0x30, 0x31, 0x50, 0xd6,
            ], // slot 2 waiver
            [
                0xaf, 0xcb, 0xd7, 0xaa, 0x54, 0x31, 0x92, 0x02, 0x86, 0xae, 0x62, 0x88, 0x56, 0x66, 0xf8, 0x19, 0xc0, 0xc8, 0x15,
                0x7b, 0xe1, 0xff, 0xfe, 0x09, 0x36, 0xc7, 0xf7, 0xfc, 0x54, 0xa0, 0x96, 0xd8,
            ], // slot 3 waiver
            [
                0x19, 0x54, 0x79, 0x90, 0x7b, 0xcf, 0x16, 0x65, 0xc7, 0xb7, 0x8f, 0xf4, 0x39, 0x9c, 0x14, 0x11, 0x9f, 0x0d, 0x57,
                0x0b, 0x44, 0xd9, 0xc0, 0xf5, 0x7e, 0x61, 0x00, 0x40, 0x17, 0xf9, 0xd0, 0x32,
            ], // slot 4 waiver
            [
                0xf4, 0xea, 0xf9, 0x0c, 0xd6, 0x53, 0x12, 0x0f, 0xe6, 0x7b, 0x6a, 0xc6, 0xd6, 0xc9, 0x8a, 0xf1, 0x84, 0xc6, 0xad,
                0x9b, 0x37, 0x79, 0x80, 0xbb, 0x24, 0x9c, 0x27, 0x3a, 0xa4, 0xa4, 0x1a, 0x57,
            ], // slot 5 waiver
            [
                0x2f, 0x8f, 0x11, 0x3e, 0xe4, 0xff, 0x95, 0x06, 0x1d, 0x63, 0x3b, 0xd2, 0x80, 0x53, 0xf9, 0xd7, 0x73, 0x53, 0x44,
                0x14, 0x39, 0xe4, 0x8e, 0x00, 0x20, 0x7d, 0x78, 0x38, 0x5a, 0xf2, 0x32, 0xde,
            ], // slot 6 run
        ],
        review: [
            0xe6, 0xe1, 0x24, 0x2c, 0xb7, 0x48, 0x15, 0xb0, 0x1d, 0xbb, 0x60, 0x93, 0x43, 0xf6, 0x3c, 0xea, 0x09, 0xd4, 0x60, 0x00,
            0x35, 0xf8, 0xf6, 0xfd, 0x5f, 0x12, 0xd9, 0xcf, 0xba, 0x8c, 0x20, 0x9d,
        ],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::ClaimHardDeadlineV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0x2e, 0x76, 0x26, 0xdc, 0xce, 0xbe, 0x48, 0xed, 0xe7, 0x2e, 0xcc, 0xad, 0xb1, 0x84, 0x62, 0x17, 0x60, 0x04, 0xbf,
                0xbe, 0xa5, 0x86, 0xb2, 0x9b, 0x28, 0x27, 0x68, 0x0e, 0x88, 0x53, 0xeb, 0xb0,
            ], // slot 0 run
            [
                0x04, 0xa8, 0x2a, 0xdb, 0xf6, 0x26, 0x1e, 0xdf, 0x9a, 0x6f, 0xfe, 0xc6, 0x4b, 0x1c, 0x2e, 0xda, 0x82, 0x0c, 0x1a,
                0x4d, 0xde, 0xec, 0xc7, 0x48, 0x7a, 0xcb, 0x1f, 0x4c, 0x69, 0xd0, 0x29, 0xc0,
            ], // slot 1 waiver
            [
                0x95, 0x83, 0x8a, 0x64, 0x8a, 0xd3, 0x09, 0x29, 0x4b, 0xdd, 0x19, 0x8e, 0x2a, 0xd4, 0x20, 0x3e, 0x2c, 0xe8, 0x92,
                0x5c, 0xc2, 0xbc, 0x87, 0x80, 0x69, 0x23, 0x5c, 0xbf, 0x61, 0x86, 0x22, 0x28,
            ], // slot 2 waiver
            [
                0xfb, 0x42, 0x7d, 0x7e, 0x73, 0xb2, 0x17, 0x5e, 0x0c, 0x40, 0x71, 0xda, 0xd4, 0xde, 0xfd, 0x64, 0x74, 0x58, 0x5e,
                0x04, 0x6c, 0x35, 0x56, 0x82, 0x27, 0x01, 0x30, 0x6d, 0xe6, 0xf5, 0x7b, 0x71,
            ], // slot 3 waiver
        ],
        review: [
            0x81, 0x22, 0xd9, 0x47, 0xd2, 0x60, 0x64, 0x6e, 0xc3, 0xd2, 0x93, 0xed, 0xa6, 0xd7, 0xe5, 0xb2, 0x61, 0x53, 0x6c, 0xbd,
            0x51, 0xf5, 0x40, 0x8c, 0xb1, 0xc7, 0x27, 0x55, 0x2b, 0x65, 0x17, 0x2a,
        ],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::G14RouteRequiredV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0xbf, 0xb8, 0x09, 0x08, 0x32, 0x19, 0xfa, 0x7d, 0xd5, 0xd1, 0x98, 0x3a, 0x1a, 0x53, 0x61, 0x29, 0x07, 0x33, 0xa0,
                0xa1, 0x7a, 0x02, 0x31, 0x9b, 0xcd, 0xae, 0x84, 0x7a, 0x98, 0xb0, 0xc3, 0x61,
            ], // slot 0 run
            [
                0x3d, 0xfe, 0xa5, 0xa3, 0x39, 0x22, 0xb4, 0x3e, 0x5c, 0x59, 0xa6, 0xb9, 0x78, 0xed, 0x0f, 0xda, 0x86, 0x20, 0x1c,
                0x45, 0x86, 0x78, 0x6a, 0xee, 0xcb, 0x2d, 0xd8, 0x25, 0xb1, 0x93, 0xab, 0x10,
            ], // slot 1 run
            [
                0x07, 0xc0, 0x9d, 0x8d, 0x29, 0xc9, 0xb9, 0x34, 0x06, 0x6b, 0x45, 0x85, 0x74, 0x9c, 0x8c, 0xc3, 0x82, 0xb4, 0x59,
                0xc8, 0x95, 0x15, 0x3c, 0x27, 0x2c, 0x0d, 0x64, 0x91, 0xff, 0x67, 0x93, 0x4c,
            ], // slot 2 waiver
            [
                0xa8, 0x0c, 0x5e, 0x5d, 0x8f, 0x11, 0x17, 0x35, 0x7f, 0x12, 0x17, 0x48, 0x84, 0x31, 0xde, 0x39, 0xc9, 0xab, 0x77,
                0x60, 0x77, 0xeb, 0x87, 0xba, 0x06, 0x86, 0x00, 0x13, 0x1c, 0x5e, 0xad, 0x8c,
            ], // slot 3 waiver
        ],
        review: [
            0x68, 0x75, 0x79, 0x46, 0x3f, 0xdd, 0xd4, 0x51, 0xbd, 0x3b, 0xde, 0xa7, 0xa9, 0xe4, 0x26, 0xb7, 0x7a, 0x48, 0xaf, 0xec,
            0x10, 0xe5, 0x7a, 0x72, 0x06, 0xf3, 0x4a, 0x66, 0xa3, 0x59, 0x4f, 0x58,
        ],
    },
];

/// The kernel-route records (complete ones only).
pub const PALW_T12_17000_KERNEL_ROUTE_RECORDS_V1: &[crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1] = &[
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0x2e, 0x88, 0xb5, 0x04, 0x06, 0x5b, 0xd1, 0x93, 0x3d, 0x55, 0xf3, 0xf2, 0xfa, 0x5a, 0x9b, 0x9d, 0xaf, 0x50, 0x80,
                0x81, 0xa3, 0x08, 0xe9, 0x41, 0x20, 0x35, 0x40, 0xbd, 0x66, 0xc9, 0x0a, 0x67,
            ], // slot 0 run
            [
                0x66, 0xce, 0x0d, 0xfa, 0x6f, 0x16, 0xa5, 0x68, 0xe5, 0x1d, 0xbf, 0xf9, 0x6c, 0x9d, 0xfc, 0xf6, 0x6e, 0x53, 0x50,
                0x52, 0x74, 0xe5, 0xf8, 0x47, 0x21, 0x5e, 0xf9, 0x4c, 0x69, 0x48, 0x02, 0x01,
            ], // slot 1 waiver
            [
                0xaa, 0xc4, 0x46, 0xda, 0x48, 0x60, 0x35, 0x14, 0x9a, 0xba, 0x1a, 0x2b, 0x81, 0x02, 0x8b, 0x57, 0xb4, 0x26, 0x4f,
                0xc5, 0xab, 0x83, 0x88, 0x51, 0xd6, 0x96, 0x2a, 0x67, 0x04, 0x9c, 0x2c, 0xdb,
            ], // slot 2 run
            [
                0x96, 0x6b, 0x71, 0x35, 0xac, 0xff, 0x50, 0xf3, 0xf1, 0x0b, 0x24, 0xa0, 0x8e, 0x19, 0xd5, 0x35, 0x46, 0x04, 0x5a,
                0x66, 0xc1, 0x21, 0xf0, 0x0a, 0x90, 0x1a, 0x05, 0x40, 0x9e, 0x59, 0x2d, 0x93,
            ], // slot 3 waiver
            [
                0x78, 0x20, 0x00, 0x91, 0x8d, 0x75, 0x21, 0x4b, 0x0b, 0x3b, 0xfc, 0xf2, 0xf8, 0xdf, 0x3b, 0xa7, 0x84, 0x43, 0x94,
                0x7d, 0x63, 0x71, 0x5c, 0xe1, 0x4c, 0x64, 0xc3, 0xbb, 0xd9, 0x14, 0xf0, 0x90,
            ], // slot 4 waiver
            [
                0x1e, 0x62, 0x0e, 0xe3, 0x97, 0x8c, 0x74, 0x90, 0xd7, 0x74, 0x44, 0x40, 0x9e, 0x60, 0x31, 0x28, 0x22, 0x0c, 0x39,
                0x8e, 0x8e, 0xaa, 0x2a, 0x89, 0x56, 0x75, 0xc3, 0x79, 0x0e, 0x92, 0x31, 0xfd,
            ], // slot 5 waiver
            [
                0xd0, 0xe1, 0xa7, 0xe0, 0xe5, 0xfe, 0xd0, 0x0b, 0xab, 0x2e, 0x2a, 0x34, 0x3c, 0x58, 0x3f, 0xb3, 0x89, 0xca, 0xad,
                0x87, 0x82, 0x3a, 0x83, 0x78, 0x7c, 0x4d, 0xb9, 0xa3, 0x26, 0x8b, 0x4b, 0xa1,
            ], // slot 6 waiver
            [
                0xd2, 0x95, 0x98, 0x16, 0x13, 0xc0, 0xa1, 0x04, 0xee, 0x31, 0xcf, 0x89, 0x42, 0x17, 0xb5, 0x81, 0x52, 0xf4, 0xd6,
                0xf6, 0x42, 0xc3, 0x08, 0xc3, 0x82, 0xa7, 0x1a, 0xec, 0x25, 0x0b, 0x10, 0xc1,
            ], // slot 7 waiver
        ],
        review: [
            0xc6, 0x5f, 0xe7, 0xb3, 0x9a, 0xea, 0xe9, 0xed, 0xd2, 0xfd, 0x5d, 0xfa, 0x99, 0x1c, 0x1e, 0x79, 0xb6, 0x93, 0xaf, 0x30,
            0xc8, 0x77, 0x24, 0x30, 0xc9, 0x8b, 0x84, 0x28, 0x23, 0x71, 0xb6, 0xd8,
        ],
    },
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProviderCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0xe0, 0x01, 0x53, 0xcb, 0xe8, 0x0d, 0xdb, 0x9d, 0xeb, 0x57, 0x3a, 0xe4, 0x93, 0xdc, 0x61, 0x04, 0x02, 0x3e, 0x67,
                0x14, 0x61, 0x3b, 0xbf, 0xea, 0xeb, 0xad, 0x15, 0xdd, 0x6a, 0xb4, 0xf3, 0x4c,
            ], // slot 0 run
            [
                0x63, 0x0e, 0x6b, 0xdc, 0x9a, 0xe9, 0x72, 0xcd, 0x3f, 0x5f, 0x37, 0xc5, 0xa3, 0x03, 0x62, 0xf7, 0xa2, 0x50, 0xb4,
                0x5a, 0xa7, 0xb2, 0x74, 0x5d, 0x71, 0x41, 0xe5, 0xa2, 0x75, 0xf5, 0x4a, 0x8e,
            ], // slot 1 waiver
            [
                0xb5, 0xa0, 0xeb, 0x3b, 0xbc, 0x5f, 0x00, 0x03, 0xc1, 0xee, 0x5a, 0xac, 0x62, 0x10, 0x69, 0x89, 0x48, 0x5a, 0x55,
                0xee, 0x76, 0x28, 0x21, 0x6c, 0x3b, 0x0a, 0x6d, 0x40, 0x70, 0x2b, 0xa6, 0xaf,
            ], // slot 2 waiver
            [
                0x0a, 0x0b, 0x00, 0x94, 0x36, 0xce, 0x6e, 0x99, 0x52, 0x26, 0xe7, 0xb7, 0x1f, 0x73, 0x28, 0xea, 0xad, 0x42, 0x21,
                0xb8, 0x47, 0x89, 0x3a, 0xd4, 0x13, 0xba, 0xc1, 0x14, 0x8a, 0xd4, 0x13, 0x3e,
            ], // slot 3 waiver
        ],
        review: [
            0x20, 0xf2, 0xf2, 0x7a, 0x2d, 0x37, 0x8e, 0xf7, 0xea, 0xb3, 0x11, 0xb7, 0x05, 0x0b, 0x61, 0xd6, 0x6d, 0x39, 0x18, 0x9b,
            0xf2, 0x97, 0x75, 0xa4, 0xb0, 0x72, 0x7b, 0xa3, 0x6c, 0x26, 0x22, 0x1c,
        ],
    },
];

/// The rfc0018 records (complete ones only).
pub const PALW_T12_17000_RFC0018_RECORDS_V1: &[crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1] =
    &[crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1 {
        version: crate::palw_rfc0018_v1::PALW_RFC0018_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_rfc0018_v1::PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0xda, 0xe0, 0x7a, 0x7b, 0xbe, 0xe9, 0x72, 0x3a, 0x10, 0xbc, 0xe8, 0xbf, 0x46, 0x58, 0xf0, 0x53, 0xb3, 0x40, 0x62,
                0xc6, 0xcd, 0x27, 0xc5, 0xa3, 0x54, 0x9b, 0x5b, 0x6c, 0x43, 0x1f, 0xbd, 0xf5,
            ], // slot 0 waiver
            [
                0x72, 0x19, 0xa7, 0xa4, 0x21, 0x73, 0xfe, 0xfa, 0x69, 0x58, 0xd5, 0x4c, 0xe7, 0x99, 0x03, 0x41, 0x77, 0x96, 0x36,
                0xb9, 0x76, 0x38, 0x22, 0x98, 0xbe, 0x69, 0xe2, 0x1e, 0x92, 0xd2, 0x8b, 0xb7,
            ], // slot 1 run
            [
                0x28, 0x49, 0x62, 0xf3, 0xf6, 0x56, 0x24, 0xb2, 0x6d, 0x44, 0x26, 0x87, 0x08, 0x51, 0xe2, 0xeb, 0xb7, 0xe2, 0x23,
                0xf2, 0x58, 0x7b, 0x54, 0xae, 0x6c, 0xe1, 0xbb, 0x37, 0xe6, 0x8f, 0x2c, 0x87,
            ], // slot 2 waiver
            [
                0x88, 0xb2, 0xe8, 0x31, 0xfc, 0x59, 0xed, 0xf4, 0x8d, 0x9c, 0x98, 0xe7, 0x78, 0xb9, 0x6b, 0x08, 0x36, 0x83, 0xd7,
                0xd8, 0x85, 0xb8, 0xbc, 0x03, 0x03, 0x3d, 0x48, 0x59, 0x8d, 0xb5, 0xbe, 0xaa,
            ], // slot 3 waiver
            [
                0x8b, 0x76, 0x24, 0x6b, 0x4f, 0xf0, 0x3d, 0xf0, 0x20, 0x16, 0xc5, 0x41, 0x2d, 0xec, 0x28, 0x04, 0x0d, 0xdc, 0x18,
                0xe0, 0xfb, 0xed, 0xb6, 0xd2, 0x34, 0xe0, 0x1a, 0xd6, 0x46, 0xa1, 0xb3, 0x07,
            ], // slot 4 waiver
            [
                0x22, 0xb7, 0x8d, 0xc5, 0x0d, 0xc6, 0x02, 0xd9, 0xb4, 0xed, 0xdd, 0x9c, 0xae, 0x2d, 0x80, 0x83, 0x2c, 0xaf, 0x87,
                0x5a, 0x32, 0x88, 0xf8, 0xda, 0xc4, 0x93, 0xd5, 0x2e, 0x3a, 0x90, 0x20, 0x86,
            ], // slot 5 waiver
        ],
        review: [
            0x1e, 0x9d, 0x57, 0xab, 0xee, 0xfd, 0x42, 0x8b, 0x7f, 0x6d, 0x9a, 0xa3, 0x66, 0x84, 0x4d, 0x51, 0xb3, 0xa1, 0x77, 0x8a,
            0x73, 0xe5, 0xfe, 0xf4, 0x64, 0x22, 0x51, 0x67, 0x5a, 0x44, 0xc9, 0xb3,
        ],
    }];

/// The frame cost record for FpSelection: the lane VAL estimate (ProvisionalEstimate) and the agent review. Emitted only
/// with the final evidence run's candidate commit as its source (a zero commit is refused, never pinned).
pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] =
    &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {
        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        measurement: [
            0xc5, 0xaa, 0x33, 0xae, 0x41, 0x09, 0x2f, 0xc6, 0x18, 0x98, 0xfb, 0xef, 0x53, 0x02, 0xf2, 0x29, 0x4f, 0x5b, 0xc8, 0x9c,
            0x37, 0x4b, 0x82, 0x50, 0x4b, 0xb6, 0xfd, 0x92, 0xc1, 0x6d, 0x5c, 0x5a,
        ],
        review: [
            0xc4, 0x27, 0x54, 0xa2, 0x0c, 0x28, 0x97, 0xef, 0x06, 0x64, 0xf6, 0x05, 0x2a, 0x42, 0x20, 0x2a, 0x15, 0x0b, 0x49, 0x7f,
            0xef, 0x7a, 0x48, 0xae, 0x52, 0x0e, 0x7f, 0xeb, 0x79, 0x97, 0x30, 0xda,
        ],
        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,
    }];

/// The model-tier record: c 300, kappa 1000 (user decision 2026-10-27), catalog edition 0 (empty) as the tip.
pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] =
    &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1 {
        version: crate::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        registered_permille: 300,
        registered_cap_permille: 1000,
        catalog_tip_edition: 0,
        catalog_tip_root: crate::Hash64::from_bytes([
            0xc9, 0x4d, 0xb3, 0x54, 0x2b, 0x1e, 0x0b, 0xe5, 0x8a, 0xd1, 0xda, 0x59, 0xb3, 0x9f, 0xab, 0xf3, 0x7b, 0x65, 0x0f, 0x6c,
            0x3e, 0x22, 0x34, 0x8e, 0x06, 0xcb, 0x2b, 0xd6, 0xb1, 0x21, 0x61, 0x43, 0xbe, 0x35, 0x19, 0xa8, 0x0b, 0xbd, 0x68, 0x57,
            0x8d, 0x62, 0xda, 0xf2, 0xd2, 0x03, 0xb7, 0xa3, 0xdb, 0x96, 0xe6, 0xd1, 0x0c, 0x95, 0xa3, 0x24, 0xce, 0xe8, 0xef, 0x9d,
            0x47, 0xf7, 0x2b, 0x6a,
        ]),
        source_commit: [
            0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2,
        ],
        evidence: &[
            [
                0xc5, 0x42, 0xa0, 0x43, 0xc4, 0x2c, 0x75, 0x7b, 0x97, 0xe4, 0x44, 0x74, 0x02, 0x6d, 0x77, 0x73, 0x6b, 0xdd, 0x44,
                0x06, 0x80, 0xd7, 0xb5, 0xc0, 0xd3, 0xfc, 0x54, 0x36, 0x2e, 0x61, 0x73, 0xd4,
            ], // slot 0 waiver
            [
                0x08, 0xfc, 0x8e, 0x2d, 0x4b, 0x0e, 0x5e, 0x71, 0x76, 0x9a, 0x64, 0x28, 0x38, 0x6d, 0x50, 0x3c, 0xc3, 0x1e, 0xc4,
                0x87, 0x72, 0x1e, 0x34, 0x75, 0xc9, 0x95, 0x16, 0x3c, 0x11, 0x18, 0xc5, 0xdd,
            ], // slot 1 run
            [
                0x29, 0x11, 0xa2, 0x01, 0x9b, 0x14, 0x68, 0x0a, 0x3d, 0xa7, 0xcb, 0xe6, 0x4c, 0xb3, 0x3f, 0x44, 0x11, 0x9e, 0xd9,
                0xc1, 0x62, 0x8c, 0xd4, 0x63, 0xe9, 0xfa, 0x01, 0xf0, 0x95, 0xe2, 0xfc, 0x62,
            ], // slot 2 run
            [
                0x4b, 0xcf, 0xc3, 0x93, 0x49, 0xb0, 0x0a, 0x2e, 0xa3, 0x94, 0x70, 0x02, 0xde, 0xfe, 0x99, 0x45, 0x69, 0xd1, 0xdb,
                0x99, 0x01, 0x34, 0x55, 0x76, 0x4b, 0x82, 0x37, 0x62, 0x02, 0x6f, 0x9d, 0x54,
            ], // slot 3 waiver
            [
                0x19, 0x60, 0x48, 0xd1, 0xe0, 0x6d, 0x85, 0x44, 0x22, 0x07, 0x58, 0xf2, 0x3e, 0xdb, 0x8a, 0x19, 0x04, 0x6e, 0x4c,
                0xe6, 0xe0, 0x3b, 0x29, 0x21, 0x6f, 0x26, 0x3a, 0x94, 0x2b, 0x18, 0xd9, 0xe7,
            ], // slot 4 waiver
            [
                0xc2, 0xd9, 0x89, 0xdb, 0x3e, 0xd3, 0x05, 0x2b, 0x8e, 0xf1, 0xd8, 0x4e, 0xa6, 0x1a, 0x4d, 0xb8, 0xff, 0x10, 0x80,
                0xe0, 0x09, 0xc4, 0x79, 0xda, 0xa4, 0x99, 0xba, 0xd6, 0x86, 0x5d, 0x34, 0x5d,
            ], // slot 5 waiver
            [
                0xcf, 0x33, 0xdc, 0xe9, 0x38, 0x30, 0x1a, 0x5e, 0xd8, 0xe5, 0xb6, 0x21, 0x8c, 0xfe, 0x2e, 0x29, 0xbe, 0xe6, 0x95,
                0x26, 0xaa, 0xe7, 0xb7, 0x08, 0xbe, 0xaa, 0x48, 0x1c, 0x4d, 0x5b, 0x7a, 0xe9,
            ], // slot 6 waiver
            [
                0xd9, 0xa7, 0xb4, 0x13, 0x38, 0xc2, 0x2d, 0xd4, 0xf0, 0xd8, 0xc0, 0x00, 0xad, 0x24, 0x51, 0x4f, 0xef, 0x3a, 0x65,
                0x51, 0xb8, 0x7c, 0x8d, 0x04, 0x73, 0x52, 0x19, 0xc5, 0x49, 0x5d, 0x02, 0x91,
            ], // slot 7 waiver
        ],
        review: [
            0xd8, 0xf2, 0x2c, 0xf0, 0xb9, 0x3e, 0x1a, 0xcc, 0x12, 0xa6, 0x79, 0x4a, 0x41, 0x91, 0x7f, 0x94, 0x1c, 0x01, 0x6b, 0x73,
            0xba, 0x54, 0x82, 0x72, 0xdd, 0x2c, 0x3e, 0x19, 0x4f, 0xbc, 0x40, 0x8d,
        ],
    }];
