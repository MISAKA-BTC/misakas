//! **AUDIT LANE A5 (supplement) — what ONE t12 `Final` costs the fold to mint permits for.**
//!
//! Lane A5's main table (`audit_ratio_table.rs`) measured permit COUNT per unit of real work.
//! This file measures the other half: the WALL COST the chain pays to mint them, by calling the
//! real `palw_execution_mint_quanta_v1` — the function `rotate_round_lane` calls inside
//! `apply_palw_transition` at a span boundary — with the credits the t12 classes actually carry.
//!
//! Run:
//!   cargo test -p kaspa-consensus-core --test audit_a5_permit_mint_cost -- --nocapture --test-threads=1

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;

fn h(n: u64) -> Hash64 {
    let mut b = [0u8; 64];
    b[..8].copy_from_slice(&n.to_le_bytes());
    Hash64::from_bytes(b)
}

fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(h(n), 0))
}

fn one_final(credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 { domain: h(1), bond: bond(7), operator_id: h(2), claim_id: h(3), execution_root: h(4), credit, accepted_blue_score: 0 }
}

/// **MEASURED**: how long `palw_execution_mint_quanta_v1` takes, and how many tickets it produces,
/// as the credit of ONE Final grows. The credits are the real t12 per-draw MAC-eq numbers where
/// they fit inside the time budget of a test.
#[test]
#[ignore = "MEASUREMENT, not a guard: times one Final's mint as its credit grows (minutes at 100% CPU in a debug build). The live guard on the bounded mint is audit_repro_01's repro_06."]
fn the_mint_cost_of_one_final_grows_superlinearly() {
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = h(99);

    println!("\n=== MINT COST OF ONE `Final`, measured through palw_execution_mint_quanta_v1 ===");
    println!("  quantum = {PALW_EXECUTION_QUANTUM_V1} (declared unit: EXPOSURE pwu; fed unit on t12: RAW MAC-eq)");
    println!("  assign_round probes a 2^16 = 65,536 round horizon, then walks linearly.\n");
    println!("  {:<24} {:>12} {:>14} {:>16} {:>14} {:>18}", "credit (raw MAC-eq)", "tickets", "wall ms", "ns per ticket", "vs prev x", "rooted bytes (>=268B)");

    let mut prev: Option<(u64, f64)> = None;
    for credit in [
        21_657_728u64,      // the t12 BASE-0 floor's derived work per draw
        100_000_000,
        1_000_000_000,
        3_000_000_000,
        6_000_000_000,
        10_000_000_000,
        13_107_200_000,     // exactly 131,072 tickets — where assign_round's probe window is full
        16_000_000_000,
        20_000_000_000,
    ] {
        let n = palw_execution_quantum_count_v1(u128::from(credit), q, seed, h(3));
        let t0 = std::time::Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[one_final(credit)], seed, q, 1_000);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(issued.len() as u32, n, "the mint issues exactly the counted tickets");
        let ns_each = ms * 1e6 / (n.max(1) as f64);
        let growth = prev.map(|(pc, pms)| (ms / pms.max(1e-9)) / (credit as f64 / pc as f64)).unwrap_or(1.0);
        println!(
            "  {credit:<24} {n:>12} {ms:>14.1} {ns_each:>16.0} {growth:>14.2} {:>18}",
            u64::from(n) * 268
        );
        prev = Some((credit, ms));
    }

    println!("\n  `vs prev x` is (time ratio) / (credit ratio): 1.0 would be linear. Anything above 1");
    println!("  is the quadratic probe in `assign_round` (palw_execution_quanta_v1.rs:200-217).");
}

/// **MEASURED**: the ticket count the two REGISTERED t12 model rows mint from one `Final`, in the
/// unit the fold actually feeds (`record_round_final` -> `credit = palw_exposure_pwu_v2(..)`, RAW
/// derived MAC-eq per draw) against the unit `PALW_EXECUTION_QUANTUM_V1` is documented in.
#[test]
fn the_t12_rows_ticket_counts_in_both_units() {
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = h(99);
    // The exposure basis the chain derives: floor declared leaves / floor derived MAC-eq per draw.
    const BASE_DECLARED: u128 = 7_708;
    const BASE_CANONICAL: u128 = 21_657_728;

    println!("\n=== TICKETS PER `Final`, the fed unit vs the declared unit ===");
    println!("  {:<24} {:>22} {:>14} {:>22} {:>14} {:>10}", "class", "credit fed (MAC-eq)", "tickets", "same, exposure pwu", "tickets", "over x");
    for (name, draw, declared_leaves) in [
        ("BASE-0 floor", 21_657_728u128, 7_708u128),
        ("Qwen3.6 hybrid @512", 158_574_200_672, 20_717_968),
        ("Qwen2.5 dense @2M", 3_357_281_757_221_376, 27_002_967_184),
    ] {
        let fed = palw_execution_quantum_count_v1(draw, q, seed, h(3));
        let in_exposure = draw * BASE_DECLARED / BASE_CANONICAL;
        let proper = palw_execution_quantum_count_v1(in_exposure, q, seed, h(3));
        println!(
            "  {name:<24} {draw:>22} {fed:>14} {in_exposure:>22} {proper:>14} {:>10.1}",
            f64::from(fed) / f64::from(proper.max(1))
        );
        // And what the class's OWN declared leaves would mint, which is the number the constant's
        // doc comment scales against ("a ~1.6M-pwu QWEN25-scale job is about 16 tickets").
        println!("      declared leaves {declared_leaves} -> {} tickets", palw_execution_quantum_count_v1(declared_leaves, q, seed, h(3)));
    }
}
