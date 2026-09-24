//! **AUDIT LANE A3 (part 3) — confirmation runs.**
//! cargo test -p kaspa-consensus-core --test audit_a3_confirm -- --nocapture --test-threads=1

use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1;
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1};
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const T12_ESCROW_SOMPI: u64 = 320_084_650_080;
const T12_RATE: u64 = 900_000_000;

fn hh(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

/// **F1 confirmation: the mint's cost past the 2^16 probe horizon, fitted against the analytic
/// op count `(n - 131_072) * 65_536 + (n - 131_072)^2 / 2`.**
#[test]
#[ignore = "MEASUREMENT, not a guard: fits the PRE-FENCE mint's O(n^2) cliff up to n = 200,000 tickets (tens of minutes in a debug build). The live guard on the bounded mint is audit_repro_01's repro_06."]
fn a3_12_mint_cost_fit_past_the_probe_horizon() {
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = hh(0x5EED);
    println!("\n=== A3-12: mint cost past the horizon (horizon = 2^16 = 65536, fill window 131_071) ===");
    println!("{:>10} {:>14} {:>18} {:>16}", "n", "elapsed_s", "model ops", "ns_per_op");
    for n in [140_000u64, 160_000, 200_000] {
        let credit = n * PALW_EXECUTION_QUANTUM_V1;
        let f = PalwExecFinalV1 {
            domain: hh(1),
            bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 }),
            operator_id: hh(2),
            claim_id: hh(3),
            execution_root: hh(4),
            credit,
            accepted_blue_score: 0,
        };
        let t0 = Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[f], seed, q, 1_000);
        let dt = t0.elapsed().as_secs_f64();
        let over = n.saturating_sub(131_071) as f64;
        let ops = over * 65_536.0 + over * over / 2.0;
        println!("{n:>10} {dt:>14.1} {ops:>18.3e} {:>16.1}", dt * 1e9 / ops.max(1.0));
        assert_eq!(issued.len() as u64, n);
    }
    println!("\n  extrapolation with the fitted constant:");
    for (label, n) in [("Qwen3.6 @512 (1 Final)", 1_585_741f64), ("Qwen2.5 @2M, u32::MAX clamp", 4_294_967_295f64)] {
        let over = n - 131_071.0;
        let ops = over * 65_536.0 + over * over / 2.0;
        println!("    {label:<30} n = {n:>14.0}  model ops = {ops:.3e}");
    }
}

/// **F2 confirmation: two classes with IDENTICAL expected real compute per claim, whose credited
/// fork weight differs by 1.99x, selected by a 0.5 % change in one declared shape number.**
#[test]
fn a3_13_identical_real_work_different_weight() {
    let w = palw_work_floor_v1(T12_ESCROW_SOMPI, T12_RATE);
    println!("\n=== A3-13: same real compute, 1.99x the fork weight ===");
    println!("W (work target) = {w} MAC-eq\n");
    println!(
        "{:>22} {:>26} {:>16} {:>22} {:>24} {:>10}",
        "declared CCU/draw", "ticket target", "exact draws", "REAL work (draws x CCU)", "credited claim.pwu", "pwu/work"
    );
    let mut rows = Vec::new();
    for ccu in [178_718_397_587u128, 177_824_805_600u128] {
        let target = palw_work_ticket_target_v1(ccu, w);
        let q32 = palw_expected_attempts_q32_v1(target);
        let exact_draws = q32 as f64 / (1u128 << 32) as f64;
        let real_work = exact_draws * ccu as f64;
        let pwu = palw_attempt_derived_pwu_v1(target, ccu);
        println!(
            "{ccu:>22} {target:>26} {exact_draws:>16.6} {real_work:>22.0} {pwu:>24} {:>10.4}",
            pwu as f64 / real_work
        );
        rows.push((ccu, real_work, pwu, palw_expected_attempts_v1(target)));
    }
    let (ccu_a, work_a, pwu_a, att_a) = rows[0];
    let (ccu_b, work_b, pwu_b, att_b) = rows[1];
    println!("\n  integer expected_attempts: {att_a} vs {att_b}  (the floor is the whole mechanism)");
    println!("  real compute differs by {:.4} %", (work_a - work_b) / work_b * 100.0);
    println!("  declared CCU differs by  {:.4} %", (ccu_a as f64 - ccu_b as f64) / ccu_b as f64 * 100.0);
    println!("  credited fork weight differs by {:.4}x", pwu_b as f64 / pwu_a as f64);
    println!(
        "\n  => a registrant that moves its class's CCU DOWN by {:.3} % (less real arithmetic per draw)",
        (ccu_a as f64 - ccu_b as f64) / ccu_a as f64 * 100.0
    );
    println!("     is credited {:.3}x the fork-choice weight for the SAME expected compute per claim.", pwu_b as f64 / pwu_a as f64);
    assert!(pwu_b as f64 / pwu_a as f64 > 1.98);
    assert!((work_a - work_b).abs() / work_b < 0.01, "the two classes really do cost the same");

    // The live t12 hybrid row, against the same boundary.
    println!("\n  LIVE t12 rows against the nearest integer boundary:");
    println!("{:>22} {:>12} {:>10} {:>24} {:>12} {:>24}", "CCU", "W/CCU", "attempts", "claim.pwu", "pwu/W", "pwu at the boundary");
    for (name, ccu) in [("Qwen3.6 @512", 158_574_200_672u128), ("BASE-0 floor", 21_657_728u128)] {
        let target = palw_work_ticket_target_v1(ccu, w);
        let pwu = palw_attempt_derived_pwu_v1(target, ccu);
        let x = w as f64 / ccu as f64;
        let n_up = x.ceil() as u128;
        let ccu_boundary = w / n_up;
        let pwu_boundary = palw_attempt_derived_pwu_v1(palw_work_ticket_target_v1(ccu_boundary, w), ccu_boundary);
        println!(
            "{ccu:>22} {x:>12.4} {:>10} {pwu:>24} {:>12.4} {pwu_boundary:>24}   <- {name}",
            palw_expected_attempts_v1(target),
            pwu as f64 / w as f64
        );
        println!(
            "      re-declaring at CCU = {ccu_boundary} ({:+.2} % compute) credits {:.4}x the weight",
            (ccu_boundary as f64 - ccu as f64) / ccu as f64 * 100.0,
            pwu_boundary as f64 / pwu as f64
        );
    }
}
