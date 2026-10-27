//! **GENERATED** by `scripts/release/gen_t12_17000_records.py` (lane ACT2) — do not edit by hand.
//! testnet-12 G14 flag-day arm-readiness records and waivers (F1 kinds only: drill skipped, E2E not run, provisional
//! estimate with its value, Claude-agent review, user decision). A record is emitted only when EVERY slot is a permitted
//! waiver or a run; leftover slots (no permitted kind) are listed below and keep their fence refused. A waiver lifts no
//! ordering, value, measurement, wiring or open-blocker refusal.
//!
//! Leftover slots:
//! - g14 palw_legacy_public_filer_v1 slot 0
//! - g14 palw_legacy_public_filer_v1 slot 3
//! - g14 palw_legacy_public_filer_v1 slot 4
//! - g14 palw_legacy_public_filer_v1 slot 5
//! - g14 palw_legacy_held_da_v2 slot 0
//! - g14 palw_legacy_held_da_v2 slot 2
//! - g14 palw_legacy_held_da_v2 slot 3
//! - g14 palw_legacy_held_da_v2 slot 4
//! - g14 palw_legacy_held_da_v2 slot 5
//! - g14 palw_staged_public_court_v1 slot 0
//! - g14 palw_staged_public_court_v1 slot 1
//! - g14 palw_staged_public_court_v1 slot 2
//! - g14 palw_staged_public_court_v1 slot 3
//! - g14 palw_staged_public_court_v1 slot 4
//! - g14 palw_staged_public_court_v1 slot 5
//! - g14 palw_staged_public_court_v1 slot 6
//! - g14 palw_staged_tir_route_v1 slot 0
//! - g14 palw_staged_tir_route_v1 slot 1
//! - g14 palw_staged_tir_route_v1 slot 2
//! - g14 palw_staged_tir_route_v1 slot 3
//! - g14 palw_staged_tir_route_v1 slot 4
//! - g14 palw_staged_tir_route_v1 slot 5
//! - g14 palw_staged_tir_route_v1 slot 6
//! - g14 palw_tir_public_conviction_v1 slot 0
//! - g14 palw_tir_public_conviction_v1 slot 1
//! - g14 palw_tir_public_conviction_v1 slot 2
//! - g14 palw_tir_public_conviction_v1 slot 3
//! - g14 palw_tir_public_conviction_v1 slot 4
//! - g14 palw_tir_public_conviction_v1 slot 5
//! - g14 palw_tir_public_conviction_v1 slot 6
//! - g14 palw_court_capacity_v1 slot 0
//! - g14 palw_court_capacity_v1 slot 6
//! - g14 palw_claim_hard_deadline_v1 slot 0
//! - g14 palw_g14_route_required_v1 slot 0
//! - g14 palw_g14_route_required_v1 slot 1
//! - kernel-route palw_probabilistic_constraints_v1 slot 0
//! - kernel-route palw_probabilistic_constraints_v1 slot 2
//! - kernel-route palw_probabilistic_constraints_v1 slot 4
//! - kernel-route palw_probabilistic_constraints_v1 slot 5
//! - kernel-route palw_probabilistic_constraints_v1 slot 6
//! - kernel-route palw_provider_court_v1 slot 0
//! - kernel-route palw_provider_court_v1 slot 2
//! - rfc0018 palw_independently_work_certified_v1 slot 1
//! - rfc0018 palw_independently_work_certified_v1 slot 3
//! - rfc0018 palw_independently_work_certified_v1 slot 4
//! - model-tier palw_model_tier_v1 slot 1
//! - model-tier palw_model_tier_v1 slot 2
//! - model-tier palw_model_tier_v1 slot 3
//! - model-tier palw_model_tier_v1 slot 4
//! - model-tier palw_model_tier_v1 slot 5
//! - model-tier palw_model_tier_v1 slot 6
//! - model-tier palw_model_tier_v1 slot 7
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
];

/// The kernel-route records (complete ones only).
pub const PALW_T12_17000_KERNEL_ROUTE_RECORDS_V1: &[crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1] = &[
];

/// The rfc0018 records (complete ones only).
pub const PALW_T12_17000_RFC0018_RECORDS_V1: &[crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1] = &[
];

/// The frame cost record for FpSelection: the lane VAL estimate (ProvisionalEstimate) and the agent review.
pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[
    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {
        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        measurement: [0xc5, 0xaa, 0x33, 0xae, 0x41, 0x09, 0x2f, 0xc6, 0x18, 0x98, 0xfb, 0xef, 0x53, 0x02, 0xf2, 0x29, 0x4f, 0x5b, 0xc8, 0x9c, 0x37, 0x4b, 0x82, 0x50, 0x4b, 0xb6, 0xfd, 0x92, 0xc1, 0x6d, 0x5c, 0x5a],
        review: [0xc4, 0x27, 0x54, 0xa2, 0x0c, 0x28, 0x97, 0xef, 0x06, 0x64, 0xf6, 0x05, 0x2a, 0x42, 0x20, 0x2a, 0x15, 0x0b, 0x49, 0x7f, 0xef, 0x7a, 0x48, 0xae, 0x52, 0x0e, 0x7f, 0xeb, 0x79, 0x97, 0x30, 0xda],
        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,
    },
];

/// The model-tier record: not emitted while slots [1, 2, 3, 4, 5, 6, 7] are leftover.
pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[];
