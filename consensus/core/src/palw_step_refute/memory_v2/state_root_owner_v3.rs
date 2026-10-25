//! Hash already-serialized held state under an admitted native workspace.
//! This does not own or admit prefix execution, serialization, or source chunks.
use super::*;
use std::mem::size_of;

const PATH: &str = "GDN borrowed held-state root workspace";
fn overflow<T>() -> PalwGdnCourtErrorV2 {
    NativeCoverageV1::overflow::<T>(PATH).into()
}
fn add(a: u64, b: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    a.checked_add(b).ok_or_else(overflow::<Hash64>)
}
fn array<T>(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    n.checked_mul(size_of::<T>() as u64).ok_or_else(overflow::<T>)
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
/// Native Writer::new (256), hash64, u32 slice, u32 block, then
/// Writer::bytes (u32 length followed by one bulk extend_from_slice).
fn chunk_writer_peak(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    let required = add((Hash64::default().as_byte_slice().len() + 3 * size_of::<u32>()) as u64, n)?;
    if required <= 256 { Ok(256) } else { add(256, required.max(512)) }
}

/// The real layout retires before its metadata ticket. Source chunks are only
/// borrowed and remain a separate input ownership/admission obligation.
pub struct PalwGdnStateRootRequestV3<'a> {
    profile: &'a crate::palw_step::PalwShapeProfileV3,
    positions: u32,
    chunks: &'a [Vec<u8>],
    _layout: maps::PalwStateLayoutV4,
    _metadata: Option<NativeDemandLeaseV1>,
    metadata_bytes: u64,
    workspace_bytes: u64,
}

/// Derive only native layout and checked type-sized spans, never a state root.
pub fn gdn_state_root_request_v3<'a>(
    profile: &'a crate::palw_step::PalwShapeProfileV3,
    positions: u32,
    chunks: &'a [Vec<u8>],
) -> Result<PalwGdnStateRootRequestV3<'a>, PalwGdnCourtErrorV2> {
    if !maps::palw_profile_is_held_v4(profile) {
        return Err(NativeCoverageV1::unsupported::<Hash64>(PATH, "retained state-root consumer requires the held map").into());
    }
    // Match the existing Base0 held-root wrapper: byte cap before layout/count.
    if chunks.iter().any(|c| c.len() > leg::PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES) {
        return Err(evidence("a state chunk is outside the leg's byte cap"));
    }
    let attention =
        (0..profile.layer_count).filter(|l| profile.layer_kind(*l) == crate::palw_step::PalwLayerKindV1::Attention).count() as u64;
    let recurrence =
        (0..profile.layer_count).filter(|l| profile.layer_kind(*l) == crate::palw_step::PalwLayerKindV1::GatedDeltaNet).count() as u64;
    // The real A/G layer counts bound the map constructor's two Vecs, including
    // inactive recurrence checkpoints. No per-prefix byte multiplier is used.
    let metadata_bytes = add(push_array::<u16>(attention)?, push_array::<u16>(recurrence)?)?;
    let metadata = reserve_native_requests_v1(metadata_bytes)?;
    let layout = maps::palw_state_layout_v4(profile, positions).map_err(|e| leg::PalwStepLegError::HeldLayout(e.to_string()))?;
    let m = chunks.len() as u64;
    if m != layout.chunk_count() {
        return Err(leg::PalwStepLegError::StateChunksOutOfRange { got: chunks.len(), max: layout.chunk_count() as usize }.into());
    }
    // Native helpers obtain each flat address before hashing its leaf.
    for flat in 0..m {
        layout.address(flat).ok_or(leg::PalwStepLegError::LeafIndexOutOfRange { index: flat, count: layout.chunk_count() })?;
    }
    let s = u64::from(layout.slice_count());
    let b = (0..layout.slice_count()).filter_map(|slice| layout.block_count(slice)).max().unwrap_or(0) as u64;
    let mut folds = add(array::<Hash64>(s)?, array::<Hash64>(b)?)?;
    folds = add(folds, tree_peak(b)?)?;
    folds = add(folds, tree_peak(s)?)?;
    folds = add(folds, 256)?; // state_top_leaf_v4 Writer (two u32s + Hash64)
    let max_chunk = chunks.iter().map(|c| c.len() as u64).max().unwrap_or(0);
    // Leaf collection can use push growth through Result's iterator adapter.
    // Native leaf/root helpers have one additional temporary layout at a time
    // while this request's layout remains live. Fold phases are sequential;
    // their role sum is a conservative bound, not minimal allocation or RSS.
    let workspace_bytes = add(metadata_bytes, add(push_array::<Hash64>(m)?, folds.max(chunk_writer_peak(max_chunk)?))?)?;
    Ok(PalwGdnStateRootRequestV3 { profile, positions, chunks, _layout: layout, _metadata: metadata, metadata_bytes, workspace_bytes })
}
impl PalwGdnStateRootRequestV3<'_> {
    pub fn metadata_bytes_v3(&self) -> u64 {
        self.metadata_bytes
    }
    pub fn workspace_bytes_v3(&self) -> u64 {
        self.workspace_bytes
    }
    /// All native nested hash arrays retire inside the same root function
    /// before this workspace ticket. Only its scalar Hash64 is returned.
    pub fn root_v3(self) -> Result<Hash64, PalwGdnCourtErrorV2> {
        let _workspace = reserve_native_requests_v1(self.workspace_bytes)?;
        maps::palw_state_chunks_root_for_map_v1(self.profile, self.positions, self.chunks).map_err(Into::into)
    }
}
