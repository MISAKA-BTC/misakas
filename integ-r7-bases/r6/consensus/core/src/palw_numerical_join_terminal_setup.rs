{
    use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3 as G,SourceGraphBudgetV3 as B};
    let profile=shared::context(&fresh,claim).unwrap().profile_id;
    let req=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&parameter_source,profile).unwrap();let up=req.signed_data_transport_upper().unwrap();
    let graph=G::derive_native_v3(frame,&parameter_source,profile,B{max_queries:req.len(),max_roles:up.maximum_custodians,max_vertices:up.signed_controls,max_edges:up.fresh_responses.checked_mul(up.maximum_custodians+1).unwrap().checked_add(2*up.streams).unwrap().checked_add(up.parts).unwrap()}).unwrap();
    let numerical=crate::palw_court_numerical_geometry_v3::NativeNumericalGeometryV3::derive_native_v3(&parameter_source,&fx.record,&fx.layout,&native.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,
        crate::palw_court_numerical_geometry_v3::NumericalGeometryBudgetV3{max_original_leaves:1_000_000,max_roots:1_000_000,max_paths:10_000_000,max_edges:10_000_000,max_path_bytes:10_000_000}).unwrap();
    let visits=(graph.nodes_v3().count() as u64).checked_mul(parameter_cover.roots_v1().iter().map(|r|1+r.inputs.len() as u64).sum::<u64>()).unwrap();
    (graph,numerical,profile,visits)
}
