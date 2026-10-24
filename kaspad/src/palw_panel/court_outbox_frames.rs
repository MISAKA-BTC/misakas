//! Array storage for admitted court copies and the carrier's unsent buffer.
//! Raw legacy enqueues, EDF/maps and carrier encodings are separate boundaries.
use super::CourtMoveV1;
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use std::mem::size_of;

pub(super) struct CourtFrameLeaseV1 {
    _lease: Option<NativeDemandLeaseV1>,
}
impl CourtFrameLeaseV1 {
    #[cfg(test)]
    pub(super) fn is_reserved(&self) -> bool {
        self._lease.is_some()
    }
    pub(super) fn reserve(capacity: usize) -> Result<Self, NativeCoverageV1> {
        let component = "court carrier array";
        // Include an old/new allocation envelope. The caller's previous ticket
        // also stays alive until growth succeeds, so refusal never detaches it.
        let bytes = capacity
            .checked_mul(size_of::<CourtMoveV1>())
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>(component))?;
        Ok(Self { _lease: reserve_native_requests_v1(bytes)? })
    }
}

/// Preallocated for at most one retry per original batch item. Data is dropped
/// before its storage ticket; no subsequent push can grow the admitted array.
pub(in crate::palw_panel) struct CourtRetryBufferV1 {
    items: Vec<CourtMoveV1>,
    frame: CourtFrameLeaseV1,
    limit: usize,
}
impl CourtRetryBufferV1 {
    pub(super) fn new(limit: usize) -> Result<Self, NativeCoverageV1> {
        let frame = CourtFrameLeaseV1::reserve(limit)?;
        let mut items = Vec::new();
        items
            .try_reserve_exact(limit)
            .map_err(|_| NativeCoverageV1::unsupported::<Self>("court carrier retry array", "local allocation refused"))?;
        Ok(Self { items, frame, limit })
    }
    pub(in crate::palw_panel) fn push(&mut self, item: CourtMoveV1) {
        assert!(self.items.len() < self.limit, "at most one retry per court batch item");
        self.items.push(item);
    }
    pub(super) fn into_parts(self) -> (Vec<CourtMoveV1>, CourtFrameLeaseV1) {
        (self.items, self.frame)
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.items.len()
    }
}
