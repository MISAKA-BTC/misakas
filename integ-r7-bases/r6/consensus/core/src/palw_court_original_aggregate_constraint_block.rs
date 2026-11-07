{
    use crate::palw_court_shared_tir_builder_store_v1::original_aggregate::{OriginalAggregateDraftV1 as A,OriginalFamilyOwnersV1 as F,TokenizerOwnersV1 as T,OriginalGrammarV1 as K,AggregateNotReadyV1 as N};
    let geometry=crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3::derive_native_v3(&source,&fx.record,&fx.layout,&fx.rules.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3{max_original_leaves:1_000_000,max_roots:1_000_000,max_paths:10_000_000,max_edges:10_000_000,max_path_bytes:10_000_000}).unwrap();
    let cover=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::root_input_cover_native_v1(&source,&fx.record,&fx.layout,crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1{max_original_leaves:1_000_000,max_total_edges:10_000_000,max_total_coordinates:1_000_000,max_root_memberships:1_000_000}).unwrap();
    let visits=(family_graph.nodes_v3().count() as u64).checked_mul(cover.roots_v1().iter().map(|r|1+r.inputs.len() as u64).sum::<u64>()).unwrap();
    let join=crate::palw_court_shared_tir_builder_store_v1::numerical_material_join::NumericalMaterialJoinV1::derive_native_v1(frame,&source,&family_graph,&geometry,&cover,profile,&fx.rules.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,visits).unwrap();
    let a=A::derive_current_v1(&b.state,frame,&source,&family_graph,profile,&join,F::Constraint{constraint:&family},T::NotApplicable).unwrap();
    assert_eq!(a.shape_v1().grammar_v1(),K::Constraint);assert!(matches!(a.reservation_readiness_v1(),Err(N::UnresolvedSupplyContract(_))));
    assert!(a.unresolved_supply_v1().any(|x|matches!(x,crate::palw_court_shared_tir_builder_store_v1::original_aggregate::UnresolvedSupplyContractV1::Constraint(_))));
}
