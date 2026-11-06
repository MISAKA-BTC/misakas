// Genuine Prefix candidate: production pinned schedules and both public Court
// fences are in force from the original birth. No cfg(test) schedule injection.
{
    use crate::palw_funded_original_coordinator_v1::*;
    use crate::palw_court_original_escrow_v1::{FundingConsentV1,FundingTermsV1,COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1};
    use crate::tx::{ScriptPublicKey,Transaction,TransactionInput,TransactionOutpoint,TransactionOutput};
    // Already resolved once before quotes, signatures and scoped rule digest.
    let params=params.clone();
    assert_eq!(borsh::to_vec(&params).unwrap(),borsh::to_vec(&family_fixture_native_params_v1(params.clone())).unwrap());
    assert_eq!(params.claim_retirement_daa(),3_000);
    let mut extras=extras.clone();extras.family_supplier_active=true;
    assert!(extras.tir_public_conviction_active&&params.court_outsider_lane_active_v1(ctx.daa_score));
    assert!(extras.funded_original_test_schedule_v1.is_none());
    let schedule=palw_funded_original_schedule_v1(&extras).unwrap();
    let admission=schedule.admission_v1().unwrap();
    let service_profile=crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1;
    assert_eq!(service_profile.budget.controls,1);
    let served=state.clone();
    if source_history.is_none(){assert!(served.court_service_state_v1().is_none(),"new original boots pinned namespace");}
    else{assert!(served.court_service_state_v1().is_some(),"consumer retains actual prior source service reservations");}
    let budgets=(schedule.numerical_budget(),schedule.input_budget(),schedule.source_budget());
    let mut ok=extras.clone();ok.tir_public_originals.insert(claim,envelope.clone());
    let table_bytes=family_table.as_ref().map(crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1);
    let candidate_inputs=vec![TransactionInput::new(TransactionOutpoint::new(h(0xf00d+ctx.blue_score),0),vec![],0,0)];
    let public_quote=palw_family_original_native_quote_v3(&served,&params,&ctx,&ok,&object,table_bytes.as_deref(),&candidate_inputs).unwrap();
    let hard=public_quote.hard_until_v2();
    assert_eq!(hard,ctx.daa_score+crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA);
    assert!(public_quote.original_plan_v2().terms.collect_until_daa<=hard,
        "every native source/semantic/filing door must fit the unchanged original Court hard");
    eprintln!("[family-native-acceptance-clock] birth={} x3Hard={:?} courtClockV2From={:?} nativeHard={} collect={} nativeMaterial={} requiredMaterial={}",
        ctx.daa_score,params.claim_hard_deadline_v1().map(|m|crate::palw_ruling_fences_v1::palw_x3_hard_deadline_at_v1(&m.policy,ctx.daa_score)),
        params.court_clock_v2_from_daa(),hard,public_quote.original_plan_v2().terms.collect_until_daa,
        public_quote.original_plan_v2().terms.material_until_daa,public_quote.material_until_v2());
    assert_eq!(public_quote.inventories_v2().len(),if prefix{4}else{3});
    assert_eq!(public_quote.accepted_daa_v2(),ctx.daa_score);
    let (graph_scope,material_until,required,derivation,_retention_quote_lease)={
        let b=TransitionBuilder::try_new(&served,&params,false,false,false,false,&ok).unwrap();
        let prepared=prepare_paired_original_v1(&b,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
        let req=prepared.enrich_native_geometry_v1(&b,&ctx).unwrap().prepare_requirements_v1(&b,&ctx,
            PreparedRequirementBudgetsV1{numerical:budgets.0,input:budgets.1,source:budgets.2}).unwrap();
        let frame=req.frame_v1();let p=frame.plan_v1();let graph=req.source_graph_v1().unwrap();
        let (derivation,retention_lease)=crate::palw_funded_original_admission_v1::derive_funded_admission_family_v2(claim,p.allocation_id,p.source_id,p.terms.accepted_daa,
            &frame.custody_v1().liable_signers_v1(),graph,&admission).unwrap();
        let retained=std::iter::once(&derivation.retention_packet).chain(&derivation.retention_tail_v2).collect::<Vec<_>>();
        assert!(retained.iter().all(|p|p.cost.bytes<=service_profile.budget.bytes));
        assert_eq!(retained.iter().try_fold(0u64,|n,p|n.checked_add(p.cost.bytes)),derivation.resources.heap_bytes.checked_add(derivation.resources.retention_bytes));
        assert_ne!(derivation.retention_packet.id,crate::palw_funded_original_admission_v1::funded_retention_packet_id_v1(claim,p.allocation_id,p.source_id));
        let required=crate::palw_funded_original_admission_v1::funded_required_amount_v1(schedule.fee_sompi(),schedule.capital_floor_sompi(),derivation.capital_sompi,p.terms.plan.deposit_sompi).unwrap();
        let material=crate::palw_court_service_state_v2::tir_service_required_material_until_v1(frame,hard).unwrap();
        let material=if archive_finance{material.max(original_archive_reservation_proposal_v1::minimum_source_material_until_v19(&b,&ctx,frame).unwrap())}else{material};
        (graph.scope_v3(),material,required,derivation,retention_lease)
    };
    assert_eq!(public_quote.required_original_funding_v2(),required);
    assert_eq!(public_quote.relative_graph_v2(),graph_scope);
    let payer=mldsa::generate_key_pair([0x7a;32]);let payer_pub=payer.verification_key.as_ref().to_vec();
    let refund=kaspa_hashes::blake2b_512_address_payload(&payer_pub);
    let terms=FundingTermsV1{version:1,network_domain:extras.tir_public_network_domain_v1.unwrap(),claim,
        job_identity:envelope.pledge.job_identity,execution_root:fp.commitment.execution_root,source_id:envelope.pledge.source_id,
        relative_graph_id:graph_scope,fee_profile_id:schedule.fee_profile_id(),capital_profile_id:schedule.capital_profile_id(),
        retention_profile_id:schedule.retention_profile_id(),settlement_policy_id:schedule.settlement_policy_id(),
        accept_from_daa:ctx.daa_score,accept_until_daa:ctx.daa_score,material_until_daa:if archive_finance&&prospective.is_some(){material_until.max(prospective.unwrap().trace_retention_daa).checked_add(if prefix{2}else{0}).unwrap()}else{material_until.checked_add(if archive_finance&&prefix{2}else{0}).unwrap()},funding_output_index:1,
        funding_amount_sompi:required,payer_key_id:crate::mldsa87_primitives::mldsa87_key_id(&payer_pub),refund_payload:refund,nonce:h(0x42ee)};
    let signature=mldsa::sign(&payer.signing_key,terms.id_v1().as_byte_slice(),COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1,[0x7b;32]).unwrap().as_ref().to_vec();
    let tfo1=crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1::new_v1(envelope.encode_v1().unwrap(),FundingConsentV1{terms,signature},payer_pub).unwrap();
    let tx=Transaction::new(0,candidate_inputs,
        vec![TransactionOutput::new(1,ScriptPublicKey::default()),TransactionOutput::new(required,crate::mldsa87_primitives::p2pkh_mldsa87_spk(&refund.as_bytes()))],
        0,crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,0,tfo1.encode_v1().unwrap());
    let pairing=palw_funded_original_pairing_v1(&tx).unwrap();ok.tir_funded_originals.insert(claim,pairing.clone());
    include!("palw_court_family_supplier_native_test_block.rs");
}
