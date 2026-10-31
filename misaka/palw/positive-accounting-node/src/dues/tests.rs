//! Pure arithmetic fixtures only. Origin fields repeat the existing common-row
//! codec fixture; zero signatures are deliberately NOT authenticated here.
//! No accepted duty, canonical price, native origin, lock or spend is created.
use super::*;
use misaka_palw_positive_contract::{funding::*, identity::*, messages::*, rows::*};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn material() -> AcceptedMaterial {
    AcceptedMaterial {
        origin: AcceptedOrigin {
            accepting_block: NativeHash64([1; 64]),
            carrier_block: NativeHash64([2; 64]),
            carrier_transaction: NativeHash64([3; 64]),
            accepted_tx_ordinal: 4,
            event_ordinal: 5,
            accepted_daa: 6,
            accepted_blue_score: 7,
        },
        digest: d(8),
    }
}
fn fixture() -> (ResultRow, CaseRow, FundingRow, ServiceReceiptRow, ServiceTerms) {
    let mut r = MinerResultCommit {
        version: 2,
        revision: 1,
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        job: d(4),
        execution: d(5),
        miner: PrincipalId(d(6)),
        registered_class: d(7),
        artifact: d(8),
        program: d(9),
        input: d(10),
        tokenizer: d(11),
        execution_rules: d(12),
        selector: d(13),
        generation_seed: [14; 32],
        temperature_q24: 0,
        prefill: 2,
        decode: 3,
        max_context: 8192,
        output: d(15),
        dependencies: d(16),
        service_contract: d(0),
    };
    r.scope.ruleset = misaka_palw_positive_contract::native::ruleset_id(d(32), d(31));
    r.miner = principal(&[0; PUBKEY_BYTES]);
    r.dependencies = material().digest;
    let mut service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: r.case_id(),
        source_catalog: d(33),
        provider_delegations: d(34),
        budget: AggregateBudget {
            rounds: 10,
            unique_source_bytes: 100,
            transmitted_bytes: 200,
            additional_work: 100,
            retained_bytes: 200,
            retention_until: 60,
            fee: 20,
            witness_attempts: 2,
        },
        funding_consent: d(0),
        deadline: 50,
    };
    let basis = FundingPlanBasis {
        profile: CheckProfile::ExactStreamV1,
        derivation: d(35),
        mandatory_relations: 10,
        source_catalog: service.source_catalog,
        resource_quote: d(36),
        policy: d(31),
        schedule: d(37),
        required_fee: 27,
        required_capital: 0,
    };
    let consent = FundingConsent {
        version: 2,
        revision: 1,
        case: r.case_id(),
        scope: funding_scope(&r, &service, &basis),
        payer: principal(&[0; PUBKEY_BYTES]),
        max_fees: 30,
        max_locked: 100,
        funding_output_index: 1,
        refund_payload: d(38),
        admission_expiry: 40,
    };
    service.funding_consent = consent.id().0;
    r.service_contract = service.id();
    let result = ResultRow {
        result: r.clone(),
        miner_key: [0; PUBKEY_BYTES],
        miner_signature: [0; SIGNATURE_BYTES],
        origin: material().origin,
        policy: basis.policy,
        fired_fork: d(32),
        funding_scope: consent.scope,
        dependencies: material(),
        role_assignment: material(),
    };
    let case = CaseRow {
        case: r.case_id(),
        result: r.id(),
        service,
        coverage: CoverageDescriptor {
            scope: r.scope,
            result: r.id(),
            plan: coverage_plan_id(&r, basis.profile, basis.derivation, basis.mandatory_relations).unwrap(),
            profile: basis.profile,
            mandatory_relations: 10,
            reserved_elements: 100,
            reserved_terms: 200,
            max_part_bytes: 10000,
        },
        status: ClaimStatus::EndedUnverified,
        miner_data: MinerDataStatus::Pending,
        witness_status: WitnessStatus::Incomplete,
        positive_witness: None,
        usage: AggregateUsage { fee: 2, ..AggregateUsage::default() },
        reservation: material(),
        liability: material(),
        maturity_daa: 100,
        unresolved_fraud: 0,
        last_event: material().origin,
    };
    let leg = |n| BalanceLeg { initial: n, paid: 0, refunded: 0, burned: 0, remaining: n };
    let point = NativeOutpoint { transaction: material().origin.carrier_transaction, index: 1 };
    let funding = FundingRow {
        consent,
        basis,
        payer_key: [0; PUBKEY_BYTES],
        payer_signature: [0; SIGNATURE_BYTES],
        funding_origin: material().origin,
        original_outpoint: point,
        current_outpoint: Some(point),
        initial_locked: 100,
        fee: leg(27),
        capital: leg(0),
        surplus: leg(73),
        liability: material(),
        last_settlement: None,
    };
    let provider = PrincipalId(d(41));
    let receipt = ServiceReceiptRow {
        case: case.case,
        kind: SourceKind::Input,
        source: r.input,
        unit: 0,
        result: r.id(),
        duty: material(),
        response: AcceptedMaterial { digest: d(42), ..material() },
        provider,
        bytes: 10,
        additional_work: 1,
        settlement: None,
    };
    (result, case, funding, receipt, ServiceTerms { amount: 10, recipient: Recipient::Fixed(provider) })
}

#[test]
fn first_source_due_preserves_meters_and_does_not_pay_or_erase_ended_service() {
    let (r, c, f, receipt, terms) = fixture();
    let before = f.clone();
    let usage = AggregateUsage { transmitted_bytes: 10, additional_work: 1, ..c.usage };
    let p = prepare_source_due(&r, &c, &f, usage, &receipt, None, terms).unwrap();
    assert_eq!(p.next_usage, AggregateUsage { fee: 12, ..usage });
    let intent = p.fee_delta.as_ref().unwrap();
    assert_eq!(intent.receipt_key, receipt_key(&receipt));
    assert_eq!(
        (intent.amount, intent.provider, intent.funding_scope, intent.schedule),
        (10, receipt.provider, r.funding_scope, f.basis.schedule)
    );
    assert_eq!(intent.accepted_response, receipt.response);
    assert_eq!(p.next_receipt.settlement, None);
    let quote = FeeScheduleCeilings { payer: f.consent.payer, fee_cap: 30, fee_hold: 100, source_fees: 20, all_fees_and_burn: 27 };
    // A native writer must compose both arithmetic checks and its actual owner
    // checks, including the pre-request worst-case reservation, atomically.
    assert_eq!(check_source_quote(&r, &CaseRow { usage: p.next_usage, ..c }, &f, quote), Ok(()));
    assert_eq!(f, before);
    assert_eq!(c.usage.fee, 2);
    assert_eq!(p.check_snapshot(c.usage, None), Ok(()));
    assert_eq!(p.check_snapshot(c.usage, Some(&receipt)), Err(SourceDueError::StaleSnapshot));
    assert_eq!(p.check_snapshot(p.next_usage, None), Err(SourceDueError::StaleSnapshot));
}

#[test]
fn duplicate_receipt_never_adds_due_and_cannot_replace_first_provider_or_response() {
    let (r, c, f, receipt, terms) = fixture();
    let usage = AggregateUsage { transmitted_bytes: 10, ..c.usage };
    let p = prepare_source_due(&r, &c, &f, usage, &receipt, Some(&receipt), terms).unwrap();
    assert_eq!(p.fee_delta, None);
    assert_eq!(p.next_usage, usage);
    assert_eq!(p.next_receipt, receipt);
    for changed in [
        ServiceReceiptRow { provider: PrincipalId(d(99)), ..receipt },
        ServiceReceiptRow { response: material(), ..receipt },
        ServiceReceiptRow { unit: 1, ..receipt },
    ] {
        assert_eq!(prepare_source_due(&r, &c, &f, usage, &changed, Some(&receipt), terms), Err(SourceDueError::ReceiptConflict));
    }
}

#[test]
fn zero_fee_first_receipt_keeps_identity_and_later_price_cannot_reopen_due() {
    let (r, c, f, receipt, terms) = fixture();
    let first = prepare_source_due(&r, &c, &f, c.usage, &receipt, None, ServiceTerms { amount: 0, ..terms }).unwrap();
    let zero_delta = first.fee_delta.as_ref().unwrap();
    assert_eq!(zero_delta.amount, 0);
    assert_eq!(zero_delta.receipt_key, receipt_key(&receipt));
    assert_eq!(first.next_usage, c.usage);
    assert_eq!(first.next_receipt, receipt);
    assert_eq!(first.next_receipt.settlement, None);
    // No actual spend is needed to preserve first-receipt identity. A later
    // supplied price cannot reopen that key, even before any settlement marker.
    let case = CaseRow { usage: first.next_usage, ..c };
    let again =
        prepare_source_due(&r, &case, &f, case.usage, &receipt, Some(&first.next_receipt), ServiceTerms { amount: u64::MAX, ..terms })
            .unwrap();
    assert_eq!(again.fee_delta, None);
    assert_eq!(again.next_usage.fee, c.usage.fee);
    assert_eq!(again.next_receipt, first.next_receipt);
}

#[test]
fn settlement_reference_is_not_first_delivery_evidence_or_a_new_due() {
    let (r, c, f, mut receipt, terms) = fixture();
    receipt.settlement = Some(material());
    assert_eq!(prepare_source_due(&r, &c, &f, c.usage, &receipt, None, terms), Err(SourceDueError::UnexpectedSettlement));
    let p = prepare_source_due(&r, &c, &f, c.usage, &receipt, Some(&receipt), terms).unwrap();
    assert!(p.fee_delta.is_none());
    assert_eq!(p.next_receipt.settlement, Some(material()));
}

#[test]
fn preincremented_fee_regressing_meters_and_completion_recipient_are_rejected() {
    let (r, c, f, receipt, terms) = fixture();
    assert_eq!(
        prepare_source_due(&r, &c, &f, AggregateUsage { fee: 12, ..c.usage }, &receipt, None, terms),
        Err(SourceDueError::Usage)
    );
    let mut c = c;
    c.usage.transmitted_bytes = 5;
    assert_eq!(
        prepare_source_due(&r, &c, &f, AggregateUsage { transmitted_bytes: 4, ..c.usage }, &receipt, None, terms),
        Err(SourceDueError::Usage)
    );
    assert_eq!(
        prepare_source_due(&r, &c, &f, c.usage, &receipt, None, ServiceTerms { recipient: Recipient::CompletingPanel, ..terms }),
        Err(SourceDueError::Recipient)
    );
}

#[test]
fn cross_result_scope_schedule_source_and_fee_cap_substitution_fail_closed() {
    let (r, c, f, receipt, terms) = fixture();
    for bad in [ServiceReceiptRow { result: ResultId(d(99)), ..receipt }, ServiceReceiptRow { source: r.result.artifact, ..receipt }] {
        assert_eq!(prepare_source_due(&r, &c, &f, c.usage, &bad, None, terms), Err(SourceDueError::Binding));
    }
    let mut bad = r.clone();
    bad.funding_scope = FundingScopeId(d(99));
    assert_eq!(prepare_source_due(&bad, &c, &f, c.usage, &receipt, None, terms), Err(SourceDueError::Binding));
    let mut bad = f.clone();
    bad.basis.schedule = d(99);
    assert!(prepare_source_due(&r, &c, &bad, c.usage, &receipt, None, terms).is_err());
    assert!(prepare_source_due(&r, &c, &f, c.usage, &receipt, None, ServiceTerms { amount: 19, ..terms }).is_err());
    assert_eq!(
        prepare_source_due(&r, &c, &f, c.usage, &receipt, None, ServiceTerms { amount: u64::MAX, ..terms }),
        Err(SourceDueError::Overflow)
    );
}

#[test]
fn whole_schedule_quote_counts_source_separately_from_court_and_never_mutates() {
    let (r, c, f, _, _) = fixture();
    let quote = FeeScheduleCeilings { payer: f.consent.payer, fee_cap: 30, fee_hold: 100, source_fees: 20, all_fees_and_burn: 27 };
    assert_eq!(check_source_quote(&r, &c, &f, quote), Ok(()));
    for bad in [
        FeeScheduleCeilings { source_fees: 21, ..quote },
        FeeScheduleCeilings { all_fees_and_burn: 28, ..quote },
        FeeScheduleCeilings { source_fees: 1, ..quote },
        FeeScheduleCeilings { fee_hold: 26, ..quote },
    ] {
        assert_eq!(check_source_quote(&r, &c, &f, bad), Err(SourceDueError::Budget));
    }
    assert_eq!((f.fee.remaining, f.capital.remaining, f.surplus.remaining), (27, 0, 73));
}
