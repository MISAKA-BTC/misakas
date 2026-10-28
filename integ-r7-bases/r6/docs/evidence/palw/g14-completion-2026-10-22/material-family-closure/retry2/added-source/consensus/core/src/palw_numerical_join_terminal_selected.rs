{
    // Actual selected root + real observed primitive reads; observation is test
    // provisioning only. Allocation inventory remains original-wide.
    use crate::palw_tir_court_v1::parameter_cover::RootInputBoundaryV1 as I;
    use crate::palw_tir_public_source_v1::PalwTirPublicRequestV1 as Q;
    let selected=parameter_cover.roots_v1().iter().find(|r|r.leaf==leaf).unwrap();
    let mut indices=std::collections::BTreeSet::from([leaf]);
    for input in &selected.inputs {match *input {I::CarryStepLeaf(n)|I::CommittedStepLeaf(n)|I::FixedCheckpointStepLeaf(n)|I::HistoryStepLeaf(n)=>{indices.insert(n);},_=>{}}}
    for index in 0..binding.step_leaf_count {if calls.contains(&format!("step_leaf({index})"))||calls.contains(&format!("step_opening({index})")){indices.insert(index);}}
    let partial_units:Vec<_>=material_units.iter().filter(|u|matches!(u.request_v1(),Q::StepLeaf{index} if indices.contains(&index))).cloned().collect();
    let rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context,&fx.execution.rows).unwrap();let ids=parameter_source.retain_complete_ids_v1(rows,&fx.execution.generated).unwrap();
    let partial=MaterialStore::new_with_retained_parameters(&round_state,claim,scope,&partial_units,&covered_parameters,Some(&ids),crate::palw_court_shared_tir_builder_store_v1::PromptMaterialV1::Flat(&fx.execution.prompt)).unwrap();
    material_join.check_named_material_v1(&partial,leaf).unwrap();
    assert!(parameter_cover.roots_v1().iter().any(|r|!indices.contains(&r.leaf)),"actual terminal leaves unrelated roots absent");
    assert!(material_join.check_material_v1(&partial).is_err());
    let (joined,joined_meter)=material_join.build_terminal_v1(&partial,&round_state,&params,&intent_rules,&intent,limit).unwrap();
    assert_eq!(borsh::to_vec(&joined).unwrap(),borsh::to_vec(&built).unwrap());assert_eq!(joined_meter,meter);
    let absent:Vec<_>=partial_units.iter().filter(|u|u.request_v1()!=(Q::StepLeaf{index:leaf})).cloned().collect();
    let absent=MaterialStore::new_with_retained_parameters(&round_state,claim,scope,&absent,&covered_parameters,Some(&ids),crate::palw_court_shared_tir_builder_store_v1::PromptMaterialV1::Flat(&fx.execution.prompt)).unwrap();
    assert!(material_join.build_terminal_v1(&absent,&round_state,&params,&intent_rules,&intent,limit).is_err());
}
