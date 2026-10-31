use misaka_palw_positive_contract::{identity::*, messages::*, rows::*, *};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn origin() -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 4,
        event_ordinal: 5,
        accepted_daa: 6,
        accepted_blue_score: 7,
    }
}
fn material() -> AcceptedMaterial {
    AcceptedMaterial { origin: origin(), digest: d(8) }
}
fn coverage() -> CoverageRow {
    CoverageRow {
        witness: WitnessId(d(1)),
        case: CaseId(d(2)),
        result: ResultId(d(3)),
        plan: PlanId(d(4)),
        mandatory_relations: 10,
        next_relation: 0,
        frame_bytes: 0,
        signatures: 0,
        elements: 0,
        terms: 0,
        state: CoverageState::Checking,
        last_event: origin(),
    }
}
#[test]
fn fixed_rows_roundtrip_and_do_not_conflate_corruption_with_absence() {
    let rows = [
        NativeRowBody::ClaimIndex(ClaimIndexRow { claim: ClaimId(d(1)), result: ResultId(d(2)), case: CaseId(d(3)) }),
        NativeRowBody::Coverage(coverage()),
        NativeRowBody::SourceUnion(SourceUnionRow {
            case: CaseId(d(2)),
            kind: SourceKind::Input,
            source: d(3),
            source_units: 9,
            requested_units: 5,
            completed_units: 2,
            union: material(),
        }),
        NativeRowBody::ServiceReceipt(ServiceReceiptRow {
            case: CaseId(d(2)),
            kind: SourceKind::Input,
            source: d(3),
            unit: 4,
            result: ResultId(d(5)),
            duty: material(),
            response: material(),
            provider: PrincipalId(d(6)),
            bytes: 4096,
            additional_work: 1,
            settlement: Some(material()),
        }),
        NativeRowBody::PacketReceipt(PacketReceiptRow {
            witness: WitnessId(d(2)),
            first_relation: 3,
            frame: EvidencePartId(d(4)),
            author: PrincipalId(d(5)),
            signed_bytes: 6000,
            origin: origin(),
        }),
        NativeRowBody::JournalHead(JournalHeadRow {
            case: CaseId(d(2)),
            sequence: 1,
            previous: d(0),
            head: d(1),
            history: material(),
        }),
    ];
    for body in rows {
        let row = NativeRowValue::new(body).unwrap();
        let key = row.body.key().encode();
        let bytes = row.encode().unwrap();
        assert_eq!(NativeRowValue::decode(&key, &bytes), Ok(row));
        let mut wrong = key.clone();
        *wrong.last_mut().unwrap() ^= 1;
        assert_eq!(NativeRowValue::decode(&wrong, &bytes), Err(ContractError::Identity));
        let mut tail = bytes.clone();
        tail.push(0);
        assert_eq!(NativeRowValue::decode(&key, &tail), Err(ContractError::NonCanonical));
        assert!(NativeRowValue::decode(&key, &bytes[..bytes.len() - 1]).is_err());
        let mut wrongversion = bytes;
        wrongversion[0] = 1;
        assert_eq!(NativeRowValue::decode(&key, &wrongversion), Err(ContractError::Version));
    }
    assert_eq!(std::mem::size_of::<NativeHash64>(), 64);
    assert_eq!(borsh::to_vec(&origin()).unwrap().len(), 216);
}
#[test]
fn no_saved_pass_or_union_summary_can_exceed_its_own_plan() {
    let mut c = coverage();
    c.state = CoverageState::Matched;
    assert_eq!(NativeRowValue::new(NativeRowBody::Coverage(c)), Err(ContractError::Range));
    c.next_relation = 10;
    assert!(NativeRowValue::new(NativeRowBody::Coverage(c)).is_ok()); // DTO only, never CheckedTerminal.
    c.next_relation = 11;
    assert_eq!(NativeRowValue::new(NativeRowBody::Coverage(c)), Err(ContractError::Range));
    c.state = CoverageState::Mismatched;
    c.next_relation = 0;
    assert_eq!(NativeRowValue::new(NativeRowBody::Coverage(c)), Err(ContractError::Range));
    let u = SourceUnionRow {
        case: CaseId(d(1)),
        kind: SourceKind::Input,
        source: d(2),
        source_units: 4,
        requested_units: 3,
        completed_units: 4,
        union: material(),
    };
    assert_eq!(NativeRowValue::new(NativeRowBody::SourceUnion(u)), Err(ContractError::Range));
    assert_eq!(NativeRowValue::decode(&[0; MAX_NATIVE_KEY_BYTES + 1], &[]), Err(ContractError::Budget));
    assert_eq!(NativeRowValue::decode(&[], &vec![0; MAX_NATIVE_ROW_BYTES + 1]), Err(ContractError::Budget));
    let packet = PacketReceiptRow {
        witness: WitnessId(d(1)),
        first_relation: 0,
        frame: EvidencePartId(d(2)),
        author: PrincipalId(d(3)),
        signed_bytes: (wire::EVIDENCE_HEADER_BYTES + SIGNATURE_BYTES) as u64,
        origin: origin(),
    };
    // A receipt for a checked r1 relation must include its inner TIR framing.
    assert_eq!(NativeRowValue::new(NativeRowBody::PacketReceipt(packet)), Err(ContractError::Range));
}
#[test]
fn balances_require_conservation_and_checked_arithmetic() {
    let valid = BalanceLeg { initial: 10, paid: 3, refunded: 2, burned: 1, remaining: 4 };
    assert_eq!(valid.validate(), Ok(()));
    assert_eq!(BalanceLeg { remaining: 5, ..valid }.validate(), Err(ContractError::Budget));
    assert_eq!(BalanceLeg { initial: 0, paid: u64::MAX, refunded: 1, burned: 0, remaining: 0 }.validate(), Err(ContractError::Budget));
}

fn signed_bundle() -> (ClaimIndexRow, ResultRow, CaseRow, FundingRow, WitnessRow, CoverageRow) {
    use libcrux_ml_dsa::ml_dsa_87::*;
    use misaka_palw_positive_contract::funding::*;
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let hex = json["result_borsh_hex"].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    let mut r: MinerResultCommit = borsh::from_slice(&bytes).unwrap();
    let miner = generate_key_pair([1; 32]);
    let payer = generate_key_pair([2; 32]);
    let panel = generate_key_pair([3; 32]);
    let policy = d(31);
    let fired_fork = d(32);
    r.scope.ruleset = misaka_palw_positive_contract::native::ruleset_id(fired_fork, policy);
    r.miner = principal(miner.verification_key.as_ref());
    r.dependencies = material().digest;
    let mut service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: r.case_id(),
        source_catalog: d(33),
        provider_delegations: d(34),
        budget: AggregateBudget {
            rounds: 2,
            unique_source_bytes: 4096,
            transmitted_bytes: 8192,
            additional_work: 8,
            retained_bytes: 4096,
            retention_until: 60,
            fee: 5,
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
        policy,
        schedule: d(37),
        required_fee: 10,
        required_capital: 20,
    };
    let consent = FundingConsent {
        version: 2,
        revision: 1,
        case: r.case_id(),
        scope: funding_scope(&r, &service, &basis),
        payer: principal(payer.verification_key.as_ref()),
        max_fees: 12,
        max_locked: 100,
        funding_output_index: 1,
        refund_payload: d(38),
        admission_expiry: 40,
    };
    service.funding_consent = consent.id().0;
    r.service_contract = service.id();
    let result = ResultRow {
        miner_signature: *sign(&miner.signing_key, &r.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref(),
        miner_key: *miner.verification_key.as_ref(),
        origin: origin(),
        policy,
        fired_fork,
        funding_scope: consent.scope,
        dependencies: material(),
        role_assignment: material(),
        result: r.clone(),
    };
    let point = NativeOutpoint { transaction: origin().carrier_transaction, index: 1 };
    let leg = |amount| BalanceLeg { initial: amount, paid: 0, refunded: 0, burned: 0, remaining: amount };
    let funding = FundingRow {
        payer_signature: *sign(&payer.signing_key, &consent.id().0.0, FUNDING_SIGN_CONTEXT, [0; 32]).unwrap().as_ref(),
        payer_key: *payer.verification_key.as_ref(),
        consent,
        basis,
        funding_origin: origin(),
        original_outpoint: point,
        current_outpoint: Some(point),
        initial_locked: 35,
        fee: leg(10),
        capital: leg(20),
        surplus: leg(5),
        liability: material(),
        last_settlement: Some(material()),
    };
    let plan = coverage_plan_id(&r, basis.profile, basis.derivation, 10).unwrap();
    let w = PanelWitnessCommit {
        version: 2,
        revision: 1,
        scope: r.scope,
        case: r.case_id(),
        result: r.id(),
        author: principal(panel.verification_key.as_ref()),
        attempt: 0,
        profile: basis.profile,
        plan,
        execution_record: d(40),
        provenance: d(41),
        candidate_output: r.output,
    };
    let witness = WitnessRow {
        panel_signature: *sign(&panel.signing_key, &w.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref(),
        panel_key: *panel.verification_key.as_ref(),
        witness: w.clone(),
        origin: origin(),
        record_header: material(),
        generated_tokens: material(),
        role_assignment: material(),
    };
    let case = CaseRow {
        case: r.case_id(),
        result: r.id(),
        service,
        coverage: CoverageDescriptor {
            scope: r.scope,
            result: r.id(),
            plan,
            profile: basis.profile,
            mandatory_relations: 10,
            reserved_elements: 100,
            reserved_terms: 200,
            max_part_bytes: 10000,
        },
        status: ClaimStatus::PositivePassed,
        miner_data: MinerDataStatus::Pending,
        witness_status: WitnessStatus::Complete,
        positive_witness: Some(w.id()),
        usage: AggregateUsage::default(),
        reservation: material(),
        liability: material(),
        maturity_daa: 100,
        unresolved_fraud: 0,
        last_event: origin(),
    };
    let coverage = CoverageRow {
        witness: w.id(),
        case: r.case_id(),
        result: r.id(),
        plan,
        mandatory_relations: 10,
        next_relation: 10,
        frame_bytes: 1000,
        signatures: 10,
        elements: 100,
        terms: 200,
        state: CoverageState::Matched,
        last_event: origin(),
    };
    (ClaimIndexRow { claim: r.claim_id(), result: r.id(), case: r.case_id() }, result, case, funding, witness, coverage)
}

#[test]
fn large_signed_rows_and_all_optional_references_fit_the_fixed_codec() {
    let (_, r, c, f, w, _) = signed_bundle();
    for body in [NativeRowBody::Result(r), NativeRowBody::Case(c), NativeRowBody::Funding(f), NativeRowBody::Witness(w)] {
        let row = NativeRowValue::new(body).unwrap();
        let bytes = row.encode().unwrap();
        assert!(bytes.len() < MAX_NATIVE_ROW_BYTES);
        assert_eq!(NativeRowValue::decode(&row.body.key().encode(), &bytes), Ok(row));
    }
}

#[test]
fn signed_bundles_bind_origin_plan_consent_and_do_not_require_panel_ack() {
    let (i, r, c, f, w, coverage) = signed_bundle();
    assert_eq!(validate_bundle(&i, &r, &c, &f), Ok(()));
    assert_eq!(validate_witness_bundle(&r, &c, &w, &coverage), Ok(()));
    // Pending data obligation survives a matched Panel witness.
    assert_eq!(c.miner_data, MinerDataStatus::Pending);
    let mut bad_index = i;
    bad_index.claim = ClaimId(d(99));
    assert_eq!(validate_bundle(&bad_index, &r, &c, &f), Err(ContractError::Identity));
    let mut bad_case = c;
    bad_case.coverage.plan = PlanId(d(99));
    assert_eq!(validate_bundle(&i, &r, &bad_case, &f), Err(ContractError::Identity));
    bad_case = c;
    bad_case.witness_status = WitnessStatus::Rejected;
    assert_eq!(validate_bundle(&i, &r, &bad_case, &f), Err(ContractError::Identity));
    let mut bad_origin = f.clone();
    bad_origin.funding_origin.accepted_daa += 1;
    assert_eq!(validate_bundle(&i, &r, &c, &bad_origin), Err(ContractError::Identity));
    let mut bad_sig = f.clone();
    bad_sig.payer_signature[0] ^= 1;
    assert_eq!(validate_bundle(&i, &r, &c, &bad_sig), Err(ContractError::Signature));
    let mut bad_point = f.clone();
    bad_point.original_outpoint.index += 1;
    assert_eq!(validate_bundle(&i, &r, &c, &bad_point), Err(ContractError::Budget));
    let mut no_lock = f;
    no_lock.current_outpoint = None;
    assert_eq!(validate_bundle(&i, &r, &c, &no_lock), Err(ContractError::Budget));
    let mut wrong_witness = coverage;
    wrong_witness.witness = WitnessId(d(99));
    assert_eq!(validate_witness_bundle(&r, &c, &w, &wrong_witness), Err(ContractError::Identity));
    let mut over_work = coverage;
    over_work.terms += 1;
    assert_eq!(validate_witness_bundle(&r, &c, &w, &over_work), Err(ContractError::Identity));
    let mut bad_panel = w;
    bad_panel.panel_signature[0] ^= 1;
    assert_eq!(validate_witness_bundle(&r, &c, &bad_panel, &coverage), Err(ContractError::Signature));
}
