use misaka_palw_positive_contract::{
    ContractError,
    assignment::{consents::*, journal::*, rows::*, *},
    identity::*,
    messages::AggregateBudget,
    native::*,
    rows::*,
};
#[path = "support/assignment.rs"]
mod support;
use support::*;
fn origin(n: u32) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([21; 64]),
        carrier_block: NativeHash64([22; 64]),
        carrier_transaction: NativeHash64([n as u8; 64]),
        accepted_tx_ordinal: n,
        event_ordinal: 0,
        accepted_daa: 10,
        accepted_blue_score: 11,
    }
}
fn material(n: u8) -> AcceptedMaterial {
    AcceptedMaterial { origin: origin(1), digest: d(n) }
}
fn assignment() -> AssignmentRow {
    let mut bundle = bundle();
    bundle.terms.ruleset = ruleset_id(d(31), d(32));
    bundle.funding.assignment = bundle.terms.id();
    bundle.future_case.assignment = bundle.terms.id();
    let source = AssignmentSourceConsentV2 {
        version: 2,
        revision: 1,
        assignment: bundle.terms.id(),
        worker: bundle.terms.worker,
        source_catalog: bundle.source_catalog,
        source_policy: bundle.terms.source_policy,
        budget: AggregateBudget {
            rounds: 10,
            unique_source_bytes: 10,
            transmitted_bytes: 20,
            additional_work: 10,
            retained_bytes: 10,
            retention_until: 500,
            fee: 20,
            witness_attempts: 2,
        },
        service_before_daa: 450,
    };
    let prover = AssignmentProverConsentV2 {
        version: 2,
        revision: 1,
        assignment: bundle.terms.id(),
        prover: bundle.terms.prover,
        positive_policy: d(32),
        max_attempts: 2,
        max_build_work: 10,
        max_submitted_bytes: 10,
        max_retained_bytes: 10,
        retention_until_daa: 500,
        build_before_daa: 400,
        max_service_fee: 20,
        cancellation_policy: bundle.cancellation_policy,
    };
    bundle.source_consent = source.id();
    bundle.prover_consent = prover.id();
    AssignmentRow {
        bundle,
        source_consent: source,
        prover_consent: prover,
        origin: origin(1),
        worker_key: [0; PUBKEY_BYTES],
        positive_policy: d(32),
        assignment_policy: d(33),
        fired_rules: d(31),
        job: d(34),
        input: d(35),
        context: bundle.job_context,
        registered_metadata: d(36),
        status: AssignmentStatusV2::Open,
        derivation: material(40),
        dependencies: AcceptedMaterial { origin: origin(1), digest: bundle.terms.empty_dependencies_id() },
        reservation: material(41),
        liability: material(42),
        last_event: origin(1),
    }
}
fn leg(n: u64) -> BalanceLeg {
    BalanceLeg { initial: n, paid: 0, refunded: 0, burned: 0, remaining: n }
}
fn funding(a: &AssignmentRow) -> AssignmentFundingRow {
    let mut authorization = a.bundle.funding;
    authorization.payer = principal(&[0; PUBKEY_BYTES]);
    AssignmentFundingRow {
        assignment: a.id(),
        authorization,
        payer_key: [0; PUBKEY_BYTES],
        payer_signature: [0; SIGNATURE_BYTES],
        funding_origin: a.origin,
        original_outpoint: NativeOutpoint { transaction: a.origin.carrier_transaction, index: authorization.funding_output_index },
        current_outpoint: Some(NativeOutpoint {
            transaction: a.origin.carrier_transaction,
            index: authorization.funding_output_index,
        }),
        initial_locked: 160,
        worker: leg(100),
        fee: leg(20),
        capital: leg(30),
        surplus: leg(10),
        worker_reserve: 100,
        earned_service_due: 20,
        liability_floor: 30,
        reservation: a.reservation,
        liability: a.liability,
        last_settlement: None,
    }
}
#[test]
fn appended_rows_preserve_discriminants_and_fixed_codec_ceiling() {
    let a = assignment();
    a.validate_shape().unwrap();
    let f = assignment_frame_v2(a.id(), 0, d(0), AssignmentJournalEventKindV2::Open, a.origin, &[2, 0, 1, 0, 8], 1024).unwrap();
    let h = f.append_to(None, 500).unwrap();
    let rows = [
        NativeRowBody::Assignment(a.clone()),
        NativeRowBody::AssignmentNonce(AssignmentNonceRow {
            poster: a.bundle.terms.poster,
            nonce: AssignmentNonceV2 { next: 1, exhausted: false },
        }),
        NativeRowBody::AssignmentUse(AssignmentUseRow {
            assignment: a.id(),
            case: CaseId(d(60)),
            result: ResultId(d(61)),
            origin: a.origin,
            binding: material(62),
        }),
        NativeRowBody::AssignmentFunding(funding(&a)),
        NativeRowBody::AssignmentSettlement(AssignmentSettlementRow {
            assignment: a.id(),
            funding: AssignmentSettlementFundingV2::Poster,
            leg: AssignmentSettlementLegV2::ServiceFee,
            party: PrincipalId(d(70)),
            ordinal: 0,
            debit: bond(71),
            recipient: d(72),
            amount: 1,
            receipt: material(73),
            once: material(74),
            origin: a.origin,
        }),
        NativeRowBody::AssignmentRole(AssignmentRoleRow {
            assignment: a.id(),
            role: AssignmentRoleV2::Poster,
            bond: a.bundle.terms.poster,
            key: [0; PUBKEY_BYTES],
            signature: [0; SIGNATURE_BYTES],
            bundle: a.bundle.id(),
            authorization: AssignmentRoleAuthorizationV2::Initial,
            admission: material(75),
            funding: None,
            lease: material(76),
            origin: a.origin,
            retired: false,
        }),
        NativeRowBody::AssignmentJournalHead(h),
        NativeRowBody::AssignmentJournalFrame(f),
    ];
    for (i, body) in rows.into_iter().enumerate() {
        let value = NativeRowValue::new(body).unwrap();
        let key = value.body.key().encode();
        let raw = value.encode().unwrap();
        assert_eq!(key[4], 10 + i as u8);
        assert_eq!(raw[4], 10 + i as u8);
        assert!(key.len() <= MAX_NATIVE_KEY_BYTES);
        assert!(raw.len() <= MAX_NATIVE_ROW_BYTES);
        assert_eq!(NativeRowValue::decode(&key, &raw).unwrap(), value);
        let mut wrong = key;
        wrong[4] = 9;
        assert!(NativeRowValue::decode(&wrong, &raw).is_err());
    }
    assert_eq!(
        NativeRowKey::AssignmentSettlement {
            assignment: a.id(),
            leg: AssignmentSettlementLegV2::Refund,
            party: PrincipalId(d(80)),
            ordinal: 0
        }
        .encode()
        .len(),
        142
    );
    assert_eq!(NativeRowKey::AssignmentRole { assignment: a.id(), role: AssignmentRoleV2::Poster, bond: bond(1) }.encode().len(), 138);
    assert!(
        NativeRowValue::new(NativeRowBody::AssignmentNonce(AssignmentNonceRow {
            poster: bond(1),
            nonce: AssignmentNonceV2 { next: 1, exhausted: true }
        }))
        .is_err()
    );
}
#[test]
fn journal_binds_literal_bytes_continuous_clock_and_exact_previous_head() {
    let id = AssignmentId(d(1));
    let f = assignment_frame_v2(id, 0, d(0), AssignmentJournalEventKindV2::Open, origin(1), &[2, 0, 1, 0, 8], 1024).unwrap();
    assert_eq!(
        assignment_frame_v2(id, 0, d(0), AssignmentJournalEventKindV2::Open, origin(1), &[2, 0, 1, 0, 8], 4),
        Err(ContractError::Budget)
    );
    let h = f.append_to(None, 500).unwrap();
    let next = assignment_frame_v2(id, 1, f.header.id(), AssignmentJournalEventKindV2::JoinProver, origin(2), &[2, 0, 1, 0, 13], 1024)
        .unwrap();
    assert!(next.append_to(Some(&h), 500).is_ok());
    assert!(next.append_to(None, 500).is_err());
    let mut stale = h;
    stale.last_frame = d(8);
    assert!(next.append_to(Some(&stale), 500).is_err());
    let mut aba = h;
    aba.next_ordinal = 2;
    assert!(next.append_to(Some(&aba), 500).is_err());
    assert!(
        assignment_frame_v2(id, 1, f.header.id(), AssignmentJournalEventKindV2::MetadataReceipt, origin(2), &[2, 0, 1, 0, 3], 1024)
            .is_err()
    );
    let metadata = AssignmentMetadataReceiptV2 {
        version: 2,
        revision: 1,
        assignment: id,
        purpose: AssignmentMetadataPurposeV2::Admission,
        provider: PrincipalId(d(3)),
        bytes: 1,
        work: 1,
        fee: 1,
        schedule: d(4),
    };
    assert!(
        assignment_frame_v2(
            id,
            1,
            f.header.id(),
            AssignmentJournalEventKindV2::MetadataReceipt,
            origin(2),
            &borsh::to_vec(&metadata).unwrap(),
            1024
        )
        .is_ok()
    );
    let raw = encode_assignment_journal_record_v2(f.header, &[2, 0, 1, 0, 8], 1024).unwrap();
    assert_eq!(raw.len(), 430);
    let view = AssignmentJournalRecordViewV2::decode(&raw, 1024).unwrap();
    assert_eq!(view.frame(), f);
    let mut bad = raw.clone();
    *bad.last_mut().unwrap() = 9;
    assert!(AssignmentJournalRecordViewV2::decode(&bad, 1024).is_err());
    let mut trailing = raw.clone();
    trailing.push(0);
    assert!(AssignmentJournalRecordViewV2::decode(&trailing, 1024).is_err());
    assert!(AssignmentJournalRecordViewV2::decode(&raw, 429).is_err());
    assert!(assignment_frame_v2(id, 1, f.header.id(), AssignmentJournalEventKindV2::Open, origin(2), &[2, 0, 1, 0, 8], 1024).is_err());
    assert!(
        assignment_frame_v2(id, 1, f.header.id(), AssignmentJournalEventKindV2::BindResult, origin(2), &[2, 0, 1, 0, 3], 1024)
            .is_err()
    );
}
#[test]
fn protected_floors_cannot_reuse_other_leg_capital_or_break_conservation() {
    let base = funding(&assignment());
    base.validate_shape().unwrap();
    for mutate in [
        |f: &mut AssignmentFundingRow| f.worker_reserve += 1,
        |f: &mut AssignmentFundingRow| f.earned_service_due += 1,
        |f: &mut AssignmentFundingRow| f.liability_floor += 1,
        |f: &mut AssignmentFundingRow| f.worker.paid = 1,
        |f: &mut AssignmentFundingRow| f.current_outpoint = None,
        |f: &mut AssignmentFundingRow| f.initial_locked += 1,
        |f: &mut AssignmentFundingRow| f.current_outpoint = Some(bond(80)),
        |f: &mut AssignmentFundingRow| f.last_settlement = Some(material(81)),
    ] {
        let mut f = base.clone();
        mutate(&mut f);
        assert_eq!(f.validate_shape(), Err(ContractError::Budget));
    }
}

#[test]
fn joined_prover_has_own_fixed_account_without_resettable_consent_in_key() {
    use misaka_palw_positive_contract::assignment::role_actions::AssignmentProverJoinV2;
    let a = assignment();
    let mut consent = AssignmentProverJoinV2 {
        version: 2,
        revision: 1,
        assignment: a.id(),
        original_bundle: a.bundle.id(),
        case: None,
        prover: bond(90),
        payer: principal(&[0; PUBKEY_BYTES]),
        funding_output_index: 0,
        max_fee: 10,
        max_capital: 20,
        max_locked: 40,
        max_attempts: 1,
        max_build_work: 1,
        max_submitted_bytes: 1,
        max_retained_bytes: 1,
        admission_before_daa: 150,
        service_before_daa: 400,
        retention_until_daa: 500,
        cancellation_policy: a.bundle.cancellation_policy,
    };
    let row = AssignmentProverFundingRow {
        assignment: a.id(),
        consent,
        payer_key: [0; PUBKEY_BYTES],
        payer_signature: [0; SIGNATURE_BYTES],
        funding_origin: a.origin,
        original_outpoint: NativeOutpoint { transaction: a.origin.carrier_transaction, index: 0 },
        current_outpoint: Some(NativeOutpoint { transaction: a.origin.carrier_transaction, index: 0 }),
        initial_locked: 40,
        fee: leg(10),
        capital: leg(20),
        surplus: leg(10),
        earned_service_due: 10,
        liability_floor: 20,
        reservation: material(92),
        liability: material(93),
        last_settlement: None,
    };
    let value = NativeRowValue::new(NativeRowBody::AssignmentProverFunding(row.clone())).unwrap();
    let key = value.body.key().encode();
    assert_eq!(key.len(), 137);
    assert_eq!(key[4], 18);
    let bytes = value.encode().unwrap();
    assert!(bytes.len() <= MAX_NATIVE_ROW_BYTES);
    assert_eq!(NativeRowValue::decode(&key, &bytes).unwrap(), value);
    consent.max_attempts = 2;
    let mut changed = row.clone();
    changed.consent = consent;
    assert_eq!(NativeRowBody::AssignmentProverFunding(changed).key(), value.body.key());
    let mut broken = row;
    broken.earned_service_due = 11;
    assert!(broken.validate_shape().is_err());
}

#[test]
fn funding_continuation_requires_actual_settlement_cursor_and_spent_legs() {
    let mut row = funding(&assignment());
    row.worker.paid = 10;
    row.worker.remaining -= 10;
    row.worker_reserve -= 10;
    assert!(row.validate_shape().is_err());
    let receipt = AcceptedMaterial { origin: origin(2), digest: d(83) };
    row.last_settlement = Some(receipt);
    assert!(row.validate_shape().is_err());
    row.current_outpoint = Some(NativeOutpoint { transaction: receipt.origin.carrier_transaction, index: 1 });
    row.validate_shape().unwrap();
    let mut backwards = receipt;
    backwards.origin.accepted_daa = 9;
    row.last_settlement = Some(backwards);
    assert!(row.validate_shape().is_err());
}
