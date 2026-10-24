//! Admitted convolution construction requests sharing the allocating builder
//! visitor and native source/range/coordinate helpers.
//! No transition predecessor or prefix-recompute bound is implied here.
use super::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1};
use crate::palw_step::PalwCanonicalStepRankV3;
use std::mem::size_of;

const PATH: &str = "GDN convolution refutation typed requests";
fn overflow<T>() -> PalwGdnCourtErrorV2 {
    NativeCoverageV1::overflow::<T>(PATH).into()
}
fn add(a: u64, b: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    a.checked_add(b).ok_or_else(overflow::<PalwGdnConvRefutationRequestsV3>)
}
fn array<T>(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    n.checked_mul(size_of::<T>() as u64).ok_or_else(overflow::<T>)
}
fn heap<T: NativeHeapV1>(value: &T) -> Result<u64, PalwGdnCourtErrorV2> {
    let quote = value.native_heap_v1(PATH);
    quote.bytes().ok_or_else(|| PalwGdnCourtErrorV2::Resource(quote))
}
/// Typed push growth and old/new overlap; do not treat Vec::with_capacity or
/// Vec::clone as push growth. These are logical array requests, not RSS.
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
    let capacity = n.max(minimum).checked_next_power_of_two().ok_or_else(overflow::<T>)?;
    array::<T>(if capacity > minimum { add(capacity, capacity / 2)? } else { capacity })
}

/// One actual required source row. It lends the existing signed binding; no
/// coordinates, indices, source widths or tile values are copied into a Vec.
#[derive(Clone, Copy)]
pub struct PalwGdnConvRequiredRowV3<'a> {
    binding: &'a leg::PalwStepBindingV2,
    rank: &'a PalwCanonicalStepRankV3<'a>,
    absolute: u32,
    source_slot: u32,
    first_tile: u32,
    last_tile: u32,
}
impl PalwGdnConvRequiredRowV3<'_> {
    pub fn tile_count_v3(&self) -> u64 {
        u64::from(self.last_tile) - u64::from(self.first_tile) + 1
    }
    pub fn visit_tiles_v3(
        &self,
        mut visit: impl FnMut(u64, PalwStepCoordinateV1) -> Result<(), PalwGdnCourtErrorV2>,
    ) -> Result<(), PalwGdnCourtErrorV2> {
        let coord0 = coordinate_at_v2(self.binding, self.absolute, self.source_slot, 0);
        for tile in self.first_tile..=self.last_tile {
            let coord = PalwStepCoordinateV1 { tile_index: tile, ..coord0 };
            let index = self.rank.index_v3(&coord).ok_or(PalwStepRefuteError::Unadjudicable)?;
            visit(index, coord)?;
        }
        Ok(())
    }
}

/// The only required-row traversal after integration: existing allocating
/// `gdn_conv_window_required_v2` must consume this visitor as well as the quote.
/// All source widths/head gathers remain native private helper decisions.
pub fn visit_gdn_conv_required_rows_v3(
    binding: &leg::PalwStepBindingV2,
    conv_slot: u32,
    head: u32,
    checkpoint_index: u32,
    mut visit: impl FnMut(PalwGdnConvRequiredRowV3<'_>) -> Result<(), PalwGdnCourtErrorV2>,
) -> Result<(), PalwGdnCourtErrorV2> {
    let geometry = PalwGdnStateGeometryV2::of(&binding.shape_profile).map_err(evidence)?;
    let (_, sources) = conv_sources_v2(&binding.shape_profile, conv_slot, &geometry)?;
    let heads = if geometry.layout == crate::palw_gdn_state_map_v2::PalwGdnStateLayoutV2::FullConvV1 { 1 } else { geometry.heads };
    if head >= heads {
        return Err(evidence("accused window head is outside the layout"));
    }
    let (_, to_first) = checkpoint_site_v2(binding, checkpoint_index)?;
    let rank = PalwCanonicalStepRankV3::new_v3(&binding.shape_profile, &binding.job_context)?;
    for absolute in to_first.saturating_sub(geometry.conv_kernel)..to_first {
        for (which, (slot, tile, _)) in sources.iter().enumerate() {
            let (lo, hi) = conv_range_v2(&geometry, head, which);
            let last = hi.checked_sub(1).ok_or(PalwStepRefuteError::Unadjudicable)? / tile;
            let first_tile = u32::try_from(lo / tile).map_err(|_| overflow::<PalwStepCoordinateV1>())?;
            let last_tile = u32::try_from(last).map_err(|_| overflow::<PalwStepCoordinateV1>())?;
            if first_tile > last_tile {
                return Err(PalwStepRefuteError::Unadjudicable.into());
            }
            visit(PalwGdnConvRequiredRowV3 { binding, rank: &rank, absolute, source_slot: *slot, first_tile, last_tile })?;
        }
    }
    Ok(())
}

/// Quote only the additional convolution roles; an accused-witness copy has
/// its own independently retained owner. Requests do not themselves allocate,
/// admit, authenticate or promise a whole-model/whole-process memory bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGdnConvRefutationRequestsV3 {
    pub persistent_bytes: u64,
    pub workspace_bytes: u64,
    pub required_rows: u64,
    pub required_tiles: u64,
    pub required_runs: u64,
}

/// Real tile callback is borrowed. It must be the same immutable source used
/// later for tile_source's clone, not a second fabricated geometry or provider.
pub fn gdn_conv_refutation_requests_v3<'m>(
    capture: &super::super::capture::PalwGdnCaptureV2<'_>,
    conv_slot: u32,
    head: u32,
    checkpoint_index: u32,
    mut tile: impl FnMut(u64, PalwStepCoordinateV1) -> Option<&'m leg::PalwStepTileLeafV1>,
) -> Result<PalwGdnConvRefutationRequestsV3, PalwGdnCourtErrorV2> {
    let n = u64::try_from(capture.step_leaf_hashes.len()).map_err(|_| overflow::<Hash64>())?;
    if n == 0 || n > capture.max_leaves {
        return Err(leg::PalwStepLegError::LeafCountOutOfRange { got: n, max: capture.max_leaves }.into());
    }
    let mut depth = 0u64;
    let mut width = n;
    while width > 1 {
        depth = add(depth, 1)?;
        width = width.div_ceil(2);
    }
    // A range needs at most two siblings per actual tree level, the existing
    // range-opening rule. Runs open sequentially: reserve the tree peak once.
    let path_cap = depth.checked_mul(2).ok_or_else(overflow::<Hash64>)?;
    let tree = array::<Hash64>(add(n, n.div_ceil(2))?)?;
    let mut q = PalwGdnConvRefutationRequestsV3 {
        persistent_bytes: heap(capture.binding)?,
        workspace_bytes: 0,
        required_rows: 0,
        required_tiles: 0,
        required_runs: 0,
    };
    let mut required_inner = 0u64;
    let mut runs_peak = 0u64;
    visit_gdn_conv_required_rows_v3(capture.binding, conv_slot, head, checkpoint_index, |row| {
        q.required_rows = add(q.required_rows, 1)?;
        let count = row.tile_count_v3();
        q.required_tiles = add(q.required_tiles, count)?;
        required_inner = add(required_inner, push_array::<(u64, PalwStepCoordinateV1)>(count)?)?;
        q.persistent_bytes = add(q.persistent_bytes, array::<leg::PalwStepTileLeafV1>(count)?)?;
        let mut runs = 0u64;
        let mut previous = None;
        row.visit_tiles_v3(|index, coordinate| {
            if index >= n {
                return Err(PalwStepRefuteError::Unadjudicable.into());
            }
            if previous.and_then(|p: u64| p.checked_add(1)) != Some(index) {
                runs = add(runs, 1)?;
            }
            previous = Some(index);
            let source = tile(index, coordinate).ok_or_else(|| evidence("the capture lacks a required input tile"))?;
            if source.coord != coordinate {
                return Err(evidence("input source changes the requested tile coordinate"));
            }
            q.persistent_bytes = add(q.persistent_bytes, heap(source)?)?;
            Ok(())
        })?;
        q.required_runs = add(q.required_runs, runs)?;
        runs_peak = runs_peak.max(push_array::<(usize, usize)>(runs)?);
        q.persistent_bytes = add(q.persistent_bytes, array::<Vec<Hash64>>(runs)?)?;
        let paths = push_array::<Hash64>(path_cap)?.checked_mul(runs).ok_or_else(overflow::<Hash64>)?;
        q.persistent_bytes = add(q.persistent_bytes, paths)?;
        Ok(())
    })?;
    q.persistent_bytes = add(q.persistent_bytes, array::<PalwStepInputRowV1>(q.required_rows)?)?;
    let required_outer = push_array::<Vec<(u64, PalwStepCoordinateV1)>>(q.required_rows)?;
    // step_merkle_leaf's actual Writer starts at 256 and writes 8+64 bytes;
    // parent hashing uses a stack hash state, not another native input clone.
    q.workspace_bytes = add(required_outer, add(required_inner, add(runs_peak, add(tree, 256)?)?)?)?;
    Ok(q)
}

/// Construction requests stay owned across required/input/path building. The
/// separately admitted rank contexts own their own actual cached arrays.
pub struct PalwGdnConvAdmissionV3 {
    request: PalwGdnConvRefutationRequestsV3,
    persistent: Option<NativeDemandLeaseV1>,
    _workspace: Option<NativeDemandLeaseV1>,
}
pub struct PalwGdnConvOwnerV3 {
    _persistent: Option<NativeDemandLeaseV1>,
}
impl PalwGdnConvRefutationRequestsV3 {
    pub fn reserve_v3(self) -> Result<PalwGdnConvAdmissionV3, PalwGdnCourtErrorV2> {
        let persistent = reserve_native_requests_v1(self.persistent_bytes)?;
        let workspace = reserve_native_requests_v1(self.workspace_bytes)?;
        Ok(PalwGdnConvAdmissionV3 { request: self, persistent, _workspace: workspace })
    }
}
impl PalwGdnConvAdmissionV3 {
    /// Borrowed validation keeps the ticket in the caller on error. The caller
    /// declares the built value after admission, so it retires before admission
    /// on refusal; do not consume this admission in a fallible finish method.
    pub fn validate_v3(&self, output: &PalwGdnConvWindowRefutationV2) -> Result<(), PalwGdnCourtErrorV2> {
        let PalwGdnConvWindowRefutationV2 { version: _, binding, conv_slot: _, head: _, accused: _, inputs } = output;
        let mut bytes = add(heap(binding)?, array::<PalwStepInputRowV1>(inputs.capacity() as u64)?)?;
        let (mut tiles, mut runs) = (0u64, 0u64);
        for row in inputs {
            let PalwStepInputRowV1 { preimages, run_siblings } = row;
            tiles = add(tiles, preimages.len() as u64)?;
            runs = add(runs, run_siblings.len() as u64)?;
            bytes = add(bytes, array::<leg::PalwStepTileLeafV1>(preimages.capacity() as u64)?)?;
            bytes = add(bytes, array::<Vec<Hash64>>(run_siblings.capacity() as u64)?)?;
            for tile in preimages {
                bytes = add(bytes, heap(tile)?)?;
            }
            for path in run_siblings {
                bytes = add(bytes, array::<Hash64>(path.capacity() as u64)?)?;
            }
        }
        if bytes > self.request.persistent_bytes
            || inputs.len() as u64 != self.request.required_rows
            || tiles != self.request.required_tiles
            || runs != self.request.required_runs
        {
            return Err(NativeCoverageV1::unsupported::<PalwGdnConvWindowRefutationV2>(
                PATH,
                "native conv output exceeds its admitted roles/counts",
            )
            .into());
        }
        Ok(())
    }
    /// Infallible transfer only after successful validation. Temporary scratch
    /// retires while the destination receives this persistent input ticket.
    pub fn into_owner_v3(self) -> PalwGdnConvOwnerV3 {
        PalwGdnConvOwnerV3 { _persistent: self.persistent }
    }
}
