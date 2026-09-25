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

/// **The lead cap (`Params::palw_clock_lead_cap`): how far past the RECEIVING node's clock a header
/// that moves the heartbeat clock may be stamped** — 132 s, one recovery interval plus 12 s, the
/// future-drift tolerance testnet-12 ran before 2026-09-25.
///
/// The clock floor spaces clock-moving STAMPS one interval apart, and nothing else bounds them in
/// WALL time but the header's future-drift tolerance. At mainnet's 1,620 s one producer can mint every
/// slot stamped at or below `now + T` at once — `⌊T / I⌋ + 1` = 14 DAA with no other block in
/// between (2 at 132 s) — and a burst that outlasts a readiness row's remaining age lapses the row
/// with no possession proof able to land (the 2026-09-25 mainnet-values review, HIGH). Under the cap
/// every step is stamped at least one interval after the last and at most this far past the
/// receiver's clock, so a burst is `⌊132 / 120⌋ + 1` = 2 steps, inside the readiness escalation's
/// 2-DAA margin — whatever the tolerance ordinary blocks keep.
///
/// **A local-clock rule, like the tolerance itself**: the answer depends on when the header is
/// judged, so a refusal is never a verdict about the block — it is not cached as invalid, and the
/// same header is admitted once wall time reaches `timestamp − cap`. History (IBD, header sync, a
/// node that was offline) is stamped in the past and always passes; trusted blocks imported with a
/// pruning proof never reach the header stage that asks it.
pub const PALW_CLOCK_LEAD_CAP_MS: u64 = crate::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS + 12_000;

/// **The lead cap's arithmetic** — `Ok` when `timestamp` is at most [`PALW_CLOCK_LEAD_CAP_MS`] past
/// `now_ms`; otherwise `Err` with the latest stamp `now_ms` admits. Header validation, the template
/// builder and the tests all ask this one function.
#[inline]
pub fn palw_clock_lead_admits_v1(timestamp: u64, now_ms: u64) -> Result<(), u64> {
    let latest = now_ms.saturating_add(PALW_CLOCK_LEAD_CAP_MS);
    if timestamp <= latest { Ok(()) } else { Err(latest) }
}
// **The carried cursor is gone, and that is the point of ADR-0142 §6a.**
//
// This module once also opened, advanced and carried a cursor from block to block. That half was
// computed on every block and written into a `DaaWindow` field no reader ever read, so its
// arithmetic — the whole-slot advance, "missed slots are lost not banked", `slots_consumed` —
// decided nothing, while a property suite exercising it passed and looked like evidence about the
// chain. It is deleted rather than wired, because wiring it is precisely the stored-cursor design
// ADR-0142 replaced: a node that joined by pruning proof cannot reconstruct a carried value, and
// that divergence is the bug the derivation below exists to remove.
//
// What runs is two functions: [`palw_clock_reference_v1`] derives where the clock last advanced
// from the DAA window every node has, and [`palw_clock_slot_admits_v1`] asks whether a beat is at
// or past the slot that reference opens.

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
    window.into_iter().filter(|b| b.daa_score == parent_daa_score).min_by_key(|b| (b.blue_score, b.hash)).map(|b| b.timestamp_ms)
}

/// **The reference past `palw_clock_floor`: the lowest blue score, then the EARLIEST timestamp,
/// then the hash** (the 2026-09-24 heartbeat audit, H5).
///
/// [`palw_clock_reference_v1`] broke a blue-score tie by hash. The tie is structural — two blocks with
/// the same parents have the same blue score — and the tied blocks are exactly the sibling STEPS that
/// merged one granted beat. So the hash chose which step's timestamp opened the next slot, and a step
/// stamped up to the 132 s drift tolerance in the future won half the time: the next slot moved back by
/// up to 132 s (measured +31 s at DAA 8 and +18 s at DAA 24). The earliest tied step is the honest
/// answer to "when did the score last move".
///
/// §6a's reason for NOT taking a timestamp minimum still holds across blue scores — an old but
/// admissible timestamp on a later block must not pull the reference back — and it is not reopened:
/// the minimum is taken only INSIDE the lowest blue score. Inside it the floor's own step rule holds
/// (a block that steps on a grant is stamped at or past the cursor it consumed), so no tied step
/// predates its slot and the earliest of them cannot open the next slot early. The hash remains the
/// last key, so the order stays total.
pub fn palw_clock_reference_v2(parent_daa_score: u64, window: impl IntoIterator<Item = ClockWindowBlockV1>) -> Option<u64> {
    window
        .into_iter()
        .filter(|b| b.daa_score == parent_daa_score)
        .min_by_key(|b| (b.blue_score, b.timestamp_ms, b.hash))
        .map(|b| b.timestamp_ms)
}

/// The cursor that reference stands for: the next slot opens one interval after it.
#[inline]
pub fn palw_clock_cursor_from_reference_v1(reference_ms: u64, interval_ms: u64) -> PalwClockCursorV1 {
    PalwClockCursorV1 { next_slot_ms: reference_ms.saturating_add(interval_ms.max(1)), slots_consumed: 0 }
}

/// **What the clock decided for one block — the other half of the DAA score's answer.**
///
/// `palw_clock_step_v1` computes, for every block, whether a heartbeat in its mergeset is granted a
/// DAA. The exemption count is the half the score reads; this is the half everything else needs —
/// the heartbeat adapter's `earliest`, the miner's hint and (behind their own fence) the header's
/// timestamp rules. It rides in the DAA window beside the score, so each of those readers takes the
/// decision the score was computed from instead of re-deriving it: ADR-0142 §5's "one function, and
/// everything asks it", extended to the readers that came after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClockStepV1 {
    /// `palw_clock_cursor` governs this block — read at its SELECTED PARENT's score, as the grant is.
    pub governs: bool,
    /// The cursor this block's own window derives at its selected parent's score. `None` where the
    /// cursor does not govern, or where the window holds no block at that score (no slot is known
    /// to be taken — the first block past the fence, or genesis).
    pub cursor: Option<PalwClockCursorV1>,
    /// This block steps the clock on a heartbeat's grant: its mergeset carries nothing `bits`
    /// prices and a beat at or past the cursor. Exactly the condition that takes one beat out of
    /// the exempt count.
    pub granted: bool,
    /// `palw_clock_floor` governs this block (the 2026-09-24 heartbeat audit's H3/H5), read at the
    /// same selected-parent score as `governs`: a heartbeat must be stamped at or past `cursor`,
    /// and so must a block that is `granted`. Never set where `governs` is not.
    pub floor: bool,
}

impl PalwClockStepV1 {
    /// The earliest timestamp a beat built on these parents can carry and still be the beat a slot
    /// grants — the cursor's next slot. `None` where no cursor governs or none is known, which is
    /// "no slot to wait for".
    #[inline]
    pub fn next_slot_ms(&self) -> Option<u64> {
        if self.governs { self.cursor.map(|cursor| cursor.next_slot_ms) } else { None }
    }

    /// **H5: may a block stamped `timestamp` step the clock on these parents?** A block that is
    /// `granted` becomes the next slot's reference, so past the floor it is stamped at or past the
    /// cursor it consumed — or the next slot would open early, and nothing would floor the spacing
    /// between two ticks. `Ok` wherever the floor does not govern, nothing is granted, or no
    /// reference is known.
    #[inline]
    pub fn step_stamp_admits(&self, timestamp: u64) -> Result<(), ClockSlotTooEarly> {
        match self.cursor {
            Some(cursor) if self.floor && self.granted => palw_clock_slot_admits_v1(&cursor, timestamp),
            _ => Ok(()),
        }
    }

    /// The timestamp a TEMPLATE for these parents must carry at least (H5's construction half):
    /// `proposed` raised to the cursor's slot where the block will step on a grant past the floor,
    /// `proposed` itself everywhere else.
    #[inline]
    pub fn floor_stamp(&self, proposed: u64) -> u64 {
        match self.cursor {
            Some(cursor) if self.floor && self.granted => proposed.max(cursor.next_slot_ms),
            _ => proposed,
        }
    }

    /// **H3: may a heartbeat header stamped `timestamp` stand on these parents?** Past the floor,
    /// only at or past the cursor its own window derives — the slot it could be granted. `Ok`
    /// wherever the floor does not govern or no reference is known ("no slot is known to be
    /// taken", the grant's own reading of a missing reference).
    #[inline]
    pub fn heartbeat_stamp_admits(&self, timestamp: u64) -> Result<(), ClockSlotTooEarly> {
        match self.cursor {
            Some(cursor) if self.floor => palw_clock_slot_admits_v1(&cursor, timestamp),
            _ => Ok(()),
        }
    }

    /// **Does the lead cap ([`PALW_CLOCK_LEAD_CAP_MS`]) govern a header of lane `pow_algo_id` on
    /// these parents?** Two kinds of header move the clock, and both are capped:
    ///
    /// * **the block that STEPS it** — `granted`, exactly the condition that takes one beat out of
    ///   the exempt count (any lane: on testnet-12 the first block of any lane built after a granted
    ///   beat carries the tick, and a beat built on a beat is one too). Its stamp is the next slot's
    ///   reference, so capping it is what bounds a burst in wall time;
    /// * **every heartbeat** — the beat a step is granted on. Its own stamp opens nothing, but an
    ///   uncapped beat is the cheap way to fill the past-median window with far-future stamps
    ///   (`2^24` hashes a beat), and a median pushed past `now + cap` holds every honest step, which is
    ///   stamped above it. Capped, that lever costs blocks of a lane `bits` or a ticket prices. An
    ///   honest beat is never stamped ahead of its miner's clock: the miner waits for its slot.
    ///
    /// Every other block keeps the network's full future-drift tolerance.
    #[inline]
    pub fn lead_capped(&self, pow_algo_id: u8) -> bool {
        self.granted || pow_algo_id == crate::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const I: u64 = 120_000;

    // A small deterministic generator, so the properties below run over sequences rather than
    // examples. No `rand` dependency and no wall clock: the seed is the test's own.

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

    /// **H3's predicate: past the floor a beat is admitted at or after its slot and not before; with
    /// no floor, or no known reference, it is admitted anywhere** (the grant's own reading of a
    /// missing reference — "no slot is known to be taken").
    #[test]
    fn a_beat_is_stamped_at_or_after_its_slot_only_where_the_floor_governs() {
        let cursor = Some(PalwClockCursorV1 { next_slot_ms: 10_000, slots_consumed: 0 });
        let floored = PalwClockStepV1 { governs: true, cursor, granted: false, floor: true };
        assert!(floored.heartbeat_stamp_admits(10_000).is_ok());
        assert!(floored.heartbeat_stamp_admits(u64::MAX).is_ok());
        let early = floored.heartbeat_stamp_admits(9_999).expect_err("one ms before the slot");
        assert_eq!((early.next_slot_ms, early.proposed_ms), (10_000, 9_999));
        assert!(PalwClockStepV1 { floor: false, ..floored }.heartbeat_stamp_admits(0).is_ok(), "no floor, no stamp rule");
        assert!(PalwClockStepV1 { cursor: None, ..floored }.heartbeat_stamp_admits(0).is_ok(), "no reference, no slot taken");
        assert_eq!(floored.next_slot_ms(), Some(10_000));
        assert_eq!(PalwClockStepV1 { governs: false, ..floored }.next_slot_ms(), None);
    }

    /// **H5: a future-stamped sibling step cannot delay the slot.** Two steps merging one beat tie on
    /// blue score by construction; under v1 the hash picked the reference, so a step stamped 132 s in
    /// the future won whenever its hash was the lower one. Under v2 the earlier step is the reference
    /// whatever the hashes, in every order.
    #[test]
    fn a_future_stamped_sibling_step_cannot_delay_the_slot() {
        let honest = win_at(10, 5, 1_000_000, 0xFF);
        let ahead = win_at(10, 5, 1_000_000 + 132_000, 0x01);
        // The attack v1 admitted: the future-stamped sibling has the lower hash and wins.
        assert_eq!(palw_clock_reference_v1(10, [honest, ahead]), Some(1_132_000), "v1: the hash picks the delay");
        for order in [[honest, ahead], [ahead, honest]] {
            assert_eq!(palw_clock_reference_v2(10, order), Some(1_000_000), "v2: the earliest tied step, in either order");
        }
        // Blue score still leads: a later block with an OLD timestamp cannot pull the reference back.
        let later_old = win_at(10, 9, 1, 0x00);
        assert_eq!(palw_clock_reference_v2(10, [honest, ahead, later_old]), Some(1_000_000), "§6a's objection still holds");
        // And a full tie falls to the hash, so the order stays total.
        let twin = win_at(10, 5, 1_000_000, 0x02);
        for order in [[honest, twin], [twin, honest]] {
            assert_eq!(palw_clock_reference_v2(10, order), Some(1_000_000));
        }
        assert_eq!(palw_clock_reference_v2(11, [honest]), None, "a score the window does not reach is no slot taken");
    }

    /// **H5: a block that steps the clock is stamped at or past the slot it consumed, and a template
    /// for one is stamped there.**
    #[test]
    fn a_step_is_stamped_at_or_past_the_slot_it_consumed() {
        let cursor = Some(PalwClockCursorV1 { next_slot_ms: 10_000, slots_consumed: 0 });
        let step = PalwClockStepV1 { governs: true, cursor, granted: true, floor: true };
        assert!(step.step_stamp_admits(10_000).is_ok());
        assert_eq!(step.step_stamp_admits(9_999).expect_err("a step one ms early").next_slot_ms, 10_000);
        assert_eq!(step.floor_stamp(1), 10_000, "a template is raised to the slot");
        assert_eq!(step.floor_stamp(12_345), 12_345, "and a later clock kept");
        for relaxed in [
            PalwClockStepV1 { granted: false, ..step },
            PalwClockStepV1 { floor: false, ..step },
            PalwClockStepV1 { cursor: None, ..step },
        ] {
            assert!(relaxed.step_stamp_admits(0).is_ok(), "{relaxed:?}");
            assert_eq!(relaxed.floor_stamp(1), 1, "{relaxed:?}");
        }
    }

    /// **The lead cap: a step and a heartbeat are admitted at most 132 s past the receiver's clock,
    /// every other header is not asked, and a burst under it is two steps.**
    #[test]
    fn the_lead_cap_governs_steps_and_beats_and_bounds_a_burst_to_two() {
        assert_eq!(PALW_CLOCK_LEAD_CAP_MS, 132_000, "one interval plus 12 s — testnet-12's tolerance before 2026-09-25");
        let now = 1_788_000_000_000u64;
        assert!(palw_clock_lead_admits_v1(now + PALW_CLOCK_LEAD_CAP_MS, now).is_ok(), "the cap itself is admitted");
        assert_eq!(palw_clock_lead_admits_v1(now + PALW_CLOCK_LEAD_CAP_MS + 1, now), Err(now + PALW_CLOCK_LEAD_CAP_MS));
        assert!(palw_clock_lead_admits_v1(0, now).is_ok(), "history always passes");
        assert!(palw_clock_lead_admits_v1(u64::MAX, u64::MAX).is_ok(), "no overflow at the top");
        // The same stamp is refused now and admitted once wall time reaches `stamp - cap`.
        let stamp = now + 1_000_000;
        assert!(palw_clock_lead_admits_v1(stamp, now).is_err());
        assert!(palw_clock_lead_admits_v1(stamp, stamp - PALW_CLOCK_LEAD_CAP_MS).is_ok());

        let hb = crate::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID;
        let other = hb.wrapping_add(1);
        let cursor = Some(PalwClockCursorV1 { next_slot_ms: 10_000, slots_consumed: 0 });
        let step = PalwClockStepV1 { governs: true, cursor, granted: true, floor: true };
        let holder = PalwClockStepV1 { granted: false, ..step };
        assert!(step.lead_capped(other), "a step of any lane");
        assert!(step.lead_capped(hb), "a beat that steps");
        assert!(holder.lead_capped(hb), "a beat that holds the slot");
        assert!(!holder.lead_capped(other), "a block that moves no clock keeps the full tolerance");
        assert!(!PalwClockStepV1::default().lead_capped(other));

        // The burst: from a reference one interval behind wall time (a slot open NOW), a producer
        // stamps each step at its slot, as fast as it can mint. Under a bound `b` it gets
        // `⌊b / I⌋ + 1` ticks before the next slot opens past `now + b`.
        let burst = |bound: u64| (0u64..).take_while(|k| now + k * I <= now + bound).count() as u64;
        assert_eq!(burst(PALW_CLOCK_LEAD_CAP_MS), 2, "under the cap: two steps, the readiness escalation's margin");
        assert_eq!(burst(1_620_000), 14, "under mainnet's tolerance alone: fourteen");
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
