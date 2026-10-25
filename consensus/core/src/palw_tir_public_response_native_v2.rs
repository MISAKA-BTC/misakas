//! Native admission for bounded public response decoding and authentication.
//! Source identity/data membership only, never computation validity.
use super::*;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, NativeWireReaderV1, reserve_native_requests_v1,
};
use crate::palw_step::PalwStepCoordinateV1;
use std::mem::size_of;
type Error = NativeDecodeErrorV1<String>;
const COMPONENT: &str = "TIR bounded response native admission";
fn bad(why: impl Into<String>) -> Error {
    Error::Decode(why.into())
}
fn capacity<T: ?Sized>(why: &str) -> Error {
    Error::Resource(NativeCoverageV1::unsupported::<T>(COMPONENT, why))
}
fn overflow<T>() -> Error {
    Error::Resource(NativeCoverageV1::overflow::<T>(COMPONENT))
}
fn add(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or_else(overflow::<PalwTirPublicResponseV1>)
}
fn slots<T>(n: usize) -> Result<u64, Error> {
    (n as u64).checked_mul(size_of::<T>() as u64).ok_or_else(overflow::<T>)
}
fn io_fault(why: io::Error) -> Error {
    match why.kind() {
        io::ErrorKind::OutOfMemory => capacity::<PalwTirPublicResponseV1>("response allocation or arithmetic capacity refused"),
        _ => bad(why.to_string()),
    }
}
fn scan_vec<R: Read, T>(r: &mut NativeWireReaderV1<R>, maximum: usize, wire_size: usize) -> Result<u64, Error> {
    let count = r.word().map_err(io_fault)? as usize;
    if count > maximum {
        return Err(bad("TIR public vector exceeds its bound before allocation"));
    }
    let wire = slots::<u8>(count)?.checked_mul(wire_size as u64).ok_or_else(overflow::<T>)?;
    r.skip(wire).map_err(io_fault)?;
    slots::<T>(count)
}
fn scan_opening<R: Read>(r: &mut NativeWireReaderV1<R>) -> Result<u64, Error> {
    r.skip((size_of::<u64>() + size_of::<Hash64>()) as u64).map_err(io_fault)?;
    scan_vec::<_, Hash64>(r, PATH, 64)
}
fn scan_leaf<R: Read>(r: &mut NativeWireReaderV1<R>, maximum: usize) -> Result<u64, Error> {
    let _version = u16::deserialize_reader(r).map_err(io_fault)?;
    let PalwStepCoordinateV1 { call_index: _, node_slot: _, position: _, tile_index: _ } =
        PalwStepCoordinateV1::deserialize_reader(r).map_err(io_fault)?;
    let _value_count = u32::deserialize_reader(r).map_err(io_fault)?;
    scan_vec::<_, u8>(r, maximum, 1)
}
/// Manual bounded Borsh scanner: each tag follows the one response decoder's
/// exact fields; every vector is checked against format and remaining bytes.
/// No decoded response, collection, serialized-size ratio or peer quote exists.
fn scan_response(bytes: &[u8], maximum: usize) -> Result<u64, Error> {
    let mut r = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    let tag = r.byte().map_err(io_fault)?;
    let heap = match tag {
        0 | 3 => {
            add(scan_vec::<_, Hash64>(&mut r, PALW_TIR_PUBLIC_FRONTIER_HASHES_V1, 64)?, scan_vec::<_, Hash64>(&mut r, PATH, 64)?)?
        }
        1 => add(scan_leaf(&mut r, maximum)?, scan_opening(&mut r)?)?,
        2 => {
            let n = r.word().map_err(io_fault)? as usize;
            if n > RUN {
                return Err(bad("TIR public run exceeds the native run bound"));
            }
            let mut heap = slots::<PalwStepTileLeafV1>(n)?;
            for _ in 0..n {
                heap = add(heap, scan_leaf(&mut r, maximum)?)?;
            }
            r.skip(8).map_err(io_fault)?;
            heap = add(heap, scan_vec::<_, Hash64>(&mut r, RUN, 64)?)?;
            add(heap, scan_vec::<_, Hash64>(&mut r, PATH * 2, 64)?)?
        }
        4 => {
            r.skip(64).map_err(io_fault)?;
            let a = scan_opening(&mut r)?;
            let b = scan_vec::<_, Hash64>(&mut r, PALW_TIR_PUBLIC_FRONTIER_HASHES_V1, 64)?;
            add(add(a, b)?, scan_vec::<_, Hash64>(&mut r, PATH, 64)?)?
        }
        5 => {
            r.skip(64).map_err(io_fault)?;
            let a = scan_opening(&mut r)?;
            let b = scan_vec::<_, i32>(&mut r, TILE_LANES, 4)?;
            add(add(a, b)?, scan_opening(&mut r)?)?
        }
        6 => {
            r.skip(64).map_err(io_fault)?;
            scan_vec::<_, u32>(&mut r, PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize, 4)?
        }
        _ => return Err(bad("unknown TIR public response")),
    };
    if r.remaining() != 0 {
        return Err(bad("TIR public response has trailing bytes"));
    }
    Ok(heap)
}
fn response_heap(response: &PalwTirPublicResponseV1) -> Result<u64, Error> {
    fn path(p: &PalwStepOpeningV1) -> Result<u64, Error> {
        slots::<Hash64>(p.siblings.len())
    }
    fn leaf(p: &PalwStepTileLeafV1) -> Result<u64, Error> {
        slots::<u8>(p.values_le.len())
    }
    use PalwTirPublicResponseV1 as R;
    match response {
        R::StepNode { frontier, siblings } | R::RowsNode { frontier, siblings } => {
            add(slots::<Hash64>(frontier.len())?, slots::<Hash64>(siblings.len())?)
        }
        R::StepLeaf { preimage, opening } => add(leaf(preimage)?, path(opening)?),
        R::StepRun { preimages, range } => {
            let mut n = slots::<PalwStepTileLeafV1>(preimages.len())?;
            for p in preimages {
                n = add(n, leaf(p)?)?;
            }
            add(add(n, slots::<Hash64>(range.leaf_hashes.len())?)?, slots::<Hash64>(range.siblings.len())?)
        }
        R::TileNode { row_root: _, row_opening, frontier, siblings } => {
            add(add(path(row_opening)?, slots::<Hash64>(frontier.len())?)?, slots::<Hash64>(siblings.len())?)
        }
        R::TraceTile { row_root: _, row_opening, lanes, opening } => {
            add(add(path(row_opening)?, slots::<i32>(lanes.len())?)?, path(opening)?)
        }
        R::Ids { rows_root: _, ids } => slots::<u32>(ids.len()),
    }
}
fn leaf_writer(p: &PalwStepTileLeafV1) -> Result<u64, Error> {
    // Writer starts256, then version + context/class hashes + coordinate4u32
    // + valuecount + u32 byte-length + actual bytes. Old/new buffers may coexist.
    let span = p.values_le.len().checked_add(2 + 2 * 64 + 4 * 4 + 4 + 4).ok_or_else(overflow::<u8>)?;
    let envelope = span.max(256).checked_next_power_of_two().ok_or_else(overflow::<u8>)?;
    add(slots::<u8>(envelope)?, slots::<u8>(envelope)?)
}
fn auth_scratch(response: &PalwTirPublicResponseV1) -> Result<u64, Error> {
    use PalwTirPublicResponseV1 as R;
    match response {
        R::StepNode { frontier, .. } | R::RowsNode { frontier, .. } | R::TileNode { frontier, .. } => {
            add(slots::<Hash64>(frontier.len())?, slots::<Hash64>(frontier.len().div_ceil(2))?)
        }
        R::StepLeaf { preimage, .. } => leaf_writer(preimage),
        R::StepRun { preimages, range } => {
            let writer = preimages.iter().try_fold(0, |peak, p| Ok::<_, Error>(peak.max(leaf_writer(p)?)))?;
            let n = range.leaf_hashes.len();
            // Native range fold keeps current nodes and capacity n/2+2 next;
            // later widths cannot increase this peak (n=1 stays1, n=2 <=2).
            let fold = if n == 0 { 0 } else { add(slots::<Hash64>(n)?, slots::<Hash64>(n / 2 + 2)?)? };
            Ok(writer.max(fold))
        }
        R::TraceTile { lanes, .. } => slots::<i32>(lanes.len()),
        R::Ids { .. } => Ok(0),
    }
}
impl PalwTirPublicSourceV1 {
    pub fn decode_response_native_v2(
        &self,
        request: &PalwTirPublicRequestV1,
        bytes: &[u8],
    ) -> Result<NativeRetainedV1<PalwTirPublicResponseV1>, Error> {
        let bound = self.unit_bound_v1(request).map_err(bad)?;
        if bytes.len() as u64 > bound.maximum_response_bytes {
            return Err(bad("TIR public wire unit exceeds native geometry before decoding"));
        }
        let heap = scan_response(bytes, self.maximum_leaf_bytes as usize)?;
        let lease = reserve_native_requests_v1(heap).map_err(Error::Resource)?;
        let mut remaining = bytes;
        let response =
            PalwTirPublicResponseV1::decode_mode_v2(&mut remaining, self.maximum_leaf_bytes as usize, true).map_err(io_fault)?;
        if !remaining.is_empty() {
            return Err(bad("TIR public response has trailing bytes"));
        }
        self.check_response_bound_v1(request, &response).map_err(bad)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(response, lease))
    }
    /// Public identity-only twin of authenticate_v1, with native ownership.
    pub fn authenticate_response_owned_v2(
        &self,
        request: &PalwTirPublicRequestV1,
        response: &PalwTirPublicResponseV1,
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
    ) -> Result<NativeRetainedV1<PalwTirPublicAuthenticatedUnitV1>, Error> {
        self.authenticate_response_native_v2(request, response, ids, None)
    }
    pub(crate) fn authenticate_response_native_v2(
        &self,
        request: &PalwTirPublicRequestV1,
        response: &PalwTirPublicResponseV1,
        ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
        native: Option<&PalwTirPublicRowsAnchorV1>,
    ) -> Result<NativeRetainedV1<PalwTirPublicAuthenticatedUnitV1>, Error> {
        self.check_request_v1(request).map_err(bad)?;
        self.check_response_bound_v1(request, response).map_err(bad)?;
        let lease = reserve_native_requests_v1(response_heap(response)?).map_err(Error::Resource)?;
        let _scratch = reserve_native_requests_v1(auth_scratch(response)?).map_err(Error::Resource)?;
        let unit = self.authenticate_with_rows_v1(request, response, ids, native).map_err(bad)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(unit, lease))
    }
}

impl PalwTirPublicSourceV1 {
    /// Complete accepted FullIds body: canonical successive Response::Ids Borsh
    /// records, not a wire trust flag. The input owner must stay live separately.
    /// One exact destination Vec, no provisional chunk Vec/growth/final clone.
    pub fn decode_complete_ids_body_native_v2(&self, body: &[u8]) -> Result<NativeRetainedV1<PalwTirPublicAuthenticatedIdsV1>, Error> {
        self.trace().map_err(bad)?;
        if self.decode_count_v1() == 0 {
            return Err(bad("complete original IDs do not reach exact nonempty EOF"));
        }
        fn next<'a>(source: &PalwTirPublicSourceV1, body: &mut &'a [u8], offset: u32) -> Result<(Hash64, &'a [u8]), Error> {
            let bound = source.unit_bound_v1(&PalwTirPublicRequestV1::Ids { offset }).map_err(bad)?;
            if body.len() < 69 || body[0] != 6 {
                return Err(bad("complete ID body has a truncated or different typed chunk"));
            }
            let root = Hash64::from_bytes(body[1..65].try_into().expect("64"));
            let count = u32::from_le_bytes(body[65..69].try_into().expect("4"));
            let expected = (source.decode_count_v1() - offset).min(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1);
            if count != expected {
                return Err(bad("ID chunk changes exact offset/canonical length"));
            }
            let span = slots::<u32>(count as usize)?.checked_add(69).ok_or_else(overflow::<u32>)?;
            if span > bound.maximum_response_bytes {
                return Err(bad("ID body chunk exceeds native geometry"));
            }
            let span = usize::try_from(span).map_err(|_| overflow::<u32>())?;
            if body.len() < span {
                return Err(bad("complete ID body has a truncated chunk"));
            }
            let ids = &body[69..span];
            *body = &body[span..];
            Ok((root, ids))
        }
        // Full scan first: malformed length/EOF never buys an allocation or
        // silently becomes capacity refusal when the actual bytes are invalid.
        let mut remaining = body;
        let mut offset = 0;
        let mut root = None;
        while offset < self.decode_count_v1() {
            let (rows, ids) = next(self, &mut remaining, offset)?;
            if root.is_some_and(|previous| previous != rows) {
                return Err(bad("ID body changes the original rows root"));
            }
            root = Some(rows);
            offset = offset.checked_add((ids.len() / 4) as u32).ok_or_else(overflow::<u32>)?;
        }
        if !remaining.is_empty() {
            return Err(bad("complete ID body has trailing chunks/bytes"));
        }
        let rows_root = root.ok_or_else(|| bad("complete ID body is empty"))?;
        let total = self.decode_count_v1() as usize;
        let lease = reserve_native_requests_v1(slots::<u32>(total)?).map_err(Error::Resource)?;
        let mut ids = Vec::new();
        ids.try_reserve_exact(total).map_err(|_| capacity::<u32>("complete ID destination allocation refused"))?;
        if ids.capacity() != total {
            return Err(capacity::<u32>("complete ID destination has unquoted capacity"));
        }
        let mut remaining = body;
        let mut offset = 0;
        while offset < self.decode_count_v1() {
            let (_, chunk) = next(self, &mut remaining, offset)?;
            for value in chunk.chunks_exact(4) {
                ids.push(u32::from_le_bytes(value.try_into().expect("4")));
            }
            offset = ids.len() as u32;
        }
        self.validate_complete_ids_classified_v2(rows_root, &ids)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(
            PalwTirPublicAuthenticatedIdsV1 { source_id: self.source_id, rows_root, ids },
            lease,
        ))
    }
}

#[cfg(test)]
#[path = "palw_tir_public_response_native_tests_v2.rs"]
mod tests;
