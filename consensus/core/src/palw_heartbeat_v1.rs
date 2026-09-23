//! **The heartbeat lane** — ADR-0060 Decisions 1 and 2 as ADR-0066 redesigned them: time is
//! permissionless, and the price of time never touches `header.bits`.
//!
//! A `ConsensusV2` network whose `palw_heartbeat` fence is in force accepts, beside its two bonded
//! PALW lanes, a **bondless, claimless, near-weightless clock** on its own algorithm id
//! ([`crate::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1`]). A heartbeat block advances the DAA (so every
//! PALW timeout — bind, receipt, challenge, court, withdrawal — sweeps on a clock no bond can
//! stop), carries ordinary transactions (so bond registration and funding can ride it when no
//! bonded lane is alive), and contributes a fixed [`HEARTBEAT_BLUE_WORK_EPSILON`] to fork choice.
//!
//! ## The two rules, and where each one lives
//!
//! * **The price is a network constant** ([`crate::pow_layer0::PALW_HEARTBEAT_WORK_LOG2`]),
//!   substituted for `header.bits` inside `StateLayer0::new`. A heartbeat header's `bits` are the
//!   GLOBAL expected bits, like every other lane's, so heartbeat rows enter the difficulty window
//!   as ordinary rows.
//! * **The slot rule** ([`check_heartbeat_slot`]) is one block deep: a heartbeat must sit at least
//!   one interval after its SELECTED PARENT's timestamp. No walk, no window, no ancestor evidence.
//!
//! ## Why both of those are different from the first implementation
//!
//! The 2026-08-30 audit recorded four structural findings and ADR-0066 sorted them by mechanism.
//! Two were `bits`:
//!
//! 1. **The lane could price the bonded lane off its own chain, permanently.** Heartbeat headers
//!    carried the lane's own 2²⁴-hard `bits`, and those rows sat in the GLOBAL difficulty window.
//!    A V2 network's ambient target is `MAX_DIFFICULTY_TARGET` — work 2 — because the class
//!    lottery, not the hash target, is its throttle. Measured over the shipped 264-row window:
//!    255 bonded + 9 heartbeat rows still demanded work 2, but **0 bonded + 263 heartbeat rows
//!    demanded 33,554,432**. After a bonded outage longer than the window a returning producer
//!    needed ~33 M inferences for one block, so no bonded block could re-enter the window, so the
//!    average never re-mixed: a heartbeat-only chain recoverable only by re-mint, which is the
//!    self-feeding refusal ADR-0060 exists to abolish, reintroduced by its own remedy.
//! 3b. **The retarget could never rise above its floor**, because the slot rule guaranteed
//!    `measured ≥ expected` and the clamp turned that back into the floor.
//!
//! A fixed target removes the quantity that fed back on itself, so 1 and 3b are gone as arithmetic
//! rather than as tuning. The retarget is deleted, not bounded.
//!
//! One was node-local:
//!
//! 4. **The evidence walk terminated on `Err(get_header) => break`** — a fact about THIS node.
//!    An archival node never hit it; a pruned node hit it at its own pruning point. Two honest
//!    nodes computed different verdicts for one header: a partition along the `--archival` flag.
//!    A retarget is what needed ancestor evidence, and there is no retarget, so the walk is gone
//!    and the slot rule reads the selected parent alone.
//!
//! And one is **still open, recorded rather than closed**:
//!
//! 3a. **Sibling width.** The slot rule bounds the chain, not the DAG: sibling heartbeats share
//!    one selected parent, so they share one admissible timestamp, and nothing here bounds how
//!    many of them exist. What bounds width is the price, which is now a fixed 2²⁴ per block
//!    rather than a floor a retarget could never leave. ADR-0066 records 3a as open.
//!
//! 2. **ε against a V2 block's work** is independent of all of the above and survives untouched —
//!    on a V2 preset `calc_work(0x207fffff) = 2`, so a heartbeat is worth half a bonded block.
//!    ADR-0066 Decision 3 is the fix (a V2 attempt block's blue work should reflect the inference
//!    it carries, not the hash target it did not need); it moves `header.blue_work` on every V2
//!    block and is deliberately staged after this.
//!
//! ## The ramp, and why it has two steps instead of three
//!
//! The interval is a step function of the SELECTED PARENT's lane: after a bonded block the chain
//! was alive one block ago, so the lane waits the full nominal hour; after a heartbeat the chain
//! is already running on the clock, so the lane runs at the recovery cadence.
//!
//! The old middle step ("above one hour of bonded silence, one per ten minutes") is gone because
//! it asked *how long has the bonded lane been silent*, and that is ancestor evidence — finding 4
//! in one question. One block deep admits exactly two states, and they are the two that matter:
//! the chain is producing, or it is not.
//!
//! ## What is deliberately NOT here
//!
//! No bond, no claim, no escrow, no court: a hash proof is self-verifying, so there is nothing to
//! slash and nobody to license. The coinbase rule (a heartbeat block's declared subsidy is zero —
//! fees only) lives with the other coinbase validation in the body processor; the ε fork-choice
//! rule lives in the GHOSTDAG protocol beside the receipt lane's zero. Both cite this module.
//!
//! ## The trap the recovery cadence set for a slow producer (ADR-0105)
//!
//! The two-step ramp assumes a bonded draw takes seconds against a 120 s block. On testnet-11 a
//! Qwen3.6 draw takes ~17 minutes, and a bonded block's timestamp is its TEMPLATE's. Once a
//! heartbeat is the selected parent (a bonded outage longer than the nominal hour is enough), eight
//! heartbeats land during every draw; the draw lands built on a parent eight ε behind the tip, so
//! it is never selected, and at `ghostdag_k = 1` that eight-heartbeat anticone made it RED — its
//! 2²⁰ never entered anyone's blue work, and the DNS anchor could not be buried. The mode sustained
//! itself for as long as any heartbeat miner ran (2026-09-10, 11:41Z-13:20Z).
//!
//! The consensus half of the answer is in the GHOSTDAG protocol behind
//! `Params::palw_heartbeat_transparent` (a heartbeat never counts against a bonded block's
//! coloring). The node half is here: [`heartbeat_yield_hint_v1`] tells a heartbeat miner that a
//! bonded block is waiting to be merged, so it can stand aside long enough for the next bonded
//! block to take the chain back. That half is **advice, not a rule** — nothing validates against
//! it, and the slot rule above is unchanged.

use crate::pow_layer0::{POW_ALGO_ID_HEARTBEAT_V1, is_palw_attempt_algo_id, is_palw_v2_algo_id};

/// The heartbeat lane's algorithm id. See [`POW_ALGO_ID_HEARTBEAT_V1`] for why it is its own id
/// and no longer `POW_ALGO_ID_BLAKE2B_SHA3`.
pub const PALW_HEARTBEAT_ALGO_ID: u8 = POW_ALGO_ID_HEARTBEAT_V1;

/// Nominal cadence: one heartbeat per hour (≈ 24/day ≈ 33‰ of the 120 s cadence) — the interval
/// that applies when the selected parent is a bonded block, i.e. the chain is producing.
pub const HEARTBEAT_NOMINAL_INTERVAL_MS: u64 = 3_600_000;

/// The recovery cadence: the full 120 s block time, applied when the selected parent is itself a
/// heartbeat — timeout sweeping at normal speed with every bonded lane dead.
pub const HEARTBEAT_RECOVERY_INTERVAL_MS: u64 = 120_000;

/// **ε: the whole fork-choice weight of a heartbeat block** — the named exception to ADR-0045's
/// DerivedV1 work equality that ADR-0060 Decision 1.2 is. One unit: any bonded PALW block
/// (≈ 10⁶ work) outweighs a million heartbeats, while among heartbeat-only branches (total
/// collapse) `ε × n` still orders the longer chain first — which zero (the receipt lane's figure)
/// would not.
///
/// **This value is known to be too large against a V2 block and is not the fix.** See finding 2 in
/// the module header: ADR-0066 Decision 3 moves the OTHER side of the comparison.
pub const HEARTBEAT_BLUE_WORK_EPSILON: u64 = 1;

/// The interval a heartbeat is held to, given the lane of its selected parent.
///
/// One block deep by construction — the argument is the parent's algo id and nothing else, so
/// there is no walk to bound, no window to sample and no node-local fact to terminate on.
pub fn heartbeat_interval_ms(selected_parent_algo_id: u8) -> u64 {
    if is_palw_v2_algo_id(selected_parent_algo_id) {
        // A bonded block one block ago: the chain is producing and the lane stays out of the way.
        HEARTBEAT_NOMINAL_INTERVAL_MS
    } else {
        // The parent is a heartbeat (or anything else this network admits): the chain is running
        // on the clock, so the clock runs at cadence.
        HEARTBEAT_RECOVERY_INTERVAL_MS
    }
}

/// **ADR-0138 §3c: past the anchor clock the interval follows the CLOCK, not the bond.**
///
/// `heartbeat_interval_ms` backs off for an hour whenever the selected parent is a bonded PALW-v2
/// block, on the reading that "a bonded block one block ago" means the chain is producing and the
/// lane should stay out of the way. Past `palw_anchor_clock` that reading is wrong, because an
/// attempt block produces without advancing the DAA at all. On a network with no `bits`-priced
/// producer — which is every ConsensusV2 network, since `PalwRulesetV2::validate` forces the
/// template to declare the attempt id — the old rule leaves the chain beating once an hour and the
/// DAA score moving once an hour with it, or not at all across a stretch that carries no beat.
/// testnet-11 measured exactly that: 60 of its last 60 selected-chain blocks are algo 6.
///
/// So the question becomes the one that was always meant: **is someone else pacing the clock?**
/// If the parent advances the DAA, the lane stays out of the way for the nominal hour. If it does
/// not, the lane runs at the recovery cadence and IS the clock — which, with the per-mergeset
/// stand-in in `daa_exempt_count`, makes the DAA advance at the target block time on a chain that
/// has no priced lane, and changes nothing on a chain that has one.
///
/// Below the fence the caller passes `anchor_clock_active = false` and the answer is byte-identical
/// to `heartbeat_interval_ms`, so no history moves.
pub fn heartbeat_interval_ms_v2(selected_parent_algo_id: u8, anchor_clock_active: bool, parent_advances_daa: bool) -> u64 {
    if heartbeat_parent_paces_the_clock_v1(selected_parent_algo_id, anchor_clock_active, parent_advances_daa) {
        HEARTBEAT_NOMINAL_INTERVAL_MS
    } else {
        HEARTBEAT_RECOVERY_INTERVAL_MS
    }
}

/// Is the selected parent pacing the chain's clock? Below `palw_anchor_clock` a bonded PALW-v2
/// parent is (every lane advanced the DAA then); past it, only a parent that advances the DAA is.
/// One block deep, a pure function of its arguments, and the single place the two regimes differ.
///
/// The below-fence reading is the 2026-09-21 stall: a BASE-0 win is a bonded parent, so the lane
/// waited the nominal hour, and the lottery did not produce a next block either. Past the fence
/// an attempt that advances no DAA is not pacing, and the recovery cadence is the clock.
#[inline]
pub fn heartbeat_parent_paces_the_clock_v1(selected_parent_algo_id: u8, anchor_clock_active: bool, parent_advances_daa: bool) -> bool {
    if anchor_clock_active { parent_advances_daa } else { is_palw_v2_algo_id(selected_parent_algo_id) }
}

/// [`check_heartbeat_slot`] under the ADR-0138 rule. The caller answers the two fence questions
/// because the lane predicate lives with the DAA arithmetic, in `kaspa-consensus`.
pub fn check_heartbeat_slot_v2(
    selected_parent_timestamp: u64,
    selected_parent_algo_id: u8,
    anchor_clock_active: bool,
    parent_advances_daa: bool,
    header_timestamp: u64,
) -> Result<(), HeartbeatTooEarly> {
    let interval_ms = heartbeat_interval_ms_v2(selected_parent_algo_id, anchor_clock_active, parent_advances_daa);
    match selected_parent_timestamp.checked_add(interval_ms) {
        Some(earliest) if header_timestamp >= earliest => Ok(()),
        _ => Err(HeartbeatTooEarly { last_heartbeat_timestamp: selected_parent_timestamp, interval_ms }),
    }
}

/// Why a heartbeat header was refused by the slot rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartbeatTooEarly {
    pub last_heartbeat_timestamp: u64,
    pub interval_ms: u64,
}

/// **The slot rule**: a heartbeat header must sit at least one [`heartbeat_interval_ms`] after its
/// SELECTED PARENT's timestamp.
///
/// The old rule measured against the youngest heartbeat in the POV's DAA window, which needed a
/// chain-order walk — and that walk terminated on a node-local fact (finding 4). This asks one
/// question of one header the caller already holds.
///
/// It bounds the CHAIN, not the DAG: siblings share a selected parent and therefore share one
/// admissible timestamp. That is finding 3a and it is open; the fixed price is what bounds width.
pub fn check_heartbeat_slot(
    selected_parent_timestamp: u64,
    selected_parent_algo_id: u8,
    header_timestamp: u64,
) -> Result<(), HeartbeatTooEarly> {
    let interval_ms = heartbeat_interval_ms(selected_parent_algo_id);
    // **`checked_add`, not `saturating_add`.** Saturating clamps the earliest admissible time DOWN
    // to `u64::MAX`, so a parent near the top of the range would admit a header at zero distance —
    // the arithmetic failing OPEN, on the one rule whose whole job is to refuse. Overflow here is
    // not reachable through the timestamp rules, which is exactly the reasoning that leaves a
    // fail-open path in place; the closed direction costs nothing.
    match selected_parent_timestamp.checked_add(interval_ms) {
        Some(earliest) if header_timestamp >= earliest => Ok(()),
        _ => Err(HeartbeatTooEarly { last_heartbeat_timestamp: selected_parent_timestamp, interval_ms }),
    }
}

/// **ADR-0105 Decision 2 — what a heartbeat miner is told about the bonded lane.** Node policy,
/// not a rule: no validation path reads it.
///
/// Three answers, because a miner has to act on three different facts and one `Option` would
/// fold two of them into `None` (the "one None for two answers" shape — the miner's yield budget
/// must reset on the first and must not reset on the second):
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartbeatYieldHintV1 {
    /// The virtual's selected parent is a bonded block. The chain is producing, the slot rule
    /// already keeps the lane out of the way for the nominal hour, and a heartbeat-led episode —
    /// if there was one — is over.
    BondedSelectedParent,
    /// The chain runs on heartbeats and no attempt-lane block waits in the virtual's mergeset.
    /// This is the regime the lane exists for: tick at the recovery cadence.
    NothingToYieldTo,
    /// The chain runs on heartbeats and an attempt-lane block is waiting in the virtual's mergeset,
    /// merged by nothing yet. A heartbeat mined now stacks ε on the parent that block's producer
    /// drew on, and every one of those is weight the next bonded draw has to beat. The value is
    /// the latest `timestamp + HEARTBEAT_NOMINAL_INTERVAL_MS` over those blocks — the same hour the
    /// slot rule would have granted each of them had it been selected.
    YieldUntil(u64),
    /// **Past the clock cursor: the slot is taken, and the next one opens at this timestamp**
    /// (the 2026-09-24 heartbeat audit, H1).
    ///
    /// The clock has already advanced into the slot a beat minted now would claim, and no beat in
    /// the virtual's mergeset is waiting for a step, so the beat could earn nothing: it would be
    /// merged, weigh ε, add a blue score, and tick no DAA. Measured on testnet-12 before this
    /// existed: 74% of blocks were heartbeats and 11% of beats got a tick. A miner told this waits
    /// until the timestamp, then asks again.
    SlotTaken(u64),
}

/// The hint, from facts the virtual state already holds: the selected parent's lane and, for every
/// OTHER block in the virtual's mergeset, its lane and timestamp.
///
/// One block deep, like the slot rule: no walk, and nothing node-local beyond the virtual itself
/// (a hint may be node-local; the point is that it needs nothing more). Only the ATTEMPT lanes
/// count as something to yield to — those are the blocks that carry weight and that pay the class
/// lottery to exist. A receipt-lane block carries no weight a heartbeat could bury.
pub fn heartbeat_yield_hint_v1(selected_parent_algo_id: u8, merged: impl IntoIterator<Item = (u8, u64)>) -> HeartbeatYieldHintV1 {
    heartbeat_yield_hint_v2(selected_parent_algo_id, false, true, true, merged)
}

/// The hint under the ADR-0138 rule. A miner that kept the v1 hint past the fence would answer
/// `BondedSelectedParent` to every attempt-lane parent and sleep the nominal hour — on a chain
/// where nothing else advances the clock, so it would sleep through the very regime the lane
/// exists for. And the wait `YieldUntil` names must be the interval the SLOT RULE would grant,
/// or the miner waits an hour for a slot it could have taken in two minutes.
pub fn heartbeat_yield_hint_v2(
    selected_parent_algo_id: u8,
    anchor_clock_active: bool,
    parent_advances_daa: bool,
    attempt_advances_daa: bool,
    merged: impl IntoIterator<Item = (u8, u64)>,
) -> HeartbeatYieldHintV1 {
    if heartbeat_parent_paces_the_clock_v1(selected_parent_algo_id, anchor_clock_active, parent_advances_daa) {
        return HeartbeatYieldHintV1::BondedSelectedParent;
    }
    // **Yield only to a block that PACES THE CLOCK** — the same question the parent arm just asked,
    // asked of the mergeset.
    //
    // The yield exists so a beat does not bury an attempt block that is waiting to be merged. Past
    // the anchor clock an attempt block carries its weight but advances no DAA, so standing aside
    // for one waits on a block that will not move the clock, on a chain where nothing else will
    // either. Worse, the economic lane produces continuously, so the yield target keeps moving
    // forward and the lane stands aside from the very job it exists for until its episode budget
    // drains — measured on the drill as a frozen DAA with the miner never minting.
    //
    // Below the fence `anchor_clock_active` is false and this returns to the ADR-0105 behaviour
    // unchanged. This is miner POLICY: no validation path reads it, and no fingerprint moves.
    if anchor_clock_active && !attempt_advances_daa {
        return HeartbeatYieldHintV1::NothingToYieldTo;
    }
    merged
        .into_iter()
        .filter(|&(algo_id, _)| is_palw_attempt_algo_id(algo_id))
        // The wait is the interval the slot rule would have granted THAT block had it been
        // selected — so it is computed from that block's lane, not the parent's, and past the
        // anchor clock an attempt block that advances no DAA earns the recovery interval rather
        // than the nominal hour. Yielding the hour there would stall the clock the lane now keeps.
        //
        // Saturating is the right direction HERE and not in the slot rule: this is a wait a miner
        // chooses, bounded by its own per-episode budget, not a refusal consensus relies on.
        .map(|(algo_id, timestamp)| {
            timestamp.saturating_add(heartbeat_interval_ms_v2(algo_id, anchor_clock_active, attempt_advances_daa))
        })
        .max()
        .map_or(HeartbeatYieldHintV1::NothingToYieldTo, HeartbeatYieldHintV1::YieldUntil)
}

/// **H3: how many heartbeats a CHAIN spanning `span_ms` of timestamps may put in one mergeset** past
/// `palw_clock_floor` — F5's chain exemption, priced again.
///
/// The exemption (`check_mergeset_heartbeat_width`) admits any number of beats over the flat bound
/// provided they form one chain, on the reasoning that a chain's length "is already priced by the
/// slot ladder". Past the cursor the ladder retired and nothing priced it: a chain of beats each
/// hanging off a heavier attempt block at the same score (every one stamped at or past the same
/// cursor, every one valid on its own) could be minted in a burst and merged whole, a blue score
/// apiece.
///
/// An honest chain holds at most TWO beats a slot — the one stamped into the open slot and the one
/// that merges it and steps the clock — and past the floor consecutive slots' references are at
/// least one interval apart (H5), with every beat stamped at or past its slot (H3). So `n` honest
/// members span at least `(⌈n/2⌉ − 1)` intervals less the clock skew between two honest miners, and
/// `flat_bound + 2·⌈span / interval⌉` covers them while that skew is under two intervals (240 s —
/// the future-drift tolerance alone keeps it under 132 s). A burst spanning nothing gets the flat
/// bound and no more.
pub fn heartbeat_chain_capacity_v1(span_ms: u64, flat_bound: u64) -> u64 {
    flat_bound.saturating_add(span_ms.div_ceil(HEARTBEAT_RECOVERY_INTERVAL_MS).saturating_mul(2))
}

/// **H1: the yield hint with the clock's own answer on top** — node policy, like the hint it wraps.
///
/// `clock` is the virtual's [`crate::palw_clock_cursor_v1::PalwClockStepV1`] where the cursor governs
/// (`None` elsewhere, and on any read failure — the hint must never hold the clock on a node-local
/// fact). The answer is [`HeartbeatYieldHintV1::SlotTaken`] exactly when a reference is known, the
/// next slot has not opened (`next > now`), and no beat in the virtual's mergeset already holds the
/// open slot (`!granted`).
///
/// **The `granted` case is deliberately NOT `SlotTaken`.** A granted beat waiting in the virtual means
/// the next block built STEPS the clock — and on a chain whose only producer is this lane, nobody
/// else will build it. The stepping block's timestamp is the next slot's reference, so every second
/// it waits is a second added to every tick. The miner mines it at once; the header rules put its
/// timestamp at or past the slot, so it cannot open the next one early.
///
/// A bonded selected parent that paces the clock keeps its own answer, because the ADR-0105 budget
/// refills on it.
pub fn heartbeat_slot_hint_v1(
    yield_hint: HeartbeatYieldHintV1,
    clock: Option<&crate::palw_clock_cursor_v1::PalwClockStepV1>,
    now_ms: u64,
) -> HeartbeatYieldHintV1 {
    if yield_hint == HeartbeatYieldHintV1::BondedSelectedParent {
        return yield_hint;
    }
    match clock.filter(|step| !step.granted).and_then(|step| step.next_slot_ms()) {
        Some(next) if next > now_ms => HeartbeatYieldHintV1::SlotTaken(next),
        _ => yield_hint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pow_layer0::{PALW_HEARTBEAT_WORK_LOG2, POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_RECEIPT_V3};

    /// **H3: F5's chain exemption, priced by the chain's own timestamps.** An honest chain — two
    /// beats a slot, slots an interval apart — always fits, with up to two intervals of clock skew
    /// between its miners; a burst stamped for one slot gets the flat bound and no more.
    #[test]
    fn an_honest_heartbeat_chain_fits_its_capacity_and_a_burst_does_not() {
        use crate::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET as BOUND;
        const I: u64 = HEARTBEAT_RECOVERY_INTERVAL_MS;
        // A burst: any number of beats inside one interval gets the flat bound plus one slot's two.
        assert_eq!(heartbeat_chain_capacity_v1(0, BOUND), BOUND);
        assert_eq!(heartbeat_chain_capacity_v1(1, BOUND), BOUND + 2);
        assert_eq!(heartbeat_chain_capacity_v1(I, BOUND), BOUND + 2);
        assert_eq!(heartbeat_chain_capacity_v1(I + 1, BOUND), BOUND + 4);
        // Honest chains: `slots` slots, two beats each (holder at the slot, step `lag` later), the
        // next slot one interval after the step; the oldest member stamped up to `skew` late.
        for slots in 1..=200u64 {
            for lag in [0u64, 1, 1_000, 30_000, 119_999] {
                for skew in [0u64, 1, 60_000, 132_000, 2 * I - 1] {
                    let mut stamps = Vec::new();
                    let mut slot = 1_000_000u64;
                    for s in 0..slots {
                        let holder = if s == 0 { slot + skew } else { slot };
                        let step = slot + lag;
                        stamps.push(holder);
                        stamps.push(step);
                        slot = step + I;
                    }
                    let span = stamps.iter().max().unwrap() - stamps.iter().min().unwrap();
                    let count = stamps.len() as u64;
                    assert!(
                        count <= heartbeat_chain_capacity_v1(span, BOUND),
                        "{slots} slots (lag {lag}, skew {skew}): {count} beats over {span} ms exceed {}",
                        heartbeat_chain_capacity_v1(span, BOUND)
                    );
                }
            }
        }
        // And the burst the rule exists for: eight beats hung off eight heavier blocks at one score,
        // all stamped for the same slot within seconds of each other.
        assert!(8 > heartbeat_chain_capacity_v1(7_000, BOUND));
        // Saturation, not overflow.
        assert_eq!(heartbeat_chain_capacity_v1(u64::MAX, u64::MAX), u64::MAX);
    }

    /// **H1, the pure half: a beat is not worth mining while the slot is taken, and is the moment a
    /// beat is waiting to be stepped over.**
    ///
    /// Before this the hint had no clock in it at all: past the cursor it answered
    /// `NothingToYieldTo` between two slots, and the miner ground a beat that could earn nothing —
    /// on testnet-12, nine beats in ten.
    #[test]
    fn a_taken_slot_holds_the_miner_until_the_next_one_opens() {
        use crate::palw_clock_cursor_v1::{PalwClockCursorV1, PalwClockStepV1};
        let cursor = |next| Some(PalwClockCursorV1 { next_slot_ms: next, slots_consumed: 0 });
        let idle = PalwClockStepV1 { governs: true, cursor: cursor(10_000), granted: false, floor: false };
        let nothing = HeartbeatYieldHintV1::NothingToYieldTo;
        // Between two slots: taken, and the answer names when the next one opens.
        assert_eq!(heartbeat_slot_hint_v1(nothing, Some(&idle), 9_999), HeartbeatYieldHintV1::SlotTaken(10_000));
        // At the boundary and after it the slot is open: mine.
        assert_eq!(heartbeat_slot_hint_v1(nothing, Some(&idle), 10_000), nothing);
        assert_eq!(heartbeat_slot_hint_v1(nothing, Some(&idle), 50_000), nothing);
        // It outranks a yield to a waiting attempt block: there is no slot to yield.
        assert_eq!(
            heartbeat_slot_hint_v1(HeartbeatYieldHintV1::YieldUntil(99_999), Some(&idle), 1),
            HeartbeatYieldHintV1::SlotTaken(10_000)
        );
        // **A granted beat waiting in the virtual is not "taken"**: the next block steps the clock,
        // and on a heartbeat-only chain this lane is the only producer that will build it.
        let pending = PalwClockStepV1 { granted: true, ..idle };
        assert_eq!(heartbeat_slot_hint_v1(nothing, Some(&pending), 1), nothing);
        // No cursor governs, or none is known: the hint is what it was.
        let ungoverned = PalwClockStepV1 { governs: false, ..idle };
        assert_eq!(heartbeat_slot_hint_v1(nothing, Some(&ungoverned), 1), nothing);
        let unknown = PalwClockStepV1 { cursor: None, ..idle };
        assert_eq!(heartbeat_slot_hint_v1(nothing, Some(&unknown), 1), nothing);
        assert_eq!(heartbeat_slot_hint_v1(nothing, None, 1), nothing);
        // A bonded selected parent keeps its answer — the ADR-0105 budget refills on it.
        assert_eq!(
            heartbeat_slot_hint_v1(HeartbeatYieldHintV1::BondedSelectedParent, Some(&idle), 1),
            HeartbeatYieldHintV1::BondedSelectedParent
        );
    }

    /// **The ramp has exactly two steps, and which one applies is a question about ONE header.**
    ///
    /// The old ladder had three, keyed on "how long has the bonded lane been silent" — which is
    /// ancestor evidence, and the walk that answered it terminated on a node-local fact (finding
    /// 4: an archival node never hit `Err(get_header)`, a pruned node hit it at its own pruning
    /// point, and the two computed different verdicts for one header). One block deep admits two
    /// states because that is how many a single parent can distinguish.
    /// **Construction and validation must read ONE answer, and this is the guard that says so.**
    ///
    /// The slot rule is asked twice: once by `pre_pow_validation` to admit a header, and once by
    /// `heartbeat_adapt_block_template` to stamp one. ADR-0066 Decision 2 already required the two
    /// to agree, and ADR-0138 §3c broke it for a while by changing the interval on the validating
    /// side only — a node would then have stamped the nominal hour onto a template its own
    /// validator granted the recovery cadence, putting the timestamp an hour into the future where
    /// the drift rule refuses it outright. The lane simply stops, on a chain that has nothing else
    /// to advance the clock.
    ///
    /// So: outside this module, nothing may call the v1 entry points. A new call site is how the
    /// two sides drift apart, and a grep is the only thing that sees it before a network does.
    #[test]
    fn only_this_module_may_ask_the_pre_fence_slot_rule() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let mut offenders = Vec::new();
        let mut walked = 0usize;
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() {
                    // Build outputs and vendored sources are not this workspace's call sites.
                    if !matches!(name.as_str(), "target" | ".git" | "node_modules" | "vendor") {
                        stack.push(path);
                    }
                    continue;
                }
                if !name.ends_with(".rs") || path.ends_with("palw_heartbeat_v1.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                walked += 1;
                for (n, line) in text.lines().enumerate() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("//") || trimmed.starts_with("///") {
                        continue;
                    }
                    for call in ["check_heartbeat_slot(", "heartbeat_interval_ms(", "heartbeat_yield_hint_v1("] {
                        if line.contains(call) {
                            offenders.push(format!("{}:{}: {}", path.display(), n + 1, trimmed.trim_end()));
                        }
                    }
                }
            }
        }
        assert!(walked > 200, "the walk found only {walked} source files — it is not looking at the workspace");
        assert!(
            offenders.is_empty(),
            "the pre-fence slot rule is asked outside its module, so construction and validation can disagree \
             (ADR-0138 §3c): call the `_v2` form with the fence answers instead.\n  {}",
            offenders.join("\n  ")
        );
    }

    /// **ADR-0138 §3c.** Below the fence the v2 rule is the v1 rule, byte for byte, so no history
    /// moves. Past it the question changes from "is the parent bonded" to "does the parent advance
    /// the clock", which is the only thing the hour of back-off was ever for.
    #[test]
    fn past_the_anchor_clock_the_interval_follows_the_clock_and_not_the_bond() {
        let attempt = POW_ALGO_ID_PALW_COMMITTED_V2;
        let receipt = POW_ALGO_ID_PALW_RECEIPT_V3;
        let beat = PALW_HEARTBEAT_ALGO_ID;
        let anchor = crate::pow_layer0::POW_ALGO_ID_BLAKE2B_SHA3;

        // Below the fence: identical to v1 for every lane, whatever the second argument says.
        for &lane in &[attempt, receipt, beat, anchor] {
            for &advances in &[true, false] {
                assert_eq!(
                    heartbeat_interval_ms_v2(lane, false, advances),
                    heartbeat_interval_ms(lane),
                    "below the fence the rule may not move (lane {lane}, advances {advances})"
                );
            }
        }

        // Past the fence, a parent that advances the DAA is pacing the clock: stay out of the way.
        assert_eq!(heartbeat_interval_ms_v2(anchor, true, true), HEARTBEAT_NOMINAL_INTERVAL_MS);
        // ...and one that does not is NOT, however bonded it is. testnet-11 2026-09-21: BASE-0 at
        // DAA 7,680 was bonded, the old rule slept an hour, and the lottery produced nothing next.
        // Past the fence that parent does not pace the DAA, so the recovery cadence is the clock.
        assert_eq!(heartbeat_interval_ms_v2(attempt, true, false), HEARTBEAT_RECOVERY_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms_v2(receipt, true, false), HEARTBEAT_RECOVERY_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms_v2(beat, true, false), HEARTBEAT_RECOVERY_INTERVAL_MS);

        // The slot rule carries the same two answers, and refuses one millisecond early.
        let t = 1_700_000_000_000u64;
        assert!(check_heartbeat_slot_v2(t, attempt, true, false, t + HEARTBEAT_RECOVERY_INTERVAL_MS).is_ok());
        assert!(check_heartbeat_slot_v2(t, attempt, true, false, t + HEARTBEAT_RECOVERY_INTERVAL_MS - 1).is_err());
        assert!(check_heartbeat_slot_v2(t, attempt, false, false, t + HEARTBEAT_RECOVERY_INTERVAL_MS).is_err());
        assert!(check_heartbeat_slot_v2(t, attempt, false, false, t + HEARTBEAT_NOMINAL_INTERVAL_MS).is_ok());
        // Overflow still fails CLOSED under the new rule.
        assert!(check_heartbeat_slot_v2(u64::MAX, attempt, true, false, u64::MAX).is_err());
    }

    /// 2026-09-21 testnet-11: BASE-0 at DAA 7,680 was a bonded selected parent, so the old slot
    /// rule treated the chain as producing and held the heartbeat for an hour. The class lottery
    /// did not produce the next block either. Past the fence that parent does not pace the DAA,
    /// and a beat one recovery interval later is admissible.
    #[test]
    fn a_bonded_attempt_that_does_not_advance_the_daa_does_not_silence_the_clock() {
        let attempt = POW_ALGO_ID_PALW_COMMITTED_V2;
        let t = 1_700_000_000_000u64;
        assert_eq!(heartbeat_interval_ms_v2(attempt, true, false), HEARTBEAT_RECOVERY_INTERVAL_MS);
        assert!(check_heartbeat_slot_v2(t, attempt, true, false, t + HEARTBEAT_RECOVERY_INTERVAL_MS).is_ok());
        assert!(check_heartbeat_slot_v2(t, attempt, true, false, t + HEARTBEAT_RECOVERY_INTERVAL_MS - 1).is_err());
        // Below the fence the history-preserving rule is still the hour.
        assert_eq!(heartbeat_interval_ms_v2(attempt, false, false), HEARTBEAT_NOMINAL_INTERVAL_MS);
        assert!(check_heartbeat_slot_v2(t, attempt, false, false, t + HEARTBEAT_RECOVERY_INTERVAL_MS).is_err());
    }

    /// The miner has to agree with the rule, or it sleeps through the regime the lane exists for.
    #[test]
    fn the_miners_hint_yields_for_exactly_as_long_as_the_slot_rule_would() {
        let attempt = POW_ALGO_ID_PALW_COMMITTED_V2;
        let anchor = crate::pow_layer0::POW_ALGO_ID_BLAKE2B_SHA3;
        let t = 1_700_000_000_000u64;

        // Below the fence: unchanged, and `heartbeat_yield_hint_v1` is the same call.
        assert_eq!(heartbeat_yield_hint_v1(attempt, [(attempt, t)]), HeartbeatYieldHintV1::BondedSelectedParent);
        assert_eq!(heartbeat_yield_hint_v2(attempt, false, false, true, [(attempt, t)]), HeartbeatYieldHintV1::BondedSelectedParent);

        // Past the fence with a parent that paces the clock: still out of the way.
        assert_eq!(heartbeat_yield_hint_v2(anchor, true, true, false, [(attempt, t)]), HeartbeatYieldHintV1::BondedSelectedParent);

        // Past the fence on a chain with no priced lane: the attempt parent is no longer a reason
        // to sleep, and the wait a waiting attempt block buys is the RECOVERY interval the slot
        // rule would grant — not the nominal hour, which would stall the clock it now carries.
        // Past the fence on a chain with no priced lane, a waiting attempt block is NOT something to
        // yield to: it advances no clock, and the lane that must advance it would wait for ever
        // behind a lane that produces continuously.
        assert_eq!(heartbeat_yield_hint_v2(attempt, true, false, false, [(attempt, t)]), HeartbeatYieldHintV1::NothingToYieldTo);
        assert_eq!(heartbeat_yield_hint_v2(attempt, true, false, false, []), HeartbeatYieldHintV1::NothingToYieldTo);
        // ...but where the attempt lane DOES still advance the clock — past the anchor clock and
        // before the single lottery — the yield is the interval the slot rule would grant it.
        assert_eq!(
            heartbeat_yield_hint_v2(attempt, true, false, true, [(attempt, t)]),
            HeartbeatYieldHintV1::YieldUntil(t + HEARTBEAT_NOMINAL_INTERVAL_MS)
        );
    }

    #[test]
    fn the_interval_is_a_function_of_the_parents_lane_and_nothing_else() {
        // A bonded parent means the chain was producing one block ago: stay out of the way.
        assert_eq!(heartbeat_interval_ms(POW_ALGO_ID_PALW_COMMITTED_V2), HEARTBEAT_NOMINAL_INTERVAL_MS);
        assert_eq!(heartbeat_interval_ms(POW_ALGO_ID_PALW_RECEIPT_V3), HEARTBEAT_NOMINAL_INTERVAL_MS);
        // A heartbeat parent means the chain is already running on the clock: run at cadence.
        assert_eq!(heartbeat_interval_ms(PALW_HEARTBEAT_ALGO_ID), HEARTBEAT_RECOVERY_INTERVAL_MS);
        // The recovery step is the real cadence, not a compromise between the two.
        assert!(HEARTBEAT_RECOVERY_INTERVAL_MS < HEARTBEAT_NOMINAL_INTERVAL_MS);
    }

    /// **The slot rule, both sides, against a bonded parent.**
    #[test]
    fn a_heartbeat_waits_a_full_hour_behind_a_producing_chain() {
        let parent_ts = 1_700_000_000_000;
        let too_soon = parent_ts + HEARTBEAT_NOMINAL_INTERVAL_MS - 1;
        assert_eq!(
            check_heartbeat_slot(parent_ts, POW_ALGO_ID_PALW_COMMITTED_V2, too_soon),
            Err(HeartbeatTooEarly { last_heartbeat_timestamp: parent_ts, interval_ms: HEARTBEAT_NOMINAL_INTERVAL_MS })
        );
        // Exactly on the boundary is admitted — the rule is "at least", and an off-by-one here
        // would silently double the effective interval.
        assert!(check_heartbeat_slot(parent_ts, POW_ALGO_ID_PALW_COMMITTED_V2, parent_ts + HEARTBEAT_NOMINAL_INTERVAL_MS).is_ok());
    }

    /// **…and against a heartbeat parent, where the whole point is that it is faster.**
    ///
    /// Asserted as a DIFFERENCE at one timestamp: the same instant that is too early behind a
    /// bonded parent is admissible behind a heartbeat one. A test that only checked each side
    /// separately would pass on a ramp that had collapsed to a single interval.
    #[test]
    fn the_recovery_cadence_is_what_makes_a_stopped_chain_recoverable() {
        let parent_ts = 1_700_000_000_000;
        let one_cadence = parent_ts + HEARTBEAT_RECOVERY_INTERVAL_MS;
        assert!(check_heartbeat_slot(parent_ts, PALW_HEARTBEAT_ALGO_ID, one_cadence).is_ok());
        assert!(
            check_heartbeat_slot(parent_ts, POW_ALGO_ID_PALW_COMMITTED_V2, one_cadence).is_err(),
            "the same instant behind a producing chain is far too early — the ramp is the difference"
        );
        assert!(check_heartbeat_slot(parent_ts, PALW_HEARTBEAT_ALGO_ID, one_cadence - 1).is_err());
    }

    /// A parent timestamp near `u64::MAX` must refuse, never wrap into admitting everything.
    #[test]
    fn the_slot_rule_saturates_rather_than_wrapping() {
        assert!(check_heartbeat_slot(u64::MAX, PALW_HEARTBEAT_ALGO_ID, u64::MAX).is_err());
        assert!(check_heartbeat_slot(u64::MAX - 1, POW_ALGO_ID_PALW_COMMITTED_V2, u64::MAX).is_err());
    }

    /// **The lane has its own id, and it is not the hash lane's.**
    ///
    /// Sharing `POW_ALGO_ID_BLAKE2B_SHA3` was what forced the triple gate, and it meant a solved
    /// header from a hash network was a heartbeat's bytes. The Layer-0 digest binds `pow_algo_id`,
    /// so distinct ids make the two lanes' solutions non-interchangeable by construction.
    #[test]
    fn the_heartbeat_id_is_its_own_and_is_known_to_this_binary() {
        assert_ne!(PALW_HEARTBEAT_ALGO_ID, crate::pow_layer0::POW_ALGO_ID_BLAKE2B_SHA3);
        assert!(!is_palw_v2_algo_id(PALW_HEARTBEAT_ALGO_ID), "it is bondless — not a V2 lineage lane");
        crate::pow_layer0::check_algo_id_known(PALW_HEARTBEAT_ALGO_ID).expect("this binary can derive the heartbeat tag");
    }

    /// **A fixed-price lane must not be able to buy pruning-proof hierarchy.**
    ///
    /// With a constant target a lucky solve lands as far under it as under a hard one, so a level
    /// derived from the digest would be luck sold as structure — at 2²⁴ hashes a go. The receipt
    /// lane answers no for the neighbouring reason (a free digest), and both go through the one
    /// predicate so neither can be exempted by editing the other.
    /// **The heartbeat buys no LEVEL and yet weighs ε — two answers, so two predicates.**
    ///
    /// Reading the shared `algo_id_carries_no_chain_position` for both is a mistake with a silent
    /// failure mode: the ghostdag zero-arm runs before the ε-arm, so the lane would weigh nothing
    /// and a fully collapsed chain could not order its own branches — the exact regime the lane
    /// exists for. This pins the difference.
    #[test]
    fn the_heartbeat_buys_no_hierarchy_but_still_weighs_epsilon() {
        assert!(crate::pow_layer0::algo_id_derives_no_block_level(PALW_HEARTBEAT_ALGO_ID), "a fixed target buys no level");
        assert!(
            !crate::pow_layer0::algo_id_carries_no_chain_position(PALW_HEARTBEAT_ALGO_ID),
            "…but it is not weightless: ε is what orders heartbeat-only branches"
        );
        // The receipt lane answers no to BOTH, which is why one predicate was enough until now.
        assert!(crate::pow_layer0::algo_id_derives_no_block_level(POW_ALGO_ID_PALW_RECEIPT_V3));
        assert!(crate::pow_layer0::algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_RECEIPT_V3));
        // And the attempt lane IS the hierarchy — its digests are inference-priced.
        assert!(!crate::pow_layer0::algo_id_derives_no_block_level(POW_ALGO_ID_PALW_COMMITTED_V2));
        assert!(!crate::pow_layer0::algo_id_carries_no_chain_position(POW_ALGO_ID_PALW_COMMITTED_V2));
    }

    /// **ADR-0105 Decision 2: the hint has three answers, and the two that are not "yield" are
    /// told apart.**
    ///
    /// A bonded selected parent ends a heartbeat-led episode (the miner's budget starts over); a
    /// heartbeat selected parent with nothing bonded waiting is the lane doing its job. Folding
    /// both into one `None` would leave a miner unable to reset its budget without also resetting
    /// it on every quiet heartbeat — the wedge the budget exists to bound.
    #[test]
    fn the_yield_hint_separates_a_producing_chain_from_a_quiet_clock() {
        use crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3;
        let t = 1_700_000_000_000u64;
        // A bonded selected parent: whatever waits in the mergeset, the episode is over.
        for sp in [POW_ALGO_ID_PALW_COMMITTED_V2, POW_ALGO_ID_PALW_EXEC_V3, POW_ALGO_ID_PALW_RECEIPT_V3] {
            assert_eq!(heartbeat_yield_hint_v1(sp, [(POW_ALGO_ID_PALW_COMMITTED_V2, t)]), HeartbeatYieldHintV1::BondedSelectedParent);
            assert_eq!(heartbeat_yield_hint_v1(sp, []), HeartbeatYieldHintV1::BondedSelectedParent);
        }
        // A heartbeat selected parent with only heartbeats (or a zero-weight receipt) beside it:
        // the clock ticks.
        assert_eq!(heartbeat_yield_hint_v1(PALW_HEARTBEAT_ALGO_ID, []), HeartbeatYieldHintV1::NothingToYieldTo);
        assert_eq!(
            heartbeat_yield_hint_v1(PALW_HEARTBEAT_ALGO_ID, [(PALW_HEARTBEAT_ALGO_ID, t), (POW_ALGO_ID_PALW_RECEIPT_V3, t)]),
            HeartbeatYieldHintV1::NothingToYieldTo,
            "a receipt carries no weight a heartbeat could bury, and a sibling heartbeat is not a bonded block"
        );
        // An attempt block waiting — either attempt id — asks for the hour its own timestamp would
        // have bought it as a selected parent; the LATEST of several wins.
        assert_eq!(
            heartbeat_yield_hint_v1(
                PALW_HEARTBEAT_ALGO_ID,
                [(POW_ALGO_ID_PALW_COMMITTED_V2, t), (POW_ALGO_ID_PALW_EXEC_V3, t + 60_000), (PALW_HEARTBEAT_ALGO_ID, t + 90_000)]
            ),
            HeartbeatYieldHintV1::YieldUntil(t + 60_000 + HEARTBEAT_NOMINAL_INTERVAL_MS)
        );
        // A timestamp at the top of the range saturates into a long wait, which the miner's budget
        // bounds — it must not wrap into a deadline in the past.
        assert_eq!(
            heartbeat_yield_hint_v1(PALW_HEARTBEAT_ALGO_ID, [(POW_ALGO_ID_PALW_COMMITTED_V2, u64::MAX - 1)]),
            HeartbeatYieldHintV1::YieldUntil(u64::MAX)
        );
    }

    /// The price is a constant this crate states once, and it is the spam floor the withdrawn
    /// design tried to hold with a retarget clamp.
    #[test]
    fn the_price_is_a_constant_and_a_real_one() {
        assert_eq!(PALW_HEARTBEAT_WORK_LOG2, 24, "≈2²⁴ hashes: seconds of one CPU per interval, per BLOCK for a flooder");
        // A price of zero would make sibling flooding free, which is the only thing standing
        // between finding 3a (open) and an unbounded DAG.
        assert!(PALW_HEARTBEAT_WORK_LOG2 > 0);
    }
}
