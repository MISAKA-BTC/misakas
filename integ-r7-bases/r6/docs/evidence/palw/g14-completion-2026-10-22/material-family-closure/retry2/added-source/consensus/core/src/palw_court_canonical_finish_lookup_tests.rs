#[test]
fn canonical_finish_lookup_matches_existing_fullscan_and_step_family_inverse(){
    let f=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None,1);let frame=&f.frame;
    let source=PalwTirPublicSourceV1::new_native_v1(&f.record,&f.layout,frame.header_v1(),frame.original_v1().execution_root,frame.original_v1().trace_root,f.rules.maximum_leaves).unwrap();
    let profile=Hash64::from_u64_word(0xF1A1);let set=TirSharedRequestSetV1::new(frame,&source,profile).unwrap();
    let up=set.signed_data_transport_upper().unwrap();let budget=SourceGraphBudgetV3{max_queries:set.len(),max_roles:up.maximum_custodians,max_vertices:up.signed_controls,max_edges:up.fresh_responses.checked_mul(up.maximum_custodians+1).unwrap().checked_add(2*up.streams).unwrap().checked_add(up.parts).unwrap()};
    let graph=NativeSourceMaterialGraphV3::derive_native_v3(frame,&source,profile,budget).unwrap();
    // One small actual native graph, no fixture shrinking/cap changes. Scan
    // equivalence is test-only and is not reintroduced to production joins.
    for index in 0..source.leaf_count_v1(){let ordinal=set.step_leaf_ordinal_v1(index).unwrap();assert_eq!(set.request_at(ordinal).unwrap(),Request::StepLeaf{index});
        for role in 0..graph.roles {
            let target=SourceMaterialTargetV3::Unit(ordinal);let key=SourceMaterialNodeKindV3::Finish{target,role};
            let old=graph.nodes_v3().find(|n|n.kind_v3()==&key).expect("old canonical Finish");
            let direct=graph.finish_node_v3(target,role).unwrap().unwrap();assert_eq!(direct,old);assert_eq!(graph.absolute_packet_id_v3(direct.id_v3()),graph.absolute_packet_id_v3(old.id_v3()));
        }
    }
    for role in 0..graph.roles{let key=SourceMaterialNodeKindV3::Finish{target:SourceMaterialTargetV3::FullIds,role};let old=graph.nodes_v3().find(|n|n.kind_v3()==&key);assert_eq!(graph.finish_node_v3(SourceMaterialTargetV3::FullIds,role).unwrap(),old);}
    assert!(set.step_leaf_ordinal_v1(source.leaf_count_v1()).is_err());assert!(set.step_leaf_ordinal_v1(u64::MAX).is_err());
    assert!(graph.finish_node_v3(SourceMaterialTargetV3::Unit(set.len()),0).is_err());assert!(graph.finish_node_v3(SourceMaterialTargetV3::Unit(u64::MAX),0).is_err());assert!(graph.finish_node_v3(SourceMaterialTargetV3::FullIds,graph.roles).is_err());
}
