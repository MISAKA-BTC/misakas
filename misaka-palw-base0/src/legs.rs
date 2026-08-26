//! **From an execution to a step leg** (external audit C-01, ADR-0049 Decision F).
//!
//! The audit's largest finding was that nothing turned a real run into the commitment a court
//! opens against: the worker captured activation taps and logits rows, not per-kernel tile outputs,
//! so `execution_root` was whatever the miner wrote and every step leg in the tree was synthesised
//! by a test. A commitment nobody derives from an execution is a commitment a producer chooses.
//!
//! The BASE-0 half of that is tractable in a way the float half is not, and for a structural
//! reason: BASE-0's executor is [`crate::engine`] — our own scalar Rust — rather than a foreign
//! C++ kernel graph. So the capture is a push per step rather than an instrumentation project.
//!
//! # What this covers, and what it does not
//!
//! [`crate::engine::ForwardProbe::steps`] records EVERY step the IR declares — thirty-six per
//! layer, at the IR's own slot numbers. It used to record ten: the ones the engine happened to
//! compute as a whole row and keep in a variable. The other twenty-six were uncaptured, so a leg
//! committed the zero hash for them and `execution_root` was, at those coordinates, whatever the
//! miner wrote.
//!
//! The per-head steps — scores, amplification, softmax and the narrowing after it — are the ones
//! that needed the loop instrumented, and they are captured head-major into one row per slot,
//! which is exactly the `KvPerHead` width the IR declares. The IR declared them once per LAYER
//! until this landed, so the step space itself was `attn_heads` times too small at the four nodes
//! attention happens in: a challenger disputing the second head's softmax had no coordinate to
//! name it with.
//!
//! What is complete is the SHAPE of the path: rows at IR slots, tiled at the profile's own
//! `tile_len`, hashed with the leg's own leaf rule, into the Merkle root a binding carries. Nothing
//! here invents a coordinate — [`kaspa_consensus_core::palw_step::canonical_step_leaf_index`] is
//! what says where a tile belongs, so a capture cannot disagree with the profile about that.

use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepCoordinateV1, PalwStepTableV1, canonical_step_leaf_index};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepTileLeafV1, step_merkle_root_v1, step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// Why a capture cannot become a leg.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegError {
    /// A capture that does not cover the step space. Committing it would say "computed zero" about
    /// every leaf nobody filled, and the court cannot tell that apart from a row that was computed.
    CaptureIncomplete { filled: u64, expected: u64 },
    /// The profile and the capture disagree about which slots exist.
    UnknownSlot { layer: u16, slot: u16 },
    /// `canonical_step_leaf_index` refused the coordinate — the capture is describing a step this
    /// class's step space does not have.
    NotACanonicalCoordinate { layer: u16, slot: u16, tile: u32 },
    /// The step space has no leaves, so there is nothing to commit.
    EmptySpace,
    /// The checkpoint capture is short. Same rule as `CaptureIncomplete` and for the same reason:
    /// `checkpoint_count` is a canonical function of the job, so a producer that commits a count it
    /// did not capture is committing to checkpoints nobody can open.
    CheckpointCaptureIncomplete { got: u32, expected: u32 },
    /// The registered state map refused this job's state — the class is not the integer family, or
    /// its geometry cannot be chunked.
    CheckpointStateMap(kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkMapError),
    /// The cache could not answer a chunk the map names. A checkpoint over a state the engine does
    /// not hold is not a checkpoint.
    CheckpointStateUnavailable { chunk_index: u64 },
}

/// One captured step row, tiled and placed at its canonical leaf index.
#[derive(Clone)]
pub struct Base0StepTilesV1 {
    pub leaves: Vec<Hash64>,
    pub tiles: Vec<(u64, PalwStepTileLeafV1)>,
}

/// One row an execution produced, tagged with the STEP TABLE it belongs to.
///
/// The table is not decoration. A global slot is `pre ‖ layer 0 ‖ … ‖ post`, so "the second post
/// node" and "the second node of layer 0" are different coordinates that an untagged `(layer,
/// slot)` pair cannot tell apart — and the court reads by global slot. Carrying the table means
/// the conversion is [`PalwShapeProfileV3::global_node_slot`]'s, which is the inverse of the walk
/// the court itself uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base0CapturedRowV1 {
    pub table: PalwStepTableV1,
    /// Ignored for `Pre` and `Post`, which have no layer.
    pub layer: u16,
    /// Index WITHIN the table.
    pub index: usize,
    pub row: Vec<i32>,
}

/// Everything one forward call produced, in the form the leg takes.
///
/// The engine records the layer tables in `steps` and the other two in `pre_steps`/`post_steps`,
/// because a row's table decides its slot. This is the one place that fact is turned back into a
/// flat list, so no caller has to know the split.
pub fn base0_captured_rows_v1(probe: &crate::engine::ForwardProbe) -> Vec<Base0CapturedRowV1> {
    let mut rows = Vec::with_capacity(probe.pre_steps.len() + probe.steps.len() + probe.post_steps.len());
    for (index, row) in &probe.pre_steps {
        rows.push(Base0CapturedRowV1 { table: PalwStepTableV1::Pre, layer: 0, index: *index as usize, row: row.clone() });
    }
    for (layer, slot, row) in &probe.steps {
        rows.push(Base0CapturedRowV1 { table: PalwStepTableV1::Attn, layer: *layer, index: *slot as usize, row: row.clone() });
    }
    for (index, row) in &probe.post_steps {
        rows.push(Base0CapturedRowV1 { table: PalwStepTableV1::Post, layer: 0, index: *index as usize, row: row.clone() });
    }
    rows
}

/// **A step-leg capture accumulated across a job's CALLS.**
///
/// A job is one prefill call over `P` positions plus `D − 1` decode calls, and the step space
/// covers every one of them. [`base0_step_tiles_v1`] tiles a SINGLE call, so a producer that used
/// it directly would commit a root over one call's rows and zeros everywhere else — and the zeros
/// would be indistinguishable, in the commitment, from rows that were computed.
///
/// So this accumulates, counts what it has filled, and [`Self::finish`] REFUSES a capture that is
/// short. An executor must never be the one that emits a commitment over a partial capture: that
/// object is what the court exists to convict, and the producer would be convicting itself.
pub struct Base0StepCaptureV1 {
    leaves: Vec<Hash64>,
    tiles: Vec<(u64, PalwStepTileLeafV1)>,
    filled: u64,
}

impl Base0StepCaptureV1 {
    pub fn new(leaf_count: u64) -> Result<Self, LegError> {
        if leaf_count == 0 {
            return Err(LegError::EmptySpace);
        }
        Ok(Self { leaves: vec![Hash64::default(); leaf_count as usize], tiles: Vec::new(), filled: 0 })
    }

    /// Place one call's rows at the coordinates the PROFILE says they belong to.
    pub fn push_call(
        &mut self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        call_index: u32,
        position: u32,
        rows: &[Base0CapturedRowV1],
    ) -> Result<(), LegError> {
        let ctx_hash = ctx.context_hash();
        let profile_hash = profile.shape_profile_id();
        for row in rows {
            let global_slot = profile
                .global_node_slot(row.table, row.layer, row.index)
                .ok_or(LegError::UnknownSlot { layer: row.layer, slot: row.index as u16 })?;
            // Checked against the profile's own forward walk rather than trusted: if the inverse
            // and the walk ever disagreed the capture would be describing a different graph,
            // silently, and every leaf would land one node off.
            let (node, resolved_layer) =
                profile.resolve_node_slot(global_slot).ok_or(LegError::UnknownSlot { layer: row.layer, slot: row.index as u16 })?;
            if matches!(row.table, PalwStepTableV1::Attn | PalwStepTableV1::Gdn) && resolved_layer != Some(row.layer) {
                return Err(LegError::UnknownSlot { layer: row.layer, slot: row.index as u16 });
            }
            let tile_len = node.tile_len as usize;
            if tile_len == 0 {
                return Err(LegError::UnknownSlot { layer: row.layer, slot: row.index as u16 });
            }
            for (tile_index, chunk) in row.row.chunks(tile_len).enumerate() {
                let coord = PalwStepCoordinateV1 { call_index, node_slot: global_slot, position, tile_index: tile_index as u32 };
                let index = canonical_step_leaf_index(profile, ctx, &coord).ok_or(LegError::NotACanonicalCoordinate {
                    layer: row.layer,
                    slot: row.index as u16,
                    tile: tile_index as u32,
                })?;
                if index as usize >= self.leaves.len() {
                    return Err(LegError::NotACanonicalCoordinate {
                        layer: row.layer,
                        slot: row.index as u16,
                        tile: tile_index as u32,
                    });
                }
                let leaf = PalwStepTileLeafV1 {
                    version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                    coord,
                    value_count: chunk.len() as u32,
                    values_le: chunk.iter().flat_map(|v| v.to_le_bytes()).collect(),
                };
                if self.leaves[index as usize] == Hash64::default() {
                    self.filled += 1;
                }
                self.leaves[index as usize] = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &leaf);
                self.tiles.push((index, leaf));
            }
        }
        Ok(())
    }

    /// How much of the step space this capture has actually filled.
    pub fn progress(&self) -> (u64, u64) {
        (self.filled, self.leaves.len() as u64)
    }

    /// Seal the capture. A short one is refused — see the type's docs.
    pub fn finish(self) -> Result<Base0StepTilesV1, LegError> {
        if self.filled != self.leaves.len() as u64 {
            return Err(LegError::CaptureIncomplete { filled: self.filled, expected: self.leaves.len() as u64 });
        }
        Ok(Base0StepTilesV1 { leaves: self.leaves, tiles: self.tiles })
    }

    /// Seal WITHOUT the completeness check — for measurement and for tests that deliberately
    /// commit a partial space. Never for a producer: the resulting root says "computed zero" about
    /// every leaf nobody filled.
    pub fn finish_partial(self) -> Base0StepTilesV1 {
        Base0StepTilesV1 { leaves: self.leaves, tiles: self.tiles }
    }
}

/// Tile a forward pass's captured rows into the leg's leaves.
///
/// `leaf_count` is the class's own step-leaf count for this job; every uncaptured leaf keeps the
/// zero hash, which is why this returns the tiles beside the leaves — a caller that needs a
/// complete root must capture completely, and can see from `tiles.len()` that it has not.
pub fn base0_step_tiles_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    leaf_count: u64,
    call_index: u32,
    position: u32,
    steps: &[(u16, u16, Vec<i32>)],
) -> Result<Base0StepTilesV1, LegError> {
    if leaf_count == 0 {
        return Err(LegError::EmptySpace);
    }
    let ctx_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    let mut out = Base0StepTilesV1 { leaves: vec![Hash64::default(); leaf_count as usize], tiles: Vec::new() };

    for (layer, slot, row) in steps {
        // **`node_slot` is GLOBAL, and the layer lives inside it.** A coordinate carries no layer
        // field: the step space is `pre` then every layer's table in order then `post`, and
        // `resolve_node_slot` is what walks it. A capture that used the per-layer index as the slot
        // would place every layer's rows on top of layer 0's, and the leg would commit the last
        // layer's values under every layer's coordinate — a producer could then execute one thing
        // and open another.
        let global_slot = (profile.pre_nodes.len() as u32)
            .checked_add(
                (*layer as u32)
                    .checked_mul(profile.attn_nodes.len() as u32)
                    .ok_or(LegError::UnknownSlot { layer: *layer, slot: *slot })?,
            )
            .and_then(|g| g.checked_add(*slot as u32))
            .ok_or(LegError::UnknownSlot { layer: *layer, slot: *slot })?;
        // Checked against the profile's own walk rather than trusted: if the two ever disagreed the
        // capture would be describing a different graph, silently.
        let (node, resolved_layer) =
            profile.resolve_node_slot(global_slot).ok_or(LegError::UnknownSlot { layer: *layer, slot: *slot })?;
        if resolved_layer != Some(*layer) {
            return Err(LegError::UnknownSlot { layer: *layer, slot: *slot });
        }
        let tile_len = node.tile_len as usize;
        if tile_len == 0 {
            return Err(LegError::UnknownSlot { layer: *layer, slot: *slot });
        }
        for (tile_index, chunk) in row.chunks(tile_len).enumerate() {
            let coord = PalwStepCoordinateV1 { call_index, node_slot: global_slot, position, tile_index: tile_index as u32 };
            let index = canonical_step_leaf_index(profile, ctx, &coord).ok_or(LegError::NotACanonicalCoordinate {
                layer: *layer,
                slot: *slot,
                tile: tile_index as u32,
            })?;
            if index as usize >= out.leaves.len() {
                return Err(LegError::NotACanonicalCoordinate { layer: *layer, slot: *slot, tile: tile_index as u32 });
            }
            let leaf = PalwStepTileLeafV1 {
                version: PALW_STEP_LEG_OBJECT_VERSION_V1,
                coord,
                value_count: chunk.len() as u32,
                // The leg's own encoding: little-endian, four bytes a value, which is what a BASE-0
                // `int32` lane is.
                values_le: chunk.iter().flat_map(|v| v.to_le_bytes()).collect(),
            };
            out.leaves[index as usize] = step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, &leaf);
            out.tiles.push((index, leaf));
        }
    }
    Ok(out)
}

/// The step leg's Merkle root over `leaves`.
pub fn base0_step_merkle_root_v1(tiles: &Base0StepTilesV1) -> Option<Hash64> {
    step_merkle_root_v1(&tiles.leaves).ok()
}

/// **The bisection's state at index `i`: a commitment to the execution prefix through leaf `i`.**
///
/// The ladder narrows by asking each party "what is your state at the midpoint?", and it can only
/// converge on a real divergence if the answer is a PREFIX commitment: two executions that agree
/// through leaf `i` must produce the same value, and two that differ before it must not. A Merkle
/// root over `leaves[..i]` has exactly that property, so the first index at which the two parties
/// disagree is the first leaf at which their executions do.
///
/// The ladder's own endpoints are the job context hash at 0 and the claim's announced root at the
/// end — anchors the state machine seeds and refuses a disclosure from repeating. Those are a
/// different kind of value (the announced root commits the whole execution, not a prefix of the
/// step space), which is why this is domain-separated and keyed by the context: it is the rung
/// scheme, not a continuation of the endpoints.
///
/// **What this is not.** It does not make a disclosure true — no node can decide that without the
/// execution, and `apply_disclosure` refuses only a state repeating an endpoint. A responder that
/// discloses distinct junk at every rung steers the interval freely. The terminal close is what
/// settles truth, on operand openings the artifact root proves. This function's job is narrower
/// and still necessary: without it the two parties have no shared way to compute the same answer
/// from the same execution, so an HONEST bisection cannot converge at all.
pub fn base0_bisect_prefix_state_v1(ctx: &PalwJobContextV2, leaves: &[Hash64], index: u64) -> Hash64 {
    const DOMAIN: &[u8] = b"misaka-palw/base0/bisect-prefix-state/v1";
    let take = (index as usize).min(leaves.len());
    let mut h = blake2b_simd::Params::new().hash_length(64).key(DOMAIN).to_state();
    h.update(ctx.context_hash().as_byte_slice());
    h.update(&index.to_le_bytes());
    h.update(&(take as u64).to_le_bytes());
    for leaf in &leaves[..take] {
        h.update(leaf.as_byte_slice());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The checkpoint leg, captured** — the half that did not exist.
///
/// Until this, `base0_binding_from_capture_v1` committed `checkpoint_count = decode_calls /
/// interval` beside `Hash64::default()` as the tree root whenever that count was non-zero. For the
/// RC's own canonical job (prefill 8, decode 4) that is **three checkpoints under a zero root**: a
/// producer could not open one, a challenger could not dispute one, and the shape check passed
/// because it only asks whether an EMPTY leg pairs with the empty sentinel.
///
/// A checkpoint here is the engine's replay state serialized through the class's REGISTERED map
/// ([`kaspa_consensus_core::palw_state_chunk_map`]), so what a producer commits and what a verifier
/// decodes are the same layout by construction rather than by agreement.
pub struct Base0CheckpointsV1 {
    pub leaves: Vec<kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2>,
    pub leaf_hashes: Vec<Hash64>,
    pub merkle_root: Hash64,
    /// Per checkpoint, its chunk bytes in map order.
    ///
    /// Retained for exactly the reason the step tiles are: a producer that discarded them could not
    /// answer a checkpoint challenge and would lose its bond by default.
    pub chunks: Vec<Vec<Vec<u8>>>,
}

/// Accumulates checkpoints in call order, chaining each to the one before it.
pub struct Base0CheckpointCaptureV1 {
    ctx: PalwJobContextV2,
    profile: PalwShapeProfileV3,
    ctx_hash: Hash64,
    checkpoint_profile_hash: Hash64,
    state_chunk_map_id: Hash64,
    interval: u32,
    prev: Hash64,
    leaves: Vec<kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2>,
    leaf_hashes: Vec<Hash64>,
    chunks: Vec<Vec<Vec<u8>>>,
}

impl Base0CheckpointCaptureV1 {
    pub fn new(
        ctx: &PalwJobContextV2,
        profile: &PalwShapeProfileV3,
        checkpoint_profile: &kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1,
    ) -> Self {
        let ctx_hash = ctx.context_hash();
        Self {
            ctx: ctx.clone(),
            profile: profile.clone(),
            ctx_hash,
            checkpoint_profile_hash: checkpoint_profile.profile_hash(),
            // **From the profile.** The binding check compares the two, so a capture that reached
            // for the family constant here would build a leg its own binding refuses.
            state_chunk_map_id: profile.state_chunk_map_id,
            interval: checkpoint_profile.checkpoint_interval,
            prev: kaspa_consensus_core::palw_step_leg::checkpoint_genesis_prev_v2(&ctx_hash),
            leaves: Vec::new(),
            leaf_hashes: Vec::new(),
            chunks: Vec::new(),
        }
    }

    /// How many decode calls the NEXT checkpoint will cover — the canonical
    /// `(index + 1) × interval` the court's `checkpoint_fault` recomputes.
    pub fn next_covered_decode_call(&self) -> u32 {
        (self.leaves.len() as u32 + 1) * self.interval
    }

    /// **Take a checkpoint of `cache`**, which must be the state after
    /// [`Self::next_covered_decode_call`] decode calls.
    ///
    /// The position count is derived from the job and the covered call, never from the cache: a
    /// cache that is a row short would otherwise be committed as a shorter state, and the shortfall
    /// would look like a job that ran fewer calls.
    pub fn push(&mut self, cache: &crate::engine::KvCache) -> Result<(), LegError> {
        let geometry = self.next_geometry()?;
        let mut chunk_bytes = Vec::with_capacity(geometry.chunk_count() as usize);
        for index in 0..geometry.chunk_count() {
            let entry = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(&geometry, index)
                .ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?;
            chunk_bytes.push(cache.state_chunk_bytes(&entry).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?);
        }
        self.push_chunks(chunk_bytes)
    }

    /// The map this capture's NEXT checkpoint is taken under.
    pub fn next_geometry(&self) -> Result<kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1, LegError> {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        let positions = map::integer_kv_positions_at_v1(&self.ctx, self.next_covered_decode_call());
        map::integer_kv_state_geometry_v1(&self.profile, positions).map_err(LegError::CheckpointStateMap)
    }

    /// **The leaf rule, in one place.** Serializing a cache and re-deriving from served bytes must
    /// produce the same leaf or a producer and a seat would disagree about what was committed, so
    /// both go through here rather than each hashing for itself.
    pub fn push_chunks(&mut self, chunk_bytes: Vec<Vec<u8>>) -> Result<(), LegError> {
        let geometry = self.next_geometry()?;
        if chunk_bytes.len() as u64 != geometry.chunk_count() {
            return Err(LegError::CheckpointStateUnavailable { chunk_index: chunk_bytes.len() as u64 });
        }
        let covered_decode_call = self.next_covered_decode_call();
        let mut chunk_hashes = Vec::with_capacity(chunk_bytes.len());
        for (index, bytes) in chunk_bytes.iter().enumerate() {
            // The map's own length for this chunk, checked here: bytes of the wrong length hash to
            // a leaf nothing can open, and a producer that committed one could not answer for it.
            let entry = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(&geometry, index as u64)
                .ok_or(LegError::CheckpointStateUnavailable { chunk_index: index as u64 })?;
            if bytes.len() as u64 != entry.byte_len() {
                return Err(LegError::CheckpointStateUnavailable { chunk_index: index as u64 });
            }
            chunk_hashes.push(kaspa_consensus_core::palw_step_leg::state_chunk_leaf_hash_v1(
                &self.state_chunk_map_id,
                index as u32,
                bytes,
            ));
        }
        let state_chunks_root =
            kaspa_consensus_core::palw_step_leg::state_chunks_root_v1(&chunk_hashes).map_err(|_| LegError::EmptySpace)?;

        let leaf = kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            checkpoint_index: self.leaves.len() as u32,
            covered_decode_call,
            prev_checkpoint_leaf_hash: self.prev,
            state_chunk_count: chunk_hashes.len() as u32,
            state_chunks_root,
        };
        let hash = kaspa_consensus_core::palw_step_leg::checkpoint_leaf_hash_v2(
            &self.ctx_hash,
            &self.checkpoint_profile_hash,
            &self.state_chunk_map_id,
            &leaf,
        );
        self.prev = hash;
        self.leaves.push(leaf);
        self.leaf_hashes.push(hash);
        self.chunks.push(chunk_bytes);
        Ok(())
    }

    /// **Re-derive the whole leg from served chunks alone** — the seat's and the challenger's
    /// entry, and the reason `push_chunks` exists separately from `push`.
    ///
    /// A seat that received a producer's state chunks can rebuild the leg and compare its root to
    /// the one the claim committed, without holding the model, without the producer's cache, and
    /// without a second implementation of the leaf rule.
    pub fn from_chunks_v1(
        ctx: &PalwJobContextV2,
        profile: &PalwShapeProfileV3,
        checkpoint_profile: &kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1,
        chunks: &[Vec<Vec<u8>>],
    ) -> Result<Base0CheckpointsV1, LegError> {
        let mut capture = Self::new(ctx, profile, checkpoint_profile);
        for c in chunks {
            capture.push_chunks(c.clone())?;
        }
        let count = chunks.len() as u32;
        capture.finish(count)
    }

    /// Seal the capture at the count the job canonically has.
    ///
    /// `expected` is `decode_calls / interval`, which the court recomputes; a producer that sealed
    /// a different number would be committing to checkpoints it never took (or hiding ones it did).
    pub fn finish(self, expected: u32) -> Result<Base0CheckpointsV1, LegError> {
        let got = self.leaves.len() as u32;
        if got != expected {
            return Err(LegError::CheckpointCaptureIncomplete { got, expected });
        }
        let merkle_root = if got == 0 {
            kaspa_consensus_core::palw_step_leg::checkpoint_empty_root_v2(&self.ctx_hash)
        } else {
            step_merkle_root_v1(&self.leaf_hashes).map_err(|_| LegError::EmptySpace)?
        };
        Ok(Base0CheckpointsV1 { leaves: self.leaves, leaf_hashes: self.leaf_hashes, merkle_root, chunks: self.chunks })
    }
}

/// **The producer's own commitment, from its own capture** (audit C-01).
///
/// A binding is not a bag of hashes a caller fills in: `verify_binding` recomputes
/// `committed_execution_root` from the four leg roots and refuses anything else, so a hand-written
/// binding fails before a single opening is read. That is the check doing its job, and it is also
/// why nothing outside the checker's own tests could ever build one — the producer side did not
/// exist.
///
/// Everything here is derived from the capture and the job: the step leg's root is the capture's,
/// the checkpoint leg is the empty one this class's single-call jobs have, and the execution root
/// is the composition. What a caller supplies is the two roots this module does not own — the
/// logits trace and the activation leg.
pub fn base0_binding_from_capture_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    tiles: &Base0StepTilesV1,
    checkpoints: &Base0CheckpointsV1,
    full_logits_trace_root: Hash64,
    activation_leg_root: Hash64,
) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, LegError> {
    use kaspa_consensus_core::palw_step_leg::{
        PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, checkpoint_empty_root_v2, checkpoint_leg_root_v2,
        execution_commitment_root_v2, step_leg_root_v1,
    };
    let context_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    // The family's registered layout, at this producer's interval. Both were `Hash64::default()`
    // — the unregistered sentinel — which was the only honest value while no map existed; filing
    // it now would file a layout the class does not register, and `verify_binding` refuses that.
    let checkpoint_profile = kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(1);
    let step_leaf_count = tiles.leaves.len() as u64;
    let step_merkle_root = step_merkle_root_v1(&tiles.leaves).map_err(|_| LegError::EmptySpace)?;
    // **From the profile, not from the family constant.** A producer files what ITS class
    // registered; reaching for the constant here would work today and would silently file the
    // integer family's map for a class that had registered something else. One source.
    let state_chunk_map_id = profile.state_chunk_map_id;
    // The canonical checkpoint count is `decode_calls / interval`; a job with one decode token has
    // no decode CALLS, so the leg is the empty one — and the shape pass refuses any other pairing
    // of count and root, which is why this is derived rather than chosen.
    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let checkpoint_count = decode_calls / checkpoint_profile.checkpoint_interval;
    // **The captured root, not a placeholder.** This line used to read
    //     if checkpoint_count == 0 { empty } else { Hash64::default() }
    // which committed a non-zero count under a ZERO tree root — three checkpoints nobody could
    // open, for the RC's own canonical job. The shape check never caught it because it only asks
    // whether an EMPTY leg pairs with the empty sentinel.
    if checkpoints.leaf_hashes.len() as u32 != checkpoint_count {
        return Err(LegError::CheckpointCaptureIncomplete { got: checkpoints.leaf_hashes.len() as u32, expected: checkpoint_count });
    }
    let checkpoint_merkle_root = checkpoints.merkle_root;
    debug_assert_eq!(checkpoint_count == 0, checkpoint_merkle_root == checkpoint_empty_root_v2(&context_hash));
    let checkpoint_profile_hash = checkpoint_profile.profile_hash();
    let checkpoint_root = checkpoint_leg_root_v2(
        &context_hash,
        &checkpoint_profile_hash,
        &state_chunk_map_id,
        decode_calls,
        checkpoint_count,
        &checkpoint_merkle_root,
    );
    let step_root = step_leg_root_v1(&context_hash, &profile_hash, step_leaf_count, &step_merkle_root);
    let committed_execution_root =
        execution_commitment_root_v2(&context_hash, &full_logits_trace_root, &activation_leg_root, &checkpoint_root, &step_root);
    Ok(PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: ctx.clone(),
        shape_profile: profile.clone(),
        checkpoint_profile,
        state_chunk_map_id,
        full_logits_trace_root,
        activation_leg_root,
        step_leaf_count,
        step_merkle_root,
        checkpoint_count,
        checkpoint_merkle_root,
        committed_execution_root,
    })
}

/// **A complete refutation, assembled from a real capture** (audit C-01's other half).
///
/// The checker existed and the prover did not. `check_execution_step_refutation_v1` computes the
/// canonical input set privately and refuses any other, so a producer had to guess the rule and
/// would learn only that its guess was "not the canonical one" — an evidence format nobody could
/// produce, which is why every refutation in the tree was a hand-built skeleton.
///
/// This closes the round trip: run the engine, tile the capture, pick a coordinate, and get the
/// object the court takes. What the court then says about it is the court's business — an honest
/// capture is `NoFaultFound`, a tampered one is a conviction — and that is exactly the property
/// worth having, because the same function produces both.
///
/// `binding` is the producer's own commitment; its `step_leaf_count` and `step_merkle_root` must be
/// the ones `tiles` produced or the openings will not verify, which is the check doing its job
/// rather than a caller's obligation.
/// **The anchor a refutation of `disputed_call` should carry**, assembled from the producer's own
/// committed leg.
///
/// The opening is over the checkpoint leaf hashes with the leg's own tree, so what comes back
/// proves against `binding.checkpoint_merkle_root` and nothing else. `None` when the call has no
/// anchor — the prefill call, or a leg with no checkpoint covering `disputed_call − 1`.
pub fn base0_kv_anchor_for_call_v1(
    checkpoints: &Base0CheckpointsV1,
    disputed_call: u32,
) -> Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1> {
    let want = disputed_call.checked_sub(1).filter(|_| disputed_call > 0)?;
    let at = checkpoints.leaves.iter().position(|l| l.covered_decode_call == want)?;
    let opening = kaspa_consensus_core::palw_step_leg::step_opening_v1(&checkpoints.leaf_hashes, at as u64).ok()?;
    Some(kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1 {
        leaf: checkpoints.leaves[at].clone(),
        opening,
        chunks: checkpoints.chunks.get(at)?.clone(),
    })
}

/// **Open a bisection ladder already narrowed to a committed checkpoint** — the bisect half's
/// caller.
///
/// [`kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open_anchored`] takes an index and a
/// state and cannot check where they came from; this derives both from the producer's OWN
/// committed leg, so the ladder is seeded with something the responder is already bound to rather
/// than with a number a challenger picked.
///
/// The index is the first step leaf of the call AFTER the anchor's coverage: everything below it
/// is execution the checkpoint already commits to, so no divergence the dispute is about can live
/// there. The state is that checkpoint's own leaf hash.
///
/// `None` when the leg has no checkpoint at `covered`, or when the remaining interval is too small
/// to bisect — both of which are answers, not faults.
pub fn base0_anchored_ladder_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    checkpoints: &Base0CheckpointsV1,
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    covered_decode_call: u32,
    challenger_id: &Hash64,
    responder_id: &Hash64,
    opened_at_daa: u64,
    first_deadline_daa: u64,
) -> Option<kaspa_consensus_core::palw_bisect::PalwBisectLadderV1> {
    use kaspa_consensus_core::palw_bisect::{PalwBisectLadderV1, PalwBisectSpaceV1};
    let at = checkpoints.leaves.iter().position(|l| l.covered_decode_call == covered_decode_call)?;
    // The first leaf of the first call the anchor does NOT cover. Found by walking the space's own
    // enumeration rather than by arithmetic on it: the bijection is `canonical_step_coordinates`'
    // to define, and a second derivation here would be a second answer for a court to disagree
    // with.
    let anchor_index = (0..binding.step_leaf_count).find(|i| {
        kaspa_consensus_core::palw_step::canonical_step_coordinates(profile, ctx, *i)
            .is_some_and(|c| c.call_index > covered_decode_call)
    })?;
    PalwBisectLadderV1::open_anchored(
        &ctx.context_hash(),
        &binding.committed_execution_root,
        challenger_id,
        responder_id,
        PalwBisectSpaceV1::StepLeaves,
        binding.step_leaf_count,
        anchor_index,
        checkpoints.leaf_hashes[at],
        opened_at_daa,
        first_deadline_daa,
    )
    .ok()
}

pub fn base0_refutation_from_capture_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    tiles: &Base0StepTilesV1,
    binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    prompt_token_ids: Vec<u32>,
    decode_tokens: Option<kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1>,
    // A verified checkpoint anchor for the KV history, when the caller holds one. `None` builds
    // the long form: one opening per cached position.
    kv_checkpoint: Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>,
) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, LegError> {
    use kaspa_consensus_core::palw_step_leg::step_opening_v1;
    use kaspa_consensus_core::palw_step_refute::{
        PalwExecutionStepRefutationV1, PalwStepInputOpeningV1, canonical_input_leaves_v1_anchored,
    };

    let leaf_of =
        |index: u64| -> Option<PalwStepTileLeafV1> { tiles.tiles.iter().find(|(i, _)| *i == index).map(|(_, leaf)| leaf.clone()) };
    let target_index = canonical_step_leaf_index(profile, ctx, &target).ok_or(LegError::NotACanonicalCoordinate {
        layer: 0,
        slot: target.node_slot as u16,
        tile: target.tile_index,
    })?;
    let output_preimage = leaf_of(target_index).ok_or(LegError::UnknownSlot { layer: 0, slot: target.node_slot as u16 })?;
    let output_opening = step_opening_v1(&tiles.leaves, target_index).map_err(|_| LegError::NotACanonicalCoordinate {
        layer: 0,
        slot: target.node_slot as u16,
        tile: target.tile_index,
    })?;

    // The canonical input set, in the checker's own order — asked for rather than reconstructed,
    // so a prover cannot disagree with the court about what a step reads.
    let required = canonical_input_leaves_v1_anchored(profile, ctx, &target, kv_checkpoint.is_some())
        .ok_or(LegError::UnknownSlot { layer: 0, slot: target.node_slot as u16 })?;
    let mut inputs = Vec::new();
    for row in &required {
        for (index, coord) in row {
            let preimage = leaf_of(*index).ok_or(LegError::UnknownSlot { layer: 0, slot: coord.node_slot as u16 })?;
            let opening = step_opening_v1(&tiles.leaves, *index).map_err(|_| LegError::NotACanonicalCoordinate {
                layer: 0,
                slot: coord.node_slot as u16,
                tile: coord.tile_index,
            })?;
            inputs.push(PalwStepInputOpeningV1 { opening, preimage });
        }
    }
    Ok(PalwExecutionStepRefutationV1 {
        binding,
        output_opening,
        output_preimage,
        inputs,
        prompt_token_ids,
        decode_tokens,
        kv_checkpoint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use kaspa_consensus_core::palw_base0_profile::{PalwBase0GeometryV1, base0_profile_v1};
    use kaspa_consensus_core::palw_step::step_leaf_count;
    use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, trace_scheme_id_v2};

    fn geometry() -> PalwBase0GeometryV1 {
        PalwBase0GeometryV1 {
            layer_count: 2,
            hidden_dim: 32,
            ffn_dim: 64,
            attn_heads: 2,
            attn_head_dim: 16,
            vocab_size: 64,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: 1 << 8,
            tile_len: 16,
        }
    }

    fn artifact() -> Base0ArtifactV1 {
        let g = geometry();
        Base0ArtifactV1::derive_deterministic(
            Base0ShapeV1 {
                n_layers: g.layer_count as usize,
                n_heads: g.attn_heads as usize,
                n_kv_heads: g.attn_heads as usize,
                d_head: g.attn_head_dim as usize,
                d_ff: g.ffn_dim as usize,
                vocab: g.vocab_size as usize,
                max_position: g.n_ctx as usize,
                ln_theta_gen_q: LN_THETA_10000_GEN_Q,
                eps_q: g.rms_eps_q,
            },
            0x1EA5,
        )
        .expect("the fixture shape is valid")
    }

    fn context(profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3) -> kaspa_consensus_core::palw_v2::PalwJobContextV2 {
        let mut ctx = kaspa_consensus_core::palw_v2::PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-palw-rc".to_vec(),
            job_id: Hash64::default(),
            job_nullifier: Hash64::default(),
            assignment_id: Hash64::default(),
            execution_seed: [0; 32],
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: 2,
            exact_decode_tokens: 1,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = trace_scheme_id_v2();
        ctx
    }

    /// **A real execution produces step-leg tiles, which nothing did before** (audit C-01).
    ///
    /// The audit's finding was that no path existed from an execution to `execution_root`: the
    /// worker captured taps and logits, not per-kernel tile outputs, so the root was whatever the
    /// miner wrote and every leg in the tree was synthesised by a test. This runs the integer
    /// engine, takes the rows it actually produced, and places them at the coordinates the PROFILE
    /// says they belong to — `canonical_step_leaf_index` decides that, so a capture cannot disagree
    /// with the class about where a tile goes.
    #[test]
    fn an_execution_produces_tiles_at_the_profiles_own_coordinates() {
        let a = artifact();
        let profile = base0_profile_v1(geometry()).expect("expressible");
        let ctx = context(&profile);
        let leaf_count = step_leaf_count(&profile, &ctx).expect("the job has a step space");

        let engine = crate::engine::Base0Engine::new(&a);
        let mut cache = crate::engine::KvCache::new(&a);
        let (_, probe) = engine.forward_token_probed(&mut cache, 3, 0).expect("the pass completes");
        assert!(!probe.steps.is_empty(), "the engine records the rows it produced");
        // **Every step, not a subset.** The probe recorded ten of the thirty-six, so a leg could
        // commit only the rows the capture happened to keep and `execution_root` was, for the
        // other twenty-six, whatever the miner wrote — the finding this module exists to close.
        assert_eq!(
            probe.steps.len(),
            kaspa_consensus_core::palw_base0_profile::BASE0_LAYER_IR.len() * geometry().layer_count as usize,
            "one captured row per IR step per layer"
        );

        let tiles = base0_step_tiles_v1(&profile, &ctx, leaf_count, 0, 0, &probe.steps).expect("the rows tile");
        assert!(!tiles.tiles.is_empty(), "and land somewhere in the step space");
        let root = base0_step_merkle_root_v1(&tiles).expect("a populated space has a root");
        assert_ne!(root, Hash64::default());

        // Deterministic: the same execution commits the same leg, which is what a court comparing a
        // producer's root against a challenger's evidence relies on.
        let mut cache2 = crate::engine::KvCache::new(&a);
        let (_, probe2) = engine.forward_token_probed(&mut cache2, 3, 0).unwrap();
        let again = base0_step_tiles_v1(&profile, &ctx, leaf_count, 0, 0, &probe2.steps).unwrap();
        assert_eq!(base0_step_merkle_root_v1(&again), Some(root), "one execution, one leg");

        // A tile whose values changed changes the root — the leaf binds the bytes, so a producer
        // cannot commit one row and execute another.
        let mut tampered = probe.steps.clone();
        tampered[0].2[0] = tampered[0].2[0].wrapping_add(1);
        let other = base0_step_tiles_v1(&profile, &ctx, leaf_count, 0, 0, &tampered).unwrap();
        assert_ne!(base0_step_merkle_root_v1(&other), Some(root), "the leg binds what ran");

        // And every tile sits where the profile says, not where the capture wished.
        for (index, leaf) in &tiles.tiles {
            let expected = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&profile, &ctx, &leaf.coord)
                .expect("the coordinate is canonical");
            assert_eq!(*index, expected, "a capture may not choose a leaf index");
        }
    }

    /// **Audit C-05: the engine and the declared graph are checked against each other, step by
    /// step.**
    ///
    /// `base0_profile_v1` is generated from `BASE0_LAYER_IR` and the engine is still written by
    /// hand, so nothing structural stops the two describing different computations — which they
    /// did, four times over, and each divergence was found by someone reading rather than by
    /// anything failing. A generator would make it impossible; short of one, this makes it
    /// FAIL LOUDLY, which is the property that was missing.
    ///
    /// What it compares is the whole observable shape of an execution: the slot sequence, in
    /// order, and each row's length against the width the profile declares for that node at this
    /// position's `kv_len`. A step the engine performs and the graph omits shows up as a slot
    /// nobody declared; a step the graph declares and the engine skips shows up as a missing slot;
    /// and a step whose width disagrees — the `KvDim`-vs-`Hidden` attention output, the per-head
    /// nodes declared once per layer — shows up as a length.
    #[test]
    fn the_engine_performs_exactly_the_graph_the_profile_declares() {
        use kaspa_consensus_core::palw_step::PalwStepOutLenV1;

        let a = artifact();
        let profile = base0_profile_v1(geometry()).expect("expressible");
        let engine = crate::engine::Base0Engine::new(&a);
        let mut cache = crate::engine::KvCache::new(&a);

        // Two positions, because every `KvScaled` width is a function of `kv_len` and a single
        // position cannot tell a per-head width from a per-layer one (both are `kv_len` at one).
        for position in 0..2u32 {
            let (_, probe) = engine.forward_token_probed(&mut cache, 3, position as usize).expect("the pass completes");
            let kv_len = u64::from(position) + 1;
            for layer in 0..profile.layer_count {
                let rows: Vec<_> = probe.steps.iter().filter(|(l, _, _)| *l == layer).collect();
                let slots: Vec<u16> = rows.iter().map(|(_, slot, _)| *slot).collect();
                let declared: Vec<u16> = (0..profile.attn_nodes.len() as u16).collect();
                assert_eq!(slots, declared, "layer {layer}: the engine's step ORDER is the graph's");
                for (_, slot, row) in rows {
                    let node = &profile.attn_nodes[*slot as usize];
                    let want = match node.out_len {
                        PalwStepOutLenV1::Fixed { elements } => u64::from(elements),
                        PalwStepOutLenV1::KvScaled { multiplier } => u64::from(multiplier) * kv_len,
                    };
                    assert_eq!(
                        row.len() as u64,
                        want,
                        "layer {layer} slot {slot} ({:?}) at kv_len {kv_len}: the engine produced {} values, the graph declares {want}",
                        node.op_kind,
                        row.len()
                    );
                }
            }
        }
    }

    /// **Audit C-01, closed end to end: a real execution becomes a refutation the court reads.**
    ///
    /// The audit's largest finding was that nothing turned a run into the commitment a court opens
    /// against. The capture half landed first; this is the other half, and it is the one that
    /// proves the format is producible: `check_execution_step_refutation_v1` computes the canonical
    /// input set privately and refuses any other, so until `canonical_input_leaves_v1` existed a
    /// producer had to guess the rule and would learn only that its guess was wrong.
    ///
    /// Both verdicts come out of the same assembly, which is what makes this a round trip rather
    /// than a demonstration: an HONEST capture is `NoFaultFound` — the challenger loses on the
    /// merits — and one tampered tile is a conviction.
    #[test]
    fn a_capture_becomes_a_refutation_the_court_adjudicates_both_ways() {
        use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, check_execution_step_refutation_v1};

        let a = artifact();
        let profile = base0_profile_v1(geometry()).expect("expressible");
        let ctx = context(&profile);
        let leaf_count = step_leaf_count(&profile, &ctx).expect("the job has a step space");

        let engine = crate::engine::Base0Engine::new(&a);
        let mut cache = crate::engine::KvCache::new(&a);
        let (_, probe) = engine.forward_token_probed(&mut cache, 3, 0).expect("the pass completes");
        let tiles = base0_step_tiles_v1(&profile, &ctx, leaf_count, 0, 0, &probe.steps).expect("the rows tile");
        let _root = base0_step_merkle_root_v1(&tiles).expect("a populated space has a root");

        // decode = 1 ⇒ zero decode CALLS ⇒ zero checkpoints, and the empty leg is the honest one.
        let no_checkpoints = Base0CheckpointCaptureV1::from_chunks_v1(
            &ctx,
            &profile,
            &kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(1),
            &[],
        )
        .expect("a job with no decode call has an empty checkpoint leg");
        let binding = |tiles: &Base0StepTilesV1| {
            base0_binding_from_capture_v1(&profile, &ctx, tiles, &no_checkpoints, Hash64::default(), Hash64::default())
                .expect("a capture yields its own commitment")
        };

        // A step with real inputs and a real weight operand: the FFN down projection's narrowing
        // (slot 34 of layer 0), which reads the accumulator the step before it produced.
        let target =
            PalwStepCoordinateV1 { call_index: 0, node_slot: profile.pre_nodes.len() as u32 + 34, position: 0, tile_index: 0 };
        let honest = base0_refutation_from_capture_v1(&profile, &ctx, &tiles, binding(&tiles), target, Vec::new(), None, None)
            .expect("a capture assembles");

        // The oracle is the PRODUCTION inventory, proven against its own root — so this exercises
        // the artifact path and the step path in one call, with no synthetic leaf anywhere.
        let inventory = crate::inventory::base0_inventory_v1(&a, geometry()).expect("a real inventory");
        let artifact_root = inventory.root();
        let openings: Vec<_> = (0..inventory.operands().len())
            .filter(|i| {
                let o = &inventory.operands()[*i];
                o.tensor_name == "blk.{layer}.ffn_down.requant" && o.layer == Some(0)
            })
            .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
            .collect();
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, artifact_root)
            .expect("the narrowing's row proves against the artifact root");

        // Honest: the challenger loses on the merits, which is a VERDICT and not an error.
        assert!(
            matches!(check_execution_step_refutation_v1(&honest, &oracle), Err(PalwStepRefuteError::NoFaultFound)),
            "an honest execution refutes nothing: {:?}",
            check_execution_step_refutation_v1(&honest, &oracle)
        );

        // Tampered: one value of the challenged tile changed, re-tiled, re-rooted — the producer
        // committed a row its own inputs do not produce, and the court says so.
        let mut lying = probe.steps.clone();
        let (_, _, row) = lying.iter_mut().find(|(l, slot, _)| *l == 0 && *slot == 34).expect("the step is captured");
        row[0] = row[0].wrapping_add(1);
        let lying_tiles = base0_step_tiles_v1(&profile, &ctx, leaf_count, 0, 0, &lying).expect("the rows tile");
        let _lying_root = base0_step_merkle_root_v1(&lying_tiles).expect("rooted");
        let fraud =
            base0_refutation_from_capture_v1(&profile, &ctx, &lying_tiles, binding(&lying_tiles), target, Vec::new(), None, None)
                .expect("a tampered capture assembles the same way");
        let verdict =
            check_execution_step_refutation_v1(&fraud, &oracle).expect("a committed row its own inputs do not produce convicts");
        // The conviction is ARITHMETIC, not structural: the tampered leaf was re-hashed and
        // re-rooted, so it is internally consistent — the only thing wrong with it is that
        // recomputing the step from its own opened inputs does not produce it.
        assert!(
            matches!(verdict.fault, kaspa_consensus_core::palw_step_leg::PalwStepFaultV1::ComputationMismatch { value_index: 0 }),
            "the fault must be the recomputation's, at the value that was changed — got {:?}",
            verdict.fault
        );
    }
}
