use misaka_palw_positive_contract::{identity::*, messages::*, ports::*, wire::*, *};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn result() -> MinerResultCommit {
    MinerResultCommit {
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
        service_contract: d(17),
    }
}
fn witness(r: &MinerResultCommit) -> PanelWitnessCommit {
    PanelWitnessCommit {
        version: 2,
        revision: 1,
        scope: r.scope,
        case: r.case_id(),
        result: r.id(),
        author: PrincipalId(d(18)),
        attempt: 0,
        profile: CheckProfile::ExactStreamV1,
        plan: PlanId(d(19)),
        execution_record: d(20),
        provenance: d(21),
        candidate_output: r.output,
    }
}
#[test]
fn identities_keep_result_case_claim_and_witness_distinct() {
    let r = result();
    let w = witness(&r);
    assert_ne!(r.id().0, r.case_id().0);
    assert_ne!(r.id().0, r.claim_id().0);
    assert_ne!(r.id().0, w.id().0);
    let mut other = r.clone();
    other.output = d(99);
    assert_ne!(r.id(), other.id());
    assert_ne!(r.claim_id(), other.claim_id());
    assert_eq!(r.case_id(), other.case_id());
    other.execution = d(99);
    assert_ne!(r.case_id(), other.case_id());
}
#[test]
fn every_signed_execution_field_changes_identity() {
    let r = result();
    let encoded = encode_result(&r).unwrap();
    for i in 9 + 4..encoded.len() {
        let mut bytes = encoded.clone();
        bytes[i] ^= 1;
        if let Ok(changed) = decode_result(&bytes) {
            assert_ne!(r.id(), changed.id(), "byte {i}");
        }
    }
}
#[test]
fn actual_mldsa_authenticates_actor_and_role_not_arithmetic() {
    use libcrux_ml_dsa::ml_dsa_87::*;
    let miner = generate_key_pair([1; 32]);
    let panel = generate_key_pair([2; 32]);
    let mk: &[u8; PUBKEY_BYTES] = miner.verification_key.as_ref();
    let pk: &[u8; PUBKEY_BYTES] = panel.verification_key.as_ref();
    let mut r = result();
    r.miner = principal(mk);
    let sig: [u8; SIGNATURE_BYTES] = *sign(&miner.signing_key, &r.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    assert_eq!(r.authenticate(mk, &sig).unwrap().commit(), &r);
    assert!(matches!(r.authenticate(pk, &sig), Err(ContractError::Identity)));
    let mut changed = r.clone();
    changed.output = d(99);
    assert!(matches!(changed.authenticate(mk, &sig), Err(ContractError::Signature)));
    let mut w = witness(&r);
    w.author = principal(pk);
    let ws: [u8; SIGNATURE_BYTES] = *sign(&panel.signing_key, &w.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    assert_eq!(w.authenticate(&r, pk, &ws), Ok(()));
    let wrong_context: [u8; SIGNATURE_BYTES] = *sign(&panel.signing_key, &w.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    assert_eq!(w.authenticate(&r, pk, &wrong_context), Err(ContractError::Signature));
    assert_eq!(require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}
#[test]
fn admission_scope_pins_weights_class_and_prevents_legacy_fallback() {
    let r = result();
    let c = AdmissionContext {
        scope: r.scope,
        expected_job: r.job,
        expected_execution: r.execution,
        expected_miner: r.miner,
        expected_input: r.input,
        registered_class: r.registered_class,
        artifact: r.artifact,
        program: r.program,
        tokenizer: r.tokenizer,
        execution_rules: r.execution_rules,
        selector: r.selector,
        required_form: ClaimForm::PositiveResultV2,
    };
    assert_eq!(c.check(ClaimForm::PositiveResultV2, &r), Ok(()));
    assert_eq!(c.check(ClaimForm::LegacyTrace, &r), Err(ContractError::Version));
    for n in 0..13 {
        let mut x = r.clone();
        match n {
            0 => x.scope.branch = d(99),
            1 => x.scope.ruleset = d(99),
            2 => x.scope.network = d(99),
            3 => x.registered_class = d(99),
            4 => x.artifact = d(99),
            5 => x.program = d(99),
            6 => x.tokenizer = d(99),
            7 => x.execution_rules = d(99),
            8 => x.selector = d(99),
            9 => x.job = d(99),
            10 => x.execution = d(99),
            11 => x.miner = PrincipalId(d(99)),
            _ => x.input = d(99),
        };
        assert_eq!(c.check(ClaimForm::PositiveResultV2, &x), Err(ContractError::Scope));
    }
}
#[test]
fn panel_ownership_and_attempt_are_signed_without_resetting_case() {
    let r = result();
    let w = witness(&r);
    let mut next = w.clone();
    next.author = PrincipalId(d(98));
    next.attempt = 1;
    assert_ne!(w.id(), next.id());
    assert_eq!(w.case, next.case);
    let mut wrong = w.clone();
    wrong.result = ResultId(d(99));
    assert_eq!(wrong.validate_binding(&r), Err(ContractError::Scope));
    wrong = w.clone();
    wrong.scope.branch = d(99);
    assert_eq!(wrong.validate_binding(&r), Err(ContractError::Scope));
    wrong = w.clone();
    wrong.profile = CheckProfile::FullRelationDirectV1;
    assert_eq!(wrong.validate_binding(&r), Err(ContractError::UnsupportedProfile));
}
#[test]
fn fixed_commit_codecs_reject_truncation_trailing_bytes_and_old_wire() {
    let r = result();
    let w = witness(&r);
    let bytes = encode_result(&r).unwrap();
    assert_eq!(decode_result(&bytes).unwrap(), r);
    for n in 0..bytes.len() {
        assert!(decode_result(&bytes[..n]).is_err());
    }
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode_result(&extra).is_err());
    let mut wrong = bytes.clone();
    wrong[0] ^= 1;
    assert_eq!(decode_result(&wrong), Err(ContractError::Version));
    assert_eq!(decode_witness(&bytes), Err(ContractError::Version));
    let wb = encode_witness(&w).unwrap();
    assert_eq!(decode_witness(&wb).unwrap(), w);
    let mut bad_version = r.clone();
    bad_version.revision = 99;
    assert_eq!(decode_result(&encode_result(&bad_version).unwrap()), Err(ContractError::Version));
    assert_eq!(decode_result(&vec![0; MAX_COMMIT_BYTES + 10]), Err(ContractError::Budget));
}
#[test]
fn zero_decode_overflow_and_context_overrun_are_rejected() {
    for (p, dn, m) in [(1, 0, 8192), (0, 1, 8192), (u32::MAX, 2, u32::MAX), (8192, 2, 8192)] {
        let mut r = result();
        r.prefill = p;
        r.decode = dn;
        r.max_context = m;
        assert_eq!(r.validate_shape(), Err(ContractError::Range));
    }
}
#[test]
fn canonical_source_request_has_no_panel_nonce_or_trace_generation_variant() {
    let r = result();
    let q = SourceRequest {
        version: 2,
        revision: 1,
        case: r.case_id(),
        contract: r.service_contract,
        source: r.artifact,
        kind: SourceKind::ArtifactInventory,
        first: 3,
        end: 5,
    };
    assert_eq!(q.validate(8), Ok(()));
    assert_eq!(decode_request(&encode_request(&q).unwrap()), Ok(q));
    let mut bad = q;
    bad.end = 3;
    assert_eq!(bad.validate(8), Err(ContractError::Range));
    bad.end = 9;
    assert_eq!(bad.validate(8), Err(ContractError::Range));
    let mut other = q;
    other.first = 4;
    assert_ne!(q.id(), other.id());
}
#[test]
fn evidence_is_borrowed_bounded_exactly_framed_and_canonically_ordered() {
    let r = result();
    let w = witness(&r);
    let payload = [1, 2, 3];
    let h = EvidenceHeader {
        case: r.case_id(),
        result: r.id(),
        witness: w.id(),
        plan: w.plan,
        first_relation: 0,
        relation_count: 2,
        payload_bytes: 3,
    };
    let f = EvidenceFrame::new(h, &payload, 4096).unwrap();
    let bytes = f.encode(4096).unwrap();
    assert_eq!(bytes.len(), EVIDENCE_HEADER_BYTES + 3);
    let decoded = EvidenceFrame::decode(&bytes, 4096).unwrap();
    assert_eq!(decoded, f);
    assert!(std::ptr::eq(decoded.payload().as_ptr(), bytes[EVIDENCE_HEADER_BYTES..].as_ptr()));
    for n in 0..bytes.len() {
        assert!(EvidenceFrame::decode(&bytes[..n], 4096).is_err());
    }
    assert_eq!(EvidenceFrame::decode(&bytes, 1), Err(ContractError::Budget));
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(EvidenceFrame::decode(&extra, 4096).is_err());
    let plan = CoverageDescriptor {
        scope: r.scope,
        result: r.id(),
        plan: w.plan,
        profile: CheckProfile::ExactStreamV1,
        mandatory_relations: 4,
        reserved_elements: 32,
        reserved_terms: 64,
        max_part_bytes: 4096,
    };
    let cursor = CoverageCursor { plan: w.plan, next_relation: 0 };
    assert_eq!(require_next(&plan, cursor, &w, &f), Ok(2));
    assert_eq!(require_next(&plan, CoverageCursor { next_relation: 1, ..cursor }, &w, &f), Err(ContractError::NonCanonical));
    let mut swapped_header = h;
    swapped_header.witness = WitnessId(d(99));
    let swapped = EvidenceFrame::new(swapped_header, &payload, 4096).unwrap();
    assert_eq!(require_next(&plan, cursor, &w, &swapped), Err(ContractError::Identity));
    swapped_header = h;
    swapped_header.relation_count = 5;
    let swapped = EvidenceFrame::new(swapped_header, &payload, 4096).unwrap();
    assert_eq!(require_next(&plan, cursor, &w, &swapped), Err(ContractError::Range));
    swapped_header = h;
    swapped_header.first_relation = u64::MAX;
    swapped_header.relation_count = 2;
    assert_eq!(swapped_header.validate(), Err(ContractError::Range));
    swapped_header = h;
    swapped_header.payload_bytes = 999;
    assert_eq!(EvidenceFrame::new(swapped_header, &payload, 4096), Err(ContractError::NonCanonical));
    let mut direct = w.clone();
    direct.profile = CheckProfile::FullRelationDirectV1;
    swapped_header = h;
    swapped_header.witness = direct.id();
    let swapped = EvidenceFrame::new(swapped_header, &payload, 4096).unwrap();
    assert_eq!(require_next(&plan, cursor, &direct, &swapped), Err(ContractError::UnsupportedProfile));
    let mut wrong_version = w.clone();
    wrong_version.revision = 99;
    swapped_header = h;
    swapped_header.witness = wrong_version.id();
    let swapped = EvidenceFrame::new(swapped_header, &payload, 4096).unwrap();
    assert_eq!(require_next(&plan, cursor, &wrong_version, &swapped), Err(ContractError::Version));
    let too_small = CoverageDescriptor { max_part_bytes: 1, ..plan };
    assert_eq!(require_next(&too_small, cursor, &w, &f), Err(ContractError::Budget));
}

#[test]
fn independent_python_byte_and_id_vectors_match_the_shared_contract() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let r = result();
    let w = witness(&r);
    assert_eq!(hex(&borsh::to_vec(&r).unwrap()), vectors["result_borsh_hex"].as_str().unwrap());
    assert_eq!(hex(&r.id().0.0), vectors["result_id"].as_str().unwrap());
    assert_eq!(hex(&r.case_id().0.0), vectors["case_id"].as_str().unwrap());
    assert_eq!(hex(&r.claim_id().0.0), vectors["claim_id"].as_str().unwrap());
    assert_eq!(hex(&borsh::to_vec(&w).unwrap()), vectors["witness_borsh_hex"].as_str().unwrap());
    assert_eq!(hex(&w.id().0.0), vectors["witness_id"].as_str().unwrap());
}
#[test]
fn signed_finite_service_consent_is_distinct_from_funding_or_completion() {
    let mut r = result();
    let contract = MinerServiceContract {
        version: 2,
        revision: 1,
        case: r.case_id(),
        source_catalog: d(40),
        provider_delegations: d(41),
        budget: AggregateBudget {
            rounds: 2,
            unique_source_bytes: 4096,
            transmitted_bytes: 8192,
            additional_work: 8,
            retained_bytes: 4096,
            retention_until: 20,
            fee: 10,
            witness_attempts: 2,
        },
        funding_consent: d(42),
        deadline: 10,
    };
    r.service_contract = contract.id();
    assert_eq!(contract.validate(&r, 1), Ok(()));
    assert_eq!(contract.validate(&r, 10), Err(ContractError::Budget));
    let mut modified = contract;
    modified.budget.fee = 11;
    assert_eq!(modified.validate(&r, 1), Err(ContractError::Identity));
    assert_eq!(require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}
#[test]
fn source_request_signature_binds_requester_but_canonical_identity_deduplicates() {
    use libcrux_ml_dsa::ml_dsa_87::*;
    let r = result();
    let q = SourceRequest {
        version: 2,
        revision: 1,
        case: r.case_id(),
        contract: r.service_contract,
        source: r.input,
        kind: SourceKind::Input,
        first: 0,
        end: 1,
    };
    let a = generate_key_pair([5; 32]);
    let b = generate_key_pair([6; 32]);
    let ak: &[u8; PUBKEY_BYTES] = a.verification_key.as_ref();
    let bk: &[u8; PUBKEY_BYTES] = b.verification_key.as_ref();
    assert_ne!(q.signing_id(principal(ak)), q.signing_id(principal(bk)));
    let s: [u8; SIGNATURE_BYTES] =
        *sign(&a.signing_key, &q.signing_id(principal(ak)).0, REQUEST_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    assert_eq!(q.authenticate(1, ak, &s), Ok(()));
    assert_eq!(q.authenticate(1, bk, &s), Err(ContractError::Signature));
}

#[test]
fn coverage_identity_binds_the_derivation_rules_and_complete_relation_count() {
    let r = result();
    let id = coverage_plan_id(&r, CheckProfile::ExactStreamV1, d(20), 4).unwrap();
    assert_ne!(Ok(id), coverage_plan_id(&r, CheckProfile::ExactStreamV1, d(20), 3));
    assert_ne!(Ok(id), coverage_plan_id(&r, CheckProfile::ExactStreamV1, d(21), 4));
    assert_eq!(coverage_plan_id(&r, CheckProfile::ExactStreamV1, d(20), 0), Err(ContractError::Range));
    assert_eq!(coverage_plan_id(&r, CheckProfile::FullRelationDirectV1, d(20), 4), Err(ContractError::UnsupportedProfile));
}

#[test]
fn relay_cannot_supply_malformed_arithmetic_as_an_honest_panel_packet() {
    use libcrux_ml_dsa::ml_dsa_87::*;
    let r = result();
    let mut w = witness(&r);
    let k = generate_key_pair([71; 32]);
    let key: &[u8; PUBKEY_BYTES] = k.verification_key.as_ref();
    w.author = principal(key);
    let payload = b"G14 material";
    let header = EvidenceHeader {
        case: w.case,
        result: w.result,
        witness: w.id(),
        plan: w.plan,
        first_relation: 0,
        relation_count: 1,
        payload_bytes: payload.len() as u32,
    };
    let frame = EvidenceFrame::new(header, payload, 4096).unwrap();
    let sig: [u8; SIGNATURE_BYTES] = *sign(&k.signing_key, &frame.id().0.0, EVIDENCE_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    let authorized = frame.authenticate(&w, key, &sig, 8192).unwrap();
    assert_eq!(authorized.frame(), frame);
    assert_eq!(authorized.author(), w.author);
    assert_eq!(authorized.id(), frame.id());
    assert_eq!(frame.signed_wire_bytes(), frame.wire_bytes() + SIGNATURE_BYTES as u64);
    let altered = EvidenceFrame::new(header, b"BAD material", 4096).unwrap();
    assert!(matches!(altered.authenticate(&w, key, &sig, 8192), Err(ContractError::Signature)));
    assert!(matches!(frame.authenticate(&w, key, &sig, frame.signed_wire_bytes() - 1), Err(ContractError::Budget)));
    let copied_to_next = EvidenceFrame::new(EvidenceHeader { first_relation: 1, ..header }, payload, 4096).unwrap();
    assert!(matches!(copied_to_next.authenticate(&w, key, &sig, 8192), Err(ContractError::Signature)));
    let wrong_domain: [u8; SIGNATURE_BYTES] = *sign(&k.signing_key, &frame.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    assert!(matches!(frame.authenticate(&w, key, &wrong_domain, 8192), Err(ContractError::Signature)));
}
