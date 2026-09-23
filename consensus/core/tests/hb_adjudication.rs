//! ADJUDICATION — the one thing the three red-team lanes did not check: the ORDER in which
//! fork choice asks its two questions.
//!
//! Lane H1 priced its critical finding (H1-1) as "K x 2^24 hashes buys a reorg of K blocks of
//! honest history" by constructing `PalwCandidateOrderV1` values directly and running
//! `decide_deep_reorg_v2` on them. That measures the comparator in isolation. In the running node
//! the comparator is NOT the thing that offers a candidate: `sink_search_algorithm`
//! (consensus/src/pipeline/virtual_processor/processor.rs:10986-10990) seeds a `BinaryHeap` of
//! `SortableBlock { blue_work }` — GHOSTDAG's own key — and the FIRST popped candidate that is
//! UTXO-valid and that `dns_reorg_outcome` accepts is returned as the sink
//! (processor.rs:11071-11099). So the PALW comparator is a downward VETO on a branch that has
//! already won on blue work; it can never promote a lighter branch.
//!
//! That changes H1-1's price, and it is the difference between "critical, reorg for 2^34 hashes"
//! and "high, a defence bypass that first requires blue-work dominance". Both halves are pinned
//! here.

use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_fork_authority_v2::{PalwDeepReorgV2, decide_deep_reorg_v2};
use kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1;
use kaspa_consensus_core::palw_heartbeat_v1::{HEARTBEAT_BLUE_WORK_EPSILON, HEARTBEAT_RECOVERY_INTERVAL_MS};
use kaspa_consensus_core::pow_layer0::{
    PALW_ATTEMPT_BLUE_WORK_LOG2, PALW_HEARTBEAT_WORK_LOG2, POW_ALGO_ID_HEARTBEAT_V1, POW_ALGO_ID_PALW_COMMITTED_V2,
    POW_ALGO_ID_PALW_EXEC_V3, POW_ALGO_ID_PALW_RECEIPT_V3, algo_id_is_priced_by_bits_v3, is_palw_attempt_algo_id,
};

fn h64(v: u64) -> kaspa_consensus_core::Hash64 {
    kaspa_consensus_core::Hash64::from_u64_word(v)
}

/// **The attempt lane pays NO hash target on t12** — so its 2^20 blue work is gated by the class
/// ticket and the bond, not by hashing.
///
/// `check_pow_layer0_v2(nonce, single_lottery = true)` returns `Ok((true, pow_512))` for an attempt
/// algo id BEFORE comparing anything to `target_512` (consensus/pow/src/lib.rs:594-596), and t12
/// arms `palw_single_lottery` at DAA 0. The predicate that says so lives in consensus-core and is
/// pinned here; the early return itself is in the `kaspa-pow` crate, quoted in the report.
///
/// The consequence cuts both ways and both matter:
///   - an attacker cannot buy attempt-lane blue work with a GPU — it needs a bond and a winning
///     class ticket, and the ticket binds the nonce to a forward, so blue work IS inference-priced;
///   - and therefore the 2^20 : 1 exchange rate below is the real wall in front of H1-1.
#[test]
fn the_attempt_lane_is_ticket_priced_not_bits_priced_on_t12() {
    let p: Params = palw_t12_shipped_params();
    assert!(p.palw_single_lottery.is_some_and(|f| f.is_active(0)), "t12 arms palw_single_lottery at DAA 0");

    println!("=== which lanes `bits` prices, past palw_single_lottery (algo_id_is_priced_by_bits_v3) ===");
    for (name, id) in [
        ("heartbeat  (8)", POW_ALGO_ID_HEARTBEAT_V1),
        ("receipt    (7)", POW_ALGO_ID_PALW_RECEIPT_V3),
        ("attempt    (6)", POW_ALGO_ID_PALW_COMMITTED_V2),
        ("attempt    (9)", POW_ALGO_ID_PALW_EXEC_V3),
    ] {
        println!("  {name}  priced_by_bits_v3 = {}", algo_id_is_priced_by_bits_v3(id));
    }
    assert!(!algo_id_is_priced_by_bits_v3(POW_ALGO_ID_PALW_COMMITTED_V2));
    assert!(!algo_id_is_priced_by_bits_v3(POW_ALGO_ID_PALW_EXEC_V3));
    assert!(is_palw_attempt_algo_id(POW_ALGO_ID_PALW_COMMITTED_V2) && is_palw_attempt_algo_id(POW_ALGO_ID_PALW_EXEC_V3));
    println!("  => on t12 NO producible lane is priced by bits: the heartbeat pays a fixed 2^{PALW_HEARTBEAT_WORK_LOG2},");
    println!("     and the attempt pays nothing to `bits` at all — its lottery is the class ticket.");
}

/// **The order of operations: blue work offers, the PALW comparator vetoes.**
///
/// This is a model of `sink_search_algorithm`, not a run of it (the real one needs the consensus
/// crate and five stores). What it pins is the arithmetic the real loop implies: a challenger that
/// does not out-blue-work the incumbent is never popped first, so the comparator is never asked
/// about it, so H1-1's frontier padding buys nothing on its own.
#[test]
fn the_comparator_is_a_veto_not_a_promoter() {
    // The honest chain: 1000 matured claims, and (being the chain that did the work) 1000 attempt
    // blocks of blue work. The attacker: one matured claim, and K heartbeats.
    let honest_attempts: u128 = 1000;
    let honest_blue_work: u128 = honest_attempts * (1u128 << PALW_ATTEMPT_BLUE_WORK_LOG2);

    let honest = PalwCandidateOrderV1::new(11_000, 1000, 0, h64(1));

    println!("=== the two questions, in the order the node asks them ===");
    println!("  Q1 (processor.rs:10986-10990)  heap key = GHOSTDAG blue work; heaviest tip is popped first");
    println!("  Q2 (processor.rs:11071)        dns_reorg_outcome -> decide_deep_reorg_v2 on that candidate");
    println!("  Q2 can only REFUSE. It cannot promote a candidate Q1 did not offer.");
    println!();
    println!("  honest branch: {honest_attempts} attempt blocks = blue work {honest_blue_work}");

    for k in [1_000u128, 1_048_576, 1_048_576 * 1000] {
        let attacker_blue_work = k * u128::from(HEARTBEAT_BLUE_WORK_EPSILON) + (1u128 << PALW_ATTEMPT_BLUE_WORK_LOG2);
        let offered = attacker_blue_work > honest_blue_work;
        // K beats raise the attacker's frontier by K above its own fork point (10_001 + K).
        let attacker = PalwCandidateOrderV1::new(10_001 + k as u64, 1, 0, h64(2));
        let veto = decide_deep_reorg_v2(&honest, &attacker);
        println!(
            "  K = {k:>12} beats: blue work {attacker_blue_work:>14} -> Q1 offers = {offered:<5} | frontier {:>8} safe 1 -> Q2 {:?}",
            10_001 + k,
            veto
        );
        if offered {
            assert_eq!(veto, PalwDeepReorgV2::Allow, "once the attacker IS offered, frontier padding carries Q2");
        } else {
            // Q2 would say Allow, but Q1 never asks.
            assert_eq!(veto, PalwDeepReorgV2::Allow);
        }
    }

    // The joint precondition, stated as the number that matters.
    let beats_for_blue_work_parity = honest_blue_work / u128::from(HEARTBEAT_BLUE_WORK_EPSILON);
    let hashes = beats_for_blue_work_parity * (1u128 << PALW_HEARTBEAT_WORK_LOG2);
    println!();
    println!("  JOINT PRECONDITION for the H1-1 bypass against this honest branch:");
    println!("    (a) blue-work parity : {beats_for_blue_work_parity} beats = {hashes} hashes = 2^{:.1}", (hashes as f64).log2());
    println!("        at 1 GH/s that is {:.1} h of hashing", hashes as f64 / 1e9 / 3600.0);
    println!("    (b) one Final claim on the attacker's own branch (bond + class draw + receipt quorum)");
    println!("    (c) K beats of frontier padding, K = the incumbent's blue-score lead: cheap by comparison");
    println!();
    println!("  So H1-1 is NOT 'a reorg for 2^34 hashes'. It is: an attacker who has ALREADY reached");
    println!("  blue-work parity does not additionally need to match PALW safe weight — heartbeats");
    println!("  substitute for it in comparator key 1. That is a defence bypass, not the purchase.");

    // 1000 attempt blocks at 2^20 each need 2^30 beats for parity; at 2^24 hashes a beat that is 2^54.
    assert_eq!(beats_for_blue_work_parity, 1000 * (1u128 << PALW_ATTEMPT_BLUE_WORK_LOG2));
}

/// The DAA clock's real cost, restated in the unit that binds: WALL CLOCK, not hashes.
///
/// `palw_clock_step_v1` (consensus/src/processes/difficulty.rs:429-470) grants the exemption only
/// at or past a cursor derived from window header TIMESTAMPS one `HEARTBEAT_RECOVERY_INTERVAL_MS`
/// apart. Header timestamps cannot run far ahead of real time, so every DAA-denominated deadline
/// costs real elapsed time and almost no hashing.
#[test]
fn every_daa_deadline_is_priced_in_wall_clock_not_hashes() {
    println!("=== the price of N DAA on a heartbeat-only branch ===");
    println!("  cursor interval = {HEARTBEAT_RECOVERY_INTERVAL_MS} ms; one DAA per mergeset that carries a beat, at most");
    for (name, daa) in [
        ("bond maturity (D1)", 1_000u64),
        ("challenge window", 1_200),
        ("execution quantum maturity", 1_200),
        ("slash liability (window_court)", 3_000),
        ("claim retirement", 3_000),
        ("bond withdrawal delay", 7_500),
        ("false-Final full exit", 10_500),
    ] {
        let hours = daa as f64 * HEARTBEAT_RECOVERY_INTERVAL_MS as f64 / 1000.0 / 3600.0;
        let hashes = daa as u128 * (1u128 << PALW_HEARTBEAT_WORK_LOG2);
        println!("  {name:<32} {daa:>6} DAA  = {hours:>6.1} h wall clock, {hashes:>14} hashes (2^{:.1}), 0 bond, 0 pwu", (hashes as f64).log2());
    }
    // 10,500 DAA is 350 h; the hashing is 2^37.4, which one CPU core produces inside that window.
    let exit_hours = 10_500.0 * HEARTBEAT_RECOVERY_INTERVAL_MS as f64 / 1000.0 / 3600.0;
    assert!((exit_hours - 350.0).abs() < 0.5);
}

/// Is the EVM lane actually on for t12? H3-2's severity depends on it and no lane printed it.
#[test]
fn is_the_evm_lane_on_for_t12() {
    let p: Params = palw_t12_shipped_params();
    println!("=== t12 EVM lane ===");
    println!("  evm_activation_daa_score = {}", p.evm_activation_daa_score);
    println!("  active at DAA 0          = {}", p.evm_activation_daa_score == 0);
    println!("  u64::MAX (inert)         = {}", p.evm_activation_daa_score == u64::MAX);
    println!("  coinbase_maturity        = {} DAA", p.coinbase_maturity);
    println!("  finality_depth           = {} blue score", p.finality_depth);
    println!("  dns_params configured    = {}", p.dns_params.is_some());
}
