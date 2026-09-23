//! AUDIT LANE A1 — the registrant's canonical job decouples DECLARED leaves from DERIVED draw work.
//! Run: cargo test -p kaspa-consensus-core --test audit_a1_deepest_job_decoupling -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1;
use kaspa_consensus_core::palw_economic_payout_v1::{palw_attempted_ccu_v1, palw_network_draws_q32_from_bits_v1, palw_panel_share_permille_v1};
use kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1, worst_case_step_leaf_count_deepest_job_capped_v1};
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

const T12_ESCROW_SOMPI: u64 = 320_084_650_080;
const T12_RATE: u64 = 900_000_000;
const SLASH: u128 = 5;
/// The t12 floor's two measures — the exposure basis every other class is renormalised through.
const FLOOR_DECLARED: u128 = 7_708;
const FLOOR_CANONICAL: u128 = 21_657_728;
/// `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` on the shipped card: 2^26.
const LADDER: u64 = 1 << 26;

fn row(p: &PalwShapeProfileV3, label: &str, prefill: u32, decode: u32, w0: u128, worst: u64) {
    let job = rc_job_context(p, prefill, decode);
    let counted = match step_leaf_count_capped_v1(p, &job, LADDER) {
        Ok(c) => c,
        Err(e) => {
            println!("  {label:<22} (prefill {prefill}, decode {decode}) REFUSED by the leaf counter: {e:?}");
            return;
        }
    };
    let Some(work) = palw_model_work_from_carriage_v1(p, &job) else {
        println!("  {label:<22} no carriage work");
        return;
    };
    let d = PalwCanonicalClassDescriptorV1::of(p, Hash64::default()).expect("one weight format");
    let draw = palw_canonical_draw_work_v1(&d, &job, true).expect("derives").provisional_scalar_v1();
    assert_eq!(draw, work.economic_ccu_per_claim, "the registry row IS the canonical draw work");
    let target = palw_work_ticket_target_v1(draw, w0);
    let attempts = palw_expected_attempts_v1(target);
    let pwu = palw_pwu_v1(target, draw.min(u64::MAX as u128) as u64);
    let exposure_u3 = draw * FLOOR_DECLARED / FLOOR_CANONICAL;
    let reserved = exposure_u3 * SLASH;
    let ea_q32 = palw_expected_attempts_q32_v1(target);
    let c_p = palw_attempted_ccu_v1(ea_q32, palw_network_draws_q32_from_bits_v1(0), draw);
    let c_v = work.verification_ccu.saturating_mul(5);
    let share = palw_panel_share_permille_v1(c_p, c_v, 100, 100, 300);
    println!("  {label:<22} (P={prefill}, D={decode})");
    println!("     U1 declared leaves  = counted = {counted}   (worst/deepest = {worst}; admissible: {})", counted <= worst);
    println!("     U2 derived draw CCU               {draw}");
    println!("     verification_ccu (full job)       {}   ({:.2}x the draw)", work.verification_ccu, work.verification_ccu as f64 / draw as f64);
    println!("     U1/U2                             {:.4}", counted as f64 / draw as f64);
    println!("     work ticket target == MAX?        {}", target == u128::MAX);
    println!("     expected attempts / block         {attempts}");
    println!("     claim.pwu (fork weight/block)     {pwu}");
    println!("     U3 exposure pwu                   {exposure_u3}");
    println!("     reserved collateral / claim       {reserved} sompi = {:.6} MSK", reserved as f64 / 1e8);
    println!("     panel share permille (5 seats)    {share}");
}

#[test]
fn a1_deepest_job_decouples_declared_leaves_from_the_priced_draw() {
    let w0 = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    println!("\nW0 = {w0} CCU\n");
    for n_ctx in [512u32, 4096] {
        let p = match qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }) {
            Ok(p) => p,
            Err(e) => {
                println!("n_ctx {n_ctx}: profile refused: {e:?}");
                continue;
            }
        };
        let worst = match worst_case_step_leaf_count_deepest_job_capped_v1(&p, LADDER) {
            Ok(w) => w,
            Err(e) => {
                println!("n_ctx {n_ctx}: no worst case under the 2^26 ladder: {e:?}");
                continue;
            }
        };
        println!("=== Qwen2.5-1.5B A16 graph-v7 @ n_ctx {n_ctx}; deepest-job leaf bound = {worst} ===");
        row(&p, "prefill-heavy", n_ctx - 2, 2, w0, worst);
        row(&p, "decode-heavy (deepest)", 1, n_ctx - 1, w0, worst);
        println!();
    }
}
