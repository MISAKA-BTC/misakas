// Real retained original worlds; no mined admission, paid-capacity or Complete.
#[test]
fn original_aggregate_draft_requires_native_variant_and_never_grants_reservation(){
    use crate::palw_court_shared_tir_builder_store_v1::{original_aggregate::{OriginalAggregateDraftV1 as A,OriginalFamilyOwnersV1 as F,TokenizerOwnersV1 as T,OriginalGrammarV1 as K,AggregateNotReadyV1 as NotReady,UnsupportedNativeGrammarV1 as Unsupported},
        output_terminal::NativeOutputDutyV1 as O,selection_terminal::NativeSelectionUniverseV1 as L,prefix_family::NativePrefixFamilyV1 as P,numerical_material_join::NumericalMaterialJoinV1 as J};
    use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1 as S;
    for kind in 0..3{
        let fx=if kind==2{crate::palw_tir_public_claim_v1::palw_tir_public_prefix_history_frame_fixture_v1(false)}else if kind==1{
            crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(1,|j|{j.version=crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;j.decode=Some(crate::palw_decode_pipeline_v4::DecodeConfigV4::NOOP);},None)
        }else{crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None,1)};
        let frame=&fx.frame;let claim=frame.plan_v1().original_claim;
        let (s,params,extras)=world(&fx,frame);let state=if kind==2{prefix_material_world(&fx).0}else{bind_clock_service(&s,frame,&params,&extras)};
        let source=S::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
        let profile=crate::palw_court_shared_tir_state_v1::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();
        let p=frame.plan_v1();let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        if kind!=2 {let context=crate::palw_court_shared_tir_state_v1::SharedTirContextV1{version:1,claim,allocation_id:p.allocation_id,source_id:p.source_id,rules_id:p.rules_id,profile_id:profile,scope_id:set.scope_id().unwrap(),queries:set.len(),original_accepted_daa:p.terms.accepted_daa,hard_daa:p.terms.accepted_daa+10000,material_until_daa:p.terms.material_until_daa};b.write_staged_row_v1(Key::CourtSharedContext(claim),Some(Row::CourtSharedContext(Box::new(context)))).unwrap();}
        let before=b.state.state_root();
        let (graph,geometry,cover,visits)=aggregate_actual_geometry_fixture_v1(&fx,&source,profile);
        let join=J::derive_native_v1(frame,&source,&graph,&geometry,&cover,profile,&fx.rules.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,visits).unwrap();
        let output=O::derive_native_v1(frame,&source,&graph,profile).unwrap();
        if kind==2 {let prefix=P::derive_native_v1(&b.state,frame,&source,&graph,profile,source.leaf_count_v1(),visits).unwrap();
            let aggregate=A::derive_current_v1(&b.state,frame,&source,&graph,profile,&join,F::Prefix{output:&output,prefix:&prefix},T::NotApplicable).unwrap();
            assert_eq!(aggregate.shape_v1().grammar_v1(),K::Prefix);
            assert!(matches!(aggregate.reservation_readiness_v1(),Err(NotReady::UnsupportedNativeGrammar(Unsupported::PrefixGeneratedSelectionAndStop))));
        }else{let selection=L::derive_native_v1(frame,&source,&graph,profile).unwrap();
            let aggregate=A::derive_current_v1(&b.state,frame,&source,&graph,profile,&join,F::Ordinary{output_and_stop:&output,selection:&selection},T::NotApplicable).unwrap();
            let required=std::cell::Cell::new(0);
            let probe=A::derive_reserved_v1(&b.state,frame,&source,&graph,profile,&join,F::Ordinary{output_and_stop:&output,selection:&selection},T::NotApplicable,|q|{required.set(q);Ok(None)}).unwrap();drop(probe);
            for limit in [required.get()-1,required.get()]{let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:limit,database_bytes:0});
                let result=A::derive_reserved_v1(&b.state,frame,&source,&graph,profile,&join,F::Ordinary{output_and_stop:&output,selection:&selection},T::NotApplicable,|q|checker.reserve_requests_v1(q,0).map(Some));
                if limit==required.get(){let held=result.unwrap();assert_eq!(checker.outstanding(),(required.get(),0));drop(held);}else{assert!(matches!(result,Err(crate::palw_court_shared_tir_builder_store_v1::original_aggregate::AggregateErrorV1::Resource(_))));}assert_eq!(checker.outstanding(),(0,0));}
            assert_eq!(aggregate.shape_v1().grammar_v1(),K::Ordinary);assert!(aggregate.unresolved_supply_v1().next().is_some());
            assert!(matches!(aggregate.reservation_readiness_v1(),Err(NotReady::UnresolvedSupplyContract(_))));
            {use crate::palw_court_shared_tir_builder_store_v1::original_aggregate::{AllBranchTermsSourceV1,UnresolvedSupplyContractV1 as U};
                struct Probe(std::cell::Cell<Option<crate::Hash64>>);
                impl AllBranchTermsSourceV1 for Probe{fn adopted_terms_v1(&self,c:crate::Hash64)->Result<std::convert::Infallible,U>{self.0.set(Some(c));Err(U::NativeAllBranchPacketAndFundingTerms)}}
                let probe=Probe(std::cell::Cell::new(None));let r=aggregate.reservation_readiness_with_v1(&probe);
                let local=aggregate.unresolved_supply_v1().find(|u|*u!=U::NativeAllBranchPacketAndFundingTerms);
                match local{Some(u)=>{assert_eq!(r.unwrap_err(),NotReady::UnresolvedSupplyContract(u));assert_eq!(probe.0.get(),None,"family-local supply is reported before the terms source");},
                    None=>{assert_eq!(r.unwrap_err(),NotReady::UnresolvedSupplyContract(U::NativeAllBranchPacketAndFundingTerms));assert_eq!(probe.0.get(),Some(claim),"terms are asked for the actual original");}}}
            assert!(A::derive_current_v1(&b.state,frame,&source,&graph,h(0xABA1),&join,F::Ordinary{output_and_stop:&output,selection:&selection},T::NotApplicable).is_err());
        }
        assert_eq!(before,b.state.state_root());
    }
}
/// Actual retained geometry and all-root relation, with unchanged test-local
/// exploration budgets; no profile authority or source-byte cloning.
fn aggregate_actual_geometry_fixture_v1(fx:&PalwTirPublicFrameFixtureV1,source:&crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_source_v1::PalwTirPublicSourceV1>,profile:Hash64)
    ->(crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3>,crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3>,crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1>,u64){
    use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as G,SourceGraphBudgetV3 as B};
    let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(&fx.frame,source,profile).unwrap();let up=set.signed_data_transport_upper().unwrap();
    let graph=G::derive_native_v3(&fx.frame,source,profile,B{max_queries:set.len(),max_roles:up.maximum_custodians,max_vertices:up.signed_controls,max_edges:up.fresh_responses.checked_mul(up.maximum_custodians+1).unwrap().checked_add(2*up.streams).unwrap().checked_add(up.parts).unwrap()}).unwrap();
    let geometry=crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3::derive_native_v3(source,&fx.record,&fx.layout,&fx.rules.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3{max_original_leaves:1_000_000,max_roots:1_000_000,max_paths:10_000_000,max_edges:10_000_000,max_path_bytes:10_000_000}).unwrap();
    let cover=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::root_input_cover_native_v1(source,&fx.record,&fx.layout,crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1{max_original_leaves:1_000_000,max_total_edges:10_000_000,max_total_coordinates:1_000_000,max_root_memberships:1_000_000}).unwrap();
    let visits=(graph.nodes_v3().count() as u64).checked_mul(cover.roots_v1().iter().map(|r|1+r.inputs.len() as u64).sum::<u64>()).unwrap().max((graph.nodes_v3().count() as u64).checked_mul(source.leaf_count_v1()).unwrap());
    (graph,geometry,cover,visits)
}
