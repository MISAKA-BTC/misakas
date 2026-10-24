//! Tickets for independently copied queue payloads, including the batch held by
//! the carrier during an async flush. Legacy raw enqueues and EDF/map/codec
//! scratch are separate, still-open admission boundaries.
use super::{CourtMoveKeyV1, CourtMoveV1};
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1};
use std::mem::size_of;

type Ticket = Option<NativeDemandLeaseV1>;
struct Entry {
    key: CourtMoveKeyV1,
    _payload: Ticket,
    _slot: Ticket,
    next: Option<Box<Entry>>,
}
#[derive(Default)]
pub(super) struct CourtPayloadTicketsV1 {
    head: Option<Box<Entry>>,
    in_flight: bool,
}
impl CourtPayloadTicketsV1 {
    /// Bound the ticket bookkeeping node before its allocation. The queue's
    /// legacy frame Vec, EDF scratch and maps remain separate open boundaries.
    pub(super) fn prepare(&self) -> Result<Ticket, NativeCoverageV1> {
        let component = "court outbox copied payload ticket";
        let bytes = size_of::<Entry>()
            .checked_mul(2)
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| NativeCoverageV1::overflow::<Entry>(component))?;
        reserve_native_requests_v1(bytes)
    }
    pub(super) fn insert(&mut self, key: CourtMoveKeyV1, payload: Ticket, slot: Ticket) {
        self.head = Some(Box::new(Entry { key, _payload: payload, _slot: slot, next: self.head.take() }));
    }
    pub(super) fn start_flush(&mut self) {
        // A canceled carrier future may have dropped its raw batch without
        // finishing. Keep those tickets conservatively until the next flush;
        // cancellation must not make the following tick panic.
        self.in_flight = true;
    }
    pub(super) fn finish_flush(&mut self, pending: &[CourtMoveV1]) {
        self.in_flight = false;
        self.prune(pending);
    }
    pub(super) fn prune(&mut self, pending: &[CourtMoveV1]) {
        if self.in_flight {
            return;
        }
        let mut slot = &mut self.head;
        while let Some(entry) = slot {
            if pending.iter().any(|(s, r, b, _)| (*s, *r, *b) == entry.key) {
                slot = &mut slot.as_mut().unwrap().next;
            } else {
                let next = entry.next.take();
                *slot = next;
            }
        }
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.head.is_none()
    }
}
impl Drop for CourtPayloadTicketsV1 {
    fn drop(&mut self) {
        // Avoid a recursive drop chain even if a future policy raises queue limits.
        while let Some(mut entry) = self.head.take() {
            self.head = entry.next.take();
        }
    }
}
