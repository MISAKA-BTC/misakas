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
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_close_legacy 9/9) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V1)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (a), outsider conviction) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V2/V3)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (a)-(d)) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V4)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 hard-deadline envelope and Valid-lock capacity)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_arm_legacy_d restart/reorg/IBD/pruned) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V7)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (D-02 / D-13 / D-16)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 tags 157-159 boundary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (b) held DA default) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V2/V3)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 lg14b suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 descent rounds, 34-session budget)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_arm_legacy_d) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V7)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (D-02 / D-11)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_court_capacity 15/15) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V1)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "under-load measurement only (M-CAP arm-legacy-2026-10-27, load ~11, one-move class); the other classes, bytes and working set and the no-load re-measurement were not run; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (rho_court; the V11 estimate 3,125 is pinned)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V3 saturating coalition)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V3 refilling coalition)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V5 maximum load)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg mid-queue)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 X3 suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (B_* terms and c_retain)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in family-fptok-report.md (g14_close_gates 12/12) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (T-N8)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in family-fptok-report.md (g14_close_gates 12/12) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (T-N10)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (TNODE run blocked by the slice ruleset)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 kernel route fold suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (g14_kernel_route_e2e; TNODE blocked)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (adversarial suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27 (soundness / beacon review)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "shadow comparison not measured; user decision 2026-10-26 treated it as complete TEMPORARILY; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (9B-8k / 2M reports)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (court resource bounds)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 provider court suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (lease reservation, bond and fee)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "U-1: Gate C activates at testnet-12 DAA 17,000 with the G14 list (user decision 2026-10-27)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (T-0 suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "U-5: certification m/k = 75/41 and the drawn verifiers bear the re-executions (user approval 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "U-6 veto: not separately ruled; armed without that ruling by user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "U-2/U-3: not separately ruled; armed without that ruling by user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (Gate C crossing, restart/IBD/reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "frame cost not measured (slowest node, release build, genesis-size classes); the lane VAL estimate PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 is pinned; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
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
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x9e, 0xf4, 0x88, 0x78, 0x61, 0xf9, 0x03, 0xa7, 0xde, 0xa9, 0xf7, 0xcf, 0xe4, 0x9d, 0x1f, 0x5c, 0x16, 0x6e, 0x3d, 0xb2, 0x0e, 0x61, 0xd4, 0x40, 0x47, 0x57, 0xef, 0x54, 0xe6, 0x8b, 0xc6, 0x0b], // slot 0 waiver
            [0xc3, 0x7d, 0xe3, 0x9a, 0x45, 0x76, 0x5d, 0x3f, 0x3b, 0x3c, 0xd8, 0x0a, 0x85, 0xb2, 0xb3, 0x3a, 0x0e, 0xa8, 0xd4, 0x7b, 0x6b, 0x37, 0xcc, 0x37, 0xbd, 0x7c, 0xee, 0x48, 0x94, 0x5e, 0xc4, 0xfb], // slot 1 waiver
            [0xb2, 0xf2, 0x68, 0xf5, 0x46, 0x1b, 0xae, 0x6a, 0xe4, 0x44, 0x02, 0xa8, 0xb0, 0x84, 0x66, 0x33, 0x8f, 0x09, 0x2d, 0xb1, 0x53, 0xc5, 0x98, 0x3b, 0x23, 0x7b, 0x64, 0x24, 0x1a, 0x8c, 0x4d, 0x90], // slot 2 waiver
            [0x4e, 0x7c, 0x68, 0x2f, 0x2c, 0x62, 0x56, 0xf3, 0x84, 0x02, 0xf4, 0x4b, 0x5f, 0x23, 0xb7, 0x60, 0xcf, 0x4f, 0xd8, 0xe0, 0x3f, 0x4b, 0x31, 0x92, 0xa5, 0xe7, 0x29, 0x5c, 0xa7, 0x68, 0x35, 0x5c], // slot 3 waiver
            [0xec, 0x23, 0xf9, 0x24, 0x43, 0xb1, 0x10, 0xfa, 0x73, 0xb5, 0x90, 0xf2, 0x7d, 0x55, 0x60, 0x19, 0xcd, 0x92, 0x69, 0x29, 0xf0, 0xdd, 0x6e, 0x37, 0xd9, 0x57, 0x0f, 0x82, 0xda, 0x89, 0x9f, 0xe3], // slot 4 waiver
            [0x4b, 0x6e, 0x89, 0xf3, 0x4b, 0x59, 0xd6, 0x8c, 0x62, 0x13, 0xb3, 0x57, 0x76, 0x84, 0x1f, 0x62, 0x24, 0x41, 0x78, 0xd1, 0xe6, 0x8e, 0x05, 0x7e, 0x8a, 0x36, 0x98, 0xde, 0x8a, 0x9c, 0xf5, 0x14], // slot 5 waiver
            [0xbc, 0x1f, 0xd1, 0xdf, 0xc0, 0x91, 0x3e, 0xc1, 0x44, 0x0c, 0xc6, 0x39, 0xed, 0x1c, 0x09, 0xde, 0xba, 0x38, 0xab, 0xf1, 0xb1, 0x85, 0x7e, 0x0a, 0x51, 0x46, 0x03, 0x63, 0xa0, 0xad, 0xf9, 0xec], // slot 6 waiver
        ],
        review: [0xaa, 0x2e, 0x27, 0xc0, 0x9e, 0xdf, 0xe0, 0x5d, 0x85, 0xdf, 0x28, 0xbe, 0x99, 0xae, 0xf1, 0x64, 0x22, 0xd7, 0x60, 0x9e, 0xce, 0x2b, 0xa9, 0x12, 0xf5, 0xdb, 0xcc, 0x13, 0x85, 0x5b, 0x12, 0x19],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyHeldDaV2,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x57, 0xe0, 0x9c, 0xb8, 0xa6, 0xf4, 0xd0, 0xdc, 0x8b, 0x1a, 0xe2, 0x0f, 0x98, 0x3b, 0x1e, 0xc0, 0xfb, 0xde, 0x9d, 0x11, 0x43, 0x46, 0x99, 0xa6, 0xfb, 0x69, 0x8c, 0xb0, 0xac, 0xb1, 0x60, 0x8d], // slot 0 waiver
            [0xfd, 0xbe, 0x55, 0x69, 0x6a, 0x5e, 0xf6, 0x70, 0x6e, 0x87, 0x85, 0x58, 0x19, 0xac, 0xf5, 0xbb, 0x36, 0x71, 0x51, 0xcb, 0x87, 0xf9, 0x22, 0x20, 0x8f, 0xe3, 0xa5, 0x9f, 0x61, 0x2a, 0xc6, 0xbb], // slot 1 waiver
            [0xee, 0x66, 0x15, 0x45, 0xfb, 0x09, 0x8a, 0xa4, 0x7a, 0x3d, 0x8b, 0xf2, 0x7b, 0xa1, 0x16, 0x1c, 0xca, 0xbd, 0xb5, 0x16, 0xb4, 0x64, 0x97, 0x01, 0x6e, 0x60, 0x0e, 0x1e, 0x15, 0x2c, 0x30, 0x19], // slot 2 waiver
            [0xd3, 0xea, 0xa6, 0xf0, 0x9e, 0x68, 0x22, 0x4f, 0x3c, 0xc8, 0x67, 0x6e, 0x28, 0xd6, 0xf2, 0x53, 0x27, 0xb8, 0xd3, 0x7c, 0x3f, 0xb2, 0xaa, 0x60, 0xd0, 0x7b, 0x7b, 0x57, 0xa2, 0x98, 0xb4, 0x78], // slot 3 waiver
            [0x7d, 0xb0, 0xe4, 0x9c, 0xd6, 0x1e, 0x79, 0x42, 0x53, 0x91, 0xf4, 0x07, 0x96, 0x72, 0x34, 0x78, 0x09, 0x1d, 0xc0, 0x31, 0xe8, 0x93, 0x91, 0x42, 0x66, 0x7f, 0xa3, 0x1f, 0x3d, 0x84, 0x7f, 0x6f], // slot 4 waiver
            [0x35, 0x51, 0x3b, 0x6a, 0x17, 0x5c, 0x19, 0xb6, 0x84, 0x04, 0x2b, 0x36, 0xd3, 0xe0, 0x02, 0xe9, 0xf5, 0xa6, 0x69, 0x71, 0x55, 0xff, 0xae, 0x19, 0x0c, 0x05, 0xaa, 0xa3, 0xa7, 0x4e, 0x47, 0xb9], // slot 5 waiver
            [0x42, 0x2a, 0xae, 0x30, 0x66, 0x67, 0x26, 0xc5, 0x0f, 0xf5, 0x1f, 0x0d, 0xa9, 0x24, 0x15, 0x12, 0x5e, 0x97, 0xef, 0xba, 0xbf, 0x08, 0xae, 0xba, 0xe3, 0xf2, 0x87, 0x5c, 0xfb, 0x1b, 0xe3, 0x1e], // slot 6 waiver
        ],
        review: [0x1e, 0x11, 0xf3, 0x8e, 0x4a, 0x51, 0xb7, 0x16, 0x28, 0x78, 0xea, 0x6b, 0x5f, 0x8a, 0xc8, 0x3f, 0x2a, 0xe1, 0x0e, 0x27, 0x8a, 0x30, 0xed, 0x92, 0xd6, 0x8b, 0x59, 0x82, 0x75, 0x72, 0x38, 0xe6],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedPublicCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xfd, 0x66, 0xc9, 0x7f, 0x35, 0x35, 0x10, 0xa2, 0xe3, 0x42, 0x26, 0x65, 0xe2, 0x41, 0xa9, 0x41, 0x9f, 0x28, 0x7c, 0xa2, 0x94, 0x98, 0xdc, 0x16, 0x65, 0xa0, 0x86, 0x34, 0x6d, 0x21, 0x61, 0x5d], // slot 0 waiver
            [0x9e, 0xee, 0x73, 0x0c, 0x19, 0x47, 0x46, 0x8f, 0x43, 0xc6, 0x21, 0xc0, 0x02, 0x3b, 0x92, 0x18, 0x6c, 0x5d, 0xa2, 0xf6, 0x7c, 0xa2, 0x6a, 0xd7, 0xd9, 0xc5, 0x2e, 0x5a, 0xaf, 0x44, 0x6c, 0x58], // slot 1 waiver
            [0x71, 0xc2, 0x2c, 0x57, 0xb9, 0xa6, 0x5d, 0x59, 0xdb, 0x71, 0x74, 0x1a, 0x69, 0xef, 0x66, 0xd0, 0x63, 0x15, 0x88, 0xa7, 0x32, 0xa7, 0x12, 0xc5, 0x14, 0x96, 0xbc, 0xd7, 0xa0, 0x15, 0x41, 0xf2], // slot 2 waiver
            [0x49, 0xbe, 0x99, 0x55, 0x6c, 0xbe, 0xf4, 0x08, 0x0f, 0xea, 0xe2, 0x39, 0xcc, 0xc0, 0x94, 0x09, 0xf5, 0xc1, 0xb6, 0x66, 0xa4, 0x7e, 0x07, 0x44, 0x8e, 0xf3, 0x0d, 0xd0, 0xda, 0xbc, 0xfc, 0x5e], // slot 3 waiver
            [0xea, 0xe7, 0x51, 0x94, 0xc5, 0xaf, 0x9d, 0x63, 0x82, 0x27, 0xf8, 0x6a, 0xb9, 0x74, 0x44, 0xf3, 0xba, 0xaf, 0xeb, 0xcb, 0x0a, 0x82, 0x07, 0x1a, 0xde, 0xfa, 0xd8, 0xd3, 0xd5, 0xe5, 0x81, 0x19], // slot 4 waiver
            [0x7d, 0xd1, 0xe1, 0x15, 0x04, 0x3a, 0xf5, 0x6c, 0x56, 0x10, 0x06, 0xfe, 0x49, 0x1f, 0xc3, 0x5f, 0xe3, 0x84, 0x06, 0x88, 0x4f, 0x66, 0x38, 0x96, 0x20, 0xdb, 0xcf, 0x0e, 0xec, 0x76, 0x21, 0xdd], // slot 5 waiver
            [0xbd, 0x0b, 0x20, 0xa5, 0x04, 0x04, 0xa8, 0x27, 0x1c, 0xc1, 0xf9, 0x6d, 0x0b, 0x00, 0x72, 0xb8, 0xfe, 0x24, 0x72, 0x97, 0xae, 0xb1, 0x3d, 0xd2, 0xda, 0x97, 0xa7, 0xec, 0x35, 0x23, 0xda, 0x1f], // slot 6 waiver
        ],
        review: [0xd7, 0xdf, 0x74, 0x79, 0x06, 0x0a, 0xb4, 0xcf, 0x1f, 0xc5, 0xfc, 0x53, 0x22, 0xc7, 0x37, 0xf9, 0x6a, 0x70, 0x91, 0x8d, 0xd1, 0xba, 0x4a, 0x01, 0x30, 0xba, 0x94, 0x4d, 0x6c, 0x54, 0x17, 0xb1],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedTirRouteV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x1d, 0x28, 0x3f, 0xae, 0xf8, 0xde, 0x2c, 0x01, 0xc5, 0xad, 0xf5, 0x69, 0x5a, 0x01, 0xd1, 0xa1, 0xa9, 0x62, 0x7c, 0xf7, 0x72, 0xc9, 0x39, 0xaa, 0xbd, 0x79, 0xd3, 0x45, 0x29, 0x02, 0xd1, 0x26], // slot 0 waiver
            [0xa1, 0x92, 0x12, 0x6f, 0x98, 0x48, 0x04, 0x39, 0xc7, 0xca, 0x80, 0x19, 0xdd, 0x12, 0xbc, 0x52, 0xe8, 0xa5, 0x8e, 0x43, 0xa1, 0xbf, 0x9c, 0xcd, 0x8c, 0x70, 0x56, 0x9e, 0x2f, 0x39, 0x08, 0x4b], // slot 1 waiver
            [0x70, 0x1f, 0xee, 0x78, 0xda, 0x2c, 0x87, 0x3c, 0x0c, 0x0e, 0x54, 0x53, 0xe9, 0xaf, 0x4e, 0x71, 0xa0, 0x01, 0x59, 0x73, 0xb1, 0xcb, 0x84, 0x5e, 0x59, 0x03, 0xae, 0x36, 0x80, 0x22, 0xc2, 0xed], // slot 2 waiver
            [0x61, 0xc1, 0x48, 0x8f, 0x04, 0xc3, 0x86, 0x02, 0x9c, 0xe2, 0x22, 0xe4, 0x85, 0xbe, 0xd8, 0x55, 0x2d, 0xe9, 0x6f, 0xd8, 0xc8, 0x74, 0x9a, 0xc2, 0x61, 0x0e, 0x1d, 0xb3, 0x81, 0xd2, 0xc7, 0x9a], // slot 3 waiver
            [0xd6, 0x13, 0xbc, 0x4f, 0xd6, 0xde, 0x71, 0xd7, 0x78, 0x48, 0x77, 0xd5, 0xf4, 0x5e, 0xc5, 0x51, 0xc2, 0xd7, 0xa8, 0x68, 0xb6, 0x98, 0xc1, 0x73, 0xf3, 0x95, 0x76, 0x88, 0x5a, 0x60, 0x25, 0x89], // slot 4 waiver
            [0x5d, 0x4d, 0x15, 0xce, 0x20, 0xc2, 0xb7, 0x56, 0x33, 0x99, 0x90, 0x46, 0xdb, 0xfa, 0x96, 0xbb, 0xb3, 0xf6, 0x7b, 0x04, 0x8a, 0x41, 0x8b, 0xa8, 0xb2, 0xe7, 0x5c, 0x6c, 0x42, 0x0f, 0xc2, 0x82], // slot 5 waiver
            [0x62, 0xff, 0x14, 0xd7, 0xca, 0x3f, 0xc1, 0xad, 0x14, 0xa8, 0x44, 0x5f, 0x76, 0xa6, 0x0e, 0xdb, 0xc6, 0xb7, 0xe1, 0xe8, 0xab, 0x16, 0xaa, 0xa5, 0x4b, 0xc3, 0xb1, 0x9f, 0xfc, 0xad, 0x40, 0xb8], // slot 6 waiver
        ],
        review: [0xa0, 0x0c, 0x1f, 0x4e, 0xc0, 0xfa, 0x15, 0x12, 0xc7, 0x8d, 0xd5, 0xbb, 0x75, 0x25, 0xf4, 0xf6, 0x36, 0xff, 0xea, 0x41, 0x5e, 0xe7, 0xff, 0x10, 0xea, 0x51, 0xb4, 0xb2, 0x3f, 0xbe, 0x60, 0x5d],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::TirPublicConvictionV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x3f, 0x7c, 0xaf, 0x31, 0x4f, 0x5b, 0x42, 0x59, 0x4c, 0xe4, 0x5e, 0x80, 0x31, 0xfd, 0xca, 0xcd, 0x3b, 0xfb, 0x27, 0xd2, 0xf5, 0xab, 0xe7, 0x10, 0x05, 0xef, 0x87, 0x05, 0xc5, 0x48, 0x78, 0x5a], // slot 0 waiver
            [0xdf, 0x08, 0x6e, 0x15, 0xa5, 0x30, 0x43, 0xbb, 0xd2, 0x9a, 0xb6, 0x94, 0x90, 0xa3, 0xeb, 0x95, 0x1f, 0x1b, 0xef, 0x3a, 0x7a, 0xbe, 0x34, 0x5c, 0x7d, 0x18, 0xbc, 0x6c, 0x6e, 0x1b, 0x36, 0x5c], // slot 1 waiver
            [0xc2, 0xee, 0xc9, 0x5f, 0x85, 0xfa, 0xa1, 0xe0, 0x79, 0x21, 0xa8, 0x05, 0x94, 0x3f, 0xa9, 0xbb, 0xca, 0x13, 0xb9, 0xf0, 0x93, 0xdf, 0xdb, 0x31, 0xb1, 0xe2, 0xb4, 0xed, 0x18, 0xde, 0x86, 0x07], // slot 2 waiver
            [0x07, 0x61, 0x9c, 0x4d, 0x28, 0xec, 0x23, 0x2e, 0x2f, 0xe2, 0x7f, 0x3c, 0xfb, 0x0d, 0xcc, 0xa2, 0xa9, 0x50, 0x67, 0x4b, 0x8e, 0x83, 0xa6, 0x38, 0xee, 0x77, 0x2c, 0x94, 0x0e, 0x91, 0x12, 0xff], // slot 3 waiver
            [0x15, 0x71, 0x82, 0xdf, 0x1d, 0x77, 0xef, 0xf2, 0x90, 0x8a, 0xe7, 0xce, 0x11, 0xa5, 0x52, 0x98, 0xc7, 0x6e, 0xc4, 0xee, 0xc1, 0x1a, 0x44, 0x13, 0xd2, 0x75, 0xbf, 0x2f, 0x7a, 0x11, 0xd2, 0x1f], // slot 4 waiver
            [0x3c, 0xff, 0xcb, 0xea, 0x0d, 0x78, 0xf7, 0x95, 0xab, 0xca, 0x7f, 0xd9, 0x4b, 0xae, 0x35, 0x5c, 0xd8, 0x3a, 0xf1, 0x55, 0x15, 0x44, 0x16, 0x86, 0x51, 0xf1, 0x6d, 0x3d, 0xfb, 0x0b, 0xec, 0x73], // slot 5 waiver
            [0x54, 0x4e, 0xe3, 0x0f, 0x7e, 0xb7, 0xb6, 0x1b, 0x51, 0x25, 0x31, 0xef, 0xe6, 0x20, 0x48, 0xe0, 0x8b, 0x6f, 0xbc, 0x42, 0xfc, 0xb3, 0x73, 0x90, 0x0d, 0x40, 0xb6, 0xa5, 0x00, 0x4b, 0x33, 0xf0], // slot 6 waiver
        ],
        review: [0x05, 0x80, 0xbf, 0x8c, 0x83, 0x0e, 0x2c, 0x2f, 0x66, 0xea, 0x38, 0xca, 0x6a, 0xba, 0x7f, 0x0c, 0x6b, 0x50, 0x63, 0x9d, 0x34, 0x17, 0xd6, 0x7b, 0x94, 0xe0, 0xac, 0x0e, 0xbf, 0x80, 0x92, 0x55],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::CourtCapacityV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xe9, 0x98, 0xa7, 0x52, 0x75, 0xcf, 0x92, 0x8d, 0xd2, 0x19, 0xa9, 0x82, 0x43, 0x13, 0x5a, 0x0b, 0x0d, 0x77, 0xe1, 0x51, 0xef, 0x6e, 0x17, 0x54, 0x4b, 0x3b, 0xdf, 0xe2, 0xa9, 0x20, 0x2c, 0xe3], // slot 0 waiver
            [0x81, 0xc6, 0x2b, 0xcd, 0x37, 0xa4, 0xa5, 0x78, 0x3c, 0xb8, 0x31, 0x0e, 0x1d, 0xb0, 0xdf, 0x02, 0x01, 0x43, 0x66, 0xf9, 0xfb, 0xe8, 0xc0, 0xdf, 0x30, 0xd1, 0x66, 0x6a, 0xbd, 0x47, 0xa4, 0x34], // slot 1 waiver
            [0x2c, 0x2c, 0x6b, 0xa2, 0xb7, 0xb9, 0xd6, 0xff, 0x0f, 0x80, 0x06, 0xb8, 0xb5, 0x29, 0x5f, 0xc8, 0x80, 0x2b, 0xad, 0x69, 0x0e, 0xc8, 0x66, 0x9c, 0x17, 0xde, 0xce, 0x83, 0xb6, 0x21, 0x04, 0x4b], // slot 2 waiver
            [0x20, 0xb2, 0xd5, 0xf3, 0x65, 0x2e, 0x79, 0xa8, 0x68, 0xd8, 0x01, 0xfd, 0x06, 0x8a, 0x8b, 0x0a, 0x51, 0x00, 0x08, 0x2a, 0x26, 0x3e, 0x7a, 0xac, 0x11, 0x16, 0xc7, 0xf0, 0x0e, 0x7f, 0x89, 0xdc], // slot 3 waiver
            [0x88, 0xd1, 0x89, 0x74, 0x02, 0x73, 0x5a, 0xa4, 0x80, 0x74, 0x79, 0xb8, 0xbb, 0x89, 0xa0, 0x2a, 0xa8, 0x4b, 0xc5, 0xfc, 0xd3, 0x74, 0x15, 0x7f, 0xd3, 0x01, 0x44, 0x4e, 0x0b, 0x0d, 0xf2, 0x5f], // slot 4 waiver
            [0xba, 0x8b, 0xfd, 0xbf, 0xf5, 0xe0, 0x8f, 0x41, 0xa9, 0x3a, 0x01, 0xe1, 0xaf, 0xe8, 0xad, 0xba, 0x1a, 0x67, 0x5a, 0xdb, 0xb6, 0x4e, 0xcc, 0xad, 0x1a, 0x9b, 0xf7, 0xdf, 0x6c, 0xdf, 0xa4, 0x96], // slot 5 waiver
            [0x72, 0x5a, 0x03, 0xfc, 0xe1, 0xf8, 0x3b, 0x84, 0x82, 0x9b, 0xfe, 0x3c, 0x53, 0x87, 0x8f, 0xad, 0x79, 0x67, 0x16, 0xcc, 0x0a, 0x70, 0xac, 0x2d, 0x73, 0xb3, 0xab, 0x89, 0xdf, 0x72, 0x51, 0xad], // slot 6 waiver
        ],
        review: [0xd0, 0xa0, 0xc6, 0xae, 0x8a, 0xc4, 0x71, 0x69, 0x48, 0x12, 0xf7, 0x1b, 0xce, 0xcb, 0xda, 0x0e, 0x6b, 0x0b, 0xf3, 0x79, 0xe7, 0x13, 0x73, 0x33, 0xdd, 0x49, 0x6f, 0x9d, 0x80, 0x5f, 0xbb, 0x52],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::ClaimHardDeadlineV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xf0, 0x4b, 0x3e, 0x34, 0x1d, 0x10, 0x59, 0x1a, 0x6f, 0x60, 0x7c, 0xd0, 0x98, 0x75, 0xc7, 0x9f, 0xb5, 0xe1, 0x0b, 0x75, 0xad, 0x19, 0x02, 0x6b, 0x97, 0xd8, 0x93, 0xed, 0xc5, 0xab, 0xea, 0x51], // slot 0 waiver
            [0x58, 0x55, 0x18, 0x40, 0x88, 0x2b, 0x7f, 0x1e, 0x0f, 0x6d, 0xb1, 0x88, 0xbd, 0x3b, 0xb1, 0xc5, 0x07, 0x2c, 0x0e, 0x98, 0x3c, 0xf6, 0x8c, 0x96, 0x23, 0xc9, 0xb4, 0x83, 0x35, 0x8d, 0xa5, 0x39], // slot 1 waiver
            [0x5e, 0xb7, 0xaf, 0x67, 0x7d, 0x41, 0x01, 0x76, 0x84, 0x58, 0x16, 0xf5, 0xb7, 0x65, 0xc7, 0xb8, 0x92, 0x27, 0x08, 0xff, 0x6d, 0x98, 0x6c, 0x8b, 0x3d, 0x48, 0x4b, 0xae, 0xec, 0x35, 0x52, 0xda], // slot 2 waiver
            [0x50, 0x54, 0x92, 0x5b, 0x56, 0xd8, 0x4c, 0xd0, 0xa5, 0x8c, 0x67, 0xaf, 0x13, 0xeb, 0xa1, 0xf0, 0xe2, 0xc3, 0xeb, 0x85, 0xfc, 0xd4, 0x09, 0xf7, 0x59, 0xa5, 0xa4, 0x9f, 0x3e, 0x83, 0x9e, 0xe8], // slot 3 waiver
        ],
        review: [0x60, 0x7d, 0xb7, 0x6b, 0x71, 0x37, 0x19, 0xcc, 0x03, 0xaf, 0xa9, 0x47, 0xe7, 0x62, 0x42, 0x51, 0x19, 0x05, 0xb2, 0xc4, 0x0d, 0xbf, 0x88, 0x17, 0xc9, 0x1f, 0xb5, 0x1b, 0x46, 0xf6, 0x79, 0xac],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::G14RouteRequiredV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x1e, 0x29, 0xd0, 0xed, 0x0c, 0x7f, 0xc9, 0xde, 0x34, 0x78, 0xa2, 0x49, 0x15, 0x74, 0xa9, 0xab, 0x0e, 0x39, 0xb0, 0x28, 0x9b, 0x0e, 0xa0, 0xfd, 0x2f, 0x29, 0x54, 0x67, 0xb3, 0xa1, 0x7f, 0x2d], // slot 0 waiver
            [0x02, 0xea, 0xb9, 0x88, 0x62, 0x6d, 0xee, 0x45, 0x61, 0x8a, 0xed, 0x53, 0x4d, 0xd8, 0x06, 0x5f, 0x61, 0xef, 0x3c, 0x77, 0xab, 0x91, 0x05, 0x7d, 0xfb, 0x73, 0x0e, 0x3f, 0x23, 0x71, 0x4c, 0xc2], // slot 1 waiver
            [0x9d, 0x2f, 0xaa, 0xa7, 0x25, 0x4e, 0xdf, 0x77, 0x1d, 0x68, 0x8b, 0xfa, 0x14, 0xd3, 0x72, 0x7b, 0x60, 0x56, 0xf1, 0x44, 0x97, 0x95, 0x8e, 0x94, 0x65, 0xfb, 0xbf, 0x9e, 0xd3, 0x19, 0xfe, 0x35], // slot 2 waiver
            [0xdf, 0x0d, 0xc6, 0x28, 0xde, 0xd8, 0xd8, 0x87, 0xd5, 0x75, 0xa8, 0x45, 0x30, 0xfc, 0xa9, 0xe3, 0xcb, 0xa5, 0x59, 0x4b, 0xb6, 0xce, 0xdf, 0x00, 0xe0, 0x55, 0x29, 0x2e, 0x0c, 0x01, 0xfd, 0xab], // slot 3 waiver
        ],
        review: [0x88, 0x8e, 0x7d, 0x39, 0xbe, 0x33, 0xd1, 0xb4, 0xb0, 0x0f, 0x2b, 0x3a, 0xe1, 0x83, 0x03, 0xa1, 0x96, 0xce, 0xdd, 0x98, 0x85, 0x1e, 0x8f, 0x02, 0x08, 0x14, 0xf5, 0x18, 0x09, 0x31, 0xbb, 0x67],
    },
];

/// The kernel-route records.
pub const PALW_T12_17000_KERNEL_ROUTE_RECORDS_V1: &[crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1] = &[
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xb6, 0x1c, 0x68, 0x23, 0xa0, 0xb2, 0xd4, 0xa6, 0x77, 0x27, 0x4c, 0x7d, 0x2e, 0x83, 0x6e, 0xe1, 0x2b, 0x36, 0x03, 0xf9, 0xa1, 0xef, 0x56, 0xba, 0x04, 0x97, 0x28, 0x2f, 0x5d, 0x10, 0x5d, 0xf0], // slot 0 waiver
            [0x51, 0xb4, 0x84, 0x47, 0x20, 0xb7, 0x13, 0x41, 0xa5, 0xdf, 0xa6, 0xf5, 0xe8, 0x0c, 0x47, 0x02, 0x97, 0xd0, 0x06, 0x6c, 0x76, 0xcb, 0xcc, 0xc0, 0xc6, 0xee, 0x52, 0xbf, 0x92, 0x68, 0xd9, 0x1e], // slot 1 waiver
            [0xe9, 0x82, 0x30, 0x7b, 0x7c, 0x79, 0x1c, 0xad, 0x64, 0xb4, 0xdd, 0xcd, 0x79, 0x32, 0x0c, 0xb9, 0xb3, 0xb4, 0xa9, 0x84, 0x69, 0x7d, 0x7e, 0x2c, 0xd1, 0xd2, 0x08, 0x4a, 0x1e, 0xb0, 0x51, 0xa3], // slot 2 waiver
            [0xd2, 0x53, 0x85, 0x7d, 0xbc, 0xbf, 0x15, 0x0e, 0x1f, 0x39, 0x0d, 0x77, 0x38, 0x9b, 0xc5, 0xeb, 0x9d, 0xd8, 0xa9, 0xfb, 0x40, 0x9f, 0xfc, 0x62, 0xd0, 0x11, 0xed, 0x31, 0x87, 0x29, 0x64, 0xbc], // slot 3 waiver
            [0x45, 0xb5, 0xe1, 0x71, 0xf3, 0x94, 0x85, 0x5b, 0xb8, 0x73, 0x59, 0x76, 0xbf, 0xc7, 0xc2, 0x72, 0x2a, 0x79, 0x6d, 0xac, 0xa9, 0x69, 0x23, 0xe1, 0xd9, 0xbb, 0xd1, 0x1a, 0x41, 0x98, 0x92, 0x95], // slot 4 waiver
            [0xc9, 0xb2, 0xab, 0xda, 0x5e, 0x5f, 0x9b, 0x5d, 0xa8, 0x51, 0xc0, 0xaa, 0x1d, 0x97, 0xf4, 0xe2, 0xeb, 0xcc, 0x4c, 0x81, 0x78, 0x28, 0x3e, 0x74, 0x18, 0x06, 0x23, 0xc7, 0x23, 0x4f, 0x4f, 0xb9], // slot 5 waiver
            [0x8d, 0x14, 0xa5, 0xbb, 0x3a, 0xeb, 0xd4, 0x7f, 0x2f, 0x38, 0x13, 0x82, 0xe7, 0xfa, 0xcd, 0xd2, 0xab, 0x20, 0x3b, 0xa6, 0x01, 0x85, 0x35, 0x8a, 0x8b, 0x69, 0x6c, 0x9b, 0xb7, 0x68, 0xe7, 0x60], // slot 6 waiver
            [0x02, 0x32, 0x4d, 0xf4, 0xb3, 0xf4, 0x44, 0x2c, 0xde, 0xaf, 0xf1, 0x6e, 0x9b, 0xa7, 0xcd, 0x35, 0xa1, 0xe3, 0x6b, 0x2f, 0xcf, 0x61, 0xbd, 0x2c, 0xef, 0xe8, 0x26, 0x54, 0x26, 0xc5, 0xf3, 0x08], // slot 7 waiver
        ],
        review: [0x2e, 0xf6, 0xe5, 0x6f, 0xe2, 0xf4, 0x47, 0xc9, 0xd3, 0x05, 0x70, 0x17, 0x11, 0x9f, 0xde, 0x15, 0x41, 0xf0, 0x1f, 0x67, 0xd6, 0x7f, 0xe6, 0x13, 0xdb, 0x67, 0x6f, 0x65, 0xa6, 0x01, 0x53, 0x18],
    },
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProviderCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xa2, 0x94, 0x0f, 0x2a, 0x87, 0x6b, 0x85, 0x7e, 0x89, 0xc7, 0x14, 0x13, 0xb2, 0x77, 0x8e, 0x21, 0x8d, 0x62, 0x1e, 0x75, 0x4a, 0x3a, 0x34, 0xa5, 0x77, 0x68, 0xd1, 0xb9, 0x94, 0x3f, 0x50, 0x56], // slot 0 waiver
            [0xfa, 0x53, 0x38, 0xec, 0xf9, 0x46, 0xb3, 0xab, 0x98, 0xf3, 0xce, 0xbb, 0x2c, 0x12, 0x72, 0x3c, 0x83, 0x55, 0x18, 0xd6, 0x61, 0x55, 0x4b, 0x18, 0xb1, 0xf3, 0x75, 0xc3, 0xea, 0x08, 0xdb, 0x18], // slot 1 waiver
            [0x40, 0x67, 0xc7, 0xcf, 0x12, 0x3b, 0xbe, 0x2c, 0xaa, 0xb2, 0x6c, 0xd2, 0x15, 0xc6, 0x3c, 0x77, 0xd6, 0xed, 0x8e, 0xad, 0x26, 0xa4, 0x74, 0xd0, 0x8f, 0x1c, 0x6e, 0x2f, 0x64, 0x31, 0xe3, 0xfd], // slot 2 waiver
            [0xa0, 0x07, 0x61, 0x67, 0xb8, 0xe8, 0x31, 0x82, 0xb5, 0x3d, 0x02, 0xc3, 0x40, 0x37, 0x59, 0x05, 0xba, 0xb2, 0xc3, 0x79, 0x46, 0xf0, 0xb0, 0xb3, 0x39, 0x84, 0xaa, 0x9d, 0x60, 0x31, 0x6b, 0x4f], // slot 3 waiver
        ],
        review: [0x43, 0xbc, 0xa1, 0x56, 0x54, 0xc5, 0x84, 0xde, 0x85, 0x6d, 0x74, 0x45, 0x88, 0x4e, 0x59, 0x04, 0xd6, 0xb1, 0x68, 0xd8, 0xe2, 0xa7, 0x38, 0x61, 0xfe, 0x96, 0xca, 0xc5, 0x37, 0x8e, 0x9d, 0x22],
    },
];

/// The rfc0018 records.
pub const PALW_T12_17000_RFC0018_RECORDS_V1: &[crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1] = &[
    crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1 {
        version: crate::palw_rfc0018_v1::PALW_RFC0018_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_rfc0018_v1::PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xef, 0xef, 0xbe, 0x0e, 0x6a, 0x4c, 0x95, 0x2d, 0xef, 0xe9, 0xbd, 0x2e, 0xe1, 0x0c, 0xcb, 0x1d, 0x51, 0x6f, 0xfb, 0xe3, 0x99, 0xa7, 0xe2, 0x37, 0xe8, 0xa4, 0xc5, 0xc6, 0x32, 0x31, 0xc2, 0xd1], // slot 0 waiver
            [0xa8, 0xf1, 0x9a, 0x96, 0xe3, 0xcf, 0x08, 0x3c, 0x95, 0x98, 0x55, 0x27, 0xd4, 0x21, 0x36, 0xc5, 0x21, 0xf2, 0x72, 0x9a, 0x7a, 0x75, 0xc7, 0xbb, 0x86, 0xba, 0x32, 0x23, 0xf0, 0xef, 0x21, 0x5d], // slot 1 waiver
            [0xdf, 0x2a, 0xcb, 0x75, 0x57, 0x1c, 0x55, 0x74, 0x36, 0x91, 0xaa, 0x28, 0xe9, 0x4c, 0xb7, 0xd5, 0x05, 0x1d, 0x65, 0x33, 0xd6, 0xf2, 0x52, 0xee, 0x55, 0x2e, 0x5d, 0xcc, 0x2a, 0xd9, 0x1f, 0xea], // slot 2 waiver
            [0x52, 0x02, 0xe4, 0x11, 0xb8, 0x3a, 0x77, 0x13, 0x6a, 0x74, 0x16, 0x67, 0x2a, 0xad, 0x0b, 0x51, 0xe3, 0x98, 0xcf, 0x45, 0xab, 0xd4, 0x0a, 0x7f, 0x9d, 0x69, 0x57, 0xe1, 0x6b, 0xce, 0xf1, 0x2f], // slot 3 waiver
            [0xf3, 0x9f, 0x0b, 0x7e, 0x0c, 0x00, 0xa4, 0x5f, 0x4f, 0x97, 0x5e, 0x85, 0x62, 0xd7, 0xc2, 0x46, 0xcd, 0x79, 0x8b, 0x7a, 0x73, 0x36, 0x0c, 0xf8, 0x94, 0x4e, 0xe8, 0x18, 0xf6, 0x79, 0x58, 0x91], // slot 4 waiver
            [0x6f, 0x4b, 0x9b, 0x6e, 0xbd, 0x33, 0xcf, 0x77, 0x4d, 0x36, 0xc8, 0xea, 0x6d, 0x7e, 0xa6, 0xc7, 0x4f, 0x46, 0x97, 0x5c, 0x0b, 0x08, 0x03, 0x75, 0x82, 0x6f, 0x6e, 0x2e, 0x70, 0xc4, 0xaa, 0xfd], // slot 5 waiver
        ],
        review: [0x1a, 0xdf, 0xde, 0x80, 0xe6, 0xe4, 0xdc, 0xa6, 0xaa, 0xa0, 0x23, 0x1c, 0xb7, 0x37, 0xf3, 0xc2, 0x83, 0x56, 0xf8, 0xa2, 0x66, 0x02, 0x16, 0x76, 0xa4, 0x32, 0x43, 0xad, 0xb4, 0xf1, 0x7d, 0xe0],
    },
];

/// The frame cost record for FpSelection: the lane VAL estimate, measurement and review waived.
pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[
    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {
        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 17000,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        measurement: [0xa5, 0x80, 0xfa, 0x71, 0x64, 0x3e, 0x19, 0x82, 0x4f, 0xa2, 0xf8, 0x96, 0x28, 0xc5, 0xa8, 0x81, 0x34, 0x9b, 0x2f, 0x5b, 0x7b, 0x59, 0x0b, 0xc7, 0x9d, 0x53, 0xf9, 0x7a, 0x8c, 0xae, 0x33, 0x06],
        review: [0x8f, 0x3e, 0x71, 0x14, 0x7f, 0xfe, 0xc6, 0xd8, 0xd6, 0x98, 0x1d, 0x7a, 0xa0, 0x07, 0x00, 0xa5, 0x7e, 0x95, 0x26, 0xfd, 0xd9, 0xf3, 0x49, 0xa4, 0x80, 0x1c, 0xee, 0xce, 0x97, 0x5d, 0x88, 0x59],
        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,
    },
];
