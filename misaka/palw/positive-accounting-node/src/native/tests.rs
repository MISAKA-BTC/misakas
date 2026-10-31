use super::*;
use kaspa_consensus_core::{
    mldsa87_primitives::p2pkh_mldsa87_spk,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{TransactionInput, TransactionOutpoint, TransactionOutput},
};
use misaka_palw_positive_accounting::fees::*;
use misaka_palw_positive_contract::{
    funding::{FundingConsent, FundingPlanBasis, funding_scope},
    identity::*,
    messages::*,
    ports::{AdmissionIntent, AtomicCourtStore},
};
use std::{cell::RefCell, rc::Rc};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn source(unit: u64) -> ServiceKey {
    ServiceKey::Source { material: d(20), kind: SourceKind::Input, unit }
}

#[derive(Clone)]
struct Authority(Rc<RefCell<Facts>>);
struct Facts {
    contract: ResolvedAdmission,
    source: BTreeMap<ServiceKey, ServiceDisposition>,
    closed: Option<ClaimStatus>,
}
impl AccountingAuthority for Authority {
    fn now(&self) -> u64 {
        20
    }
    fn admission(&self, intent: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
        let facts = self.0.borrow();
        if facts.contract.intent != *intent {
            return Err(AccountingError::Binding);
        }
        Ok(facts.contract.clone())
    }
    fn service(&self, intent: &AdmissionIntent, key: ServiceKey) -> Result<ServiceDisposition, AccountingError> {
        self.admission(intent)?;
        Ok(self.0.borrow().source.get(&key).copied().unwrap_or(ServiceDisposition::NotProvided))
    }
    fn closed_case(&self, intent: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        self.admission(intent)?;
        self.0.borrow().closed.ok_or(AccountingError::MissingCourtClosure)
    }
}
struct Fixture {
    result: MinerResultCommit,
    miner_key: [u8; PUBKEY_BYTES],
    miner_signature: [u8; SIGNATURE_BYTES],
    payer_key: [u8; PUBKEY_BYTES],
    funding_signature: [u8; SIGNATURE_BYTES],
    funding: FundingConsent,
    basis: FundingPlanBasis,
    service: MinerServiceContract,
    funding_carrier: Transaction,
    provider: PrincipalId,
    keys: FeeRecipientKeys,
    authority: Authority,
    store: PrivateAccountingStore<Authority>,
    admitted: AdmittedCase,
    escrow: PalwFundedEscrowRowV1,
    legacy_terms: FundingTermsV1,
}
impl Fixture {
    fn new(burn: u64) -> Self {
        Self::configured(burn, 100, 25, 25 + burn, 0, d(30))
    }
    fn configured(burn: u64, payer_cap: u64, source_cap: u64, required_fee: u64, capital: u64, quote: Digest) -> Self {
        use libcrux_ml_dsa::ml_dsa_87::*;
        let miner = generate_key_pair([5; 32]);
        let miner_key = *miner.verification_key.as_ref();
        let payer = generate_key_pair([6; 32]);
        let payer_key = *payer.verification_key.as_ref();
        let provider = generate_key_pair([7; 32]);
        let provider_key = *provider.verification_key.as_ref();
        let provider = principal(&provider_key);
        let keys = FeeRecipientKeys::new([provider_key.as_slice()], 1).unwrap();
        let mut result = MinerResultCommit {
            version: 2,
            revision: 1,
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            job: d(4),
            execution: d(5),
            miner: principal(&miner_key),
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
        let mut service = MinerServiceContract {
            version: 2,
            revision: 1,
            case: result.case_id(),
            source_catalog: d(20),
            provider_delegations: d(21),
            budget: AggregateBudget {
                rounds: 2,
                unique_source_bytes: 8192,
                transmitted_bytes: 16384,
                additional_work: 2,
                retained_bytes: 8192,
                retention_until: 1000,
                fee: source_cap,
                witness_attempts: 1,
            },
            funding_consent: d(0),
            deadline: 800,
        };
        let basis = FundingPlanBasis {
            profile: CheckProfile::ExactStreamV1,
            derivation: d(22),
            mandatory_relations: 10,
            source_catalog: service.source_catalog,
            resource_quote: quote,
            policy: d(33),
            schedule: d(32),
            required_fee,
            required_capital: capital,
        };
        let funding = FundingConsent {
            version: 2,
            revision: 1,
            case: result.case_id(),
            scope: funding_scope(&result, &service, &basis),
            payer: principal(&payer_key),
            max_fees: payer_cap,
            max_locked: 100,
            funding_output_index: 1,
            refund_payload: Digest(kaspa_hashes::blake2b_512_address_payload(&payer_key).as_bytes()),
            admission_expiry: 500,
        };
        let funding_signature = *sign(&payer.signing_key, &funding.id().0.0, FUNDING_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        service.funding_consent = funding.id().0;
        result.service_contract = service.id();
        let miner_signature = *sign(&miner.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        let intent = AdmissionIntent {
            scope: result.scope,
            case: result.case_id(),
            result: result.id(),
            resource_quote: d(30),
            funding_consent: funding.id().0,
            obligations: d(32),
            expiry: 500,
        };
        let contract = ResolvedAdmission {
            intent,
            payer: principal(&payer_key),
            fee_cap: 100,
            hold: 100,
            contractual_burn: burn,
            resource_units: 4,
            services: BTreeMap::from([
                (source(0), ServiceTerms { amount: 10, recipient: Recipient::Fixed(provider) }),
                (source(1), ServiceTerms { amount: 15, recipient: Recipient::Fixed(provider) }),
            ]),
        };
        let authority = Authority(Rc::new(RefCell::new(Facts { contract, source: BTreeMap::new(), closed: None })));
        let mut store = PrivateAccountingStore::new(
            authority.clone(),
            BTreeMap::from([(principal(&payer_key), 1000)]),
            AccountingLimits { max_accounts: 10, max_cases: 2, max_services_per_case: 10, resource_capacity: 10 },
        )
        .unwrap();
        let prepared = store.prepare_admission(intent).unwrap();
        let admitted = store.commit_admission(prepared).unwrap();
        // A literal OLD namespace operand for the existing arithmetic engine.
        // This is unrelated to new funding.id()/scope/result and grants no
        // Positive right to spend an old escrow or install a native row.
        let legacy_terms = FundingTermsV1 {
            version: 1,
            network_domain: h(d(1)),
            claim: h(d(78)),
            job_identity: h(d(79)),
            execution_root: h(d(80)),
            source_id: h(d(81)),
            relative_graph_id: h(d(82)),
            fee_profile_id: h(d(83)),
            capital_profile_id: h(d(84)),
            retention_profile_id: h(d(85)),
            settlement_policy_id: h(d(86)),
            accept_from_daa: 10,
            accept_until_daa: 500,
            material_until_daa: 20000,
            funding_output_index: 0,
            funding_amount_sompi: 100,
            payer_key_id: h(principal(&payer_key).0),
            refund_payload: h(funding.refund_payload),
            nonce: h(d(87)),
        };
        let escrow = PalwFundedEscrowRowV1::fresh_v1(
            legacy_terms.id_v1(),
            legacy_terms.claim,
            TransactionOutpoint::new(h(d(77)), 0),
            100,
            h(principal(&payer_key).0),
            kaspa_hashes::blake2b_512_address_payload(&payer_key),
            10,
            1000,
            true,
        )
        .unwrap();
        let funding_carrier = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(h(d(90)), 0), vec![], 0, 0)],
            vec![
                TransactionOutput::new(1, p2pkh_mldsa87_spk(&keys.payload(provider).unwrap().as_bytes())),
                TransactionOutput::new(100, p2pkh_mldsa87_spk(&h(funding.refund_payload).as_bytes())),
            ],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        Self {
            result,
            miner_key,
            miner_signature,
            payer_key,
            funding_signature,
            funding,
            basis,
            service,
            funding_carrier,
            provider,
            keys,
            authority,
            store,
            admitted,
            escrow,
            legacy_terms,
        }
    }
    fn deliver(&self, unit: u64) {
        self.authority.0.borrow_mut().source.insert(source(unit), ServiceDisposition::Delivered { recipient: self.provider });
    }
    fn payee(&self, amount: u64) -> TransactionOutput {
        TransactionOutput::new(amount, p2pkh_mldsa87_spk(&self.keys.payload(self.provider).unwrap().as_bytes()))
    }
    fn remainder(&self, amount: u64) -> TransactionOutput {
        TransactionOutput::new(amount, p2pkh_mldsa87_spk(&self.escrow.refund_payload.as_bytes()))
    }
    fn transaction(&self, outputs: Vec<TransactionOutput>) -> Transaction {
        Transaction::new(
            0,
            vec![TransactionInput::new(self.escrow.funding_outpoint, vec![], 0, 0)],
            outputs,
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        )
    }
    fn check(
        &self,
        projection: &FeeSettlementProjection,
        tx: &Transaction,
        daa: u64,
    ) -> Result<ExistingEscrowFeeCheck, ExistingEscrowFeeError> {
        check_existing_escrow_fee_spend(
            &self.store,
            &self.result.authenticate(&self.miner_key, &self.miner_signature).unwrap(),
            projection,
            &self.legacy_terms,
            &self.escrow,
            &self.keys,
            tx,
            &[self.escrow.unreleased_v1().unwrap()],
            daa,
            10,
        )
    }
    fn inspect<'a>(
        &self,
        projection: &FeeSettlementProjection,
        carrier: &'a Transaction,
        now: u64,
    ) -> Result<PayerSignedFeeOnlyOutput<'a>, FeeOnlyFundingError> {
        let result = self.result.authenticate(&self.miner_key, &self.miner_signature).unwrap();
        let funding =
            self.funding.authenticate(&self.result, &self.service, &self.basis, 20, &self.payer_key, &self.funding_signature).unwrap();
        inspect_signed_fee_only_funding(&self.store, &result, &funding, projection, carrier, now)
    }
}

#[test]
fn actual_native_engine_checks_a_source_fee_and_exact_continuation() {
    let f = Fixture::new(2);
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let tx = f.transaction(vec![f.payee(10), f.remainder(90)]);
    let checked = f.check(&projection, &tx, 20).unwrap();
    assert_eq!(checked.result(), f.result.id());
    assert_eq!((checked.plan().paid_now, checked.plan().continued_now), (10, 90));
    assert_eq!(checked.plan().new.unreleased_v1(), Ok(90));
    assert_ne!(f.keys.payload(f.provider).unwrap(), h(f.provider.0), "key IDs are not native addresses");
    assert_eq!(checked.plan().old.contract_id, f.legacy_terms.id_v1());
    assert_ne!(checked.plan().old.contract_id, h(f.funding.id().0));
    assert_ne!(checked.plan().old.claim, h(f.result.claim_id().0));
    // A private legacy arithmetic proposal does not install a Positive row.
    let wrong = f.transaction(vec![f.remainder(10), f.remainder(90)]);
    assert_eq!(f.check(&projection, &wrong, 20).err(), Some(ExistingEscrowFeeError::Escrow(PalwEscrowRefusalV1::PayeeOutputMismatch)));
}

#[test]
fn unverified_closure_pays_only_delivered_service_then_burn_and_refund() {
    let mut f = Fixture::new(2);
    f.deliver(0);
    f.authority.0.borrow_mut().closed = Some(ClaimStatus::EndedUnverified);
    let prepared = f.store.prepare_close(&f.admitted).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let tx = f.transaction(vec![f.payee(10), f.remainder(88)]);
    assert_eq!(f.check(&projection, &tx, 20).err(), Some(ExistingEscrowFeeError::LiabilityStillOpen));
    // This fixture explicitly models the existing native liability sweep's
    // resolution. The A5 checker itself cannot clear that rooted flag.
    f.escrow.liability_open = false;
    let checked = f.check(&projection, &tx, 20).unwrap();
    assert_eq!((checked.plan().paid_now, checked.plan().burned_now, checked.plan().returned_now), (10, 2, 88));
    assert!(checked.plan().new.settled_by.is_some());
    assert_eq!(checked.plan().new.unreleased_v1(), Ok(0));
}

#[test]
fn late_lower_source_unit_preserves_the_real_native_paid_prefix() {
    let mut f = Fixture::new(2);
    f.deliver(1);
    let first = f.store.prepare_service(&f.admitted, source(1)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &first).unwrap();
    let tx = f.transaction(vec![f.payee(15), f.remainder(85)]);
    let checked = f.check(&projection, &tx, 20).unwrap();
    f.escrow = checked.plan().new;
    f.store.apply_transition(&f.admitted, first).unwrap();
    f.deliver(0);
    let second = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &second).unwrap();
    assert_eq!(projection.payments().iter().map(|(key, _)| *key).collect::<Vec<_>>(), vec![source(1), source(0)]);
    let tx = f.transaction(vec![f.payee(10), f.remainder(75)]);
    let checked = f.check(&projection, &tx, 21).unwrap();
    assert_eq!((checked.plan().paid_now, checked.plan().new.paid_sompi, checked.plan().continued_now), (10, 25, 75));
}

#[test]
fn changed_legacy_operands_cannot_rewrite_the_private_engine_calculation() {
    let mut f = Fixture::new(2);
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let tx = f.transaction(vec![f.payee(10), f.remainder(90)]);
    let claim = f.escrow.claim;
    f.escrow.claim = h(d(90));
    assert_eq!(f.check(&projection, &tx, 20).err(), Some(ExistingEscrowFeeError::EscrowBinding));
    f.escrow.claim = claim;
    f.escrow.paid_sompi = 1;
    assert_eq!(f.check(&projection, &tx, 20).err(), Some(ExistingEscrowFeeError::Totals));
}

#[test]
fn hard_deadline_payer_return_cannot_silently_waive_an_earned_service_fee() {
    let mut f = Fixture::new(2);
    f.deliver(0);
    f.authority.0.borrow_mut().closed = Some(ClaimStatus::EndedUnverified);
    f.escrow.liability_open = false;
    let prepared = f.store.prepare_close(&f.admitted).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let tx = f.transaction(vec![f.remainder(100)]);
    assert_eq!(f.check(&projection, &tx, 1001).err(), Some(ExistingEscrowFeeError::Totals));
}

#[test]
fn empty_fee_timeout_uses_only_the_existing_deadline_return_path() {
    let mut f = Fixture::new(0);
    f.authority.0.borrow_mut().closed = Some(ClaimStatus::EndedUnverified);
    f.escrow.liability_open = false;
    let prepared = f.store.prepare_close(&f.admitted).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let tx = f.transaction(vec![f.remainder(100)]);
    assert_eq!(
        f.check(&projection, &tx, 20).err(),
        Some(ExistingEscrowFeeError::Escrow(PalwEscrowRefusalV1::SettlementScheduleMissing))
    );
    assert_eq!(f.check(&projection, &tx, 1001).unwrap().plan().returned_now, 100);
}

#[test]
fn unknown_or_malformed_recipient_keys_and_unbounded_due_counts_are_refused() {
    assert_eq!(FeeRecipientKeys::new([&[0u8; 2][..]], 1).err(), Some(ExistingEscrowFeeError::RecipientKey));
    let f = Fixture::new(2);
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let tx = f.transaction(vec![f.payee(10), f.remainder(90)]);
    let auth = f.result.authenticate(&f.miner_key, &f.miner_signature).unwrap();
    assert_eq!(
        check_existing_escrow_fee_spend(
            &f.store,
            &auth,
            &projection,
            &f.legacy_terms,
            &f.escrow,
            &FeeRecipientKeys::new([], 0).unwrap(),
            &tx,
            &[100],
            20,
            10
        )
        .err(),
        Some(ExistingEscrowFeeError::RecipientKey)
    );
    assert_eq!(
        check_existing_escrow_fee_spend(&f.store, &auth, &projection, &f.legacy_terms, &f.escrow, &f.keys, &tx, &[100], 20, 0).err(),
        Some(ExistingEscrowFeeError::Budget)
    );
}

#[test]
fn changing_outputs_without_refreshing_the_actual_transaction_identity_is_rejected() {
    let f = Fixture::new(2);
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let mut tx = f.transaction(vec![f.remainder(10), f.remainder(90)]);
    tx.outputs[0] = f.payee(10);
    assert!(!tx.id_is_current());
    assert_eq!(f.check(&projection, &tx, 20).err(), Some(ExistingEscrowFeeError::TransactionIdentity));
}

#[test]
fn an_old_projection_cannot_reenter_the_native_checker_after_payment_or_rollback() {
    let mut f = Fixture::new(2);
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let tx = f.transaction(vec![f.payee(10), f.remainder(90)]);
    let undo = f.store.apply_transition(&f.admitted, prepared).unwrap();
    assert_eq!(f.check(&projection, &tx, 20).err(), Some(ExistingEscrowFeeError::Accounting(AccountingError::StalePreparation)));
    f.store.rollback(undo).unwrap();
    assert_eq!(f.check(&projection, &tx, 20).err(), Some(ExistingEscrowFeeError::Accounting(AccountingError::StalePreparation)));
}

#[test]
fn real_acyclic_payer_signature_and_actual_output_preflight_are_distinct_from_native_admission() {
    let f = Fixture::new(2);
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let checked = f.inspect(&projection, &f.funding_carrier, 20).unwrap();
    assert_eq!(checked.consent(), f.funding.id());
    assert_eq!(checked.scope(), funding_scope(&f.result, &f.service, &f.basis));
    assert_eq!(checked.result(), f.result.id());
    assert_eq!(checked.outpoint(), TransactionOutpoint::new(f.funding_carrier.id(), 1));
    assert_eq!(checked.amount(), 100);
    assert_eq!(checked.require_native_admission(), Err(ContractError::NativeAdapterUnassigned));
    // A new consent ID is never installed as an old FundingTerms ID. The two
    // separate private checks cannot grant a Positive claim an old-row spend.
    assert_ne!(h(checked.consent().0), f.escrow.contract_id);
    assert_eq!(f.escrow.contract_id, f.legacy_terms.id_v1());
}

#[test]
fn full_planned_source_ceiling_and_court_ceiling_use_different_signed_caps() {
    for (fixture, expected) in [
        (Fixture::configured(2, 100, 24, 27, 0, d(30)), FeeOnlyFundingError::MinerSourceFeeCap),
        (Fixture::configured(2, 50, 25, 27, 0, d(30)), FeeOnlyFundingError::PayerFeeCap),
        (Fixture::configured(2, 100, 25, 26, 0, d(30)), FeeOnlyFundingError::QuotedFeeCeiling),
    ] {
        // Only unit 0 (10) has been delivered, but all scheduled source units
        // together cost 25. A delivered-only check would miss the first case.
        fixture.deliver(0);
        let prepared = fixture.store.prepare_service(&fixture.admitted, source(0)).unwrap();
        let projection = fixture.store.settlement_projection(&fixture.admitted, &prepared).unwrap();
        assert_eq!(fixture.inspect(&projection, &fixture.funding_carrier, 20).err(), Some(expected));
    }
}

#[test]
fn capital_bearing_funding_and_different_resource_quotes_are_explicitly_refused() {
    for (fixture, expected) in [
        (Fixture::configured(2, 100, 25, 27, 1, d(30)), FeeOnlyFundingError::UnsupportedCapital),
        (Fixture::configured(2, 100, 25, 27, 0, d(99)), FeeOnlyFundingError::IntentBinding),
    ] {
        fixture.deliver(0);
        let prepared = fixture.store.prepare_service(&fixture.admitted, source(0)).unwrap();
        let projection = fixture.store.settlement_projection(&fixture.admitted, &prepared).unwrap();
        assert_eq!(fixture.inspect(&projection, &fixture.funding_carrier, 20).err(), Some(expected));
    }
}

#[test]
fn actual_carrier_value_refund_index_and_admission_expiry_are_checked_again() {
    let f = Fixture::new(2);
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    let mut value = f.funding_carrier.clone();
    value.outputs[1].value = 99;
    value.finalize();
    assert_eq!(f.inspect(&projection, &value, 20).err(), Some(FeeOnlyFundingError::Output));
    let mut refund = f.funding_carrier.clone();
    refund.outputs[1] = f.payee(100);
    refund.finalize();
    assert_eq!(f.inspect(&projection, &refund, 20).err(), Some(FeeOnlyFundingError::Output));
    let mut missing = f.funding_carrier.clone();
    missing.outputs.pop();
    missing.finalize();
    assert_eq!(f.inspect(&projection, &missing, 20).err(), Some(FeeOnlyFundingError::Carrier(FundingRefusalV1::OutputMissing)));
    assert_eq!(f.inspect(&projection, &f.funding_carrier, 500).err(), Some(FeeOnlyFundingError::Contract(ContractError::Budget)));
    let mut stale = f.funding_carrier.clone();
    stale.outputs[1] = f.payee(100);
    assert_eq!(f.inspect(&projection, &stale, 20).err(), Some(FeeOnlyFundingError::Carrier(FundingRefusalV1::CarrierIdentity)));
}

#[test]
fn actual_payer_signatures_reject_substitution_even_after_acyclic_links_are_recomputed() {
    let f = Fixture::new(2);
    let mut bad_signature = f.funding_signature;
    bad_signature[0] ^= 1;
    assert!(matches!(
        f.funding.authenticate(&f.result, &f.service, &f.basis, 20, &f.payer_key, &bad_signature),
        Err(ContractError::Signature)
    ));
    for field in 0..4 {
        let mut funding = f.funding.clone();
        match field {
            0 => funding.refund_payload = d(99),
            1 => funding.funding_output_index = 0,
            2 => funding.max_fees = 99,
            _ => funding.admission_expiry = 499,
        }
        let mut service = f.service;
        service.funding_consent = funding.id().0;
        let mut result = f.result.clone();
        result.service_contract = service.id();
        // Structure still closes without a hash cycle, but neither party
        // signed these newly linked bytes.
        assert_eq!(funding.validate(&result, &service, &f.basis, 20), Ok(()));
        assert!(matches!(
            funding.authenticate(&result, &service, &f.basis, 20, &f.payer_key, &f.funding_signature),
            Err(ContractError::Signature)
        ));
        assert!(matches!(result.authenticate(&f.miner_key, &f.miner_signature), Err(ContractError::Signature)));
    }
}

#[test]
fn a_narrower_admission_intent_deadline_remains_exclusive() {
    let mut f = Fixture::new(2);
    let undo = f.admitted.into_admission_undo().unwrap();
    f.store.rollback(undo).unwrap();
    let intent = {
        let mut facts = f.authority.0.borrow_mut();
        facts.contract.intent.expiry = 400;
        facts.contract.intent
    };
    let admission = f.store.prepare_admission(intent).unwrap();
    f.admitted = f.store.commit_admission(admission).unwrap();
    f.deliver(0);
    let prepared = f.store.prepare_service(&f.admitted, source(0)).unwrap();
    let projection = f.store.settlement_projection(&f.admitted, &prepared).unwrap();
    assert_eq!(f.funding.admission_expiry, 500);
    assert!(f.inspect(&projection, &f.funding_carrier, 399).is_ok());
    for now in [400, 450] {
        assert_eq!(f.inspect(&projection, &f.funding_carrier, now).err(), Some(FeeOnlyFundingError::IntentBinding));
    }
}
