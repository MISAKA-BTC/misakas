//! Node-local ownership of court pending moves, their due dates and submission
//! debounce. Historical enqueue-attempt stamps are retained at their existing sites;
//! neither those stamps nor successful mempool submission are chain acceptance. Native
//! reads and existing filer/recovery books remain the authority for acceptance.
use super::{Hash64, PalwConsensusObjectV2};
#[cfg(test)]
use super::palw_court_queue_edf_v1;
use std::collections::HashMap;
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1, NativeDemandLeaseV1};
#[path = "court_outbox_tickets.rs"]
mod tickets;
use tickets::CourtPayloadTicketsV1;
#[path = "court_outbox_frames.rs"]
mod frames;
use frames::{CourtFrameLeaseV1, CourtRetryBufferV1};
#[path = "court_outbox_order.rs"]
mod order;
use order::CourtOrderPlanV1;
#[path = "court_outbox_routing.rs"]
mod routing;
use routing::CourtRoutingV2;
#[path = "court_outbox_source_response.rs"]
mod source_response;
use source_response::CourtSourceResponsesV1;
pub(super) use source_response::{SourceResponseSnapshotV1,SourceResponseFaultV1,is_source_response_v1};

/// This invalidates only the local pending execution. It is neither a claim
/// verdict nor a monetary penalty. Read/import/capacity faults remain retryable.
pub(super) fn rider_refusal_should_retry_v2(why:&kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2)->bool {
    !matches!(why,kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2::Identity(_))
}

pub(super) type CourtMoveKeyV1 = (Hash64, u32, bool);
pub(super) type CourtMoveV1 = (Hash64, u32, bool, PalwConsensusObjectV2);

#[derive(Default)]
pub(crate) struct CourtOutboxV1 {
    pending: Vec<CourtMoveV1>,
    due: HashMap<CourtMoveKeyV1, u64>,
    submitted: HashMap<CourtMoveKeyV1, u64>,
    // Pending payloads drop before the tickets that authorize their storage.
    payload_tickets: CourtPayloadTicketsV1,
    rider_guards: CourtRoutingV2,
    source_guards: CourtSourceResponsesV1,
    rider_due_frame: Option<NativeDemandLeaseV1>,
    pending_frame: Option<CourtFrameLeaseV1>,
    flight_frame: Option<CourtFrameLeaseV1>,
}
impl CourtOutboxV1 {
    pub(super) fn pending(&self) -> &[CourtMoveV1] {
        &self.pending
    }
    /// Existing callers decide replacement/duplicate policy; no new dedup rule.
    pub(super) fn push_pending(&mut self, item: CourtMoveV1) {
        self.pending.push(item);
    }
    /// Destination admission occurs before detaching the source batch. A refusal
    /// returns that same box to the in-process outbox without losing its leases.
    pub(super) fn try_push_rider_batch(&mut self,mut batch:Box<crate::palw_rider_outbox::RiderBatchV2>,daa:u64)->Result<(),Box<crate::palw_rider_outbox::RiderBatchV2>> {
        let admitted=(||->Result<(),NativeCoverageV1> {
            let capacity=self.pending.len().checked_add(1).map(|n|n.max(self.pending.capacity()))
                .ok_or_else(||NativeCoverageV1::overflow::<Self>("rider court pending array"))?;
            let frame=CourtFrameLeaseV1::reserve(capacity)?;
            // The due map retains capacity after a batch is sent, so its ticket
            // belongs to the outbox rather than the shorter-lived batch.
            let due_frame=crate::palw_rider_outbox::RiderBatchV2::reserve_court_due(self.due.len(),self.due.capacity())?;
            self.rider_due_frame=due_frame;
            // Retain admission even if a later allocation fails after the Vec
            // has grown; its capacity remains owned by this outbox.
            self.pending_frame=Some(frame);
            self.pending.try_reserve_exact(1).map_err(|_|NativeCoverageV1::unsupported::<Self>("rider court pending array","local allocation refused"))?;
            self.due.try_reserve(1).map_err(|_|NativeCoverageV1::unsupported::<Self>("rider court due map","local allocation refused"))?;
            Ok(())
        })();
        if let Err(why)=admitted {kaspa_core::warn!("rider court admission deferred: {why}");return Err(batch);}
        let lead=batch.lead;
        let object=batch.take_object();
        self.rider_guards.insert(batch);
        self.due.insert((lead,crate::palw_rider_outbox::PALW_RIDER_QUEUE_ROUND_V1,false),daa);
        self.pending.push((lead,crate::palw_rider_outbox::PALW_RIDER_QUEUE_ROUND_V1,false,object));
        Ok(())
    }
    pub(super) fn refresh_rider(&self,object:&PalwConsensusObjectV2,params:&kaspa_consensus_core::config::params::Params,session:&kaspa_consensusmanager::ConsensusProxy)->Result<(),kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> {
        let active=params.palw_memory_routing_court_v2.is_some_and(|f|f!=kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(session.get_virtual_daa_score()));
        self.rider_guards.refresh(object,active,session)
    }
    /// Copy admission precedes deep Clone. The outbox retains the payload ticket
    /// through its carrier batch and any retry; a refusal leaves the queue unchanged.
    pub(super) fn try_push_clone(&mut self, key: CourtMoveKeyV1, source: &PalwConsensusObjectV2) -> Result<(), NativeCoverageV1> {
        let slot = self.payload_tickets.prepare()?;
        let capacity = self.pending.len().checked_add(1).map(|n| n.max(self.pending.capacity()))
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("court pending array"))?;
        let frame = CourtFrameLeaseV1::reserve(capacity)?;
        let copy = NativeRetainedV1::try_from_clone_v1(source)?;
        self.pending.try_reserve_exact(1).map_err(|_| NativeCoverageV1::unsupported::<Self>(
            "court outbox copied payload slot", "queue allocation refused"))?;
        let (object, ticket) = copy.into_parts_v1();
        self.payload_tickets.insert(key, ticket, slot);
        self.pending.push((key.0, key.1, key.2, object));
        self.pending_frame = Some(frame);
        Ok(())
    }
    /// Reserve the retained due-map capacity before copying the payload. A
    /// failed payload admission changes no pending move or logical due entry.
    pub(super) fn try_push_clone_with_due(&mut self, key: CourtMoveKeyV1, source: &PalwConsensusObjectV2, due: u64) -> Result<(), NativeCoverageV1> {
        let frame=crate::palw_rider_outbox::RiderBatchV2::reserve_court_due(self.due.len(),self.due.capacity())?;
        self.rider_due_frame=frame;
        self.due.try_reserve(1).map_err(|why|NativeCoverageV1::unsupported::<Self>("court due map admission",&why.to_string()))?;
        self.try_push_clone(key,source)?;
        self.due.insert(key,due);
        Ok(())
    }
    /// Filer payload copies also retain host demand when native quotas are off.
    /// Both quota families precede Clone; no queue/due publication on refusal.
    pub(super) fn try_push_filer_clone_with_due_v2(
        &mut self,
        key: CourtMoveKeyV1,
        source: &PalwConsensusObjectV2,
        due: u64,
    ) -> Result<(), NativeCoverageV1> {
        use kaspa_consensus_core::palw_native_resource_v1::NativeHeapV1;
        let payload = source.native_heap_v1("TIR filer court payload");
        let bytes = payload
            .bytes()
            .ok_or(payload)?
            .checked_add(CourtPayloadTicketsV1::host_slot_bytes()?)
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("TIR filer host payload"))?;
        let host = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tir-filer-court-payload",
                    class_id: Default::default(),
                    job: key.0,
                },
                bytes,
            )
            .map_err(|_| NativeCoverageV1::unsupported::<Self>("TIR filer court payload", "host ledger admission refused"))?;
        let slot = self.payload_tickets.prepare()?;
        let frame = crate::palw_rider_outbox::RiderBatchV2::reserve_court_due(self.due.len(), self.due.capacity())?;
        let capacity = self
            .pending
            .len()
            .checked_add(1)
            .map(|n| n.max(self.pending.capacity()))
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("TIR filer court frame"))?;
        let pending = CourtFrameLeaseV1::reserve(capacity)?;
        let copy = NativeRetainedV1::try_from_clone_v1(source)?;
        // Actual capacity may grow even if a later allocation refuses; keep
        // the complete metadata tickets before making those allocation calls.
        self.pending_frame = Some(pending);
        self.rider_due_frame = frame;
        self.pending
            .try_reserve_exact(1)
            .map_err(|_| NativeCoverageV1::unsupported::<Self>("TIR filer court frame", "allocation refused"))?;
        self.due.try_reserve(1).map_err(|_| NativeCoverageV1::unsupported::<Self>("TIR filer court due", "allocation refused"))?;
        let (object, ticket) = copy.into_parts_v1();
        self.payload_tickets.insert_with_host(key, ticket, slot, Some(host));
        self.pending.push((key.0, key.1, key.2, object));
        self.due.insert(key, due);
        Ok(())
    }
    pub(super) fn try_push_source_response(&mut self,key:CourtMoveKeyV1,source:&PalwConsensusObjectV2,due:u64,snapshot:SourceResponseSnapshotV1,memory:crate::palw_memory_ledger::PalwMemoryReservationV1,packet:Option<crate::palw_memory_ledger::PalwMemoryReservationV1>)->Result<(),NativeCoverageV1> {
        let guard=CourtSourceResponsesV1::prepare(key,due,snapshot,memory,packet,source)?;
        self.try_push_clone_with_due(key,source,due)?;
        self.source_guards.insert(guard);
        Ok(())
    }
    pub(super) fn finish_source_packet(&mut self,object:&PalwConsensusObjectV2) {self.source_guards.finish_packet(object);}
    /// Recover only still-outstanding exact packets after a canceled carrier
    /// future. Prepare every copy/ticket before changing logical queue entries.
    fn recover_source_flight(&mut self)->Result<(),NativeCoverageV1> {
        let count=self.source_guards.recovery_packets().filter(|(_,o,_)|!self.pending.iter().any(|(_,_,_,p)|p==*o)).count();
        if count==0 {return Ok(());}
        type Recovery=(CourtMoveKeyV1,u64,NativeRetainedV1<PalwConsensusObjectV2>,Option<NativeDemandLeaseV1>);
        let requests=count.checked_mul(std::mem::size_of::<Recovery>()).ok_or_else(||NativeCoverageV1::overflow::<Recovery>("source canceled recovery array"))?;
        let _workspace=kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(requests as u64)?;
        let mut recovery=Vec::<Recovery>::new();
        recovery.try_reserve_exact(count).map_err(|why|NativeCoverageV1::unsupported::<Recovery>("source canceled recovery array",&why.to_string()))?;
        for (key,object,due) in self.source_guards.recovery_packets().filter(|(_,o,_)|!self.pending.iter().any(|(_,_,_,p)|p==*o)) {
            let slot=self.payload_tickets.prepare()?;
            let copy=NativeRetainedV1::try_from_clone_v1(object)?;
            recovery.push((key,due,copy,slot));
        }
        let target=self.pending.len().checked_add(count).ok_or_else(||NativeCoverageV1::overflow::<Self>("source canceled pending array"))?;
        self.pending_frame=Some(CourtFrameLeaseV1::reserve(target.max(self.pending.capacity()))?);
        self.pending.try_reserve_exact(count).map_err(|why|NativeCoverageV1::unsupported::<Self>("source canceled pending array",&why.to_string()))?;
        let due_target=self.due.len().checked_add(count).and_then(|n|n.checked_sub(1)).ok_or_else(||NativeCoverageV1::overflow::<Self>("source canceled due map"))?;
        self.rider_due_frame=crate::palw_rider_outbox::RiderBatchV2::reserve_court_due(due_target,self.due.capacity())?;
        self.due.try_reserve(count).map_err(|why|NativeCoverageV1::unsupported::<Self>("source canceled due map",&why.to_string()))?;
        for (key,due,copy,slot) in recovery {
            let (object,ticket)=copy.into_parts_v1();
            self.payload_tickets.insert(key,ticket,slot);
            self.pending.push((key.0,key.1,key.2,object));
            self.due.insert(key,due);
        }
        Ok(())
    }
    pub(super) fn current_source_then<T>(&self,object:&PalwConsensusObjectV2,session:&kaspa_consensusmanager::ConsensusProxy,operation:impl FnOnce()->T)->Result<T,SourceResponseFaultV1> {
        self.source_guards.current_then(object,session,operation)
    }
    #[cfg(test)]
    pub(super) fn current_source_then_for_tests<T>(&self,object:&PalwConsensusObjectV2,read:impl FnOnce(&SourceResponseSnapshotV1)->Result<Option<SourceResponseSnapshotV1>,String>,operation:impl FnOnce()->T)->Result<T,SourceResponseFaultV1> {
        self.source_guards.current_then_with(object,read,operation)
    }
    pub(super) fn retain_pending(&mut self, keep: impl FnMut(&CourtMoveV1) -> bool) {
        self.pending.retain(keep);
        self.payload_tickets.prune(&self.pending);
        self.rider_guards.prune(&self.pending);
        self.source_guards.prune(&self.pending);
    }
    pub(super) fn set_due(&mut self, key: CourtMoveKeyV1, due: u64) -> Option<u64> {
        self.due.insert(key, due)
    }
    pub(super) fn forget_due(&mut self, key: &CourtMoveKeyV1) -> Option<u64> {
        self.due.remove(key)
    }
    pub(super) fn date_if_absent(&mut self, key: CourtMoveKeyV1, due: u64) {
        self.due.entry(key).or_insert(due);
    }
    pub(super) fn tighten_due(&mut self, key: CourtMoveKeyV1, due: u64) {
        self.due.entry(key).and_modify(|at| *at = (*at).min(due)).or_insert(due);
    }
    pub(super) fn tighten_existing_due(&mut self, key: CourtMoveKeyV1, due: u64) {
        if let Some(at) = self.due.get_mut(&key) {
            *at = (*at).min(due);
        }
    }
    pub(super) fn extend_due(&mut self, due: impl IntoIterator<Item = (CourtMoveKeyV1, u64)>) {
        self.due.extend(due);
    }
    pub(super) fn retain_due(&mut self, keep: impl FnMut(&CourtMoveKeyV1, &mut u64) -> bool) {
        self.due.retain(keep);
    }
    pub(super) fn submitted_at(&self, key: &CourtMoveKeyV1) -> Option<&u64> {
        self.submitted.get(key)
    }
    pub(super) fn has_submitted(&self, key: &CourtMoveKeyV1) -> bool {
        self.submitted.contains_key(key)
    }
    /// Historical move/debounce timestamp: never a Done/accepted transition.
    /// The carrier stamps successful submission; legacy eager sites still stamp queue attempts.
    pub(super) fn mark_submitted(&mut self, key: CourtMoveKeyV1, at: u64) -> Option<u64> {
        self.submitted.insert(key, at)
    }
    pub(super) fn forget_submitted(&mut self, key: &CourtMoveKeyV1) -> Option<u64> {
        self.submitted.remove(key)
    }
    pub(super) fn retain_submitted(&mut self, keep: impl FnMut(&CourtMoveKeyV1, &mut u64) -> bool) {
        self.submitted.retain(keep);
    }
    /// Prepare before draining or doing any funding/submission work. On local
    /// refusal the queue is untouched and the service can retry on a later tick.
    pub(super) fn try_retry_buffer(&mut self) -> Result<CourtRetryBufferV1, NativeCoverageV1> {
        // The service calls this at the start of a new flush, after any previous
        // carrier future (and its raw batch) has been dropped. Reclaim canceled
        // batch tickets before asking for new scratch, avoiding quota deadlock.
        self.flight_frame = None;
        self.payload_tickets.finish_flush(&self.pending);
        self.rider_guards.finish_flush(&self.pending);
        self.recover_source_flight()?;
        self.source_guards.finish_flush(&self.pending);
        CourtRetryBufferV1::new(self.pending.len())
    }
    pub(super) fn try_take_priority(&mut self) -> Result<Vec<CourtMoveV1>, NativeCoverageV1> {
        let plan = CourtOrderPlanV1::new(&self.pending, &self.due)?;
        plan.apply(&mut self.pending);
        self.flight_frame = self.pending_frame.take();
        self.payload_tickets.start_flush();
        self.rider_guards.start_flush();
        self.source_guards.start_flush();
        Ok(std::mem::take(&mut self.pending))
    }
    /// Legacy EDF remains only as a fixture/parity reference.
    #[cfg(test)]
    pub(super) fn take_priority(&mut self) -> Vec<CourtMoveV1> {
        self.flight_frame = self.pending_frame.take();
        self.payload_tickets.start_flush();
        self.rider_guards.start_flush();
        self.source_guards.start_flush();
        palw_court_queue_edf_v1(&mut self.pending, &self.due);
        std::mem::take(&mut self.pending)
    }
    /// Restore exactly the unsent items and prune due dates exactly as the old
    /// carrier loop did. Submission debounce deliberately survives this step.
    pub(super) fn finish_flush(&mut self, unsent: CourtRetryBufferV1) {
        let (items, frame) = unsent.into_parts();
        self.finish_parts(items, Some(frame));
    }
    fn finish_parts(&mut self, unsent: Vec<CourtMoveV1>, frame: Option<CourtFrameLeaseV1>) {
        self.pending = unsent;
        self.pending_frame = frame;
        // The service consumed/dropped the original batch before reaching here.
        self.flight_frame = None;
        self.payload_tickets.finish_flush(&self.pending);
        self.rider_guards.finish_flush(&self.pending);
        self.source_guards.finish_flush(&self.pending);
        self.due.retain(|key, _| self.pending.iter().any(|(s, r, b, _)| (*s, *r, *b) == *key));
    }
    #[cfg(test)]
    fn finish_fixture_flush(&mut self, unsent: Vec<CourtMoveV1>) {
        self.finish_parts(unsent, None);
    }
    /// Test-only adapter for pre-refactor fixtures. Production takes the typed owner.
    #[cfg(test)]
    pub(crate) fn with_fixture_parts<R>(
        pending: &mut Vec<CourtMoveV1>,
        due: &mut HashMap<CourtMoveKeyV1, u64>,
        submitted: &mut HashMap<CourtMoveKeyV1, u64>,
        run: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let mut owner = Self::from_parts(std::mem::take(pending), std::mem::take(due), std::mem::take(submitted));
        let result = run(&mut owner);
        (*pending, *due, *submitted) = owner.into_parts();
        result
    }
    #[cfg(test)]
    pub(super) fn from_parts(
        pending: Vec<CourtMoveV1>,
        due: HashMap<CourtMoveKeyV1, u64>,
        submitted: HashMap<CourtMoveKeyV1, u64>,
    ) -> Self {
        Self { pending, due, submitted, payload_tickets: CourtPayloadTicketsV1::default(), rider_guards: CourtRoutingV2::default(), source_guards: CourtSourceResponsesV1::default(), rider_due_frame:None, pending_frame: None, flight_frame: None }
    }
    #[cfg(test)]
    pub(super) fn into_parts(self) -> (Vec<CourtMoveV1>, HashMap<CourtMoveKeyV1, u64>, HashMap<CourtMoveKeyV1, u64>) {
        assert!(self.payload_tickets.is_empty(), "fixture extraction must not detach payload tickets");
        assert!(self.rider_guards.is_empty(), "fixture extraction must not detach native rider guards");
        assert!(self.source_guards.is_empty(), "fixture extraction must not detach original source response guards");
        assert!(self.pending_frame.as_ref().is_none_or(|f| !f.is_reserved()) && self.flight_frame.as_ref().is_none_or(|f| !f.is_reserved()), "fixture extraction must not detach frame tickets");
        (self.pending, self.due, self.submitted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(session: u64, round: u32) -> CourtMoveV1 {
        let key = Hash64::from_u64_word(session);
        (key, round, false, PalwConsensusObjectV2::ReceiptLicensed { claim: key, receipts: Vec::new() })
    }
    fn key(item: &CourtMoveV1) -> CourtMoveKeyV1 {
        (item.0, item.1, item.2)
    }

    /// Several flushes with a single free carrier: preserve stable EDF, duplicates,
    /// the remaining due dates and the submission debounce, even when no acceptance arrives.
    #[test]
    fn constrained_flush_and_lost_submission_match_the_legacy_queue() {
        let pending = vec![item(1, 0), item(2, 7), item(3, 1), item(2, 5), item(3, 1), item(4, 0)];
        let due = [(key(&pending[1]), 90), (key(&pending[2]), 50), (key(&pending[3]), 120)].into_iter().collect();
        let mut owner = CourtOutboxV1::from_parts(pending.clone(), due, HashMap::new());
        let (mut old_pending, mut old_due, mut old_moved) = (pending, owner.due.clone(), HashMap::new());
        for daa in [100, 101, 100 + super::super::COURT_MOVE_REPLAN_DAA] {
            palw_court_queue_edf_v1(&mut old_pending, &old_due);
            let mut batch = std::mem::take(&mut old_pending);
            let mut actual = owner.take_priority();
            assert_eq!(actual, batch);
            let sent = batch.remove(0);
            assert_eq!(actual.remove(0), sent);
            // This is the old carrier's success path: one slot, all other items unsent.
            old_moved.insert(key(&sent), daa);
            owner.mark_submitted(key(&sent), daa);
            old_pending = batch;
            old_due.retain(|k, _| old_pending.iter().any(|item| key(item) == *k));
            owner.finish_fixture_flush(actual);
            assert_eq!(
                (owner.pending.clone(), owner.due.clone(), owner.submitted.clone()),
                (old_pending.clone(), old_due.clone(), old_moved.clone())
            );
        }
        // A mempool yes is not a settled claim: old recovery may drop its stamp and rebuild.
        let lost = key(&item(3, 1));
        assert!(owner.has_submitted(&lost));
        owner.forget_submitted(&lost);
        old_moved.remove(&lost);
        owner.push_pending(item(3, 1));
        old_pending.push(item(3, 1));
        owner.date_if_absent(lost, 140);
        old_due.entry(lost).or_insert(140);
        palw_court_queue_edf_v1(&mut old_pending, &old_due);
        assert_eq!(owner.take_priority(), old_pending);
        assert_eq!(owner.submitted, old_moved);
    }

    #[test]
    fn admitted_edf_moves_existing_payload_buffers_without_cloning() {
        use kaspa_consensus_core::{palw_state_v2::PalwBondKeyV2, tx::{TransactionId, TransactionOutpoint}};
        let mut pending = Vec::new();
        let mut pointers = Vec::new();
        for age in 0..3u8 {
            let bytes = vec![age; 64];
            let signature = vec![age; 96];
            pointers.push((bytes.as_ptr(), signature.as_ptr()));
            // These opaque payload bytes are only ordering fixtures. They are
            // not signed native actions or evidence of receipt acceptance.
            let mut entry = item(u64::from(age) + 1, 0);
            entry.3 = PalwConsensusObjectV2::KernelRouteV1 {
                bytes,
                signer: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(1), 0)),
                signature,
            };
            pending.push(entry);
        }
        let due = [(key(&pending[1]), 10), (key(&pending[2]), 5)].into_iter().collect();
        let mut owner = CourtOutboxV1::from_parts(pending, due, HashMap::new());
        let batch = owner.try_take_priority().unwrap();
        let mut actual = Vec::new();
        for (_, _, _, object) in batch {
            let PalwConsensusObjectV2::KernelRouteV1 { bytes, signature, .. } = object else { unreachable!() };
            let age = bytes[0] as usize;
            assert_eq!((bytes.as_ptr(), signature.as_ptr()), pointers[age]);
            actual.push(age);
        }
        assert_eq!(actual, vec![2, 1, 0]);
    }

    #[test]
    fn admitted_edf_matches_legacy_for_duplicate_and_orphan_deadlines() {
        for seed in 0..80u64 {
            let mut pending = Vec::new();
            let mut due = HashMap::new();
            let mut state = seed + 1;
            for age in 0..97u64 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let mut entry = item((state >> 19) % 11, ((state >> 39) % 7) as u32);
                entry.2 = state & 1 != 0;
                // Distinguish equal queue keys so stable order is observable.
                entry.3 = PalwConsensusObjectV2::ReceiptLicensed { claim: Hash64::from_u64_word(age), receipts: Vec::new() };
                if state % 3 == 0 {
                    due.insert(key(&entry), if state % 5 == 0 { u64::MAX } else { state % 13 });
                }
                pending.push(entry);
            }
            // These due records are not queued: they cannot date an otherwise undated claim.
            due.insert((Hash64::from_u64_word(3), 999, true), 0);
            due.insert((Hash64::from_u64_word(99), 0, false), 0);
            let mut expected = pending.clone();
            palw_court_queue_edf_v1(&mut expected, &due);
            let mut owner = CourtOutboxV1::from_parts(pending, due, HashMap::new());
            assert_eq!(owner.try_take_priority().unwrap(), expected, "EDF parity seed {seed}");
        }
    }

    #[test]
    fn payload_ticket_child() {
        if std::env::var("MISAKA_COURT_PAYLOAD_TICKETS").ok().as_deref() != Some("1") {
            return;
        }
        use kaspa_consensus_core::palw_native_resource_v1::{
            NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
        };
        let memory = 1 << 20;
        let source = item(77, 3);
        let source_key = key(&source);
        let mut owner = CourtOutboxV1::default();
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        let blocker = reserve_native_requests_v1(memory).unwrap();
        assert!(owner.try_push_clone(source_key, &source.3).is_err());
        assert!(owner.pending.is_empty());
        assert!(owner.payload_tickets.is_empty());
        drop(blocker);
        owner.try_push_clone(source_key, &source.3).unwrap();
        owner.try_push_clone(source_key, &source.3).unwrap();
        assert!(reserve_native_requests_v1(memory).is_err());
        owner.set_due(source_key, 1);
        owner.forget_due(&source_key);
        owner.mark_submitted(source_key, 2);
        owner.forget_submitted(&source_key);
        assert!(reserve_native_requests_v1(memory).is_err());
        let mut batch = owner.take_priority();
        owner.retain_pending(|_| false);
        assert!(reserve_native_requests_v1(memory).is_err(), "batch still owns copied data");
        drop(batch.pop());
        owner.finish_fixture_flush(batch);
        assert!(reserve_native_requests_v1(memory).is_err(), "duplicate key still pending");
        let batch = owner.take_priority();
        drop(batch);
        owner.finish_fixture_flush(Vec::new());
        assert!(owner.payload_tickets.is_empty());
        assert!(reserve_native_requests_v1(memory).is_ok(), "last payload and its tickets released");
        owner.try_push_clone(source_key, &source.3).unwrap();
        drop(owner.take_priority()); // Carrier future canceled before finish_flush.
        assert!(reserve_native_requests_v1(memory).is_err());
        let next = owner.take_priority();
        assert!(next.is_empty());
        owner.finish_fixture_flush(next);
        assert!(reserve_native_requests_v1(memory).is_ok(), "next flush cleans up canceled batch tickets");
        // A pre-existing raw queue is an input boundary here. Its new retry
        // array must be admitted before draining even when no copied payload
        // tickets exist.
        owner.push_pending(item(88, 0));
        let blocker = reserve_native_requests_v1(memory).unwrap();
        assert!(owner.try_retry_buffer().is_err());
        assert_eq!(owner.pending().len(), 1, "refusal does not drain the queue");
        assert!(owner.try_take_priority().is_err());
        assert_eq!(owner.pending().len(), 1, "EDF refusal does not sort or drain the queue");
        drop(blocker);
        let mut unsent = owner.try_retry_buffer().unwrap();
        for item in owner.try_take_priority().unwrap() {
            unsent.push(item);
        }
        assert_eq!(unsent.len(), 1);
        owner.finish_flush(unsent);
        assert!(reserve_native_requests_v1(memory).is_err());
        owner.retain_pending(|_| false);
        assert!(reserve_native_requests_v1(memory).is_err(), "empty Vec still retains allocated capacity");
        let empty = owner.try_retry_buffer().unwrap();
        drop(owner.take_priority());
        owner.finish_flush(empty);
        assert!(reserve_native_requests_v1(memory).is_ok(), "zero-capacity retry replaces the empty retained array");
        owner.try_push_clone(source_key, &source.3).unwrap();
        drop(owner.try_take_priority().unwrap());
        assert!(reserve_native_requests_v1(memory).is_err(), "canceled batch has conservative tickets");
        let empty = owner.try_retry_buffer().unwrap();
        assert!(reserve_native_requests_v1(memory).is_ok(), "reclaim cancellation before new scratch admission");
        drop(owner.try_take_priority().unwrap());
        owner.finish_flush(empty);
        owner.try_push_clone(source_key, &source.3).unwrap();
        owner.retain_pending(|_| false);
        assert!(owner.payload_tickets.is_empty());
        assert!(reserve_native_requests_v1(memory).is_err(), "strict enqueue frame outlives removed payloads");
        let empty = owner.try_retry_buffer().unwrap();
        drop(owner.take_priority());
        owner.finish_flush(empty);
        assert!(reserve_native_requests_v1(memory).is_ok());
    }

    #[test]
    fn disabled_capacity_fixture_can_extract_unreserved_copies() {
        // Runs in the ordinary test process with capacity limits disabled.
        let source = item(7, 2);
        let mut owner = CourtOutboxV1::default();
        owner.try_push_clone(key(&source), &source.3).unwrap();
        let mut unsent = owner.try_retry_buffer().unwrap();
        for item in owner.try_take_priority().unwrap() {
            unsent.push(item);
        }
        owner.finish_flush(unsent);
        let (pending, _, _) = owner.into_parts();
        assert_eq!(pending, vec![source]);
    }

    #[test]
    fn copied_payload_tickets_survive_flush_retry_and_duplicates() {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "palw_panel::court_outbox::tests::payload_ticket_child", "--nocapture"])
            .env("MISAKA_COURT_PAYLOAD_TICKETS", "1")
            .output().unwrap();
        assert!(out.status.success(), "{} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed"));
    }

    #[test]
    fn missing_deadline_is_only_inserted_by_the_existing_inserting_operations() {
        let mut owner = CourtOutboxV1::default();
        let undated = item(8, 0);
        let dated = item(9, 0);
        owner.push_pending(undated.clone());
        owner.push_pending(dated.clone());
        owner.tighten_existing_due(key(&undated), 1);
        owner.set_due(key(&dated), 30);
        owner.date_if_absent(key(&dated), 2);
        owner.tighten_due(key(&dated), 20);
        assert_eq!(owner.take_priority(), vec![dated.clone(), undated]);
        assert_eq!(owner.due.get(&key(&dated)), Some(&20));
        assert_eq!(owner.due.len(), 1);
    }
}

#[cfg(test)]
mod rider_handoff_tests {
    use super::*;
    use crate::palw_rider_outbox::{RiderBatchV2,rider_fixture_v2};
    #[test]
    fn rider_guard_capacity_lifetime_isolated() {
        let status=std::process::Command::new(std::env::current_exe().unwrap())
            .arg("rider_guard_capacity_child").arg("--nocapture")
            .env("MISAKA_RIDER_GUARD_CAPACITY","1").status().unwrap();
        assert!(status.success());
    }
    #[test]
    fn rider_guard_capacity_child() {
        if std::env::var("MISAKA_RIDER_GUARD_CAPACITY").ok().as_deref()!=Some("1") {return;}
        use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1,arm_native_logical_limits_v1,reserve_native_requests_v1};
        let memory=1<<20;
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 {memory_bytes:memory,database_bytes:0}));
        let lead=Hash64::from_u64_word(701);
        let mut batch=RiderBatchV2::new(lead,1).unwrap();batch.push(rider_fixture_v2(lead,0),None,None).unwrap();
        // Fill exactly the remaining demand budget without assuming fixture sizes.
        let(mut lo,mut hi)=(0,memory);
        while lo<hi {let mid=lo+(hi-lo+1)/2;if reserve_native_requests_v1(mid).is_ok(){lo=mid;}else{hi=mid-1;}}
        let blocker=reserve_native_requests_v1(lo).unwrap();
        let mut outbox=CourtOutboxV1::default();
        batch=match outbox.try_push_rider_batch(batch,1) {Err(batch)=>batch,Ok(())=>panic!("capacity must refuse before source detach")};
        assert!(outbox.pending().is_empty());assert_eq!(batch.len(),1);
        drop(blocker);
        assert!(outbox.try_push_rider_batch(batch,1).is_ok());
        assert!(!outbox.rider_guards.is_empty());
        assert!(reserve_native_requests_v1(memory).is_err());
        let mut retry=outbox.try_retry_buffer().unwrap();
        let flight=outbox.try_take_priority().unwrap();
        outbox.retain_pending(|_|false);
        assert!(!outbox.rider_guards.is_empty(),"async flight keeps original owner/ticket");
        for item in flight {retry.push(item);}
        outbox.finish_flush(retry);
        assert_eq!(outbox.pending().len(),1);assert!(!outbox.rider_guards.is_empty(),"refused carrier retains batch owner");
        let retry=outbox.try_retry_buffer().unwrap();drop(outbox.try_take_priority().unwrap());
        assert!(!outbox.rider_guards.is_empty(),"cancellation conservatively retains owner");
        outbox.finish_flush(retry);
        assert!(outbox.rider_guards.is_empty());
        assert!(reserve_native_requests_v1(memory).is_err(),"due map capacity remains owned after batch retirement");
        drop(outbox);
        assert!(reserve_native_requests_v1(memory).is_ok(),"last payload/guard/map/frame ticket released");
    }
}

#[cfg(test)]
mod rider_native_owner_tests {
    use super::*;
    use crate::palw_rider_outbox::{RiderBatchV2,palw_rider_outbox_push_v2,palw_rider_outbox_import_v2};
    use kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 as Fault;
    #[test]
    fn rider_guard_original_native_arc_survives_source_flight_and_retry() {
        let (original,envelope,job,lead)=crate::palw_rider_outbox::native_owner_tests::native_rider_fixture();
        let weak=std::sync::Arc::downgrade(&original);
        let mut batch=RiderBatchV2::new(lead,1).unwrap();batch.push(envelope,Some(original),Some(&job)).unwrap();
        palw_rider_outbox_push_v2(batch);
        assert!(weak.upgrade().is_some(),"actual original native owner retained in source queue");
        let mut outbox=CourtOutboxV1::default();
        palw_rider_outbox_import_v2(|batch|outbox.try_push_rider_batch(batch,11));
        assert_eq!(outbox.pending().len(),1);
        let mut calls=0;
        let mut check=|object:&PalwConsensusObjectV2,outbox:&CourtOutboxV1| {
            outbox.rider_guards.refresh_with(object,true,|owner| {
                calls+=1;assert!(std::sync::Arc::ptr_eq(owner,&weak.upgrade().expect("same original owner")));Ok(())
            },|_,_,_,_,_|panic!("routed original must not become None")).unwrap();
        };
        check(&outbox.pending()[0].3,&outbox);
        let mut retry=outbox.try_retry_buffer().unwrap();let flight=outbox.try_take_priority().unwrap();
        check(&flight[0].3,&outbox);
        let refusal=outbox.rider_guards.refresh_with(&flight[0].3,true,|_|Err(Fault::Resource(NativeCoverageV1::unsupported::<u8>("test native refresh","capacity refused"))),|_,_,_,_,_|panic!("no fallback"));
        assert!(matches!(refusal,Err(Fault::Resource(_))));
        for item in flight {retry.push(item);}outbox.finish_flush(retry);
        check(&outbox.pending()[0].3,&outbox);drop(check);assert_eq!(calls,3);
        let mut changed=outbox.pending()[0].3.clone();let PalwConsensusObjectV2::AttemptRidersV1 {riders,..}=&mut changed else {unreachable!()};
        riders[0].attempt.output_root=Hash64::from_u64_word(900);
        assert!(matches!(outbox.rider_guards.refresh_with(&changed,true,|_|panic!("foreign batch must not use original owner"),|_,_,_,_,_|panic!("no fallback")),Err(Fault::Identity(_))));
        let retry=outbox.try_retry_buffer().unwrap();drop(outbox.try_take_priority().unwrap());
        assert!(weak.upgrade().is_some(),"original survives canceled carrier future");
        outbox.finish_flush(retry);assert!(weak.upgrade().is_none(),"owner drops only after final payload retirement");
    }
}

#[cfg(test)]
mod rider_disposition_tests {
    use super::*;
    use crate::palw_rider_outbox::RiderBatchV2;
    use kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 as Fault;
    #[test]
    fn rider_guard_native_refusal_retries_resources_but_retires_invalid_execution() {
        let (original,envelope,job,lead)=crate::palw_rider_outbox::native_owner_tests::native_rider_fixture();
        let weak=std::sync::Arc::downgrade(&original);
        let mut batch=RiderBatchV2::new(lead,1).unwrap();batch.push(envelope,Some(original),Some(&job)).unwrap();
        let mut outbox=CourtOutboxV1::default();assert!(outbox.try_push_rider_batch(batch,11).is_ok());
        let key=(lead,crate::palw_rider_outbox::PALW_RIDER_QUEUE_ROUND_V1,false);
        for why in [Fault::Resource(NativeCoverageV1::unsupported::<u8>("native refresh","capacity")),Fault::Unavailable("native import/reorg read unavailable")] {
            let mut retry=outbox.try_retry_buffer().unwrap();
            for item in outbox.try_take_priority().unwrap() {
                // The same disposition helper is used at all three actual
                // pre-funding/pre-sign/pre-submit guard doors.
                assert!(rider_refusal_should_retry_v2(&why));
                if rider_refusal_should_retry_v2(&why) {retry.push(item);}
            }
            outbox.finish_flush(retry);
            assert_eq!(outbox.pending().len(),1);assert!(weak.upgrade().is_some());assert!(!outbox.has_submitted(&key));
        }
        let retry=outbox.try_retry_buffer().unwrap();
        for item in outbox.try_take_priority().unwrap() {
            let why=Fault::Identity("current native rider window expired");
            assert!(!rider_refusal_should_retry_v2(&why));
            assert!(weak.upgrade().is_some(),"in-flight original remains alive until the payload drops");drop(item);
        }
        outbox.finish_flush(retry);
        assert!(outbox.pending().is_empty());assert!(weak.upgrade().is_none());assert!(!outbox.has_submitted(&key),"local retirement is never success/approval");
    }
    #[test]
    fn rider_guard_original_none_cannot_invent_routing_owner_or_wait_forever() {
        let (owner,envelope,job,lead)=crate::palw_rider_outbox::native_owner_tests::native_rider_fixture();
        let mut batch=RiderBatchV2::new(lead,1).unwrap();batch.push(envelope,None,Some(&job)).unwrap();
        let object=batch.take_object();let mut guards=CourtRoutingV2::default();guards.insert(batch);
        let mut observed=false;
        let why=guards.refresh_with(&object,true,|_|panic!("original None cannot become an original Arc"),|retained,executor,l,i,retention| {
            observed=true;assert_eq!(retained,&job);assert_eq!(executor,owner.executor());assert_eq!((l,i,retention),(lead,0,100));
            routing::require_original_unrouted_v2(Some(std::sync::Arc::clone(&owner)))
        }).unwrap_err();
        assert!(observed);assert!(matches!(why,Fault::Identity(_)));assert!(!rider_refusal_should_retry_v2(&why));
        let why=CourtRoutingV2::default().refresh_with(&object,true,|_|panic!("no original"),|_,_,_,_,_|panic!("no marker")).unwrap_err();
        assert!(matches!(why,Fault::Identity(_)));assert!(!rider_refusal_should_retry_v2(&why));
        assert!(routing::require_original_unrouted_v2(None).is_ok(),"explicit unchanged nonrouted result remains supported");
    }
}
