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
