//! Separate own-node authority for the versioned routed-material journal.
//! A saved journal record is data identity, never current serving authority or
//! selected-verifier approval. The old V3 response keeps its original tuple.
use super::*;

/// Authenticate the prospective selected-parent context using the same virtual
/// snapshot that template.rs uses. Merely containing `checkpoint` among caller
/// supplied parents does not establish that GHOSTDAG selected it. Exact own
/// direct-parent SET and clock/work equality do, without rebuilding a template
/// or allocating another parent graph. Higher levels remain ordinary block
/// validation's responsibility.
fn require_routing_template_context_v2(own: &VirtualState, checkpoint: BlockHash, candidate: &Header) -> Result<(), String> {
    let parents = candidate.direct_parents();
    if own.ghostdag_data.selected_parent != checkpoint
        || parents.len() != own.parents.len()
        || !own.parents.iter().all(|parent| parents.contains(parent))
        || candidate.daa_score != own.daa_score
        || candidate.bits != own.bits
        || candidate.blue_score != own.ghostdag_data.blue_score
        || candidate.blue_work != own.ghostdag_data.blue_work
    {
        return Err("routing execution candidate differs from the own selected-parent template context".into());
    }
    Ok(())
}

impl VirtualStateProcessor {
    pub(crate) fn palw_native_routing_claim_authority_v2_impl(
        &self,
        params: &kaspa_consensus_core::config::params::Params,
        claim: Hash64,
        job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
    ) -> Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2>>, String> {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return Err("routing replay requires native ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let checkpoint = before.ghostdag_data.selected_parent;
        let (import, stable) = self.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() {
            return Err("routing replay own import is unavailable".into());
        }
        let (point, state) =
            self.palw_v2_reader_state(&bundle.state).map_err(|e| e.to_string())?.ok_or("routing replay own native state absent")?;
        if point != checkpoint {
            return Err("routing replay reader returned another point".into());
        }
        let header = self.headers_store.get_header(checkpoint).map_err(|e| e.to_string())?;
        if self.statuses_store.read().get(checkpoint).ok() != Some(kaspa_consensus_core::blockstatus::BlockStatus::StatusUTXOValid)
            || kaspa_consensus_core::hashing::header::hash(&header) != checkpoint
            || state.last_point().is_none_or(|p| p.daa_score != header.daa_score)
        {
            return Err("routing replay own point is not an immutable validated checkpoint".into());
        }
        let result = kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2::from_native_claim_v2(
            state,
            params,
            checkpoint,
            claim,
            job,
            &self.network_id_bytes,
            import.epoch,
        )
        .map_err(|e| e.to_string())?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || import.epoch != end.epoch
            || after.ghostdag_data.selected_parent != checkpoint
            || before.daa_score != after.daa_score
        {
            return Err("routing replay own snapshot/import generation changed".into());
        }
        Ok(result.map(Arc::new))
    }
    pub(crate) fn palw_native_routing_execution_authority_v2_impl(
        &self,
        params: &kaspa_consensus_core::config::params::Params,
        checkpoint: BlockHash,
        candidate: &Header,
        job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
        executor: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    ) -> Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingExecutionAuthorityV2>>, String> {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return Err("routing execution requires native ConsensusV2".into());
        };
        let before = self.lkg_virtual_state.load();
        let (import, stable) = self.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable || import.active.is_some() || before.ghostdag_data.selected_parent != checkpoint {
            return Err("routing execution own snapshot/import is unavailable".into());
        }
        require_routing_template_context_v2(&before, checkpoint, candidate)?;
        let (point, state) =
            self.palw_v2_reader_state(&bundle.state).map_err(|e| e.to_string())?.ok_or("routing execution native state missing")?;
        if point != checkpoint {
            return Err("routing execution reader returned an old point".into());
        }
        let header = self.headers_store.get_header(checkpoint).map_err(|e| e.to_string())?;
        if self.statuses_store.read().get(checkpoint).ok() != Some(kaspa_consensus_core::blockstatus::BlockStatus::StatusUTXOValid)
            || kaspa_consensus_core::hashing::header::hash(&header) != checkpoint
        {
            return Err("routing execution own selected parent is not an immutable validated header".into());
        }
        // `state` is AFTER(checkpoint). The candidate/template, not `header`,
        // commits this state. The Core factory computes and checks that root
        // under native allocation coverage before creating any authority.
        let result = kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingExecutionAuthorityV2::from_imported_snapshot_v2(
            state,
            params,
            &header,
            candidate,
            job,
            executor,
            &self.network_id_bytes,
            import.epoch,
        )
        .map_err(|e| e.to_string())?;
        let after = self.lkg_virtual_state.load();
        let (end, stable) = self.pruning_import_status().map_err(|e| e.to_string())?;
        if !stable
            || end.active.is_some()
            || import.epoch != end.epoch
            || after.ghostdag_data.selected_parent != checkpoint
            || before.daa_score != after.daa_score
        {
            return Err("routing execution native/import generation changed".into());
        }
        require_routing_template_context_v2(&after, checkpoint, candidate)?;
        Ok(result.map(Arc::new))
    }
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

#[cfg(test)]
mod routing_execution_context_tests {
    use super::*;

    #[test]
    fn candidate_must_use_actual_own_selected_parent_context() {
        let checkpoint = Hash64::from_u64_word(1);
        let other = Hash64::from_u64_word(2);
        let mut own = VirtualState { parents: vec![checkpoint, other], daa_score: 7, ..Default::default() };
        own.ghostdag_data.selected_parent = checkpoint;
        let mut candidate = Header::from_precomputed_hash(Hash64::from_u64_word(3), vec![other, checkpoint]);
        candidate.daa_score = own.daa_score;
        require_routing_template_context_v2(&own, checkpoint, &candidate).unwrap();
        // Containing the checkpoint is insufficient when another parent could
        // change the selected parent. Reordering the identical own set is fine.
        candidate.parents_by_level.set_direct_parents(vec![checkpoint, Hash64::from_u64_word(4)]);
        assert!(require_routing_template_context_v2(&own, checkpoint, &candidate).is_err());
        candidate.parents_by_level.set_direct_parents(vec![checkpoint, other]);
        candidate.daa_score += 1;
        assert!(require_routing_template_context_v2(&own, checkpoint, &candidate).is_err());
        candidate.daa_score = own.daa_score;
        own.ghostdag_data.selected_parent = other;
        assert!(require_routing_template_context_v2(&own, checkpoint, &candidate).is_err());
    }
}
