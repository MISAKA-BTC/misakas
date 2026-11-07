//! **ADR-0133 S1 runtime — resume a V2 segment from the checkpoint at its start.**
//!
//! The protocol half (`palw_verification_v2`) cuts the job's leaves into `K = seats − 1` segments
//! and assigns masks. This module names the window a partial seat actually replays: the leaf range
//! of one segment, mapped onto the decode-call range those leaves live in, so a producer can
//! publish the checkpoint the trace already commits (ADR-0082 D9) and a runtime can restore the
//! KV cache at that boundary (ADR-0119 §7 / the existing checkpoint replay) instead of walking
//! from leaf 0.
//!
//! Segment 0 may include the prefill (`first_call == 0`); that window has no checkpoint before it
//! and is replayed from genesis. Every later segment resumes from the latest committed checkpoint
//! whose `covered_decode_call` is strictly before `first_call`.

use crate::palw_step::{PalwShapeProfileV3, canonical_step_coordinates};
use crate::palw_v2::PalwJobContextV2;
use crate::palw_verification_v2::palw_segment_leaf_range_v2;
use kaspa_hashes::Hash64;

/// The decode-call window a V2 segment occupies, and the leaf range it attests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSegmentResumeWindowV1 {
    pub segment_index: u16,
    pub leaf_start: u64,
    pub leaf_end: u64,
    /// Decode-call of the first leaf in the segment (`0` = prefill).
    pub first_call: u32,
    /// Decode-call of the last leaf in the segment, inclusive.
    pub last_call: u32,
}

impl PalwSegmentResumeWindowV1 {
    pub fn is_empty(self) -> bool {
        self.leaf_end <= self.leaf_start
    }

    /// `true` when this window has no checkpoint before it and must start from the prompt.
    pub fn genesis(self) -> bool {
        self.first_call == 0
    }

    /// How many decode calls a resume from a checkpoint covering `covered` must run to reach
    /// `last_call`. `None` when `covered` is not strictly before the last call (nothing to replay).
    pub fn calls_from_covered(self, covered: u32) -> Option<u32> {
        if self.is_empty() || covered >= self.last_call {
            return None;
        }
        Some(self.last_call - covered)
    }
}

/// A V2 segment-checkpoint pull rides the interval lane under bit 29 (bits 31 and 30 are the
/// block-leaves and held-resume requests). A plain interval index never reaches bit 29.
pub const PALW_SEGMENT_OPENING_REQUEST_BIT_V1: u32 = 1 << 29;

pub fn palw_segment_opening_request_index_v1(seat_count: u16, segment_index: u16) -> u32 {
    PALW_SEGMENT_OPENING_REQUEST_BIT_V1 | (u32::from(seat_count) << 16) | u32::from(segment_index)
}

pub fn palw_segment_opening_request_decode_v1(index: u32) -> Option<(u16, u16)> {
    if index & PALW_SEGMENT_OPENING_REQUEST_BIT_V1 == 0 || index & (1 << 30) != 0 || index & (1 << 31) != 0 {
        return None;
    }
    let rest = index & !PALW_SEGMENT_OPENING_REQUEST_BIT_V1;
    Some(((rest >> 16) as u16, rest as u16))
}

/// The leaf range of segment `index` of `k` over `leaf_count`, mapped onto the decode calls those
/// leaves occupy. `None` when the cut names no such segment, the range is empty, or a leaf is not
/// a canonical step of this job.
pub fn palw_segment_resume_window_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    leaf_count: u64,
    k: u16,
    index: u16,
) -> Option<PalwSegmentResumeWindowV1> {
    let (leaf_start, leaf_end) = palw_segment_leaf_range_v2(leaf_count, k, index)?;
    if leaf_end <= leaf_start {
        return Some(PalwSegmentResumeWindowV1 { segment_index: index, leaf_start, leaf_end, first_call: 0, last_call: 0 });
    }
    let first = canonical_step_coordinates(profile, ctx, leaf_start)?;
    let last = canonical_step_coordinates(profile, ctx, leaf_end - 1)?;
    Some(PalwSegmentResumeWindowV1 {
        segment_index: index,
        leaf_start,
        leaf_end,
        first_call: first.call_index,
        last_call: last.call_index,
    })
}

/// **What a partial seat's segment replay is judged against** (SEAT-S4): the claim's committed
/// roots, read off chain, and the seat's own assignment — never anything an opening says about
/// itself.
///
/// An opening is served by the producer (or anyone relaying it), so every field in it is the
/// server's; the replay is worth a signature only if the opening is tied to THIS claim. The family
/// checks the opening's binding against `execution_root` and `trace_root` (and its job against the
/// job the seat derived), its segment against `(seat_count, segment_index)`, and only then replays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSegmentClaimV1 {
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// The panel's seat count — the cut is `palw_segment_count_v2(seat_count)` segments.
    pub seat_count: u16,
    /// The segment this seat is replaying (a bit of its assigned mask).
    pub segment_index: u16,
}

/// What a seat recomputed for one assigned segment.
///
/// `matches` is whether the leaves the seat recomputed from the claim's committed state root, under
/// the opening's path, to the claim's `step_merkle_root` (SEAT-S4) — an opening that is not the
/// claim's is refused before anything is replayed and never reaches this struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSegmentReplayV1 {
    pub window: PalwSegmentResumeWindowV1,
    /// The leaf hashes the replay recomputed, from its window's first leaf, in leaf order —
    /// diagnostics only, never read by the verdict, and EMPTY when the window recomputed more than
    /// the family keeps (a held segment is tens of millions of leaves).
    pub leaf_hashes: Vec<(u64, Hash64)>,
    /// Steps re-executed: prefill positions and decode calls.
    pub calls_replayed: u32,
    pub matches: bool,
}

impl PalwSegmentReplayV1 {
    /// The hashes that fall inside the attested segment, in leaf order.
    pub fn attested_hashes(&self) -> impl Iterator<Item = (u64, Hash64)> + '_ {
        let (start, end) = (self.window.leaf_start, self.window.leaf_end);
        self.leaf_hashes.iter().copied().filter(move |(i, _)| *i >= start && *i < end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_verification_v2::{palw_segment_assignment_v2, palw_segment_count_v2};

    #[test]
    fn an_empty_segment_is_genesis_and_replays_nothing() {
        let window = PalwSegmentResumeWindowV1 { segment_index: 0, leaf_start: 4, leaf_end: 4, first_call: 0, last_call: 0 };
        assert!(window.is_empty() && window.genesis());
        assert!(window.calls_from_covered(0).is_none());
    }

    #[test]
    fn a_later_segment_replays_only_the_calls_since_the_checkpoint() {
        let window = PalwSegmentResumeWindowV1 { segment_index: 1, leaf_start: 10, leaf_end: 20, first_call: 3, last_call: 6 };
        assert!(!window.genesis());
        assert_eq!(window.calls_from_covered(2), Some(4), "resume after call 2, run through call 6");
        assert!(window.calls_from_covered(6).is_none());
    }

    #[test]
    fn every_assigned_mask_names_a_resume_window_the_cut_owns() {
        let claim = Hash64::from_u64_word(7);
        let anchor = Hash64::from_u64_word(11);
        for seats in 1u16..=8 {
            let assignment = palw_segment_assignment_v2(anchor, claim, seats);
            let k = palw_segment_count_v2(seats);
            assert_eq!(assignment.segments, k);
            for i in 0..k {
                let (start, end) = palw_segment_leaf_range_v2(1_000, k, i).expect("the cut names this segment");
                assert!(end >= start, "segment {i} of {k} is a range");
            }
        }
    }

    #[test]
    fn a_segment_opening_request_is_not_a_plain_interval_or_a_resume() {
        let packed = palw_segment_opening_request_index_v1(5, 3);
        assert_eq!(palw_segment_opening_request_decode_v1(packed), Some((5, 3)));
        assert!(palw_segment_opening_request_decode_v1(3).is_none());
        assert!(palw_segment_opening_request_decode_v1(1 << 30).is_none());
        assert!(palw_segment_opening_request_decode_v1(1 << 31).is_none());
    }
}
