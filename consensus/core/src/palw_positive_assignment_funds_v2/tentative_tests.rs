//! Money/signature fixtures only. Private test dues are NOT a native accepted
//! service/P session and never exercise a placeholder positive admission.
use super::*;
use crate::{
    hashing::{
        sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash},
        sighash_type::SIG_HASH_ALL,
    },
    tx::{PopulatedTransaction, TransactionInput, TransactionOutput},
};
use misaka_palw_positive_contract::{
    assignment::{AssignmentFundingAuthorizationV2, AssignmentId, rows::AssignmentSettlementLegV2},
    identity::{SIGNATURE_BYTES, principal},
    rows::{AcceptedOrigin, NativeHash64},
};

fn origin(transaction: Hash64, daa: u64) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64(Hash64::from_u64_word(daa).as_bytes()),
        carrier_block: NativeHash64(Hash64::from_u64_word(daa + 1000).as_bytes()),
        carrier_transaction: NativeHash64(transaction.as_bytes()),
        accepted_tx_ordinal: 1,
        event_ordinal: 0,
        accepted_daa: daa,
        accepted_blue_score: daa,
    }
}
pub(crate) fn fixture(
    key: &[u8; PUBKEY_BYTES],
) -> (AssignmentFundingRow, Transaction, Vec<UtxoEntry>, AssignmentSpendAuthorizationV2) {
    let payer = principal(key);
    let refund = Digest(kaspa_hashes::blake2b_512_address_payload(key).as_bytes());
    let spk = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&refund.0);
    let original = point(TransactionOutpoint::new(Hash64::from_u64_word(77), 0));
    let funding_origin = origin(Hash64::from_u64_word(77), 10);
    let material = AcceptedMaterial { origin: funding_origin, digest: Digest([3; 64]) };
    let fresh = |n| BalanceLeg { initial: n, paid: 0, refunded: 0, burned: 0, remaining: n };
    let authorization = AssignmentFundingAuthorizationV2 {
        assignment: AssignmentId(Digest([4; 64])),
        quote: Digest([5; 64]),
        payer,
        funding_output_index: 0,
        refund_payload: refund,
        max_fee: 20,
        max_capital: 30,
        max_reward: 100,
        max_locked: 175,
        admission_expiry: 100,
    };
    let row = AssignmentFundingRow {
        assignment: authorization.assignment,
        authorization,
        payer_key: *key,
        payer_signature: [0; SIGNATURE_BYTES],
        funding_origin,
        original_outpoint: original,
        current_outpoint: Some(original),
        initial_locked: 175,
        worker: fresh(100),
        fee: fresh(20),
        capital: fresh(30),
        surplus: fresh(25),
        worker_reserve: 100,
        earned_service_due: 5,
        liability_floor: 30,
        reservation: material,
        liability: material,
        last_settlement: None,
    };
    let due = AssignmentAuthorizedDueV2 {
        source_leg: 1,
        kind: AssignmentSettlementLegV2::ServiceFee,
        party: principal(&[19; PUBKEY_BYTES]),
        recipient: Digest(kaspa_hashes::blake2b_512_address_payload(&[19; PUBKEY_BYTES]).as_bytes()),
        amount: 5,
        receipt: material,
    };
    let body = crate::palw_positive_court_v2::coordinator::assignment_body::PositiveAssignmentNativeBodyV2 {
        version: 2,
        revision: 1,
        operation:
            crate::palw_positive_court_v2::coordinator::assignment_body::PositiveAssignmentNativeOperationV2::SettleAssignment {
                assignment: row.assignment,
                case: None,
                receipts: vec![material],
            },
    };
    let mut payload = vec![1, 0, 176];
    payload.extend_from_slice(&borsh::to_vec(&body).unwrap());
    let auth = AssignmentSpendAuthorizationV2 {
        before: row.clone(),
        dues: vec![due.clone()],
        protected_after: [100, 0, 30, 0],
        daa: 20,
        max_frame_bytes: 16000,
        max_history_frames: 100,
        payload: payload.clone(),
    };
    let transaction = Transaction::new(
        0,
        vec![
            TransactionInput::new(native_assignment_outpoint_v2(original), vec![], 0, 1),
            TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(88), 0), vec![], 0, 1),
        ],
        vec![
            TransactionOutput::new(5, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&due.recipient.0)),
            TransactionOutput::new(170, spk.clone()),
        ],
        0,
        crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    let entries = vec![UtxoEntry::new(175, spk.clone(), 10, false), UtxoEntry::new(10, spk, 10, false)];
    (row, transaction, entries, auth)
}

#[test]
fn payer_signed_partial_op12_projects_without_inventing_acceptance() {
    use libcrux_ml_dsa::ml_dsa_87::portable as mldsa;
    let keypair = mldsa::generate_key_pair([17; 32]);
    let key: &[u8; PUBKEY_BYTES] = keypair.verification_key.as_ref();
    let (row, mut transaction, entries, auth) = fixture(key);
    let context = b"kaspa-pq-v2/tx/mldsa87";
    for index in 0..transaction.inputs.len() {
        let hash = calc_mldsa87_signature_hash(
            &PopulatedTransaction::new(&transaction, entries.clone()),
            index,
            SIG_HASH_ALL,
            &Mldsa87SigHashReusedValuesUnsync::new(),
        );
        let signature = mldsa::sign(&keypair.signing_key, hash.as_byte_slice(), context, [index as u8; 32]).unwrap();
        mldsa::verify(&keypair.verification_key, hash.as_byte_slice(), context, &signature).unwrap();
        // Canonical two PUSHDATA2 items: signature+hash type, then native key.
        let mut script = vec![0x4d];
        script.extend_from_slice(&((SIGNATURE_BYTES + 1) as u16).to_le_bytes());
        script.extend_from_slice(signature.as_ref());
        script.push(SIG_HASH_ALL.to_u8());
        script.push(0x4d);
        script.extend_from_slice(&(PUBKEY_BYTES as u16).to_le_bytes());
        script.extend_from_slice(key);
        transaction.inputs[index].signature_script = script;
    }
    assert!(transaction.id_is_current());
    let plan = qualify_assignment_spend_v2(&row, &auth, &transaction, &[175, 10], 20).unwrap();
    assert_eq!((plan.paid_now, plan.continued_now, plan.burned_now), (5, 170, 0));
    assert_eq!((plan.new.worker.remaining, plan.new.fee.remaining, plan.new.capital.remaining), (100, 15, 30));
    assert_eq!(row.last_settlement, None, "the tentative plan did not mutate an accepted row");
    use misaka_palw_positive_contract::assignment::{
        control_material::{ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2, encode_assignment_control_v2},
        journal::assignment_frame_v2,
    };
    let boundary = [9; ASSIGNMENT_CONTROL_BOUNDARY_BYTES_V2];
    let inner = &transaction.payload[3..];
    let cap = auth.max_frame_bytes;
    let frame_for = |payload: &[u8]| {
        assignment_frame_v2(
            row.assignment,
            1,
            Digest([8; 64]),
            AssignmentJournalEventKindV2::Settlement,
            origin(transaction.id(), 20),
            payload,
            cap,
        )
        .unwrap()
    };
    let payload = encode_assignment_control_v2(&boundary, inner, cap).unwrap();
    let frame = frame_for(&payload);
    assert_ne!(frame.payload.digest, plan.body_digest);
    let rooted = plan.accepted_funding_row_v2(&frame, &payload).unwrap();
    assert_eq!(rooted.last_settlement, Some(frame.payload));
    assert!(qualify_assignment_spend_v2(&rooted, &auth, &transaction, &[170, 10], 20).is_err(), "stale before row cannot spend twice");
    let mut wrong = frame;
    wrong.payload.origin.carrier_transaction = NativeHash64([9; 64]);
    assert!(plan.accepted_funding_row_v2(&wrong, &payload).is_err());
    assert!(plan.accepted_funding_row_v2(&frame_for(inner), inner).is_err());
    let mut wrong_inner = inner.to_vec();
    wrong_inner[0] ^= 1;
    let wrong_payload = encode_assignment_control_v2(&boundary, &wrong_inner, cap).unwrap();
    assert!(plan.accepted_funding_row_v2(&frame_for(&wrong_payload), &wrong_payload).is_err());
    let mut capped = auth.clone();
    capped.max_frame_bytes = payload.len() as u64 - 1;
    assert!(qualify_assignment_spend_v2(&row, &capped, &transaction, &[175, 10], 20).is_err());
    capped.max_frame_bytes += 1;
    assert!(qualify_assignment_spend_v2(&row, &capped, &transaction, &[175, 10], 20).is_ok());
}

#[test]
fn tentative_plan_rejects_changed_payload_duplicate_dues_and_unsafe_debit() {
    let (row, transaction, _, auth) = fixture(&[17; PUBKEY_BYTES]);
    let mut changed = transaction.clone();
    changed.payload.push(0);
    changed.finalize();
    assert!(qualify_assignment_spend_v2(&row, &auth, &changed, &[175, 10], 20).is_err());
    let mut double = auth.clone();
    double.dues.push(double.dues[0].clone());
    assert!(qualify_assignment_spend_v2(&row, &double, &transaction, &[175, 10], 20).is_err());
    let mut worker = auth.clone();
    worker.dues[0].source_leg = 0;
    assert!(
        qualify_assignment_spend_v2(&row, &worker, &transaction, &[175, 10], 20).is_err(),
        "service cannot debit worker success reserve"
    );
    assert!(qualify_assignment_spend_v2(&row, &auth, &transaction, &[174, 10], 20).is_err());
    assert!(qualify_assignment_spend_v2(&row, &auth, &transaction, &[175, 10], 21).is_err());
}

#[path = "../../../../misaka/palw/positive-contract/tests/support/assignment.rs"]
mod assignment_fixture;

#[test]
fn archived_assignment_coin_rejects_a_conserving_but_different_initial_split() {
    let (mut funding, _, entries, _) = fixture(&[17; PUBKEY_BYTES]);
    let money = AssignmentMoneyQuoteV2 {
        fee: 20,
        capital: 30,
        usage: AssignmentPhysicalUsageV2 {
            admission_bytes: 1,
            retained_bytes: 1,
            replay_bytes: 2,
            heap_bytes: 1,
            utxo_writes: 1,
            signatures: 3,
        },
        retention_daa: 100,
        fee_schedule: assignment_fixture::d(71),
        reserve_schedule: assignment_fixture::d(72),
        physical_profile: assignment_fixture::d(73),
    };
    let mut bundle = assignment_fixture::bundle();
    bundle.terms.payer = funding.authorization.payer;
    bundle.terms.refund_payload = funding.authorization.refund_payload;
    bundle.quote = money.id();
    bundle.funding.assignment = bundle.terms.id();
    bundle.funding.quote = bundle.quote;
    bundle.funding.payer = bundle.terms.payer;
    bundle.funding.refund_payload = bundle.terms.refund_payload;
    bundle.future_case.assignment = bundle.terms.id();
    bundle.future_case.payer = bundle.terms.payer;
    bundle.future_case.refund_payload = bundle.terms.refund_payload;
    let tx = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(90), 0), vec![], 0, 1)],
        vec![TransactionOutput::new(funding.initial_locked, entries[0].script_public_key.clone())],
        0,
        crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        vec![1, 0, 176, 2, 0, 1, 0, 8],
    );
    funding.assignment = bundle.terms.id();
    funding.authorization = bundle.funding;
    funding.funding_origin = origin(tx.id(), 10);
    funding.reservation.origin = funding.funding_origin;
    funding.liability.origin = funding.funding_origin;
    funding.original_outpoint = point(TransactionOutpoint::new(tx.id(), 0));
    funding.current_outpoint = Some(funding.original_outpoint);
    assert_eq!(check_archived_assignment_funding_v2(&bundle, &funding, &tx, &money), Ok(()));

    funding.fee.initial -= 1;
    funding.fee.remaining -= 1;
    funding.surplus.initial += 1;
    funding.surplus.remaining += 1;
    assert!(funding.validate_shape().is_ok());
    assert_eq!(check_archived_assignment_funding_v2(&bundle, &funding, &tx, &money), Err(AssignmentFundsErrorV2::Conservation));
}
