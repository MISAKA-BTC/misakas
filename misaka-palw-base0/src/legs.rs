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

use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, PalwStepCoordinateV1, canonical_step_leaf_index};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepTileLeafV1, step_merkle_root_v1, step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// Why a capture cannot become a leg.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegError {
    /// The profile and the capture disagree about which slots exist.
    UnknownSlot { layer: u16, slot: u16 },
    /// `canonical_step_leaf_index` refused the coordinate — the capture is describing a step this
    /// class's step space does not have.
    NotACanonicalCoordinate { layer: u16, slot: u16, tile: u32 },
    /// The step space has no leaves, so there is nothing to commit.
    EmptySpace,
}

/// One captured step row, tiled and placed at its canonical leaf index.
pub struct Base0StepTilesV1 {
    pub leaves: Vec<Hash64>,
    pub tiles: Vec<(u64, PalwStepTileLeafV1)>,
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
            .checked_add((*layer as u32).checked_mul(profile.attn_nodes.len() as u32).ok_or(LegError::UnknownSlot {
                layer: *layer,
                slot: *slot,
            })?)
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
            let coord = PalwStepCoordinateV1 {
                call_index,
                node_slot: global_slot,
                position,
                tile_index: tile_index as u32,
            };
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
}
