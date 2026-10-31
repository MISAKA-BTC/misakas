use misaka_palw_positive_contract::{funding::*, identity::*, messages::*, *};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn result() -> MinerResultCommit {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let s = v["result_borsh_hex"].as_str().unwrap();
    let b: Vec<_> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect();
    borsh::from_slice(&b).unwrap()
}
fn fixture() -> (MinerResultCommit, MinerServiceContract, FundingPlanBasis, FundingConsent) {
    let mut r = result();
    let mut service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: r.case_id(),
        source_catalog: d(51),
        provider_delegations: d(52),
        budget: AggregateBudget {
            rounds: 2,
            unique_source_bytes: 4096,
            transmitted_bytes: 16384,
            additional_work: 32,
            retained_bytes: 4096,
            retention_until: 30,
            fee: 40,
            witness_attempts: 2,
        },
        funding_consent: d(0),
        deadline: 20,
    };
    let basis = FundingPlanBasis {
        profile: CheckProfile::ExactStreamV1,
        derivation: d(53),
        mandatory_relations: 9,
        source_catalog: service.source_catalog,
        resource_quote: d(54),
        policy: d(55),
        schedule: d(56),
        required_fee: 50,
        required_capital: 100,
    };
    let c = FundingConsent {
        version: 2,
        revision: 1,
        case: r.case_id(),
        scope: funding_scope(&r, &service, &basis),
        payer: PrincipalId(d(57)),
        max_fees: 60,
        max_locked: 200,
        funding_output_index: 1,
        refund_payload: d(58),
        admission_expiry: 15,
    };
    service.funding_consent = c.id().0;
    r.service_contract = service.id();
    (r, service, basis, c)
}
#[test]
fn consent_service_result_can_be_constructed_once_without_a_hash_fixedpoint() {
    let (r, s, b, c) = fixture();
    assert_eq!(c.scope, funding_scope(&r, &s, &b));
    assert_eq!(s.funding_consent, c.id().0);
    assert_eq!(r.service_contract, s.id());
    assert_eq!(c.validate(&r, &s, &b, 1), Ok(()));
    let mut unrelated = r.clone();
    unrelated.service_contract = d(99);
    assert_eq!(funding_scope(&unrelated, &s, &b), c.scope);
    assert_eq!(c.validate(&unrelated, &s, &b, 1), Err(ContractError::Identity));
    let mut wrong = s;
    wrong.funding_consent = d(99);
    unrelated.service_contract = wrong.id();
    assert_eq!(funding_scope(&unrelated, &wrong, &b), c.scope);
    assert_eq!(c.validate(&unrelated, &wrong, &b, 1), Err(ContractError::Identity));
}
#[test]
fn full_execution_source_service_and_plan_projection_are_bound() {
    let (r, s, b, c) = fixture();
    for i in 0..8 {
        let mut x = r.clone();
        match i {
            0 => x.output = d(99),
            1 => x.input = d(99),
            2 => x.program = d(99),
            3 => x.artifact = d(99),
            4 => x.scope.branch = d(99),
            5 => x.execution = d(99),
            6 => x.dependencies = d(99),
            _ => x.miner = PrincipalId(d(99)),
        };
        assert_ne!(funding_scope(&x, &s, &b), c.scope);
    }
    for i in 0..5 {
        let mut x = s;
        match i {
            0 => x.budget.fee += 1,
            1 => x.deadline += 1,
            2 => x.source_catalog = d(99),
            3 => x.provider_delegations = d(99),
            _ => x.budget.witness_attempts += 1,
        };
        assert_ne!(funding_scope(&r, &x, &b), c.scope);
    }
    for i in 0..4 {
        let mut x = b;
        match i {
            0 => x.required_fee += 1,
            1 => x.mandatory_relations += 1,
            2 => x.resource_quote = d(99),
            _ => x.policy = d(99),
        };
        assert_ne!(funding_scope(&r, &s, &x), c.scope);
    }
}
#[test]
fn real_payer_signature_cannot_be_reused_for_changed_terms_or_actor() {
    use libcrux_ml_dsa::ml_dsa_87::*;
    let (mut r, mut s, b, mut c) = fixture();
    let key = generate_key_pair([81; 32]);
    let public: &[u8; PUBKEY_BYTES] = key.verification_key.as_ref();
    c.payer = principal(public);
    s.funding_consent = c.id().0;
    r.service_contract = s.id();
    let sig: [u8; SIGNATURE_BYTES] = *sign(&key.signing_key, &c.id().0.0, FUNDING_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    let auth = c.authenticate(&r, &s, &b, 1, public, &sig).unwrap();
    assert_eq!(auth.consent(), &c);
    assert_eq!(auth.result(), &r);
    assert_eq!(auth.service(), &s);
    assert_eq!(auth.basis(), &b);
    for i in 0..4 {
        let mut changed = c.clone();
        match i {
            0 => changed.max_locked += 1,
            1 => changed.refund_payload = d(99),
            2 => changed.funding_output_index += 1,
            _ => changed.admission_expiry += 1,
        };
        let mut other = s;
        other.funding_consent = changed.id().0;
        let mut result = r.clone();
        result.service_contract = other.id();
        assert!(matches!(changed.authenticate(&result, &other, &b, 1, public, &sig), Err(ContractError::Signature)));
    }
    let other = generate_key_pair([82; 32]);
    assert!(matches!(c.authenticate(&r, &s, &b, 1, other.verification_key.as_ref(), &sig), Err(ContractError::Identity)));
    let wrong: [u8; SIGNATURE_BYTES] = *sign(&key.signing_key, &c.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    assert!(matches!(c.authenticate(&r, &s, &b, 1, public, &wrong), Err(ContractError::Signature)));
    assert_eq!(require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}
#[test]
fn amount_overflow_zero_plan_and_exclusive_admission_expiry_fail_closed() {
    let (r, s, b, c) = fixture();
    assert_eq!(c.validate(&r, &s, &b, 15), Err(ContractError::Budget));
    for i in 0..4 {
        let mut changed = c.clone();
        let mut basis = b;
        match i {
            0 => changed.max_fees = 49,
            1 => changed.max_locked = 149,
            2 => {
                basis.required_fee = u64::MAX;
                basis.required_capital = 1
            }
            _ => basis.mandatory_relations = 0,
        };
        changed.scope = funding_scope(&r, &s, &basis);
        let mut service = s;
        service.funding_consent = changed.id().0;
        let mut result = r.clone();
        result.service_contract = service.id();
        assert_eq!(changed.validate(&result, &service, &basis, 1), Err(ContractError::Budget));
    }
}

#[test]
fn independent_python_projection_and_final_links_match() {
    let (r, s, _b, c) = fixture();
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/funding-vectors-r1.json")).unwrap();
    let h = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(h(&c.scope.0.0), v["scope_id"].as_str().unwrap());
    assert_eq!(h(&c.id().0.0), v["consent_id"].as_str().unwrap());
    assert_eq!(h(&s.id().0), v["service_id"].as_str().unwrap());
    assert_eq!(h(&r.id().0.0), v["result_id"].as_str().unwrap());
    assert_eq!(h(&borsh::to_vec(&c).unwrap()), v["consent_borsh_hex"].as_str().unwrap());
}
