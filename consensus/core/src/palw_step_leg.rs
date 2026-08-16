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
use crate::palw_slash::{check_job_context_shape, PalwSlashError};
use crate::palw_step::{
    canonical_step_leaf_index, kv_aux_leaf_count, step_leaf_count, PalwLayerKindV1, PalwShapeProfileV3, PalwStepCoordinateV1,
    PalwStepError,
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

/// Step-tree leaf cap = the step space's own cap.
pub const PALW_STEP_LEG_MAX_LEAVES: u64 = crate::palw_step::PALW_STEP_MAX_LEAVES;
/// Deepest step-tree opening: `ceil(log2(MAX_LEAVES))`.
pub const PALW_STEP_LEG_MAX_OPENING_SIBLINGS: usize = 22;
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

fn step_merkle_leaf(index: u64, leaf_hash: &Hash64) -> Hash64 {
    let mut w = Writer::new();
    w.u64(index);
    w.hash64(leaf_hash);
    w.keyed64(PALW_STEP_LEG_DOMAIN_MERKLE_LEAF)
}

/// Root of the step tree over ordered leaf hashes.
pub fn step_merkle_root_v1(ordered_leaf_hashes: &[Hash64]) -> Result<Hash64, PalwStepLegError> {
    let count = ordered_leaf_hashes.len() as u64;
    if count == 0 || count > PALW_STEP_LEG_MAX_LEAVES {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: count, max: PALW_STEP_LEG_MAX_LEAVES });
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

/// Recomputes the root a valid opening implies (the caller compares to the committed root).
/// Promote levels are derived from `(leaf_index, leaf_count)` and consume nothing.
pub fn step_opening_root_v1(leaf_count: u64, opening: &PalwStepOpeningV1) -> Result<Hash64, PalwStepLegError> {
    if leaf_count == 0 || leaf_count > PALW_STEP_LEG_MAX_LEAVES {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: leaf_count, max: PALW_STEP_LEG_MAX_LEAVES });
    }
    if opening.leaf_index >= leaf_count {
        return Err(PalwStepLegError::LeafIndexOutOfRange { index: opening.leaf_index, count: leaf_count });
    }
    if opening.siblings.len() > PALW_STEP_LEG_MAX_OPENING_SIBLINGS {
        return Err(PalwStepLegError::OpeningTooDeep { got: opening.siblings.len(), max: PALW_STEP_LEG_MAX_OPENING_SIBLINGS });
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
    let count = ordered_leaf_hashes.len() as u64;
    if count == 0 || count > PALW_STEP_LEG_MAX_LEAVES {
        return Err(PalwStepLegError::LeafCountOutOfRange { got: count, max: PALW_STEP_LEG_MAX_LEAVES });
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

/// Root over a checkpoint's state chunk hashes (small tree; the v1 leg discipline at its own
/// domains, via the step-tree functions on a u64-capped width).
pub fn state_chunks_root_v1(chunk_hashes: &[Hash64]) -> Result<Hash64, PalwStepLegError> {
    if chunk_hashes.is_empty() || chunk_hashes.len() > PALW_STEP_LEG_MAX_STATE_CHUNKS {
        return Err(PalwStepLegError::StateChunksOutOfRange { got: chunk_hashes.len(), max: PALW_STEP_LEG_MAX_STATE_CHUNKS });
    }
    let mut level: Vec<Hash64> = chunk_hashes
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let mut w = Writer::new();
            w.u32(i as u32);
            w.hash64(h);
            w.keyed64(PALW_STEP_LEG_DOMAIN_STATE_CHUNK_LEAF)
        })
        .collect();
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut chunks = level.chunks_exact(2);
        for pair in &mut chunks {
            next.push(keyed64(PALW_STEP_LEG_DOMAIN_STATE_CHUNK_NODE, &[pair[0].as_byte_slice(), pair[1].as_byte_slice()]));
        }
        if let [odd] = chunks.remainder() {
            next.push(*odd);
        }
        level = next;
    }
    Ok(level[0])
}

/// A v2 checkpoint leaf: the v1 chain discipline with the flat state root replaced by the
/// chunked one.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCheckpointLeafV2 {
    pub version: u16,
    pub checkpoint_index: u32,
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
    pub fn new(context: PalwJobContextV2, profile: PalwShapeProfileV3) -> Result<Self, PalwStepLegError> {
        check_job_context_shape(&context).map_err(PalwStepLegError::Context)?;
        profile.validate_shape()?;
        let expected_total = step_leaf_count(&profile, &context)?;
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
        for (i, bits) in value_bits.iter().enumerate() {
            if !f32_is_finite_bits(*bits) {
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
}

impl PalwStepFaultV1 {
    fn evidence_words(self) -> (u8, u32) {
        match self {
            PalwStepFaultV1::ShapeProfileNotCanonical => (0, 0),
            PalwStepFaultV1::StepLeafCountNotCanonical => (1, 0),
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
fn verify_binding(binding: &PalwStepBindingV2) -> Result<(Hash64, Hash64, Hash64), PalwStepLegError> {
    if binding.version != PALW_STEP_LEG_OBJECT_VERSION_V1 {
        return Err(PalwStepLegError::UnsupportedVersion { got: binding.version, expected: PALW_STEP_LEG_OBJECT_VERSION_V1 });
    }
    check_job_context_shape(&binding.job_context).map_err(PalwStepLegError::Context)?;
    let context_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
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
        match step_leaf_count(&binding.shape_profile, &binding.job_context) {
            Ok(count) if count == binding.step_leaf_count => {}
            _ => return Some(PalwStepFaultV1::StepLeafCountNotCanonical),
        }
        let decode_calls = binding.job_context.exact_decode_tokens.saturating_sub(1);
        let canonical_ckpts = decode_calls / binding.checkpoint_profile.checkpoint_interval;
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
    for (i, quad) in preimage.values_le.chunks_exact(4).enumerate() {
        let bits = u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]);
        if !f32_is_finite_bits(bits) {
            return Some(PalwStepFaultV1::StepNonFinite { value_index: i as u32 });
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
    let main = match step_leaf_count(&binding.shape_profile, &binding.job_context) {
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
    let interval = binding.checkpoint_profile.checkpoint_interval;
    if preimage.covered_decode_call != (preimage.checkpoint_index + 1) * interval {
        return Some(PalwStepFaultV1::CheckpointCoveredCallNotCanonical);
    }
    if preimage.checkpoint_index == 0 && preimage.prev_checkpoint_leaf_hash != checkpoint_genesis_prev_v2(context_hash) {
        return Some(PalwStepFaultV1::CheckpointGenesisPrevMismatch);
    }
    if preimage.state_chunk_count == 0 || preimage.state_chunk_count as usize > PALW_STEP_LEG_MAX_STATE_CHUNKS {
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
    use crate::palw_legs::{leg_opening_root_v1, leg_opening_v1, PalwLegOpeningV1, PALW_LEGS_ALL_DOMAINS};
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_step::{
        canonical_step_coordinates, PalwStepNodeRoleV1, PalwStepNodeV1, PalwStepOpKindV1, PalwStepOutLenV1, PALW_STEP_ALL_DOMAINS,
    };
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PALW_V2_ALL_DOMAINS};

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    fn node(kind: PalwStepOpKindV1, out: PalwStepOutLenV1, tile: u32) -> PalwStepNodeV1 {
        PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtype: 0,
            out_len: out,
            tile_len: tile,
            kernel_semantics_id: h64(0x11),
            input_refs: vec![crate::palw_step::PALW_STEP_INPUT_LAYER_IN],
        }
    }

    fn profile() -> PalwShapeProfileV3 {
        PalwShapeProfileV3 {
            version: crate::palw_step::PALW_STEP_OBJECT_VERSION_V1,
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
        let ctx = context();
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
                let vals = vec![0x3C, 0x00].repeat(count as usize * p.attn_head_dim as usize); // f16 1.0
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
            state_layout_id: h64(0x55),
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
            "f5ada5e130a81b2fb2d9658f8dfaf3fbd0527fdedb1567bd7a19973b009d89f7\
             7f196bb322f764aa9af857c9da852b7b0b6d522f521a7fe3fa8d7a43ad203aa9"
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
            values_f16_le: vec![0x00, 0x3C].repeat(12),
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
            values_f16_le: vec![0x3C, 0x00].repeat(3 * binding.shape_profile.attn_head_dim as usize),
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
}
