//! **ADR-0103 Decision 1 — a checkpoint's chunk is tried against the rows it claims to hold, in
//! one move.**
//!
//! A class whose map addresses history tiles commits a checkpoint at every position (ADR-0082
//! Decision 4, amended), and the court's attention bottom reads a history tile OUT of such a
//! checkpoint (`palw_attn_court_v1`'s checkpoint route) — while nothing anywhere asks whether the
//! chunk's rows are the rows the execution actually wrote into its cache. The step leaves of a
//! layer's `KCacheWrite` / `VCacheWrite` node ARE those rows ("the engine pushes one `k_rot` into
//! the cache and records the same vector as a step row"), and the checkpoint is a second
//! commitment to the same bytes. An executor that committed honest cache-write rows beside a
//! forged checkpoint, and ran its attention over the forged one, leaves every step leaf consistent
//! with its own inputs: the one-move court acquits each leaf, and the dissection's bottom recomputes
//! the forged history from the forged chunk and acquits that too. The two commitments disagree and
//! no court asked.
//!
//! This is the court that asks. An accusation names a checkpoint, one chunk of it, and one row of
//! that chunk — `(kind, attention layer, position)` — and carries the committed cache-write row at
//! that position, opened against the claim's step root. The chain opens the checkpoint against the
//! claim's checkpoint leg, the chunk against the checkpoint's state root (under the class's own map,
//! v3 or the held v4), reads the row out of the chunk, and compares it with the committed row
//! byte for byte. Different: the executor committed two different caches, and the claim is void.
//! Equal: the accusation was false, and the accuser pays what the one-move court charges.
//!
//! **What it does not try.** A recurrence chunk (a hybrid's delta or convolution state) is not a
//! row the execution committed anywhere else — it is a state accumulated over positions — so it
//! has no second commitment to disagree with; refused by name (`RecurrenceChunkNotAccusable`).
//! The seat that recomputes (ADR-0082 Decision 9) or resumes (ADR-0103 Decision 2) from the class's
//! kernels reaches a forged recurrence state through the rows it replays after it, which the
//! one-move court tries.

use crate::Hash64;
use crate::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnChunkOpeningV1, PalwAttnRowOpeningV1};
use crate::palw_state_chunk_map::{
    PalwStateChunkKindV1, integer_kv_state_chunk_entry_v1, integer_kv_state_locate_v1, integer_kv_state_row_v1,
    palw_map_addresses_history_tiles_v1, palw_map_is_held_v4, palw_state_chunk_leaf_for_map_v1, palw_state_chunk_membership_root_v1,
    palw_state_layout_v4, tiled_kv_state_geometry_v3,
};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step::{PalwLayerKindV1, PalwStepCoordinateV1, PalwStepNodeRoleV1, PalwStepTableV1};
use crate::palw_step_leg::{PalwStepBindingV2, checkpoint_leaf_hash_v2, step_opening_root_capped_v1, step_tile_leaf_hash_v1, verify_binding_v1};

pub const PALW_CHECKPOINT_COURT_VERSION_V1: u16 = 1;
pub const PALW_CHECKPOINT_COURT_DOMAIN_SESSION_V1: &[u8] = b"misaka-palw/checkpoint-court/session/v1";
/// The ML-DSA-87 signing context of a checkpoint accusation — in
/// [`crate::palw_mode_v2::PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V4`] and in no set a live network
/// committed to; `Params::palw_held_context` arms only over a bundle that states V4.
pub const PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT: &[u8] = b"misaka-palw/checkpoint-court/accuse/mldsa87/v1";
pub const PALW_CHECKPOINT_COURT_ALL_DOMAINS: &[&[u8]] =
    &[PALW_CHECKPOINT_COURT_DOMAIN_SESSION_V1, PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT];

/// **The accusation: a row of a checkpoint's chunk, named, with the row the execution wrote.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCheckpointAccusationV1 {
    pub version: u16,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub accuser_bond: PalwBondKeyV2,
    /// The claim's binding, authenticated against `execution_root` by recomputation.
    pub binding: PalwStepBindingV2,
    /// The checkpoint the chunk is read from, opened against the binding's checkpoint leg.
    pub anchor: PalwAttnCheckpointAnchorV1,
    /// The chunk, at its flat index, with its path to the checkpoint's state root under the
    /// class's own map.
    pub chunk: PalwAttnChunkOpeningV1,
    /// Which series: 0 = K, 1 = V.
    pub kind: u8,
    /// The attention layer, in the profile's numbering.
    pub attn_layer: u16,
    /// The history position whose row is accused — inside the chunk, inside the checkpoint.
    pub position: u32,
    /// The committed cache-write row at that position: every tile of the layer's `KCacheWrite`
    /// (or `VCacheWrite`) node, in tile order, each opened against the binding's step root.
    pub rows: Vec<PalwAttnRowOpeningV1>,
    /// The accuser's ML-DSA-87 over [`palw_checkpoint_court_session_id_v1`], under
    /// [`PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT`].
    pub signature: Vec<u8>,
}

/// The session id: the network domain and every field that names what is accused. The evidence
/// (binding, openings) is bound by its own roots — two accusations of the same row with different
/// evidence are the same accusation.
pub fn palw_checkpoint_court_session_id_v1(network_domain: &[u8], a: &PalwCheckpointAccusationV1) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_CHECKPOINT_COURT_DOMAIN_SESSION_V1).to_state();
    s.update(&(network_domain.len() as u32).to_le_bytes());
    s.update(network_domain);
    s.update(&a.version.to_le_bytes());
    s.update(a.claim.as_byte_slice());
    s.update(a.execution_root.as_byte_slice());
    s.update(a.trace_root.as_byte_slice());
    s.update(&borsh::to_vec(&a.executor_bond).expect("borsh"));
    s.update(&borsh::to_vec(&a.accuser_bond).expect("borsh"));
    s.update(&a.anchor.leaf.checkpoint_index.to_le_bytes());
    s.update(&a.chunk.chunk_index.to_le_bytes());
    s.update(&[a.kind]);
    s.update(&a.attn_layer.to_le_bytes());
    s.update(&a.position.to_le_bytes());
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// The bytes a close ceiling prices: the whole object but the signature, on the wire.
pub fn palw_checkpoint_court_accusation_bytes_v1(a: &PalwCheckpointAccusationV1) -> u64 {
    let whole = borsh::to_vec(a).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    whole.saturating_sub(a.signature.len() as u64)
}

/// What the one move decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCheckpointCourtVerdictV1 {
    /// The chunk's row is not the row the execution committed: two caches, one claim — void.
    ExecutorGuilty,
    /// The chunk holds the committed row: the accusation is false.
    FalseAccusation,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwCheckpointCourtError {
    #[error("accusation version {got}; this build adjudicates version {expected}")]
    Version { got: u16, expected: u16 },
    #[error("the accuser is the accused")]
    AccuserIsTheAccused,
    #[error("the binding does not authenticate: {0}")]
    Binding(String),
    #[error("the binding commits to execution root {binding} and the accusation names {named}")]
    BindingRootMismatch { binding: Hash64, named: Hash64 },
    #[error("the binding's profile hashes to {declared}, not to the claim's class {class}")]
    ClassMismatch { declared: Hash64, class: Hash64 },
    #[error("the class's map does not address history tiles: it has no per-position checkpoint to accuse")]
    NotATiledClass,
    #[error("a series kind is 0 (K) or 1 (V), got {0}")]
    Kind(u8),
    #[error("layer {0} is not an attention layer of this class")]
    NotAnAttentionLayer(u16),
    #[error("the class declares no single node writing the {0} cache")]
    NoCacheWriter(&'static str),
    #[error("the checkpoint is not committed in the claim's checkpoint leg")]
    AnchorNotCommitted,
    #[error("checkpoint covering {covered} positions does not hold position {position}")]
    AnchorDoesNotHoldThePosition { covered: u32, position: u32 },
    #[error("the checkpoint declares {declared} chunks and the class's map derives {derived}")]
    ChunkCountMismatch { declared: u32, derived: u64 },
    #[error("the chunk holding that row is {expected}, not {got}")]
    WrongChunk { got: u32, expected: u64 },
    #[error("the chunk is not in the checkpoint's state root")]
    ChunkNotInCheckpoint,
    #[error("the chunk does not carry the accused row")]
    RowNotInChunk,
    #[error("the accused chunk is a recurrence state, which no other commitment holds")]
    RecurrenceChunkNotAccusable,
    #[error("the committed row is {got} tiles; the writing node commits {expected}")]
    RowTileCount { got: usize, expected: usize },
    #[error("tile {tile} of the committed row is not the leaf at its coordinate")]
    RowNotCommitted { tile: usize },
    #[error("the committed row is {got} lanes and the cache row is {expected}")]
    RowWidth { got: usize, expected: usize },
    #[error("the class's layout is not derivable at this checkpoint: {0}")]
    Layout(String),
}

/// **The one move, adjudicated.** Pure: the caller compares the accusation's claim, executor and
/// roots with the claim's record, and supplies the claim's class id and the ruleset's ladder.
pub fn palw_checkpoint_court_verdict_v1(
    a: &PalwCheckpointAccusationV1,
    class_id: Hash64,
    ladder: u64,
) -> Result<PalwCheckpointCourtVerdictV1, PalwCheckpointCourtError> {
    use PalwCheckpointCourtError as E;
    if a.version != PALW_CHECKPOINT_COURT_VERSION_V1 {
        return Err(E::Version { got: a.version, expected: PALW_CHECKPOINT_COURT_VERSION_V1 });
    }
    if a.accuser_bond == a.executor_bond {
        return Err(E::AccuserIsTheAccused);
    }
    if a.binding.committed_execution_root != a.execution_root {
        return Err(E::BindingRootMismatch { binding: a.binding.committed_execution_root, named: a.execution_root });
    }
    let (context_hash, profile_hash, checkpoint_profile_hash) = verify_binding_v1(&a.binding).map_err(|e| E::Binding(e.to_string()))?;
    if profile_hash != class_id {
        return Err(E::ClassMismatch { declared: profile_hash, class: class_id });
    }
    let profile = &a.binding.shape_profile;
    if !palw_map_addresses_history_tiles_v1(profile) {
        return Err(E::NotATiledClass);
    }
    let kind = match a.kind {
        0 => PalwStateChunkKindV1::Key,
        1 => PalwStateChunkKindV1::Value,
        other => return Err(E::Kind(other)),
    };
    if a.attn_layer >= profile.layer_count || profile.layer_kind(a.attn_layer) != PalwLayerKindV1::Attention {
        return Err(E::NotAnAttentionLayer(a.attn_layer));
    }
    // The node that writes this series, by the role the graph declares — exactly one.
    let (role, what) = match kind {
        PalwStateChunkKindV1::Key => (PalwStepNodeRoleV1::KCacheWrite, "K"),
        PalwStateChunkKindV1::Value => (PalwStepNodeRoleV1::VCacheWrite, "V"),
    };
    let mut writers = profile.attn_nodes.iter().enumerate().filter(|(_, n)| n.role == role);
    let (writer_index, writer) = writers.next().ok_or(E::NoCacheWriter(what))?;
    if writers.next().is_some() {
        return Err(E::NoCacheWriter(what));
    }
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, a.attn_layer, writer_index).ok_or(E::NoCacheWriter(what))?;

    // The checkpoint, opened against the claim's checkpoint leg.
    let leaf = &a.anchor.leaf;
    if a.anchor.opening.leaf_index != u64::from(leaf.checkpoint_index)
        || checkpoint_leaf_hash_v2(&context_hash, &checkpoint_profile_hash, &a.binding.state_chunk_map_id, leaf) != a.anchor.opening.leaf_hash
    {
        return Err(E::AnchorNotCommitted);
    }
    let root = step_opening_root_capped_v1(u64::from(a.binding.checkpoint_count), &a.anchor.opening, ladder)
        .map_err(|_| E::AnchorNotCommitted)?;
    if root != a.binding.checkpoint_merkle_root {
        return Err(E::AnchorNotCommitted);
    }
    let positions =
        crate::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, &a.binding.job_context, leaf.covered_decode_call);
    if a.position >= positions {
        return Err(E::AnchorDoesNotHoldThePosition { covered: positions, position: a.position });
    }

    // The class's layout at that checkpoint: the chunk count the leaf must declare, and where the
    // accused row lives.
    let (geometry, count) = if palw_map_is_held_v4(&profile.state_chunk_map_id) {
        let layout = palw_state_layout_v4(profile, positions).map_err(|e| E::Layout(e.to_string()))?;
        let count = layout.chunk_count();
        (layout.attn, count)
    } else {
        let count = crate::palw_state_chunk_map::palw_state_chunk_count_at_v1(profile, positions)
            .ok_or_else(|| E::Layout("the map's count is not derivable".into()))?;
        (tiled_kv_state_geometry_v3(profile, positions).map_err(|e| E::Layout(e.to_string()))?, count)
    };
    if u64::from(leaf.state_chunk_count) != count {
        return Err(E::ChunkCountMismatch { declared: leaf.state_chunk_count, derived: count });
    }
    if u64::from(a.chunk.chunk_index) >= geometry.chunk_count() {
        return Err(E::RecurrenceChunkNotAccusable);
    }
    let (expected, _) = integer_kv_state_locate_v1(&geometry, kind, a.attn_layer, a.position).ok_or(E::RowNotInChunk)?;
    if u64::from(a.chunk.chunk_index) != expected {
        return Err(E::WrongChunk { got: a.chunk.chunk_index, expected });
    }
    let chunk_hash = palw_state_chunk_leaf_for_map_v1(profile, positions, a.chunk.chunk_index, &a.chunk.chunk_bytes)
        .ok_or_else(|| E::Layout("no leaf for this chunk".into()))?;
    let folded = palw_state_chunk_membership_root_v1(profile, positions, a.chunk.chunk_index, &chunk_hash, &a.chunk.siblings)
        .map_err(|_| E::ChunkNotInCheckpoint)?;
    if folded != leaf.state_chunks_root {
        return Err(E::ChunkNotInCheckpoint);
    }
    let entry = integer_kv_state_chunk_entry_v1(&geometry, expected).ok_or(E::RowNotInChunk)?;
    let chunk_row = integer_kv_state_row_v1(&entry, &a.chunk.chunk_bytes, a.position).ok_or(E::RowNotInChunk)?;
    if chunk_row.len() % 4 != 0 {
        return Err(E::RowNotInChunk);
    }

    // The committed row: every tile of the writer's leaf at this position.
    let kv_dim = (profile.attn_kv_heads as usize).saturating_mul(profile.attn_head_dim as usize);
    let tile = (writer.tile_len as usize).max(1);
    let tiles = kv_dim.div_ceil(tile);
    if a.rows.len() != tiles {
        return Err(E::RowTileCount { got: a.rows.len(), expected: tiles });
    }
    let prefill = a.binding.job_context.declared_prefill_tokens;
    let j = u64::from(a.position);
    let (call_index, position) = if j < u64::from(prefill) { (0u32, a.position) } else { ((j - u64::from(prefill) + 1) as u32, 0u32) };
    let mut committed: Vec<u8> = Vec::with_capacity(kv_dim * 4);
    for (t, row) in a.rows.iter().enumerate() {
        let want = PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index: t as u32 };
        if row.leaf.coord != want {
            return Err(E::RowNotCommitted { tile: t });
        }
        if step_tile_leaf_hash_v1(&context_hash, &profile_hash, &row.leaf) != row.opening.leaf_hash {
            return Err(E::RowNotCommitted { tile: t });
        }
        let at = step_opening_root_capped_v1(a.binding.step_leaf_count, &row.opening, ladder).map_err(|_| E::RowNotCommitted { tile: t })?;
        if at != a.binding.step_merkle_root {
            return Err(E::RowNotCommitted { tile: t });
        }
        let lanes = tile.min(kv_dim - t * tile);
        if row.leaf.value_count as usize != lanes || row.leaf.values_le.len() != lanes * 4 {
            return Err(E::RowWidth { got: row.leaf.value_count as usize, expected: lanes });
        }
        committed.extend_from_slice(&row.leaf.values_le);
    }
    if committed.len() != chunk_row.len() {
        return Err(E::RowWidth { got: committed.len() / 4, expected: chunk_row.len() / 4 });
    }
    Ok(if committed.as_slice() == chunk_row { PalwCheckpointCourtVerdictV1::FalseAccusation } else { PalwCheckpointCourtVerdictV1::ExecutorGuilty })
}
