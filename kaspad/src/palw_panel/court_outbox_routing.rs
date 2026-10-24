//! Non-wire original execution owners, keyed by the complete ordered rider
//! attempt identities rather than the shared lead/round debounce key.
use super::CourtMoveV1;
use crate::palw_rider_outbox::RiderBatchV2;
use kaspa_consensus_core::{palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 as Fault, palw_state_v2::PalwConsensusObjectV2};
#[derive(Default)]
pub(super) struct CourtRoutingV2 {
    head: Option<Box<RiderBatchV2>>,
    in_flight: bool,
}
impl CourtRoutingV2 {
    pub(super) fn insert(&mut self, mut batch: Box<RiderBatchV2>) {
        batch.next = self.head.take();
        self.head = Some(batch);
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.head.is_none()
    }
    pub(super) fn start_flush(&mut self) {
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
        while let Some(batch) = slot {
            if pending.iter().any(|(_, _, _, o)| batch.matches(o)) {
                slot = &mut slot.as_mut().unwrap().next;
            } else {
                let next = batch.next.take();
                *slot = next;
            }
        }
    }
    pub(super) fn refresh(
        &self,
        object: &PalwConsensusObjectV2,
        active: bool,
        session: &kaspa_consensusmanager::ConsensusProxy,
    ) -> Result<(), Fault> {
        self.refresh_with(
            object,
            active,
            |owner| crate::palw_native_routing_work_v2::refresh_v2(session, Some(owner)),
            |job, executor, lead, index, retention| {
                require_original_unrouted_v2(
                    session.palw_native_routing_rider_work_authority_v2(job, executor, lead, index, retention)?,
                )
            },
        )
    }
    pub(super) fn refresh_with(
        &self,
        object: &PalwConsensusObjectV2,
        active: bool,
        mut read: impl FnMut(
            &std::sync::Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>,
        ) -> Result<(), Fault>,
        mut unrouted: impl FnMut(
            &kaspa_consensus_core::palw_v2::PalwJobContextV2,
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
            kaspa_consensus_core::Hash64,
            u32,
            u64,
        ) -> Result<(), Fault>,
    ) -> Result<(), Fault> {
        if !matches!(object, PalwConsensusObjectV2::AttemptRidersV1 { .. }) {
            return Ok(());
        }
        let mut node = self.head.as_deref();
        while let Some(batch) = node {
            if batch.matches(object) {
                return batch.refresh(object, active, &mut read, &mut unrouted);
            }
            node = batch.next.as_deref();
        }
        if active { Err(Fault::Identity("rider pending batch has no original execution guard")) } else { Ok(()) }
    }
}
impl Drop for CourtRoutingV2 {
    fn drop(&mut self) {
        while let Some(mut batch) = self.head.take() {
            self.head = batch.next.take();
        }
    }
}

pub(super) fn require_original_unrouted_v2(
    current: Option<std::sync::Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>>,
) -> Result<(), Fault> {
    if current.is_some() {
        Err(Fault::Identity("pending rider now requires native routing; reexecute before signing"))
    } else {
        Ok(())
    }
}
