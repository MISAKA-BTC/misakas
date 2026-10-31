//! **GENERATED** by `scripts/release/gen_t12_18400_records.py` — do not edit by hand.
//! DAA 18,400 height-only rebind, user 2026-10-30. Historical 17,400 records/logs remain unchanged.
//! Historical passing suites name their original source commit; these are NOT fresh 18,400-candidate runs.
//! Final same-candidate tests, source manifest, boundary/startup validation and deployment remain separate gates.
//! testnet-12 G14 flag-day arm-readiness records and waivers (F1 kinds only: drill skipped, E2E not run, provisional
//! estimate with its value, Claude-agent review, user decision). A record is emitted only when EVERY slot is a permitted
//! waiver or a run; leftover slots (no permitted kind) are listed below and keep their fence refused. A waiver lifts no
//! ordering, value, measurement, wiring or open-blocker refusal.
//!
//! Leftover slots:
use crate::palw_arming_waiver_v1::{PALW_ARMING_WAIVER_REVIEW_V1, PalwArmingWaiverKindV1, PalwArmingWaiverV1};

/// Every pinned waiver.
#[rustfmt::skip]
pub const PALW_T12_18400_WAIVERS_V1: &[PalwArmingWaiverV1] = &[
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "hard-deadline envelope = the X3 decided policy (Delta 2,016 DAA, inside trace retention minus the disclosure window); Panel capacity under the Valid lock = the lane VAL Panel=1 schedule (2 packets per 833-DAA interval)",
        reason: "estimate from the pinned X3 terms and the lane VAL V1 schedule (user 2026-10-22 estimates); not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "reservation budget 34 sessions (design constant), court session cap 42 (V11), answer bytes within the 34,000 B one-move budget; descent rounds bounded by the same budget",
        reason: "estimate from design constants and the user-approved V11 estimate (2026-10-26); not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "maximum legal delay = the X3 hard deadline (Delta 2,016 DAA); capacity under contention = V11 cap 42 sessions with the 42-DAA turn deadline",
        reason: "estimate from the pinned X3 decided terms and the user-approved V11 estimate (user 2026-10-22 / 2026-10-26); not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run. No node E2E of a lying FpConstraint claim on a TIR class with a TFP1 frame across a restart or reorg (CI-TRIAGE / lane FPCP addendum 3): in-process the same lie without restart/reorg is convicted (6 fresh_public_bond_service_*constraint* tests pass); the restart/reorg harness failure is its recovery mode not answering input; in the release an A16 unframed job is refused at admission; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "maximum legal delay = the X3 hard deadline (Delta 2,016 DAA); capacity under contention = V11 cap 42 sessions with the 42-DAA turn deadline",
        reason: "estimate from the pinned X3 decided terms and the user-approved V11 estimate (user 2026-10-22 / 2026-10-26); not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "maximum legal delay = the X3 hard deadline (Delta 2,016 DAA); capacity under contention = V11 cap 42 sessions with the 42-DAA turn deadline",
        reason: "estimate from the pinned X3 decided terms and the user-approved V11 estimate (user 2026-10-22 / 2026-10-26); not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1: cap 42, session weight 13, op_work [5,13,1,11], rho_court 3,125 sompi/DAA/unit, block budget work 13, one-move budget work 5 / 34,000 B",
        reason: "user approval 2026-10-26 of the V11 estimate as the value; only the one-move class was measured (M-CAP arm-legacy-2026-10-27, p95 141.7 us under load ~11); the all-class match and the no-load re-measurement are waived; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "rho_court 3,125 sompi/DAA/unit, session weight 13",
        reason: "lane VAL V11 estimate approved by the user 2026-10-26; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "PALW_X3_DECIDED_DELTA_TERMS_V1 (Delta 2,016 DAA) and c_retain 36,058 sompi/DAA (PALW_X3_RETENTION_COST_PER_DAA_SOMPI_V1)",
        reason: "lane P3 V10 decided 2026-10-22; c_retain = ceil(2,250,000 B x 50 sompi/B / 3,120 DAA); estimates, user 2026-10-22; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "9B-8k: producer exec ~440 s / ~56 GB, verifier ~41 GB, court ~710 s / ~19 GB (x4.5 of the Qwen3.5-2B measurement); 2M: not estimated - in this release only the wide128 class is attested, so no 9B class can be registered",
        reason: "provisional value derived from KCONV measurements on OTHER models (claude/kconv 0d98e8e6c, kconv-report.md): Dolphin 8k exec 115 s / 6.47 GB, verifier 4.24 GB, court 132 s / 2.22 GB; Qwen3.5-2B exec 97.6 s / 12.4 GB, verifier 9.19 GB, court 158 s / 4.22 GB; scaled linearly to parameter count; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "a late single fault is localized within the V11 court budgets (cap 42, 34,000 B one-move) and the X3 hard deadline (Delta 2,016 DAA); concurrent disputes bounded by cap 42",
        reason: "estimate from the user-approved V11 estimate and the pinned X3 terms; not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "the user decided to arm without this node E2E; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "lease reservation >= 100 MSK, challenge bond 10 MSK, challenge fee 1 MSK, response window 20 DAA, >= 2 providers, <= 8 open challenges (palw_provider_court_v1 interim constants)",
        reason: "the interim constants in palw_provider_court_v1, pinned as estimates (user 2026-10-22); not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "Gate C activates at testnet-12 DAA 18,400 with the G14 list",
        reason: "user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "certification m/k = 75/41; the drawn verifiers bear the re-executions",
        reason: "user approval 2026-10-26; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "U-6: no veto (m/k 75/41, veto 0)",
        reason: "user ruling 2026-10-27 (\"推奨どおり\", relayed by the coordinator); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "U-2/U-3: the issuance-leg hold is paid only after SecurityCertified; free-prompt gets no coinbase carve before certification (code: hold_issuance_reward_at_final_v1, palw_rfc0018_v1 free-prompt refusal)",
        reason: "user ruling 2026-10-27 (\"推奨どおり\", relayed by the coordinator); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::DrillSkipped,
        value: "",
        reason: "the user decided to skip the drill; user decision 2026-10-27; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "c = 300 permille, kappa = 1000 permille from DAA 18,400; catalog edition 0 empty",
        reason: "user decision 2026-10-27; c is the frozen premise (PRINCIPLES 9); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "lane V-REVIEW (read-only Claude agent) at fbd1998dd6e495562cbcf79500872e5bd334d7f2: PASS - the preset is genesis_only(Vec::new()) (full_activation_v1.rs:464-466), catalog-tip refusal palw_model_tier_v1.rs:465-470, KCAT 1792ea6aa not in the release",
        reason: "review: edition 0 only, none adds a model at 1.0 / new local LLM after the fence (independent review, user-approved reviewer); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "lane V-REVIEW (read-only Claude agent) at fbd1998dd6e495562cbcf79500872e5bd334d7f2: PASS - the preset is genesis_only(Vec::new()) (full_activation_v1.rs:464-466), catalog-tip refusal palw_model_tier_v1.rs:465-470, KCAT 1792ea6aa not in the release",
        reason: "review: edition 0 only, none adds a model at 1.0 / new local LLM after the fence (independent review, user-approved reviewer); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::E2ENotRun,
        value: "",
        reason: "Lead decision 2026-10-27 + user 10-26 drill skip; not recorded as run; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::UserDecision,
        value: "readiness M1-M5: satisfied by the ACT2 report (edition 0 only, nothing added after the fence)",
        reason: "user ruling 2026-10-27 (\"推奨どおり\", relayed by the coordinator); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::ProvisionalEstimate,
        value: "PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 (inclusion margin 4 DAA, rents 1, acquisition 1 DAA, collection 8 DAA, reorg retention 360 DAA)",
        reason: "lane VAL estimate, user approval 2026-10-26 of the estimates as values; not measured; Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::AgentReview,
        value: "docs/design/palw/integration/g14-17000/independent-review-2026-10-26.md, independent-review-part2-2026-10-26.md (Claude-agent independent review, user-approved reviewer)",
        reason: "independent review by a Claude agent (user-approved 2026-10-26); Height-only rebind of the unpublished adopted G14 list: user decision 2026-10-30, DAA 18,400. Historical evidence source/digests and waiver scope are retained; not current-candidate PASS evidence.",
        decided_by: "user 2026-10-27; height user 2026-10-30",
    },
];

/// The g14 records (complete ones only).
#[rustfmt::skip]
pub const PALW_T12_18400_G14_RECORDS_V1: &[crate::palw_g14_gates_v1::PalwG14ArmingRecordV1] = &[
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyPublicFilerV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0x51, 0x0b, 0x8c, 0x8d, 0xb3, 0x50, 0xc6, 0x0c, 0xab, 0xd8, 0xe5, 0x69, 0xac, 0x39, 0x6f, 0x2c, 0x44, 0xd9, 0x16, 0x98, 0xc3, 0xa4, 0xa4, 0x57, 0xf6, 0xea, 0x26, 0x5f, 0x3e, 0xce, 0xff, 0x85], // slot 0 run
            [0x51, 0x36, 0x32, 0x2b, 0x80, 0x79, 0xc7, 0x1d, 0xef, 0x6b, 0x75, 0xf6, 0x28, 0x65, 0x46, 0xb2, 0x45, 0x64, 0xe4, 0xdf, 0xbc, 0x50, 0xe6, 0x7f, 0x38, 0x1d, 0x9e, 0xea, 0x30, 0x1a, 0x19, 0x0d], // slot 1 waiver
            [0xec, 0xeb, 0x4d, 0x23, 0x0d, 0x6a, 0x25, 0x01, 0xc3, 0x41, 0xe4, 0x0d, 0xe0, 0xac, 0xcd, 0x92, 0xbd, 0xa1, 0xaa, 0xd2, 0x46, 0xa0, 0x7c, 0x56, 0x97, 0x94, 0xfd, 0xff, 0x76, 0xdc, 0x25, 0x8f], // slot 2 waiver
            [0x00, 0xa7, 0x12, 0x31, 0x71, 0x92, 0xfc, 0xc0, 0xed, 0x8e, 0x38, 0xeb, 0x97, 0x1c, 0x6f, 0xbf, 0x73, 0x88, 0x96, 0x29, 0x7d, 0xd7, 0xef, 0x65, 0xb8, 0x9a, 0x45, 0x63, 0xa0, 0xd0, 0x73, 0xf8], // slot 3 waiver
            [0x74, 0x85, 0x5b, 0x12, 0xb4, 0x0c, 0xfb, 0xf7, 0x38, 0x02, 0x25, 0xd4, 0xa7, 0xef, 0x31, 0x7d, 0xe8, 0x84, 0x73, 0xe1, 0x56, 0xf2, 0x7a, 0xa9, 0x67, 0x29, 0x3c, 0xa0, 0x99, 0x06, 0x11, 0x0f], // slot 4 waiver
            [0x09, 0x2f, 0xd2, 0x78, 0xc8, 0xd1, 0x94, 0x53, 0x12, 0xcd, 0xce, 0x6c, 0x50, 0xa9, 0x04, 0x94, 0xc4, 0x31, 0x77, 0xb5, 0xb9, 0xd4, 0x12, 0x3e, 0x6d, 0x30, 0x89, 0xbc, 0x8c, 0x23, 0x11, 0xb1], // slot 5 run
            [0x4a, 0x76, 0xea, 0x04, 0xc4, 0xf3, 0x24, 0x7c, 0xd5, 0x72, 0xd6, 0x10, 0x7c, 0xea, 0x87, 0xaa, 0x27, 0xbd, 0xe5, 0xb7, 0xfa, 0x5d, 0xcf, 0x5a, 0xc2, 0x87, 0x8d, 0xeb, 0x2b, 0x15, 0x4d, 0xa8], // slot 6 waiver
        ],
        review: [0x57, 0xb5, 0x03, 0x63, 0xc1, 0xcc, 0x64, 0x81, 0x0a, 0xe9, 0x12, 0xf8, 0xd8, 0x9b, 0xcf, 0xea, 0x4f, 0x70, 0x0c, 0x48, 0x67, 0xad, 0xeb, 0xa9, 0x9f, 0x11, 0x5f, 0xb9, 0x92, 0xcd, 0x86, 0xeb],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyHeldDaV2,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0x51, 0x0b, 0x8c, 0x8d, 0xb3, 0x50, 0xc6, 0x0c, 0xab, 0xd8, 0xe5, 0x69, 0xac, 0x39, 0x6f, 0x2c, 0x44, 0xd9, 0x16, 0x98, 0xc3, 0xa4, 0xa4, 0x57, 0xf6, 0xea, 0x26, 0x5f, 0x3e, 0xce, 0xff, 0x85], // slot 0 run
            [0x9c, 0x3e, 0x9c, 0x19, 0x99, 0x01, 0xc0, 0x86, 0x97, 0x34, 0x95, 0xf7, 0x52, 0x33, 0x07, 0x8f, 0x65, 0xb8, 0xb9, 0x36, 0xee, 0x1d, 0xfa, 0xf2, 0xb7, 0x05, 0x32, 0x18, 0x6c, 0x99, 0x92, 0x3b], // slot 1 waiver
            [0x69, 0xc8, 0x7e, 0xec, 0xc5, 0xb8, 0xfe, 0xb1, 0xa1, 0x27, 0x98, 0x89, 0xa0, 0x1a, 0x58, 0x49, 0x18, 0xe4, 0x27, 0x88, 0xed, 0xe5, 0x99, 0xe2, 0x8f, 0xf6, 0xde, 0xe5, 0x0b, 0x6b, 0x72, 0xdd], // slot 2 run
            [0x16, 0x10, 0x3a, 0xd0, 0x8a, 0x4e, 0x66, 0xe4, 0xc1, 0x17, 0x61, 0xd1, 0xd0, 0x31, 0xfa, 0x01, 0x79, 0xf3, 0x6f, 0x8d, 0x5f, 0x85, 0xa2, 0xc9, 0x6d, 0x82, 0xea, 0xd7, 0xe2, 0x39, 0x4b, 0xda], // slot 3 waiver
            [0xa2, 0x5a, 0x75, 0x48, 0xf7, 0xe8, 0xa3, 0x17, 0x05, 0xf5, 0xa2, 0xc6, 0x94, 0xb5, 0x6b, 0x61, 0xbe, 0x2e, 0xc7, 0x8f, 0x3b, 0x1f, 0x7e, 0x03, 0x1a, 0xa3, 0x89, 0xf4, 0x53, 0x5b, 0xec, 0x08], // slot 4 waiver
            [0x65, 0x50, 0xba, 0x94, 0xa2, 0x75, 0x32, 0xe6, 0xde, 0xe7, 0xd3, 0x7c, 0x32, 0x9f, 0x30, 0x23, 0x0e, 0x19, 0x88, 0x64, 0x24, 0xc6, 0xb2, 0x2c, 0x0a, 0xc6, 0x9e, 0x9b, 0x80, 0x0e, 0x4d, 0x18], // slot 5 run
            [0x2e, 0x21, 0xe7, 0xdf, 0xa1, 0x25, 0x99, 0x29, 0x56, 0x74, 0x63, 0xe4, 0x1b, 0x9b, 0x80, 0xb0, 0x9b, 0xc6, 0x4a, 0x62, 0xda, 0x93, 0x4b, 0x5f, 0xb6, 0x0d, 0x01, 0x58, 0x47, 0x91, 0x20, 0x67], // slot 6 waiver
        ],
        review: [0xbe, 0xcc, 0x96, 0xb7, 0xde, 0xd3, 0x96, 0x31, 0x2d, 0xe0, 0x93, 0x22, 0x43, 0x0c, 0xb2, 0x00, 0xfb, 0x87, 0x10, 0xdf, 0x92, 0x35, 0x79, 0x88, 0x45, 0x2f, 0x3d, 0xfc, 0x83, 0x06, 0x0d, 0x77],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedPublicCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0xa6, 0x09, 0x91, 0x32, 0xba, 0xd2, 0xf1, 0x0f, 0xb8, 0x74, 0x34, 0x7f, 0xe9, 0x4c, 0x26, 0xe3, 0xe7, 0xfc, 0x07, 0xa6, 0xe1, 0x02, 0x66, 0x52, 0x23, 0x85, 0xd9, 0x20, 0x95, 0x83, 0x0f, 0x15], // slot 0 run
            [0x15, 0x3c, 0x5b, 0x35, 0x13, 0x06, 0x5f, 0x07, 0xb3, 0x0f, 0xc4, 0xef, 0x78, 0x6d, 0xc8, 0x9c, 0xac, 0xec, 0x0d, 0x19, 0xf3, 0xf6, 0x2b, 0x3e, 0x64, 0xec, 0xff, 0xf6, 0xd5, 0x32, 0x86, 0x99], // slot 1 waiver
            [0xdf, 0xb2, 0x3c, 0x7a, 0x96, 0x85, 0x1c, 0xce, 0x67, 0x8d, 0x87, 0x5d, 0x59, 0xb6, 0xdf, 0xe6, 0x9a, 0x51, 0x86, 0xa2, 0x3c, 0xa2, 0xaf, 0xa7, 0x3a, 0x44, 0xca, 0x6d, 0x4b, 0xad, 0xa0, 0xec], // slot 2 waiver
            [0xcf, 0x9b, 0x13, 0xfb, 0x69, 0x18, 0x6e, 0x2e, 0xa1, 0x7b, 0x3c, 0xe5, 0x91, 0xcd, 0x06, 0xbd, 0xa6, 0x44, 0xb6, 0x47, 0xa6, 0x1c, 0x5b, 0x3a, 0x90, 0xb5, 0x3f, 0xa9, 0xd3, 0x63, 0xf6, 0x87], // slot 3 waiver
            [0x2d, 0x41, 0x2e, 0xf5, 0x51, 0x4b, 0x70, 0x20, 0xc0, 0x8f, 0xca, 0x24, 0x05, 0x8f, 0x8c, 0xde, 0x5c, 0x31, 0x40, 0xa5, 0x39, 0x1e, 0x7d, 0xa8, 0x46, 0x64, 0x28, 0x27, 0x59, 0x13, 0x41, 0x72], // slot 4 waiver
            [0xcb, 0x91, 0x7b, 0x48, 0xea, 0x13, 0x65, 0x22, 0x7e, 0x69, 0x2c, 0x43, 0x8c, 0x1c, 0x3a, 0x78, 0xcc, 0x80, 0x01, 0x9b, 0xcb, 0xd7, 0x40, 0x23, 0x79, 0x09, 0xd2, 0x44, 0x4e, 0xd2, 0x9f, 0x7f], // slot 5 waiver
            [0xdb, 0x94, 0x31, 0x4e, 0x71, 0xfb, 0x5e, 0x71, 0xd3, 0xdf, 0x50, 0x5f, 0x1a, 0x77, 0xd0, 0x3e, 0x93, 0x55, 0x72, 0x4f, 0x5f, 0xe3, 0x42, 0x81, 0x77, 0x44, 0x96, 0x99, 0x88, 0x57, 0xb2, 0xc3], // slot 6 waiver
        ],
        review: [0x85, 0x93, 0x38, 0x4a, 0xbe, 0x99, 0xa7, 0x5b, 0x0b, 0x97, 0xb4, 0x24, 0x79, 0xd1, 0x58, 0x6a, 0x82, 0xb7, 0x8d, 0xa1, 0x64, 0xc6, 0x3d, 0x91, 0x0d, 0x6d, 0xff, 0x97, 0x2d, 0x04, 0x81, 0xad],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedTirRouteV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0xc7, 0xa6, 0xcf, 0x2a, 0xee, 0x8a, 0xfa, 0x57, 0x37, 0x01, 0x95, 0x72, 0x6d, 0xa9, 0x69, 0xac, 0xa1, 0x80, 0xac, 0xdb, 0x46, 0x97, 0xad, 0x93, 0x83, 0x20, 0x50, 0x28, 0xc3, 0xb2, 0x7b, 0x70], // slot 0 run
            [0x75, 0xa1, 0xb2, 0x3d, 0x71, 0x93, 0xe3, 0xc6, 0xfd, 0xe7, 0xe5, 0xeb, 0x48, 0xa1, 0x1b, 0xbb, 0x37, 0x89, 0xb5, 0xfe, 0x11, 0x5d, 0xea, 0xae, 0xb8, 0x66, 0x47, 0xcf, 0xee, 0x27, 0x15, 0x6e], // slot 1 waiver
            [0x26, 0x82, 0x0a, 0xd2, 0x18, 0xf6, 0x4b, 0xe9, 0xb7, 0x26, 0x83, 0x42, 0xe3, 0x32, 0xc6, 0x04, 0xf5, 0x08, 0xfc, 0x33, 0x03, 0x2d, 0x9a, 0x23, 0x29, 0xf1, 0x34, 0xaf, 0x9f, 0xbd, 0x93, 0x82], // slot 2 waiver
            [0x6d, 0x64, 0x43, 0x4b, 0xa5, 0x05, 0x39, 0x25, 0x4f, 0x72, 0x77, 0xc8, 0x7e, 0xfa, 0x7f, 0xaf, 0xfa, 0xd7, 0x16, 0xfe, 0x4e, 0x69, 0x64, 0xa7, 0xd2, 0xe2, 0x0a, 0x4b, 0x8d, 0x8c, 0x00, 0x16], // slot 3 waiver
            [0xd7, 0x1c, 0xe2, 0x17, 0x61, 0x24, 0x31, 0xf7, 0x66, 0x81, 0x0e, 0xa2, 0xf4, 0xa6, 0x70, 0xe3, 0x3f, 0xbf, 0x7c, 0x74, 0x1b, 0xd2, 0x21, 0xac, 0x14, 0x7d, 0xff, 0x27, 0xdc, 0x8c, 0x4c, 0xf5], // slot 4 waiver
            [0x82, 0x8b, 0xa9, 0x76, 0x5a, 0x1d, 0xd7, 0x9c, 0xf0, 0x23, 0xa4, 0xdd, 0xe9, 0x47, 0x29, 0x7b, 0x75, 0xd8, 0x7b, 0x9d, 0xb1, 0x3d, 0x0c, 0x12, 0xdf, 0x2a, 0x70, 0x41, 0xf5, 0x62, 0x6d, 0xef], // slot 5 waiver
            [0x83, 0xb4, 0xdb, 0xeb, 0x54, 0xd7, 0x45, 0x0a, 0xe6, 0xf2, 0xf4, 0xec, 0x11, 0x0e, 0x9a, 0xcd, 0x58, 0x91, 0xca, 0x11, 0x6a, 0x88, 0xe2, 0xbf, 0x63, 0xe3, 0xc9, 0xb0, 0x0f, 0x01, 0xa4, 0x99], // slot 6 waiver
        ],
        review: [0x1f, 0x22, 0xa1, 0x0d, 0x9a, 0xa8, 0xb7, 0x14, 0xcd, 0xb5, 0x08, 0x15, 0x4b, 0x59, 0x1b, 0x31, 0x09, 0x7c, 0xe7, 0x23, 0x6b, 0x22, 0x8d, 0x8e, 0xd1, 0x85, 0x8f, 0xe6, 0xdd, 0xe9, 0x43, 0x71],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::TirPublicConvictionV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0x8a, 0xe8, 0x17, 0xbf, 0xe7, 0x96, 0x72, 0x82, 0x1a, 0x7f, 0xcd, 0xac, 0x7c, 0x5f, 0xc2, 0x3a, 0x77, 0x6c, 0x16, 0xb4, 0xc5, 0x75, 0x69, 0x35, 0x82, 0x69, 0x88, 0x5a, 0x31, 0x0e, 0xd3, 0x19], // slot 0 run
            [0x59, 0xeb, 0xb5, 0x4e, 0xc8, 0x47, 0x24, 0x16, 0xfb, 0x0c, 0x01, 0x20, 0x8e, 0xab, 0xda, 0x9f, 0x44, 0x48, 0xc4, 0x4f, 0xc4, 0x1e, 0x3b, 0x7f, 0x81, 0xbf, 0x41, 0x6b, 0xaf, 0xc3, 0x22, 0x2b], // slot 1 waiver
            [0x29, 0x70, 0xa6, 0xf6, 0x76, 0x86, 0x28, 0xdd, 0x2b, 0xc9, 0x72, 0x06, 0xc1, 0xbe, 0x12, 0x50, 0x2b, 0x56, 0x2a, 0xf9, 0xb9, 0xbc, 0x70, 0xe8, 0xb7, 0x42, 0x52, 0xd5, 0xc3, 0x5a, 0xb0, 0x75], // slot 2 waiver
            [0x41, 0xba, 0x2a, 0xe3, 0x82, 0xeb, 0x38, 0xc3, 0x36, 0x5f, 0xd0, 0x3c, 0x48, 0x45, 0x87, 0xaa, 0x89, 0x39, 0x88, 0x57, 0x7e, 0x1f, 0x1a, 0x25, 0x0b, 0xd3, 0x52, 0xe3, 0x70, 0xd8, 0xf5, 0x24], // slot 3 waiver
            [0x5c, 0x8b, 0x4f, 0x43, 0x9a, 0xf7, 0x66, 0x2c, 0x04, 0xda, 0x17, 0x83, 0x65, 0x69, 0xbb, 0x8b, 0x94, 0x0d, 0x49, 0xc2, 0xc1, 0x42, 0x29, 0x9d, 0xf2, 0x2a, 0x6d, 0x23, 0xca, 0xac, 0x52, 0x00], // slot 4 waiver
            [0xd3, 0xaf, 0x99, 0xbb, 0x34, 0x2b, 0x51, 0x6b, 0x43, 0xfa, 0xe8, 0x7b, 0x6c, 0x71, 0x6b, 0xd4, 0x29, 0xcc, 0xab, 0xbc, 0x59, 0x53, 0xf1, 0xf3, 0x91, 0x8d, 0xbe, 0x40, 0x9c, 0x80, 0x77, 0x34], // slot 5 waiver
            [0x5a, 0x0c, 0xa1, 0x15, 0x93, 0x9e, 0x3e, 0x45, 0x40, 0xbd, 0x8f, 0x89, 0x35, 0x66, 0x8e, 0xeb, 0x75, 0xbd, 0x53, 0xd7, 0xbe, 0xca, 0x66, 0x87, 0x90, 0x05, 0xca, 0x7f, 0x78, 0x6d, 0x88, 0x3c], // slot 6 waiver
        ],
        review: [0x58, 0xdf, 0x27, 0x93, 0xab, 0xcc, 0x92, 0x17, 0xb4, 0x4a, 0x8e, 0xea, 0x1b, 0x9e, 0x18, 0x70, 0x18, 0x56, 0xcc, 0x99, 0x68, 0x08, 0x3d, 0x4b, 0xf8, 0x91, 0xf5, 0x26, 0x3b, 0xa1, 0xee, 0x1f],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::CourtCapacityV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0xfc, 0x62, 0x1b, 0xc4, 0xbb, 0x4a, 0x68, 0x8c, 0x18, 0x49, 0xeb, 0x5d, 0x2f, 0x3c, 0xc1, 0x61, 0x05, 0x3c, 0xe6, 0xfb, 0x0f, 0xbf, 0x31, 0xcc, 0x70, 0x01, 0x2a, 0xde, 0xf5, 0x6f, 0xd6, 0xd9], // slot 0 run
            [0xda, 0x12, 0x78, 0xe7, 0x2a, 0x21, 0x62, 0xe2, 0x72, 0xf3, 0xdb, 0xa4, 0x53, 0x4c, 0xfa, 0x6a, 0x9b, 0xb1, 0xc1, 0xe5, 0xf5, 0x8e, 0x8b, 0x87, 0xaf, 0x34, 0x61, 0xb6, 0x6e, 0x70, 0xb1, 0x69], // slot 1 waiver
            [0x0f, 0x8a, 0xf0, 0xc8, 0x52, 0x69, 0x83, 0xec, 0x26, 0x46, 0xf2, 0x91, 0xdd, 0x3b, 0x51, 0x4d, 0xcf, 0x88, 0x09, 0xbd, 0x94, 0xea, 0x5d, 0x3a, 0x9e, 0x83, 0x9d, 0x84, 0x31, 0x4a, 0xd2, 0x40], // slot 2 waiver
            [0x1c, 0x56, 0x3b, 0x57, 0x48, 0x8e, 0xa8, 0xec, 0x66, 0x68, 0xd1, 0x66, 0x6c, 0x26, 0x70, 0xf7, 0xdf, 0xa0, 0x81, 0xa2, 0x54, 0xea, 0xf7, 0x09, 0xe1, 0x96, 0xbe, 0x4d, 0x9f, 0x7d, 0x3e, 0xcd], // slot 3 waiver
            [0xa3, 0x9b, 0x10, 0xcf, 0x66, 0xe7, 0xde, 0xba, 0x78, 0xc4, 0x30, 0xe4, 0x2e, 0x96, 0x93, 0xc2, 0x6e, 0x7c, 0x0b, 0x26, 0x1f, 0x2a, 0xc8, 0xac, 0xb7, 0x96, 0x22, 0x03, 0x5b, 0x3d, 0x8a, 0x40], // slot 4 waiver
            [0xfb, 0x54, 0xa6, 0x8b, 0x78, 0x48, 0xb6, 0x57, 0x0d, 0x5b, 0xff, 0x61, 0x2b, 0x45, 0xbe, 0x89, 0xd6, 0x9e, 0x9f, 0xfc, 0x26, 0xee, 0x52, 0xb5, 0xe4, 0xe0, 0xc4, 0x56, 0xee, 0x74, 0xb4, 0x9d], // slot 5 waiver
            [0x2f, 0x8f, 0x11, 0x3e, 0xe4, 0xff, 0x95, 0x06, 0x1d, 0x63, 0x3b, 0xd2, 0x80, 0x53, 0xf9, 0xd7, 0x73, 0x53, 0x44, 0x14, 0x39, 0xe4, 0x8e, 0x00, 0x20, 0x7d, 0x78, 0x38, 0x5a, 0xf2, 0x32, 0xde], // slot 6 run
        ],
        review: [0xe7, 0x08, 0xee, 0xfd, 0x62, 0x10, 0xe7, 0xee, 0x74, 0x4b, 0x88, 0x67, 0x99, 0x0b, 0xaf, 0x16, 0x8c, 0x77, 0xf9, 0x19, 0x41, 0x93, 0xdc, 0x32, 0x5c, 0xe3, 0x7f, 0x28, 0x0f, 0x55, 0x31, 0x25],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::ClaimHardDeadlineV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0x2e, 0x76, 0x26, 0xdc, 0xce, 0xbe, 0x48, 0xed, 0xe7, 0x2e, 0xcc, 0xad, 0xb1, 0x84, 0x62, 0x17, 0x60, 0x04, 0xbf, 0xbe, 0xa5, 0x86, 0xb2, 0x9b, 0x28, 0x27, 0x68, 0x0e, 0x88, 0x53, 0xeb, 0xb0], // slot 0 run
            [0xed, 0x85, 0x02, 0x96, 0x67, 0xfd, 0x75, 0x63, 0xbe, 0xce, 0x72, 0x88, 0xf4, 0xef, 0x13, 0x6a, 0x1b, 0x8a, 0xd5, 0x8f, 0xc1, 0xbf, 0xba, 0xeb, 0xf7, 0x75, 0xcf, 0x5a, 0x66, 0xce, 0xbe, 0x40], // slot 1 waiver
            [0xa9, 0xd3, 0x6a, 0xd3, 0x66, 0x4c, 0x6d, 0xbd, 0xe3, 0x1f, 0xa9, 0x12, 0x90, 0x8b, 0x05, 0xfa, 0x72, 0x64, 0xfb, 0x06, 0x5b, 0xe0, 0x72, 0x34, 0xc9, 0x45, 0xae, 0x9c, 0x27, 0xb4, 0x60, 0xc8], // slot 2 waiver
            [0xc9, 0x17, 0xd0, 0x35, 0x62, 0x1e, 0x54, 0x2c, 0x56, 0x2c, 0x26, 0xdb, 0x02, 0x25, 0x74, 0x73, 0x59, 0x29, 0xde, 0x3d, 0x38, 0x13, 0x2d, 0x59, 0xd2, 0xd9, 0xf9, 0x10, 0x4f, 0x9e, 0x2b, 0x52], // slot 3 waiver
        ],
        review: [0x5a, 0xd6, 0x1b, 0x87, 0x19, 0xdc, 0xe8, 0xdf, 0x03, 0x10, 0xfc, 0xb8, 0x18, 0x03, 0xd3, 0xbd, 0x92, 0xc5, 0xfe, 0x7d, 0x98, 0x1c, 0xfe, 0xbd, 0xf0, 0xa6, 0x34, 0x09, 0x3f, 0x1a, 0x3c, 0x41],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::G14RouteRequiredV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0xbf, 0xb8, 0x09, 0x08, 0x32, 0x19, 0xfa, 0x7d, 0xd5, 0xd1, 0x98, 0x3a, 0x1a, 0x53, 0x61, 0x29, 0x07, 0x33, 0xa0, 0xa1, 0x7a, 0x02, 0x31, 0x9b, 0xcd, 0xae, 0x84, 0x7a, 0x98, 0xb0, 0xc3, 0x61], // slot 0 run
            [0x3d, 0xfe, 0xa5, 0xa3, 0x39, 0x22, 0xb4, 0x3e, 0x5c, 0x59, 0xa6, 0xb9, 0x78, 0xed, 0x0f, 0xda, 0x86, 0x20, 0x1c, 0x45, 0x86, 0x78, 0x6a, 0xee, 0xcb, 0x2d, 0xd8, 0x25, 0xb1, 0x93, 0xab, 0x10], // slot 1 run
            [0x5e, 0xc2, 0x25, 0x82, 0x12, 0x98, 0xa0, 0xf8, 0x40, 0x6a, 0xcc, 0x46, 0x2b, 0x0b, 0xa0, 0x8c, 0x7b, 0xde, 0x67, 0x8e, 0x97, 0xa8, 0x20, 0xd0, 0x8c, 0xe1, 0x46, 0xc4, 0x0c, 0xd6, 0x8c, 0x4d], // slot 2 waiver
            [0x28, 0xc3, 0xf6, 0x86, 0x4d, 0xb2, 0xf1, 0xe4, 0xf2, 0x27, 0x4b, 0xe8, 0xed, 0x81, 0x31, 0xa1, 0xbc, 0x27, 0x26, 0x23, 0x06, 0x51, 0xa5, 0x11, 0xec, 0x82, 0x00, 0x6e, 0xcd, 0x7f, 0x1e, 0x87], // slot 3 waiver
        ],
        review: [0x0a, 0x39, 0xe0, 0x46, 0x73, 0x4a, 0xa7, 0x4b, 0x4c, 0xc2, 0x3c, 0x3a, 0xa1, 0x70, 0x88, 0x51, 0x38, 0x99, 0x6e, 0xb3, 0x51, 0x9a, 0xaa, 0xbc, 0x43, 0xbb, 0x84, 0x81, 0x03, 0x9d, 0xe2, 0x3d],
    },
];

/// The kernel-route records (complete ones only).
#[rustfmt::skip]
pub const PALW_T12_18400_KERNEL_ROUTE_RECORDS_V1: &[crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1] = &[
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0x2e, 0x88, 0xb5, 0x04, 0x06, 0x5b, 0xd1, 0x93, 0x3d, 0x55, 0xf3, 0xf2, 0xfa, 0x5a, 0x9b, 0x9d, 0xaf, 0x50, 0x80, 0x81, 0xa3, 0x08, 0xe9, 0x41, 0x20, 0x35, 0x40, 0xbd, 0x66, 0xc9, 0x0a, 0x67], // slot 0 run
            [0xde, 0xae, 0xa7, 0xa6, 0x81, 0x8e, 0x4e, 0x8d, 0xcd, 0x19, 0x33, 0x23, 0xe4, 0x93, 0x01, 0x1f, 0xb6, 0x95, 0xb9, 0x61, 0x4f, 0x3f, 0x23, 0x08, 0xc5, 0xaa, 0xd8, 0x8b, 0x8b, 0xe6, 0x40, 0x5e], // slot 1 waiver
            [0xaa, 0xc4, 0x46, 0xda, 0x48, 0x60, 0x35, 0x14, 0x9a, 0xba, 0x1a, 0x2b, 0x81, 0x02, 0x8b, 0x57, 0xb4, 0x26, 0x4f, 0xc5, 0xab, 0x83, 0x88, 0x51, 0xd6, 0x96, 0x2a, 0x67, 0x04, 0x9c, 0x2c, 0xdb], // slot 2 run
            [0xac, 0xc0, 0x50, 0x95, 0x8b, 0x47, 0x08, 0x7c, 0x1b, 0x9a, 0x66, 0x84, 0x43, 0x33, 0x0b, 0x9c, 0x87, 0xce, 0x77, 0x61, 0xa2, 0x48, 0x03, 0x96, 0xf5, 0x0f, 0xff, 0xff, 0x2a, 0xae, 0xb7, 0xa1], // slot 3 waiver
            [0x1e, 0xef, 0xcd, 0x23, 0x4c, 0xb0, 0x7c, 0x1c, 0x8a, 0x7d, 0x2e, 0x0c, 0x04, 0xd3, 0x85, 0xf0, 0xce, 0x68, 0xc2, 0x64, 0x50, 0x40, 0xfc, 0x21, 0xee, 0x51, 0xfc, 0x29, 0xe3, 0xd3, 0x07, 0x9e], // slot 4 waiver
            [0xfc, 0x12, 0xf5, 0x07, 0x34, 0x8e, 0x56, 0x79, 0x98, 0x28, 0x26, 0x3d, 0xf9, 0xad, 0x86, 0xc3, 0x9f, 0xac, 0x62, 0xeb, 0x9c, 0x12, 0x62, 0xb2, 0x55, 0x4e, 0xb4, 0xdf, 0x4c, 0xa0, 0xb3, 0xb7], // slot 5 waiver
            [0xad, 0x78, 0x0b, 0x9a, 0x19, 0x35, 0x2f, 0x7a, 0x1f, 0x11, 0xa8, 0xe9, 0x53, 0x88, 0xdf, 0xe9, 0x74, 0xe6, 0xf2, 0xfe, 0x6b, 0xce, 0x1d, 0xf8, 0x7e, 0x69, 0xd9, 0xc5, 0x48, 0x9e, 0xd6, 0xc4], // slot 6 waiver
            [0x80, 0x90, 0xe3, 0x8c, 0x4a, 0x29, 0x78, 0x81, 0x6a, 0x39, 0x22, 0x7d, 0x16, 0x8c, 0x07, 0x3b, 0xc8, 0xc2, 0x66, 0x81, 0x78, 0x6f, 0xb9, 0x7f, 0x2f, 0xf0, 0x06, 0xa1, 0xad, 0x94, 0x24, 0x0f], // slot 7 waiver
        ],
        review: [0x9e, 0x99, 0x0d, 0x67, 0xc8, 0x05, 0x15, 0x3a, 0x93, 0xcf, 0x5d, 0x7b, 0x88, 0x20, 0x07, 0x72, 0x99, 0x0a, 0xe7, 0x5e, 0x20, 0x62, 0xdb, 0xfa, 0x9e, 0x8f, 0xbb, 0x04, 0x61, 0x0a, 0xe1, 0x07],
    },
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProviderCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0xe0, 0x01, 0x53, 0xcb, 0xe8, 0x0d, 0xdb, 0x9d, 0xeb, 0x57, 0x3a, 0xe4, 0x93, 0xdc, 0x61, 0x04, 0x02, 0x3e, 0x67, 0x14, 0x61, 0x3b, 0xbf, 0xea, 0xeb, 0xad, 0x15, 0xdd, 0x6a, 0xb4, 0xf3, 0x4c], // slot 0 run
            [0xa2, 0x14, 0x04, 0x1c, 0x12, 0x41, 0x29, 0xf7, 0x12, 0x99, 0x20, 0x34, 0x27, 0x91, 0x8e, 0x1d, 0xd5, 0x77, 0xbf, 0x88, 0x43, 0xd2, 0x16, 0xf7, 0x28, 0xb3, 0xdf, 0x86, 0xf6, 0x7d, 0x12, 0x91], // slot 1 waiver
            [0xdd, 0x20, 0xbb, 0xb9, 0xa4, 0x4d, 0x09, 0xdd, 0xa7, 0xd2, 0x26, 0xe8, 0xae, 0x86, 0x96, 0xb6, 0x61, 0x34, 0xef, 0x60, 0x88, 0x70, 0x2e, 0x0c, 0xa2, 0x31, 0xd2, 0xde, 0xcb, 0x70, 0x72, 0xb4], // slot 2 waiver
            [0xf2, 0x53, 0x2a, 0x87, 0xeb, 0xeb, 0xc0, 0x46, 0xcf, 0xbc, 0xe5, 0xf7, 0x2f, 0xfb, 0x67, 0x8d, 0x88, 0xe9, 0xf3, 0xb7, 0x73, 0x98, 0x78, 0xf9, 0x60, 0x23, 0xda, 0x4a, 0x81, 0x43, 0x07, 0x8b], // slot 3 waiver
        ],
        review: [0x3c, 0xf8, 0x37, 0xab, 0xbb, 0xe3, 0x55, 0x44, 0x62, 0x86, 0x74, 0x22, 0x27, 0x71, 0x69, 0xab, 0x0b, 0x25, 0x94, 0x27, 0x38, 0x49, 0x96, 0x3a, 0x93, 0xcd, 0x93, 0x8a, 0x78, 0xfe, 0xa1, 0x5e],
    },
];

/// The rfc0018 records (complete ones only).
#[rustfmt::skip]
pub const PALW_T12_18400_RFC0018_RECORDS_V1: &[crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1] = &[
    crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1 {
        version: crate::palw_rfc0018_v1::PALW_RFC0018_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_rfc0018_v1::PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0xba, 0x60, 0xc1, 0xc3, 0xf4, 0xce, 0x62, 0xf7, 0x21, 0x3d, 0x52, 0x65, 0xc5, 0x5d, 0x45, 0xcc, 0x84, 0x71, 0xc6, 0x39, 0xb4, 0x1c, 0xf3, 0x3a, 0x3f, 0x0e, 0xea, 0xd9, 0xfa, 0x29, 0xd1, 0xab], // slot 0 waiver
            [0x72, 0x19, 0xa7, 0xa4, 0x21, 0x73, 0xfe, 0xfa, 0x69, 0x58, 0xd5, 0x4c, 0xe7, 0x99, 0x03, 0x41, 0x77, 0x96, 0x36, 0xb9, 0x76, 0x38, 0x22, 0x98, 0xbe, 0x69, 0xe2, 0x1e, 0x92, 0xd2, 0x8b, 0xb7], // slot 1 run
            [0x9b, 0x72, 0x5b, 0x01, 0x40, 0x96, 0xbd, 0x2f, 0x32, 0x30, 0x27, 0x53, 0x17, 0xeb, 0x5e, 0xfc, 0xdd, 0x5c, 0x5a, 0x94, 0xe1, 0x77, 0x97, 0x20, 0x0c, 0xd3, 0x00, 0xaa, 0x1a, 0xd1, 0xac, 0xd9], // slot 2 waiver
            [0x40, 0xd1, 0xeb, 0xb3, 0x9f, 0x28, 0x35, 0x03, 0x63, 0x8e, 0x13, 0x46, 0xbc, 0x41, 0x83, 0xf8, 0x70, 0x76, 0x45, 0x70, 0x1c, 0xd0, 0xaa, 0x50, 0x5a, 0x56, 0x55, 0x1d, 0xef, 0x77, 0x59, 0xac], // slot 3 waiver
            [0x96, 0x02, 0x77, 0x7b, 0x95, 0xd9, 0x94, 0xaf, 0x1d, 0xab, 0x21, 0x3d, 0xe7, 0x5f, 0xde, 0x9d, 0xa4, 0x8f, 0x43, 0x9a, 0x87, 0xc0, 0x8a, 0x54, 0xf1, 0x3c, 0x5e, 0xc4, 0xc9, 0xed, 0x3b, 0x2e], // slot 4 waiver
            [0x8b, 0xf5, 0x9f, 0x58, 0xa7, 0xea, 0xa3, 0xd5, 0x8d, 0x0d, 0xaa, 0x19, 0x9b, 0x97, 0x66, 0xc4, 0x87, 0x60, 0x02, 0x25, 0x75, 0x3f, 0x3f, 0x78, 0x81, 0xb7, 0xd2, 0x8e, 0xdb, 0x7e, 0x76, 0x15], // slot 5 waiver
        ],
        review: [0x3b, 0x76, 0x10, 0x07, 0x69, 0xc6, 0x1c, 0x0d, 0x8f, 0x21, 0xa6, 0x92, 0x13, 0x8b, 0x38, 0x8e, 0x58, 0x6c, 0xc5, 0x69, 0xbe, 0x91, 0x64, 0x13, 0x81, 0x81, 0xd2, 0x6a, 0xaf, 0x34, 0xd6, 0x4a],
    },
];

/// The frame cost record for FpSelection: the lane VAL estimate (ProvisionalEstimate) and the agent review. Emitted only
/// with the final evidence run's candidate commit as its source (a zero commit is refused, never pinned).
#[rustfmt::skip]
pub const PALW_T12_18400_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[
    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {
        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        measurement: [0xa6, 0x50, 0x64, 0x60, 0xd5, 0x82, 0x16, 0x1b, 0x16, 0x87, 0x8a, 0x52, 0x79, 0xf0, 0x00, 0x20, 0xc2, 0x2b, 0x83, 0xe6, 0xb1, 0xf7, 0xff, 0x15, 0xbc, 0x79, 0x37, 0xe4, 0xee, 0x5b, 0x57, 0xad],
        review: [0x49, 0x76, 0x2d, 0x34, 0xcb, 0x76, 0x70, 0x04, 0x8a, 0x2c, 0xe5, 0x38, 0x79, 0x10, 0x82, 0x8b, 0xce, 0x47, 0xb5, 0x72, 0x96, 0xd3, 0x63, 0xa7, 0x0b, 0x5a, 0x94, 0x73, 0x01, 0x99, 0x0f, 0x3a],
        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,
    },
];

/// The model-tier record: c 300, kappa 1000 (user decision 2026-10-27), catalog edition 0 (empty) as the tip.
#[rustfmt::skip]
pub const PALW_T12_18400_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[
    crate::palw_model_tier_v1::PalwModelTierArmingRecordV1 {
        version: crate::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 18400,
        registered_permille: 300,
        registered_cap_permille: 1000,
        catalog_tip_edition: 0,
        catalog_tip_root: crate::Hash64::from_bytes([0xc9, 0x4d, 0xb3, 0x54, 0x2b, 0x1e, 0x0b, 0xe5, 0x8a, 0xd1, 0xda, 0x59, 0xb3, 0x9f, 0xab, 0xf3, 0x7b, 0x65, 0x0f, 0x6c, 0x3e, 0x22, 0x34, 0x8e, 0x06, 0xcb, 0x2b, 0xd6, 0xb1, 0x21, 0x61, 0x43, 0xbe, 0x35, 0x19, 0xa8, 0x0b, 0xbd, 0x68, 0x57, 0x8d, 0x62, 0xda, 0xf2, 0xd2, 0x03, 0xb7, 0xa3, 0xdb, 0x96, 0xe6, 0xd1, 0x0c, 0x95, 0xa3, 0x24, 0xce, 0xe8, 0xef, 0x9d, 0x47, 0xf7, 0x2b, 0x6a]),
        source_commit: [0xda, 0x46, 0x13, 0x1a, 0x41, 0xfe, 0x7c, 0x90, 0xec, 0x26, 0x3b, 0x0f, 0x26, 0xab, 0x9b, 0x44, 0x4e, 0x4c, 0x74, 0xa2],
        evidence: &[
            [0x80, 0x84, 0xae, 0x49, 0xdb, 0xce, 0xa5, 0x1a, 0x0c, 0x03, 0x1d, 0x2e, 0x8a, 0x07, 0x28, 0x95, 0xe8, 0x6f, 0xf9, 0xe8, 0x1a, 0x68, 0xa4, 0x33, 0xfe, 0x57, 0x6c, 0x6c, 0x81, 0x89, 0xd3, 0xe8], // slot 0 waiver
            [0x08, 0xfc, 0x8e, 0x2d, 0x4b, 0x0e, 0x5e, 0x71, 0x76, 0x9a, 0x64, 0x28, 0x38, 0x6d, 0x50, 0x3c, 0xc3, 0x1e, 0xc4, 0x87, 0x72, 0x1e, 0x34, 0x75, 0xc9, 0x95, 0x16, 0x3c, 0x11, 0x18, 0xc5, 0xdd], // slot 1 run
            [0x29, 0x11, 0xa2, 0x01, 0x9b, 0x14, 0x68, 0x0a, 0x3d, 0xa7, 0xcb, 0xe6, 0x4c, 0xb3, 0x3f, 0x44, 0x11, 0x9e, 0xd9, 0xc1, 0x62, 0x8c, 0xd4, 0x63, 0xe9, 0xfa, 0x01, 0xf0, 0x95, 0xe2, 0xfc, 0x62], // slot 2 run
            [0x5e, 0xbc, 0xca, 0x59, 0x1c, 0x2f, 0xf6, 0xf5, 0x41, 0xa2, 0x1e, 0xfb, 0x76, 0x20, 0xe9, 0x35, 0x52, 0xac, 0x90, 0x9b, 0xd4, 0x8c, 0x9e, 0x27, 0x26, 0x17, 0x40, 0xa6, 0x6b, 0x95, 0xdd, 0x4b], // slot 3 waiver
            [0x71, 0xc6, 0x36, 0x7c, 0xb4, 0x09, 0x77, 0x5b, 0x82, 0xb8, 0x36, 0xdf, 0xc7, 0xef, 0xf6, 0x15, 0x6a, 0x40, 0xd0, 0x87, 0xfd, 0xa3, 0xf5, 0xb8, 0xaa, 0x4d, 0xc7, 0x83, 0x90, 0xfa, 0x22, 0x5a], // slot 4 waiver
            [0x8d, 0x66, 0x19, 0xd8, 0x04, 0x9b, 0xb0, 0xf4, 0xe8, 0x78, 0xde, 0x2b, 0x12, 0x32, 0x7e, 0x39, 0xf7, 0xcb, 0x1a, 0xda, 0x37, 0xd7, 0xb8, 0x22, 0x64, 0x10, 0xb5, 0x71, 0x8e, 0x4b, 0x8b, 0xfa], // slot 5 waiver
            [0xb3, 0xf2, 0xfc, 0x07, 0x0f, 0x85, 0x93, 0x26, 0xeb, 0x1b, 0x0e, 0x0f, 0x7d, 0x59, 0x11, 0xd0, 0x89, 0x57, 0xc3, 0x9c, 0xaf, 0x27, 0xce, 0x22, 0x7a, 0x0c, 0x61, 0x86, 0xef, 0x36, 0xc1, 0x63], // slot 6 waiver
            [0x67, 0xd9, 0x0b, 0xde, 0x01, 0x27, 0x15, 0x9c, 0xf2, 0x81, 0x4a, 0x30, 0xd6, 0x3f, 0x98, 0x3e, 0x61, 0xa1, 0x5e, 0x20, 0x57, 0x56, 0xc1, 0xab, 0xac, 0xc9, 0x27, 0x21, 0xa9, 0x09, 0xba, 0xd0], // slot 7 waiver
        ],
        review: [0xe3, 0x10, 0xc4, 0xed, 0x68, 0x26, 0x74, 0xdb, 0x92, 0xb7, 0x4d, 0x31, 0xa9, 0xdb, 0x62, 0x09, 0x62, 0x9a, 0x67, 0x26, 0x21, 0x75, 0x6c, 0xd8, 0xc8, 0x9f, 0x78, 0xf7, 0x28, 0xc8, 0x69, 0x66],
    },
];
