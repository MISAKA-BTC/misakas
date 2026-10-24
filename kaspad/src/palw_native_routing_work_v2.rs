//! Live pre-execution FP/rider consumers of own native routing, separate from replay authority.
use kaspa_consensus_core::{
    Hash64,
    palw_backend::PalwExecutionBackendV1,
    palw_freeprompt_v3::PalwFreePromptJobV3,
    palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as MaterialFault,
    palw_native_resource_v1::reserve_native_requests_v1,
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_reader_router_v2::{PalwNativeRoutingWorkAuthorityV2 as Owner, PalwNativeRoutingWorkFaultV2 as Fault},
    palw_state_v2::PalwBondKeyV2,
    palw_v2::PalwJobContextV2,
};
use std::sync::Arc;

fn bind(backend: &mut dyn PalwExecutionBackendV1, owner: &Option<Arc<Owner>>) -> Result<(), Fault> {
    if let Some(owner) = owner {
        backend.bind_native_routing_work_v2(Arc::clone(owner)).map_err(|why| match why {
            MaterialFault::LocalResource(e) => Fault::Resource(e),
            MaterialFault::Substitution | MaterialFault::Authentication(_) => {
                Fault::Identity("backend rejected native routing work identity")
            }
            _ => Fault::Unavailable("backend cannot bind native routing work plan"),
        })?;
    }
    Ok(())
}
pub(crate) fn bind_fp_v2(
    session: &kaspa_consensusmanager::ConsensusProxy,
    backend: &mut dyn PalwExecutionBackendV1,
    params: &kaspa_consensus_core::config::params::Params,
    daa: u64,
    fp: &PalwFreePromptJobV3,
) -> Result<Option<Arc<Owner>>, Fault> {
    if !params
        .palw_memory_routing_court_v2
        .is_some_and(|f| f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(daa))
    {
        return Ok(None);
    }
    let mut q = PalwMaterialConstructionRequestsV2::new();
    q.vec_v1::<PalwJobContextV2>(1)?;
    q.vec_v1::<u8>(kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES as u64)?;
    let _context = reserve_native_requests_v1(q.bytes_v1())?;
    let context = backend
        .fp_job_context_for_executed_v1(fp, fp.decode_token_limit)
        .ok_or(Fault::Unavailable("FP backend context unavailable before execution"))?;
    let owner = session.palw_native_routing_fp_work_authority_v2(fp, &context)?;
    bind(backend, &owner)?;
    Ok(owner)
}
pub(crate) fn bind_rider_v2(
    session: &kaspa_consensusmanager::ConsensusProxy,
    backend: &mut dyn PalwExecutionBackendV1,
    job: &PalwJobContextV2,
    executor: PalwBondKeyV2,
    lead: Hash64,
    index: u32,
    retention: u64,
) -> Result<Option<Arc<Owner>>, Fault> {
    let owner = session.palw_native_routing_rider_work_authority_v2(job, executor, lead, index, retention)?;
    bind(backend, &owner)?;
    Ok(owner)
}
pub(crate) fn refresh_v2(session: &kaspa_consensusmanager::ConsensusProxy, owner: Option<&Arc<Owner>>) -> Result<(), Fault> {
    let Some(owner) = owner else { return Ok(()) };
    // The native door checks current eligibility and canonical ancestry in the
    // same import/snapshot bracket. Keep the original execution/plan owner alive.
    let _fresh = session.palw_native_routing_refresh_work_authority_v2(owner)?;
    Ok(())
}
