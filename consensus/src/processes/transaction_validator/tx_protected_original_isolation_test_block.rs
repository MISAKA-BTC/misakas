// Stateless/header boundary only. Fixed-width signature/material placeholders
// deliberately provide no admission, Panel approval or settlement evidence.
{
    use kaspa_consensus_core::{
        mldsa87_primitives::{MLDSA87_PUBKEY_LEN, MLDSA87_SIGNATURE_LEN, mldsa87_key_id},
        palw_court_original_escrow_v1::{FundingConsentV1, FundingTermsV1},
        palw_fp_answer_accounting_v1::{palw_fp_accounting_original_bytes_v1, palw_fp_answer_envelope_encode_v1},
        palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1,
        palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3,
        palw_tir_protected_fp_envelope_v2::PalwTirProtectedFpEnvelopeV2,
        palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1,
    };
    use misaka_palw_positive_contract::public_one_honest::{ManifestV1, SignedPublicOriginalV1};
    let tfp = PalwTirPublicFpEnvelopeV1::decode_v1(&funded.payload).unwrap();
    let fp = tfp.original_fp_v1().unwrap();
    let h = kaspa_hashes::Hash64::from_u64_word;
    let key = vec![0; MLDSA87_PUBKEY_LEN];
    let source = kaspa_consensus_core::palw_tir_public_source_v1::public_source_header_id_v1(&tfp.header).unwrap();
    let terms = FundingTermsV1 {
        version: 1,
        network_domain: fp.commitment.job.network_domain,
        claim: fp.claim_id(),
        job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment),
        execution_root: fp.commitment.execution_root,
        source_id: source,
        relative_graph_id: h(2),
        fee_profile_id: h(3),
        capital_profile_id: h(4),
        retention_profile_id: h(5),
        settlement_policy_id: h(6),
        accept_from_daa: 1500,
        accept_until_daa: 2000,
        material_until_daa: 4000,
        funding_output_index: 0,
        funding_amount_sompi: 1,
        payer_key_id: mldsa87_key_id(&key),
        refund_payload: h(7),
        nonce: h(8),
    };
    let original = PalwTirProtectedFpEnvelopeV2::new_v1(tfp.original_payload.clone(), tfp.header.clone()).unwrap();
    let base = PalwTirFundedOriginalEnvelopeV1::new_protected_v2(
        original.clone(),
        FundingConsentV1 { terms, signature: vec![0; MLDSA87_SIGNATURE_LEN] },
        key,
    )
    .unwrap();
    let inner = base.encode_protected_inner_v2().unwrap();
    let companion = [1; SignedPublicOriginalV1::WIRE_BYTES];
    let manifest = [1; ManifestV1::WIRE_BYTES as usize];
    let own = validator(PqEnforcementMode::Disabled)
        .with_fp_decode_rules_fence(Some(ForkActivation::new(1500)))
        .with_tir_public_conviction_fence(Some(ForkActivation::new(1000)))
        .with_protected_original_fence(Some(ForkActivation::new(1600)))
        .with_fp_answer_accounting_fence(Some(ForkActivation::new(1700)));
    let wires = [
        PalwTirFundedOriginalEnvelopeV3::from_parts_protected_v3(&inner, b"a", &companion, &manifest, b"d")
            .unwrap()
            .encode_v3()
            .unwrap(),
        PalwTirFundedOriginalEnvelopeV3::from_parts_relative_v4(&inner, b"a", &companion, &manifest, b"d")
            .unwrap()
            .encode_v3()
            .unwrap(),
        PalwTirFundedOriginalEnvelopeV3::from_parts_segmented_v5(&inner, b"PPS1\nt:1:a", &companion, &manifest, b"d")
            .unwrap()
            .encode_v3()
            .unwrap(),
    ];
    for wire in wires {
        let mut candidate = funded.clone();
        candidate.payload = wire.clone();
        candidate.finalize();
        let id = candidate.id();
        assert!(new_lane.validate_tx_in_isolation(&candidate).is_err(), "public TIR alone cannot enable protected admission");
        assert!(own.validate_tx_in_isolation(&candidate).is_ok());
        for (protected, tir) in [(None, Some(1000)), (Some(u64::MAX), Some(1000)), (Some(1600), None)] {
            let missing = validator(PqEnforcementMode::Disabled)
                .with_fp_decode_rules_fence(Some(ForkActivation::new(1500)))
                .with_tir_public_conviction_fence(tir.map(ForkActivation::new))
                .with_protected_original_fence(protected.map(ForkActivation::new));
            assert!(missing.validate_tx_in_isolation(&candidate).is_err(), "all declared transport gates remain required");
        }
        assert!(header(&own, &candidate, 1599).is_err());
        assert!(header(&own, &candidate, 1600).is_ok());
        assert_eq!(own.palw_fp_door_payload_v1(&candidate, Some(1600)).unwrap().as_ref(), tfp.original_payload.as_slice());
        assert_eq!(candidate.id(), id);
        assert_eq!(candidate.payload, wire, "inspection preserves the actual signed carrier");

        let mut answered = candidate.clone();
        answered.payload = palw_fp_answer_envelope_encode_v1(&wire, &[7, 8]);
        answered.finalize();
        assert!(own.validate_tx_in_isolation(&answered).is_ok());
        assert!(header(&own, &answered, 1699).is_err(), "outer accounting has its own clock");
        assert!(header(&own, &answered, 1700).is_ok());

        let original_bytes = palw_fp_accounting_original_bytes_v1(&wire).unwrap();
        let offset = original_bytes.as_ptr() as usize - wire.as_ptr() as usize;
        let mut invalid_fp = wire.clone();
        invalid_fp[offset..offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        let mut unknown_version = wire.clone();
        unknown_version[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
        for invalid in [wire[..wire.len() - 1].to_vec(), unknown_version, invalid_fp, inner.clone(), original.encode_v1().unwrap()] {
            candidate.payload = invalid;
            candidate.finalize();
            assert!(own.validate_tx_in_isolation(&candidate).is_err(), "no malformed or stripped fallback");
        }
    }
}
