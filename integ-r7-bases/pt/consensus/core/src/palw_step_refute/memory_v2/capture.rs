//! **Evidence assembly for the version-2 recurrence court, from material a holder already has** — the producer's own capture or a
//! challenger's honest re-execution of the committed job.
//!
//! The assembler derives every index the checker derives — the exact predecessor checkpoint ([`gdn_replay_start_v2`]), the
//! bounded replay window (`canonical_input_leaves_with_start_v2`), the selected layer/head's delta chunks and their position in the
//! class's flat or held state tree — from the claim's binding alone. Nothing it emits is a choice: a holder that assembled a
//! different set would be refused by the checker, which re-derives the same values. The output authenticates data membership only;
//! the verdict is the deterministic replay over it, and nothing here confers validator approval or consensus credit.
use super::*;

/// **Where a class's checkpoint at `positions` keeps its recurrence half, and how many chunks the whole checkpoint has** —
/// `(offset of the first recurrence chunk, total chunk count, held layout if the map is held)`. The one spelling the assembler and
/// the checkpoint-transition / convolution-window terminals ([`super::transition`]) share; it reads the same functions
/// `authenticate_anchor_v2` reads (the held layout, the v3 composition at its covered cadence, the solo enumeration).
pub(crate) fn gdn_recurrence_section_v2(
    profile: &crate::palw_step::PalwShapeProfileV3,
    positions: u32,
) -> Result<(u64, u64, Option<maps::PalwStateLayoutV4>), PalwGdnCourtErrorV2> {
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    if maps::palw_profile_is_held_v4(profile) {
        let layout =
            maps::palw_state_layout_v4(profile, positions).map_err(|_| evidence("held checkpoint has no recurrence layout"))?;
        if layout.gdn_layers.is_empty() {
            return Err(evidence("this held checkpoint does not carry recurrence state"));
        }
        let offset = layout.attn.chunk_count();
        let count = layout.chunk_count();
        return Ok((offset, count, Some(layout)));
    }
    if profile.state_chunk_map_id == maps::hybrid_state_chunk_map_id_v3() {
        let layout = maps::hybrid_state_geometry_for_covered_v1(profile, positions)
            .map_err(|_| evidence("hybrid checkpoint has no recurrence layout"))?;
        if layout.gdn_layers.is_empty() {
            return Err(evidence("this hybrid checkpoint does not carry recurrence state"));
        }
        return Ok((layout.attn.chunk_count(), layout.chunk_count(), None));
    }
    if let Some(attention) = maps::hybrid_composed_attention_chunks_v2(profile, positions) {
        // RFC-0006 consumer v2's NEW composed maps: the integer-kv v2 attention half, then the recurrence enumeration, one flat
        // tree. Historical hybrid v1/v2 ids never reach here (they carry no recurrence in their roots).
        let attention = attention.map_err(|_| evidence("composed checkpoint's attention half is not derivable"))?;
        let count = attention.checked_add(geometry.chunk_count()).ok_or(PalwStepRefuteError::Unadjudicable)?;
        if count > leg::PALW_STEP_LEG_MAX_STATE_CHUNKS as u64 {
            return Err(evidence("composed checkpoint exceeds the flat chunk-count cap"));
        }
        return Ok((attention, count, None));
    }
    Ok((0, geometry.chunk_count(), None))
}

/// **The selected recurrence state chunks' flat indices**, ascending — every delta block of `head` (or of every head when `None`,
/// the float whole-row core) at `layer`, offset into the checkpoint. The order `authenticate_anchor_v2` walks.
pub(crate) fn gdn_selected_delta_chunks_v2(
    profile: &crate::palw_step::PalwShapeProfileV3,
    positions: u32,
    layer: u16,
    head: Option<u32>,
) -> Result<Vec<u32>, PalwGdnCourtErrorV2> {
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    let (offset, _, _) = gdn_recurrence_section_v2(profile, positions)?;
    let heads = head.map_or(0..geometry.heads, |h| h..h + 1);
    let blocks = geometry.v_dim.div_ceil(geometry.delta_rows_per_chunk);
    let mut out = Vec::new();
    for h in heads {
        for block in 0..blocks {
            let local = geometry.delta_chunk_index(layer, h, block).ok_or(PalwStepRefuteError::Unadjudicable)?;
            out.push(offset.checked_add(local).and_then(|i| u32::try_from(i).ok()).ok_or(PalwStepRefuteError::Unadjudicable)?);
        }
    }
    Ok(out)
}

/// **One chunk's membership witness, from the checkpoint's whole chunk list** — its bytes and its path under the class's own tree
/// (`palw_state_chunk_path_for_map_v1`: flat for solo / v3 / composed maps, the two-level held tree otherwise).
pub(crate) fn gdn_chunk_witness_v2(
    profile: &crate::palw_step::PalwShapeProfileV3,
    positions: u32,
    chunks: &[Vec<u8>],
    flat: u32,
) -> Result<PalwGdnStateChunkWitnessV2, PalwGdnCourtErrorV2> {
    let bytes = chunks.get(flat as usize).ok_or_else(|| evidence("the captured checkpoint lacks a selected chunk"))?.clone();
    let siblings = maps::palw_state_chunk_path_for_map_v1(profile, positions, chunks, flat)?;
    Ok(PalwGdnStateChunkWitnessV2 { chunk_index: flat, bytes, siblings })
}

/// The canonical input rows over `required`, each preimage read from the holder's material and each derived run opened against
/// the step tree the binding commits.
pub(crate) fn gdn_input_rows_from_capture_v2(
    required: &[Vec<(u64, PalwStepCoordinateV1)>],
    step_leaf_hashes: &[Hash64],
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    max_leaves: u64,
) -> Result<Vec<PalwStepInputRowV1>, PalwGdnCourtErrorV2> {
    let mut rows = Vec::with_capacity(required.len());
    for row in required {
        let mut preimages = Vec::with_capacity(row.len());
        for (_, coord) in row {
            preimages.push(tile(*coord).ok_or_else(|| evidence("the capture lacks a required input tile"))?);
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
            run_siblings.push(leg::step_merkle_range_siblings_capped_v1(step_leaf_hashes, row[start].0 as usize, len, max_leaves)?);
        }
        rows.push(PalwStepInputRowV1 { preimages, run_siblings });
    }
    Ok(rows)
}

/// What a holder of an execution's committed material supplies.
pub struct PalwGdnCaptureV2<'a> {
    /// The claim's binding (its committed execution root is the claim's).
    pub binding: &'a leg::PalwStepBindingV2,
    /// Every step leaf hash, in canonical order (the tree under `binding.step_merkle_root`).
    pub step_leaf_hashes: &'a [Hash64],
    /// Every checkpoint leaf hash, in index order (the tree under `binding.checkpoint_merkle_root`).
    pub checkpoint_leaf_hashes: &'a [Hash64],
    /// The ruleset's step ladder.
    pub max_leaves: u64,
}

/// **Assemble [`PalwGdnStepRefutationV2`] for the recurrence step at `coord`** from captured material: `tile` answers a committed
/// step tile by coordinate, `checkpoint` answers a committed checkpoint's leaf and its WHOLE chunk list by index (what the producer's
/// capture holds; the evidence carries only the selected chunks and their paths).
///
/// The output is the same object the checker re-derives; on honest material a holder's own court terminal therefore answers
/// `NoFaultFound`, and on a committed wrong row it convicts. Refused (never approximated) when the material lacks a tile, a
/// checkpoint or a selected chunk.
pub fn gdn_refutation_from_capture_v2(
    capture: &PalwGdnCaptureV2<'_>,
    coord: PalwStepCoordinateV1,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    checkpoint: &dyn Fn(u32) -> Option<(leg::PalwCheckpointLeafV2, Vec<Vec<u8>>)>,
) -> Result<PalwGdnStepRefutationV2, PalwGdnCourtErrorV2> {
    let binding = capture.binding;
    let profile = &binding.shape_profile;
    let (node, layer) = profile.resolve_node_slot(coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let layer = layer.ok_or(PalwStepRefuteError::Unadjudicable)?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let sliced = match program {
        KernelProgram::GdnCore { .. } => false,
        KernelProgram::Qwen36(Qwen36Op::GdnStep) => true,
        _ => return Err(PalwStepRefuteError::Unadjudicable.into()),
    };
    let start = gdn_replay_start_v2(binding, &coord)?;
    let required = canonical_input_leaves_with_start_v2(
        profile,
        &binding.job_context,
        coord.node_slot,
        &coord,
        program,
        false,
        Some(start.first_position),
        None,
    )
    .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let inputs = gdn_input_rows_from_capture_v2(&required, capture.step_leaf_hashes, tile, capture.max_leaves)?;
    let out_index = canonical_step_leaf_index(profile, &binding.job_context, &coord).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let output_opening = leg::step_opening_capped_v1(capture.step_leaf_hashes, out_index, capture.max_leaves)?;
    let output_preimage = tile(coord).ok_or_else(|| evidence("the capture lacks the challenged output tile"))?;
    let checkpoint = match start.checkpoint_index {
        None => None,
        Some(index) => {
            let (leaf, chunks) = checkpoint(index).ok_or_else(|| evidence("the capture lacks the predecessor checkpoint"))?;
            let opening = leg::step_opening_capped_v1(capture.checkpoint_leaf_hashes, u64::from(index), capture.max_leaves)?;
            let selected = gdn_selected_delta_chunks_v2(
                profile,
                start.first_position,
                layer,
                if sliced { Some(coord.tile_index) } else { None },
            )?;
            let mut witnesses = Vec::with_capacity(selected.len());
            for flat in selected {
                witnesses.push(gdn_chunk_witness_v2(profile, start.first_position, &chunks, flat)?);
            }
            Some(PalwGdnCheckpointWitnessV2 { leaf, opening, chunks: witnesses })
        }
    };
    Ok(PalwGdnStepRefutationV2 {
        version: PALW_GDN_STEP_EVIDENCE_VERSION_V2,
        step: PalwExecutionStepRefutationV1 {
            binding: binding.clone(),
            output_opening,
            output_preimage,
            inputs,
            prompt_token_ids: Vec::new(),
            decode_tokens: None,
            kv_checkpoint: None,
        },
        checkpoint,
    })
}
