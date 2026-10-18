//! **History-tile bisection for one attention reduction — ADR-0191 (2). DORMANT: pure functions,
//! no fence, no consensus wiring.**
//!
//! # What is disputed
//!
//! The attention reduction of one head at one step is a sum over history tiles of
//! [`PalwAttnRangeClaimV1`] triples `(max, exp_sum, v_acc)` computed against the common root
//! `(m*, S*)`. Two parties, the responder and the challenger, each commit a vector of per-tile
//! partials and a stated root. The combine of partials is the existing exact integer fold
//! ([`palw_attn_fold_v1`], the function the executor folds its own tiles with, see
//! [`a16_attn_root_claim_v1`]). The per-tile partial is the existing A16 tile kernel
//! ([`a16_attn_tile_triple_v1`]). No arithmetic is introduced here.
//!
//! # The protocol
//!
//! 1. **Combine check (round 0).** Each party's stated root must fold from its own disclosure of
//!    the whole range. A mismatch is refused by name (`max`, `exp_sum`, or a value lane).
//! 2. **Bisection.** While the range has more than one tile, each party discloses its `b = 16`
//!    children in the pinned order ([`palw_attn_child_ranges_v1`]). Each disclosure must fold to
//!    that party's current claim for the range, else the party is refused by name. The challenger
//!    descends into the FIRST child whose two claims differ.
//! 3. **Terminal.** The range is one tile. The caller recomputes that tile from its authenticated
//!    operands ([`TileOperandsV1`]: the query row and the tile's K and V rows) with
//!    [`adjudicate_terminal_v1`] and convicts each party whose claim is not the recomputation.
//!
//! The tile space is padded with identity partials `(i32::MIN, 0, 0)` to the smallest `16^R >= n`.
//! Every disagreement then walks exactly `R = ⌈log16 n⌉` rounds, whichever tile it ends at. An
//! unpadded cut would end a short last child early and make the depth depend on the tile.
//!
//! # Commitments
//!
//! Each round's disclosure is committed by a 64-byte BLAKE2b digest over its canonical bytes. The
//! digest identifies the data and binds the transcript. It is never evidence that the reduction
//! was computed correctly: correctness is decided only by the terminal recomputation (PRINCIPLES §8).
//!
//! # Terminal read set
//!
//! The terminal reads the query row, the tile's K and V rows, and the round commitments, nothing
//! else. [`TileOperandsV1::read_bytes`] counts exactly the operands, at 2 bytes per A16 code.

use crate::palw_attn_dissect::{
    PALW_ATTN_DISSECT_MAX_CHILDREN, PalwAttnDissectError, PalwAttnRangeClaimV1, palw_attn_child_ranges_v1, palw_attn_fold_check_v1,
    palw_attn_fold_v1,
};
use crate::palw_base0_a16::{A16AttnFusedParamsV1, PalwA16OpError, a16_attn_tile_triple_v1};
use crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4;

/// Wire version of the objects in this module.
pub const PALW_TIR_HISTORY_BISECTION_OBJECT_VERSION_V1: u16 = 1;

/// The bisection arity `b` of ADR-0191 (1).
pub const PALW_TIR_HISTORY_BISECTION_ARITY_V1: u8 = 16;

/// Domain tag of a round commitment.
pub const PALW_TIR_HISTORY_BISECTION_ROUND_DOMAIN_V1: &[u8] = b"misaka/palw/tir-history-bisection/round/v1";

/// Bytes per A16 code on the wire (codes are 16-bit, `A16_CODE_MAX = i16::MAX`).
pub const PALW_TIR_HISTORY_BISECTION_CODE_BYTES_V1: u64 = 2;

/// Bytes of one round commitment (BLAKE2b-512).
pub const PALW_TIR_HISTORY_BISECTION_COMMITMENT_BYTES_V1: u64 = 64;

/// Which of the two parties a verdict or a refusal names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalwTirHistoryPartyV1 {
    Responder,
    Challenger,
}

/// Why the bisection refuses a disclosure, names a party, or finds nothing to localise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTirHistoryBisectionError {
    /// No history tiles at all.
    NoTiles,
    /// A party's claim or disclosure does not fold, at the given round (0 = the whole-range combine).
    Refused { party: PalwTirHistoryPartyV1, round: u32, error: PalwAttnDissectError },
    /// A party disclosed a different number of children than the pinned cut has.
    ChildCountMismatch { party: PalwTirHistoryPartyV1, round: u32, expected: usize, got: usize },
    /// Both disclosures fold to their claims, yet every child agrees. Unreachable for consistent claims.
    NoChildDisagreement { round: u32 },
    /// The terminal tile lies in the identity padding, so the parties cannot disagree there.
    TerminalInPadding { tile: u64 },
    /// The tile index is past the history.
    TileOutOfRange { tile: u64, tiles: u64 },
    /// The operands do not describe a well-formed tile.
    Operands(PalwA16OpError),
    /// A series that is not a whole number of rows.
    RowsNotAligned { len: usize, kv_dim: usize },
}

impl core::fmt::Display for PalwTirHistoryBisectionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoTiles => write!(f, "the reduction has no history tiles"),
            Self::Refused { party, round, error } => write!(f, "{party:?} refused at round {round}: {error}"),
            Self::ChildCountMismatch { party, round, expected, got } => {
                write!(f, "{party:?} disclosed {got} children at round {round}; the pinned cut has {expected}")
            }
            Self::NoChildDisagreement { round } => write!(f, "round {round}: both disclosures fold, yet no child disagrees"),
            Self::TerminalInPadding { tile } => write!(f, "tile {tile} is identity padding; the parties cannot disagree there"),
            Self::TileOutOfRange { tile, tiles } => write!(f, "tile {tile} is past the {tiles} history tiles"),
            Self::Operands(e) => write!(f, "tile operands are malformed: {e:?}"),
            Self::RowsNotAligned { len, kv_dim } => write!(f, "a series of {len} codes is not a whole number of {kv_dim}-wide rows"),
        }
    }
}

impl std::error::Error for PalwTirHistoryBisectionError {}

/// The number of rounds a bisection over `tiles` tiles runs: `⌈log16 tiles⌉`, by the recurrence
/// `c ← ⌈c / 16⌉` (the recurrence the pinned cut uses). `0` for one tile.
pub fn palw_tir_history_rounds_v1(tiles: u64) -> u32 {
    let mut count = tiles;
    let mut rounds = 0u32;
    while count > 1 {
        count = count.div_ceil(PALW_TIR_HISTORY_BISECTION_ARITY_V1 as u64);
        rounds += 1;
    }
    rounds
}

/// The identity partial of a padding tile: the fold's neutral element, for `lanes` value lanes.
fn identity_partial_v1(lanes: usize) -> PalwAttnRangeClaimV1 {
    PalwAttnRangeClaimV1 { max: i32::MIN, exp_sum: 0, v_acc: vec![0; lanes] }
}

/// The fold of the partials of `tiles[first .. first + count]` (padded space: the identity beyond
/// the history), folded in arity-sized groups as the court's rounds would fold them.
pub fn palw_tir_history_fold_range_v1(
    tiles: &[PalwAttnRangeClaimV1],
    first: u64,
    count: u64,
) -> Result<PalwAttnRangeClaimV1, PalwAttnDissectError> {
    let Some(lanes) = tiles.first().map(|t| t.v_acc.len()) else {
        return Err(PalwAttnDissectError::NoChildren);
    };
    let items: Vec<PalwAttnRangeClaimV1> = (first..first + count)
        .map(|i| match tiles.get(i as usize) {
            Some(t) => t.clone(),
            None => identity_partial_v1(lanes),
        })
        .collect();
    let mut level = items.chunks(PALW_ATTN_DISSECT_MAX_CHILDREN).map(palw_attn_fold_v1).collect::<Result<Vec<_>, _>>()?;
    while level.len() > 1 {
        level = level.chunks(PALW_ATTN_DISSECT_MAX_CHILDREN).map(palw_attn_fold_v1).collect::<Result<Vec<_>, _>>()?;
    }
    level.pop().ok_or(PalwAttnDissectError::NoChildren)
}

/// The honest disclosure of one range: its children in the pinned order, each folded from the
/// party's own partials. `(first, count)` is in padded tile space.
pub fn palw_tir_history_disclose_v1(
    tiles: &[PalwAttnRangeClaimV1],
    first: u64,
    count: u64,
) -> Result<Vec<PalwAttnRangeClaimV1>, PalwAttnDissectError> {
    palw_attn_child_ranges_v1(first, count, PALW_TIR_HISTORY_BISECTION_ARITY_V1)?
        .into_iter()
        .map(|(f, c)| palw_tir_history_fold_range_v1(tiles, f, c))
        .collect()
}

/// The 64-byte commitment of one round's disclosure: domain, version, the range, then every
/// child's `max`, `exp_sum` and value lanes in little-endian.
pub fn palw_tir_history_round_commitment_v1(first: u64, count: u64, children: &[PalwAttnRangeClaimV1]) -> [u8; 64] {
    let mut buf = Vec::new();
    buf.extend_from_slice(&PALW_TIR_HISTORY_BISECTION_OBJECT_VERSION_V1.to_le_bytes());
    buf.extend_from_slice(&first.to_le_bytes());
    buf.extend_from_slice(&count.to_le_bytes());
    buf.extend_from_slice(&(children.len() as u32).to_le_bytes());
    for c in children {
        buf.extend_from_slice(&c.max.to_le_bytes());
        buf.extend_from_slice(&c.exp_sum.to_le_bytes());
        buf.extend_from_slice(&(c.v_acc.len() as u32).to_le_bytes());
        for v in &c.v_acc {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_TIR_HISTORY_BISECTION_ROUND_DOMAIN_V1).to_state();
    state.update(&buf);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// One party's view for the bisection: its stated root and its disclosure function. The function
/// returns the children of a PADDED range in the pinned order. An honest party discloses
/// [`palw_tir_history_disclose_v1`] of its own partials. A dishonest one may return anything, and
/// the bisection refuses it by name.
pub struct PalwTirHistoryPartyV1View<'a> {
    pub root: PalwAttnRangeClaimV1,
    pub disclose: &'a dyn Fn(u64, u64) -> Result<Vec<PalwAttnRangeClaimV1>, PalwAttnDissectError>,
}

/// What the bisection localised: one tile, the two claims on it, and the round transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirHistoryLocalisedV1 {
    pub version: u16,
    /// The disputed tile, in history order.
    pub tile: u64,
    /// The rounds played. Always `palw_tir_history_rounds_v1(tiles)`.
    pub rounds: u32,
    /// Each round's commitments, responder then challenger.
    pub commitments: Vec<[u8; 64]>,
    /// The responder's and the challenger's claim on the disputed tile.
    pub claims: [PalwAttnRangeClaimV1; 2],
}

/// The outcome of the bisection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTirHistoryVerdictV1 {
    /// The two stated roots are equal: nothing to localise.
    Agree,
    /// A disagreement narrowed to one tile.
    Localised(PalwTirHistoryLocalisedV1),
}

/// **Run the bisection.** Checks the combine (round 0), then bisects with `b = 16` until one tile
/// remains. Returns the localised tile and the claims on it. The caller adjudicates that tile with
/// [`adjudicate_terminal_v1`]. `n_tiles` is the number of real history tiles (padding excluded).
pub fn palw_tir_history_bisect_v1(
    n_tiles: u64,
    responder: &PalwTirHistoryPartyV1View<'_>,
    challenger: &PalwTirHistoryPartyV1View<'_>,
) -> Result<PalwTirHistoryVerdictV1, PalwTirHistoryBisectionError> {
    if n_tiles == 0 {
        return Err(PalwTirHistoryBisectionError::NoTiles);
    }
    let rounds_total = palw_tir_history_rounds_v1(n_tiles);
    let padded = (PALW_TIR_HISTORY_BISECTION_ARITY_V1 as u64).pow(rounds_total);
    let parties = [(PalwTirHistoryPartyV1::Responder, responder), (PalwTirHistoryPartyV1::Challenger, challenger)];

    // Round 0: each stated root must fold from its own whole-range disclosure.
    if padded > 1 {
        for (party, view) in parties {
            let children =
                (view.disclose)(0, padded).map_err(|error| PalwTirHistoryBisectionError::Refused { party, round: 0, error })?;
            palw_attn_fold_check_v1(&view.root, &children).map_err(|error| PalwTirHistoryBisectionError::Refused { party, round: 0, error })?;
        }
    }
    if responder.root == challenger.root {
        return Ok(PalwTirHistoryVerdictV1::Agree);
    }

    let (mut first, mut count) = (0u64, padded);
    let mut claims = [responder.root.clone(), challenger.root.clone()];
    let mut commitments = Vec::new();
    let mut round = 0u32;
    while count > 1 {
        round += 1;
        let ranges = palw_attn_child_ranges_v1(first, count, PALW_TIR_HISTORY_BISECTION_ARITY_V1)
            .map_err(|error| PalwTirHistoryBisectionError::Refused { party: PalwTirHistoryPartyV1::Responder, round, error })?;
        let mut disclosed: Vec<Vec<PalwAttnRangeClaimV1>> = Vec::with_capacity(2);
        for (i, (party, view)) in parties.iter().enumerate() {
            let children =
                (view.disclose)(first, count).map_err(|error| PalwTirHistoryBisectionError::Refused { party: *party, round, error })?;
            if children.len() != ranges.len() {
                return Err(PalwTirHistoryBisectionError::ChildCountMismatch {
                    party: *party,
                    round,
                    expected: ranges.len(),
                    got: children.len(),
                });
            }
            palw_attn_fold_check_v1(&claims[i], &children)
                .map_err(|error| PalwTirHistoryBisectionError::Refused { party: *party, round, error })?;
            commitments.push(palw_tir_history_round_commitment_v1(first, count, &children));
            disclosed.push(children);
        }
        let Some(index) = (0..ranges.len()).find(|&k| disclosed[0][k] != disclosed[1][k]) else {
            return Err(PalwTirHistoryBisectionError::NoChildDisagreement { round });
        };
        let (next_first, next_count) = ranges[index];
        claims = [disclosed[0][index].clone(), disclosed[1][index].clone()];
        first = next_first;
        count = next_count;
    }
    if first >= n_tiles {
        return Err(PalwTirHistoryBisectionError::TerminalInPadding { tile: first });
    }
    debug_assert_eq!(round, rounds_total);
    Ok(PalwTirHistoryVerdictV1::Localised(PalwTirHistoryLocalisedV1 {
        version: PALW_TIR_HISTORY_BISECTION_OBJECT_VERSION_V1,
        tile: first,
        rounds: round,
        commitments,
        claims,
    }))
}

/// The operands of ONE history tile: the query row, and the tile's K and V rows in full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileOperandsV1 {
    pub tile: u64,
    pub q: Vec<i32>,
    pub k_tile: Vec<i32>,
    pub v_tile: Vec<i32>,
    pub kv_dim: usize,
    pub kv_off: usize,
    pub lanes: (usize, usize),
    pub params: A16AttnFusedParamsV1,
}

impl TileOperandsV1 {
    /// Bytes the terminal reads for this tile: the query row plus the tile's K and V rows.
    pub fn read_bytes(&self) -> u64 {
        (self.q.len() + self.k_tile.len() + self.v_tile.len()) as u64 * PALW_TIR_HISTORY_BISECTION_CODE_BYTES_V1
    }

    /// The whole terminal read set: the tile's operands plus one commitment per party per round.
    pub fn read_bytes_with_rounds(&self, rounds: u32) -> u64 {
        self.read_bytes() + rounds as u64 * 2 * PALW_TIR_HISTORY_BISECTION_COMMITMENT_BYTES_V1
    }
}

/// Cut the operands of tile `tile` out of the full K and V series (position-major rows of `kv_dim`
/// codes). Touches only that tile's rows.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_history_tile_operands_v1(
    q: &[i32],
    k_series: &[i32],
    v_series: &[i32],
    kv_dim: usize,
    kv_off: usize,
    lanes: (usize, usize),
    params: A16AttnFusedParamsV1,
    tile: u64,
) -> Result<TileOperandsV1, PalwTirHistoryBisectionError> {
    if kv_dim == 0 || k_series.len() % kv_dim != 0 {
        return Err(PalwTirHistoryBisectionError::RowsNotAligned { len: k_series.len(), kv_dim });
    }
    if v_series.len() != k_series.len() {
        return Err(PalwTirHistoryBisectionError::RowsNotAligned { len: v_series.len(), kv_dim });
    }
    let h = PALW_ATTN_HISTORY_TILE_V4 as usize;
    let positions = k_series.len() / kv_dim;
    let tiles = positions.div_ceil(h) as u64;
    if tile >= tiles {
        return Err(PalwTirHistoryBisectionError::TileOutOfRange { tile, tiles });
    }
    let start = tile as usize * h;
    let end = (start + h).min(positions);
    Ok(TileOperandsV1 {
        tile,
        q: q.to_vec(),
        k_tile: k_series[start * kv_dim..end * kv_dim].to_vec(),
        v_tile: v_series[start * kv_dim..end * kv_dim].to_vec(),
        kv_dim,
        kv_off,
        lanes,
        params,
    })
}

/// The terminal adjudication: recompute the disputed tile's partial from its operands against the
/// root `(m*, S*)` and compare each party's claim with it, in every field. Returns the parties
/// whose claim is NOT the recomputation (the convicted), in party order.
pub fn adjudicate_terminal_v1(
    operands: &TileOperandsV1,
    m_star: i32,
    s_star: i64,
    claims: &[PalwAttnRangeClaimV1; 2],
) -> Result<Vec<PalwTirHistoryPartyV1>, PalwTirHistoryBisectionError> {
    let recomputed = a16_attn_tile_triple_v1(
        &operands.q,
        &operands.k_tile,
        &operands.v_tile,
        operands.kv_dim,
        operands.kv_off,
        operands.lanes,
        operands.params,
        m_star,
        s_star,
    )
    .map_err(PalwTirHistoryBisectionError::Operands)?;
    let mut convicted = Vec::new();
    if claims[0] != recomputed {
        convicted.push(PalwTirHistoryPartyV1::Responder);
    }
    if claims[1] != recomputed {
        convicted.push(PalwTirHistoryPartyV1::Challenger);
    }
    Ok(convicted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_a16::{A16QuantParams, a16_attn_finalize_v1, a16_attn_root_claim_v1};

    const TILE: usize = PALW_ATTN_HISTORY_TILE_V4 as usize;

    fn params() -> A16AttnFusedParamsV1 {
        A16AttnFusedParamsV1 {
            scores: A16QuantParams { multiplier: 1 << 10, shift: 30, zero: 3 },
            probs: A16QuantParams { multiplier: 1 << 15, shift: 24, zero: 0 },
            values: A16QuantParams { multiplier: 1, shift: 22, zero: -5 },
            up_bits: 2,
        }
    }

    fn codes(n: usize, seed: u64) -> Vec<i32> {
        let mut state = seed | 1;
        (0..n)
            .map(|_| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                ((state >> 33) % 65_535) as i32 - 32_767
            })
            .collect()
    }

    /// One head of one step: the query, the K and V series, the root and the honest per-tile partials.
    struct Step {
        q: Vec<i32>,
        k: Vec<i32>,
        v: Vec<i32>,
        kv_dim: usize,
        kv_off: usize,
        lanes: (usize, usize),
        root: PalwAttnRangeClaimV1,
        tiles: Vec<PalwAttnRangeClaimV1>,
        n: u64,
    }

    /// The fixture geometry: 2 kv heads of 8 lanes; the query head reads the second slice.
    fn step(positions: usize, seed: u64) -> Step {
        let (d_head, kv_dim) = (8usize, 16usize);
        let (kv_off, lanes) = (8usize, (2usize, 5usize));
        let q = codes(d_head, seed);
        let k = codes(positions * kv_dim, seed ^ 0xa5a5);
        let v = codes(positions * kv_dim, seed ^ 0x5a5a);
        let root = a16_attn_root_claim_v1(&q, &k, &v, kv_dim, kv_off, lanes, params(), TILE).expect("an honest root");
        let (m, s) = (root.max, root.exp_sum);
        let mut tiles = Vec::new();
        let mut start = 0usize;
        while start < positions {
            let end = (start + TILE).min(positions);
            tiles.push(
                a16_attn_tile_triple_v1(
                    &q,
                    &k[start * kv_dim..end * kv_dim],
                    &v[start * kv_dim..end * kv_dim],
                    kv_dim,
                    kv_off,
                    lanes,
                    params(),
                    m,
                    s,
                )
                .expect("an honest tile"),
            );
            start = end;
        }
        let n = tiles.len() as u64;
        Step { q, k, v, kv_dim, kv_off, lanes, root, tiles, n }
    }

    /// The root that folds from `tiles` the honest way.
    fn root_of(tiles: &[PalwAttnRangeClaimV1]) -> PalwAttnRangeClaimV1 {
        palw_tir_history_fold_range_v1(tiles, 0, tiles.len() as u64).expect("a fold")
    }

    /// Forge one tile's first value lane.
    fn forged(tiles: &[PalwAttnRangeClaimV1], t: usize) -> Vec<PalwAttnRangeClaimV1> {
        let mut out = tiles.to_vec();
        out[t].v_acc[0] += 1;
        out
    }

    /// Run the bisection with each party's root equal to the fold of its own tiles, then adjudicate.
    fn run(st: &Step, responder_tiles: &[PalwAttnRangeClaimV1], challenger_tiles: &[PalwAttnRangeClaimV1]) -> (PalwTirHistoryVerdictV1, Vec<PalwTirHistoryPartyV1>) {
        let ra = root_of(responder_tiles);
        let rb = root_of(challenger_tiles);
        let da = |f, c| palw_tir_history_disclose_v1(responder_tiles, f, c);
        let db = |f, c| palw_tir_history_disclose_v1(challenger_tiles, f, c);
        let a = PalwTirHistoryPartyV1View { root: ra, disclose: &da };
        let b = PalwTirHistoryPartyV1View { root: rb, disclose: &db };
        let verdict = palw_tir_history_bisect_v1(st.n, &a, &b).expect("a well-formed dispute");
        let convicted = match &verdict {
            PalwTirHistoryVerdictV1::Agree => Vec::new(),
            PalwTirHistoryVerdictV1::Localised(l) => {
                let ops = palw_tir_history_tile_operands_v1(&st.q, &st.k, &st.v, st.kv_dim, st.kv_off, st.lanes, params(), l.tile)
                    .expect("the tile's operands");
                adjudicate_terminal_v1(&ops, st.root.max, st.root.exp_sum, &l.claims).expect("a terminal")
            }
        };
        (verdict, convicted)
    }

    /// Boundary history lengths: every length up to 40, then the tile edges of larger histories.
    fn lengths() -> Vec<usize> {
        let mut v: Vec<usize> = (1..=40).collect();
        v.extend([47, 48, 49, 63, 64, 65, 127, 128, 129, 255, 256, 257, 300, 513]);
        v
    }

    #[test]
    fn rounds_are_ceil_log16_for_the_adr_sizes() {
        for (tiles, want) in [(1u64, 0u32), (16, 1), (17, 2), (256, 2), (1024, 3), (65_536, 4)] {
            assert_eq!(palw_tir_history_rounds_v1(tiles), want, "rounds over {tiles} tiles");
            let r = (0..).find(|&r: &u32| 16u64.pow(r) >= tiles).unwrap();
            assert_eq!(r, want);
        }
    }

    /// (1) An honest committed reduction is never refuted, for every step and every boundary length.
    /// Each case pits the honest responder against a challenger who forges one tile. The forged tile
    /// is the one localised, and only the challenger is convicted.
    #[test]
    fn honest_committed_reduction_is_never_refuted() {
        let mut cases = 0usize;
        for positions in lengths() {
            let st = step(positions, positions as u64 * 7 + 1);
            let (v, c) = run(&st, &st.tiles, &st.tiles);
            assert_eq!(v, PalwTirHistoryVerdictV1::Agree, "H={positions}");
            assert!(c.is_empty());
            for t in 0..st.tiles.len() {
                let liar = forged(&st.tiles, t);
                let (v, c) = run(&st, &st.tiles, &liar);
                match v {
                    PalwTirHistoryVerdictV1::Localised(l) => assert_eq!(l.tile, t as u64, "H={positions} forged tile {t}"),
                    PalwTirHistoryVerdictV1::Agree => panic!("H={positions}: forged tile {t} was not seen"),
                }
                assert_eq!(c, vec![PalwTirHistoryPartyV1::Challenger], "H={positions} tile {t}: only the liar is convicted");
                cases += 1;
            }
        }
        let expected: usize = lengths().iter().map(|h| h.div_ceil(TILE)).sum();
        assert_eq!(cases, expected, "every tile of every length was forged once");
    }

    /// The honest partials fold to the honest root that the A16 kernel finalizes, so the combine is
    /// the executor's own fold.
    #[test]
    fn the_combine_is_the_executors_fold() {
        for positions in lengths() {
            let st = step(positions, positions as u64 + 3);
            assert_eq!(root_of(&st.tiles), st.root, "H={positions}");
            assert_eq!(a16_attn_finalize_v1(&st.root.v_acc, params().values).len(), st.lanes.1);
        }
    }

    /// (2) A forged partial in any one tile is localised to exactly that tile and convicted, at every
    /// position of the history, including the first and the last tile.
    #[test]
    fn a_forged_partial_is_localised_and_convicted() {
        for positions in lengths() {
            let st = step(positions, positions as u64 * 3 + 11);
            for t in 0..st.tiles.len() {
                let liar = forged(&st.tiles, t);
                let (v, c) = run(&st, &liar, &st.tiles);
                let PalwTirHistoryVerdictV1::Localised(l) = v else { panic!("H={positions} tile {t}: no localisation") };
                assert_eq!(l.tile, t as u64, "H={positions}");
                assert_eq!(l.rounds, palw_tir_history_rounds_v1(st.n));
                assert_eq!(c, vec![PalwTirHistoryPartyV1::Responder], "H={positions} tile {t}");
            }
        }
    }

    /// (3) A forged combine is caught at the combine level, by name: correct partials, a wrong total
    /// in the root's max, exponent sum or value lane. A disclosure that lies about a child, with an
    /// honest root, is refused at the round that discloses it.
    #[test]
    fn a_forged_combine_is_caught_at_the_combine_level() {
        for positions in [1usize, 16, 17, 40, 256, 300] {
            let st = step(positions, 99 + positions as u64);
            let honest = |f, c| palw_tir_history_disclose_v1(&st.tiles, f, c);
            let mut bad_max = st.root.clone();
            bad_max.max -= 1;
            let mut bad_sum = st.root.clone();
            bad_sum.exp_sum += 1;
            let mut bad_val = st.root.clone();
            bad_val.v_acc[1] ^= 1;
            for claim in [bad_max, bad_sum, bad_val] {
                let a = PalwTirHistoryPartyV1View { root: claim, disclose: &honest };
                let b = PalwTirHistoryPartyV1View { root: st.root.clone(), disclose: &honest };
                let got = palw_tir_history_bisect_v1(st.n, &a, &b);
                if st.n > 1 {
                    assert!(
                        matches!(got, Err(PalwTirHistoryBisectionError::Refused { party: PalwTirHistoryPartyV1::Responder, round: 0, .. })),
                        "H={positions}: {got:?}"
                    );
                } else {
                    assert!(matches!(got, Ok(PalwTirHistoryVerdictV1::Localised(_))), "H={positions}: {got:?}");
                }
            }
            if st.n > 1 {
                let lying = |f: u64, c: u64| -> Result<Vec<PalwAttnRangeClaimV1>, PalwAttnDissectError> {
                    let mut kids = palw_tir_history_disclose_v1(&st.tiles, f, c)?;
                    kids[0].exp_sum += 1;
                    Ok(kids)
                };
                let a = PalwTirHistoryPartyV1View { root: st.root.clone(), disclose: &lying };
                let b = PalwTirHistoryPartyV1View { root: st.root.clone(), disclose: &honest };
                let got = palw_tir_history_bisect_v1(st.n, &a, &b);
                // Round 0 is the whole-range combine, which lies too; the refusal names round 0 or later.
                assert!(
                    matches!(
                        got,
                        Err(PalwTirHistoryBisectionError::Refused { party: PalwTirHistoryPartyV1::Responder, error: PalwAttnDissectError::SumDoesNotFold { .. }, .. })
                    ),
                    "H={positions}: {got:?}"
                );
            }
        }
    }

    /// (4) The terminal reads exactly one tile's operands plus the round commitments. The bytes do
    /// not grow with the history.
    #[test]
    fn the_terminal_reads_one_tile_and_the_round_commitments() {
        for positions in [16usize, 17, 256, 300, 513] {
            let st = step(positions, 5 + positions as u64);
            let last = st.tiles.len() - 1;
            let rounds = palw_tir_history_rounds_v1(st.n);
            let one_tile = (st.q.len() + 2 * TILE * st.kv_dim) as u64 * PALW_TIR_HISTORY_BISECTION_CODE_BYTES_V1;
            for t in [0usize, last] {
                let ops = palw_tir_history_tile_operands_v1(&st.q, &st.k, &st.v, st.kv_dim, st.kv_off, st.lanes, params(), t as u64)
                    .expect("operands");
                assert!(ops.k_tile.len() / st.kv_dim <= TILE);
                assert!(ops.read_bytes() <= one_tile, "H={positions} tile {t}");
                assert!(ops.read_bytes_with_rounds(rounds) <= one_tile + rounds as u64 * 2 * PALW_TIR_HISTORY_BISECTION_COMMITMENT_BYTES_V1);
            }
        }
    }

    /// (5) The round count is `⌈log16 tiles⌉` for the ADR sizes, for every localisation, not only the
    /// first tile. Tiles are synthetic one-lane partials, so the sizes are the real ones.
    #[test]
    fn the_round_count_is_ceil_log16_for_the_adr_sizes() {
        for n in [1u64, 16, 17, 256, 1024, 65_536] {
            let tiles: Vec<PalwAttnRangeClaimV1> = (0..n)
                .map(|i| PalwAttnRangeClaimV1 { max: (i % 1000) as i32, exp_sum: 1 + (i % 7) as i64, v_acc: vec![(i % 13) as i64] })
                .collect();
            let honest_root = root_of(&tiles);
            let mut liar = tiles.clone();
            for t in [0usize, (n / 2) as usize, (n - 1) as usize] {
                liar[t].v_acc[0] += 1;
                let liar_root = root_of(&liar);
                let da = |f, c| palw_tir_history_disclose_v1(&liar, f, c);
                let db = |f, c| palw_tir_history_disclose_v1(&tiles, f, c);
                let a = PalwTirHistoryPartyV1View { root: liar_root, disclose: &da };
                let b = PalwTirHistoryPartyV1View { root: honest_root.clone(), disclose: &db };
                let got = palw_tir_history_bisect_v1(n, &a, &b).expect("a dispute");
                let PalwTirHistoryVerdictV1::Localised(l) = got else { panic!("n={n}: no localisation") };
                assert_eq!(l.tile, t as u64, "n={n}");
                assert_eq!(l.rounds, palw_tir_history_rounds_v1(n), "n={n} tile {t}");
                assert_eq!(l.commitments.len(), 2 * l.rounds as usize);
                liar[t].v_acc[0] -= 1;
            }
        }
    }
}
