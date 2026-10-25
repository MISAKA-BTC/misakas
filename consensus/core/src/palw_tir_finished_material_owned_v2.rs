//! Same-state original material context and admitted finished-body reconstruction.
//! The retained Source is constructed once by the existing full native row validator.
use super::*;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1 as Fault, NativeDemandLeaseV1, NativeRetainedV1, TirNativeRequestsV1,
    reserve_native_requests_v1,
};
use crate::palw_tir_public_owned_v1::{palw_tir_public_body_hash_v1, request_body_bound_v1};
use crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1;

#[cfg(test)]
thread_local! {static STAGES:std::cell::Cell<(u64,u64,u64,u64)>=const{std::cell::Cell::new((0,0,0,0))};}
#[cfg(test)]
pub(crate) fn material_stages_v2() -> (u64, u64, u64, u64) {
    STAGES.with(std::cell::Cell::get)
}
#[cfg(test)]
pub(crate) fn reset_material_stages_v2() {
    STAGES.with(|v| v.set((0, 0, 0, 0)));
}
/// Opaque, non-wire exact-state source capability. The state cannot change
/// while borrowed; into_source consumes that binding before the owner escapes.
pub struct PalwTirPublicMaterialContextV2<'a> {
    state: &'a crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    row: &'a PalwTirPublicAcquisitionRowV1,
    source: NativeRetainedV1<PalwTirPublicSourceV1>,
}
impl std::ops::Deref for PalwTirPublicMaterialContextV2<'_> {
    type Target = PalwTirPublicSourceV1;
    fn deref(&self) -> &Self::Target {
        &self.source
    }
}
#[derive(Debug)]
pub enum PalwTirPublicFinishedOwnedV2 {
    Unit(NativeRetainedV1<PalwTirPublicAuthenticatedUnitV1>),
    Ids(NativeRetainedV1<PalwTirPublicAuthenticatedIdsV1>),
}
type Error = Fault<String>;
fn allocation<T>(role: &'static str) -> Error {
    Fault::Resource(NativeCoverageV1::unsupported::<T>(role, "allocator refused admitted material capacity"))
}
fn span_fault(error: std::io::Error) -> Error {
    if error.kind() == std::io::ErrorKind::OutOfMemory {
        Fault::Resource(NativeCoverageV1::overflow::<Vec<u8>>("material identity Borsh span"))
    } else {
        Fault::Decode(error.to_string())
    }
}
/// Borsh starts1024 and may grow while the previous buffer remains live.
/// This is the actual serialized field span of retained identity objects, not
/// a decoded-heap estimate. No payload is encoded to obtain its length.
fn identity_scratch_v2(
    row: &PalwTirPublicAcquisitionRowV1,
    reference: Option<&PalwTirPublicAcceptedBodyRefV1>,
) -> Result<Option<NativeDemandLeaseV1>, Error> {
    let owner = borsh::object_length(&(&row.descriptor, row.owned.contract_v1())).map_err(span_fault)?;
    let reference_bytes = reference.map(borsh::object_length).transpose().map_err(span_fault)?.unwrap_or(0);
    // Exact Borsh field span of a chunk identity and its bounded part bytes;
    // no serialized temporary is constructed to obtain the writer capacity.
    let chunk = if let Some(r) = reference {
        borsh::object_length(&(r.demand_id, r.answerer, Hash64::default(), 0u64))
            .map_err(span_fault)?
            .checked_add(std::mem::size_of::<u32>())
            .and_then(|n| n.checked_add(r.total_bytes.min(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) as usize))
            .ok_or_else(|| Fault::Resource(NativeCoverageV1::overflow::<Vec<u8>>("material chunk identity span")))?
    } else {
        0
    };
    let envelope = owner
        .max(reference_bytes)
        .max(chunk)
        .max(1024)
        .checked_next_power_of_two()
        .ok_or_else(|| Fault::Resource(NativeCoverageV1::overflow::<Vec<u8>>("material identity writers")))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(envelope as u64).map_err(Fault::Resource)?;
    q.exact_v1::<u8>(envelope as u64).map_err(Fault::Resource)?;
    let bytes = q.bytes_v1();
    #[cfg(test)]
    STAGES.with(|s| {
        let (a, b, _, d) = s.get();
        s.set((a, b, bytes, d));
    });
    reserve_native_requests_v1(bytes).map_err(Fault::Resource)
}
pub fn palw_tir_public_material_context_v2<'a>(
    state: &'a crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    maximum_leaves: u64,
) -> Result<PalwTirPublicMaterialContextV2<'a>, Error> {
    let Some(Row::TirAcquisition(row)) = state.staged_court_row_v1(&key) else {
        return Err(Fault::Decode("TIR material has no current acquisition".into()));
    };
    let _identity = identity_scratch_v2(row, None)?;
    // This existing constructor performs the complete current frame/legacy
    // context, registered program/layout, descriptor, contract/work validation.
    // Caller-supplied rows or merely matching Source IDs are never accepted.
    let source = palw_tir_public_source_for_row_owned_v2(state, row, maximum_leaves)?;
    Ok(PalwTirPublicMaterialContextV2 { state, key, row, source })
}
impl PalwTirPublicMaterialContextV2<'_> {
    pub fn into_source(self) -> NativeRetainedV1<PalwTirPublicSourceV1> {
        self.source
    }
    pub fn finished_material_owned_v2(
        &self,
        reference: &PalwTirPublicAcceptedBodyRefV1,
    ) -> Result<Option<PalwTirPublicFinishedOwnedV2>, Error> {
        let _identity = identity_scratch_v2(self.row, Some(reference))?;
        let id = reference.hash_v1().map_err(Fault::Decode)?;
        let Some(Row::TirAcceptedBody(row)) = self.state.staged_court_row_v1(&Key::TirAcceptedBody(id)) else { return Ok(None) };
        row.check_structure_v1(id).map_err(Fault::Decode)?;
        let owner = material_scope(self.state, row.acquisition, row.source_id, row.material_until_daa).map_err(Fault::Decode)?;
        if row.acquisition != self.key || !std::ptr::eq(owner, self.row) {
            return Err(Fault::Decode("TIR finished reference changes current acquisition context".into()));
        }
        check_reference(owner, &self.source, row, None).map_err(Fault::Decode)?;
        let r = &row.reference;
        if r.total_bytes > request_body_bound_v1(&self.source, r.request).map_err(Fault::Decode)?.0 {
            return Err(Fault::Decode("TIR accepted body exceeds its native bound".into()));
        }
        let size = usize::try_from(r.total_bytes).map_err(|_| {
            Fault::Resource(NativeCoverageV1::unsupported::<Vec<u8>>("finished material body", "accepted body exceeds local usize"))
        })?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<u8>(r.total_bytes).map_err(Fault::Resource)?;
        let bytes = q.bytes_v1();
        #[cfg(test)]
        STAGES.with(|s| {
            let (a, b, c, _) = s.get();
            s.set((a + 1, b, c, bytes));
        });
        let _body_lease = reserve_native_requests_v1(bytes).map_err(Fault::Resource)?;
        #[cfg(test)]
        STAGES.with(|s| {
            let (a, b, c, d) = s.get();
            s.set((a, b + 1, c, d));
        });
        let mut body = Vec::new();
        body.try_reserve_exact(size).map_err(|_| allocation::<Vec<u8>>("finished material body"))?;
        if body.capacity() != size {
            return Err(allocation::<Vec<u8>>("unexpected exact material body capacity"));
        }
        body.resize(size, 0);
        let (mut head, mut offset, mut parts) = (r.chunk_head, r.total_bytes, 0u64);
        while offset > 0 {
            parts += 1;
            if parts > r.total_bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64) {
                return Err(Fault::Decode("TIR accepted body has more chunks than its canonical split".into()));
            }
            let chunk = palw_tir_public_chunk_v1(self.state, head)
                .map_err(Fault::Decode)?
                .ok_or_else(|| Fault::Decode("TIR accepted body lost a rooted chunk".into()))?;
            let record = &chunk.record;
            if record.demand_id != r.demand_id
                || record.answerer != r.answerer
                || record.head_v1().map_err(Fault::Decode)? != head
                || record.offset.checked_add(record.bytes.len() as u64) != Some(offset)
            {
                return Err(Fault::Decode("TIR rooted chunk substitutes demand/signer/head/offset".into()));
            }
            body[record.offset as usize..offset as usize].copy_from_slice(&record.bytes);
            head = record.previous;
            offset = record.offset;
        }
        if head != Hash64::default() || palw_tir_public_body_hash_v1(&body) != r.body_hash {
            return Err(Fault::Decode("TIR reassembled body does not reproduce its accepted checksum/head".into()));
        }
        if matches!(r.request, crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids { offset: 0 }) {
            return self.source.decode_complete_ids_body_native_v2(&body).map(|ids| Some(PalwTirPublicFinishedOwnedV2::Ids(ids)));
        }
        let response = self.source.decode_response_native_v2(&r.request, &body)?;
        self.source
            .authenticate_response_native_v2(&r.request, &response, None, None)
            .map(|unit| Some(PalwTirPublicFinishedOwnedV2::Unit(unit)))
    }
}
