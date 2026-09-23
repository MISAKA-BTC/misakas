//! **The per-class terms behind `PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI`, asserted one by one.**
//!
//! The constant's doc comment prints a table. A table transcribed from a run goes stale the first
//! time a profile moves, and the constant itself would still match the card — so the drift would be
//! in the EXPLANATION, which is the part a reader trusts. These assertions are what keep the table
//! true, and they are per-term rather than on the total so a failure says which row moved.

use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_fp_devnet_v3::{
    PALW_MODEL_CLAIM_CONCURRENCY_V1, PALW_T12_GENESIS_FLOOR_CONCURRENCY_V1, palw_exposure_unit_pwu_v1,
};

const SLASH: u128 = 5;
const RATIO: u128 = 500;

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

    let dense_draw = |n_ctx: u32| {
        let p = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
            kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 { n_ctx, ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B },
        )
        .unwrap();
        draw(&p, kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(n_ctx))
    };
    let d8k_draw = dense_draw(8_192);
    let d2m_draw = dense_draw(2_097_152);

    // **The escrow a genesis-era claim carries, as the card computes it** (option A, the 2026-09-23
    // audit's U2): the subsidy the FIRST block really pays, through the overlay's worker carve — not
    // `pre_deflationary_phase_base_subsidy`, which no ConsensusV2 block ever pays.
    let base = kaspa_consensus_core::config::params::palw_t12_base_params();
    let subsidy = kaspa_consensus_core::config::params::palw_genesis_block_subsidy_sompi(&base) as u128;
    let carve = base.palw_overlay_carve.map(|c| c.worker_carve_permille).unwrap_or(0) as u128;
    let escrow = subsidy / 1_000 * carve;

    let msk = |s: u128| s as f64 / 1e8;
    println!("\n  floor derived/draw   {floor_draw}   declared {floor_declared}   escrow {escrow} ({:.5} MSK)", msk(escrow));
    let mut total = 0u128;
    for (name, d, conc) in [
        ("BASE-0 floor", floor_draw, PALW_T12_GENESIS_FLOOR_CONCURRENCY_V1 as u128),
        ("held Qwen2.5 @8,192", d8k_draw, PALW_MODEL_CLAIM_CONCURRENCY_V1 as u128),
        ("held Qwen2.5 @2M", d2m_draw, PALW_MODEL_CLAIM_CONCURRENCY_V1 as u128),
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
    assert_eq!(subsidy, 444_562_014_000, "the subsidy block one pays at the 120 s cadence");
    assert_eq!(carve, 720, "the overlay's worker carve");
    assert_eq!(escrow, 320_084_650_080, "3,200.84650080 MSK: the escrow a genesis-era claim carries");
    assert_eq!(floor_draw, 21_657_728, "the floor's derived MAC-eq per draw is the basis every other row divides by");
    assert_eq!(palw_exposure_unit_pwu_v1(floor_draw, floor_declared, floor_draw), 7_708, "the floor renormalises to itself");
    assert_eq!(palw_exposure_unit_pwu_v1(d8k_draw, floor_declared, floor_draw), 494_320_046, "held Qwen2.5 @8,192 exposure pwu");
    assert_eq!(palw_exposure_unit_pwu_v1(d2m_draw, floor_declared, floor_draw), 1_194_858_841_364, "held Qwen2.5 @2M exposure pwu");
    assert_eq!(PALW_T12_GENESIS_FLOOR_CONCURRENCY_V1, 64, "the floor concurrency a genesis seat is sized for");
    assert_eq!(total, 93_906_321_001_040, "939,063.21001040 MSK a seat — the sum the premine carves");
    assert_eq!(
        total,
        kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI as u128,
        "and the constant the premine actually uses"
    );
}
