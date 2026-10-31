//! Reserved176 parsing defenses do not install an event decoder or native authority.
use kaspa_consensus_core::{
    Hash64,
    config::params::ForkActivation,
    palw_lifecycle_objects_v2::{
        PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleKindFencesV1, PalwLifecycleTxError,
        palw_lifecycle_objects_from_accepted_txs_in_context_v2, palw_lifecycle_objects_from_accepted_txs_v2,
        validate_palw_lifecycle_tx, validate_palw_lifecycle_tx_in_context_v1,
    },
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{ScriptPublicKey, Transaction, TransactionOutput},
};
use misaka_palw_positive_contract::native::LIFECYCLE_OBJECT_TAG;

fn payload(body: &[u8]) -> Vec<u8> {
    let mut bytes = PALW_LIFECYCLE_TX_VERSION_V2.to_le_bytes().to_vec();
    bytes.push(LIFECYCLE_OBJECT_TAG);
    bytes.extend_from_slice(body);
    bytes
}
fn expire_body() -> Vec<u8> {
    let mut body = vec![2, 0, 1, 0, 6];
    body.extend_from_slice(&[1; 128]); // fixed CaseId, ResultId; no peer clock/status
    body
}
fn fences() -> PalwLifecycleKindFencesV1 {
    PalwLifecycleKindFencesV1::default().with_positive_court_reserved_v2(Some(ForkActivation::new(100)))
}
fn tx(bytes: Vec<u8>, outputs: Vec<TransactionOutput>) -> Transaction {
    Transaction::new(0, vec![], outputs, 0, SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, bytes)
}

#[test]
fn active_positive_predecode_rejects_malformed_before_generic_borsh() {
    let good = payload(&expire_body());
    let mut trailing = good.clone();
    trailing.push(0);
    let mut huge_vec = vec![2, 0, 1, 0, 2];
    huge_vec.extend_from_slice(&u32::MAX.to_le_bytes());
    let malformed = [payload(&[]), payload(&[2, 0, 1, 0, 255]), payload(&huge_vec), good[..good.len() - 1].to_vec(), trailing];
    for bytes in malformed {
        assert!(matches!(validate_palw_lifecycle_tx_in_context_v1(&bytes, &fences(), 100), Err(PalwLifecycleTxError::Undecodable)));
        let extracted = palw_lifecycle_objects_from_accepted_txs_in_context_v2(&[tx(bytes, vec![])], &fences(), 100);
        assert!(extracted.objects.is_empty());
        assert_eq!(extracted.skipped[0].1, "payload does not decode");
    }
    let mut unsupported = payload(&huge_vec);
    unsupported[..2].copy_from_slice(&99u16.to_le_bytes());
    assert!(matches!(
        validate_palw_lifecycle_tx_in_context_v1(&unsupported, &fences(), 100),
        Err(PalwLifecycleTxError::UnsupportedVersion { got: 99, .. })
    ));
    // Bounded syntax alone still cannot create a decoded global176 event.
    assert!(palw_lifecycle_objects_from_accepted_txs_in_context_v2(&[tx(good, vec![])], &fences(), 100).objects.is_empty());
}

#[test]
fn below_fence_and_context_free_positive_remain_opaque() {
    // Both the outer version and inner body would fail the active preflight.
    let bytes = vec![99, 0, LIFECYCLE_OBJECT_TAG, 255];
    assert!(validate_palw_lifecycle_tx_in_context_v1(&bytes, &fences(), 99).is_ok());
    assert!(validate_palw_lifecycle_tx(&bytes, true).is_ok());
    assert!(matches!(validate_palw_lifecycle_tx(&bytes, false), Err(PalwLifecycleTxError::Undecodable)));
    let carrier = tx(bytes, vec![]);
    for extracted in [
        palw_lifecycle_objects_from_accepted_txs_in_context_v2(std::slice::from_ref(&carrier), &fences(), 99),
        palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&carrier)),
    ] {
        assert!(extracted.objects.is_empty());
        assert_eq!(extracted.skipped[0].1, "positive2 is undecodable below its owning fence");
    }
}

#[test]
fn positive_payload_never_binds_unrelated_sinks_or_registration() {
    use kaspa_consensus_core::{
        palw_activation_pool_v1 as activation, palw_improve_pool_v1 as improve, palw_model_market_v1 as market,
    };
    let id = Hash64::from_bytes([7; 64]);
    let mut huge = vec![2, 0, 1, 0, 2];
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    let payee: ScriptPublicKey = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&[3; 64]);
    for (sink, check, expected) in [
        (
            activation::palw_activation_sink_spk_v1(&id),
            activation::palw_activation_sink_binding_refusal_v1 as fn(&Transaction) -> _,
            "an activation sink's carrier carries no ActivationPoolFunded",
        ),
        (
            improve::palw_improvement_sink_spk_v1(&id),
            improve::palw_improvement_sink_binding_refusal_v1 as fn(&Transaction) -> _,
            "an improvement sink's carrier carries no ImprovementPoolFunded",
        ),
        (
            market::palw_model_sink_spk_v1(&id),
            market::palw_model_sink_binding_refusal_v1 as fn(&Transaction) -> _,
            "a model sink's carrier carries no ModelBuy or ModelSeed",
        ),
    ] {
        let carrier = tx(payload(&huge), vec![TransactionOutput::new(1, sink), TransactionOutput::new(1, payee.clone())]);
        assert_eq!(check(&carrier), Some((0, expected)));
        assert!(kaspa_consensus_core::palw_model_registration_v1::palw_registration_carrier_object_v1(&carrier).is_none());
        assert!(kaspa_consensus_core::palw_state_v2::palw_model_carrier_payout_rows_v1(&carrier).is_none());
    }
}
