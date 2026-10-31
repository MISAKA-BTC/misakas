//! Independent arithmetic oracle over public A5 accounting APIs. The authority
//! is synthetic; these tests do not stand in for native funding/Court adapters.

use misaka_palw_positive_accounting::fees::{
    AccountingAuthority, AccountingError, AccountingLimits, AdmittedCase, PrivateAccountingStore, Recipient, ResolvedAdmission,
    ServiceDisposition, ServiceKey, ServiceTerms,
};
use misaka_palw_positive_contract::{
    identity::{CaseId, ChainScope, Digest, PlanId, PrincipalId, ResultId},
    messages::{ClaimStatus, SourceKind},
    ports::{AdmissionIntent, AtomicCourtStore},
};
use std::collections::BTreeMap;

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn p(n: u8) -> PrincipalId {
    PrincipalId(d(n))
}
fn key(unit: usize) -> ServiceKey {
    ServiceKey::Source { material: d(40), kind: SourceKind::ArtifactInventory, unit: unit as u64 }
}
fn completion() -> ServiceKey {
    ServiceKey::PositiveCompletion { plan: PlanId(d(41)) }
}
fn intent() -> AdmissionIntent {
    AdmissionIntent {
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        case: CaseId(d(10)),
        result: ResultId(d(11)),
        resource_quote: d(12),
        funding_consent: d(13),
        obligations: d(14),
        expiry: 100,
    }
}
const PRICES: [u64; 3] = [3, 5, 11];
const ORDERS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
#[derive(Clone, Copy)]
struct Authority {
    delivered: u8,
    positive: bool,
}
impl AccountingAuthority for Authority {
    fn now(&self) -> u64 {
        1
    }
    fn admission(&self, supplied: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
        if *supplied != intent() {
            return Err(AccountingError::Binding);
        }
        let mut services: BTreeMap<_, _> =
            (0..3).map(|n| (key(n), ServiceTerms { amount: PRICES[n], recipient: Recipient::Fixed(p(n as u8 + 2)) })).collect();
        services.insert(completion(), ServiceTerms { amount: 7, recipient: Recipient::CompletingPanel });
        Ok(ResolvedAdmission {
            intent: intent(),
            payer: p(1),
            fee_cap: 37,
            hold: 37,
            contractual_burn: 2,
            resource_units: 3,
            services,
        })
    }
    fn service(&self, supplied: &AdmissionIntent, service: ServiceKey) -> Result<ServiceDisposition, AccountingError> {
        if *supplied != intent() {
            return Err(AccountingError::Binding);
        }
        if service == completion() {
            return Ok(if self.positive {
                ServiceDisposition::PositiveCompleted { recipient: p(9) }
            } else {
                ServiceDisposition::NotProvided
            });
        }
        let n = (0..3).find(|n| key(*n) == service).ok_or(AccountingError::UnknownService)?;
        Ok(if self.delivered & (1 << n) != 0 {
            ServiceDisposition::Delivered { recipient: p(n as u8 + 2) }
        } else {
            ServiceDisposition::NotProvided
        })
    }
    fn claim_status(&self, _: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        Ok(if self.positive { ClaimStatus::PositivePassed } else { ClaimStatus::EndedUnverified })
    }
    fn closed_case(&self, supplied: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        self.claim_status(supplied)
    }
}
fn store(authority: Authority) -> PrivateAccountingStore<Authority> {
    PrivateAccountingStore::new(
        authority,
        BTreeMap::from([(p(1), 137)]),
        AccountingLimits { max_accounts: 5, max_cases: 1, max_services_per_case: 4, resource_capacity: 3 },
    )
    .unwrap()
}
fn admitted(store: &mut PrivateAccountingStore<Authority>) -> AdmittedCase {
    let prepared = store.prepare_admission(intent()).unwrap();
    store.commit_admission(prepared).unwrap()
}

#[test]
fn every_delivery_subset_payment_order_and_retry_conserves_and_reverses_exact_value() {
    for delivered in 0u8..8 {
        for positive in [false, true] {
            for order in ORDERS {
                for paid_before_close in 0u8..8 {
                    let mut s = store(Authority { delivered, positive });
                    let admitted = admitted(&mut s);
                    let mut undos = Vec::new();
                    let mut paid = 0;
                    let mut expected_order = Vec::new();
                    for n in order {
                        if paid_before_close & (1 << n) == 0 {
                            continue;
                        }
                        match s.prepare_service(&admitted, key(n)) {
                            Ok(prepared) => {
                                let projection = s.settlement_projection(&admitted, &prepared).unwrap();
                                expected_order.push(key(n));
                                assert_eq!(projection.payments().iter().map(|(key, _)| *key).collect::<Vec<_>>(), expected_order);
                                undos.push(s.apply_transition(&admitted, prepared).unwrap());
                                paid += PRICES[n];
                                // A second path to exactly this obligation must be a no-op.
                                let retry = s.prepare_service(&admitted, key(n)).unwrap();
                                let retry_projection = s.settlement_projection(&admitted, &retry).unwrap();
                                assert_eq!(retry_projection.before(), retry_projection.after());
                                assert_eq!(
                                    retry_projection.payments().iter().map(|(key, _)| *key).collect::<Vec<_>>(),
                                    expected_order
                                );
                                s.apply_transition(&admitted, retry).unwrap();
                            }
                            Err(error) => {
                                assert_eq!(error, AccountingError::ServiceNotProvided);
                                assert_eq!(delivered & (1 << n), 0);
                            }
                        }
                        assert!(s.conserved());
                    }
                    let before_close = s.totals(intent().case).unwrap();
                    assert_eq!(before_close.paid_unique_service, paid);
                    assert_eq!(before_close.remaining, 37 - paid);
                    let close = s.prepare_close(&admitted).unwrap();
                    let projection = s.settlement_projection(&admitted, &close).unwrap();
                    assert_eq!(
                        &projection.payments()[..expected_order.len()].iter().map(|(key, _)| *key).collect::<Vec<_>>(),
                        &expected_order
                    );
                    for unit in 0..3 {
                        if delivered & (1 << unit) != 0 && !expected_order.contains(&key(unit)) {
                            expected_order.push(key(unit));
                        }
                    }
                    if positive {
                        expected_order.push(completion());
                    }
                    assert_eq!(projection.payments().iter().map(|(key, _)| *key).collect::<Vec<_>>(), expected_order);
                    let undo_close = s.apply_transition(&admitted, close).unwrap();
                    let source_fee: u64 = PRICES.iter().enumerate().filter(|(n, _)| delivered & (1 << n) != 0).map(|(_, n)| *n).sum();
                    let fee = source_fee + if positive { 7 } else { 0 };
                    // The oracle accounts directly for each recipient's service,
                    // without using the implementation's totals/conservation helper.
                    assert_eq!(s.balance(p(1)), 137 - fee - 2);
                    for (n, price) in PRICES.iter().enumerate() {
                        assert_eq!(s.balance(p(n as u8 + 2)), if delivered & (1 << n) != 0 { *price } else { 0 });
                    }
                    assert_eq!(s.balance(p(9)), if positive { 7 } else { 0 });
                    assert_eq!(s.reserved_resources(), 0);
                    let totals = s.totals(intent().case).unwrap();
                    assert_eq!(
                        (totals.initial, totals.paid_unique_service, totals.refunded, totals.remaining, totals.contractual_burn),
                        (37, fee, 37 - fee - 2, 0, 2)
                    );
                    let close_again = s.prepare_close(&admitted).unwrap();
                    s.apply_transition(&admitted, close_again).unwrap();
                    assert_eq!(s.balance(p(1)), 137 - fee - 2);
                    s.rollback(undo_close).unwrap();
                    assert_eq!(s.totals(intent().case).unwrap(), before_close);
                    assert_eq!(s.reserved_resources(), 3);
                    for undo in undos.into_iter().rev() {
                        s.rollback(undo).unwrap();
                    }
                    for n in 0..3 {
                        assert_eq!(s.balance(p(n + 2)), 0);
                    }
                    assert_eq!(s.balance(p(9)), 0);
                    s.rollback(admitted.into_admission_undo().unwrap()).unwrap();
                    assert_eq!(s.balance(p(1)), 137);
                    assert_eq!(s.totals(intent().case), None);
                    assert_eq!(s.reserved_resources(), 0);
                    assert!(s.conserved());
                }
            }
        }
    }
}

#[test]
fn a_free_unit_keeps_its_dedupe_position_without_creating_a_native_value_due() {
    struct FreeFirst(Authority);
    impl AccountingAuthority for FreeFirst {
        fn now(&self) -> u64 {
            self.0.now()
        }
        fn admission(&self, intent: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
            let mut resolved = self.0.admission(intent)?;
            resolved.services.get_mut(&key(0)).unwrap().amount = 0;
            Ok(resolved)
        }
        fn service(&self, intent: &AdmissionIntent, key: ServiceKey) -> Result<ServiceDisposition, AccountingError> {
            self.0.service(intent, key)
        }
        fn claim_status(&self, intent: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
            self.0.claim_status(intent)
        }
        fn closed_case(&self, intent: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
            self.0.closed_case(intent)
        }
    }
    let mut s = PrivateAccountingStore::new(
        FreeFirst(Authority { delivered: 7, positive: false }),
        BTreeMap::from([(p(1), 137)]),
        AccountingLimits { max_accounts: 5, max_cases: 1, max_services_per_case: 4, resource_capacity: 3 },
    )
    .unwrap();
    let prepared = s.prepare_admission(intent()).unwrap();
    let admitted = s.commit_admission(prepared).unwrap();
    for n in [2, 0, 1] {
        let prepared = s.prepare_service(&admitted, key(n)).unwrap();
        s.apply_transition(&admitted, prepared).unwrap();
    }
    let close = s.prepare_close(&admitted).unwrap();
    let projection = s.settlement_projection(&admitted, &close).unwrap();
    assert_eq!(
        projection.payments().iter().map(|(k, p)| (*k, p.amount)).collect::<Vec<_>>(),
        vec![(key(2), 11), (key(0), 0), (key(1), 5)]
    );
    assert_eq!(projection.after().paid_unique_service, 16);
    assert_eq!(projection.after().refunded, 19);
    s.apply_transition(&admitted, close).unwrap();
    assert_eq!(s.balance(p(1)), 119);
    assert_eq!(s.payment(intent().case, key(0)).unwrap().amount, 0);
    assert!(s.conserved());
}
