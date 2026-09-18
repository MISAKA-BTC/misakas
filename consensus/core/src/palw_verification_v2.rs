//! **ADR-0133 Verification V2 — S1, segmented replay: the protocol (2026-09-18).**
//!
//! V1 licenses a claim on a quorum of seats that each replayed the WHOLE job. V2 keeps the panel
//! and the court and changes what a receipt attests: the job's leaves are cut into `K = seats − 1`
//! segments; the panel anchor names one FULL-REPLAY seat and gives every other seat two segments
//! (its own and the next, so each segment has two partial attestations beside the full seat's); and
//! a claim licenses when the receipt set carries the V1 quorum AND every segment is attested `Valid`
//! by at least [`PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT`] receipts. A receipt that names no
//! segments (a V1 receipt, or a V3 receipt with the full mask) is a full attestation, so a receipt
//! set of full attestations licenses exactly as V1 does — which is what makes the fence safe to cross
//! before every seat replays by segments, and what lets a fleet adopt segment replay seat by seat.
//!
//! The order the operator set (2026-09-18): V1 full replay → **S1 segmented replay** → S3 (layer
//! sampling) if needed → S2 (optimistic licensing) if speed wins. No zero-knowledge proof (S4) is
//! used or planned; PALW is built on the premise that verification is re-execution.
//!
//! **What this module does NOT do (S1's second half, ADR-0133 §11):** make a partial seat's replay
//! cheaper. Today a seat can only replay a job from its first leaf, so attesting segment `i` costs
//! the prefix up to its end; the capacity gain the ADR counts needs the producer to publish the
//! checkpoint at each segment's start (the residual stream and the KV cache at that boundary, under
//! a commitment the trace carries) and a runtime that resumes from one (ADR-0119 §7). The
//! assignment, the masks and the licensing rule here are what those plug into; until they land a
//! seat attests the full mask and V2 is byte-for-byte V1 at the licence.
use kaspa_hashes::Hash64;

/// How many `Valid` attestations every segment needs before a claim licenses under V2. Two: the
/// full seat and one partial seat, or two partial seats when the full seat is late.
pub const PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT: u16 = 2;
/// Segments a partial seat is assigned: its own and the next one round, so every segment has two
/// partial attestations available beside the full seat's.
pub const PALW_VERIFICATION_V2_SEGMENTS_PER_PARTIAL_SEAT: u16 = 2;
/// The mask is a `u32`, so a panel is cut into at most this many segments.
pub const PALW_VERIFICATION_V2_MAX_SEGMENTS: u16 = 32;
const PALW_VERIFICATION_V2_ASSIGNMENT_DOMAIN: &[u8] = b"misaka-palw/verification-v2/assignment/v1";

/// The segments a receipt attests, one bit a segment (bit `i` = segment `i`). The full mask over
/// `k` segments is a full attestation; a V1 receipt reads as one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSegmentMaskV2(pub u32);

impl PalwSegmentMaskV2 {
    pub const NONE: Self = Self(0);

    /// Every segment of a `k`-segment cut.
    pub fn full(k: u16) -> Self {
        if k >= 32 { Self(u32::MAX) } else { Self((1u32 << k) - 1) }
    }

    pub fn single(index: u16) -> Self {
        if index >= 32 { Self::NONE } else { Self(1u32 << index) }
    }

    pub fn covers(self, index: u16) -> bool {
        index < 32 && self.0 & (1u32 << index) != 0
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether this mask attests every one of `k` segments.
    pub fn is_full(self, k: u16) -> bool {
        let full = Self::full(k).0;
        self.0 & full == full
    }

    /// How many of the `k` segments this mask attests.
    pub fn count(self, k: u16) -> u16 {
        (self.0 & Self::full(k).0).count_ones() as u16
    }
}

/// `K = seats − 1`: one seat replays whole, the rest split the job. One seat is one segment.
pub fn palw_segment_count_v2(seat_count: u16) -> u16 {
    seat_count.saturating_sub(1).clamp(1, PALW_VERIFICATION_V2_MAX_SEGMENTS)
}

/// The leaf range `[start, end)` of segment `index` of `k` over `leaf_count` leaves: equal ranges,
/// the first `leaf_count mod k` one leaf longer, so every leaf is in exactly one segment.
pub fn palw_segment_leaf_range_v2(leaf_count: u64, k: u16, index: u16) -> Option<(u64, u64)> {
    if k == 0 || index >= k {
        return None;
    }
    let (k, i) = (k as u64, index as u64);
    let base = leaf_count / k;
    let rem = leaf_count % k;
    let start = i * base + i.min(rem);
    let len = base + u64::from(i < rem);
    Some((start, start + len))
}

/// What the panel anchor drew for a claim: the full-replay seat and every seat's mask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSegmentAssignmentV2 {
    pub segments: u16,
    pub full_seat: u16,
    /// Indexed by the seat's position in the panel.
    pub masks: Vec<PalwSegmentMaskV2>,
}

impl PalwSegmentAssignmentV2 {
    pub fn mask_of(&self, seat_index: u16) -> PalwSegmentMaskV2 {
        self.masks.get(seat_index as usize).copied().unwrap_or(PalwSegmentMaskV2::NONE)
    }
}

fn assignment_draw(anchor: Hash64, claim_id: Hash64, seat_count: u16) -> u64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_VERIFICATION_V2_ASSIGNMENT_DOMAIN).to_state();
    state.update(anchor.as_bytes().as_slice());
    state.update(claim_id.as_bytes().as_slice());
    state.update(&seat_count.to_le_bytes());
    let digest = state.finalize();
    u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("eight bytes"))
}

/// **The assignment is a function of the bind, not of the seats' choosing.** The full seat is drawn
/// from the anchor; the partial seats, in panel order, take segments `(j + rotation, j + rotation + 1)
/// mod K` with the rotation drawn too, so which segment a given seat sees is not knowable before the
/// panel is bound and every segment has exactly two partial holders (`K = seats − 1`).
pub fn palw_segment_assignment_v2(anchor: Hash64, claim_id: Hash64, seat_count: u16) -> PalwSegmentAssignmentV2 {
    let k = palw_segment_count_v2(seat_count);
    let draw = assignment_draw(anchor, claim_id, seat_count);
    let full_seat = if seat_count == 0 { 0 } else { (draw % seat_count as u64) as u16 };
    let rotation = ((draw >> 32) % k as u64) as u16;
    let mut masks = Vec::with_capacity(seat_count as usize);
    let mut ordinal: u16 = 0;
    for seat in 0..seat_count {
        if seat == full_seat {
            masks.push(PalwSegmentMaskV2::full(k));
            continue;
        }
        let first = (ordinal + rotation) % k;
        let mut mask = PalwSegmentMaskV2::single(first);
        for extra in 1..PALW_VERIFICATION_V2_SEGMENTS_PER_PARTIAL_SEAT {
            mask = mask.union(PalwSegmentMaskV2::single((first + extra) % k));
        }
        masks.push(mask);
        ordinal += 1;
    }
    PalwSegmentAssignmentV2 { segments: k, full_seat, masks }
}

/// How many `Valid` attestations each segment received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCoverageV2 {
    pub segments: u16,
    pub attestations: Vec<u16>,
}

impl PalwCoverageV2 {
    /// The first segment with fewer than `need` attestations, as `(segment, have)`.
    pub fn short(&self, need: u16) -> Option<(u16, u16)> {
        self.attestations.iter().enumerate().find(|(_, have)| **have < need).map(|(i, have)| (i as u16, *have))
    }

    pub fn licenses(&self) -> bool {
        self.short(PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT).is_none()
    }
}

/// Count, per segment, the `Valid` receipts whose mask names it. Any seat's attestation counts for
/// the segments it names — the assignment is a seat's DUTY (what it must replay to be paid), not a
/// cap on what it may attest; a seat that replayed more attests more.
pub fn palw_coverage_v2(k: u16, valid_masks: &[PalwSegmentMaskV2]) -> PalwCoverageV2 {
    let k = k.clamp(1, PALW_VERIFICATION_V2_MAX_SEGMENTS);
    let attestations = (0..k).map(|i| valid_masks.iter().filter(|m| m.covers(i)).count().min(u16::MAX as usize) as u16).collect();
    PalwCoverageV2 { segments: k, attestations }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    #[test]
    fn the_segments_partition_the_leaves_and_the_masks_say_what_they_cover() {
        for (leaves, k) in [(10u64, 4u16), (7, 4), (4, 4), (3, 4), (1_000_003, 7), (0, 4), (5, 1)] {
            let mut next = 0u64;
            for i in 0..k {
                let (s, e) = palw_segment_leaf_range_v2(leaves, k, i).unwrap();
                assert_eq!(s, next, "segment {i} of {k} over {leaves} starts where the last ended");
                assert!(e >= s);
                next = e;
            }
            assert_eq!(next, leaves, "…and the last ends at the last leaf");
            assert!(palw_segment_leaf_range_v2(leaves, k, k).is_none(), "no segment past K");
        }
        assert!(palw_segment_leaf_range_v2(10, 0, 0).is_none());
        let full = PalwSegmentMaskV2::full(4);
        assert_eq!(full.0, 0b1111);
        assert!(full.is_full(4) && !full.is_full(5) && full.covers(3) && !full.covers(4));
        assert_eq!(PalwSegmentMaskV2::full(32).0, u32::MAX);
        assert_eq!(PalwSegmentMaskV2::single(2).union(PalwSegmentMaskV2::single(3)).count(4), 2);
        assert_eq!(PalwSegmentMaskV2::single(40), PalwSegmentMaskV2::NONE, "a segment past the mask is nothing");
        assert_eq!(palw_segment_count_v2(5), 4);
        assert_eq!(palw_segment_count_v2(1), 1, "a one-seat panel is one full seat and one segment");
        assert_eq!(palw_segment_count_v2(0), 1);
        assert_eq!(palw_segment_count_v2(200), PALW_VERIFICATION_V2_MAX_SEGMENTS);
    }

    #[test]
    fn the_anchor_assigns_one_full_seat_and_two_partial_holders_a_segment() {
        for n in 0..64u64 {
            let seats = 5u16;
            let a = palw_segment_assignment_v2(h(n), h(1_000 + n), seats);
            assert_eq!(a.segments, 4);
            assert!(a.full_seat < seats);
            assert_eq!(a.masks.len(), seats as usize);
            assert!(a.mask_of(a.full_seat).is_full(4), "the full seat holds every segment");
            for i in 0..seats {
                if i != a.full_seat {
                    assert_eq!(a.mask_of(i).count(4), 2, "a partial seat holds two segments");
                }
            }
            let partial: Vec<_> = (0..seats).filter(|i| *i != a.full_seat).map(|i| a.mask_of(i)).collect();
            let cover = palw_coverage_v2(4, &partial);
            assert_eq!(cover.attestations, vec![2, 2, 2, 2], "every segment has exactly two partial holders: {a:?}");
            assert_eq!(a, palw_segment_assignment_v2(h(n), h(1_000 + n), seats), "deterministic");
            assert_eq!(a.mask_of(9), PalwSegmentMaskV2::NONE, "a seat the panel does not have holds nothing");
        }
        let full_seats: std::collections::BTreeSet<u16> =
            (0..200u64).map(|n| palw_segment_assignment_v2(h(n), h(7), 5).full_seat).collect();
        assert_eq!(full_seats.len(), 5, "over many anchors every seat is the full seat sometimes");
        let one = palw_segment_assignment_v2(h(1), h(2), 1);
        assert_eq!((one.segments, one.full_seat, one.masks), (1, 0, vec![PalwSegmentMaskV2::full(1)]));
        let none = palw_segment_assignment_v2(h(1), h(2), 0);
        assert!(none.masks.is_empty());
    }

    #[test]
    fn coverage_licenses_when_every_segment_has_two_valid_attestations_and_a_full_mask_is_v1() {
        let full = PalwSegmentMaskV2::full(4);
        let (s01, s12, s23, s30) =
            (PalwSegmentMaskV2(0b0011), PalwSegmentMaskV2(0b0110), PalwSegmentMaskV2(0b1100), PalwSegmentMaskV2(0b1001));
        // V1 semantics: three full attestations license (two would too — the V1 quorum is checked beside this).
        assert!(palw_coverage_v2(4, &[full, full, full]).licenses());
        assert!(palw_coverage_v2(4, &[full, full]).licenses());
        assert_eq!(palw_coverage_v2(4, &[full]).short(2), Some((0, 1)), "one full attestation is not two");
        // The assignment's shape: the full seat and the four partial seats.
        assert!(palw_coverage_v2(4, &[full, s01, s12, s23, s30]).licenses());
        // Without the full seat, the four partial seats still cover every segment twice.
        assert!(palw_coverage_v2(4, &[s01, s12, s23, s30]).licenses());
        // The full seat and two partial seats leave a segment with one attestation.
        let c = palw_coverage_v2(4, &[full, s01, s12]);
        assert_eq!(c.attestations, vec![2, 3, 2, 1]);
        assert_eq!(c.short(2), Some((3, 1)));
        assert!(!c.licenses());
        assert_eq!(palw_coverage_v2(4, &[]).attestations, vec![0, 0, 0, 0]);
        assert_eq!(palw_coverage_v2(0, &[full]).segments, 1, "K is at least one");
    }
}
