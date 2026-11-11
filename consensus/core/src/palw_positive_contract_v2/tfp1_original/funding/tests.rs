use super::*;
use crate::{
    palw_court_original_escrow_v1::{COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1, FundingTermsV1},
    palw_court_service_v1::{ServiceCost, ServiceReservations},
    palw_court_shared_material_intake_v1::batch::plain_output::projection::PlainBatchOriginalProjectionV2,
    palw_native_resource_v1::NativeRetainedV1,
    palw_positive_contract_v2::{PublicOriginalAdmissionV1, PublicOriginalReservationV1},
    palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1,
    tx::{
        PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction,
    },
};
use misaka_palw_positive_contract::{public_one_honest::SignedPublicOriginalV1, rows::NativeHash64};

/// Native booking/companion and payer signatures are exercised together.
/// FP validation, accepted UTXOs and profile prices are fixtures, not public
/// payment or a complete protected original admission.
#[allow(clippy::too_many_arguments)]
pub(crate) fn exercise_funding_v1(
    fixture: &PalwTirPublicFrameFixtureV1,
    source: &PalwTirPublicSourceV1,
    admission: &PublicOriginalAdmissionV1<'_>,
    reservation: &PublicOriginalReservationV1<'_>,
    companion: &OriginalCommit,
    terms: &ProtectedBatchPreclaimTermsV1,
    booking: &ProtectedBatchBookingV1,
    policy: Tfp1FundingPolicyV1,
    projection: &PlainBatchOriginalProjectionV2,
    empty_book: &ServiceReservations,
    maximum_packets: usize,
    sign_companion: impl Fn(&OriginalCommit) -> Vec<u8>,
) {
    let frame = &fixture.frame;
    let fp = frame.original_v1();
    let quotes = booking.quotes_v1();
    let derive = |policy| {
        Tfp1FundingRequirementsV1::derive_v1(fp, source, companion, terms, reservation.budget, reservation.usage.fee, booking, policy)
    };
    let mut unsupported = fp.clone();
    unsupported.job.version = u16::MAX;
    assert!(matches!(
        Tfp1FundingRequirementsV1::derive_v1(
            &unsupported,
            source,
            companion,
            terms,
            reservation.budget,
            reservation.usage.fee,
            booking,
            policy
        ),
        Err(Tfp1FundingErrorV1::Funding(FundingRefusalV1::Scope))
    ));
    let required = derive(policy).unwrap();
    let node_storage: u64 = quotes.iter().map(|q| q.numerical_fault_storage_rent_sompi).sum();
    let carrier_storage: u64 = quotes.iter().map(|q| q.carrier_storage_rent_sompi).sum();
    assert_eq!(required.original_fee_sompi_v1(), policy.original_fee_sompi + node_storage);
    assert_eq!(required.future_carrier_fee_sompi_v1(), carrier_storage);
    assert_eq!(required.scope_v1().required_amount_sompi, policy.original_fee_sompi + node_storage + policy.capital_floor_sompi);
    assert!(derive(Tfp1FundingPolicyV1 { original_fee_sompi: policy.original_fee_sompi + 1, ..policy }).is_err());

    let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    let public = keypair.verification_key.as_ref().to_vec();
    let a = reservation.origin.accepted_daa;
    let at = PalwBlockContextV2 {
        block: Hash64::from_bytes(reservation.origin.accepting_block.0),
        daa_score: a,
        blue_score: reservation.origin.accepted_blue_score,
        subsidy: 0,
    };
    let expected = required.scope_v1();
    let funding_terms = FundingTermsV1 {
        version: 1,
        network_domain: fp.job.network_domain,
        claim: expected.claim,
        job_identity: expected.job_identity,
        execution_root: expected.execution_root,
        source_id: expected.source_id,
        relative_graph_id: expected.relative_graph_id,
        fee_profile_id: expected.fee_profile_id,
        capital_profile_id: expected.capital_profile_id,
        retention_profile_id: expected.retention_profile_id,
        settlement_policy_id: expected.settlement_policy_id,
        accept_from_daa: a,
        accept_until_daa: a,
        material_until_daa: expected.required_material_until_daa,
        funding_output_index: 1,
        funding_amount_sompi: expected.required_amount_sompi,
        payer_key_id: crate::mldsa87_primitives::mldsa87_key_id(&public),
        refund_payload: kaspa_hashes::blake2b_512_address_payload(&public),
        nonce: Hash64::from_u64_word(930),
    };
    let sign_funding = |terms: FundingTermsV1| {
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keypair.signing_key,
            terms.id_v1().as_byte_slice(),
            COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1,
            [93; 32],
        )
        .unwrap();
        crate::palw_court_original_escrow_v1::FundingConsentV1 { terms, signature: signature.as_ref().to_vec() }
    };
    let signed: [u8; SignedPublicOriginalV1::WIRE_BYTES] = sign_companion(companion).try_into().unwrap();
    let make = |funding: crate::palw_court_original_escrow_v1::FundingConsentV1, attach: bool| {
        let mut envelope = crate::palw_tir_funded_original_envelope_v1::funded_original_transport_fixture_v1();
        let payload = crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            commitment: fp.clone(),
            prompt_token_ids: fixture.execution.prompt.clone(),
            signature: vec![7; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        envelope.original_envelope = crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1::new_v1(
            borsh::to_vec(&payload).unwrap(),
            frame.header_v1().clone(),
            frame.custody_v1().pledge_v1().clone(),
            frame.plan_v1().terms.plan.rent_sompi,
        )
        .unwrap()
        .encode_v1()
        .unwrap();
        envelope.payer_pubkey = public.clone();
        envelope.funding = funding;
        let body = crate::palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3::from_parts_v3(
            &envelope.encode_v1().unwrap(),
            b"fixture prompt",
            &signed,
        )
        .unwrap()
        .encode_v3()
        .unwrap();
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(931), 0), vec![], 0, 0)],
            vec![
                TransactionOutput::new(1, ScriptPublicKey::default()),
                TransactionOutput::new(
                    envelope.funding.terms.funding_amount_sompi,
                    crate::mldsa87_primitives::p2pkh_mldsa87_spk(&envelope.funding.terms.refund_payload.as_bytes()),
                ),
            ],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            body,
        );
        let mut pairing = crate::palw_funded_original_coordinator_v1::palw_funded_original_pairing_v1(&tx).unwrap();
        let origin = misaka_palw_positive_contract::rows::AcceptedOrigin {
            carrier_transaction: NativeHash64(tx.id().as_bytes()),
            ..reservation.origin
        };
        let actual_reservation = PublicOriginalReservationV1 { origin, ..*reservation };
        let prepared = admission.prepare_tfp1_pairing_v1(&pairing, terms.obligation_v1(), &actual_reservation).unwrap();
        if attach {
            let input = UtxoEntry::new(tx.outputs.iter().map(|o| o.value).sum::<u64>() + 20, ScriptPublicKey::default(), 1, false);
            let validated = ValidatedTransaction::new(PopulatedTransaction::new(&tx, vec![input]), 20);
            let seed = crate::palw_tfp1_original_history_v1::ValidatedTfp1OriginalInputsV1::prepare_v1(&validated).unwrap().unwrap();
            pairing.bind_history_inputs_v1(seed, origin, Hash64::from_u64_word(932)).unwrap();
        }
        (pairing, prepared)
    };
    // Rebook on the actual signed candidate, then verify funding before
    // committing any reservation. Local price/UTXO inputs remain fixtures.
    let atomic =
        |pairing: &PalwFundedOriginalPairingV1, prepared: &PreparedTfp1PublicOriginalV1, expected: Option<FundingRefusalV1>| {
            let mut book = NativeRetainedV1::retain_existing_v1(empty_book.clone()).unwrap();
            let result = prepared.reserve_and_prepare_protected_funding_v1(
                fp,
                source,
                companion,
                projection,
                terms,
                pairing,
                &at,
                &mut book,
                maximum_packets,
                |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 }),
                policy,
            );
            match expected {
                Some(expected) => {
                    match result {
                        Err(admission::Tfp1ProtectedFundingAdmissionErrorV1::Funding(actual)) => assert_eq!(actual, expected),
                        _ => panic!("expected funding refusal after signed candidate booking"),
                    }
                    assert_eq!(&*book, empty_book, "failed payer check must not leave reservations");
                }
                None => {
                    let (accepted_booking, funded) = result.unwrap();
                    assert_eq!(accepted_booking.quotes_v1(), booking.quotes_v1());
                    assert_eq!(accepted_booking.graph_v1(), booking.graph_v1());
                    assert_eq!(funded.actual().outpoint(), pairing.outpoint());
                    assert_ne!(&*book, empty_book);
                    let before = (*book).clone();
                    let refused = prepared.reserve_and_prepare_protected_funding_v1(
                        fp,
                        source,
                        companion,
                        projection,
                        terms,
                        pairing,
                        &at,
                        &mut book,
                        maximum_packets,
                        |_, bytes| Ok(ServiceCost { bytes, work: 1, io: 1, controls: 1 }),
                        policy,
                    );
                    assert!(matches!(refused, Err(admission::Tfp1ProtectedFundingAdmissionErrorV1::Batch(_))));
                    assert_eq!(&*book, &before, "duplicate booking must not modify the accepted candidate");
                }
            }
        };
    let (pairing, prepared) = make(sign_funding(funding_terms.clone()), true);
    atomic(&pairing, &prepared, None);
    let funded = required.prepare_pairing_v1(&prepared, &pairing, &at).unwrap();
    assert_eq!(funded.actual().outpoint(), pairing.outpoint());
    assert_eq!(funded.actual().output().value, expected.required_amount_sompi);
    assert_eq!(funded.accepted_daa(), a);
    let (without_inputs, prepared_without_inputs) = make(sign_funding(funding_terms.clone()), false);
    assert!(matches!(
        required.prepare_pairing_v1(&prepared_without_inputs, &without_inputs, &at),
        Err(Tfp1FundingErrorV1::Funding(FundingRefusalV1::Scope))
    ));
    atomic(&without_inputs, &prepared_without_inputs, Some(FundingRefusalV1::Scope));
    let wrong_point = PalwBlockContextV2 { block: Hash64::from_u64_word(933), ..at };
    assert!(required.prepare_pairing_v1(&prepared, &pairing, &wrong_point).is_err());
    for wrong_point in [PalwBlockContextV2 { daa_score: a + 1, ..at }, PalwBlockContextV2 { blue_score: at.blue_score + 1, ..at }] {
        assert!(matches!(
            required.prepare_pairing_v1(&prepared, &pairing, &wrong_point),
            Err(Tfp1FundingErrorV1::Funding(FundingRefusalV1::Scope))
        ));
    }
    let wrong = derive(Tfp1FundingPolicyV1 { other_relative_graph: Hash64::from_u64_word(934), ..policy }).unwrap();
    assert!(matches!(wrong.prepare_pairing_v1(&prepared, &pairing, &at), Err(Tfp1FundingErrorV1::Funding(FundingRefusalV1::Scope))));
    let short = FundingTermsV1 { funding_amount_sompi: expected.required_amount_sompi - 1, ..funding_terms.clone() };
    let (short_pairing, short_prepared) = make(sign_funding(short), true);
    assert!(matches!(
        required.prepare_pairing_v1(&short_prepared, &short_pairing, &at),
        Err(Tfp1FundingErrorV1::Funding(FundingRefusalV1::Coverage))
    ));
    atomic(&short_pairing, &short_prepared, Some(FundingRefusalV1::Coverage));
    let short_horizon = FundingTermsV1 { material_until_daa: expected.required_material_until_daa - 1, ..funding_terms.clone() };
    let (short_pairing, short_prepared) = make(sign_funding(short_horizon), true);
    assert!(matches!(
        required.prepare_pairing_v1(&short_prepared, &short_pairing, &at),
        Err(Tfp1FundingErrorV1::Funding(FundingRefusalV1::Coverage))
    ));
    atomic(&short_pairing, &short_prepared, Some(FundingRefusalV1::Coverage));
    let mut bad = sign_funding(funding_terms);
    bad.signature[0] ^= 1;
    let (bad_pairing, bad_prepared) = make(bad, true);
    atomic(&bad_pairing, &bad_prepared, Some(FundingRefusalV1::Signature));
    assert!(matches!(
        required.prepare_pairing_v1(&bad_prepared, &bad_pairing, &at),
        Err(Tfp1FundingErrorV1::Funding(FundingRefusalV1::Signature))
    ));
}
