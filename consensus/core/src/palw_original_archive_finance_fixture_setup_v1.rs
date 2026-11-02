// The driver selected archive_finance before any source execution or quote.
// This transforms producer construction material before its ONE actual fold;
// an old accepted Claim is never swapped for a financially different source.
{
if archive_finance {
    use original_archive_reservation_proposal_v1 as finance;
    use crate::palw_court_original_escrow_v1::{FundingConsentV1,COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1};
    assert!(production&&scoped&&!constraint&&!producer_material_only);
    assert!(!active.original_archive_data_observer_v16);
    // Observation is solely a read-only quote basis. The eventual candidate
    // uses the recognized signed profile and normal actual-full-fold basis.
    let mut quote_extras=active.clone();quote_extras.original_archive_data_observer_v16=true;
    let mut read=TransitionBuilder::try_new(&family_state,&family_params,false,false,false,false,&quote_extras).unwrap().with_native_original_parent_v19(&family_state);
    read.claim_local_input_basis_v12=Some(NativeFoldInputBasisV12::from_native_fold_v12(&ctx,std::slice::from_ref(&object),PalwBlockWorkV3::None,&[],Hash64::default()).unwrap());
    let prepared=prepare_paired_original_v1(&read,&ctx,&object,Some(&fp.commitment)).unwrap().unwrap();
    let quote=quote_prepared_original_final_archive_v11(&read,&ctx,claim,prepared,|view|if archive_legacy_geometry{finance::quote_minimum_original_archive_legacy_oar2_v3(&read,&ctx,view)}else if archive_mixed{finance::quote_minimum_original_archive_at_native_v2(&read,&ctx,view)}else{finance::quote_minimum_original_archive_at_native_v1(&read,&ctx,view)}).unwrap();
    let rent=quote.total_declared_rent_v1().unwrap();let funding=quote.signed_funding_amount_v1().unwrap();
    let quote_fn=if archive_legacy_geometry{palw_original_archive_native_quote_legacy_oar2_v3}else if archive_mixed{palw_original_archive_native_quote_v2}else{palw_original_archive_native_quote_v1};
    let public=quote_fn(&family_state,&family_params,&ctx,&active,&object,
        PalwOriginalArchivePayerBudgetV1{additional_carrier_rent_sompi:rent-envelope.declared_rent_sompi,dedicated_archive_capital_sompi:quote.capital_v1()}).unwrap();
    assert_eq!(public.profiles_v1(),quote.profiles_v1());assert_eq!(public.declared_rent_sompi_v1(),rent);
    assert_eq!(public.funding_output_amount_sompi_v1(),funding);assert_eq!(public.input_archive_capacity_bytes_v1(),quote.input_capacity_v1());
    assert!(rent>envelope.declared_rent_sompi&&quote.capital_v1()>0);
    let mut literal=active.tir_public_originals[&claim].clone();literal.declared_rent_sompi=public.declared_rent_sompi_v1();
    let mut terms=actual.envelope().funding.terms.clone();public.bind_existing_funding_terms_v1(&mut terms).unwrap();
    let payer=mldsa::generate_key_pair([0x7a;32]);
    let signature=mldsa::sign(&payer.signing_key,terms.id_v1().as_byte_slice(),COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1,[0x7b;32]).unwrap().as_ref().to_vec();
    let inner=crate::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1::new_v1(literal.encode_v1().unwrap(),FundingConsentV1{terms,signature},payer.verification_key.as_ref().to_vec()).unwrap();
    let wrapped=crate::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2{version:wrapper.version,funded_original:inner.encode_v1().unwrap(),suppliers:consents.clone(),semantic_table:wrapper.semantic_table.clone()};
    let wire=wrapped.encode_v2().unwrap();let mut carrier=actual.carrier().clone();carrier.outputs[inner.funding.terms.funding_output_index as usize].value=funding;
    let mut normal=active.clone();normal.tir_public_originals.insert(claim,literal.clone());
    carrier.payload=if let Some(prefix)=normal.prefix_dependencies_v1.get_mut(&claim){prefix.inner=wire.clone();prefix.encode_v1().unwrap()}else{wire.clone()};carrier.finalize();
    let paired=palw_funded_original_pairing_payload_v1(&carrier,&wire).unwrap();normal.tir_funded_originals.insert(claim,paired.clone());
    assert!(!normal.original_archive_data_observer_v16);
    assert_eq!(paired.suppliers_v2(),actual.suppliers_v2(),"no supplier duty or fee was changed to buy archive storage");
    (normal,paired,carrier,wire,wrapped,literal,Some(quote))
}else{(active,actual,tx,wire,wrapper,envelope,None)}
}
