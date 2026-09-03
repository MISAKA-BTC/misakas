//! **The history dissection — ADR-0082 Decisions 2 and 3: the objects a round carries, the fold
//! the court checks between rounds, and the arithmetic that sizes the protocol.**
//!
//! # What this replaces
//!
//! On the shipped graphs an attention site commits three context-wide rows per position (the
//! scores, the softmax, the requantized probabilities — `attn_heads × n_ctx` lanes each) and the
//! values node opens the whole probability row of its head, so the close of an attention dispute
//! is linear in the context and the job's leaf count is quadratic in it (ADR-0082 §1.2–1.3). The
//! fused attention node (`PalwStepOpKindV1::AttnFused`) commits only its OUTPUT row; what was a
//! committed row becomes a claim made at dispute time and checked by dissection.
//!
//! # The protocol, in the court's own terms
//!
//! A dispute over one output tile of a fused attention leaf, for one head `h` and the tile's
//! `lane_count` lanes of that head, runs as follows.
//!
//! 1. The responder states a ROOT CLAIM over the whole history `0..p`: the row max `m*`, the
//!    exponent sum `S* = Σ_j int_exp(((s_j − m*) << up).clamp(i32::MIN, 0))`, and the value
//!    partials `V*[l] = Σ_j c_j · v_j[l]` for the disputed lanes, where `c_j` is the requantized
//!    probability computed from `(e_j, S*)` exactly as `softmax_shifted` and the probs
//!    requantization compute it. The court checks that `a16_attn_finalize_v1(V*)` reproduces the
//!    opened output tile; a root claim that does not is refused before a round is played.
//! 2. Each round the responder discloses, for each of the `arity` children of the disputed tile
//!    range ([`palw_attn_child_ranges_v1`] — pinned cut points, so both parties name the same
//!    children), a [`PalwAttnRangeClaimV1`]. The court checks the FOLD ([`palw_attn_fold_check_v1`]):
//!    `max = max(max_c)`, `exp_sum = Σ exp_sum_c`, `v_acc = Σ v_acc_c`, all exact integer
//!    operations (ADR-0040 Decision E). A disclosure that does not fold to the parent's claim is a
//!    conviction. The challenger names the child it disagrees with.
//! 3. At the bottom — one history tile of `PALW_ATTN_HISTORY_TILE_V4` positions — the court opens
//!    the head's query slice and the tile's K and V rows and recomputes the tile's triple with the
//!    shipped kernels (`palw_base0_a16::a16_attn_tile_triple_v1`), using the ROOT's `(m*, S*)` for
//!    the exponent and the probability, and compares all three fields.
//!
//! Every claim at every level is computed against the same `(m*, S*)`, so there is no rescaling
//! and no rounding between children: a lie in `m*` is found by the max fold at the tile whose true
//! max exceeds the claim, a lie in `S*` by the sum fold, a lie in the output by the value fold.
//!
//! # Nothing here is armed
//!
//! These are types and arithmetic. The court arm that plays a round, the leg that authenticates a
//! bottom opening and the cost derivation that prices a move are ADR-0082 U-03/U-04's; a consensus
//! path that reached this module before they land would be a bug. `Params::palw_kary_court` is
//! the fence under which the arm is admissible, `None` on every shipped preset.

use borsh::{BorshDeserialize, BorshSerialize};

/// Wire version of every object in this module.
pub const PALW_ATTN_DISSECT_OBJECT_VERSION_V1: u16 = 1;

/// The smallest arity a dissection may declare — binary, the shipped ladder's own shape.
pub const PALW_ATTN_DISSECT_MIN_ARITY: u8 = 2;

/// The largest arity a dissection may declare. Sixty-four children of `(4 + 8 + 8 × lanes)` bytes
/// at a 128-lane tile is 66,560 bytes of children — the last power of two whose disclosure fits
/// one carrier beside its own framing at that tile. At a 256-lane tile the same arity is 132,096
/// bytes and does NOT fit; whether an `(arity, lanes)` pair fits is a derivation
/// ([`palw_attn_dissect_arity_fits_carrier_v1`]) the court's arity derivation must apply for the
/// widest tile the ruleset registers, never a property of the arity alone.
pub const PALW_ATTN_DISSECT_MAX_ARITY: u8 = 64;

/// The most children one round may carry on the wire — the arity cap, restated for decoders.
pub const PALW_ATTN_DISSECT_MAX_CHILDREN: usize = PALW_ATTN_DISSECT_MAX_ARITY as usize;

/// The most lanes one root claim may dispute at once: one output tile of the fused node, which
/// `PalwStepNodeV1::tile_len` bounds at `PALW_STEP_MAX_TILE_LEN` on the graph side; here the wire
/// cap is the widest head dimension the tier registers (`attn_head_dim` 256 on the hybrid).
pub const PALW_ATTN_DISSECT_MAX_LANES: usize = 256;

/// One range's claim — the three quantities a range of history positions contributes, all
/// computed against the root's `(m*, S*)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnRangeClaimV1 {
    /// `max_j s_j` over the range — the requantized score, an A16 code.
    pub max: i32,
    /// `Σ_j e_j` over the range, `e_j = int_exp(((s_j − m*) << up).clamp(i32::MIN, 0))`.
    pub exp_sum: i64,
    /// `Σ_j c_j · v_j[lane]` for each disputed lane, in lane order.
    pub v_acc: Vec<i64>,
}

/// The responder's root claim for one disputed output tile of one head.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnRootClaimV1 {
    pub version: u16,
    /// The query head the disputed tile belongs to.
    pub head: u16,
    /// The first disputed lane WITHIN the head (`0..attn_head_dim`).
    pub lane_first: u16,
    /// How many lanes are disputed — the output tile's width.
    pub lane_count: u16,
    /// How many history positions the site reads: `kv_len` at the disputed position.
    pub history_positions: u32,
    /// `(m*, S*, V*)` over the whole history.
    pub claim: PalwAttnRangeClaimV1,
}

/// One round's disclosure: the children of the disputed range, in the pinned order.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwAttnDissectRoundV1 {
    pub version: u16,
    pub children: Vec<PalwAttnRangeClaimV1>,
}

/// Why a claim, a round or a derivation is refused. Total: every arm is a refusal, never a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwAttnDissectError {
    UnsupportedVersion { got: u16 },
    NoChildren,
    TooManyChildren { got: usize, max: usize },
    LaneCountMismatch { parent: usize, child: usize },
    TooManyLanes { got: usize, max: usize },
    ArityOutOfRange { got: u8 },
    MaxDoesNotFold { claimed: i32, folded: i32 },
    SumDoesNotFold { claimed: i64, folded: i64 },
    ValueDoesNotFold { lane: usize, claimed: i64, folded: i64 },
    Overflow,
}

impl core::fmt::Display for PalwAttnDissectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedVersion { got } => {
                write!(f, "dissection object version {got} is not {PALW_ATTN_DISSECT_OBJECT_VERSION_V1}")
            }
            Self::NoChildren => write!(f, "a dissection round discloses at least one child"),
            Self::TooManyChildren { got, max } => write!(f, "a round discloses {got} children; the arity cap is {max}"),
            Self::LaneCountMismatch { parent, child } => write!(f, "a child claims {child} lanes against the parent's {parent}"),
            Self::TooManyLanes { got, max } => write!(f, "a claim disputes {got} lanes; the cap is {max}"),
            Self::ArityOutOfRange { got } => {
                write!(
                    f,
                    "dissection arity {got} is outside {PALW_ATTN_DISSECT_MIN_ARITY}..={PALW_ATTN_DISSECT_MAX_ARITY} or not a power of two"
                )
            }
            Self::MaxDoesNotFold { claimed, folded } => write!(f, "the children's max folds to {folded}, the parent claims {claimed}"),
            Self::SumDoesNotFold { claimed, folded } => {
                write!(f, "the children's exponent sums fold to {folded}, the parent claims {claimed}")
            }
            Self::ValueDoesNotFold { lane, claimed, folded } => {
                write!(f, "lane {lane}: the children's value partials fold to {folded}, the parent claims {claimed}")
            }
            Self::Overflow => write!(f, "a fold overflowed i64 — no honest history reaches this"),
        }
    }
}

impl std::error::Error for PalwAttnDissectError {}

/// Is `arity` a legal dissection arity — a power of two inside the cap?
pub const fn palw_attn_arity_is_legal_v1(arity: u8) -> bool {
    arity >= PALW_ATTN_DISSECT_MIN_ARITY && arity <= PALW_ATTN_DISSECT_MAX_ARITY && arity.is_power_of_two()
}

/// **The fold** — what the children of a range must sum and max to. Exact integer arithmetic:
/// `max` is the max of the children's maxes, `exp_sum` and every `v_acc` lane are sums. Refuses
/// an empty disclosure, a child with a different lane count, and an `i64` overflow (unreachable
/// for an honest history: `|c · v| < 2^30` and `e ≤ 2^24`, so `2^32` positions fit).
pub fn palw_attn_fold_v1(children: &[PalwAttnRangeClaimV1]) -> Result<PalwAttnRangeClaimV1, PalwAttnDissectError> {
    let Some(first) = children.first() else {
        return Err(PalwAttnDissectError::NoChildren);
    };
    if children.len() > PALW_ATTN_DISSECT_MAX_CHILDREN {
        return Err(PalwAttnDissectError::TooManyChildren { got: children.len(), max: PALW_ATTN_DISSECT_MAX_CHILDREN });
    }
    let lanes = first.v_acc.len();
    if lanes > PALW_ATTN_DISSECT_MAX_LANES {
        return Err(PalwAttnDissectError::TooManyLanes { got: lanes, max: PALW_ATTN_DISSECT_MAX_LANES });
    }
    let mut max = i32::MIN;
    let mut exp_sum: i64 = 0;
    let mut v_acc = vec![0i64; lanes];
    for child in children {
        if child.v_acc.len() != lanes {
            return Err(PalwAttnDissectError::LaneCountMismatch { parent: lanes, child: child.v_acc.len() });
        }
        max = max.max(child.max);
        exp_sum = exp_sum.checked_add(child.exp_sum).ok_or(PalwAttnDissectError::Overflow)?;
        for (acc, v) in v_acc.iter_mut().zip(&child.v_acc) {
            *acc = acc.checked_add(*v).ok_or(PalwAttnDissectError::Overflow)?;
        }
    }
    Ok(PalwAttnRangeClaimV1 { max, exp_sum, v_acc })
}

/// **The check between rounds**: the disclosed children fold to exactly the parent's claim, or
/// the disclosure is refused BY NAME — which field, and both values.
pub fn palw_attn_fold_check_v1(parent: &PalwAttnRangeClaimV1, children: &[PalwAttnRangeClaimV1]) -> Result<(), PalwAttnDissectError> {
    if parent.v_acc.len() > PALW_ATTN_DISSECT_MAX_LANES {
        return Err(PalwAttnDissectError::TooManyLanes { got: parent.v_acc.len(), max: PALW_ATTN_DISSECT_MAX_LANES });
    }
    let folded = palw_attn_fold_v1(children)?;
    if folded.v_acc.len() != parent.v_acc.len() {
        return Err(PalwAttnDissectError::LaneCountMismatch { parent: parent.v_acc.len(), child: folded.v_acc.len() });
    }
    if folded.max != parent.max {
        return Err(PalwAttnDissectError::MaxDoesNotFold { claimed: parent.max, folded: folded.max });
    }
    if folded.exp_sum != parent.exp_sum {
        return Err(PalwAttnDissectError::SumDoesNotFold { claimed: parent.exp_sum, folded: folded.exp_sum });
    }
    for (lane, (claimed, f)) in parent.v_acc.iter().zip(&folded.v_acc).enumerate() {
        if claimed != f {
            return Err(PalwAttnDissectError::ValueDoesNotFold { lane, claimed: *claimed, folded: *f });
        }
    }
    Ok(())
}

/// **The pinned cut points**: a range of `count` tiles starting at `first` is cut into at most
/// `arity` children of `⌈count / arity⌉` tiles each, in order, the last one shorter; children
/// that would be empty are omitted. Both parties derive the same list, so a round names a child
/// by its INDEX in this list and nothing else. A single tile has no children (the bottom).
pub fn palw_attn_child_ranges_v1(first: u64, count: u64, arity: u8) -> Result<Vec<(u64, u64)>, PalwAttnDissectError> {
    if !palw_attn_arity_is_legal_v1(arity) {
        return Err(PalwAttnDissectError::ArityOutOfRange { got: arity });
    }
    if count <= 1 {
        return Ok(Vec::new());
    }
    let width = count.div_ceil(arity as u64);
    let mut out = Vec::with_capacity(arity as usize);
    let mut start = 0u64;
    while start < count {
        let len = width.min(count - start);
        out.push((first + start, len));
        start += len;
    }
    Ok(out)
}

/// **How many rounds a k-ary search over `space` items takes**: the number of times the pinned
/// cut ([`palw_attn_child_ranges_v1`]) must be applied before one item remains. Computed by the
/// cut's own recurrence rather than by a logarithm, so the two cannot disagree; `None` for an
/// illegal arity. `space ≤ 1` is zero rounds.
pub fn palw_kary_rounds_v1(space: u64, arity: u8) -> Option<u32> {
    if !palw_attn_arity_is_legal_v1(arity) {
        return None;
    }
    let mut count = space;
    let mut rounds = 0u32;
    while count > 1 {
        count = count.div_ceil(arity as u64);
        rounds += 1;
    }
    Some(rounds)
}

/// The rounds a dissection over `history_positions` positions takes at `tile` positions a tile.
pub fn palw_attn_dissection_rounds_v1(history_positions: u64, tile: u32, arity: u8) -> Option<u32> {
    if tile == 0 {
        return None;
    }
    palw_kary_rounds_v1(history_positions.div_ceil(tile as u64), arity)
}

/// **What one round's disclosure weighs**, counted the way `arithmetic_close_bytes_v2` counts:
/// `arity` children of `(4 + 8 + 8 × lanes)` payload bytes plus the object's version and length
/// prefixes. The per-move number the court's window arithmetic and the close ceiling both read.
pub fn palw_attn_dissect_move_bytes_v1(arity: u8, lane_count: usize) -> u64 {
    let per_child = 4u64 + 8 + 8 * lane_count as u64;
    // version (2) + vec length prefix (4) + per child: the fields plus the v_acc length prefix (4).
    2 + 4 + (arity as u64) * (per_child + 4)
}

/// **Does a round at this arity, disputing this many lanes, fit one carrier?** `carrier_bytes` is
/// the COUNTED budget of one chunk after framing (`palw_close_bytes_for_chunks_v1(1)` on a
/// shipped ruleset). The derivation that picks the court's arity applies this at the widest
/// output tile the ruleset registers: an arity whose round no carrier holds is not a shorter
/// court, it is an unplayable one.
pub fn palw_attn_dissect_arity_fits_carrier_v1(arity: u8, lane_count: usize, carrier_bytes: u64) -> bool {
    palw_attn_arity_is_legal_v1(arity) && palw_attn_dissect_move_bytes_v1(arity, lane_count) <= carrier_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(max: i32, exp_sum: i64, v: &[i64]) -> PalwAttnRangeClaimV1 {
        PalwAttnRangeClaimV1 { max, exp_sum, v_acc: v.to_vec() }
    }

    /// **The fold is a max and two sums, and the check names the field that does not fold.**
    #[test]
    fn the_fold_is_exact_and_the_check_names_its_refusal() {
        let a = claim(5, 100, &[1, -2, 3]);
        let b = claim(-7, 250, &[10, 20, -30]);
        let c = claim(9, 1, &[0, 0, 1]);
        let folded = palw_attn_fold_v1(&[a.clone(), b.clone(), c.clone()]).expect("folds");
        assert_eq!(folded, claim(9, 351, &[11, 18, -26]));
        palw_attn_fold_check_v1(&folded, &[a.clone(), b.clone(), c.clone()]).expect("the honest parent folds");

        let mut lied = folded.clone();
        lied.max = 10;
        assert_eq!(
            palw_attn_fold_check_v1(&lied, &[a.clone(), b.clone(), c.clone()]),
            Err(PalwAttnDissectError::MaxDoesNotFold { claimed: 10, folded: 9 })
        );
        let mut lied = folded.clone();
        lied.exp_sum += 1;
        assert_eq!(
            palw_attn_fold_check_v1(&lied, &[a.clone(), b.clone(), c.clone()]),
            Err(PalwAttnDissectError::SumDoesNotFold { claimed: 352, folded: 351 })
        );
        let mut lied = folded.clone();
        lied.v_acc[1] = 19;
        assert_eq!(
            palw_attn_fold_check_v1(&lied, &[a.clone(), b, c]),
            Err(PalwAttnDissectError::ValueDoesNotFold { lane: 1, claimed: 19, folded: 18 })
        );
        // Structural refusals come before arithmetic ones.
        assert_eq!(palw_attn_fold_v1(&[]), Err(PalwAttnDissectError::NoChildren));
        assert_eq!(
            palw_attn_fold_v1(&[a.clone(), claim(0, 0, &[1])]),
            Err(PalwAttnDissectError::LaneCountMismatch { parent: 3, child: 1 })
        );
        assert_eq!(palw_attn_fold_v1(&[claim(0, i64::MAX, &[]), claim(0, 1, &[])]), Err(PalwAttnDissectError::Overflow));
        let too_many: Vec<_> = (0..=PALW_ATTN_DISSECT_MAX_CHILDREN).map(|_| a.clone()).collect();
        assert_eq!(
            palw_attn_fold_v1(&too_many),
            Err(PalwAttnDissectError::TooManyChildren {
                got: PALW_ATTN_DISSECT_MAX_CHILDREN + 1,
                max: PALW_ATTN_DISSECT_MAX_CHILDREN
            })
        );
    }

    /// **The cut covers the range exactly once, in order, and a single tile has no children.**
    #[test]
    fn the_pinned_cut_covers_the_range_exactly_once() {
        for arity in [2u8, 4, 16, 64] {
            for count in [1u64, 2, 3, 15, 16, 17, 100, 8_192, 8_193] {
                let ranges = palw_attn_child_ranges_v1(1_000, count, arity).expect("legal arity");
                if count <= 1 {
                    assert!(ranges.is_empty(), "one tile is the bottom");
                    continue;
                }
                assert!(ranges.len() <= arity as usize, "{count} tiles at arity {arity} gave {} children", ranges.len());
                assert!(ranges.len() >= 2, "a range of {count} tiles must be cut at least in two");
                let mut next = 1_000u64;
                for (start, len) in &ranges {
                    assert_eq!(*start, next, "children are contiguous and in order");
                    assert!(*len >= 1);
                    next += len;
                }
                assert_eq!(next, 1_000 + count, "the children cover the range exactly");
            }
        }
        assert_eq!(palw_attn_child_ranges_v1(0, 10, 3), Err(PalwAttnDissectError::ArityOutOfRange { got: 3 }));
        assert_eq!(palw_attn_child_ranges_v1(0, 10, 128), Err(PalwAttnDissectError::ArityOutOfRange { got: 128 }));
    }

    /// **The round count is the ADR's table**: 2^32 in 32 binary rounds and 8 at sixteen; 8,192
    /// tiles (131,072 positions at a 16-position tile) in 13 and 4; and the recurrence agrees with
    /// `⌈log₂ space / log₂ arity⌉` at every power of two.
    #[test]
    fn the_round_count_is_the_adrs_table() {
        assert_eq!(palw_kary_rounds_v1(1 << 32, 2), Some(32));
        assert_eq!(palw_kary_rounds_v1(1 << 32, 16), Some(8));
        assert_eq!(palw_attn_dissection_rounds_v1(131_072, 16, 2), Some(13));
        assert_eq!(palw_attn_dissection_rounds_v1(131_072, 16, 16), Some(4));
        assert_eq!(palw_attn_dissection_rounds_v1(512, 16, 2), Some(5));
        assert_eq!(palw_attn_dissection_rounds_v1(512, 16, 16), Some(2));
        assert_eq!(palw_attn_dissection_rounds_v1(16, 16, 16), Some(0), "one tile is the bottom");
        assert_eq!(palw_attn_dissection_rounds_v1(17, 16, 16), Some(1));
        assert_eq!(palw_kary_rounds_v1(10, 3), None);
        assert_eq!(palw_attn_dissection_rounds_v1(10, 0, 2), None);
        for log2 in 0..=40u32 {
            let space = 1u64 << log2;
            for (arity, bits) in [(2u8, 1u32), (4, 2), (16, 4), (64, 6)] {
                assert_eq!(palw_kary_rounds_v1(space, arity), Some(log2.div_ceil(bits)), "space 2^{log2} at arity {arity}");
            }
        }
        // The rounds are exactly the cuts: applying the pinned cut that many times reaches one tile.
        let (mut count, arity) = (8_193u64, 16u8);
        let mut applied = 0;
        while count > 1 {
            let ranges = palw_attn_child_ranges_v1(0, count, arity).expect("legal");
            count = ranges.iter().map(|(_, len)| *len).max().expect("non-empty");
            applied += 1;
        }
        assert_eq!(Some(applied), palw_kary_rounds_v1(8_193, arity));
    }

    /// **A move's bytes are the ADR's numbers, and which `(arity, lanes)` pairs fit one carrier is a
    /// derivation** — every legal arity fits at a 128-lane tile; at 256 lanes sixty-four does not,
    /// which is why the court's arity derivation applies the bound at the widest registered tile.
    #[test]
    fn a_moves_bytes_are_derived_and_the_carrier_bound_is_a_pair() {
        // The ADR's table: sixteen children at a 64-lane tile, 8,384 bytes of child payload.
        let payload = 16u64 * (4 + 8 + 8 * 64);
        assert_eq!(payload, 8_384);
        assert_eq!(palw_attn_dissect_move_bytes_v1(16, 64), 2 + 4 + 16 * (4 + 8 + 8 * 64 + 4));
        // One framed carrier, counted: 100,000 × 10 / 12.
        let carrier = 100_000u64 * 10 / 12;
        for arity in [2u8, 4, 8, 16, 32, 64] {
            assert!(palw_attn_dissect_arity_fits_carrier_v1(arity, 128, carrier), "arity {arity} at 128 lanes must fit one carrier");
        }
        for arity in [2u8, 4, 8, 16, 32] {
            assert!(
                palw_attn_dissect_arity_fits_carrier_v1(arity, PALW_ATTN_DISSECT_MAX_LANES, carrier),
                "arity {arity} at 256 lanes"
            );
        }
        assert!(
            !palw_attn_dissect_arity_fits_carrier_v1(64, PALW_ATTN_DISSECT_MAX_LANES, carrier),
            "sixty-four children of 256 lanes are {} bytes — the derivation must refuse that pair",
            palw_attn_dissect_move_bytes_v1(64, PALW_ATTN_DISSECT_MAX_LANES)
        );
        assert!(!palw_attn_dissect_arity_fits_carrier_v1(3, 8, carrier), "an illegal arity never fits");
        // The objects round-trip on the wire.
        let round = PalwAttnDissectRoundV1 {
            version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            children: vec![claim(1, 2, &[3, 4]), claim(-1, 0, &[0, 0])],
        };
        let bytes = borsh::to_vec(&round).expect("serializes");
        assert_eq!(borsh::from_slice::<PalwAttnDissectRoundV1>(&bytes).expect("decodes"), round);
        let root = PalwAttnRootClaimV1 {
            version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            head: 3,
            lane_first: 64,
            lane_count: 2,
            history_positions: 512,
            claim: claim(7, 99, &[1, 2]),
        };
        let bytes = borsh::to_vec(&root).expect("serializes");
        assert_eq!(borsh::from_slice::<PalwAttnRootClaimV1>(&bytes).expect("decodes"), root);
    }
}
