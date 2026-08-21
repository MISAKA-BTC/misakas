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
    full_logits_trace_root: Hash64,
    activation_leg_root: Hash64,
) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, LegError> {
    use kaspa_consensus_core::palw_step_leg::{
        PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, checkpoint_empty_root_v2, checkpoint_leg_root_v2,
        execution_commitment_root_v2, step_leg_root_v1,
    };
    let context_hash = ctx.context_hash();
    let profile_hash = profile.shape_profile_id();
    let checkpoint_profile = kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1 {
        version: kaspa_consensus_core::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
        checkpoint_interval: 1,
        state_layout_id: Hash64::default(),
    };
    let step_leaf_count = tiles.leaves.len() as u64;
    let step_merkle_root = step_merkle_root_v1(&tiles.leaves).map_err(|_| LegError::EmptySpace)?;
    let state_chunk_map_id = Hash64::default();
    // The canonical checkpoint count is `decode_calls / interval`; a job with one decode token has
    // no decode CALLS, so the leg is the empty one — and the shape pass refuses any other pairing
    // of count and root, which is why this is derived rather than chosen.
    let decode_calls = ctx.exact_decode_tokens.saturating_sub(1);
    let checkpoint_count = decode_calls / checkpoint_profile.checkpoint_interval;
    let checkpoint_merkle_root = if checkpoint_count == 0 { checkpoint_empty_root_v2(&context_hash) } else { Hash64::default() };
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
pub fn base0_refutation_from_capture_v1(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    tiles: &Base0StepTilesV1,
    binding: kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    prompt_token_ids: Vec<u32>,
) -> Result<kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1, LegError> {
    use kaspa_consensus_core::palw_step_leg::step_opening_v1;
    use kaspa_consensus_core::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepInputOpeningV1, canonical_input_leaves_v1};

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
    let required =
        canonical_input_leaves_v1(profile, ctx, &target).ok_or(LegError::UnknownSlot { layer: 0, slot: target.node_slot as u16 })?;
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
    Ok(PalwExecutionStepRefutationV1 { binding, output_opening, output_preimage, inputs, prompt_token_ids, decode_tokens: None })
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
        let root = base0_step_merkle_root_v1(&tiles).expect("a populated space has a root");

        let binding = |tiles: &Base0StepTilesV1| {
            base0_binding_from_capture_v1(&profile, &ctx, tiles, Hash64::default(), Hash64::default())
                .expect("a capture yields its own commitment")
        };

        // A step with real inputs and a real weight operand: the FFN down projection's narrowing
        // (slot 33 of layer 0), which reads the accumulator the step before it produced.
        let target =
            PalwStepCoordinateV1 { call_index: 0, node_slot: profile.pre_nodes.len() as u32 + 33, position: 0, tile_index: 0 };
        let honest = base0_refutation_from_capture_v1(&profile, &ctx, &tiles, binding(&tiles), target, Vec::new())
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
        let (_, _, row) = lying.iter_mut().find(|(l, slot, _)| *l == 0 && *slot == 33).expect("the step is captured");
        row[0] = row[0].wrapping_add(1);
        let lying_tiles = base0_step_tiles_v1(&profile, &ctx, leaf_count, 0, 0, &lying).expect("the rows tile");
        let lying_root = base0_step_merkle_root_v1(&lying_tiles).expect("rooted");
        let fraud = base0_refutation_from_capture_v1(&profile, &ctx, &lying_tiles, binding(&lying_tiles), target, Vec::new())
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
