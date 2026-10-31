use super::*;
use kaspa_consensus_core::{
    mldsa87_primitives::p2pkh_mldsa87_spk,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{TransactionInput, TransactionOutpoint, TransactionOutput},
};
use misaka_palw_positive_accounting::fees::*;
use misaka_palw_positive_contract::{
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
    provider: PrincipalId,
    keys: FeeRecipientKeys,
    authority: Authority,
    store: PrivateAccountingStore<Authority>,
    admitted: AdmittedCase,
    escrow: PalwFundedEscrowRowV1,
}
impl Fixture {
    fn new(burn: u64) -> Self {
        use libcrux_ml_dsa::ml_dsa_87::*;
        let miner = generate_key_pair([5; 32]);
        let miner_key = *miner.verification_key.as_ref();
        let payer = generate_key_pair([6; 32]);
        let payer_key = *payer.verification_key.as_ref();
        let provider = generate_key_pair([7; 32]);
        let provider_key = *provider.verification_key.as_ref();
        let provider = principal(&provider_key);
        let keys = FeeRecipientKeys::new([provider_key.as_slice()], 1).unwrap();
        let result = MinerResultCommit {
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
            service_contract: d(17),
        };
        let miner_signature = *sign(&miner.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        let intent = AdmissionIntent {
            scope: result.scope,
            case: result.case_id(),
            result: result.id(),
            resource_quote: d(30),
            funding_consent: d(31),
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
        let escrow = PalwFundedEscrowRowV1::fresh_v1(
            h(intent.funding_consent),
            h(result.claim_id().0),
            TransactionOutpoint::new(h(d(77)), 0),
            100,
            h(principal(&payer_key).0),
            kaspa_hashes::blake2b_512_address_payload(&payer_key),
            10,
            1000,
            true,
        )
        .unwrap();
        Self { result, miner_key, miner_signature, provider, keys, authority, store, admitted, escrow }
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
            &self.escrow,
            &self.keys,
            tx,
            &[self.escrow.unreleased_v1().unwrap()],
            daa,
            10,
        )
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
    assert_eq!(checked.recheck_rooted_before(&PalwChainStateV2::genesis()), Err(ExistingEscrowFeeError::MissingRootedEscrow));
    // Source signatures and a local checked proposal do not install a native row.
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
fn a_legacy_claim_or_changed_before_balance_cannot_borrow_the_new_result_fee_plan() {
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
        check_existing_escrow_fee_spend(&f.store, &auth, &projection, &f.escrow, &f.keys, &tx, &[100], 20, 0).err(),
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
