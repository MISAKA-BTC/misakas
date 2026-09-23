//! **AUDIT LANE A3 (part 2) — the work-target -> pwu -> fraud-gain chain, in integers.**
//!
//! Every function called here is the production one. Run:
//!   cargo test -p kaspa-consensus-core --test audit_a3_work_target_pwu -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1};
use kaspa_consensus_core::palw_economic_payout_v1::palw_network_draws_q32_from_bits_v1;
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_realizable_before_maturity_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_offence_v1::{PALW_PANEL_COLLUDING_QUORUM_V1, palw_colluding_quorum_covers_v1};
use kaspa_consensus_core::palw_panel_var_v1::palw_fork_weight_sompi_v1;
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_target_step_v1, palw_work_ticket_target_v1};

const T12_ESCROW_SOMPI: u64 = 320_084_650_080;
const T12_RATE: u64 = 900_000_000;
const T12_SLASH_PER_PWU: u64 = 5;
const T12_POSTED_COLLATERAL_SOMPI: u64 = 51_642_979_663_480; // PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI
const T12_WINDOW_CHALLENGE: u64 = 1_200;
const T12_WINDOW_COURT: u64 = 3_000;
const T12_BLOCK_MS: u64 = 120_000;

fn job_of(p: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    rc_job_context(p, prefill, decode)
}

fn t12_rows() -> Vec<(&'static str, u128)> {
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
    let hybrid = qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).unwrap();
    let dense = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).unwrap();
    let f = palw_attempt_economic_compute_v1(
        &floor,
        &job_of(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1),
        true,
        &PALW_ECONOMIC_COST_TABLE_V1,
    )
    .unwrap();
    let (hp, hd) = qwen36_held_canonical_v1(512);
    let h = palw_attempt_economic_compute_v1(&hybrid, &job_of(&hybrid, hp, hd), true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let d = palw_attempt_economic_compute_v1(&dense, &job_of(&dense, dp, dd), true, &PALW_ECONOMIC_COST_TABLE_V1).unwrap();
    vec![("BASE-0 floor", f), ("Qwen3.6 @512", h), ("Qwen2.5 A16 @2M", d)]
}

/// **The t12 genesis bits and what network factor they put on every claim's attempted_ccu.**
#[test]
fn a3_9_t12_genesis_bits_and_the_network_factor() {
    let p = palw_t12_shipped_params();
    let bits = p.genesis.bits;
    let q32 = palw_network_draws_q32_from_bits_v1(bits);
    let draws = q32 as f64 / (1u128 << 32) as f64;
    println!("\n=== A3-9: t12 genesis bits ===");
    println!("  genesis.bits = {bits:#010x} ({bits})");
    println!("  palw_network_draws_q32_from_bits_v1 = {q32}  => {draws:.6} network draws per class win");
    println!("\n  effect on priced_reward = min(escrow, ccu x net x rate/1e9), escrow = {T12_ESCROW_SOMPI}:");
    println!("{:<20} {:>22} {:>26} {:>18} {:>14}", "class", "ccu/draw", "ccu x net_draws", "priced sompi", "MSK");
    for (name, ccu) in t12_rows() {
        let attempted = (ccu.saturating_mul(q32)) >> 32;
        let paid = palw_rate_priced_reward_v1(T12_ESCROW_SOMPI, attempted, T12_RATE as u128);
        println!("{name:<20} {ccu:>22} {attempted:>26} {paid:>18} {:>14.4}", paid as f64 / 1e8);
    }
    // At what network factor does the CHEAPEST class collect the whole escrow?
    let w0 = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    let floor_ccu = t12_rows()[0].1;
    println!("\n  W0 = palw_work_floor_v1({T12_ESCROW_SOMPI}, {T12_RATE}) = {w0} MAC-eq");
    println!("  the 21.66M-MAC-eq floor class reaches the full escrow once network draws >= {}", w0.div_ceil(floor_ccu));
}

/// **The class targets the ADR-0137 work target actually produces, and the pwu they derive.**
#[test]
fn a3_10_work_target_to_pwu_and_the_integer_floor_loss() {
    let w0 = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    println!("\n=== A3-10: palw_work_ticket_target_v1 -> palw_attempt_derived_pwu_v1, W = W0 = {w0} ===");
    println!("{:<20} {:>22} {:>12} {:>10} {:>24} {:>10}", "class", "CCU", "CCU/W0", "attempts", "claim.pwu", "pwu/W0");
    for (name, ccu) in t12_rows() {
        let target = palw_work_ticket_target_v1(ccu, w0);
        let attempts = palw_expected_attempts_v1(target);
        let pwu = palw_attempt_derived_pwu_v1(target, ccu);
        println!("{name:<20} {ccu:>22} {:>12.6} {attempts:>10} {pwu:>24} {:>10.4}", ccu as f64 / w0 as f64, pwu as f64 / w0 as f64);
    }
    println!("\n  The design intent is pwu == W for every class below W (equal weight per escrow).");
    println!("  palw_expected_attempts_v1 FLOORS, so pwu/W = floor(W/CCU)*CCU/W = floor(x)/x.");
    println!("\n  worst-case sweep of floor(x)/x, x = W/CCU (the under-weight a class suffers):");
    println!("{:>16} {:>22} {:>10} {:>24} {:>10}", "x = W/CCU", "CCU", "attempts", "pwu", "pwu/W");
    let mut worst = (f64::MAX, 0u128);
    for num in [100u128, 101, 133, 150, 166, 199, 200, 201, 250, 299, 300, 333, 400, 500, 999, 1000] {
        let ccu = w0 * 100 / num; // x = num/100
        let target = palw_work_ticket_target_v1(ccu, w0);
        let attempts = palw_expected_attempts_v1(target);
        let pwu = palw_attempt_derived_pwu_v1(target, ccu);
        let ratio = pwu as f64 / w0 as f64;
        if ratio < worst.0 {
            worst = (ratio, ccu);
        }
        println!("{:>16.2} {ccu:>22} {attempts:>10} {pwu:>24} {ratio:>10.4}", num as f64 / 100.0);
    }
    println!("\n  WORST measured: pwu/W = {:.4} at CCU = {} -- that class carries {:.2}x LESS", worst.0, worst.1, 1.0 / worst.0);
    println!("  fork-choice weight than the floor class, for the same escrow and the same honest work.");
    assert!(worst.0 < 0.55, "the integer floor of expected_attempts costs a class up to half its weight");
}

/// **Is `palw_pwu_v1`'s u64::MAX saturation reachable on t12, and what happens there.**
#[test]
fn a3_11_pwu_saturation_via_the_work_target_ratchet() {
    let w0 = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    let dense_ccu = t12_rows()[2].1;
    let max_factor = 4u32; // t12 class_daa_max_factor
    println!("\n=== A3-11: W ratchet (palw_work_target_step_v1, max_factor = {max_factor}, epoch = 1000 DAA) ===");
    println!("{:>6} {:>26} {:>10} {:>24} {:>12} {:>26}", "epoch", "W", "attempts", "dense claim.pwu", "saturated?", "max_fraud_gain sompi");
    let mut w = w0;
    let mut first_sat: Option<u32> = None;
    for epoch in 0..=16u32 {
        let target = palw_work_ticket_target_v1(dense_ccu, w);
        let attempts = palw_expected_attempts_v1(target);
        let pwu = palw_attempt_derived_pwu_v1(target, dense_ccu);
        let sat = pwu == u64::MAX;
        if sat && first_sat.is_none() {
            first_sat = Some(epoch);
        }
        let gain = (T12_ESCROW_SOMPI as u128).saturating_add(palw_fork_weight_sompi_v1(pwu, T12_SLASH_PER_PWU));
        if epoch % 2 == 0 || sat {
            println!("{epoch:>6} {w:>26} {attempts:>10} {pwu:>24} {:>12} {gain:>26}", if sat { "YES" } else { "no" });
        }
        // the maximum legal step: models produced >= max_factor x the expected blocks
        w = palw_work_target_step_v1(w, w0, 4_000, 1_000, max_factor);
    }
    match first_sat {
        Some(e) => println!(
            "\n  palw_pwu_v1 SATURATES at epoch {e} of maximum W growth = {} DAA = {:.1} days at 120 s/block",
            e * 1_000,
            (e as f64 * 1_000.0 * 120.0) / 86_400.0
        ),
        None => println!("\n  not reached inside 16 epochs"),
    }
    // What saturation does to the seat lock.
    println!("\n  seat-lock consequence at each pwu (quorum {PALW_PANEL_COLLUDING_QUORUM_V1}, margin 10 %):");
    println!("{:>24} {:>26} {:>26} {:>10}", "claim.pwu", "max_fraud_gain sompi", "seat lock required sompi", "affordable");
    let quanta = palw_execution_quantum_count_v1(dense_ccu, u128::from(PALW_EXECUTION_QUANTUM_V1), Hash64::default(), Hash64::default())
        .saturating_add(1);
    let rights = palw_realizable_before_maturity_v1(
        quanta,
        T12_WINDOW_CHALLENGE,
        T12_WINDOW_COURT,
        T12_BLOCK_MS,
        PALW_T12_PERMIT_FEE_CEILING_SOMPI,
    );
    println!("  (quanta minted = {quanta}, realizable extra rights = {rights} sompi = {:.2} MSK)", rights as f64 / 1e8);
    for pwu in [dense_ccu.min(u64::MAX as u128) as u64, 5_494 * 3_357_281_757_221_376u64.min(u64::MAX), u64::MAX] {
        let gain = (T12_ESCROW_SOMPI as u128)
            .saturating_add(palw_fork_weight_sompi_v1(pwu, T12_SLASH_PER_PWU))
            .saturating_add(rights);
        let lock = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        let ok = lock <= T12_POSTED_COLLATERAL_SOMPI as u128;
        println!("{pwu:>24} {gain:>26} {lock:>26} {:>10}", if ok { "yes" } else { "NO" });
        let _ = palw_colluding_quorum_covers_v1(T12_POSTED_COLLATERAL_SOMPI as u128, PALW_PANEL_COLLUDING_QUORUM_V1, gain);
    }
    println!("  posted collateral per genesis seat = {T12_POSTED_COLLATERAL_SOMPI} sompi = {:.2} MSK", T12_POSTED_COLLATERAL_SOMPI as f64 / 1e8);
}
