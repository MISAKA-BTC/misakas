//! Public-API integration with A5's actual PRIVATE accounting store. Carrier
//! facts and balances are explicit test inputs, never native UTXO/lock evidence.
//! This file may also be included under the fresh exec integration driver.
use kaspa_consensus_core::palw_positive_court_v2::coordinator::{
    admission::{
        AtomicAdmissionCoordinatorV2, CourtAdmissionErrorV2, CourtAdmissionRootV2, CourtAdmissionViewV2, ResolvedCourtAdmissionV2,
    },
    funding::*,
};
use misaka_palw_positive_accounting::fees::{
    AccountingAuthority, AccountingError, AccountingLimits, PrivateAccountingStore, ResolvedAdmission, UnassignedAccountingAuthority,
};
use misaka_palw_positive_contract::{
    ContractError,
    funding::{FundingConsent, FundingPlanBasis, funding_scope},
    identity::{ChainScope, Digest, FUNDING_SIGN_CONTEXT, PUBKEY_BYTES, PrincipalId, RESULT_SIGN_CONTEXT, SIGNATURE_BYTES, principal},
    messages::{AdmissionContext, AggregateBudget, CheckProfile, ClaimForm, MinerResultCommit, MinerServiceContract},
    ports::AdmissionIntent,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
fn fd(n: u8) -> Digest {
    Digest([n; 64])
}
struct FundingFixture {
    result: MinerResultCommit,
    service: MinerServiceContract,
    consent: FundingConsent,
    miner_key: [u8; PUBKEY_BYTES],
    payer_key: [u8; PUBKEY_BYTES],
    miner_signature: [u8; SIGNATURE_BYTES],
    payer_signature: [u8; SIGNATURE_BYTES],
}
impl FundingFixture {
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
}
#[derive(Clone)]
struct TestCarrier {
    id: Digest,
    amount: u64,
}
struct FundingFacts {
    root: CourtAdmissionRootV2,
    quote: RootedFundingQuoteV2,
    admission: ResolvedCourtAdmissionV2,
    accounting: ResolvedAdmission,
}
#[derive(Clone)]
struct FundingView(Rc<RefCell<FundingFacts>>);
impl RootedFundingViewV2 for FundingView {
    type Error = &'static str;
    type Carrier = TestCarrier;
    fn root(&self) -> CourtAdmissionRootV2 {
        self.0.borrow().root
    }
    fn resolve_quote(&self, input: &FundingBasisInputV2) -> Result<RootedFundingQuoteV2, Self::Error> {
        assert_eq!(input.result_core().service_contract, Digest([0; 64]));
        assert_eq!(input.service_core().funding_consent, Digest([0; 64]));
        Ok(self.0.borrow().quote)
    }
    fn resolve_output(&self, carrier: &TestCarrier, index: u32) -> Result<ResolvedFundingOutputV2, Self::Error> {
        if index != 1 {
            return Err("actual carrier output absent");
        }
        Ok(ResolvedFundingOutputV2 { carrier: carrier.id, index, amount: carrier.amount })
    }
}
impl CourtAdmissionViewV2 for FundingView {
    type Error = &'static str;
    fn root(&self) -> CourtAdmissionRootV2 {
        self.0.borrow().root
    }
    fn resolve(&self, _: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
        Ok(self.0.borrow().admission)
    }
    fn resolve_committed(&self, _: &AdmissionIntent, _: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
        // This private accounting fixture has no native Case/anchor/deadline
        // resolver. New-admission tests must not manufacture such authority.
        Err("committed-case native history adapter unassigned in fixture")
    }
}
impl AccountingAuthority for FundingView {
    fn now(&self) -> u64 {
        self.0.borrow().root.now
    }
    fn admission(&self, intent: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
        let facts = self.0.borrow();
        if facts.accounting.intent != *intent {
            return Err(AccountingError::MissingConsentOrFunding);
        }
        Ok(facts.accounting.clone())
    }
}
fn funding_fixture() -> (FundingFixture, FundingView, TestCarrier) {
    let miner = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([13; 32]);
    let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([14; 32]);
    let miner_key: [u8; PUBKEY_BYTES] = *miner.verification_key.as_ref();
    let payer_key: [u8; PUBKEY_BYTES] = *payer.verification_key.as_ref();
    let scope = ChainScope { network: fd(1), ruleset: fd(2), branch: fd(3) };
    let mut result = MinerResultCommit {
        version: 2,
        revision: 1,
        scope,
        job: fd(4),
        execution: fd(5),
        miner: principal(&miner_key),
        registered_class: fd(6),
        artifact: fd(7),
        program: fd(8),
        input: fd(9),
        tokenizer: fd(10),
        execution_rules: fd(11),
        selector: fd(12),
        generation_seed: [0; 32],
        temperature_q24: 0,
        prefill: 2,
        decode: 2,
        max_context: 4,
        output: fd(13),
        dependencies: fd(14),
        service_contract: Digest([0; 64]),
    };
    let mut service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: result.case_id(),
        source_catalog: fd(16),
        provider_delegations: fd(17),
        budget: AggregateBudget {
            rounds: 1,
            unique_source_bytes: 4096,
            transmitted_bytes: 8192,
            additional_work: 1,
            retained_bytes: 4096,
            retention_until: 30,
            fee: 40,
            witness_attempts: 2,
        },
        funding_consent: Digest([0; 64]),
        deadline: 20,
    };
    let basis = FundingPlanBasis {
        profile: CheckProfile::ExactStreamV1,
        derivation: fd(18),
        mandatory_relations: 9,
        source_catalog: service.source_catalog,
        resource_quote: fd(19),
        policy: fd(20),
        schedule: fd(21),
        required_fee: 50,
        required_capital: 100,
    };
    let consent = FundingConsent {
        version: 2,
        revision: 1,
        case: result.case_id(),
        scope: funding_scope(&result, &service, &basis),
        payer: principal(&payer_key),
        max_fees: 60,
        max_locked: 200,
        funding_output_index: 1,
        refund_payload: fd(22),
        admission_expiry: 15,
    };
    service.funding_consent = consent.id().0;
    result.service_contract = service.id();
    let payer_signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&payer.signing_key, &consent.id().0.0, FUNDING_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    let miner_signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&miner.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    let context = AdmissionContext {
        scope,
        expected_job: result.job,
        expected_execution: result.execution,
        expected_miner: result.miner,
        expected_input: result.input,
        registered_class: result.registered_class,
        artifact: result.artifact,
        program: result.program,
        tokenizer: result.tokenizer,
        execution_rules: result.execution_rules,
        selector: result.selector,
        required_form: ClaimForm::PositiveResultV2,
    };
    let intent = AdmissionIntent {
        scope,
        case: result.case_id(),
        result: result.id(),
        resource_quote: basis.resource_quote,
        funding_consent: consent.id().0,
        obligations: fd(23),
        expiry: consent.admission_expiry,
    };
    let view = FundingView(Rc::new(RefCell::new(FundingFacts {
        root: CourtAdmissionRootV2 { scope, generation: 1, now: 1 },
        quote: RootedFundingQuoteV2 { context, basis, source_service_fee: 20 },
        admission: ResolvedCourtAdmissionV2 { context, service, intent },
        accounting: ResolvedAdmission {
            intent,
            payer: consent.payer,
            fee_cap: consent.max_fees,
            hold: basis.required_fee,
            contractual_burn: 0,
            resource_units: 4,
            services: BTreeMap::new(),
        },
    })));
    let fixture = FundingFixture { result, service, consent, miner_key, payer_key, miner_signature, payer_signature };
    (fixture, view, TestCarrier { id: fd(24), amount: 150 })
}
fn accounting(
    view: &FundingView,
    payer: PrincipalId,
) -> AtomicAdmissionCoordinatorV2<PrivateAccountingStore<FundingView>, FundingView> {
    let store = PrivateAccountingStore::new(
        view.clone(),
        BTreeMap::from([(payer, 1000)]),
        AccountingLimits { max_accounts: 4, max_cases: 4, max_services_per_case: 4, resource_capacity: 10 },
    )
    .unwrap();
    AtomicAdmissionCoordinatorV2::new(store, view.clone())
}

#[test]
fn funding_atomic_prepare_is_read_only_and_actual_store_idempotence_preserves_one_fee_hold() {
    let (f, view, carrier) = funding_fixture();
    let funding = FundingPreflightCoordinatorV2::new(view.clone());
    let mut admission = accounting(&view, f.consent.payer);
    let prepared = funding.prepare_atomic(&admission, f.inputs(), &carrier).unwrap();
    assert_eq!(admission.store().balance(f.consent.payer), 1000);
    assert_eq!(admission.store().reserved_resources(), 0);
    let admitted = funding.commit_atomic(&mut admission, prepared, &carrier).unwrap();
    assert_eq!(admitted.funding().basis().required_fee, 50);
    assert_eq!(admitted.funding().basis().required_capital, 100);
    assert_eq!(admitted.funding().output().amount, 150);
    assert_eq!(admitted.admitted().intent().funding_consent, f.consent.id().0);
    // Actual native capital is NOT a private fee-book balance or a coin lock.
    assert_eq!(admission.store().totals(f.result.case_id()).unwrap().initial, 50);
    let retry = funding.prepare_atomic(&admission, f.inputs(), &carrier).unwrap();
    funding.commit_atomic(&mut admission, retry, &carrier).unwrap();
    assert_eq!(admission.store().balance(f.consent.payer), 950);
    assert_eq!(admission.store().reserved_resources(), 4);
    assert!(admission.store().conserved());
}

#[test]
fn funding_atomic_rechecks_generation_quote_actual_output_expiry_and_real_store_terms() {
    for mode in 0..8 {
        let (f, view, mut carrier) = funding_fixture();
        let funding = FundingPreflightCoordinatorV2::new(view.clone());
        let mut admission = accounting(&view, f.consent.payer);
        let prepared = funding.prepare_atomic(&admission, f.inputs(), &carrier).unwrap();
        match mode {
            0 => view.0.borrow_mut().root.generation += 1,
            1 => carrier.amount += 1,
            2 => view.0.borrow_mut().quote.basis.policy = fd(99),
            3 => view.0.borrow_mut().admission.intent.resource_quote = fd(99),
            4 => view.0.borrow_mut().accounting.hold -= 1,
            5 => view.0.borrow_mut().root.now = f.consent.admission_expiry,
            6 => carrier.id = fd(99),
            _ => view.0.borrow_mut().quote.source_service_fee += 1,
        }
        assert!(funding.commit_atomic(&mut admission, prepared, &carrier).is_err(), "mode {mode}");
        assert_eq!(admission.store().balance(f.consent.payer), 1000);
        assert_eq!(admission.store().reserved_resources(), 0);
        assert_eq!(admission.store().totals(f.result.case_id()), None);
        assert!(admission.store().conserved());
    }
}

#[test]
fn funding_atomic_rejects_different_root_quote_or_consent_intent_before_admission() {
    for mode in 0..4 {
        let (f, view, carrier) = funding_fixture();
        let funding = FundingPreflightCoordinatorV2::new(view.clone());
        let mut wrong = view.0.borrow().admission;
        match mode {
            0 => wrong.intent.resource_quote = fd(99),
            1 => wrong.intent.funding_consent = fd(99),
            2 => wrong.intent.expiry = f.consent.admission_expiry + 1,
            _ => wrong.service.provider_delegations = fd(99),
        }
        {
            let mut facts = view.0.borrow_mut();
            facts.admission = wrong;
            facts.accounting.intent = wrong.intent;
        }
        let admission = accounting(&view, f.consent.payer);
        assert!(funding.prepare_atomic(&admission, f.inputs(), &carrier).is_err());
        assert_eq!(admission.store().balance(f.consent.payer), 1000);
        assert_eq!(admission.store().reserved_resources(), 0);
    }
}

#[test]
fn funding_atomic_rejects_foreign_capability_and_preserves_native_unassigned_error() {
    let (f, view, carrier) = funding_fixture();
    let funding = FundingPreflightCoordinatorV2::new(view.clone());
    let other = FundingPreflightCoordinatorV2::new(view.clone());
    let mut admission = accounting(&view, f.consent.payer);
    let prepared = funding.prepare_atomic(&admission, f.inputs(), &carrier).unwrap();
    assert!(matches!(
        other.commit_atomic(&mut admission, prepared, &carrier),
        Err(FundingAtomicErrorV2::Funding(FundingPreflightErrorV2::ForeignCapability))
    ));
    assert_eq!(admission.store().balance(f.consent.payer), 1000);
    let store = PrivateAccountingStore::new(
        UnassignedAccountingAuthority,
        BTreeMap::from([(f.consent.payer, 1000)]),
        AccountingLimits { max_accounts: 4, max_cases: 4, max_services_per_case: 4, resource_capacity: 10 },
    )
    .unwrap();
    let admission = AtomicAdmissionCoordinatorV2::new(store, view);
    assert!(matches!(
        funding.prepare_atomic(&admission, f.inputs(), &carrier),
        Err(FundingAtomicErrorV2::Admission(CourtAdmissionErrorV2::Store(AccountingError::NativeAdapterUnassigned)))
    ));
    assert_eq!(admission.store().balance(f.consent.payer), 1000);
}

fn funding_body(
    f: &FundingFixture,
    basis: FundingPlanBasis,
) -> kaspa_consensus_core::palw_positive_court_v2::coordinator::native_body::PositiveNativeBodyV2 {
    use kaspa_consensus_core::palw_positive_court_v2::coordinator::native_body::*;
    PositiveNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveNativeOperationV2::ResultAdmission {
            result: f.result.clone(),
            miner_signature: f.miner_signature,
            service: f.service,
            basis,
            consent: f.consent.clone(),
            payer_signature: f.payer_signature,
        },
    }
}

#[test]
fn native_result_body_uses_rooted_keys_and_rejects_other_kinds_or_versions() {
    use kaspa_consensus_core::palw_positive_court_v2::coordinator::native_body::PositiveNativeOperationV2;
    let (f, view, carrier) = funding_fixture();
    let basis = view.0.borrow().quote.basis;
    let funding = FundingPreflightCoordinatorV2::new(view);
    let body = funding_body(&f, basis);
    let checked = funding.check_native_admission(&body, &f.miner_key, &f.payer_key, &carrier).unwrap();
    assert_eq!(checked.result(), &f.result);
    assert_eq!(checked.basis(), &basis);
    assert_eq!(checked.output().amount, 150);
    let mut wrong = body.clone();
    wrong.version = 1;
    assert!(matches!(
        funding.check_native_admission(&wrong, &f.miner_key, &f.payer_key, &carrier),
        Err(FundingPreflightErrorV2::Contract(ContractError::Version))
    ));
    wrong = body.clone();
    wrong.revision = 0;
    assert!(matches!(
        funding.check_native_admission(&wrong, &f.miner_key, &f.payer_key, &carrier),
        Err(FundingPreflightErrorV2::Contract(ContractError::Version))
    ));
    wrong = body.clone();
    wrong.operation = PositiveNativeOperationV2::Expire { case: f.result.case_id(), result: f.result.id() };
    assert!(matches!(
        funding.check_native_admission(&wrong, &f.miner_key, &f.payer_key, &carrier),
        Err(FundingPreflightErrorV2::NativeBodyKind)
    ));
    assert!(funding.check_native_admission(&body, &f.payer_key, &f.payer_key, &carrier).is_err());
    assert!(funding.check_native_admission(&body, &f.miner_key, &f.miner_key, &carrier).is_err());
}

#[test]
fn native_body_peer_basis_must_equal_every_independently_derived_field() {
    let (f, view, carrier) = funding_fixture();
    let basis = view.0.borrow().quote.basis;
    let funding = FundingPreflightCoordinatorV2::new(view.clone());
    let admission = accounting(&view, f.consent.payer);
    for mode in 0..9 {
        let mut declared = basis;
        match mode {
            0 => declared.mandatory_relations += 1,
            1 => declared.derivation = fd(99),
            2 => declared.source_catalog = fd(99),
            3 => declared.resource_quote = fd(99),
            4 => declared.policy = fd(99),
            5 => declared.schedule = fd(99),
            6 => declared.required_fee += 1,
            7 => declared.required_capital += 1,
            _ => declared.profile = CheckProfile::FullRelationDirectV1,
        }
        let body = funding_body(&f, declared);
        assert!(
            matches!(
                funding.check_native_admission(&body, &f.miner_key, &f.payer_key, &carrier),
                Err(FundingPreflightErrorV2::PeerBasis)
            ),
            "mode {mode}"
        );
        assert!(matches!(
            funding.prepare_native_atomic(&admission, &body, &f.miner_key, &f.payer_key, &carrier),
            Err(FundingAtomicErrorV2::Funding(FundingPreflightErrorV2::PeerBasis))
        ));
    }
    assert_eq!(admission.store().balance(f.consent.payer), 1000);
    assert_eq!(admission.store().reserved_resources(), 0);
    assert_eq!(admission.store().totals(f.result.case_id()), None);
}

#[test]
fn native_body_preflight_prepares_the_same_atomic_store_and_rechecks_actual_output() {
    let (f, view, mut carrier) = funding_fixture();
    let basis = view.0.borrow().quote.basis;
    let funding = FundingPreflightCoordinatorV2::new(view.clone());
    let mut admission = accounting(&view, f.consent.payer);
    let body = funding_body(&f, basis);
    let prepared = funding.prepare_native_atomic(&admission, &body, &f.miner_key, &f.payer_key, &carrier).unwrap();
    assert_eq!(admission.store().balance(f.consent.payer), 1000);
    carrier.amount = 151;
    assert!(matches!(
        funding.commit_atomic(&mut admission, prepared, &carrier),
        Err(FundingAtomicErrorV2::Funding(FundingPreflightErrorV2::OutputChanged))
    ));
    assert_eq!(admission.store().totals(f.result.case_id()), None);
    carrier.amount = 150;
    let prepared = funding.prepare_native_atomic(&admission, &body, &f.miner_key, &f.payer_key, &carrier).unwrap();
    let admitted = funding.commit_atomic(&mut admission, prepared, &carrier).unwrap();
    assert_eq!(admitted.funding().basis(), &basis);
    assert_eq!(admitted.admitted().intent().result, f.result.id());
    assert_eq!(admission.store().balance(f.consent.payer), 950);
    assert_eq!(admission.store().reserved_resources(), 4);
    assert!(admission.store().conserved());
    // This success is only the explicitly provisioned PRIVATE store. It did
    // not create native UTXO-lock, lifecycle or settlement authorization.
}
