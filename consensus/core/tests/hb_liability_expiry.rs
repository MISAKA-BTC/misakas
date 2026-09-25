//! **LANE H2 — which economic deadlines can a heartbeat-only history consume?**
//!
//! Every number below is read out of the shipped testnet-12 `Params` at this commit and out of the
//! production functions the fold itself calls. Nothing is a fixture.

use std::collections::BTreeMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_T12_BOND_MATURITY_WINDOW_DAA, Params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_exec_quantum_matures_at_v1, palw_exec_quantum_maturity_daa_v1,
    palw_realizable_before_maturity_v1, palw_rounds_per_daa_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_execution_lane_v1::palw_execution_round_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
use kaspa_consensus_core::palw_panel_var_v1::{
    PalwSlashableExposureLedgerV1, PalwSlashableLockV1, palw_panel_liability_expiry_v1,
};
use kaspa_consensus_core::palw_settlement_v1::palw_settlement_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwTransitionExtrasV1, apply_palw_transition_v7,
    palw_second_clock_depth_v1,
};
use kaspa_consensus_core::pow_layer0::{PALW_HEARTBEAT_MAX_PER_MERGESET, PALW_HEARTBEAT_WORK_LOG2};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn bundle(p: &Params) -> &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b,
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

/// **H2-1. A slash liability set at a false Final expires on the DAA clock alone, and the bond
/// becomes withdrawable — with zero PALW anchors under it.**
///
/// The two production predicates are `PalwSlashableLockV1::is_live(now_daa) = now_daa <
/// expiry_daa` (`palw_panel_var_v1.rs:127`) and `withdraw_allowed = live_locked == 0`
/// (`palw_panel_var_v1.rs:203`). The expiry is `palw_panel_liability_expiry_v1(final_daa,
/// window_court)` — `palw_state_v2.rs:9421` passes `self.params.window_court`.
#[test]
fn a_slash_liability_expires_on_the_heartbeat_clock() {
    let p = t12();
    let b = bundle(&p);
    let window_court = b.state.window_court();
    let cadence_ms = p.target_time_per_block();

    let bond = bond_key(1);
    let claim = h(0xFA15E);
    // The whole seat lock this network would demand of one Valid signature on the dearest class is
    // irrelevant to the timer; take a round 1,000 MSK to keep the arithmetic readable.
    let locked: u128 = 100_000_000_000;

    let final_daa = 12_345u64;
    let expiry = palw_panel_liability_expiry_v1(final_daa, window_court);

    let mut ledger = PalwSlashableExposureLedgerV1 { posted: BTreeMap::new(), locks: BTreeMap::new() };
    ledger.posted.insert(bond, locked);
    ledger.locks.insert(
        (bond, claim),
        PalwSlashableLockV1 {
            claim,
            amount: locked,
            expiry_daa: expiry,
            settled_at_final: 0,
            attested: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::NONE,
            segments: 0,
        },
    );

    assert!(!ledger.withdraw_allowed(&bond, final_daa), "the lie is fresh: the bond is pinned");
    assert!(!ledger.withdraw_allowed(&bond, expiry - 1), "one DAA short: still pinned");
    assert!(ledger.withdraw_allowed(&bond, expiry), "at expiry the liability is gone");

    // **THE FIX** (`Params::palw_settled_anchor_depth`, past `palw_audit_2026_09_23`): the lock is
    // live while EITHER clock still runs. The heartbeat history above settles no anchor, so the
    // second clock has not moved at all and the liability does not expire at its DAA expiry — it
    // waits for `depth` more anchors, which past the fence are attempt LICENCES, not `Final`s
    // (2026-09-24 DoS audit, fix #2: the DAA sweep reaches `Final` on heartbeats alone).
    let lock = *ledger.locks.get(&(bond, claim)).expect("the lock is in the ledger");
    let depth = kaspa_consensus_core::config::params::PALW_T12_SETTLED_ANCHOR_DEPTH;
    assert!(lock.is_live_v2(expiry, 0, Some(depth)), "armed: zero anchors settled, so the liability stands at its DAA expiry");
    assert!(
        lock.is_live_v2(expiry + 1_000_000, 0, Some(depth)),
        "armed, raw predicate: no amount of heartbeat-only history expires it — only settled anchors do"
    );
    // …but the fold and the processor never hand the raw depth past the liveness escape (fix #3,
    // `palw_second_clock_depth_v1`): once no anchor has settled for `2 × window_court` the second
    // clock is waived, so a stalled lane bounds the freeze instead of making it eternal. The last
    // anchor here settled no later than the lie's `Final`.
    let escaped = |now: u64| palw_second_clock_depth_v1(Some(depth), &[final_daa], now, window_court);
    assert!(
        lock.is_live_v2(final_daa + 2 * window_court - 1, 0, escaped(final_daa + 2 * window_court - 1)),
        "armed: bound up to E − 1"
    );
    assert!(
        !lock.is_live_v2(final_daa + 2 * window_court, 0, escaped(final_daa + 2 * window_court)),
        "armed: from E the DAA clock alone decides"
    );
    assert!(
        lock.is_live_v2(expiry - 1, depth, Some(depth)),
        "armed: `depth` anchors alone do not expire it either — the DAA window must also have run"
    );
    assert!(!lock.is_live_v2(expiry, depth, Some(depth)), "armed: BOTH clocks run out, and only then is the bond free");
    assert!(!lock.is_live_v2(expiry, 0, None), "dormant: the DAA-only rule, byte for byte — every network below the fence");

    // What it cost to get there. The clock is the heartbeat lane and nothing else on this network
    // (`palw_clock_advances_without_a_claim_v1`), one tick per recovery interval under ADR-0142's
    // cursor, at a fixed 2^work_log2 target.
    let ticks = window_court;
    let hashes: u128 = u128::from(ticks) * (1u128 << PALW_HEARTBEAT_WORK_LOG2);
    let wall_hours = (ticks * cadence_ms) as f64 / 3_600_000.0;

    println!("=== H2-1: slash liability expiry on a heartbeat-only history ===");
    println!("network                          = {}", p.net);
    println!("window_court (liability horizon) = {window_court} DAA");
    println!("Final at DAA                     = {final_daa}");
    println!("lock expiry_daa                  = {expiry}");
    println!("is_live predicate                = now_daa < expiry_daa   (palw_panel_var_v1.rs:127)");
    println!("withdraw_allowed at expiry-1     = {}", ledger.withdraw_allowed(&bond, expiry - 1));
    println!("withdraw_allowed at expiry       = {}", ledger.withdraw_allowed(&bond, expiry));
    println!("--- attacker cost to consume it ---");
    println!("heartbeats needed                = {ticks}  (1 DAA per beat, stand-in rule)");
    println!("bond required for a heartbeat    = 0        (bondless, claimless lane)");
    println!("PALW anchors (licences) made     = 0");
    println!("LLM inference performed          = 0");
    println!("hashes                           = {hashes} (= {ticks} x 2^{PALW_HEARTBEAT_WORK_LOG2})");
    println!("wall clock at {cadence_ms} ms/tick    = {wall_hours:.1} h");
    println!("max heartbeats per mergeset      = {PALW_HEARTBEAT_MAX_PER_MERGESET}");
}

/// **H2-2. The fold advances its clock through claimless blocks, and the settlement depth does not
/// move.** `palw_settlement_v1` is the ONE place in this tree that counts anchors instead of DAA;
/// nothing in the economy reads it.
#[test]
fn the_fold_clock_runs_while_the_settlement_depth_stays_zero() {
    let p = t12();
    let b = bundle(&p);
    let state_params = b.state.clone();
    let extras = PalwTransitionExtrasV1::default();

    let mut state = PalwChainStateV2::genesis();
    let start_daa = 1_000u64;
    let beats = 3_000u64; // window_court on this network
    for i in 0..beats {
        let daa = start_daa + i;
        let ctx = PalwBlockContextV2 { block: h(0xB000_0000 + i), daa_score: daa, blue_score: i + 1, subsidy: 0 };
        let (next, _, _) = apply_palw_transition_v7(
            &state,
            &state_params,
            None,
            &ctx,
            &[],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            &extras,
        )
        .expect("a claimless block is a valid transition");
        state = next;
    }

    let settled_at_start = palw_settlement_v1(&state, &state_params, None, start_daa);
    let sink = state.last_point().map(|pt| pt.daa_score).unwrap_or(0);

    println!("=== H2-2: {beats} claimless blocks through the real fold ===");
    println!("DAA advanced                     = {start_daa} -> {sink}  (+{})", sink - start_daa);
    println!("claims in state                  = {}", state.claims_iter().count());
    println!("settlement read at DAA {start_daa}      = {settled_at_start:?}");
    let depth = settled_at_start.map(|s| s.depth).unwrap_or(0);
    let settled = settled_at_start.map(|s| s.settled).unwrap_or(false);
    println!("palwSettlementDepth              = {depth}");
    println!("settled                          = {settled}");
    assert_eq!(sink - start_daa, beats - 1, "the clock ran");
    assert_eq!(depth, 0, "and no anchor was produced");
    assert!(!settled, "heartbeat-only history is never `settled`");
}

/// **H2-3. Every DAA-denominated economic deadline this network ships, and what a heartbeat-only
/// history does to each.** A table, printed, so the classification is a fact and not prose.
#[test]
fn the_deadline_table_is_all_one_clock() {
    let p = t12();
    let b = bundle(&p);
    let rows: Vec<(&str, u64, &str, &str)> = vec![
        ("bind timeout", b.state.window_bind(), "DAA", "LIVENESS — fine"),
        ("receipt window", b.state.window_receipt(), "DAA", "LIVENESS — fine"),
        ("challenge window", b.state.window_challenge(), "DAA", "ECONOMIC"),
        ("court window / slash liability", b.state.window_court(), "DAA", "ECONOMIC"),
        ("court turn deadline", b.state.turn_deadline_daa(), "DAA", "LIVENESS — fine"),
        ("claim retirement", b.state.claim_retirement_daa(), "DAA", "ECONOMIC (settlement depth decays)"),
        ("fp abandon hold", b.state.fp_abandon_hold_daa(), "DAA", "LIVENESS — fine"),
        ("bond withdrawal delay", b.bond.withdrawal_delay_daa(), "DAA", "ECONOMIC"),
        ("bond maturity (D1)", PALW_T12_BOND_MATURITY_WINDOW_DAA, "DAA", "ECONOMIC"),
        ("reorg margin", b.reorg_margin_daa, "DAA", "ECONOMIC"),
        ("receipt maturity", b.freeprompt.receipt_maturity_daa(), "DAA", "ECONOMIC"),
    ];
    println!("=== H2-3: the testnet-12 deadline table ===");
    println!("{:<34} {:>8}  {:<6} {}", "deadline", "value", "unit", "what the invariant demands");
    for (name, value, unit, demand) in &rows {
        println!("{name:<34} {value:>8}  {unit:<6} {demand}");
    }
    println!();
    println!("EVERY row above is consumed by `now_daa`, and on testnet-12 `now_daa` is the");
    println!("heartbeat lane: palw_clock_advances_without_a_claim_v1 (params.rs:14462) is true");
    println!("because no lane this network can produce is priced by bits.");
    for (name, value, _, _) in &rows {
        assert!(*value > 0 || *name == "reorg margin", "{name} is {value}");
    }
}

/// **H2-4. The one deadline that is NOT on the DAA clock is on the WALL clock, and its safety
/// argument assumes the two move together.**
///
/// `span_open_round` is `palw_execution_round_v1(header timestamp, genesis timestamp)` — wall-clock
/// seconds (`processor.rs:8283`, `palw_execution_lane_v1.rs:156`). The maturity handed to
/// `palw_execution_schedule_assign_quanta_matured_v1` is `maturity_daa x rounds_per_daa`
/// (`palw_state_v2.rs:10840`), i.e. a DAA window converted to seconds at a FIXED 120 s/DAA. The
/// liability horizon it is priced against (`window_court`) stays on the DAA clock. Slow the DAA
/// clock down — which any heartbeat miner does by simply not beating — and the rights mature while
/// the liability is still running, beyond what the seat lock was sized for.
#[test]
fn the_quantum_maturity_and_the_liability_horizon_read_different_clocks() {
    let p = t12();
    let b = bundle(&p);
    let window_challenge = b.state.window_challenge();
    let window_court = b.state.window_court();
    let cadence_ms = p.target_time_per_block();
    let rpd = palw_rounds_per_daa_v1(cadence_ms);

    // The held 2M row's mint, as palw_economic_safety_v1's own test records it.
    let quanta: u32 = 270_029;

    // **testnet-12's maturity is the challenge window it applies, 120 DAA** (user decision
    // 2026-09-25), not the lattice's 1,200 — which is what closes this finding: the gap grows to
    // 2,880 DAA = 345,600 rounds, more than the mint, so the price is the whole mint and no slower
    // clock can realize a right it did not price.
    assert_eq!(palw_exec_quantum_maturity_daa_v1(window_challenge, window_court), 1_200, "the None rule");
    let maturity_daa = p.palw_exec_quantum_maturity_v1();
    assert_eq!(maturity_daa, 120);
    let maturity_rounds = maturity_daa * rpd;
    let priced_gap_daa = window_court - maturity_daa;
    let priced_rounds = priced_gap_daa * rpd;
    let priced = palw_realizable_before_maturity_v1(quanta, maturity_daa, window_court, cadence_ms, PALW_T12_PERMIT_FEE_CEILING_SOMPI);

    println!("=== H2-4: two clocks under one inequality ===");
    println!("maturity (DAA)                   = {maturity_daa}");
    println!("maturity as the lane spends it   = {maturity_rounds} rounds = wall-clock SECONDS");
    println!("  (palw_execution_round_v1: round = (timestamp_ms - genesis_ms)/1000)");
    println!("liability horizon (DAA)          = {window_court}");
    println!("priced gap                       = {priced_gap_daa} DAA -> {priced_rounds} rounds");
    println!("quanta one held-2M Final mints   = {quanta}");
    println!("realizable priced into the lock  = {priced} sompi (= min({quanta},{priced_rounds}) x {PALW_T12_PERMIT_FEE_CEILING_SOMPI})");
    assert_eq!(priced, u128::from(quanta) * u128::from(PALW_T12_PERMIT_FEE_CEILING_SOMPI), "the mint binds, not the rounds");

    // Now let the DAA clock run slower than 120 s per tick. Nothing forbids it: the cursor sets a
    // MINIMUM interval between beats, never a maximum, and a heartbeat that is not mined is a DAA
    // that does not advance while wall-clock rounds keep accruing.
    println!("--- if the heartbeat clock runs slower than one beat per {cadence_ms} ms ---");
    println!("{:>12}  {:>16}  {:>12}  {:>20}", "ms per DAA", "rounds in the gap", "realizable", "under-priced by");
    let mut first_full = None;
    for slow_ms in [120_000u64, 150_000, 180_000, 240_000, 360_000, 3_600_000] {
        let rounds_in_gap = priced_gap_daa * (slow_ms / 1_000);
        let realizable = u128::from(quanta).min(u128::from(rounds_in_gap)) * u128::from(PALW_T12_PERMIT_FEE_CEILING_SOMPI);
        let short = realizable.saturating_sub(priced);
        if rounds_in_gap >= u64::from(quanta) && first_full.is_none() {
            first_full = Some(slow_ms);
        }
        println!("{slow_ms:>12}  {rounds_in_gap:>16}  {realizable:>12}  {short:>20}");
    }
    let full = first_full.expect("some slowdown makes every quantum realizable");
    println!("every minted quantum is realizable once the clock is at {full} ms/DAA");
    println!("  -- that is {:.2}x the cadence the pricing assumes", full as f64 / cadence_ms as f64);
    assert_eq!(full, cadence_ms, "at the 120-DAA maturity the cadence itself already realizes the whole mint");

    // And the seat lock the fold demands is computed from the PRICED figure — which is now the worst
    // case. (At the 1,200-DAA maturity it was 216,000 of 270,029: 20.0 % short.)
    let worst = u128::from(quanta) * u128::from(PALW_T12_PERMIT_FEE_CEILING_SOMPI);
    println!("worst-case realizable (all quanta) = {worst} sompi");
    println!("shortfall vs priced                = {} sompi", worst - priced);
    assert_eq!(worst, priced, "the pricing IS the worst case: a slower DAA clock realizes nothing unpriced");
    let at_lattice =
        palw_realizable_before_maturity_v1(quanta, window_challenge, window_court, cadence_ms, PALW_T12_PERMIT_FEE_CEILING_SOMPI);
    assert!(worst > at_lattice, "and at the unshortened 1,200 it was not");
    let _ = palw_seat_lock_required_v2(0, PALW_PANEL_COLLUDING_QUORUM_V1);
    let _ = palw_exec_quantum_matures_at_v1(0, window_challenge, window_court);
    let _ = palw_execution_round_v1(1, 0);
}

/// **H2-5. The quantum maturity outlives the schedule that holds it.**
///
/// A quantum is scheduled at a WALL-CLOCK round `open_round + maturity_rounds`. The schedule row
/// that holds it is dropped by `rotate_round_lane` once `span_now` has moved two spans on
/// (`palw_state_v2.rs:10906`: `keep_from = span_now - 1`, `round_schedules.range(..keep_from)`),
/// and a span is `daa_score / schedule_span_daa` — the DAA clock. So the maturity is measured on
/// one clock and its own storage lifetime on another.
#[test]
fn the_maturity_outlives_the_schedule_row_that_holds_it() {
    let p = t12();
    let lane = p.palw_execution_lane.expect("testnet-12 arms the execution lane at 0");
    let span_daa = lane.schedule_span_daa_at(0);
    let cadence_ms = p.target_time_per_block();
    let rpd = palw_rounds_per_daa_v1(cadence_ms);
    // testnet-12's maturity (user decision 2026-09-25): 120 DAA, the challenge window it applies.
    let maturity_daa = p.palw_exec_quantum_maturity_v1();
    let maturity_rounds = maturity_daa * rpd;

    // A schedule for span S is kept while span_now < S + 2 (keep_from = span_now - 1 drops
    // everything strictly below it), so its whole life is two spans of DAA.
    let schedule_life_daa = span_daa * 2;
    let schedule_life_rounds = schedule_life_daa * rpd;

    println!("=== H2-5: maturity vs the schedule's own lifetime ===");
    println!("schedule_span_daa at DAA 0       = {span_daa} DAA");
    println!("schedule row lifetime            = {schedule_life_daa} DAA = {schedule_life_rounds} rounds (at {cadence_ms} ms/DAA)");
    println!("quantum maturity                 = {maturity_daa} DAA = {maturity_rounds} rounds");
    println!("ratio maturity / lifetime        = {:.0}x", maturity_rounds as f64 / schedule_life_rounds as f64);
    println!(
        "=> the earliest round any quantum is scheduled for is {} rounds past the point its\n   schedule row is deleted.",
        maturity_rounds.saturating_sub(schedule_life_rounds)
    );
    assert!(maturity_rounds > schedule_life_rounds, "if this ever inverts, the finding is closed");
}

/// **H2-6. The lane the maturity freezes is armed on this network, and the clock that consumes
/// every other deadline is the heartbeat.** Both premises, asserted rather than assumed.
#[test]
fn the_premises_hold_on_this_network() {
    use kaspa_consensus_core::config::params::palw_clock_advances_without_a_claim_v1;
    let p = t12();
    let lane = p.palw_execution_lane.expect("execution lane");
    println!("=== H2-6: premises ===");
    println!("palw_clock_advances_without_a_claim_v1 = {}", palw_clock_advances_without_a_claim_v1(&p));
    println!("heartbeat activation active at DAA 0   = {}", p.palw_heartbeat.expect("hb").activation.is_active(0));
    println!("execution lane active at DAA 0         = {}", lane.activation.is_active(0));
    println!("short span active at DAA 0             = {}", lane.short_span.activation.is_active(0));
    println!("execution quanta fence at DAA 0        = {:?}", p.palw_execution_quanta.map(|f| f.is_active(0)));
    println!("economic safety fence at DAA 0         = {:?}", p.palw_economic_safety.map(|f| f.is_active(0)));
    println!("bond maturity fence active at DAA 0    = {:?}", p.palw_bond_maturity.map(|m| m.activation.is_active(0)));
    println!("bond maturity fence active at DAA 1000 = {:?}", p.palw_bond_maturity.map(|m| m.activation.is_active(1_000)));
    println!("lane.schedule_span_daa (raw)           = {}", lane.schedule_span_daa);
    println!("lane.short_span                        = {:?}", lane.short_span);
    println!("schedule_span_daa_at(0)                = {}", lane.schedule_span_daa_at(0));
    assert!(palw_clock_advances_without_a_claim_v1(&p), "the heartbeat IS the clock on testnet-12");
    assert!(lane.activation.is_active(0));
}

/// **H2-7. The whole exit of a lying seat, end to end, on heartbeats only.**
///
/// `BondRetireRequested` is refused while any slashable lock is live
/// (`palw_state_v2.rs:15501`), the lock is live while `now_daa < expiry_daa`
/// (`palw_panel_var_v1.rs:127`), and once `Retiring { since_daa }` the collateral stays locked for
/// `withdrawal_delay_daa` (`palw_state_v2.rs:1882`). Three DAA-denominated gates in series, and
/// nothing between them looks at what the chain did with that time.
#[test]
fn the_whole_exit_is_one_clock() {
    let p = t12();
    let b = bundle(&p);
    let window_court = b.state.window_court();
    let delay = b.bond.withdrawal_delay_daa();
    let cadence_ms = p.target_time_per_block();

    let final_daa = 50_000u64;
    let lock_expiry = palw_panel_liability_expiry_v1(final_daa, window_court);
    let retire_at = lock_expiry; // the first DAA at which the retire request is not refused
    let collateral_free_at = retire_at + delay;
    let total = collateral_free_at - final_daa;

    println!("=== H2-7: the exit of a seat that signed a false Final ===");
    println!("Final (the lie) at DAA           = {final_daa}");
    println!("slashable lock live until        = {lock_expiry}   (+{window_court} window_court)");
    println!("BondRetireRequested accepted at  = {retire_at}");
    println!("Retiring -> collateral free at   = {collateral_free_at}   (+{delay} withdrawal_delay)");
    println!("total DAA the attacker must burn = {total}");
    println!("= {:.0} h of wall clock at {cadence_ms} ms/DAA", (total * cadence_ms) as f64 / 3_600_000.0);
    println!("PALW anchors required            = 0");
    println!("bond required to mint the clock  = 0");
    println!("hashes                           = {} (= {total} x 2^{PALW_HEARTBEAT_WORK_LOG2})", u128::from(total) * (1u128 << PALW_HEARTBEAT_WORK_LOG2));
    assert_eq!(total, window_court + delay);
}
