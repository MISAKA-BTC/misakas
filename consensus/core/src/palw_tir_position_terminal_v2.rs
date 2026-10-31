//! **RFC-0024 §4 / §5 — the versioned position-terminal and lookahead court moves (DORMANT).**
//!
//! G14 item 7 (P2), lane `r5-p2-terminal`. Everything here is dormant behind `palw_tir_bounded_door_v1`
//! (the ADR-0190 door fence, which RFC-0024 A changes the terminal of): below it the four object kinds
//! are the live build's undecodable payload (A-2) and are refused by name at acceptance.
//!
//! * **`PositionDissectionV2`** ([`PalwTirPositionDissectionV2`], tag 201): the responder's signed
//!   position round over the terminal tile — one claim per history position, each against the ROOT
//!   totals (no per-position renormalisation), folding exactly to the tile's claim
//!   ([`crate::palw_tir_dissect_v1::PalwTirPositionNarrowingV1::check_position_round_v1`]). It is
//!   bound to the session, the phase round, the tile's position range and digests of the root and tile
//!   claims (RFC-0024 §7: domain-separated by session/round/root claim).
//! * **`PositionCloseV2`** ([`PalwTirPositionCloseV2`], tag 202): the close. It carries the signed
//!   position round, the challenger's signed position choice, the one-position bottom (a
//!   `TirDissection` / `GenDissection` / `EvalDissection` proof whose carriage is that position's read
//!   set) and, optionally, the read set's step leaves as ONE hash-based Merkle multiproof. It is
//!   adjudicated by `palw_court_v2::adjudicate_tir_position_close_v2` and then applied through the
//!   same `CourtClosed` arm of the fold (one close path, one verdict rule).
//! * **`LookaheadDisclosureV2` / `ChoicePathV2`** (tags 203 / 204): RFC-0024 §5's depth-2 disclosure
//!   and the challenger's one path over it ([`crate::palw_tir_dissect_v1::PalwTirLookaheadDisclosureV2`]).
//!
//! The multiproof ([`PalwStepMultiproofV2`]) is a **Merkle multiproof of hashes only** — the same leaf
//! domain, node hash and promote-odd shape as `palw_step_leg::step_merkle_root_v1`, with shared paths
//! deduplicated. It authenticates that bytes are the committed ones; it proves nothing about whether
//! a computation was right (docs/PRINCIPLES.md §8: no computation proofs — the court still re-executes).
//!
//! The position witness budget (RFC-0024 §4.4–4.5) is a typed-missing parameter: until a profile's
//! exact byte bound is derived and adopted, [`PALW_TIR_POSITION_WITNESS_BUDGET_V2`] is `None` and every
//! position close is refused as [`PalwTirPositionWitnessErrorV2::PositionWitnessBudgetIncomplete`] — a
//! refusal of the move, never a slash (§11 A-06). The whole-tile close is unchanged and stays available.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::palw_step_leg::{step_merkle_leaf_v1, step_merkle_node_v1};
use crate::palw_tir_dissect_v1::{PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirPositionNarrowingV1, PalwTirRangeClaimV1};

/// Wire version of every object in this module.
pub const PALW_TIR_POSITION_OBJECT_VERSION_V2: u16 = 2;

/// Object tags (lowest unused above 200 in the A-2 kind table at the lane base `64bc87dcd`).
pub const PALW_TIR_POSITION_DISSECTED_TAG_V2: u8 = 201;
pub const PALW_TIR_POSITION_CLOSED_TAG_V2: u8 = 202;
pub const PALW_TIR_LOOKAHEAD_DISCLOSED_TAG_V2: u8 = 203;
pub const PALW_TIR_LOOKAHEAD_CHOSEN_TAG_V2: u8 = 204;

/// **The adopted position-witness byte budget** (RFC-0024 §4.5 `B_position`): the counted bytes ONE
/// chunk-group part carries after framing, `palw_close_bytes_for_chunks_v1(1)` = 83,333 B — so a
/// position close is always one carrier transaction (the terminal adds at most one carrier unit).
/// **Decided 2026-10-22 (lane P3, user delegation of value decisions), awaiting independent
/// verification** — `docs/evidence/palw/studio-values-2026-10-22/values-p3-slash-clock.md` V8. The
/// "約 12 KB" was an estimate (two ML-DSA-87 signatures alone are 9,262 B). A larger honest witness is
/// refused as a MOVE (`PositionWitnessOverBudget`), never slashed (§11 A-06), and the whole-tile close
/// (up to `DEFAULT_MAX_CLOSE_BYTES`) stays the path for it. `None` (a caller passing no budget) still
/// refuses as `PositionWitnessBudgetIncomplete`; never zero-filled.
pub const PALW_TIR_POSITION_WITNESS_BUDGET_V2: Option<u64> = Some(crate::palw_mode_v2::palw_close_bytes_for_chunks_v1(1));

/// **The adopted lookahead frontier bound** (RFC-0024 §5.5: `r`, summed over the frontier): 3,072
/// values. Derived so that a tag-203 / tag-204 object at the bound fits ONE chunk-group part
/// (≤ 83,333 counted bytes) at the worst framing of 24 B per value (one-element reductions) with the
/// ML-DSA-87 signature. A bundle over it is refused by name and the V1 one-level rounds stay the path
/// (§5.5 "足りない場合は profile 未対応"); never a slash. **Decided 2026-10-22 (lane P3), awaiting
/// independent verification** — same record, V8.
pub const PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2: Option<usize> = Some(3_072);

/// Hard parser cap on a multiproof's leaf count, checked before any hashing (RFC-0024 §7).
pub const PALW_STEP_MULTIPROOF_MAX_LEAVES_V2: usize = 65_536;

// =================================================================================================
// Domains and digests
// =================================================================================================

pub const PALW_TIR_POSITION_DISSECT_DOMAIN_V2: &[u8] = b"misaka-palw/tir/position/dissect/v2";
pub const PALW_TIR_POSITION_CHOICE_DOMAIN_V2: &[u8] = b"misaka-palw/tir/position/choice/v2";
const PALW_TIR_POSITION_DIGEST_KEY_V2: &[u8] = b"misaka-palw/tir/position/digest/v2";

fn digest64(parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_TIR_POSITION_DIGEST_KEY_V2).to_state();
    for p in parts {
        h.update(&(p.len() as u64).to_le_bytes());
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The digest of a range claim (root totals or the tile's claim) a position move is bound to.
pub fn palw_tir_range_claim_digest_v2(claim: &PalwTirRangeClaimV1) -> Hash64 {
    digest64(&[b"claim", &borsh::to_vec(claim).expect("borsh")])
}

// =================================================================================================
// The position round (tag 201) and the challenger's position choice
// =================================================================================================

/// **`PositionDissectionV2`**: the responder's position round over the terminal tile, bound to it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPositionDissectionV2 {
    pub version: u16,
    pub session_id: Hash64,
    /// The phase round at which the tile became terminal.
    pub tile_round: u32,
    /// The tile's history positions `[from, to)`.
    pub tile_from: u64,
    pub tile_to: u64,
    pub root_claim_digest: Hash64,
    pub tile_claim_digest: Hash64,
    /// One claim per position of the tile, in position order.
    pub round: PalwTirDissectRoundV1,
}

/// **The challenger's position choice** (signed under the ADR-0082 challenger context).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPositionChoiceV2 {
    pub version: u16,
    pub session_id: Hash64,
    pub tile_round: u32,
    /// The digest of the exact position round it chooses in ([`palw_tir_position_dissection_digest_v2`]).
    pub dissection_digest: Hash64,
    /// The index of the named position inside the tile.
    pub position_child: u8,
}

pub fn palw_tir_position_dissection_digest_v2(d: &PalwTirPositionDissectionV2) -> Hash64 {
    digest64(&[b"dissection", &borsh::to_vec(d).expect("borsh")])
}

/// The message the responder signs for a position round.
pub fn palw_tir_position_dissection_message_v2(d: &PalwTirPositionDissectionV2) -> Vec<u8> {
    let mut m = PALW_TIR_POSITION_DISSECT_DOMAIN_V2.to_vec();
    m.extend(borsh::to_vec(d).expect("borsh"));
    m
}

/// The message the challenger signs for a position choice.
pub fn palw_tir_position_choice_message_v2(c: &PalwTirPositionChoiceV2) -> Vec<u8> {
    let mut m = PALW_TIR_POSITION_CHOICE_DOMAIN_V2.to_vec();
    m.extend(borsh::to_vec(c).expect("borsh"));
    m
}

/// Why a position move is refused. Every arm is a refusal of the MOVE, never a verdict or a slash.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirPositionMoveErrorV2 {
    #[error("unsupported object version {0}")]
    UnsupportedVersion(u16),
    #[error("the position move is not bound to this phase: {0}")]
    Binding(&'static str),
    #[error("the dissection is not terminal on a tile wider than one position")]
    NoPositionRound,
    #[error("the position round is refused: {0}")]
    Round(String),
    #[error("the position choice is refused: {0}")]
    Choice(&'static str),
}

/// The responder's position round a terminal `phase` expects, built honestly from `round` (a builder
/// helper: binds everything a [`PalwTirPositionDissectionV2`] must carry).
pub fn palw_tir_position_dissection_for_v2(
    phase: &PalwTirDissectPhaseV1,
    round: PalwTirDissectRoundV1,
) -> Option<PalwTirPositionDissectionV2> {
    let narrowing = phase.position_narrowing_v1()?;
    let (from, to) = narrowing.tile_range();
    Some(PalwTirPositionDissectionV2 {
        version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
        session_id: phase.session_id(),
        tile_round: phase.round(),
        tile_from: from as u64,
        tile_to: to as u64,
        root_claim_digest: palw_tir_range_claim_digest_v2(phase.root()),
        tile_claim_digest: palw_tir_range_claim_digest_v2(phase.claim()),
        round,
    })
}

/// **Check a position round against the session's terminal phase** and return the narrowing it
/// opens (not yet named). Pure; the signature is the acceptance layer's.
pub fn check_tir_position_dissection_v2(
    phase: &PalwTirDissectPhaseV1,
    d: &PalwTirPositionDissectionV2,
) -> Result<PalwTirPositionNarrowingV1, PalwTirPositionMoveErrorV2> {
    use PalwTirPositionMoveErrorV2 as E;
    if d.version != PALW_TIR_POSITION_OBJECT_VERSION_V2 {
        return Err(E::UnsupportedVersion(d.version));
    }
    // Lane r6-p2-shared: an OWED position round (the phase waits on the responder) narrows as the terminal it will be.
    let narrowing = phase.position_narrowing_v2().ok_or(E::NoPositionRound)?;
    if !narrowing.awaits_position_round() {
        return Err(E::NoPositionRound);
    }
    // Once a round is ON CHAIN (tag 201 or a version-3 lookahead), the close names a position of THAT round only.
    if let Some(disclosed) = phase.disclosed_position_round_v2()
        && d.round.children.as_slice() != disclosed
    {
        return Err(E::Binding("not the position round on chain"));
    }
    let (from, to) = narrowing.tile_range();
    if d.session_id != phase.session_id() {
        return Err(E::Binding("session"));
    }
    if d.tile_round != phase.round() {
        return Err(E::Binding("round"));
    }
    if (d.tile_from, d.tile_to) != (from as u64, to as u64) {
        return Err(E::Binding("tile range"));
    }
    if d.root_claim_digest != palw_tir_range_claim_digest_v2(phase.root()) {
        return Err(E::Binding("root claim"));
    }
    if d.tile_claim_digest != palw_tir_range_claim_digest_v2(phase.claim()) {
        return Err(E::Binding("tile claim"));
    }
    narrowing.check_position_round_v1(&d.round).map_err(|e| E::Round(e.to_string()))?;
    Ok(narrowing)
}

/// **Name the position**: the checked round and the challenger's choice give the one-position
/// narrowing the bottom is judged against.
pub fn palw_tir_position_named_v2(
    phase: &PalwTirDissectPhaseV1,
    d: &PalwTirPositionDissectionV2,
    c: &PalwTirPositionChoiceV2,
) -> Result<PalwTirPositionNarrowingV1, PalwTirPositionMoveErrorV2> {
    use PalwTirPositionMoveErrorV2 as E;
    let narrowing = check_tir_position_dissection_v2(phase, d)?;
    if c.version != PALW_TIR_POSITION_OBJECT_VERSION_V2 {
        return Err(E::UnsupportedVersion(c.version));
    }
    if c.session_id != d.session_id || c.tile_round != d.tile_round {
        return Err(E::Choice("session or round"));
    }
    if c.dissection_digest != palw_tir_position_dissection_digest_v2(d) {
        return Err(E::Choice("names another position round"));
    }
    narrowing.choose_position_v1(&d.round, c.position_child).map_err(|_| E::Choice("position outside the tile"))
}

// =================================================================================================
// The close (tag 202)
// =================================================================================================

/// **`PositionCloseV2`** (RFC-0024 §4.5). `proof` must be a dissection bottom (`TirDissection`,
/// `GenDissection` or `EvalDissection`) whose carriage is the NAMED position's read set.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPositionCloseV2 {
    pub version: u16,
    pub session_id: Hash64,
    pub verdict: crate::palw_state_v2::PalwCourtVerdictV2,
    pub dissection: PalwTirPositionDissectionV2,
    /// The claim's bond over [`palw_tir_position_dissection_message_v2`] (responder context).
    pub responder_signature: Vec<u8>,
    pub choice: PalwTirPositionChoiceV2,
    /// The session's challenger bond over [`palw_tir_position_choice_message_v2`] (challenger context).
    pub challenger_signature: Vec<u8>,
    pub proof: crate::palw_court_v2::PalwCourtVerdictProofV2,
    /// The read set's step leaves as one multiproof under the binding's step root (TIR only; optional).
    pub step_multiproof: Option<PalwStepMultiproofV2>,
}

/// Why a position close's witness is not admitted (RFC-0024 §4.4–4.5). Not a verdict; not a slash.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirPositionWitnessErrorV2 {
    #[error("PositionWitnessBudgetIncomplete: no position witness bound is adopted for this profile (RFC-0024 §4.4)")]
    PositionWitnessBudgetIncomplete,
    #[error("the position witness is {bytes} bytes, over the adopted {budget}")]
    PositionWitnessOverBudget { bytes: u64, budget: u64 },
}

/// The witness bytes `B_position` of a close: every coordinate, root, opening, signature and codec
/// byte — its whole encoding (RFC-0024 §4.5: not only the Q/K/V body).
pub fn palw_tir_position_witness_bytes_v2(close: &PalwTirPositionCloseV2) -> u64 {
    borsh::to_vec(close).map(|b| b.len() as u64).unwrap_or(u64::MAX)
}

/// **The position witness budget** — checked before any evaluation. `budget` is the adopted bound
/// ([`PALW_TIR_POSITION_WITNESS_BUDGET_V2`] at the call sites).
pub fn check_tir_position_witness_budget_v2(
    close: &PalwTirPositionCloseV2,
    budget: Option<u64>,
) -> Result<u64, PalwTirPositionWitnessErrorV2> {
    let budget = budget.ok_or(PalwTirPositionWitnessErrorV2::PositionWitnessBudgetIncomplete)?;
    let bytes = palw_tir_position_witness_bytes_v2(close);
    if bytes > budget {
        return Err(PalwTirPositionWitnessErrorV2::PositionWitnessOverBudget { bytes, budget });
    }
    Ok(bytes)
}

// =================================================================================================
// Lookahead carriers (tags 203 / 204)
// =================================================================================================

/// **Tag 203's body**: the responder's depth-2 disclosure at the phase's round.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirLookaheadDisclosedV2 {
    pub session_id: Hash64,
    pub disclosure: crate::palw_tir_dissect_v1::PalwTirLookaheadDisclosureV2,
    /// The claim's bond over `palw_tir_lookahead_message_v2` (responder context).
    pub signature: Vec<u8>,
}

/// **Tag 204's body**: the challenger's path, with the disclosure it chooses in (re-checked against
/// the pending round, so the fold needs no stored frontier).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirLookaheadChosenV2 {
    pub disclosure: crate::palw_tir_dissect_v1::PalwTirLookaheadDisclosureV2,
    pub path: crate::palw_tir_dissect_v1::PalwTirChoicePathV2,
    /// The session's challenger bond over `palw_tir_choice_path_message_v2` (challenger context).
    pub signature: Vec<u8>,
}

// =================================================================================================
// Hash-only Merkle multiproof over the step tree
// =================================================================================================

/// **One multiproof for many leaves of the step tree** (`step_merkle_root_v1`'s tree: index-bound
/// leaves, `step_merkle_node_v1` interiors, an odd last node promoted). `indices` ascending and
/// distinct; `leaf_hashes[i]` is leaf `indices[i]`'s hash (as `PalwStepOpeningV1::leaf_hash`); `proof`
/// is every sibling the walk needs and cannot derive, level by level, ascending index.
/// Authenticates bytes only; it is not a proof of any computation (PRINCIPLES §8).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepMultiproofV2 {
    pub leaf_count: u64,
    pub indices: Vec<u64>,
    pub leaf_hashes: Vec<Hash64>,
    pub proof: Vec<Hash64>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwStepMultiproofErrorV2 {
    #[error("the multiproof's shape is invalid: {0}")]
    Shape(&'static str),
    #[error("the multiproof needs more sibling hashes than it carries")]
    Short,
    #[error("the multiproof carries sibling hashes its walk does not use")]
    Long,
}

/// One level of the walk: from the known `(index, hash)` nodes of a level of `width`, the parents —
/// taking any sibling not known from `next_sibling`.
fn multiproof_level(
    known: &[(u64, Hash64)],
    width: u64,
    mut next_sibling: impl FnMut(u64) -> Result<Hash64, PalwStepMultiproofErrorV2>,
) -> Result<Vec<(u64, Hash64)>, PalwStepMultiproofErrorV2> {
    let mut parents = Vec::with_capacity(known.len());
    let mut k = 0;
    while k < known.len() {
        let (i, h) = known[k];
        if i % 2 == 0 {
            if i + 1 >= width {
                // The promoted odd node.
                parents.push((i / 2, h));
                k += 1;
            } else if k + 1 < known.len() && known[k + 1].0 == i + 1 {
                parents.push((i / 2, step_merkle_node_v1(&h, &known[k + 1].1)));
                k += 2;
            } else {
                let right = next_sibling(i + 1)?;
                parents.push((i / 2, step_merkle_node_v1(&h, &right)));
                k += 1;
            }
        } else {
            let left = next_sibling(i - 1)?;
            parents.push((i / 2, step_merkle_node_v1(&left, &h)));
            k += 1;
        }
    }
    Ok(parents)
}

impl PalwStepMultiproofV2 {
    /// **Build** the multiproof of `indices` over the ordered leaf hashes of the whole tree.
    pub fn build(ordered_leaf_hashes: &[Hash64], indices: &[u64]) -> Result<Self, PalwStepMultiproofErrorV2> {
        let count = ordered_leaf_hashes.len() as u64;
        if count == 0 || indices.is_empty() || indices.len() > PALW_STEP_MULTIPROOF_MAX_LEAVES_V2 {
            return Err(PalwStepMultiproofErrorV2::Shape("leaf count or index count"));
        }
        if indices.windows(2).any(|w| w[0] >= w[1]) || *indices.last().unwrap() >= count {
            return Err(PalwStepMultiproofErrorV2::Shape("indices ascending, distinct and inside the tree"));
        }
        let mut levels: Vec<Vec<Hash64>> =
            vec![ordered_leaf_hashes.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(i as u64, h)).collect()];
        while levels.last().unwrap().len() > 1 {
            let level = levels.last().unwrap();
            let mut next: Vec<Hash64> = level.chunks_exact(2).map(|p| step_merkle_node_v1(&p[0], &p[1])).collect();
            if level.len() % 2 == 1 {
                next.push(*level.last().unwrap());
            }
            levels.push(next);
        }
        let mut proof = Vec::new();
        let mut known: Vec<(u64, Hash64)> = indices.iter().map(|&i| (i, levels[0][i as usize])).collect();
        for level in &levels[..levels.len() - 1] {
            let width = level.len() as u64;
            known = multiproof_level(&known, width, |s| {
                proof.push(level[s as usize]);
                Ok(level[s as usize])
            })?;
        }
        Ok(Self {
            leaf_count: count,
            indices: indices.to_vec(),
            leaf_hashes: indices.iter().map(|&i| ordered_leaf_hashes[i as usize]).collect(),
            proof,
        })
    }

    /// **The root this multiproof implies** (lengths and order checked before any hash).
    pub fn root(&self) -> Result<Hash64, PalwStepMultiproofErrorV2> {
        if self.leaf_count == 0 || self.indices.is_empty() || self.indices.len() > PALW_STEP_MULTIPROOF_MAX_LEAVES_V2 {
            return Err(PalwStepMultiproofErrorV2::Shape("leaf count or index count"));
        }
        if self.leaf_hashes.len() != self.indices.len() {
            return Err(PalwStepMultiproofErrorV2::Shape("one hash per index"));
        }
        if self.indices.windows(2).any(|w| w[0] >= w[1]) || *self.indices.last().unwrap() >= self.leaf_count {
            return Err(PalwStepMultiproofErrorV2::Shape("indices ascending, distinct and inside the tree"));
        }
        // A proof can never need more than one sibling per known node per level.
        let depth = 64 - (self.leaf_count - 1).leading_zeros() as usize;
        if self.proof.len() > self.indices.len().saturating_mul(depth.max(1)) {
            return Err(PalwStepMultiproofErrorV2::Long);
        }
        let mut known: Vec<(u64, Hash64)> =
            self.indices.iter().zip(&self.leaf_hashes).map(|(&i, h)| (i, step_merkle_leaf_v1(i, h))).collect();
        let mut proof = self.proof.iter();
        let mut width = self.leaf_count;
        while width > 1 {
            known = multiproof_level(&known, width, |_| proof.next().copied().ok_or(PalwStepMultiproofErrorV2::Short))?;
            width = width.div_ceil(2);
        }
        if proof.next().is_some() {
            return Err(PalwStepMultiproofErrorV2::Long);
        }
        Ok(known[0].1)
    }

    /// Verify against a committed root and leaf count.
    pub fn verify(&self, root: &Hash64, leaf_count: u64) -> Result<(), PalwStepMultiproofErrorV2> {
        if self.leaf_count != leaf_count {
            return Err(PalwStepMultiproofErrorV2::Shape("leaf count is not the committed one"));
        }
        if self.root()? != *root {
            return Err(PalwStepMultiproofErrorV2::Shape("implies another root"));
        }
        Ok(())
    }

    /// The hash this multiproof authenticates for leaf `index`, if it carries it.
    pub fn leaf(&self, index: u64) -> Option<&Hash64> {
        self.indices.binary_search(&index).ok().map(|k| &self.leaf_hashes[k])
    }

    /// Encoded bytes (borsh).
    pub fn encoded_bytes(&self) -> u64 {
        borsh::to_vec(self).map(|b| b.len() as u64).unwrap_or(u64::MAX)
    }
}

/// The encoded bytes of the same leaves as INDIVIDUAL `PalwStepOpeningV1`s (one full path each),
/// for the byte comparison against [`PalwStepMultiproofV2`]: a deterministic count from the tree
/// shape, not a timing claim.
pub fn palw_step_single_openings_bytes_v2(ordered_leaf_hashes: &[Hash64], indices: &[u64]) -> Option<u64> {
    let mut total = 0u64;
    for &i in indices {
        let siblings = crate::palw_step_leg::step_merkle_path_v1(ordered_leaf_hashes, i as usize).ok()?;
        let opening = crate::palw_step_leg::PalwStepOpeningV1 { leaf_index: i, leaf_hash: ordered_leaf_hashes[i as usize], siblings };
        total += borsh::to_vec(&opening).ok()?.len() as u64;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(n: u64) -> Vec<Hash64> {
        (0..n).map(|i| Hash64::from_bytes([(i % 251) as u8 ^ 0x5A; 64])).map(|h| digest64(&[h.as_byte_slice()])).collect()
    }

    #[test]
    fn a_multiproof_roots_to_the_step_tree_for_every_shape() {
        for n in [1u64, 2, 3, 5, 7, 8, 9, 16, 17, 33, 100] {
            let l = leaves(n);
            let root = crate::palw_step_leg::step_merkle_root_v1(&l).unwrap();
            let sets: Vec<Vec<u64>> = vec![vec![0], vec![n - 1], (0..n).collect(), (0..n).step_by(3).collect(), (n / 2..n).collect()];
            for set in sets {
                let mp = PalwStepMultiproofV2::build(&l, &set).unwrap();
                assert_eq!(mp.root().unwrap(), root, "n={n} set={set:?}");
                mp.verify(&root, n).unwrap();
                for &i in &set {
                    assert_eq!(mp.leaf(i), Some(&l[i as usize]));
                }
            }
        }
    }

    #[test]
    fn a_tampered_multiproof_is_refused() {
        let l = leaves(37);
        let root = crate::palw_step_leg::step_merkle_root_v1(&l).unwrap();
        let mp = PalwStepMultiproofV2::build(&l, &[3, 4, 20, 36]).unwrap();
        let mut bad = mp.clone();
        bad.leaf_hashes[1] = Hash64::from_bytes([1; 64]);
        assert!(bad.verify(&root, 37).is_err());
        let mut short = mp.clone();
        short.proof.pop();
        assert!(short.verify(&root, 37).is_err());
        let mut long = mp.clone();
        long.proof.push(Hash64::from_bytes([2; 64]));
        assert!(long.verify(&root, 37).is_err());
        let mut moved = mp.clone();
        moved.indices[0] = 2; // the leaf is index-bound
        assert!(moved.verify(&root, 37).is_err());
        let mut unsorted = mp.clone();
        unsorted.indices.swap(0, 1);
        assert!(unsorted.root().is_err());
        assert!(mp.verify(&root, 38).is_err());
    }

    /// **Evidence bytes: multiproof vs single openings** — deterministic byte counts from the tree
    /// shape (2^12 leaves, a read set of one contiguous run of 64 plus 8 scattered leaves). Not timed.
    #[test]
    fn a_multiproof_carries_fewer_bytes_than_single_openings() {
        let n = 1u64 << 12;
        let l = leaves(n);
        let mut set: Vec<u64> = (1000..1064).collect();
        set.extend([7u64, 513, 2047, 2048, 3000, 3333, 4000, 4095]);
        set.sort();
        let mp = PalwStepMultiproofV2::build(&l, &set).unwrap();
        let single = palw_step_single_openings_bytes_v2(&l, &set).unwrap();
        let multi = mp.encoded_bytes();
        // Single: 72 openings * (8 + 64 + 4 + 12*64) = 72 * 844 = 60,768 bytes (depth 12).
        assert_eq!(single, 72 * (8 + 64 + 4 + 12 * 64));
        // Multi: 8 + (4 + 72*8) + (4 + 72*64) + (4 + |proof|*64); the run shares its interior paths.
        assert_eq!(multi, 8 + 4 + 72 * 8 + 4 + 72 * 64 + 4 + 64 * mp.proof.len() as u64);
        assert!(multi * 2 < single, "multiproof {multi} B vs single openings {single} B");
        // One leaf: the multiproof is one path, a few framing bytes away from a single opening.
        let one = PalwStepMultiproofV2::build(&l, &[1234]).unwrap();
        assert_eq!(one.proof.len(), 12);
    }

    /// V8 (decided 2026-10-22, lane P3): the budget is exactly one framed chunk part.
    #[test]
    fn p3_the_position_witness_budget_is_one_carrier_part() {
        let budget = PALW_TIR_POSITION_WITNESS_BUDGET_V2.expect("decided");
        assert_eq!(budget, 83_333);
        assert_eq!(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(budget), 1, "a close at the budget is one carrier part");
        // The framing floor lets one part carry 83,334 counted bytes; the budget stays at the exact derivation, one byte under.
        assert_eq!(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(budget + 1), 1);
        assert_eq!(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(budget + 2), 2, "two bytes more need a second part");
        assert!(budget < crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES, "the whole-tile close ceiling stays above it");
        // r5-p2-terminal §3: fixed part 9,466 B (two ML-DSA-87 signatures are 9,262), the w=16 example round 3,548 B and the
        // 72-leaf depth-12 multiproof 10,068 B leave 60,251 B for the dissection-bottom carriage.
        assert_eq!(budget - (9_466 + 3_548 + 10_068), 60_251);
    }

    /// V8 (decided 2026-10-22, lane P3): a lookahead at the frontier bound fits one carrier part in the worst framing (24 B per
    /// value: every reduction carries one element), for the disclosure (tag 203) and the chosen path (tag 204).
    #[test]
    fn p3_a_lookahead_at_the_frontier_bound_fits_one_carrier_part() {
        use crate::palw_tir_dissect_v1::{
            PalwTirChoicePathV2, PalwTirLookaheadDisclosureV2, PalwTirLookaheadExpansionV2, PalwTirRangeClaimV1,
        };
        let cap = PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2.expect("decided");
        let budget = PALW_TIR_POSITION_WITNESS_BUDGET_V2.expect("decided");
        assert_eq!(cap, 3_072);
        // Worst bytes per value: 16 groups of 192 one-value claims of one one-element reduction (4 + 4 + 16 = 24 B per value).
        let worst = PalwTirLookaheadDisclosureV2 {
            version: 2,
            frontier: vec![vec![PalwTirRangeClaimV1 { partials: vec![vec![i128::MAX]] }; cap / 16]; 16],
        };
        // The two-level shape: 16 groups × 16 claims × 12 one-element reductions.
        let shaped = PalwTirLookaheadDisclosureV2 {
            version: 2,
            frontier: vec![vec![PalwTirRangeClaimV1 { partials: vec![vec![i128::MIN]; 12] }; 16]; 16],
        };
        for disclosure in [worst, shaped] {
            assert_eq!(PalwTirLookaheadExpansionV2::frontier_values(&disclosure), cap);
            let disclosed = PalwTirLookaheadDisclosedV2 {
                session_id: Hash64::from_bytes([7; 64]),
                disclosure: disclosure.clone(),
                signature: vec![0xA5; 4_627],
            };
            let path =
                PalwTirChoicePathV2 { version: 2, session_id: Hash64::from_bytes([7; 64]), round: u32::MAX, path: vec![15, 15] };
            let chosen = PalwTirLookaheadChosenV2 { disclosure, path, signature: vec![0x5A; 4_627] };
            for bytes in [borsh::to_vec(&disclosed).unwrap().len() as u64, borsh::to_vec(&chosen).unwrap().len() as u64] {
                assert!(bytes <= budget, "{bytes} B is over one carrier part");
                assert_eq!(crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(bytes), 1);
            }
        }
    }
}
