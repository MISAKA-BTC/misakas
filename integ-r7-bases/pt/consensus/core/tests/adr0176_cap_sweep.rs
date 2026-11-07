//! **ADR-0176 cap derivation — Stage 2 / Stage 3 test sources** (ユーザー方針 2026-10-15 items 8–9; ユーザー判断 2026-10-15 item 7).
//!
//! **TEST SOURCES ONLY — not run in the round that wrote them; nothing here is a PASS, and no value here is a proposal.** The fixture
//! profiles are shapes, not measurements: Stage 1 (`scripts/drills/adr0176_stage1_measure.py`) produces the measured profile, and
//! `stage2_sweep_from_a_measured_profile` (ignored) runs the sweep on it in the validation phase.
//!
//! | test | guidance item 9 |
//! |---|---|
//! | `a_cap_below_the_honest_rate_is_too_low_and_stalls_real` | network continuity: too-low caps are detected as REAL stalls |
//! | `a_cap_far_above_the_honest_rate_is_too_high` | caps an honest miner cannot use are detected |
//! | `a_reservation_too_big_is_collateral_bound` | collateral, not the cap, stopping the honest miner is detected |
//! | `a_reservation_below_the_collectable_loss_is_refused` | under-collectable reservations are detected |
//! | `the_fast_forger_never_gets_more_than_the_cap` | accounting cap test: forgery / colluding Valid / mass submission |
//! | `minimal_configuration_sustains_near_cap_with_bounded_gaps` | honest-miner reachability; bounded REAL gaps |
//! | `no_feasible_candidate_reports_the_conflicts` | Stage 2: report conflicting constraints instead of inventing numbers |

use kaspa_consensus_core::palw_bond_budget_v1::*;
use kaspa_consensus_core::palw_bond_cap_sweep_v1::*;

const KAS: u64 = 100_000_000;
const DAY: u64 = PALW_BOND_HOLD_DAA_PER_DAY_V1;

/// A SHAPE fixture: one accepted claim every 240 DAA (3 a day), Final 60 DAA later, 13,000 BILI of capital.
fn profile(mode: PalwHoldModeV1) -> PalwHonestProfileV1 {
    PalwHonestProfileV1 {
        mode,
        capital_sompi: 13_000 * KAS,
        claim_intervals_daa: vec![240],
        final_delays_daa: vec![60],
        finals_per_day: 0,
        reward_per_claim_sompi: KAS,
        final_weight_per_claim: 1,
    }
}

/// A candidate whose daily claim cap on 13,000 BILI is `per_day` (capital unit 13,000 BILI), R/F/B ample, reservation `res` BILI.
fn candidate(per_day: u64, res: u64) -> PalwCapCandidateV1 {
    PalwCapCandidateV1 {
        capital_unit_sompi: 13_000 * KAS,
        claims_per_unit_day: per_day,
        block_units_per_unit_day: per_day * PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_day_sompi: per_day * KAS,
        final_weight_per_unit_day: per_day,
        reservation_per_claim_sompi: res * KAS,
        collectable_loss_per_claim_sompi: KAS,
        concurrent_liability_permille: 500,
        burst_factor_permille: 4_000,
    }
}

#[test]
fn a_cap_below_the_honest_rate_is_too_low_and_stalls_real() {
    for mode in [PalwHoldModeV1::PanelOne, PalwHoldModeV1::PanelZero] {
        let r = palw_bond_cap_simulate_v1(&profile(mode), &candidate(2, 10), 14);
        assert_eq!(r.region, PalwCapRegionV1::TooLow, "{mode:?}: {r:?}");
        assert!(r.cap_stall_daa > 0 && r.honest_refused_for_cap > 0, "REAL stood still only for lack of cap");
    }
}

#[test]
fn a_cap_far_above_the_honest_rate_is_too_high() {
    let r = palw_bond_cap_simulate_v1(&profile(PalwHoldModeV1::PanelOne), &candidate(30, 10), 14);
    assert_eq!(r.region, PalwCapRegionV1::TooHigh, "{r:?}");
    assert!(r.forger_accepted > r.honest_accepted, "only a faster producer could use the rest");
}

#[test]
fn a_reservation_too_big_is_collateral_bound() {
    // 3 a day with Final 600 DAA after acceptance: two or three open at once; 4,000 BILI each > 6,500 BILI of φ·C.
    let mut slow = profile(PalwHoldModeV1::PanelOne);
    slow.final_delays_daa = vec![600];
    let r = palw_bond_cap_simulate_v1(&slow, &candidate(3, 4_000), 14);
    assert_eq!(r.region, PalwCapRegionV1::CollateralBound, "{r:?}");
}

#[test]
fn a_reservation_below_the_collectable_loss_is_refused() {
    let mut c = candidate(3, 10);
    c.collectable_loss_per_claim_sompi = 20 * KAS;
    let r = palw_bond_cap_simulate_v1(&profile(PalwHoldModeV1::PanelOne), &c, 14);
    assert_eq!(r.region, PalwCapRegionV1::UnderCollectable, "{r:?}");
}

#[test]
fn the_fast_forger_never_gets_more_than_the_cap() {
    for mode in [PalwHoldModeV1::PanelOne, PalwHoldModeV1::PanelZero] {
        for per_day in [1u64, 3, 10, 30] {
            let r = palw_bond_cap_simulate_v1(&profile(mode), &candidate(per_day, 1), 21);
            assert_eq!(r.forger_extra_over_cap, 0, "{mode:?} {per_day}/day: forging, colluding Valid and mass submission buy nothing");
        }
    }
}

#[test]
fn minimal_configuration_sustains_near_cap_with_bounded_gaps() {
    for mode in [PalwHoldModeV1::PanelOne, PalwHoldModeV1::PanelZero] {
        let r = palw_bond_cap_simulate_v1(&profile(mode), &candidate(3, 10), 28);
        assert_eq!(r.region, PalwCapRegionV1::Feasible, "{mode:?}: {r:?}");
        assert_eq!(r.honest_refused_for_cap, 0);
        assert!(r.utilization_permille >= PALW_CAP_SWEEP_UTILIZATION_FLOOR_PERMILLE_V1);
        assert!(r.max_real_gap_daa <= 240, "REAL gaps stay the honest pipeline's own: {}", r.max_real_gap_daa);
        assert!(r.peak_open_liability_sompi <= (6_500 * KAS) as u128, "λ·a·H within φ·C");
    }
    let _ = DAY;
}

#[test]
fn no_feasible_candidate_reports_the_conflicts() {
    // Every candidate either too low or collateral-bound: the sweep names both instead of a number.
    let grid = [candidate(1, 10), candidate(2, 10), candidate(3, 5_000), candidate(4, 5_000)];
    let mut slow = profile(PalwHoldModeV1::PanelOne);
    slow.final_delays_daa = vec![600];
    let (rows, conflicts) = palw_bond_cap_sweep_v1(&slow, &grid, 14);
    assert_eq!(rows.len(), 4);
    let conflicts = conflicts.expect("no feasible candidate");
    assert!(conflicts.contains(&PalwCapRegionV1::TooLow) && conflicts.contains(&PalwCapRegionV1::CollateralBound), "{conflicts:?}");
}

/// **Stage 2 on a MEASURED profile** (validation phase): `ADR0176_PROFILE=<intervals>;<final delays>;<capital BILI>;<mode 0|1>`
/// (DAA lists comma-separated) as Stage 1 printed it. Prints every candidate's report and the conflicts, if any.
#[test]
#[ignore]
fn stage2_sweep_from_a_measured_profile() {
    let raw = std::env::var("ADR0176_PROFILE").expect("ADR0176_PROFILE from the Stage 1 driver");
    let parts: Vec<&str> = raw.split(';').collect();
    let list = |s: &str| s.split(',').filter_map(|x| x.trim().parse::<u64>().ok()).collect::<Vec<_>>();
    let measured = PalwHonestProfileV1 {
        mode: if parts.get(3).map(|m| m.trim()) == Some("0") { PalwHoldModeV1::PanelZero } else { PalwHoldModeV1::PanelOne },
        capital_sompi: parts.get(2).and_then(|c| c.trim().parse::<u64>().ok()).unwrap_or(13_000) * KAS,
        claim_intervals_daa: list(parts[0]),
        final_delays_daa: list(parts.get(1).copied().unwrap_or("60")),
        finals_per_day: 0,
        reward_per_claim_sompi: KAS,
        final_weight_per_claim: 1,
    };
    let mut grid = Vec::new();
    for per_day in [1u64, 2, 3, 4, 6, 8, 12, 16, 24, 32] {
        for res in [10u64, 30, 100, 300, 1_000, 3_200] {
            grid.push(candidate(per_day, res));
        }
    }
    let (rows, conflicts) = palw_bond_cap_sweep_v1(&measured, &grid, 28);
    for (c, r) in &rows {
        println!("{c:?} => {r:?}");
    }
    println!("conflicts: {conflicts:?}");
}
