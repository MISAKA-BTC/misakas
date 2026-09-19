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
/// **The carried cursor is gone, and that is the point of ADR-0142 §6a.**
///
/// This module once also opened, advanced and carried a cursor from block to block. That half was
/// computed on every block and written into a `DaaWindow` field no reader ever read, so its
/// arithmetic — the whole-slot advance, "missed slots are lost not banked", `slots_consumed` —
/// decided nothing, while a property suite exercising it passed and looked like evidence about the
/// chain. It is deleted rather than wired, because wiring it is precisely the stored-cursor design
/// ADR-0142 replaced: a node that joined by pruning proof cannot reconstruct a carried value, and
/// that divergence is the bug the derivation below exists to remove.
///
/// What runs is two functions: [`palw_clock_reference_v1`] derives where the clock last advanced
/// from the DAA window every node has, and [`palw_clock_slot_admits_v1`] asks whether a beat is at
/// or past the slot that reference opens.

/// One block of the DAA window, as the clock reads it.
///
/// **The hash is here to make the selection TOTAL, and that is a rule rather than a nicety.** The
/// reference is a minimum over the window, and a minimum over a key that can tie is decided by
/// iteration order — which is exactly the defect ADR-0143 was written to close one layer up, and
/// which would be a consensus divergence here: two blocks with the same parent set have the same
/// blue score by construction (GHOSTDAG is a function of that set), so the tie is structural and not
/// incidental. The legacy retarget's own `DifficultyBlock` carries `sortable_block` for the same
/// reason; this reader dropped it and gets it back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockWindowBlockV1 {
    pub daa_score: u64,
    pub blue_score: u64,
    pub timestamp_ms: u64,
    /// The block's own hash: the identity that makes `(blue_score, hash)` a total order.
    pub hash: crate::Hash64,
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
    window
        .into_iter()
        .filter(|b| b.daa_score == parent_daa_score)
        .min_by_key(|b| (b.blue_score, b.hash))
        .map(|b| b.timestamp_ms)
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

    /// **The four questions the carried cursor's tests used to answer, asked of the rule that runs.**
    ///
    /// Those tests exercised an advance function whose result was written into a field no reader
    /// read, so they were evidence about a path nothing ran. The derived design answers three of the
    /// four by construction and the fourth by arithmetic, and that is what this pins.
    #[test]
    fn the_derived_rule_answers_what_the_carried_one_was_asked() {
        // 1. SEVERAL BEATS IN ONE MERGESET. The reference is a function of the WINDOW, not of the
        //    mergeset's walk order, and the tie inside it is total — so every node computes one
        //    slot for all of them. (`a_tie_at_the_parents_score_...` walks the permutations.)
        let window = [win_at(10, 5, 1_000, 0xAA), win_at(10, 5, 1_000, 0xBB), win_at(10, 7, 4_000, 0xCC)];
        let r = palw_clock_reference_v1(10, window).expect("a reference");
        let cursor = palw_clock_cursor_from_reference_v1(r, I);
        for beat in [r, r + 1, r + I - 1] {
            assert!(palw_clock_slot_admits_v1(&cursor, beat).is_err(), "every beat in the slot meets the same closed slot");
        }
        assert!(palw_clock_slot_admits_v1(&cursor, r + I).is_ok(), "and the same open one");

        // 2. A REORG DRAGS NOTHING. There is no stored value to drag: the answer is a pure function
        //    of the window the block itself carries, so a branch cannot inherit another's cursor.
        let other_branch = [win_at(10, 5, 8_000, 0xAA)];
        assert_eq!(palw_clock_reference_v1(10, other_branch), Some(8_000), "a different branch, a different window, its own answer");

        // 3. IBD REBUILDS THE SAME VALUE. Same window, same answer, with no state to reconstruct —
        //    which is the property a stored cursor could not give a node that joined by pruning
        //    proof, and the reason ADR-0142 §6a derives instead of carrying.
        assert_eq!(palw_clock_reference_v1(10, window), Some(1_000));
        assert_eq!(palw_clock_reference_v1(10, window), palw_clock_reference_v1(10, window));

        // 4. A FUTURE TIMESTAMP CANNOT PULL THE SLOT IN. The reference is the timestamp of the block
        //    that ADVANCED the score, and a producer stamping far ahead only pushes its own next
        //    slot further out — it can never open one early, which is the direction that would let
        //    the lane run faster than the interval.
        let honest = palw_clock_cursor_from_reference_v1(1_000, I);
        let stamped_ahead = palw_clock_cursor_from_reference_v1(1_000 + 10 * I, I);
        assert!(stamped_ahead.next_slot_ms > honest.next_slot_ms, "a future stamp delays its own lane and nothing else");
        assert!(palw_clock_slot_admits_v1(&stamped_ahead, honest.next_slot_ms).is_err(), "it cannot open the honest slot early");
    }

    fn win(daa: u64, blue: u64, ts: u64) -> ClockWindowBlockV1 {
        // A distinct hash per (daa, blue, ts), so the fixtures below cannot accidentally tie on the
        // identity the selection now breaks ties with.
        win_at(daa, blue, ts, daa ^ (blue << 20) ^ (ts << 40))
    }

    fn win_at(daa: u64, blue: u64, ts: u64, id: u64) -> ClockWindowBlockV1 {
        ClockWindowBlockV1 { daa_score: daa, blue_score: blue, timestamp_ms: ts, hash: crate::Hash64::from_u64_word(id) }
    }

    /// **The reference is a TOTAL order, so no iteration order can change a DAA score.**
    ///
    /// Two blocks with the same parent set have the same blue score by construction — GHOSTDAG is a
    /// function of that set — so a tie at the parent's score is structural. Before the identity was
    /// part of the key, `min_by_key` returned whichever of them a node's window happened to yield
    /// first, and the reference is what decides whether the next beat is admitted: two nodes would
    /// have granted different DAA scores to the same block. This walks every permutation of a tied
    /// window and requires one answer.
    #[test]
    fn a_tie_at_the_parents_score_is_broken_by_identity_and_not_by_iteration_order() {
        let tied = [win_at(10, 5, 1_000, 0xAA), win_at(10, 5, 9_000, 0xBB), win_at(10, 5, 5_000, 0xCC)];
        let expected = palw_clock_reference_v1(10, tied).expect("a reference");
        assert_eq!(expected, 1_000, "the lowest hash of the tied blue scores, whatever the order");
        for order in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
            let permuted: Vec<_> = order.iter().map(|i| tied[*i]).collect();
            assert_eq!(palw_clock_reference_v1(10, permuted).expect("a reference"), expected, "{order:?} answered differently");
        }
        // The blue score still leads: a strictly lower blue score wins however high its hash.
        let mixed = [win_at(10, 5, 1_000, 0x11), win_at(10, 4, 7_000, 0xFF)];
        assert_eq!(palw_clock_reference_v1(10, mixed), Some(7_000), "blue score first, identity only to break a tie");
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
