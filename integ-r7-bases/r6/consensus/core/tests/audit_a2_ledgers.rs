//! AUDIT LANE A2 (part 3) — the two independent ceilings on one bond, and the dead credit lane.

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_panel_economy_v1::palw_seat_has_headroom_v1;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// `palw_seat_has_headroom_v1` bounds `reserved_exposure + registration_exposure` at
/// `collateral x max_exposure_ratio_permille / 1000` (500 permille on t12).
/// `slashable_available` (palw_state_v2.rs:9382) bounds `sum(live locks)` at the WHOLE posted
/// collateral and subtracts nothing for the exposure ledger. Two ceilings, one bond.
#[test]
fn a2_two_ceilings_on_one_bond_sum_above_the_posted_collateral() {
    let params = t12();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("t12 is ConsensusV2")
    };
    let ratio = bundle.state.fp_max_exposure_ratio_permille();
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let exposure_ceiling = (collateral as u128) * ratio as u128 / 1000;
    println!("\n  fp_max_exposure_ratio_permille {ratio}");
    println!("  posted collateral            {collateral} = {:.2} MSK", collateral as f64 / 1e8);
    println!("  ledger A ceiling (exposure)  {exposure_ceiling} = {:.2} MSK", exposure_ceiling as f64 / 1e8);
    println!("  ledger B ceiling (locks)     {collateral} = {:.2} MSK   <- no ratio, and blind to A", collateral as f64 / 1e8);
    println!(
        "  a bond can owe up to         {} = {:.2} MSK = {:.1}% of what it posted",
        exposure_ceiling + collateral as u128,
        (exposure_ceiling + collateral as u128) as f64 / 1e8,
        (exposure_ceiling + collateral as u128) as f64 * 100.0 / collateral as f64
    );
    // The headroom predicate does not see a single locked sompi:
    assert!(
        palw_seat_has_headroom_v1(collateral, 0, exposure_ceiling, ratio),
        "with the WHOLE collateral already locked for Valid signatures, the draw still sees full headroom"
    );

    // The Qwen3.6 @512 row, at the numbers measured in audit_a2_mirrors.
    let seat_exposure_per_claim: u128 = 25_606_772_006;
    let lock_per_claim: u128 = 778_003_107_493;
    let n_locks = collateral as u128 / lock_per_claim;
    let n_exposure = exposure_ceiling / seat_exposure_per_claim;
    let n = n_locks.min(n_exposure);
    let owed = n * (seat_exposure_per_claim + lock_per_claim);
    println!("\n  Qwen3.6 @512: seat_exposure {seat_exposure_per_claim}/claim, Valid lock {lock_per_claim}/claim");
    println!("  ledger A admits {n_exposure} claims, ledger B admits {n_locks}; the seat takes {n}");
    println!(
        "  at {n} claims the bond owes {owed} = {:.2} MSK = {:.2}% of posted; overcommit {} = {:.2} MSK",
        owed as f64 / 1e8,
        owed as f64 * 100.0 / collateral as f64,
        owed.saturating_sub(collateral as u128),
        owed.saturating_sub(collateral as u128) as f64 / 1e8
    );
}

/// The ADR-0033 credit lane the lane brief asked about: dead on t12.
#[test]
fn a2_the_credit_lane_is_not_wired_on_t12() {
    let params = t12();
    println!("  params.palw_credit = {:?}", params.palw_credit.is_some());
    assert!(params.palw_credit.is_none(), "t12 carries no PalwCreditParamsV1, so one_job_ceiling/base/attester_share are unreachable");
}
