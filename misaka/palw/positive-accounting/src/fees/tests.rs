use super::*;
use misaka_palw_positive_contract::identity::{ChainScope, ResultId};
use std::{cell::RefCell, rc::Rc};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn party(n: u8) -> PrincipalId {
    PrincipalId(d(n))
}
fn data(unit: u64) -> ServiceKey {
    ServiceKey::Source { material: d(20), kind: SourceKind::Input, unit }
}
fn completion() -> ServiceKey {
    ServiceKey::PositiveCompletion { plan: PlanId(d(21)) }
}
fn carrier() -> ServiceKey {
    ServiceKey::Carrier { carrier: d(22) }
}
fn intent(n: u8) -> AdmissionIntent {
    AdmissionIntent {
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        case: CaseId(d(n)),
        result: ResultId(d(n)),
        resource_quote: d(30),
        funding_consent: d(31),
        obligations: d(32),
        expiry: 100,
    }
}
fn contract(n: u8) -> ResolvedAdmission {
    ResolvedAdmission {
        intent: intent(n),
        payer: party(1),
        fee_cap: 100,
        hold: 100,
        contractual_burn: 2,
        resource_units: 4,
        services: BTreeMap::from([
            (data(0), ServiceTerms { amount: 10, recipient: Recipient::Fixed(party(2)) }),
            (data(1), ServiceTerms { amount: 10, recipient: Recipient::Fixed(party(2)) }),
            (carrier(), ServiceTerms { amount: 5, recipient: Recipient::Fixed(party(4)) }),
            (completion(), ServiceTerms { amount: 30, recipient: Recipient::CompletingPanel }),
        ]),
    }
}
#[derive(Default)]
struct Facts {
    now: u64,
    contracts: BTreeMap<CaseId, ResolvedAdmission>,
    delivered: BTreeMap<(CaseId, ServiceKey), ServiceDisposition>,
    closed: BTreeMap<CaseId, ClaimStatus>,
    status: BTreeMap<CaseId, ClaimStatus>,
}
#[derive(Clone)]
struct Authority(Rc<RefCell<Facts>>);
impl AccountingAuthority for Authority {
    fn now(&self) -> u64 {
        self.0.borrow().now
    }
    fn admission(&self, intent: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
        self.0
            .borrow()
            .contracts
            .get(&intent.case)
            .filter(|c| c.intent == *intent)
            .cloned()
            .ok_or(AccountingError::MissingConsentOrFunding)
    }
    fn service(&self, intent: &AdmissionIntent, key: ServiceKey) -> Result<ServiceDisposition, AccountingError> {
        self.admission(intent)?;
        Ok(self.0.borrow().delivered.get(&(intent.case, key)).copied().unwrap_or(ServiceDisposition::NotProvided))
    }
    fn closed_case(&self, intent: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        self.admission(intent)?;
        self.0.borrow().closed.get(&intent.case).copied().ok_or(AccountingError::MissingCourtClosure)
    }
    fn claim_status(&self, intent: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        self.admission(intent)?;
        Ok(self.0.borrow().status.get(&intent.case).copied().unwrap_or(ClaimStatus::Pending))
    }
}
fn limits() -> AccountingLimits {
    AccountingLimits { max_accounts: 10, max_cases: 10, max_services_per_case: 10, resource_capacity: 10 }
}
fn authority() -> Authority {
    Authority(Rc::new(RefCell::new(Facts { contracts: BTreeMap::from([(intent(10).case, contract(10))]), ..Default::default() })))
}
fn store(authority: &Authority) -> PrivateAccountingStore<Authority> {
    PrivateAccountingStore::new(authority.clone(), BTreeMap::from([(party(1), 1000)]), limits()).unwrap()
}
fn admit(store: &mut PrivateAccountingStore<Authority>, n: u8) -> AdmittedCase {
    let prepared = store.prepare_admission(intent(n)).unwrap();
    store.commit_admission(prepared).unwrap()
}
fn delivered(a: &Authority, key: ServiceKey, recipient: u8) {
    a.0.borrow_mut().delivered.insert((intent(10).case, key), ServiceDisposition::Delivered { recipient: party(recipient) });
}
fn pay(store: &mut PrivateAccountingStore<Authority>, admitted: &AdmittedCase, key: ServiceKey) -> AccountingUndo {
    let prepared = store.prepare_service(admitted, key).unwrap();
    let undo = store.apply_transition(admitted, prepared).unwrap();
    assert!(store.conserved());
    undo
}
fn close(store: &mut PrivateAccountingStore<Authority>, admitted: &AdmittedCase) -> AccountingUndo {
    let prepared = store.prepare_close(admitted).unwrap();
    let undo = store.apply_transition(admitted, prepared).unwrap();
    assert!(store.conserved());
    undo
}

#[test]
fn preparation_is_read_only_and_identical_admission_is_one_hold() {
    let a = authority();
    let mut s = store(&a);
    let first = s.prepare_admission(intent(10)).unwrap();
    let retry = s.prepare_admission(intent(10)).unwrap();
    assert_eq!(s.balance(party(1)), 1000);
    assert_eq!(s.reserved_resources(), 0);
    assert_eq!(s.totals(intent(10).case), None);
    let admitted = s.commit_admission(first).unwrap();
    let retried = s.commit_admission(retry).unwrap();
    assert!(retried.into_admission_undo().is_none());
    assert_eq!(s.balance(party(1)), 900);
    assert_eq!(s.reserved_resources(), 4);
    assert!(s.conserved());
    s.rollback(admitted.into_admission_undo().unwrap()).unwrap();
    assert_eq!(s.balance(party(1)), 1000);
    assert_eq!(s.reserved_resources(), 0);
    assert_eq!(s.totals(intent(10).case), None);
}

#[test]
fn failed_capacity_funding_or_consent_leaves_no_case_or_debit() {
    for mode in 0..3 {
        let a = authority();
        match mode {
            0 => a.0.borrow_mut().contracts.get_mut(&intent(10).case).unwrap().resource_units = 11,
            1 => a.0.borrow_mut().contracts.get_mut(&intent(10).case).unwrap().payer = party(9),
            _ => {
                a.0.borrow_mut().contracts.clear();
            }
        }
        let s = store(&a);
        let error = s.prepare_admission(intent(10)).err().unwrap();
        assert_eq!(
            error,
            [AccountingError::Capacity, AccountingError::InsufficientFunds, AccountingError::MissingConsentOrFunding][mode]
        );
        assert_eq!(s.balance(party(1)), 1000);
        assert_eq!(s.reserved_resources(), 0);
        assert_eq!(s.totals(intent(10).case), None);
        assert!(s.conserved());
    }
}

#[test]
fn fee_cap_full_schedule_burn_and_expiry_are_checked_before_admission() {
    for mode in 0..4 {
        let a = authority();
        let expected = match mode {
            0 => {
                a.0.borrow_mut().contracts.get_mut(&intent(10).case).unwrap().fee_cap = 99;
                AccountingError::Binding
            }
            1 => {
                a.0.borrow_mut().contracts.get_mut(&intent(10).case).unwrap().hold = 40;
                AccountingError::InsufficientFunds
            }
            2 => {
                a.0.borrow_mut().contracts.get_mut(&intent(10).case).unwrap().contractual_burn = u64::MAX;
                AccountingError::Overflow
            }
            _ => {
                a.0.borrow_mut().now = 100;
                AccountingError::Expired
            }
        };
        let s = store(&a);
        assert_eq!(s.prepare_admission(intent(10)).err(), Some(expected));
        assert!(s.conserved());
    }
}

#[test]
fn delivered_service_is_paid_once_when_the_claim_ends_unverified() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    pay(&mut s, &admitted, data(0));
    for _ in 0..3 {
        pay(&mut s, &admitted, data(0));
    }
    assert_eq!(s.balance(party(2)), 10);
    // Panel replacement/relay changes do not acquire another unit or fee.
    delivered(&a, data(0), 8);
    pay(&mut s, &admitted, data(0));
    assert_eq!(s.balance(party(8)), 0);
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::EndedUnverified);
    close(&mut s, &admitted);
    assert_eq!(s.balance(party(1)), 988);
    assert_eq!(s.balance(party(3)), 0);
    assert_eq!(s.reserved_resources(), 0);
    assert_eq!(
        s.totals(intent(10).case),
        Some(HoldTotals { initial: 100, paid_unique_service: 10, refunded: 88, remaining: 0, contractual_burn: 2 })
    );
    close(&mut s, &admitted);
    assert_eq!(s.balance(party(1)), 988);
}

#[test]
fn source_delivery_panel_abandonment_and_bad_witness_do_not_earn_completion_fee() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    pay(&mut s, &admitted, data(0));
    assert_eq!(s.prepare_service(&admitted, completion()).err(), Some(AccountingError::ServiceNotProvided));
    // Even a delivered packet is not a checked Positive terminal.
    delivered(&a, completion(), 3);
    assert_eq!(s.prepare_service(&admitted, completion()).err(), Some(AccountingError::ServiceNotProvided));
    assert_eq!(s.prepare_close(&admitted).err(), Some(AccountingError::MissingCourtClosure));
    assert_eq!(s.balance(party(1)), 900);
    assert_eq!(s.balance(party(2)), 10);
    assert_eq!(s.balance(party(3)), 0);
    assert_eq!(s.totals(intent(10).case).unwrap().remaining, 90);
}

#[test]
fn completing_panel_gets_one_scope_fee_without_mining_or_gate_c_credit() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    a.0.borrow_mut().status.insert(intent(10).case, ClaimStatus::PositivePassed);
    a.0.borrow_mut().delivered.insert((intent(10).case, completion()), ServiceDisposition::PositiveCompleted { recipient: party(3) });
    pay(&mut s, &admitted, completion());
    a.0.borrow_mut().delivered.insert((intent(10).case, completion()), ServiceDisposition::PositiveCompleted { recipient: party(8) });
    pay(&mut s, &admitted, completion());
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::PositivePassed);
    close(&mut s, &admitted);
    assert_eq!(s.balance(party(3)), 30);
    assert_eq!(s.balance(party(8)), 0);
    assert_eq!(s.balance(party(1)), 968);
    assert_eq!(s.totals(intent(10).case).unwrap().paid_unique_service, 30);
    // No reward/weight/mint interface exists on this store. This is prepaid fee.
    assert!(s.conserved());
}

#[test]
fn closure_settles_delivered_data_and_carriage_before_refunding_the_remainder() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    delivered(&a, carrier(), 4);
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::ClaimRefuted);
    close(&mut s, &admitted);
    assert_eq!(s.balance(party(2)), 10);
    assert_eq!(s.balance(party(4)), 5);
    assert_eq!(s.balance(party(1)), 983);
    assert_eq!(s.payment(intent(10).case, completion()), None);
    assert_eq!(s.prepare_service(&admitted, data(1)).err(), Some(AccountingError::Closed));
    pay(&mut s, &admitted, data(0));
    assert_eq!(s.balance(party(2)), 10);
}

#[test]
fn resource_and_payer_reservations_are_shared_across_cases_and_never_reset_by_retry() {
    let a = authority();
    a.0.borrow_mut().contracts.insert(intent(11).case, contract(11));
    a.0.borrow_mut().contracts.insert(intent(12).case, contract(12));
    let mut s = store(&a);
    admit(&mut s, 10);
    admit(&mut s, 11);
    assert_eq!(s.balance(party(1)), 800);
    assert_eq!(s.prepare_admission(intent(12)).err(), Some(AccountingError::Capacity));
    let mut changed = contract(10);
    changed.intent.result = ResultId(d(90));
    let changed_intent = changed.intent;
    a.0.borrow_mut().contracts.insert(changed_intent.case, changed);
    assert_eq!(s.prepare_admission(changed_intent).err(), Some(AccountingError::AlreadyAdmittedDifferently));
    assert_eq!(s.reserved_resources(), 8);
    assert!(s.conserved());
}

#[test]
fn changes_between_prepare_and_commit_are_rechecked_with_no_partial_mutation() {
    let a = authority();
    let mut s = store(&a);
    let prepared = s.prepare_admission(intent(10)).unwrap();
    a.0.borrow_mut().contracts.get_mut(&intent(10).case).unwrap().payer = party(8);
    assert_eq!(s.commit_admission(prepared).err(), Some(AccountingError::StalePreparation));
    assert_eq!(s.balance(party(1)), 1000);
    a.0.borrow_mut().contracts.insert(intent(10).case, contract(10));
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    let prepared = s.prepare_service(&admitted, data(0)).unwrap();
    a.0.borrow_mut().delivered.remove(&(intent(10).case, data(0)));
    assert_eq!(s.apply_transition(&admitted, prepared).err(), Some(AccountingError::ServiceNotProvided));
    assert_eq!(s.balance(party(2)), 0);
    assert_eq!(s.totals(intent(10).case).unwrap().remaining, 100);
}

#[test]
fn recipient_overflow_during_closure_cannot_leave_an_earlier_payment_or_refund() {
    let a = authority();
    let mut s = PrivateAccountingStore::new(a.clone(), BTreeMap::from([(party(1), 1000), (party(4), u64::MAX)]), limits()).unwrap();
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    delivered(&a, carrier(), 4);
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::EndedUnverified);
    let prepared = s.prepare_close(&admitted).unwrap();
    assert_eq!(s.apply_transition(&admitted, prepared).err(), Some(AccountingError::Overflow));
    assert_eq!(s.balance(party(2)), 0);
    assert_eq!(s.balance(party(1)), 900);
    assert_eq!(s.reserved_resources(), 4);
    assert_eq!(s.payment(intent(10).case, data(0)), None);
    assert_eq!(s.totals(intent(10).case).unwrap().remaining, 100);
    assert!(s.conserved());
}

#[test]
fn payment_refund_burn_reservation_and_dedupe_rollback_together() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    let undo_payment = pay(&mut s, &admitted, data(0));
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::EndedUnverified);
    let undo_close = close(&mut s, &admitted);
    s.rollback(undo_close).unwrap();
    assert_eq!(s.balance(party(1)), 900);
    assert_eq!(s.balance(party(2)), 10);
    assert_eq!(s.totals(intent(10).case).unwrap().remaining, 90);
    assert_eq!(s.reserved_resources(), 4);
    s.rollback(undo_payment).unwrap();
    assert_eq!(s.balance(party(2)), 0);
    assert_eq!(s.payment(intent(10).case, data(0)), None);
    assert_eq!(s.totals(intent(10).case).unwrap().remaining, 100);
    s.rollback(admitted.into_admission_undo().unwrap()).unwrap();
    assert_eq!(s.balance(party(1)), 1000);
    assert_eq!(s.reserved_resources(), 0);
    assert_eq!(s.totals(intent(10).case), None);
    assert!(s.conserved());
}

#[test]
fn rollback_prevents_aba_reuse_of_an_old_prepared_payment() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    let stale = s.prepare_service(&admitted, data(0)).unwrap();
    let undo = pay(&mut s, &admitted, data(0));
    s.rollback(undo).unwrap();
    assert_eq!(s.apply_transition(&admitted, stale).err(), Some(AccountingError::StalePreparation));
    pay(&mut s, &admitted, data(0));
    assert_eq!(s.balance(party(2)), 10);
}

#[test]
fn tokens_and_undo_cannot_be_moved_between_identically_funded_stores() {
    let a = authority();
    let mut first = store(&a);
    let mut second = store(&a);
    let prepared = first.prepare_admission(intent(10)).unwrap();
    assert_eq!(second.commit_admission(prepared).err(), Some(AccountingError::ForeignStore));
    let admitted = admit(&mut first, 10);
    delivered(&a, data(0), 2);
    let undo = pay(&mut first, &admitted, data(0));
    assert_eq!(second.rollback(undo), Err(AccountingError::ForeignStore));
    assert_eq!(second.balance(party(1)), 1000);
    assert!(second.conserved());
}

#[test]
fn failed_recipient_or_unknown_unit_never_charges_the_payer() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 9);
    assert_eq!(s.prepare_service(&admitted, data(0)).err(), Some(AccountingError::Recipient));
    assert_eq!(s.prepare_service(&admitted, data(2)).err(), Some(AccountingError::UnknownService));
    assert_eq!(s.totals(intent(10).case).unwrap().remaining, 100);
    assert!(s.conserved());
}

#[test]
fn account_and_tombstone_capacity_fail_closed_without_evicting_payment_markers() {
    let a = authority();
    let mut caps = limits();
    caps.max_accounts = 1;
    caps.max_cases = 1;
    let mut s = PrivateAccountingStore::new(a.clone(), BTreeMap::from([(party(1), 1000)]), caps).unwrap();
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    let prepared = s.prepare_service(&admitted, data(0)).unwrap();
    assert_eq!(s.apply_transition(&admitted, prepared).err(), Some(AccountingError::Budget));
    a.0.borrow_mut().delivered.clear();
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::EndedUnverified);
    close(&mut s, &admitted);
    a.0.borrow_mut().contracts.insert(intent(11).case, contract(11));
    assert_eq!(s.prepare_admission(intent(11)).err(), Some(AccountingError::Budget));
    assert!(s.conserved());
}

#[test]
fn native_funding_has_no_placeholder_success_path() {
    let s = PrivateAccountingStore::new(UnassignedAccountingAuthority, BTreeMap::from([(party(1), 1000)]), limits()).unwrap();
    assert_eq!(s.prepare_admission(intent(10)).err(), Some(AccountingError::NativeAdapterUnassigned));
    assert_eq!(s.balance(party(1)), 1000);
}

#[test]
fn a_stale_positive_delivery_cannot_pay_completion_for_a_current_nonpassed_claim() {
    for status in [
        ClaimStatus::ResultCommitted,
        ClaimStatus::Pending,
        ClaimStatus::EndedUnverified,
        ClaimStatus::ClaimRefuted,
        ClaimStatus::Revoked,
    ] {
        let a = authority();
        let mut s = store(&a);
        let admitted = admit(&mut s, 10);
        a.0.borrow_mut().status.insert(intent(10).case, status);
        a.0.borrow_mut()
            .delivered
            .insert((intent(10).case, completion()), ServiceDisposition::PositiveCompleted { recipient: party(3) });
        assert_eq!(s.prepare_service(&admitted, completion()).err(), Some(AccountingError::ServiceNotProvided));
        assert_eq!(s.balance(party(3)), 0);
        assert_eq!(s.totals(intent(10).case).unwrap().remaining, 100);
    }
}

#[test]
fn a_pending_source_duty_cannot_be_closed_by_positive_completion_alone() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    a.0.borrow_mut().status.insert(intent(10).case, ClaimStatus::PositivePassed);
    assert_eq!(s.prepare_close(&admitted).err(), Some(AccountingError::MissingCourtClosure));
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::Pending);
    assert_eq!(s.prepare_close(&admitted).err(), Some(AccountingError::NotClosed));
    assert_eq!(s.reserved_resources(), 4);
    assert_eq!(s.balance(party(1)), 900);
}

#[test]
fn out_of_order_rollback_or_expired_admission_cannot_change_balances() {
    let a = authority();
    let mut s = store(&a);
    let first = s.prepare_admission(intent(10)).unwrap();
    a.0.borrow_mut().now = 100;
    assert_eq!(s.commit_admission(first).err(), Some(AccountingError::Expired));
    assert_eq!(s.balance(party(1)), 1000);
    a.0.borrow_mut().now = 1;
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    let undo_payment = pay(&mut s, &admitted, data(0));
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::EndedUnverified);
    let undo_close = close(&mut s, &admitted);
    assert_eq!(s.rollback(undo_payment), Err(AccountingError::RollbackOrder));
    assert_eq!(s.balance(party(1)), 988);
    assert_eq!(s.balance(party(2)), 10);
    s.rollback(undo_close).unwrap();
    assert_eq!(s.balance(party(1)), 900);
    assert_eq!(s.reserved_resources(), 4);
    assert!(s.conserved());
}

#[test]
fn recover_existing_private_case_is_read_only_even_after_authority_removal_and_expiry() {
    let a = authority();
    let mut s = store(&a);
    let admitted = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    pay(&mut s, &admitted, data(0));
    let before = (s.book.clone(), s.epoch, s.head, s.initial_supply);
    a.0.borrow_mut().now = 101;
    a.0.borrow_mut().contracts.clear();
    let recovered = {
        // Any call to either authority.now() or admission() would borrow this
        // RefCell and panic. A reader of the committed book needs neither.
        let _authority_unavailable = a.0.borrow_mut();
        s.recover_committed_case(intent(10)).unwrap()
    };
    assert_eq!(recovered.intent(), intent(10));
    assert!(recovered.into_admission_undo().is_none());
    assert_eq!(s.prepare_admission(intent(10)).err(), Some(AccountingError::MissingConsentOrFunding));
    a.0.borrow_mut().contracts.insert(intent(10).case, contract(10));
    assert_eq!(s.prepare_admission(intent(10)).err(), Some(AccountingError::Expired));
    assert_eq!((s.book.clone(), s.epoch, s.head, s.initial_supply), before);
    assert!(s.conserved());
}

#[test]
fn recover_requires_every_literal_intent_field_to_match_the_committed_case() {
    let a = authority();
    let mut s = store(&a);
    admit(&mut s, 10);
    let before = (s.book.clone(), s.epoch, s.head, s.initial_supply);
    for field in 0..9 {
        let mut changed = intent(10);
        match field {
            0 => changed.scope.network = d(90),
            1 => changed.scope.ruleset = d(90),
            2 => changed.scope.branch = d(90),
            3 => changed.case = CaseId(d(90)),
            4 => changed.result = ResultId(d(90)),
            5 => changed.resource_quote = d(90),
            6 => changed.funding_consent = d(90),
            7 => changed.obligations = d(90),
            _ => changed.expiry += 1,
        }
        let expected = if field == 3 { AccountingError::UnknownCase } else { AccountingError::Binding };
        assert_eq!(s.recover_committed_case(changed).err(), Some(expected), "field {field}");
    }
    assert_eq!((s.book.clone(), s.epoch, s.head, s.initial_supply), before);
}

#[test]
fn a_recovered_handle_cannot_be_used_by_an_identically_funded_foreign_store() {
    let a = authority();
    let mut first = store(&a);
    let mut second = store(&a);
    admit(&mut first, 10);
    admit(&mut second, 10);
    assert_eq!(first.book, second.book);
    let recovered = first.recover_committed_case(intent(10)).unwrap();
    let before = (second.book.clone(), second.epoch, second.head, second.initial_supply);
    assert_eq!(second.prepare_service(&recovered, data(0)).err(), Some(AccountingError::ForeignStore));
    assert_eq!(second.prepare_close(&recovered).err(), Some(AccountingError::ForeignStore));
    assert!(recovered.into_admission_undo().is_none());
    assert_eq!((second.book.clone(), second.epoch, second.head, second.initial_supply), before);
}

#[test]
fn recovered_handles_preserve_once_only_pay_refund_and_real_admission_rollback() {
    let a = authority();
    let mut s = store(&a);
    let original = admit(&mut s, 10);
    delivered(&a, data(0), 2);
    let recovered = s.recover_committed_case(intent(10)).unwrap();
    let payment_undo = pay(&mut s, &recovered, data(0));
    let again = s.recover_committed_case(intent(10)).unwrap();
    pay(&mut s, &again, data(0));
    assert_eq!(s.balance(party(2)), 10);
    assert_eq!(s.totals(intent(10).case).unwrap().remaining, 90);
    a.0.borrow_mut().closed.insert(intent(10).case, ClaimStatus::EndedUnverified);
    let close_undo = close(&mut s, &again);
    let closed = s.recover_committed_case(intent(10)).unwrap();
    let closed_before = (s.book.clone(), s.epoch, s.head, s.initial_supply);
    close(&mut s, &closed);
    assert_eq!((s.book.clone(), s.epoch, s.head, s.initial_supply), closed_before);
    assert_eq!(s.balance(party(1)), 988);
    assert_eq!(s.balance(party(2)), 10);
    assert_eq!(s.reserved_resources(), 0);
    s.rollback(close_undo).unwrap();
    s.rollback(payment_undo).unwrap();
    s.rollback(original.into_admission_undo().unwrap()).unwrap();
    assert_eq!(s.recover_committed_case(intent(10)).err(), Some(AccountingError::UnknownCase));
    assert_eq!(s.prepare_service(&recovered, data(0)).err(), Some(AccountingError::UnknownCase));
    assert_eq!(s.balance(party(1)), 1000);
    assert_eq!(s.balance(party(2)), 0);
    assert_eq!(s.reserved_resources(), 0);
    assert!(s.conserved());
}
