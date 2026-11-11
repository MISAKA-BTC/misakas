#[test]
fn protected_carrier_preserves_signed_fp_and_separates_fee_from_funding_output() {
    use kaspa_consensus_core::{
        mldsa87_primitives::{mldsa87_key_id, p2pkh_mldsa87_spk},
        palw_court_original_escrow_v1::{COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1, FundingConsentV1, FundingTermsV1},
        palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3, palw_fp_job_pin_v1},
        palw_funded_original_coordinator_v1::palw_funded_original_pairing_v1,
        palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1,
        palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3,
        palw_tir_protected_fp_envelope_v2::PalwTirProtectedFpEnvelopeV2,
        palw_tir_public_source_v1::public_source_header_id_v1,
        palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, palw_tir_execution_root_v1},
    };
    use std::cell::Cell;
    struct Recording<'a> {
        key: &'a ValidatorKey,
        calls: Cell<usize>,
    }
    impl MessageSigner for Recording<'_> {
        fn public_key(&self) -> &[u8] {
            self.key.public_key()
        }
        fn sign_message(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
            assert_eq!(context, MLDSA87_TX_CONTEXT, "only the final input is signed by this builder");
            self.calls.set(self.calls.get() + 1);
            MessageSigner::sign_message(self.key, message, context)
        }
    }
    let key = compute_key();
    let h = Hash64::from_u64_word;
    let (mut fp, ids) = fp_commitment_fixture(h(1), &key);
    let context = palw_fp_job_context_v3(
        &fp.job,
        &PalwFpClassFactsV3 {
            model_profile_id: h(2),
            runtime_manifest_hash: h(3),
            runtime_class_id: h(4),
            shape_profile_id: fp.job.class_id,
            cu_ruleset_id: h(5),
        },
        &PalwFpRunFactsV3 {
            decode_tokens_executed: fp.decode_tokens_executed,
            stop_reason: fp.stop_reason,
            full_logits_trace_root: fp.trace_root,
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: h(6),
            step_leaf_count: fp.work_leaves,
        },
        b"devnet",
    )
    .unwrap();
    fp.execution_root = palw_tir_execution_root_v1(&context.context_hash(), &fp.trace_root, &fp.job.class_id, fp.work_leaves, &h(6));
    let claim = fp_claim_id_v3(&fp);
    let signed_fp = PalwFpCommitmentTxPayloadV3 {
        version: PALW_FP_V3_VERSION,
        commitment: fp.clone(),
        prompt_token_ids: ids,
        signature: key.sign_with_context(claim.as_bytes().as_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT).to_vec(),
    };
    let literal = borsh::to_vec(&signed_fp).unwrap();
    let header = PalwPublicTirBindingHeaderV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: context,
        artifact_root: h(7),
        full_logits_trace_root: fp.trace_root,
        step_leaf_count: fp.work_leaves,
        step_merkle_root: h(6),
        committed_execution_root: fp.execution_root,
    };
    let payer_script = p2pkh_mldsa87_spk(&blake2b_512_address_payload(key.public_key()).as_bytes());
    let funding = UtxoEntry::new(1_000_000, payer_script, 0, false);
    let recorder = Recording { key: &key, calls: Cell::new(0) };
    for index in [0, 1] {
        let terms = FundingTermsV1 {
            version: 1,
            network_domain: fp.job.network_domain,
            claim,
            job_identity: palw_fp_job_pin_v1(&fp),
            execution_root: fp.execution_root,
            source_id: public_source_header_id_v1(&header).unwrap(),
            relative_graph_id: h(8),
            fee_profile_id: h(9),
            capital_profile_id: h(10),
            retention_profile_id: h(11),
            settlement_policy_id: h(12),
            accept_from_daa: 100,
            accept_until_daa: 200,
            material_until_daa: 20_000,
            funding_output_index: index,
            funding_amount_sompi: 100_000,
            payer_key_id: mldsa87_key_id(key.public_key()),
            refund_payload: blake2b_512_address_payload(key.public_key()),
            nonce: h(13),
        };
        let consent = FundingConsentV1 {
            signature: key
                .sign_with_context(terms.id_v1().as_bytes().as_slice(), COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1)
                .to_vec(),
            terms,
        };
        let envelope = PalwTirFundedOriginalEnvelopeV1::new_protected_v2(
            PalwTirProtectedFpEnvelopeV2::new_v1(literal.clone(), header.clone()).unwrap(),
            consent,
            key.public_key().to_vec(),
        )
        .unwrap();
        let inner = envelope.encode_protected_inner_v2().unwrap();
        // Opaque metadata here exercises only carrier construction. It is
        // deliberately not an accepted companion or authenticated manifest.
        let payload = PalwTirFundedOriginalEnvelopeV3::from_parts_protected_v3(
            &inner,
            b"fixture prompt",
            &std::array::from_fn(|_| 0),
            &std::array::from_fn(|_| 0),
            &[1],
        )
        .unwrap()
        .encode_v3()
        .unwrap();
        let tx = build_protected_original_carrier_v3_with(&recorder, payload.clone(), fop(9, 0), &funding, SF_FEE).unwrap();
        assert_eq!(tx.payload, payload);
        assert_eq!(tx.outputs[index as usize].value, 100_000);
        assert_eq!(funding.amount - tx.outputs.iter().map(|output| output.value).sum::<u64>(), SF_FEE);
        let paired = palw_funded_original_pairing_v1(&tx).unwrap();
        assert_eq!(paired.claim(), claim);
        assert_eq!(paired.envelope().protected_original_v2().unwrap().original_payload_v1(), literal);
        assert_eq!(paired.envelope().original_fp_v2().unwrap(), signed_fp);
        assert_eq!(paired.envelope().legacy_declared_rent_sompi_v2().unwrap(), None);
        assert_eq!(paired.outpoint(), TransactionOutpoint::new(tx.id(), index));
        let before = recorder.calls.get();
        assert!(build_protected_original_carrier_v3_with(&recorder, payload, fop(9, 0), &funding, 950_001).is_err());
        assert_eq!(recorder.calls.get(), before, "insufficient funds are rejected before asking for a signature");
    }
    assert_eq!(recorder.calls.get(), 2);
}
