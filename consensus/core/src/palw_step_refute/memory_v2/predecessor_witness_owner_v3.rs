//! Borrowed, already-replayed predecessor -> selected witness only.
//! Prefix/cache/state serialization allocations are not admitted by this owner.
use super::*;
use crate::palw_native_resource_v1::NativeHeapV1;
use std::{mem::size_of, ops::Deref};

const PATH: &str = "GDN borrowed predecessor witness construction";
fn overflow<T>() -> PalwGdnCourtErrorV2 {
    NativeCoverageV1::overflow::<T>(PATH).into()
}
fn add(a: u64, b: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    a.checked_add(b).ok_or_else(overflow::<PalwGdnCheckpointWitnessV2>)
}
fn array<T>(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    n.checked_mul(size_of::<T>() as u64).ok_or_else(overflow::<T>)
}
fn heap<T: NativeHeapV1>(value: &T) -> Result<u64, PalwGdnCourtErrorV2> {
    let request = value.native_heap_v1(PATH);
    request.bytes().ok_or_else(|| request.into())
}
fn push_array<T>(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    if n == 0 {
        return Ok(0);
    }
    let minimum = if size_of::<T>() == 1 {
        8
    } else if size_of::<T>() <= 1024 {
        4
    } else {
        1
    };
    let cap = n.max(minimum).checked_next_power_of_two().ok_or_else(overflow::<T>)?;
    array::<T>(if cap > minimum { add(cap, cap / 2)? } else { cap })
}
fn tree_peak(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    array::<Hash64>(add(n, n.div_ceil(2))?)
}
/// The frozen state_chunk_leaf_hash_v4 Writer starts at 256 and extends map,
/// slice, block, length prefix, then one bulk borrowed chunk, in that order.
fn chunk_writer_peak(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    let required = add((Hash64::default().as_byte_slice().len() + 3 * size_of::<u32>()) as u64, n)?;
    if required <= 256 { Ok(256) } else { add(256, required.max(512)) }
}

/// The existing caller's exact borrowed source stays live through construction.
/// Native layout values precede their metadata lease inside metadata's owner.
pub struct PalwGdnPredecessorRequestV3<'a> {
    profile: &'a crate::palw_step::PalwShapeProfileV3,
    checkpoint_hashes: &'a [Hash64],
    max_leaves: u64,
    leaf: &'a leg::PalwCheckpointLeafV2,
    chunks: &'a [Vec<u8>],
    metadata: recurrence_layout_owner::RetainedRecurrenceSectionV3,
    positions: u32,
    layer: u16,
    head: Option<u32>,
    persistent: u64,
    workspace: u64,
    selected: u64,
}
pub struct PalwGdnPredecessorOwnerV3 {
    _lease: Option<NativeDemandLeaseV1>,
}
/// Field order releases every copied path/chunk before its independent lease.
pub struct PalwGdnPredecessorBuiltV3 {
    witness: PalwGdnCheckpointWitnessV2,
    owner: PalwGdnPredecessorOwnerV3,
}
impl Deref for PalwGdnPredecessorBuiltV3 {
    type Target = PalwGdnCheckpointWitnessV2;
    fn deref(&self) -> &Self::Target {
        &self.witness
    }
}
impl PalwGdnPredecessorBuiltV3 {
    pub(crate) fn into_parts_v3(self) -> (PalwGdnCheckpointWitnessV2, PalwGdnPredecessorOwnerV3) {
        (self.witness, self.owner)
    }
}

/// This route quotes actual held-map construction, never an assumed prefix
/// bound. Unsupported layouts refuse rather than using an unreserved fallback.
pub fn gdn_predecessor_witness_request_v3<'a>(
    capture: &PalwGdnCaptureV2<'a>,
    index: u32,
    positions: u32,
    layer: u16,
    head: Option<u32>,
    leaf: &'a leg::PalwCheckpointLeafV2,
    chunks: &'a [Vec<u8>],
) -> Result<PalwGdnPredecessorRequestV3<'a>, PalwGdnCourtErrorV2> {
    let profile = &capture.binding.shape_profile;
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    if !maps::palw_profile_is_held_v4(profile) {
        return Err(
            NativeCoverageV1::unsupported::<PalwGdnCheckpointWitnessV2>(PATH, "retained predecessor requires the held map").into()
        );
    }
    let n = capture.checkpoint_leaf_hashes.len() as u64;
    if n == 0 || n > capture.max_leaves || u64::from(index) >= n || leaf.checkpoint_index != index {
        return Err(evidence("predecessor is outside the committed checkpoint leg"));
    }
    let metadata = gdn_recurrence_section_retained_v3(profile, positions)?;
    let layout = metadata.layout_v3().ok_or_else(|| evidence("predecessor has no held recurrence layout"))?;
    let m = chunks.len() as u64;
    if m != metadata.count || m != u64::from(leaf.state_chunk_count) {
        return Err(evidence("replayed predecessor chunk count differs from native layout"));
    }
    let s = u64::from(layout.slice_count());
    let max_blocks = (0..layout.slice_count()).filter_map(|slice| layout.block_count(slice)).max().unwrap_or(0) as u64;
    let max_chunk = chunks.iter().map(|c| c.len() as u64).max().unwrap_or(0);
    let checkpoint_depth = u64::from(leg::state_v4_path_len(n, u64::from(index)));
    let mut persistent = add(heap(leaf)?, push_array::<Hash64>(checkpoint_depth)?)?;
    let mut selected = 0u64;
    visit_selected_delta_chunks_at_v3(&geometry, layer, head, metadata.offset, |flat| {
        let bytes = chunks.get(flat as usize).ok_or_else(|| evidence("replay lacks a selected predecessor chunk"))?;
        let address = layout.address(u64::from(flat)).ok_or_else(|| evidence("selected predecessor chunk has no held address"))?;
        let depth = u64::from(leg::state_v4_path_len(u64::from(address.block_count), u64::from(address.block)))
            + u64::from(leg::state_v4_path_len(u64::from(address.slice_count), u64::from(address.slice)));
        persistent = add(persistent, add(bytes.len() as u64, push_array::<Hash64>(depth)?)?)?;
        selected = add(selected, 1)?;
        Ok(())
    })?;
    persistent = add(persistent, push_array::<PalwGdnStateChunkWitnessV2>(selected)?)?;
    // Per selected chunk: full leaf hashes; held top array; one slice hashes;
    // sequential slice/top Merkle levels; temporary top path. Prior paths and
    // cloned chunk bytes are in persistent, not multiplied into workspace.
    // Result<Vec<_>> leaf collection uses a conservative typed push envelope.
    let mut state_peak = push_array::<Hash64>(m)?;
    state_peak = add(state_peak, array::<Hash64>(s)?)?;
    state_peak = add(state_peak, array::<Hash64>(max_blocks)?)?;
    state_peak = add(state_peak, tree_peak(max_blocks)?)?;
    state_peak = add(state_peak, tree_peak(s)?)?;
    let top_depth = (0..s).map(|i| u64::from(leg::state_v4_path_len(s, i))).max().unwrap_or(0);
    state_peak = add(state_peak, push_array::<Hash64>(top_depth)?)?;
    state_peak = add(state_peak, chunk_writer_peak(max_chunk)?)?;
    // Existing map helpers derive their own temporary layout while our quoted
    // native layout remains borrowed; that additional real layout is admitted.
    let workspace = add(metadata.layer_bytes_v3(), state_peak.max(add(tree_peak(n)?, 256)?))?;
    Ok(PalwGdnPredecessorRequestV3 {
        profile,
        checkpoint_hashes: capture.checkpoint_leaf_hashes,
        max_leaves: capture.max_leaves,
        leaf,
        chunks,
        metadata,
        positions,
        layer,
        head,
        persistent,
        workspace,
        selected,
    })
}
impl PalwGdnPredecessorRequestV3<'_> {
    pub fn requests_v3(&self) -> (u64, u64) {
        (self.persistent, self.workspace)
    }
    /// A separate stage from source/metadata quote: tests can occupy the budget
    /// after requesting, then target this exact copy/tree admission.
    pub fn build_v3(self) -> Result<PalwGdnPredecessorBuiltV3, PalwGdnCourtErrorV2> {
        let owner = PalwGdnPredecessorOwnerV3 { _lease: reserve_native_requests_v1(self.persistent)? };
        let _workspace = reserve_native_requests_v1(self.workspace)?;
        let opening = leg::step_opening_capped_v1(self.checkpoint_hashes, u64::from(self.leaf.checkpoint_index), self.max_leaves)?;
        let geometry = PalwGdnStateGeometryV2::of(self.profile).map_err(evidence)?;
        let mut chunks = Vec::new();
        visit_selected_delta_chunks_at_v3(&geometry, self.layer, self.head, self.metadata.offset, |flat| {
            chunks.push(gdn_chunk_witness_v2(self.profile, self.positions, self.chunks, flat)?);
            Ok(())
        })?;
        if chunks.len() as u64 != self.selected {
            return Err(evidence("predecessor selection changed during construction"));
        }
        let witness = PalwGdnCheckpointWitnessV2 { leaf: self.leaf.clone(), opening, chunks };
        let mut actual = add(heap(&witness.leaf)?, array::<Hash64>(witness.opening.siblings.capacity() as u64)?)?;
        actual = add(actual, array::<PalwGdnStateChunkWitnessV2>(witness.chunks.capacity() as u64)?)?;
        for chunk in &witness.chunks {
            actual = add(actual, add(chunk.bytes.capacity() as u64, array::<Hash64>(chunk.siblings.capacity() as u64)?)?)?;
        }
        if actual > self.persistent {
            return Err(NativeCoverageV1::unsupported::<PalwGdnCheckpointWitnessV2>(
                PATH,
                "native output exceeded admitted typed roles",
            )
            .into());
        }
        Ok(PalwGdnPredecessorBuiltV3 { witness, owner })
    }
}
