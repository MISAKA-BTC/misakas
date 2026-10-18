//! Own accepted-claim context for actual replay workers. Native state is the
//! authority; served captures and matching arithmetic never supply it.
use super::*;
use kaspa_consensus_core::{
    palw_backend::PalwExecutionBackendV1, palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2, palw_v2::PalwJobContextV2,
};

impl PalwPanelService {
    fn bind_native_routing_replay_v2(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        backend: &mut dyn PalwExecutionBackendV1,
        claim: Hash64,
        job: &PalwJobContextV2,
    ) -> Result<Option<Arc<PalwNativeRoutingClaimAuthorityV2>>, String> {
        if !self.consensus_config.params.palw_memory_routing_court_v2.is_some_and(|f| {
            f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(session.get_virtual_daa_score())
        }) {
            return Ok(None);
        }
        let authority = session.palw_native_routing_claim_authority_v2(claim, job)?;
        if let Some(owner) = &authority {
            backend.bind_native_routing_claim_v2(Arc::clone(owner)).map_err(|e| e.to_string())?;
        }
        Ok(authority)
    }
    fn refresh_native_routing_replay_v2(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        authority: Option<&Arc<PalwNativeRoutingClaimAuthorityV2>>,
    ) -> Result<(), String> {
        if let Some(owner) = authority {
            let fresh = session
                .palw_native_routing_claim_authority_v2(owner.claim(), owner.job())?
                .ok_or("routing replay native stamp disappeared")?;
            if !owner.same_claim_v2(&fresh) {
                return Err("routing replay accepted claim/import/declaration changed".into());
            }
        }
        Ok(())
    }
}
