//! **The per-class terms behind `PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI`, asserted one by one.**
//!
//! The constant's doc comment prints a table. A table transcribed from a run goes stale the first
//! time a profile moves, and the constant itself would still match the card — so the drift would be
//! in the EXPLANATION, which is the part a reader trusts. These assertions are what keep the table
//! true, and they are per-term rather than on the total so a failure says which row moved.

use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_fp_devnet_v3::{PALW_MODEL_CLAIM_CONCURRENCY_V1, palw_exposure_unit_pwu_v1};

const SLASH: u128 = 5;
const RATIO: u128 = 500;
const EXPOSURE_DAA: u128 = 7_200;

#[test]
fn the_doc_comments_table_is_the_derivation() {
    let draw = |profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3, c: (u32, u32)| -> u128 {
        let d = PalwCanonicalClassDescriptorV1::of(profile, kaspa_consensus_core::Hash64::default()).unwrap();
        let j = kaspa_consensus_core::palw_base0_profile::rc_job_context(profile, c.0, c.1);
        palw_canonical_draw_work_v1(&d, &j, true).unwrap().provisional_scalar_v1()
    };

    let floor_p = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).unwrap();
    let floor_draw = draw(&floor_p, kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL);
    let floor_declared: u64 = 7_708;

    let h_p = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(
        kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx: 512,
            ..kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B
        }),
    ).unwrap();
    let h_draw = draw(&h_p, kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(512));

    let d_p = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
        kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B },
    ).unwrap();
    let d_draw = draw(&d_p, kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(2_097_152));

    // The escrow a genesis-era claim carries, as the card computes it.
    let escrow: u128 = 266_736_960;

    let msk = |s: u128| s as f64 / 1e8;
    println!("\n  floor derived/draw   {floor_draw}   declared {floor_declared}");
    let mut total = 0u128;
    for (name, d, conc) in [
        ("BASE-0 floor", floor_draw, EXPOSURE_DAA + 1),
        ("held Qwen3.6 @512", h_draw, PALW_MODEL_CLAIM_CONCURRENCY_V1 as u128),
        ("held Qwen2.5 @2M", d_draw, PALW_MODEL_CLAIM_CONCURRENCY_V1 as u128),
    ] {
        let pwu = palw_exposure_unit_pwu_v1(d, floor_declared, floor_draw) as u128;
        let reservation = pwu * SLASH;
        let gain = escrow + reservation;
        let term = gain * conc * 1000 / RATIO;
        total += term;
        println!(
            "  {name:20} draw={d:>18}  exposure_pwu={pwu:>14}  reservation={:>12.5} MSK  gain={:>12.5} MSK  x{conc:<5} -> collateral {:>13.2} MSK",
            msk(reservation), msk(gain), msk(term)
        );
    }
    println!("\n  TOTAL collateral a seat   {} sompi = {:.8} MSK", total, msk(total));
    println!("  eight seats               {:.2} MSK = {:.4}% of the 10B cap", msk(total * 8), msk(total * 8) / 1e10 * 100.0);

    // The table in `premine.rs`, term by term.
    assert_eq!(floor_draw, 21_657_728, "the floor's derived MAC-eq per draw is the basis every other row divides by");
    assert_eq!(palw_exposure_unit_pwu_v1(floor_draw, floor_declared, floor_draw), 7_708, "the floor renormalises to itself");
    assert_eq!(palw_exposure_unit_pwu_v1(h_draw, floor_declared, floor_draw), 56_436_664, "held Qwen3.6 @512 exposure pwu");
    assert_eq!(palw_exposure_unit_pwu_v1(d_draw, floor_declared, floor_draw), 1_194_858_841_364, "held Qwen2.5 @2M exposure pwu");
    assert_eq!(total, 51_642_979_663_480, "516,429.79663480 MSK a seat — the sum the premine carves");
    assert_eq!(
        total,
        kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI as u128,
        "and the constant the premine actually uses"
    );
}
