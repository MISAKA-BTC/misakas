//! Actual transaction crypto, signing-template DATA only. Native registration,
//! Full scripts, source/P entitlement and payment require separate tests.
use kaspa_consensus_core::{
    Hash64, mldsa87_primitives::p2pkh_mldsa87_spk, palw_positive_relay_signature_v2::*, subnets::SUBNETWORK_ID_PALW_LIFECYCLE, tx::*,
};
use libcrux_ml_dsa::ml_dsa_87::portable as mldsa;
use misaka_palw_positive_contract::{assignment::AssignmentId, identity::Digest};

fn assignment() -> AssignmentId {
    AssignmentId(Digest([3; 64]))
}
fn spend(key: &[u8; 2592]) -> (Transaction, UtxoEntry) {
    let script = p2pkh_mldsa87_spk(&kaspa_hashes::blake2b_512_address_payload(key).as_bytes());
    (
        Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(8), 2), vec![], 0, 1)],
            vec![TransactionOutput::new(500, script.clone())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            assignment_relay_payload_v2(assignment()).to_vec(),
        ),
        UtxoEntry::new(1000, script, 42, false),
    )
}
#[test]
fn literal_template_matches_existing_bounded_op12_codec() {
    use kaspa_consensus_core::palw_positive_court_v2::coordinator::assignment_body::*;
    let raw = assignment_relay_payload_v2(assignment());
    assert_eq!(raw.len(), 77);
    assert_eq!(&raw[..8], &[1, 0, 176, 2, 0, 1, 0, 12]);
    let body: PositiveAssignmentNativeBodyV2 = borsh::from_slice(&raw[3..]).unwrap();
    assert_eq!(
        body.operation,
        PositiveAssignmentNativeOperationV2::SettleAssignment { assignment: assignment(), case: None, receipts: vec![] }
    );
    assert_eq!(borsh::to_vec(&body).unwrap(), raw[3..]);
}
#[test]
fn real_signature_allows_outputs_and_foreign_inputs_without_new_payer_signature() {
    let keys = mldsa::generate_key_pair([11; 32]);
    let key = *keys.verification_key.as_ref();
    let (mut tx, entry) = spend(&key);
    let template = FundingRelaySignatureTemplateV2::new(assignment(), tx.inputs[0].previous_outpoint, entry.amount, key).unwrap();
    let sig = template.sign(&keys.signing_key, [19; 32]).unwrap();
    template.verify_spend_signature(&tx, &entry, &sig).unwrap();
    tx.outputs[0].value = 123;
    tx.outputs.push(TransactionOutput::new(876, entry.script_public_key.clone()));
    tx.inputs.push(TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(9), 0), vec![5], 123, 1));
    tx.inputs[0].signature_script = template.unlock_script(&sig).unwrap();
    tx.finalize();
    template.verify_spend_signature(&tx, &entry, &sig).unwrap();
    let script = &tx.inputs[0].signature_script;
    assert_eq!(&script[..3], &[0x4d, 0x14, 0x12]);
    assert_eq!(script[3 + 4627], 0x82);
    assert_eq!(&script[3 + 4628..3 + 4628 + 3], &[0x4d, 0x20, 0x0a]);
    let other = mldsa::generate_key_pair([12; 32]);
    assert_eq!(template.sign(&other.signing_key, [19; 32]), Err(RelaySignatureErrorV2::Signature));
}
#[test]
fn every_current_input_and_fixed_template_field_remains_bound() {
    let keys = mldsa::generate_key_pair([13; 32]);
    let key = *keys.verification_key.as_ref();
    let (tx, entry) = spend(&key);
    let template = FundingRelaySignatureTemplateV2::new(assignment(), tx.inputs[0].previous_outpoint, entry.amount, key).unwrap();
    let sig = template.sign(&keys.signing_key, [20; 32]).unwrap();
    for alter in 0..9 {
        let mut changed = tx.clone();
        let mut changed_entry = entry.clone();
        match alter {
            0 => changed.version = 1,
            1 => changed.lock_time = 1,
            2 => changed.gas = 1,
            3 => changed.payload[10] ^= 1,
            4 => changed.inputs[0].sequence = 1,
            5 => changed.inputs[0].sig_op_count = 2,
            6 => changed.inputs[0].previous_outpoint.index += 1,
            7 => changed_entry.amount += 1,
            _ => changed_entry.script_public_key = ScriptPublicKey::default(),
        }
        changed.finalize();
        assert!(template.verify_spend_signature(&changed, &changed_entry, &sig).is_err(), "field {alter}");
    }
    let mut damaged = sig;
    damaged[30] ^= 1;
    assert_eq!(template.verify(&damaged), Err(RelaySignatureErrorV2::Signature));
    let mut metadata = entry;
    metadata.block_daa_score += 1;
    metadata.is_coinbase = true;
    // Actual maturity must still be checked by Full; neither field is signed.
    template.verify_spend_signature(&tx, &metadata, &sig).unwrap();
}

#[test]
fn fixed_template_recipe_matches_independent_borsh_vector() {
    assert_eq!(
        funding_relay_template_id_v2().0,
        [
            150, 199, 145, 200, 230, 254, 4, 222, 138, 220, 47, 18, 109, 29, 123, 0, 176, 253, 94, 24, 124, 164, 88, 22, 138, 161,
            240, 41, 235, 53, 203, 56, 215, 69, 67, 158, 218, 40, 100, 37, 246, 150, 156, 238, 168, 229, 138, 185, 21, 98, 248, 11,
            53, 13, 211, 69, 30, 57, 158, 206, 114, 44, 142, 172
        ]
    );
}
