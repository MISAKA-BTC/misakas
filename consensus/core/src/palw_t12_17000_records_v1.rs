//! **GENERATED** by `scripts/release/gen_t12_17000_records.py` (lane ACT2) — do not edit by hand.
//! testnet-12 DAA 17,000 arm-readiness records. EVERY slot is a typed waiver (`crate::palw_arming_waiver_v1`): no
//! requirement here was run as pinned evidence. User decision 2026-10-27. A waiver lifts no ordering, value, measurement,
//! wiring or open-blocker refusal.
use crate::palw_arming_waiver_v1::{PALW_ARMING_WAIVER_REVIEW_V1, PalwArmingWaiverKindV1, PalwArmingWaiverV1};

/// Every waiver the records below carry.
pub const PALW_T12_17000_WAIVERS_V1: &[PalwArmingWaiverV1] = &[
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_close_legacy 9/9) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V1)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (a), outsider conviction) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V2/V3)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (a)-(d)) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V4)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 hard-deadline envelope and Valid-lock capacity)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_arm_legacy_d restart/reorg/IBD/pruned) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V7)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (D-02 / D-13 / D-16)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 tags 157-159 boundary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (b) held DA default) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V2/V3)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 lg14b suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 descent rounds, 34-session budget)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_arm_legacy_d) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V7)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (D-02 / D-11)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_court_capacity 15/15) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V1)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "V11 court capacity uses the user-approved estimate PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1 (cap 42, session weight 13, rho 3,125; user approval 2026-10-26 of the V1-V5/V11 estimates as values). Not a measurement: only the one-move class was measured, under load ~11 (M-CAP arm-legacy-2026-10-27); the match against all-op-class measurements and the no-load re-measurement are waived; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (rho_court; the V11 estimate 3,125 is pinned)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V3 saturating coalition)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V3 refilling coalition)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V5 maximum load)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg mid-queue)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 X3 suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (B_* terms and c_retain)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in family-fptok-report.md (g14_close_gates 12/12) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (T-N8)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in family-fptok-report.md (g14_close_gates 12/12) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (T-N10)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (TNODE run blocked by the slice ruleset)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 kernel route fold suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (g14_kernel_route_e2e; TNODE blocked)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (adversarial suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27 (soundness / beacon review)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "shadow comparison not measured; user decision 2026-10-26 treated it as complete TEMPORARILY; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (9B-8k / 2M reports)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (court resource bounds)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 provider court suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (lease reservation, bond and fee)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "U-1: Gate C activates at testnet-12 DAA 17,000 with the G14 list (user decision 2026-10-27)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (T-0 suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "U-5: certification m/k = 75/41 and the drawn verifiers bear the re-executions (user approval 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "U-6 veto: not separately ruled; armed without that ruling by user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "U-2/U-3: not separately ruled; armed without that ruling by user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (Gate C crossing, restart/IBD/reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "frame cost not measured (slowest node, release build, genesis-size classes); the lane VAL estimate PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 is pinned; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "c = 300 permille (frozen premise, PRINCIPLES 9) and kappa = 1000 permille from DAA 17,000 (user decision 2026-10-27); genesis-registered classes are 1.0 as a chain fact, catalog edition 0 is empty",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "Gate C non-shared-model test in the legacy/FP families not run; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (RFC-0019 section 11 unit tests A-E)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (RFC-0019 R-1..R-3, R-5, R-6 with the shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "R-4: no edition lists a model at 1.0 after the fence (catalog edition 0 only); user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "R-7 / R-8: no edition adds a local LLM after the fence (catalog edition 0 only); user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no annual catalog edition release manifest (edition 0 only, empty); user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "RFC-0019 readiness M1-M5 not reported as a run; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
];

/// The g14 records.
pub const PALW_T12_17000_G14_RECORDS_V1: &[crate::palw_g14_gates_v1::PalwG14ArmingRecordV1] = &[
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyPublicFilerV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x17, 0x54, 0x23, 0xb7, 0xd2, 0xf0, 0x8d, 0x66, 0x73, 0xb8, 0x33, 0xb0, 0x57, 0x45, 0x80, 0x83, 0xb0, 0x3b, 0x44, 0xe9, 0xe0, 0x44, 0x11, 0xf4, 0x4b, 0xe3, 0xa1, 0x3b, 0x38, 0x02, 0xd6, 0x83], // slot 0 waiver
            [0x6a, 0xce, 0xfa, 0x8c, 0xed, 0x7a, 0x00, 0x38, 0xdb, 0x26, 0xf1, 0xc0, 0xeb, 0xbe, 0x5b, 0x7c, 0x5e, 0x65, 0x33, 0x2c, 0x61, 0xd3, 0x24, 0xc4, 0xe2, 0x7c, 0x4f, 0x42, 0xb3, 0xee, 0xc3, 0xb6], // slot 1 waiver
            [0x81, 0x50, 0x4f, 0x4f, 0xaf, 0x06, 0xd9, 0xaf, 0xd2, 0x13, 0x91, 0x2d, 0x7f, 0x1b, 0x95, 0x84, 0x36, 0xce, 0x14, 0x1e, 0x61, 0x48, 0xfe, 0x35, 0x37, 0x0a, 0x54, 0x7e, 0xbc, 0x96, 0x8d, 0xa0], // slot 2 waiver
            [0xd5, 0xbe, 0xc5, 0xdd, 0xbc, 0x9a, 0x9e, 0x09, 0xc6, 0x95, 0xc8, 0xab, 0xa9, 0xa4, 0x31, 0x5a, 0x0f, 0x94, 0x7c, 0xb0, 0x06, 0xdf, 0x1f, 0xa0, 0x86, 0x3b, 0xc6, 0xa8, 0x2b, 0x07, 0xdd, 0xc2], // slot 3 waiver
            [0x9c, 0xd0, 0x96, 0xc7, 0xce, 0xa4, 0x7d, 0x82, 0xfb, 0x17, 0x23, 0xf3, 0x45, 0x89, 0xb7, 0x34, 0x04, 0x59, 0x7b, 0xe8, 0x0f, 0xa8, 0x25, 0xca, 0xfe, 0x30, 0x0f, 0x73, 0x10, 0xf3, 0x4f, 0x70], // slot 4 waiver
            [0x09, 0x39, 0x72, 0xd1, 0x5c, 0xb9, 0x22, 0x37, 0x1b, 0x96, 0xaa, 0x10, 0x63, 0x9c, 0x6f, 0x61, 0xa8, 0x39, 0x4d, 0xa1, 0x14, 0x70, 0x8d, 0x79, 0x1b, 0x1e, 0x8c, 0x2a, 0x0a, 0x4e, 0x08, 0xb0], // slot 5 waiver
            [0x6d, 0xd5, 0x55, 0xc5, 0xfb, 0x48, 0xb6, 0xdf, 0x28, 0xb3, 0xd7, 0xbb, 0xd7, 0x13, 0x58, 0xb0, 0x5e, 0x29, 0x7e, 0xfd, 0x4d, 0x36, 0xe7, 0x5a, 0x0f, 0x6a, 0x5a, 0xe6, 0xdb, 0xae, 0xb5, 0x33], // slot 6 waiver
        ],
        review: [0xb9, 0xae, 0xd9, 0x82, 0xeb, 0x18, 0x53, 0x26, 0xd1, 0xce, 0xf8, 0x3c, 0x87, 0x87, 0xc0, 0xe7, 0x23, 0x7f, 0xa3, 0x68, 0x22, 0xed, 0x2e, 0xef, 0x7c, 0x28, 0x8f, 0x19, 0xbe, 0x37, 0x7b, 0xb5],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyHeldDaV2,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xf5, 0x14, 0xa1, 0x86, 0x5c, 0xc8, 0xf4, 0x66, 0x97, 0x56, 0x07, 0x8f, 0xfd, 0x8d, 0x68, 0x7f, 0x7b, 0x73, 0x38, 0x79, 0x54, 0xa7, 0x3a, 0x3b, 0x42, 0x78, 0x80, 0x31, 0x02, 0x6f, 0x11, 0x8a], // slot 0 waiver
            [0xb6, 0x53, 0x42, 0x3b, 0x3c, 0x8b, 0xd0, 0x1a, 0xcd, 0xa6, 0x49, 0x32, 0x68, 0x62, 0x10, 0xfd, 0xf1, 0xa7, 0x58, 0x90, 0xec, 0xb7, 0x00, 0x99, 0x71, 0x98, 0x36, 0x90, 0x45, 0x65, 0xd9, 0xb7], // slot 1 waiver
            [0x3b, 0xd6, 0x98, 0x58, 0x5e, 0x34, 0x4b, 0xd2, 0xd5, 0xec, 0x9c, 0x2f, 0x81, 0x3a, 0xe6, 0xe1, 0x9b, 0x73, 0xfa, 0xcf, 0xd4, 0xb2, 0x6f, 0x72, 0x68, 0xd9, 0x73, 0xbb, 0x3d, 0x99, 0x5a, 0x2c], // slot 2 waiver
            [0x46, 0xa4, 0x13, 0x00, 0xd9, 0x6b, 0x1a, 0xd3, 0x95, 0xed, 0x79, 0x56, 0x98, 0xa4, 0x3a, 0x54, 0x36, 0xb8, 0x64, 0x15, 0x3e, 0xbf, 0x42, 0x07, 0xcf, 0xe4, 0x9f, 0x91, 0xbe, 0xba, 0xd7, 0x3d], // slot 3 waiver
            [0x7c, 0x21, 0x81, 0xd3, 0x8b, 0x36, 0xe7, 0x21, 0xcb, 0xc1, 0xca, 0x3e, 0xff, 0x03, 0x28, 0xfa, 0x32, 0xc3, 0x18, 0x66, 0x50, 0xd7, 0x7e, 0x5e, 0x33, 0x2a, 0xfc, 0x05, 0x6d, 0x5f, 0x0b, 0x5a], // slot 4 waiver
            [0x8a, 0x25, 0x43, 0x72, 0x8f, 0xb9, 0x81, 0x73, 0x11, 0x9a, 0xb3, 0xe3, 0x69, 0xd3, 0x9e, 0x6c, 0x3a, 0x0a, 0x74, 0xb9, 0x2b, 0x43, 0x0d, 0xbe, 0x71, 0x69, 0x92, 0xf6, 0x5f, 0x53, 0xa1, 0xc4], // slot 5 waiver
            [0x06, 0x04, 0xef, 0x3b, 0x97, 0xae, 0xae, 0x3c, 0x9b, 0x0d, 0x47, 0xb9, 0x76, 0x18, 0x85, 0xce, 0xea, 0x11, 0x2d, 0x45, 0xd4, 0x37, 0x9a, 0x4f, 0x6d, 0x89, 0x0a, 0xe3, 0x20, 0x0f, 0x6d, 0xc5], // slot 6 waiver
        ],
        review: [0xe0, 0xcd, 0xff, 0x84, 0xf0, 0xd7, 0xd2, 0x8f, 0xcd, 0x54, 0xaa, 0xb4, 0x1c, 0x79, 0x4b, 0x2f, 0x16, 0xe4, 0x50, 0x21, 0x0e, 0x2e, 0xcb, 0x4c, 0x57, 0x70, 0x71, 0xf0, 0x26, 0x0b, 0x67, 0xbc],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedPublicCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xb9, 0xc3, 0x5a, 0x89, 0xd2, 0x91, 0xdd, 0x8a, 0x41, 0xf2, 0xde, 0xf9, 0x52, 0x90, 0x63, 0x34, 0xda, 0xe1, 0xb1, 0x6d, 0xd4, 0x83, 0xa3, 0x48, 0xbe, 0xc7, 0x76, 0x3e, 0xdc, 0xb3, 0x1e, 0xfb], // slot 0 waiver
            [0x68, 0x9b, 0x3a, 0x1b, 0x36, 0x5f, 0x27, 0x97, 0x09, 0x73, 0x68, 0xc2, 0xf9, 0x69, 0xcf, 0xcc, 0x0e, 0x31, 0xdd, 0xec, 0x85, 0xcb, 0xc8, 0x24, 0x67, 0xfa, 0x43, 0x40, 0x70, 0xe9, 0x65, 0x25], // slot 1 waiver
            [0x97, 0x4f, 0xec, 0x22, 0xb1, 0x58, 0xe4, 0x97, 0x20, 0xf0, 0xe0, 0x5e, 0xf6, 0x56, 0x63, 0xa0, 0xe8, 0x31, 0x60, 0x61, 0x86, 0x17, 0x5a, 0x82, 0x4e, 0x02, 0x50, 0x69, 0x61, 0x98, 0xa5, 0xf9], // slot 2 waiver
            [0xd7, 0x3c, 0x12, 0x3b, 0x11, 0xe4, 0x5f, 0x1f, 0x95, 0x11, 0x6d, 0xd2, 0xa3, 0xea, 0x47, 0x25, 0xb7, 0xe5, 0x82, 0x65, 0xa0, 0x44, 0x93, 0x01, 0x54, 0x0f, 0x5b, 0x9c, 0x1c, 0x78, 0x7c, 0x77], // slot 3 waiver
            [0x27, 0xef, 0xee, 0x8f, 0xcc, 0x15, 0xb8, 0x2f, 0x63, 0x45, 0x69, 0x66, 0xda, 0xc7, 0x1e, 0xad, 0x23, 0x45, 0xed, 0x69, 0xf4, 0x1f, 0xa4, 0x4c, 0x9a, 0x99, 0xa6, 0x97, 0x63, 0xf0, 0x55, 0xf2], // slot 4 waiver
            [0x24, 0xbf, 0xc0, 0x3c, 0xa6, 0xa6, 0x96, 0x12, 0xfd, 0x9c, 0x20, 0x27, 0x76, 0x7b, 0xdc, 0x3f, 0x7f, 0x17, 0x62, 0xd3, 0x64, 0x00, 0xa5, 0xc4, 0xba, 0x30, 0xce, 0x00, 0x87, 0xb1, 0x4e, 0xc7], // slot 5 waiver
            [0xfa, 0x56, 0x7a, 0x73, 0xc1, 0x21, 0x09, 0xfb, 0xb7, 0x2a, 0xfa, 0xfd, 0x20, 0xdb, 0xe2, 0xa3, 0x15, 0xe9, 0xf7, 0xf1, 0x6a, 0xb6, 0x6b, 0x7e, 0xfc, 0x34, 0x14, 0x37, 0x14, 0x12, 0xd1, 0xf5], // slot 6 waiver
        ],
        review: [0x4e, 0xf9, 0x90, 0x3d, 0xa9, 0x65, 0x22, 0x15, 0xcf, 0x04, 0x4a, 0x6b, 0x32, 0x9e, 0x8e, 0xe0, 0x14, 0xcb, 0x1c, 0x09, 0xf9, 0x4e, 0x4b, 0xd4, 0x0a, 0x9b, 0x25, 0xf1, 0xba, 0x75, 0x04, 0x4f],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedTirRouteV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x55, 0x2d, 0x5d, 0x97, 0xaa, 0xa6, 0x54, 0xaf, 0xe7, 0xf7, 0x97, 0x73, 0xd3, 0xa0, 0xb6, 0xec, 0x3e, 0x4b, 0x2a, 0x09, 0xdd, 0xd3, 0x1b, 0x15, 0xa3, 0x4f, 0x26, 0x47, 0x21, 0xc5, 0x6a, 0x5b], // slot 0 waiver
            [0x9c, 0x40, 0x0c, 0x51, 0x5a, 0x7a, 0x6e, 0xdc, 0xeb, 0x8a, 0xb6, 0x1f, 0x30, 0xef, 0x07, 0xf8, 0x57, 0x24, 0x7c, 0xea, 0xca, 0xfa, 0x51, 0x26, 0xbb, 0xd5, 0x35, 0x66, 0x5d, 0xdd, 0x00, 0xc3], // slot 1 waiver
            [0x9f, 0x92, 0x93, 0x00, 0xbe, 0xac, 0xf9, 0x86, 0x2f, 0xd3, 0xdf, 0x14, 0x98, 0xc1, 0x93, 0x44, 0xec, 0x32, 0xd0, 0x9b, 0x45, 0x27, 0x39, 0x52, 0x18, 0xcc, 0x74, 0xd2, 0xd9, 0x8b, 0xb6, 0xc4], // slot 2 waiver
            [0xb6, 0xe0, 0x42, 0x8a, 0xa9, 0x19, 0x77, 0xec, 0x85, 0x1d, 0x4e, 0x77, 0x67, 0x8d, 0xf3, 0x21, 0x1b, 0xd7, 0xbc, 0x8c, 0x8f, 0x1d, 0x98, 0x15, 0x2a, 0x6b, 0x43, 0x2c, 0xdf, 0x2b, 0xf1, 0xd1], // slot 3 waiver
            [0x2c, 0x3a, 0xc5, 0x88, 0x5a, 0xa1, 0xa9, 0x6e, 0x01, 0xa7, 0x15, 0xd5, 0x13, 0x1d, 0xcd, 0x1b, 0x3a, 0x7b, 0x05, 0x5a, 0x50, 0x57, 0xb8, 0xa4, 0x6d, 0x42, 0x10, 0x4b, 0xc5, 0x0b, 0xb0, 0x2b], // slot 4 waiver
            [0x7d, 0x61, 0x45, 0x19, 0x57, 0x14, 0x9e, 0x2e, 0x14, 0x7f, 0xa4, 0xa2, 0x5d, 0x63, 0x92, 0xc3, 0xa9, 0xc8, 0x1d, 0x6e, 0xdf, 0x20, 0x1f, 0xf4, 0x29, 0x38, 0xf4, 0x57, 0x2a, 0x88, 0x9c, 0x9a], // slot 5 waiver
            [0x98, 0x06, 0x0c, 0x8d, 0x9d, 0x0f, 0x41, 0x19, 0x6e, 0xdc, 0x58, 0x07, 0x47, 0x13, 0x0f, 0x1c, 0x97, 0xff, 0x6d, 0x8b, 0x5e, 0xd8, 0xc0, 0x4c, 0xe6, 0x4b, 0x53, 0x7f, 0x0e, 0x1d, 0x9b, 0x0f], // slot 6 waiver
        ],
        review: [0x4e, 0x85, 0x31, 0xf0, 0x9b, 0x67, 0x02, 0xd4, 0x1c, 0x24, 0x52, 0xcf, 0xcf, 0x3a, 0x6d, 0xa9, 0x2a, 0x5c, 0x59, 0xdb, 0x50, 0x72, 0xc9, 0x2d, 0xfa, 0xd6, 0x23, 0x3c, 0x3d, 0x48, 0xc9, 0xf1],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::TirPublicConvictionV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xdf, 0xe7, 0x91, 0x3e, 0xdf, 0xa4, 0xbd, 0x68, 0xbd, 0x59, 0xd6, 0xe0, 0x9b, 0x91, 0xc5, 0xc5, 0xf1, 0x62, 0xf5, 0x56, 0x93, 0xac, 0x2e, 0x0f, 0x0e, 0x8b, 0x0a, 0xe4, 0x9e, 0xcf, 0x71, 0x56], // slot 0 waiver
            [0xe3, 0x29, 0x84, 0xcb, 0xc1, 0x03, 0x29, 0xe9, 0x7b, 0xeb, 0xac, 0x7d, 0xe4, 0x63, 0x0a, 0xe6, 0x36, 0x09, 0x45, 0xc6, 0x46, 0xb2, 0x52, 0xe0, 0x6f, 0x1e, 0x38, 0x63, 0x4f, 0x68, 0x8d, 0xdc], // slot 1 waiver
            [0x17, 0xbf, 0xc7, 0xb6, 0x3b, 0x49, 0xfd, 0xe3, 0x77, 0x73, 0x62, 0x1d, 0xd2, 0x57, 0x4c, 0x81, 0x13, 0x5e, 0xf5, 0x6f, 0xb8, 0x6c, 0x53, 0x24, 0xfa, 0x72, 0xeb, 0x4a, 0xa9, 0xd3, 0x94, 0x6a], // slot 2 waiver
            [0xcb, 0xa6, 0x74, 0xf6, 0x38, 0xad, 0xd3, 0x3d, 0xd0, 0xe3, 0xae, 0x44, 0xfd, 0xf1, 0x2f, 0x7d, 0x3b, 0xc2, 0xc1, 0x8c, 0x40, 0x33, 0x44, 0xec, 0xc2, 0x75, 0x42, 0x12, 0xf9, 0x05, 0x44, 0x9e], // slot 3 waiver
            [0xbd, 0xf2, 0x4c, 0x11, 0x23, 0xf7, 0xe7, 0xcd, 0x24, 0x68, 0xe7, 0xc2, 0x4e, 0x99, 0x2e, 0x22, 0x09, 0x3e, 0xd6, 0x74, 0x0f, 0xee, 0xe9, 0xd1, 0x84, 0x95, 0xa7, 0x6a, 0x2b, 0x68, 0xef, 0x5e], // slot 4 waiver
            [0xc5, 0xa4, 0x68, 0xbb, 0xa9, 0xac, 0xe8, 0x6f, 0xca, 0xd2, 0x4b, 0x38, 0x7b, 0xa8, 0xd5, 0x0a, 0x29, 0xed, 0x48, 0x31, 0xee, 0x7d, 0x35, 0xcf, 0x9c, 0xc4, 0x15, 0x95, 0xee, 0x78, 0xe7, 0x3c], // slot 5 waiver
            [0x24, 0xb2, 0xd3, 0xae, 0xd6, 0x8d, 0x3a, 0xf8, 0x4f, 0x26, 0xf5, 0xb1, 0x9c, 0x0f, 0x6b, 0x2c, 0xf7, 0x7e, 0x2f, 0x83, 0xe3, 0x4e, 0xbe, 0xda, 0x1e, 0x26, 0x39, 0xb5, 0x59, 0x51, 0x7a, 0x84], // slot 6 waiver
        ],
        review: [0x2e, 0xa7, 0x83, 0x33, 0x75, 0xac, 0x7a, 0x5e, 0x11, 0xe1, 0xa1, 0x69, 0x9c, 0x23, 0xd0, 0x80, 0x96, 0xef, 0xb7, 0x18, 0xce, 0xc7, 0xc3, 0x6e, 0x0e, 0x6c, 0xf3, 0x46, 0x7d, 0xd5, 0xb2, 0xe4],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::CourtCapacityV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x83, 0x5d, 0xde, 0xbd, 0xf9, 0x50, 0x28, 0x98, 0x83, 0xc8, 0xc3, 0x75, 0x9d, 0xf0, 0xcc, 0xe5, 0x0c, 0xe8, 0xe6, 0x79, 0x0e, 0x40, 0x89, 0x90, 0xeb, 0xa8, 0xfb, 0xbb, 0xdf, 0x1c, 0xdd, 0x49], // slot 0 waiver
            [0xbe, 0x88, 0x07, 0x0e, 0xab, 0x3c, 0xc0, 0x57, 0x54, 0xff, 0xcb, 0xca, 0x61, 0x2e, 0xf0, 0x1d, 0x7d, 0xfd, 0xe2, 0xe8, 0x7a, 0x4a, 0x60, 0xeb, 0xed, 0x5b, 0xe8, 0xad, 0x14, 0x2d, 0xbd, 0xbe], // slot 1 waiver
            [0x9e, 0xfd, 0x60, 0x00, 0xd4, 0x65, 0xf7, 0x3e, 0x5d, 0xd8, 0x23, 0xb3, 0xa8, 0x62, 0xeb, 0x95, 0xe2, 0x57, 0x57, 0x0a, 0x6b, 0x76, 0xb2, 0x90, 0xd9, 0x38, 0xa8, 0x3e, 0x4c, 0xd9, 0xc9, 0x07], // slot 2 waiver
            [0x7e, 0xd9, 0x3a, 0xc8, 0x4c, 0x77, 0xfe, 0x22, 0xdd, 0x4d, 0x1b, 0xe7, 0xd2, 0xa6, 0xa6, 0xf0, 0x71, 0xdf, 0x18, 0xef, 0x11, 0x93, 0x75, 0x8d, 0x64, 0x7d, 0xac, 0xa0, 0x14, 0x5d, 0x8e, 0xfa], // slot 3 waiver
            [0xbf, 0x44, 0xe4, 0x79, 0xbe, 0x73, 0x74, 0x11, 0xb4, 0xe8, 0xcd, 0x5a, 0x70, 0xbd, 0xbb, 0xe8, 0xd2, 0x90, 0x46, 0x36, 0x8f, 0xe1, 0x0e, 0xb3, 0x95, 0xa1, 0x20, 0x0f, 0x08, 0xf6, 0x86, 0x57], // slot 4 waiver
            [0x0a, 0x87, 0xe3, 0x82, 0x98, 0x05, 0x50, 0xe2, 0x27, 0x3d, 0xd4, 0x89, 0xf2, 0x98, 0x3b, 0x28, 0xba, 0x71, 0x6e, 0x23, 0xd0, 0xa6, 0x59, 0x4e, 0x69, 0xce, 0xfa, 0xa9, 0xc0, 0x92, 0x8d, 0xa9], // slot 5 waiver
            [0x80, 0x50, 0x8b, 0x00, 0x25, 0xa3, 0x15, 0x13, 0xa9, 0xfd, 0xa4, 0xa2, 0x86, 0x65, 0x3a, 0x12, 0xde, 0x8e, 0xc3, 0x4a, 0xbf, 0xd2, 0xcf, 0x23, 0x1e, 0x9b, 0x63, 0xcc, 0x32, 0x03, 0x8d, 0x5f], // slot 6 waiver
        ],
        review: [0x57, 0x7a, 0x9e, 0xfc, 0x47, 0x1a, 0xdb, 0xb4, 0x07, 0x42, 0x11, 0x81, 0xa9, 0xc7, 0xc6, 0x0e, 0x76, 0xd7, 0x52, 0xac, 0x0f, 0x3d, 0x22, 0x2c, 0x0a, 0x6d, 0xd1, 0x85, 0xfc, 0x53, 0x7b, 0x8c],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::ClaimHardDeadlineV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x98, 0x18, 0xe3, 0xf6, 0x40, 0x01, 0xe5, 0x04, 0xdc, 0xe4, 0x7c, 0x64, 0xdc, 0xb8, 0x61, 0x46, 0xac, 0x6f, 0xb0, 0xdd, 0xa8, 0x32, 0x80, 0xe0, 0xc8, 0x52, 0x49, 0x16, 0xb4, 0xc1, 0x02, 0x3d], // slot 0 waiver
            [0x89, 0x01, 0xfc, 0x98, 0xd8, 0xa2, 0xbf, 0xcf, 0x84, 0xd7, 0x63, 0x43, 0xba, 0xf8, 0xe8, 0x34, 0xee, 0x3f, 0xf8, 0x59, 0x07, 0xfe, 0x49, 0xe5, 0x12, 0x52, 0x28, 0xa0, 0x34, 0x22, 0x07, 0x17], // slot 1 waiver
            [0xd7, 0xb1, 0x32, 0x4f, 0x98, 0x5e, 0x89, 0xbd, 0x0e, 0x46, 0xb9, 0xc0, 0x41, 0x2e, 0xf9, 0x42, 0x4e, 0x6c, 0xc1, 0xf7, 0x60, 0x94, 0x18, 0x76, 0xc7, 0xb2, 0x06, 0x61, 0x0a, 0xba, 0xf2, 0xb7], // slot 2 waiver
            [0x94, 0xfb, 0x06, 0x7d, 0x4b, 0x4b, 0x64, 0xb1, 0x47, 0xbb, 0xbe, 0x5b, 0x43, 0xa0, 0x29, 0x1d, 0xfa, 0x5a, 0x0b, 0xa7, 0xea, 0x45, 0x64, 0xc7, 0xd6, 0x1c, 0x50, 0x91, 0x1e, 0xad, 0xa4, 0x6b], // slot 3 waiver
        ],
        review: [0x72, 0xe9, 0xc1, 0xe5, 0xb8, 0x64, 0x3a, 0x3d, 0x24, 0xad, 0xbd, 0x6a, 0xc1, 0x9e, 0x9d, 0xa8, 0xe9, 0x4f, 0xce, 0x18, 0x46, 0x1e, 0xc4, 0x94, 0x1a, 0x84, 0xdc, 0x44, 0x32, 0xf9, 0x05, 0xff],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::G14RouteRequiredV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xc2, 0x78, 0x18, 0x97, 0x3f, 0xfa, 0x4d, 0x29, 0x6f, 0x52, 0xb3, 0xff, 0x8a, 0xa2, 0xa5, 0x67, 0xe1, 0xe3, 0xf8, 0xa9, 0x90, 0xd9, 0x29, 0x00, 0xbf, 0x16, 0xa9, 0x11, 0x2e, 0xd1, 0x10, 0x13], // slot 0 waiver
            [0x58, 0xfd, 0x2a, 0xde, 0x5b, 0x14, 0xda, 0xb6, 0xad, 0x51, 0x33, 0x20, 0x6b, 0xf5, 0xd1, 0x11, 0x19, 0x9b, 0xe5, 0x24, 0xad, 0xf7, 0xed, 0xb5, 0x80, 0x7e, 0xc7, 0xc5, 0xb5, 0x5c, 0x2a, 0x2c], // slot 1 waiver
            [0xb3, 0x14, 0x25, 0xe3, 0x90, 0x9f, 0x73, 0x15, 0xd8, 0x53, 0xe4, 0x42, 0xbb, 0xde, 0x18, 0xfe, 0x30, 0x70, 0xc0, 0xec, 0x97, 0xd9, 0xec, 0x74, 0x9d, 0x6a, 0xe9, 0xc0, 0x95, 0x09, 0x14, 0xe8], // slot 2 waiver
            [0x24, 0xbd, 0x1e, 0x7f, 0x5e, 0xe1, 0xee, 0x18, 0x06, 0xab, 0x2b, 0xb8, 0x36, 0xa1, 0x68, 0x2a, 0x32, 0xdd, 0xf7, 0xbd, 0x74, 0xaf, 0x90, 0x16, 0xdf, 0xe2, 0x92, 0xce, 0xd2, 0xd8, 0x26, 0xc7], // slot 3 waiver
        ],
        review: [0x30, 0x3b, 0x91, 0x25, 0x79, 0x07, 0x25, 0x6e, 0x1d, 0x89, 0x7b, 0x6a, 0x63, 0x8a, 0xc4, 0x45, 0xeb, 0x2d, 0xb2, 0xf5, 0xd0, 0x95, 0x00, 0xb2, 0x04, 0xce, 0x1d, 0x06, 0x85, 0x85, 0xc5, 0x8f],
    },
];

/// The kernel-route records.
pub const PALW_T12_17000_KERNEL_ROUTE_RECORDS_V1: &[crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1] = &[
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x8d, 0xf2, 0xf2, 0xb4, 0x3c, 0xda, 0xa6, 0xeb, 0x30, 0x46, 0x84, 0x9d, 0x77, 0xb0, 0xb3, 0xcd, 0xd5, 0x58, 0xd3, 0x01, 0x4a, 0x29, 0xed, 0x84, 0x22, 0xa4, 0x98, 0xbc, 0xd9, 0x35, 0x81, 0xf6], // slot 0 waiver
            [0xe9, 0x70, 0x33, 0x70, 0xd8, 0x14, 0x35, 0x9a, 0xe3, 0x7c, 0xd4, 0x06, 0x7e, 0x30, 0xc7, 0x7f, 0x15, 0x84, 0x8e, 0xb1, 0xe8, 0x65, 0x63, 0xff, 0x0d, 0xb5, 0xc4, 0x86, 0x1b, 0xec, 0xa3, 0xeb], // slot 1 waiver
            [0x2e, 0x81, 0x18, 0x24, 0x2d, 0x18, 0xb0, 0x9e, 0x08, 0x83, 0x7a, 0x7e, 0x3d, 0x52, 0xef, 0xd2, 0xc7, 0x61, 0x01, 0xe8, 0x45, 0x57, 0x4c, 0x0c, 0xca, 0xf7, 0x2b, 0x08, 0x24, 0x30, 0xfb, 0x39], // slot 2 waiver
            [0x90, 0x7b, 0xb7, 0x65, 0x7d, 0x4f, 0x23, 0x19, 0xab, 0xb5, 0x36, 0x0c, 0x49, 0xb2, 0xe6, 0xd0, 0x2d, 0xe8, 0x63, 0x1f, 0x5c, 0x5f, 0x8a, 0xff, 0xa2, 0x5f, 0xb9, 0x64, 0xb2, 0x38, 0x9a, 0xe4], // slot 3 waiver
            [0xcb, 0x35, 0xab, 0x9c, 0x34, 0x57, 0xba, 0x2e, 0x7c, 0x99, 0x06, 0xac, 0x3e, 0x9d, 0x38, 0x4f, 0xe2, 0x72, 0xb0, 0x7b, 0xee, 0x66, 0xac, 0x12, 0xa4, 0x42, 0x2a, 0x46, 0xd9, 0x7a, 0x51, 0xaa], // slot 4 waiver
            [0xde, 0x87, 0xc7, 0x3a, 0x95, 0x1b, 0xd3, 0x46, 0xfe, 0x03, 0x5a, 0x9b, 0x5d, 0x32, 0x8d, 0xe2, 0x1e, 0xc6, 0xaa, 0x49, 0x5b, 0x9d, 0x49, 0x69, 0xba, 0xc2, 0x05, 0xc2, 0x0d, 0xf7, 0xda, 0xb0], // slot 5 waiver
            [0xfc, 0xd4, 0x5d, 0x69, 0x93, 0xcd, 0x52, 0x36, 0x61, 0x60, 0xb6, 0x88, 0x0c, 0x16, 0x94, 0xff, 0xda, 0x3f, 0x71, 0xab, 0x92, 0x73, 0x55, 0x74, 0x3f, 0x19, 0x50, 0x78, 0x02, 0x40, 0x67, 0xd7], // slot 6 waiver
            [0x8e, 0x4f, 0xf2, 0xf3, 0x24, 0x28, 0xd2, 0x93, 0x7d, 0xf2, 0xaa, 0xb3, 0x8d, 0x16, 0x8e, 0xc7, 0xe9, 0x57, 0x20, 0x95, 0x37, 0x34, 0xbf, 0x8c, 0x88, 0x27, 0x6d, 0xf8, 0xc0, 0x2a, 0xa0, 0x0d], // slot 7 waiver
        ],
        review: [0x7d, 0xe6, 0x9b, 0x6b, 0xdf, 0x1e, 0x4a, 0x6e, 0xe1, 0x31, 0x2c, 0xbd, 0xa0, 0x72, 0x01, 0x4e, 0x10, 0x34, 0x06, 0xc4, 0xf7, 0x7e, 0x64, 0x08, 0x22, 0x4c, 0x0c, 0x16, 0x1b, 0xe2, 0x94, 0xa5],
    },
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProviderCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x9b, 0xf9, 0x33, 0x83, 0x21, 0x42, 0xb6, 0xd6, 0x09, 0xd4, 0x2e, 0xe3, 0xc8, 0xcc, 0x58, 0x46, 0xc2, 0x68, 0x06, 0xb7, 0xa6, 0xc7, 0x89, 0x07, 0xc7, 0xba, 0x1f, 0x06, 0x4b, 0x7d, 0x49, 0x9b], // slot 0 waiver
            [0x88, 0x43, 0x71, 0xb3, 0x76, 0xf0, 0x0c, 0xfe, 0x59, 0x01, 0x8a, 0x1b, 0xf5, 0x2f, 0x0e, 0xd6, 0xd9, 0xc6, 0x24, 0x21, 0x83, 0xba, 0xed, 0x7c, 0x5e, 0xd8, 0x76, 0xf4, 0xe7, 0x54, 0xfd, 0x77], // slot 1 waiver
            [0x8f, 0xc3, 0x24, 0x0d, 0xec, 0x1a, 0x16, 0xc7, 0x4e, 0x02, 0x45, 0xcf, 0xe0, 0xed, 0xb4, 0xa5, 0x20, 0x24, 0xcd, 0xfe, 0xc7, 0xc0, 0x78, 0x80, 0x7e, 0xfa, 0xd4, 0x1e, 0x20, 0x5d, 0x95, 0xf9], // slot 2 waiver
            [0x66, 0xaf, 0x62, 0x03, 0x72, 0x50, 0x80, 0x6c, 0xf8, 0x8f, 0xd7, 0xbe, 0xa6, 0x5b, 0x89, 0x81, 0xe9, 0x8c, 0x24, 0x6c, 0xa7, 0xa8, 0x8c, 0x5e, 0x02, 0xac, 0xd4, 0x43, 0x62, 0x59, 0x66, 0xab], // slot 3 waiver
        ],
        review: [0x4a, 0xc5, 0xb9, 0x76, 0x85, 0xa2, 0x87, 0x1c, 0xa4, 0x7a, 0x82, 0x75, 0x5a, 0xfe, 0x05, 0x47, 0x4c, 0x11, 0xed, 0xbb, 0x67, 0x06, 0x69, 0x37, 0x1c, 0xb2, 0xa2, 0xe2, 0xd9, 0xd6, 0x7d, 0x2e],
    },
];

/// The rfc0018 records.
pub const PALW_T12_17000_RFC0018_RECORDS_V1: &[crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1] = &[
    crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1 {
        version: crate::palw_rfc0018_v1::PALW_RFC0018_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_rfc0018_v1::PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x44, 0xa7, 0xae, 0x4f, 0x30, 0x38, 0x53, 0xfa, 0x58, 0x3f, 0x01, 0x37, 0xa9, 0x1f, 0x56, 0xc4, 0xe2, 0x22, 0xd5, 0x9a, 0x9c, 0xd0, 0xc8, 0x0d, 0x81, 0xf7, 0x8b, 0x81, 0x8a, 0xb1, 0x04, 0x58], // slot 0 waiver
            [0x1c, 0xad, 0xff, 0x19, 0xf3, 0x1e, 0x0f, 0xf4, 0xf4, 0x16, 0x7f, 0x31, 0x6c, 0x08, 0xb2, 0xf0, 0x27, 0x18, 0x30, 0xdc, 0xcc, 0x2e, 0x60, 0x10, 0x54, 0x35, 0xf8, 0xa9, 0x51, 0xfe, 0x3f, 0xc4], // slot 1 waiver
            [0x07, 0x7b, 0x9d, 0xcc, 0x50, 0xf6, 0x20, 0x83, 0x02, 0x5d, 0xf5, 0x63, 0x82, 0x35, 0xed, 0x09, 0x37, 0x60, 0x78, 0xf5, 0x2d, 0x23, 0x79, 0x11, 0x37, 0xfb, 0x50, 0xa2, 0xb7, 0xd0, 0x26, 0x16], // slot 2 waiver
            [0x5f, 0xe6, 0x8a, 0x16, 0x3f, 0xac, 0x57, 0x90, 0x7a, 0x50, 0x7d, 0xad, 0x03, 0x33, 0xef, 0x3e, 0x8b, 0x00, 0x85, 0x52, 0xde, 0xf5, 0xb1, 0x64, 0x3c, 0x2a, 0xfd, 0x0b, 0xdd, 0x10, 0x69, 0x35], // slot 3 waiver
            [0xc4, 0x75, 0xcd, 0x9a, 0x4d, 0x6e, 0x0b, 0x0c, 0x2f, 0x3b, 0x1e, 0xa2, 0x3d, 0xfb, 0xb7, 0xf3, 0x9e, 0xa1, 0x5d, 0xdb, 0x01, 0x86, 0x34, 0x0f, 0x1f, 0x1f, 0x0c, 0x19, 0x98, 0x19, 0x0a, 0xe2], // slot 4 waiver
            [0xd5, 0x25, 0x4c, 0xb6, 0x6e, 0x69, 0x9d, 0x85, 0xd8, 0x43, 0xc1, 0x62, 0x3e, 0xc7, 0xb9, 0xeb, 0x76, 0xb1, 0xe3, 0xab, 0x38, 0x81, 0x45, 0xbd, 0x1c, 0xc6, 0xf3, 0xb0, 0xbe, 0x52, 0x00, 0x5d], // slot 5 waiver
        ],
        review: [0x6d, 0x5c, 0xe3, 0x09, 0x12, 0xfb, 0x12, 0x90, 0x43, 0xd3, 0x0b, 0xcb, 0xa8, 0x5f, 0xe5, 0x13, 0x26, 0xa8, 0xc6, 0xea, 0xde, 0x75, 0xb8, 0xda, 0x00, 0xde, 0xa2, 0x2e, 0x69, 0x26, 0x97, 0x07],
    },
];

/// The frame cost record for FpSelection: the lane VAL estimate, measurement and review waived.
pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[
    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {
        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        measurement: [0x1f, 0x88, 0xbb, 0x33, 0xb6, 0x76, 0xcd, 0x81, 0xf0, 0x67, 0x1a, 0xb3, 0x63, 0x8b, 0x07, 0x12, 0x7e, 0x3d, 0x8a, 0xeb, 0x46, 0x63, 0x9d, 0xf4, 0xdf, 0x72, 0x9c, 0xda, 0x64, 0xf3, 0x8e, 0x14],
        review: [0xa2, 0xc8, 0x4f, 0xc1, 0x6d, 0x2e, 0x4f, 0xad, 0x45, 0x1a, 0x40, 0x30, 0x85, 0x0d, 0xb1, 0xbe, 0x3c, 0x6f, 0x4e, 0xa2, 0x99, 0x7c, 0x43, 0xfb, 0x57, 0xd3, 0x82, 0x52, 0x63, 0x64, 0xab, 0x21],
        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,
    },
];

/// The model-tier record: c 300, κ 1000 (user decision 2026-10-27), catalog edition 0 (empty) as the tip.
pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[
    crate::palw_model_tier_v1::PalwModelTierArmingRecordV1 {
        version: crate::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17400,
        registered_permille: 300,
        registered_cap_permille: 1000,
        catalog_tip_edition: 0,
        catalog_tip_root: crate::Hash64::from_bytes([0xc9, 0x4d, 0xb3, 0x54, 0x2b, 0x1e, 0x0b, 0xe5, 0x8a, 0xd1, 0xda, 0x59, 0xb3, 0x9f, 0xab, 0xf3, 0x7b, 0x65, 0x0f, 0x6c, 0x3e, 0x22, 0x34, 0x8e, 0x06, 0xcb, 0x2b, 0xd6, 0xb1, 0x21, 0x61, 0x43, 0xbe, 0x35, 0x19, 0xa8, 0x0b, 0xbd, 0x68, 0x57, 0x8d, 0x62, 0xda, 0xf2, 0xd2, 0x03, 0xb7, 0xa3, 0xdb, 0x96, 0xe6, 0xd1, 0x0c, 0x95, 0xa3, 0x24, 0xce, 0xe8, 0xef, 0x9d, 0x47, 0xf7, 0x2b, 0x6a]),
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x0f, 0x4b, 0xf4, 0xf7, 0xee, 0x4e, 0x67, 0xec, 0xdb, 0x90, 0xb1, 0x60, 0xae, 0xf5, 0x8a, 0x18, 0x29, 0x31, 0x44, 0x37, 0x01, 0x23, 0x33, 0x0f, 0x39, 0x70, 0x65, 0xb7, 0x0f, 0x95, 0x00, 0xeb], // slot 0
            [0x31, 0x26, 0xd4, 0x2f, 0xeb, 0x45, 0x12, 0xe3, 0x94, 0xe6, 0x99, 0x03, 0xb1, 0x86, 0xea, 0x96, 0x59, 0x3a, 0x72, 0xff, 0x06, 0xf1, 0xf9, 0x0a, 0xbb, 0x55, 0xa2, 0xd2, 0x18, 0xa1, 0x80, 0x51], // slot 1
            [0xe5, 0x09, 0xc2, 0xb6, 0x26, 0x76, 0x1d, 0x4d, 0x2b, 0xfb, 0xa9, 0xc5, 0x78, 0xbc, 0xaf, 0x48, 0xef, 0x08, 0xcd, 0x3f, 0x7b, 0x77, 0x82, 0x79, 0xbf, 0x82, 0x41, 0xb3, 0x83, 0x4d, 0x7a, 0xbf], // slot 2
            [0x80, 0x5d, 0xc2, 0x66, 0xbd, 0x61, 0x47, 0x0d, 0x1c, 0x2f, 0x1e, 0xdf, 0x17, 0xd3, 0x15, 0xa7, 0xac, 0x9e, 0xa6, 0x37, 0xc6, 0x12, 0x44, 0x82, 0x27, 0x21, 0x71, 0x94, 0x72, 0x09, 0xf1, 0xc7], // slot 3
            [0xa6, 0xa8, 0xc1, 0x20, 0x8a, 0xf5, 0xc7, 0x73, 0x36, 0x2b, 0x14, 0xc0, 0xae, 0x11, 0xda, 0xac, 0x94, 0x1a, 0xc5, 0x8f, 0x24, 0x56, 0x52, 0xf8, 0x67, 0xd0, 0xb2, 0xea, 0x89, 0x50, 0x17, 0xc4], // slot 4
            [0x79, 0x0f, 0x78, 0xe4, 0x62, 0xfa, 0x10, 0x31, 0xfc, 0xea, 0x5f, 0x83, 0xba, 0x09, 0xa2, 0x3a, 0xea, 0xdb, 0xa2, 0xb9, 0xd9, 0x35, 0x75, 0x52, 0x7b, 0x94, 0x84, 0x13, 0x2c, 0x48, 0x12, 0xe2], // slot 5
            [0x6a, 0xa5, 0x0b, 0x77, 0x2e, 0x12, 0x4b, 0x6e, 0xcf, 0x45, 0x54, 0xe6, 0x30, 0x0f, 0x03, 0x55, 0xf3, 0xec, 0xed, 0xf5, 0xd5, 0xb2, 0xf1, 0xe1, 0xe9, 0xb3, 0x5b, 0x21, 0x3e, 0x22, 0x14, 0xce], // slot 6
            [0x10, 0xb3, 0x3d, 0xb6, 0xfe, 0x69, 0xf8, 0x60, 0xf4, 0xcf, 0x38, 0x1f, 0x63, 0x94, 0xbe, 0xb8, 0xaf, 0x71, 0xce, 0x51, 0x50, 0xce, 0xeb, 0x17, 0x3b, 0xe5, 0xbb, 0xd1, 0xd5, 0xee, 0x2d, 0xd8], // slot 7
        ],
        review: [0x7c, 0x9f, 0x3f, 0xc9, 0x7b, 0xa2, 0xe8, 0x5a, 0x3c, 0xb5, 0xa6, 0x9a, 0xad, 0xb7, 0xc9, 0x4e, 0x8c, 0x99, 0xb3, 0x03, 0x42, 0x7f, 0x3c, 0x77, 0xe0, 0x36, 0x4b, 0x4a, 0x27, 0xf8, 0x61, 0x3f],
    },
];
