//! **AUDIT LANE A3 — integer behaviour on testnet-12.**
//!
//! Read-only probe. Every number printed here is produced by a real, `pub`, consensus-path
//! function in this worktree at detached HEAD 077d4c7f. Nothing is asserted from a doc comment.
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_a3_integer_sweep -- --nocapture


use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
    palw_attempted_compute_per_claim_v1, palw_attempted_compute_q32_per_claim_v1, palw_expected_attempts_q32_v1,
    palw_network_expected_attempts_q32_v1, palw_priced_reward_u128_v1,
};
use kaspa_consensus_core::palw_economic_payout_v1::{
    palw_attempted_ccu_v1, palw_cap_utilization_permille_v1, palw_network_draws_q32_from_bits_v1, palw_panel_share_permille_v1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::{PALW_LEDGER_RATE_SCALE_V1, palw_rate_priced_reward_v1};
use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecFinalV1, palw_execution_credit_v1};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_panel_economy_v1::palw_work_priced_reward_v1;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7, qwen25_a16_held_canonical_v1};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_held_canonical_v1, qwen36_profile_v7,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

// ---------------------------------------------------------------------------------------------
// fixtures (copied verbatim from consensus/core/tests/audit_harness_probe.rs; NOT imported —
// a test binary cannot import another test binary)
// ---------------------------------------------------------------------------------------------

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("BASE-0 builds")
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

/// The three t12 genesis classes: (name, derived MAC-eq per DRAW, declared leaves per inference).
/// The per-draw number is exactly what `palw_model_work_from_carriage_v1` writes into the registry
/// as `economic_ccu_per_claim`, and what `PalwChainStateV2::canonical_per_draw` returns.
fn t12_rows() -> Vec<(&'static str, u128, u64)> {
    let floor = floor_profile();
    let hybrid = hybrid_512_profile();
    let dense = dense_2m_profile();
    let f = palw_attempt_economic_compute_v1(
        &floor,
        &job_of(&floor, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1),
        true,
        &PALW_ECONOMIC_COST_TABLE_V1,
    )
    .expect("floor draw");
    let (hp, hd) = qwen36_held_canonical_v1(512);
    let h = palw_attempt_economic_compute_v1(&hybrid, &job_of(&hybrid, hp, hd), true, &PALW_ECONOMIC_COST_TABLE_V1).expect("hybrid draw");
    let (dp, dd) = qwen25_a16_held_canonical_v1(2_097_152);
    let d = palw_attempt_economic_compute_v1(&dense, &job_of(&dense, dp, dd), true, &PALW_ECONOMIC_COST_TABLE_V1).expect("dense draw");
    vec![("BASE-0 floor", f, 7_708), ("Qwen3.6 @512", h, 20_717_968), ("Qwen2.5 A16 @2M", d, 27_002_967_184)]
}

fn hh(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}
fn bondk(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}
fn final_of(claim: u64, root: u64, credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 { domain: hh(1), bond: bondk(1), operator_id: hh(2), claim_id: hh(claim), execution_root: hh(root), credit }
}

// t12 runtime constants, all re-derived below from the recon brief's measured values.
const T12_ESCROW_SOMPI: u64 = 320_084_650_080; // 444_562_014_000 * 720 / 1000
const T12_RATE_SOMPI_PER_GIGA: u64 = 900_000_000;

// =============================================================================================
// A3-1  THE EXECUTION-QUANTUM COUNT: what the u32::MAX clamp actually buys
// =============================================================================================

#[test]
fn a3_1_execution_quantum_count_on_the_real_t12_credits() {
    println!("\n=== A3-1: PALW_EXECUTION_QUANTUM_V1 = {PALW_EXECUTION_QUANTUM_V1} (declared unit: exposure pwu) ===");
    println!("{:<20} {:>22} {:>18} {:>16} {:>16}", "class", "derived MAC-eq/draw", "declared leaves", "quanta(MAC-eq)", "quanta(leaves)");
    let seed = hh(0xABCD);
    let id = hh(0x1234);
    for (name, draw_mac_eq, declared) in t12_rows() {
        // record_round_final: credit = palw_exposure_pwu_v2(class, claim.pwu, canonical)
        //                            = canonical.unwrap_or(declared)   [DerivedV1 arm]
        // and lane.execution_quantum > 0 on t12, so NO clamp at the work-price unit.
        let credit_mac_eq = draw_mac_eq.min(u64::MAX as u128) as u64;
        let n_mac = palw_execution_quantum_count_v1(u128::from(credit_mac_eq), u128::from(PALW_EXECUTION_QUANTUM_V1), seed, id);
        let n_leaf = palw_execution_quantum_count_v1(u128::from(declared), u128::from(PALW_EXECUTION_QUANTUM_V1), seed, id);
        let unclamped = u128::from(credit_mac_eq) / u128::from(PALW_EXECUTION_QUANTUM_V1);
        println!("{name:<20} {draw_mac_eq:>22} {declared:>18} {n_mac:>16} {n_leaf:>16}");
        if unclamped > u128::from(u32::MAX) {
            println!("    ^^ CLAMPED: unclamped quotient = {unclamped}, returned = {n_mac} (u32::MAX = {})", u32::MAX);
        }
    }
}

#[test]
fn a3_2_quantum_count_boundary_sweep() {
    println!("\n=== A3-2: palw_execution_quantum_count_v1 boundary sweep, quantum = {PALW_EXECUTION_QUANTUM_V1} ===");
    let (seed, id) = (hh(9), hh(9));
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let cases: Vec<(&str, u128)> = vec![
        ("0", 0),
        ("1", 1),
        ("Q-1", q - 1),
        ("Q", q),
        ("Q+1", q + 1),
        ("2Q-1", 2 * q - 1),
        ("(u32::MAX-1)*Q", (u128::from(u32::MAX) - 1) * q),
        ("u32::MAX*Q", u128::from(u32::MAX) * q),
        ("u32::MAX*Q + 1", u128::from(u32::MAX) * q + 1),
        ("(u32::MAX+1)*Q", (u128::from(u32::MAX) + 1) * q),
        ("u64::MAX (credit type max)", u128::from(u64::MAX)),
        ("u128::MAX", u128::MAX),
    ];
    println!("{:<30} {:>26} {:>14} {:>16}", "credit", "value", "quanta", "value-per-quantum");
    for (label, credit) in cases {
        let n = palw_execution_quantum_count_v1(credit, q, seed, id);
        let vpq = if n == 0 { 0 } else { credit / u128::from(n) };
        println!("{label:<30} {credit:>26} {n:>14} {vpq:>16}");
    }
    // The discontinuity, stated as an assertion so it cannot rot.
    let at_max = palw_execution_quantum_count_v1(u128::from(u32::MAX) * q, q, seed, id);
    let past_max = palw_execution_quantum_count_v1(u128::from(u64::MAX), q, seed, id);
    assert_eq!(at_max, u32::MAX);
    assert_eq!(past_max, u32::MAX, "every credit past u32::MAX*Q mints exactly u32::MAX tickets");
    println!(
        "\n  DISCONTINUITY: credit {} and credit {} both mint {} tickets. Value per ticket moves\n  from {} to {} ({}x) with no extra ticket.",
        u128::from(u32::MAX) * q,
        u64::MAX,
        u32::MAX,
        q,
        u128::from(u64::MAX) / u128::from(u32::MAX),
        (u128::from(u64::MAX) / u128::from(u32::MAX)) / q
    );
}

/// **The cost the CONSENSUS FOLD pays per Final.** `palw_execution_mint_quanta_matured_v1` loops
/// `0..n` doing one keyed blake2b-512 plus one `BTreeSet` probe per ticket, then pushes a
/// `PalwExecQuantumV1` into a `Vec` that is written into rooted state
/// (`write_round_schedule` -> `round_schedules` -> `PalwDeltaEntryV2::RoundSchedule{old,new}`).
#[test]
fn a3_3_mint_cost_scaling_and_extrapolation() {
    println!("\n=== A3-3: real cost of palw_execution_mint_quanta_v1 as n grows ===");
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = hh(0x5EED);
    let per_quantum = std::mem::size_of::<kaspa_consensus_core::palw_execution_quanta_v1::PalwExecQuantumV1>();
    println!("sizeof(PalwExecQuantumV1) = {per_quantum} bytes");
    println!("{:<12} {:>14} {:>14} {:>16} {:>14}", "n(target)", "n(minted)", "elapsed_ms", "ns_per_ticket", "Vec bytes");
    let mut last: Option<(u64, f64)> = None;
    for n_target in [1_000u64, 10_000, 40_000, 80_000, 160_000] {
        let credit = n_target * PALW_EXECUTION_QUANTUM_V1;
        let t0 = Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[final_of(1, 1, credit)], seed, q, 1_000);
        let dt = t0.elapsed();
        let ns_each = dt.as_nanos() as f64 / issued.len().max(1) as f64;
        println!(
            "{:<12} {:>14} {:>14.1} {:>16.0} {:>14}",
            n_target,
            issued.len(),
            dt.as_secs_f64() * 1_000.0,
            ns_each,
            issued.len() * per_quantum
        );
        last = Some((issued.len() as u64, dt.as_secs_f64()));
    }
    let (n_last, t_last) = last.expect("swept");
    // Linear extrapolation is the OPTIMISTIC bound: assign_round's tail is quadratic past the
    // 2^16 probe horizon, so the true cost is at least this.
    for (label, n) in [("Qwen3.6 @512 Final", 1_585_741u128), ("Qwen2.5 @2M Final (clamped)", u128::from(u32::MAX))] {
        let linear_s = t_last * (n as f64) / (n_last as f64);
        let bytes = n * per_quantum as u128;
        println!(
            "  {label:<30} n = {n:>12}  >= {:>12.1} s  ({:.2} h) linear-lower-bound, Vec = {:.2} GiB",
            linear_s,
            linear_s / 3600.0,
            bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
    }
}

// =============================================================================================
// A3-4  palw_pwu_v1 SATURATION: is u64::MAX reachable on t12, and what happens there
// =============================================================================================

#[test]
fn a3_4_pwu_saturation_reachability_per_class() {
    println!("\n=== A3-4: palw_pwu_v1 saturation, per t12 class ===");
    println!("{:<20} {:>22} {:>20} {:>26}", "class", "per-draw (MAC-eq)", "attempts to sat.", "class target that does it");
    for (name, draw, _) in t12_rows() {
        let per = draw.min(u64::MAX as u128) as u64;
        // saturation when attempts * per > u64::MAX
        let attempts_needed = (u64::MAX as u128) / (per as u128) + 1;
        // target t with expected_attempts(t) == attempts_needed  =>  t ~= 2^128/attempts - 1
        let target = if attempts_needed == 0 { u128::MAX } else { (u128::MAX / attempts_needed).saturating_sub(1) };
        let got = palw_expected_attempts_v1(target);
        let pwu = palw_pwu_v1(target, per);
        println!("{name:<20} {per:>22} {attempts_needed:>20} {target:>26}");
        println!("        at that target: expected_attempts = {got}, palw_pwu_v1 = {pwu} (u64::MAX = {})", u64::MAX);
        assert!(pwu <= u64::MAX);
    }
    // The ADR-0137 work-target rule on t12: target = MAX * min(1, CCU/W0). W0 = escrow*1e9/rate.
    let w0 = (T12_ESCROW_SOMPI as u128) * PALW_LEDGER_RATE_SCALE_V1 / (T12_RATE_SOMPI_PER_GIGA as u128);
    println!("\n  t12 work floor W0 = escrow {T12_ESCROW_SOMPI} x 1e9 / rate {T12_RATE_SOMPI_PER_GIGA} = {w0} MAC-eq");
    println!("{:<20} {:>22} {:>14} {:>12} {:>24}", "class", "CCU/draw", "CCU/W0", "attempts", "claim.pwu (derived)");
    for (name, draw, _) in t12_rows() {
        let target = if draw >= w0 { u128::MAX } else { (u128::MAX / w0).saturating_mul(draw) };
        let attempts = palw_expected_attempts_v1(target);
        let pwu = palw_pwu_v1(target, draw.min(u64::MAX as u128) as u64);
        println!("{name:<20} {draw:>22} {:>14.6} {attempts:>12} {pwu:>24}", draw as f64 / w0 as f64);
    }
}

// =============================================================================================
// A3-5  Q32: is the 2^32 scale applied exactly once on each path?
// =============================================================================================

#[test]
fn a3_5_q32_scaling_is_applied_once_on_each_path() {
    println!("\n=== A3-5: Q32 scaling ===");
    println!("PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 = {PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1} (2^32 = {})", 1u128 << 32);
    // integer vs Q32 expected attempts must agree in the integer part
    for shift in [0u32, 1, 2, 10, 24, 40, 63] {
        let t = u128::MAX >> shift;
        let int = palw_expected_attempts_v1(t);
        let q32 = palw_expected_attempts_q32_v1(t);
        println!("  shift {shift:>2}: integer = {int:>22}, q32 >> 32 = {:>22}, frac = {:>10}", q32 >> 32, q32 & 0xFFFF_FFFF);
        assert_eq!(q32 >> 32, u128::from(int), "the Q32 integer part must equal the integer form at shift {shift}");
    }
    // the nested double application in palw_attempted_ccu_v1
    let draw = 1_000_000u128;
    let class_q32 = palw_expected_attempts_q32_v1(u128::MAX / 4); // 4 draws
    let net_q32 = palw_network_draws_q32_from_bits_v1(0); // exactly 1
    let ccu = palw_attempted_ccu_v1(class_q32, net_q32, draw);
    println!("\n  class 4 draws x network 1 draw x {draw} = {ccu} (expect {})", 4 * draw);
    assert_eq!(ccu, 4 * draw, "a Q32 applied twice or not at all would be off by 2^32");
    // ADR-0131 vs ADR-0132 spelling of the SAME claim
    let adr0131 = palw_attempted_compute_per_claim_v1(palw_expected_attempts_v1(u128::MAX / 4), draw);
    println!("  palw_attempted_compute_per_claim_v1 (no network factor) = {adr0131}");
    let floor_bits = palw_network_draws_q32_from_bits_v1(0x207f_ffff);
    println!("  network_q32 at difficulty floor 0x207fffff = {floor_bits} (= {} draws)", floor_bits as f64 / (1u128 << 32) as f64);
    // the network factor at an extreme bits
    for bits in [0u32, 0x207f_ffff, 0x1d00_ffff, 0x1800_0001, 0x0300_0001, 0x0100_0001] {
        let q = palw_network_expected_attempts_q32_v1(bits);
        let n = palw_network_draws_q32_from_bits_v1(bits);
        println!("  bits {bits:#010x}: raw_q32 = {q:>42}, via_from_bits = {n:>42} (= {:.4e} draws)", n as f64 / (1u128 << 32) as f64);
    }
}

#[test]
fn a3_6_attempted_ccu_saturation_and_what_it_pays() {
    println!("\n=== A3-6: what saturation of attempted_ccu pays at the t12 escrow/rate ===");
    let escrow = T12_ESCROW_SOMPI;
    let rate = T12_RATE_SOMPI_PER_GIGA as u128;
    println!("{:<34} {:>26} {:>18} {:>12}", "attempted_ccu", "value", "priced_reward", "cap ‰");
    let w0 = (escrow as u128) * PALW_LEDGER_RATE_SCALE_V1 / rate;
    for (label, ccu) in [
        ("0", 0u128),
        ("1", 1),
        ("W0-1", w0 - 1),
        ("W0 (escrow-exact)", w0),
        ("W0+1", w0 + 1),
        ("floor draw 21_657_728", 21_657_728),
        ("dense draw 3.357e15", 3_357_281_757_221_376),
        ("u64::MAX", u128::from(u64::MAX)),
        ("u128::MAX", u128::MAX),
    ] {
        let paid = palw_rate_priced_reward_v1(escrow, ccu, rate);
        let cap = palw_cap_utilization_permille_v1(ccu, T12_RATE_SOMPI_PER_GIGA, escrow);
        println!("{label:<34} {ccu:>26} {paid:>18} {cap:>12}");
    }
    println!("\n  NOTE: palw_rate_priced_reward_v1 = (ccu * rate).saturating_mul then / 1e9, then min(escrow).");
    println!("  saturating_mul at ccu >= u128::MAX/rate = {} -> quotient 3.4e29 -> still min(escrow). No wrap.", u128::MAX / rate);
}

// =============================================================================================
// A3-7  DENOMINATORS: minimum attainable value of every divisor on a live path
// =============================================================================================

#[test]
fn a3_7_denominator_minima() {
    println!("\n=== A3-7: denominators and their minima ===");
    // palw_work_priced_reward_v1(escrow, exposure, unit): unit == 0 pays the escrow WHOLE.
    println!("palw_work_priced_reward_v1 / unit_pwu:");
    for unit in [0u64, 1, 2, 7_708, 21_657_728] {
        let r = palw_work_priced_reward_v1(T12_ESCROW_SOMPI, 1, unit);
        println!("  exposure = 1, unit = {unit:>12} -> reward = {r:>18} sompi ({:.6} MSK)", r as f64 / 1e8);
    }
    assert_eq!(palw_work_priced_reward_v1(T12_ESCROW_SOMPI, 1, 0), T12_ESCROW_SOMPI, "unit 0 pays the escrow whole for 1 pwu");
    // palw_priced_reward_u128_v1: same shape over u128
    println!("\npalw_priced_reward_u128_v1 / unit:");
    for (m, u) in [(1u128, 0u128), (1, 1), (0, 1), (1, u128::MAX), (u128::MAX - 1, u128::MAX), (1u128 << 63, (1u128 << 63) + 1)] {
        println!("  measure = {m:>40}, unit = {u:>40} -> {}", palw_priced_reward_u128_v1(T12_ESCROW_SOMPI, m, u));
    }
    // palw_cap_utilization_permille_v1 / escrow
    println!("\npalw_cap_utilization_permille_v1 / escrow_sompi:");
    for e in [0u64, 1, 2, T12_ESCROW_SOMPI] {
        println!("  escrow = {e:>18} -> {} ‰", palw_cap_utilization_permille_v1(3_357_281_757_221_376, T12_RATE_SOMPI_PER_GIGA, e));
    }
    // palw_panel_share_permille_v1 / denominator
    println!("\npalw_panel_share_permille_v1 (alpha=100‰, min=100, max=300) over C_P, C_V:");
    for (c_p, c_v) in [(0u128, 0u128), (0, 1), (1, 0), (u128::MAX, 1), (1, u128::MAX), (u128::MAX, u128::MAX)] {
        println!("  C_P = {c_p:>42}, C_V = {c_v:>42} -> {} ‰", palw_panel_share_permille_v1(c_p, c_v, 100, 100, 300));
    }
    // palw_execution_credit_v1 / unit
    println!("\npalw_execution_credit_v1(exposure, unit): unit == 0 is UNCLAMPED");
    for u in [0u64, 1, 100_000] {
        println!("  exposure = {:>22}, unit = {u:>10} -> {}", u64::MAX, palw_execution_credit_v1(u64::MAX, u));
    }
}

// =============================================================================================
// A3-8  ORDER OF OPERATIONS: where a divide precedes a multiply
// =============================================================================================

#[test]
fn a3_8_division_order_precision_loss() {
    println!("\n=== A3-8: (a/c)*b vs (a*b)/c on the live expressions ===");
    // palw_cap_utilization_permille_v1: uncapped = (ccu*rate)/1e9 ; then (uncapped*1000)/escrow
    // vs the exact (ccu*rate*1000)/(1e9*escrow)
    let escrow = T12_ESCROW_SOMPI as u128;
    let rate = T12_RATE_SOMPI_PER_GIGA as u128;
    println!("{:<26} {:>14} {:>14} {:>10}", "attempted_ccu", "as coded ‰", "exact ‰", "under by");
    for ccu in [1u128, 355u128, 100_000_000_000u128, 250_000_000_000, 284_519_688_960, 355_649_611_199] {
        let coded = palw_cap_utilization_permille_v1(ccu, T12_RATE_SOMPI_PER_GIGA, T12_ESCROW_SOMPI) as u128;
        let exact = ccu * rate * 1_000 / (PALW_LEDGER_RATE_SCALE_V1 * escrow);
        println!("{ccu:<26} {coded:>14} {exact:>14} {:>10}", exact.saturating_sub(coded));
    }
    // palw_attempted_compute_q32_per_claim_v1 splits whole/frac before the shift -- check it
    // against the exact (ea * draw) >> 32 in u128 where that does not overflow.
    println!("\npalw_attempted_compute_q32_per_claim_v1 vs exact (ea*draw)>>32:");
    for (ea, draw) in [
        (PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, 1_000_000u128),
        (PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 + 1, 1_000_000),
        (PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 * 3 / 2, 21_657_728),
        (palw_expected_attempts_q32_v1(u128::MAX / 3), 3_357_281_757_221_376),
    ] {
        let coded = palw_attempted_compute_q32_per_claim_v1(ea, draw);
        let exact = ea.checked_mul(draw).map(|p| p >> 32);
        println!("  ea_q32 = {ea:>24}, draw = {draw:>20} -> coded {coded:>26}, exact {exact:?}");
        if let Some(e) = exact {
            assert!(coded.abs_diff(e) <= 1, "whole/frac split must agree with the exact shift to within 1");
        }
    }
}
