//! Separate own-node authority for the versioned routed-material journal.
//! A saved journal record is data identity, never current serving authority or
//! selected-verifier approval. The old V3 response keeps its original tuple.
use super::*;

impl VirtualStateProcessor {
    pub(crate) fn palw_native_material_provider_authority_v4_impl(
        &self,
        params: &kaspa_consensus_core::config::params::Params,
        checkpoint: BlockHash,
        header: &Header,
        ready: &kaspa_consensus_core::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3,
    ) -> Result<kaspa_consensus_core::api::PalwNativeMaterialProviderAuthorityV4, String> {
        use kaspa_consensus_core::palw_routed_material_v2::PalwMaterialNativeAuthorityV2;
        let (before, stable) = self.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || before.active.is_some() {
            return Err("routed provider own import is unavailable".into());
        }
        // This validates the exact signed attempt, producer/class/artifact,
        // execution anchor, existing covenant and canonical provider roster at
        // this fixed parent. It does not reinterpret the returned V1 tuple.
        let authenticated = self.palw_native_material_provider_authority_common_v3(checkpoint, header, ready, true)?;
        let state = self
            .palw_candidate_state_v2_checked(checkpoint)
            .map_err(|e| format!("routed provider own candidate unavailable: {e:?}"))?;
        let a = &ready.base.attempt;
        let network_ladder = self
            .palw_court_params_v2
            .as_ref()
            .map(|court| self.palw_court_step_ladder_at(header.daa_score, court))
            .ok_or("routed provider actual native ladder absent")?;
        let material = PalwMaterialNativeAuthorityV2::from_native_candidate(
            &state,
            params,
            self.palw_network_domain_v2(),
            ready.ready.binding.claim,
            a.class_id,
            a.execution_root,
            header.daa_score,
            network_ladder,
        )
        .map_err(|e| e.to_string())?;
        // Reading the same immutable checkpoint twice cannot authorize a new
        // current consent. Refuse a generation change across either snapshot.
        let row = state.monetary_material_row_v1(ready.ready.session).ok_or("routed provider session absent")?;
        if row != authenticated.row {
            return Err("routed provider covenant changed across own candidate reads".into());
        }
        let (after, stable) = self.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || after.active.is_some() || before.epoch != after.epoch {
            return Err("routed provider import generation changed".into());
        }
        Ok(kaspa_consensus_core::api::PalwNativeMaterialProviderAuthorityV4 {
            row,
            material,
            provider_keys: authenticated.provider_keys,
        })
    }
}
