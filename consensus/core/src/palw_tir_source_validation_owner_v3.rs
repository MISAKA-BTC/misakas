//! Transient source-validation writers. No program/model/credit authority.
use super::{PalwTirPublicAcquisitionRowV1, PalwTirPublicPreparationRowV1};
use crate::{
    Hash64,
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_tir_public_owned_v1::PalwTirPublicWorkStateV1,
    palw_tir_public_source_v1::{PalwTirPublicDemandScopeV1, PalwTirPublicRequestV1},
};
use borsh::BorshSerialize;
use std::io;
type Error = NativeDecodeErrorV1<String>;
fn span_error(error: io::Error) -> Error {
    if error.kind() == io::ErrorKind::OutOfMemory {
        Error::Resource(NativeCoverageV1::overflow::<Vec<u8>>("source validation Borsh span"))
    } else {
        Error::Decode(error.to_string())
    }
}
struct HashWriter(blake2b_simd::State);
impl io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Same keyed hash of the exact Borsh bytes as the old temporary Vec helper.
/// Borsh writes directly into fixed hash state; errors preserve their text.
pub(crate) fn hash_v3<T: BorshSerialize>(domain: &[u8], value: &T) -> Result<Hash64, String> {
    let mut writer = HashWriter(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    value.serialize(&mut writer).map_err(|error| error.to_string())?;
    Ok(Hash64::from_bytes(writer.0.finalize().as_bytes().try_into().expect("64")))
}
fn requests(
    work: Option<&PalwTirPublicWorkStateV1>,
    demand: Option<(Hash64, PalwTirPublicDemandScopeV1, PalwTirPublicRequestV1)>,
) -> Result<u64, Error> {
    let mut span = 0usize;
    if let Some(work) = work {
        // Preparation.check_v1 hashes both its aggregate receipt and each Ready
        // actor's last receipt. Owned.check_v1 hashes the aggregate receipt.
        for receipt in work.last.iter().chain(work.actors.values().filter_map(|actor| actor.last.as_ref())) {
            span = span.max(borsh::object_length(receipt).map_err(span_error)?);
        }
    }
    if let Some(demand) = demand {
        span = span.max(borsh::object_length(&demand).map_err(span_error)?);
    }
    if span == 0 {
        return Ok(0);
    }
    // Borsh starts1024. All receipt/demand hashes are sequential; reserve one
    // old/new growth envelope, rather than summing every actor's writer.
    let envelope = span
        .max(1024)
        .checked_next_power_of_two()
        .ok_or_else(|| Error::Resource(NativeCoverageV1::overflow::<Vec<u8>>("source validation hash writer")))?;
    (envelope as u64)
        .checked_mul(2)
        .ok_or_else(|| Error::Resource(NativeCoverageV1::overflow::<Vec<u8>>("source validation hash writer")))
}
pub(crate) fn preparation_requests_v3(row: &PalwTirPublicPreparationRowV1) -> Result<u64, Error> {
    requests(Some(row.preparation.work_v1()), None)
}
pub(crate) fn acquisition_requests_v3(row: &PalwTirPublicAcquisitionRowV1) -> Result<u64, Error> {
    let c = row.owned.contract_v1();
    let demand = row.owned.active_v1().map(|p| {
        (
            c.source_id,
            PalwTirPublicDemandScopeV1 {
                claim: c.claim,
                owner: c.owner,
                reserved_daa: c.reserved_daa,
                bundle_id: c.bundle_id,
                turn: p.turn,
                opened_daa: p.opened_daa,
                deadline_daa: p.respond_until_daa,
            },
            p.request,
        )
    });
    requests(row.owned.work_v1(), demand)
}
pub(super) fn preparation_reserve_v3(row: &PalwTirPublicPreparationRowV1) -> Result<Option<NativeDemandLeaseV1>, Error> {
    reserve_native_requests_v1(preparation_requests_v3(row)?).map_err(Error::Resource)
}
pub(super) fn acquisition_reserve_v3(row: &PalwTirPublicAcquisitionRowV1) -> Result<Option<NativeDemandLeaseV1>, Error> {
    reserve_native_requests_v1(acquisition_requests_v3(row)?).map_err(Error::Resource)
}
#[cfg(test)]
thread_local! {static LAST:std::cell::Cell<Option<fn()>>=const{std::cell::Cell::new(None)};}
#[cfg(test)]
pub(crate) fn observe_last_v3(callback: Option<fn()>) {
    LAST.with(|value| value.set(callback));
}
#[cfg(test)]
pub(super) fn last_borrow_v3() {
    LAST.with(|value| {
        if let Some(callback) = value.get() {
            callback();
        }
    });
}
