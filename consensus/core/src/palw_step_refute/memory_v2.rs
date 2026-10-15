//! Recurrence state is an authenticated operand, never a computation argument.
//! A checkpoint opening establishes only the bytes the executor committed.
//! The court directly recomputes the subsequent bounded recurrence transitions.
//! A matching replay is not validator approval and confers no consensus credit.
//!
//! Native carrier/fence, provider/capture and registration coverage integration
//! are separate consumers. V1 evidence dispatch and public presets are unchanged.
use super::*;
use crate::palw_context_ladder as cadence;
use crate::palw_gdn_state_map_v2::PalwGdnStateGeometryV2;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_state_chunk_map as maps;
use crate::palw_step_leg as leg;

mod capture;
pub use capture::{PalwGdnCaptureV2, gdn_refutation_from_capture_v2};
// RFC-0006 consumer v2: the checkpoint's own two facts (delta transition, convolution window), the court that keeps an anchored
// replay sound — see the module's docs.
mod transition;
pub use transition::{
    PALW_GDN_CHECKPOINT_EVIDENCE_VERSION_V2, PALW_GDN_CONV_WINDOW_EVIDENCE_KIND_V2, PALW_GDN_TRANSITION_EVIDENCE_KIND_V2,
    PalwGdnCheckpointVerdictV2, PalwGdnConvWindowRefutationV2, PalwGdnMismatchV2, PalwGdnTransitionRefutationV2,
    check_gdn_conv_window_refutation_v2, check_gdn_transition_refutation_v2, gdn_conv_window_refutation_from_capture_v2,
    gdn_conv_window_required_v2, gdn_transition_refutation_from_capture_v2,
};

pub const PALW_GDN_STEP_EVIDENCE_VERSION_V2: u16 = 2;

/// Merkle membership of existing state bytes. This is not a proof of the
/// correctness of any model computation. Index/address/count are rederived.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGdnStateChunkWitnessV2 {
    pub chunk_index: u32,
    pub bytes: Vec<u8>,
    /// Flat path, or held block path followed by held top path.
    pub siblings: Vec<Hash64>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGdnCheckpointWitnessV2 {
    pub leaf: leg::PalwCheckpointLeafV2,
    pub opening: PalwStepOpeningV1,
    /// Exactly the challenged layer/head's delta chunks, ascending by map index.
    /// A float whole-row recurrence carries every head; a sliced Q36 leaf one.
    pub chunks: Vec<PalwGdnStateChunkWitnessV2>,
}

/// Additive versioned evidence. Its inner V1 has no KV anchor: recurrence and KV
/// anchoring answer different operand questions and cannot substitute for one
/// another. The output and step inputs retain their existing committed encoding.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGdnStepRefutationV2 {
    pub version: u16,
    pub step: PalwExecutionStepRefutationV1,
    pub checkpoint: Option<PalwGdnCheckpointWitnessV2>,
}

#[derive(Debug, thiserror::Error)]
pub enum PalwGdnCourtErrorV2 {
    #[error("local recurrence court capacity unavailable: {0:?}")]
    Resource(NativeCoverageV1),
    #[error("recurrence evidence: {0}")]
    Evidence(#[from] PalwStepRefuteError),
}
impl From<leg::PalwStepLegError> for PalwGdnCourtErrorV2 {
    fn from(value: leg::PalwStepLegError) -> Self {
        Self::Evidence(value.into())
    }
}
impl From<NativeCoverageV1> for PalwGdnCourtErrorV2 {
    fn from(value: NativeCoverageV1) -> Self {
        Self::Resource(value)
    }
}
fn evidence(message: &'static str) -> PalwGdnCourtErrorV2 {
    PalwStepRefuteError::InputSetNotCanonical(message).into()
}

/// Builder/checker share the exact start and checkpoint index. A missing index
/// means genesis zero state, never an unauthenticated caller-supplied state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGdnReplayStartV2 {
    pub first_position: u32,
    pub checkpoint_index: Option<u32>,
    pub covered: Option<u32>,
}

/// Latest committed recurrence checkpoint STRICTLY BEFORE this step. KV's
/// per-position anchor is after the current cache write, so its helper must not
/// be reused for recurrence: doing so would feed the answer back as its input.
pub fn gdn_replay_start_v2(
    binding: &leg::PalwStepBindingV2,
    coord: &PalwStepCoordinateV1,
) -> Result<PalwGdnReplayStartV2, PalwGdnCourtErrorV2> {
    let p = &binding.shape_profile;
    let id = p.state_chunk_map_id;
    // Legacy hybrid1/2 name a composition but do not serialize its recurrence.
    // Solo key-head v3 likewise has no production capture consumer yet.
    // RFC-0006 consumer v2's composed hybrid maps (new ids) DO serialize both halves (`capture::gdn_recurrence_section_v2`).
    if id != maps::gdn_state_chunk_map_id_v1()
        && id != maps::gdn_state_chunk_map_id_v2()
        && id != maps::hybrid_state_chunk_map_id_v3()
        && !maps::palw_map_is_hybrid_composed_v2(&id)
        && !maps::palw_profile_is_held_v4(p)
    {
        return Err(evidence("this map has no authentic recurrence capture consumer"));
    }
    // Held attention-only maps are not recurrence maps. Geometry is borrowed
    // and allocation-free, including when the replay starts at genesis.
    PalwGdnStateGeometryV2::of(p).map_err(evidence)?;
    let (_, layer) = p.resolve_node_slot(coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if layer.is_none_or(|layer| p.layer_kind(layer) != crate::palw_step::PalwLayerKindV1::GatedDeltaNet) {
        return Err(evidence("recurrence coordinate does not belong to a recurrence layer"));
    }
    canonical_step_leaf_index(p, &binding.job_context, coord).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let here = cadence::palw_absolute_position_v1(&binding.job_context, coord.call_index, coord.position)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let (first_position, covered, index) = match cadence::palw_checkpoint_cadence_v1(p) {
        cadence::PalwCheckpointCadenceV1::PerPosition => {
            let interval = cadence::palw_anchored_interval_for_profile_v1(p);
            if interval == 0 {
                return Err(evidence("zero recurrence interval"));
            }
            let first = here / interval * interval;
            (first, first, first.checked_sub(1))
        }
        cadence::PalwCheckpointCadenceV1::PerDecodeCall => {
            let interval = binding.checkpoint_profile.checkpoint_interval;
            if interval == 0 {
                return Err(evidence("zero checkpoint interval"));
            }
            let covered = here.saturating_sub(binding.job_context.declared_prefill_tokens) / interval * interval;
            if covered == 0 {
                (0, 0, None)
            } else {
                let first =
                    binding.job_context.declared_prefill_tokens.checked_add(covered).ok_or(PalwStepRefuteError::Unadjudicable)?;
                (first, covered, Some(covered / interval - 1))
            }
        }
    };
    Ok(PalwGdnReplayStartV2 { first_position, checkpoint_index: index, covered: index.map(|_| covered) })
}

struct VerifiedGdnAnchorV2<'a> {
    chunks: &'a [PalwGdnStateChunkWitnessV2],
    geometry: PalwGdnStateGeometryV2<'a>,
    head: Option<u32>,
}

fn authenticate_anchor_v2<'a>(
    binding: &'a leg::PalwStepBindingV2,
    coord: &PalwStepCoordinateV1,
    start: PalwGdnReplayStartV2,
    supplied: Option<&'a PalwGdnCheckpointWitnessV2>,
    max_leaves: u64,
    head: Option<u32>,
) -> Result<Option<VerifiedGdnAnchorV2<'a>>, PalwGdnCourtErrorV2> {
    let Some(want_index) = start.checkpoint_index else {
        if supplied.is_some() {
            return Err(evidence("genesis replay must not carry a checkpoint"));
        }
        return Ok(None);
    };
    let supplied = supplied.ok_or_else(|| evidence("the latest recurrence checkpoint is missing"))?;
    let leaf = &supplied.leaf;
    if leaf.version != leg::PALW_STEP_LEG_OBJECT_VERSION_V1
        || leaf.checkpoint_index != want_index
        || supplied.opening.leaf_index != u64::from(want_index)
        || Some(leaf.covered_decode_call) != start.covered
        || cadence::palw_checkpoint_covered_at_index_v1(
            &binding.shape_profile,
            want_index,
            binding.checkpoint_profile.checkpoint_interval,
        ) != start.covered
    {
        return Err(evidence("checkpoint index/cadence is not the exact predecessor"));
    }
    let context_hash = binding.job_context.context_hash();
    if want_index == 0 && leaf.prev_checkpoint_leaf_hash != leg::checkpoint_genesis_prev_v2(&context_hash) {
        return Err(evidence("first recurrence checkpoint has another genesis predecessor"));
    }
    let hash =
        leg::checkpoint_leaf_hash_v2(&context_hash, &binding.checkpoint_profile.profile_hash(), &binding.state_chunk_map_id, leaf);
    if hash != supplied.opening.leaf_hash
        || leg::step_opening_root_capped_v1(u64::from(binding.checkpoint_count), &supplied.opening, max_leaves)?
            != binding.checkpoint_merkle_root
    {
        return Err(evidence("recurrence checkpoint is not in the committed execution"));
    }
    let geometry = PalwGdnStateGeometryV2::of(&binding.shape_profile).map_err(evidence)?;
    if head.is_some_and(|h| h >= geometry.heads) {
        return Err(evidence("challenged recurrence head is outside the class"));
    }
    let (_, layer) = binding.shape_profile.resolve_node_slot(coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let layer = layer.ok_or(PalwStepRefuteError::Unadjudicable)?;
    // One spelling of "where the recurrence half lives" (held, v3 composition, consumer v2's composed maps, solo), shared with the
    // assembler and the checkpoint terminals.
    let (offset, expected_count, layout) = capture::gdn_recurrence_section_v2(&binding.shape_profile, start.first_position)?;
    if expected_count != u64::from(leaf.state_chunk_count) {
        return Err(evidence("checkpoint count differs from its registered state layout"));
    }
    let mut supplied_chunks = supplied.chunks.iter();
    let heads = head.map_or(0..geometry.heads, |h| h..h + 1);
    let blocks = geometry.v_dim.div_ceil(geometry.delta_rows_per_chunk);
    for local in heads.flat_map(|h| (0..blocks).map(move |block| (h, block))) {
        let local = geometry.delta_chunk_index(layer, local.0, local.1).ok_or(PalwStepRefuteError::Unadjudicable)?;
        let entry = geometry.entry(local).ok_or(PalwStepRefuteError::Unadjudicable)?;
        let witness = supplied_chunks.next().ok_or_else(|| evidence("a required recurrence state chunk is missing"))?;
        let flat = offset.checked_add(local).and_then(|i| u32::try_from(i).ok()).ok_or(PalwStepRefuteError::Unadjudicable)?;
        if witness.chunk_index != flat || witness.bytes.len() as u64 != entry.byte_len() {
            return Err(evidence("recurrence chunk index or length differs from the selected layer/head"));
        }
        let root = if let Some(layout) = &layout {
            let address = layout.address(u64::from(flat)).ok_or(PalwStepRefuteError::Unadjudicable)?;
            let hash = leg::state_chunk_leaf_hash_v4(&binding.state_chunk_map_id, address.slice, address.block, &witness.bytes);
            leg::state_chunk_opening_root_v4(
                address.slice_count,
                address.slice,
                address.block_count,
                address.block,
                &hash,
                &witness.siblings,
            )?
        } else {
            let hash = leg::state_chunk_leaf_hash_v1(&binding.state_chunk_map_id, flat, &witness.bytes);
            leg::state_chunk_opening_root_v1(leaf.state_chunk_count as usize, flat, &hash, &witness.siblings)?
        };
        if root != leaf.state_chunks_root {
            return Err(evidence("recurrence state bytes are not members of this checkpoint"));
        }
    }
    if supplied_chunks.next().is_some() {
        return Err(evidence("extra or duplicate recurrence state chunks"));
    }
    Ok(Some(VerifiedGdnAnchorV2 { chunks: &supplied.chunks, geometry, head }))
}

impl VerifiedGdnAnchorV2<'_> {
    fn delta_rows(&self) -> Vec<Vec<u32>> {
        let count = if self.head.is_some() { 1 } else { self.geometry.heads };
        let per_head = self.geometry.v_dim.div_ceil(self.geometry.delta_rows_per_chunk) as usize;
        self.chunks
            .chunks(per_head)
            .take(count as usize)
            .map(|chunks| {
                chunks
                    .iter()
                    .flat_map(|chunk| {
                        chunk.bytes.chunks_exact(4).map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte lanes")))
                    })
                    .collect()
            })
            .collect()
    }
}

fn reserve_workspace_v2(
    r: &PalwGdnStepRefutationV2,
    start: PalwGdnReplayStartV2,
    coord: &PalwStepCoordinateV1,
    sliced: bool,
) -> Result<Option<NativeDemandLeaseV1>, PalwGdnCourtErrorV2> {
    let binding = &r.step.binding;
    let p = &binding.shape_profile;
    let (node, _) = p.resolve_node_slot(coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if node.input_refs.len() != 5 {
        return Err(evidence("recurrence must have five registered input references"));
    }
    let here = cadence::palw_absolute_position_v1(&binding.job_context, coord.call_index, coord.position)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let positions = u64::from(here - start.first_position) + 1;
    let rows = TirNativeRequestsV1::product_v1(positions, 5)?;
    let first_slot = coord.node_slot - intra_table_index(p, coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)? as u32;
    let mut q = TirNativeRequestsV1::new();
    let repeated_vectors = |q: &mut TirNativeRequestsV1, count: u64, elements: u64| {
        let mut one = TirNativeRequestsV1::new();
        one.vec_v1::<u32>(elements)?;
        q.add_v1(TirNativeRequestsV1::product_v1(count, one.bytes_v1())?)
    };
    // Structural output checker clones these actual records. Binding hash
    // serializers simultaneously hold canonical buffers, counted by their
    // exact serialization length rather than by a native/codec multiplier.
    q.add_v1(crate::palw_native_resource_v1::native_value_requests_v1(binding)?)?;
    q.add_v1(crate::palw_native_resource_v1::native_value_requests_v1(&r.step.output_opening)?)?;
    q.add_v1(crate::palw_native_resource_v1::native_value_requests_v1(&r.step.output_preimage)?)?;
    q.vec_v1::<u8>(
        (borsh::object_length(&binding.shape_profile).map_err(|_| evidence("cannot measure shape profile"))? as u64).max(1024),
    )?;
    // The context CanonicalWriter starts at 512 bytes and writes 754 fixed
    // bytes plus network_id. Other fixed leg writers start at 256 bytes.
    q.vec_v1::<u8>((754 + binding.job_context.network_id.len() as u64).max(1024))?;
    q.vec_v1::<u8>(512)?;
    // One required-position vector, canonical rows, decoded input rows and
    // the float kernel's narrowed rows coexist. Every source is fixed-width.
    q.vec_v1::<(u32, u32)>(positions)?;
    q.vec_v1::<Vec<(u64, PalwStepCoordinateV1)>>(rows)?;
    q.exact_v1::<Vec<u32>>(rows)?;
    q.vec_v1::<Vec<u32>>(rows)?;
    q.vec_v1::<u64>(5)?;
    for &reference in &node.input_refs {
        if reference >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
            return Err(evidence("recurrence V2 requires actual registered step input references"));
        }
        let (source, _) = p.resolve_node_slot(first_slot + u32::from(reference)).ok_or(PalwStepRefuteError::Unadjudicable)?;
        let crate::palw_step::PalwStepOutLenV1::Fixed { elements } = source.out_len else {
            return Err(evidence("recurrence input row is not fixed-width"));
        };
        if source.tile_len == 0 {
            return Err(evidence("zero source tile length"));
        }
        let tiles = u64::from(elements).div_ceil(u64::from(source.tile_len));
        // Full source rows conservatively cover the Q36 selected-head tiles,
        // including their boundary-tile overlap. No unknown-width fallback.
        let lanes = TirNativeRequestsV1::product_v1(tiles, u64::from(source.tile_len))?;
        let mut one_row = TirNativeRequestsV1::new();
        one_row.vec_v1::<(u64, PalwStepCoordinateV1)>(tiles)?;
        q.add_v1(TirNativeRequestsV1::product_v1(positions, one_row.bytes_v1())?)?;
        repeated_vectors(&mut q, positions, lanes)?;
        repeated_vectors(&mut q, positions, lanes)?;
        // One current row's leaf hashes, derived runs and current range-opening
        // clone. All earlier decoded rows remain covered above.
        q.exact_v1::<Hash64>(tiles)?;
        q.vec_v1::<(usize, usize)>(tiles)?;
        q.exact_v1::<Hash64>(tiles)?;
        q.exact_v1::<Hash64>(2 * u64::from(64 - binding.step_leaf_count.leading_zeros()))?;
        q.vec_v1::<u8>((150 + u64::from(source.tile_len) * 4).max(256))?;
        q.vec_v1::<(u64, u64)>(1)?;
    }
    // Map geometry owns attention/recurrence layer lists, never state bytes.
    q.vec_v1::<u16>(u64::from(p.layer_count))?;
    q.vec_v1::<u16>(u64::from(p.layer_count))?;
    let heads = if sliced { 1 } else { u64::from(p.gdn_heads) };
    let per_head = TirNativeRequestsV1::product_v1(u64::from(p.gdn_head_k_dim), u64::from(p.gdn_head_v_dim))?;
    q.vec_v1::<Vec<u32>>(heads)?;
    q.vec_v1::<Vec<u32>>(heads)?;
    repeated_vectors(&mut q, heads, per_head)?; // authenticated delta decoding
    repeated_vectors(&mut q, heads, per_head)?; // mutable deterministic replay state
    q.vec_v1::<u32>(TirNativeRequestsV1::product_v1(heads, u64::from(p.gdn_head_v_dim))?)?;
    if let Some(checkpoint) = &r.checkpoint {
        // One membership leaf is hashed at a time. Its writer has the explicit
        // map/index/length framing plus the exact selected chunk bytes.
        let maximum = checkpoint.chunks.iter().map(|chunk| chunk.bytes.len() as u64).max().unwrap_or(0);
        if maximum > leg::PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 {
            return Err(evidence("recurrence chunk exceeds the existing byte cap"));
        }
        q.vec_v1::<u8>((64 + 4 + 4 + 4 + maximum).max(256))?;
    }
    if sliced {
        // The exact four registered Q36 quantization triples and their bytes;
        // per-step extraction holds k,v,q,decay,beta and the output buffers.
        q.exact_v1::<crate::palw_base0_a16::A16QuantParams>(4)?;
        q.exact_v1::<u8>(4 * crate::palw_base0_a16::A16QuantParams::WIRE_BYTES as u64)?;
        q.vec_v1::<u8>(node.weight_name.len() as u64)?;
        q.vec_v1::<i32>(u64::from(p.gdn_head_k_dim))?;
        q.vec_v1::<i32>(u64::from(p.gdn_head_k_dim))?;
        q.vec_v1::<i32>(u64::from(p.gdn_head_v_dim))?;
        q.vec_v1::<i32>(1)?;
        q.vec_v1::<i32>(1)?;
        q.vec_v1::<i32>(u64::from(p.gdn_head_v_dim))?;
        q.vec_v1::<u32>(u64::from(p.gdn_head_v_dim))?;
        q.exact_v1::<i32>(u64::from(p.gdn_head_v_dim))?; // q36_gdn_step's delta u
    }
    reserve_native_requests_v1(q.bytes_v1()).map_err(Into::into)
}

fn verified_input_rows_v2(
    binding: &leg::PalwStepBindingV2,
    supplied: &[PalwStepInputRowV1],
    required: &[Vec<(u64, PalwStepCoordinateV1)>],
    max_leaves: u64,
) -> Result<Vec<Vec<u32>>, PalwGdnCourtErrorV2> {
    if supplied.len() != required.len() {
        return Err(evidence("recurrence input row count is not canonical"));
    }
    let profile = binding.shape_profile.shape_profile_id();
    let mut inputs = Vec::with_capacity(required.len());
    for (supplied, required) in supplied.iter().zip(required) {
        if supplied.preimages.len() != required.len() {
            return Err(evidence("recurrence input tile count is not canonical"));
        }
        let mut hashes = Vec::with_capacity(required.len());
        let mut row = Vec::new();
        for (preimage, (_, coord)) in supplied.preimages.iter().zip(required) {
            if preimage.version != leg::PALW_STEP_LEG_OBJECT_VERSION_V1 || preimage.coord != *coord {
                return Err(evidence("recurrence input coordinate/version is not canonical"));
            }
            // A step root alone can commit a malformed length. The operand's
            // canonical value count, not its caller-declared count, selects the
            // bytes arithmetic may consume.
            let (source, _) = binding.shape_profile.resolve_node_slot(coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
            let crate::palw_step::PalwStepOutLenV1::Fixed { elements } = source.out_len else {
                return Err(evidence("recurrence source is not fixed-width"));
            };
            let tile_start = u64::from(coord.tile_index) * u64::from(source.tile_len);
            let count = u64::from(elements)
                .checked_sub(tile_start)
                .filter(|remaining| *remaining > 0)
                .ok_or(PalwStepRefuteError::Unadjudicable)?
                .min(u64::from(source.tile_len)) as u32;
            if preimage.value_count != count || preimage.values_le.len() as u64 != u64::from(count) * 4 {
                return Err(evidence("recurrence input value count is not canonical"));
            }
            hashes.push(leg::step_tile_leaf_hash_ctx_v1(&binding.job_context, &profile, preimage));
            row.extend(preimage.values_le.chunks_exact(4).map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte lanes"))));
        }
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (i, (index, _)) in required.iter().enumerate() {
            match runs.last_mut() {
                Some((start, len)) if required[*start].0 + *len as u64 == *index => *len += 1,
                _ => runs.push((i, 1)),
            }
        }
        if runs.len() != supplied.run_siblings.len() {
            return Err(evidence("recurrence input run count is not canonical"));
        }
        for ((start, len), siblings) in runs.iter().zip(&supplied.run_siblings) {
            if siblings.len() as u64 > 2 * u64::from(64 - binding.step_leaf_count.leading_zeros()) {
                return Err(evidence("recurrence range opening exceeds its derived depth bound"));
            }
            let opening = leg::PalwStepRangeOpeningV1 {
                first_leaf_index: required[*start].0,
                leaf_hashes: hashes[*start..*start + *len].to_vec(),
                siblings: siblings.clone(),
            };
            if leg::step_range_opening_root_capped_v1(binding.step_leaf_count, &opening, max_leaves)? != binding.step_merkle_root {
                return Err(evidence("recurrence input does not belong to the committed step root"));
            }
        }
        inputs.push(row);
    }
    Ok(inputs)
}

/// Versioned deterministic terminal, bound to the claim read by its caller.
/// This returns a local offence verdict only. Native activation/registration and
/// randomly selected validator approval are independent consumers, not implied
/// by this arithmetic or by a matching local replay.
pub fn check_gdn_step_refutation_v2(
    r: &PalwGdnStepRefutationV2,
    claim_execution_root: Hash64,
    registered_class: Hash64,
    weights: &dyn PalwWeightOracleV1,
    max_leaves: u64,
) -> Result<PalwStepRefutationVerdictV1, PalwGdnCourtErrorV2> {
    if r.version != PALW_GDN_STEP_EVIDENCE_VERSION_V2 || r.step.kv_checkpoint.is_some() {
        return Err(evidence("recurrence V2 version/KV anchor is not canonical"));
    }
    let binding = &r.step.binding;
    if binding.committed_execution_root != claim_execution_root {
        return Err(evidence("recurrence evidence names another claim execution"));
    }
    let coord =
        crate::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, r.step.output_opening.leaf_index)
            .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let (node, layer) = binding.shape_profile.resolve_node_slot(coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let sliced = match program {
        KernelProgram::GdnCore { .. } => false,
        KernelProgram::Qwen36(Qwen36Op::GdnStep) => true,
        _ => return Err(PalwStepRefuteError::Unadjudicable.into()),
    };
    let start = gdn_replay_start_v2(binding, &coord)?;
    let _workspace = reserve_workspace_v2(r, start, &coord, sliced)?;
    leg::verify_binding_v1(binding)?;
    if binding.shape_profile.shape_profile_id() != registered_class {
        return Err(evidence("recurrence shape is not the claim's registered class"));
    }
    let anchor = authenticate_anchor_v2(
        binding,
        &coord,
        start,
        r.checkpoint.as_ref(),
        max_leaves,
        if sliced { Some(coord.tile_index) } else { None },
    )?;
    let required = canonical_input_leaves_with_start_v2(
        &binding.shape_profile,
        &binding.job_context,
        coord.node_slot,
        &coord,
        program,
        false,
        Some(start.first_position),
        None,
    )
    .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let mut inputs = verified_input_rows_v2(binding, &r.step.inputs, &required, max_leaves)?;
    // A recurrence consumes no prompt/decode token carriage; extra, ignored
    // evidence would create a second meaning for this version's canonical form.
    if !r.step.prompt_token_ids.is_empty() || r.step.decode_tokens.is_some() {
        return Err(evidence("recurrence evidence must not carry unrelated token ids"));
    }
    let structural = leg::PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: leg::PalwStepEvidenceV1::StepTile {
            opening: r.step.output_opening.clone(),
            preimage: r.step.output_preimage.clone(),
        },
    };
    match leg::check_step_refutation_capped_v1(&structural, max_leaves) {
        Ok(verdict) => return Ok(verdict),
        Err(leg::PalwStepLegError::NoFaultFound) => {}
        Err(error) => return Err(error.into()),
    }
    let initial = anchor.as_ref().map(VerifiedGdnAnchorV2::delta_rows);
    let (row, offset) = match program {
        KernelProgram::GdnCore { dot } => {
            let heads = binding.shape_profile.gdn_heads as usize;
            for position in inputs.chunks_exact_mut(5) {
                if position[3].len() < heads || position[4].len() < heads {
                    return Err(evidence("recurrence gate row is shorter than its registered heads"));
                }
                position[3].truncate(heads);
                position[4].truncate(heads);
            }
            (gdn_core_anchored_replay_v1(&binding.shape_profile, initial.as_deref(), &inputs, dot)?.out_row, 0)
        }
        KernelProgram::Qwen36(Qwen36Op::GdnStep) => {
            let prompt = crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::EMPTY;
            let initial_head = match &initial {
                None => None,
                Some(rows) if rows.len() == 1 => Some(rows[0].as_slice()),
                Some(_) => return Err(evidence("selected recurrence head count differs")),
            };
            (
                qwen36_row(
                    Qwen36Op::GdnStep,
                    node,
                    layer,
                    &binding.shape_profile,
                    &inputs,
                    weights,
                    u64::from(
                        cadence::palw_absolute_position_v1(&binding.job_context, coord.call_index, coord.position)
                            .ok_or(PalwStepRefuteError::Unadjudicable)?,
                    ) + 1,
                    (&coord, prompt, &[]),
                    initial_head,
                    None,
                )?,
                coord.tile_index as usize * node.tile_len as usize,
            )
        }
        _ => unreachable!("program checked before construction"),
    };
    let tile_start = coord.tile_index as usize * node.tile_len as usize;
    let local = tile_start.checked_sub(offset).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let committed = &r.step.output_preimage.values_le;
    let recomputed = row
        .get(local..local + committed.len() / 4)
        .ok_or_else(|| evidence("recomputed recurrence row is shorter than the output tile"))?;
    let mismatch = recomputed.iter().zip(committed.chunks_exact(4)).position(|(word, bytes)| word.to_le_bytes().as_slice() != bytes);
    match mismatch {
        None => Err(PalwStepRefuteError::NoFaultFound.into()),
        Some(index) => {
            let fault = PalwStepFaultV1::ComputationMismatch { value_index: index as u32 };
            Ok(PalwStepRefutationVerdictV1 {
                fault,
                // Same local offence identity as the existing deterministic
                // arithmetic terminal; evidence encoding is not a new offence.
                evidence_id: leg::step_refutation_evidence_id(&claim_execution_root, 5, r.step.output_opening.leaf_index, fault),
            })
        }
    }
}

#[cfg(test)]
#[path = "memory_v2_tests.rs"]
pub(crate) mod tests;
