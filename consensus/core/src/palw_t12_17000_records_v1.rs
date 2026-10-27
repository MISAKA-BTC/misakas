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
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_close_legacy 9/9) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V1)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (a), outsider conviction) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V2/V3)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (a)-(d)) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V4)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 hard-deadline envelope and Valid-lock capacity)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_arm_legacy_d restart/reorg/IBD/pruned) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V7)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (D-02 / D-13 / D-16)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_public_filer_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 tags 157-159 boundary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (node E2E (b) held DA default) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V2/V3)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 lg14b suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 descent rounds, 34-session budget)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_arm_legacy_d) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V7)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (D-02 / D-11)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_legacy_held_da_v2",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_public_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_staged_tir_route_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 build / regressions / boundary decode)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V2 one correct seat)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V3 full collusion, one outside bond)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V4 every violation family and honest twin)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (V5 maximum profile and delay under contention)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V6 real network and economic accounting)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_public_conviction_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in arm-legacy-report.md (g14_court_capacity 15/15) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (V1)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "V11 court capacity uses the user-approved estimate PALW_G14_COURT_CAPACITY_POLICY_ESTIMATE_V1 (cap 42, session weight 13, rho 3,125; user approval 2026-10-26 of the V1-V5/V11 estimates as values). Not a measurement: only the one-move class was measured, under load ~11 (M-CAP arm-legacy-2026-10-27); the match against all-op-class measurements and the no-load re-measurement are waived; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (rho_court; the V11 estimate 3,125 is pinned)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V3 saturating coalition)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V3 refilling coalition)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (V5 maximum load)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (V7 restart / IBD / pruned / reorg mid-queue)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 X3 suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (B_* terms and c_retain)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_claim_hard_deadline_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in family-fptok-report.md (g14_close_gates 12/12) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (T-N8)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "run reported in family-fptok-report.md (g14_close_gates 12/12) but no run-output file in the pin form was kept; counted as not run; user decision 2026-10-27 (T-N10)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (TNODE run blocked by the slice ruleset)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_g14_route_required_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 kernel route fold suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed) (g14_kernel_route_e2e; TNODE blocked)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (adversarial suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27 (soundness / beacon review)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "shadow comparison not measured; user decision 2026-10-26 treated it as complete TEMPORARILY; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (9B-8k / 2M reports)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (court resource bounds)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_probabilistic_constraints_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (V1 provider court suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "node E2E not run on testnet-12 as pinned evidence; user decision 2026-10-27 (E2E not executed)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not measured; user decision 2026-10-27 (unmeasured; the lane VAL estimate is what is pinned) (lease reservation, bond and fee)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_provider_court_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "U-1: Gate C activates at testnet-12 DAA 17,000 with the G14 list (user decision 2026-10-27)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (T-0 suite)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "U-5: certification m/k = 75/41 and the drawn verifiers bear the re-executions (user approval 2026-10-26)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "U-6 veto: not separately ruled; armed without that ruling by user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "U-2/U-3: not separately ruled; armed without that ruling by user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (Gate C crossing, restart/IBD/reorg)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_independently_work_certified_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "frame cost not measured (slowest node, release build, genesis-size classes); the lane VAL estimate PALW_G14_TIR_FRAME_COST_ESTIMATE_V1 is pinned; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_tir_frame_cost_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: PALW_ARMING_WAIVER_REVIEW_V1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no independent review output in the pin form; the 2026-10-26 Claude-agent reviews (independent-review-2026-10-26*.md) are documents, not this record's review; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 0,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "c = 300 permille (frozen premise, PRINCIPLES 9) and kappa = 1000 permille from DAA 17,000 (user decision 2026-10-27); genesis-registered classes are 1.0 as a chain fact, catalog edition 0 is empty",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 1,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "Gate C non-shared-model test in the legacy/FP families not run; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 2,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "not run as pinned evidence; user decision 2026-10-27 (armed at DAA 17,000 without it) (RFC-0019 section 11 unit tests A-E)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 3,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "drill not run; user decision 2026-10-27 (drill skipped) (RFC-0019 R-1..R-3, R-5, R-6 with the shipped binary)",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 4,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "R-4: no edition lists a model at 1.0 after the fence (catalog edition 0 only); user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 5,
        kind: PalwArmingWaiverKindV1::UserDecision,
        reason: "R-7 / R-8: no edition adds a local LLM after the fence (catalog edition 0 only); user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 6,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "no annual catalog edition release manifest (edition 0 only, empty); user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        requirement: 7,
        kind: PalwArmingWaiverKindV1::NotRun,
        reason: "RFC-0019 readiness M1-M5 not reported as a run; user decision 2026-10-27",
        decided_by: "user 2026-10-27",
    },
    PalwArmingWaiverV1 {
        fence: "palw_model_tier_v1",
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
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
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xcb, 0xf0, 0x0b, 0x89, 0xce, 0x42, 0xb8, 0x98, 0x3e, 0x0c, 0x9f, 0x1e, 0xe4, 0x70, 0x30, 0x4b, 0x67, 0x9d, 0x07, 0xbe, 0x0f, 0x7e, 0x2b, 0x6e, 0x42, 0x48, 0x88, 0xc4, 0xce, 0xe7, 0xc8, 0x16], // slot 0 waiver
            [0xd4, 0xb8, 0x6e, 0xec, 0xb3, 0x16, 0xe5, 0xe6, 0x9b, 0x78, 0x3a, 0x9d, 0x29, 0x01, 0x47, 0x63, 0x63, 0x09, 0xcd, 0xc0, 0x23, 0xd0, 0x04, 0x10, 0x48, 0x21, 0x54, 0x93, 0xfe, 0x4d, 0xdd, 0xc7], // slot 1 waiver
            [0x4c, 0x6d, 0x4b, 0x01, 0x54, 0xa7, 0xad, 0x3a, 0x23, 0x9f, 0x3f, 0xff, 0xd1, 0x04, 0x2a, 0xc4, 0x39, 0xea, 0x46, 0x1a, 0xa6, 0x19, 0x52, 0xbe, 0x57, 0x49, 0xdb, 0xc7, 0x09, 0xec, 0x21, 0x68], // slot 2 waiver
            [0x4f, 0x5c, 0xf7, 0xb6, 0xf2, 0x06, 0x9e, 0xf3, 0x93, 0xf4, 0x07, 0xa2, 0x96, 0x7d, 0xdd, 0xc4, 0xd2, 0xe5, 0x16, 0x64, 0xa4, 0x1a, 0x63, 0x8c, 0x14, 0x19, 0x09, 0xb4, 0xb5, 0xa9, 0xe0, 0xa7], // slot 3 waiver
            [0x68, 0x23, 0xf8, 0x21, 0x0e, 0x89, 0xd0, 0x7b, 0x55, 0x28, 0xc8, 0x84, 0x08, 0x84, 0x9f, 0x94, 0xd5, 0x2c, 0x90, 0x2c, 0x86, 0xf6, 0x09, 0xb3, 0x0b, 0x44, 0xde, 0x76, 0x33, 0x1e, 0x07, 0x92], // slot 4 waiver
            [0x06, 0xb3, 0x64, 0xa3, 0x8a, 0xef, 0xf3, 0x75, 0x39, 0x6d, 0x37, 0xef, 0x9b, 0xa5, 0x38, 0xc4, 0x46, 0x03, 0x92, 0x1f, 0x82, 0x61, 0xd7, 0x95, 0xb2, 0xc6, 0x4d, 0x4c, 0x08, 0x60, 0x3a, 0x93], // slot 5 waiver
            [0x4e, 0xf7, 0x04, 0x78, 0x46, 0x76, 0x1f, 0x4c, 0x1f, 0x99, 0x53, 0x43, 0xd4, 0xc0, 0xa1, 0x6a, 0x2b, 0x0a, 0xa5, 0x2d, 0xd3, 0xd4, 0x87, 0xc0, 0x91, 0x3f, 0x90, 0x66, 0x50, 0x69, 0x32, 0xcf], // slot 6 waiver
        ],
        review: [0x15, 0x3b, 0x4a, 0x0e, 0x65, 0x05, 0x66, 0x1c, 0x02, 0x46, 0x4e, 0x83, 0xc4, 0x81, 0xb1, 0x21, 0xfd, 0x56, 0xb8, 0x25, 0x75, 0x69, 0xe8, 0xa0, 0x95, 0x09, 0x2b, 0x06, 0x9e, 0x61, 0x28, 0x58],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyHeldDaV2,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xdd, 0x01, 0x5d, 0xb2, 0x71, 0x91, 0x57, 0x0e, 0xe8, 0x3d, 0xf7, 0x45, 0x79, 0x68, 0x15, 0x28, 0x9f, 0xca, 0xee, 0xe0, 0x3d, 0xa9, 0x2f, 0xf7, 0x8d, 0x5a, 0x68, 0xda, 0x34, 0x3a, 0x64, 0xb6], // slot 0 waiver
            [0xca, 0x2d, 0x30, 0x66, 0x6d, 0x57, 0x99, 0x9f, 0x8a, 0xf1, 0x25, 0x66, 0x67, 0xac, 0x4d, 0xfd, 0x7f, 0xf8, 0x59, 0x39, 0x57, 0x9b, 0x5e, 0x1f, 0x64, 0xc1, 0x28, 0x61, 0xcf, 0x52, 0x1b, 0xe8], // slot 1 waiver
            [0x22, 0x90, 0xc1, 0x70, 0x87, 0xf3, 0x8c, 0x12, 0x75, 0x54, 0xae, 0x03, 0x1f, 0x4c, 0xd4, 0xfb, 0xa7, 0x81, 0x5f, 0xba, 0x4f, 0xd1, 0x47, 0x2b, 0xd8, 0x85, 0x47, 0x41, 0xcb, 0x05, 0xc9, 0x20], // slot 2 waiver
            [0xd8, 0x1e, 0x13, 0x49, 0xa5, 0xe8, 0x39, 0x24, 0xe7, 0xb3, 0xb6, 0x18, 0x0e, 0xcc, 0xf7, 0x47, 0x81, 0x0b, 0x0e, 0xcc, 0xde, 0x3b, 0xe0, 0x55, 0x6b, 0x7a, 0x7a, 0xe6, 0xdb, 0x9c, 0x6b, 0xf0], // slot 3 waiver
            [0x0d, 0xf3, 0x17, 0x39, 0x1c, 0xaa, 0xae, 0x73, 0xce, 0x3a, 0x41, 0x9b, 0xc8, 0xd1, 0xd1, 0x01, 0x2f, 0x6b, 0x8b, 0xc5, 0x37, 0xa2, 0xfe, 0xbc, 0x5d, 0x43, 0x2d, 0x27, 0xbb, 0x0b, 0xe1, 0xa9], // slot 4 waiver
            [0xa7, 0xba, 0x34, 0x5e, 0xfc, 0xc7, 0xdc, 0x52, 0x80, 0xda, 0x25, 0x32, 0x68, 0xd1, 0x98, 0xbc, 0x08, 0x38, 0x9f, 0x74, 0x1f, 0x62, 0xfd, 0x15, 0x94, 0xa8, 0x8b, 0xa7, 0xcc, 0x81, 0xf0, 0x68], // slot 5 waiver
            [0x95, 0xb5, 0xa5, 0x3c, 0x6c, 0x7b, 0x10, 0xe5, 0xff, 0xf6, 0x17, 0x80, 0xb9, 0x15, 0x9b, 0x66, 0x22, 0x18, 0x77, 0x23, 0x74, 0x4e, 0xb0, 0xe9, 0xf7, 0xff, 0x92, 0x94, 0xa8, 0x25, 0x2a, 0x49], // slot 6 waiver
        ],
        review: [0x5c, 0x5b, 0x2e, 0x10, 0x9c, 0x52, 0x11, 0xfd, 0x65, 0xfa, 0xa1, 0xf8, 0xb2, 0x8e, 0x92, 0x55, 0x51, 0xcd, 0xfc, 0xd1, 0xa6, 0xc2, 0xed, 0xca, 0xf9, 0x39, 0xb6, 0xcc, 0x2f, 0xb5, 0x77, 0x3b],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedPublicCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xa0, 0xe4, 0x39, 0x77, 0x52, 0x3d, 0x52, 0x44, 0xe4, 0xa6, 0xd4, 0x07, 0x70, 0x1e, 0xf8, 0x79, 0xb7, 0xef, 0x2f, 0x17, 0x69, 0xac, 0xc2, 0x6c, 0x8c, 0x57, 0x68, 0xac, 0xd1, 0x68, 0xca, 0x0a], // slot 0 waiver
            [0x4d, 0xa4, 0x7a, 0x0e, 0x6b, 0xb1, 0x63, 0x6f, 0x54, 0x39, 0xb2, 0xe6, 0xbb, 0xa5, 0x33, 0x22, 0xda, 0x48, 0x5f, 0x68, 0xa3, 0x39, 0x20, 0x35, 0x2b, 0xd2, 0x55, 0x98, 0x97, 0x79, 0xf9, 0x96], // slot 1 waiver
            [0xe2, 0xa6, 0xc8, 0x8e, 0x2d, 0x63, 0xf6, 0x02, 0xd0, 0x91, 0x89, 0x25, 0xfb, 0x51, 0x19, 0x70, 0xbd, 0xf6, 0x3b, 0x42, 0xc2, 0x45, 0xb5, 0x7d, 0xa0, 0x77, 0x60, 0x0d, 0xd4, 0xd6, 0xa2, 0x6b], // slot 2 waiver
            [0x1b, 0x1b, 0x3d, 0x87, 0x51, 0x00, 0x7c, 0x14, 0xe4, 0xd7, 0x9a, 0xa6, 0xa2, 0xf1, 0x91, 0xb3, 0x3e, 0x79, 0xe4, 0xdc, 0xb7, 0x1a, 0x88, 0xc6, 0x63, 0xe9, 0xe7, 0x18, 0x01, 0xef, 0x2a, 0x7b], // slot 3 waiver
            [0xd6, 0xe2, 0x92, 0x88, 0xb2, 0x5a, 0xf8, 0x2b, 0xe8, 0xc2, 0x29, 0xb0, 0x45, 0xab, 0x45, 0x73, 0x41, 0xd9, 0x78, 0xce, 0x2a, 0x70, 0xf7, 0xca, 0x95, 0x65, 0xfe, 0x95, 0x16, 0x5c, 0x7b, 0xcc], // slot 4 waiver
            [0xa4, 0x9f, 0x83, 0x0b, 0x36, 0x76, 0xa8, 0x58, 0xc3, 0x2a, 0x34, 0x66, 0xe2, 0x2c, 0x77, 0x1e, 0xe8, 0x88, 0x16, 0x71, 0xec, 0xa2, 0x3e, 0x57, 0x00, 0xfe, 0x55, 0x70, 0x2f, 0xf0, 0x03, 0x2b], // slot 5 waiver
            [0x23, 0xa1, 0xec, 0x81, 0x2f, 0x41, 0xc9, 0x3a, 0xee, 0x70, 0xcd, 0xaf, 0x8a, 0x13, 0x0d, 0x1e, 0x6b, 0x02, 0x12, 0xfb, 0xca, 0xd9, 0x4d, 0x25, 0xc9, 0xbe, 0x2e, 0xf7, 0x67, 0xce, 0x74, 0x12], // slot 6 waiver
        ],
        review: [0x2a, 0xd3, 0x26, 0x2a, 0x6f, 0x43, 0x34, 0xab, 0x39, 0x70, 0x74, 0xc8, 0x89, 0xbf, 0x1e, 0x4b, 0xae, 0xfd, 0xd5, 0x76, 0x0a, 0x4e, 0x4c, 0xd0, 0x99, 0x23, 0x48, 0xaa, 0xb4, 0x88, 0xa8, 0x09],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::StagedTirRouteV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x36, 0x8a, 0xb8, 0x81, 0x11, 0x63, 0x92, 0x49, 0x2a, 0xe7, 0x9f, 0x73, 0xc8, 0x0f, 0xe5, 0x7b, 0xd0, 0xc0, 0x93, 0xd2, 0x48, 0x49, 0xed, 0x57, 0xf0, 0xb6, 0x9e, 0x11, 0xe4, 0x8c, 0x23, 0x20], // slot 0 waiver
            [0x36, 0xd8, 0x5f, 0xff, 0xc1, 0xd0, 0x22, 0x19, 0x57, 0x0c, 0x3c, 0x58, 0xd0, 0xa5, 0xb2, 0x35, 0xf7, 0x2e, 0x1c, 0xe7, 0x0a, 0x61, 0x28, 0x52, 0x84, 0x8e, 0x45, 0x5f, 0x6a, 0x4a, 0x51, 0x39], // slot 1 waiver
            [0x37, 0x6a, 0xfa, 0xa9, 0xee, 0x7d, 0x38, 0x52, 0x18, 0x7a, 0xe8, 0xec, 0x28, 0x38, 0xa6, 0xec, 0x8e, 0xe9, 0xb3, 0x35, 0xf5, 0xd4, 0xcc, 0xd8, 0xbc, 0x4f, 0xe2, 0xd6, 0xe3, 0xd3, 0xa5, 0x66], // slot 2 waiver
            [0xbd, 0x7f, 0x0e, 0x95, 0x35, 0x9d, 0xc0, 0x7f, 0x79, 0x00, 0x72, 0x35, 0x0d, 0xc9, 0x4c, 0xa9, 0xc6, 0x36, 0xbb, 0x9d, 0xaa, 0x3d, 0xef, 0x9f, 0x0b, 0x43, 0x5f, 0xed, 0x4c, 0xc6, 0x27, 0x16], // slot 3 waiver
            [0xd2, 0x59, 0xc7, 0x53, 0xd1, 0x17, 0xe2, 0x48, 0x08, 0xf4, 0xea, 0x77, 0x75, 0x38, 0x0c, 0x08, 0xc2, 0x5f, 0xd5, 0xdc, 0x0d, 0x8d, 0xd1, 0x0d, 0x04, 0x38, 0x10, 0x2b, 0xd8, 0xed, 0xa9, 0x26], // slot 4 waiver
            [0x76, 0xc8, 0xd0, 0xb9, 0xff, 0xaf, 0x59, 0xe5, 0xc8, 0x54, 0x5a, 0x83, 0x60, 0x58, 0x9e, 0x6e, 0x50, 0xf9, 0x00, 0x77, 0x15, 0x6e, 0xcb, 0x42, 0xe6, 0x3f, 0x41, 0x1d, 0x63, 0x90, 0x29, 0xee], // slot 5 waiver
            [0x75, 0x0a, 0x39, 0x3c, 0x42, 0x8d, 0x74, 0x4f, 0xb2, 0x5e, 0x78, 0x29, 0x5a, 0x26, 0x3e, 0xcb, 0xdf, 0x29, 0xc5, 0xe5, 0x6c, 0x73, 0x30, 0xbe, 0x25, 0xb1, 0x97, 0x4d, 0xd4, 0xb1, 0xe6, 0xc2], // slot 6 waiver
        ],
        review: [0x59, 0xca, 0x95, 0x2a, 0xfc, 0x41, 0x06, 0x66, 0xd2, 0xeb, 0x19, 0x7a, 0xcb, 0x26, 0x98, 0x12, 0xbe, 0xfa, 0x20, 0xac, 0x60, 0x16, 0xcb, 0x56, 0x15, 0xed, 0xb4, 0x8f, 0xce, 0xf5, 0xbb, 0xe9],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::TirPublicConvictionV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xf5, 0xd3, 0x31, 0x15, 0x9e, 0x35, 0x45, 0xe0, 0x5b, 0x42, 0x8b, 0x93, 0x6d, 0x38, 0x1a, 0x05, 0x41, 0xab, 0x61, 0xf3, 0x25, 0xba, 0x9e, 0x90, 0xb6, 0xbd, 0xf0, 0xeb, 0x3f, 0x43, 0xef, 0xd9], // slot 0 waiver
            [0x8d, 0xfb, 0x71, 0x1f, 0xb4, 0xcc, 0x68, 0x80, 0xe1, 0xd9, 0xd2, 0x7c, 0xd1, 0x14, 0xf9, 0xb4, 0x2a, 0x41, 0x2d, 0xe3, 0x7b, 0x0e, 0xb1, 0x5d, 0x77, 0x79, 0x51, 0xf0, 0x36, 0x8e, 0x5e, 0xa6], // slot 1 waiver
            [0x2d, 0xcf, 0x3c, 0x8e, 0xe0, 0x89, 0xfd, 0x0e, 0xe9, 0xa9, 0x7d, 0x25, 0xd3, 0x66, 0x69, 0x97, 0x46, 0x8e, 0xfd, 0x3d, 0xfe, 0x26, 0x8c, 0x72, 0xa5, 0x91, 0x02, 0x78, 0x4f, 0x79, 0xa1, 0x1a], // slot 2 waiver
            [0x9a, 0x59, 0xeb, 0x4d, 0xad, 0xd2, 0xf4, 0x23, 0x68, 0x2a, 0x57, 0x71, 0x94, 0x6f, 0xe3, 0x41, 0x2b, 0x1b, 0x9c, 0x10, 0xf9, 0xc2, 0x49, 0x5d, 0x15, 0x3c, 0xfb, 0x5e, 0x26, 0x6c, 0x8a, 0x14], // slot 3 waiver
            [0xe6, 0xf5, 0x5e, 0xa7, 0x01, 0x6d, 0xfa, 0x56, 0x76, 0xeb, 0xd8, 0xd6, 0xb0, 0x84, 0x23, 0x4a, 0x30, 0x95, 0xe2, 0x9d, 0xd5, 0x4d, 0xcc, 0x14, 0x8e, 0x52, 0xbd, 0xbb, 0x6a, 0xd1, 0x69, 0x59], // slot 4 waiver
            [0xab, 0x72, 0xef, 0x0c, 0xac, 0x04, 0x49, 0x0d, 0x30, 0x15, 0xb9, 0xc0, 0x80, 0x6c, 0x1a, 0x94, 0x04, 0x35, 0x5c, 0xaa, 0xef, 0x16, 0x8b, 0xfb, 0x1a, 0xd5, 0x7c, 0x41, 0xaf, 0xca, 0xf5, 0xcf], // slot 5 waiver
            [0x07, 0xfa, 0x5e, 0xc2, 0x2f, 0x93, 0xfe, 0xe3, 0x8b, 0x15, 0x1b, 0x06, 0x3d, 0x82, 0x2c, 0xae, 0x00, 0x3e, 0xce, 0xfb, 0x7e, 0x9e, 0x68, 0x60, 0xed, 0xf8, 0x75, 0x63, 0xfb, 0x71, 0x34, 0x56], // slot 6 waiver
        ],
        review: [0xba, 0xc0, 0x2e, 0x78, 0x08, 0x9a, 0xcc, 0x40, 0xf7, 0x76, 0x3b, 0x9c, 0x2b, 0x9d, 0xfe, 0x4d, 0x62, 0x29, 0x16, 0x6b, 0xbc, 0xf9, 0xcc, 0xe2, 0x3f, 0x70, 0x41, 0x3d, 0xd5, 0x41, 0xa1, 0x19],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::CourtCapacityV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x1c, 0xe1, 0x7e, 0x8c, 0x2b, 0xec, 0xa5, 0x78, 0xa1, 0xa7, 0x2d, 0x04, 0xc7, 0x67, 0xab, 0xe2, 0x35, 0x1e, 0xd8, 0xcb, 0x20, 0x4f, 0x55, 0xbf, 0x85, 0x3d, 0x8f, 0xb9, 0x51, 0x4c, 0xfc, 0xba], // slot 0 waiver
            [0x67, 0xba, 0xe7, 0x8b, 0xe2, 0x24, 0x9f, 0x92, 0xc6, 0xdf, 0x19, 0xe1, 0x23, 0x52, 0x30, 0x30, 0xd3, 0x53, 0x06, 0xd3, 0x74, 0x35, 0xfb, 0x54, 0x32, 0xab, 0x75, 0xd3, 0xb3, 0x23, 0x9e, 0x0d], // slot 1 waiver
            [0xac, 0xf6, 0xc9, 0x08, 0xaa, 0x33, 0xae, 0xc8, 0x8c, 0xa9, 0xfd, 0xd1, 0x6f, 0x71, 0xb3, 0xfa, 0xe8, 0x3f, 0x3c, 0xb9, 0x18, 0x47, 0x9f, 0x74, 0x5a, 0x0d, 0x52, 0x2a, 0x23, 0xce, 0x00, 0xd3], // slot 2 waiver
            [0x98, 0xfa, 0x79, 0x04, 0x75, 0x69, 0xfb, 0xc4, 0x3b, 0x78, 0xd6, 0x68, 0x3b, 0x27, 0x3a, 0xa4, 0xfa, 0x6b, 0xec, 0x00, 0x78, 0xa6, 0x9b, 0xf2, 0x7e, 0x19, 0x29, 0xa0, 0xe8, 0x99, 0xb7, 0x69], // slot 3 waiver
            [0xd1, 0x7f, 0x05, 0x3d, 0x12, 0xe7, 0xb4, 0xff, 0x76, 0x8b, 0x2e, 0x13, 0xaa, 0x5d, 0x20, 0x98, 0x81, 0xfd, 0x1f, 0x60, 0x03, 0x26, 0x36, 0x4b, 0xb5, 0xa7, 0xc1, 0xd1, 0xa9, 0x94, 0xc5, 0xe2], // slot 4 waiver
            [0x4e, 0xa7, 0x13, 0xc1, 0x3c, 0x6c, 0x9b, 0x49, 0xfb, 0x2c, 0x86, 0xd6, 0xd2, 0x2d, 0xae, 0x38, 0x08, 0x6f, 0x6c, 0x12, 0xdc, 0x0f, 0xf6, 0xfc, 0x08, 0x99, 0x46, 0xf3, 0x47, 0xee, 0x1b, 0xad], // slot 5 waiver
            [0x8e, 0xdd, 0xd5, 0x79, 0xa2, 0x31, 0x3e, 0x4b, 0x16, 0x5b, 0x0f, 0x3d, 0x6f, 0xab, 0xa1, 0x16, 0x95, 0xc3, 0x6b, 0xa7, 0xc3, 0xaa, 0x94, 0xc5, 0x58, 0xab, 0x29, 0xbc, 0xd4, 0xc8, 0xbd, 0x9e], // slot 6 waiver
        ],
        review: [0x36, 0x3b, 0x3c, 0x4e, 0xa2, 0xcc, 0xf0, 0x28, 0x94, 0x99, 0x0f, 0x2c, 0xc8, 0x10, 0x60, 0x3d, 0x44, 0x6e, 0x9b, 0xab, 0x61, 0xd9, 0xb1, 0x31, 0xe1, 0xe8, 0x88, 0xd2, 0xfa, 0xad, 0xad, 0x64],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::ClaimHardDeadlineV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x96, 0x4b, 0xd0, 0xe1, 0xb2, 0xb9, 0x0e, 0x45, 0x8f, 0x03, 0xbb, 0xe0, 0xe3, 0x62, 0xf2, 0x5a, 0xeb, 0x74, 0xd6, 0x1a, 0x20, 0x30, 0xc1, 0x96, 0xb2, 0x76, 0xfd, 0xcf, 0xb5, 0x0c, 0xfa, 0x36], // slot 0 waiver
            [0x22, 0xcf, 0x78, 0xa7, 0x19, 0x8c, 0x27, 0xde, 0x2e, 0xce, 0x12, 0x40, 0xe4, 0x63, 0x07, 0x3b, 0x87, 0x5d, 0x40, 0x6f, 0xf0, 0x56, 0x1f, 0xc9, 0x94, 0x61, 0xbe, 0xbf, 0x8b, 0x4f, 0xf2, 0xc3], // slot 1 waiver
            [0x98, 0xb8, 0xb7, 0xeb, 0x19, 0x2b, 0x00, 0xb2, 0x7c, 0xa4, 0x9a, 0xeb, 0x01, 0xff, 0xc9, 0x69, 0xdd, 0x76, 0x7a, 0x74, 0x04, 0x5a, 0x14, 0x41, 0x0f, 0x00, 0x35, 0xec, 0x38, 0x1a, 0x80, 0x14], // slot 2 waiver
            [0xa1, 0x99, 0xa1, 0x05, 0x08, 0x0f, 0x7b, 0x89, 0x65, 0x15, 0x5c, 0xf9, 0xb8, 0x96, 0x21, 0x4c, 0x6f, 0x4b, 0xd2, 0x56, 0x23, 0x77, 0x92, 0x97, 0xc9, 0x46, 0x7d, 0xfb, 0x90, 0x26, 0xcb, 0xef], // slot 3 waiver
        ],
        review: [0xd5, 0x0f, 0x42, 0xa2, 0x4d, 0x25, 0x11, 0xcf, 0x82, 0xb3, 0xbf, 0xd6, 0x0f, 0xa0, 0x87, 0x71, 0xd0, 0x25, 0x54, 0x45, 0xf8, 0x29, 0xe7, 0x21, 0xe2, 0xe9, 0x24, 0x72, 0xab, 0x82, 0xc0, 0x4d],
    },
    crate::palw_g14_gates_v1::PalwG14ArmingRecordV1 {
        version: crate::palw_g14_gates_v1::PALW_G14_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_g14_gates_v1::PalwG14GatedFenceV1::G14RouteRequiredV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x81, 0x83, 0x87, 0xde, 0x1c, 0x1a, 0xf3, 0xe9, 0x47, 0x23, 0xe3, 0x9e, 0x90, 0xb4, 0xca, 0xbe, 0x02, 0x28, 0xc0, 0x26, 0xb7, 0x4c, 0x89, 0x7b, 0xea, 0x10, 0x15, 0xa3, 0xfe, 0xf0, 0x91, 0xce], // slot 0 waiver
            [0x3b, 0x49, 0xff, 0xb7, 0x47, 0x1c, 0xbd, 0xf9, 0x48, 0x38, 0xfd, 0x0f, 0x95, 0x26, 0xe0, 0x3c, 0x63, 0x43, 0x95, 0x62, 0xc6, 0x10, 0x07, 0x2e, 0xef, 0x22, 0x11, 0xce, 0xd0, 0x3b, 0xe7, 0x7f], // slot 1 waiver
            [0x3c, 0xb0, 0xb1, 0xac, 0xce, 0xbf, 0x15, 0x2e, 0xf9, 0xe1, 0x09, 0x1c, 0x5c, 0x0a, 0x98, 0x80, 0x9b, 0x94, 0x78, 0x52, 0x94, 0x70, 0x1a, 0xd6, 0x14, 0xb0, 0x86, 0x13, 0xc0, 0xf3, 0xd1, 0xb9], // slot 2 waiver
            [0xfe, 0x62, 0x06, 0x01, 0x6f, 0xc8, 0x13, 0x8d, 0xba, 0x4c, 0xfd, 0x30, 0x4c, 0xe1, 0x81, 0xd7, 0x7e, 0xf3, 0x54, 0x61, 0xa9, 0xe0, 0x87, 0xa6, 0xdb, 0x4f, 0xec, 0xba, 0x79, 0x5e, 0x3a, 0xcb], // slot 3 waiver
        ],
        review: [0x08, 0xae, 0xfc, 0x22, 0xd6, 0xfe, 0xa5, 0x53, 0x28, 0xcb, 0x71, 0xfd, 0x65, 0x56, 0x7e, 0x06, 0x76, 0x6d, 0xbf, 0x66, 0x75, 0x1c, 0x80, 0xa3, 0xee, 0x0d, 0x80, 0xf7, 0xc0, 0xa5, 0x4d, 0xc4],
    },
];

/// The kernel-route records.
pub const PALW_T12_17000_KERNEL_ROUTE_RECORDS_V1: &[crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1] = &[
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xa4, 0xcd, 0x00, 0x5c, 0xec, 0x3b, 0x98, 0x35, 0x1b, 0x9c, 0x01, 0x28, 0xe9, 0xdd, 0xdc, 0x46, 0xb6, 0x18, 0x1c, 0x78, 0x42, 0x67, 0x4a, 0xb3, 0xfa, 0x73, 0x09, 0x1b, 0x42, 0xdb, 0x25, 0x52], // slot 0 waiver
            [0x6c, 0xdb, 0xbd, 0x6a, 0x40, 0x0b, 0x47, 0xcd, 0x74, 0x16, 0x00, 0xe0, 0xbe, 0x43, 0x14, 0xb7, 0x15, 0xa3, 0xfe, 0xdf, 0x07, 0x43, 0xa4, 0xee, 0x8e, 0x63, 0x77, 0xa1, 0xbe, 0xd8, 0x84, 0x4b], // slot 1 waiver
            [0x8b, 0x21, 0x55, 0xb8, 0x90, 0xd3, 0x35, 0x90, 0xb4, 0x2a, 0xc9, 0x34, 0xa8, 0x37, 0x7e, 0x15, 0x14, 0xd5, 0xad, 0xb6, 0x30, 0x5b, 0xba, 0xc0, 0x7d, 0x7e, 0x1f, 0x1f, 0xd2, 0x30, 0x10, 0xee], // slot 2 waiver
            [0x29, 0x81, 0xa4, 0x9d, 0xf7, 0x03, 0x65, 0x6f, 0x98, 0x3b, 0x15, 0xb2, 0x17, 0x62, 0x0f, 0xd8, 0xbf, 0x14, 0x16, 0xe4, 0xf3, 0xec, 0x2b, 0x54, 0x49, 0xcc, 0x3e, 0x37, 0x8f, 0x79, 0x71, 0xe5], // slot 3 waiver
            [0x43, 0x9a, 0x1d, 0xc0, 0xe2, 0x48, 0x5b, 0x45, 0xf6, 0x85, 0x00, 0xf5, 0x6a, 0x76, 0x96, 0x23, 0xe8, 0xe6, 0x1b, 0xe6, 0xce, 0x0f, 0x8c, 0x62, 0x7b, 0x64, 0x7d, 0x7b, 0x3f, 0xe1, 0x63, 0xb5], // slot 4 waiver
            [0x51, 0x7c, 0x1e, 0x61, 0xc8, 0x41, 0xe9, 0x7b, 0xd2, 0x51, 0xc2, 0xac, 0x1d, 0x3d, 0x8a, 0xba, 0x0a, 0xd0, 0xa9, 0x05, 0x14, 0xa8, 0x45, 0x10, 0x29, 0x95, 0xa9, 0x85, 0xca, 0x22, 0x8d, 0xca], // slot 5 waiver
            [0x3d, 0xde, 0xd4, 0x1e, 0x29, 0xb3, 0x61, 0x54, 0x8f, 0xcc, 0x41, 0x5d, 0x8c, 0xb5, 0x5d, 0x36, 0x1a, 0x75, 0x98, 0xec, 0x64, 0xd7, 0xac, 0xa2, 0x0a, 0x1b, 0x66, 0x41, 0x4f, 0xdb, 0xe7, 0x9f], // slot 6 waiver
            [0xf1, 0x6f, 0x4c, 0x9b, 0x96, 0x69, 0x7b, 0xbe, 0x21, 0x18, 0x6b, 0x9a, 0xb0, 0xe6, 0xdf, 0x44, 0xca, 0x9c, 0xac, 0xb4, 0x13, 0x37, 0x91, 0xff, 0xdf, 0x0d, 0x5f, 0x9b, 0xdb, 0xe5, 0x72, 0x35], // slot 7 waiver
        ],
        review: [0x2d, 0x3b, 0x9b, 0xbc, 0xcb, 0x75, 0x15, 0xaa, 0x45, 0x9c, 0x1e, 0x42, 0x38, 0xc8, 0xce, 0x98, 0xcd, 0x36, 0x35, 0xb3, 0x30, 0x93, 0x26, 0x66, 0xba, 0x7a, 0x47, 0x4a, 0xc9, 0x6f, 0x08, 0x80],
    },
    crate::palw_kernel_route_arming_v1::PalwKernelRouteArmingRecordV1 {
        version: crate::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1::ProviderCourtV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0x78, 0xcf, 0x82, 0x99, 0x31, 0x46, 0x31, 0x6b, 0xed, 0xa1, 0x6e, 0x83, 0x8e, 0xd1, 0xab, 0xd1, 0x23, 0x99, 0x34, 0x20, 0x76, 0x22, 0xdb, 0x10, 0x64, 0x46, 0x16, 0x3d, 0x7d, 0x44, 0x3d, 0x9d], // slot 0 waiver
            [0xb4, 0x04, 0x8d, 0x44, 0xd1, 0x54, 0x68, 0x3a, 0x38, 0xea, 0x5e, 0x93, 0xff, 0x27, 0x75, 0xa9, 0x2c, 0xae, 0xcc, 0x7f, 0x57, 0x1a, 0x48, 0x5f, 0xb8, 0xd8, 0x82, 0xb8, 0xc1, 0x55, 0x69, 0x58], // slot 1 waiver
            [0xf0, 0xbc, 0xc2, 0xbf, 0xa8, 0x78, 0x6b, 0x74, 0xe4, 0x3b, 0x88, 0x8e, 0xb6, 0x52, 0xd4, 0x26, 0x58, 0x85, 0x35, 0x63, 0xd1, 0xd3, 0xaa, 0xf6, 0xe3, 0x84, 0xfa, 0xce, 0x95, 0xfb, 0x93, 0x8f], // slot 2 waiver
            [0x52, 0x3a, 0x23, 0x85, 0xa9, 0x6b, 0xcb, 0x1d, 0xe4, 0x0f, 0xcb, 0x76, 0x30, 0xce, 0xb0, 0x94, 0xcd, 0xab, 0x4a, 0xf2, 0xde, 0xa2, 0x80, 0x51, 0x23, 0xd5, 0x54, 0xe8, 0xe9, 0x83, 0xa0, 0x9e], // slot 3 waiver
        ],
        review: [0x2b, 0xfd, 0x89, 0xd3, 0xc3, 0x24, 0x31, 0xb1, 0xbf, 0x0e, 0x16, 0x5c, 0x1a, 0xdf, 0x7d, 0x18, 0xf9, 0xf2, 0x7b, 0xe4, 0xff, 0x59, 0x23, 0xc4, 0x6a, 0xc0, 0x65, 0xfd, 0x0f, 0x50, 0x3b, 0x5d],
    },
];

/// The rfc0018 records.
pub const PALW_T12_17000_RFC0018_RECORDS_V1: &[crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1] = &[
    crate::palw_rfc0018_v1::PalwRfc0018ArmingRecordV1 {
        version: crate::palw_rfc0018_v1::PALW_RFC0018_ARMING_RECORD_VERSION_V1,
        fence: crate::palw_rfc0018_v1::PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xff, 0xf1, 0xdd, 0x08, 0x37, 0xf2, 0x06, 0x09, 0xf2, 0x79, 0x4a, 0x4a, 0x2b, 0x39, 0xd2, 0x06, 0xbf, 0x5e, 0x14, 0xa6, 0xda, 0xb1, 0xaa, 0x9e, 0xc0, 0x81, 0xd0, 0x34, 0x27, 0x3a, 0x61, 0xb8], // slot 0 waiver
            [0x93, 0xda, 0x87, 0x84, 0x61, 0x86, 0xf7, 0x68, 0x6e, 0x96, 0x76, 0xea, 0xe6, 0xff, 0x23, 0xd7, 0x5a, 0xe8, 0x5b, 0x29, 0xad, 0x17, 0xec, 0x4c, 0x51, 0x2e, 0xdb, 0x0f, 0x0b, 0x73, 0x81, 0x85], // slot 1 waiver
            [0x6a, 0xe7, 0xa1, 0xf9, 0x8f, 0x62, 0x9a, 0x38, 0x64, 0x49, 0xcb, 0x0d, 0x31, 0xfb, 0x8f, 0xa3, 0x9a, 0x5a, 0xc9, 0x1f, 0x7e, 0x63, 0x89, 0xef, 0x7e, 0xd8, 0x79, 0x41, 0x74, 0x7f, 0x2d, 0x15], // slot 2 waiver
            [0x42, 0xd9, 0xa9, 0x66, 0xe0, 0xee, 0xa4, 0x95, 0x48, 0x85, 0x84, 0x25, 0x47, 0x13, 0xb3, 0xf8, 0xe8, 0x0c, 0xcc, 0x21, 0xc7, 0xe4, 0x2e, 0x1f, 0xdf, 0x09, 0xbc, 0xff, 0xe5, 0xf7, 0x55, 0x00], // slot 3 waiver
            [0xdd, 0x81, 0x3e, 0xd1, 0xe7, 0x3a, 0xed, 0x00, 0x63, 0x4a, 0xe1, 0x22, 0x85, 0xf8, 0x8b, 0xf3, 0xe2, 0xfd, 0x3d, 0x82, 0x85, 0x41, 0x8d, 0x2b, 0x53, 0x70, 0x88, 0x34, 0x33, 0x86, 0xb9, 0xff], // slot 4 waiver
            [0x46, 0x25, 0xe5, 0x9d, 0x36, 0x0e, 0x92, 0xd3, 0xd8, 0x02, 0x6e, 0xe8, 0xe9, 0x81, 0xfe, 0x4f, 0xf4, 0x45, 0xf7, 0xc5, 0x9d, 0xc1, 0xfe, 0x51, 0xd9, 0x4f, 0x1c, 0x6c, 0x29, 0x7e, 0x7d, 0x64], // slot 5 waiver
        ],
        review: [0x77, 0xad, 0xa1, 0x0e, 0x42, 0x9b, 0x1e, 0x7f, 0x55, 0x64, 0xe2, 0xbd, 0x5f, 0x90, 0xc0, 0xf7, 0xa3, 0xd2, 0x27, 0x5e, 0xd8, 0x67, 0x8e, 0x5f, 0x25, 0x30, 0x51, 0x9f, 0x9b, 0x0e, 0xa1, 0xda],
    },
];

/// The frame cost record for FpSelection: the lane VAL estimate, measurement and review waived.
pub const PALW_T12_17000_FRAME_COST_RECORDS_V1: &[crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1] = &[
    crate::palw_tir_frame_cost_v1::PalwTirFrameCostRecordV1 {
        version: crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        measurement: [0x9e, 0x66, 0x55, 0x12, 0x42, 0xd1, 0x7a, 0x81, 0x56, 0x90, 0x56, 0x70, 0x3e, 0x67, 0x2f, 0xaa, 0x92, 0xfb, 0xf5, 0x70, 0xbe, 0x30, 0x53, 0x80, 0x1c, 0x92, 0xa5, 0xdc, 0x7d, 0x98, 0x2e, 0xb2],
        review: [0x7d, 0x51, 0x5f, 0xdd, 0x28, 0x88, 0x29, 0x98, 0xbc, 0x2f, 0x7d, 0x76, 0xa6, 0xf3, 0xdc, 0xf3, 0x36, 0x00, 0x80, 0xb7, 0xdf, 0x7a, 0x64, 0xf7, 0x90, 0xf3, 0x73, 0x7a, 0x7a, 0xfe, 0xd5, 0xcc],
        cost: crate::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1,
    },
];

/// The model-tier record: c 300, κ 1000 (user decision 2026-10-27), catalog edition 0 (empty) as the tip.
pub const PALW_T12_17000_MODEL_TIER_RECORDS_V1: &[crate::palw_model_tier_v1::PalwModelTierArmingRecordV1] = &[
    crate::palw_model_tier_v1::PalwModelTierArmingRecordV1 {
        version: crate::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORD_VERSION_V1,
        genesis: crate::config::genesis::PALW_T12_GENESIS.hash,
        activation_daa: 16600,
        registered_permille: 300,
        registered_cap_permille: 1000,
        catalog_tip_edition: 0,
        catalog_tip_root: crate::Hash64::from_bytes([0xc9, 0x4d, 0xb3, 0x54, 0x2b, 0x1e, 0x0b, 0xe5, 0x8a, 0xd1, 0xda, 0x59, 0xb3, 0x9f, 0xab, 0xf3, 0x7b, 0x65, 0x0f, 0x6c, 0x3e, 0x22, 0x34, 0x8e, 0x06, 0xcb, 0x2b, 0xd6, 0xb1, 0x21, 0x61, 0x43, 0xbe, 0x35, 0x19, 0xa8, 0x0b, 0xbd, 0x68, 0x57, 0x8d, 0x62, 0xda, 0xf2, 0xd2, 0x03, 0xb7, 0xa3, 0xdb, 0x96, 0xe6, 0xd1, 0x0c, 0x95, 0xa3, 0x24, 0xce, 0xe8, 0xef, 0x9d, 0x47, 0xf7, 0x2b, 0x6a]),
        source_commit: [0xd1, 0x81, 0xe8, 0x15, 0xea, 0x6b, 0x66, 0x2b, 0x84, 0xae, 0x40, 0x9f, 0x69, 0x6e, 0x92, 0xeb, 0x9a, 0x5e, 0x05, 0x7d],
        evidence: &[
            [0xba, 0x62, 0xa4, 0xac, 0x81, 0xb0, 0xc0, 0x8f, 0xd0, 0xc4, 0xaf, 0xfd, 0xe8, 0xbd, 0x6f, 0xff, 0xdd, 0x1e, 0x3d, 0x0d, 0x0c, 0x2d, 0xf9, 0x51, 0xb8, 0xc6, 0xea, 0x63, 0xf7, 0x2d, 0x14, 0xfa], // slot 0
            [0x30, 0x4e, 0x73, 0x74, 0x51, 0x81, 0x55, 0xad, 0x48, 0x88, 0xf0, 0x79, 0x76, 0x5d, 0xb1, 0xa9, 0xb7, 0x0a, 0xec, 0x4b, 0xb8, 0x7f, 0xf9, 0x5b, 0x26, 0xbe, 0xb3, 0x3a, 0xd6, 0xef, 0xbc, 0xaa], // slot 1
            [0x92, 0xa7, 0x68, 0xb1, 0xe4, 0x70, 0x31, 0x5f, 0x53, 0x61, 0xd1, 0x7d, 0xd2, 0x11, 0xf9, 0xa6, 0xb9, 0x4a, 0x89, 0x43, 0xe3, 0xa3, 0x9e, 0xc9, 0xb1, 0x0a, 0x04, 0x0e, 0x22, 0xa3, 0x37, 0x3b], // slot 2
            [0x60, 0x04, 0x2c, 0x55, 0xf7, 0x6c, 0x59, 0x74, 0x78, 0xf1, 0x08, 0xbd, 0xa0, 0x06, 0x02, 0x4e, 0xfe, 0x3c, 0xa1, 0xa5, 0x1f, 0x3b, 0x12, 0x0c, 0x4d, 0x1b, 0x5f, 0x34, 0x95, 0x06, 0x2b, 0x8e], // slot 3
            [0x01, 0xe6, 0x07, 0x22, 0x1c, 0x4e, 0x98, 0x18, 0xb8, 0x7a, 0x5f, 0x1b, 0x26, 0xf1, 0xf2, 0x7e, 0x80, 0x21, 0xcb, 0x7e, 0x3a, 0x4f, 0xdb, 0xeb, 0xe0, 0x52, 0x94, 0x7f, 0x1b, 0x25, 0xeb, 0x32], // slot 4
            [0x93, 0x78, 0x38, 0xed, 0x19, 0xaa, 0x09, 0xaa, 0x2f, 0x9c, 0x9c, 0xf5, 0x97, 0xaa, 0x64, 0x57, 0x64, 0x39, 0x3b, 0x3c, 0x04, 0x34, 0x42, 0x79, 0x48, 0x91, 0x2c, 0x5d, 0xf6, 0x5e, 0xee, 0x71], // slot 5
            [0xdd, 0xd6, 0x2e, 0x5e, 0x14, 0x97, 0xca, 0x1d, 0xb1, 0xc3, 0x20, 0xe4, 0x82, 0xc5, 0x6f, 0x40, 0xd0, 0x07, 0x0b, 0x3a, 0xa3, 0x9e, 0x7d, 0xff, 0x9a, 0xfe, 0x1f, 0x45, 0x5a, 0x22, 0x43, 0x27], // slot 6
            [0x6b, 0x0e, 0x4b, 0x3e, 0x5f, 0x7d, 0x1c, 0x72, 0x4e, 0xc6, 0x5b, 0xd8, 0xc1, 0x65, 0xb5, 0x4e, 0xab, 0x52, 0x6a, 0xc7, 0x4a, 0x94, 0x2e, 0x15, 0x2b, 0xb4, 0x77, 0x97, 0x2c, 0xa0, 0xf6, 0x37], // slot 7
        ],
        review: [0x2b, 0xd2, 0x0e, 0x6e, 0xdf, 0xdb, 0x8c, 0x2a, 0x02, 0xbe, 0xdf, 0xbe, 0x88, 0x4e, 0x52, 0x55, 0x76, 0x10, 0xcf, 0x61, 0xde, 0xeb, 0xf5, 0x15, 0xdd, 0xd2, 0x09, 0xe6, 0x55, 0xfe, 0x4a, 0xe9],
    },
];
