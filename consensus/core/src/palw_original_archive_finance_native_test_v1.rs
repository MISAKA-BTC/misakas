// Included after the ONE actual normal new-profile fold. Source history, if
// present, came from its own independently executed/signed new-profile fold.
{
    use original_archive_reservation_proposal_v1 as finance;
    use crate::palw_court_original_escrow_v1::{FundingConsentV1,COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1};
    let quote=archive_finance_quote.as_ref().unwrap();
    assert!(!active.original_archive_data_observer_v16);
    finance::assert_actual_literal_alias_geometry_v3(&accepted,claim,ctx,archive_mixed&&!archive_legacy_geometry).unwrap();
    let receipt=finance::verified_original_archive_reservation_v1(&accepted,claim,ctx).unwrap().unwrap();
    assert!(receipt.constructed_v1()&&!receipt.released_v1());
    assert_eq!(quote.profiles_v1(),[actual.envelope().funding.terms.fee_profile_id,actual.envelope().funding.terms.capital_profile_id,actual.envelope().funding.terms.retention_profile_id,actual.envelope().funding.terms.settlement_policy_id],"the original actual payer signed exactly the selected quote before the one native fold");
    finance::assert_actual_receipt_version_codec_v2(&accepted,claim,ctx).unwrap();
    assert_eq!(receipt.body_bounds_v1(),quote.body_bounds_v1());assert_eq!(receipt.input_capacity_v1(),quote.input_capacity_v1());
    assert_eq!(receipt.contract_v1(),actual.envelope().funding.terms.id_v1());
    assert_eq!(receipt.material_until_v1(),actual.envelope().funding.terms.material_until_daa);
    let escrow=&accepted.funded_escrows_v1()[&receipt.contract_v1()];
    assert_eq!(escrow.version,crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1);
    assert!(finance::archive_capital_pending_v1(&accepted,escrow).unwrap());
    assert_eq!(escrow.fee_sompi,schedule.fee_sompi(),"unchanged original settlement split");
    let f1=crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(ctx.block);
    assert!(!accepted.kernel_route_state_v1().unwrap().aux.contains_key(&(crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1,f1.to_vec())),"verified normal original uses scoped publication before world copies");
    if let Some(parent)=source_history.as_ref(){
        assert!(quote.paid_known_alias_count_v19()>=2,"actual same-class Prefix source shares paid class/layout bodies");
        assert!(finance::assert_actual_ancestor_contingent_holding_v20(&accepted,claim,ctx).unwrap()>0,"real native ancestor funds same-H contingent capacity as well as beyond-H coverage");
        assert!(finance::assert_actual_paid_alias_boundaries_v19(&accepted,claim,ctx).unwrap()>=2);
        let parent_receipt=finance::verified_original_archive_reservation_v1(&parent.state,parent.claim,parent.point).unwrap().unwrap();
        assert_eq!(receipt.material_until_v1(),parent_receipt.material_until_v1()+2,"only child payer archive interval is extended");
        let found=finance::with_verified_ancestor_descriptor_v19(&accepted,parent.claim,parent.point.block,parent.point.daa_score,parent.point.blue_score,ctx.daa_score,|descriptor|{
            assert_eq!(descriptor.point_v19(),parent.point);assert_eq!(descriptor.old_paid_until_v19(),Some(parent_receipt.material_until_v1()));Ok(())
        }).unwrap();assert!(found.is_some(),"actual child owns indexed verified parent lease");
    }
    // Ground the reservation in the actual native generation: canonical aux
    // outer framing, every new retained N/B/I/A entry, and real undo entries.
    // This is a generation upper check only: unrelated permanent Source9
    // Summary/Reverse ownership is not assigned to finite archiveH by it.
    let(mut published,mut journal)=(0u64,0u64);
    for entry in &accepted_delta.entries {
        if let PalwDeltaEntryV2::KernelRouteRow{table,key,old,new}=entry {
            if *table!=crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1||!matches!(key.first().copied(),Some(b'N'|b'B'|b'I'|b'A')){continue;}
            journal=journal.checked_add(borsh::object_length(entry).unwrap() as u64).unwrap();
            for raw in [old.as_ref(),new.as_ref()].into_iter().flatten(){
                let actual=borsh::object_length(&((*table,key.as_slice()),raw.as_slice())).unwrap() as u64;
                assert_eq!(actual,finance::canonical_aux_entry_retained_bytes_v19(key.len() as u64,raw.len() as u64).unwrap());
            }
            if let(None,Some(raw))=(old,new){published=published.checked_add(borsh::object_length(&((*table,key.as_slice()),raw.as_slice())).unwrap() as u64).unwrap();}
        }
    }
    assert!(published>0&&journal>published);
    assert!(published<=quote.geometry_v1().retained,"actual native canonical publication exceeds prepaid upper reservation");
    assert!(journal<=quote.geometry_v1().journal,"actual native undo generation exceeds prepaid upper reservation");
    let cold=PalwStateCarriageV2::from_state(&accepted).into_state(&family_params,Some(accepted.state_root())).unwrap();
    let restored=finance::verified_original_archive_reservation_v1(&cold,claim,ctx).unwrap().unwrap();
    assert_eq!(restored.body_bounds_v1(),receipt.body_bounds_v1());assert_eq!(restored.contract_v1(),receipt.contract_v1());
    let reverted=super::super::revert_delta_v2(&accepted,&accepted_delta,&family_params).unwrap();assert_eq!(reverted.state_root(),family_state.state_root());
    let replay=super::super::apply_delta_v2(&reverted,&accepted_delta,&family_params).unwrap();assert_eq!(replay.state_root(),accepted.state_root());
    if archive_legacy_geometry {
        // Genuine old-tuple replay: rerun the SAME signed object on the exact
        // undone native parent; do not manufacture a legacy receipt after fold.
        let (reexecuted,reexecuted_delta)=apply_palw_transition_v2_with_extras(&reverted,&family_params,&ctx,std::slice::from_ref(&object),None,false,false,false,false,&active).unwrap();
        assert_eq!(reexecuted.state_root(),accepted.state_root());assert_eq!(reexecuted_delta.entries.len(),accepted_delta.entries.len());
        let roundtrip=PalwStateCarriageV2::from_state(&reexecuted).into_state(&family_params,Some(reexecuted.state_root())).unwrap();
        let old=finance::verified_original_archive_reservation_v1(&roundtrip,claim,ctx).unwrap().unwrap();assert!(old.constructed_v1());assert_eq!(old.contract_v1(),actual.envelope().funding.terms.id_v1());
        finance::assert_actual_literal_alias_geometry_v3(&roundtrip,claim,ctx,false).unwrap();
        let undone_again=super::super::revert_delta_v2(&reexecuted,&reexecuted_delta,&family_params).unwrap();assert_eq!(undone_again.state_root(),family_state.state_root());
        println!("legacy_oar2_signed_fullfold_constructed_cold_undo_reexecution=PASS claim={:?} contract={:?}",claim,old.contract_v1());
    }
    let payer=mldsa::generate_key_pair([0x7a;32]);
    for (rent,funding) in [(envelope.declared_rent_sompi-1,actual.envelope().funding.terms.funding_amount_sompi),(envelope.declared_rent_sompi,actual.envelope().funding.terms.funding_amount_sompi-1)] {
        let mut literal=active.tir_public_originals[&claim].clone();literal.declared_rent_sompi=rent;
        let mut terms=actual.envelope().funding.terms.clone();terms.funding_amount_sompi=funding;
        let signature=mldsa::sign(&payer.signing_key,terms.id_v1().as_byte_slice(),COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1,[0x7b;32]).unwrap().as_ref().to_vec();
        let inner=crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1::new_v1(literal.encode_v1().unwrap(),FundingConsentV1{terms,signature},payer.verification_key.as_ref().to_vec()).unwrap();
        let wrapped=crate::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2{version:wrapper.version,funded_original:inner.encode_v1().unwrap(),suppliers:consents.clone(),semantic_table:wrapper.semantic_table.clone()};
        let wire=wrapped.encode_v2().unwrap();let mut carrier=actual.carrier().clone();carrier.outputs[inner.funding.terms.funding_output_index as usize].value=funding;
        let mut short=active.clone();short.tir_public_originals.insert(claim,literal);
        carrier.payload=if let Some(prefix)=short.prefix_dependencies_v1.get_mut(&claim){prefix.inner=wire.clone();prefix.encode_v1().unwrap()}else{wire.clone()};carrier.finalize();
        short.tir_funded_originals.insert(claim,palw_funded_original_pairing_payload_v1(&carrier,&wire).unwrap());
        let mut rejected=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&short).unwrap().with_native_original_parent_v19(&family_state);
        rejected.claim_local_input_basis_v12=Some(NativeFoldInputBasisV12::from_native_fold_v12(&ctx,std::slice::from_ref(&object),PalwBlockWorkV3::None,&[],Hash64::default()).unwrap());
        let before=rejected.state.state_root();assert!(super::super::objects::apply_object(&mut rejected,&ctx,&object).is_err(),"re-signed real payer ceiling shortfall must refuse");
        assert_eq!(rejected.state.state_root(),before);assert!(rejected.entries.is_empty());
    }
    // Core native consensus assertions do not replace actual processor UTXO fee
    // burn. Joint lifetime/transfer assertions use the same actual accepted pair.
    include!("palw_original_archive_lease_native_test_v19.rs");
}
