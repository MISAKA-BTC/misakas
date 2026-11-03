// Clock/financial journal component regression. Producer Ready and owner Open
// use the existing verified-control fixture helper, not an assertion that a
// network has independently acquired material or selected verifier approval.
{
    let frame=crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&accepted,claim).unwrap().clone();
    let p=frame.plan_v1();
    assert_eq!(p.acquisition.clock.version,crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_MATERIAL_V4);
    assert_eq!(p.acquisition.config.reorg_retention_daa,crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA);
    let mut legacy_rules=fx.rules;
    legacy_rules.clock.version=crate::palw_tir_frame_cost_v1::PALW_TIR_FRAME_CLOCK_FAMILY_VERSION_V3;
    assert!(matches!(crate::palw_tir_public_allocation_v1::derive_tir_public_allocation_native_v2(
        &crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1{class_id:class,producer,
            execution_root:fp.commitment.execution_root,job_identity:envelope.pledge.job_identity,accepted_daa:ctx.daa_score},
        &fp.commitment,frame.header_v1(),&fx.record,&fx.layout,family_semantics.as_ref(),family_eof.as_ref(),legacy_rules),
        Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))),"legacy native allocator never interprets clock3");
    if prefix {
        let context=crate::palw_staged_court_state_v1::palw_tir_prefix_context_v1(&fx.record,&fx.layout,frame.header_v1(),&fp.commitment.job,frame.descriptor_v1().maximum_leaves).unwrap();
        let actual=crate::palw_tir_prefix_owned_v1::tir_prefix_resource_plan_v1(context.template.total_v1(),context.leaf_bytes,context.depth,
            p.acquisition.maximum_custodians.try_into().unwrap(),p.acquisition.config.prices).unwrap();
        let stage=p.semantic.as_ref().unwrap().stages.iter().find(|s|s.family==crate::palw_staged_bundle_v1::PalwStagedFamilyV1::PublicPrefix).unwrap();
        assert_eq!(stage.terms.plan,actual,"native TIR prefix prices its real slots and authenticated leaves; unreachable legacy PublicPrefix recovery is absent");
        assert!(p.maximum_semantic_history_records>0,"signed legacy metadata capacity is preserved without an invented completed history");
    }
    let ready=producer_ready(&frame,&accepted,&family_params,&active);
    let offset=p.preparation_until_daa-p.terms.accepted_daa;
    let opened=open(&frame,&ready,&family_params,&active,offset,outsider()).unwrap();
    let key=owner_key(&frame,outsider(),p.terms.accepted_daa+offset).unwrap();
    let before_row=owner_row(&opened,&frame,outsider(),p.terms.accepted_daa+offset);
    let grace=p.acquisition.owner_idle_grace_family_v3().unwrap();
    assert_eq!(before_row.owner_idle_grace_daa_v1(),grace);
    let tenure=crate::palw_tir_public_claim_v1::palw_tir_owner_tenure_v1(&frame,grace);
    assert!(tenure>=p.acquisition.envelope.inclusion_daa+p.acquisition.config.prices.inclusion_margin_daa,
        "active owner gets its full native localization plus final collection, separately from idle timeout");
    assert!(grace<p.acquisition.config.reorg_retention_daa,"new owner response timing stays separate from long evidence retention");
    let last=before_row.idle_since_v1();
    assert!(!before_row.owner_idle_at_v1(last+grace-1));
    assert!(before_row.owner_idle_at_v1(last+grace));
    let point=PalwBlockContextV2{block:h(0x43c10c),daa_score:last+grace,blue_score:last+grace,subsidy:0};
    let (at_idle,_)=apply_palw_transition_v2_with_extras(&opened,&family_params,&point,&[],None,false,false,false,false,&active).unwrap();
    let collateral=at_idle.bond(&outsider()).unwrap().collateral;
    let mut eviction=TransitionBuilder::try_new(&at_idle,&family_params,false,false,false,false,&active).unwrap();
    let saved=eviction.try_checkpoint().unwrap();
    reclaim_owner_slot_v1(&mut eviction,&point,key,outsider()).unwrap();
    let after=owner_row(&eviction.state,&frame,outsider(),p.terms.accepted_daa+offset);
    assert!(!after.holds_owner_slot_v1());
    assert!(eviction.state.bond(&outsider()).unwrap().collateral<collateral,"idle owner forfeit follows the actual financial branch");
    let closed=eviction.state.clone();
    let encoded=borsh::to_vec(&PalwStateCarriageV2::from_state(&closed)).unwrap();
    let restored=borsh::from_slice::<PalwStateCarriageV2>(&encoded).unwrap().into_state(&family_params,Some(closed.state_root())).unwrap();
    assert_eq!(restored.state_root(),closed.state_root());
    eviction.restore(saved);
    assert_eq!(eviction.state.state_root(),at_idle.state_root(),"reorg journal restores slot, balances and reservations together");
    assert!(owner_row(&eviction.state,&frame,outsider(),p.terms.accepted_daa+offset).holds_owner_slot_v1());
    let dormant=family_params.clone().with_family_supplier_from(None);
    assert!(PalwStateCarriageV2::from_state(&accepted).into_state(&dormant,None).is_err(),"clock3 cannot become a historical frame when the new cohort fence is absent");
}
