{
    use crate::palw_court_shared_tir_builder_store_v1::{original_aggregate::{OriginalAggregateDraftV1 as A,OriginalFamilyOwnersV1 as F,TokenizerOwnersV1 as T,TokenizerApplicabilityV1 as Applicable},output_terminal::NativeOutputDutyV1 as O,selection_terminal::NativeSelectionUniverseV1 as L};
    let profile=shared::context(&b.state,claim).unwrap().profile_id;
    let (graph,geometry,cover,visits)=aggregate_actual_geometry_fixture_v1(&fx,&source_owner,profile);
    let join=crate::palw_court_shared_tir_builder_store_v1::numerical_material_join::NumericalMaterialJoinV1::derive_native_v1(frame,&source_owner,&graph,&geometry,&cover,profile,&fx.rules.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,visits).unwrap();
    let output=O::derive_native_v1(frame,&source_owner,&graph,profile).unwrap();let selection=L::derive_native_v1(frame,&source_owner,&graph,profile).unwrap();
    assert!(A::derive_current_v1(&b.state,frame,&source_owner,&graph,profile,&join,F::Ordinary{output_and_stop:&output,selection:&selection},T::NotApplicable).is_err(),"actual Tokenizer stage cannot be omitted");
    let aggregate=A::derive_current_v1(&b.state,frame,&source_owner,&graph,profile,&join,F::Ordinary{output_and_stop:&output,selection:&selection},T::Required(&family)).unwrap();
    assert_eq!(aggregate.shape_v1().tokenizer_v1(),Applicable::Required);
    assert!(aggregate.unresolved_supply_v1().any(|s|matches!(s,crate::palw_court_shared_tir_builder_store_v1::original_aggregate::UnresolvedSupplyContractV1::Tokenizer(_))));
    assert!(aggregate.reservation_readiness_v1().is_err());
}
