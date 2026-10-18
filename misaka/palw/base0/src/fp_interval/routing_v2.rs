//! Gather the actual routed reader's rows from a sparse fold. This retains
//! only the requested tiles and their boundary blocks; it never materializes
//! the whole execution or substitutes the historical implicit V1 selector.
use super::*;
use kaspa_consensus_core::{
    palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E,
    palw_native_resource_v1::{NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_reader_router_v2::PalwReaderRouterDeclarationV2,
    palw_routed_material_v2::{PalwHeldMaterialUnitV2, PalwMaterialConstructionRequestsV2, construct_material_unit_v2},
    palw_step::canonical_step_coordinates,
    palw_step_leg::PalwStepOpeningV1,
    palw_step_refute::{
        PalwExecutionStepRefutationV1, PalwStepInputRowV1,
        routing_v2::{
            PalwRoutedStepRefutationV2, PalwRoutingCourtErrorV2, canonical_reader_inputs_v2, check_routed_step_committed_half_v2,
        },
    },
};

fn routing_error(e: PalwRoutingCourtErrorV2) -> E {
    match e {
        PalwRoutingCourtErrorV2::Resource(e) => E::LocalResource(e),
        other => E::Authentication(other.to_string()),
    }
}

/// The caller holds the actual decoded fold and a separately admitted native
/// runtime working set for `kernels`. This function admits all coexisting
/// coordinates, tile copies, retained boundary blocks and proof construction
/// before replay. No persistent anchor memo is used: a missing chunk resumes
/// from the actual prompt. Parameter rows are not owed for data retrieval.
#[allow(clippy::too_many_arguments)]
pub fn routed_unit_from_fold_v2<K: Base0FpIntervalKernelsV1>(
    material: &crate::produce::Base0FpMaterialV2,
    leaf: u64,
    prompt: &[u32],
    family_checkpoint_interval: u32,
    maximum_leaves: u64,
    kernels: &K,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    declaration: &PalwReaderRouterDeclarationV2,
) -> Result<NativeRetainedV1<PalwHeldMaterialUnitV2>, E> {
    let b = &material.binding;
    if b.step_leaf_count == 0 || b.step_leaf_count > maximum_leaves || leaf >= b.step_leaf_count {
        return Err(E::Incomplete);
    }
    let mut coordinates = PalwMaterialConstructionRequestsV2::new();
    coordinates.vec_v1::<(u128, u128, u128)>(u64::from(b.shape_profile.global_node_count())).map_err(E::LocalResource)?;
    coordinates.vec_v1::<(u128, u128, u128)>(u64::from(b.shape_profile.global_node_count())).map_err(E::LocalResource)?;
    let _coordinates = reserve_native_requests_v1(coordinates.bytes_v1()).map_err(E::LocalResource)?;
    let target = canonical_step_coordinates(&b.shape_profile, &b.job_context, leaf).ok_or(E::Incomplete)?;
    let required = canonical_reader_inputs_v2(&b.shape_profile, &b.job_context, &target, declaration)
        .map_err(PalwRoutingCourtErrorV2::from)
        .map_err(routing_error)?;
    let mut count = 1u64;
    let mut words = u64::from(
        kaspa_consensus_core::palw_step_leg::palw_expected_tile_values_v1(&b.shape_profile, &b.job_context, &target)
            .ok_or(E::Incomplete)?,
    );
    for row in required.iter() {
        count = count.checked_add(row.len() as u64).ok_or(E::Capacity)?;
        for (_, coord) in row {
            words = words
                .checked_add(u64::from(
                    kaspa_consensus_core::palw_step_leg::palw_expected_tile_values_v1(&b.shape_profile, &b.job_context, coord)
                        .ok_or(E::Incomplete)?,
                ))
                .ok_or(E::Capacity)?;
        }
    }
    let tree = &material.step_tree;
    // Validate before a peer's retain_level can become a shift or allocation.
    tree.validate_v1().map_err(|e| E::Authentication(e.to_string()))?;
    let block = 1u64.checked_shl(tree.retain_level()).ok_or(E::Capacity)?.min(b.step_leaf_count);
    let depth = u64::from(64 - b.step_leaf_count.leading_zeros());
    let mut plan = PalwMaterialConstructionRequestsV2::new();
    plan.vec_v1::<u64>(count.checked_mul(2).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    let _plan = reserve_native_requests_v1(plan.bytes_v1()).map_err(E::LocalResource)?;
    let mut needed: Vec<u64> = Vec::with_capacity(usize::try_from(count).map_err(|_| E::Capacity)?);
    needed.push(leaf);
    for row in required.iter() {
        needed.extend(row.iter().map(|(at, _)| *at));
    }
    needed.sort_unstable();
    needed.dedup();
    let mut blocks: Vec<u64> = needed.iter().map(|at| at / block).collect();
    blocks.dedup();
    let boundary_hashes = blocks
        .iter()
        .try_fold(0u64, |sum, index| {
            let first = index.checked_mul(block)?;
            let width = b.step_leaf_count.checked_sub(first)?.min(block);
            sum.checked_add(width)
        })
        .ok_or(E::Capacity)?;
    let mut q = PalwMaterialConstructionRequestsV2::new();
    let binding_heap = b.native_heap_v1("routed fold binding copy");
    q.add_v1(binding_heap.bytes().ok_or_else(|| E::LocalResource(binding_heap))?).map_err(E::LocalResource)?;
    // One needed-index list, its unique block list, indexed retained tiles,
    // indexed block vectors, current replay block and fold/proof scratch.
    q.vec_v1::<u64>(count.checked_mul(3).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    q.vec_v1::<(u64, PalwStepTileLeafV1)>(count).map_err(E::LocalResource)?;
    q.vec_v1::<(u64, Vec<Hash64>)>(blocks.len() as u64).map_err(E::LocalResource)?;
    q.vec_v1::<Hash64>(boundary_hashes).map_err(E::LocalResource)?;
    q.vec_v1::<Hash64>(block.checked_mul(8).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    q.vec_v1::<Hash64>((tree.retained_len() as u64).checked_mul(4).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    q.vec_v1::<PalwStepTileLeafV1>(count.checked_mul(2).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    q.vec_v1::<u8>(words.checked_mul(12).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    q.vec_v1::<PalwStepInputRowV1>(required.len() as u64).map_err(E::LocalResource)?;
    q.vec_v1::<Vec<Hash64>>(count).map_err(E::LocalResource)?;
    q.vec_v1::<Hash64>(count.checked_mul(2 * depth).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    q.vec_v1::<(u64, &[Hash64])>(2).map_err(E::LocalResource)?;
    q.vec_v1::<(u64, Vec<Vec<Hash64>>)>(2).map_err(E::LocalResource)?;
    q.vec_v1::<Vec<Hash64>>(2 * (u64::from(tree.retain_level()) + 1) + depth).map_err(E::LocalResource)?;
    q.vec_v1::<(u64, u64)>(count).map_err(E::LocalResource)?;
    q.vec_v1::<usize>(prompt.len() as u64).map_err(E::LocalResource)?;
    // Resume chunks and geometry/checkpoint construction may temporarily copy
    // the actual source fields. The source owner itself remains independently
    // retained by the caller throughout complete-bundle construction.
    let source_heap = material.native_heap_v1("routed fold resume fields");
    q.add_v1(source_heap.bytes().ok_or_else(|| E::LocalResource(source_heap))?.checked_mul(3).ok_or(E::Capacity)?)
        .map_err(E::LocalResource)?;
    q.vec_v1::<u8>((borsh::object_length(&b.shape_profile).map_err(|_| E::Capacity)? as u64).max(1024)).map_err(E::LocalResource)?;
    q.vec_v1::<u8>((754 + b.job_context.network_id.len() as u64).max(1024)).map_err(E::LocalResource)?;
    construct_material_unit_v2(q, || {
        if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(form, prompt, &b.job_context.prompt_token_ids_hash) {
            return Err(E::Substitution);
        }
        let leaves = base0_fp_binding_step_space_v1(b, maximum_leaves).map_err(|e| E::Authentication(e.to_string()))?;
        let geometry = Base0FpIntervalGeometryV1::from_binding_capped_v1(b, family_checkpoint_interval, maximum_leaves)
            .map_err(|e| E::Authentication(e.to_string()))?;
        if tree.leaf_count() != leaves || tree.root().map_err(|e| E::Authentication(e.to_string()))? != b.step_merkle_root {
            return Err(E::Substitution);
        }
        let first = blocks.first().copied().ok_or(E::Incomplete)?.checked_mul(block).ok_or(E::Capacity)?;
        let end = blocks
            .last()
            .copied()
            .ok_or(E::Incomplete)?
            .checked_add(1)
            .and_then(|v| v.checked_mul(block))
            .ok_or(E::Capacity)?
            .min(leaves);
        let mut tiles: Vec<(u64, PalwStepTileLeafV1)> = Vec::with_capacity(needed.len());
        let mut retained_blocks: Vec<(u64, Vec<Hash64>)> = Vec::with_capacity(blocks.len());
        let mut current = Vec::with_capacity(block as usize);
        let mut diverged = false;
        base0_replay_span_into_v1(
            kernels,
            b,
            &material.checkpoint_chunks,
            &material.generated_token_ids,
            prompt,
            &geometry,
            first,
            end,
            &|_| None,
            &mut |index, hash, tile| {
                if needed.binary_search(&index).is_ok() {
                    let coord = canonical_step_coordinates(&b.shape_profile, &b.job_context, index)
                        .ok_or_else(|| "routed replay emitted a noncanonical index".to_string())?;
                    let width =
                        kaspa_consensus_core::palw_step_leg::palw_expected_tile_values_v1(&b.shape_profile, &b.job_context, &coord)
                            .ok_or_else(|| "routed replay emitted an unbounded tile".to_string())?;
                    if tile.coord != coord || tile.values_le.len() as u64 != u64::from(width) * 4 || tile.value_count != width {
                        return Err("routed replay emitted a tile outside its admitted geometry".to_string());
                    }
                    tiles.push((index, tile.clone()));
                }
                current.push(hash);
                let base = (index / block) * block;
                if index + 1 == (base + block).min(leaves) {
                    let finished = std::mem::take(&mut current);
                    if tree.retained_nodes().get((base / block) as usize).copied()
                        != crate::fp_capture::base0_fold_block_digest_v1(base, &finished)
                    {
                        diverged = true;
                    }
                    if blocks.binary_search(&(base / block)).is_ok() {
                        retained_blocks.push((base, finished));
                    }
                }
                Ok(())
            },
        )
        .map_err(|e| E::Authentication(e.to_string()))?;
        if diverged {
            return Err(E::Substitution);
        }
        let tile = |at| tiles.binary_search_by_key(&at, |(i, _)| *i).ok().map(|i| &tiles[i].1).ok_or(E::Incomplete);
        let siblings = |first: u64, len: u64| -> Result<Vec<Hash64>, E> {
            let last = first.checked_add(len).and_then(|v| v.checked_sub(1)).ok_or(E::Capacity)?;
            let mut edges = Vec::with_capacity(2);
            for base in [(first / block) * block, (last / block) * block] {
                if edges.iter().any(|(at, _)| *at == base) {
                    continue;
                }
                let pos = retained_blocks.binary_search_by_key(&base, |(at, _)| *at).map_err(|_| E::Incomplete)?;
                edges.push((base, retained_blocks[pos].1.as_slice()));
            }
            tree.range_siblings_from_edges_v1(&edges, first, len).map_err(|e| E::Authentication(e.to_string()))
        };
        let output = tile(leaf)?;
        let output_opening = PalwStepOpeningV1 {
            leaf_index: leaf,
            leaf_hash: crate::step_tile_hasher::StepTileHasherV1::new(&b.job_context, b.shape_profile.shape_profile_id()).hash(output),
            siblings: siblings(leaf, 1)?,
        };
        let mut inputs = Vec::with_capacity(required.len());
        for row in required.iter() {
            let mut preimages = Vec::with_capacity(row.len());
            let mut runs: Vec<(u64, u64)> = Vec::new();
            for (at, _) in row {
                preimages.push(tile(*at)?.clone());
                match runs.last_mut() {
                    Some((from, n)) if *from + *n == *at => *n += 1,
                    _ => runs.push((*at, 1)),
                }
            }
            let mut run_siblings = Vec::with_capacity(runs.len());
            for (from, n) in runs {
                run_siblings.push(siblings(from, n)?);
            }
            inputs.push(PalwStepInputRowV1 { preimages, run_siblings });
        }
        let r = PalwRoutedStepRefutationV2 {
            version: 2,
            step: PalwExecutionStepRefutationV1 {
                binding: b.clone(),
                output_opening,
                output_preimage: output.clone(),
                inputs,
                prompt_token_ids: Vec::new(),
                decode_tokens: None,
                kv_checkpoint: None,
            },
        };
        check_routed_step_committed_half_v2(&r, b.committed_execution_root, declaration, maximum_leaves).map_err(routing_error)?;
        Ok(PalwHeldMaterialUnitV2::Routed(r))
    })
}
