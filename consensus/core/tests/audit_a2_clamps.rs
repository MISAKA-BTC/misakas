//! AUDIT LANE A2 — clamp / floor / ceiling asymmetry between the reward path and the
//! credit / collateral path, measured on the testnet-12 shipped card. Read-only: this file
//! computes and prints, it changes nothing.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, PALW_T12_DENSE_N_CTX, PALW_T12_HYBRID_N_CTX};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_expected_attempts_q32_v1, palw_job_economic_compute_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{
    palw_attempted_ccu_v1, palw_cap_utilization_permille_v1, palw_network_draws_q32_from_bits_v1, palw_panel_share_permille_v1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_execution_lane_v1::palw_execution_credit_v1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_offence_v1::{PALW_PANEL_COLLUDING_QUORUM_V1, palw_min_slashable_per_colluding_seat_v1};
use kaspa_consensus_core::palw_panel_economy_v1::{
    PALW_PANEL_POOL_PERMILLE_V1, palw_panel_seat_exposure_v1, palw_panel_split_permille_v1, palw_panel_split_v1,
};
use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

/// The block subsidy testnet-12 actually pays, from `CoinbaseManager::calc_block_subsidy` (recon,
/// re-derived here only as an input so this crate needs no kaspa-consensus dependency).
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
const SLASH_VALUE_PER_PWU: u64 = 5;
const SEATS: u16 = 5;
const WINDOW_CHALLENGE: u64 = 1_200;
const WINDOW_COURT: u64 = 3_000;
const CADENCE_MS: u64 = 120_000;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn floor_profile() -> (PalwShapeProfileV3, (u32, u32)) {
    (
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL,
    )
}

fn dense_profile() -> (PalwShapeProfileV3, (u32, u32)) {
    (
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
            kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
                n_ctx: PALW_T12_DENSE_N_CTX,
                ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
            },
        )
        .expect("dense profile"),
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(PALW_T12_DENSE_N_CTX),
    )
}

fn hybrid_profile() -> (PalwShapeProfileV3, (u32, u32)) {
    (
        kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(
            kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
                    n_ctx: PALW_T12_HYBRID_N_CTX,
                    ..kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B
                },
            ),
        )
        .expect("hybrid profile"),
        kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(PALW_T12_HYBRID_N_CTX),
    )
}

/// (draw_ccu, verification_ccu) exactly as `palw_model_work_from_carriage_v1` derives them.
fn work_of(p: &PalwShapeProfileV3, canonical: (u32, u32)) -> (u128, u128) {
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(p, canonical.0, canonical.1);
    let draw = palw_attempt_economic_compute_v1(p, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("draw");
    let verification = palw_job_economic_compute_v1(p, &job, &PALW_ECONOMIC_COST_TABLE_V1).expect("job");
    (draw, verification)
}

fn scalar_of(p: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    let d = PalwCanonicalClassDescriptorV1::of(p, Hash64::default()).expect("one weight format");
    let j = kaspa_consensus_core::palw_base0_profile::rc_job_context(p, canonical.0, canonical.1);
    palw_canonical_draw_work_v1(&d, &j, true).expect("derives").provisional_scalar_v1()
}

#[test]
fn a2_the_t12_clamp_table() {
    let params = t12();
    let payout = params.palw_economic_payout.expect("t12 arms the economic payout at 0");
    let carve = params.palw_overlay_carve.expect("t12 arms the overlay carve at 0");
    let lambda = params.palw_panel_exposure_floor.expect("t12 arms the panel exposure floor at 0");
    let lane = params.palw_execution_lane.expect("t12 arms the execution lane at 0");

    println!("\n=== t12 economic fences (runtime) ===");
    println!("  rate_sompi_per_giga          {}", payout.rate_sompi_per_giga);
    println!("  panel_share_alpha_permille   {}", payout.panel_share_alpha_permille);
    println!("  panel_share_min_permille     {}", payout.panel_share_min_permille);
    println!("  panel_share_max_permille     {}", payout.panel_share_max_permille);
    println!("  cap_utilization_max_permille {}", payout.cap_utilization_max_permille);
    println!("  worker_carve_permille        {}", carve.worker_carve_permille);
    println!("  lambda reward_multiple_permille {}", lambda.reward_multiple_permille);
    println!("  lane permits_per_round       {}  max_per_mergeset {}", lane.permits_per_round, lane.max_per_mergeset);
    println!("  PALW_PANEL_POOL_PERMILLE_V1  {PALW_PANEL_POOL_PERMILLE_V1}  (the FIXED pool the exposure floor uses)");
    println!("  PALW_EXECUTION_QUANTUM_V1    {PALW_EXECUTION_QUANTUM_V1}");

    let escrow = ((T12_BLOCK_SUBSIDY_SOMPI as u128) * carve.worker_carve_permille as u128 / 1000) as u64;
    println!("\n  block subsidy {T12_BLOCK_SUBSIDY_SOMPI}  ->  claim escrow {escrow} sompi = {:.5} MSK", escrow as f64 / 1e8);

    let w0 = palw_work_floor_v1(escrow, payout.rate_sompi_per_giga);
    println!("  W0 = escrow*1e9/rate = {w0} CCU");

    let floor_scalar = scalar_of(&floor_profile().0, floor_profile().1);
    let floor_declared: u128 = 7_708;
    println!("\n  floor basis: declared {floor_declared} leaves, derived {floor_scalar} MAC-eq/draw");

    for (name, (p, canon), declared) in [
        ("BASE-0 floor    ", floor_profile(), 7_708u128),
        ("Qwen3.6 @512    ", hybrid_profile(), 20_717_968u128),
        ("Qwen2.5 A16 @2M ", dense_profile(), 27_002_967_184u128),
    ] {
        let (draw, verification) = work_of(&p, canon);
        let scalar = scalar_of(&p, canon);
        assert_eq!(scalar, draw, "the canonical scalar IS the attempt economic compute");
        let target = palw_work_ticket_target_v1(draw, w0);
        let attempts = palw_expected_attempts_v1(target);
        let attempts_q32 = palw_expected_attempts_q32_v1(target);
        // claim.pwu as admission forces it past palw_canonical_work.
        let claim_pwu = palw_pwu_v1(target, draw.min(u64::MAX as u128) as u64);
        // what the runtime reserves on (exposure_pwu_v3) and what it credits on (exposure_pwu_v2).
        let exposure_v3 = (draw * floor_declared / floor_scalar).min(u64::MAX as u128) as u64;
        let exposure_v2 = draw.min(u64::MAX as u128) as u64;
        let reserved = (exposure_v3 as u128) * SLASH_VALUE_PER_PWU as u128;

        // ---- the reward path, with its escrow ceiling ----
        let attempted = palw_attempted_ccu_v1(attempts_q32, palw_network_draws_q32_from_bits_v1(0), draw);
        let priced = palw_rate_priced_reward_v1(escrow, attempted, payout.rate_sompi_per_giga as u128);
        let cap_util = palw_cap_utilization_permille_v1(attempted, payout.rate_sompi_per_giga, escrow);
        let share = palw_panel_share_permille_v1(
            attempted,
            verification * SEATS as u128,
            payout.panel_share_alpha_permille,
            payout.panel_share_min_permille,
            payout.panel_share_max_permille,
        );
        let paid = palw_panel_split_permille_v1(priced, share, SEATS as usize, SEATS as usize);

        // ---- the collateral path ----
        let seat_exposure = palw_panel_seat_exposure_v1(reserved, escrow, SEATS as usize, lambda.reward_multiple_permille);
        let assumed_per_seat = palw_panel_split_v1(escrow, SEATS as usize, 0).per_seat; // the 200-permille reading
        let facts = PalwClaimFraudFactsV1 {
            reserved,
            escrowed_reward: escrow,
            exposure_pwu: claim_pwu,
            slash_value_per_pwu: SLASH_VALUE_PER_PWU,
            extra_economic_rights_sompi: 0,
        };
        let gain0 = palw_max_fraud_gain_v1(&facts);
        let seat_lock = palw_min_slashable_per_colluding_seat_v1(gain0, PALW_PANEL_COLLUDING_QUORUM_V1);

        // ---- the execution-credit path ----
        let unit = exposure_v2; // the work-price unit is at least this class's own measure
        let clamped_credit = palw_execution_credit_v1(exposure_v2, unit);
        let quanta_unclamped =
            palw_execution_quantum_count_v1(exposure_v2 as u128, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default());
        let quanta_declared = palw_execution_quantum_count_v1(
            declared,
            PALW_EXECUTION_QUANTUM_V1 as u128,
            Hash64::default(),
            Hash64::default(),
        );

        println!("\n=== {name} ===");
        println!("  declared leaves/draw     {declared}");
        println!("  derived MAC-eq/draw      {draw}   verification_ccu {verification}");
        println!("  live target              {target}");
        println!("  expected attempts        {attempts}  (q32 {attempts_q32})");
        println!("  claim.pwu (U7, MAC-eq)   {claim_pwu}");
        println!("  exposure_v3 (reservation unit) {exposure_v3}   reserved {reserved} sompi = {:.5} MSK", reserved as f64 / 1e8);
        println!("  exposure_v2 (credit unit, RAW) {exposure_v2}");
        println!("  --- reward path ---");
        println!("  attempted_ccu            {attempted}");
        println!("  uncapped reward          {} sompi", attempted * payout.rate_sompi_per_giga as u128 / 1_000_000_000);
        println!("  priced_reward (min esc)  {priced} sompi = {:.5} MSK   cap_utilization {cap_util} permille", priced as f64 / 1e8);
        println!("  panel share              {share} permille   per_seat paid {} sompi = {:.5} MSK", paid.per_seat, paid.per_seat as f64 / 1e8);
        println!("  --- collateral path ---");
        println!("  3 x reserved (stake)     {}", reserved * 3);
        println!("  lambda floor input per_seat (200 permille FIXED) {assumed_per_seat}");
        println!("  seat_exposure reserved   {seat_exposure} sompi = {:.5} MSK", seat_exposure as f64 / 1e8);
        if paid.per_seat > 0 {
            println!(
                "  EFFECTIVE lambda = seat_exposure / per_seat_paid = {:.4}x  (declared {}x)",
                seat_exposure as f64 / paid.per_seat as f64,
                lambda.reward_multiple_permille as f64 / 1000.0
            );
        }
        println!("  max_fraud_gain(extra=0)  {gain0} sompi = {:.2} MSK", gain0 as f64 / 1e8);
        println!("  seat lock required       {seat_lock} sompi = {:.2} MSK", seat_lock as f64 / 1e8);
        println!("  --- execution credit path ---");
        println!("  credit WITH the clamp    {clamped_credit}");
        println!("  quanta from RAW credit   {quanta_unclamped}  (u32::MAX = {})", u32::MAX);
        println!("  quanta from declared     {quanta_declared}");
    }
}

/// The panel-pool permille the payout uses vs the one the exposure floor assumes.
#[test]
fn a2_the_lambda_floor_reads_a_pool_the_payout_no_longer_uses() {
    let params = t12();
    let payout = params.palw_economic_payout.expect("armed");
    let carve = params.palw_overlay_carve.expect("armed");
    let lambda = params.palw_panel_exposure_floor.expect("armed");
    let escrow = ((T12_BLOCK_SUBSIDY_SOMPI as u128) * carve.worker_carve_permille as u128 / 1000) as u64;

    println!("\n=== the two pool permille on one claim ===");
    let assumed = palw_panel_split_v1(escrow, SEATS as usize, 0).per_seat;
    let lam_floor = (assumed as u128) * lambda.reward_multiple_permille as u128 / 1000;
    println!("  escrow {escrow}");
    println!("  exposure floor reads PALW_PANEL_POOL_PERMILLE_V1 = {PALW_PANEL_POOL_PERMILLE_V1} -> per_seat {assumed}");
    println!("  lambda floor = {} permille x that = {lam_floor} sompi", lambda.reward_multiple_permille);
    for share in [payout.panel_share_min_permille, 200u16, payout.panel_share_max_permille] {
        let paid = palw_panel_split_permille_v1(escrow, share, SEATS as usize, SEATS as usize).per_seat;
        let intended = (paid as u128) * lambda.reward_multiple_permille as u128 / 1000;
        println!(
            "  share {share:>4} permille -> per_seat paid {paid:>15}  intended floor {intended:>17}  shortfall {:>17} sompi = {:>12.5} MSK  effective lambda {:.4}x",
            intended.saturating_sub(lam_floor),
            intended.saturating_sub(lam_floor) as f64 / 1e8,
            lam_floor as f64 / paid as f64
        );
    }
}

/// How expensive `palw_execution_mint_quanta_matured_v1` gets as `n` grows — the mint has no
/// mirror of the schedule's own byte bounds.
#[test]
fn a2_the_mint_cost_grows_superlinearly() {
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
    use kaspa_consensus_core::palw_execution_quanta_v1::palw_execution_mint_quanta_v1;
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use std::time::Instant;

    let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([7u8; 64]), 0));
    println!("\n=== mint cost vs quanta ===");
    let mut last: Option<(u64, f64)> = None;
    for credit in [100_000u64 * 1_000, 100_000 * 4_000, 100_000 * 16_000, 100_000 * 64_000, 100_000 * 128_000] {
        let f = PalwExecFinalV1 {
            domain: Hash64::from_u64_word(1),
            bond,
            operator_id: Hash64::from_u64_word(2),
            claim_id: Hash64::from_u64_word(3),
            execution_root: Hash64::from_u64_word(4),
            credit,
            accepted_blue_score: 0,
        };
        let n = credit / PALW_EXECUTION_QUANTUM_V1;
        let t = Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[f], Hash64::from_u64_word(9), PALW_EXECUTION_QUANTUM_V1 as u128, 1_000);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let bytes = borsh::to_vec(&issued).expect("borsh").len();
        println!("  n={n:>8}  minted {:>8}  {ms:>10.1} ms  rooted bytes {bytes:>12}", issued.len());
        if let Some((pn, pms)) = last {
            let rn = n as f64 / pn as f64;
            println!("            x{rn:.0} quanta -> x{:.2} time (linear would be x{rn:.0})", ms / pms);
        }
        last = Some((n, ms));
    }
}
