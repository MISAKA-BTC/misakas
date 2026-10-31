//! Private, bounded fee accounting implementing A0's atomic-store port.
//!
//! The authority resolves payer consent, real funding and A4's checked service
//! facts. The provided native authority has no success path. This reference
//! book moves only explicitly provisioned private balances. It never pays an
//! inference reward, computes a guilty verdict or creates slash liability.

use misaka_palw_positive_contract::{
    identity::{CaseId, Digest, PlanId, PrincipalId},
    messages::{ClaimStatus, SourceKind},
    ports::{AdmissionIntent, AtomicCourtStore},
};
use std::{collections::BTreeMap, sync::Arc};

/// Payee, Panel attempt, relay, consumer and nonce are absent from the key.
/// Source units must use A4's canonical original-material identity/numbering;
/// overlapping requests cannot create new paid units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceKey {
    Source { material: Digest, kind: SourceKind, unit: u64 },
    Carrier { carrier: Digest },
    PositiveCompletion { plan: PlanId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipient {
    Fixed(PrincipalId),
    /// A4 resolves the actual successful witness author. A replacement does not
    /// gain another fee for the same completed scope.
    CompletingPanel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceTerms {
    pub amount: u64,
    pub recipient: Recipient,
}

/// Internal resolved contract, not a new wire format or signed DTO. The trusted
/// adapter must authenticate the exact consent and its payer, fee cap, schedule,
/// burn and refund conditions under the intent before returning this value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAdmission {
    pub intent: AdmissionIntent,
    pub payer: PrincipalId,
    pub fee_cap: u64,
    pub hold: u64,
    pub contractual_burn: u64,
    pub resource_units: u64,
    pub services: BTreeMap<ServiceKey, ServiceTerms>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceDisposition {
    NotProvided,
    Delivered {
        recipient: PrincipalId,
    },
    /// Only from a locally checked A4 PositivePassed terminal, not a signature,
    /// saved flag, source completion or a rejected candidate.
    PositiveCompleted {
        recipient: PrincipalId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountingError {
    NativeAdapterUnassigned,
    MissingConsentOrFunding,
    MissingServiceEvidence,
    MissingCourtClosure,
    Binding,
    Expired,
    Budget,
    Overflow,
    InsufficientFunds,
    Capacity,
    AlreadyAdmittedDifferently,
    StalePreparation,
    ForeignStore,
    UnknownCase,
    UnknownService,
    ServiceNotProvided,
    Recipient,
    NotClosed,
    Closed,
    RollbackOrder,
    Conservation,
}

/// Native integration must resolve all answers against one rooted execution
/// view. No public method on the fee book accepts `paid`, `guilty`, `complete`
/// or `payer_consented` flags. Defaults refuse missing adapters.
pub trait AccountingAuthority {
    fn now(&self) -> u64;
    fn admission(&self, _: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
        Err(AccountingError::MissingConsentOrFunding)
    }
    fn service(&self, _: &AdmissionIntent, _: ServiceKey) -> Result<ServiceDisposition, AccountingError> {
        Err(AccountingError::MissingServiceEvidence)
    }
    fn claim_status(&self, _: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        Err(AccountingError::MissingServiceEvidence)
    }
    /// Return a terminal only when all service duties/fees can be closed. A4
    /// may finish Positive before an accepted Miner source duty has ended.
    fn closed_case(&self, _: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
        Err(AccountingError::MissingCourtClosure)
    }
}

pub struct UnassignedAccountingAuthority;
impl AccountingAuthority for UnassignedAccountingAuthority {
    fn now(&self) -> u64 {
        0
    }
    fn admission(&self, _: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
        Err(AccountingError::NativeAdapterUnassigned)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccountingLimits {
    pub max_accounts: u64,
    /// Includes closed tombstones; deleting dedupe records is not a reset API.
    pub max_cases: u64,
    pub max_services_per_case: u64,
    pub resource_capacity: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldTotals {
    pub initial: u64,
    pub paid_unique_service: u64,
    pub refunded: u64,
    pub remaining: u64,
    pub contractual_burn: u64,
}
impl HoldTotals {
    pub fn conserved(self) -> bool {
        self.paid_unique_service
            .checked_add(self.refunded)
            .and_then(|n| n.checked_add(self.remaining))
            .and_then(|n| n.checked_add(self.contractual_burn))
            == Some(self.initial)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServicePayment {
    pub recipient: PrincipalId,
    pub amount: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CaseAccount {
    contract: ResolvedAdmission,
    totals: HoldTotals,
    settled: BTreeMap<ServiceKey, ServicePayment>,
    closed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Book {
    balances: BTreeMap<PrincipalId, u64>,
    cases: BTreeMap<CaseId, CaseAccount>,
    reserved: u64,
    burned: u128,
}

/// Preparation and undo are bound to one owner and one view generation. The
/// monotonically increasing epoch prevents reuse after rollback (ABA).
pub struct PreparedAdmission {
    owner: Arc<()>,
    epoch: u64,
    contract: ResolvedAdmission,
}

pub struct AdmittedCase {
    owner: Arc<()>,
    intent: AdmissionIntent,
    admission_undo: Option<AccountingUndo>,
}
impl AdmittedCase {
    pub fn intent(&self) -> AdmissionIntent {
        self.intent
    }
    /// Native A6 must journal this alongside the admission's case/resource rows.
    pub fn into_admission_undo(self) -> Option<AccountingUndo> {
        self.admission_undo
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Service(ServiceKey),
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SettlementPlan {
    payments: BTreeMap<ServiceKey, ServicePayment>,
    close_status: Option<ClaimStatus>,
}

pub struct CheckedFeeTransition {
    owner: Arc<()>,
    epoch: u64,
    intent: AdmissionIntent,
    operation: Operation,
    plan: SettlementPlan,
}

/// Reference undo contains the bounded before-image, never a persisted Pass.
/// A6's native implementation must journal touched rows in its storage transaction.
/// It is not Clone or deserializable, and rollback is strictly LIFO.
pub struct AccountingUndo {
    owner: Arc<()>,
    change: Option<UndoChange>,
}
struct UndoChange {
    head: u64,
    previous_head: Option<u64>,
    before: Book,
}

/// Private reference implementation of the atomic port. Account provisioning is
/// an explicit trusted test/integration input, not production funding consent.
/// There is no deposit/mint function or public mutable state accessor.
pub struct PrivateAccountingStore<A: AccountingAuthority> {
    authority: A,
    limits: AccountingLimits,
    owner: Arc<()>,
    epoch: u64,
    head: Option<u64>,
    book: Book,
    initial_supply: u128,
}

impl<A: AccountingAuthority> PrivateAccountingStore<A> {
    pub fn new(authority: A, balances: BTreeMap<PrincipalId, u64>, limits: AccountingLimits) -> Result<Self, AccountingError> {
        if limits.max_accounts == 0
            || limits.max_cases == 0
            || limits.max_services_per_case == 0
            || limits.resource_capacity == 0
            || balances.len() as u64 > limits.max_accounts
        {
            return Err(AccountingError::Budget);
        }
        let initial_supply =
            balances.values().try_fold(0u128, |n, amount| n.checked_add(*amount as u128)).ok_or(AccountingError::Overflow)?;
        Ok(Self {
            authority,
            limits,
            owner: Arc::new(()),
            epoch: 0,
            head: None,
            book: Book { balances, cases: BTreeMap::new(), reserved: 0, burned: 0 },
            initial_supply,
        })
    }
    pub fn balance(&self, party: PrincipalId) -> u64 {
        self.book.balances.get(&party).copied().unwrap_or(0)
    }
    pub fn totals(&self, case: CaseId) -> Option<HoldTotals> {
        self.book.cases.get(&case).map(|c| c.totals)
    }
    /// Read a receipt for an already committed case in THIS private store.
    /// Checks every intent field against the stored admission; does not run
    /// new-admission authority/expiry checks, reserve funds, or change the book,
    /// resource totals, journal head or epoch. The receipt carries no admission
    /// undo, so reading it cannot acquire authority to undo the original hold.
    ///
    /// This is not native restart/restore authority. A4's recovery adapter must
    /// independently validate its current root/generation, signed Result and
    /// committed view before binding a fresh coordinator-owned receipt. A real
    /// native store also requires actual history, funding and lease evidence.
    pub fn recover_committed_case(&self, intent: AdmissionIntent) -> Result<AdmittedCase, AccountingError> {
        self.case(intent)?;
        Ok(AdmittedCase { owner: self.owner.clone(), intent, admission_undo: None })
    }
    pub fn payment(&self, case: CaseId, key: ServiceKey) -> Option<ServicePayment> {
        self.book.cases.get(&case)?.settled.get(&key).copied()
    }
    pub fn reserved_resources(&self) -> u64 {
        self.book.reserved
    }
    pub fn conserved(&self) -> bool {
        self.conserved_book(&self.book)
    }
    fn conserved_book(&self, book: &Book) -> bool {
        let balances = book.balances.values().try_fold(0u128, |n, amount| n.checked_add(*amount as u128));
        let holds = book.cases.values().try_fold(0u128, |n, c| n.checked_add(c.totals.remaining as u128));
        book.cases.values().all(|c| c.totals.conserved())
            && balances.zip(holds).and_then(|(b, h)| b.checked_add(h)).and_then(|n| n.checked_add(book.burned))
                == Some(self.initial_supply)
    }
    fn owned(&self, owner: &Arc<()>) -> Result<(), AccountingError> {
        if Arc::ptr_eq(&self.owner, owner) { Ok(()) } else { Err(AccountingError::ForeignStore) }
    }
    fn account(&self, admitted: &AdmittedCase) -> Result<&CaseAccount, AccountingError> {
        self.owned(&admitted.owner)?;
        self.case(admitted.intent)
    }
    fn case(&self, intent: AdmissionIntent) -> Result<&CaseAccount, AccountingError> {
        let case = self.book.cases.get(&intent.case).ok_or(AccountingError::UnknownCase)?;
        if case.contract.intent != intent {
            return Err(AccountingError::Binding);
        }
        Ok(case)
    }
    fn validate_contract(&self, intent: AdmissionIntent, contract: &ResolvedAdmission) -> Result<(), AccountingError> {
        if contract.intent != intent || contract.hold > contract.fee_cap || contract.resource_units == 0 {
            return Err(AccountingError::Binding);
        }
        if self.authority.now() >= intent.expiry {
            return Err(AccountingError::Expired);
        }
        if contract.services.len() as u64 > self.limits.max_services_per_case {
            return Err(AccountingError::Budget);
        }
        let mut ceiling = contract.contractual_burn;
        for (key, service) in &contract.services {
            if matches!(service.recipient, Recipient::CompletingPanel) != matches!(key, ServiceKey::PositiveCompletion { .. }) {
                return Err(AccountingError::Recipient);
            }
            ceiling = ceiling.checked_add(service.amount).ok_or(AccountingError::Overflow)?;
        }
        if ceiling > contract.hold {
            return Err(AccountingError::InsufficientFunds);
        }
        Ok(())
    }
    fn check_admission_capacity(&self, contract: &ResolvedAdmission) -> Result<(), AccountingError> {
        if let Some(existing) = self.book.cases.get(&contract.intent.case) {
            return if existing.contract == *contract { Ok(()) } else { Err(AccountingError::AlreadyAdmittedDifferently) };
        }
        if self.book.cases.len() as u64 >= self.limits.max_cases {
            return Err(AccountingError::Budget);
        }
        if !self.book.balances.contains_key(&contract.payer) || self.balance(contract.payer) < contract.hold {
            return Err(AccountingError::InsufficientFunds);
        }
        let reserved = self.book.reserved.checked_add(contract.resource_units).ok_or(AccountingError::Overflow)?;
        if reserved > self.limits.resource_capacity {
            return Err(AccountingError::Capacity);
        }
        Ok(())
    }
    fn payment_plan(&self, account: &CaseAccount, key: ServiceKey) -> Result<Option<ServicePayment>, AccountingError> {
        let terms = account.contract.services.get(&key).ok_or(AccountingError::UnknownService)?;
        if account.settled.contains_key(&key) {
            return Ok(None);
        }
        if matches!(key, ServiceKey::PositiveCompletion { .. })
            && self.authority.claim_status(&account.contract.intent)? != ClaimStatus::PositivePassed
        {
            return Err(AccountingError::ServiceNotProvided);
        }
        let delivered = self.authority.service(&account.contract.intent, key)?;
        let recipient = match (key, delivered) {
            (_, ServiceDisposition::NotProvided) => return Err(AccountingError::ServiceNotProvided),
            (ServiceKey::PositiveCompletion { .. }, ServiceDisposition::PositiveCompleted { recipient }) => recipient,
            (ServiceKey::Source { .. } | ServiceKey::Carrier { .. }, ServiceDisposition::Delivered { recipient }) => recipient,
            _ => return Err(AccountingError::ServiceNotProvided),
        };
        if let Recipient::Fixed(expected) = terms.recipient {
            if expected != recipient {
                return Err(AccountingError::Recipient);
            }
        }
        Ok(Some(ServicePayment { recipient, amount: terms.amount }))
    }
    fn plan(&self, intent: AdmissionIntent, operation: Operation) -> Result<SettlementPlan, AccountingError> {
        let account = self.case(intent)?;
        let mut plan = SettlementPlan { payments: BTreeMap::new(), close_status: None };
        match operation {
            Operation::Service(key) => {
                if account.closed {
                    return if account.settled.contains_key(&key) { Ok(plan) } else { Err(AccountingError::Closed) };
                }
                if let Some(payment) = self.payment_plan(account, key)? {
                    plan.payments.insert(key, payment);
                }
            }
            Operation::Close => {
                if account.closed {
                    return Ok(plan);
                }
                let status = self.authority.closed_case(&intent)?;
                if !matches!(
                    status,
                    ClaimStatus::PositivePassed | ClaimStatus::ClaimRefuted | ClaimStatus::EndedUnverified | ClaimStatus::Revoked
                ) {
                    return Err(AccountingError::NotClosed);
                }
                for key in account.contract.services.keys().copied() {
                    // Completion is not earned by source delivery, Panel refusal,
                    // timeout or a refuted output. Other delivered fees survive.
                    if matches!(key, ServiceKey::PositiveCompletion { .. }) && status != ClaimStatus::PositivePassed {
                        continue;
                    }
                    match self.payment_plan(account, key) {
                        Ok(Some(payment)) => {
                            plan.payments.insert(key, payment);
                        }
                        Ok(None) | Err(AccountingError::ServiceNotProvided) => {}
                        Err(error) => return Err(error),
                    }
                }
                plan.close_status = Some(status);
            }
        }
        Ok(plan)
    }
    fn prepare_transition(&self, admitted: &AdmittedCase, operation: Operation) -> Result<CheckedFeeTransition, AccountingError> {
        self.account(admitted)?;
        Ok(CheckedFeeTransition {
            owner: self.owner.clone(),
            epoch: self.epoch,
            intent: admitted.intent,
            operation,
            plan: self.plan(admitted.intent, operation)?,
        })
    }
    pub fn prepare_service(&self, admitted: &AdmittedCase, key: ServiceKey) -> Result<CheckedFeeTransition, AccountingError> {
        self.prepare_transition(admitted, Operation::Service(key))
    }
    pub fn prepare_close(&self, admitted: &AdmittedCase) -> Result<CheckedFeeTransition, AccountingError> {
        self.prepare_transition(admitted, Operation::Close)
    }
    fn install(&mut self, candidate: Book) -> Result<AccountingUndo, AccountingError> {
        if !self.conserved_book(&candidate) {
            return Err(AccountingError::Conservation);
        }
        if candidate == self.book {
            return Ok(AccountingUndo { owner: self.owner.clone(), change: None });
        }
        let next = self.epoch.checked_add(1).ok_or(AccountingError::Overflow)?;
        let before = std::mem::replace(&mut self.book, candidate);
        let previous_head = self.head.replace(next);
        self.epoch = next;
        debug_assert!(self.conserved());
        Ok(AccountingUndo { owner: self.owner.clone(), change: Some(UndoChange { head: next, previous_head, before }) })
    }
    fn credit(candidate: &mut Book, party: PrincipalId, amount: u64, max_accounts: u64) -> Result<(), AccountingError> {
        if amount == 0 {
            return Ok(());
        }
        if !candidate.balances.contains_key(&party) && candidate.balances.len() as u64 >= max_accounts {
            return Err(AccountingError::Budget);
        }
        let balance = candidate.balances.entry(party).or_default();
        *balance = balance.checked_add(amount).ok_or(AccountingError::Overflow)?;
        Ok(())
    }
}

impl<A: AccountingAuthority> AtomicCourtStore for PrivateAccountingStore<A> {
    type PreparedAdmission = PreparedAdmission;
    type AdmittedCase = AdmittedCase;
    type CheckedTransition = CheckedFeeTransition;
    type UndoRecord = AccountingUndo;
    type Error = AccountingError;

    fn prepare_admission(&self, intent: AdmissionIntent) -> Result<PreparedAdmission, AccountingError> {
        let contract = self.authority.admission(&intent)?;
        self.validate_contract(intent, &contract)?;
        self.check_admission_capacity(&contract)?;
        Ok(PreparedAdmission { owner: self.owner.clone(), epoch: self.epoch, contract })
    }
    fn commit_admission(&mut self, prepared: PreparedAdmission) -> Result<AdmittedCase, AccountingError> {
        self.owned(&prepared.owner)?;
        let intent = prepared.contract.intent;
        // Retrying an identical admitted case never takes a second hold, even
        // when the prepared object predates the first commit.
        if let Some(existing) = self.book.cases.get(&intent.case) {
            if existing.contract != prepared.contract {
                return Err(AccountingError::AlreadyAdmittedDifferently);
            }
            return Ok(AdmittedCase { owner: self.owner.clone(), intent, admission_undo: None });
        }
        if prepared.epoch != self.epoch {
            return Err(AccountingError::StalePreparation);
        }
        let current = self.authority.admission(&intent)?;
        if current != prepared.contract {
            return Err(AccountingError::StalePreparation);
        }
        self.validate_contract(intent, &current)?;
        self.check_admission_capacity(&current)?;
        // The bounded candidate is invisible until every debit, case row and
        // resource reservation succeeds. No fallible work follows install.
        let mut candidate = self.book.clone();
        let balance = candidate.balances.entry(current.payer).or_default();
        *balance = balance.checked_sub(current.hold).ok_or(AccountingError::InsufficientFunds)?;
        candidate.reserved = candidate.reserved.checked_add(current.resource_units).ok_or(AccountingError::Overflow)?;
        let initial = current.hold;
        candidate.cases.insert(
            intent.case,
            CaseAccount {
                contract: current,
                totals: HoldTotals { initial, remaining: initial, ..Default::default() },
                settled: BTreeMap::new(),
                closed: false,
            },
        );
        let undo = self.install(candidate)?;
        Ok(AdmittedCase { owner: self.owner.clone(), intent, admission_undo: Some(undo) })
    }
    fn apply_transition(
        &mut self,
        admitted: &AdmittedCase,
        transition: CheckedFeeTransition,
    ) -> Result<AccountingUndo, AccountingError> {
        self.owned(&transition.owner)?;
        self.account(admitted)?;
        if transition.intent != admitted.intent {
            return Err(AccountingError::Binding);
        }
        if transition.epoch != self.epoch {
            return Err(AccountingError::StalePreparation);
        }
        let plan = self.plan(admitted.intent, transition.operation)?;
        if plan != transition.plan {
            return Err(AccountingError::StalePreparation);
        }
        let mut candidate = self.book.clone();
        // No update can escape if a later recipient overflows or lacks capacity.
        for payment in plan.payments.values() {
            Self::credit(&mut candidate, payment.recipient, payment.amount, self.limits.max_accounts)?;
        }
        let account = candidate.cases.get_mut(&admitted.intent.case).ok_or(AccountingError::UnknownCase)?;
        for (key, payment) in plan.payments {
            account.totals.remaining =
                account.totals.remaining.checked_sub(payment.amount).ok_or(AccountingError::InsufficientFunds)?;
            account.totals.paid_unique_service =
                account.totals.paid_unique_service.checked_add(payment.amount).ok_or(AccountingError::Overflow)?;
            account.settled.insert(key, payment);
        }
        if plan.close_status.is_some() {
            let burn = account.contract.contractual_burn;
            let refund = account.totals.remaining.checked_sub(burn).ok_or(AccountingError::InsufficientFunds)?;
            account.totals.contractual_burn = burn;
            account.totals.refunded = refund;
            account.totals.remaining = 0;
            account.closed = true;
            let payer = account.contract.payer;
            candidate.reserved = candidate.reserved.checked_sub(account.contract.resource_units).ok_or(AccountingError::Binding)?;
            candidate.burned = candidate.burned.checked_add(burn as u128).ok_or(AccountingError::Overflow)?;
            Self::credit(&mut candidate, payer, refund, self.limits.max_accounts)?;
        }
        self.install(candidate)
    }
    fn rollback(&mut self, undo: AccountingUndo) -> Result<(), AccountingError> {
        self.owned(&undo.owner)?;
        let Some(change) = undo.change else {
            return Ok(());
        };
        if self.head != Some(change.head) {
            return Err(AccountingError::RollbackOrder);
        }
        let next = self.epoch.checked_add(1).ok_or(AccountingError::Overflow)?;
        self.book = change.before;
        self.head = change.previous_head;
        self.epoch = next;
        debug_assert!(self.conserved());
        Ok(())
    }
}

#[cfg(test)]
mod tests;
