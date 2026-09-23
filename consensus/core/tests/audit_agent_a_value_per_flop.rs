//! **AGENT A (t12 lane A3+A4), part 3: sompi per real MAC-equivalent, per t12 class.**
//!
//! Uses each class's GENESIS target (params.rs:10593/10628/10743 card) and the t12 difficulty
//! floor bits, and asks the question the lane asks: how much value does one unit of real compute
//! buy in each class?
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_agent_a_value_per_flop -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1, palw_expected_attempts_q32_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{palw_attempted_ccu_v1, palw_network_draws_q32_from_bits_v1};
use kaspa_consensus_core::palw_economic_safety_v1::PALW_T12_PERMIT_FEE_CEILING_SOMPI;
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

const T12_ESCROW_SOMPI: u64 = 320_084_650_080;
const T12_RATE_SOMPI_PER_GIGA: u64 = 900_000_000;
const T12_DIFFICULTY_FLOOR_BITS: u32 = 0x207f_ffff;

// The genesis card's `initial_target` per class (recon-verified against the t12 ConsensusV2 bundle).
const FLOOR_TARGET: u128 = 1_218_938_590_613_259_230_285_389_391_303_016_447;
const HYBRID_TARGET: u128 = 3_282_893_071_338_656_179_608_481_139_309_674_495;
const DENSE_TARGET: u128 = u128::MAX;

fn per_draw(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> u128 {
    palw_attempt_economic_compute_v1(profile, &rc_job_context(profile, prefill, decode), true, &PALW_ECONOMIC_COST_TABLE_V1)
        .expect("economic compute")
}

#[test]
fn a4_sompi_per_real_mac_eq_by_class_at_the_t12_genesis_targets() {
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor");
    let hybrid = qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid");
    let dense = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense");
    let (fp, fd) = PALW_RC_BASE0_CANONICAL;
    let (hp, hd) = qwen36_held_canonical_v1(512);
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);

    let rows: [(&str, u128, u128); 3] = [
        ("BASE-0 floor", per_draw(&floor, fp, fd), FLOOR_TARGET),
        ("Qwen3.6 @512", per_draw(&hybrid, hp, hd), HYBRID_TARGET),
        ("Qwen2.5 @2M ", per_draw(&dense, dp, dd), DENSE_TARGET),
    ];

    let net_q32 = palw_network_draws_q32_from_bits_v1(T12_DIFFICULTY_FLOOR_BITS);
    let net_draws = net_q32 as f64 / 2f64.powi(32);
    println!("\n  t12 difficulty floor bits 0x{T12_DIFFICULTY_FLOOR_BITS:08x} -> {net_draws} network draws per win");
    println!("  escrow per claim {T12_ESCROW_SOMPI} sompi, rate {T12_RATE_SOMPI_PER_GIGA} sompi per 10^9 CCU\n");

    let mut best: Option<(&str, f64)> = None;
    let mut worst: Option<(&str, f64)> = None;
    for (name, draw, target) in rows {
        let class_attempts = palw_expected_attempts_v1(target);
        let class_q32 = palw_expected_attempts_q32_v1(target);
        // The compute the producer REALLY spends to land one claim: class draws x network draws,
        // each a full execution of the canonical job (ADR-0072: a lost network draw is a lost
        // inference).
        let real_mac_eq = palw_attempted_ccu_v1(class_q32, net_q32, draw);
        let reward = palw_rate_priced_reward_v1(T12_ESCROW_SOMPI, real_mac_eq, T12_RATE_SOMPI_PER_GIGA as u128);
        let sompi_per_mac = reward as f64 / real_mac_eq as f64;
        // The second primitive the same claim collects: execution permits, at the chain's own
        // declared permit value.
        let quanta = palw_execution_quantum_count_v1(draw, PALW_EXECUTION_QUANTUM_V1 as u128, Hash64::default(), Hash64::default());
        let permit_sompi = quanta as u128 * PALW_T12_PERMIT_FEE_CEILING_SOMPI as u128;
        let total = reward as u128 + permit_sompi;
        let total_per_mac = total as f64 / real_mac_eq as f64;
        println!("  {name}");
        println!("    per-draw CCU         {draw}");
        println!("    class expected draws {class_attempts}  (target {target})");
        println!("    real MAC-eq a claim  {real_mac_eq}");
        println!("    escrow reward        {reward} sompi   capped = {}", reward == T12_ESCROW_SOMPI);
        println!("    execution permits    {quanta} x {PALW_T12_PERMIT_FEE_CEILING_SOMPI} = {permit_sompi} sompi");
        println!("    cash  per MAC-eq     {sompi_per_mac:.9} sompi");
        println!("    total per MAC-eq     {total_per_mac:.9} sompi  (cash + permits)\n");
        if best.is_none_or(|(_, v)| total_per_mac > v) {
            best = Some((name, total_per_mac));
        }
        if worst.is_none_or(|(_, v)| total_per_mac < v) {
            worst = Some((name, total_per_mac));
        }
    }
    let (bn, bv) = best.expect("rows");
    let (wn, wv) = worst.expect("rows");
    println!("  BEST  value per unit of real compute: {bn} at {bv:.9} sompi/MAC-eq");
    println!("  WORST value per unit of real compute: {wn} at {wv:.9} sompi/MAC-eq");
    println!("  ratio = {:.1}x — a miner maximises MSK per FLOP by mining the {bn} row.", bv / wv);
}
