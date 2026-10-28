#[test]
fn numerical_material_join_actual_all_root_readset_and_supplier_finishes(){
    use crate::palw_court_shared_tir_builder_store_v1::{SharedTirBuilderStoreV1 as Store,PromptMaterialV1 as P,numerical_material_join::{NumericalMaterialJoinV1 as Join,NativeMaterialTargetV1 as M,UnresolvedSupplyV1 as U}};
    use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as S,PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
    use crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 as B;
    use crate::palw_court_numerical_geometry_v3::{NativeNumericalGeometryV3 as G,NumericalGeometryBudgetV3 as GB};
    use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as Graph,SourceGraphBudgetV3};
    use crate::palw_tir_court_v1::{PalwTirEvidenceStoreV1,parameter_cover::RootInputBoundaryV1 as I};
    use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1 as F,prompt_ids_opening_v1,PALW_PROMPT_IDS_TILE_LEN};
    let budget=B{max_original_leaves:1_000_000,max_total_edges:10_000_000,max_total_coordinates:1_000_000,max_root_memberships:1_000_000};
    let geometry=GB{max_original_leaves:1_000_000,max_roots:1_000_000,max_paths:10_000_000,max_edges:10_000_000,max_path_bytes:10_000_000};
    for kind in 0..5 {
        let form=if kind==1{F::MerkleV1}else{F::Flat};
        let fx=match kind {0|1=>crate::palw_tir_public_claim_v1::palw_tir_public_input_history_frame_fixture_v1(form),
            2|3=>crate::palw_tir_public_claim_v1::palw_tir_public_fixed_history_frame_fixture_v1(form,kind==2),
            _=>crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1()};
        let frame=&fx.frame;let p=frame.plan_v1();let claim=p.original_claim;
        let (state,params,extras)=world(&fx,frame);let state=bind_clock_service(&state,frame,&params,&extras);
        let source=S::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
        let profile=crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();let scope=set.scope_id().unwrap();let upper=set.signed_data_transport_upper().unwrap();
        let graph=Graph::derive_native_v3(frame,&source,profile,SourceGraphBudgetV3{max_queries:set.len(),max_roles:upper.maximum_custodians,max_vertices:upper.signed_controls,
            max_edges:upper.fresh_responses.checked_mul(upper.maximum_custodians+1).unwrap().checked_add(2*upper.streams).unwrap().checked_add(upper.parts).unwrap()}).unwrap();
        let numerical=G::derive_native_v3(&source,&fx.record,&fx.layout,&fx.rules.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,geometry).unwrap();
        let cover=S::root_input_cover_native_v1(&source,&fx.record,&fx.layout,budget).unwrap();assert!(!cover.roots_v1().is_empty());
        let visits=(graph.nodes_v3().count() as u64).checked_mul(cover.roots_v1().iter().map(|r|1+r.inputs.len() as u64).sum::<u64>()).unwrap();
        let demand=crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7;
        let required=std::cell::Cell::new(0);let joined=Join::derive_reserved_v1(frame,&source,&graph,&numerical,&cover,profile,&fx.rules.court,demand,visits,|q|{required.set(q);Ok(None)}).unwrap();
        assert!(!joined.dependencies_v1().is_empty());
        for d in joined.dependencies_v1(){assert!(cover.roots_v1().iter().any(|r|r.leaf==d.root));
            assert!(graph.nodes_v3().any(|n|n.id_v3()==d.logical&&graph.absolute_packet_id_v3(n.id_v3())==d.absolute));
            if let M::StepLeaf(index)=d.material {assert!(index<source.leaf_count_v1());}}
        for limit in [required.get()-1,required.get()]{let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:limit,database_bytes:0});
            let got=Join::derive_reserved_v1(frame,&source,&graph,&numerical,&cover,profile,&fx.rules.court,demand,visits,|q|checker.reserve_requests_v1(q,0).map(Some));
            if limit==required.get(){let held=got.unwrap();assert_eq!(checker.outstanding(),(required.get(),0));drop(held);}else{assert!(matches!(got,Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));}assert_eq!(checker.outstanding(),(0,0));}
        assert!(matches!(Join::derive_native_v1(frame,&source,&graph,&numerical,&cover,profile,&fx.rules.court,demand,0),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        assert!(Join::derive_native_v1(frame,&source,&graph,&numerical,&cover,h(0xAAEF),&fx.rules.court,demand,visits).is_err());
        let c=crate::palw_court_shared_tir_state_v1::SharedTirContextV1{version:1,claim,allocation_id:p.allocation_id,source_id:p.source_id,rules_id:p.rules_id,profile_id:profile,scope_id:scope,queries:set.len(),original_accepted_daa:p.terms.accepted_daa,hard_daa:p.terms.accepted_daa+10000,material_until_daa:p.terms.material_until_daa};
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);b.write_staged_row_v1(Key::CourtSharedContext(claim),Some(Row::CourtSharedContext(Box::new(c)))).unwrap();let before=b.state.state_root();
        let units:Vec<_>=(0..source.leaf_count_v1()).map(|index|source.authenticate_v1(&Q::StepLeaf{index},&R::StepLeaf{preimage:fx.execution.step_leaf(index).unwrap(),opening:fx.execution.step_opening(index).unwrap()},None).unwrap()).collect();
        let rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context,&fx.execution.rows).unwrap();let ids=source.retain_complete_ids_v1(rows,&fx.execution.generated).unwrap();
        let parameters:Vec<_>=cover.parameters_v1().union.iter().map(|&n|fx.execution.param_opening(n).unwrap()).collect();
        let tiles:Vec<_>=(0..fx.execution.prompt.len()).step_by(PALW_PROMPT_IDS_TILE_LEN as usize).map(|p|prompt_ids_opening_v1(&fx.execution.prompt,p as u32).unwrap()).collect();
        let prompt=if form==F::Flat{P::Flat(&fx.execution.prompt)}else{P::Merkle(&tiles)};
        let store=Store::new_with_prompt(&b.state,claim,scope,&units,&parameters,Some(&ids),prompt).unwrap();joined.check_material_v1(&store).unwrap();
        if kind<2 {assert!(joined.unresolved_v1().any(|u|matches!(u,U::Prompt{..})));assert!(cover.roots_v1().iter().any(|r|r.inputs.iter().any(|i|matches!(i,I::CarryStepLeaf(_)))));}
        if kind==2 {assert!(cover.roots_v1().iter().any(|r|r.inputs.iter().any(|i|matches!(i,I::CommittedStepLeaf(_)))));}
        if kind==3 {assert!(cover.roots_v1().iter().any(|r|r.inputs.iter().any(|i|matches!(i,I::FixedCheckpointStepLeaf(_)))));}
        // Keep only one actual root's authenticated input relation and its own
        // output. Deliberately omit every unrelated root from this runtime Store.
        let selected=&cover.roots_v1()[0];
        let mut selected_steps=std::collections::BTreeSet::from([selected.leaf]);
        for i in &selected.inputs{match *i {I::CarryStepLeaf(n)|I::CommittedStepLeaf(n)|I::FixedCheckpointStepLeaf(n)|I::HistoryStepLeaf(n)=>{selected_steps.insert(n);},_=>{}}}
        let sparse:Vec<_>=units.iter().filter(|u|matches!(u.request_v1(),Q::StepLeaf{index} if selected_steps.contains(&index))).cloned().collect();
        let selected_params=&cover.parameters_v1().roots.iter().find(|r|r.leaf==selected.leaf).unwrap().parameters;
        let sparse_params:Vec<_>=parameters.iter().filter(|p|selected_params.contains(&p.leaf_index)).cloned().collect();
        let partial=Store::new_with_prompt(&b.state,claim,scope,&sparse,&sparse_params,Some(&ids),prompt).unwrap();
        joined.check_named_material_v1(&partial,selected.leaf).unwrap();
        assert!(cover.roots_v1().iter().any(|r|!selected_steps.contains(&r.leaf)),"actual fixture has unrelated numerical roots");
        assert!(joined.check_material_v1(&partial).is_err(),"allroot audit remains distinct from selected runtime");
        let no_output:Vec<_>=sparse.iter().filter(|u|u.request_v1()!=(Q::StepLeaf{index:selected.leaf})).cloned().collect();
        let no_output=Store::new_with_prompt(&b.state,claim,scope,&no_output,&sparse_params,Some(&ids),prompt).unwrap();assert!(joined.check_named_material_v1(&no_output,selected.leaf).is_err());
        let empty=Store::new_with_prompt(&b.state,claim,scope,&[],&parameters,Some(&ids),prompt).unwrap();assert!(joined.check_material_v1(&empty).is_err());
        let missing=Store::new(&b.state,claim,scope,&units,&parameters,Some(&ids)).unwrap();if kind<2{assert!(joined.check_material_v1(&missing).is_err());}
        if !parameters.is_empty(){let missing=Store::new_with_prompt(&b.state,claim,scope,&units,&[],Some(&ids),prompt).unwrap();assert!(joined.check_material_v1(&missing).is_err());}
        let foreign=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1();let foreign_source=S::new_native_v1(&foreign.record,&foreign.layout,foreign.frame.header_v1(),foreign.frame.original_v1().execution_root,foreign.frame.original_v1().trace_root,foreign.rules.maximum_leaves).unwrap();
        let foreign_cover=S::root_input_cover_native_v1(&foreign_source,&foreign.record,&foreign.layout,budget).unwrap();assert!(Join::derive_native_v1(frame,&source,&graph,&numerical,&foreign_cover,profile,&fx.rules.court,demand,visits).is_err());
        assert_eq!(before,b.state.state_root());
    }
}
