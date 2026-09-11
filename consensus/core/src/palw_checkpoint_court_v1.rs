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
use crate::palw_step_leg::{
    PalwStepBindingV2, checkpoint_leaf_hash_v2, step_opening_root_capped_v1, step_tile_leaf_hash_v1, verify_binding_v1,
};

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
    let (context_hash, profile_hash, checkpoint_profile_hash) =
        verify_binding_v1(&a.binding).map_err(|e| E::Binding(e.to_string()))?;
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
        || checkpoint_leaf_hash_v2(&context_hash, &checkpoint_profile_hash, &a.binding.state_chunk_map_id, leaf)
            != a.anchor.opening.leaf_hash
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
        let at = step_opening_root_capped_v1(a.binding.step_leaf_count, &row.opening, ladder)
            .map_err(|_| E::RowNotCommitted { tile: t })?;
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
    Ok(if committed.as_slice() == chunk_row {
        PalwCheckpointCourtVerdictV1::FalseAccusation
    } else {
        PalwCheckpointCourtVerdictV1::ExecutorGuilty
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_step::{PalwShapeProfileV3, PalwStepCoordinateV1, canonical_step_leaf_index, step_leaf_count_capped_v1};
    use crate::palw_step_leg::{
        PalwCheckpointLeafV2, PalwStepOpeningV1, PalwStepTileLeafV1, checkpoint_genesis_prev_v2, checkpoint_leg_root_v2,
        execution_commitment_root_v2, step_leg_root_v1, step_merkle_path_v1, step_merkle_root_v1,
    };
    use crate::palw_v2::PalwJobContextV2;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// A dense graph-v5 geometry small enough to hand-build a whole step tree for: two layers, one
    /// KV head of 32 lanes, so one cache row is one 32-lane leaf.
    pub(crate) fn tiny_dense_geometry(n_ctx: u32) -> crate::palw_qwen25_profile::PalwQwen25GeometryV1 {
        crate::palw_qwen25_profile::PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 64,
            ffn_dim: 128,
            attn_heads: 2,
            attn_kv_heads: 1,
            attn_head_dim: 32,
            vocab_size: 64,
            n_ctx,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 32,
        }
    }

    /// **A claim's execution, hand-built so every commitment the court reads is authentic**: the
    /// step tree (every leaf synthetic except the cache-write rows, which are the cache's own), a
    /// checkpoint at every position whose chunks are the cache under the class's map, and the
    /// execution root recomputed from both — so `verify_binding_v1` accepts the binding and every
    /// opening walks to it.
    pub(crate) struct HeldFixture {
        pub binding: PalwStepBindingV2,
        pub leaves: Vec<Hash64>,
        pub preimages: std::collections::BTreeMap<u64, PalwStepTileLeafV1>,
        pub checkpoint_leaves: Vec<PalwCheckpointLeafV2>,
        pub checkpoint_hashes: Vec<Hash64>,
        pub chunks: Vec<Vec<Vec<u8>>>,
    }

    /// The prompt the fixture's job commits to — under the tiled root (ADR-0081 Decision 3), so a
    /// held DA court can open one tile of it.
    pub(crate) fn fixture_prompt_ids(prefill: u32) -> Vec<u32> {
        (0..prefill).map(|i| i * 3 + 1).collect()
    }

    /// The cache row the engine would have written at `(kind, layer, position)`.
    fn row(kind: u8, layer: u16, position: u32, lanes: usize) -> Vec<i32> {
        (0..lanes).map(|i| ((kind as i32 * 7919 + layer as i32 * 131 + position as i32 * 17 + i as i32) % 509) - 254).collect()
    }

    fn le(values: &[i32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// `held` picks the map (v4 under graph-v7, v3 under graph-v5); `forge` replaces one cache row
    /// in every checkpoint that holds it — the executor that committed honest cache-write rows
    /// beside a forged checkpoint.
    pub(crate) fn held_fixture(held: bool, prefill: u32, forge: Option<(u8, u16, u32)>) -> HeldFixture {
        let geometry = tiny_dense_geometry(32);
        let profile: PalwShapeProfileV3 = if held {
            crate::palw_qwen25_profile::qwen25_a16_profile_v7(geometry).expect("the v7 row builds")
        } else {
            crate::palw_qwen25_profile::qwen25_a16_profile_v5(geometry).expect("the v5 row builds")
        };
        let context = PalwJobContextV2 {
            version: crate::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"held-fixture".to_vec(),
            job_id: h64(1),
            job_nullifier: h64(2),
            assignment_id: h64(3),
            execution_seed: [7; 32],
            model_profile_id: h64(4),
            runtime_manifest_hash: h64(5),
            runtime_class_id: h64(6),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: crate::palw_step_refute::tiled_logits_scheme_id_v1(),
            cu_ruleset_id: h64(9),
            tokenizer_id: h64(10),
            prompt_token_ids_hash: crate::palw_prompt_ids_v1::prompt_token_ids_root_v1(&fixture_prompt_ids(prefill))
                .expect("a tiled prompt root"),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: 1,
            max_context_tokens: 32,
        };
        let context_hash = context.context_hash();
        let profile_hash = profile.shape_profile_id();
        let count = step_leaf_count_capped_v1(&profile, &context, u64::MAX).expect("the job's leaves");
        let mut leaves: Vec<Hash64> = (0..count).map(|i| h64(0xF000_0000 + i)).collect();
        let mut preimages = std::collections::BTreeMap::new();
        let kv_dim = (profile.attn_kv_heads as usize) * (profile.attn_head_dim as usize);
        let writer_slot = |layer: u16, role: crate::palw_step::PalwStepNodeRoleV1| -> (u32, u32) {
            let (index, node) = profile.attn_nodes.iter().enumerate().find(|(_, n)| n.role == role).expect("a writer");
            (profile.global_node_slot(crate::palw_step::PalwStepTableV1::Attn, layer, index).expect("a slot"), node.tile_len)
        };
        for layer in 0..profile.layer_count {
            for (kind, role) in
                [(0u8, crate::palw_step::PalwStepNodeRoleV1::KCacheWrite), (1u8, crate::palw_step::PalwStepNodeRoleV1::VCacheWrite)]
            {
                let (slot, tile) = writer_slot(layer, role);
                for position in 0..prefill {
                    let values = row(kind, layer, position, kv_dim);
                    for (t, lanes) in values.chunks(tile as usize).enumerate() {
                        let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position, tile_index: t as u32 };
                        let leaf = PalwStepTileLeafV1 { version: 1, coord, value_count: lanes.len() as u32, values_le: le(lanes) };
                        let index = canonical_step_leaf_index(&profile, &context, &coord).expect("a committed coordinate");
                        leaves[index as usize] = step_tile_leaf_hash_v1(&context_hash, &profile_hash, &leaf);
                        preimages.insert(index, leaf);
                    }
                }
            }
        }
        let step_root = step_merkle_root_v1(&leaves).expect("a step root");
        let checkpoint_profile = crate::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
            crate::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        );
        let checkpoint_profile_hash = checkpoint_profile.profile_hash();
        let map_id = profile.state_chunk_map_id;
        let checkpoint_count =
            crate::palw_context_ladder::palw_checkpoint_count_v1(&profile, &context, checkpoint_profile.checkpoint_interval);
        assert_eq!(checkpoint_count, prefill, "a per-position class checkpoints every position");
        let mut checkpoint_leaves = Vec::new();
        let mut checkpoint_hashes = Vec::new();
        let mut chunks_by_checkpoint = Vec::new();
        let mut prev = checkpoint_genesis_prev_v2(&context_hash);
        for c in 0..checkpoint_count {
            let positions = c + 1;
            let geometry = if held {
                crate::palw_state_chunk_map::palw_state_layout_v4(&profile, positions).expect("a held layout").attn
            } else {
                tiled_kv_state_geometry_v3(&profile, positions).expect("a tiled layout")
            };
            let chunks: Vec<Vec<u8>> = (0..geometry.chunk_count())
                .map(|i| {
                    let entry = integer_kv_state_chunk_entry_v1(&geometry, i).expect("an entry");
                    let kind = match entry.kind {
                        PalwStateChunkKindV1::Key => 0u8,
                        PalwStateChunkKindV1::Value => 1u8,
                    };
                    (entry.position_start..entry.position_start + entry.position_count)
                        .flat_map(|p| {
                            let mut values = row(kind, entry.attn_layer, p, kv_dim);
                            if forge == Some((kind, entry.attn_layer, p)) {
                                values[0] ^= 0x55;
                            }
                            le(&values)
                        })
                        .collect()
                })
                .collect();
            let root =
                crate::palw_state_chunk_map::palw_state_chunks_root_for_map_v1(&profile, positions, &chunks).expect("a state root");
            let leaf = PalwCheckpointLeafV2 {
                version: 1,
                checkpoint_index: c,
                covered_decode_call: positions,
                prev_checkpoint_leaf_hash: prev,
                state_chunk_count: chunks.len() as u32,
                state_chunks_root: root,
            };
            let hash = checkpoint_leaf_hash_v2(&context_hash, &checkpoint_profile_hash, &map_id, &leaf);
            prev = hash;
            checkpoint_leaves.push(leaf);
            checkpoint_hashes.push(hash);
            chunks_by_checkpoint.push(chunks);
        }
        let checkpoint_root = step_merkle_root_v1(&checkpoint_hashes).expect("a checkpoint root");
        let (logits, activation) = (h64(0xAAA), h64(0xBBB));
        let committed = execution_commitment_root_v2(
            &context_hash,
            &logits,
            &activation,
            &checkpoint_leg_root_v2(&context_hash, &checkpoint_profile_hash, &map_id, 0, checkpoint_count, &checkpoint_root),
            &step_leg_root_v1(&context_hash, &profile_hash, count, &step_root),
        );
        let binding = PalwStepBindingV2 {
            version: 1,
            job_context: context,
            shape_profile: profile,
            checkpoint_profile,
            state_chunk_map_id: map_id,
            full_logits_trace_root: logits,
            activation_leg_root: activation,
            step_leaf_count: count,
            step_merkle_root: step_root,
            checkpoint_count,
            checkpoint_merkle_root: checkpoint_root,
            committed_execution_root: committed,
        };
        verify_binding_v1(&binding).expect("the hand-built binding authenticates");
        HeldFixture { binding, leaves, preimages, checkpoint_leaves, checkpoint_hashes, chunks: chunks_by_checkpoint }
    }

    impl HeldFixture {
        pub(crate) fn class_id(&self) -> Hash64 {
            self.binding.shape_profile.shape_profile_id()
        }

        /// The accusation of `(kind, layer, position)` read out of checkpoint `c`.
        pub(crate) fn accusation(&self, c: u32, kind: u8, layer: u16, position: u32) -> PalwCheckpointAccusationV1 {
            let profile = &self.binding.shape_profile;
            let positions = c + 1;
            let geometry = if palw_map_is_held_v4(&profile.state_chunk_map_id) {
                palw_state_layout_v4(profile, positions).expect("a layout").attn
            } else {
                tiled_kv_state_geometry_v3(profile, positions).expect("a layout")
            };
            let series = if kind == 0 { PalwStateChunkKindV1::Key } else { PalwStateChunkKindV1::Value };
            let (index, _) = integer_kv_state_locate_v1(&geometry, series, layer, position).expect("a chunk");
            let chunks = &self.chunks[c as usize];
            let siblings = crate::palw_state_chunk_map::palw_state_chunk_path_for_map_v1(profile, positions, chunks, index as u32)
                .expect("a chunk path");
            let role = if kind == 0 { PalwStepNodeRoleV1::KCacheWrite } else { PalwStepNodeRoleV1::VCacheWrite };
            let writer = profile.attn_nodes.iter().position(|n| n.role == role).expect("a writer");
            let slot = profile.global_node_slot(PalwStepTableV1::Attn, layer, writer).expect("a slot");
            let rows = self
                .preimages
                .iter()
                .filter(|(_, leaf)| leaf.coord.node_slot == slot && leaf.coord.position == position && leaf.coord.call_index == 0)
                .map(|(index, leaf)| PalwAttnRowOpeningV1 {
                    leaf: leaf.clone(),
                    opening: PalwStepOpeningV1 {
                        leaf_index: *index,
                        leaf_hash: self.leaves[*index as usize],
                        siblings: step_merkle_path_v1(&self.leaves, *index as usize).expect("a path"),
                    },
                })
                .collect();
            PalwCheckpointAccusationV1 {
                version: PALW_CHECKPOINT_COURT_VERSION_V1,
                claim: h64(0xC1A1),
                execution_root: self.binding.committed_execution_root,
                trace_root: h64(0x7A),
                executor_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
                    crate::tx::TransactionId::from_u64_word(1),
                    0,
                )),
                accuser_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
                    crate::tx::TransactionId::from_u64_word(2),
                    0,
                )),
                binding: self.binding.clone(),
                anchor: PalwAttnCheckpointAnchorV1 {
                    leaf: self.checkpoint_leaves[c as usize].clone(),
                    opening: PalwStepOpeningV1 {
                        leaf_index: u64::from(c),
                        leaf_hash: self.checkpoint_hashes[c as usize],
                        siblings: step_merkle_path_v1(&self.checkpoint_hashes, c as usize).expect("a checkpoint path"),
                    },
                },
                chunk: PalwAttnChunkOpeningV1 { chunk_index: index as u32, chunk_bytes: chunks[index as usize].clone(), siblings },
                kind,
                attn_layer: layer,
                position,
                rows,
                signature: vec![1, 2, 3],
            }
        }
    }

    const LADDER: u64 = 1 << 26;

    /// **An honest checkpoint acquits, on both maps, at every row of every checkpoint that holds
    /// it** — and the one-move court charges the accuser.
    #[test]
    fn an_honest_checkpoint_acquits_on_both_maps() {
        for held in [false, true] {
            let fx = held_fixture(held, 20, None);
            for (c, kind, layer, position) in [(19u32, 0u8, 0u16, 0u32), (19, 1, 1, 19), (7, 0, 1, 3), (16, 1, 0, 16), (15, 0, 0, 15)]
            {
                let a = fx.accusation(c, kind, layer, position);
                assert_eq!(
                    palw_checkpoint_court_verdict_v1(&a, fx.class_id(), LADDER),
                    Ok(PalwCheckpointCourtVerdictV1::FalseAccusation),
                    "held={held} c={c} kind={kind} layer={layer} position={position}"
                );
            }
        }
    }

    /// **A checkpoint that holds a row the execution never wrote convicts its executor** — the gap
    /// ADR-0103 Decision 1 closes: every step leaf of this execution is consistent with its inputs,
    /// and only the checkpoint disagrees with the cache-write row it claims to hold.
    #[test]
    fn a_forged_checkpoint_row_convicts_on_both_maps() {
        for held in [false, true] {
            let fx = held_fixture(held, 20, Some((0, 1, 5)));
            for c in [5u32, 9, 19] {
                let a = fx.accusation(c, 0, 1, 5);
                assert_eq!(
                    palw_checkpoint_court_verdict_v1(&a, fx.class_id(), LADDER),
                    Ok(PalwCheckpointCourtVerdictV1::ExecutorGuilty),
                    "held={held} c={c}"
                );
            }
            // Another row of the same forged checkpoint is honest, and accusing it is false.
            assert_eq!(
                palw_checkpoint_court_verdict_v1(&fx.accusation(9, 0, 1, 6), fx.class_id(), LADDER),
                Ok(PalwCheckpointCourtVerdictV1::FalseAccusation)
            );
        }
    }

    /// **Evidence that is not what it says is refused by name, never adjudicated.**
    #[test]
    fn evidence_that_is_not_what_it_says_is_refused_by_name() {
        for held in [false, true] {
            let fx = held_fixture(held, 20, None);
            let good = fx.accusation(9, 0, 1, 5);
            let refuse = |a: &PalwCheckpointAccusationV1| palw_checkpoint_court_verdict_v1(a, fx.class_id(), LADDER);
            // The position is past what the checkpoint holds.
            let mut past = good.clone();
            past.position = 10;
            assert!(matches!(refuse(&past), Err(PalwCheckpointCourtError::AnchorDoesNotHoldThePosition { .. })), "held={held}");
            // The chunk's bytes changed: not in the checkpoint.
            let mut bytes = good.clone();
            bytes.chunk.chunk_bytes[0] ^= 1;
            assert_eq!(refuse(&bytes), Err(PalwCheckpointCourtError::ChunkNotInCheckpoint), "held={held}");
            // Another chunk's index for this row.
            let mut other = good.clone();
            other.chunk.chunk_index += 1;
            assert!(matches!(refuse(&other), Err(PalwCheckpointCourtError::WrongChunk { .. })), "held={held}");
            // The committed row opened at another position.
            let mut elsewhere = good.clone();
            elsewhere.rows = fx.accusation(9, 0, 1, 4).rows;
            assert_eq!(refuse(&elsewhere), Err(PalwCheckpointCourtError::RowNotCommitted { tile: 0 }), "held={held}");
            // A checkpoint the leg never committed.
            let mut anchor = good.clone();
            anchor.anchor.leaf.state_chunks_root = h64(9);
            assert_eq!(refuse(&anchor), Err(PalwCheckpointCourtError::AnchorNotCommitted), "held={held}");
            // Another class's id.
            assert!(matches!(
                palw_checkpoint_court_verdict_v1(&good, h64(77), LADDER),
                Err(PalwCheckpointCourtError::ClassMismatch { .. })
            ));
            // The accuser is the accused.
            let mut own = good.clone();
            own.accuser_bond = own.executor_bond;
            assert_eq!(refuse(&own), Err(PalwCheckpointCourtError::AccuserIsTheAccused));
        }
    }
}
