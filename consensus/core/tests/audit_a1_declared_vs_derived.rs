//! AUDIT LANE A1 — canonical (derived) work vs declared work on testnet-12.
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_a1_declared_vs_derived -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_payout_v1::{palw_attempted_ccu_v1, palw_network_draws_q32_from_bits_v1, palw_panel_share_permille_v1};
use kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1;
use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

/// t12, runtime-verified elsewhere: block subsidy 444_562_014_000 sompi x 720 permille worker carve.
const T12_ESCROW_SOMPI: u64 = 320_084_650_080;
/// `PalwEconomicPayoutV1::rate_sompi_per_giga` on t12.
const T12_RATE: u64 = 900_000_000;
/// t12 `panel_share_alpha_permille` / min / max.
const ALPHA: u32 = 100;
const SHARE_MIN: u16 = 100;
const SHARE_MAX: u16 = 300;

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("floor builds")
}
fn dense_2m_profile() -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx: 2_097_152, ..QWEN25_1_5B }).expect("dense @2M builds")
}
fn hybrid_512_profile() -> PalwShapeProfileV3 {
    qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).expect("hybrid @512 builds")
}
fn job_of(p: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    rc_job_context(p, prefill, decode)
}
fn draw_mac_eq(p: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u128 {
    let d = PalwCanonicalClassDescriptorV1::of(p, Hash64::default()).expect("one weight format");
    palw_canonical_draw_work_v1(&d, job, true).expect("derives").provisional_scalar_v1()
}

/// **The whole A1 table for the three t12 registered classes.**
#[test]
fn a1_the_t12_class_table() {
    let w0 = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    println!("\nW0 = escrow * 1e9 / rate = {} * 1e9 / {} = {w0} CCU", T12_ESCROW_SOMPI, T12_RATE);

    let rows: Vec<(&str, PalwShapeProfileV3, (u32, u32), u64)> = vec![
        ("BASE-0 floor", floor_profile(), PALW_RC_BASE0_CANONICAL, 7_708),
        ("Qwen3.6 @512", hybrid_512_profile(), qwen36_held_canonical_v1(512), 20_717_968),
        ("Qwen2.5 @2M", dense_2m_profile(), qwen25_a16_held_canonical_v1(2_097_152), 27_002_967_184),
    ];
    let mut floor_pwu = 0u64;
    for (name, p, canonical, declared_leaves) in &rows {
        let job = job_of(p, canonical.0, canonical.1);
        let work = palw_model_work_from_carriage_v1(p, &job).expect("carriage works");
        let draw = work.economic_ccu_per_claim;
        let verif = work.verification_ccu;
        // The floor keeps its own class target on t12 (the work-target fence exempts the base class);
        // every model row is priced MAX * min(1, CCU/W0).
        let target = palw_work_ticket_target_v1(draw, w0);
        let attempts = palw_expected_attempts_v1(target);
        let pwu = palw_pwu_v1(target, draw.min(u64::MAX as u128) as u64);
        if *name == "BASE-0 floor" {
            floor_pwu = pwu;
        }
        println!("\n--- {name} ---");
        println!("  canonical (prefill, decode)      {canonical:?}");
        println!("  U1 declared leaves / draw        {declared_leaves}");
        println!("  U2 derived MAC-eq / draw (C_P1)  {draw}");
        println!("  verification_ccu (full job)      {verif}");
        println!("  verif / draw                     {:.6}x", verif as f64 / draw as f64);
        println!("  U2 / U1                          {:.3}x", draw as f64 / *declared_leaves as f64);
        println!("  work ticket target               {target}  (= MAX ? {})", target == u128::MAX);
        println!("  expected attempts                {attempts}");
        println!("  claim.pwu (= fork weight)        {pwu}");
        if floor_pwu > 0 {
            println!("  fork weight / floor's            {:.2}x", pwu as f64 / floor_pwu as f64);
        }
        // panel share, 5 seats, network draws = 1 (bits 0)
        let ea_q32 = palw_expected_attempts_q32_v1(target);
        let net_q32 = palw_network_draws_q32_from_bits_v1(0);
        let c_p = palw_attempted_ccu_v1(ea_q32, net_q32, draw);
        let c_v = verif.saturating_mul(5);
        let share = palw_panel_share_permille_v1(c_p, c_v, ALPHA, SHARE_MIN, SHARE_MAX);
        println!("  C_P (attempted CCU, bits=0)      {c_p}");
        println!("  C_V (5 seats)                    {c_v}");
        println!("  panel share permille             {share}");
    }
}

/// **The declared DECODE budget is stripped from the producer's price and kept in the verifier's.**
///
/// `palw_model_work_from_carriage_v1` computes BOTH numbers from the same registrant-declared
/// canonical job: `economic_ccu_per_claim` through `palw_attempt_job_v1` (decode forced to 1) and
/// `verification_ccu` through the untouched job. The ratio of the two sets a money split.
#[test]
fn a1_declared_decode_moves_the_money_split_though_not_the_weight() {
    let p = dense_2m_profile();
    let prefill = 262_143u32;
    let w0 = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    println!("\nDense @2M, prefill fixed at {prefill}, sweeping the DECLARED decode budget:");
    println!("  {:>8} {:>24} {:>24} {:>10} {:>26} {:>8}", "decode", "economic_ccu_per_claim", "verification_ccu", "V/C", "C_P (bits=0)", "share");
    let mut first_draw = 0u128;
    for decode in [2u32, 4, 8, 16, 64, 256, 1024] {
        let job = job_of(&p, prefill, decode);
        let Some(work) = palw_model_work_from_carriage_v1(&p, &job) else {
            println!("  decode {decode}: no work (job refused by the cost walk)");
            continue;
        };
        let draw = work.economic_ccu_per_claim;
        if first_draw == 0 {
            first_draw = draw;
        }
        let target = palw_work_ticket_target_v1(draw, w0);
        let ea_q32 = palw_expected_attempts_q32_v1(target);
        let c_p = palw_attempted_ccu_v1(ea_q32, palw_network_draws_q32_from_bits_v1(0), draw);
        let c_v = work.verification_ccu.saturating_mul(5);
        let share = palw_panel_share_permille_v1(c_p, c_v, ALPHA, SHARE_MIN, SHARE_MAX);
        println!("  {decode:>8} {draw:>24} {:>24} {:>10.4} {c_p:>26} {share:>8}", work.verification_ccu, work.verification_ccu as f64 / draw as f64);
        assert_eq!(draw, first_draw, "the PRODUCER's priced draw is flat in the declared decode budget");
    }
    println!("\n  reward at t12 escrow {T12_ESCROW_SOMPI} sompi:");
    for share in [SHARE_MIN, 200u16, SHARE_MAX] {
        let pool = (T12_ESCROW_SOMPI as u128) * share as u128 / 1000;
        println!("    share {share}permille -> panel pool {pool} sompi = {:.2} MSK, producer keeps {:.2} MSK",
            pool as f64 / 1e8, (T12_ESCROW_SOMPI as u128 - pool) as f64 / 1e8);
    }
}

/// **The cliff: a class whose declared CCU reaches W gets the EASIEST possible ticket target,**
/// so its expected attempts pin at 1 and its fork weight is its whole declared CCU.
#[test]
fn a1_ccu_at_or_above_w_gets_max_target_and_unbounded_weight() {
    let w0 = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    println!("\nW0 = {w0} CCU");
    let floor_job = job_of(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
    let floor_draw = draw_mac_eq(&floor_profile(), &floor_job);
    let floor_target = palw_work_ticket_target_v1(floor_draw, w0);
    let floor_pwu = palw_pwu_v1(floor_target, floor_draw as u64);
    println!("floor: draw={floor_draw} target={floor_target} attempts={} pwu={floor_pwu}", palw_expected_attempts_v1(floor_target));

    println!("\n  {:>26} {:>10} {:>14} {:>26} {:>14}", "declared CCU/draw", "CCU/W0", "attempts", "claim.pwu", "x floor pwu");
    for ccu in [
        floor_draw,
        w0 / 2,
        w0,
        w0 * 2,
        3_357_281_757_221_376u128, // the shipped dense @2M row
        u64::MAX as u128,
    ] {
        let target = palw_work_ticket_target_v1(ccu, w0);
        let attempts = palw_expected_attempts_v1(target);
        let pwu = palw_pwu_v1(target, ccu.min(u64::MAX as u128) as u64);
        println!("  {ccu:>26} {:>10.4} {attempts:>14} {pwu:>26} {:>14.2}", ccu as f64 / w0 as f64, pwu as f64 / floor_pwu as f64);
    }
}
