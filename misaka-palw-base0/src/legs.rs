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
    /// The FOLD refused (ADR-0082 Decision 7). Carried rather than flattened to a string: the
    /// sparse capture's refusals name the call, the slot and the width they were given, and a
    /// producer that has just lost a job needs to read which one it was.
    Fold(crate::fp_capture::Base0SparseCaptureError),
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

/// **The same rows, from the A16 engine's trace.**
///
/// `A16Engine::forward_token_traced` records `pre` and `post` as step SEQUENCES at layer 0 and
/// `attn` as one list per layer, which is the coordinate system [`Base0CapturedRowV1`] carries —
/// so this is a flattening of the TRACE's own numbering.
///
/// **On today's A16 class it is ALMOST the profile's numbering, and the exception is the point.**
/// Measured: the per-layer and post tables agree exactly, and the pre table does not — the engine
/// records the embedding gather and the requant that lifts it onto the A16 stream, while the
/// profile declares only the gather. A requant is a narrowing, and ADR-0049 Decision F requires a
/// class to name every narrowing its engine performs; `A16Engine` has no `plan()` and there is no
/// counterpart to `base0_check_graph_v1` to enforce it.
///
/// So `Qwen25A16Backend::execute_free_prompt` checks the correspondence and refuses rather than
/// dropping a row it cannot prove is undeclared-on-purpose. This flattening is the right shape for
/// the class that declares that node; it is not a capture path for the one that does not. Written as its own function rather than a generic over the two probe types
/// because the two engines' traces are different structs for good reasons, and a trait to unify
/// them would be three lines of abstraction over eleven lines of loop.
///
/// This is the first half of giving the A16 family adjudicable executions. The other two are the
/// checkpoint leg (its cache is not `engine::KvCache`, so the chunks go in through
/// `Base0CheckpointCaptureV1::push_chunks` against `next_geometry`) and the court's rung methods
/// (`disclose` and `bisect_prefix_state`), without which `supports_court` must stay false: a
/// family that cannot answer at a rung loses every dispute whichever party is honest.
pub fn a16_captured_rows_v1(trace: &crate::engine_a16::A16TraceV1) -> Vec<Base0CapturedRowV1> {
    let mut rows = Vec::with_capacity(trace.pre.len() + trace.post.len() + trace.attn.iter().map(Vec::len).sum::<usize>());
    for (index, row) in trace.pre.iter().enumerate() {
        rows.push(Base0CapturedRowV1 { table: PalwStepTableV1::Pre, layer: 0, index, row: row.clone() });
    }
    for (layer, nodes) in trace.attn.iter().enumerate() {
        for (index, row) in nodes.iter().enumerate() {
            rows.push(Base0CapturedRowV1 { table: PalwStepTableV1::Attn, layer: layer as u16, index, row: row.clone() });
        }
    }
    for (index, row) in trace.post.iter().enumerate() {
        rows.push(Base0CapturedRowV1 { table: PalwStepTableV1::Post, layer: 0, index, row: row.clone() });
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

/// **What a run's leaves are folded into: the dense tiles, or the fold** (ADR-0082 Decision 7).
///
/// One capture LOOP per family, one enumeration, two sinks. The dense arm holds every tile and
/// every leaf hash — ~50 MB a position on the dense tier — because the attempt lane's court
/// assembly reads tiles back out of it (`base0_refutation_from_capture_capped_v1`); the sparse arm
/// holds one node per `2^retain_level` leaves and nothing else, and an opening asked for later is
/// re-derived by replay (`fp_interval`). A family chooses the sink at the top of its loop and the
/// rest of the loop cannot tell the difference, which is the property that makes the roots equal:
/// the same rows, in the same order, through the same leaf hash.
pub enum Base0CaptureSinkV1 {
    Dense(Base0StepCaptureV1),
    Sparse(crate::fp_capture::Base0SparseStepCaptureV1),
}

/// What a sealed capture hands its caller: the tree (always), the tiles (dense only), and the two
/// numbers the binding is built from.
pub struct Base0CaptureOutcomeV1 {
    pub tiles: Option<Base0StepTilesV1>,
    pub tree: crate::fp_capture::Base0SparseStepTreeV1,
    pub step_leaf_count: u64,
    pub step_merkle_root: Hash64,
}

/// Which sink a family's capture loop runs. The free-prompt lane folds (ADR-0082 Decision 7); the
/// attempt lane keeps its tiles, because the court's assembly reads them back out of the retained
/// material and that path is ADR-0082 U-03's, not this unit's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0CaptureKindV1 {
    DenseTiles,
    Fold,
}

impl Base0CaptureOutcomeV1 {
    /// The two fields a `Base0ExecutionV1` carries: the tiles (empty when the run folded) and the
    /// tree (`Some` exactly then).
    pub fn into_execution_parts(self) -> (Base0StepTilesV1, Option<crate::fp_capture::Base0SparseStepTreeV1>) {
        match self.tiles {
            Some(tiles) => (tiles, None),
            None => (Base0StepTilesV1 { leaves: Vec::new(), tiles: Vec::new() }, Some(self.tree)),
        }
    }
}

impl Base0CaptureSinkV1 {
    /// The sink a run's lane asks for.
    pub fn for_kind(
        kind: Base0CaptureKindV1,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        leaf_count: u64,
        max_step_leaf_count: u64,
    ) -> Result<Self, LegError> {
        match kind {
            Base0CaptureKindV1::DenseTiles => Self::dense(leaf_count),
            Base0CaptureKindV1::Fold => Self::sparse(profile, ctx, leaf_count, max_step_leaf_count),
        }
    }

    /// The dense sink: every tile kept.
    pub fn dense(leaf_count: u64) -> Result<Self, LegError> {
        Ok(Self::Dense(Base0StepCaptureV1::new(leaf_count)?))
    }

    /// **The fold**, at the level the ruleset's ladder derives
    /// (`crate::fp_capture::palw_base0_sparse_retain_level_v1`).
    pub fn sparse(
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        leaf_count: u64,
        max_step_leaf_count: u64,
    ) -> Result<Self, LegError> {
        let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(max_step_leaf_count);
        Ok(Self::Sparse(
            crate::fp_capture::Base0SparseStepCaptureV1::new_capped_v1(profile, ctx, leaf_count, level, max_step_leaf_count)
                .map_err(LegError::Fold)?,
        ))
    }

    /// One call's rows. The signature is the dense capture's, so a loop that switches sinks
    /// switches one line.
    pub fn push_call(
        &mut self,
        profile: &PalwShapeProfileV3,
        ctx: &PalwJobContextV2,
        call_index: u32,
        position: u32,
        rows: &[Base0CapturedRowV1],
    ) -> Result<(), LegError> {
        match self {
            Self::Dense(capture) => capture.push_call(profile, ctx, call_index, position, rows),
            Self::Sparse(capture) => capture.push_call(profile, call_index, position, rows).map_err(LegError::Fold),
        }
    }

    pub fn progress(&self) -> (u64, u64) {
        match self {
            Self::Dense(capture) => capture.progress(),
            Self::Sparse(capture) => capture.progress(),
        }
    }

    /// Seal it. Both arms answer with the same tree, because the dense arm folds its own leaves
    /// through the same accumulator — so `step_merkle_root` has ONE derivation whichever sink ran,
    /// and "the roots are equal through both routes" is a property of this function rather than a
    /// coincidence two code paths arrive at.
    pub fn finish(self, max_step_leaf_count: u64) -> Result<Base0CaptureOutcomeV1, LegError> {
        match self {
            Self::Dense(capture) => {
                let tiles = capture.finish()?;
                let level = crate::fp_capture::palw_base0_sparse_retain_level_v1(max_step_leaf_count);
                let tree = crate::fp_capture::Base0SparseStepTreeV1::from_leaves_capped_v1(&tiles.leaves, level, max_step_leaf_count)
                    .map_err(LegError::Fold)?;
                let step_merkle_root = tree.root().map_err(LegError::Fold)?;
                debug_assert_eq!(
                    Some(step_merkle_root),
                    kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&tiles.leaves, max_step_leaf_count).ok(),
                    "the fold and the whole-vector root are one rule"
                );
                Ok(Base0CaptureOutcomeV1 { step_leaf_count: tiles.leaves.len() as u64, step_merkle_root, tree, tiles: Some(tiles) })
            }
            Self::Sparse(capture) => {
                let tree = capture.finish().map_err(LegError::Fold)?;
                let step_merkle_root = tree.root().map_err(LegError::Fold)?;
                Ok(Base0CaptureOutcomeV1 { step_leaf_count: tree.leaf_count(), step_merkle_root, tree, tiles: None })
            }
        }
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

/// The step leg's Merkle root over `leaves`, against the DEFAULT ladder top.
pub fn base0_step_merkle_root_v1(tiles: &Base0StepTilesV1) -> Option<Hash64> {
    base0_step_merkle_root_capped_v1(tiles, kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES)
}

/// The step leg's Merkle root against the ladder top the RULESET froze
/// (`PalwCourtParamsV2::max_step_leaf_count`). The leg's own cap is a default, not the rule, and a
/// class whose step space is wider than the default cannot commit against it — see
/// `kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1`.
pub fn base0_step_merkle_root_capped_v1(tiles: &Base0StepTilesV1, step_ladder_cap: u64) -> Option<Hash64> {
    kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&tiles.leaves, step_ladder_cap).ok()
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
    /// answer a checkpoint challenge and would lose its bond by default. EMPTY under the
    /// per-position cadence, whose retention is the leaves (`Base0CheckpointRetentionV1::Fold`).
    pub chunks: Vec<Vec<Vec<u8>>>,
    /// **Payload bytes this capture actually serialised**, over every push — the H-3 measurement,
    /// carried onto the finished leg so a test can read it without holding the capture. Not a
    /// consensus quantity and not in any hash.
    pub bytes_serialised: u64,
}

/// **The state layout the CLASS declares, at `positions`** — the one dispatch on
/// `profile.state_chunk_map_id`, for both directions.
///
/// The capture side and the replay side must chunk a state the same way or the producer commits
/// bytes the verifier cannot read back. They did not: `Base0CheckpointCaptureV1::next_geometry`
/// dispatched on the declared map id while `base0_replay_from_checkpoint_v1` called
/// `integer_kv_state_geometry_v1` unconditionally, so a class registering the FOUR-byte map (the
/// one an `i32` cache actually fits) would have its checkpoints taken at four bytes per element
/// and restored at one — an honest producer whose own anchored replay refuses its own state. Every
/// class in the tree declares v1 today, which is why nothing caught it; ADR-0077 Decision 10 is
/// what makes a second map real.
///
/// A map this family does not implement is an error rather than a fallback: falling back to v1
/// would chunk an unknown class's state at a width nobody chose.
pub fn base0_state_chunk_geometry_v1(
    profile: &PalwShapeProfileV3,
    positions: u32,
) -> Result<kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1, LegError> {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    let declared = profile.state_chunk_map_id;
    // **A hybrid's map names both halves, and this is the CACHE half of it.** The composition is
    // `attn=<integer-kv v2>/gdn=<recurrence>`, so a class that registers either hybrid id has
    // declared the four-byte cache layout for its attention layers — chunking it at v1's one byte
    // per element is the same quarter-width defect the v2 dispatch above exists to close, and
    // refusing it outright would leave a Qwen3.6-shaped class unable to take a KV checkpoint at
    // all. The recurrence half of the same name is `fp_capture`'s
    // (`base0_gdn_state_geometry_v*`); this function answers only about the cache.
    let geometry = if declared == map::integer_kv_state_chunk_map_id_v1() {
        map::integer_kv_state_geometry_v1(profile, positions)
    } else if declared == map::integer_kv_state_chunk_map_id_v2()
        || declared == map::hybrid_state_chunk_map_id_v1()
        || declared == map::hybrid_state_chunk_map_id_v2()
    {
        map::integer_kv_state_geometry_v2(profile, positions)
    } else if declared == map::tiled_kv_state_chunk_map_id_v3() || declared == map::hybrid_state_chunk_map_id_v3() {
        // **Graph v4's tiled enumeration.** The same four-byte cache as v2, chunked at
        // `PALW_ATTN_HISTORY_TILE_V4` positions instead of at the transport leg's 1 MiB cap — the
        // capture side of the same rule the court reads in `verify_kv_anchor`. Mirrored here and
        // not re-derived: a producer that chunked a v3 class at v2's width would commit chunks
        // whose count is not the map's, and its own anchored replay would refuse its own state.
        map::tiled_kv_state_geometry_v3(profile, positions)
    } else {
        return Err(LegError::CheckpointStateUnavailable { chunk_index: 0 });
    };
    geometry.map_err(LegError::CheckpointStateMap)
}

/// **The cache layout the checkpoint carrying `covered` was chunked at** (audit B, C-2).
///
/// The one spelling of "un-chunk what that checkpoint chunked", which four replay sites were each
/// writing as `integer_kv_state_geometry_*(profile, integer_kv_positions_at_v1(ctx, covered))` —
/// the PER-CALL conversion. Under the per-position cadence `covered` already IS the row count, so
/// that spelling asks for `prefill + covered` rows, `from_state_chunks_v1` is handed chunks for a
/// shorter state, and an honest seat's replay refuses an honest producer.
///
/// Chunking at capture and un-chunking at replay are one decision; deriving it here is what keeps
/// them one.
pub fn base0_checkpoint_geometry_at_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    covered: u32,
) -> Result<kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1, LegError> {
    let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, ctx, covered);
    base0_state_chunk_geometry_v1(profile, positions)
}

/// **The layout ONE checkpoint of this class is taken under** — the attention cache alone, or the
/// whole composition a hybrid's map names (ADR-0082 Decision 4; audit B, H-1).
///
/// [`base0_state_chunk_geometry_v1`] answers about the CACHE half and is what the seat's A16
/// kernels and the court's row reads take. A hybrid's registered map is not the cache half: its
/// name is `palw-hybrid-state/attn=…/gdn=…/v3`, and a producer that enumerated only the `attn=`
/// part would commit `attn.chunk_count()` chunks under a root the seat rebuilds over
/// `attn + gdn` — a `CheckpointRootMismatch` against an honest producer, and a recurrence
/// committed by nobody and adjudicable by nobody. One enumeration, here, for both directions.
pub enum Base0CaptureGeometryV1 {
    /// A class whose checkpoint is the attention cache (or the recurrence's own standalone map).
    Flat(kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1),
    /// A class whose checkpoint is the composition, in the order its map's NAME spells.
    Hybrid(kaspa_consensus_core::palw_state_chunk_map::PalwHybridStateGeometryV1),
}

impl Base0CaptureGeometryV1 {
    /// Chunks this checkpoint has, under the class's own map.
    pub fn chunk_count(&self) -> u64 {
        match self {
            Self::Flat(g) => g.chunk_count(),
            Self::Hybrid(g) => g.chunk_count(),
        }
    }

    /// The attention half's geometry — the same object either way, so a caller that only reads
    /// cache rows (`integer_kv_state_locate_v1`) does not have to know which shape it has.
    pub fn attn(&self) -> &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1 {
        match self {
            Self::Flat(g) => g,
            Self::Hybrid(g) => &g.attn,
        }
    }

    /// The entry at `index`, or `None` past the end — [`kaspa_consensus_core::palw_state_chunk_map::hybrid_state_chunk_entry_v3`]
    /// for the composition and the flat enumeration wrapped in its attention arm otherwise, so one
    /// walk serves both.
    pub fn entry(&self, index: u64) -> Option<kaspa_consensus_core::palw_state_chunk_map::PalwHybridChunkEntryV1> {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        match self {
            Self::Flat(g) => map::integer_kv_state_chunk_entry_v1(g, index).map(map::PalwHybridChunkEntryV1::AttentionCache),
            Self::Hybrid(g) => map::hybrid_state_chunk_entry_v3(g, index),
        }
    }
}

/// **The layout of one checkpoint at `positions`, composition included** — the dispatch every
/// capture takes, and the twin of [`base0_state_chunk_geometry_v1`] for the classes whose map
/// names two halves.
///
/// The recurrence section is not a caller's choice: `hybrid_state_geometry_for_covered_v1` asks
/// `palw_checkpoint_leaf_carries_recurrence_v1` whether THIS checkpoint carries it, so a
/// per-position leg commits the attention tiles at every position and the recurrence at its
/// derived spacing — the same answer the seat's `Qwen36RecomputeKernelsV1::state_chunks` gets from
/// the same function.
pub fn base0_capture_geometry_v1(profile: &PalwShapeProfileV3, positions: u32) -> Result<Base0CaptureGeometryV1, LegError> {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    if profile.state_chunk_map_id == map::hybrid_state_chunk_map_id_v3() {
        return map::hybrid_state_geometry_for_covered_v1(profile, positions)
            .map(Base0CaptureGeometryV1::Hybrid)
            .map_err(LegError::CheckpointStateMap);
    }
    base0_state_chunk_geometry_v1(profile, positions).map(Base0CaptureGeometryV1::Flat)
}

/// **The one walk of a class's checkpoint layout** — producer and seat (audit B, H-1).
///
/// `attn` serializes one attention-cache entry out of whatever cache the caller holds;
/// `gdn_chunks` are the recurrence's chunks in its own map's order, consumed in the order the
/// composition enumerates them. The enumeration is
/// [`Base0CaptureGeometryV1::entry`] — `hybrid_state_chunk_entry_v3` for a composed map and the
/// flat one otherwise — so the executor's chunk `i` and the seat's chunk `i` are the same bytes by
/// construction. Before this there were three spellings: the producer and the court enumerated the
/// attention half alone and the seat enumerated attention plus recurrence, which is a
/// `CheckpointRootMismatch` against an honest producer at every checkpoint that carries the
/// recurrence.
pub fn base0_composed_state_chunks_v1<F>(
    geometry: &Base0CaptureGeometryV1,
    mut attn: F,
    gdn_chunks: &[Vec<u8>],
) -> Result<Vec<Vec<u8>>, LegError>
where
    F: FnMut(&kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>>,
{
    use kaspa_consensus_core::palw_state_chunk_map::PalwHybridChunkEntryV1;
    let attn_count = geometry.attn().chunk_count();
    let mut out = Vec::with_capacity(geometry.chunk_count() as usize);
    for index in 0..geometry.chunk_count() {
        let entry = geometry.entry(index).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?;
        let bytes = match entry {
            PalwHybridChunkEntryV1::AttentionCache(entry) => {
                attn(&entry).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?
            }
            PalwHybridChunkEntryV1::RecurrenceState { byte_len, .. } => {
                let bytes: Vec<u8> = gdn_chunks
                    .get((index - attn_count) as usize)
                    .cloned()
                    .ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?;
                // The composition's own length for the slice, checked where the bytes are picked
                // up rather than two layers later: a recurrence chunk of another width is a leaf
                // nothing can open at the index the map named.
                if bytes.len() as u64 != byte_len {
                    return Err(LegError::CheckpointStateUnavailable { chunk_index: index });
                }
                bytes
            }
        };
        out.push(bytes);
    }
    Ok(out)
}

/// **What a finished capture holds on to** (ADR-0082 Decision 4, amended).
///
/// Not a caller's choice — [`Base0CheckpointCaptureV1::new`] derives it from the class's cadence,
/// for the reason every other number in this tree is derived: a producer that picked the cheap one
/// for a class the court will ask chunks of has thrown away evidence it signed for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base0CheckpointRetentionV1 {
    /// Every checkpoint's chunk bytes, in map order. What the per-call cadence keeps: the
    /// checkpoints are `decode_calls / interval` snapshots of a state that is not prefix-stable in
    /// general (a hybrid's recurrence is not), so the bytes are the only copy.
    Chunks,
    /// The leaves and their hashes, and NOT one byte of state.
    ///
    /// The per-position cadence's retention, and the reason it is affordable at all. Retaining
    /// chunks per checkpoint would be QUADRATIC — checkpoint `i` holds `i + 1` positions, so the
    /// sum over a job is `Θ(n²)` rows, 13.5 GB on a 4,096-position dense job. The attention cache
    /// is prefix-stable, so the executor keeps the cache ONCE (it is holding it anyway, to run) and
    /// re-derives any checkpoint's chunks from it with
    /// [`base0_checkpoint_chunks_at_v1`]: `A16Cache::state_chunk_bytes_v1` answers an earlier
    /// entry out of a later cache byte-identically, because it reads
    /// `layer[position_start .. position_start + position_count]` and those rows never move.
    Fold,
}

/// Accumulates checkpoints in call order, chaining each to the one before it.
pub struct Base0CheckpointCaptureV1 {
    ctx: PalwJobContextV2,
    profile: PalwShapeProfileV3,
    ctx_hash: Hash64,
    checkpoint_profile_hash: Hash64,
    state_chunk_map_id: Hash64,
    interval: u32,
    retention: Base0CheckpointRetentionV1,
    prev: Hash64,
    leaves: Vec<kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2>,
    leaf_hashes: Vec<Hash64>,
    chunks: Vec<Vec<Vec<u8>>>,
    /// **The previous checkpoint's chunk hashes, and the attention geometry they were taken at**
    /// (ADR-0082 Decision 4, amended; audit B, H-3).
    ///
    /// The fold's economy was a RETENTION economy only: each push re-serialised and re-hashed the
    /// WHOLE cache, so a per-position leg cost `Σ 2·layers·(p+1)·row` bytes — 7.5 GB on a
    /// 512-position dense row, 1.9 TB at the ladder's top — on the block-producing path. The
    /// attention cache is prefix-stable, so a COMPLETE tile's chunk hash is the value it already
    /// had; only the ragged last tile of each `(kind, layer)` slice moves. Kept here so that
    /// property is used rather than merely stated.
    prev_chunk_hashes: Vec<Hash64>,
    prev_attn: Option<Base0PrevAttnGeometryV1>,
    bytes_serialised: u64,
}

/// What the previous push's ATTENTION geometry was, in the three numbers the reuse rule needs.
#[derive(Clone, Copy, Debug)]
struct Base0PrevAttnGeometryV1 {
    positions_per_chunk: u32,
    chunks_per_slice: u32,
    positions: u32,
    layers: usize,
}

impl Base0CheckpointCaptureV1 {
    pub fn new(
        ctx: &PalwJobContextV2,
        profile: &PalwShapeProfileV3,
        checkpoint_profile: &kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1,
    ) -> Self {
        let ctx_hash = ctx.context_hash();
        // **The retention is the cadence's, not the caller's** (ADR-0082 Decision 4, amended).
        let retention = match kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(profile) {
            kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerDecodeCall => Base0CheckpointRetentionV1::Chunks,
            kaspa_consensus_core::palw_context_ladder::PalwCheckpointCadenceV1::PerPosition => Base0CheckpointRetentionV1::Fold,
        };
        Self {
            ctx: ctx.clone(),
            profile: profile.clone(),
            ctx_hash,
            checkpoint_profile_hash: checkpoint_profile.profile_hash(),
            // **From the profile.** The binding check compares the two, so a capture that reached
            // for the family constant here would build a leg its own binding refuses.
            state_chunk_map_id: profile.state_chunk_map_id,
            interval: checkpoint_profile.checkpoint_interval,
            retention,
            prev: kaspa_consensus_core::palw_step_leg::checkpoint_genesis_prev_v2(&ctx_hash),
            leaves: Vec::new(),
            leaf_hashes: Vec::new(),
            chunks: Vec::new(),
            prev_chunk_hashes: Vec::new(),
            prev_attn: None,
            bytes_serialised: 0,
        }
    }

    /// **Bytes this capture has actually SERIALISED**, over every push — the measurement H-3 is
    /// about, and the number `the_per_position_capture_touches_one_tile_a_position` prints. It
    /// counts payload handed to the leaf hash, not the tree's own 128-byte nodes.
    pub fn bytes_serialised_v1(&self) -> u64 {
        self.bytes_serialised
    }

    /// What this capture holds after each push — derived from the class's cadence at construction.
    pub fn retention(&self) -> Base0CheckpointRetentionV1 {
        self.retention
    }

    /// The unit count the NEXT checkpoint will cover — the canonical value the court's
    /// `checkpoint_fault` recomputes, at the cadence the class's own map runs.
    ///
    /// `(index + 1) × interval` DECODE CALLS on every shipped class; `index + 1` POSITIONS on a
    /// class whose map addresses history tiles.
    pub fn next_covered_decode_call(&self) -> u32 {
        kaspa_consensus_core::palw_context_ladder::palw_checkpoint_covered_at_index_v1(
            &self.profile,
            self.leaves.len() as u32,
            self.interval,
        )
        .unwrap_or(u32::MAX)
    }

    /// **Does the cadence put a checkpoint after the forward at this coordinate?**
    ///
    /// The one predicate both backends ask, so neither invents its own boundary: a producer that
    /// checkpointed at a coordinate the court does not expect files a leg whose count is not
    /// canonical, and one that skipped a coordinate opts out of the positions it did not commit.
    pub fn wants_checkpoint_after_v1(&self, call_index: u32, position: u32) -> bool {
        use kaspa_consensus_core::palw_context_ladder as ladder;
        match ladder::palw_checkpoint_cadence_v1(&self.profile) {
            // The prefill is uncovered and a decode call is covered when its own number is the
            // next canonical one.
            ladder::PalwCheckpointCadenceV1::PerDecodeCall => call_index > 0 && call_index == self.next_covered_decode_call(),
            // Every position, prefill included: after the forward at absolute position `p` the
            // cache holds `p + 1` rows, which is exactly the next leaf's covered count.
            ladder::PalwCheckpointCadenceV1::PerPosition => ladder::palw_absolute_position_v1(&self.ctx, call_index, position)
                .and_then(|p| p.checked_add(1))
                .is_some_and(|covered| covered == self.next_covered_decode_call()),
        }
    }

    /// **Take a checkpoint of `cache`**, which must be the state after
    /// [`Self::next_covered_decode_call`] decode calls.
    ///
    /// The position count is derived from the job and the covered call, never from the cache: a
    /// cache that is a row short would otherwise be committed as a shorter state, and the shortfall
    /// would look like a job that ran fewer calls.
    pub fn push(&mut self, cache: &crate::engine::KvCache) -> Result<(), LegError> {
        self.push_with_v1(|entry| cache.state_chunk_bytes(entry))
    }

    /// The map this capture's NEXT checkpoint is taken under — the geometry the CLASS declares,
    /// through [`base0_state_chunk_geometry_v1`], which is the one dispatch both directions take.
    ///
    /// The position count is the cadence's
    /// ([`kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1`]), which on
    /// every per-call class is `integer_kv_positions_at_v1` verbatim.
    pub fn next_geometry(&self) -> Result<kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1, LegError> {
        base0_state_chunk_geometry_v1(&self.profile, self.next_positions_v1())
    }

    /// The positions the NEXT checkpoint's state covers, at the class's cadence.
    pub fn next_positions_v1(&self) -> u32 {
        kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
            &self.profile,
            &self.ctx,
            self.next_covered_decode_call(),
        )
    }

    /// **The whole layout of the next checkpoint, composition included** — what `push_chunks`
    /// prices and validates against, so a hybrid's recurrence half is part of the leaf rather than
    /// a section the producer enumerated and the seat did not (audit B, H-1).
    pub fn next_capture_geometry_v1(&self) -> Result<Base0CaptureGeometryV1, LegError> {
        base0_capture_geometry_v1(&self.profile, self.next_positions_v1())
    }

    /// **Take the next checkpoint from a chunk serializer** — the entry both backends use, so
    /// neither writes its own enumeration of the map.
    ///
    /// `chunk` is the cache's own serializer at one entry (`A16Cache::state_chunk_bytes_v1`,
    /// `KvCache::state_chunk_bytes`); `None` from it is a state the class's declared map cannot
    /// describe, and the run fails here rather than committing a checkpoint that opens to a state
    /// the producer never held.
    pub fn push_with_v1<F>(&mut self, chunk: F) -> Result<(), LegError>
    where
        F: FnMut(&kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>>,
    {
        self.push_composed_v1(chunk, &[])
    }

    /// **Take the next checkpoint of a class whose map names TWO halves** (audit B, H-1 and C-3).
    ///
    /// `attn` is the cache's own serializer, as in [`Self::push_with_v1`]; `gdn_chunks` are the
    /// recurrence's chunks in ITS map's order (`base0_gdn_state_chunks_v2`), which the composition
    /// enumerates after the attention tiles because
    /// `palw-hybrid-state/attn=…/gdn=…/v3` spells them in that order. The walk is
    /// `hybrid_state_chunk_entry_v3` itself, so the producer's enumeration IS the seat's
    /// (`Qwen36RecomputeKernelsV1::state_chunks` walks the same function) rather than a second
    /// derivation that agrees until it does not.
    ///
    /// A checkpoint whose leaf does not carry the recurrence at all (the per-position cadence,
    /// away from the derived spacing) enumerates zero recurrence chunks and `gdn_chunks` is
    /// ignored — the section question is `palw_checkpoint_leaf_carries_recurrence_v1`'s and no
    /// caller answers it.
    pub fn push_composed_v1<F>(&mut self, mut attn: F, gdn_chunks: &[Vec<u8>]) -> Result<(), LegError>
    where
        F: FnMut(&kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>>,
    {
        use kaspa_consensus_core::palw_state_chunk_map::PalwHybridChunkEntryV1;
        let geometry = self.next_capture_geometry_v1()?;
        let attn_geometry = geometry.attn().clone();
        let attn_count = attn_geometry.chunk_count();
        let total = geometry.chunk_count();
        // A retaining capture needs every chunk's BYTES, so there is nothing to skip; the reuse
        // exists exactly where the bytes are thrown away, which is the per-position cadence.
        let retains = self.retention == Base0CheckpointRetentionV1::Chunks;
        let mut chunk_hashes = Vec::with_capacity(total as usize);
        let mut kept: Vec<Vec<u8>> = Vec::new();
        for index in 0..total {
            if !retains
                && index < attn_count
                && let Some(hash) = self.reusable_attn_hash_v1(&attn_geometry, index)
            {
                chunk_hashes.push(hash);
                continue;
            }
            let entry = geometry.entry(index).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?;
            let bytes = match entry {
                PalwHybridChunkEntryV1::AttentionCache(entry) => {
                    attn(&entry).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?
                }
                PalwHybridChunkEntryV1::RecurrenceState { .. } => gdn_chunks
                    .get((index - attn_count) as usize)
                    .cloned()
                    .ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?,
            };
            if bytes.len() as u64 != entry.byte_len() {
                return Err(LegError::CheckpointStateUnavailable { chunk_index: index });
            }
            self.bytes_serialised = self.bytes_serialised.saturating_add(bytes.len() as u64);
            chunk_hashes.push(kaspa_consensus_core::palw_step_leg::state_chunk_leaf_hash_v1(
                &self.state_chunk_map_id,
                index as u32,
                &bytes,
            ));
            if retains {
                kept.push(bytes);
            }
        }
        self.push_chunk_hashes_v1(chunk_hashes, kept, &attn_geometry)
    }

    /// **Is chunk `index`'s hash the value it already had?** (audit B, H-3, with M-3's exception.)
    ///
    /// True only when the previous push took the attention half at the SAME `positions_per_chunk`
    /// and this chunk's `(kind, layer, block)` was a COMPLETE tile then. Complete is the load-
    /// bearing word: the cache is append-only, so a tile whose every position was already written
    /// holds the same bytes in every later state (`A16Cache::state_chunk_bytes_v1` reads
    /// `layer[start .. start + count]`), while the ragged last tile of a slice grows by a row.
    ///
    /// The `positions_per_chunk` equality is M-3: `tiled_kv_state_geometry_v3` pins the width to
    /// `min(16, positions)`, so below 16 positions the tile boundaries themselves move with every
    /// position and NO tile is prefix-stable. That is why the rule is a comparison and not an
    /// assumption — a memo keyed by chunk index alone would produce a wrong root for every job
    /// whose prefill starts inside the first tile.
    fn reusable_attn_hash_v1(
        &self,
        geometry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1,
        index: u64,
    ) -> Option<Hash64> {
        let prev = self.prev_attn?;
        if prev.positions_per_chunk != geometry.positions_per_chunk
            || prev.layers != geometry.attn_layers.len()
            || geometry.positions_per_chunk == 0
        {
            return None;
        }
        let per_kind = geometry.attn_layers.len() as u64 * geometry.chunks_per_slice as u64;
        let kind = index / per_kind;
        let within_kind = index % per_kind;
        let layer_ordinal = within_kind / geometry.chunks_per_slice as u64;
        let block = within_kind % geometry.chunks_per_slice as u64;
        // Complete in the PREVIOUS state, which is what makes its bytes final.
        let covered_then = block.checked_add(1)?.checked_mul(prev.positions_per_chunk as u64)?;
        if covered_then > prev.positions as u64 || block >= prev.chunks_per_slice as u64 {
            return None;
        }
        // …and the leaf hash binds the chunk INDEX, so a tile whose index moved is a different
        // leaf even though its bytes did not.
        let prev_index =
            kind * (prev.layers as u64 * prev.chunks_per_slice as u64) + layer_ordinal * prev.chunks_per_slice as u64 + block;
        if prev_index != index {
            return None;
        }
        self.prev_chunk_hashes.get(index as usize).copied()
    }

    /// **The leaf rule, in one place.** Serializing a cache and re-deriving from served bytes must
    /// produce the same leaf or a producer and a seat would disagree about what was committed, so
    /// both go through here rather than each hashing for itself.
    pub fn push_chunks(&mut self, chunk_bytes: Vec<Vec<u8>>) -> Result<(), LegError> {
        let geometry = self.next_capture_geometry_v1()?;
        if chunk_bytes.len() as u64 != geometry.chunk_count() {
            return Err(LegError::CheckpointStateUnavailable { chunk_index: chunk_bytes.len() as u64 });
        }
        let mut chunk_hashes = Vec::with_capacity(chunk_bytes.len());
        for (index, bytes) in chunk_bytes.iter().enumerate() {
            // The map's own length for this chunk, checked here: bytes of the wrong length hash to
            // a leaf nothing can open, and a producer that committed one could not answer for it.
            // Through the COMPOSITION's enumeration, so a hybrid's recurrence chunk is priced by
            // its own head slice and not by an attention tile's width.
            let entry = geometry.entry(index as u64).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index as u64 })?;
            if bytes.len() as u64 != entry.byte_len() {
                return Err(LegError::CheckpointStateUnavailable { chunk_index: index as u64 });
            }
            chunk_hashes.push(kaspa_consensus_core::palw_step_leg::state_chunk_leaf_hash_v1(
                &self.state_chunk_map_id,
                index as u32,
                bytes,
            ));
        }
        let attn_geometry = geometry.attn().clone();
        // Every chunk was serialised on this route, so the measurement counts them here too.
        self.bytes_serialised = self.bytes_serialised.saturating_add(chunk_bytes.iter().map(|b| b.len() as u64).sum::<u64>());
        let kept = match self.retention {
            Base0CheckpointRetentionV1::Chunks => chunk_bytes,
            Base0CheckpointRetentionV1::Fold => Vec::new(),
        };
        self.push_chunk_hashes_v1(chunk_hashes, kept, &attn_geometry)
    }

    /// **The leaf rule itself** — one place, whether the hashes came from bytes just serialised or
    /// from the previous checkpoint's memo (audit B, H-3). A capture that built its leaf two ways
    /// would be two producers.
    ///
    /// `kept` is the bytes to retain, already empty for a fold: **the fold retains nothing**
    /// (ADR-0082 Decision 4, amended). The bytes were needed to compute the leaf and are not
    /// needed again — the cache they came from is prefix-stable, so
    /// [`base0_checkpoint_chunks_at_v1`] re-derives this checkpoint's chunks from the cache the
    /// executor is holding anyway. Retaining them would make the per-position cadence quadratic in
    /// the job's length, which is the one cost this amendment must not have.
    fn push_chunk_hashes_v1(
        &mut self,
        chunk_hashes: Vec<Hash64>,
        kept: Vec<Vec<u8>>,
        attn_geometry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1,
    ) -> Result<(), LegError> {
        let covered_decode_call = self.next_covered_decode_call();
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
        self.prev_attn = Some(Base0PrevAttnGeometryV1 {
            positions_per_chunk: attn_geometry.positions_per_chunk,
            chunks_per_slice: attn_geometry.chunks_per_slice,
            positions: attn_geometry.positions,
            layers: attn_geometry.attn_layers.len(),
        });
        self.prev_chunk_hashes = chunk_hashes;
        if !kept.is_empty() {
            self.chunks.push(kept);
        }
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
        // **This entry always retains.** The caller handed the bytes over precisely so the leg it
        // rebuilds can be opened — an anchor, an interval start — so folding them away here would
        // throw out the argument. The FOLD is the executor's economy while it still holds the
        // cache, not a property of the leg.
        capture.retention = Base0CheckpointRetentionV1::Chunks;
        for c in chunks {
            capture.push_chunks(c.clone())?;
        }
        let count = chunks.len() as u32;
        capture.finish(count)
    }

    /// **Re-derive the whole leg from its LEAVES alone** — the fold's entry, and the half that
    /// makes a per-position class checkable without one byte of state (ADR-0082 Decision 4,
    /// amended; audit B, C-1).
    ///
    /// [`Self::from_chunks_v1`] rebuilds a leg from the bytes its leaves are hashes of. A
    /// per-position class has no such bytes to hand anyone — retaining a chunk per position is the
    /// `Θ(n²)` term the amendment removes — and it does not need them for THIS question: a leaf is
    /// `(index, covered, prev, chunk_count, state_chunks_root)`, and whether a vector of leaves is
    /// the leg the binding committed is decided by chaining and hashing them. What the leaves
    /// cannot establish is that `state_chunks_root` is the root of a state the job actually
    /// reaches — no served bytes can, because that is arithmetic — and under Decision 9 that is
    /// exactly what the seat's own recompute answers instead.
    ///
    /// Every structural rule the court's own `checkpoint_fault` recomputes is applied here rather
    /// than trusted: the index is its position, the counter is the cadence's canonical value, and
    /// the chain is the previous leaf's hash. A leaf that fails one is refused, so this can never
    /// "rebuild" a leg the producer could not have filed.
    pub fn from_leaves_v1(
        ctx: &PalwJobContextV2,
        profile: &PalwShapeProfileV3,
        checkpoint_profile: &kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1,
        leaves: &[kaspa_consensus_core::palw_step_leg::PalwCheckpointLeafV2],
    ) -> Result<Base0CheckpointsV1, LegError> {
        let mut capture = Self::new(ctx, profile, checkpoint_profile);
        for (index, leaf) in leaves.iter().enumerate() {
            if leaf.checkpoint_index as usize != index
                || leaf.covered_decode_call != capture.next_covered_decode_call()
                || leaf.prev_checkpoint_leaf_hash != capture.prev
                || leaf.state_chunk_count == 0
            {
                return Err(LegError::CheckpointCaptureIncomplete { got: index as u32, expected: leaves.len() as u32 });
            }
            let hash = kaspa_consensus_core::palw_step_leg::checkpoint_leaf_hash_v2(
                &capture.ctx_hash,
                &capture.checkpoint_profile_hash,
                &capture.state_chunk_map_id,
                leaf,
            );
            capture.prev = hash;
            capture.leaves.push(leaf.clone());
            capture.leaf_hashes.push(hash);
        }
        let count = leaves.len() as u32;
        capture.finish(count)
    }

    /// **Seal the capture at the count the CLASS's cadence says the job has** — the count
    /// `palw_step_leg`'s shape pass recomputes, derived rather than passed in.
    ///
    /// [`Self::finish`] takes the number because a caller may be rebuilding a leg from served
    /// chunks and asserting against what it received; a PRODUCER has no such excuse, and every one
    /// of them spelled `decode_calls / interval` for itself.
    pub fn finish_canonical_v1(self) -> Result<Base0CheckpointsV1, LegError> {
        let expected = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_count_v1(&self.profile, &self.ctx, self.interval);
        self.finish(expected)
    }

    /// Seal the capture at the count the job canonically has.
    ///
    /// `expected` is the count the CLASS's cadence gives the job — `decode_calls / interval` under
    /// `PerDecodeCall` and `prefill + decode_calls` under `PerPosition`
    /// ([`kaspa_consensus_core::palw_context_ladder::palw_checkpoint_count_v1`], which the court
    /// recomputes). A producer that sealed a different number would be committing to checkpoints it
    /// never took (or hiding ones it did), and one that spelled the per-call rule for itself would
    /// seal a per-position leg at zero: [`Self::finish_canonical_v1`] is the producer's entry for
    /// exactly that reason.
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
        Ok(Base0CheckpointsV1 {
            leaves: self.leaves,
            leaf_hashes: self.leaf_hashes,
            merkle_root,
            chunks: self.chunks,
            bytes_serialised: self.bytes_serialised,
        })
    }
}

/// **The two derived leg roots, in one place.**
///
/// Neither is a value the binding stores: `PalwStepBindingV2` carries the COMPONENTS — the merkle
/// roots, the counts, the profiles — and `committed_execution_root` is built from the two roots
/// derived here. A caller that needed them (the free-prompt lane needs both by name) would
/// otherwise re-derive them from the components, and a re-derivation that drifted by one argument
/// would produce an execution root the court recomputes differently: an honest producer,
/// unconvictable and unpayable. So the derivation exists once and both callers take it.
#[allow(clippy::too_many_arguments)]
fn leg_roots_v1(
    context_hash: &Hash64,
    profile_hash: &Hash64,
    checkpoint_profile_hash: &Hash64,
    state_chunk_map_id: &Hash64,
    decode_calls: u32,
    checkpoint_count: u32,
    checkpoint_merkle_root: &Hash64,
    step_leaf_count: u64,
    step_merkle_root: &Hash64,
) -> (Hash64, Hash64) {
    use kaspa_consensus_core::palw_step_leg::{checkpoint_leg_root_v2, step_leg_root_v1};
    (
        checkpoint_leg_root_v2(
            context_hash,
            checkpoint_profile_hash,
            state_chunk_map_id,
            decode_calls,
            checkpoint_count,
            checkpoint_merkle_root,
        ),
        step_leg_root_v1(context_hash, profile_hash, step_leaf_count, step_merkle_root),
    )
}

/// The same two roots, read back off a finished binding — what the free-prompt lane commits as
/// `PalwFpRunFactsV3`'s checkpoint and step legs.
///
/// `decode_calls` is recovered the way the builder computes it, from the context's own decode
/// count, so this cannot disagree with the binding it was handed.
pub fn base0_leg_roots_from_binding_v1(binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2) -> (Hash64, Hash64) {
    let context_hash = binding.job_context.context_hash();
    leg_roots_v1(
        &context_hash,
        &binding.shape_profile.shape_profile_id(),
        &binding.checkpoint_profile.profile_hash(),
        &binding.state_chunk_map_id,
        binding.job_context.exact_decode_tokens.saturating_sub(1),
        binding.checkpoint_count,
        &binding.checkpoint_merkle_root,
        binding.step_leaf_count,
        &binding.step_merkle_root,
    )
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
    // The family's registered layout, at this producer's interval. Both were `Hash64::default()`
    // — the unregistered sentinel — which was the only honest value while no map existed; filing
    // it now would file a layout the class does not register, and `verify_binding` refuses that.
    let checkpoint_profile = kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
        kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
    );
    base0_binding_from_capture_with_profile_v1(
        profile,
        ctx,
        tiles,
        checkpoints,
        &checkpoint_profile,
        full_logits_trace_root,
        activation_leg_root,
    )
}

/// The same commitment at a CALLER-supplied checkpoint cadence. The interval is the one free
/// parameter of a checkpoint profile (`palw_step_leg`'s shape pass recomputes
/// `decode_calls / interval` from whatever the binding files), and a class with no registered
/// state map files an interval its jobs can never reach — `n_ctx` is canonical for that: every
/// legal job's decode-call count is below its own context, so the leg is the empty one and the
/// sentinel map id is never asked to chunk anything.
pub fn base0_binding_from_capture_with_profile_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    tiles: &Base0StepTilesV1,
    checkpoints: &Base0CheckpointsV1,
    checkpoint_profile: &kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1,
    full_logits_trace_root: Hash64,
    activation_leg_root: Hash64,
) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, LegError> {
    base0_binding_from_capture_with_profile_capped_v1(
        profile,
        ctx,
        tiles,
        checkpoints,
        checkpoint_profile,
        full_logits_trace_root,
        activation_leg_root,
        kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
    )
}

/// The same commitment against the ladder top the RULESET froze — the COMMIT side of the same
/// defect the opening side had. A class whose step space is wider than the leg's default constant
/// could not build a binding at all, so "arm the deeper ladder" needed this as well as the leg's
/// opening depth. A caller with no ruleset in scope passes the default and nothing moves.
#[allow(clippy::too_many_arguments)]
pub fn base0_binding_from_capture_with_profile_capped_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    tiles: &Base0StepTilesV1,
    checkpoints: &Base0CheckpointsV1,
    checkpoint_profile: &kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1,
    full_logits_trace_root: Hash64,
    activation_leg_root: Hash64,
    step_ladder_cap: u64,
) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, LegError> {
    use kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1;
    let step_leaf_count = tiles.leaves.len() as u64;
    let step_merkle_root = step_merkle_root_capped_v1(&tiles.leaves, step_ladder_cap).map_err(|_| LegError::EmptySpace)?;
    base0_binding_from_step_root_v1(
        profile,
        ctx,
        step_leaf_count,
        step_merkle_root,
        checkpoints,
        checkpoint_profile,
        full_logits_trace_root,
        activation_leg_root,
    )
}

/// **The same commitment, from the step leg's two NUMBERS rather than from a leaf vector.**
///
/// Everything above this line in a binding is a function of `(step_leaf_count, step_merkle_root)`
/// and the job — never of the tiles — so a capture that kept no tiles (ADR-0082 Decision 7's fold)
/// commits through exactly this function, and the dense path reaches it after computing the same
/// two numbers. One derivation of `committed_execution_root`, whichever sink the run used.
#[allow(clippy::too_many_arguments)]
pub fn base0_binding_from_step_root_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    step_leaf_count: u64,
    step_merkle_root: Hash64,
    checkpoints: &Base0CheckpointsV1,
    checkpoint_profile: &kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1,
    full_logits_trace_root: Hash64,
    activation_leg_root: Hash64,
) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, LegError> {
    use kaspa_consensus_core::palw_step_leg::{
        PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, checkpoint_empty_root_v2, execution_commitment_root_v2,
    };
    if step_leaf_count == 0 {
        return Err(LegError::EmptySpace);
    }
    let context_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    // **From the profile, not from the family constant.** A producer files what ITS class
    // registered; reaching for the constant here would work today and would silently file the
    // integer family's map for a class that had registered something else. One source.
    let state_chunk_map_id = profile.state_chunk_map_id;
    // The canonical checkpoint count is `decode_calls / interval`; a job with one decode token has
    // no decode CALLS, so the leg is the empty one — and the shape pass refuses any other pairing
    // of count and root, which is why this is derived rather than chosen.
    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    // **At the cadence the CLASS's map runs** (ADR-0082 Decision 4, amended). `decode_calls /
    // interval` for every shipped class, verbatim; `prefill + decode_calls` for a class whose map
    // addresses history tiles, because its leg commits after every position. The consensus side
    // recomputes exactly this (`palw_step_leg`'s shape pass), so a binding built at the other
    // cadence is a leg the court convicts on sight.
    let checkpoint_count =
        kaspa_consensus_core::palw_context_ladder::palw_checkpoint_count_v1(profile, ctx, checkpoint_profile.checkpoint_interval);
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
    let (checkpoint_root, step_root) = leg_roots_v1(
        &context_hash,
        &profile_hash,
        &checkpoint_profile_hash,
        &state_chunk_map_id,
        decode_calls,
        checkpoint_count,
        &checkpoint_merkle_root,
        step_leaf_count,
        &step_merkle_root,
    );
    let committed_execution_root =
        execution_commitment_root_v2(&context_hash, &full_logits_trace_root, &activation_leg_root, &checkpoint_root, &step_root);
    Ok(PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: ctx.clone(),
        shape_profile: profile.clone(),
        checkpoint_profile: checkpoint_profile.clone(),
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
/// **Any checkpoint's chunks, re-derived from a cache that has run PAST it** (ADR-0082 Decision 4,
/// amended — the half that makes the per-position cadence affordable).
///
/// The attention cache is prefix-stable: the K or V row written at position `j` is the same bytes
/// in every later cache, because rows are appended and never revised. So an entry naming
/// `position_start .. position_start + position_count` reads the same bytes out of the cache at
/// position 4,000 that it read out of the cache at position 40, and a producer that folded its
/// checkpoints away ([`Base0CheckpointRetentionV1::Fold`]) can still answer for every one of them
/// with the cache it is holding anyway.
///
/// `covered` is the leaf's own counter, so the caller passes what the leaf says rather than
/// recomputing which position it means; the geometry is taken at the cadence's position count,
/// which is the one both the capture and the court take.
///
/// This is the DA obligation ADR-0082 Decision 7 states — "to SERVE any opening the leg names, not
/// to STORE it" — for the half of the state that is prefix-stable. A recurrence state is not, and
/// is committed at the derived spacing instead
/// ([`kaspa_consensus_core::palw_context_ladder::palw_checkpoint_leaf_carries_recurrence_v1`]).
pub fn base0_checkpoint_chunks_at_v1<F>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    covered: u32,
    mut chunk: F,
) -> Result<Vec<Vec<u8>>, LegError>
where
    F: FnMut(&kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>>,
{
    use kaspa_consensus_core::palw_state_chunk_map::PalwHybridChunkEntryV1;
    let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, ctx, covered);
    let geometry = base0_capture_geometry_v1(profile, positions)?;
    let mut out = Vec::with_capacity(geometry.chunk_count() as usize);
    for index in 0..geometry.chunk_count() {
        match geometry.entry(index).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })? {
            PalwHybridChunkEntryV1::AttentionCache(entry) => {
                out.push(chunk(&entry).ok_or(LegError::CheckpointStateUnavailable { chunk_index: index })?)
            }
            // **The recurrence is not prefix-stable and is not re-derivable from a later state.**
            // That is the whole reason it rides at a derived spacing rather than at every position
            // (`palw_checkpoint_leaf_carries_recurrence_v1`): a `heads × k_dim × v_dim` delta
            // matrix at position `p` is not a prefix of the one at `p + 1`. A producer answering
            // for such a checkpoint must have retained it, and saying so is better than serving an
            // attention half under a root that was taken over both.
            PalwHybridChunkEntryV1::RecurrenceState { .. } => {
                return Err(LegError::CheckpointStateUnavailable { chunk_index: index });
            }
        }
    }
    Ok(out)
}

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
    base0_kv_anchor_at_covered_v1(checkpoints, want)
}

/// **The anchor for a step at `(call_index, position)`, at the cadence the CLASS's map runs**
/// (ADR-0082 Decision 4, amended).
///
/// The producer's side of `palw_step_refute::verify_kv_anchor`, and it reads the same rule
/// (`palw_checkpoint_covered_for_step_v1`) so the anchor an executor assembles is the anchor the
/// court will demand. On a per-position class this answers for a PREFILL position too, which is
/// the whole point: before the amendment a prefill dispute had no anchor and its bottom was three
/// chunks that no carrier can file.
/// **The chunks come from the LIVE CACHE when the leg retained none** (audit B, C-1). `chunk` is
/// the executor's own serializer — the cache it is holding to run, which is prefix-stable, so it
/// answers for an earlier checkpoint byte-identically ([`base0_checkpoint_chunks_at_v1`]). A
/// retaining leg ignores it and serves what it kept, so the two retentions produce the same
/// operand and the caller does not have to know which one it has.
pub fn base0_kv_anchor_for_step_v1<F>(
    checkpoints: &Base0CheckpointsV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    call_index: u32,
    position: u32,
    chunk: F,
) -> Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>
where
    F: FnMut(&kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>>,
{
    let want = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_covered_for_step_v1(profile, ctx, call_index, position)?;
    base0_kv_anchor_at_covered_with_v1(checkpoints, profile, ctx, want, chunk)
}

fn base0_kv_anchor_at_covered_v1(
    checkpoints: &Base0CheckpointsV1,
    want: u32,
) -> Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1> {
    let at = checkpoints.leaves.iter().position(|l| l.covered_decode_call == want)?;
    let opening = kaspa_consensus_core::palw_step_leg::step_opening_v1(&checkpoints.leaf_hashes, at as u64).ok()?;
    Some(kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1 {
        leaf: checkpoints.leaves[at].clone(),
        opening,
        chunks: checkpoints.chunks.get(at)?.clone(),
    })
}

/// The same anchor, with a chunk source for the leg that folded its bytes away.
fn base0_kv_anchor_at_covered_with_v1<F>(
    checkpoints: &Base0CheckpointsV1,
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    want: u32,
    chunk: F,
) -> Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>
where
    F: FnMut(&kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkEntryV1) -> Option<Vec<u8>>,
{
    let at = checkpoints.leaves.iter().position(|l| l.covered_decode_call == want)?;
    let opening = kaspa_consensus_core::palw_step_leg::step_opening_v1(&checkpoints.leaf_hashes, at as u64).ok()?;
    let chunks = match checkpoints.chunks.get(at) {
        Some(kept) => kept.clone(),
        None => base0_checkpoint_chunks_at_v1(profile, ctx, want, chunk).ok()?,
    };
    Some(kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1 { leaf: checkpoints.leaves[at].clone(), opening, chunks })
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
///
/// **Seeded with the CHAIN's identity inputs, not the binding's internals.** The V2 transition
/// opens every ladder as `open(claim_id, claim.trace_root, …)` and refuses a ladder whose derived
/// id is not the session's (`court_session_id_v2` reads the claim), so a producer-side ladder
/// seeded from `(context_hash, committed_execution_root)` derived a session id NO court would ever
/// carry — the anchored open was unreachable from any real dispute, and the test beside it only
/// compared against a plain ladder built the same wrong way.
pub fn base0_anchored_ladder_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    checkpoints: &Base0CheckpointsV1,
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    claim_id: &Hash64,
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
        claim_id,
        &binding.full_logits_trace_root,
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
    base0_refutation_from_capture_capped_v1(
        profile,
        ctx,
        tiles,
        binding,
        target,
        prompt_token_ids,
        decode_tokens,
        kv_checkpoint,
        kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
    )
}

/// [`base0_refutation_from_capture_v1`] against the ladder top the RULESET froze.
///
/// The two openings this builds — the output opening and the per-run range siblings — are the
/// prover's side of the court's question, and both were capped by a module constant rather than by
/// `PalwCourtParamsV2::max_step_leaf_count`. On a ladder deeper than the default that made an
/// HONEST prover unable to answer at all, which is the same defect as the leg's opening-depth
/// literal and is fixed the same way: the cap arrives from the caller, and the caller that has no
/// ruleset in scope passes the default and behaves exactly as before.
#[allow(clippy::too_many_arguments)]
pub fn base0_refutation_from_capture_capped_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    tiles: &Base0StepTilesV1,
    binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    prompt_token_ids: Vec<u32>,
    decode_tokens: Option<kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1>,
    kv_checkpoint: Option<kaspa_consensus_core::palw_step_refute::PalwCheckpointKvOperandsV1>,
    step_ladder_cap: u64,
) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, LegError> {
    use kaspa_consensus_core::palw_step_leg::step_opening_capped_v1;
    use kaspa_consensus_core::palw_step_refute::{
        PalwExecutionStepRefutationV1, PalwStepInputRowV1, canonical_input_leaves_v1_anchored,
    };

    let leaf_of =
        |index: u64| -> Option<PalwStepTileLeafV1> { tiles.tiles.iter().find(|(i, _)| *i == index).map(|(_, leaf)| leaf.clone()) };
    let target_index = canonical_step_leaf_index(profile, ctx, &target).ok_or(LegError::NotACanonicalCoordinate {
        layer: 0,
        slot: target.node_slot as u16,
        tile: target.tile_index,
    })?;
    let output_preimage = leaf_of(target_index).ok_or(LegError::UnknownSlot { layer: 0, slot: target.node_slot as u16 })?;
    let output_opening = step_opening_capped_v1(&tiles.leaves, target_index, step_ladder_cap)
        .map_err(|_| LegError::NotACanonicalCoordinate { layer: 0, slot: target.node_slot as u16, tile: target.tile_index })?;

    // The canonical input set, in the checker's own order — asked for rather than reconstructed,
    // so a prover cannot disagree with the court about what a step reads.
    let required = canonical_input_leaves_v1_anchored(profile, ctx, &target, kv_checkpoint.is_some())
        .ok_or(LegError::UnknownSlot { layer: 0, slot: target.node_slot as u16 })?;
    // Row form: preimages in canonical order plus one range sibling set per contiguous run —
    // the runs DERIVED from the canonical indices, the same derivation the court applies, so the
    // prover cannot disagree with the checker about where a run begins.
    let mut inputs = Vec::new();
    for row in &required {
        let mut preimages = Vec::with_capacity(row.len());
        for (index, coord) in row {
            preimages.push(leaf_of(*index).ok_or(LegError::UnknownSlot { layer: 0, slot: coord.node_slot as u16 })?);
        }
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (i, (index, _)) in row.iter().enumerate() {
            match runs.last_mut() {
                Some((start, len)) if row[*start].0 + *len as u64 == *index => *len += 1,
                _ => runs.push((i, 1)),
            }
        }
        let mut run_siblings = Vec::with_capacity(runs.len());
        for (start, len) in runs {
            let first = row[start].0 as usize;
            run_siblings.push(
                kaspa_consensus_core::palw_step_leg::step_merkle_range_siblings_capped_v1(&tiles.leaves, first, len, step_ladder_cap)
                    .map_err(|_| LegError::NotACanonicalCoordinate {
                        layer: 0,
                        slot: row[start].1.node_slot as u16,
                        tile: row[start].1.tile_index,
                    })?,
            );
        }
        inputs.push(PalwStepInputRowV1 { preimages, run_siblings });
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
mod a16_row_tests {
    use super::*;
    use crate::artifact::{Base0ArtifactV1, Base0ShapeV1};
    use crate::engine_a16::{A16Cache, A16Engine, derived_a16_store};

    /// A small deterministic A16 class — the same construction the engine's own tests use, kept
    /// tiny so the assertion is about coordinates rather than about arithmetic.
    fn artifact() -> Base0ArtifactV1 {
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 2,
            n_kv_heads: 2,
            d_head: 4,
            d_ff: 8,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("the derived store is sorted and unique")
    }

    /// **The serializer follows the map's declared width, and refuses rather than narrowing.**
    ///
    /// Three cases, because the third is the one that decides whether this family can ever commit
    /// a sound checkpoint: a one-byte map over a row that does not fit must produce NOTHING. The
    /// tempting implementation — the one `KvCache::state_chunk_bytes` uses correctly for its own
    /// `i8` cache — would produce bytes here, pass every downstream check, and commit a state the
    /// producer never held.
    #[test]
    fn a16_state_chunks_follow_the_declared_width_and_refuse_what_does_not_fit() {
        use kaspa_consensus_core::palw_state_chunk_map::{PalwStateChunkEntryV1, PalwStateChunkKindV1};

        let artifact = artifact();
        let engine = A16Engine::new(&artifact).expect("an A16 class");
        let mut cache = A16Cache::new(artifact.shape.n_layers);
        engine.forward_token_traced(&mut cache, 5, 0).expect("one position runs");
        let row_len = cache.key_rows_for_test()[0].len();

        let entry = |row_bytes: u32| PalwStateChunkEntryV1 {
            kind: PalwStateChunkKindV1::Key,
            attn_layer: 0,
            position_start: 0,
            position_count: 1,
            row_bytes,
        };

        // Four bytes per element: the width this cache actually has.
        let wide = cache.state_chunk_bytes_v1(&entry((row_len * 4) as u32)).expect("an i32 row encodes at four bytes each");
        assert_eq!(wide.len(), row_len * 4);
        let first: i32 = i32::from_le_bytes(wide[..4].try_into().expect("four bytes"));
        assert_eq!(first, cache.key_rows_for_test()[0][0], "and it round-trips, little-endian");

        // One byte per element — the map this class declares — against a state that does not fit.
        assert!(
            cache.state_chunk_bytes_v1(&entry(row_len as u32)).is_none(),
            "a row with values outside i8 must be refused under a one-byte map, never truncated"
        );

        // A width that is neither belongs to some other class's map.
        assert!(cache.state_chunk_bytes_v1(&entry((row_len * 2) as u32)).is_none());
    }

    /// **The capture chunks at the width the class DECLARES, and refuses a map it does not know.**
    ///
    /// `next_geometry` read `state_chunk_map_id` for the first time in this commit; before it, the
    /// field was decorative and a class declaring the four-byte map would have had its state
    /// chunked at one byte per element — the exact failure the id exists to prevent, arriving
    /// through the code that is supposed to honour it.
    #[test]
    fn the_checkpoint_capture_follows_the_declared_state_map() {
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
        use kaspa_consensus_core::palw_state_chunk_map as map;

        let base = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the RC geometry is a profile");
        let ctx = rc_job_context(&base, 4, 4);
        let checkpoint_profile = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);

        let width_for = |map_id| {
            let mut profile = base.clone();
            profile.state_chunk_map_id = map_id;
            Base0CheckpointCaptureV1::new(&ctx, &profile, &checkpoint_profile).next_geometry().map(|g| g.row_bytes)
        };

        let v1 = width_for(map::integer_kv_state_chunk_map_id_v1()).expect("v1 is implemented here");
        let v2 = width_for(map::integer_kv_state_chunk_map_id_v2()).expect("v2 is implemented here");
        assert_eq!(v2, v1 * 4, "the declared map decides the width, and v2 is four bytes per element");

        // An unregistered or foreign map is refused rather than approximated: falling back to v1
        // would chunk an unknown class's state at a width nobody chose.
        assert!(width_for(Hash64::default()).is_err(), "the unregistered sentinel is not a map");
        assert!(width_for(Hash64::from_u64_word(0xDEAD)).is_err(), "nor is a map this family does not implement");
    }

    /// **The A16 class registers a checkpoint map that cannot describe its own state.**
    ///
    /// `integer_kv_state_geometry_v1` derives `row_bytes = attn_kv_heads × attn_head_dim` — ONE
    /// byte per KV element — which is exact for BASE-0, whose cache is `Vec<Vec<Vec<i8>>>`.
    /// `Qwen25A16Backend`'s cache is `Vec<Vec<Vec<i32>>>`, and `palw_qwen25_profile` nevertheless
    /// declares `integer_kv_state_chunk_map_id_v1()`.
    ///
    /// The hazard is not that this fails loudly. `KvCache::state_chunk_bytes` guards by comparing
    /// the engine's row LENGTH against the map's `row_bytes` — and for A16 those are the same
    /// number, because 256 i32 elements and 256 declared bytes coincide. A checkpoint written
    /// through that path would pass every check and lose every value outside `i8`, producing a
    /// checkpoint nobody can resume from: worse than no checkpoint, because the producer would
    /// have committed to it.
    ///
    /// So this test measures the state rather than trusting the type: it runs the A16 engine and
    /// asserts real KV values fall outside `i8`. If a future change narrows the cache to `i8`, or
    /// gives the class a 4-byte map, this test is the one that should be revisited — with the
    /// class id, which `state_chunk_map_id` is part of.
    #[test]
    fn a16_kv_state_does_not_fit_the_one_byte_map_its_class_declares() {
        let artifact = artifact();
        let engine = A16Engine::new(&artifact).expect("an A16 class");
        let mut cache = A16Cache::new(artifact.shape.n_layers);
        for position in 0..4 {
            engine.forward_token_traced(&mut cache, (position * 7 + 3) % artifact.shape.vocab, position).expect("runs");
        }
        let rows = cache.key_rows_for_test();
        assert!(!rows.is_empty(), "the cache holds the positions that were run");
        let widest = rows.iter().flatten().copied().map(i32::abs).max().expect("a value");
        assert!(
            widest > i8::MAX as i32,
            "a KV value of {widest} fits in a byte, so this test's premise needs re-measuring rather than assuming"
        );
    }

    // =============================================================================================
    // ADR-0082 Decision 4, amended — the per-position cadence, and the fold that pays for it
    // =============================================================================================

    /// A profile at the RC geometry with the map that puts the class on the per-position cadence.
    #[cfg(test)]
    fn per_position_profile() -> kaspa_consensus_core::palw_step::PalwShapeProfileV3 {
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
        let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the RC geometry is a profile");
        profile.state_chunk_map_id = kaspa_consensus_core::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3();
        profile
    }

    /// Deterministic bytes for a geometry's chunks, of exactly the lengths the map declares.
    #[cfg(test)]
    fn chunks_for(geometry: &kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkGeometryV1, salt: u8) -> Vec<Vec<u8>> {
        (0..geometry.chunk_count())
            .map(|index| {
                let entry = kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_entry_v1(geometry, index)
                    .expect("the map has this chunk");
                (0..entry.byte_len()).map(|b| (b as u8) ^ salt ^ (index as u8)).collect()
            })
            .collect()
    }

    /// **A tiled-map class files a checkpoint at every position, prefill included, and the counter
    /// counts positions.**
    ///
    /// The leg's own arithmetic, without an engine: what the court recomputes is
    /// `palw_checkpoint_count_v1` and `palw_checkpoint_covered_at_index_v1`, and this is the
    /// producer answering them.
    #[test]
    fn a_tiled_map_class_checkpoints_every_position_and_a_shipped_one_does_not() {
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
        use kaspa_consensus_core::palw_context_ladder as ladder;
        use kaspa_consensus_core::palw_state_chunk_map as map;

        let shipped = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("a profile");
        let tiled = per_position_profile();
        let ctx = rc_job_context(&shipped, 4, 4); // prefill 4, exact_decode 4 -> 3 decode calls
        let cp = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);

        // The shipped class: three checkpoints, one per decode call, none over the prefill.
        let mut per_call = Base0CheckpointCaptureV1::new(&ctx, &shipped, &cp);
        assert_eq!(per_call.retention(), Base0CheckpointRetentionV1::Chunks);
        for call in 1..=3u32 {
            assert!(!per_call.wants_checkpoint_after_v1(0, call), "the prefill is uncovered on the shipped cadence");
            assert!(per_call.wants_checkpoint_after_v1(call, 0));
            let g = per_call.next_geometry().expect("a geometry");
            per_call.push_chunks(chunks_for(&g, 0x11)).expect("the chunks are the map's");
        }
        let shipped_leg = per_call.finish_canonical_v1().expect("sealed at the canonical count");
        assert_eq!(shipped_leg.leaves.len(), 3, "decode_calls / interval, the shipped rule");
        for (index, leaf) in shipped_leg.leaves.iter().enumerate() {
            assert_eq!(leaf.covered_decode_call, index as u32 + 1, "(index + 1) x interval, and the interval is one");
        }

        // The tiled class: seven — every position the cache ever holds.
        let mut per_position = Base0CheckpointCaptureV1::new(&ctx, &tiled, &cp);
        assert_eq!(per_position.retention(), Base0CheckpointRetentionV1::Fold, "the retention is the cadence's");
        let mut taken = 0u32;
        for position in 0..ctx.declared_prefill_tokens {
            assert!(per_position.wants_checkpoint_after_v1(0, position), "a PREFILL position is a checkpoint boundary now");
            let g = per_position.next_geometry().expect("a geometry");
            assert_eq!(g.positions, position + 1, "the geometry is taken at the positions the cache holds");
            per_position.push_chunks(chunks_for(&g, 0x22)).expect("the chunks are the map's");
            taken += 1;
        }
        for call in 1..=3u32 {
            assert!(per_position.wants_checkpoint_after_v1(call, 0));
            let g = per_position.next_geometry().expect("a geometry");
            per_position.push_chunks(chunks_for(&g, 0x22)).expect("the chunks are the map's");
            taken += 1;
        }
        assert_eq!(taken, ladder::palw_checkpoint_count_v1(&tiled, &ctx, 1));
        let tiled_leg = per_position.finish_canonical_v1().expect("sealed at the canonical count");
        assert_eq!(tiled_leg.leaves.len(), 7, "prefill 4 + 3 decode calls");
        for (index, leaf) in tiled_leg.leaves.iter().enumerate() {
            assert_eq!(leaf.covered_decode_call, index as u32 + 1, "index + 1 POSITIONS");
        }
        // The two legs are different objects, and the reason is the map id inside the leaf hash —
        // not a version field, which is why `PalwCheckpointLeafV2`'s wire form did not have to move.
        assert_ne!(shipped_leg.merkle_root, tiled_leg.merkle_root);
        assert_eq!(shipped_leg.leaves[0].covered_decode_call, tiled_leg.leaves[0].covered_decode_call, "same number");
        assert_ne!(shipped_leg.leaf_hashes[0], tiled_leg.leaf_hashes[0], "different leaf, because the map id is in the preimage");
    }

    /// **The fold retains nothing, and what a chunk-retaining capture would have retained is
    /// QUADRATIC in the job's length.**
    ///
    /// The measurement Decision 4 rests on. Checkpoint `i` of a per-position leg holds `i + 1`
    /// positions of the cache, so keeping every checkpoint's bytes is `Σ (i+1) = Θ(n²)` rows — and
    /// the cache they came from is prefix-stable, so keeping the cache once answers all of them.
    #[test]
    fn the_folds_retention_is_constant_and_the_alternative_is_quadratic() {
        use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
        use kaspa_consensus_core::palw_state_chunk_map as map;

        let tiled = per_position_profile();
        let cp = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);
        let mut measured: Vec<(u32, u64, u64)> = Vec::new();
        for prefill in [4u32, 8, 16] {
            let ctx = rc_job_context(&base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("a profile"), prefill, 1);
            let mut fold = Base0CheckpointCaptureV1::new(&ctx, &tiled, &cp);
            let mut would_have_retained = 0u64;
            for position in 0..prefill {
                let g = fold.next_geometry().expect("a geometry");
                let bytes = chunks_for(&g, 0x33);
                would_have_retained += bytes.iter().map(|c| c.len() as u64).sum::<u64>();
                assert!(fold.wants_checkpoint_after_v1(0, position));
                fold.push_chunks(bytes).expect("pushes");
            }
            let leg = fold.finish_canonical_v1().expect("sealed");
            let retained: u64 = leg.chunks.iter().flatten().map(|c| c.len() as u64).sum();
            assert!(leg.chunks.is_empty(), "the fold keeps no chunk list at all");
            assert_eq!(retained, 0, "and therefore no bytes");
            assert_eq!(leg.leaves.len() as u32, prefill, "it still commits every one of them");
            measured.push((prefill, retained, would_have_retained));
        }
        // Constant in the job's length — the property being asserted.
        assert!(measured.iter().all(|(_, retained, _)| *retained == 0), "the fold's retention moved: {measured:?}");
        // And the alternative is quadratic: doubling the job more than doubles it, twice over.
        let (_, _, at4) = measured[0];
        let (_, _, at8) = measured[1];
        let (_, _, at16) = measured[2];
        assert!(at8 > 2 * at4 && at16 > 2 * at8, "the chunk-retaining alternative is not superlinear: {measured:?}");
        println!("chunk-retaining alternative at 4/8/16 positions: {at4} / {at8} / {at16} bytes; the fold retains 0 at all three");
    }

    /// **A checkpoint's chunks are re-derivable from a cache that has run PAST it, byte for byte.**
    ///
    /// This is the whole of what makes the fold sound, and it is a property of the CACHE rather
    /// than of the leg: a K or V row written at position `j` is never revised, so an entry naming
    /// `j` reads the same bytes out of every later cache. Measured on the real A16 cache with the
    /// real serializer, at the four-byte width the tiled map declares.
    #[test]
    fn an_earlier_checkpoints_chunks_come_out_of_a_later_cache_unchanged() {
        use kaspa_consensus_core::palw_state_chunk_map::{PalwStateChunkEntryV1, PalwStateChunkKindV1};

        let artifact = artifact();
        let engine = A16Engine::new(&artifact).expect("an A16 class");
        let mut cache = A16Cache::new(artifact.shape.n_layers);
        let row_len = {
            engine.forward_token_traced(&mut cache, 5, 0).expect("one position runs");
            cache.key_rows_for_test()[0].len()
        };
        let entry = |position_start: u32, position_count: u32| PalwStateChunkEntryV1 {
            kind: PalwStateChunkKindV1::Key,
            attn_layer: 0,
            position_start,
            position_count,
            row_bytes: (row_len * 4) as u32,
        };

        // Snapshot every prefix's bytes as the cache grows, exactly as a per-position capture would.
        let mut as_taken: Vec<Vec<u8>> = vec![cache.state_chunk_bytes_v1(&entry(0, 1)).expect("position 0")];
        for position in 1..6u32 {
            engine
                .forward_token_traced(&mut cache, (position as usize * 7 + 3) % artifact.shape.vocab, position as usize)
                .expect("runs");
            as_taken.push(cache.state_chunk_bytes_v1(&entry(0, position + 1)).expect("the prefix so far"));
        }
        // Now re-derive every one of them from the FINAL cache — which is what the fold does.
        for (index, taken) in as_taken.iter().enumerate() {
            let rederived = cache.state_chunk_bytes_v1(&entry(0, index as u32 + 1)).expect("the same entry, a later cache");
            assert_eq!(
                &rederived, taken,
                "checkpoint {index}'s chunk changed once the cache ran on — the attention cache is not prefix-stable and the \
                 fold is unsound"
            );
        }
        // A tile in the MIDDLE of the history too, which is what the dissection's bottom opens.
        let mid = cache.state_chunk_bytes_v1(&entry(2, 3)).expect("positions 2..5");
        assert_eq!(mid.len(), 3 * row_len * 4);
        assert_eq!(&mid[..row_len * 4], &as_taken[2][2 * row_len * 4..3 * row_len * 4], "and it is the same rows, at an offset");
    }

    /// **The converter must preserve the trace's own coordinates, not invent an ordering.**
    ///
    /// A step leaf is addressed by (table, layer, index), and the court recomputes the row at that
    /// address. A converter that renumbered — flattened the layers into one sequence, say — would
    /// commit rows that are individually correct and collectively unfindable, and the failure
    /// would appear only when somebody disputed a claim. So this asserts the addresses against the
    /// trace that produced them, field by field.
    #[test]
    fn the_a16_trace_maps_to_leaf_coordinates_one_for_one() {
        let artifact = artifact();
        let engine = A16Engine::new(&artifact).expect("an A16 class");
        let mut cache = A16Cache::new(artifact.shape.n_layers);
        let (_logits, trace) = engine.forward_token_traced(&mut cache, 7, 0).expect("one position runs");

        let rows = a16_captured_rows_v1(&trace);
        let attn_total: usize = trace.attn.iter().map(Vec::len).sum();
        assert_eq!(rows.len(), trace.pre.len() + attn_total + trace.post.len(), "every recorded node becomes exactly one row");

        let pre: Vec<_> = rows.iter().filter(|r| r.table == PalwStepTableV1::Pre).collect();
        assert_eq!(pre.len(), trace.pre.len());
        for (i, row) in pre.iter().enumerate() {
            assert_eq!((row.layer, row.index), (0, i), "pre is a step sequence at layer 0");
            assert_eq!(row.row, trace.pre[i], "and it carries that step's row");
        }

        for (layer, nodes) in trace.attn.iter().enumerate() {
            for (index, expected) in nodes.iter().enumerate() {
                let found = rows
                    .iter()
                    .find(|r| r.table == PalwStepTableV1::Attn && r.layer == layer as u16 && r.index == index)
                    .expect("every attention node is addressable by its own (layer, index)");
                assert_eq!(&found.row, expected);
            }
        }

        let post: Vec<_> = rows.iter().filter(|r| r.table == PalwStepTableV1::Post).collect();
        assert_eq!(post.len(), trace.post.len());
        for (i, row) in post.iter().enumerate() {
            assert_eq!((row.layer, row.index), (0, i));
            assert_eq!(row.row, trace.post[i]);
        }
    }

    /// **H-3, plan item 7: the per-position capture touches ONE ragged tile a position, not the
    /// whole cache.**
    ///
    /// The fold's economy was a RETENTION economy only: `push` enumerated every chunk and
    /// serialized it, so checkpoint `p` wrote `2 · layers · (p+1) · row` bytes and the job's total
    /// was `Θ(n²)` — 7.5 GB on the dense graph-v5 row at `n_ctx` 512, 1.9 TB at the ladder's top,
    /// on the block-producing path. The attention cache is append-only, so a COMPLETE tile's chunk
    /// hash is the value it already had; only the ragged last tile of each `(kind, layer)` slice
    /// moves.
    ///
    /// Measured on the shape the report names (28 attention layers, `kv_row = 2·128·4 = 1024 B`)
    /// and asserted against derived bounds rather than against a constant.
    ///
    /// # What does NOT go away, and why it is the map's and not this memo's
    ///
    /// The leaf hash binds the chunk's INDEX (`state_chunk_leaf_hash_v1(map_id, index, bytes)`),
    /// and the tiled map's index is `(kind · layers + layer_ordinal) · chunks_per_slice + block`.
    /// So when a slice grows a block — once every `PALW_ATTN_HISTORY_TILE_V4` positions — every
    /// index above the first slice MOVES, and a chunk whose bytes did not change is nevertheless a
    /// different leaf. Its hash cannot be reused, and hashing it needs its bytes, so that
    /// checkpoint re-serialises the whole cache. That leaves a `2 · layers · row · n² / (2 · tile)`
    /// term, which is `tile` times smaller than the `Θ(n²/2)` it replaced and is a fact about the
    /// MAP: only a second copy of the cache (a full retention) or an index scheme that does not
    /// move could remove it, and this capture is allowed neither.
    ///
    /// So the assertion is the ORDER, stated in both directions: an order below the whole-cache
    /// form, and at most the map's own residual.
    #[test]
    fn the_per_position_capture_touches_one_tile_a_position() {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        let mut profile = per_position_profile();
        profile.layer_count = 28;
        profile.full_attention_interval = 1; // every layer is an attention layer, as the dense tier's are
        profile.attn_kv_heads = 2;
        profile.attn_head_dim = 128;
        profile.n_ctx = 512;
        let n: u32 = 512;
        let shape = base0_state_chunk_geometry_v1(&profile, n).expect("the tiled map describes this shape");
        let row_bytes = shape.row_bytes as u64;
        let layers = shape.attn_layers.len() as u64;
        assert_eq!((layers, row_bytes), (28, 1024), "the shape the report's number is derived over");
        let ctx = {
            let mut ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, n - 1, 2);
            ctx.job_id = Hash64::from_u64_word(0x0000_8243);
            ctx
        };
        assert_eq!(
            kaspa_consensus_core::palw_context_ladder::palw_checkpoint_count_v1(&profile, &ctx, 1),
            n,
            "the leg is one checkpoint a position"
        );
        let cp = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);
        let mut capture = Base0CheckpointCaptureV1::new(&ctx, &profile, &cp);
        assert_eq!(capture.retention(), Base0CheckpointRetentionV1::Fold);
        // The bytes are not a cache's — nothing here runs a model — but they are exactly the
        // lengths the map declares, which is all the capture's arithmetic reads.
        for _ in 0..n {
            capture.push_with_v1(|entry| Some(vec![0u8; entry.byte_len() as usize])).expect("the fold takes every position");
        }
        let leg = capture.finish_canonical_v1().expect("sealed at the canonical count");
        assert_eq!(leg.leaves.len() as u32, n);

        let touched = leg.bytes_serialised;
        let tile = map::PALW_ATTN_HISTORY_TILE_V4 as u64;
        let quadratic = 2 * layers * row_bytes * (n as u64) * (n as u64 + 1) / 2;
        // One ragged tile per `(kind, layer)` slice at every position…
        let linear_bound = 2 * layers * row_bytes * (n as u64) * tile;
        // …plus the whole cache once at every slice-growth boundary, where every chunk index above
        // the first slice moves and its leaf hash therefore moves with it.
        let residual = 2 * layers * row_bytes * (n as u64) * (n as u64 + tile) / (2 * tile);
        eprintln!(
            "H-3 at n_ctx {n}, {layers} attention layers, kv_row {row_bytes} B: {touched} bytes serialised \
             ({:.2} MB) against {quadratic} ({:.2} GB) for the whole-cache form — {:.0}x less",
            touched as f64 / 1e6,
            quadratic as f64 / 1e9,
            quadratic as f64 / touched.max(1) as f64
        );
        assert!(touched < quadratic / 8, "the whole-cache term must be gone by an order: {touched} against {quadratic}");
        assert!(
            touched <= linear_bound + residual,
            "what may remain is one ragged tile a position plus the map's index-shift term: {touched} against {}",
            linear_bound + residual
        );
    }

    /// **M-3 is the guard, and it is measured rather than assumed.**
    ///
    /// `tiled_kv_state_geometry_v3` pins `positions_per_chunk = min(16, positions)`, so below 16
    /// positions the tile boundaries move with every position and NO complete tile is prefix-
    /// stable. A memo keyed by chunk index that assumed otherwise would produce a wrong root for
    /// every job whose prefill starts inside the first tile — so the roots the memoising capture
    /// commits are compared, position by position, against a capture that reuses nothing.
    #[test]
    fn the_memo_agrees_with_a_capture_that_reuses_nothing_across_the_sixteen_position_boundary() {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        let profile = per_position_profile();
        let n = 3 * map::PALW_ATTN_HISTORY_TILE_V4 + 1; // across two boundaries and past them
        let ctx = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, n - 1, 2);
        let cp = map::integer_kv_checkpoint_profile_v1(map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1);

        // The bytes are a function of (kind, layer, position) alone — which is what "the cache is
        // append-only" means — so a chunk's content is the same however the geometry is tiled.
        let byte_of = |entry: &map::PalwStateChunkEntryV1, offset: usize| -> u8 {
            let position = entry.position_start as usize + offset / entry.row_bytes as usize;
            let within = offset % entry.row_bytes as usize;
            (position as u8).wrapping_mul(31).wrapping_add(within as u8).wrapping_add(entry.attn_layer as u8) ^ (entry.kind as u8)
        };
        let fill = |entry: &map::PalwStateChunkEntryV1| -> Option<Vec<u8>> {
            Some((0..entry.byte_len() as usize).map(|o| byte_of(entry, o)).collect())
        };

        let mut memoising = Base0CheckpointCaptureV1::new(&ctx, &profile, &cp);
        let mut naive = Base0CheckpointCaptureV1::new(&ctx, &profile, &cp);
        for _ in 0..n {
            memoising.push_with_v1(fill).expect("the memoising capture takes it");
            // `push_chunks` is the route that reuses nothing: it is handed every chunk's bytes.
            let geometry = naive.next_geometry().expect("a geometry");
            let chunks: Vec<Vec<u8>> = (0..geometry.chunk_count())
                .map(|i| fill(&map::integer_kv_state_chunk_entry_v1(&geometry, i).expect("an entry")).expect("bytes"))
                .collect();
            naive.push_chunks(chunks).expect("the naive capture takes it");
        }
        let a = memoising.finish_canonical_v1().expect("sealed");
        let b = naive.finish_canonical_v1().expect("sealed");
        assert_eq!(a.leaf_hashes, b.leaf_hashes, "the memo must commit exactly what a full re-hash commits");
        assert_eq!(a.merkle_root, b.merkle_root);
        assert!(a.bytes_serialised < b.bytes_serialised, "…and it must do less work to get there");
    }
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
    /// This was written when the profile was generated from `BASE0_LAYER_IR` and the engine was
    /// not, so nothing structural stopped the two describing different computations — which they
    /// did, four times over, each found by someone reading rather than by anything failing. The
    /// generator it asked for exists now (`plan::Base0PlanV1`): the engine's op sequence is
    /// compiled from the same table, so a divergence is not detected here, it is unrepresentable.
    ///
    /// The test stays, and it is a different kind of statement now. `plan::base0_check_graph_v1`
    /// compares two TABLES; this runs the engine and measures what it actually emitted, which is
    /// the only check that would notice a plan that compiles cleanly and executes something else —
    /// a kernel dispatch that produced the wrong number of values, a per-head loop that ran the
    /// wrong number of times. Tables agreeing is not rows agreeing.
    ///
    /// What it compares is the whole observable shape of an execution: the slot sequence, in
    /// order, and each row's length against the width the profile declares for that node at this
    /// position's `kv_len`.
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
            &kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
                kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
            ),
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
