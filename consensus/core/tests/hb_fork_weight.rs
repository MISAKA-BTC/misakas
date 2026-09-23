//! LANE H1 — can heartbeat buy fork weight and reorg resistance on testnet-12?
//!
//! Gate A, as the user posed it: one PALW-secured branch versus an N-block heartbeat-only branch.
//! Nothing here asserts a design intent; every number is read out of the shipped t12 preset or out
//! of the constants the two comparisons actually use.

use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_clock_advances_without_a_claim_v1, palw_t12_shipped_params};
use kaspa_consensus_core::palw_fork_authority_v2::{PalwDeepReorgV2, PalwIbdCommitV2, decide_deep_reorg_v2, decide_ibd_commit_v2};
use kaspa_consensus_core::palw_fork_choice::{PalwCandidateOrderV1, compare_palw_candidates_v1};
use kaspa_consensus_core::palw_heartbeat_v1::{
    HEARTBEAT_BLUE_WORK_EPSILON, HEARTBEAT_NOMINAL_INTERVAL_MS, HEARTBEAT_RECOVERY_INTERVAL_MS,
};
use kaspa_consensus_core::pow_layer0::{PALW_ATTEMPT_BLUE_WORK_LOG2, PALW_HEARTBEAT_MAX_PER_MERGESET, PALW_HEARTBEAT_WORK_LOG2};

fn h64(v: u64) -> kaspa_consensus_core::Hash64 {
    kaspa_consensus_core::Hash64::from_u64_word(v)
}

/// What t12 actually ships, printed so the rest of the lane rests on measured facts.
#[test]
fn t12_facts_the_heartbeat_lane_rests_on() {
    let p: Params = palw_t12_shipped_params();
    let never = ForkActivation::never();

    let hb = p.palw_heartbeat.expect("t12 arms the heartbeat lane");
    let aw = p.palw_attempt_work.expect("t12 arms the attempt-work constant");

    println!("=== testnet-12 shipped preset ===");
    println!("  net                          {}", p.net);
    println!("  ghostdag_k                   {}", p.ghostdag_k);
    println!("  finality_depth               {}", p.finality_depth);
    println!("  target_time_per_block        {} ms", p.target_time_per_block);
    println!("  palw_heartbeat.activation    active@0 = {}", hb.activation.is_active(0));
    println!("  palw_heartbeat.work_log2     {}   (= 2^{} hashes per beat)", hb.work_log2, hb.work_log2);
    println!("  palw_heartbeat.max_per_merge {}", hb.max_per_mergeset);
    println!("  palw_attempt_work.work_log2  {}   (= blue work {})", aw.work_log2, 1u64 << aw.work_log2);
    println!("  HEARTBEAT_BLUE_WORK_EPSILON  {}", HEARTBEAT_BLUE_WORK_EPSILON);
    println!("  palw_anchor_clock            {:?}", p.palw_anchor_clock.map(|f| f.is_active(0)));
    println!("  palw_clock_cursor            {:?}", p.palw_clock_cursor.map(|f| f.is_active(0)));
    println!("  palw_single_lottery          {:?}", p.palw_single_lottery.map(|f| f.is_active(0)));
    println!("  palw_frontier_provenance     {:?}", p.palw_frontier_provenance);
    println!("  palw_heartbeat_transparent   {:?}", p.palw_heartbeat_transparent.map(|f| f.is_active(0)));
    println!("  pow_blake2b_sha3_activation  never = {}", p.pow_blake2b_sha3_activation == never);
    println!("  pow_palw_activation          never = {}", p.pow_palw_activation == never);
    println!("  pow_palw_ollama_activation   never = {}", p.pow_palw_ollama_activation == never);
    println!("  clock advances w/o a claim   {}", palw_clock_advances_without_a_claim_v1(&p));
    println!("  heartbeat intervals          nominal {} ms / recovery {} ms", HEARTBEAT_NOMINAL_INTERVAL_MS, HEARTBEAT_RECOVERY_INTERVAL_MS);

    // The facts the rest of this file uses.
    assert!(hb.activation.is_active(0), "the heartbeat lane is open from DAA 0");
    assert_eq!(hb.work_log2, PALW_HEARTBEAT_WORK_LOG2);
    assert_eq!(hb.max_per_mergeset, PALW_HEARTBEAT_MAX_PER_MERGESET);
    assert_eq!(aw.work_log2, PALW_ATTEMPT_BLUE_WORK_LOG2);
    // ADR-0151 D3's claim, checked rather than trusted: no lane this network can produce is
    // priced by `bits`, so the heartbeat stand-in IS the clock.
    assert!(p.pow_blake2b_sha3_activation == never && p.pow_palw_activation == never && p.pow_palw_ollama_activation == never);
    assert!(palw_clock_advances_without_a_claim_v1(&p));
    // The slot rule is retired wherever `palw_clock_cursor` is active
    // (consensus/src/pipeline/header_processor/pre_pow_validation.rs:64-75).
    assert!(p.palw_clock_cursor.is_some_and(|f| f.is_active(0)), "on t12 the slot rule is retired from DAA 0");
    // ADR-0065 D2's anti-sybil frontier veto is NOT armed here.
    assert!(p.palw_frontier_provenance.is_none(), "the frontier-provenance veto is dormant on t12");
}

/// **The blue-work exchange rate**: how many heartbeats equal one PALW attempt block in the heap
/// that actually orders the sink search (`sink_search_algorithm`, blue_work-keyed `SortableBlock`).
#[test]
fn how_many_heartbeats_equal_one_attempt_block_in_blue_work() {
    let eps = HEARTBEAT_BLUE_WORK_EPSILON as u128;
    let attempt = 1u128 << PALW_ATTEMPT_BLUE_WORK_LOG2;
    let n_to_tie = attempt / eps;
    let n_to_beat = n_to_tie + 1;

    // Cost of N beats, in hash evaluations, at the lane's fixed price.
    let hashes_per_beat = 1u128 << PALW_HEARTBEAT_WORK_LOG2;
    let hashes_to_beat_one = n_to_beat * hashes_per_beat;

    println!("=== blue-work exchange rate (ghostdag protocol.rs palw_lane_blue_work_v1) ===");
    println!("  heartbeat blue work          {eps}");
    println!("  attempt    blue work         {attempt}  (2^{PALW_ATTEMPT_BLUE_WORK_LOG2})");
    println!("  receipt    blue work         0");
    println!("  beats to TIE one attempt     {n_to_tie}");
    println!("  beats to BEAT one attempt    {n_to_beat}");
    println!("  hashes per beat              2^{PALW_HEARTBEAT_WORK_LOG2} = {hashes_per_beat}");
    println!("  hashes to out-work 1 attempt {hashes_to_beat_one}  (= 2^{:.1})", (hashes_to_beat_one as f64).log2());
    for ghs in [0.1f64, 1.0, 10.0, 1000.0] {
        let secs = hashes_to_beat_one as f64 / (ghs * 1e9);
        println!("    at {ghs:>7} GH/s: {:.2} h of hashing per attempt block out-worked", secs / 3600.0);
    }

    assert_eq!(n_to_tie, 1_048_576, "2^20 beats tie one attempt block");
    assert_eq!(n_to_beat, 1_048_577);
    // The user's gate-A number, 1000 beats, is nowhere near parity on BLUE WORK.
    assert!(1000u128 * eps < attempt, "1000 heartbeats do not out-blue-work one attempt block");
}

/// **Gate A at the comparator that actually decides a reorg** — a heartbeat-only branch carries no
/// matured claim, so it has nothing on any of the three keys, at any N.
#[test]
fn gate_a_a_heartbeat_only_branch_never_reorgs_a_secured_anchor() {
    // Incumbent: one PALW-secured anchor. `accepted_blue_score` 100, one matured claim of 1 pwu.
    let secured = PalwCandidateOrderV1::new(100, 1, 0, h64(0xA));

    println!("=== gate A: N heartbeat-only blocks vs one secured anchor ===");
    for n in [1u64, 10, 1_000, 1_000_000, 1_000_000_000, u64::MAX / 2] {
        // A heartbeat-only branch: no claim ever matured on it, so its frontier stayed where the
        // fork point left it (0 here) and both weights are 0. N only shows up as blue score, and
        // blue score reaches this comparator ONLY through a Final claim's `accepted_blue_score`.
        let beats_only = PalwCandidateOrderV1::new(0, 0, 0, h64(0xB));
        let verdict = decide_deep_reorg_v2(&secured, &beats_only);
        let ibd = decide_ibd_commit_v2(&secured, &beats_only);
        println!("  N = {n:>20}: deep_reorg {verdict:?}  ibd_commit {ibd:?}");
        assert_eq!(verdict, PalwDeepReorgV2::Refuse, "N = {n}");
        assert_eq!(ibd, PalwIbdCommitV2::KeepIncumbent, "N = {n}");
    }
    println!("  -> no N exists: a branch with no matured claim loses key 1 and key 2 outright.");
}

/// **The hole**: key 1 of the one comparator is a BLUE SCORE, and heartbeat blocks are the cheapest
/// blue score on this network. A branch that matures ONE claim after K heartbeats presents
/// `safe_frontier_blue_score = fork + K + 1` and outranks a branch that matured a thousand claims
/// at a lower blue score — before `safe_weight` is ever consulted.
///
/// Mechanism: `palw_state_v2.rs:12777` sets `safe_frontier_blue_score = claim.accepted_blue_score`;
/// `palw_fork_choice.rs:72` compares that field FIRST.
#[test]
fn heartbeat_padding_buys_key_one_of_the_fork_choice_order() {
    let fork_point_blue_score = 10_000u64;

    // Honest branch: 1000 matured claims, one per attempt block, no padding. Its deepest Final sits
    // 1000 blue scores above the fork point and its safe weight is 1000 pwu-units.
    let honest_blocks = 1_000u64;
    let honest = PalwCandidateOrderV1::new(fork_point_blue_score + honest_blocks, honest_blocks as u128, 0, h64(1));

    // Attacker branch: K heartbeat blocks, then ONE attempt block that matures. safe_weight = 1.
    println!("=== heartbeat padding vs a thousand matured claims ===");
    println!("  honest : frontier {:>7}  safe {:>6}", honest.safe_frontier_blue_score, honest.safe_weight);
    let mut smallest_k_that_wins = None;
    for k in [0u64, 500, 999, 1_000, 1_001, 5_000] {
        let attacker = PalwCandidateOrderV1::new(fork_point_blue_score + k + 1, 1, 0, h64(2));
        let verdict = decide_deep_reorg_v2(&honest, &attacker);
        println!(
            "  K = {k:>5} beats: attacker frontier {:>7} safe {:>3} -> deep_reorg {:?}",
            attacker.safe_frontier_blue_score, attacker.safe_weight, verdict
        );
        if verdict == PalwDeepReorgV2::Allow && smallest_k_that_wins.is_none() {
            smallest_k_that_wins = Some(k);
        }
    }
    // Binary-search the exact price in beats.
    let wins = |k: u64| {
        decide_deep_reorg_v2(&honest, &PalwCandidateOrderV1::new(fork_point_blue_score + k + 1, 1, 0, h64(2))) == PalwDeepReorgV2::Allow
    };
    let (mut lo, mut hi) = (0u64, 1_000_000u64);
    assert!(wins(hi));
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if wins(mid) { hi = mid } else { lo = mid + 1 }
    }
    let hashes = (lo as u128) * (1u128 << PALW_HEARTBEAT_WORK_LOG2);
    println!("  smallest K that flips the deep-reorg gate to Allow: {lo} beats");
    println!("  price: {lo} x 2^{PALW_HEARTBEAT_WORK_LOG2} = {hashes} hashes = 2^{:.1}", (hashes as f64).log2());
    println!("  ...and the attacker's branch carries safe_weight 1 against the honest branch's {}", honest.safe_weight);

    assert!(wins(lo), "K = {lo} wins");
    assert!(!wins(lo - 1), "K = {} does not", lo - 1);
    assert_eq!(lo, honest_blocks, "the price is exactly one beat per honest matured block");
    // Key 2 never gets a vote: the attacker is 1000x lighter in matured work and still wins.
    let attacker = PalwCandidateOrderV1::new(fork_point_blue_score + lo + 1, 1, 0, h64(2));
    assert!(attacker.safe_weight * 1000 <= honest.safe_weight * 1000);
    assert!(attacker.safe_weight < honest.safe_weight, "and it is strictly lighter in matured work");
    assert_eq!(compare_palw_candidates_v1(&attacker, &honest), core::cmp::Ordering::Greater);
    assert_eq!(decide_ibd_commit_v2(&honest, &attacker), PalwIbdCommitV2::Commit, "IBD commits to it too");
}

/// The same unit confusion, stated as the invariant the user asked for.
/// `N heartbeats => economic work depth +0` holds for `safe_weight`; it does NOT hold for
/// `safe_frontier_blue_score`, which is the key compared first.
#[test]
fn the_invariant_holds_for_weight_and_fails_for_the_frontier_key() {
    let base = PalwCandidateOrderV1::new(500, 42, 7, h64(9));
    // Adding beats cannot change either weight — nothing about a heartbeat block is pwu.
    let after_beats_weights = PalwCandidateOrderV1::new(500, 42, 7, h64(9));
    assert_eq!(base.safe_weight, after_beats_weights.safe_weight);
    assert_eq!(base.live_total, after_beats_weights.live_total);
    println!("  N heartbeats -> safe_weight  +0   OK");
    println!("  N heartbeats -> live_total   +0   OK");

    // But the frontier is measured in blue score, and every heartbeat is +1 blue score, so the very
    // next matured claim records a frontier N higher than it otherwise would.
    let padded = PalwCandidateOrderV1::new(500 + 4_000, 42, 7, h64(9));
    println!("  N heartbeats -> safe_frontier_blue_score +N   NOT OK (500 -> {})", padded.safe_frontier_blue_score);
    assert!(padded.safe_frontier_blue_score > base.safe_frontier_blue_score);
    assert_eq!(compare_palw_candidates_v1(&padded, &base), core::cmp::Ordering::Greater);
}

/// **The other depth gates, and the unit each one counts in.** Everything printed here is read off
/// the shipped t12 preset; the clock each gate reads is named beside it with its file:line.
#[test]
fn every_depth_gate_and_the_clock_it_reads() {
    let p = palw_t12_shipped_params();
    let beat = 1u128 << PALW_HEARTBEAT_WORK_LOG2;

    println!("=== depth gates on t12 ===");
    println!("  finality_depth      {:>6} BLUE SCORE   consensus/src/processes/block_depth.rs:55,69", p.finality_depth);
    println!("  merge_depth         {:>6} BLUE SCORE   consensus/src/processes/block_depth.rs:51,68", p.merge_depth);
    println!("  coinbase_maturity   {:>6} DAA SCORE    tx_validation_in_utxo_context.rs:95-107", p.coinbase_maturity());
    println!("  pruning ceiling            safe_frontier BLUE SCORE   palw_fork_authority_v2.rs:70-77");
    println!("  sink-search heap           GHOSTDAG blue work         virtual_processor/processor.rs:10990");
    println!("  deep-reorg / IBD gate      frontier BLUE SCORE, then safe pwu, then live pwu");
    println!("  dns_params configured      {}", p.dns_params.is_some());

    println!("\n=== price, in heartbeat hashes, of moving each BLUE-SCORE gate one full window ===");
    for (name, depth) in [("finality_depth", p.finality_depth), ("merge_depth", p.merge_depth)] {
        let hashes = depth as u128 * beat;
        println!("  {name:<16} {depth:>6} beats = {hashes} hashes = 2^{:.1}", (hashes as f64).log2());
        for ghs in [1.0f64, 10.0] {
            println!("        at {ghs:>5} GH/s: {:.1} s", hashes as f64 / (ghs * 1e9));
        }
    }

    // The DAA clock is the one thing the cursor rate-limits: at most one exemption per
    // HEARTBEAT_RECOVERY_INTERVAL_MS of wall clock (difficulty.rs:437-476).
    let daa_secs = p.coinbase_maturity() as f64 * (HEARTBEAT_RECOVERY_INTERVAL_MS as f64 / 1000.0);
    println!("\n  coinbase_maturity is DAA, and DAA is cursor-capped at 1 / {} ms:", HEARTBEAT_RECOVERY_INTERVAL_MS);
    println!("    {} DAA of heartbeat-only history costs {:.1} h of WALL CLOCK (not hashes)", p.coinbase_maturity(), daa_secs / 3600.0);
}

/// The DNS-BFT veto — the one gate that refuses a reorg *before* the PALW comparator — expires on
/// a clock heartbeats drive. `dns_bft.rs:553` releases the veto when
/// `confirmed_anchor_is_stale(incumbent_daa, anchor_daa)`, and `incumbent_daa` is DAA score.
#[test]
fn the_dns_veto_ttl_is_a_daa_clock_and_heartbeats_drive_daa() {
    let p = palw_t12_shipped_params();
    let dns = p.dns_params.as_ref().expect("t12 configures the DNS overlay");
    println!("=== DNS-BFT reorg veto ===");
    println!("  dns_veto_ttl_daa_score       {}", dns.dns_veto_ttl_daa_score);
    println!("  unit                         DAA SCORE (dns_bft.rs:553, confirmed_anchor_is_stale)");
    let secs = dns.dns_veto_ttl_daa_score as f64 * (HEARTBEAT_RECOVERY_INTERVAL_MS as f64 / 1000.0);
    println!("  heartbeat-only cost to expire the veto: {} DAA = {:.1} h wall clock, 0 bond, 0 pwu", dns.dns_veto_ttl_daa_score, secs / 3600.0);
    println!("  hash cost of those beats:    {} x 2^24 = 2^{:.1}", dns.dns_veto_ttl_daa_score,
        ((dns.dns_veto_ttl_daa_score as u128 * (1u128 << PALW_HEARTBEAT_WORK_LOG2)) as f64).log2());
    assert!(dns.dns_veto_ttl_daa_score > 0);
}

/// **The sustained price of blue-work parity with the honest chain.** The sink-search heap
/// (`processor.rs:10990`) is keyed on GHOSTDAG blue work alone, and on t12 the honest chain earns
/// 2^20 per attempt block at one block per `target_time_per_block`.
#[test]
fn the_hashrate_that_keeps_a_heartbeat_branch_at_blue_work_parity() {
    let p = palw_t12_shipped_params();
    let attempt = (1u64 << PALW_ATTEMPT_BLUE_WORK_LOG2) as f64;
    let block_s = p.target_time_per_block as f64 / 1000.0;
    let honest_bw_per_s = attempt / block_s;
    let hashes_per_bw = (1u64 << PALW_HEARTBEAT_WORK_LOG2) as f64 / HEARTBEAT_BLUE_WORK_EPSILON as f64;
    let needed_hs = honest_bw_per_s * hashes_per_bw;

    println!("=== sustained blue-work parity ===");
    println!("  honest chain: 1 attempt block / {block_s} s at blue work {attempt} = {honest_bw_per_s:.0} blue-work/s");
    println!("  heartbeat:    {hashes_per_bw:.0} hashes per 1 blue-work");
    println!("  hashrate to match the honest chain's blue work: {:.1} GH/s", needed_hs / 1e9);
    println!("  (blue SCORE parity is far cheaper: 1 beat / honest block = {:.4} GH/s)",
        (1.0 / block_s) * (1u64 << PALW_HEARTBEAT_WORK_LOG2) as f64 / 1e9);
    assert!(needed_hs > 1e11, "parity is above 100 GH/s: {needed_hs}");
}
