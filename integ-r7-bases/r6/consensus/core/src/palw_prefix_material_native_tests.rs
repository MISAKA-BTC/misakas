// Private read-only material replay on actual version11 re-executed original.
// Existing world is verified-half acceptance setup, not funded original success.
fn prefix_material_world(fx:&PalwTirPublicFrameFixtureV1)->(PalwChainStateV2,Hash64,Hash64) {
    use crate::palw_court_shared_tir_state_v1 as shared;
    let frame=&fx.frame;let p=frame.plan_v1();let claim=p.original_claim;
    let (mut state,params,mut extras)=world(fx,frame);
    // Recreate the exact immutable metadata used to prepare this real frame;
    // TIR prefix state itself remains the existing structural relation.
    let semantics=crate::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(
        frame.original_v1().job.class_id,frame.header_v1().artifact_root,&frame.header_v1().job_context,0);
    state.class_semantics.insert(semantics.class_id,semantics.clone());
    state.claim_semantics.insert(claim,semantics.clone());
    assert!(crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(&state,frame,p.terms.material_until_daa).is_err());
    let producer=frame.custody_v1().pledge_v1().producer;
    let key=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81;32]);
    state.bonds.get_mut(&producer).unwrap().pubkey=key.verification_key.as_ref().to_vec();
    let mut pledge=crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1{version:1,claim,producer,
        execution_root:frame.original_v1().execution_root,
        job_identity:crate::palw_fp_execution_v3::palw_fp_job_pin_v1(frame.original_v1()),
        cu_ruleset_id:semantics.cu_ruleset_id,resource_plan_id:p.semantic_resource_plan_id.unwrap(),
        maximum_records:p.maximum_semantic_history_records,maximum_custodians:p.acquisition.maximum_custodians,
        accept_until_daa:p.terms.accepted_daa+1,material_until_daa:p.terms.material_until_daa,signature:vec![]};
    pledge.signature=libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key,pledge.pledge_id_v1().as_byte_slice(),
        crate::palw_history_custody_v1::PALW_HISTORY_PLEDGE_CONTEXT_V1,[82;32]).unwrap().as_ref().to_vec();
    extras.staged_public_court=Some(fx.rules.config);
    extras.history_custody_pledges.insert(claim,pledge);
    let mut dependency=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
    let bundle=frame_semantic_bundle_v1(&dependency,frame,p.terms.accepted_daa).unwrap().unwrap();
    dependency.write_staged_row_v1(Key::Claim(claim),Some(bundle)).unwrap();
    let state=bind_clock_service(&dependency.state,frame,&params,&extras);
    crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(&state,frame,p.terms.material_until_daa).unwrap();
    let source=frame.source_v1(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
    let profile=shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
    let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();let scope=set.scope_id().unwrap();
    let c=shared::SharedTirContextV1{version:1,claim,allocation_id:p.allocation_id,source_id:p.source_id,rules_id:p.rules_id,
        profile_id:profile,scope_id:scope,queries:set.len(),original_accepted_daa:p.terms.accepted_daa,
        hard_daa:p.terms.accepted_daa+10_000,material_until_daa:p.terms.material_until_daa};
    let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
    b.write_staged_row_v1(Key::CourtSharedContext(claim),Some(Row::CourtSharedContext(Box::new(c)))).unwrap();
    (b.state.clone(),claim,scope)
}
#[test]
fn private_prefix_material_replay_authenticates_canonical_boundary_and_refuses_missing_other_source_or_corruption() {
    use crate::palw_court_shared_tir_builder_store_v1::{SharedTirBuilderStoreV1 as Store,prefix_terminal::Budget};
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    use crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1 as V;
    for fault in [false,true] {
        let fx=crate::palw_tir_public_claim_v1::palw_tir_public_prefix_history_frame_fixture_v1(fault);
        let frame=&fx.frame;let (state,claim,scope)=prefix_material_world(&fx);let before=state.state_root();
        let source=frame.source_v1(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
        let units:Vec<_>=(0..fx.execution.binding.step_leaf_count).map(|index|source.authenticate_v1(&Q::StepLeaf{index},
            &R::StepLeaf{preimage:fx.execution.step_leaf(index).unwrap(),opening:fx.execution.step_opening(index).unwrap()},None).unwrap()).collect();
        let store=Store::new(&state,claim,scope,&units,&[],None).unwrap();
        // Test capacity comes from the SAME checked actual-metadata quote as
        // replay; it is not a protocol cap or a new native enforced quota.
        let prefix=crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(&fx.record,&fx.layout,frame.header_v1(),
            &frame.original_v1().job,fx.rules.maximum_leaves).unwrap();
        let initial=crate::palw_court_shared_tir_builder_store_v1::prefix_terminal::prelude_requests(frame,&fx.record,&fx.layout).unwrap();
        let additional=crate::palw_court_shared_tir_builder_store_v1::prefix_terminal::replay_requests(prefix.leaf_bytes).unwrap();
        let required=initial.checked_add(additional).unwrap();
        let budget=Budget{max_leaves:source.leaf_count_v1(),max_requests:required};
        let shape=crate::palw_native_resource_v1::standalone_tir_program_shape_v1(&fx.record.program).unwrap();
        let source_requests=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(&fx.record,&fx.layout,frame.header_v1()).unwrap();
        let step_requests=crate::palw_tir_step_v1::native_tir_step_space_additional_requests_v1(&shape).unwrap();
        println!("prefix local test actual quote: initial={initial} replay={additional} total={required} source={source_requests} step_space_additional={step_requests} structural_items={}",shape.structural_items_v1());
        let replay=store.replay_prefix_material_v1(&state,claim,budget).unwrap();
        assert!(replay.leaves()>0);
        assert_eq!(matches!(replay.verdict(),V::StateRootMismatch{..}),fault);
        let named=crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(&frame.original_v1().job).unwrap();
        let canonical=prefix.space.prefix_state_leaf_v1(&prefix.job_context,named.prefix_tokens,0).unwrap();
        let missing=canonical.index;
        // Local borrowed preflight regression, not a forged authenticated Unit
        // or a full malicious-claim admission case. Primitive is not called.
        let expected=canonical.value_count as usize*4;
        let long=vec![0;expected+4];
        assert!(matches!(crate::palw_court_shared_tir_builder_store_v1::prefix_terminal::check_prefix_body_length_v1(
            &canonical,prefix.leaf_bytes,&long),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))));
        let short=vec![0;expected-1];
        assert!(crate::palw_court_shared_tir_builder_store_v1::prefix_terminal::check_prefix_body_length_v1(&canonical,prefix.leaf_bytes,&short).is_err());
        assert!(crate::palw_court_shared_tir_builder_store_v1::prefix_terminal::check_prefix_body_length_v1(&canonical,
            canonical.value_count as u64*4-1,&fx.execution.step_leaf(missing).unwrap().values_le).is_err());
        let sparse:Vec<_>=units.iter().filter(|u|u.request_v1()!=(Q::StepLeaf{index:missing})).cloned().collect();
        let no=Store::new(&state,claim,scope,&sparse,&[],None).unwrap();
        assert!(matches!(no.replay_prefix_material_v1(&state,claim,budget),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(s)) if s.contains("missing authenticated boundary")));
        let mut bad=fx.execution.step_leaf(missing).unwrap();bad.values_le[0]^=1;
        assert!(source.authenticate_v1(&Q::StepLeaf{index:missing},&R::StepLeaf{preimage:bad,opening:fx.execution.step_opening(missing).unwrap()},None).is_err());
        let other=crate::palw_tir_public_claim_v1::palw_tir_public_prefix_history_frame_fixture_v1(!fault);
        let (other_state,other_claim,_)=prefix_material_world(&other);
        assert!(matches!(store.replay_prefix_material_v1(&other_state,other_claim,budget),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(s)) if s.contains("another original source")));
        // Real local native checker, no global quota mode or invented reserve flag.
        let checker=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
            crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:required,database_bytes:0});
        let first=std::cell::Cell::new(0);
        let held=store.replay_prefix_reserved_v1(&state,claim,budget,|bytes|{first.set(bytes);checker.reserve_requests_v1(bytes,0).map(Some)}).unwrap();
        let total=checker.outstanding().0;assert_eq!(first.get(),initial);assert_eq!(total,required);assert!(total>first.get());drop(held);assert_eq!(checker.outstanding(),(0,0));
        for limit in [first.get()-1,total-1,total] {
            let q=crate::palw_native_resource_v1::NativeDemandCheckerV1::new(
                crate::palw_native_resource_v1::NativeSnapshotResourceLimitsV1{memory_bytes:limit,database_bytes:0});
            let result=store.replay_prefix_reserved_v1(&state,claim,budget,|bytes|q.reserve_requests_v1(bytes,0).map(Some));
            if limit==total {let own=result.unwrap();assert_eq!(own.verdict(),replay.verdict());assert_eq!(q.outstanding(),(total,0));drop(own);}
            else {assert!(matches!(result,Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));}
            assert_eq!(q.outstanding(),(0,0));
        }
        assert!(matches!(store.replay_prefix_material_v1(&state,claim,Budget{max_leaves:0,..budget}),Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(_))));
        assert_eq!(state.state_root(),before);
    }
}
