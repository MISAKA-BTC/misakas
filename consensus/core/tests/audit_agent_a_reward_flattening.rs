//! **AGENT A (t12 lane A3+A4), part 2: where ADR-0132's compute price stops being a price.**
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_agent_a_reward_flattening -- --nocapture

use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{palw_attempted_ccu_v1, palw_network_draws_q32_from_bits_v1};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

const T12_ESCROW_SOMPI: u64 = 320_084_650_080;
const T12_RATE_SOMPI_PER_GIGA: u64 = 900_000_000;

fn per_draw(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> u128 {
    palw_attempt_economic_compute_v1(profile, &rc_job_context(profile, prefill, decode), true, &PALW_ECONOMIC_COST_TABLE_V1)
        .expect("economic compute")
}

/// The escrow cap binds at `attempted_ccu >= escrow * 10^9 / rate`. Below that, pay is proportional
/// to compute; at or above it every class is paid the same escrow whatever it ran.
#[test]
fn a4_the_compute_price_flattens_once_the_network_factor_carries_the_light_class_over_the_cap() {
    let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor");
    let (fp, fd) = PALW_RC_BASE0_CANONICAL;
    let floor_draw = per_draw(&floor, fp, fd);
    let dense = qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense");
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let dense_draw = per_draw(&dense, dp, dd);

    let cap_ccu = (T12_ESCROW_SOMPI as u128) * 1_000_000_000u128 / T12_RATE_SOMPI_PER_GIGA as u128;
    println!("\n  escrow {T12_ESCROW_SOMPI} sompi, rate {T12_RATE_SOMPI_PER_GIGA} sompi/10^9 CCU");
    println!("  the price saturates at attempted_ccu >= {cap_ccu} CCU");
    println!("  floor draw {floor_draw} CCU, dense@2M draw {dense_draw} CCU  (ratio {:.1}x)", dense_draw as f64 / floor_draw as f64);
    println!("  the floor needs a network factor of {:.1} to reach the cap", cap_ccu as f64 / floor_draw as f64);

    println!("\n  bits        network draws        floor reward        dense reward   dense/floor");
    let mut first_flat: Option<u32> = None;
    for exp in 0x1cu32..=0x20 {
        for mant in [0x7f_ffffu32, 0x40_0000, 0x20_0000, 0x10_0000, 0x08_0000, 0x04_0000, 0x02_0000, 0x01_0000] {
            let bits = (exp << 24) | mant;
            let net = palw_network_draws_q32_from_bits_v1(bits);
            if net == u128::MAX {
                continue;
            }
            let f = palw_rate_priced_reward_v1(
                T12_ESCROW_SOMPI,
                palw_attempted_ccu_v1(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, net, floor_draw),
                T12_RATE_SOMPI_PER_GIGA as u128,
            );
            let d = palw_rate_priced_reward_v1(
                T12_ESCROW_SOMPI,
                palw_attempted_ccu_v1(PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, net, dense_draw),
                T12_RATE_SOMPI_PER_GIGA as u128,
            );
            if f == T12_ESCROW_SOMPI && first_flat.is_none() {
                first_flat = Some(bits);
            }
            println!("  0x{bits:08x}  {:>18.1}  {f:>18}  {d:>18}  {:>10.2}", net as f64 / 2f64.powi(32), d as f64 / f.max(1) as f64);
        }
    }
    if let Some(bits) = first_flat {
        let net = palw_network_draws_q32_from_bits_v1(bits);
        println!("\n  FIRST FLAT at bits 0x{bits:08x}: {:.0} network draws — from there the BASE-0 floor class",
            net as f64 / 2f64.powi(32));
        println!("  and the Qwen2.5 @2M class are both paid the whole {T12_ESCROW_SOMPI}-sompi escrow");
        println!("  for work that differs by {:.0}x.", dense_draw as f64 / floor_draw as f64);
        println!("  The t12 difficulty floor is 0x207fffff = 2.0 draws, so this is {:.0}x the floor difficulty.",
            net as f64 / palw_network_draws_q32_from_bits_v1(0x207f_ffff) as f64);
    }
}
