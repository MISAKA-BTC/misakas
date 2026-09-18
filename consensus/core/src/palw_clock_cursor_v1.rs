//! **The consensus clock cursor** — ADR-0142.
//!
//! The heartbeat lane's admissibility used to read `selected_parent.timestamp + interval`, and the
//! selected parent is replaced by every new chain block. Past `palw_anchor_clock` an attempt block
//! advances no clock and yet moved the next opportunity to advance it, so a chain producing faster
//! than the interval suppressed the lane entirely and the score stopped while blocks kept coming.
//! Measured on the registry drill at a 21-second cadence: DAA frozen at 20, zero beats minted.
//!
//! The invariant this module exists to hold:
//!
//! > **A block that does not advance the consensus clock MUST NOT postpone the next opportunity to
//! > advance the consensus clock.**
//!
//! Two concerns were travelling through one value, and separating them is the whole fix:
//!
//! ```text
//! chain attachment   → the selected parent          (unchanged; a beat still builds on the tip)
//! clock eligibility  → the cursor below             (only a beat moves it)
//! ```
//!
//! Nothing here reads a parent, a lane or a walk. The cursor is state; a beat consumes a slot.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// **Where the clock stands.** Rooted state: written only by an admitted heartbeat's transition,
/// left untouched by every other lane, reverted with every other field on a reorg.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PalwClockCursorV1 {
    /// The earliest timestamp the next heartbeat may carry. The whole rule.
    pub next_slot_ms: u64,
    /// Slots consumed since the cursor opened. Telemetry and a reorg cross-check; no rule reads it.
    pub slots_consumed: u64,
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwClockCursorV1 {
    /// Two `u64`s and nothing behind them, so the estimate is exact. Implemented rather than left
    /// to the panicking default because the cursor is cached per block by the store layer.
    fn estimate_mem_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

/// Why a heartbeat was refused by the slot rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSlotTooEarly {
    pub next_slot_ms: u64,
    pub proposed_ms: u64,
}

impl ClockSlotTooEarly {
    /// Milliseconds still to wait. Zero is not reachable from a refusal, but saturating costs
    /// nothing and a wait that wrapped would be a miner spinning.
    #[inline]
    pub const fn wait_ms(&self) -> u64 {
        self.next_slot_ms.saturating_sub(self.proposed_ms)
    }
}

/// **The one definition.** Header validation, `heartbeat_adapt_block_template`, the miner's wait and
/// the tests all ask this — ADR-0066 Decision 2 required construction and validation to read one
/// answer, ADR-0138 §3c broke that by changing one side, and ADR-0142 §5 makes it a single function
/// so there is no second side to change.
#[inline]
pub fn palw_clock_slot_admits_v1(cursor: &PalwClockCursorV1, proposed_ms: u64) -> Result<(), ClockSlotTooEarly> {
    if proposed_ms >= cursor.next_slot_ms { Ok(()) } else { Err(ClockSlotTooEarly { next_slot_ms: cursor.next_slot_ms, proposed_ms }) }
}

/// **Open the cursor** at the first heartbeat past the fence: the rule starts where the chain is,
/// not at a boundary derived from history no node need still hold.
#[inline]
pub fn palw_clock_cursor_open_v1(first_beat_ms: u64, interval_ms: u64) -> PalwClockCursorV1 {
    PalwClockCursorV1 { next_slot_ms: first_beat_ms.saturating_add(interval_ms.max(1)), slots_consumed: 1 }
}

/// **Consume one slot.** The only writer of the cursor, and it is reached only for a heartbeat that
/// validation has already admitted.
///
/// `slots_skipped` is why this is not `beat_ms + interval`. Without it an outage leaves a backlog of
/// owed slots that a returning miner consumes back to back, running the clock fast exactly when the
/// windows it feeds are most stretched. With it the cursor lands on the first boundary strictly
/// after the beat: **missed slots are lost, not banked.** A beat that is late costs the chain the
/// DAA it did not tick, which is the honest accounting — the score counts elapsed slots, not a debt.
///
/// The beat's timestamp selects a slot and never a point inside one, so two beats with different
/// timestamps in the same slot leave the cursor in the same place, and a producer can move the next
/// opportunity only by whole intervals and only as far as the future-drift rule already allows.
#[inline]
pub fn palw_clock_cursor_advance_v1(cursor: &PalwClockCursorV1, beat_ms: u64, interval_ms: u64) -> PalwClockCursorV1 {
    let interval = interval_ms.max(1);
    let slots_skipped = beat_ms.saturating_sub(cursor.next_slot_ms) / interval;
    PalwClockCursorV1 {
        next_slot_ms: cursor.next_slot_ms.saturating_add(slots_skipped.saturating_add(1).saturating_mul(interval)),
        slots_consumed: cursor.slots_consumed.saturating_add(1),
    }
}

/// The cursor after one block, whatever lane it is. **Identity for everything but a heartbeat** —
/// this function IS the invariant, and `a_block_that_does_not_advance_the_clock_never_postpones_it`
/// is it as a property.
#[inline]
pub fn palw_clock_cursor_after_block_v1(
    cursor: Option<PalwClockCursorV1>,
    is_heartbeat: bool,
    beat_ms: u64,
    interval_ms: u64,
) -> Option<PalwClockCursorV1> {
    if !is_heartbeat {
        return cursor;
    }
    Some(match cursor {
        Some(c) => palw_clock_cursor_advance_v1(&c, beat_ms, interval_ms),
        None => palw_clock_cursor_open_v1(beat_ms, interval_ms),
    })
}

/// One block of the DAA window, as the clock reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockWindowBlockV1 {
    pub daa_score: u64,
    pub blue_score: u64,
    pub timestamp_ms: u64,
}

/// **Where the clock last advanced, derived rather than stored** (ADR-0142 §6a).
///
/// The cursor's whole content is "when did the DAA score last move". That is not private state: the
/// score is in every header, so the block that moved it to its current value is **the block at the
/// selected parent's score with the lowest blue score**, and its timestamp is the reference. Every
/// node that can process the block at all has the window this reads, so a pruned node and a node
/// that joined by pruning proof compute the same answer as an archival one — which is the property
/// a stored cursor could not give.
///
/// **The invariant survives.** An attempt block and a refused beat both carry the selected parent's
/// score and a HIGHER blue score, so neither is ever the minimum and neither moves the reference.
/// Only an advance does, because only an advance creates a block at a new score.
///
/// **Selected by blue score, not by timestamp.** Timestamps are not monotonic across a DAG, so a
/// minimum over timestamps could be pulled backwards by a merged block with an old but admissible
/// one, and an early reference opens a slot early. Blue score is monotonic along the chain, so the
/// minimum picks the block that actually advanced the score and nothing else can impersonate it.
///
/// `None` when the window holds no block at that score — the first block past the fence, or a
/// window that does not reach back to the advance. Both mean "no slot is known to be taken".
pub fn palw_clock_reference_v1(parent_daa_score: u64, window: impl IntoIterator<Item = ClockWindowBlockV1>) -> Option<u64> {
    window.into_iter().filter(|b| b.daa_score == parent_daa_score).min_by_key(|b| b.blue_score).map(|b| b.timestamp_ms)
}

/// The cursor that reference stands for: the next slot opens one interval after it.
#[inline]
pub fn palw_clock_cursor_from_reference_v1(reference_ms: u64, interval_ms: u64) -> PalwClockCursorV1 {
    PalwClockCursorV1 { next_slot_ms: reference_ms.saturating_add(interval_ms.max(1)), slots_consumed: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const I: u64 = 120_000;

    /// A small deterministic generator, so the properties below run over sequences rather than
    /// examples. No `rand` dependency and no wall clock: the seed is the test's own.
    fn lcg(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        *seed >> 11
    }

    /// **Property 1, ADR-0142 §6 — the one that failed.** For any sequence of blocks that do not
    /// advance the clock, at any spacing, the cursor does not move. The spacings are the measured
    /// ones: the drill's 21 s, the projected burst's 3.3 s, one second, and 119 s — every value
    /// under the interval, which is exactly where the old rule starved.
    #[test]
    fn a_block_that_does_not_advance_the_clock_never_postpones_it() {
        let start = PalwClockCursorV1 { next_slot_ms: 1_000_000, slots_consumed: 7 };
        for spacing_ms in [1_000u64, 3_300, 21_000, 119_000, 119_999] {
            let mut cursor = start;
            let mut t = start.next_slot_ms - 500_000;
            for _ in 0..5_000 {
                t += spacing_ms;
                cursor = palw_clock_cursor_after_block_v1(Some(cursor), false, t, I).expect("some");
                assert_eq!(cursor, start, "a non-clock block moved the cursor at spacing {spacing_ms} ms");
            }
            // ...and the slot the chain was owed is still open the moment it arrives.
            assert!(palw_clock_slot_admits_v1(&cursor, start.next_slot_ms).is_ok());
        }
    }

    /// **Property 2.** Past the cursor a beat is admissible, and one millisecond before it is not.
    /// There is no gap between "the slot opened" and "a beat may be built".
    #[test]
    fn the_slot_opens_exactly_at_the_cursor() {
        let c = PalwClockCursorV1 { next_slot_ms: 5_000, slots_consumed: 0 };
        assert!(palw_clock_slot_admits_v1(&c, 5_000).is_ok());
        assert!(palw_clock_slot_admits_v1(&c, u64::MAX).is_ok());
        let e = palw_clock_slot_admits_v1(&c, 4_999).expect_err("one ms early is early");
        assert_eq!((e.next_slot_ms, e.proposed_ms, e.wait_ms()), (5_000, 4_999, 1));
        assert_eq!(palw_clock_slot_admits_v1(&c, 0).expect_err("zero").wait_ms(), 5_000);
    }

    /// **Property 4.** Only a heartbeat advances the cursor — asserted over a generated mix of
    /// lanes rather than one of each.
    #[test]
    fn only_a_heartbeat_advances_the_cursor() {
        let mut seed = 0x5eed_1142u64;
        let mut cursor = Some(PalwClockCursorV1 { next_slot_ms: 0, slots_consumed: 0 });
        let (mut beats, mut others) = (0u64, 0u64);
        let mut t = 0u64;
        for _ in 0..20_000 {
            t += lcg(&mut seed) % 200_000;
            let is_beat = lcg(&mut seed).is_multiple_of(8);
            let before = cursor;
            cursor = palw_clock_cursor_after_block_v1(cursor, is_beat, t, I);
            if is_beat {
                beats += 1;
                assert_ne!(cursor, before, "a heartbeat must consume a slot");
                assert_eq!(cursor.unwrap().slots_consumed, beats);
            } else {
                others += 1;
                assert_eq!(cursor, before, "only a heartbeat may write the cursor");
            }
        }
        assert!(beats > 1_000 && others > 10_000, "the generator produced a degenerate mix: {beats}/{others}");
    }

    /// **Property 6.** A producer cannot lock the clock with a future timestamp beyond the slot it
    /// names: the cursor moves in whole intervals, so every timestamp inside one slot leaves it in
    /// the same place, and pushing it further costs a proportionally further-future timestamp —
    /// which the header's own drift rule bounds.
    #[test]
    fn the_cursor_moves_in_whole_slots_so_a_timestamp_cannot_place_it_freely() {
        let c = PalwClockCursorV1 { next_slot_ms: 1_000_000, slots_consumed: 3 };
        // Every instant inside the open slot is the same slot.
        let inside: Vec<_> =
            [0u64, 1, 500, I / 2, I - 1].iter().map(|d| palw_clock_cursor_advance_v1(&c, c.next_slot_ms + d, I)).collect();
        for step in &inside {
            assert_eq!(step.next_slot_ms, c.next_slot_ms + I, "a timestamp inside the slot places the cursor identically");
        }
        // And k slots late costs exactly k+1 intervals — linear, never further.
        for k in 0..50u64 {
            let beat = c.next_slot_ms + k * I + I / 3;
            let next = palw_clock_cursor_advance_v1(&c, beat, I);
            assert_eq!(next.next_slot_ms, c.next_slot_ms + (k + 1) * I);
            assert!(next.next_slot_ms > beat, "the next slot is always after the beat that consumed this one");
            assert!(next.next_slot_ms <= beat + I, "and never more than one interval after it");
        }
    }

    /// **Property 7.** One beat normalises any outage: whatever the silence, a single heartbeat
    /// leaves the cursor at the first boundary after it, with no backlog to consume. The failure
    /// this rules out is a returning miner minting slots back to back and running the clock fast
    /// exactly when the windows it feeds are most stretched.
    #[test]
    fn one_beat_normalises_an_outage_and_leaves_no_backlog() {
        let c = PalwClockCursorV1 { next_slot_ms: 1_000, slots_consumed: 1 };
        for silence_ms in [I, 10 * I, 3_600_000, 86_400_000, 30 * 86_400_000] {
            let beat = c.next_slot_ms + silence_ms;
            let after = palw_clock_cursor_advance_v1(&c, beat, I);
            assert!(after.next_slot_ms > beat, "the outage is not banked: the next slot is still ahead");
            assert!(after.next_slot_ms <= beat + I, "and it is one interval ahead, not the backlog");
            // A second beat at the same instant is refused, so the backlog cannot be drained.
            assert!(palw_clock_slot_admits_v1(&after, beat).is_err(), "no second beat in the slot just consumed");
            assert_eq!(after.slots_consumed, c.slots_consumed + 1, "one beat, one slot");
        }
    }

    /// Opening the cursor is the same shape as advancing it: the first beat past the fence sets the
    /// next boundary one interval out, and nothing before it is reachable.
    #[test]
    fn the_cursor_opens_where_the_chain_is() {
        assert_eq!(palw_clock_cursor_after_block_v1(None, false, 9_999, I), None, "a non-beat does not open it");
        let opened = palw_clock_cursor_after_block_v1(None, true, 9_999, I).expect("a beat opens it");
        assert_eq!(opened, PalwClockCursorV1 { next_slot_ms: 9_999 + I, slots_consumed: 1 });
        assert!(palw_clock_slot_admits_v1(&opened, 9_999 + I).is_ok());
        assert!(palw_clock_slot_admits_v1(&opened, 9_999 + I - 1).is_err());
    }

    /// Arithmetic that must not wrap. A cursor near the top of the range is not reachable through
    /// the timestamp rules, and saturating there costs nothing; wrapping would open every slot at
    /// once, which is the one failure worse than a stalled clock.
    #[test]
    fn nothing_here_wraps() {
        let high = PalwClockCursorV1 { next_slot_ms: u64::MAX - 5, slots_consumed: u64::MAX };
        let after = palw_clock_cursor_advance_v1(&high, u64::MAX, I);
        assert_eq!(after.next_slot_ms, u64::MAX, "saturating, never wrapping");
        assert_eq!(after.slots_consumed, u64::MAX);
        assert!(palw_clock_slot_admits_v1(&after, u64::MAX - 1).is_err(), "a saturated cursor still refuses");
        // A zero interval is not a configuration this ships, and it must not divide by zero.
        let z = palw_clock_cursor_advance_v1(&PalwClockCursorV1 { next_slot_ms: 10, slots_consumed: 0 }, 10, 0);
        assert_eq!(z.next_slot_ms, 11);
    }

    /// **Operator check 1: several beats in one mergeset, and which consumes the slot.**
    ///
    /// The rule takes the NEWEST beat in the mergeset, and `max` is order-independent — so nodes
    /// that walk an unordered mergeset in different orders land on the same cursor. Ties are not a
    /// case: two beats with the same timestamp give the same answer, which is why the rule is
    /// stated over a timestamp rather than over a block identity that would need tie-breaking.
    ///
    /// Newest and not oldest, deliberately: a mergeset carrying one stale beat and one current beat
    /// must grant, or a chain could be starved by merging an old beat beside a good one.
    #[test]
    fn several_beats_in_one_mergeset_land_on_one_cursor_whatever_the_walk_order() {
        let cursor = PalwClockCursorV1 { next_slot_ms: 1_000_000, slots_consumed: 4 };
        let beats = [1_000_000u64, 1_000_001, 1_030_000, 999_999, 1_119_999];
        let newest = *beats.iter().max().expect("non-empty");
        let expected = palw_clock_cursor_advance_v1(&cursor, newest, I);

        // Every permutation of the walk reduces to the same newest beat, so to the same cursor.
        let mut seed = 0xC0FFEEu64;
        for _ in 0..500 {
            let mut order = beats;
            for i in (1..order.len()).rev() {
                let j = (lcg(&mut seed) as usize) % (i + 1);
                order.swap(i, j);
            }
            let folded = order.iter().fold(None::<u64>, |acc, &t| Some(acc.map_or(t, |a: u64| a.max(t))));
            assert_eq!(palw_clock_cursor_advance_v1(&cursor, folded.expect("some"), I), expected, "order {order:?}");
        }
        // And a stale beat beside a current one does not stop the current one being granted.
        assert!(palw_clock_slot_admits_v1(&cursor, newest).is_ok());
        assert!(palw_clock_slot_admits_v1(&cursor, 999_999).is_err(), "the stale one alone would not have been");
    }

    /// **Operator check 4: a producer cannot send the clock into the future.**
    ///
    /// The cursor lands at most one interval past the beat that consumed the slot, and a beat's own
    /// timestamp is bounded by the header rule `check_block_timestamp_in_isolation` at
    /// `now + TIMESTAMP_DEVIATION_TOLERANCE`. So the worst a producer can do, per slot and at the
    /// price of one beat's work, is push the next opportunity to `now + tolerance + interval`.
    ///
    /// With the shipped constants that is 132 s + 120 s = 252 s against a nominal 120 s: the clock
    /// can be slowed to about 2.1x, and it cannot be stopped, moved backwards, or moved once and
    /// left there. This test pins the arithmetic so a change to either constant is a decision.
    #[test]
    fn a_future_timestamp_slows_the_clock_by_a_bounded_factor_and_can_do_nothing_else() {
        const TOLERANCE_MS: u64 = crate::config::constants::consensus::TIMESTAMP_DEVIATION_TOLERANCE * 1_000;
        let now = 1_700_000_000_000u64;
        let cursor = PalwClockCursorV1 { next_slot_ms: now, slots_consumed: 9 };

        // The furthest-future beat a header rule admits, and the answer it buys.
        let worst = now + TOLERANCE_MS;
        let after = palw_clock_cursor_advance_v1(&cursor, worst, I);

        // **The slot granularity quantises the attack, which is the property worth having.** The
        // cursor lands on a slot BOUNDARY, never at `beat + interval`, so a timestamp buys whole
        // slots and nothing finer: 132 s of drift and 1 ms of drift cost the chain the same two
        // slots, and a producer gains nothing by aiming carefully.
        let slots = 1 + TOLERANCE_MS / I;
        assert_eq!(after.next_slot_ms, now + slots * I, "the cursor lands on a boundary, not at beat + interval");
        assert_eq!(slots, 2, "with the shipped constants the worst a beat can buy is two slots");
        assert!(after.next_slot_ms - now < 3 * I, "so the clock can be slowed to about 2x and no further");

        // It is bounded PER SLOT, not once and for ever: the next beat starts from the new cursor
        // and is bounded the same way, so the attack costs a beat's work every slot to hold.
        let again = palw_clock_cursor_advance_v1(&after, after.next_slot_ms + TOLERANCE_MS, I);
        assert_eq!(again.next_slot_ms - after.next_slot_ms, slots * I, "the same bound again, not compounding");

        // And it can never move the cursor BACK, which would be the serious failure: a cursor that
        // moved backwards would reopen a slot already consumed and let the clock run free.
        for beat in [0u64, 1, now - 1, now, now + 1] {
            assert!(palw_clock_cursor_advance_v1(&cursor, beat, I).next_slot_ms > cursor.next_slot_ms, "beat {beat}");
        }
    }

    /// **Operator check 3, and it FAILS: a node that cannot read the parent's cursor answers
    /// differently from one that can.**
    ///
    /// This test exists to pin an open defect, not a property. `palw_clock_step_v1` reads the
    /// cursor of the selected parent from a store, and treats an absent row as "no cursor yet",
    /// which grants the exemption unconditionally. An archival node holding the row does not grant.
    /// Two nodes therefore compute different DAA scores for the same block, and the one without the
    /// row rejects a header the network accepted.
    ///
    /// It is reachable two ways. The store is pruned nowhere today, so it also grows without bound;
    /// once pruning is added, the block below the pruning point loses its row. And a node that IBDs
    /// from a pruning proof never had the row at all, because the cursor is derived data with no
    /// commitment and no carriage.
    ///
    /// **This is ADR-0066 finding 4 in a new place** — "an archival node never hit `Err(get_header)`,
    /// a pruned node hit it at its own pruning point, and the two computed different verdicts for
    /// one header" — and it is why ADR-0142 is not armed on any preset. The fix has to make the
    /// cursor either carried and verifiable, or derivable from data every node has: the fence stays
    /// dormant until it is one of those.
    #[test]
    fn a_missing_parent_cursor_answers_differently_from_a_present_one() {
        // The same mergeset, the same beat, two nodes. One has the parent's cursor; one does not.
        let beat_ms = 1_000_000u64;
        let parent_cursor = PalwClockCursorV1 { next_slot_ms: beat_ms + 60_000, slots_consumed: 3 };

        // The node that has it refuses the slot: the beat is inside one already consumed.
        assert!(palw_clock_slot_admits_v1(&parent_cursor, beat_ms).is_err(), "with the row, the slot is shut");

        // The node that does not has nothing to compare against, so it opens the cursor and grants.
        let granted_without = palw_clock_cursor_after_block_v1(None, true, beat_ms, I);
        assert_eq!(granted_without, Some(palw_clock_cursor_open_v1(beat_ms, I)), "without the row, the beat is granted");

        // The two answers differ, and the difference is a DAA score: one node counts the exemption
        // and the other does not. That is a fork, not a discrepancy.
        let with_row = palw_clock_cursor_after_block_v1(Some(parent_cursor), false, beat_ms, I);
        assert_ne!(with_row, granted_without, "the same block, two cursors — and so two scores");
    }

    fn win(daa: u64, blue: u64, ts: u64) -> ClockWindowBlockV1 {
        ClockWindowBlockV1 { daa_score: daa, blue_score: blue, timestamp_ms: ts }
    }

    /// **The derived reference is the stored cursor, without the store.** ADR-0142 §6a check 3.
    ///
    /// The invariant is the same one: a block that does not advance the clock must not move the
    /// reference. Here that is structural rather than enforced — an attempt block and a refused
    /// beat carry the selected parent's score and a higher blue score, so neither can be the
    /// minimum.
    #[test]
    fn the_reference_is_the_block_that_advanced_the_score_and_nothing_else_moves_it() {
        // The score advanced at blue 100, timestamp 1_000_000. Everything after it sits at the same
        // score with higher blue scores: attempts, refused beats, whatever the chain produced.
        let advance = win(7, 100, 1_000_000);
        let mut window = vec![advance];
        assert_eq!(palw_clock_reference_v1(7, window.clone()), Some(1_000_000));

        for (i, ts) in [1_000_100u64, 1_003_300, 1_021_000, 1_119_999, 1_200_000].into_iter().enumerate() {
            window.push(win(7, 101 + i as u64, ts));
            assert_eq!(palw_clock_reference_v1(7, window.clone()), Some(1_000_000), "a block at the same score moved it");
        }

        // **Blue score and not timestamp.** A merged block with an OLD but admissible timestamp
        // cannot pull the reference back, because it cannot have a lower blue score than the block
        // that advanced. A minimum over timestamps would have taken it and opened a slot early.
        window.push(win(7, 200, 1));
        assert_eq!(palw_clock_reference_v1(7, window.clone()), Some(1_000_000), "an old timestamp did not pull it back");
        assert_eq!(window.iter().map(|b| b.timestamp_ms).min(), Some(1), "…and a timestamp minimum would have");

        // Blocks at other scores are other slots and are not this one's business.
        window.push(win(6, 1, 500_000));
        window.push(win(8, 300, 1_300_000));
        assert_eq!(palw_clock_reference_v1(7, window.clone()), Some(1_000_000));
        assert_eq!(palw_clock_reference_v1(8, window.clone()), Some(1_300_000), "the next score's own advance");
        assert_eq!(palw_clock_reference_v1(9, window), None, "a score the window does not reach is no slot taken");
    }

    /// The reference and the cursor are two spellings of one fact, so they must agree on when the
    /// next slot opens — otherwise the derived path and the stored path would admit different beats.
    #[test]
    fn the_derived_reference_and_the_cursor_open_the_same_slot() {
        let reference = 1_000_000u64;
        let cursor = palw_clock_cursor_from_reference_v1(reference, I);
        assert_eq!(cursor.next_slot_ms, reference + I);
        assert!(palw_clock_slot_admits_v1(&cursor, reference + I).is_ok());
        assert!(palw_clock_slot_admits_v1(&cursor, reference + I - 1).is_err());
        // A beat that consumed the slot becomes the next reference, and the slot after it opens one
        // interval later — the same arithmetic the stored cursor performs on an exact beat.
        let next = palw_clock_cursor_from_reference_v1(reference + I, I);
        assert_eq!(next.next_slot_ms, reference + 2 * I);
        // And a zero interval does not divide by zero here either.
        assert_eq!(palw_clock_cursor_from_reference_v1(10, 0).next_slot_ms, 11);
    }

    /// The type is rooted state, so its encoding is a consensus fact: pin the byte layout.
    #[test]
    fn the_cursor_round_trips_and_its_bytes_are_pinned() {
        let c = PalwClockCursorV1 { next_slot_ms: 0x0102_0304_0506_0708, slots_consumed: 1 };
        let bytes = borsh::to_vec(&c).expect("borsh");
        assert_eq!(bytes, vec![8, 7, 6, 5, 4, 3, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0], "two little-endian u64s, in field order");
        assert_eq!(PalwClockCursorV1::try_from_slice(&bytes).expect("decode"), c);
    }
}
