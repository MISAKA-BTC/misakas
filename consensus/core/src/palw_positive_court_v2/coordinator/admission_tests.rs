//! Integration tests use A5's real private store implementation. Explicit test
//! balances/authority are not a native funding or storage adapter.
use super::*;
use misaka_palw_positive_accounting::fees::{
    AccountingAuthority, AccountingError, AccountingLimits, PrivateAccountingStore, ResolvedAdmission, UnassignedAccountingAuthority,
};
use misaka_palw_positive_contract::{identity::*, messages::*};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
struct Signed {
    result: MinerResultCommit,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
}
fn sign_result(result: &MinerResultCommit) -> [u8; SIGNATURE_BYTES] {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([7; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref()
}
struct Facts {
    root: CourtAdmissionRootV2,
    resolved: ResolvedCourtAdmissionV2,
    fee: ResolvedAdmission,
    closed: bool,
}
#[derive(Clone)]
struct View(Rc<RefCell<Facts>>);
impl CourtAdmissionViewV2 for View {
    type Error = &'static str;
    fn root(&self) -> CourtAdmissionRootV2 {
        self.0.borrow().root
    }
    fn resolve(&self, _: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
        Ok(self.0.borrow().resolved)
    }
    fn resolve_committed(&self, _: &AdmissionIntent, _: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
        // This private accounting fixture has no native Case/anchor/deadline
        // resolver. New-admission tests must not manufacture such authority.
        Err("committed-case native history adapter unassigned in fixture")
    }
}
impl AccountingAuthority for View {
    fn now(&self) -> u64 {
        self.0.borrow().root.now
    }
    fn admission(&self, intent: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
        let facts = self.0.borrow();
        if facts.fee.intent != *intent {
            return Err(AccountingError::MissingConsentOrFunding);
        }
        Ok(facts.fee.clone())
    }
    fn closed_case(&self, _: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        if self.0.borrow().closed { Ok(ClaimStatus::EndedUnverified) } else { Err(AccountingError::MissingCourtClosure) }
    }
}
fn fixture() -> (Signed, View) {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([7; 32]);
    let key: [u8; PUBKEY_BYTES] = *key.verification_key.as_ref();
    let scope = ChainScope { network: d(1), ruleset: d(2), branch: d(3) };
    let mut result = MinerResultCommit {
        version: 2,
        revision: 1,
        scope,
        job: d(4),
        execution: d(5),
        miner: principal(&key),
        registered_class: d(6),
        artifact: d(7),
        program: d(8),
        input: d(9),
        tokenizer: d(10),
        execution_rules: d(11),
        selector: d(12),
        generation_seed: [0; 32],
        temperature_q24: 0,
        prefill: 2,
        decode: 2,
        max_context: 4,
        output: d(13),
        dependencies: d(14),
        service_contract: d(15),
    };
    let service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: result.case_id(),
        source_catalog: d(16),
        provider_delegations: d(17),
        budget: AggregateBudget {
            rounds: 1,
            unique_source_bytes: 4096,
            transmitted_bytes: 8192,
            additional_work: 1,
            retained_bytes: 4096,
            retention_until: 120,
            fee: 100,
            witness_attempts: 2,
        },
        funding_consent: d(18),
        deadline: 100,
    };
    result.service_contract = service.id();
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
        resource_quote: d(19),
        funding_consent: service.funding_consent,
        obligations: d(20),
        expiry: 90,
    };
    let fee = ResolvedAdmission {
        intent,
        payer: result.miner,
        fee_cap: 100,
        hold: 100,
        contractual_burn: 0,
        resource_units: 4,
        services: BTreeMap::new(),
    };
    let view = View(Rc::new(RefCell::new(Facts {
        root: CourtAdmissionRootV2 { scope, generation: 1, now: 10 },
        resolved: ResolvedCourtAdmissionV2 { context, service, intent },
        fee,
        closed: false,
    })));
    let signature = sign_result(&result);
    (Signed { result, key, signature }, view)
}
fn coordinator(v: &View) -> AtomicAdmissionCoordinatorV2<PrivateAccountingStore<View>, View> {
    let payer = v.0.borrow().fee.payer;
    let store = PrivateAccountingStore::new(
        v.clone(),
        BTreeMap::from([(payer, 1000)]),
        AccountingLimits { max_accounts: 4, max_cases: 4, max_services_per_case: 4, resource_capacity: 10 },
    )
    .unwrap();
    AtomicAdmissionCoordinatorV2::new(store, v.clone())
}
fn prepare(
    c: &AtomicAdmissionCoordinatorV2<PrivateAccountingStore<View>, View>,
    s: &Signed,
) -> PreparedCourtAdmissionV2<<PrivateAccountingStore<View> as AtomicCourtStore>::PreparedAdmission> {
    c.prepare(s.result.clone(), &s.key, &s.signature).unwrap()
}
#[test]
fn prepare_is_not_admission_and_identical_retries_take_exactly_one_hold() {
    let (s, v) = fixture();
    let mut c = coordinator(&v);
    let payer = s.result.miner;
    let a = prepare(&c, &s);
    let retry = prepare(&c, &s);
    assert_eq!(c.store().balance(payer), 1000);
    assert_eq!(c.store().reserved_resources(), 0);
    let admitted = c.commit(a).unwrap();
    let retried = c.commit(retry).unwrap();
    assert_eq!(admitted.intent(), v.0.borrow().resolved.intent);
    assert_eq!(retried.intent(), admitted.intent());
    assert_eq!(c.store().balance(payer), 900);
    assert_eq!(c.store().reserved_resources(), 4);
    assert!(c.store().conserved());
    assert!(retried.into_store_case().into_admission_undo().is_none());
    let undo = admitted.into_store_case().into_admission_undo().unwrap();
    let (mut store, _) = c.into_parts();
    store.rollback(undo).unwrap();
    assert_eq!(store.balance(payer), 1000);
    assert_eq!(store.reserved_resources(), 0);
}
#[test]
fn root_generation_branch_clock_and_changed_obligations_are_revalidated_before_commit() {
    for mode in 0..4 {
        let (s, v) = fixture();
        let mut c = coordinator(&v);
        let prepared = prepare(&c, &s);
        match mode {
            0 => v.0.borrow_mut().root.generation += 1,
            1 => v.0.borrow_mut().root.scope.branch = d(99),
            2 => v.0.borrow_mut().root.now = 90,
            _ => v.0.borrow_mut().resolved.intent.obligations = d(99),
        }
        assert!(c.commit(prepared).is_err());
        assert_eq!(c.store().balance(s.result.miner), 1000);
        assert_eq!(c.store().reserved_resources(), 0);
        assert_eq!(c.store().totals(s.result.case_id()), None);
    }
}
#[test]
fn changed_real_store_funding_terms_fail_atomically_without_a_court_row() {
    let (s, v) = fixture();
    let mut c = coordinator(&v);
    let prepared = prepare(&c, &s);
    v.0.borrow_mut().fee.hold = 99;
    assert_eq!(c.commit(prepared).err(), Some(CourtAdmissionErrorV2::Store(AccountingError::StalePreparation)));
    assert_eq!(c.store().balance(s.result.miner), 1000);
    assert_eq!(c.store().reserved_resources(), 0);
    assert_eq!(c.store().totals(s.result.case_id()), None);
}
#[test]
fn same_case_different_signed_output_cannot_create_another_admission_or_budget() {
    let (mut s, v) = fixture();
    let mut c = coordinator(&v);
    c.commit(prepare(&c, &s)).unwrap();
    let case = s.result.case_id();
    s.result.output = d(99);
    s.signature = sign_result(&s.result);
    {
        let mut f = v.0.borrow_mut();
        f.resolved.intent.result = s.result.id();
        f.fee.intent.result = s.result.id();
    }
    assert_eq!(case, s.result.case_id());
    assert_eq!(
        c.prepare(s.result.clone(), &s.key, &s.signature).err(),
        Some(CourtAdmissionErrorV2::Store(AccountingError::AlreadyAdmittedDifferently))
    );
    assert_eq!(c.store().balance(s.result.miner), 900);
    assert_eq!(c.store().reserved_resources(), 4);
}
#[test]
fn forged_signature_wrong_root_and_foreign_prepare_never_reach_an_admitted_case() {
    let (s, v) = fixture();
    let c = coordinator(&v);
    let mut other = coordinator(&v);
    let prepared = prepare(&c, &s);
    assert_eq!(other.commit(prepared).err(), Some(CourtAdmissionErrorV2::ForeignCapability));
    let mut signature = s.signature;
    signature[0] ^= 1;
    assert_eq!(c.prepare(s.result.clone(), &s.key, &signature).err(), Some(CourtAdmissionErrorV2::Contract(ContractError::Signature)));
    v.0.borrow_mut().resolved.context.artifact = d(99);
    assert_eq!(c.prepare(s.result.clone(), &s.key, &s.signature).err(), Some(CourtAdmissionErrorV2::Contract(ContractError::Scope)));
    assert_eq!(other.store().balance(s.result.miner), 1000);
}
#[test]
fn native_unassigned_store_error_is_preserved_instead_of_successful_admission() {
    let (s, v) = fixture();
    let store = PrivateAccountingStore::new(
        UnassignedAccountingAuthority,
        BTreeMap::from([(s.result.miner, 1000)]),
        AccountingLimits { max_accounts: 4, max_cases: 4, max_services_per_case: 4, resource_capacity: 10 },
    )
    .unwrap();
    let c = AtomicAdmissionCoordinatorV2::new(store, v);
    assert_eq!(
        c.prepare(s.result.clone(), &s.key, &s.signature).err(),
        Some(CourtAdmissionErrorV2::Store(AccountingError::NativeAdapterUnassigned))
    );
    assert_eq!(c.store().balance(s.result.miner), 1000);
    assert_eq!(c.store().reserved_resources(), 0);
}
#[test]
fn checked_store_transition_and_undo_preserve_source_of_authority() {
    let (s, v) = fixture();
    let mut c = coordinator(&v);
    let admitted = c.commit(prepare(&c, &s)).unwrap();
    v.0.borrow_mut().closed = true;
    let close = c.store().prepare_close(admitted.store_case()).unwrap();
    let undo = c.apply_transition(&admitted, close).unwrap();
    assert_eq!(c.store().balance(s.result.miner), 1000);
    assert_eq!(c.store().reserved_resources(), 0);
    // Reorg changed branch: old-branch undo is still permitted by its private
    // store capability; new old-branch transitions are refused.
    v.0.borrow_mut().root.scope.branch = d(99);
    c.rollback(undo).unwrap();
    assert_eq!(c.store().balance(s.result.miner), 900);
    assert_eq!(c.store().reserved_resources(), 4);
    assert!(c.store().conserved());
}

#[test]
fn funding_or_resource_rejection_never_publishes_separate_a4_state() {
    for capacity in [false, true] {
        let (s, v) = fixture();
        if capacity {
            v.0.borrow_mut().fee.resource_units = 11;
        } else {
            v.0.borrow_mut().fee.payer = PrincipalId(d(99));
        }
        let store = PrivateAccountingStore::new(
            v.clone(),
            BTreeMap::from([(s.result.miner, 1000)]),
            AccountingLimits { max_accounts: 4, max_cases: 4, max_services_per_case: 4, resource_capacity: 10 },
        )
        .unwrap();
        let c = AtomicAdmissionCoordinatorV2::new(store, v);
        let error = if capacity { AccountingError::Capacity } else { AccountingError::InsufficientFunds };
        assert_eq!(c.prepare(s.result.clone(), &s.key, &s.signature).err(), Some(CourtAdmissionErrorV2::Store(error)));
        assert_eq!(c.store().balance(s.result.miner), 1000);
        assert_eq!(c.store().reserved_resources(), 0);
        assert_eq!(c.store().totals(s.result.case_id()), None);
        assert!(c.store().conserved());
    }
}

#[test]
fn stale_checked_transition_cannot_cross_undo_and_restore_in_same_branch() {
    let (s, v) = fixture();
    let mut c = coordinator(&v);
    let admitted = c.commit(prepare(&c, &s)).unwrap();
    v.0.borrow_mut().closed = true;
    let old = c.store().prepare_close(admitted.store_case()).unwrap();
    let current = c.store().prepare_close(admitted.store_case()).unwrap();
    let undo = c.apply_transition(&admitted, current).unwrap();
    c.rollback(undo).unwrap();
    assert_eq!(c.apply_transition(&admitted, old).err(), Some(CourtAdmissionErrorV2::Store(AccountingError::StalePreparation)));
    assert_eq!(c.store().balance(s.result.miner), 900);
    assert_eq!(c.store().reserved_resources(), 4);
    assert!(c.store().conserved());
}
mod committed_recovery {
    use super::*;
    use misaka_palw_positive_accounting::fees::{AdmittedCase, HoldTotals};
    use std::cell::Cell;

    /// Test wrapper delegates EVERY effect and opaque receipt to the actual A5
    /// book. It has no native success implementation or synthetic case handle.
    struct RecoveryBook {
        inner: PrivateAccountingStore<View>,
        view: View,
        reads: Cell<u32>,
        change_after_read: Cell<bool>,
    }
    impl AtomicCourtStore for RecoveryBook {
        type PreparedAdmission = <PrivateAccountingStore<View> as AtomicCourtStore>::PreparedAdmission;
        type AdmittedCase = <PrivateAccountingStore<View> as AtomicCourtStore>::AdmittedCase;
        type CheckedTransition = <PrivateAccountingStore<View> as AtomicCourtStore>::CheckedTransition;
        type UndoRecord = <PrivateAccountingStore<View> as AtomicCourtStore>::UndoRecord;
        type Error = AccountingError;
        fn prepare_admission(&self, intent: AdmissionIntent) -> Result<Self::PreparedAdmission, Self::Error> {
            self.inner.prepare_admission(intent)
        }
        fn commit_admission(&mut self, prepared: Self::PreparedAdmission) -> Result<Self::AdmittedCase, Self::Error> {
            self.inner.commit_admission(prepared)
        }
        fn apply_transition(
            &mut self,
            case: &Self::AdmittedCase,
            next: Self::CheckedTransition,
        ) -> Result<Self::UndoRecord, Self::Error> {
            self.inner.apply_transition(case, next)
        }
        fn rollback(&mut self, undo: Self::UndoRecord) -> Result<(), Self::Error> {
            self.inner.rollback(undo)
        }
    }
    impl CommittedCourtRecoveryStoreV2 for RecoveryBook {
        fn recover_committed_case(
            &self,
            intent: AdmissionIntent,
            root: CourtAdmissionRootV2,
        ) -> Result<AdmittedCase, AccountingError> {
            if root != self.view.root() {
                return Err(AccountingError::Binding);
            }
            self.reads.set(self.reads.get() + 1);
            let case = self.inner.recover_committed_case(intent)?;
            if self.change_after_read.get() {
                self.view.0.borrow_mut().root.generation += 1;
            }
            Ok(case)
        }
    }
    #[derive(Clone)]
    struct RecoveryView {
        inner: View,
        change_during_resolve: Rc<Cell<bool>>,
    }
    impl CourtAdmissionViewV2 for RecoveryView {
        type Error = &'static str;
        fn root(&self) -> CourtAdmissionRootV2 {
            self.inner.root()
        }
        fn resolve(&self, _: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
            Err("recovery fixture forbids new-intake resolution")
        }
        fn resolve_committed(&self, _: &AdmissionIntent, _: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
            // Explicit PRIVATE completed-history fixture, not a native Case or
            // deadline adapter. Real native implementations need actual rows.
            let mut facts = self.inner.0.borrow_mut();
            if !facts.closed {
                return Err("completed history unavailable");
            }
            if self.change_during_resolve.get() {
                facts.root.generation += 1;
            }
            Ok(facts.resolved)
        }
    }
    type RecoveryCoordinator = AtomicAdmissionCoordinatorV2<RecoveryBook, RecoveryView>;
    fn wrap(store: PrivateAccountingStore<View>, view: &View) -> RecoveryCoordinator {
        AtomicAdmissionCoordinatorV2::new(
            RecoveryBook { inner: store, view: view.clone(), reads: Cell::new(0), change_after_read: Cell::new(false) },
            RecoveryView { inner: view.clone(), change_during_resolve: Rc::new(Cell::new(false)) },
        )
    }
    fn snapshot(c: &RecoveryCoordinator, s: &Signed) -> (u64, u64, Option<HoldTotals>) {
        (c.store().inner.balance(s.result.miner), c.store().inner.reserved_resources(), c.store().inner.totals(s.result.case_id()))
    }
    #[test]
    fn completed_case_recovers_after_intake_and_service_expiry_without_readmission_or_epoch_change() {
        let (s, v) = fixture();
        let mut original = coordinator(&v);
        let old = original.commit(prepare(&original, &s)).unwrap();
        v.0.borrow_mut().closed = true;
        let before_restart = original.store().prepare_close(old.store_case()).unwrap();
        let (book, _) = original.into_parts();
        let mut fresh = wrap(book, &v);
        {
            let mut f = v.0.borrow_mut();
            f.root.now = 110;
            f.root.generation = 8;
            // New-admission authority is no longer supplied. The committed book
            // is still real, held, and recoverable; no replacement hold is made.
            f.fee.intent.obligations = d(98);
        }
        let before = snapshot(&fresh, &s);
        let recovered = fresh.recover_committed_case(s.result.clone(), &s.key, &s.signature, old.intent()).unwrap();
        assert_eq!(recovered.root(), v.root());
        assert_eq!(recovered.intent(), old.intent());
        assert_eq!(snapshot(&fresh, &s), before);
        assert_eq!(fresh.store().reads.get(), 1);
        assert!(fresh.prepare(s.result.clone(), &s.key, &s.signature).is_err());
        let foreign = fresh.store().inner.prepare_close(old.store_case()).unwrap();
        assert_eq!(fresh.apply_transition(&old, foreign).err(), Some(CourtAdmissionErrorV2::ForeignCapability));
        // This transition was prepared before restart/recovery. Its real A5
        // epoch still matches, proving recovery did not advance the book.
        fresh.apply_transition(&recovered, before_restart).unwrap();
        assert_eq!(fresh.store().inner.balance(s.result.miner), 1000);
        assert_eq!(fresh.store().inner.reserved_resources(), 0);
        assert!(fresh.store().inner.conserved());
        assert!(recovered.into_store_case().into_admission_undo().is_none());
    }
    #[test]
    fn recovery_rejects_forgery_foreign_scope_different_result_intent_missing_case_and_root_races() {
        for mode in 0..7 {
            let (s, v) = fixture();
            let mut original = coordinator(&v);
            let old = original.commit(prepare(&original, &s)).unwrap();
            v.0.borrow_mut().closed = true;
            let old_transition = original.store().prepare_close(old.store_case()).unwrap();
            let (book, _) = original.into_parts();
            let book = if mode == 4 { coordinator(&v).into_parts().0 } else { book };
            let fresh = wrap(book, &v);
            {
                let mut f = v.0.borrow_mut();
                f.root.now = 110;
                f.root.generation = 8;
            }
            let before = snapshot(&fresh, &s);
            let mut result = s.result.clone();
            let mut signature = s.signature;
            let mut intent = old.intent();
            let expected = match mode {
                0 => {
                    signature[0] ^= 1;
                    CourtAdmissionErrorV2::Contract(ContractError::Signature)
                }
                1 => {
                    intent.scope.branch = d(99);
                    CourtAdmissionErrorV2::Binding
                }
                2 => {
                    result.output = d(99);
                    signature = sign_result(&result);
                    intent.result = result.id();
                    CourtAdmissionErrorV2::Binding
                }
                3 => {
                    intent.resource_quote = d(99);
                    CourtAdmissionErrorV2::Binding
                }
                4 => CourtAdmissionErrorV2::Store(AccountingError::UnknownCase),
                5 => {
                    fresh.view().change_during_resolve.set(true);
                    CourtAdmissionErrorV2::StaleRoot
                }
                _ => {
                    fresh.store().change_after_read.set(true);
                    CourtAdmissionErrorV2::StaleRoot
                }
            };
            assert_eq!(fresh.recover_committed_case(result, &s.key, &signature, intent).err(), Some(expected), "mode {mode}");
            assert_eq!(snapshot(&fresh, &s), before, "mode {mode}");
            assert_eq!(fresh.store().reads.get(), if mode == 4 || mode == 6 { 1 } else { 0 });
            if mode != 4 {
                // No wrapper epoch/receipt substitution: the original actual
                // A5 transition is still usable after every failed recovery.
                let (mut actual, _) = fresh.into_parts();
                actual.inner.apply_transition(old.store_case(), old_transition).unwrap();
                assert_eq!(actual.inner.balance(s.result.miner), 1000);
                assert_eq!(actual.inner.reserved_resources(), 0);
                assert!(actual.inner.conserved());
            } else {
                assert!(fresh.store().inner.conserved());
            }
        }
    }
}
