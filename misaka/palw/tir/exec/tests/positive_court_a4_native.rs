//! Native-body shape and funding PREflight only. No successful live decoder,
//! coin lock, UTXO mutation, native admission or DB batch is claimed here.
#![cfg(feature = "node")]
use kaspa_consensus_core::palw_positive_court_v2::coordinator::{admission::CourtAdmissionRootV2, funding::*, native_body::*};
use misaka_palw_positive_contract::{
    ContractError,
    funding::*,
    identity::*,
    messages::*,
    tir::{exact_derivation_id, greedy_selector_id},
};
use std::{cell::RefCell, rc::Rc};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn key(seed: u8) -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]).verification_key.as_ref()
}
fn sign(seed: u8, digest: Digest, context: &[u8]) -> [u8; SIGNATURE_BYTES] {
    let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, &digest.0, context, [3; 32]).unwrap().as_ref()
}
struct Fixture {
    result: MinerResultCommit,
    service: MinerServiceContract,
    basis: FundingPlanBasis,
    consent: FundingConsent,
    miner_key: [u8; PUBKEY_BYTES],
    miner_signature: [u8; SIGNATURE_BYTES],
    payer_key: [u8; PUBKEY_BYTES],
    payer_signature: [u8; SIGNATURE_BYTES],
}
impl Fixture {
    fn new() -> Self {
        let miner_key = key(101);
        let payer_key = key(102);
        let mut result = MinerResultCommit {
            version: 2,
            revision: 1,
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            job: d(4),
            execution: d(5),
            miner: principal(&miner_key),
            registered_class: d(6),
            artifact: d(7),
            program: d(8),
            input: d(9),
            tokenizer: d(10),
            execution_rules: d(11),
            selector: greedy_selector_id(),
            generation_seed: [12; 32],
            temperature_q24: 0,
            prefill: 2,
            decode: 2,
            max_context: 4,
            output: d(13),
            dependencies: d(14),
            service_contract: d(0),
        };
        let mut service = MinerServiceContract {
            version: 2,
            revision: 1,
            case: result.case_id(),
            source_catalog: d(15),
            provider_delegations: d(16),
            budget: AggregateBudget {
                rounds: 4,
                unique_source_bytes: 4096,
                transmitted_bytes: 16384,
                additional_work: 32,
                retained_bytes: 8192,
                retention_until: 60,
                fee: 20,
                witness_attempts: 2,
            },
            funding_consent: d(0),
            deadline: 50,
        };
        let basis = FundingPlanBasis {
            profile: CheckProfile::ExactStreamV1,
            derivation: exact_derivation_id(),
            mandatory_relations: 8,
            source_catalog: service.source_catalog,
            resource_quote: d(17),
            policy: d(18),
            schedule: d(19),
            required_fee: 30,
            required_capital: 70,
        };
        let consent = FundingConsent {
            version: 2,
            revision: 1,
            case: result.case_id(),
            scope: funding_scope(&result, &service, &basis),
            payer: principal(&payer_key),
            max_fees: 40,
            max_locked: 120,
            funding_output_index: 1,
            // Literal synthetic address payload; not an unkeyed PrincipalId.
            // The live native output/address adapter is still unassigned.
            refund_payload: d(20),
            admission_expiry: 40,
        };
        service.funding_consent = consent.id().0;
        result.service_contract = service.id();
        let miner_signature = sign(101, result.id().0, RESULT_SIGN_CONTEXT);
        let payer_signature = sign(102, consent.id().0, FUNDING_SIGN_CONTEXT);
        Self { result, service, basis, consent, miner_key, miner_signature, payer_key, payer_signature }
    }
    fn refresh_links_and_signatures(&mut self) {
        self.consent.scope = funding_scope(&self.result, &self.service, &self.basis);
        self.service.funding_consent = self.consent.id().0;
        self.result.service_contract = self.service.id();
        self.miner_signature = sign(101, self.result.id().0, RESULT_SIGN_CONTEXT);
        self.payer_signature = sign(102, self.consent.id().0, FUNDING_SIGN_CONTEXT);
    }
    fn context(&self) -> AdmissionContext {
        AdmissionContext {
            scope: self.result.scope,
            expected_job: self.result.job,
            expected_execution: self.result.execution,
            expected_miner: self.result.miner,
            expected_input: self.result.input,
            registered_class: self.result.registered_class,
            artifact: self.result.artifact,
            program: self.result.program,
            tokenizer: self.result.tokenizer,
            execution_rules: self.result.execution_rules,
            selector: self.result.selector,
            required_form: ClaimForm::PositiveResultV2,
        }
    }
    fn inputs(&self) -> SignedFundingInputsV2<'_> {
        SignedFundingInputsV2 {
            result: &self.result,
            service: &self.service,
            consent: &self.consent,
            miner_key: &self.miner_key,
            miner_signature: &self.miner_signature,
            payer_key: &self.payer_key,
            payer_signature: &self.payer_signature,
        }
    }
    fn result_body(&self) -> PositiveNativeBodyV2 {
        PositiveNativeBodyV2 {
            version: 2,
            revision: 1,
            operation: PositiveNativeOperationV2::ResultAdmission {
                result: self.result.clone(),
                miner_signature: self.miner_signature,
                service: self.service,
                basis: self.basis,
                consent: self.consent.clone(),
                payer_signature: self.payer_signature,
            },
        }
    }
}
fn body_limits() -> NativeBodyLimitsV2 {
    NativeBodyLimitsV2 { max_body_bytes: 1 << 20, max_opaque_bytes: 1 << 18, max_exact_relations: 32, max_signatures: 2 }
}
fn witness(f: &Fixture) -> PanelWitnessCommit {
    PanelWitnessCommit {
        version: 2,
        revision: 1,
        scope: f.result.scope,
        case: f.result.case_id(),
        result: f.result.id(),
        author: principal(&key(103)),
        attempt: 0,
        profile: CheckProfile::ExactStreamV1,
        plan: PlanId(d(23)),
        execution_record: d(24),
        provenance: d(25),
        candidate_output: f.result.output,
    }
}
fn exact_frame(f: &Fixture, count: u64) -> Vec<u8> {
    let w = witness(f);
    misaka_palw_positive_contract::wire::EvidenceFrame::new(
        misaka_palw_positive_contract::wire::EvidenceHeader {
            case: f.result.case_id(),
            result: f.result.id(),
            witness: w.id(),
            plan: w.plan,
            first_relation: 0,
            relation_count: count,
            payload_bytes: 1,
        },
        &[7],
        body_limits().max_opaque_bytes,
    )
    .unwrap()
    .encode(body_limits().max_opaque_bytes)
    .unwrap()
}

#[test]
fn all_local_body_operations_roundtrip_without_becoming_native_admission() {
    let f = Fixture::new();
    let w = witness(&f);
    let request = SourceRequest {
        version: 2,
        revision: 1,
        case: f.result.case_id(),
        contract: f.service.id(),
        source: d(26),
        kind: SourceKind::ArtifactInventory,
        first: 0,
        end: 2,
    };
    let operations = [
        f.result_body().operation,
        PositiveNativeOperationV2::WitnessCommit { witness: w.clone(), panel_signature: [0; SIGNATURE_BYTES], record: vec![1, 2, 3] },
        PositiveNativeOperationV2::ExactFrame { frame: exact_frame(&f, 1), panel_signature: [0; SIGNATURE_BYTES] },
        PositiveNativeOperationV2::SourceRequest { request, requester: w.author, signature: [0; SIGNATURE_BYTES] },
        PositiveNativeOperationV2::SourceResponse { case: f.result.case_id(), request: RequestId(d(27)), response: vec![4, 5] },
        PositiveNativeOperationV2::Finish { case: f.result.case_id(), result: f.result.id(), witness: w.id() },
        PositiveNativeOperationV2::Expire { case: f.result.case_id(), result: f.result.id() },
    ];
    for operation in operations {
        let body = PositiveNativeBodyV2 { version: 2, revision: 1, operation };
        let bytes = encode_native_body_v2(&body, body_limits()).unwrap();
        assert_eq!(decode_native_body_v2(&bytes, body_limits()).unwrap(), body);
    }
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}

#[test]
fn body_preflight_refuses_unknown_version_kind_truncation_and_trailing_bytes() {
    let f = Fixture::new();
    let valid = encode_native_body_v2(&f.result_body(), body_limits()).unwrap();
    for end in [0, 1, 3, 4, 5, valid.len() - 1] {
        assert!(decode_native_body_v2(&valid[..end], body_limits()).is_err());
    }
    for offset in [0, 2, 4] {
        let mut invalid = valid.clone();
        invalid[offset] = 255;
        assert!(decode_native_body_v2(&invalid, body_limits()).is_err());
    }
    let mut trailing = valid;
    trailing.push(0);
    assert!(decode_native_body_v2(&trailing, body_limits()).is_err());
}

#[test]
fn nested_opaque_length_prefixes_cannot_allocate_or_bypass_body_and_heap_caps() {
    let f = Fixture::new();
    let mut hostile = vec![2, 0, 1, 0, 2]; // header + ExactFrame kind
    hostile.extend_from_slice(&u32::MAX.to_le_bytes());
    assert!(scan_native_body_v2(&hostile, body_limits()).is_err());
    assert!(decode_native_body_v2(&hostile, body_limits()).is_err());
    let body = PositiveNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveNativeOperationV2::ExactFrame { frame: exact_frame(&f, 1), panel_signature: [0; SIGNATURE_BYTES] },
    };
    let bytes = encode_native_body_v2(&body, body_limits()).unwrap();
    let summary = scan_native_body_v2(&bytes, body_limits()).unwrap();
    assert_eq!(summary.signatures, 1);
    assert_eq!(summary.relations, 1);
    let mut caps = body_limits();
    caps.max_body_bytes = bytes.len() as u64 - 1;
    assert!(scan_native_body_v2(&bytes, caps).is_err());
    caps = body_limits();
    caps.max_opaque_bytes = exact_frame(&f, 1).len() as u64 - 1;
    assert!(scan_native_body_v2(&bytes, caps).is_err());
    caps = body_limits();
    caps.max_signatures = 0;
    assert!(scan_native_body_v2(&bytes, caps).is_err());
    caps = body_limits();
    caps.max_exact_relations = 0;
    assert!(scan_native_body_v2(&bytes, caps).is_err());
}

#[test]
fn result_signature_count_and_exact_relation_count_are_independently_bounded() {
    let f = Fixture::new();
    let bytes = encode_native_body_v2(&f.result_body(), body_limits()).unwrap();
    assert_eq!(scan_native_body_v2(&bytes, body_limits()).unwrap().signatures, 2);
    let mut cap = body_limits();
    cap.max_signatures = 1;
    assert!(decode_native_body_v2(&bytes, cap).is_err());
    let frame_body = PositiveNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveNativeOperationV2::ExactFrame { frame: exact_frame(&f, 33), panel_signature: [0; SIGNATURE_BYTES] },
    };
    assert!(encode_native_body_v2(&frame_body, body_limits()).is_err());
}

// Explicit native-resolver fixture facts. A synthetic carrier digest/amount
// exercises production validation but is not a validated UTXO or coin lock.
struct FundingFacts {
    root: CourtAdmissionRootV2,
    quote: RootedFundingQuoteV2,
    output: ResolvedFundingOutputV2,
}
#[derive(Clone)]
struct FundingView(Rc<RefCell<FundingFacts>>);
impl RootedFundingViewV2 for FundingView {
    type Error = &'static str;
    type Carrier = Digest;
    fn root(&self) -> CourtAdmissionRootV2 {
        self.0.borrow().root
    }
    fn resolve_quote(&self, projected: &FundingBasisInputV2) -> Result<RootedFundingQuoteV2, Self::Error> {
        // Acyclicity is observable at the resolver boundary: it cannot consume
        // either final link or a carrying-transaction/consent ID from this input.
        assert_eq!(projected.result_core().service_contract, d(0));
        assert_eq!(projected.service_core().funding_consent, d(0));
        Ok(self.0.borrow().quote)
    }
    fn resolve_output(&self, carrier: &Digest, index: u32) -> Result<ResolvedFundingOutputV2, Self::Error> {
        let facts = self.0.borrow();
        if *carrier != facts.output.carrier || index != facts.output.index {
            return Err("different carrier or output index");
        }
        Ok(facts.output)
    }
}
fn funding_view(f: &Fixture) -> FundingView {
    FundingView(Rc::new(RefCell::new(FundingFacts {
        root: CourtAdmissionRootV2 { scope: f.result.scope, generation: 1, now: 1 },
        quote: RootedFundingQuoteV2 { context: f.context(), basis: f.basis, source_service_fee: 12 },
        output: ResolvedFundingOutputV2 { carrier: d(30), index: 1, amount: 110 },
    })))
}

#[test]
fn literal_payer_and_miner_signatures_bind_acyclic_final_links_without_a_lock_claim() {
    let f = Fixture::new();
    assert_ne!(principal(&f.miner_key), principal(&f.payer_key));
    assert_eq!(f.result.service_contract, f.service.id());
    assert_eq!(f.service.funding_consent, f.consent.id().0);
    assert_eq!(f.consent.scope, funding_scope(&f.result, &f.service, &f.basis));
    let view = funding_view(&f);
    let checker = FundingPreflightCoordinatorV2::new(view);
    let checked = checker.check(f.inputs(), &d(30)).unwrap();
    assert_eq!(checked.result(), &f.result);
    assert_eq!(checked.service(), &f.service);
    assert_eq!(checked.consent(), &f.consent);
    assert_eq!(checked.basis(), &f.basis);
    assert_eq!(checked.output().amount, 110);
    assert_eq!(checked.basis().required_fee + checked.basis().required_capital, 100);
    checker.revalidate(&checked, &d(30)).unwrap();
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}

#[test]
fn payer_authorization_cannot_be_forged_repurposed_or_detached_from_the_final_result() {
    for mode in 0..8 {
        let mut f = Fixture::new();
        let view = funding_view(&f);
        match mode {
            0 => f.miner_signature[0] ^= 1,
            1 => f.payer_signature[0] ^= 1,
            2 => f.payer_signature = sign(102, f.consent.id().0, RESULT_SIGN_CONTEXT),
            3 => f.payer_key = key(104),
            4 => {
                f.result.output = d(80);
                f.miner_signature = sign(101, f.result.id().0, RESULT_SIGN_CONTEXT);
            }
            5 => {
                f.result.service_contract = d(81);
                f.miner_signature = sign(101, f.result.id().0, RESULT_SIGN_CONTEXT);
            }
            6 => {
                f.service.funding_consent = d(82);
                f.result.service_contract = f.service.id();
                f.miner_signature = sign(101, f.result.id().0, RESULT_SIGN_CONTEXT);
            }
            _ => {
                f.consent.funding_output_index = 2;
                f.service.funding_consent = f.consent.id().0;
                f.result.service_contract = f.service.id();
                f.miner_signature = sign(101, f.result.id().0, RESULT_SIGN_CONTEXT);
                // Deliberately retain the payer's signature of output 1.
            }
        }
        assert!(FundingPreflightCoordinatorV2::new(view).check(f.inputs(), &d(30)).is_err(), "forgery mode {mode}");
    }
}

#[test]
fn fully_resigned_peer_policy_quote_or_schedule_cannot_replace_the_rooted_basis() {
    for mode in 0..5 {
        let mut f = Fixture::new();
        let view = funding_view(&f); // the canonical basis predates the attack
        match mode {
            0 => f.basis.policy = d(83),
            1 => f.basis.schedule = d(84),
            2 => f.basis.resource_quote = d(85),
            3 => f.basis.required_fee -= 1,
            _ => f.basis.mandatory_relations += 1,
        }
        f.refresh_links_and_signatures();
        assert!(FundingPreflightCoordinatorV2::new(view).check(f.inputs(), &d(30)).is_err(), "basis mode {mode}");
    }
}

#[test]
fn source_fee_court_fee_capital_actual_output_and_checked_add_are_separate_bounds() {
    for mode in 0..8 {
        let mut f = Fixture::new();
        match mode {
            0 => f.consent.max_fees = 29,
            1 => f.consent.max_locked = 99,
            5 => f.service.budget.fee = 40,
            6 => {
                f.basis.required_fee = u64::MAX;
                f.basis.required_capital = 1;
                f.consent.max_fees = u64::MAX;
                f.consent.max_locked = u64::MAX;
            }
            7 => f.basis.mandatory_relations = 0,
            _ => (),
        }
        f.refresh_links_and_signatures();
        let view = funding_view(&f);
        match mode {
            2 => view.0.borrow_mut().output.amount = 99,
            3 => view.0.borrow_mut().output.amount = 121,
            4 => view.0.borrow_mut().quote.source_service_fee = 21,
            5 => view.0.borrow_mut().quote.source_service_fee = 31,
            6 => view.0.borrow_mut().output.amount = u64::MAX,
            _ => (),
        }
        assert!(FundingPreflightCoordinatorV2::new(view).check(f.inputs(), &d(30)).is_err(), "amount mode {mode}");
    }
}

#[test]
fn funding_prepare_is_bound_to_current_root_quote_output_and_coordinator_owner() {
    for mode in 0..7 {
        let f = Fixture::new();
        let view = funding_view(&f);
        let checker = FundingPreflightCoordinatorV2::new(view.clone());
        let checked = checker.check(f.inputs(), &d(30)).unwrap();
        match mode {
            0 => view.0.borrow_mut().root.generation += 1,
            1 => view.0.borrow_mut().root.scope.branch = d(90),
            2 => view.0.borrow_mut().root.now = f.consent.admission_expiry,
            3 => view.0.borrow_mut().quote.basis.resource_quote = d(91),
            4 => view.0.borrow_mut().output.amount = 111, // still affordable, but a different prepared output
            5 => view.0.borrow_mut().output.carrier = d(92),
            _ => (),
        }
        if mode == 6 {
            let foreign = FundingPreflightCoordinatorV2::new(view.clone());
            assert_eq!(foreign.revalidate(&checked, &d(30)), Err(FundingPreflightErrorV2::ForeignCapability));
        } else {
            assert!(checker.revalidate(&checked, &d(30)).is_err(), "stale mode {mode}");
        }
    }
}

#[test]
fn admission_expiry_is_exclusive_but_does_not_turn_into_a_payer_authorized_refund() {
    let f = Fixture::new();
    let view = funding_view(&f);
    view.0.borrow_mut().root.now = f.consent.admission_expiry - 1;
    let checker = FundingPreflightCoordinatorV2::new(view.clone());
    let checked = checker.check(f.inputs(), &d(30)).unwrap();
    view.0.borrow_mut().root.now = f.consent.admission_expiry;
    assert!(checker.revalidate(&checked, &d(30)).is_err());
    assert!(checker.check(f.inputs(), &d(30)).is_err());
    // This module has produced only a signature/amount preflight; its API has
    // no refund, release, UTXO-lock or native-acceptance operation to invoke.
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}

#[path = "../../../../../consensus/core/src/palw_positive_court_v2/coordinator/funding_tests.rs"]
mod atomic_funding_tests;

#[test]
fn p_subject_attestation_body_is_bounded_and_its_literal_signature_is_not_draw_approval() {
    use misaka_palw_positive_contract::certification::{
        POSITIVE_ATTESTATION_BYTES_V2, POSITIVE_ATTESTATION_SIGN_CONTEXT_V2, PositiveAttestationExpectationsV2,
        PositiveCourtAttestationV2, PositiveCourtVerdictV2,
    };
    let f = Fixture::new();
    let verifier_key = key(104);
    let expected = PositiveAttestationExpectationsV2 {
        scope: f.result.scope,
        policy: d(61),
        draw_anchor: d(62),
        witness: witness(&f).id(),
        verifier_bond: d(63),
        verifier: principal(&verifier_key),
        checked_material: d(64),
    };
    for verdict in
        [PositiveCourtVerdictV2::PositiveChecked, PositiveCourtVerdictV2::Unavailable, PositiveCourtVerdictV2::NotReproducible]
    {
        let attestation = PositiveCourtAttestationV2 {
            version: 2,
            revision: 1,
            scope: f.result.scope,
            claim: f.result.claim_id(),
            result: f.result.id(),
            case: f.result.case_id(),
            subject: misaka_palw_positive_contract::native::certification_subject(&f.result),
            policy: d(61),
            draw_anchor: d(62),
            witness: witness(&f).id(),
            verifier_bond: d(63),
            verifier: principal(&verifier_key),
            verdict,
            checked_material: d(64),
        };
        let signature = sign(104, attestation.id(), POSITIVE_ATTESTATION_SIGN_CONTEXT_V2);
        assert_eq!(attestation.authenticate(&f.result, &expected, &verifier_key, &signature), Ok(()));
        let body = PositiveNativeBodyV2 {
            version: 2,
            revision: 1,
            operation: PositiveNativeOperationV2::PositiveCourtAttestation { attestation, signature },
        };
        let bytes = encode_native_body_v2(&body, body_limits()).unwrap();
        assert_eq!(bytes.len(), 5 + POSITIVE_ATTESTATION_BYTES_V2 + SIGNATURE_BYTES);
        assert_eq!(bytes[4], 7);
        let summary = scan_native_body_v2(&bytes, body_limits()).unwrap();
        assert_eq!((summary.signatures, summary.relations, summary.opaque_bytes), (1, 0, 0));
        assert_eq!(decode_native_body_v2(&bytes, body_limits()).unwrap(), body);
        for offset in [5, 7, 5 + 4 + 12 * 64] {
            let mut tampered = bytes.clone();
            tampered[offset] = 255;
            assert!(decode_native_body_v2(&tampered, body_limits()).is_err());
        }
        for end in [5, 5 + POSITIVE_ATTESTATION_BYTES_V2 - 1, bytes.len() - 1] {
            assert!(decode_native_body_v2(&bytes[..end], body_limits()).is_err());
        }
        let mut cap = body_limits();
        cap.max_body_bytes = bytes.len() as u64 - 1;
        assert!(decode_native_body_v2(&bytes, cap).is_err());
        cap = body_limits();
        cap.max_signatures = 0;
        assert!(decode_native_body_v2(&bytes, cap).is_err());
        // A different *valid* verdict remains syntax but the original literal
        // signature does not authenticate it. No Miner conviction is inferred.
        let mut changed = attestation;
        changed.verdict = if verdict == PositiveCourtVerdictV2::PositiveChecked {
            PositiveCourtVerdictV2::Unavailable
        } else {
            PositiveCourtVerdictV2::PositiveChecked
        };
        assert_eq!(changed.authenticate(&f.result, &expected, &verifier_key, &signature), Err(ContractError::Signature));
    }
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}
