//! Owner-audited capture assembly. Input slices are borrowed from a real capture;
//! membership roots, not a caller's label, decide whether its bytes belong to the
//! accused execution. The returned evidence retains its construction reservation.
use super::*;
use crate::palw_native_resource_v1::{NativeRetainedV1, native_value_requests_v1};

fn extend(lease: &mut Option<NativeDemandLeaseV1>, bytes: u64) -> Result<(), PalwGdnCourtErrorV2> {
    if let Some(lease) = lease {
        lease.grow_memory_v1(bytes)?;
    } else {
        *lease = reserve_native_requests_v1(bytes)?;
    }
    Ok(())
}

/// Assemble the exact V2 recurrence evidence from captured step tiles and the
/// predecessor checkpoint's actual chunks. A folded capture must re-derive the
/// requested historical chunks first; current recurrence state cannot stand in
/// for historical state. Every supplied state chunk is hashed against the leaf.
///
/// This authenticates committed data and prepares direct recomputation. It does
/// not approve a model, create validator approval, or confer consensus credit.
/// Input capture ownership/retention is a separate caller obligation. Historical
/// V1 assembly remains unchanged and this API never guesses a missing checkpoint.
#[allow(clippy::too_many_arguments)]
pub fn gdn_refutation_from_capture_v2(
    binding: &leg::PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    step_hashes: &[Hash64],
    tiles: &[(u64, leg::PalwStepTileLeafV1)],
    checkpoint_leaves: &[leg::PalwCheckpointLeafV2],
    checkpoint_hashes: &[Hash64],
    predecessor_chunks: &[Vec<u8>],
    max_leaves: u64,
) -> Result<NativeRetainedV1<PalwGdnStepRefutationV2>, PalwGdnCourtErrorV2> {
    let p = &binding.shape_profile;
    let ctx = &binding.job_context;
    let start = gdn_replay_start_v2(binding, &target)?;
    let index = canonical_step_leaf_index(p, ctx, &target).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if binding.step_leaf_count == 0
        || binding.step_leaf_count > max_leaves
        || step_hashes.len() as u64 != binding.step_leaf_count
        || checkpoint_hashes.len() != checkpoint_leaves.len()
        || checkpoint_hashes.len() as u64 != u64::from(binding.checkpoint_count)
    {
        return Err(evidence("capture leaf counts differ from the committed binding"));
    }
    let (node, layer) = p.resolve_node_slot(target.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let sliced = match program {
        KernelProgram::GdnCore { .. } => false,
        KernelProgram::Qwen36(Qwen36Op::GdnStep) => true,
        _ => return Err(PalwStepRefuteError::Unadjudicable.into()),
    };
    if node.input_refs.len() != 5 {
        return Err(evidence("recurrence capture requires the actual five kernel operands"));
    }
    let here =
        cadence::palw_absolute_position_v1(ctx, target.call_index, target.position).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let positions = u64::from(here - start.first_position) + 1;
    let rows = TirNativeRequestsV1::product_v1(positions, node.input_refs.len() as u64)?;
    let first_slot = target.node_slot - intra_table_index(p, target.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)? as u32;
    // First reserve the canonical plan itself, before constructing its Vecs.
    // Whole fixed rows bound sliced rows, without a byte-to-heap multiplier.
    let mut plan = TirNativeRequestsV1::new();
    plan.vec_v1::<(u32, u32)>(positions)?;
    plan.vec_v1::<Vec<(u64, PalwStepCoordinateV1)>>(rows)?;
    plan.vec_v1::<u64>(node.input_refs.len() as u64)?;
    for &reference in &node.input_refs {
        if reference >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
            return Err(evidence("recurrence capture input reference is not an actual registered node"));
        }
        let (source, _) = p.resolve_node_slot(first_slot + u32::from(reference)).ok_or(PalwStepRefuteError::Unadjudicable)?;
        let crate::palw_step::PalwStepOutLenV1::Fixed { elements } = source.out_len else {
            return Err(evidence("recurrence capture source has no fixed row bound"));
        };
        if source.tile_len == 0 {
            return Err(evidence("recurrence capture source has zero tile length"));
        }
        let mut one = TirNativeRequestsV1::new();
        one.vec_v1::<(u64, PalwStepCoordinateV1)>(u64::from(elements).div_ceil(u64::from(source.tile_len)))?;
        plan.add_v1(TirNativeRequestsV1::product_v1(positions, one.bytes_v1())?)?;
    }
    let mut lease = reserve_native_requests_v1(plan.bytes_v1())?;
    let required = canonical_input_leaves_with_start_v2(p, ctx, target.node_slot, &target, program, false, Some(start.first_position))
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let leaf_of = |want| {
        let mut found = tiles.iter().filter(|(i, _)| *i == want);
        let (_, leaf) = found.next().ok_or_else(|| evidence("capture is missing a canonical recurrence tile"))?;
        if found.next().is_some() {
            return Err(evidence("capture repeats a canonical recurrence tile"));
        }
        Ok(leaf)
    };
    let output = leaf_of(index)?;
    let depth = u64::from(64 - binding.step_leaf_count.leading_zeros());
    let geometry = PalwGdnStateGeometryV2::of(p).map_err(evidence)?;
    let heads = if sliced { target.tile_index..target.tile_index.saturating_add(1) } else { 0..geometry.heads };
    if heads.end > geometry.heads {
        return Err(evidence("capture selected head is outside its registered geometry"));
    }
    let blocks = geometry.v_dim.div_ceil(geometry.delta_rows_per_chunk);
    let mut build = TirNativeRequestsV1::new();
    build.add_v1(native_value_requests_v1(binding)?)?;
    build.add_v1(native_value_requests_v1(output)?)?;
    build.vec_v1::<Hash64>(depth)?; // retained output path
    build.vec_v1::<PalwStepInputRowV1>(required.len() as u64)?;
    let mut maximum_tile_bytes = output.values_le.len() as u64;
    for row in &required {
        build.vec_v1::<leg::PalwStepTileLeafV1>(row.len() as u64)?;
        build.vec_v1::<Vec<Hash64>>(row.len() as u64)?;
        build.vec_v1::<(usize, usize)>(row.len() as u64)?; // current run plan
        for (wanted, _) in row {
            let leaf = leaf_of(*wanted)?;
            maximum_tile_bytes = maximum_tile_bytes.max(leaf.values_le.len() as u64);
            build.add_v1(native_value_requests_v1(leaf)?)?;
            build.vec_v1::<Hash64>(2 * depth)?; // at most one run per tile
        }
    }
    // Each opening builder holds the leaf level and its parent at once.
    // Step/checkpoint trees are sequential, state hashing remains live beside
    // its path builder. Explicit typed buffers cover these actual memberships.
    build.vec_v1::<Hash64>(binding.step_leaf_count)?;
    build.vec_v1::<Hash64>(binding.step_leaf_count.div_ceil(2))?;
    build.vec_v1::<Hash64>(u64::from(binding.checkpoint_count))?;
    build.vec_v1::<Hash64>(u64::from(binding.checkpoint_count).div_ceil(2))?;
    build.vec_v1::<Hash64>(u64::from(32 - binding.checkpoint_count.leading_zeros()))?;
    build.vec_v1::<u16>(u64::from(p.layer_count))?;
    build.vec_v1::<u16>(u64::from(p.layer_count))?;
    let mut offset = 0u64;
    let checkpoint = if let Some(at) = start.checkpoint_index {
        let leaf = checkpoint_leaves.get(at as usize).ok_or_else(|| evidence("predecessor checkpoint is absent from capture"))?;
        if predecessor_chunks.len() as u64 != u64::from(leaf.state_chunk_count) {
            return Err(evidence("capture does not contain all predecessor state chunks"));
        }
        let expected = if maps::palw_profile_is_held_v4(p) {
            // Derivation allocates layer lists, already reserved in plan below.
            // A separate pre-allocation ticket covers layout derivation itself.
            let mut layout_q = TirNativeRequestsV1::new();
            layout_q.vec_v1::<u16>(u64::from(p.layer_count))?;
            layout_q.vec_v1::<u16>(u64::from(p.layer_count))?;
            extend(&mut lease, layout_q.bytes_v1())?;
            let layout =
                maps::palw_state_layout_v4(p, start.first_position).map_err(|_| evidence("capture held layout is unavailable"))?;
            offset = layout.attn.chunk_count();
            layout.chunk_count()
        } else if p.state_chunk_map_id == maps::hybrid_state_chunk_map_id_v3() {
            let mut layout_q = TirNativeRequestsV1::new();
            layout_q.vec_v1::<u16>(u64::from(p.layer_count))?;
            layout_q.vec_v1::<u16>(u64::from(p.layer_count))?;
            extend(&mut lease, layout_q.bytes_v1())?;
            let layout = maps::hybrid_state_geometry_for_covered_v1(p, start.first_position)
                .map_err(|_| evidence("capture hybrid layout is unavailable"))?;
            offset = layout.attn.chunk_count();
            layout.chunk_count()
        } else {
            geometry.chunk_count()
        };
        if expected != predecessor_chunks.len() as u64 {
            return Err(evidence("capture chunk count differs from registered recurrence layout"));
        }
        // Actual hash Vec, held top/subtree collection, root/path current and
        // parent levels. Each is a concrete Hash64 buffer, not a codec factor.
        for _role in ["chunk hashes", "subtree hashes", "subtree level", "subtree parent", "top leaves", "top level", "top parent"] {
            build.vec_v1::<Hash64>(expected)?;
        }
        let count = TirNativeRequestsV1::product_v1(u64::from(heads.end - heads.start), u64::from(blocks))?;
        build.vec_v1::<PalwGdnStateChunkWitnessV2>(count)?;
        for h in heads.clone() {
            for block in 0..blocks {
                let local = geometry
                    .delta_chunk_index(layer.ok_or(PalwStepRefuteError::Unadjudicable)?, h, block)
                    .ok_or(PalwStepRefuteError::Unadjudicable)?;
                let flat = offset.checked_add(local).ok_or(PalwStepRefuteError::Unadjudicable)?;
                let chunk = predecessor_chunks.get(flat as usize).ok_or_else(|| evidence("capture selected delta chunk is absent"))?;
                build.vec_v1::<u8>(chunk.len() as u64)?;
                build.vec_v1::<Hash64>(u64::from(leg::PALW_STEP_LEG_MAX_STATE_DEPTH_V4))?;
            }
        }
        Some((at, leaf))
    } else {
        if !predecessor_chunks.is_empty() {
            return Err(evidence("genesis capture must not supply checkpoint state"));
        }
        None
    };
    let maximum_chunk = predecessor_chunks.iter().map(|chunk| chunk.len() as u64).max().unwrap_or(0);
    if maximum_chunk > leg::PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 {
        return Err(evidence("capture chunk exceeds existing state chunk cap"));
    }
    build.vec_v1::<u8>((maximum_chunk + 80).max(256))?; // current canonical state leaf writer
    build.vec_v1::<u8>((maximum_tile_bytes + 192).max(256))?; // current canonical tile writer
    build.vec_v1::<u8>((borsh::object_length(p).map_err(|_| evidence("cannot measure captured profile"))? as u64).max(1024))?;
    build.vec_v1::<u8>((754 + ctx.network_id.len() as u64).max(1024))?;
    build.vec_v1::<u8>(512)?; // fixed checkpoint/context root framing
    extend(&mut lease, build.bytes_v1())?;
    leg::verify_binding_v1(binding)?;
    let output_opening = leg::step_opening_capped_v1(step_hashes, index, max_leaves)?;
    if leg::step_opening_root_capped_v1(binding.step_leaf_count, &output_opening, max_leaves)? != binding.step_merkle_root {
        return Err(evidence("captured step hashes differ from the committed execution"));
    }
    let profile_id = p.shape_profile_id();
    let verify_tile = |leaf: &leg::PalwStepTileLeafV1, wanted: u64, coord: &PalwStepCoordinateV1| {
        if leaf.coord != *coord || leg::step_tile_leaf_hash_ctx_v1(ctx, &profile_id, leaf) != step_hashes[wanted as usize] {
            return Err(evidence("captured tile is not the committed canonical preimage"));
        }
        Ok(())
    };
    verify_tile(output, index, &target)?;
    let mut inputs = Vec::with_capacity(required.len());
    for row in &required {
        let mut preimages = Vec::with_capacity(row.len());
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (i, (wanted, coord)) in row.iter().enumerate() {
            let leaf = leaf_of(*wanted)?;
            verify_tile(leaf, *wanted, coord)?;
            preimages.push(leaf.clone());
            match runs.last_mut() {
                Some((from, len)) if row[*from].0 + *len as u64 == *wanted => *len += 1,
                _ => runs.push((i, 1)),
            }
        }
        let mut run_siblings = Vec::with_capacity(runs.len());
        for (from, len) in runs {
            run_siblings.push(leg::step_merkle_range_siblings_capped_v1(step_hashes, row[from].0 as usize, len, max_leaves)?);
        }
        inputs.push(PalwStepInputRowV1 { preimages, run_siblings });
    }
    let checkpoint = if let Some((at, leaf)) = checkpoint {
        let opening = leg::step_opening_capped_v1(checkpoint_hashes, u64::from(at), max_leaves)?;
        let mut hashes = Vec::with_capacity(predecessor_chunks.len());
        for (flat, bytes) in predecessor_chunks.iter().enumerate() {
            hashes.push(
                maps::palw_state_chunk_leaf_for_map_v1(p, start.first_position, flat as u32, bytes)
                    .ok_or_else(|| evidence("capture cannot address a predecessor state chunk"))?,
            );
        }
        let root = maps::palw_state_root_from_leaves_for_map_v1(p, start.first_position, &hashes)?;
        if root != leaf.state_chunks_root {
            return Err(evidence("capture predecessor chunks differ from its committed root"));
        }
        let mut chunks = Vec::new();
        for head in heads {
            for block in 0..blocks {
                let local = geometry
                    .delta_chunk_index(layer.ok_or(PalwStepRefuteError::Unadjudicable)?, head, block)
                    .ok_or(PalwStepRefuteError::Unadjudicable)?;
                let flat = u32::try_from(offset + local).map_err(|_| evidence("capture chunk index overflows its wire type"))?;
                chunks.push(PalwGdnStateChunkWitnessV2 {
                    chunk_index: flat,
                    bytes: predecessor_chunks[flat as usize].clone(),
                    siblings: maps::palw_state_chunk_path_from_leaves_for_map_v1(p, start.first_position, &hashes, flat)?,
                });
            }
        }
        let witness = PalwGdnCheckpointWitnessV2 { leaf: leaf.clone(), opening, chunks };
        authenticate_anchor_v2(binding, &target, start, Some(&witness), max_leaves, sliced.then_some(target.tile_index))?;
        Some(witness)
    } else {
        None
    };
    let value = PalwGdnStepRefutationV2 {
        version: PALW_GDN_STEP_EVIDENCE_VERSION_V2,
        step: PalwExecutionStepRefutationV1 {
            binding: binding.clone(),
            output_opening,
            output_preimage: output.clone(),
            inputs,
            prompt_token_ids: Vec::new(),
            decode_tokens: None,
            kv_checkpoint: None,
        },
        checkpoint,
    };
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
}
