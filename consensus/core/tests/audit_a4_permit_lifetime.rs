//! **AUDIT LANE A4 (part 4) — the permit a Final mints is scheduled onto a round whose schedule
//! has already been pruned.** FIXED (the 2026-09-23 route-matrix audit's #2): past ADR-0151's
//! bundle `rotate_round_lane` delays the SNAPSHOT by the maturity and mints with
//! `palw_execution_mint_quanta_windowed_v1`, which puts every ticket on the span's own rounds. This
//! file keeps the measurement of what the old mint did and asserts what the fold's mint does now;
//! the fold itself is pinned by
//! `past_the_economic_safety_bundle_a_finals_tickets_land_where_their_schedule_is_judged`.
//!
//! What it measured:
//!
//! `rotate_round_lane` (palw_state_v2.rs:10824) mints with
//! `maturity_rounds = palw_exec_quantum_maturity_daa_v1(window_challenge, window_court)
//!                    * palw_rounds_per_daa_v1(target_time_per_block_ms)`
//! and then, at every block, prunes every `round_schedules` key below `span_now - 1`
//! (palw_state_v2.rs:10905). A round block is judged against the schedule of its anchor's span,
//! and only "this block's span or the one before" (processor.rs:8110 doc).
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_a4_permit_lifetime -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_economic_safety_v1::{palw_exec_quantum_maturity_daa_v1, palw_rounds_per_daa_v1};
use kaspa_consensus_core::palw_execution_lane_v1::{PALW_EXEC_ROUND_MS, PalwExecFinalV1};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_matured_v1, palw_execution_mint_quanta_windowed_v1,
    palw_execution_span_rounds_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

#[test]
fn the_folds_mint_puts_every_ticket_inside_the_span_its_schedule_is_judged_at() {
    let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("ConsensusV2") };
    let lane = params.palw_execution_lane.expect("t12 opens the lane");
    let ttpb = params.target_time_per_block_history().after();

    let span_daa = lane.schedule_span_daa;
    let rounds_per_daa = palw_rounds_per_daa_v1(ttpb);
    // rotate_round_lane reads `self.params.window_challenge()` — the UNSHORTENED window, not
    // `window_challenge_at`, which on t12 returns the 120-DAA short window from DAA 0.
    let maturity_daa = palw_exec_quantum_maturity_daa_v1(bundle.state.window_challenge(), bundle.state.window_court());
    let maturity_rounds = maturity_daa.saturating_mul(rounds_per_daa);

    println!("\n=== t12 lane geometry ===");
    println!("  PALW_EXEC_ROUND_MS        {PALW_EXEC_ROUND_MS} ms  -> 1 round = 1 second");
    println!("  target_time_per_block     {ttpb} ms -> {rounds_per_daa} rounds per DAA");
    println!("  schedule_span_daa         {span_daa} DAA -> 1 span = {} rounds = {:.1} min", span_daa * rounds_per_daa, (span_daa * rounds_per_daa) as f64 / 60.0);
    println!("  window_challenge()        {} DAA   (window_challenge_at(0) = {} DAA, the SHORT one)", bundle.state.window_challenge(), bundle.state.window_challenge_at(0));
    println!("  window_court              {} DAA", bundle.state.window_court());
    println!("  maturity                  {maturity_daa} DAA = {maturity_rounds} rounds = {:.1} hours", maturity_rounds as f64 / 3600.0);

    println!("\n=== how long a schedule lives ===");
    println!("  rotate_round_lane prunes round_schedules below span_now - 1 (palw_state_v2.rs:10905)");
    println!("  so a schedule written at span S is readable at spans S and S+1 only");
    let schedule_life_rounds = 2 * span_daa * rounds_per_daa;
    println!("  = {schedule_life_rounds} rounds = {:.1} minutes of round coverage", schedule_life_rounds as f64 / 60.0);

    // Mint one hybrid-sized Final exactly the way rotate_round_lane does, but with a credit small
    // enough to finish: the SCHEDULED ROUND is what this test is about, not the count.
    let open_round = 1_000_000u64;
    let f = PalwExecFinalV1 {
        domain: Hash64::from_u64_word(1),
        bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 }),
        operator_id: Hash64::from_u64_word(2),
        claim_id: Hash64::from_u64_word(3),
        execution_root: Hash64::from_u64_word(4),
        credit: 40 * PALW_EXECUTION_QUANTUM_V1,
        accepted_blue_score: 0,
    };
    let issued = palw_execution_mint_quanta_matured_v1(
        &[f],
        Hash64::from_u64_word(7),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        open_round,
        maturity_rounds,
        &std::collections::BTreeSet::new(),
    );
    let earliest = issued.iter().map(|q| q.scheduled_round).min().expect("tickets");
    let latest = issued.iter().map(|q| q.scheduled_round).max().expect("tickets");
    println!("\n=== a mint at open_round = {open_round} ===");
    println!("  tickets                {}", issued.len());
    println!("  earliest scheduled     {earliest}  (+{} rounds)", earliest - open_round);
    println!("  latest scheduled       {latest}  (+{} rounds)", latest - open_round);
    println!("  last readable round    {}  (+{schedule_life_rounds})", open_round + schedule_life_rounds);
    println!(
        "  gap between the last readable round and the earliest ticket: {} rounds = {:.1} hours",
        earliest - (open_round + schedule_life_rounds),
        (earliest - (open_round + schedule_life_rounds)) as f64 / 3600.0
    );

    assert!(
        earliest > open_round + schedule_life_rounds,
        "the OLD mint put every ticket after its schedule was pruned: earliest {earliest} vs last readable {}",
        open_round + schedule_life_rounds
    );
    println!("\n  => with the old mint NO execution quantum was convertible into an algo-10 permit.");

    // **The mint the fold runs now.** The maturity was served before the snapshot was taken (the
    // snapshot targets `span + 1 + maturity spans`), so the schedule's tickets start at its own
    // opening round, and a span's rounds are how many the window holds.
    let span_rounds = palw_execution_span_rounds_v1(span_daa, ttpb);
    let windowed = palw_execution_mint_quanta_windowed_v1(
        &[f],
        Hash64::from_u64_word(7),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        open_round,
        span_rounds,
        &std::collections::BTreeSet::new(),
    );
    let first = windowed.iter().map(|q| q.scheduled_round).min().expect("tickets");
    let last = windowed.iter().map(|q| q.scheduled_round).max().expect("tickets");
    println!("\n=== the fold's mint now (windowed, span of {span_rounds} rounds) ===");
    println!("  tickets                {}", windowed.len());
    println!("  scheduled              +{} .. +{} rounds", first - open_round, last - open_round);
    assert_eq!(windowed.len(), issued.len(), "the window takes every ticket of a Final this size");
    assert!(
        first > open_round && last < open_round + span_rounds,
        "every ticket is on a round of the span whose schedule lists it: +{}..+{} of {span_rounds}",
        first - open_round,
        last - open_round
    );
}
