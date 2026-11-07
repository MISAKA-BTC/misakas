// Actual registered/executed Input(0) numerical root material; verified-half
// original/world fixture, not native financial/activation success.
#[test]
fn root_input_material_store_obeys_actual_retained_flat_and_merkle_forms() {
    use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1 as F,prompt_ids_opening_v1,PALW_PROMPT_IDS_TILE_LEN};
    use crate::palw_tir_public_source_v1::{PalwTirPublicSourceV1 as Source,PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
    use crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 as B;
    use crate::palw_tir_court_v1::{PalwTirEvidenceStoreV1,parameter_cover::RootInputBoundaryV1 as I};
    use crate::palw_court_shared_tir_builder_store_v1::{SharedTirBuilderStoreV1 as Store,PromptMaterialV1 as P};
    use crate::palw_court_shared_tir_state_v1 as shared;
    let mut covers:Vec<crate::palw_native_resource_v1::NativeRetainedV1<
        crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1>>=Vec::new();
    for form in [F::Flat,F::MerkleV1] {
        let fx=crate::palw_tir_public_claim_v1::palw_tir_public_input_history_frame_fixture_v1(form);
        let frame=&fx.frame;let p=frame.plan_v1();let claim=p.original_claim;
        let (state,params,extras)=world(&fx,frame);
        let state=bind_clock_service(&state,frame,&params,&extras);
        let source=Source::new_native_v1(&fx.record,&fx.layout,frame.header_v1(),
            frame.original_v1().execution_root,frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
        let profile=shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();
        let scope=set.scope_id().unwrap();
        let context=shared::SharedTirContextV1{version:1,claim,allocation_id:p.allocation_id,source_id:p.source_id,
            rules_id:p.rules_id,profile_id:profile,scope_id:scope,queries:set.len(),original_accepted_daa:p.terms.accepted_daa,
            hard_daa:p.terms.accepted_daa+10_000,material_until_daa:p.terms.material_until_daa};
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        b.write_staged_row_v1(Key::CourtSharedContext(claim),Some(Row::CourtSharedContext(Box::new(context)))).unwrap();
        let before=b.state.state_root();
        assert_eq!(frame.plan_v1().owner_work.rules.prompt_form,form);
        let cover=Source::root_input_cover_native_v1(&source,&fx.record,&fx.layout,B{max_original_leaves:1_000_000,
            max_total_edges:10_000_000,max_total_coordinates:1_000_000,max_root_memberships:1_000_000}).unwrap();
        assert!(cover.roots_v1().iter().any(|r|r.inputs.iter().any(|i|matches!(i,I::PromptPosition(_)))));
        assert!(cover.roots_v1().iter().any(|r|r.inputs.iter().any(|i|matches!(i,I::GeneratedId(_)))));
        let carry=cover.roots_v1().iter().flat_map(|r|r.inputs.iter()).find_map(|i|if let I::CarryStepLeaf(index)=i {Some(*index)}else{None})
            .expect("actual carry dependency");
        let units:Vec<_>=(0..fx.execution.binding.step_leaf_count).map(|index|source.authenticate_v1(&Q::StepLeaf{index},
            &R::StepLeaf{preimage:fx.execution.step_leaf(index).unwrap(),opening:fx.execution.step_opening(index).unwrap()},None).unwrap()).collect();
        let rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context,&fx.execution.rows).unwrap();
        let ids=source.retain_complete_ids_v1(rows,&fx.execution.generated).unwrap();
        let tiles:Vec<_>=(0..fx.execution.prompt.len()).step_by(PALW_PROMPT_IDS_TILE_LEN as usize)
            .map(|pos|prompt_ids_opening_v1(&fx.execution.prompt,pos as u32).unwrap()).collect();
        let material=match form {F::Flat=>P::Flat(&fx.execution.prompt),F::MerkleV1=>P::Merkle(&tiles)};
        let store=Store::new_with_prompt(&b.state,claim,scope,&units,&[],Some(&ids),material).unwrap();
        store.check_root_input_cover_v1(&cover).unwrap();
        if let Some(other)=covers.first() {assert!(store.check_root_input_cover_v1(other).is_err(),"other original source");}
        let absent=Store::new(&b.state,claim,scope,&units,&[],Some(&ids)).unwrap();
        assert!(absent.check_root_input_cover_v1(&cover).unwrap_err().contains("prompt coordinate"));
        let no_ids=Store::new_with_prompt(&b.state,claim,scope,&units,&[],None,material).unwrap();
        assert!(no_ids.check_root_input_cover_v1(&cover).is_err());
        let some_units:Vec<_>=units.iter().filter(|u|u.request_v1()!=(Q::StepLeaf{index:carry})).cloned().collect();
        let no_carry=Store::new_with_prompt(&b.state,claim,scope,&some_units,&[],Some(&ids),material).unwrap();
        assert!(no_carry.check_root_input_cover_v1(&cover).unwrap_err().contains("carry unit"));
        match form {
            F::Flat=>{
                assert!(Store::new_with_prompt(&b.state,claim,scope,&units,&[],Some(&ids),P::Merkle(&tiles)).is_err());
                let mut bad=fx.execution.prompt.clone();bad[0]^=1;
                assert!(Store::new_with_prompt(&b.state,claim,scope,&units,&[],Some(&ids),P::Flat(&bad)).is_err());
            },
            F::MerkleV1=>{
                assert!(Store::new_with_prompt(&b.state,claim,scope,&units,&[],Some(&ids),P::Flat(&fx.execution.prompt)).is_err());
                let empty=Store::new_with_prompt(&b.state,claim,scope,&units,&[],Some(&ids),P::Merkle(&[])).unwrap();
                assert!(empty.check_root_input_cover_v1(&cover).is_err());
                let mut bad=tiles.clone();bad[0].tile_ids[0]^=1;
                assert!(Store::new_with_prompt(&b.state,claim,scope,&units,&[],Some(&ids),P::Merkle(&bad)).is_err());
            },
        }
        assert_eq!(before,b.state.state_root());
        covers.push(cover);
    }
}
