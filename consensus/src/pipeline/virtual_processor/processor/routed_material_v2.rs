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
    fn work_refresh_requires_native_canonical_ancestry_and_same_import() {
        // This is the actual refresh door's decision after its fallible native
        // reachability lookup; an advanced canonical tip is allowed.
        assert!(require_routing_work_refresh_origin_v2(4,4,true).is_ok());
        use kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 as F;
        assert!(matches!(require_routing_work_refresh_origin_v2(4,4,false),Err(F::Unavailable(_))));
        assert!(matches!(require_routing_work_refresh_origin_v2(4,5,true),Err(F::Identity(_))));
        assert!(matches!(require_routing_work_refresh_origin_v2(5,4,true),Err(F::Identity(_))));
    }

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

fn require_routing_work_refresh_origin_v2(original_epoch:u64,current_epoch:u64,canonical:bool)->Result<(),kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> {
    if original_epoch!=current_epoch {
        // A completed import advances the local epoch permanently. This exact
        // original execution cannot acquire a replacement authority by retry.
        return Err(kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2::Identity("routing work import generation changed; reexecute"));
    }
    if !canonical {
        // A branch change may be reversed; do not classify a fallible or
        // currently noncanonical ancestry as terminal execution misconduct.
        return Err(kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2::Unavailable("routing work pinned checkpoint no longer canonical"));
    }
    Ok(())
}

fn routing_work_reachability_fault_v2(error:crate::processes::reachability::ReachabilityError)->kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 {
    match error {
        crate::processes::reachability::ReachabilityError::StoreError(error)=>routing_work_store_fault_v2(error),
        _=>kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2::Unavailable("routing work native ancestry unreadable"),
    }
}

fn routing_work_store_fault_v2(error:kaspa_database::prelude::StoreError)->kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 {
    use kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 as F;
    match error {
        kaspa_database::prelude::StoreError::LocalResourceUnavailable(reason)=>F::Resource(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::Unsupported {
            component:"native routing work snapshot".into(),native_type:"NativeChainStateV1".into(),reason,
        }),
        _=>F::Unavailable("native routing work store unreadable"),
    }
}
impl VirtualStateProcessor {
    // The general pruning_import_status API intentionally returns a formatted
    // context error. Keep these native-work capacity failures typed instead.
    fn routing_work_import_status_v2(&self)->Result<(kaspa_consensus_core::pruning::PruningImportSessionStateV1,bool),kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> {
        let meta=self.pruning_meta_stores.read();
        let state=meta.import_session().map_err(routing_work_store_fault_v2)?;
        let stable=meta.try_pruning_utxoset_stable_flag().map_err(routing_work_store_fault_v2)?;
        Ok((state,stable))
    }
    pub(crate) fn palw_native_routing_fp_work_authority_v2_impl(&self,params:&kaspa_consensus_core::config::params::Params,
        fp:&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,job:&kaspa_consensus_core::palw_v2::PalwJobContextV2,
    )->Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>>,kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> {
        self.native_routing_work_authority_v2_impl(params,job,kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(fp.executor_bond),Some(fp),None,None)
    }
    pub(crate) fn palw_native_routing_rider_work_authority_v2_impl(&self,params:&kaspa_consensus_core::config::params::Params,
        job:&kaspa_consensus_core::palw_v2::PalwJobContextV2,executor:kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        lead:Hash64,index:u32,retention:u64,
    )->Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>>,kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> {
        self.native_routing_work_authority_v2_impl(params,job,executor,None,Some((lead,index,retention)),None)
    }
    pub(crate) fn palw_native_routing_refresh_work_authority_v2_impl(&self,params:&kaspa_consensus_core::config::params::Params,
        original:&kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2,
    )->Result<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>,kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> {
        use kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2 as F;
        self.native_routing_work_authority_v2_impl(params,original.job(),original.executor(),original.fp_job(),original.rider(),Some(original))?
            .ok_or(F::Unavailable("native routing work fence or profile disappeared"))
    }
    fn native_routing_work_authority_v2_impl(&self,params:&kaspa_consensus_core::config::params::Params,
        job:&kaspa_consensus_core::palw_v2::PalwJobContextV2,executor:kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
        fp:Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,rider:Option<(Hash64,u32,u64)>,
        original:Option<&kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>,
    )->Result<Option<Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkAuthorityV2>>,kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingWorkFaultV2> {
        use kaspa_consensus_core::palw_reader_router_v2::{PalwNativeRoutingWorkAuthorityV2 as Owner,PalwNativeRoutingWorkFaultV2 as F};
        let before=self.lkg_virtual_state.load();
        if !params.palw_memory_routing_court_v2.is_some_and(|f| f!=kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(before.daa_score)) { return Ok(None); }
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)=&params.palw_consensus_mode else { return Err(F::Unavailable("routing work requires native ConsensusV2")); };
        let checkpoint=before.ghostdag_data.selected_parent;
        let (import,stable)=self.routing_work_import_status_v2()?;
        if !stable || import.active.is_some() { return Err(F::Unavailable("routing work own import unavailable")); }
        if let Some(original)=original {
            if self.statuses_store.read().get(original.checkpoint()).map_err(routing_work_store_fault_v2)?!=kaspa_consensus_core::blockstatus::BlockStatus::StatusUTXOValid {
                return Err(F::Unavailable("routing work pinned checkpoint no longer validated"));
            }
            let canonical=self.reachability_service.try_is_chain_ancestor_of(original.checkpoint(),checkpoint).map_err(routing_work_reachability_fault_v2)?;
            require_routing_work_refresh_origin_v2(original.import_epoch(),import.epoch,canonical)?;
        }
        let (point,state)=self.palw_v2_reader_state(&bundle.state).map_err(routing_work_store_fault_v2)?.ok_or(F::Unavailable("routing work own state absent"))?;
        if point!=checkpoint { return Err(F::Unavailable("routing work selected point changed")); }
        let header=self.headers_store.get_header(checkpoint).map_err(routing_work_store_fault_v2)?;
        if self.statuses_store.read().get(checkpoint).map_err(routing_work_store_fault_v2)?!=kaspa_consensus_core::blockstatus::BlockStatus::StatusUTXOValid
            || kaspa_consensus_core::hashing::header::hash(&header)!=checkpoint { return Err(F::Unavailable("routing work selected header not validated")); }
        let owner=if let Some(fp)=fp {
            let anchor=self.headers_store.get_header(fp.anchor_block).map_err(routing_work_store_fault_v2)?;
            if self.statuses_store.read().get(fp.anchor_block).map_err(routing_work_store_fault_v2)?!=kaspa_consensus_core::blockstatus::BlockStatus::StatusUTXOValid
                || kaspa_consensus_core::hashing::header::hash(&anchor)!=fp.anchor_block
                || !self.reachability_service.try_is_dag_ancestor_of(fp.anchor_block,checkpoint).map_err(routing_work_reachability_fault_v2)? {
                return Err(F::Unavailable("routing FP anchor is not own validated ancestor"));
            }
            Owner::from_native_fp_v2(state,params,&header,&anchor,before.daa_score,fp,job,&self.network_id_bytes,import.epoch)?
        } else if let Some((lead,index,retention))=rider {
            Owner::from_native_rider_v2(state,params,&header,before.daa_score,job,executor,lead,index,retention,&self.network_id_bytes,import.epoch)?
        } else { return Err(F::Identity("routing work origin absent")); };
        let after=self.lkg_virtual_state.load();
        let (end,stable)=self.routing_work_import_status_v2()?;
        if !stable || end.active.is_some() || import.epoch!=end.epoch || after.ghostdag_data.selected_parent!=checkpoint || before.daa_score!=after.daa_score {
            return Err(F::Unavailable("routing work native/import generation changed"));
        }
        if let Some(original)=original {
            if owner.as_ref().is_none_or(|fresh| !original.same_work_identity_v2(fresh)) {
                return Err(F::Unavailable("routing work fixed identity changed during execution"));
            }
        }
        Ok(owner.map(Arc::new))
    }
}
