//! Non-wire original duty snapshots for exact locally signed source packets.
//! A stale packet is only a local retirement, never a verdict against a claim.
use super::{CourtMoveKeyV1, CourtMoveV1, PalwConsensusObjectV2};
use crate::palw_memory_ledger::PalwMemoryReservationV1;
use kaspa_consensus_core::{
    palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1 as Action,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_staged_court_state_v1::{
        PalwStagedFlatProgressV1 as Flat, PalwStagedFloatProgressV1 as Float, PalwStagedTirPrefixProgressV1 as Tir,
    },
};

pub(in crate::palw_panel) enum SourceResponseSnapshotV1 {
    Flat(Flat),
    Float(Float),
    Tir(Tir),
}
#[derive(Debug)]
pub(in crate::palw_panel) enum SourceResponseFaultV1 {
    Retry(String),
    Stale,
}
impl std::fmt::Display for SourceResponseFaultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retry(why) => write!(f, "source duty read deferred: {why}"),
            Self::Stale => write!(f, "original source packet is no longer current"),
        }
    }
}
pub(in crate::palw_panel) fn is_source_response_v1(object: &PalwConsensusObjectV2) -> bool {
    matches!(object,PalwConsensusObjectV2::PublicCourtStreamMoveV1 {movement}
        if matches!(movement.action,Action::FlatSourceAnswer(_)|Action::FloatSourceAnswer(_)|Action::NativeTirPrefixAnswer(_)))
}
impl SourceResponseSnapshotV1 {
    fn read(&self, session: &kaspa_consensusmanager::ConsensusProxy) -> Result<Option<Self>, String> {
        match self {
            Self::Flat(old) => session.palw_staged_flat_source_v1((old.session.claim, old.session.owner)).map(|p| p.map(Self::Flat)),
            Self::Float(old) => {
                session.palw_staged_float_source_v1((old.session.claim, old.session.owner)).map(|p| p.map(Self::Float))
            }
            Self::Tir(old) => session.palw_staged_tir_prefix_v1((old.row.claim, old.row.reserver)).map(|p| p.map(Self::Tir)),
        }
    }
    fn current(&self, fresh: &Self, object: &PalwConsensusObjectV2) -> bool {
        let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = object else { return false };
        match (self, fresh, &movement.action) {
            (Self::Flat(old), Self::Flat(new), Action::FlatSourceAnswer(a)) => {
                crate::palw_panel::response_presign::flat_current_v1(old, new, movement, a.answerer)
            }
            (Self::Float(old), Self::Float(new), Action::FloatSourceAnswer(a)) => {
                crate::palw_panel::response_presign::float_current_v1(old, new, movement, a.answerer)
            }
            (Self::Tir(old), Self::Tir(new), Action::NativeTirPrefixAnswer(a)) => {
                crate::palw_panel::response_presign::tir_current_v1(old, new, movement, a.answerer)
            }
            _ => false,
        }
    }
}
pub(super) struct Entry {
    // Drop actual snapshots and copied packet before their host/native tickets.
    key: CourtMoveKeyV1,
    due: u64,
    finished: bool,
    snapshot: SourceResponseSnapshotV1,
    object: Option<NativeRetainedV1<PalwConsensusObjectV2>>,
    _snapshot_memory: PalwMemoryReservationV1,
    _packet_memory: Option<PalwMemoryReservationV1>,
    _metadata: Option<NativeDemandLeaseV1>,
    next: Option<Box<Entry>>,
}
#[derive(Default)]
pub(super) struct CourtSourceResponsesV1 {
    head: Option<Box<Entry>>,
    in_flight: bool,
}
impl CourtSourceResponsesV1 {
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.head.is_none()
    }
    pub(super) fn prepare(
        key: CourtMoveKeyV1,
        due: u64,
        snapshot: SourceResponseSnapshotV1,
        memory: PalwMemoryReservationV1,
        packet: Option<PalwMemoryReservationV1>,
        object: &PalwConsensusObjectV2,
    ) -> Result<Box<Entry>, NativeCoverageV1> {
        let mut entry = Entry {
            key,
            due,
            finished: false,
            snapshot,
            object: None,
            _snapshot_memory: memory,
            _packet_memory: packet,
            _metadata: None,
            next: None,
        };
        entry.object = Some(NativeRetainedV1::try_from_clone_v1(object)?);
        entry._metadata = reserve_native_requests_v1(std::mem::size_of::<Entry>() as u64)?;
        Ok(Box::new(entry))
    }
    pub(super) fn insert(&mut self, mut entry: Box<Entry>) {
        entry.next = self.head.take();
        self.head = Some(entry);
    }
    pub(super) fn recovery_packets(&self) -> impl Iterator<Item = (CourtMoveKeyV1, &PalwConsensusObjectV2, u64)> {
        std::iter::successors(self.head.as_deref(), |entry| entry.next.as_deref())
            .filter(|entry| self.in_flight && !entry.finished)
            .map(|entry| (entry.key, &**entry.object.as_ref().expect("admitted source packet"), entry.due))
    }
    pub(super) fn finish_packet(&mut self, object: &PalwConsensusObjectV2) {
        let mut node = self.head.as_deref_mut();
        while let Some(entry) = node {
            if entry.object.as_ref().is_some_and(|old| &**old == object) {
                entry.finished = true;
            }
            node = entry.next.as_deref_mut();
        }
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
        while let Some(entry) = slot {
            if !entry.finished && pending.iter().any(|(_, _, _, object)| entry.object.as_ref().is_some_and(|old| &**old == object)) {
                slot = &mut slot.as_mut().unwrap().next;
            } else {
                *slot = entry.next.take();
            }
        }
    }
    /// The same synchronous guard-and-operation boundary gates real carrier
    /// preparation/submission and controls. No await can intervene inside it.
    pub(super) fn current_then<T>(
        &self,
        object: &PalwConsensusObjectV2,
        session: &kaspa_consensusmanager::ConsensusProxy,
        operation: impl FnOnce() -> T,
    ) -> Result<T, SourceResponseFaultV1> {
        self.current_then_with(object, |old| old.read(session), operation)
    }
    pub(in crate::palw_panel) fn current_then_with<T>(
        &self,
        object: &PalwConsensusObjectV2,
        read: impl FnOnce(&SourceResponseSnapshotV1) -> Result<Option<SourceResponseSnapshotV1>, String>,
        operation: impl FnOnce() -> T,
    ) -> Result<T, SourceResponseFaultV1> {
        if !is_source_response_v1(object) {
            return Ok(operation());
        }
        let mut node = self.head.as_deref();
        while let Some(entry) = node {
            if !entry.finished && entry.object.as_ref().is_some_and(|old| &**old == object) {
                let fresh = read(&entry.snapshot).map_err(SourceResponseFaultV1::Retry)?.ok_or(SourceResponseFaultV1::Stale)?;
                if !entry.snapshot.current(&fresh, object) {
                    return Err(SourceResponseFaultV1::Stale);
                }
                return Ok(operation());
            }
            node = entry.next.as_deref();
        }
        // Restart/foreign enqueue cannot recreate an original accepted context.
        Err(SourceResponseFaultV1::Stale)
    }
}
impl Drop for CourtSourceResponsesV1 {
    fn drop(&mut self) {
        while let Some(mut node) = self.head.take() {
            self.head = node.next.take();
        }
    }
}
