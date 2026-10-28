#[test]
fn prefix_family_actual_canonical_boundaries_all_suppliers_and_replay(){
    use crate::palw_court_shared_tir_builder_store_v1::{SharedTirBuilderStoreV1 as Store,prefix_family::NativePrefixFamilyV1 as Family,prefix_terminal::{Budget,prelude_requests,replay_requests}};
    use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as S,PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
    use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as G,SourceGraphBudgetV3 as B};
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    for fault in [false,true]{
        let fx=crate::palw_tir_public_claim_v1::palw_tir_public_prefix_history_frame_fixture_v1(fault);let frame=&fx.frame;
        let (state,claim,scope)=prefix_material_world(&fx);let before=state.state_root();
        let source=S::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
        let profile=crate::palw_court_shared_tir_state_v1::context(&state,claim).unwrap().profile_id;
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();let up=set.signed_data_transport_upper().unwrap();
        let graph=G::derive_native_v3(frame,&source,profile,B{max_queries:set.len(),max_roles:up.maximum_custodians,max_vertices:up.signed_controls,max_edges:up.fresh_responses.checked_mul(up.maximum_custodians+1).unwrap().checked_add(2*up.streams).unwrap().checked_add(up.parts).unwrap()}).unwrap();
        let prefix=crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(&fx.record,&fx.layout,frame.header_v1(),&frame.original_v1().job,fx.rules.maximum_leaves).unwrap();
        let total=prefix.template.total_v1();assert!(total>0);let visits=total.checked_mul(graph.nodes_v3().count() as u64).unwrap();
        let first=std::cell::Cell::new(0);let family=Family::derive_reserved_v1(&state,frame,&source,&graph,profile,total,visits,|q|{first.set(q);Ok(None)}).unwrap();
        let lookup_work=total.checked_mul(graph.role_slots_v3()).unwrap();
        let exact=Family::derive_native_v1(&state,frame,&source,&graph,profile,total,lookup_work).unwrap();assert_eq!(exact.dependencies_v1(),family.dependencies_v1());
        assert!(matches!(Family::derive_native_v1(&state,frame,&source,&graph,profile,total,lookup_work-1),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        assert_eq!(family.total_v1(),total);assert_eq!(family.dependencies_v1().len() as u64,total*graph.role_slots_v3());
        let named=crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(&frame.original_v1().job).unwrap();
        for d in family.dependencies_v1(){let leaf=prefix.space.prefix_state_leaf_v1(&prefix.job_context,named.prefix_tokens,d.ordinal).unwrap();assert_eq!(d.index,leaf.index);
            assert!(graph.nodes_v3().any(|n|n.id_v3()==d.logical&&graph.absolute_packet_id_v3(n.id_v3())==d.absolute));}
        let mut extra=crate::palw_native_resource_v1::TirNativeRequestsV1::new();extra.exact_v1::<crate::palw_court_shared_tir_builder_store_v1::prefix_family::NativePrefixFamilyV1<'_>>(1).unwrap();
        extra.exact_v1::<crate::palw_court_shared_tir_builder_store_v1::prefix_family::PrefixBoundaryDependencyV1>(total*graph.role_slots_v3()).unwrap();let required=first.get().checked_add(extra.bytes_v1()).unwrap();
        for limit in [first.get()-1,required-1,required]{let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:limit,database_bytes:0});
            let got=Family::derive_reserved_v1(&state,frame,&source,&graph,profile,total,visits,|q|checker.reserve_requests_v1(q,0).map(Some));
            if limit==required{let held=got.unwrap();assert_eq!(checker.outstanding(),(required,0));drop(held);}else{assert!(matches!(got,Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));}assert_eq!(checker.outstanding(),(0,0));}
        assert!(matches!(Family::derive_native_v1(&state,frame,&source,&graph,profile,0,visits),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        assert!(matches!(Family::derive_native_v1(&state,frame,&source,&graph,profile,total,0),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        assert!(Family::derive_native_v1(&state,frame,&source,&graph,h(0xAF91),total,visits).is_err());
        let units:Vec<_>=(0..source.leaf_count_v1()).map(|index|source.authenticate_v1(&Q::StepLeaf{index},&R::StepLeaf{preimage:fx.execution.step_leaf(index).unwrap(),opening:fx.execution.step_opening(index).unwrap()},None).unwrap()).collect();
        let store=Store::new(&state,claim,scope,&units,&[],None).unwrap();let budget=Budget{max_leaves:source.leaf_count_v1(),max_requests:prelude_requests(frame,&fx.record,&fx.layout).unwrap().checked_add(replay_requests(prefix.leaf_bytes).unwrap()).unwrap()};
        let replay=family.replay_v1(&store,&state,budget).unwrap();let old=store.replay_prefix_material_v1(&state,claim,budget).unwrap();assert_eq!(replay.verdict(),old.verdict());assert_eq!(replay.leaves(),old.leaves());
        let missing=family.dependencies_v1()[0].index;let sparse:Vec<_>=units.iter().filter(|u|u.request_v1()!=(Q::StepLeaf{index:missing})).cloned().collect();let no=Store::new(&state,claim,scope,&sparse,&[],None).unwrap();
        assert!(matches!(family.replay_v1(&no,&state,budget),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(s)) if s.contains("authenticated boundary body")));
        let other=crate::palw_tir_public_claim_v1::palw_tir_public_prefix_history_frame_fixture_v1(!fault);let (other_state,_,_)=prefix_material_world(&other);assert!(family.replay_v1(&store,&other_state,budget).is_err());
        assert_eq!(state.state_root(),before);
    }
}
