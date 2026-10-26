//! Exact native identity and allocation ownership for SEAT-S4 segment workers.
//! Authentication of identity does not authenticate computation or grant credit.
use super::*;
use kaspa_consensus_core::{
    palw_backend::PalwExecutionBackendV1,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1,
    },
    palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2 as Owner,
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_v2::PalwJobContextV2,
};

#[derive(Debug)]
pub(super) enum SegmentRoutingFaultV2 {
    Resource(NativeCoverageV1),
    Retry(String),
    Stale,
}
impl std::fmt::Display for SegmentRoutingFaultV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(e) => write!(f, "{e}"),
            Self::Retry(e) => write!(f, "{e}"),
            Self::Stale => write!(f, "original segment replay native identity changed"),
        }
    }
}

pub(super) struct SegmentRoutingGuardV2 {
    claim: Hash64,
    segment: u16,
    job: NativeRetainedV1<PalwJobContextV2>,
    authority: Option<Arc<Owner>>,
    // Includes the one matched-sidecar node and prompt retained by this run.
    _metadata: Option<NativeDemandLeaseV1>,
}
impl SegmentRoutingGuardV2 {
    fn current_with(
        &self,
        active: bool,
        read: &mut impl FnMut(Hash64, &PalwJobContextV2) -> Result<Option<Arc<Owner>>, String>,
    ) -> Result<(), SegmentRoutingFaultV2> {
        // An original Some must still be checked even if the current fence is off.
        if !active && self.authority.is_none() {
            return Ok(());
        }
        let fresh = read(self.claim, &self.job).map_err(SegmentRoutingFaultV2::Retry)?;
        match (&self.authority, fresh) {
            (None, None) => Ok(()),
            (Some(old), Some(fresh)) if old.same_claim_v2(&fresh) => Ok(()),
            _ => Err(SegmentRoutingFaultV2::Stale),
        }
    }
}

pub(super) struct BoundSegmentReplayV2 {
    backend: Box<dyn PalwExecutionBackendV1>,
    original: Arc<SegmentRoutingGuardV2>,
    prompt: Vec<usize>,
}
impl BoundSegmentReplayV2 {
    pub(super) fn bind_with(
        mut backend: Box<dyn PalwExecutionBackendV1>,
        claim: Hash64,
        segment: u16,
        job: &PalwJobContextV2,
        prompt: &[usize],
        active: bool,
        read: impl FnOnce(Hash64, &PalwJobContextV2) -> Result<Option<Arc<Owner>>, String>,
    ) -> Result<Self, SegmentRoutingFaultV2> {
        // Quote every newly retained prompt/guard/matched-node allocation before
        // cloning. The independent job owner admits its own nested network bytes.
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<usize>(u64::try_from(prompt.len()).map_err(|_| {
            SegmentRoutingFaultV2::Resource(NativeCoverageV1::overflow::<Self>(
                "segment prompt length",
            ))
        })?)
        .map_err(SegmentRoutingFaultV2::Resource)?;
        q.vec_v1::<SegmentRoutingGuardV2>(1)
            .map_err(SegmentRoutingFaultV2::Resource)?;
        q.vec_v1::<usize>(3)
            .map_err(SegmentRoutingFaultV2::Resource)?;
        q.vec_v1::<MatchedSegmentV2>(1)
            .map_err(SegmentRoutingFaultV2::Resource)?;
        let metadata =
            reserve_native_requests_v1(q.bytes_v1()).map_err(SegmentRoutingFaultV2::Resource)?;
        let job =
            NativeRetainedV1::try_from_clone_v1(job).map_err(SegmentRoutingFaultV2::Resource)?;
        let authority = if active {
            read(claim, &job).map_err(SegmentRoutingFaultV2::Retry)?
        } else {
            None
        };
        if let Some(owner) = &authority {
            // Both ordinary attempt and free-prompt native owners are valid here;
            // the exact native job and claim, rather than the form, must agree.
            if owner.claim() != claim || owner.job() != &*job {
                return Err(SegmentRoutingFaultV2::Stale);
            }
            backend.bind_native_routing_claim_v2(Arc::clone(owner)).map_err(|e| match e {
                kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1::LocalResource(e) => {
                    SegmentRoutingFaultV2::Resource(e)
                }
                e => SegmentRoutingFaultV2::Retry(e.to_string()),
            })?;
        }
        let prompt = prompt.to_vec();
        let original = Arc::new(SegmentRoutingGuardV2 {
            claim,
            segment,
            job,
            authority,
            _metadata: metadata,
        });
        Ok(Self {
            backend,
            original,
            prompt,
        })
    }
    pub(super) fn execute(
        self,
        opening: Vec<u8>,
        claim: kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1,
    ) -> PalwSeatTaskOutV1 {
        let result = self
            .backend
            .replay_segment_from_checkpoint_v1(&self.original.job, &self.prompt, &opening, claim)
            .map(|replay| replay.matches)
            .map_err(palw_seat_s4_refusal_v1);
        PalwSeatTaskOutV1::RoutedSegment {
            result,
            original: self.original,
        }
    }
}

struct MatchedSegmentV2 {
    original: Arc<SegmentRoutingGuardV2>,
    next: Option<Box<MatchedSegmentV2>>,
}
#[derive(Default)]
pub(super) struct SegmentMatchBookV2 {
    head: Option<Box<MatchedSegmentV2>>,
}
impl Drop for SegmentMatchBookV2 {
    fn drop(&mut self) {
        // Avoid recursive destruction proportional to the live duty count.
        while let Some(mut node) = self.head.take() {
            self.head = node.next.take();
        }
    }
}
impl SegmentMatchBookV2 {
    fn get(&self, claim: Hash64, segment: u16) -> Option<&Arc<SegmentRoutingGuardV2>> {
        let mut node = self.head.as_deref();
        while let Some(n) = node {
            if n.original.claim == claim && n.original.segment == segment {
                return Some(&n.original);
            }
            node = n.next.as_deref();
        }
        None
    }
    pub(super) fn remember(&mut self, original: Arc<SegmentRoutingGuardV2>) {
        if self.get(original.claim, original.segment).is_some() {
            return;
        }
        // This unique node was admitted by the guard before execution. No new
        // growable map or unreserved sidecar array is introduced at completion.
        self.head = Some(Box::new(MatchedSegmentV2 {
            original,
            next: self.head.take(),
        }));
    }
    pub(super) fn retain_live(&mut self, live: impl Fn(&Hash64) -> bool) {
        let mut link = &mut self.head;
        while link.is_some() {
            if !live(&link.as_ref().unwrap().original.claim) {
                let mut removed = link.take().unwrap();
                *link = removed.next.take();
            } else {
                link = &mut link.as_mut().unwrap().next;
            }
        }
    }
}

impl PalwSeatReplaysV1 {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn start_routed_segment_v2(
        &mut self,
        key: PalwSeatReplayKeyV1,
        class: Hash64,
        heavy: bool,
        now_daa: u64,
        reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
        bound: BoundSegmentReplayV2,
        opening: Vec<u8>,
        claim: kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1,
    ) {
        let handle = tokio::task::spawn_blocking(move || {
            let _held_for_the_segment = reservation;
            bound.execute(opening, claim)
        });
        self.running.insert(
            key,
            PalwSeatReplayRunV1 {
                handle,
                class,
                started_daa: now_daa,
                heavy,
                overdue: false,
            },
        );
    }
    pub(super) fn segment_original_v2(
        &self,
        key: &PalwSeatReplayKeyV1,
    ) -> Option<Arc<SegmentRoutingGuardV2>> {
        match self.done.get(key).map(|done| &done.result) {
            Some(PalwSeatTaskOutV1::RoutedSegment { original, .. }) => Some(Arc::clone(original)),
            _ => None,
        }
    }
}
impl PalwSeatResumesV1 {
    pub(super) fn refresh_routing_with_v2(
        &mut self,
        claim: Hash64,
        seats: u16,
        mask: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2,
        replays: &mut PalwSeatReplaysV1,
        active: bool,
        require_complete: bool,
        mut read: impl FnMut(Hash64, &PalwJobContextV2) -> Result<Option<Arc<Owner>>, String>,
    ) -> Result<(), SegmentRoutingFaultV2> {
        let k = kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2(seats);
        let checked = (|| {
            for segment in (0..k).filter(|segment| mask.covers(*segment)) {
                if !self.matched.contains(&(claim, segment)) {
                    if require_complete {
                        return Err(SegmentRoutingFaultV2::Stale);
                    }
                    continue;
                }
                match self.routing.get(claim, segment) {
                    Some(original) => original.current_with(active, &mut read)?,
                    None if active => return Err(SegmentRoutingFaultV2::Stale),
                    None => {}
                }
            }
            Ok(())
        })();
        if matches!(checked, Err(SegmentRoutingFaultV2::Stale)) {
            // Retire only local execution state. Live blocking work remains
            // counted and can be drained; its old guarded result is checked again.
            self.matched.retain(|(id, _)| *id != claim);
            self.routing.retain_live(|id| *id != claim);
            self.in_flight.retain(|(id, _), _| *id != claim);
            self.started.retain(|(id, _), _| *id != claim);
            replays.done.retain(|key, _| key.0 != claim);
        }
        checked
    }
}

#[cfg(test)]
#[path = "palw_segment_replay_routing_tests_v2.rs"]
mod tests;
