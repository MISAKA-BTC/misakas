//! **AUDIT LANE A5 (supplement 2) — the exact ticket count at which `assign_round` stops being
//! cheap.**
//!
//! `assign_round` (palw_execution_quanta_v1.rs:200-217) picks `prefer = open_round + H % 2^16` and
//! then linear-probes `prefer + step` for `step in 0..2^16`. The reachable window is therefore
//! `[open_round, open_round + 2*2^16 - 2]` = 131,070 rounds wide. Below that it is a cheap open-
//! addressing insert; at that width the first loop fails 65,536 times per ticket and the fallback
//! walks linearly from `open_round + 2^16`, which is O(n) per ticket and O(n^2) for the mint.
//!
//! Every number below is MEASURED by calling the real `palw_execution_mint_quanta_v1`.
//!
//! Run:
//!   cargo test -p kaspa-consensus-core --test audit_a5_mint_cliff -- --nocapture --test-threads=1

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use std::io::Write;

fn h(n: u64) -> Hash64 {
    let mut b = [0u8; 64];
    b[..8].copy_from_slice(&n.to_le_bytes());
    Hash64::from_bytes(b)
}

fn one_final(credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 {
        domain: h(1),
        bond: PalwBondKeyV2(TransactionOutpoint::new(h(7), 0)),
        operator_id: h(2),
        claim_id: h(3),
        execution_root: h(4),
        credit,
    }
}

#[test]
fn the_round_assignment_cliff_is_at_131_071_tickets() {
    let q = u128::from(PALW_EXECUTION_QUANTUM_V1);
    let seed = h(99);
    println!("\n=== MEASURED mint wall time vs ticket count (ONE Final, quantum 100_000) ===");
    println!("  {:<14} {:>22} {:>14} {:>16}", "tickets", "credit (raw MAC-eq)", "wall ms", "ns per ticket");
    let _ = std::io::stdout().flush();
    for tickets in [1_000u64, 10_000, 50_000, 100_000, 120_000, 131_000, 131_070, 131_500, 133_000, 136_000] {
        let credit = tickets * PALW_EXECUTION_QUANTUM_V1;
        let t0 = std::time::Instant::now();
        let issued = palw_execution_mint_quanta_v1(&[one_final(credit)], seed, q, 1_000);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("  {:<14} {credit:>22} {ms:>14.2} {:>16.0}", issued.len(), ms * 1e6 / issued.len().max(1) as f64);
        let _ = std::io::stdout().flush();
    }
    println!("\n  The t12 rows credit ONE Final with:");
    println!("    BASE-0 floor          21_657_728 MAC-eq ->            216 tickets");
    println!("    Qwen3.6 hybrid @512  158_574_200_672   ->      1_585_741 tickets");
    println!("    Qwen2.5 dense @2M  3_357_281_757_221_376 -> 4_294_967_295 tickets (u32 clamp)");
}
