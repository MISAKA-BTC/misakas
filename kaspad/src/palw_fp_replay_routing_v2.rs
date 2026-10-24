//! Exact backend/original-native-context handoff into FP replay workers.
//! These owners authenticate execution identity, never computation or credit.
use super::*;
use kaspa_consensus_core::{
    palw_backend::{PalwExecutionBackendV1, PalwReplayRootsV1},
    palw_freeprompt_v3::PalwFreePromptJobV3,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2 as Owner,
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_v2::PalwJobContextV2,
};
#[derive(Debug)]
pub(super) enum FpReplayRoutingFaultV2 {
    Resource(NativeCoverageV1),
    Retry(String),
    Stale,
}
impl std::fmt::Display for FpReplayRoutingFaultV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resource(e) => write!(f, "{e}"),
            Self::Retry(e) => write!(f, "{e}"),
            Self::Stale => write!(f, "original FP replay native identity changed"),
        }
    }
}
pub(super) struct FpReplayRoutingGuardV2 {
    claim: Hash64,
    job: NativeRetainedV1<PalwJobContextV2>,
    authority: Option<Arc<Owner>>,
    _metadata: Option<NativeDemandLeaseV1>,
}
impl FpReplayRoutingGuardV2 {
    pub(super) fn current_with(
        &self,
        read: impl FnOnce(Hash64, &PalwJobContextV2) -> Result<Option<Arc<Owner>>, String>,
    ) -> Result<(), FpReplayRoutingFaultV2> {
        let fresh = read(self.claim, &self.job).map_err(FpReplayRoutingFaultV2::Retry)?;
        match (&self.authority, fresh) {
            (None, None) => Ok(()),
            (Some(old), Some(fresh)) if old.same_claim_v2(&fresh) => Ok(()),
            _ => Err(FpReplayRoutingFaultV2::Stale),
        }
    }
}
pub(super) struct BoundFpReplayV2 {
    backend: Box<dyn PalwExecutionBackendV1>,
    guard: Option<Arc<FpReplayRoutingGuardV2>>,
    // Covers context/capture-shape scratch before the backend can allocate it.
    _context: Option<NativeDemandLeaseV1>,
}
impl BoundFpReplayV2 {
    pub(super) fn legacy(backend: Box<dyn PalwExecutionBackendV1>) -> Self {
        Self { backend, guard: None, _context: None }
    }
    pub(super) fn context_scratch_v2() -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<kaspa_consensus_core::palw_backend::PalwCaptureShapeV1>(2)?;
        q.vec_v1::<u8>(
            (kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES as u64)
                .checked_mul(2)
                .ok_or_else(|| NativeCoverageV1::overflow::<Self>("FP replay context scratch"))?,
        )?;
        reserve_native_requests_v1(q.bytes_v1())
    }
    pub(super) fn bind_with(
        mut backend: Box<dyn PalwExecutionBackendV1>,
        claim: Hash64,
        job: &PalwJobContextV2,
        active: bool,
        read: impl FnOnce(Hash64, &PalwJobContextV2) -> Result<Option<Arc<Owner>>, String>,
        context: Option<NativeDemandLeaseV1>,
    ) -> Result<Self, FpReplayRoutingFaultV2> {
        let guard = if active {
            let job = NativeRetainedV1::try_from_clone_v1(job).map_err(FpReplayRoutingFaultV2::Resource)?;
            let authority = read(claim, &job).map_err(FpReplayRoutingFaultV2::Retry)?;
            if let Some(owner) = &authority {
                if owner.claim() != claim || owner.job() != &*job || !owner.is_free_prompt() {
                    return Err(FpReplayRoutingFaultV2::Stale);
                }
                backend.bind_native_routing_claim_v2(Arc::clone(owner)).map_err(|e| match e {
                    kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1::LocalResource(e) => {
                        FpReplayRoutingFaultV2::Resource(e)
                    }
                    e => FpReplayRoutingFaultV2::Retry(e.to_string()),
                })?;
            }
            let mut q = PalwMaterialConstructionRequestsV2::new();
            q.vec_v1::<FpReplayRoutingGuardV2>(1).map_err(FpReplayRoutingFaultV2::Resource)?;
            q.vec_v1::<usize>(3).map_err(FpReplayRoutingFaultV2::Resource)?;
            let metadata = reserve_native_requests_v1(q.bytes_v1()).map_err(FpReplayRoutingFaultV2::Resource)?;
            Some(Arc::new(FpReplayRoutingGuardV2 { claim, job, authority, _metadata: metadata }))
        } else {
            None
        };
        Ok(Self { backend, guard, _context: context })
    }
    pub(super) fn backend(&self) -> &dyn PalwExecutionBackendV1 {
        self.backend.as_ref()
    }
    pub(super) fn backend_and_context_mut_v2(
        &mut self,
    ) -> (&mut dyn PalwExecutionBackendV1, Option<&PalwJobContextV2>) {
        (self.backend.as_mut(), self.guard.as_ref().map(|guard| &*guard.job))
    }
    pub(super) fn backend_mut(&mut self) -> &mut dyn PalwExecutionBackendV1 {
        self.backend.as_mut()
    }
    pub(super) fn execute(self, job: &PalwFreePromptJobV3, prompt: &[usize]) -> PalwSeatTaskOutV1 {
        let result = self.backend.execute_free_prompt(job, prompt).and_then(|run| {
            if let Some(original) = &self.guard {
                let shape = self
                    .backend
                    .capture_shape(&run.outcome.material)
                    .ok_or("FP routed replay cannot authenticate executed context")?;
                if shape.job_context != *original.job {
                    return Err("FP replay executed another native context".into());
                }
            }
            Ok(PalwReplayRootsV1 {
                execution_root: run.outcome.execution_root,
                trace_root: run.facts.full_logits_trace_root,
                work_leaves: Some(run.facts.step_leaf_count),
                output_root: Some(run.outcome.output_root),
            })
        });
        match self.guard {
            Some(original) => PalwSeatTaskOutV1::RoutedReplay { result, original },
            None => PalwSeatTaskOutV1::Replay(result),
        }
    }
}

#[cfg(test)]
#[path = "palw_fp_replay_routing_tests_v2.rs"]
mod tests;
