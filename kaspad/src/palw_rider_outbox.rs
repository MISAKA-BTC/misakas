//! In-process rider handoff. Opaque execution owners and their allocation ticket
//! follow the exact unsigned-attempt batch; nothing here is serialized/restored.
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_v2::{PalwAttemptEnvelopeV2, attempt_id_v2},
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_reader_router_v2::{PalwNativeRoutingWorkAuthorityV2 as Owner, PalwNativeRoutingWorkFaultV2 as Fault},
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_state_v2::PalwConsensusObjectV2,
};
use std::sync::{Arc, Mutex};
pub const PALW_RIDER_QUEUE_ROUND_V1: u32 = u32::MAX - 9;
struct RiderGuard {
    id: Hash64,
    owner: Option<Arc<Owner>>,
    unrouted_job: Option<NativeRetainedV1<kaspa_consensus_core::palw_v2::PalwJobContextV2>>,
}
pub(crate) struct RiderBatchV2 {
    pub(crate) lead: Hash64,
    riders: Vec<PalwAttemptEnvelopeV2>,
    guards: Vec<RiderGuard>,
    pub(crate) next: Option<Box<Self>>,
    limit: usize,
    _lease: Option<NativeDemandLeaseV1>,
}
impl RiderBatchV2 {
    /// Reserve before the producer allocates signatures/pubkeys or either array.
    pub(crate) fn new(lead: Hash64, limit: usize) -> Result<Box<Self>, NativeCoverageV1> {
        if limit > kaspa_consensus_core::palw_capacity_s567_v1::PALW_RIDERS_MAX_V1 {
            return Err(NativeCoverageV1::unsupported::<Self>("rider batch", "too many riders"));
        }
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<Self>(1)?;
        q.vec_v1::<usize>(3)?;
        q.vec_v1::<PalwAttemptEnvelopeV2>(limit as u64)?;
        q.vec_v1::<RiderGuard>(limit as u64)?;
        let bytes = kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
        q.vec_v1::<u8>(
            (limit as u64).checked_mul(bytes as u64).ok_or_else(|| NativeCoverageV1::overflow::<Self>("rider signatures"))?,
        )?;
        let lease = reserve_native_requests_v1(q.bytes_v1())?;
        let mut riders = Vec::new();
        let mut guards = Vec::new();
        riders
            .try_reserve_exact(limit)
            .map_err(|_| NativeCoverageV1::unsupported::<Self>("rider batch", "rider array allocation refused"))?;
        guards
            .try_reserve_exact(limit)
            .map_err(|_| NativeCoverageV1::unsupported::<Self>("rider batch", "guard array allocation refused"))?;
        Ok(Box::new(Self { lead, riders, guards, next: None, limit, _lease: lease }))
    }
    pub(crate) fn reserve_court_due(len: usize, capacity: usize) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
        // HashMap::capacity reports usable entries, not physical buckets.
        // Clearing a large map leaves those buckets allocated even at len=0.
        let retained_buckets = if capacity == 0 {
            0
        } else {
            capacity
                .checked_add(1)
                .and_then(|n| n.checked_next_power_of_two())
                .ok_or_else(|| NativeCoverageV1::overflow::<Self>("retained rider due buckets"))?
        };
        let need = len
            .checked_add(1)
            .and_then(|n| n.checked_next_power_of_two())
            .and_then(|n| n.checked_mul(4))
            .map(|n| n.max(retained_buckets))
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("rider due map"))?;
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<((Hash64, u32, bool), u64)>(need as u64)?;
        let controls = need.checked_add(16).ok_or_else(|| NativeCoverageV1::overflow::<Self>("rider due controls"))?;
        q.vec_v1::<u8>(controls as u64)?;
        reserve_native_requests_v1(q.bytes_v1())
    }
    pub(crate) fn len(&self) -> usize {
        self.riders.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.riders.is_empty()
    }
    pub(crate) fn clear(&mut self) {
        self.riders.clear();
        self.guards.clear();
    }
    pub(crate) fn push(
        &mut self,
        envelope: PalwAttemptEnvelopeV2,
        owner: Option<Arc<Owner>>,
        job: Option<&kaspa_consensus_core::palw_v2::PalwJobContextV2>,
    ) -> Result<(), Fault> {
        if self.riders.len() >= self.limit {
            return Err(Fault::Identity("rider batch limit exceeded"));
        }
        if envelope.attempt.executor_pubkey.len() > kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || envelope.signature.len() > kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        {
            return Err(Fault::Identity("rider key/signature exceeds reserved shape"));
        }
        if let Some(owner) = &owner {
            let index = self.riders.len() as u32;
            if owner.rider() != Some((self.lead, index, envelope.attempt.trace_retention_daa))
                || owner.executor().0 != envelope.attempt.executor_bond
                || owner.row().declaration.class_id != envelope.attempt.class_id
                || envelope.attempt.challenge
                    != kaspa_consensus_core::palw_capacity_s567_v1::palw_rider_challenge_v1(&self.lead, index)
            {
                return Err(Fault::Identity("rider envelope differs from executed native work"));
            }
        }
        let unrouted_job = if owner.is_none() { job.map(NativeRetainedV1::try_from_clone_v1).transpose()? } else { None };
        self.guards.push(RiderGuard { id: attempt_id_v2(&envelope.attempt), owner, unrouted_job });
        self.riders.push(envelope);
        Ok(())
    }
    pub(crate) fn take_object(&mut self) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::AttemptRidersV1 { lead: self.lead, riders: std::mem::take(&mut self.riders) }
    }
    pub(crate) fn matches(&self, object: &PalwConsensusObjectV2) -> bool {
        let PalwConsensusObjectV2::AttemptRidersV1 { lead, riders } = object else { return false };
        *lead == self.lead
            && riders.len() == self.guards.len()
            && riders.iter().zip(&self.guards).all(|(r, g)| attempt_id_v2(&r.attempt) == g.id)
    }
    pub(crate) fn refresh(
        &self,
        object: &PalwConsensusObjectV2,
        active: bool,
        mut read: impl FnMut(&Arc<Owner>) -> Result<(), Fault>,
        mut read_unrouted: impl FnMut(
            &kaspa_consensus_core::palw_v2::PalwJobContextV2,
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
            Hash64,
            u32,
            u64,
        ) -> Result<(), Fault>,
    ) -> Result<(), Fault> {
        if !self.matches(object) {
            return Err(Fault::Identity("pending rider batch changed"));
        }
        let PalwConsensusObjectV2::AttemptRidersV1 { riders, .. } = object else { unreachable!("checked exact batch") };
        for (index, (guard, envelope)) in self.guards.iter().zip(riders).enumerate() {
            match (&guard.owner, &guard.unrouted_job) {
                (Some(owner), _) => read(owner)?,
                (None, Some(job)) => read_unrouted(
                    job,
                    kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(envelope.attempt.executor_bond),
                    self.lead,
                    index as u32,
                    envelope.attempt.trace_retention_daa,
                )?,
                (None, None) if active => {
                    return Err(Fault::Identity("pending rider lacks its original native execution decision"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
static OUTBOX: Mutex<Option<Box<RiderBatchV2>>> = Mutex::new(None);
pub(crate) fn palw_rider_outbox_push_v2(mut batch: Box<RiderBatchV2>) {
    let mut outbox = OUTBOX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    {
        let mut tail = &mut *outbox;
        while let Some(node) = tail {
            tail = &mut node.next;
        }
        batch.next = None;
        *tail = Some(batch);
    }
}
/// Refused destination admission restores the same boxed source; no allocation,
/// owner loss or authority reconstruction occurs during the handoff.
pub(crate) fn palw_rider_outbox_import_v2(mut import: impl FnMut(Box<RiderBatchV2>) -> Result<(), Box<RiderBatchV2>>) {
    let mut outbox = OUTBOX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    import_queue_v2(&mut outbox, &mut import);
}
fn import_queue_v2(
    outbox: &mut Option<Box<RiderBatchV2>>,
    import: &mut impl FnMut(Box<RiderBatchV2>) -> Result<(), Box<RiderBatchV2>>,
) {
    while let Some(mut batch) = outbox.take() {
        let rest = batch.next.take();
        match import(batch) {
            Ok(()) => *outbox = rest,
            Err(mut batch) => {
                batch.next = rest;
                *outbox = Some(batch);
                break;
            }
        }
    }
}
pub fn palw_rider_queued_v1(round: u32, responder: bool, object: &PalwConsensusObjectV2) -> bool {
    matches!(object, PalwConsensusObjectV2::AttemptRidersV1 { .. }) && (round, responder) == (PALW_RIDER_QUEUE_ROUND_V1, false)
}

#[cfg(test)]
pub(crate) fn rider_fixture_v2(lead: Hash64, index: u32) -> PalwAttemptEnvelopeV2 {
    use kaspa_consensus_core::{palw_attempt_v2::PalwAttemptUnsignedV2, tx::TransactionOutpoint};
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: 2,
            network_domain: Hash64::from_u64_word(3),
            challenge: kaspa_consensus_core::palw_capacity_s567_v1::palw_rider_challenge_v1(&lead, index),
            class_id: Hash64::from_u64_word(4),
            executor_bond: TransactionOutpoint::new(Hash64::from_u64_word(5), 0),
            executor_pubkey: vec![7],
            operator_id: Hash64::from_u64_word(6),
            artifact_root: Hash64::from_u64_word(7),
            trace_root: Hash64::from_u64_word(8),
            output_root: Hash64::from_u64_word(9),
            pwu: 1,
            trace_manifest_root: Hash64::from_u64_word(10),
            trace_chunk_count: 1,
            trace_retention_daa: 100,
            execution_root: Hash64::from_u64_word(11),
        },
        signature: vec![12],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rider_guard_binds_complete_ordered_attempts_and_never_infers_missing_owner() {
        let lead = Hash64::from_u64_word(91);
        let mut batch = RiderBatchV2::new(lead, 2).unwrap();
        batch.push(rider_fixture_v2(lead, 0), None, None).unwrap();
        batch.push(rider_fixture_v2(lead, 1), None, None).unwrap();
        let object = batch.take_object();
        assert!(batch.matches(&object));
        assert!(
            batch
                .refresh(&object, false, |_| panic!("no synthesized owner"), |_, _, _, _, _| panic!("no synthesized decision"))
                .is_ok()
        );
        assert!(matches!(
            batch.refresh(&object, true, |_| panic!("no synthesized owner"), |_, _, _, _, _| panic!("no synthesized decision")),
            Err(Fault::Identity(_))
        ));
        let mut changed = object.clone();
        let PalwConsensusObjectV2::AttemptRidersV1 { riders, .. } = &mut changed else { unreachable!() };
        riders.swap(0, 1);
        assert!(!batch.matches(&changed));
        let mut changed = object.clone();
        let PalwConsensusObjectV2::AttemptRidersV1 { riders, .. } = &mut changed else { unreachable!() };
        riders[0].attempt.output_root = Hash64::from_u64_word(999);
        assert!(!batch.matches(&changed));
        // Signatures are intentionally witnesses, not consensus attempt identity.
        let mut resigned = object.clone();
        let PalwConsensusObjectV2::AttemptRidersV1 { riders, .. } = &mut resigned else { unreachable!() };
        riders[0].signature.push(1);
        assert!(batch.matches(&resigned));
    }
    #[test]
    fn rider_guard_due_retained_capacity_isolated() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("rider_guard_due_retained_capacity_child")
            .arg("--nocapture")
            .env("MISAKA_RIDER_DUE_CAPACITY", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[test]
    fn rider_guard_due_retained_capacity_child() {
        if std::env::var("MISAKA_RIDER_DUE_CAPACITY").ok().as_deref() != Some("1") {
            return;
        }
        let mut map = std::collections::HashMap::<(Hash64, u32, bool), u64>::new();
        for i in 0..100 {
            map.insert((Hash64::from_u64_word(i), 0, false), i);
        }
        let retained = map.capacity();
        map.clear();
        assert_eq!(map.capacity(), retained);
        assert_eq!(map.len(), 0);
        assert!(retained >= 100);
        // A quota that fits the old usable-entry estimate must refuse the
        // physical retained bucket envelope. No guessed bucket count is used
        // to construct the actual map being handed to the production planner.
        let mut old = PalwMaterialConstructionRequestsV2::new();
        old.vec_v1::<((Hash64, u32, bool), u64)>(retained as u64).unwrap();
        old.vec_v1::<u8>((retained + 16) as u64).unwrap();
        kaspa_consensus_core::palw_native_resource_v1::arm_native_logical_limits_v1(Some(
            kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 {
                memory_bytes: old.bytes_v1(),
                database_bytes: 0,
            },
        ));
        assert!(RiderBatchV2::reserve_court_due(0, 0).is_ok());
        assert!(
            RiderBatchV2::reserve_court_due(map.len(), map.capacity()).is_err(),
            "retained bucket memory cannot be quoted as usable entries"
        );
    }
    #[test]
    fn rider_source_import_refusal_preserves_same_box_and_fifo() {
        let first = RiderBatchV2::new(Hash64::from_u64_word(92), 0).unwrap();
        let pointer = &*first as *const _;
        let mut queue = Some(first);
        queue.as_mut().unwrap().next = Some(RiderBatchV2::new(Hash64::from_u64_word(93), 0).unwrap());
        import_queue_v2(&mut queue, &mut |batch| Err(batch));
        assert_eq!(&**queue.as_ref().unwrap() as *const _, pointer);
        let mut ids = Vec::new();
        import_queue_v2(&mut queue, &mut |batch| {
            ids.push(batch.lead);
            Ok(())
        });
        assert_eq!(ids, vec![Hash64::from_u64_word(92), Hash64::from_u64_word(93)]);
        assert!(queue.is_none());
    }
}

#[cfg(test)]
#[path = "palw_rider_outbox_native_tests.rs"]
pub(crate) mod native_owner_tests;
