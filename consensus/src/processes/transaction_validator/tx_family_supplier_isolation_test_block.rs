// Door regression only: signatures are shape placeholders here. Actual joint
// authority, native inventory and funding are checked by the production fold.
{
    use kaspa_consensus_core::palw_court_family_supplier_v1::*;
    use kaspa_consensus_core::palw_court_original_escrow_v1::{FundingConsentV1,FundingTermsV1};
    let tfp=kaspa_consensus_core::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1::decode_v1(&funded.payload).unwrap();
    let fp=tfp.original_fp_v1().unwrap();let h=kaspa_hashes::Hash64::from_u64_word;
    let key=vec![0;kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
    let terms=FundingTermsV1{version:1,network_domain:h(1),claim:fp.claim_id(),job_identity:tfp.pledge.job_identity,
        execution_root:fp.commitment.execution_root,source_id:tfp.pledge.source_id,relative_graph_id:h(2),fee_profile_id:h(3),
        capital_profile_id:h(4),retention_profile_id:h(5),settlement_policy_id:h(6),accept_from_daa:1500,accept_until_daa:2000,
        material_until_daa:4000,funding_output_index:0,funding_amount_sompi:1,payer_key_id:kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(&key),refund_payload:h(7),nonce:h(8)};
    let literal=kaspa_consensus_core::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1::new_v1(
        funded.payload.clone(),FundingConsentV1{terms,signature:vec![0;kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]},key).unwrap().encode_v1().unwrap();
    let consent=SignedFamilySupplierV1{terms:FamilySupplierTermsV1{version:1,network:h(1),ruleset:h(2),claim:fp.claim_id(),
        source:tfp.pledge.source_id,relative_graph:h(2),role:FamilySupplierRoleV1::ParameterAndPrompt,inventory:h(3),
        supplier:tfp.pledge.producer,sponsor:tfp.pledge.producer,max_material_bytes:1,accept_from_daa:1500,accept_until_daa:2000,
        respond_until_daa:3000,material_until_daa:4000,fee_profile:h(4),capital_profile:h(5),retention_profile:h(6),settlement_policy:h(7),
        fee_sompi:1,collateral_sompi:1,nonce:h(8)},supplier_signature:vec![0;kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        sponsor_signature:vec![0;kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]};
    let wire=kaspa_consensus_core::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2{version:2,funded_original:literal,
        suppliers:vec![consent],semantic_table:None}.encode_v2().unwrap();
    let mut candidate=funded.clone();candidate.payload=wire;candidate.finalize();
    let own=validator(PqEnforcementMode::Disabled).with_fp_decode_rules_fence(Some(ForkActivation::new(1500)))
        .with_tir_public_conviction_fence(Some(ForkActivation::new(1000)))
        .with_court_family_supplier_fence(Some(ForkActivation::new(1600)));
    let id=candidate.id();let literal=candidate.payload.clone();
    assert!(new_lane.validate_tx_in_isolation(&candidate).is_err(),"old public fence cannot activate the new transport");
    assert!(own.validate_tx_in_isolation(&candidate).is_ok(),"declared new transport reaches its original's ordinary FP isolation checks");
    assert!(header(&own,&candidate,1599).is_err());
    assert!(header(&own,&candidate,1600).is_ok());
    assert_eq!(candidate.id(),id);assert_eq!(candidate.payload,literal,"door views preserve actual signed outer bytes");
}
