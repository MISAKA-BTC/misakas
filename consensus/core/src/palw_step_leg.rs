//! PALW execution-commitment v2 — the step leg (ADR-0030 §3).
//!
//! A **new scheme family** (`misaka-palw/execution-commitment/v2`), never a field bolted onto
//! v1: v1's domains, goldens and meaning do not move, and which commitment form a class
//! produces stays a registration fact. The v2 composite binds the frozen v2 logits root, the
//! v1 activation leg **unchanged**, a **chunked** checkpoint leg (per-semantic-unit state
//! chunks, so a mid-interval GDN step opens one 64 KiB head slice, not a ~26 MiB blob), and
//! the **step leg**: node-output tiles at the `palw_step` coordinates plus the KV aux-chunk
//! series that makes full-context reductions openable in ~10 chunks.
//!
//! Same discipline as `palw_legs`: producer code is fail-closed (an honest executor aborts
//! rather than commit a refutable byte — non-finite values, out-of-order arrival, wrong tile
//! lengths are all construction failures, not commitments), adjudication is model-free, and
//! `NoFaultFound` is the verdict that costs a challenger their bond. Consensus-inert.
//!
//! The tree functions here mirror `palw_legs`' generalized Merkle exactly (leaf-index-bound
//! leaves, domain-separated nodes, odd promote) with one difference: the step tree is up to
//! `1 << 22` leaves, past the v1 opening-sibling cap — so the functions are reimplemented
//! and **cross-frozen against the v1 implementation by equivalence tests** on shared shapes
//! rather than by call reuse.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

use crate::palw_legs::PalwCheckpointProfileV1;
use crate::palw_slash::{PalwSlashError, check_job_context_shape};
use crate::palw_step::{
    PalwLayerKindV1, PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepError, canonical_step_leaf_index, kv_aux_leaf_count,
};
use crate::palw_v2::PalwJobContextV2;

// ---------------------------------------------------------------------------------------------
// Versions, domains, caps
// ---------------------------------------------------------------------------------------------

pub const PALW_STEP_LEG_OBJECT_VERSION_V1: u16 = 1;

/// The composite scheme name; [`execution_commitment_scheme_id_v2`] is its identity.
pub const PALW_EXECUTION_COMMITMENT_SCHEME_NAME_V2: &str = "misaka-palw/execution-commitment/v2";

pub const PALW_STEP_LEG_DOMAIN_SCHEME_ID: &[u8] = b"misaka-palw/execution-commitment-scheme-id/v2";
pub const PALW_STEP_LEG_DOMAIN_TILE_LEAF: &[u8] = b"misaka-palw/step-tile-leaf/v1";
pub const PALW_STEP_LEG_DOMAIN_KV_CHUNK_LEAF: &[u8] = b"misaka-palw/step-kv-chunk-leaf/v1";
pub const PALW_STEP_LEG_DOMAIN_MERKLE_LEAF: &[u8] = b"misaka-palw/step-merkle-leaf/v1";
pub const PALW_STEP_LEG_DOMAIN_MERKLE_NODE: &[u8] = b"misaka-palw/step-merkle-node/v1";
pub const PALW_STEP_LEG_DOMAIN_LEG: &[u8] = b"misaka-palw/step-leg/v1";
pub const PALW_STEP_LEG_DOMAIN_CHECKPOINT_LEAF_V2: &[u8] = b"misaka-palw/checkpoint-leaf/v2";
pub const PALW_STEP_LEG_DOMAIN_STATE_CHUNK_LEAF: &[u8] = b"misaka-palw/state-chunk-leaf/v1";
pub const PALW_STEP_LEG_DOMAIN_STATE_CHUNK_NODE: &[u8] = b"misaka-palw/state-chunk-node/v1";
pub const PALW_STEP_LEG_DOMAIN_CHECKPOINT_GENESIS_V2: &[u8] = b"misaka-palw/checkpoint-genesis/v2";
pub const PALW_STEP_LEG_DOMAIN_CHECKPOINT_EMPTY_V2: &[u8] = b"misaka-palw/checkpoint-empty/v2";
pub const PALW_STEP_LEG_DOMAIN_CHECKPOINT_LEG_V2: &[u8] = b"misaka-palw/checkpoint-leg/v2";
pub const PALW_STEP_LEG_DOMAIN_EXECUTION_COMMITMENT_V2: &[u8] = b"misaka-palw/execution-commitment/v2";
pub const PALW_STEP_LEG_DOMAIN_EVIDENCE_ID: &[u8] = b"misaka-palw/step-refutation-evidence-id/v1";

pub const PALW_STEP_LEG_ALL_DOMAINS: &[&[u8]] = &[
    PALW_STEP_LEG_DOMAIN_SCHEME_ID,
    PALW_STEP_LEG_DOMAIN_TILE_LEAF,
    PALW_STEP_LEG_DOMAIN_KV_CHUNK_LEAF,
    PALW_STEP_LEG_DOMAIN_MERKLE_LEAF,
    PALW_STEP_LEG_DOMAIN_MERKLE_NODE,
    PALW_STEP_LEG_DOMAIN_LEG,
    PALW_STEP_LEG_DOMAIN_CHECKPOINT_LEAF_V2,
    PALW_STEP_LEG_DOMAIN_STATE_CHUNK_LEAF,
    PALW_STEP_LEG_DOMAIN_STATE_CHUNK_NODE,
    PALW_STEP_LEG_DOMAIN_CHECKPOINT_GENESIS_V2,
    PALW_STEP_LEG_DOMAIN_CHECKPOINT_EMPTY_V2,
    PALW_STEP_LEG_DOMAIN_CHECKPOINT_LEG_V2,
    PALW_STEP_LEG_DOMAIN_EXECUTION_COMMITMENT_V2,
    PALW_STEP_LEG_DOMAIN_EVIDENCE_ID,
];

/// Step-tree leaf cap — **the DEFAULT ladder top, not the rule.**
///
/// The rule is `PalwCourtParamsV2::max_step_leaf_count`: the number a network actually froze into
/// `palw_ruleset_id_v2`, which every shipped preset froze at this constant. The `_capped_v1` entry
/// points below take that number; the un-capped names pass this one, so a caller with no ruleset
/// in scope keeps exactly the behaviour it had.
pub const PALW_STEP_LEG_MAX_LEAVES: u64 = crate::palw_step::PALW_STEP_MAX_LEAVES;

/// **The deepest opening a ladder of `max_step_leaf_count` leaves can need: `ceil(log2(n))`.**
///
/// This was the literal `22`, under a doc line that already called it `ceil(log2(MAX_LEAVES))` —
/// a derived quantity spelled as a constant, and therefore one that does not move when the thing
/// it derives from does. A ruleset that freezes a `2^32` ladder needs 32 siblings per opening and
/// the leg refused at 22, so on that ladder EVERY honest opening was refused by the very court
/// that asked for it. That, and not the ladder constant, is why "arm the deeper ladder" was never
/// a one-line change. The derivation lives here now, and the caps are the ruleset's.
///
/// It is `PalwCourtParamsV2::bisection_rounds` reached by another route, and the two must agree:
/// the court budgets one bisection round per level and the leg carries one sibling per level, so a
/// leg cap below the court's round count is a dispute the court can schedule and the leg cannot
/// close. The agreement is asserted over a sweep in this module's tests.
pub const fn step_leg_max_opening_siblings_v1(max_step_leaf_count: u64) -> usize {
    // A zero- or one-leaf tree is its own root: no level, and so no sibling.
    if max_step_leaf_count <= 1 {
        return 0;
    }
    // `ceil(log2(n)) = floor(log2(n - 1)) + 1` for `n >= 2`. Written this way rather than through
    // `next_power_of_two`, which overflows above `2^63` instead of answering 64.
    (max_step_leaf_count - 1).ilog2() as usize + 1
}

/// The deepest opening on the DEFAULT ladder: [`step_leg_max_opening_siblings_v1`] of
/// [`PALW_STEP_LEG_MAX_LEAVES`] — 22, and 22 for as long as every preset freezes `2^22`.
pub const PALW_STEP_LEG_MAX_OPENING_SIBLINGS: usize = step_leg_max_opening_siblings_v1(PALW_STEP_LEG_MAX_LEAVES);
/// Cap on one carried tile (bytes): `4 × MAX_TILE_LEN`.
pub const PALW_STEP_LEG_MAX_TILE_BYTES: usize = 4 * crate::palw_step::PALW_STEP_MAX_TILE_LEN as usize;
/// Cap on one carried KV chunk (bytes).
pub const PALW_STEP_LEG_MAX_KV_CHUNK_BYTES: usize = 1 << 20;
/// Cap on state chunks per checkpoint and on one carried state chunk (bytes).
pub const PALW_STEP_LEG_MAX_STATE_CHUNKS: usize = 1 << 16;
pub const PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES: usize = 1 << 20;

pub fn execution_commitment_scheme_id_v2() -> Hash64 {
    keyed64(PALW_STEP_LEG_DOMAIN_SCHEME_ID, &[PALW_EXECUTION_COMMITMENT_SCHEME_NAME_V2.as_bytes()])
}

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        h.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

struct Writer(Vec<u8>);
impl Writer {
    fn new() -> Self {
        Self(Vec::with_capacity(256))
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn hash64(&mut self, v: &Hash64) {
        self.0.extend_from_slice(v.as_byte_slice());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
    }
    fn keyed64(self, domain: &[u8]) -> Hash64 {
        keyed64(domain, &[&self.0])
    }
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwStepLegError {
    #[error("unsupported step-leg object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("job context is malformed: {0}")]
    Context(PalwSlashError),
    #[error("the binding's shape profile ({got}) is not the one the job context declares ({declared})")]
    ShapeProfileNotTheDeclaredOne { declared: Hash64, got: Hash64 },
    #[error("the binding's state chunk map ({carried}) is not the one its shape profile registers ({registered})")]
    StateChunkMapNotTheRegisteredOne { registered: Hash64, carried: Hash64 },
    #[error("step space error: {0}")]
    Step(PalwStepError),
    #[error("leaf count {got} is outside 1..={max}")]
    LeafCountOutOfRange { got: u64, max: u64 },
    #[error("leaf index {index} is not below leaf count {count}")]
    LeafIndexOutOfRange { index: u64, count: u64 },
    #[error("opening carries {got} siblings, exceeding the {max}-level cap")]
    OpeningTooDeep { got: usize, max: usize },
    #[error("opening path ended short of the root")]
    OpeningPathTooShort,
    #[error("opening path carries {extra} sibling(s) past the root")]
    OpeningPathTooLong { extra: usize },
    #[error("carried binding does not recompute the committed execution commitment root")]
    CommittedRootMismatch,
    #[error("carried {leaf} preimage does not hash to the opened leaf hash")]
    LeafPreimageMismatch { leaf: &'static str },
    #[error("carried payload exceeds its byte cap (got {got}, max {max})")]
    PayloadTooLarge { got: usize, max: usize },
    #[error("the addressed material is honest under every pinned rule — refutation rejected")]
    NoFaultFound,

    // Producer-side: the errors an honest executor hits INSTEAD of emitting a commitment.
    #[error("step tile arrived out of canonical order (expected leaf {expected}, got {got})")]
    TilesOutOfOrder { expected: u64, got: u64 },
    #[error("step tile coordinates are not canonical for this (profile, context)")]
    TileCoordinatesNotCanonical,
    #[error("step tile carries {got} values but the canonical tile length is {expected}")]
    TileLengthNotCanonical { got: u32, expected: u32 },
    #[error("step value {value_index} of leaf {leaf_index} is non-finite — execution is invalid, emit no receipt")]
    NonFiniteStepValue { leaf_index: u64, value_index: u32 },
    #[error("kv chunk value {value_index} of leaf {leaf_index} is non-finite — execution is invalid, emit no receipt")]
    NonFiniteKvValue { leaf_index: u64, value_index: u32 },
    #[error("kv chunk arrived out of the canonical aux order")]
    KvChunksOutOfOrder,
    #[error("the {what} leg holds {got} of the {expected} entries the job mandates")]
    LegIncomplete { what: &'static str, got: u64, expected: u64 },
    #[error("checkpoint {index} covers decode call {got}, but the interval mandates {expected}")]
    CheckpointNotCanonical { index: u32, got: u32, expected: u32 },
    #[error("state chunk count {got} exceeds the {max} cap")]
    StateChunksOutOfRange { got: usize, max: usize },
}

impl From<PalwStepError> for PalwStepLegError {
    fn from(e: PalwStepError) -> Self {
        PalwStepLegError::Step(e)
    }
}

// ---------------------------------------------------------------------------------------------
// The step tree (leaf-index-bound leaves, domain-separated nodes, odd promote) — the v1
// generalization at step depth. Cross-frozen against `palw_legs` by equivalence tests.
// ---------------------------------------------------------------------------------------------

/// Membership proof of one leaf in the step tree (u64 index space).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepOpeningV1 {
    pub leaf_index: u64,
    pub leaf_hash: Hash64,
    pub siblings: Vec<Hash64>,
}

/// **`H(domain ‖ index_le ‖ leaf)` — the step tree's leaf, index-bound so a leaf cannot be moved.**
///
/// Public because an executor that folds the same tree sparsely (`misaka-palw-base0`'s
/// `Base0SparseStepAccumulatorV1`, ADR-0077's Decision 8 openings) has to produce byte-identical
/// nodes, and the only two ways to arrange that are to export this or to restate it. It was
/// restated, and a restatement is a second spelling of a consensus hash: a root the court
/// recomputes differently is an honest producer who can neither be convicted nor paid. So the
/// spelling lives here, once, and the engine crate calls it.
pub fn step_merkle_leaf_v1(index: u64, leaf_hash: &Hash64) -> Hash64 {
    step_merkle_leaf(index, leaf_hash)
}

/// **The step tree's interior node**, `H(domain ‖ left ‖ right)` — [`step_merkle_leaf_v1`]'s
/// companion, exported for the same reason and with the same rule: one spelling, in the crate that
/// owns the domain constants.
pub fn step_merkle_node_v1(left: &Hash64, right: &Hash64) -> Hash64 {
    keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[left.as_byte_slice(), right.as_byte_slice()])
}

fn step_merkle_leaf(index: u64, leaf_hash: &Hash64) -> Hash64 {
    let mut w = Writer::new();
    w.u64(index);
    w.hash64(leaf_hash);
    w.keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_LEAF)
}

/// Root of the step tree over ordered leaf hashes.
pub fn step_merkle_root_v1(ordered_leaf_hashes: &[Hash64]) -> Result<Hash64, PalwStepLegError> {
    step_merkle_root_capped_v1(ordered_leaf_hashes, PALW_STEP_LEG_MAX_LEAVES)
}

/// [`step_merkle_root_v1`] against the ladder top the RULESET froze — `max_step_leaf_count`.
///
/// Nothing about the tree changes with the cap: the cap decides which trees this function will
/// build, never how it folds one, so a caller that passes [`PALW_STEP_LEG_MAX_LEAVES`] gets the
/// byte-identical root it got before the split.
pub fn step_merkle_root_capped_v1(ordered_leaf_hashes: &[Hash64], max_step_leaf_count: u64) -> Result<Hash64, PalwStepLegError> {
    let count = ordered_leaf_hashes.len() as u64;
    if count == 0 || count > max_step_leaf_count {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: count, max: max_step_leaf_count });
    }
    let mut level: Vec<Hash64> = ordered_leaf_hashes.iter().enumerate().map(|(i, leaf)| step_merkle_leaf(i as u64, leaf)).collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    Ok(level[0])
}

/// **The sibling path for one leaf of the tree [`step_merkle_root_v1`] builds** — the producing
/// side of the opening [`step_opening_root_v1`] verifies.
///
/// Exported beside the root builder rather than reimplemented by each carrier, because the
/// promote-odd shape lives in exactly two loops today and a third copy in a challenger is the
/// "second name-to-bytes mapping" class of defect: it fails silently as an opening nobody can
/// verify, or worse, verifies against the wrong tree.
pub fn step_merkle_path_v1(ordered_leaf_hashes: &[Hash64], index: usize) -> Result<Vec<Hash64>, PalwStepLegError> {
    step_merkle_path_capped_v1(ordered_leaf_hashes, index, PALW_STEP_LEG_MAX_LEAVES)
}

/// [`step_merkle_path_v1`] against the ruleset's `max_step_leaf_count`.
pub fn step_merkle_path_capped_v1(
    ordered_leaf_hashes: &[Hash64],
    mut index: usize,
    max_step_leaf_count: u64,
) -> Result<Vec<Hash64>, PalwStepLegError> {
    let count = ordered_leaf_hashes.len() as u64;
    if count == 0 || count > max_step_leaf_count {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: count, max: max_step_leaf_count });
    }
    if index as u64 >= count {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: index as u64, count });
    }
    let mut level: Vec<Hash64> = ordered_leaf_hashes.iter().enumerate().map(|(i, leaf)| step_merkle_leaf(i as u64, leaf)).collect();
    let mut path = Vec::new();
    while level.len() > 1 {
        let promoted = !level.len().is_multiple_of(2) && index == level.len() - 1;
        if !promoted {
            let sibling = if index.is_multiple_of(2) { index + 1 } else { index - 1 };
            path.push(level[sibling]);
        }
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        index /= 2;
        level = next;
    }
    Ok(path)
}

/// **A contiguous RANGE of leaves, opened as one subtree** — the carrier form that makes a court
/// evidence row cost one path instead of one path per leaf.
///
/// The canonical input set opens rows of consecutive tiles (the leaf enumeration puts a node's
/// tiles at consecutive indices), and the per-leaf opening charged each of them a full
/// `depth × 64` bytes of siblings: a 2,048-lane row at tile 8 paid 256 paths — 327 KiB — to
/// authenticate 8 KiB of lanes. A range needs at most two siblings per level while it is wider
/// than one node and one after, so the whole row rides `≲ (depth + log₂ k) × 64`.
///
/// The walk mirrors [`step_merkle_root_v1`]'s promote-odd shape exactly: per level, a left
/// sibling is consumed when the range starts on an odd node, then a right sibling when it ends
/// before an even boundary that is not the promoted odd tail. Sibling ORDER is part of the form:
/// left-then-right within a level, levels bottom-up.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepRangeOpeningV1 {
    pub first_leaf_index: u64,
    pub leaf_hashes: Vec<Hash64>,
    pub siblings: Vec<Hash64>,
}

/// Recomputes the root a valid range opening implies; the caller compares to the committed root.
/// Rejection means the evidence is about some other commitment, never a verdict.
pub fn step_range_opening_root_v1(leaf_count: u64, opening: &PalwStepRangeOpeningV1) -> Result<Hash64, PalwStepLegError> {
    step_range_opening_root_capped_v1(leaf_count, opening, PALW_STEP_LEG_MAX_LEAVES)
}

/// [`step_range_opening_root_v1`] against the ruleset's `max_step_leaf_count`. The range form
/// spends at most two siblings a level, so its depth cap is twice the single-leaf one — derived
/// from the same [`step_leg_max_opening_siblings_v1`], never restated.
pub fn step_range_opening_root_capped_v1(
    leaf_count: u64,
    opening: &PalwStepRangeOpeningV1,
    max_step_leaf_count: u64,
) -> Result<Hash64, PalwStepLegError> {
    if leaf_count == 0 || leaf_count > max_step_leaf_count {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: leaf_count, max: max_step_leaf_count });
    }
    let k = opening.leaf_hashes.len() as u64;
    if k == 0 || opening.first_leaf_index.checked_add(k).is_none_or(|end| end > leaf_count) {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: opening.first_leaf_index, count: leaf_count });
    }
    let sibling_cap = 2 * step_leg_max_opening_siblings_v1(max_step_leaf_count);
    if opening.siblings.len() > sibling_cap {
        return Err(PalwStepLegError::OpeningTooDeep { got: opening.siblings.len(), max: sibling_cap });
    }
    let mut nodes: Vec<Hash64> =
        opening.leaf_hashes.iter().enumerate().map(|(i, leaf)| step_merkle_leaf(opening.first_leaf_index + i as u64, leaf)).collect();
    let (mut a, mut b) = (opening.first_leaf_index, opening.first_leaf_index + k);
    let mut width = leaf_count;
    let mut siblings = opening.siblings.iter();
    let mut next = || siblings.next().copied().ok_or(PalwStepLegError::OpeningPathTooShort);
    while width > 1 {
        let mut level = Vec::with_capacity(nodes.len() / 2 + 2);
        // The left edge: an odd start pairs with a carried left sibling.
        let mut i = 0usize;
        if !a.is_multiple_of(2) {
            let left = next()?;
            level.push(keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[left.as_byte_slice(), nodes[0].as_byte_slice()]));
            i = 1;
        }
        // The interior pairs.
        while i + 1 < nodes.len() {
            level.push(keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[nodes[i].as_byte_slice(), nodes[i + 1].as_byte_slice()]));
            i += 2;
        }
        // The right edge: a lone last node either takes a right sibling, or promotes when it is
        // the level's odd tail — the same rule the builder applies.
        if i < nodes.len() {
            let promoted = !width.is_multiple_of(2) && b == width;
            if promoted {
                level.push(nodes[i]);
            } else {
                let right = next()?;
                level.push(keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[nodes[i].as_byte_slice(), right.as_byte_slice()]));
            }
        }
        nodes = level;
        a /= 2;
        b = b.div_ceil(2);
        width = width.div_ceil(2);
    }
    if siblings.next().is_some() {
        return Err(PalwStepLegError::OpeningPathTooLong { extra: 1 });
    }
    Ok(nodes[0])
}

/// The challenger side: the sibling set [`step_range_opening_root_v1`] consumes for
/// `[start, start + count)` of `ordered_leaf_hashes` — the same one-implementation rule
/// [`step_merkle_path_v1`] follows.
pub fn step_merkle_range_siblings_v1(
    ordered_leaf_hashes: &[Hash64],
    start: usize,
    count: usize,
) -> Result<Vec<Hash64>, PalwStepLegError> {
    step_merkle_range_siblings_capped_v1(ordered_leaf_hashes, start, count, PALW_STEP_LEG_MAX_LEAVES)
}

/// [`step_merkle_range_siblings_v1`] against the ruleset's `max_step_leaf_count`.
pub fn step_merkle_range_siblings_capped_v1(
    ordered_leaf_hashes: &[Hash64],
    start: usize,
    count: usize,
    max_step_leaf_count: u64,
) -> Result<Vec<Hash64>, PalwStepLegError> {
    let total = ordered_leaf_hashes.len() as u64;
    if total == 0 || total > max_step_leaf_count {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: total, max: max_step_leaf_count });
    }
    if count == 0 || start + count > ordered_leaf_hashes.len() {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: start as u64, count: total });
    }
    let mut level: Vec<Hash64> = ordered_leaf_hashes.iter().enumerate().map(|(i, l)| step_merkle_leaf(i as u64, l)).collect();
    let (mut a, mut b) = (start, start + count);
    let mut out = Vec::new();
    while level.len() > 1 {
        if !a.is_multiple_of(2) {
            out.push(level[a - 1]);
        }
        if !b.is_multiple_of(2) {
            let promoted = !level.len().is_multiple_of(2) && b == level.len();
            if !promoted {
                out.push(level[b]);
            }
        }
        let mut parent = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            parent.push(keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            parent.push(*odd);
        }
        level = parent;
        a /= 2;
        b = b.div_ceil(2);
    }
    Ok(out)
}

/// The sibling COUNT the range form needs, computable without the tree — the cost bound's side
/// of the one implementation (a bound that guessed would drift from the walk above).
pub fn step_range_sibling_count_v1(leaf_count: u64, first: u64, k: u64) -> u64 {
    let (mut a, mut b, mut width, mut n) = (first, first + k, leaf_count, 0u64);
    while width > 1 {
        if !a.is_multiple_of(2) {
            n += 1;
        }
        if !b.is_multiple_of(2) && (b != width || width.is_multiple_of(2)) {
            n += 1;
        }
        a /= 2;
        b = b.div_ceil(2);
        width = width.div_ceil(2);
    }
    n
}

/// Recomputes the root a valid opening implies (the caller compares to the committed root).
/// Promote levels are derived from `(leaf_index, leaf_count)` and consume nothing.
pub fn step_opening_root_v1(leaf_count: u64, opening: &PalwStepOpeningV1) -> Result<Hash64, PalwStepLegError> {
    step_opening_root_capped_v1(leaf_count, opening, PALW_STEP_LEG_MAX_LEAVES)
}

/// [`step_opening_root_v1`] against the ruleset's `max_step_leaf_count` — **the refusal that made
/// the deeper ladder unusable.** The sibling cap is `ceil(log2(max_step_leaf_count))`, so a court
/// running a `2^25` ladder accepts the 25-sibling path its own bisection asked for.
pub fn step_opening_root_capped_v1(
    leaf_count: u64,
    opening: &PalwStepOpeningV1,
    max_step_leaf_count: u64,
) -> Result<Hash64, PalwStepLegError> {
    if leaf_count == 0 || leaf_count > max_step_leaf_count {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: leaf_count, max: max_step_leaf_count });
    }
    if opening.leaf_index >= leaf_count {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: opening.leaf_index, count: leaf_count });
    }
    let sibling_cap = step_leg_max_opening_siblings_v1(max_step_leaf_count);
    if opening.siblings.len() > sibling_cap {
        return Err(PalwStepLegError::OpeningTooDeep { got: opening.siblings.len(), max: sibling_cap });
    }
    let mut current = step_merkle_leaf(opening.leaf_index, &opening.leaf_hash);
    let mut position = opening.leaf_index;
    let mut width = leaf_count;
    let mut siblings = opening.siblings.iter();
    while width > 1 {
        let promoted = !width.is_multiple_of(2) && position == width - 1;
        if !promoted {
            let Some(sibling) = siblings.next() else {
                return Err(PalwStepLegError::OpeningPathTooShort);
            };
            current = if position.is_multiple_of(2) {
                keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[current.as_byte_slice(), sibling.as_byte_slice()])
            } else {
                keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[sibling.as_byte_slice(), current.as_byte_slice()])
            };
        }
        position /= 2;
        width = width.div_ceil(2);
    }
    let leftover = siblings.count();
    if leftover != 0 {
        return Err(PalwStepLegError::OpeningPathTooLong { extra: leftover });
    }
    Ok(current)
}

/// Produces the membership proof of `leaf_index` from the same ordered leaf hashes the root
/// was built over. Commit / open / adjudicate are held together by the mirror test.
pub fn step_opening_v1(ordered_leaf_hashes: &[Hash64], leaf_index: u64) -> Result<PalwStepOpeningV1, PalwStepLegError> {
    step_opening_capped_v1(ordered_leaf_hashes, leaf_index, PALW_STEP_LEG_MAX_LEAVES)
}

/// [`step_opening_v1`] against the ruleset's `max_step_leaf_count`.
pub fn step_opening_capped_v1(
    ordered_leaf_hashes: &[Hash64],
    leaf_index: u64,
    max_step_leaf_count: u64,
) -> Result<PalwStepOpeningV1, PalwStepLegError> {
    let count = ordered_leaf_hashes.len() as u64;
    if count == 0 || count > max_step_leaf_count {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: count, max: max_step_leaf_count });
    }
    if leaf_index >= count {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: leaf_index, count });
    }
    let leaf_hash = ordered_leaf_hashes[leaf_index as usize];
    let mut level: Vec<Hash64> = ordered_leaf_hashes.iter().enumerate().map(|(i, leaf)| step_merkle_leaf(i as u64, leaf)).collect();
    let mut position = leaf_index as usize;
    let mut siblings = Vec::new();
    while level.len() > 1 {
        let width = level.len();
        let promoted = !width.is_multiple_of(2) && position == width - 1;
        if !promoted {
            siblings.push(level[position ^ 1]);
        }
        let mut next = Vec::with_capacity(width.div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        position /= 2;
        level = next;
    }
    Ok(PalwStepOpeningV1 { leaf_index, leaf_hash, siblings })
}

// ---------------------------------------------------------------------------------------------
// Leaf preimages
// ---------------------------------------------------------------------------------------------

/// One committed node-output tile: exact canonical f32-LE bytes at pinned coordinates.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepTileLeafV1 {
    pub version: u16,
    pub coord: PalwStepCoordinateV1,
    pub value_count: u32,
    /// `4 × value_count` little-endian f32 bytes.
    pub values_le: Vec<u8>,
}

pub fn step_tile_leaf_hash_v1(job_context_hash: &Hash64, shape_profile_hash: &Hash64, leaf: &PalwStepTileLeafV1) -> Hash64 {
    let mut w = Writer::new();
    w.u16(leaf.version);
    w.hash64(job_context_hash);
    w.hash64(shape_profile_hash);
    w.u32(leaf.coord.call_index);
    w.u32(leaf.coord.node_slot);
    w.u32(leaf.coord.position);
    w.u32(leaf.coord.tile_index);
    w.u32(leaf.value_count);
    w.bytes(&leaf.values_le);
    w.keyed64(PALW_STEP_LEG_DOMAIN_TILE_LEAF)
}

/// One committed KV aux chunk: the F16-LE cache rows of `position_count` consecutive
/// positions for one (attention layer, kv head, K|V) — ADR-0030 §3's openability series.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKvChunkLeafV1 {
    pub version: u16,
    pub attn_layer: u16,
    pub kv_head: u16,
    /// 0 = K, 1 = V.
    pub is_v: u8,
    pub chunk_index: u32,
    pub position_count: u32,
    /// `2 × head_dim × position_count` little-endian f16 bytes.
    pub values_f16_le: Vec<u8>,
}

pub fn kv_chunk_leaf_hash_v1(job_context_hash: &Hash64, shape_profile_hash: &Hash64, leaf: &PalwKvChunkLeafV1) -> Hash64 {
    let mut w = Writer::new();
    w.u16(leaf.version);
    w.hash64(job_context_hash);
    w.hash64(shape_profile_hash);
    w.u16(leaf.attn_layer);
    w.u16(leaf.kv_head);
    w.u8(leaf.is_v);
    w.u32(leaf.chunk_index);
    w.u32(leaf.position_count);
    w.bytes(&leaf.values_f16_le);
    w.keyed64(PALW_STEP_LEG_DOMAIN_KV_CHUNK_LEAF)
}

/// One state chunk of a v2 checkpoint (per-semantic-unit slice of the serialized replay
/// state). Bound to the measured chunk-map identity so the same bytes under two different
/// map claims are two different leaves.
pub fn state_chunk_leaf_hash_v1(state_chunk_map_id: &Hash64, chunk_index: u32, chunk_bytes: &[u8]) -> Hash64 {
    let mut w = Writer::new();
    w.hash64(state_chunk_map_id);
    w.u32(chunk_index);
    w.bytes(chunk_bytes);
    w.keyed64(PALW_STEP_LEG_DOMAIN_STATE_CHUNK_LEAF)
}

/// **The leaf [`state_chunks_root_v1`] folds for chunk `index`**: its chunk hash, index-bound
/// under the same domain.
///
/// Exported for the reason [`step_merkle_leaf_v1`] is: a per-chunk OPENING has to hash the same
/// leaf the root builder hashes, and the only two ways to arrange that are to export this or to
/// restate it. It was about to be restated (ADR-0082 Decision 4's bottom), and a restatement is a
/// second spelling of a consensus hash.
pub fn state_chunk_tree_leaf_v1(chunk_index: u32, chunk_hash: &Hash64) -> Hash64 {
    let mut w = Writer::new();
    w.u32(chunk_index);
    w.hash64(chunk_hash);
    w.keyed64(PALW_STEP_LEG_DOMAIN_STATE_CHUNK_LEAF)
}

/// **The sibling path proving ONE chunk is in [`state_chunks_root_v1`]** — the producing side of
/// [`state_chunk_opening_root_v1`], and the primitive ADR-0082 Decision 4's bottom needs.
///
/// Without it the only evidence about a checkpoint's state is `PalwCheckpointKvOperandsV1`, which
/// carries `chunks: Vec<Vec<u8>>` — EVERY chunk of the checkpoint. Under the graph-v4 tiled map a
/// chunk is a sixteen-position tile, so a court that wanted one tile still had to be handed the
/// whole history: tiling the map made the chunk small and left it unopenable. This is the missing
/// half.
///
/// Promote-odd, left to right, exactly as [`state_chunks_root_v1`] folds — the same loop, and the
/// pin that says so is `a_state_chunk_opening_is_the_root_the_builder_builds`.
pub fn state_chunk_path_v1(chunk_hashes: &[Hash64], mut index: usize) -> Result<Vec<Hash64>, PalwStepLegError> {
    if chunk_hashes.is_empty() || chunk_hashes.len() > PALW_STEP_LEG_MAX_STATE_CHUNKS {
        return Err(PalwStepLegError::StateChunksOutOfRange { got: chunk_hashes.len(), max: PALW_STEP_LEG_MAX_STATE_CHUNKS });
    }
    if index >= chunk_hashes.len() {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: index as u64, count: chunk_hashes.len() as u64 });
    }
    let mut level: Vec<Hash64> = chunk_hashes.iter().enumerate().map(|(i, h)| state_chunk_tree_leaf_v1(i as u32, h)).collect();
    let mut path = Vec::new();
    while level.len() > 1 {
        let promoted = !level.len().is_multiple_of(2) && index == level.len() - 1;
        if !promoted {
            let sibling = if index.is_multiple_of(2) { index + 1 } else { index - 1 };
            path.push(level[sibling]);
        }
        level = state_chunk_fold_level_v1(&level);
        index /= 2;
    }
    Ok(path)
}

/// **Recompute [`state_chunks_root_v1`] from ONE chunk and its path.** The caller compares the
/// answer with the checkpoint leaf's `state_chunks_root`; a mismatch is "this chunk is not in that
/// checkpoint", which is the only question a tile-addressed opening asks.
///
/// Promote levels are derived from `(chunk_index, chunk_count)` and consume no sibling — the same
/// discipline [`step_opening_root_capped_v1`] states, and the reason a path can be checked without
/// the tree.
pub fn state_chunk_opening_root_v1(
    chunk_count: usize,
    chunk_index: u32,
    chunk_hash: &Hash64,
    siblings: &[Hash64],
) -> Result<Hash64, PalwStepLegError> {
    if chunk_count == 0 || chunk_count > PALW_STEP_LEG_MAX_STATE_CHUNKS {
        return Err(PalwStepLegError::StateChunksOutOfRange { got: chunk_count, max: PALW_STEP_LEG_MAX_STATE_CHUNKS });
    }
    if chunk_index as usize >= chunk_count {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: chunk_index as u64, count: chunk_count as u64 });
    }
    let mut current = state_chunk_tree_leaf_v1(chunk_index, chunk_hash);
    let mut position = chunk_index as usize;
    let mut width = chunk_count;
    let mut siblings = siblings.iter();
    while width > 1 {
        let promoted = !width.is_multiple_of(2) && position == width - 1;
        if !promoted {
            let Some(sibling) = siblings.next() else {
                return Err(PalwStepLegError::OpeningPathTooShort);
            };
            current = if position.is_multiple_of(2) {
                keyed64(PALW_STEP_LEG_DOMAIN_STATE_CHUNK_NODE, &[current.as_byte_slice(), sibling.as_byte_slice()])
            } else {
                keyed64(PALW_STEP_LEG_DOMAIN_STATE_CHUNK_NODE, &[sibling.as_byte_slice(), current.as_byte_slice()])
            };
        }
        position /= 2;
        width = width.div_ceil(2);
    }
    let leftover = siblings.count();
    if leftover != 0 {
        return Err(PalwStepLegError::OpeningPathTooLong { extra: leftover });
    }
    Ok(current)
}

/// One level of the state-chunk tree's fold: pair left to right, promote a lone odd tail. The one
/// loop [`state_chunks_root_v1`] and [`state_chunk_path_v1`] share, so a change to the shape moves
/// both or neither.
fn state_chunk_fold_level_v1(level: &[Hash64]) -> Vec<Hash64> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    let mut chunks = level.chunks_exact(2);
    for pair in &mut chunks {
        next.push(keyed64(PALW_STEP_LEG_DOMAIN_STATE_CHUNK_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
    }
    if let [odd] = chunks.remainder() {
        next.push(*odd);
    }
    next
}

/// Root over a checkpoint's state chunk hashes (small tree; the v1 leg discipline at its own
/// domains, via the step-tree functions on a u64-capped width).
pub fn state_chunks_root_v1(chunk_hashes: &[Hash64]) -> Result<Hash64, PalwStepLegError> {
    if chunk_hashes.is_empty() || chunk_hashes.len() > PALW_STEP_LEG_MAX_STATE_CHUNKS {
        return Err(PalwStepLegError::StateChunksOutOfRange { got: chunk_hashes.len(), max: PALW_STEP_LEG_MAX_STATE_CHUNKS });
    }
    let mut level: Vec<Hash64> = chunk_hashes.iter().enumerate().map(|(i, h)| state_chunk_tree_leaf_v1(i as u32, h)).collect();
    while level.len() > 1 {
        level = state_chunk_fold_level_v1(&level);
    }
    Ok(level[0])
}

/// A v2 checkpoint leaf: the v1 chain discipline with the flat state root replaced by the
/// chunked one.
///
/// # `covered_decode_call` counts what the CLASS's map says it counts (ADR-0082 Decision 4)
///
/// There is no v3 leaf and there does not need to be one. Under
/// [`crate::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall`] — every shipped class —
/// the field is decode calls, `(index + 1) × interval`. Under `PerPosition`, which a class
/// registering the tiled attention map runs at, it is POSITIONS of the cache, `index + 1`, prefill
/// positions included.
///
/// The two are told apart without a second field because [`checkpoint_leaf_hash_v2`] already binds
/// `state_chunk_map_id` into the preimage beside the counter, and the map id is what chooses the
/// cadence: the same numbers under two maps are two different leaf hashes. A version field would
/// be a second name for a fact the first one already fixes, and two names for one fact is how a
/// producer commits at one cadence and a court judges at another.
///
/// The wire form is byte-for-byte the shipped one, so every shipped row's checkpoint leg is the
/// leg it files today — `the_shipped_cadence_is_per_call_and_its_leaves_do_not_move`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCheckpointLeafV2 {
    pub version: u16,
    pub checkpoint_index: u32,
    /// Decode calls covered, or — for a class whose map addresses history tiles — POSITIONS
    /// covered. [`crate::palw_context_ladder::palw_checkpoint_covered_at_index_v1`] is the rule.
    pub covered_decode_call: u32,
    pub prev_checkpoint_leaf_hash: Hash64,
    pub state_chunk_count: u32,
    pub state_chunks_root: Hash64,
}

pub fn checkpoint_leaf_hash_v2(
    job_context_hash: &Hash64,
    checkpoint_profile_hash: &Hash64,
    state_chunk_map_id: &Hash64,
    leaf: &PalwCheckpointLeafV2,
) -> Hash64 {
    let mut w = Writer::new();
    w.u16(leaf.version);
    w.hash64(job_context_hash);
    w.hash64(checkpoint_profile_hash);
    w.hash64(state_chunk_map_id);
    w.u32(leaf.checkpoint_index);
    w.u32(leaf.covered_decode_call);
    w.hash64(&leaf.prev_checkpoint_leaf_hash);
    w.u32(leaf.state_chunk_count);
    w.hash64(&leaf.state_chunks_root);
    w.keyed64(PALW_STEP_LEG_DOMAIN_CHECKPOINT_LEAF_V2)
}

/// Job-bound chain genesis (v2 domain — a v1 chain cannot be grafted here).
pub fn checkpoint_genesis_prev_v2(job_context_hash: &Hash64) -> Hash64 {
    keyed64(PALW_STEP_LEG_DOMAIN_CHECKPOINT_GENESIS_V2, &[job_context_hash.as_byte_slice()])
}

/// The empty-leg sentinel: `count == 0 ⟺ merkle_root == this` is the canonical-form rule.
pub fn checkpoint_empty_root_v2(job_context_hash: &Hash64) -> Hash64 {
    keyed64(PALW_STEP_LEG_DOMAIN_CHECKPOINT_EMPTY_V2, &[job_context_hash.as_byte_slice()])
}

// ---------------------------------------------------------------------------------------------
// Leg roots and the composite
// ---------------------------------------------------------------------------------------------

/// Outer hash of the step leg: context, the FULL shape-profile identity, the canonical count,
/// and the tree.
pub fn step_leg_root_v1(job_context_hash: &Hash64, shape_profile_hash: &Hash64, leaf_count: u64, merkle_root: &Hash64) -> Hash64 {
    let mut w = Writer::new();
    w.u16(PALW_STEP_LEG_OBJECT_VERSION_V1);
    w.hash64(job_context_hash);
    w.hash64(shape_profile_hash);
    w.u64(leaf_count);
    w.hash64(merkle_root);
    w.keyed64(PALW_STEP_LEG_DOMAIN_LEG)
}

/// Outer hash of the v2 checkpoint leg (chunked state).
pub fn checkpoint_leg_root_v2(
    job_context_hash: &Hash64,
    checkpoint_profile_hash: &Hash64,
    state_chunk_map_id: &Hash64,
    decode_calls: u32,
    checkpoint_count: u32,
    merkle_root: &Hash64,
) -> Hash64 {
    let mut w = Writer::new();
    w.u16(PALW_STEP_LEG_OBJECT_VERSION_V1);
    w.hash64(job_context_hash);
    w.hash64(checkpoint_profile_hash);
    w.hash64(state_chunk_map_id);
    w.u32(decode_calls);
    w.u32(checkpoint_count);
    w.hash64(merkle_root);
    w.keyed64(PALW_STEP_LEG_DOMAIN_CHECKPOINT_LEG_V2)
}

/// The v2 composite: the frozen v2 logits root, the v1 activation leg root **as computed by
/// the frozen v1 code**, the chunked checkpoint leg, and the step leg — context-bound.
/// Profile hashes live inside their leg roots (the dual-source rule).
pub fn execution_commitment_root_v2(
    job_context_hash: &Hash64,
    full_logits_trace_root: &Hash64,
    activation_leg_root: &Hash64,
    checkpoint_leg_root: &Hash64,
    step_leg_root: &Hash64,
) -> Hash64 {
    let mut w = Writer::new();
    w.u16(PALW_STEP_LEG_OBJECT_VERSION_V1);
    w.hash64(job_context_hash);
    w.hash64(full_logits_trace_root);
    w.hash64(activation_leg_root);
    w.hash64(checkpoint_leg_root);
    w.hash64(step_leg_root);
    w.keyed64(PALW_STEP_LEG_DOMAIN_EXECUTION_COMMITMENT_V2)
}

// ---------------------------------------------------------------------------------------------
// Producer: the fail-closed step-leg builder
// ---------------------------------------------------------------------------------------------

#[inline]
fn f32_is_finite_bits(bits: u32) -> bool {
    (bits & 0x7F80_0000) != 0x7F80_0000
}

#[inline]
fn f16_is_finite_bits(bits: u16) -> bool {
    (bits & 0x7C00) != 0x7C00
}

/// Streaming builder for the step tree. Tiles must arrive in canonical leaf order with
/// canonical lengths and finite values; KV chunks follow in the canonical aux order. Every
/// violation is a construction failure — the fifteen v1 faults' step-side counterparts are
/// unbuildable, not carried.
pub struct PalwStepLegBuilderV1 {
    context_hash: Hash64,
    profile_hash: Hash64,
    profile: PalwShapeProfileV3,
    context: PalwJobContextV2,
    expected_total: u64,
    expected_aux: u64,
    leaf_hashes: Vec<Hash64>,
}

impl PalwStepLegBuilderV1 {
    /// **The DEFAULT ladder, for a builder with no ruleset in scope** — see
    /// [`PALW_STEP_LEG_MAX_LEAVES`]. A producer that knows which network it serves calls
    /// [`Self::new_capped_v1`] with `PalwCourtParamsV2::max_step_leaf_count`; building at the
    /// executor's constant on a `2^26` network refuses the class's own honest job.
    pub fn new(context: PalwJobContextV2, profile: PalwShapeProfileV3) -> Result<Self, PalwStepLegError> {
        Self::new_capped_v1(context, profile, PALW_STEP_LEG_MAX_LEAVES)
    }

    /// [`Self::new`] against the ladder the RULESET froze (ADR-0080 W1b, ADR-0082 Decision 1).
    ///
    /// The count this sizes the tree with is the same enumeration the court's shape pass
    /// recomputes, so a leg built at one ladder and judged at another is a leg whose honest
    /// producer is convicted for a number nobody disagreed with.
    pub fn new_capped_v1(
        context: PalwJobContextV2,
        profile: PalwShapeProfileV3,
        max_step_leaf_count: u64,
    ) -> Result<Self, PalwStepLegError> {
        check_job_context_shape(&context).map_err(PalwStepLegError::Context)?;
        profile.validate_shape()?;
        let expected_total = crate::palw_step::step_leaf_count_capped_v1(&profile, &context, max_step_leaf_count)?;
        let expected_aux = kv_aux_leaf_count(&profile, &context);
        Ok(Self {
            context_hash: context.context_hash(),
            profile_hash: profile.shape_profile_id(),
            profile,
            context,
            expected_total,
            expected_aux,
            leaf_hashes: Vec::new(),
        })
    }

    pub fn expected_main_leaves(&self) -> u64 {
        self.expected_total - self.expected_aux
    }

    pub fn expected_total_leaves(&self) -> u64 {
        self.expected_total
    }

    /// Canonical tile length at `coord`: the node's `tile_len` except a ragged last tile.
    fn canonical_tile_values(&self, coord: &PalwStepCoordinateV1) -> Option<u32> {
        let (node, _layer) = self.profile.resolve_node_slot(coord.node_slot)?;
        let kv_len = if coord.call_index == 0 {
            coord.position as u64 + 1
        } else {
            self.context.declared_prefill_tokens as u64 + coord.call_index as u64
        };
        let len = match node.out_len {
            crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements as u64,
            crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
        };
        let tiles = len.div_ceil(node.tile_len as u64);
        if (coord.tile_index as u64) >= tiles {
            return None;
        }
        let start = coord.tile_index as u64 * node.tile_len as u64;
        Some((len - start).min(node.tile_len as u64) as u32)
    }

    /// Push the next node-output tile (canonical order enforced).
    pub fn push_step_tile(&mut self, coord: PalwStepCoordinateV1, value_bits: &[u32]) -> Result<u64, PalwStepLegError> {
        let next = self.leaf_hashes.len() as u64;
        if next >= self.expected_main_leaves() {
            return Err(PalwStepLegError::TilesOutOfOrder { expected: self.expected_main_leaves(), got: next });
        }
        let canonical =
            canonical_step_leaf_index(&self.profile, &self.context, &coord).ok_or(PalwStepLegError::TileCoordinatesNotCanonical)?;
        if canonical != next {
            return Err(PalwStepLegError::TilesOutOfOrder { expected: next, got: canonical });
        }
        let expected_values = self.canonical_tile_values(&coord).ok_or(PalwStepLegError::TileCoordinatesNotCanonical)?;
        if value_bits.len() as u32 != expected_values {
            return Err(PalwStepLegError::TileLengthNotCanonical { got: value_bits.len() as u32, expected: expected_values });
        }
        let mut values_le = Vec::with_capacity(value_bits.len() * 4);
        // The finiteness rule belongs to FLOAT lanes only. On an integer class every bit pattern
        // is a legal value, and applying the float rule there rejected every activation in
        // `[-8_388_608, -1]` — the all-ones-exponent range — which is essentially every negative
        // BASE-0 code. The RC's liveness floor could not commit a leg at all.
        let check_finite = self.profile.lane == crate::palw_step::PalwStepLaneV1::Float32;
        for (i, bits) in value_bits.iter().enumerate() {
            if check_finite && !f32_is_finite_bits(*bits) {
                return Err(PalwStepLegError::NonFiniteStepValue { leaf_index: next, value_index: i as u32 });
            }
            values_le.extend_from_slice(&bits.to_le_bytes());
        }
        let leaf = PalwStepTileLeafV1 { version: PALW_STEP_LEG_OBJECT_VERSION_V1, coord, value_count: expected_values, values_le };
        self.leaf_hashes.push(step_tile_leaf_hash_v1(&self.context_hash, &self.profile_hash, &leaf));
        Ok(next)
    }

    /// Push the next KV aux chunk (canonical aux order: attention layer ↑, kv head ↑, K then
    /// V, chunk ↑ — enforced by rank).
    pub fn push_kv_chunk(&mut self, leaf: &PalwKvChunkLeafV1) -> Result<u64, PalwStepLegError> {
        let next = self.leaf_hashes.len() as u64;
        let main = self.expected_main_leaves();
        if next < main || next >= self.expected_total {
            return Err(PalwStepLegError::KvChunksOutOfOrder);
        }
        if leaf.version != PALW_STEP_LEG_OBJECT_VERSION_V1 {
            return Err(PalwStepLegError::UnsupportedVersion { got: leaf.version, expected: PALW_STEP_LEG_OBJECT_VERSION_V1 });
        }
        let rank = self.kv_chunk_rank(leaf).ok_or(PalwStepLegError::KvChunksOutOfOrder)?;
        if main + rank != next {
            return Err(PalwStepLegError::KvChunksOutOfOrder);
        }
        if leaf.values_f16_le.len() > PALW_STEP_LEG_MAX_KV_CHUNK_BYTES {
            return Err(PalwStepLegError::PayloadTooLarge { got: leaf.values_f16_le.len(), max: PALW_STEP_LEG_MAX_KV_CHUNK_BYTES });
        }
        if leaf.values_f16_le.len() != 2 * leaf.position_count as usize * self.profile.attn_head_dim as usize
            || !leaf.values_f16_le.len().is_multiple_of(2)
        {
            return Err(PalwStepLegError::KvChunksOutOfOrder);
        }
        for (i, pair) in leaf.values_f16_le.chunks_exact(2).enumerate() {
            let bits = u16::from_le_bytes([pair[0], pair[1]]);
            if !f16_is_finite_bits(bits) {
                return Err(PalwStepLegError::NonFiniteKvValue { leaf_index: next, value_index: i as u32 });
            }
        }
        self.leaf_hashes.push(kv_chunk_leaf_hash_v1(&self.context_hash, &self.profile_hash, leaf));
        Ok(next)
    }

    /// Rank of a KV chunk in the canonical aux order, with canonical `position_count`
    /// (full chunks except a ragged last).
    fn kv_chunk_rank(&self, leaf: &PalwKvChunkLeafV1) -> Option<u64> {
        if self.profile.kv_chunk_calls == 0 || leaf.is_v > 1 {
            return None;
        }
        let attn_layers: Vec<u16> =
            (0..self.profile.layer_count).filter(|&l| self.profile.layer_kind(l) == PalwLayerKindV1::Attention).collect();
        let layer_ordinal = attn_layers.iter().position(|&l| l == leaf.attn_layer)? as u64;
        if leaf.kv_head >= self.profile.attn_kv_heads {
            return None;
        }
        let positions = self.context.declared_prefill_tokens as u64 + self.context.exact_decode_tokens.saturating_sub(1) as u64;
        let chunks = positions.div_ceil(self.profile.kv_chunk_calls as u64);
        if (leaf.chunk_index as u64) >= chunks {
            return None;
        }
        let start = leaf.chunk_index as u64 * self.profile.kv_chunk_calls as u64;
        let canonical_count = (positions - start).min(self.profile.kv_chunk_calls as u64) as u32;
        if leaf.position_count != canonical_count {
            return None;
        }
        let per_layer = self.profile.attn_kv_heads as u64 * 2 * chunks;
        Some(layer_ordinal * per_layer + (leaf.kv_head as u64 * 2 + leaf.is_v as u64) * chunks + leaf.chunk_index as u64)
    }

    /// Finishes the tree: every mandated leaf must have arrived.
    pub fn finish(self) -> Result<PalwStepLegMaterialV1, PalwStepLegError> {
        let got = self.leaf_hashes.len() as u64;
        if got != self.expected_total {
            return Err(PalwStepLegError::LegIncomplete { what: "step", got, expected: self.expected_total });
        }
        let merkle_root = step_merkle_root_v1(&self.leaf_hashes)?;
        let leg_root = step_leg_root_v1(&self.context_hash, &self.profile_hash, got, &merkle_root);
        Ok(PalwStepLegMaterialV1 { leaf_hashes: self.leaf_hashes, merkle_root, leg_root, leaf_count: got })
    }
}

/// The finished step leg plus the tree material an answering executor retains.
pub struct PalwStepLegMaterialV1 {
    pub leaf_hashes: Vec<Hash64>,
    pub merkle_root: Hash64,
    pub leg_root: Hash64,
    pub leaf_count: u64,
}

// ---------------------------------------------------------------------------------------------
// Adjudication: binding + structural faults
// ---------------------------------------------------------------------------------------------

/// The pinned rule a step-leg refutation proves broken. Discriminants wire-frozen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepFaultV1 {
    ShapeProfileNotCanonical = 0,
    StepLeafCountNotCanonical = 1,
    StepCoordinatesNotCanonical = 2,
    StepLeafIndexNotCanonical = 3,
    StepBytesNotFourPerValue = 4,
    StepValueCountNotCanonical = 5,
    StepNonFinite {
        value_index: u32,
    } = 6,
    KvChunkNotCanonical = 7,
    KvChunkBytesNotCanonical = 8,
    KvChunkNonFinite {
        value_index: u32,
    } = 9,
    CheckpointCountNotCanonical = 10,
    CheckpointIndexNotCanonical = 11,
    CheckpointCoveredCallNotCanonical = 12,
    CheckpointGenesisPrevMismatch = 13,
    CheckpointChainBroken = 14,
    /// ADR-0027 §1's arithmetic verdict: the step's committed output tile differs from the
    /// canonical recomputation at `value_index` (added same-session with the step-refutation
    /// increment; discriminants 0-14 unmoved).
    ComputationMismatch {
        value_index: u32,
    } = 15,
    /// ADR-0049 Decision E's verdict: the committed decode token at `position` is not what the
    /// pinned selection rule (`base0_decode_token_select_v1`) produces from that position's own
    /// committed logits row (discriminants 0-15 unmoved).
    DecodeTokenMismatch {
        position: u32,
    } = 16,
    /// The binding's checkpoint profile is not the family's canonical one — a free interval let a
    /// producer file zero checkpoints, and made two honest parties compute different execution
    /// roots for the same job (discriminants 0-16 unmoved).
    CheckpointProfileNotCanonical = 17,
    /// **The committed job exceeds the class's registered context.** The class's every court cost
    /// is derived over `profile.n_ctx`; a job whose footprint — `prefill + exact_decode − 1`
    /// cached positions, the enumeration's own count from `step_leaf_count` — exceeds it is work
    /// the ceilings were never derived over, and a close against it could exceed the carrier
    /// while the claim stood unprosecutable. So the OVERSIZED COMMITMENT ITSELF is the fault,
    /// convictable from the binding alone: the profile is authenticated against the committed
    /// root, the job context is too, and the comparison is two integers (discriminants 0-17
    /// unmoved).
    JobExceedsClassContext = 18,
}

impl PalwStepFaultV1 {
    fn evidence_words(self) -> (u8, u32) {
        match self {
            PalwStepFaultV1::ShapeProfileNotCanonical => (0, 0),
            PalwStepFaultV1::StepLeafCountNotCanonical => (1, 0),
            PalwStepFaultV1::CheckpointProfileNotCanonical => (17, 0),
            PalwStepFaultV1::StepCoordinatesNotCanonical => (2, 0),
            PalwStepFaultV1::StepLeafIndexNotCanonical => (3, 0),
            PalwStepFaultV1::StepBytesNotFourPerValue => (4, 0),
            PalwStepFaultV1::StepValueCountNotCanonical => (5, 0),
            PalwStepFaultV1::StepNonFinite { value_index } => (6, value_index),
            PalwStepFaultV1::KvChunkNotCanonical => (7, 0),
            PalwStepFaultV1::KvChunkBytesNotCanonical => (8, 0),
            PalwStepFaultV1::KvChunkNonFinite { value_index } => (9, value_index),
            PalwStepFaultV1::CheckpointCountNotCanonical => (10, 0),
            PalwStepFaultV1::CheckpointIndexNotCanonical => (11, 0),
            PalwStepFaultV1::CheckpointCoveredCallNotCanonical => (12, 0),
            PalwStepFaultV1::CheckpointGenesisPrevMismatch => (13, 0),
            PalwStepFaultV1::CheckpointChainBroken => (14, 0),
            PalwStepFaultV1::ComputationMismatch { value_index } => (15, value_index),
            PalwStepFaultV1::DecodeTokenMismatch { position } => (16, position),
            PalwStepFaultV1::JobExceedsClassContext => (18, 0),
        }
    }
}

/// The transparent preimage of a committed v2 execution commitment: everything needed to
/// recompute [`execution_commitment_root_v2`] from parts. The activation leg rides opaquely
/// as its v1 leg root (its own refutations live in `palw_legs`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepBindingV2 {
    pub version: u16,
    pub job_context: PalwJobContextV2,
    /// Carried in full: adjudication needs the node tables, and the id is recomputed, never
    /// trusted.
    pub shape_profile: PalwShapeProfileV3,
    pub checkpoint_profile: PalwCheckpointProfileV1,
    pub state_chunk_map_id: Hash64,
    pub full_logits_trace_root: Hash64,
    pub activation_leg_root: Hash64,
    pub step_leaf_count: u64,
    pub step_merkle_root: Hash64,
    pub checkpoint_count: u32,
    pub checkpoint_merkle_root: Hash64,
    pub committed_execution_root: Hash64,
}

/// The variable half: which committed object is refuted, with openings. Checkpoint openings
/// ride the step-side tree functions (the v2 checkpoint tree uses the SAME generalized
/// discipline at its own width, kept small: one leaf per checkpoint).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwStepEvidenceV1 {
    /// No opening needed: decidable from the binding alone.
    Shape = 0,
    /// One opened step-tree leaf claimed to be a node-output tile.
    StepTile { opening: PalwStepOpeningV1, preimage: PalwStepTileLeafV1 } = 1,
    /// One opened step-tree leaf claimed to be a KV aux chunk.
    KvChunk { opening: PalwStepOpeningV1, preimage: PalwKvChunkLeafV1 } = 2,
    /// One opened checkpoint leaf.
    Checkpoint { opening: PalwStepOpeningV1, preimage: PalwCheckpointLeafV2 } = 3,
    /// Two adjacent opened checkpoints; convicts iff their hashes do not chain.
    CheckpointChain {
        earlier_opening: PalwStepOpeningV1,
        earlier_preimage: PalwCheckpointLeafV2,
        later_opening: PalwStepOpeningV1,
        later_preimage: PalwCheckpointLeafV2,
    } = 4,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStepRefutationV1 {
    pub binding: PalwStepBindingV2,
    pub evidence: PalwStepEvidenceV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStepRefutationVerdictV1 {
    pub fault: PalwStepFaultV1,
    pub evidence_id: Hash64,
}

/// The §24.1 dedup key of a step-family refutation — public so the arithmetic checker
/// (`palw_step_refute`) mints ids in the same namespace (evidence kind 5).
pub fn step_refutation_evidence_id(committed_root: &Hash64, evidence_kind: u8, leaf_index: u64, fault: PalwStepFaultV1) -> Hash64 {
    evidence_id(committed_root, evidence_kind, leaf_index, fault)
}

fn evidence_id(committed_root: &Hash64, evidence_kind: u8, leaf_index: u64, fault: PalwStepFaultV1) -> Hash64 {
    let (code, argument) = fault.evidence_words();
    let mut w = Writer::new();
    w.hash64(committed_root);
    w.u8(evidence_kind);
    w.u64(leaf_index);
    w.u8(code);
    w.u32(argument);
    w.keyed64(PALW_STEP_LEG_DOMAIN_EVIDENCE_ID)
}

/// Recomputes the committed root from the binding; returns (context hash, profile hash,
/// checkpoint profile hash). Rejection here means the evidence is about some other
/// commitment — never that the commitment is honest. A malformed shape PROFILE, by contrast,
/// is itself the fault (checked by the caller against the recomputed root first).
/// [`verify_binding`], public: the decode-token adjudication (ADR-0049 Decision E) pins a
/// binding to a claim's `execution_root` and then needs exactly this recomputation — the same
/// one every structural and arithmetic arm runs — without carrying a leaf to open.
pub fn verify_binding_v1(binding: &PalwStepBindingV2) -> Result<(Hash64, Hash64, Hash64), PalwStepLegError> {
    verify_binding(binding)
}

fn verify_binding(binding: &PalwStepBindingV2) -> Result<(Hash64, Hash64, Hash64), PalwStepLegError> {
    if binding.version != PALW_STEP_LEG_OBJECT_VERSION_V1 {
        return Err(PalwStepLegError::UnsupportedVersion { got: binding.version, expected: PALW_STEP_LEG_OBJECT_VERSION_V1 });
    }
    check_job_context_shape(&binding.job_context).map_err(PalwStepLegError::Context)?;
    let context_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    // The binding must carry the SAME profile the job declared. Without this the adjudicator
    // recomputed a step under whatever profile the binding happened to include — different
    // geometry, different epsilons, different kernel ids — and convicted the producer for not
    // matching arithmetic its job never claimed (re-audit §3.3). The context already names the
    // profile; it was simply never compared.
    if profile_hash != binding.job_context.shape_profile_id {
        return Err(PalwStepLegError::ShapeProfileNotTheDeclaredOne {
            declared: binding.job_context.shape_profile_id,
            got: profile_hash,
        });
    }
    // **The same defect one field over.** The binding carries `state_chunk_map_id` beside a
    // profile that already registers one, and every checkpoint hash below is computed FROM the
    // carried copy — `checkpoint_leaf_hash_v2` and `checkpoint_leg_root_v2` both take it as an
    // argument. So a filer could carry any map id it liked and build a checkpoint leg that
    // verifies against itself perfectly, under a layout the class never registered; the court
    // would then read state bytes through the accuser's geometry.
    //
    // It has been invisible because every shipped class registers `Hash64::default()` and every
    // honest filer copies it, so the two sources have never disagreed. Registering a real map is
    // exactly what would end that, which is why this lands before the registration and not after.
    if binding.state_chunk_map_id != binding.shape_profile.state_chunk_map_id {
        return Err(PalwStepLegError::StateChunkMapNotTheRegisteredOne {
            registered: binding.shape_profile.state_chunk_map_id,
            carried: binding.state_chunk_map_id,
        });
    }
    let checkpoint_profile_hash = binding.checkpoint_profile.profile_hash();
    let decode_calls = binding.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&context_hash, &profile_hash, binding.step_leaf_count, &binding.step_merkle_root);
    let checkpoint_root = checkpoint_leg_root_v2(
        &context_hash,
        &checkpoint_profile_hash,
        &binding.state_chunk_map_id,
        decode_calls,
        binding.checkpoint_count,
        &binding.checkpoint_merkle_root,
    );
    let recomputed = execution_commitment_root_v2(
        &context_hash,
        &binding.full_logits_trace_root,
        &binding.activation_leg_root,
        &checkpoint_root,
        &step_root,
    );
    if recomputed != binding.committed_execution_root {
        return Err(PalwStepLegError::CommittedRootMismatch);
    }
    Ok((context_hash, profile_hash, checkpoint_profile_hash))
}

/// Model-free adjudication of a structural step-leg refutation. Honest material yields
/// `NoFaultFound` on every arm (the challenger-loses verdict); arithmetic recomputation
/// (`ExecutionStepRefutationV1`) is the separate, catalog-bound increment.
pub fn check_step_refutation_v1(refutation: &PalwStepRefutationV1) -> Result<PalwStepRefutationVerdictV1, PalwStepLegError> {
    let binding = &refutation.binding;
    let (context_hash, profile_hash, checkpoint_profile_hash) = verify_binding(binding)?;
    let committed = &binding.committed_execution_root;

    // Shape-level rules first: a profile that fails its own validation, or counts that are
    // not the canonical function of (profile, context), convict from the binding alone.
    let shape_fault = (|| {
        if binding.shape_profile.validate_shape().is_err() {
            return Some(PalwStepFaultV1::ShapeProfileNotCanonical);
        }
        if binding.checkpoint_profile.validate_shape().is_err() {
            return Some(PalwStepFaultV1::ShapeProfileNotCanonical);
        }
        // The class-context bound, in the enumeration's own form — AFTER the profile's own
        // validation (an ill-formed profile is its own fault and its n_ctx means nothing), BEFORE
        // the counting walks the enumeration the bound exists to cap, so an oversized job
        // convicts as WHAT IT IS rather than as whatever its leaf count happens to disagree with.
        let footprint = (binding.job_context.declared_prefill_tokens as u64)
            .saturating_add(binding.job_context.exact_decode_tokens.max(1) as u64)
            .saturating_sub(1);
        if footprint > binding.shape_profile.n_ctx as u64 {
            return Some(PalwStepFaultV1::JobExceedsClassContext);
        }
        // **The cap is the ACCUSATION's own claim, not the executor's constant** (ADR-0082
        // Decision 1: "the ruleset's is read from the bundle, never typed").
        //
        // The rule on this line is an EQUALITY — the committed count must be the canonical
        // function of `(profile, context)` — and the cap was only ever the enumeration's own
        // bound. Spelling it `PALW_STEP_MAX_LEAVES` made the shape pass answer
        // `StepLeafCountNotCanonical` for every binding above `2^22`, which on a `2^26` ruleset is
        // a CONVICTION of the honest producer of a class the admission gate accepted: the graph-v5
        // dense 512 row's canonical job counts 6,630,544 leaves, so its every refutation — the
        // honest one included — convicted, and the family could not certify (`HonestRunConvicted`)
        // let alone be prosecuted.
        //
        // `binding.step_leaf_count` is the exact cap this comparison needs and it needs no
        // ruleset: the walk returns `Ok(n)` iff the true count is at most the claim, so
        // `Ok(n) && n == claim` is the same predicate at every ladder, and a claim the enumeration
        // overruns still convicts by the same name. `step_leaf_count_capped_v1` is a closed form
        // (at most 256 node visits, plus `⌈log₂⌉` evaluations to locate an overrun), so a
        // stranger's inflated claim buys no walk.
        match crate::palw_step::step_leaf_count_capped_v1(&binding.shape_profile, &binding.job_context, binding.step_leaf_count) {
            Ok(count) if count == binding.step_leaf_count => {}
            _ => return Some(PalwStepFaultV1::StepLeafCountNotCanonical),
        }
        // **The state layout is the family's, not the filer's.**
        //
        // The checkpoint profile is hashed into `committed_execution_root` and nothing checked any
        // of it. `state_layout_id` is the half that has a canonical answer — one layout for the
        // whole deterministic-integer family, which is exactly why
        // `integer_kv_state_layout_id_v1` exists ("before this there was no canonical
        // `state_layout_id` to file at all, so every producer would have invented one and every
        // one of them would have been a different class of checkpoint"). An invented layout makes
        // an honest execution unreproducible by anyone else, which is a conviction waiting for a
        // challenger to run the same job and get a different root.
        //
        // **The INTERVAL is still the filer's, and that is the residual.** A producer that names
        // an interval past its own decode count files zero checkpoints and opts out of the leg.
        // Pinning it here would be wrong rather than merely expensive: the right interval is a
        // property of the CLASS — a long-context class checkpointing every call pays for evidence
        // nobody needs — so it belongs in the catalog entry beside the geometry, which moves the
        // catalog root and the class id. Recorded rather than half-done.
        if binding.shape_profile.lane == crate::palw_step::PalwStepLaneV1::Int32
            && binding.checkpoint_profile.state_layout_id != crate::palw_state_chunk_map::integer_kv_state_layout_id_v1()
        {
            return Some(PalwStepFaultV1::CheckpointProfileNotCanonical);
        }
        // **How many checkpoints this job canonically has, at the cadence its map runs**
        // (ADR-0082 Decision 4). `decode_calls / interval` for every shipped class; `prefill +
        // decode_calls` for a class whose map addresses history tiles, which is every position the
        // cache ever holds. A leg short of that is a producer that opted out of the positions it
        // did not commit, which is the whole reason this count is recomputed rather than trusted.
        let canonical_ckpts = crate::palw_context_ladder::palw_checkpoint_count_v1(
            &binding.shape_profile,
            &binding.job_context,
            binding.checkpoint_profile.checkpoint_interval,
        );
        if binding.checkpoint_count != canonical_ckpts {
            return Some(PalwStepFaultV1::CheckpointCountNotCanonical);
        }
        let empty = checkpoint_empty_root_v2(&context_hash);
        if (binding.checkpoint_count == 0) != (binding.checkpoint_merkle_root == empty) {
            return Some(PalwStepFaultV1::CheckpointCountNotCanonical);
        }
        None
    })();
    if let Some(fault) = shape_fault {
        return Ok(PalwStepRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 0, 0, fault) });
    }
    if matches!(refutation.evidence, PalwStepEvidenceV1::Shape) {
        return Err(PalwStepLegError::NoFaultFound);
    }

    match &refutation.evidence {
        PalwStepEvidenceV1::Shape => unreachable!("handled above"),
        PalwStepEvidenceV1::StepTile { opening, preimage } => {
            open_against(binding.step_leaf_count, &binding.step_merkle_root, opening)?;
            if step_tile_leaf_hash_v1(&context_hash, &profile_hash, preimage) != opening.leaf_hash {
                return Err(PalwStepLegError::LeafPreimageMismatch { leaf: "step tile" });
            }
            let fault = step_tile_fault(binding, opening, preimage);
            match fault {
                Some(fault) => {
                    Ok(PalwStepRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 1, opening.leaf_index, fault) })
                }
                None => Err(PalwStepLegError::NoFaultFound),
            }
        }
        PalwStepEvidenceV1::KvChunk { opening, preimage } => {
            open_against(binding.step_leaf_count, &binding.step_merkle_root, opening)?;
            if kv_chunk_leaf_hash_v1(&context_hash, &profile_hash, preimage) != opening.leaf_hash {
                return Err(PalwStepLegError::LeafPreimageMismatch { leaf: "kv chunk" });
            }
            let fault = kv_chunk_fault(binding, opening, preimage);
            match fault {
                Some(fault) => {
                    Ok(PalwStepRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 2, opening.leaf_index, fault) })
                }
                None => Err(PalwStepLegError::NoFaultFound),
            }
        }
        PalwStepEvidenceV1::Checkpoint { opening, preimage } => {
            open_checkpoint(binding, &context_hash, &checkpoint_profile_hash, opening, preimage)?;
            let fault = checkpoint_fault(binding, &context_hash, opening, preimage);
            match fault {
                Some(fault) => {
                    Ok(PalwStepRefutationVerdictV1 { fault, evidence_id: evidence_id(committed, 3, opening.leaf_index, fault) })
                }
                None => Err(PalwStepLegError::NoFaultFound),
            }
        }
        PalwStepEvidenceV1::CheckpointChain { earlier_opening, earlier_preimage, later_opening, later_preimage } => {
            open_checkpoint(binding, &context_hash, &checkpoint_profile_hash, earlier_opening, earlier_preimage)?;
            open_checkpoint(binding, &context_hash, &checkpoint_profile_hash, later_opening, later_preimage)?;
            if later_opening.leaf_index != earlier_opening.leaf_index + 1 {
                return Err(PalwStepLegError::Step(PalwStepError::CoordinatesNotCanonical));
            }
            if later_preimage.prev_checkpoint_leaf_hash != earlier_opening.leaf_hash {
                let fault = PalwStepFaultV1::CheckpointChainBroken;
                return Ok(PalwStepRefutationVerdictV1 {
                    fault,
                    evidence_id: evidence_id(committed, 4, later_opening.leaf_index, fault),
                });
            }
            Err(PalwStepLegError::NoFaultFound)
        }
    }
}

fn open_against(leaf_count: u64, committed_root: &Hash64, opening: &PalwStepOpeningV1) -> Result<(), PalwStepLegError> {
    let implied = step_opening_root_v1(leaf_count, opening)?;
    if implied != *committed_root {
        return Err(PalwStepLegError::CommittedRootMismatch);
    }
    Ok(())
}

fn open_checkpoint(
    binding: &PalwStepBindingV2,
    context_hash: &Hash64,
    checkpoint_profile_hash: &Hash64,
    opening: &PalwStepOpeningV1,
    preimage: &PalwCheckpointLeafV2,
) -> Result<(), PalwStepLegError> {
    let implied = step_opening_root_v1(binding.checkpoint_count as u64, opening)?;
    if implied != binding.checkpoint_merkle_root {
        return Err(PalwStepLegError::CommittedRootMismatch);
    }
    if checkpoint_leaf_hash_v2(context_hash, checkpoint_profile_hash, &binding.state_chunk_map_id, preimage) != opening.leaf_hash {
        return Err(PalwStepLegError::LeafPreimageMismatch { leaf: "checkpoint" });
    }
    Ok(())
}

fn step_tile_fault(
    binding: &PalwStepBindingV2,
    opening: &PalwStepOpeningV1,
    preimage: &PalwStepTileLeafV1,
) -> Option<PalwStepFaultV1> {
    if preimage.version != PALW_STEP_LEG_OBJECT_VERSION_V1 {
        return Some(PalwStepFaultV1::StepCoordinatesNotCanonical);
    }
    let Some(canonical_index) = canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &preimage.coord) else {
        return Some(PalwStepFaultV1::StepCoordinatesNotCanonical);
    };
    if canonical_index != opening.leaf_index {
        return Some(PalwStepFaultV1::StepLeafIndexNotCanonical);
    }
    if preimage.values_le.len() != 4 * preimage.value_count as usize {
        return Some(PalwStepFaultV1::StepBytesNotFourPerValue);
    }
    // The canonical tile length at these (already canonical) coordinates.
    let expected = expected_tile_values(&binding.shape_profile, &binding.job_context, &preimage.coord);
    if Some(preimage.value_count) != expected {
        return Some(PalwStepFaultV1::StepValueCountNotCanonical);
    }
    // Float lanes only, for the reason `PalwStepLaneV1` gives: on an integer class this rule
    // convicts every negative activation of being "non-finite", which would have made BASE-0 — the
    // RC's own liveness floor — a class where every honest step is a provable fault.
    if binding.shape_profile.lane == crate::palw_step::PalwStepLaneV1::Float32 {
        for (i, quad) in preimage.values_le.chunks_exact(4).enumerate() {
            let bits = u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]);
            if !f32_is_finite_bits(bits) {
                return Some(PalwStepFaultV1::StepNonFinite { value_index: i as u32 });
            }
        }
    }
    None
}

fn expected_tile_values(profile: &PalwShapeProfileV3, context: &PalwJobContextV2, coord: &PalwStepCoordinateV1) -> Option<u32> {
    let (node, _layer) = profile.resolve_node_slot(coord.node_slot)?;
    let kv_len = if coord.call_index == 0 {
        coord.position as u64 + 1
    } else {
        context.declared_prefill_tokens as u64 + coord.call_index as u64
    };
    let len = match node.out_len {
        crate::palw_step::PalwStepOutLenV1::Fixed { elements } => elements as u64,
        crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => multiplier as u64 * kv_len,
    };
    let tiles = len.div_ceil(node.tile_len as u64);
    if (coord.tile_index as u64) >= tiles {
        return None;
    }
    let start = coord.tile_index as u64 * node.tile_len as u64;
    Some((len - start).min(node.tile_len as u64) as u32)
}

fn kv_chunk_fault(binding: &PalwStepBindingV2, opening: &PalwStepOpeningV1, preimage: &PalwKvChunkLeafV1) -> Option<PalwStepFaultV1> {
    if preimage.version != PALW_STEP_LEG_OBJECT_VERSION_V1 {
        return Some(PalwStepFaultV1::KvChunkNotCanonical);
    }
    // Reuse the builder's rank derivation via a throwaway view: canonical rank must equal
    // (leaf_index − main leaves).
    // Same cap, same reason as the shape pass above: the binding's own claim, never the executor's
    // constant. This runs only AFTER `shape_fault` has established `count == step_leaf_count`, so
    // the cap is exact here and the `Err` arm is unreachable through `check_step_refutation_v1`;
    // it stays fail-closed for any other caller.
    let main = match crate::palw_step::step_leaf_count_capped_v1(&binding.shape_profile, &binding.job_context, binding.step_leaf_count)
    {
        Ok(total) => total - kv_aux_leaf_count(&binding.shape_profile, &binding.job_context),
        Err(_) => return Some(PalwStepFaultV1::StepLeafCountNotCanonical),
    };
    if opening.leaf_index < main {
        return Some(PalwStepFaultV1::KvChunkNotCanonical);
    }
    let rank = kv_chunk_rank_standalone(&binding.shape_profile, &binding.job_context, preimage);
    match rank {
        Some(r) if main + r == opening.leaf_index => {}
        _ => return Some(PalwStepFaultV1::KvChunkNotCanonical),
    }
    if preimage.values_f16_le.len() != 2 * preimage.position_count as usize * binding.shape_profile.attn_head_dim as usize {
        return Some(PalwStepFaultV1::KvChunkBytesNotCanonical);
    }
    for (i, pair) in preimage.values_f16_le.chunks_exact(2).enumerate() {
        let bits = u16::from_le_bytes([pair[0], pair[1]]);
        if !f16_is_finite_bits(bits) {
            return Some(PalwStepFaultV1::KvChunkNonFinite { value_index: i as u32 });
        }
    }
    None
}

fn kv_chunk_rank_standalone(profile: &PalwShapeProfileV3, context: &PalwJobContextV2, leaf: &PalwKvChunkLeafV1) -> Option<u64> {
    if profile.kv_chunk_calls == 0 || leaf.is_v > 1 {
        return None;
    }
    let attn_layers: Vec<u16> = (0..profile.layer_count).filter(|&l| profile.layer_kind(l) == PalwLayerKindV1::Attention).collect();
    let layer_ordinal = attn_layers.iter().position(|&l| l == leaf.attn_layer)? as u64;
    if leaf.kv_head >= profile.attn_kv_heads {
        return None;
    }
    let positions = context.declared_prefill_tokens as u64 + context.exact_decode_tokens.saturating_sub(1) as u64;
    let chunks = positions.div_ceil(profile.kv_chunk_calls as u64);
    if (leaf.chunk_index as u64) >= chunks {
        return None;
    }
    let start = leaf.chunk_index as u64 * profile.kv_chunk_calls as u64;
    let canonical_count = (positions - start).min(profile.kv_chunk_calls as u64) as u32;
    if leaf.position_count != canonical_count {
        return None;
    }
    let per_layer = profile.attn_kv_heads as u64 * 2 * chunks;
    Some(layer_ordinal * per_layer + (leaf.kv_head as u64 * 2 + leaf.is_v as u64) * chunks + leaf.chunk_index as u64)
}

fn checkpoint_fault(
    binding: &PalwStepBindingV2,
    context_hash: &Hash64,
    opening: &PalwStepOpeningV1,
    preimage: &PalwCheckpointLeafV2,
) -> Option<PalwStepFaultV1> {
    if preimage.version != PALW_STEP_LEG_OBJECT_VERSION_V1 {
        return Some(PalwStepFaultV1::CheckpointIndexNotCanonical);
    }
    if preimage.checkpoint_index as u64 != opening.leaf_index {
        return Some(PalwStepFaultV1::CheckpointIndexNotCanonical);
    }
    // **The counter's canonical value at this index, at the cadence the CLASS's map runs**
    // (ADR-0082 Decision 4). On every shipped class this is `(index + 1) × interval` verbatim.
    let interval = binding.checkpoint_profile.checkpoint_interval;
    let canonical =
        crate::palw_context_ladder::palw_checkpoint_covered_at_index_v1(&binding.shape_profile, preimage.checkpoint_index, interval);
    if canonical != Some(preimage.covered_decode_call) {
        return Some(PalwStepFaultV1::CheckpointCoveredCallNotCanonical);
    }
    if preimage.checkpoint_index == 0 && preimage.prev_checkpoint_leaf_hash != checkpoint_genesis_prev_v2(context_hash) {
        return Some(PalwStepFaultV1::CheckpointGenesisPrevMismatch);
    }
    if preimage.state_chunk_count == 0 || preimage.state_chunk_count as usize > PALW_STEP_LEG_MAX_STATE_CHUNKS {
        return Some(PalwStepFaultV1::CheckpointIndexNotCanonical);
    }
    // **And the count must be the CLASS's map's for this state** (audit B, M-4). The range check
    // above admits any count in `[1, 2^16]`, so a leg whose leaves declare one chunk per
    // checkpoint — the whole cache as a blob — passed the shape pass and then made every anchor
    // built on it `Unadjudicable`: a leg that advertises a route it cannot serve, which on a
    // per-position class leaves the attention site with no route at all, because the cache-write
    // route is refused there too.
    //
    // **The fault is `CheckpointIndexNotCanonical` and not a new discriminant**: `PalwStepFaultV1`
    // is borsh-serialised with `use_discriminant = true` and its discriminants are wire-frozen, so
    // a new arm is a consensus object change; the existing arm already carries the other
    // `state_chunk_count` refusal two lines above, which is the same rule.
    //
    // `None` from the map is "this crate cannot enumerate that map" (the standalone recurrence
    // maps, whose geometry lives in the executor crate) and is not a fault: refusing a leg for the
    // court's own missing arithmetic would convict an honest class.
    let positions = crate::palw_context_ladder::palw_checkpoint_positions_at_v1(
        &binding.shape_profile,
        &binding.job_context,
        preimage.covered_decode_call,
    );
    if let Some(canonical_chunks) = crate::palw_state_chunk_map::palw_state_chunk_count_at_v1(&binding.shape_profile, positions)
        && preimage.state_chunk_count as u64 != canonical_chunks
    {
        return Some(PalwStepFaultV1::CheckpointIndexNotCanonical);
    }
    None
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS;
    use crate::palw_legs::{PALW_LEGS_ALL_DOMAINS, PalwLegOpeningV1, leg_opening_root_v1, leg_opening_v1};
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_step::{
        PALW_STEP_ALL_DOMAINS, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, canonical_step_coordinates,
    };
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_ALL_DOMAINS};

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    /// **The two exported node rules ARE the tree's own**, checked against a real root rather
    /// than against a second copy of the same expression.
    ///
    /// A two-leaf tree is exactly `node(leaf(0), leaf(1))`, so this pins both exports at once: if
    /// either drifted from what `step_merkle_root_v1` folds, the equality fails here rather than
    /// in an executor whose captured root nobody can open.
    #[test]
    fn the_exported_merkle_rules_are_the_step_trees_own() {
        let (a, b) = (h64(0x21), h64(0x22));
        assert_eq!(step_merkle_leaf_v1(0, &a), step_merkle_leaf(0, &a));
        assert_ne!(step_merkle_leaf_v1(0, &a), step_merkle_leaf_v1(1, &a), "the leaf is index-bound");
        assert_eq!(
            step_merkle_root_v1(&[a, b]).expect("a two-leaf root"),
            step_merkle_node_v1(&step_merkle_leaf_v1(0, &a), &step_merkle_leaf_v1(1, &b)),
            "the exported rules do not fold the tree the leg folds"
        );
        assert_ne!(step_merkle_node_v1(&a, &b), step_merkle_node_v1(&b, &a), "the node is ordered");
    }

    fn node(kind: PalwStepOpKindV1, out: PalwStepOutLenV1, tile: u32) -> PalwStepNodeV1 {
        PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: out,
            tile_len: tile,
            kernel_semantics_id: h64(0x11),
            input_refs: vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
        }
    }

    fn profile() -> PalwShapeProfileV3 {
        PalwShapeProfileV3 {
            version: crate::palw_step::PALW_STEP_OBJECT_VERSION_V1,
            lane: crate::palw_step::PalwStepLaneV1::Float32,
            layer_count: 2,
            full_attention_interval: 2,
            hidden_dim: 8,
            ffn_dim: 16,
            attn_heads: 2,
            attn_kv_heads: 1,
            attn_head_dim: 4,
            rope_dims: 2,
            rope_sections: [1, 1, 0, 0],
            rope_freq_base_bits: 0x4CBE_BC20,
            rms_eps_bits: 0x358637BD,
            base0_rms_eps_q: 1 << 8,
            logits_scheme_id: crate::palw_step_refute::flat_logits_scheme_id_v1(),
            l2_eps_bits: 0x358637BD,
            gdn_heads: 2,
            gdn_head_k_dim: 4,
            gdn_head_v_dim: 4,
            gdn_conv_kernel: 4,
            vocab_size: 40,
            repack_on: 1,
            llamafile_on: 1,
            flash_attn_disabled: 1,
            fused_gdn_on: 1,
            use_ref_off: 1,
            kv_cache_f16: 1,
            n_ctx: 64,
            n_batch: 64,
            n_ubatch: 64,
            n_seq: 1,
            n_threads: 4,
            pre_nodes: vec![node(PalwStepOpKindV1::EmbedLookup, PalwStepOutLenV1::Fixed { elements: 8 }, 16)],
            gdn_nodes: vec![
                node(PalwStepOpKindV1::RmsNorm, PalwStepOutLenV1::Fixed { elements: 8 }, 16),
                node(PalwStepOpKindV1::GatedDeltaNet, PalwStepOutLenV1::Fixed { elements: 24 }, 16),
            ],
            attn_nodes: vec![
                node(PalwStepOpKindV1::MatMulF16, PalwStepOutLenV1::KvScaled { multiplier: 2 }, 16),
                node(PalwStepOpKindV1::SoftMax, PalwStepOutLenV1::KvScaled { multiplier: 2 }, 16),
            ],
            post_nodes: vec![node(PalwStepOpKindV1::MatMulQuant, PalwStepOutLenV1::Fixed { elements: 40 }, 16)],
            reference_ruleset_id: h64(0x22),
            transcendental_bindings: vec![],
            contraction_facts: vec![],
            kv_chunk_calls: 3,
            state_chunk_map_id: h64(0x44),
        }
    }

    fn context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"step-leg-test".to_vec(),
            job_id: h64(1),
            job_nullifier: h64(2),
            assignment_id: h64(3),
            execution_seed: [7; 32],
            model_profile_id: h64(4),
            runtime_manifest_hash: h64(5),
            runtime_class_id: h64(6),
            // Placeholder; the fixtures below overwrite it with the profile they actually carry,
            // because honest material declares the profile it was produced under.
            shape_profile_id: h64(7),
            trace_scheme_id: h64(8),
            cu_ruleset_id: h64(9),
            tokenizer_id: h64(10),
            prompt_token_ids_hash: h64(11),
            declared_prefill_tokens: 2,
            exact_decode_tokens: 3,
            max_context_tokens: 64,
        }
        .with_real_scheme_id()
    }

    trait WithScheme {
        fn with_real_scheme_id(self) -> Self;
    }
    impl WithScheme for PalwJobContextV2 {
        fn with_real_scheme_id(mut self) -> Self {
            self.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
            self
        }
    }

    /// Builds the honest full commitment for the tiny job, returning everything a test needs.
    fn honest() -> (PalwStepBindingV2, PalwStepLegMaterialV1, Vec<Hash64>, Vec<PalwCheckpointLeafV2>) {
        let p = profile();
        // Honest material declares the profile it was produced under — the equality the verifier
        // now enforces. A fixture that leaves them inconsistent is not honest material.
        let mut ctx = context();
        ctx.shape_profile_id = p.shape_profile_id();
        let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
        let main = b.expected_main_leaves();
        for i in 0..main {
            let coord = canonical_step_coordinates(&p, &ctx, i).unwrap();
            let n = expected_tile_values(&p, &ctx, &coord).unwrap();
            let vals: Vec<u32> = (0..n).map(|k| 0x3F80_0000 + k).collect(); // 1.0 + k ulps
            b.push_step_tile(coord, &vals).unwrap();
        }
        // Aux chunks in canonical order: 1 attn layer (layer 1), 1 kv head, K then V,
        // ceil(4 positions / 3) = 2 chunks each.
        let positions = 4u32;
        let chunk_calls = p.kv_chunk_calls;
        for is_v in 0..2u8 {
            for chunk_index in 0..positions.div_ceil(chunk_calls) {
                let start = chunk_index * chunk_calls;
                let count = (positions - start).min(chunk_calls);
                let vals = [0x3C, 0x00].repeat(count as usize * p.attn_head_dim as usize); // f16 1.0
                b.push_kv_chunk(&PalwKvChunkLeafV1 {
                    version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                    attn_layer: 1,
                    kv_head: 0,
                    is_v,
                    chunk_index,
                    position_count: count,
                    values_f16_le: vals,
                })
                .unwrap();
            }
        }
        let material = b.finish().unwrap();

        // Checkpoints: interval 1 over decode_calls 2 → 2 checkpoints, chained from genesis.
        let ctx_hash = ctx.context_hash();
        let ckpt_profile = PalwCheckpointProfileV1 {
            version: crate::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: 1,
            state_layout_id: crate::palw_state_chunk_map::integer_kv_state_layout_id_v1(),
        };
        let map_id = h64(0x44);
        let chunk0 = state_chunk_leaf_hash_v1(&map_id, 0, b"state-a");
        let chunk1 = state_chunk_leaf_hash_v1(&map_id, 1, b"state-b");
        let ckpt_profile_hash = ckpt_profile.profile_hash();
        let mut leaves = Vec::new();
        let mut hashes = Vec::new();
        let mut prev = checkpoint_genesis_prev_v2(&ctx_hash);
        for i in 0..2u32 {
            let leaf = PalwCheckpointLeafV2 {
                version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                checkpoint_index: i,
                covered_decode_call: i + 1,
                prev_checkpoint_leaf_hash: prev,
                state_chunk_count: 2,
                state_chunks_root: state_chunks_root_v1(&[chunk0, chunk1]).unwrap(),
            };
            let h = checkpoint_leaf_hash_v2(&ctx_hash, &ckpt_profile_hash, &map_id, &leaf);
            prev = h;
            hashes.push(h);
            leaves.push(leaf);
        }
        let ckpt_merkle = step_merkle_root_v1(&hashes).unwrap();
        let profile_hash = p.shape_profile_id();
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &material.merkle_root);
        let ckpt_root = checkpoint_leg_root_v2(&ctx_hash, &ckpt_profile_hash, &map_id, 2, 2, &ckpt_merkle);
        let committed = execution_commitment_root_v2(&ctx_hash, &h64(0xAA), &h64(0xBB), &ckpt_root, &step_root);
        let binding = PalwStepBindingV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            job_context: ctx,
            shape_profile: p,
            checkpoint_profile: ckpt_profile,
            state_chunk_map_id: map_id,
            full_logits_trace_root: h64(0xAA),
            activation_leg_root: h64(0xBB),
            step_leaf_count: material.leaf_count,
            step_merkle_root: material.merkle_root,
            checkpoint_count: 2,
            checkpoint_merkle_root: ckpt_merkle,
            committed_execution_root: committed,
        };
        (binding, material, hashes, leaves)
    }

    #[test]
    fn step_leg_domains_are_unique_across_all_palw_modules() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_STEP_LEG_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate step-leg domain");
            assert!(d.len() <= 64, "blake2b key cap exceeded");
        }
        for d in PALW_V2_ALL_DOMAINS
            .iter()
            .chain(PALW_S_ALL_DOMAINS.iter())
            .chain(PALW_LEGS_ALL_DOMAINS.iter())
            .chain(PALW_REFERENCE_ALL_DOMAINS.iter())
            .chain(PALW_SCHEDULE_ALL_DOMAINS.iter())
            .chain(PALW_CARRIAGE_ALL_DOMAINS.iter())
            .chain(PALW_STEP_ALL_DOMAINS.iter())
        {
            assert!(!seen.contains(d), "step-leg module reuses a foreign domain: {}", String::from_utf8_lossy(d));
        }
    }

    /// **§3.3: a step is adjudicated under the profile its job DECLARED.**
    ///
    /// The context has always named a `shape_profile_id`; nothing compared it to the profile the
    /// binding carried. So a binding could hand the court a different profile — different geometry,
    /// different epsilons, different kernel ids — and the producer was judged against arithmetic its
    /// job never claimed. The mismatch is refused, and it is refused by NAME so an operator can see
    /// which two ids disagreed.
    #[test]
    fn a_binding_must_carry_the_profile_its_context_declares() {
        let (binding, _material, _, _) = honest();
        // Honest material passes: the fixture now declares what it carries.
        assert!(verify_binding(&binding).is_ok());

        // Swap in a profile the context does not name. Everything else is untouched.
        let mut swapped = binding.clone();
        let mut other = profile();
        other.base0_rms_eps_q = binding.shape_profile.base0_rms_eps_q + 1;
        let declared = swapped.job_context.shape_profile_id;
        let got = other.shape_profile_id();
        assert_ne!(declared, got, "the fixture must actually change the profile");
        swapped.shape_profile = other;
        assert_eq!(
            verify_binding(&swapped),
            Err(PalwStepLegError::ShapeProfileNotTheDeclaredOne { declared, got }),
            "a profile the job never declared must be refused, by name"
        );
    }

    /// **The same defect one field over: the carried state chunk map.**
    ///
    /// Every checkpoint hash is computed FROM `binding.state_chunk_map_id`, so a filer carrying an
    /// id its class never registered builds a checkpoint leg that verifies against itself and
    /// describes state under a layout of its own choosing. Two sources for one fact, and the
    /// second one is the accuser's.
    #[test]
    fn a_binding_must_carry_the_state_chunk_map_its_profile_registers() {
        let (binding, _material, _, _) = honest();
        assert!(verify_binding(&binding).is_ok());

        // The profile keeps its registered map; only the free copy beside it moves. Nothing else
        // in the binding is touched, which is exactly the shape of the attack.
        let mut forged = binding.clone();
        forged.state_chunk_map_id = h64(0xAD);
        assert_ne!(forged.state_chunk_map_id, forged.shape_profile.state_chunk_map_id);
        assert_eq!(
            verify_binding(&forged),
            Err(PalwStepLegError::StateChunkMapNotTheRegisteredOne {
                registered: binding.shape_profile.state_chunk_map_id,
                carried: h64(0xAD),
            }),
            "a state chunk map the class never registered must be refused, by name"
        );
    }

    #[test]
    fn scheme_id_and_composite_root_golden_vectors() {
        // Frozen 2026-08-16 — layout changes are a new scheme version, never an edit.
        assert_eq!(
            execution_commitment_scheme_id_v2().to_string(),
            "832da58ebfd0926ae7b0564d2dd457ed6a15f4f653506d20884925544e1f48d3\
             bce85627058b36727092bcafae9f1cc8d0b13378ca007f4a526352b8102e9eec"
        );
        let (binding, ..) = honest();
        assert_eq!(
            binding.committed_execution_root.to_string(),
            // Re-frozen 2026-08-17, twice: the profile gained `base0_rms_eps_q` (see `palw_step`'s
            // golden), and the fixture's job context now DECLARES the profile it carries, which the
            // verifier began enforcing. The second move is the interesting one — the fixture had
            // been incoherent (a context naming `h64(7)` while carrying a real profile) for as long
            // as nothing compared them.
            //
            // Re-frozen 2026-08-20: it descends from the shape profile id, which moved when
            // `weight_dtype` became a per-layer list. See `palw_step`'s golden for why the single
            // byte could not describe the pinned model.
            // …and again for `lane` (see `palw_step`'s golden).
            //
            // Re-frozen 2026-08-26: the profile gained `logits_scheme_id` — the class's logits
            // commitment became part of its identity, so every id downstream of the profile moved
            // with it. Deliberate and network-wide (the RC re-mint the ADR-0053 state bump already
            // forces); a class that could change its commitment scheme without changing identity
            // was the fail-open the field exists to close.
            "5466a0a2a7a5342232aeb1e96d4ba9aaae20bdc7b13c84348f470f8b980a7148\
             6e65dcf218166ad0d166bae68f32948ecd51236cc46cdc26a39348ee7ba91bda" // Re-derived once more at the MERGE with `fix/audit-batch-1` (2026-08-27), which moved
                                                                               // it from its own side: the checkpoint profile joined the binding and the fault table
                                                                               // gained a variant. One value over both lines, not two values summed.
        );
    }

    /// The step tree must be byte-equivalent to the frozen v1 generalized tree run at the step
    /// domains, for every shape both implementations accept, and openings must cross-verify.
    #[test]
    fn step_tree_is_the_v1_tree_at_step_domains() {
        let leaves: Vec<Hash64> = (0u8..17).map(h64).collect();
        for width in 1..=17usize {
            let slice = &leaves[..width];
            let ours = step_merkle_root_v1(slice).unwrap();
            for index in 0..width as u64 {
                let opening = step_opening_v1(slice, index).unwrap();
                assert_eq!(step_opening_root_v1(width as u64, &opening).unwrap(), ours, "w={width} i={index}");
                // Tampered leaf hash must not verify.
                let mut bad = opening.clone();
                bad.leaf_hash = h64(0xEE);
                assert_ne!(step_opening_root_v1(width as u64, &bad).unwrap(), ours);
                // The v1 verifier with the same domains must agree wherever index layouts
                // coincide (u32 vs u64 leaf-index prefix differ — asserted different below).
                let v1_opening =
                    leg_opening_v1(PALW_STEP_LEG_DOMAIN_MERKLE_LEAF, PALW_STEP_LEG_DOMAIN_MERKLE_NODE, slice, index as u32, 1 << 17)
                        .unwrap();
                let v1_root = leg_opening_root_v1(
                    PALW_STEP_LEG_DOMAIN_MERKLE_LEAF,
                    PALW_STEP_LEG_DOMAIN_MERKLE_NODE,
                    width as u32,
                    &PalwLegOpeningV1 { leaf_index: index as u32, leaf_hash: v1_opening.leaf_hash, siblings: v1_opening.siblings },
                    1 << 17,
                )
                .unwrap();
                // Same discipline, DIFFERENT index width in the leaf preimage (u64 here, u32
                // in v1) — the trees must NOT collide, or the domains would be a bridge.
                assert_ne!(v1_root, ours, "u32/u64 index widths must separate the trees");
            }
        }
    }

    #[test]
    fn builder_is_fail_closed() {
        let p = profile();
        let ctx = context();
        // Out-of-order tile.
        let mut b = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
        let c1 = canonical_step_coordinates(&p, &ctx, 1).unwrap();
        assert!(matches!(
            b.push_step_tile(c1, &vec![0x3F80_0000; expected_tile_values(&p, &ctx, &c1).unwrap() as usize]),
            Err(PalwStepLegError::TilesOutOfOrder { .. })
        ));
        // Wrong length.
        let c0 = canonical_step_coordinates(&p, &ctx, 0).unwrap();
        assert!(matches!(b.push_step_tile(c0, &[0x3F80_0000]), Err(PalwStepLegError::TileLengthNotCanonical { .. })));
        // Non-finite value.
        let n0 = expected_tile_values(&p, &ctx, &c0).unwrap() as usize;
        let mut vals = vec![0x3F80_0000u32; n0];
        vals[n0 - 1] = 0x7F80_0000; // +Inf
        assert!(matches!(b.push_step_tile(c0, &vals), Err(PalwStepLegError::NonFiniteStepValue { .. })));
        // Incomplete finish.
        let b2 = PalwStepLegBuilderV1::new(ctx.clone(), p.clone()).unwrap();
        assert!(matches!(b2.finish(), Err(PalwStepLegError::LegIncomplete { .. })));
        // Aux before main is refused.
        let mut b3 = PalwStepLegBuilderV1::new(ctx, p).unwrap();
        let chunk = PalwKvChunkLeafV1 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            attn_layer: 1,
            kv_head: 0,
            is_v: 0,
            chunk_index: 0,
            position_count: 3,
            values_f16_le: [0x00, 0x3C].repeat(12),
        };
        assert!(matches!(b3.push_kv_chunk(&chunk), Err(PalwStepLegError::KvChunksOutOfOrder)));
    }

    #[test]
    fn honest_material_yields_no_fault_on_every_arm() {
        let (binding, material, ckpt_hashes, ckpt_leaves) = honest();
        // Shape arm.
        let r = PalwStepRefutationV1 { binding: binding.clone(), evidence: PalwStepEvidenceV1::Shape };
        assert_eq!(check_step_refutation_v1(&r), Err(PalwStepLegError::NoFaultFound));
        // Every step-tile arm.
        let main = material.leaf_count - kv_aux_leaf_count(&binding.shape_profile, &binding.job_context);
        for i in [0, 1, main - 1] {
            let opening = step_opening_v1(&material.leaf_hashes, i).unwrap();
            let coord = canonical_step_coordinates(&binding.shape_profile, &binding.job_context, i).unwrap();
            let n = expected_tile_values(&binding.shape_profile, &binding.job_context, &coord).unwrap();
            let vals: Vec<u8> = (0..n).flat_map(|k| (0x3F80_0000u32 + k).to_le_bytes()).collect();
            let preimage = PalwStepTileLeafV1 { version: PALW_STEP_LEG_OBJECT_VERSION_V1, coord, value_count: n, values_le: vals };
            let r = PalwStepRefutationV1 { binding: binding.clone(), evidence: PalwStepEvidenceV1::StepTile { opening, preimage } };
            assert_eq!(check_step_refutation_v1(&r), Err(PalwStepLegError::NoFaultFound), "tile {i}");
        }
        // A KV chunk arm.
        let aux_first = main;
        let opening = step_opening_v1(&material.leaf_hashes, aux_first).unwrap();
        let preimage = PalwKvChunkLeafV1 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            attn_layer: 1,
            kv_head: 0,
            is_v: 0,
            chunk_index: 0,
            position_count: 3,
            values_f16_le: [0x3C, 0x00].repeat(3 * binding.shape_profile.attn_head_dim as usize),
        };
        let r = PalwStepRefutationV1 { binding: binding.clone(), evidence: PalwStepEvidenceV1::KvChunk { opening, preimage } };
        assert_eq!(check_step_refutation_v1(&r), Err(PalwStepLegError::NoFaultFound));
        // Checkpoint + chain arms.
        let o0 = step_opening_v1(&ckpt_hashes, 0).unwrap();
        let o1 = step_opening_v1(&ckpt_hashes, 1).unwrap();
        let r = PalwStepRefutationV1 {
            binding: binding.clone(),
            evidence: PalwStepEvidenceV1::Checkpoint { opening: o0.clone(), preimage: ckpt_leaves[0].clone() },
        };
        assert_eq!(check_step_refutation_v1(&r), Err(PalwStepLegError::NoFaultFound));
        let r = PalwStepRefutationV1 {
            binding,
            evidence: PalwStepEvidenceV1::CheckpointChain {
                earlier_opening: o0,
                earlier_preimage: ckpt_leaves[0].clone(),
                later_opening: o1,
                later_preimage: ckpt_leaves[1].clone(),
            },
        };
        assert_eq!(check_step_refutation_v1(&r), Err(PalwStepLegError::NoFaultFound));
    }

    #[test]
    fn tampered_material_convicts_with_the_right_fault() {
        let (binding, material, _ckpt_hashes, _ckpt_leaves) = honest();
        // A non-finite value smuggled into a leaf the tree actually committed: rebuild a
        // dishonest tree around one bad leaf (the builder refuses, so hash by hand).
        let p = binding.shape_profile.clone();
        let ctx = binding.job_context.clone();
        let ctx_hash = ctx.context_hash();
        let profile_hash = p.shape_profile_id();
        let main = material.leaf_count - kv_aux_leaf_count(&p, &ctx);
        let mut hashes = material.leaf_hashes.clone();
        let bad_i = 1u64;
        let coord = canonical_step_coordinates(&p, &ctx, bad_i).unwrap();
        let n = expected_tile_values(&p, &ctx, &coord).unwrap();
        let mut vals: Vec<u8> = (0..n).flat_map(|k| (0x3F80_0000u32 + k).to_le_bytes()).collect();
        vals[0..4].copy_from_slice(&0x7FC0_0000u32.to_le_bytes()); // NaN in value 0
        let bad_leaf = PalwStepTileLeafV1 { version: PALW_STEP_LEG_OBJECT_VERSION_V1, coord, value_count: n, values_le: vals };
        hashes[bad_i as usize] = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &bad_leaf);
        let merkle = step_merkle_root_v1(&hashes).unwrap();
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, material.leaf_count, &merkle);
        let ckpt_profile_hash = binding.checkpoint_profile.profile_hash();
        let ckpt_root = checkpoint_leg_root_v2(
            &ctx_hash,
            &ckpt_profile_hash,
            &binding.state_chunk_map_id,
            2,
            binding.checkpoint_count,
            &binding.checkpoint_merkle_root,
        );
        let committed = execution_commitment_root_v2(
            &ctx_hash,
            &binding.full_logits_trace_root,
            &binding.activation_leg_root,
            &ckpt_root,
            &step_root,
        );
        let mut dishonest = binding.clone();
        dishonest.step_merkle_root = merkle;
        dishonest.committed_execution_root = committed;
        let opening = step_opening_v1(&hashes, bad_i).unwrap();
        let r = PalwStepRefutationV1 {
            binding: dishonest.clone(),
            evidence: PalwStepEvidenceV1::StepTile { opening, preimage: bad_leaf },
        };
        let verdict = check_step_refutation_v1(&r).unwrap();
        assert_eq!(verdict.fault, PalwStepFaultV1::StepNonFinite { value_index: 0 });
        let _ = main;

        // A wrong leaf-count binding convicts from the binding alone.
        let mut wrong_count = binding.clone();
        wrong_count.step_leaf_count += 1;
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, wrong_count.step_leaf_count, &wrong_count.step_merkle_root);
        wrong_count.committed_execution_root = execution_commitment_root_v2(
            &ctx_hash,
            &wrong_count.full_logits_trace_root,
            &wrong_count.activation_leg_root,
            &ckpt_root,
            &step_root,
        );
        let r = PalwStepRefutationV1 { binding: wrong_count, evidence: PalwStepEvidenceV1::Shape };
        assert_eq!(check_step_refutation_v1(&r).unwrap().fault, PalwStepFaultV1::StepLeafCountNotCanonical);

        // A binding that does not recompute the committed root is about some other commitment.
        let mut alien = binding.clone();
        alien.committed_execution_root = h64(0xDD);
        let r = PalwStepRefutationV1 { binding: alien, evidence: PalwStepEvidenceV1::Shape };
        assert_eq!(check_step_refutation_v1(&r), Err(PalwStepLegError::CommittedRootMismatch));
    }

    /// **An oversized committed job convicts from the binding alone.** The producer commits a
    /// job whose footprint exceeds the class's registered context, self-consistently — the
    /// binding recomputes its own root — and the court's answer is the fault BY NAME, before any
    /// enumeration of the oversized space runs. An honest binding at the class's own bound stays
    /// honest, so the rule separates rather than merely refuses.
    #[test]
    fn a_job_past_the_registered_context_convicts_by_name() {
        let (binding, _m, _h, _l) = honest();
        // The honest fixture is inside its bound and clears the shape sweep.
        let r = PalwStepRefutationV1 { binding: binding.clone(), evidence: PalwStepEvidenceV1::Shape };
        assert!(
            matches!(check_step_refutation_v1(&r), Err(PalwStepLegError::NoFaultFound)),
            "the honest job is inside the class's context"
        );

        // A producer commits — self-consistently — a job past the class's bound. The profile is
        // untouched (it is the CLASS's, authenticated by id); the JOB is the lie.
        let mut oversized = binding.clone();
        oversized.job_context.declared_prefill_tokens = oversized.shape_profile.n_ctx + 5;
        // The job's OWN budget field is producer-declared, so the liar declares a bigger one —
        // which is exactly the leak: nothing bound `max_context_tokens` to the class until now.
        oversized.job_context.max_context_tokens = oversized.shape_profile.n_ctx * 4;
        // Its committed root is recomputed over the lying context, exactly as a fraudulent
        // producer would commit it — the fault is inside the commitment, not beside it.
        let ctx_hash = oversized.job_context.context_hash();
        let profile_hash = oversized.shape_profile.shape_profile_id();
        let decode_calls = oversized.job_context.exact_decode_tokens.saturating_sub(1);
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, oversized.step_leaf_count, &oversized.step_merkle_root);
        let ckpt_root = checkpoint_leg_root_v2(
            &ctx_hash,
            &oversized.checkpoint_profile.profile_hash(),
            &oversized.state_chunk_map_id,
            decode_calls,
            oversized.checkpoint_count,
            &oversized.checkpoint_merkle_root,
        );
        oversized.committed_execution_root = execution_commitment_root_v2(
            &ctx_hash,
            &oversized.full_logits_trace_root,
            &oversized.activation_leg_root,
            &ckpt_root,
            &step_root,
        );
        let r = PalwStepRefutationV1 { binding: oversized, evidence: PalwStepEvidenceV1::Shape };
        assert_eq!(
            check_step_refutation_v1(&r).expect("the oversized commitment is the fault").fault,
            PalwStepFaultV1::JobExceedsClassContext,
            "convicted as WHAT IT IS, not as whatever its leaf count disagrees with"
        );
    }

    /// **The range form and the per-leaf form agree on every root** — swept over tree widths
    /// covering the promote-odd shapes and every (start, count) inside them, with the builder's
    /// sibling set, the verifier's walk and the counter's arithmetic checked against each other.
    /// One wrong promote rule in any of the three shows up here as a root mismatch or a count
    /// mismatch, not at a challenger's first real close.
    #[test]
    fn a_range_opening_reaches_the_same_root_as_the_leaves() {
        for width in [1usize, 2, 3, 5, 8, 11, 16, 21, 33] {
            let leaves: Vec<Hash64> = (0..width).map(|i| Hash64::from_u64_word(0x9000 + i as u64)).collect();
            let root = step_merkle_root_v1(&leaves).expect("well-formed");
            for start in 0..width {
                for count in 1..=(width - start) {
                    let siblings = step_merkle_range_siblings_v1(&leaves, start, count).expect("buildable");
                    assert_eq!(
                        siblings.len() as u64,
                        step_range_sibling_count_v1(width as u64, start as u64, count as u64),
                        "the counter must price exactly what the builder emits (width {width}, {start}+{count})"
                    );
                    let opening = PalwStepRangeOpeningV1 {
                        first_leaf_index: start as u64,
                        leaf_hashes: leaves[start..start + count].to_vec(),
                        siblings,
                    };
                    assert_eq!(
                        step_range_opening_root_v1(width as u64, &opening).expect("verifiable"),
                        root,
                        "range root (width {width}, {start}+{count})"
                    );
                    // And a bent leaf inside the range moves the root — the range authenticates
                    // its members, not just its span.
                    let mut bent = opening.clone();
                    bent.leaf_hashes[count - 1] = Hash64::from_u64_word(0xBEEF);
                    let bent_root = step_range_opening_root_v1(width as u64, &bent).expect("still walks");
                    assert_ne!(bent_root, root, "a bent member must not reach the root");
                }
            }
        }
    }

    #[test]
    fn broken_checkpoint_chain_convicts() {
        let (binding, _material, _hashes, mut leaves) = honest();
        // Rebuild the checkpoint tree with leaf 1's prev pointing somewhere else.
        let ctx_hash = binding.job_context.context_hash();
        let ckpt_profile_hash = binding.checkpoint_profile.profile_hash();
        leaves[1].prev_checkpoint_leaf_hash = h64(0xEF);
        let h0 = checkpoint_leaf_hash_v2(&ctx_hash, &ckpt_profile_hash, &binding.state_chunk_map_id, &leaves[0]);
        let h1 = checkpoint_leaf_hash_v2(&ctx_hash, &ckpt_profile_hash, &binding.state_chunk_map_id, &leaves[1]);
        let merkle = step_merkle_root_v1(&[h0, h1]).unwrap();
        let profile_hash = binding.shape_profile.shape_profile_id();
        let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, binding.step_leaf_count, &binding.step_merkle_root);
        let ckpt_root = checkpoint_leg_root_v2(&ctx_hash, &ckpt_profile_hash, &binding.state_chunk_map_id, 2, 2, &merkle);
        let committed = execution_commitment_root_v2(
            &ctx_hash,
            &binding.full_logits_trace_root,
            &binding.activation_leg_root,
            &ckpt_root,
            &step_root,
        );
        let mut dishonest = binding.clone();
        dishonest.checkpoint_merkle_root = merkle;
        dishonest.committed_execution_root = committed;
        let o0 = step_opening_v1(&[h0, h1], 0).unwrap();
        let o1 = step_opening_v1(&[h0, h1], 1).unwrap();
        let r = PalwStepRefutationV1 {
            binding: dishonest,
            evidence: PalwStepEvidenceV1::CheckpointChain {
                earlier_opening: o0,
                earlier_preimage: leaves[0].clone(),
                later_opening: o1,
                later_preimage: leaves[1].clone(),
            },
        };
        assert_eq!(check_step_refutation_v1(&r).unwrap().fault, PalwStepFaultV1::CheckpointChainBroken);
    }

    #[test]
    fn state_chunk_leaves_bind_the_map_identity() {
        let a = state_chunk_leaf_hash_v1(&h64(1), 0, b"bytes");
        let b = state_chunk_leaf_hash_v1(&h64(2), 0, b"bytes");
        assert_ne!(a, b, "the same bytes under two map claims must be two leaves");
        assert_ne!(state_chunks_root_v1(&[a]).unwrap(), state_chunks_root_v1(&[b]).unwrap());
    }

    // -----------------------------------------------------------------------------------------
    // The step leg's caps are the RULESET's, not this module's literals.
    // -----------------------------------------------------------------------------------------

    /// The leaf a virtual tree puts at `index` — cheap, and a pure function of the index so the
    /// tree is reproducible without being stored.
    fn virtual_leaf(index: u64) -> Hash64 {
        Hash64::from_u64_word(index ^ 0x5A5A_0000_0000_0001)
    }

    /// **A perfect `2^height` tree, walked instead of stored.**
    ///
    /// `step_merkle_root_v1` takes a slice, and a slice of `2^25` `Hash64` is 2 GiB — a test that
    /// allocates it is a test nobody runs, so the acceptance case would go unwritten, which is how
    /// the 22 survived. This recursion holds one node per LEVEL and calls the module's own
    /// exported [`step_merkle_leaf_v1`] / [`step_merkle_node_v1`]: it is a second shape, never a
    /// second hash rule, and [`the_virtual_tree_is_the_shipped_tree`] proves the shape against the
    /// shipped builder before anything below leans on it.
    ///
    /// `target` rides down only the side that contains it; each frame pushes the sibling of that
    /// side after both children return, so the deepest frame pushes first and `path` comes back
    /// bottom-up — the order [`step_opening_root_v1`] consumes.
    fn virtual_subtree(first: u64, height: u32, target: Option<u64>, path: &mut Vec<Hash64>) -> Hash64 {
        if height == 0 {
            return step_merkle_leaf_v1(first, &virtual_leaf(first));
        }
        let half = 1u64 << (height - 1);
        let mid = first + half;
        let left = virtual_subtree(first, height - 1, target.filter(|t| *t < mid), path);
        let right = virtual_subtree(mid, height - 1, target.filter(|t| *t >= mid), path);
        if let Some(t) = target {
            path.push(if t < mid { right } else { left });
        }
        step_merkle_node_v1(&left, &right)
    }

    fn virtual_root_and_path(height: u32, target: u64) -> (Hash64, Vec<Hash64>) {
        let mut path = Vec::with_capacity(height as usize);
        let root = virtual_subtree(0, height, Some(target), &mut path);
        (root, path)
    }

    /// The walked tree IS the folded tree, at every perfect height the folded one can afford.
    #[test]
    fn the_virtual_tree_is_the_shipped_tree() {
        for height in 0..=10u32 {
            let width = 1usize << height;
            let leaves: Vec<Hash64> = (0..width as u64).map(virtual_leaf).collect();
            let shipped_root = step_merkle_root_v1(&leaves).expect("a shipped root");
            for target in [0u64, 1, (width as u64) / 3, width as u64 - 1].into_iter().filter(|t| *t < width as u64) {
                let (root, path) = virtual_root_and_path(height, target);
                assert_eq!(root, shipped_root, "height {height}: the walk and the fold disagree");
                let shipped = step_opening_v1(&leaves, target).expect("a shipped opening");
                assert_eq!(path, shipped.siblings, "height {height} target {target}: sibling path");
                assert_eq!(path.len(), height as usize, "a perfect 2^{height} tree opens {height} siblings");
            }
        }
    }

    /// **The leg's opening depth IS the court's bisection depth.** Two derivations of
    /// `ceil(log2(max_step_leaf_count))` that could drift are one dispute the court schedules and
    /// the leg cannot close, so they are checked against each other rather than against a literal.
    #[test]
    fn the_legs_opening_depth_is_the_courts_bisection_depth() {
        // Not `u64::MAX`: `PalwCourtParamsV2::bisection_rounds` reaches it through
        // `next_power_of_two`, which overflows above `2^63` rather than answering 64. That is the
        // court's own edge and is left exactly as it is here; the leg's derivation does answer 64,
        // asserted below.
        for n in [2u64, 3, 4, 5, 7, 8, 9, 1023, 1024, 1025, 1 << 22, (1 << 22) + 1, 1 << 25, 1 << 32, 1 << 62] {
            let court = crate::palw_mode_v2::PalwCourtParamsV2::new(n, 4, 2).expect("a well-formed court");
            assert_eq!(
                step_leg_max_opening_siblings_v1(n),
                court.bisection_rounds() as usize,
                "the leg and the court must count the same levels for a {n}-leaf ladder"
            );
        }
        // Degenerate ladders have no level, and the top of the u64 range answers 64 rather than
        // overflowing a `next_power_of_two`.
        assert_eq!(step_leg_max_opening_siblings_v1(0), 0);
        assert_eq!(step_leg_max_opening_siblings_v1(1), 0);
        assert_eq!(step_leg_max_opening_siblings_v1(u64::MAX), 64);
        // And the derived depth is exactly the path a real tree of that many leaves produces.
        for width in 1..=40usize {
            let leaves: Vec<Hash64> = (0..width as u64).map(virtual_leaf).collect();
            let longest = (0..width as u64)
                .map(|i| step_opening_v1(&leaves, i).expect("an opening").siblings.len())
                .max()
                .expect("a non-empty tree");
            assert_eq!(
                longest,
                step_leg_max_opening_siblings_v1(width as u64),
                "width {width}: the derived cap must be the deepest real path"
            );
        }
    }

    /// **The shipped default is unchanged, at every site, byte for byte.**
    ///
    /// Nothing may move while the fence is dormant, so every `_capped_v1` entry point called with
    /// [`PALW_STEP_LEG_MAX_LEAVES`] must return exactly what the un-capped name returns — and the
    /// two constants must still read 2^22 and 22.
    #[test]
    fn the_dormant_default_is_byte_identical_at_every_site() {
        assert_eq!(PALW_STEP_LEG_MAX_LEAVES, 1 << 22, "the shipped ladder moved without a fence");
        assert_eq!(PALW_STEP_LEG_MAX_OPENING_SIBLINGS, 22, "the shipped opening depth moved without a fence");
        assert_eq!(PALW_STEP_LEG_MAX_OPENING_SIBLINGS, step_leg_max_opening_siblings_v1(PALW_STEP_LEG_MAX_LEAVES));

        const CAP: u64 = PALW_STEP_LEG_MAX_LEAVES;
        for width in 1..=33usize {
            let leaves: Vec<Hash64> = (0..width as u64).map(virtual_leaf).collect();
            assert_eq!(step_merkle_root_v1(&leaves), step_merkle_root_capped_v1(&leaves, CAP), "root, width {width}");
            let root = step_merkle_root_v1(&leaves).expect("rooted");
            for index in 0..width {
                assert_eq!(
                    step_merkle_path_v1(&leaves, index),
                    step_merkle_path_capped_v1(&leaves, index, CAP),
                    "path, width {width} index {index}"
                );
                let opening = step_opening_v1(&leaves, index as u64).expect("an opening");
                assert_eq!(Ok(opening.clone()), step_opening_capped_v1(&leaves, index as u64, CAP), "opening, {width}/{index}");
                assert_eq!(
                    step_opening_root_v1(width as u64, &opening),
                    step_opening_root_capped_v1(width as u64, &opening, CAP),
                    "verify, {width}/{index}"
                );
                assert_eq!(step_opening_root_v1(width as u64, &opening), Ok(root), "the default still verifies");
                for count in 1..=(width - index) {
                    let siblings = step_merkle_range_siblings_v1(&leaves, index, count).expect("a range");
                    assert_eq!(
                        Ok(siblings.clone()),
                        step_merkle_range_siblings_capped_v1(&leaves, index, count, CAP),
                        "range siblings, {width}/{index}+{count}"
                    );
                    let range = PalwStepRangeOpeningV1 {
                        first_leaf_index: index as u64,
                        leaf_hashes: leaves[index..index + count].to_vec(),
                        siblings,
                    };
                    assert_eq!(
                        step_range_opening_root_v1(width as u64, &range),
                        step_range_opening_root_capped_v1(width as u64, &range, CAP),
                        "range verify, {width}/{index}+{count}"
                    );
                    assert_eq!(step_range_opening_root_v1(width as u64, &range), Ok(root));
                }
            }
        }
        // A leaf count one past the default is refused by the default entry points and by the
        // capped ones called AT the default — the cap is a parameter, not a second rule.
        let over = CAP + 1;
        let opening = PalwStepOpeningV1 { leaf_index: 0, leaf_hash: virtual_leaf(0), siblings: vec![h64(0x11); 23] };
        assert_eq!(step_opening_root_v1(over, &opening), Err(PalwStepLegError::LeafCountOutOfRange { got: over, max: CAP }));
        assert_eq!(step_opening_root_v1(over, &opening), step_opening_root_capped_v1(over, &opening, CAP));
    }

    /// **The three sites a ladder past `2^22` is refused at today, each named.**
    ///
    /// A tree of `2^22 + 1` leaves is the smallest one the shipped literals refuse, and it is
    /// refused three separate times on the way through commit → open → verify. A partial fix
    /// cannot look green here because each refusal is asserted by NAME and by the site that
    /// produced it, and the fourth assertion — the opening-depth cap, the literal 22 itself — is
    /// reached with a leaf count the default admits, so it is not masked by the count guard.
    #[test]
    fn a_ladder_past_the_default_is_refused_at_three_named_sites_and_the_ruleset_cap_clears_them() {
        const LADDER: u64 = PALW_STEP_LEG_MAX_LEAVES + 1;
        let leaves: Vec<Hash64> = (0..LADDER).map(virtual_leaf).collect();

        // SITE 1 — the root builder (`step_merkle_root_v1`): an honest executor cannot commit.
        assert_eq!(
            step_merkle_root_v1(&leaves),
            Err(PalwStepLegError::LeafCountOutOfRange { got: LADDER, max: PALW_STEP_LEG_MAX_LEAVES }),
            "site 1: the root builder refuses the deeper ladder"
        );
        // SITE 2 — the opening builder (`step_opening_v1`): the prover cannot answer the court.
        assert_eq!(
            step_opening_v1(&leaves, 0),
            Err(PalwStepLegError::LeafCountOutOfRange { got: LADDER, max: PALW_STEP_LEG_MAX_LEAVES }),
            "site 2: the opening builder refuses the deeper ladder"
        );

        // Now the same three calls against the RULESET's ladder top. All three clear.
        let root = step_merkle_root_capped_v1(&leaves, LADDER).expect("site 1 clears under the ruleset cap");
        let opening = step_opening_capped_v1(&leaves, 0, LADDER).expect("site 2 clears under the ruleset cap");
        assert_eq!(opening.siblings.len(), 23, "a 2^22+1 ladder opens 23 siblings, past the literal 22");
        assert_eq!(
            step_opening_root_capped_v1(LADDER, &opening, LADDER),
            Ok(root),
            "the round trip closes once the caps are the ruleset's"
        );

        // SITE 3 — the verifier's leaf-count cap (`step_opening_root_v1`): the court cannot check
        // the answer even if it were handed one.
        assert_eq!(
            step_opening_root_v1(LADDER, &opening),
            Err(PalwStepLegError::LeafCountOutOfRange { got: LADDER, max: PALW_STEP_LEG_MAX_LEAVES }),
            "site 3: the verifier's leaf-count cap refuses the deeper ladder"
        );
        // SITE 4 — the verifier's opening-DEPTH cap, the literal this item exists to delete.
        // Reached with a leaf count the default admits, so the count guard cannot mask it: the
        // refusal here is about the 23 siblings alone.
        assert_eq!(
            step_opening_root_v1(PALW_STEP_LEG_MAX_LEAVES, &opening),
            Err(PalwStepLegError::OpeningTooDeep { got: 23, max: 22 }),
            "site 4: the opening-depth cap refuses 23 siblings at a leaf count it admits"
        );
        // Under a ruleset that froze the deeper ladder the depth cap steps aside, and what is left
        // is the honest shape refusal — this opening is not an opening of a 2^22 tree — which is a
        // different answer and must stay a different answer.
        assert_eq!(
            step_opening_root_capped_v1(PALW_STEP_LEG_MAX_LEAVES, &opening, LADDER),
            Err(PalwStepLegError::OpeningPathTooLong { extra: 1 }),
            "a cap refusal and a shape refusal are not the same verdict"
        );
        // And the depth cap is still a real bound under the deeper ruleset: one sibling past the
        // ladder's own depth is refused, with the DERIVED maximum in the error.
        let mut too_deep = opening.clone();
        too_deep.siblings.push(h64(0x77));
        assert_eq!(
            step_opening_root_capped_v1(LADDER, &too_deep, LADDER),
            Err(PalwStepLegError::OpeningTooDeep { got: 24, max: 23 }),
            "the derived cap still bounds the carried path"
        );
    }

    /// **The acceptance case: a `2^25` ladder, a 25-sibling path, verified.**
    ///
    /// This is what ADR-0077's Phase B fence would arm, and it is the thing the shipped tree could
    /// not demonstrate. The tree is walked rather than stored (see [`virtual_subtree`]) because
    /// `2^25` leaf hashes are 2 GiB.
    #[test]
    fn a_2_25_ladder_opens_25_siblings_and_verifies_under_its_own_ruleset() {
        const HEIGHT: u32 = 25;
        const LADDER: u64 = 1 << HEIGHT;
        // An index with mixed parity all the way up, so the walk exercises both node orders.
        const TARGET: u64 = 0x0155_5555;

        let (root, siblings) = virtual_root_and_path(HEIGHT, TARGET);
        assert_eq!(siblings.len(), 25, "a 2^25 ladder needs 25 siblings and the leg refused at 22");
        assert_eq!(step_leg_max_opening_siblings_v1(LADDER), 25);
        let opening = PalwStepOpeningV1 { leaf_index: TARGET, leaf_hash: virtual_leaf(TARGET), siblings };

        // Today, with the module constant: refused before it is even walked.
        assert_eq!(
            step_opening_root_v1(LADDER, &opening),
            Err(PalwStepLegError::LeafCountOutOfRange { got: LADDER, max: PALW_STEP_LEG_MAX_LEAVES }),
            "the default ladder still refuses 2^25 — nothing here arms anything"
        );
        // Under a ruleset that froze `max_step_leaf_count = 2^25`: it verifies.
        assert_eq!(step_opening_root_capped_v1(LADDER, &opening, LADDER), Ok(root), "a 25-sibling opening must reach the 2^25 root");
        // A bent sibling must not, and an over-long path is still refused at the DERIVED depth.
        let mut bent = opening.clone();
        bent.siblings[7] = h64(0xEE);
        assert_ne!(step_opening_root_capped_v1(LADDER, &bent, LADDER), Ok(root), "a bent path must not reach the root");
        let mut long = opening.clone();
        long.siblings.push(h64(0xEE));
        assert_eq!(step_opening_root_capped_v1(LADDER, &long, LADDER), Err(PalwStepLegError::OpeningTooDeep { got: 26, max: 25 }));
        // A ruleset one rung shallower refuses the ladder itself, by leaf count.
        assert_eq!(
            step_opening_root_capped_v1(LADDER, &opening, LADDER / 2),
            Err(PalwStepLegError::LeafCountOutOfRange { got: LADDER, max: LADDER / 2 })
        );
    }
}

// =============================================================================================
// Tests — ADR-0082 Decision 4: a state chunk is individually openable
// =============================================================================================

#[cfg(test)]
mod state_chunk_opening_tests {
    use super::*;

    fn chunk_hashes(n: usize) -> Vec<Hash64> {
        (0..n).map(|i| Hash64::from_bytes([(i % 251) as u8 + 3; 64])).collect()
    }

    /// **A chunk opening recomputes the root the builder builds** — at every chunk count, every
    /// index, and every odd width where the promote-odd rule is the only thing that could make an
    /// opening and a root disagree.
    ///
    /// This is the pin ADR-0082 Decision 4 rests on: the bottom of a dissection opens ONE tile of
    /// the checkpoint, and it can only do that if a chunk's membership is provable without the
    /// other chunks.
    #[test]
    fn a_state_chunk_opening_is_the_root_the_builder_builds() {
        for count in [1usize, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 64, 100, 1_000] {
            let hashes = chunk_hashes(count);
            let root = state_chunks_root_v1(&hashes).expect("a root");
            for index in 0..count {
                let path = state_chunk_path_v1(&hashes, index).expect("a path for a chunk in the tree");
                assert_eq!(
                    state_chunk_opening_root_v1(count, index as u32, &hashes[index], &path).expect("an opening"),
                    root,
                    "count {count}, index {index}: the opening does not rebuild the root"
                );
                // A path is at most the tree's depth, and the promoted tail spends fewer.
                assert!(path.len() <= count.next_power_of_two().trailing_zeros() as usize, "count {count}, index {index}");
            }
        }
    }

    /// **What a chunk opening may not be**: another chunk's hash, another index, a path of the
    /// wrong length, an index past the map, an empty or oversized map — each refused by name.
    #[test]
    fn a_state_chunk_opening_refuses_what_is_not_in_the_tree() {
        let count = 13usize;
        let hashes = chunk_hashes(count);
        let root = state_chunks_root_v1(&hashes).expect("a root");
        let path = state_chunk_path_v1(&hashes, 5).expect("a path");
        assert_eq!(state_chunk_opening_root_v1(count, 5, &hashes[5], &path).expect("honest"), root);
        // A different chunk's bytes under the same index rebuild a different root — the refusal
        // the CALLER makes, and the reason this function returns a root rather than a bool.
        assert_ne!(state_chunk_opening_root_v1(count, 5, &hashes[6], &path).expect("still a root"), root);
        // The same chunk claimed at another index likewise.
        assert_ne!(state_chunk_opening_root_v1(count, 4, &hashes[5], &path).expect("still a root"), root);
        // Structural refusals.
        assert_eq!(
            state_chunk_opening_root_v1(count, 13, &hashes[5], &path),
            Err(PalwStepLegError::LeafIndexOutOfRange { index: 13, count: 13 })
        );
        assert_eq!(
            state_chunk_opening_root_v1(0, 0, &hashes[0], &[]),
            Err(PalwStepLegError::StateChunksOutOfRange { got: 0, max: PALW_STEP_LEG_MAX_STATE_CHUNKS })
        );
        assert_eq!(state_chunk_opening_root_v1(count, 5, &hashes[5], &path[..1]), Err(PalwStepLegError::OpeningPathTooShort));
        let mut long = path.clone();
        long.push(Hash64::from_bytes([9; 64]));
        assert_eq!(state_chunk_opening_root_v1(count, 5, &hashes[5], &long), Err(PalwStepLegError::OpeningPathTooLong { extra: 1 }));
        assert_eq!(state_chunk_path_v1(&hashes, 13), Err(PalwStepLegError::LeafIndexOutOfRange { index: 13, count: 13 }));
        assert_eq!(
            state_chunk_path_v1(&[], 0),
            Err(PalwStepLegError::StateChunksOutOfRange { got: 0, max: PALW_STEP_LEG_MAX_STATE_CHUNKS })
        );
    }

    /// **The exported leaf is the leaf the root builder uses.** A one-chunk map's root IS that
    /// leaf, which is the cheapest possible witness that the two spellings are one.
    #[test]
    fn the_exported_state_chunk_leaf_is_the_builders_own() {
        let h = Hash64::from_bytes([7; 64]);
        assert_eq!(state_chunks_root_v1(&[h]).expect("a one-chunk root"), state_chunk_tree_leaf_v1(0, &h));
        // And the leaf is bound to BOTH the index and the chunk hash.
        assert_ne!(state_chunk_tree_leaf_v1(0, &h), state_chunk_tree_leaf_v1(1, &h));
        assert_ne!(state_chunk_tree_leaf_v1(0, &h), state_chunk_tree_leaf_v1(0, &Hash64::from_bytes([8; 64])));
    }
}
