//! AUDIT LANE A2 (part 2) — the mirrors: what the reward clamps and the collateral does not,
//! and the exact cliff in the execution-quanta mint. Read-only.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_T12_DENSE_N_CTX, PALW_T12_HYBRID_N_CTX, Params};
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_job_economic_compute_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::palw_panel_share_permille_v1;
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_realizable_before_maturity_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
use kaspa_consensus_core::palw_panel_economy_v1::{palw_panel_seat_exposure_v1, palw_panel_split_permille_v1, palw_panel_split_v1};
use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
const SLASH: u64 = 5;
const SEATS: usize = 5;
const WINDOW_CHALLENGE: u64 = 1_200;
const WINDOW_COURT: u64 = 3_000;
const CADENCE_MS: u64 = 120_000;
const FLOOR_DECLARED: u128 = 7_708;
const FLOOR_DERIVED: u128 = 21_657_728;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn work_of(p: &PalwShapeProfileV3, c: (u32, u32)) -> (u128, u128) {
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(p, c.0, c.1);
    (
        palw_attempt_economic_compute_v1(p, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("draw"),
        palw_job_economic_compute_v1(p, &job, &PALW_ECONOMIC_COST_TABLE_V1).expect("job"),
    )
}

fn rows() -> Vec<(&'static str, u128, u128)> {
    let floor = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .expect("floor");
    let dense = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
        kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
            n_ctx: PALW_T12_DENSE_N_CTX,
            ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
        },
    )
    .expect("dense");
    let hybrid = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(
        kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
            kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
                n_ctx: PALW_T12_HYBRID_N_CTX,
                ..kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B
            },
        ),
    )
    .expect("hybrid");
    let (fd, fv) = work_of(&floor, kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL);
    let (hd, hv) = work_of(&hybrid, kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(PALW_T12_HYBRID_N_CTX));
    let (dd, dv) = work_of(&dense, kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(PALW_T12_DENSE_N_CTX));
    vec![("BASE-0 floor", fd, fv), ("Qwen3.6 @512", hd, hv), ("Qwen2.5 A16 @2M", dd, dv)]
}

/// **The reward has an escrow ceiling; `max_fraud_gain` has none.** Same claim, same block.
#[test]
fn a2_reward_is_capped_at_escrow_and_the_seat_lock_is_not() {
    let params = t12();
    let payout = params.palw_economic_payout.expect("armed");
    let carve = params.palw_overlay_carve.expect("armed");
    let escrow = ((T12_BLOCK_SUBSIDY_SOMPI as u128) * carve.worker_carve_permille as u128 / 1000) as u64;
    let w0 = palw_work_floor_v1(escrow, payout.rate_sompi_per_giga);
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI as u128;

    println!("\n=== escrow {escrow}   W0 {w0}   genesis bond collateral {collateral} = {:.2} MSK ===", collateral as f64 / 1e8);
    for (name, draw, _verification) in rows() {
        let target = palw_work_ticket_target_v1(draw, w0);
        let claim_pwu = palw_pwu_v1(target, draw.min(u64::MAX as u128) as u64);
        let exposure_v3 = (draw * FLOOR_DECLARED / FLOOR_DERIVED).min(u64::MAX as u128) as u64;
        let reserved = exposure_v3 as u128 * SLASH as u128;
        let exposure_v2 = draw.min(u64::MAX as u128) as u64;

        // reward side: the ceiling
        let uncapped = draw.saturating_mul(payout.rate_sompi_per_giga as u128) / 1_000_000_000;
        let priced = palw_rate_priced_reward_v1(escrow, draw, payout.rate_sompi_per_giga as u128);

        // collateral side: no ceiling
        let quanta = palw_execution_quantum_count_v1(
            exposure_v2 as u128,
            PALW_EXECUTION_QUANTUM_V1 as u128,
            Hash64::default(),
            Hash64::default(),
        )
        .saturating_add(1);
        let extra = palw_realizable_before_maturity_v1(quanta, WINDOW_CHALLENGE, WINDOW_COURT, CADENCE_MS, PALW_T12_PERMIT_FEE_CEILING_SOMPI);
        let facts = PalwClaimFraudFactsV1 {
            reserved,
            escrowed_reward: escrow,
            exposure_pwu: claim_pwu,
            slash_value_per_pwu: SLASH,
            extra_economic_rights_sompi: extra,
        };
        let gain = palw_max_fraud_gain_v1(&facts);
        let required = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);

        println!("\n-- {name} --");
        println!("   one draw           {draw} MAC-eq   claim.pwu {claim_pwu} MAC-eq");
        println!("   uncapped reward    {uncapped} sompi = {:.2} MSK", uncapped as f64 / 1e8);
        println!("   priced (CLAMPED)   {priced} sompi = {:.5} MSK   clamp swallowed {:.0}x", priced as f64 / 1e8, uncapped as f64 / priced.max(1) as f64);
        println!("   quanta minted +1   {quanta}   realizable extra rights {extra} sompi");
        println!("   max_fraud_gain     {gain} sompi = {:.2} MSK   (weight term = pwu x {SLASH} = {})", gain as f64 / 1e8, claim_pwu as u128 * SLASH as u128);
        println!("   seat lock required {required} sompi = {:.2} MSK", required as f64 / 1e8);
        println!("   vs one genesis bond: {:.3}x the WHOLE posted collateral", required as f64 / collateral as f64);
        if required > collateral {
            println!("   => SeatValidLockRefused for EVERY genesis seat: this class cannot be licensed.");
        } else {
            println!("   => affordable: {} concurrent Valid locks per genesis bond", collateral / required.max(1));
        }
    }
}

/// **The lambda floor prices the seat's reward at a pool permille the payout stopped using.**
#[test]
fn a2_the_lambda_floor_band_where_it_under_reserves() {
    let params = t12();
    let payout = params.palw_economic_payout.expect("armed");
    let carve = params.palw_overlay_carve.expect("armed");
    let lambda = params.palw_panel_exposure_floor.expect("armed").reward_multiple_permille;
    let escrow = ((T12_BLOCK_SUBSIDY_SOMPI as u128) * carve.worker_carve_permille as u128 / 1000) as u64;
    let w0 = palw_work_floor_v1(escrow, payout.rate_sompi_per_giga);

    println!("\n=== a registrable class in the band: share > 200 permille AND the lambda floor still binds ===");
    println!("  W0 = {w0} CCU ; lambda = {lambda} permille ; fixed pool the floor reads = 200 permille");
    // verification/draw ratio measured on the shipped hybrid row (1.0190x); use it for a synthetic row.
    for draw in [1_000_000_000_000u128, 2_000_000_000_000, 4_000_000_000_000] {
        let verification = draw * 1019 / 1000;
        let target = palw_work_ticket_target_v1(draw, w0);
        let attempts_q32 = kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
        let attempted = kaspa_consensus_core::palw_economic_payout_v1::palw_attempted_ccu_v1(
            attempts_q32,
            kaspa_consensus_core::palw_economic_payout_v1::palw_network_draws_q32_from_bits_v1(0),
            draw,
        );
        let share = palw_panel_share_permille_v1(
            attempted,
            verification * SEATS as u128,
            payout.panel_share_alpha_permille,
            payout.panel_share_min_permille,
            payout.panel_share_max_permille,
        );
        let priced = palw_rate_priced_reward_v1(escrow, attempted, payout.rate_sompi_per_giga as u128);
        let paid = palw_panel_split_permille_v1(priced, share, SEATS, SEATS).per_seat;
        let exposure_v3 = (draw * FLOOR_DECLARED / FLOOR_DERIVED).min(u64::MAX as u128) as u64;
        let reserved = exposure_v3 as u128 * SLASH as u128;
        let seat_exposure = palw_panel_seat_exposure_v1(reserved, escrow, SEATS, lambda);
        let assumed = palw_panel_split_v1(escrow, SEATS, 0).per_seat;
        let intended = paid as u128 * lambda as u128 / 1000;
        println!(
            "\n  draw {draw} MAC-eq -> share {share} permille, priced {priced}, per-seat paid {paid}"
        );
        println!("     3 x reserved (stake)  {}", reserved * 3);
        println!("     lambda floor          {} (from the FIXED 200-permille per_seat {assumed})", assumed as u128 * lambda as u128 / 1000);
        println!("     seat_exposure written {seat_exposure}");
        println!(
            "     intended floor (lambda x what the seat is REALLY paid) {intended}  SHORTFALL {} sompi = {:.5} MSK per seat, {:.5} MSK per claim",
            intended.saturating_sub(seat_exposure),
            intended.saturating_sub(seat_exposure) as f64 / 1e8,
            intended.saturating_sub(seat_exposure) as f64 * SEATS as f64 / 1e8
        );
        println!("     effective lambda {:.4}x against a declared {:.1}x", seat_exposure as f64 / paid.max(1) as f64, lambda as f64 / 1000.0);
    }
}

/// **The mint's cliff is exactly the 2^16 probe horizon.**
#[test]
fn a2_the_mint_cliff_is_the_probe_horizon() {
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
    use kaspa_consensus_core::palw_execution_quanta_v1::palw_execution_mint_quanta_v1;
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use std::time::Instant;

    let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([7u8; 64]), 0));
    println!("\n=== the 2^16 = 65,536 probe horizon in assign_round ===");
    for n in [60_000u64, 65_000, 66_000, 70_000, 85_000, 100_000] {
        let f = PalwExecFinalV1 {
            domain: Hash64::from_u64_word(1),
            bond,
            operator_id: Hash64::from_u64_word(2),
            claim_id: Hash64::from_u64_word(3),
            execution_root: Hash64::from_u64_word(4),
            credit: n * PALW_EXECUTION_QUANTUM_V1,
        };
        let t = Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[f], Hash64::from_u64_word(9), PALW_EXECUTION_QUANTUM_V1 as u128, 1_000);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let excess = n.saturating_sub(65_536);
        let per_excess_ns = if excess > 0 { ms * 1e6 / excess as f64 } else { 0.0 };
        println!("  n={n:>7}  excess over 65,536 = {excess:>7}  {ms:>10.1} ms   ns per excess quantum {per_excess_ns:>12.0}");
        assert_eq!(issued.len(), n as usize);
    }
}

/// The two fences the findings above depend on, read from the shipped card at runtime.
#[test]
fn a2_the_gating_fences_are_armed_at_genesis_on_t12() {
    let p = t12();
    println!("  palw_objective_offence  {:?}", p.palw_objective_offence);
    println!("  palw_execution_quanta   {:?}", p.palw_execution_quanta);
    println!("  palw_economic_safety    {:?}", p.palw_economic_safety);
    println!("  palw_canonical_work     {:?}", p.palw_canonical_work);
    println!("  palw_work_target        {:?}", p.palw_work_target);
    println!("  palw_model_registry     {:?}", p.palw_model_registry);
    assert!(p.palw_objective_offence.is_some_and(|f| f.is_active(0)), "seat locks are live from DAA 0");
    assert!(p.palw_execution_quanta.is_some_and(|f| f.is_active(0)), "the quantum mint is live from DAA 0");
    assert!(p.palw_canonical_work.is_some_and(|f| f.is_active(0)), "the derived basis is live from DAA 0");
}
