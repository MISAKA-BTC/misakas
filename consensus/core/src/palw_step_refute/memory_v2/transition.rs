//! **The recurrence checkpoint's own two facts, tried directly** (RFC-0006 memory court consumer v2).
//!
//! [`check_gdn_step_refutation_v2`] replays a recurrence step from the exact predecessor checkpoint's AUTHENTICATED state. A
//! checkpoint opening authenticates only the bytes the executor committed: if the executor committed a delta state that is not the
//! replay of its own committed rows, an anchored replay from it reproduces whatever output the executor chose, and that terminal
//! acquits. The anchored court is therefore sound only together with a court for the checkpoint itself. This module is that court,
//! for the two parts of a recurrence checkpoint:
//!
//! * **Transition** ([`check_gdn_transition_refutation_v2`]): the accused checkpoint's delta state for one layer (one Q36 head, or
//!   every head of the fused float core) must equal the deterministic replay of its exact predecessor checkpoint's state — or of the
//!   genesis zero state, for the first one — over the committed step rows between them. By induction over the checkpoint chain the
//!   first forged checkpoint is always convictable this way, so an honest challenger whose own replay disagrees with an anchor
//!   accuses the anchor, not the step.
//! * **Convolution window** ([`check_gdn_conv_window_refutation_v2`]): the accused checkpoint's convolution window for one head (or
//!   the whole window under the full-window v1 layout) must equal the committed `[q | k | v]` projection rows of the last
//!   `conv_kernel` positions it covers (zeros before the sequence start) — the rows the class's own convolution node reads. Two
//!   value heads that alias one key head under the key-head layout each carry a copy of its `q`/`k` channels; each copy is compared
//!   with the same committed rows, so an alias that disagrees convicts.
//!
//! A count that is not the registered layout's is itself a shape fault ([`PalwGdnMismatchV2::CheckpointShape`]): no anchor can
//! stand on such a checkpoint, and refusing every accusation against it would make the class's recurrence unadjudicable by
//! construction. Every verdict here is deterministic recomputation over authenticated data; hashes and Merkle paths authenticate
//! identity and membership only, and nothing here is validator approval or consensus credit.
use super::capture::{gdn_chunk_witness_v2, gdn_input_rows_from_capture_v2, gdn_recurrence_section_v2, gdn_selected_delta_chunks_v2};
use super::*;
#[path = "conv_request_owner_v3.rs"]
mod conv_request_owner;
#[path = "conv_demand_borrow_v3.rs"]
mod conv_demand_borrow;
pub use conv_demand_borrow::gdn_check_conv_demands_borrowed_v3;
#[path = "transition_predecessor_retained_v3.rs"]
mod transition_predecessor_retained;
pub use transition_predecessor_retained::{PalwGdnTransitionBuiltV3, gdn_transition_refutation_with_accused_retained_v3};

pub use conv_request_owner::{PalwGdnConvAdmissionV3, PalwGdnConvOwnerV3, PalwGdnConvRefutationRequestsV3, PalwGdnConvRequiredRowV3,
    gdn_conv_refutation_requests_v3, visit_gdn_conv_required_rows_v3};


/// The evidence version of both checkpoint terminals.
pub const PALW_GDN_CHECKPOINT_EVIDENCE_VERSION_V2: u16 = 2;
/// The step-family evidence kind of a convicted checkpoint transition (dedup namespace beside the arithmetic terminal's 5).
pub const PALW_GDN_TRANSITION_EVIDENCE_KIND_V2: u8 = 0x53;
/// The step-family evidence kind of a convicted convolution window.
pub const PALW_GDN_CONV_WINDOW_EVIDENCE_KIND_V2: u8 = 0x54;

/// **The objective class of a recurrence mismatch** — what the deterministic recomputation found, named. Each is a fault of the
/// claim's committed execution, decidable from authenticated data alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGdnMismatchV2 {
    /// The committed output tile differs from the anchored replay ([`check_gdn_step_refutation_v2`]).
    OutputSlice { value_index: u32 },
    /// A checkpoint's delta state differs from its predecessor's replay over the committed rows.
    DeltaState { head: u32, row: u32, lane: u32 },
    /// A checkpoint's convolution window differs from the committed projection rows it must hold.
    ConvWindow { head: u32, row: u32, lane: u32 },
    /// A checkpoint leaf's chunk count is not the registered layout's at its cadence.
    CheckpointShape { declared: u32, derived: u64 },
}

/// **A recurrence checkpoint accused of a delta state that is not its predecessor's replay.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGdnTransitionRefutationV2 {
    pub version: u16,
    pub binding: leg::PalwStepBindingV2,
    /// The recurrence node (global slot) whose layer is accused.
    pub node_slot: u32,
    /// The Q36 head; 0 for the fused float core, whose state is every head.
    pub head: u32,
    /// The accused checkpoint: its leaf, opening and EXACTLY the selected delta chunks of `(layer, head)`.
    pub accused: PalwGdnCheckpointWitnessV2,
    /// The exact predecessor recurrence checkpoint with the same selection; `None` only where the accused is the first one.
    pub predecessor: Option<PalwGdnCheckpointWitnessV2>,
    /// The canonical replay rows between them (the window `canonical_input_leaves_with_start_v2` derives).
    pub inputs: Vec<PalwStepInputRowV1>,
}

/// **A recurrence checkpoint accused of a convolution window that is not the committed projection rows.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGdnConvWindowRefutationV2 {
    pub version: u16,
    pub binding: leg::PalwStepBindingV2,
    /// The layer's unique Q36 convolution node (global slot): its three references are the `[q | k | v]` projection rows.
    pub conv_slot: u32,
    /// The window head (head-sliced layouts); 0 for the full-window v1 layout.
    pub head: u32,
    /// The accused checkpoint with EXACTLY the selected convolution chunks of `(layer, head)`.
    pub accused: PalwGdnCheckpointWitnessV2,
    /// The committed projection rows ([`gdn_conv_window_required_v2`]'s set).
    pub inputs: Vec<PalwStepInputRowV1>,
}

/// A convicting verdict with its objective class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGdnCheckpointVerdictV2 {
    pub verdict: PalwStepRefutationVerdictV1,
    pub mismatch: PalwGdnMismatchV2,
}

/// The cadence facts of checkpoint `index`: `(covered counter, positions covered)`. `positions` is also the absolute position of
/// the first step AFTER the checkpoint. Refused where the index names no checkpoint of the leg, or — under the per-position cadence —
/// a checkpoint that does not carry the recurrence half.
fn checkpoint_site_v2(binding: &leg::PalwStepBindingV2, index: u32) -> Result<(u32, u32), PalwGdnCourtErrorV2> {
    let p = &binding.shape_profile;
    if index >= binding.checkpoint_count {
        return Err(evidence("accused checkpoint index is outside the committed checkpoint leg"));
    }
    let covered = cadence::palw_checkpoint_covered_at_index_v1(p, index, binding.checkpoint_profile.checkpoint_interval)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let positions = cadence::palw_checkpoint_positions_at_v1(p, &binding.job_context, covered);
    if positions == 0 {
        return Err(evidence("a checkpoint covering no position has no recurrence state"));
    }
    if !cadence::palw_checkpoint_leaf_carries_recurrence_v1(p, positions) {
        return Err(evidence("this checkpoint does not carry the recurrence half"));
    }
    Ok((covered, positions))
}

/// The step coordinate of absolute position `absolute` at `node_slot`, tile `tile`.
fn coordinate_at_v2(binding: &leg::PalwStepBindingV2, absolute: u32, node_slot: u32, tile: u32) -> PalwStepCoordinateV1 {
    let prefill = binding.job_context.declared_prefill_tokens;
    let (call_index, position) = if absolute < prefill { (0, absolute) } else { (absolute - prefill + 1, 0) };
    PalwStepCoordinateV1 { call_index, node_slot, position, tile_index: tile }
}

/// **Authenticate a checkpoint witness carrying EXACTLY `selected` chunks** — the leaf at `index` with its cadence counter, its
/// opening against the binding's checkpoint root, the chunk count against the registered layout at `positions`, and every chunk's
/// index, length and membership under the class's own state tree. Returns `Err(Ok(shape))` for an authenticated leaf whose count is
/// not the layout's: that is the executor's fault, not the accuser's.
fn authenticate_selected_v2(
    binding: &leg::PalwStepBindingV2,
    witness: &PalwGdnCheckpointWitnessV2,
    index: u32,
    covered: u32,
    positions: u32,
    selected: &[u32],
    max_leaves: u64,
) -> Result<Result<(), PalwGdnMismatchV2>, PalwGdnCourtErrorV2> {
    let profile = &binding.shape_profile;
    let leaf = &witness.leaf;
    if leaf.version != leg::PALW_STEP_LEG_OBJECT_VERSION_V1
        || leaf.checkpoint_index != index
        || witness.opening.leaf_index != u64::from(index)
        || leaf.covered_decode_call != covered
    {
        return Err(evidence("checkpoint index/cadence is not the accused one"));
    }
    let context_hash = binding.job_context.context_hash();
    let hash =
        leg::checkpoint_leaf_hash_v2(&context_hash, &binding.checkpoint_profile.profile_hash(), &binding.state_chunk_map_id, leaf);
    if hash != witness.opening.leaf_hash
        || leg::step_opening_root_capped_v1(u64::from(binding.checkpoint_count), &witness.opening, max_leaves)?
            != binding.checkpoint_merkle_root
    {
        return Err(evidence("checkpoint is not in the committed execution"));
    }
    let (_, derived, layout) = gdn_recurrence_section_v2(profile, positions)?;
    if derived != u64::from(leaf.state_chunk_count) {
        return Ok(Err(PalwGdnMismatchV2::CheckpointShape { declared: leaf.state_chunk_count, derived }));
    }
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    let (offset, _, _) = gdn_recurrence_section_v2(profile, positions)?;
    if witness.chunks.len() != selected.len() {
        return Err(evidence("missing, extra or duplicate checkpoint chunks"));
    }
    for (chunk, want) in witness.chunks.iter().zip(selected) {
        let local = u64::from(*want).checked_sub(offset).ok_or(PalwStepRefuteError::Unadjudicable)?;
        let entry = geometry.entry(local).ok_or(PalwStepRefuteError::Unadjudicable)?;
        if chunk.chunk_index != *want || chunk.bytes.len() as u64 != entry.byte_len() {
            return Err(evidence("checkpoint chunk index or length differs from the derived selection"));
        }
        let root = if let Some(layout) = &layout {
            let address = layout.address(u64::from(*want)).ok_or(PalwStepRefuteError::Unadjudicable)?;
            let leaf_hash = leg::state_chunk_leaf_hash_v4(&binding.state_chunk_map_id, address.slice, address.block, &chunk.bytes);
            leg::state_chunk_opening_root_v4(
                address.slice_count,
                address.slice,
                address.block_count,
                address.block,
                &leaf_hash,
                &chunk.siblings,
            )?
        } else {
            let leaf_hash = leg::state_chunk_leaf_hash_v1(&binding.state_chunk_map_id, *want, &chunk.bytes);
            leg::state_chunk_opening_root_v1(leaf.state_chunk_count as usize, *want, &leaf_hash, &chunk.siblings)?
        };
        if root != leaf.state_chunks_root {
            return Err(evidence("checkpoint chunk is not a member of its state root"));
        }
    }
    Ok(Ok(()))
}

/// The authenticated chunks' rows as `u32` lanes, concatenated per head (`heads` groups of `per_head` chunks).
fn rows_per_head_v2(chunks: &[PalwGdnStateChunkWitnessV2], per_head: usize) -> Vec<Vec<u32>> {
    chunks
        .chunks(per_head.max(1))
        .map(|group| {
            group
                .iter()
                .flat_map(|chunk| chunk.bytes.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().expect("four-byte lanes"))))
                .collect()
        })
        .collect()
}

/// **The integer Q36 recurrence of ONE head, replayed from `initial`, returning the head's FINAL state** — the `GdnStep` arm's
/// arithmetic (`q36_gdn_step`, the slice derivation `qwen36_gdn_slice_v1`, the four registered triples at `4 · head`), standing on
/// an authenticated state. The arm itself answers only the last output row; the transition terminal needs the state.
fn q36_gdn_replay_state_v2(
    profile: &crate::palw_step::PalwShapeProfileV3,
    node_slot: u32,
    head: u32,
    initial: Option<&[u32]>,
    inputs: &[Vec<u32>],
    weights: &dyn PalwWeightOracleV1,
) -> Result<Vec<i32>, PalwStepRefuteError> {
    use crate::palw_base0_a16::A16QuantParams;
    use crate::palw_qwen36_ops as q36;
    let (node, layer) = profile.resolve_node_slot(node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if !matches!(resolve_kernel(&node.kernel_semantics_id), Some(KernelProgram::Qwen36(Qwen36Op::GdnStep))) {
        return Err(PalwStepRefuteError::Unadjudicable);
    }
    let first = node_slot
        .checked_sub(intra_table_index(profile, node_slot).ok_or(PalwStepRefuteError::Unadjudicable)? as u32)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let mut widths = Vec::with_capacity(5);
    let mut tiles = Vec::with_capacity(5);
    for &r in &node.input_refs {
        if r >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
            return Err(PalwStepRefuteError::Unadjudicable);
        }
        let (n, _) = profile.resolve_node_slot(first + u32::from(r)).ok_or(PalwStepRefuteError::Unadjudicable)?;
        match n.out_len {
            crate::palw_step::PalwStepOutLenV1::Fixed { elements } => widths.push(u64::from(elements)),
            crate::palw_step::PalwStepOutLenV1::KvScaled { .. } => return Err(PalwStepRefuteError::Unadjudicable),
        }
        tiles.push(u64::from(n.tile_len));
    }
    let slices = qwen36_gdn_slice_v1(profile, node, &widths, head).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let (hd_k, hd_v) = (profile.gdn_head_k_dim as usize, profile.gdn_head_v_dim as usize);
    if hd_k == 0
        || hd_v == 0
        || hd_k > PALW_GDN_MAX_DIM
        || hd_v > PALW_GDN_MAX_DIM
        || hd_k.checked_mul(hd_v).is_none_or(|n| n as u64 > PALW_GDN_MAX_DIM_SQUARED)
        || tiles.len() < 5
    {
        return Err(PalwStepRefuteError::Unadjudicable);
    }
    if inputs.is_empty() || !inputs.len().is_multiple_of(5) {
        return Err(PalwStepRefuteError::InputSetNotCanonical("the recurrence reads five sliced rows per position"));
    }
    let width = A16QuantParams::WIRE_BYTES;
    let offset = 4usize
        .checked_mul(head as usize)
        .and_then(|n| n.checked_mul(width))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let len = u32::try_from(4 * width).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
    let bytes = weights.operand_bytes(node.weight_name.as_str(), layer, offset, len).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if bytes.len() != len as usize {
        return Err(PalwStepRefuteError::Unadjudicable);
    }
    let p: Vec<A16QuantParams> = bytes
        .chunks_exact(width)
        .map(|c| {
            A16QuantParams::from_wire(c).map_err(|_| PalwStepRefuteError::InputSetNotCanonical("an a16 parameter triple is malformed"))
        })
        .collect::<Result<_, _>>()?;
    let gdn = q36::Qwen36GdnParamsV1 { read: p[0], delta: p[1], write_shift: p[2].zero as i32, out: p[3] };
    let s = match initial {
        None => vec![0i32; hd_v * hd_k],
        Some(words) if words.len() == hd_v * hd_k => words.iter().map(|w| *w as i32).collect(),
        Some(_) => return Err(PalwStepRefuteError::InputSetNotCanonical("the recurrence anchor has another head shape")),
    };
    let mut state = q36::Qwen36GdnStateV1 { d_v: hd_v, d_k: hd_k, s };
    let extract = |opened: &Vec<u32>, src_tile: u64, start: u64, end: u64| -> Result<Vec<i32>, PalwStepRefuteError> {
        let first_tile_lane = (start / src_tile.max(1)) * src_tile;
        let from = (start - first_tile_lane) as usize;
        let to = (end - first_tile_lane) as usize;
        if to > opened.len() || from > to {
            return Err(PalwStepRefuteError::InputSetNotCanonical("an opened slice does not cover its kernel's lanes"));
        }
        Ok(opened[from..to].iter().map(|v| *v as i32).collect())
    };
    for step in inputs.chunks_exact(5) {
        let unit_k = extract(&step[0], tiles[0], slices[0].0, slices[0].1)?;
        let v = extract(&step[1], tiles[1], slices[1].0, slices[1].1)?;
        let unit_q = extract(&step[2], tiles[2], slices[2].0, slices[2].1)?;
        let decay = extract(&step[3], tiles[3], slices[3].0, slices[3].1)?;
        let beta = extract(&step[4], tiles[4], slices[4].0, slices[4].1)?;
        if unit_k.len() != hd_k || v.len() != hd_v || unit_q.len() != hd_k || decay.len() != 1 || beta.len() != 1 {
            return Err(PalwStepRefuteError::InputSetNotCanonical("a replay position's slices do not match the head geometry"));
        }
        q36::q36_gdn_step(&mut state, &unit_k, &v, &unit_q, decay[0] as i64, beta[0] as i64, gdn)
            .map_err(|_| PalwStepRefuteError::InputSetNotCanonical("a q36 op refused its operand shape"))?;
    }
    Ok(state.s)
}

fn convict_v2(
    binding: &leg::PalwStepBindingV2,
    kind: u8,
    index: u32,
    value_index: u64,
    mismatch: PalwGdnMismatchV2,
) -> PalwGdnCheckpointVerdictV2 {
    let fault = PalwStepFaultV1::ComputationMismatch { value_index: u32::try_from(value_index).unwrap_or(u32::MAX) };
    PalwGdnCheckpointVerdictV2 {
        verdict: PalwStepRefutationVerdictV1 {
            fault,
            evidence_id: leg::step_refutation_evidence_id(&binding.committed_execution_root, kind, u64::from(index), fault),
        },
        mismatch,
    }
}

fn bind_to_claim_v2(
    version: u16,
    binding: &leg::PalwStepBindingV2,
    claim_execution_root: Hash64,
    registered_class: Hash64,
) -> Result<(), PalwGdnCourtErrorV2> {
    if version != PALW_GDN_CHECKPOINT_EVIDENCE_VERSION_V2 {
        return Err(evidence("recurrence checkpoint evidence version is not canonical"));
    }
    if binding.committed_execution_root != claim_execution_root {
        return Err(evidence("recurrence checkpoint evidence names another claim execution"));
    }
    leg::verify_binding_v1(binding)?;
    if binding.shape_profile.shape_profile_id() != registered_class
        || binding.state_chunk_map_id != binding.shape_profile.state_chunk_map_id
    {
        return Err(evidence("recurrence checkpoint evidence is not the claim's registered class and map"));
    }
    Ok(())
}

/// Typed before-allocation admission of the checkpoint terminals' workspace: the decoded window rows, the replay and the decoded
/// checkpoint states coexist. Counted from the profile's declared widths, never from carried lengths alone.
fn reserve_checkpoint_workspace_v2(
    binding: &leg::PalwStepBindingV2,
    positions: u64,
    row_lanes: u64,
    heads: u64,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwGdnCourtErrorV2> {
    let p = &binding.shape_profile;
    let mut q = TirNativeRequestsV1::new();
    let rows = TirNativeRequestsV1::product_v1(positions, 5)?;
    q.vec_v1::<(u32, u32)>(positions)?;
    q.vec_v1::<Vec<(u64, PalwStepCoordinateV1)>>(rows)?;
    q.vec_v1::<Vec<u32>>(rows)?;
    let mut one = TirNativeRequestsV1::new();
    one.vec_v1::<u32>(row_lanes)?;
    q.add_v1(TirNativeRequestsV1::product_v1(rows, one.bytes_v1())?)?;
    let per_head = TirNativeRequestsV1::product_v1(u64::from(p.gdn_head_k_dim), u64::from(p.gdn_head_v_dim))?;
    // Predecessor state, accused state and the mutable replay state.
    for _ in 0..3 {
        let mut state = TirNativeRequestsV1::new();
        state.vec_v1::<u32>(per_head)?;
        q.add_v1(TirNativeRequestsV1::product_v1(heads, state.bytes_v1())?)?;
    }
    q.vec_v1::<u8>(leg::PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES as u64 + 256)?;
    reserve_native_requests_v1(q.bytes_v1()).map_err(Into::into)
}

/// **The transition terminal.** `Ok` convicts (the accused delta state is not its predecessor's replay, or the accused leaf's count
/// is not the layout's); `Err(Evidence(NoFaultFound))` is a false accusation; any other `Err` refuses the evidence.
pub fn check_gdn_transition_refutation_v2(
    r: &PalwGdnTransitionRefutationV2,
    claim_execution_root: Hash64,
    registered_class: Hash64,
    weights: &dyn PalwWeightOracleV1,
    max_leaves: u64,
) -> Result<PalwGdnCheckpointVerdictV2, PalwGdnCourtErrorV2> {
    let binding = &r.binding;
    bind_to_claim_v2(r.version, binding, claim_execution_root, registered_class)?;
    let profile = &binding.shape_profile;
    let (node, layer) = profile.resolve_node_slot(r.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let layer = layer.ok_or(PalwStepRefuteError::Unadjudicable)?;
    if profile.layer_kind(layer) != crate::palw_step::PalwLayerKindV1::GatedDeltaNet {
        return Err(evidence("accused node is not a recurrence layer's"));
    }
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let sliced = match program {
        KernelProgram::GdnCore { .. } => false,
        KernelProgram::Qwen36(Qwen36Op::GdnStep) => true,
        _ => return Err(PalwStepRefuteError::Unadjudicable.into()),
    };
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    if (sliced && r.head >= geometry.heads) || (!sliced && r.head != 0) {
        return Err(evidence("accused head is not this recurrence's selection"));
    }
    let head = if sliced { Some(r.head) } else { None };
    let index = r.accused.leaf.checkpoint_index;
    let (covered, to_first) = checkpoint_site_v2(binding, index)?;
    let last = coordinate_at_v2(binding, to_first - 1, r.node_slot, if sliced { r.head } else { 0 });
    let start = gdn_replay_start_v2(binding, &last)?;
    let heads = if sliced { 1 } else { u64::from(geometry.heads) };
    let window = u64::from(to_first - start.first_position);
    let row_lanes = u64::from(profile.gdn_heads) * u64::from(profile.gdn_head_v_dim.max(profile.gdn_head_k_dim));
    let _workspace = reserve_checkpoint_workspace_v2(binding, window, row_lanes, heads)?;

    let selected_accused = gdn_selected_delta_chunks_v2(profile, to_first, layer, head)?;
    if let Err(shape) = authenticate_selected_v2(binding, &r.accused, index, covered, to_first, &selected_accused, max_leaves)? {
        let derived = match shape {
            PalwGdnMismatchV2::CheckpointShape { derived, .. } => derived,
            _ => 0,
        };
        return Ok(convict_v2(binding, PALW_GDN_TRANSITION_EVIDENCE_KIND_V2, index, derived, shape));
    }
    let predecessor = match (start.checkpoint_index, r.predecessor.as_ref()) {
        (None, None) => None,
        (None, Some(_)) => return Err(evidence("the first recurrence checkpoint has no predecessor to carry")),
        (Some(_), None) => return Err(evidence("the exact predecessor checkpoint is missing")),
        (Some(prev), Some(witness)) => {
            let prev_covered = start.covered.ok_or(PalwStepRefuteError::Unadjudicable)?;
            let selected = gdn_selected_delta_chunks_v2(profile, start.first_position, layer, head)?;
            // A predecessor of the wrong shape is ITS OWN fault, accused on its own: here it is no anchor.
            authenticate_selected_v2(binding, witness, prev, prev_covered, start.first_position, &selected, max_leaves)?
                .map_err(|_| evidence("the predecessor checkpoint's layout is itself faulty; accuse it"))?;
            Some(witness)
        }
    };
    let required = canonical_input_leaves_with_start_v2(
        profile,
        &binding.job_context,
        r.node_slot,
        &last,
        program,
        false,
        Some(start.first_position),
        None,
    )
    .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let mut inputs = verified_input_rows_v2(binding, &r.inputs, &required, max_leaves)?;
    let per_head = geometry.v_dim.div_ceil(geometry.delta_rows_per_chunk) as usize;
    let initial = predecessor.map(|w| rows_per_head_v2(&w.chunks, per_head));
    let accused = rows_per_head_v2(&r.accused.chunks, per_head);
    let replayed: Vec<Vec<u32>> = match program {
        KernelProgram::GdnCore { dot } => {
            let heads = profile.gdn_heads as usize;
            for position in inputs.chunks_exact_mut(5) {
                if position[3].len() < heads || position[4].len() < heads {
                    return Err(evidence("recurrence gate row is shorter than its registered heads"));
                }
                position[3].truncate(heads);
                position[4].truncate(heads);
            }
            gdn_core_anchored_replay_v1(profile, initial.as_deref(), &inputs, dot)?.state
        }
        KernelProgram::Qwen36(Qwen36Op::GdnStep) => {
            let initial_head = match &initial {
                None => None,
                Some(rows) if rows.len() == 1 => Some(rows[0].as_slice()),
                Some(_) => return Err(evidence("selected recurrence head count differs")),
            };
            let state = q36_gdn_replay_state_v2(profile, r.node_slot, r.head, initial_head, &inputs, weights)?;
            vec![state.into_iter().map(|v| v as u32).collect()]
        }
        _ => return Err(PalwStepRefuteError::Unadjudicable.into()),
    };
    if replayed.len() != accused.len() {
        return Err(evidence("replayed and accused head counts differ"));
    }
    let k = geometry.k_dim as usize;
    let words_per_head = (geometry.k_dim as usize) * (geometry.v_dim as usize);
    for (ordinal, (mine, theirs)) in replayed.iter().zip(accused.iter()).enumerate() {
        if mine.len() != theirs.len() {
            return Err(evidence("replayed and accused head states differ in shape"));
        }
        if let Some(i) = mine.iter().zip(theirs.iter()).position(|(a, b)| a != b) {
            let head = head.unwrap_or(ordinal as u32);
            let flat = ordinal as u64 * words_per_head as u64 + i as u64;
            return Ok(convict_v2(
                binding,
                PALW_GDN_TRANSITION_EVIDENCE_KIND_V2,
                index,
                flat,
                PalwGdnMismatchV2::DeltaState { head, row: (i / k.max(1)) as u32, lane: (i % k.max(1)) as u32 },
            ));
        }
    }
    Err(PalwStepRefuteError::NoFaultFound.into())
}

/// The layer's unique Q36 convolution node and its three projection references, resolved and checked against the recurrence
/// geometry: `(q, k, v)` global slots with their tile lengths and widths.
fn conv_sources_v2(
    profile: &crate::palw_step::PalwShapeProfileV3,
    conv_slot: u32,
    geometry: &PalwGdnStateGeometryV2<'_>,
) -> Result<(u16, [(u32, u64, u64); 3]), PalwGdnCourtErrorV2> {
    let (node, layer) = profile.resolve_node_slot(conv_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let layer = layer.ok_or(PalwStepRefuteError::Unadjudicable)?;
    if !matches!(resolve_kernel(&node.kernel_semantics_id), Some(KernelProgram::Qwen36(Qwen36Op::SsmConv))) {
        return Err(evidence("the named node is not the layer's Q36 convolution"));
    }
    let table = profile.layer_table(layer);
    if table.iter().filter(|n| n.kernel_semantics_id == node.kernel_semantics_id).count() != 1 || node.input_refs.len() != 3 {
        return Err(evidence("the layer's convolution is not unique with three projection sources"));
    }
    let first = conv_slot
        .checked_sub(intra_table_index(profile, conv_slot).ok_or(PalwStepRefuteError::Unadjudicable)? as u32)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let want = [
        u64::from(geometry.key_heads) * u64::from(geometry.k_dim),
        u64::from(geometry.key_heads) * u64::from(geometry.k_dim),
        u64::from(geometry.heads) * u64::from(geometry.v_dim),
    ];
    let mut sources = [(0u32, 0u64, 0u64); 3];
    for (i, &r) in node.input_refs.iter().enumerate() {
        if r >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
            return Err(evidence("a convolution source is not a registered step row"));
        }
        let slot = first + u32::from(r);
        let (source, _) = profile.resolve_node_slot(slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
        let crate::palw_step::PalwStepOutLenV1::Fixed { elements } = source.out_len else {
            return Err(evidence("a convolution source is not fixed-width"));
        };
        if u64::from(elements) != want[i] || source.tile_len == 0 {
            return Err(evidence("a convolution source's width is not the recurrence geometry's"));
        }
        sources[i] = (slot, u64::from(source.tile_len), want[i]);
    }
    Ok((layer, sources))
}

/// The lane range of source `which` (0 = q, 1 = k, 2 = v) a window `head` reads: one key head's `q`/`k` lanes and the value head's
/// `v` lanes under the head layouts, the whole row under the full-window v1 layout.
fn conv_range_v2(geometry: &PalwGdnStateGeometryV2<'_>, head: u32, which: usize) -> (u64, u64) {
    if geometry.layout == crate::palw_gdn_state_map_v2::PalwGdnStateLayoutV2::FullConvV1 {
        let width = if which < 2 {
            u64::from(geometry.key_heads) * u64::from(geometry.k_dim)
        } else {
            u64::from(geometry.heads) * u64::from(geometry.v_dim)
        };
        return (0, width);
    }
    let key = u64::from(head % geometry.key_heads);
    if which < 2 {
        (key * u64::from(geometry.k_dim), (key + 1) * u64::from(geometry.k_dim))
    } else {
        (u64::from(head) * u64::from(geometry.v_dim), (u64::from(head) + 1) * u64::from(geometry.v_dim))
    }
}

/// **The canonical committed rows a convolution-window accusation carries** — for each window position at or after the sequence
/// start (ascending), each of the three sources (`q`, `k`, `v`), the covering tiles of the head's lane range. Shared by the
/// assembler and the checker, so the two derive one set.
pub fn gdn_conv_window_required_v2(
    binding: &leg::PalwStepBindingV2,
    conv_slot: u32,
    head: u32,
    checkpoint_index: u32,
) -> Result<Vec<Vec<(u64, PalwStepCoordinateV1)>>, PalwGdnCourtErrorV2> {
    let mut out = Vec::new();
    visit_gdn_conv_required_rows_v3(binding, conv_slot, head, checkpoint_index, |source| {
        let mut row = Vec::new();
        source.visit_tiles_v3(|index, coordinate| { row.push((index, coordinate)); Ok(()) })?;
        out.push(row);
        Ok(())
    })?;
    Ok(out)
}

/// **The convolution-window terminal.** `Ok` convicts (a window lane is not the committed projection lane it must hold, or the
/// accused leaf's count is not the layout's); `Err(Evidence(NoFaultFound))` is a false accusation; any other `Err` refuses.
pub fn check_gdn_conv_window_refutation_v2(
    r: &PalwGdnConvWindowRefutationV2,
    claim_execution_root: Hash64,
    registered_class: Hash64,
    max_leaves: u64,
) -> Result<PalwGdnCheckpointVerdictV2, PalwGdnCourtErrorV2> {
    let binding = &r.binding;
    bind_to_claim_v2(r.version, binding, claim_execution_root, registered_class)?;
    let profile = &binding.shape_profile;
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    let conv_heads =
        if geometry.layout == crate::palw_gdn_state_map_v2::PalwGdnStateLayoutV2::FullConvV1 { 1 } else { geometry.heads };
    if r.head >= conv_heads {
        return Err(evidence("accused window head is outside the layout"));
    }
    let (layer, sources) = conv_sources_v2(profile, r.conv_slot, &geometry)?;
    let index = r.accused.leaf.checkpoint_index;
    let (covered, to_first) = checkpoint_site_v2(binding, index)?;
    let blocks = geometry.conv_kernel.div_ceil(geometry.conv_rows_per_chunk);
    let (offset, _, _) = gdn_recurrence_section_v2(profile, to_first)?;
    let mut selected = Vec::with_capacity(blocks as usize);
    for block in 0..blocks {
        let local = geometry.conv_chunk_index(layer, r.head, block).ok_or(PalwStepRefuteError::Unadjudicable)?;
        selected.push(offset.checked_add(local).and_then(|i| u32::try_from(i).ok()).ok_or(PalwStepRefuteError::Unadjudicable)?);
    }
    let lanes = 2 * u64::from(geometry.k_dim) + u64::from(geometry.v_dim);
    let _workspace =
        reserve_checkpoint_workspace_v2(binding, u64::from(geometry.conv_kernel), lanes.max(u64::from(geometry.conv_width)), 1)?;
    if let Err(shape) = authenticate_selected_v2(binding, &r.accused, index, covered, to_first, &selected, max_leaves)? {
        let derived = match shape {
            PalwGdnMismatchV2::CheckpointShape { derived, .. } => derived,
            _ => 0,
        };
        return Ok(convict_v2(binding, PALW_GDN_CONV_WINDOW_EVIDENCE_KIND_V2, index, derived, shape));
    }
    let required = gdn_conv_window_required_v2(binding, r.conv_slot, r.head, index)?;
    let opened = verified_input_rows_v2(binding, &r.inputs, &required, max_leaves)?;
    // The window, oldest first, decoded from the authenticated chunks.
    let window: Vec<u32> = r
        .accused
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().expect("four-byte lanes"))))
        .collect();
    let row_lanes = (window.len() / geometry.conv_kernel.max(1) as usize).max(1);
    let k_rows = geometry.conv_kernel as usize;
    let from = to_first.saturating_sub(geometry.conv_kernel);
    let missing = k_rows - (to_first - from) as usize;
    for row in 0..k_rows {
        // Expected lanes of window row `row`: zeros before the sequence start, else the committed `[q | k | v]` slice.
        let expected: Vec<u32> = if row < missing {
            vec![0u32; row_lanes]
        } else {
            let base = (row - missing) * 3;
            let mut lanes_out = Vec::with_capacity(row_lanes);
            for (which, (_, tile, _)) in sources.iter().enumerate() {
                let (lo, hi) = conv_range_v2(&geometry, r.head, which);
                let first_tile_lane = (lo / tile) * tile;
                let opened_row = opened.get(base + which).ok_or_else(|| evidence("a committed projection row is missing"))?;
                let a = (lo - first_tile_lane) as usize;
                let b = (hi - first_tile_lane) as usize;
                lanes_out.extend_from_slice(
                    opened_row.get(a..b).ok_or_else(|| evidence("an opened projection row does not cover its range"))?,
                );
            }
            lanes_out
        };
        if expected.len() != row_lanes {
            return Err(evidence("the window row width is not the layout's"));
        }
        let actual = &window[row * row_lanes..(row + 1) * row_lanes];
        if let Some(lane) = expected.iter().zip(actual.iter()).position(|(a, b)| a != b) {
            let flat = (row * row_lanes + lane) as u64;
            return Ok(convict_v2(
                binding,
                PALW_GDN_CONV_WINDOW_EVIDENCE_KIND_V2,
                index,
                flat,
                PalwGdnMismatchV2::ConvWindow { head: r.head, row: row as u32, lane: lane as u32 },
            ));
        }
    }
    Err(PalwStepRefuteError::NoFaultFound.into())
}

/// The layer, the head selection (one Q36 head, or every head of the fused float core) and the kernel of the recurrence at
/// `node_slot` — the facts every transition assembler and demand derives from the binding alone.
fn transition_site_v2(
    profile: &crate::palw_step::PalwShapeProfileV3,
    node_slot: u32,
    head: u32,
) -> Result<(u16, Option<u32>, KernelProgram), PalwGdnCourtErrorV2> {
    let (node, layer) = profile.resolve_node_slot(node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let layer = layer.ok_or(PalwStepRefuteError::Unadjudicable)?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let sliced = matches!(program, KernelProgram::Qwen36(Qwen36Op::GdnStep));
    Ok((layer, if sliced { Some(head) } else { None }, program))
}

/// **Assemble a transition accusation from captured material** — the accused checkpoint `index`, its exact predecessor and the
/// replay window rows, every index re-derived as the checker derives it. The holder supplies its own honest replay's judgement: this
/// assembles the evidence and does not decide whether to file it.
pub fn gdn_transition_refutation_from_capture_v2(
    capture: &super::capture::PalwGdnCaptureV2<'_>,
    node_slot: u32,
    head: u32,
    index: u32,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    checkpoint: &dyn Fn(u32) -> Option<(leg::PalwCheckpointLeafV2, Vec<Vec<u8>>)>,
) -> Result<PalwGdnTransitionRefutationV2, PalwGdnCourtErrorV2> {
    transition_refutation_v2(capture, node_slot, head, index, None, tile, checkpoint)
}

/// **[`gdn_transition_refutation_from_capture_v2`] with the ACCUSED checkpoint's witness supplied** — the per-position (held)
/// route, where the producer's retention serves no state and the accused bytes reach the accuser only as the producer's answers to
/// held data-availability demands ([`gdn_transition_demands_v2`] → [`gdn_checkpoint_witness_from_disclosures_v2`]). `checkpoint`
/// then serves only the PREDECESSOR, which the accuser recomputed itself. The court authenticates the supplied witness exactly as
/// it authenticates an assembled one.
pub fn gdn_transition_refutation_with_accused_v2(
    capture: &super::capture::PalwGdnCaptureV2<'_>,
    node_slot: u32,
    head: u32,
    index: u32,
    accused: PalwGdnCheckpointWitnessV2,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    checkpoint: &dyn Fn(u32) -> Option<(leg::PalwCheckpointLeafV2, Vec<Vec<u8>>)>,
) -> Result<PalwGdnTransitionRefutationV2, PalwGdnCourtErrorV2> {
    transition_refutation_v2(capture, node_slot, head, index, Some(accused), tile, checkpoint)
}

fn transition_refutation_v2(
    capture: &super::capture::PalwGdnCaptureV2<'_>,
    node_slot: u32,
    head: u32,
    index: u32,
    supplied: Option<PalwGdnCheckpointWitnessV2>,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    checkpoint: &dyn Fn(u32) -> Option<(leg::PalwCheckpointLeafV2, Vec<Vec<u8>>)>,
) -> Result<PalwGdnTransitionRefutationV2, PalwGdnCourtErrorV2> {
    let witness = |at: u32, positions: u32, layer: u16, selection: Option<u32>| -> Result<PalwGdnCheckpointWitnessV2, PalwGdnCourtErrorV2> {
        let (leaf, chunks) = checkpoint(at).ok_or_else(|| evidence("the capture lacks a checkpoint"))?;
        let opening = leg::step_opening_capped_v1(capture.checkpoint_leaf_hashes, u64::from(at), capture.max_leaves)?;
        let mut out = Vec::new();
        for flat in gdn_selected_delta_chunks_v2(&capture.binding.shape_profile, positions, layer, selection)? {
            out.push(gdn_chunk_witness_v2(&capture.binding.shape_profile, positions, &chunks, flat)?);
        }
        Ok(PalwGdnCheckpointWitnessV2 { leaf, opening, chunks: out })
    };
    transition_refutation_with_witness_v3(capture, node_slot, head, index, supplied, tile, &witness)
}

#[allow(clippy::too_many_arguments)]
fn transition_refutation_with_witness_v3(
    capture: &super::capture::PalwGdnCaptureV2<'_>,
    node_slot: u32, head: u32, index: u32,
    supplied: Option<PalwGdnCheckpointWitnessV2>,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    witness: &dyn Fn(u32, u32, u16, Option<u32>) -> Result<PalwGdnCheckpointWitnessV2, PalwGdnCourtErrorV2>,
) -> Result<PalwGdnTransitionRefutationV2, PalwGdnCourtErrorV2> {
    let binding = capture.binding;
    let profile = &binding.shape_profile;
    let (layer, selection, program) = transition_site_v2(profile, node_slot, head)?;
    let sliced = selection.is_some();
    let (_, to_first) = checkpoint_site_v2(binding, index)?;
    let last = coordinate_at_v2(binding, to_first - 1, node_slot, if sliced { head } else { 0 });
    let start = gdn_replay_start_v2(binding, &last)?;
    let accused = match supplied {
        Some(accused) => accused,
        None => witness(index, to_first, layer, selection)?,
    };
    let predecessor = match start.checkpoint_index {
        None => None,
        Some(prev) => Some(witness(prev, start.first_position, layer, selection)?),
    };
    let required = canonical_input_leaves_with_start_v2(
        profile,
        &binding.job_context,
        node_slot,
        &last,
        program,
        false,
        Some(start.first_position),
        None,
    )
    .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let inputs = gdn_input_rows_from_capture_v2(&required, capture.step_leaf_hashes, tile, capture.max_leaves)?;
    Ok(PalwGdnTransitionRefutationV2 {
        version: PALW_GDN_CHECKPOINT_EVIDENCE_VERSION_V2,
        binding: binding.clone(),
        node_slot,
        head: if sliced { head } else { 0 },
        accused,
        predecessor,
        inputs,
    })
}

/// The flat indices of `head`'s convolution-window chunks in the checkpoint at `positions` — the order the conv-window terminal
/// authenticates them in.
fn conv_window_chunks_v2(
    binding: &leg::PalwStepBindingV2,
    conv_slot: u32,
    head: u32,
    positions: u32,
) -> Result<Vec<u32>, PalwGdnCourtErrorV2> {
    conv_demand_borrow::collect_conv_chunks_v3(binding, conv_slot, head, positions)
}

/// **Assemble a convolution-window accusation from captured material.**
pub fn gdn_conv_window_refutation_from_capture_v2(
    capture: &super::capture::PalwGdnCaptureV2<'_>,
    conv_slot: u32,
    head: u32,
    index: u32,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    checkpoint: &dyn Fn(u32) -> Option<(leg::PalwCheckpointLeafV2, Vec<Vec<u8>>)>,
) -> Result<PalwGdnConvWindowRefutationV2, PalwGdnCourtErrorV2> {
    let binding = capture.binding;
    let profile = &binding.shape_profile;
    let (_, to_first) = checkpoint_site_v2(binding, index)?;
    let (leaf, chunks) = checkpoint(index).ok_or_else(|| evidence("the capture lacks the accused checkpoint"))?;
    let opening = leg::step_opening_capped_v1(capture.checkpoint_leaf_hashes, u64::from(index), capture.max_leaves)?;
    let mut witnesses = Vec::new();
    for flat in conv_window_chunks_v2(binding, conv_slot, head, to_first)? {
        witnesses.push(gdn_chunk_witness_v2(profile, to_first, &chunks, flat)?);
    }
    gdn_conv_window_refutation_with_accused_v2(
        capture,
        conv_slot,
        head,
        index,
        PalwGdnCheckpointWitnessV2 { leaf, opening, chunks: witnesses },
        tile,
    )
}

/// **[`gdn_conv_window_refutation_from_capture_v2`] with the accused checkpoint's witness supplied** — the per-position (held) route,
/// the witness built from the producer's held data-availability answers ([`gdn_conv_window_demands_v2`]).
pub fn gdn_conv_window_refutation_with_accused_v2(
    capture: &super::capture::PalwGdnCaptureV2<'_>,
    conv_slot: u32,
    head: u32,
    index: u32,
    accused: PalwGdnCheckpointWitnessV2,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
) -> Result<PalwGdnConvWindowRefutationV2, PalwGdnCourtErrorV2> {
    let binding = capture.binding;
    let required = gdn_conv_window_required_v2(binding, conv_slot, head, index)?;
    let inputs = gdn_input_rows_from_capture_v2(&required, capture.step_leaf_hashes, tile, capture.max_leaves)?;
    Ok(PalwGdnConvWindowRefutationV2 {
        version: PALW_GDN_CHECKPOINT_EVIDENCE_VERSION_V2,
        binding: binding.clone(),
        conv_slot,
        head,
        accused,
        inputs,
    })
}

// -------------------------------------------------------------------------------------------------------------------------------
// The per-position (held) route: the accused checkpoint's bytes through the EXISTING held data-availability court
// -------------------------------------------------------------------------------------------------------------------------------

/// **The held DA units a transition accusation against checkpoint `index` reads** — one `StateChunk` per selected delta chunk, in
/// the order the terminal authenticates them. A per-position class retains no checkpoint state (its leaves only), so an accuser
/// whose own recompute disagrees with checkpoint `index` demands exactly these of the producer through the existing held DA court
/// (`PalwDaUnitV1::Held` sessions, answered with `MaterialDisclosedV2`; no new object, tag, inner kind or table): an answer is the
/// producer's own bytes and paths, hash-checked against the claim's committed roots by that court
/// (`palw_held_da_check_disclosure_v1`), and silence is that court's DA-7 default against the producer. Derived from the binding
/// alone, exactly as the terminal derives them.
pub fn gdn_transition_demands_v2(
    binding: &leg::PalwStepBindingV2,
    node_slot: u32,
    head: u32,
    index: u32,
) -> Result<Vec<crate::palw_held_da_v1::PalwHeldMissingV1>, PalwGdnCourtErrorV2> {
    let (layer, selection, _) = transition_site_v2(&binding.shape_profile, node_slot, head)?;
    let (_, to_first) = checkpoint_site_v2(binding, index)?;
    Ok(gdn_selected_delta_chunks_v2(&binding.shape_profile, to_first, layer, selection)?
        .into_iter()
        .map(|chunk| crate::palw_held_da_v1::PalwHeldMissingV1::StateChunk { checkpoint: index, chunk })
        .collect())
}

/// **The held DA units a convolution-window accusation against checkpoint `index` reads** (`head`'s window chunks, in order).
pub fn gdn_conv_window_demands_v2(
    binding: &leg::PalwStepBindingV2,
    conv_slot: u32,
    head: u32,
    index: u32,
) -> Result<Vec<crate::palw_held_da_v1::PalwHeldMissingV1>, PalwGdnCourtErrorV2> {
    let (_, to_first) = checkpoint_site_v2(binding, index)?;
    Ok(conv_window_chunks_v2(binding, conv_slot, head, to_first)?
        .into_iter()
        .map(|chunk| crate::palw_held_da_v1::PalwHeldMissingV1::StateChunk { checkpoint: index, chunk })
        .collect())
}

/// **The accused checkpoint's witness from the producer's held DA answers** — `answers[i]` answers `demands[i]`, every answer names
/// the same checkpoint leaf and opening, and each chunk keeps the producer's own bytes and path. Pure re-packaging: nothing is
/// trusted here, because the terminal authenticates every chunk against the claim's committed checkpoint root
/// ([`check_gdn_transition_refutation_v2`] / [`check_gdn_conv_window_refutation_v2`]), and the DA court already checked each
/// answer by hash arithmetic when it accepted it.
pub fn gdn_checkpoint_witness_from_disclosures_v2(
    demands: &[crate::palw_held_da_v1::PalwHeldMissingV1],
    answers: &[crate::palw_held_da_v1::PalwHeldDisclosureV1],
) -> Result<PalwGdnCheckpointWitnessV2, PalwGdnCourtErrorV2> {
    use crate::palw_held_da_v1::{PalwHeldDisclosureV1 as D, PalwHeldMissingV1 as M};
    if demands.is_empty() || demands.len() != answers.len() {
        return Err(evidence("one held DA answer per demanded checkpoint chunk"));
    }
    let mut anchor: Option<&crate::palw_attn_court_v1::PalwAttnCheckpointAnchorV1> = None;
    let mut chunks = Vec::with_capacity(answers.len());
    for (demand, answer) in demands.iter().zip(answers) {
        let (M::StateChunk { checkpoint, chunk }, D::StateChunk { anchor: a, chunk: opening }) = (demand, answer) else {
            return Err(evidence("a checkpoint witness is built only from state-chunk answers"));
        };
        if a.leaf.checkpoint_index != *checkpoint || opening.chunk_index != *chunk {
            return Err(evidence("a held DA answer is of another unit than its demand"));
        }
        match anchor {
            None => anchor = Some(a),
            Some(first) if first == a => {}
            Some(_) => return Err(evidence("the answers name different checkpoint leaves")),
        }
        chunks.push(PalwGdnStateChunkWitnessV2 {
            chunk_index: opening.chunk_index,
            bytes: opening.chunk_bytes.clone(),
            siblings: opening.siblings.clone(),
        });
    }
    let anchor = anchor.ok_or_else(|| evidence("no held DA answer"))?;
    Ok(PalwGdnCheckpointWitnessV2 { leaf: anchor.leaf.clone(), opening: anchor.opening.clone(), chunks })
}

#[cfg(test)]
#[path = "transition_tests.rs"]
mod tests;
