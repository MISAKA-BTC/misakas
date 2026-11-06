//! A4 admission boundary over A0's single atomic-store transaction.
//!
//! This module owns no balances, holds, resource pool or journal copy. The
//! supplied store must commit those rows together under `AtomicCourtStore`'s
//! contract. A missing native funding/storage adapter is an error, never an
//! admitted case. The returned capability is opaque and cannot be decoded from
//! a DTO, a successful prepare, or a saved status flag.
use super::contract::{AdmittedCheckedFactV2, CheckedCourtFactV2, bind_admitted_fact};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{ChainScope, PUBKEY_BYTES, SIGNATURE_BYTES},
    messages::{AdmissionContext, ClaimForm, MinerResultCommit, MinerServiceContract},
    ports::{AdmissionIntent, AtomicCourtStore},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourtAdmissionRootV2 {
    pub scope: ChainScope,
    /// Monotonic state generation, including undo/reorg (no ABA reuse).
    pub generation: u64,
    pub now: u64,
}

/// Resolved by the trusted chain adapter from canonical assignment, registered
/// roots, signed finite service consent and Node-derived resource obligations.
/// These are data inputs, not an admission capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedCourtAdmissionV2 {
    pub context: AdmissionContext,
    pub service: MinerServiceContract,
    pub intent: AdmissionIntent,
}

pub trait CourtAdmissionViewV2 {
    type Error;
    fn root(&self) -> CourtAdmissionRootV2;
    /// MUST resolve from the rooted state, not copy expected fields from the
    /// submitted result. Missing consent, duty, capacity or native adapters fail.
    fn resolve(&self, result: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error>;
    /// Resolve a previously COMMITTED case, not eligibility for a new intake.
    /// Native implementations MUST check the current Case row, canonical anchor
    /// membership and independent Court/service obligations/deadlines. Funding
    /// admission_expiry does not erase an existing Court or earned service.
    /// Already checked/closed cases must retain their real rooted completion
    /// history; a missing row or saved flag alone is not authority. The actual
    /// store/credit gate additionally checks current row/epoch before effects.
    /// There is deliberately no default successful adapter.
    fn resolve_committed(&self, intent: &AdmissionIntent, result: &MinerResultCommit)
    -> Result<ResolvedCourtAdmissionV2, Self::Error>;
}

/// Read-only reissuance of an EXISTING store capability after restart.
/// Native implementations MUST authenticate the actual current Case row,
/// canonical accepted history/anchor, funding/lease ownership and current Case
/// epoch under `current_root`. Missing adapters/rows are errors. Deserializing
/// an intent/status or running new admission is not recovery. No balance,
/// reservation, epoch, liability or journal mutation is permitted, and the
/// funding NEW-admission cutoff must not erase already earned obligations.
pub trait CommittedCourtRecoveryStoreV2: AtomicCourtStore {
    fn recover_committed_case(
        &self,
        intent: AdmissionIntent,
        current_root: CourtAdmissionRootV2,
    ) -> Result<Self::AdmittedCase, Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum CourtAdmissionErrorV2<V, S> {
    Contract(ContractError),
    View(V),
    Store(S),
    Binding,
    Expired,
    StaleRoot,
    ForeignCapability,
}

/// Read-only preparation. Private fields prohibit constructing it from an
/// `AdmissionIntent` or deserializing an alleged earlier admission.
pub struct PreparedCourtAdmissionV2<P> {
    owner: Arc<()>,
    root: CourtAdmissionRootV2,
    result: MinerResultCommit,
    resolved: ResolvedCourtAdmissionV2,
    store: P,
}

impl<P> PreparedCourtAdmissionV2<P> {
    pub(crate) fn root(&self) -> CourtAdmissionRootV2 {
        self.root
    }
    pub(crate) fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub(crate) fn resolved(&self) -> ResolvedCourtAdmissionV2 {
        self.resolved
    }
}

/// A successful atomic-store commit or authenticated read-only recovery
/// constructs this capability. It means
/// admitted under the supplied store, never PositivePassed, native funding,
/// credit, or a computation certificate by itself.
pub struct AdmittedCourtV2<C> {
    owner: Arc<()>,
    intent: AdmissionIntent,
    root: CourtAdmissionRootV2,
    result: MinerResultCommit,
    resolved: ResolvedCourtAdmissionV2,
    store: C,
}
impl<C> AdmittedCourtV2<C> {
    pub fn intent(&self) -> AdmissionIntent {
        self.intent
    }
    pub fn root(&self) -> CourtAdmissionRootV2 {
        self.root
    }
    pub fn store_case(&self) -> &C {
        &self.store
    }
    /// Transfers the genuine store capability to A5/A6, e.g. to extract the
    /// admission undo that its concrete store keeps in the receipt.
    pub fn into_store_case(self) -> C {
        self.store
    }
}

pub struct CourtAdmissionUndoV2<U> {
    owner: Arc<()>,
    store: U,
}

pub struct AtomicAdmissionCoordinatorV2<S, V> {
    owner: Arc<()>,
    store: S,
    view: V,
}
impl<S: AtomicCourtStore, V: CourtAdmissionViewV2> AtomicAdmissionCoordinatorV2<S, V> {
    pub fn new(store: S, view: V) -> Self {
        Self { owner: Arc::new(()), store, view }
    }
    pub fn store(&self) -> &S {
        &self.store
    }
    pub fn view(&self) -> &V {
        &self.view
    }
    pub fn into_parts(self) -> (S, V) {
        (self.store, self.view)
    }

    fn resolve_current(
        &self,
        result: &MinerResultCommit,
        root: CourtAdmissionRootV2,
    ) -> Result<ResolvedCourtAdmissionV2, CourtAdmissionErrorV2<V::Error, S::Error>> {
        let resolved = self.view.resolve(result).map_err(CourtAdmissionErrorV2::View)?;
        resolved.context.check(ClaimForm::PositiveResultV2, result).map_err(CourtAdmissionErrorV2::Contract)?;
        resolved.service.validate(result, root.now).map_err(CourtAdmissionErrorV2::Contract)?;
        let intent = resolved.intent;
        if root.scope != result.scope
            || resolved.context.scope != root.scope
            || intent.scope != root.scope
            || intent.case != result.case_id()
            || intent.result != result.id()
            || intent.funding_consent != resolved.service.funding_consent
            || intent.expiry > resolved.service.deadline
        {
            return Err(CourtAdmissionErrorV2::Binding);
        }
        if root.now >= intent.expiry {
            return Err(CourtAdmissionErrorV2::Expired);
        }
        if self.view.root() != root {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        Ok(resolved)
    }

    pub fn prepare(
        &self,
        result: MinerResultCommit,
        miner_key: &[u8; PUBKEY_BYTES],
        miner_signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<PreparedCourtAdmissionV2<S::PreparedAdmission>, CourtAdmissionErrorV2<V::Error, S::Error>> {
        result.authenticate(miner_key, miner_signature).map_err(CourtAdmissionErrorV2::Contract)?;
        let root = self.view.root();
        let resolved = self.resolve_current(&result, root)?;
        let store = self.store.prepare_admission(resolved.intent).map_err(CourtAdmissionErrorV2::Store)?;
        // Store preparation is read-only by contract; a concurrently changed
        // rooted view invalidates it without creating any visible obligation.
        if self.view.root() != root {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        Ok(PreparedCourtAdmissionV2 { owner: self.owner.clone(), root, result, resolved, store })
    }

    pub fn commit(
        &mut self,
        prepared: PreparedCourtAdmissionV2<S::PreparedAdmission>,
    ) -> Result<AdmittedCourtV2<S::AdmittedCase>, CourtAdmissionErrorV2<V::Error, S::Error>> {
        if !Arc::ptr_eq(&self.owner, &prepared.owner) {
            return Err(CourtAdmissionErrorV2::ForeignCapability);
        }
        let root = self.view.root();
        if root.scope != prepared.root.scope || root.generation != prepared.root.generation || root.now < prepared.root.now {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        let current = self.resolve_current(&prepared.result, root)?;
        if current != prepared.resolved {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        // One store transaction owns case+funding+resource+liability+journal.
        // It MUST revalidate its own current funds, consent, generation and
        // clock, including a same-case/different-result collision. No separate
        // capacity mutation or fallible work follows a successful commit.
        let store = self.store.commit_admission(prepared.store).map_err(CourtAdmissionErrorV2::Store)?;
        Ok(AdmittedCourtV2 {
            owner: self.owner.clone(),
            intent: current.intent,
            root,
            result: prepared.result,
            resolved: current,
            store,
        })
    }

    /// Attach an independently checked terminal to a genuine committed case.
    /// Normal block/generation advances do not expire that case. The native
    /// committed-case resolver enforces its own Court clocks/current history;
    /// the NEW funding/intake cutoff is deliberately not reused here. This bound
    /// fact is not native credit authority: real store/current-row and credit
    /// gate checks remain mandatory before any effect.
    pub fn bind_checked_fact<'a, T>(
        &self,
        admitted: &'a AdmittedCourtV2<S::AdmittedCase>,
        fact: CheckedCourtFactV2<T>,
    ) -> Result<AdmittedCheckedFactV2<'a, S::AdmittedCase, T>, CourtAdmissionErrorV2<V::Error, S::Error>> {
        if !Arc::ptr_eq(&self.owner, &admitted.owner) {
            return Err(CourtAdmissionErrorV2::ForeignCapability);
        }
        let root = self.view.root();
        if root.scope != admitted.root.scope || root.generation < admitted.root.generation || root.now < admitted.root.now {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        let resolved = self.view.resolve_committed(&admitted.intent, &admitted.result).map_err(CourtAdmissionErrorV2::View)?;
        if resolved != admitted.resolved || self.view.root() != root {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        bind_admitted_fact(admitted, fact, root).map_err(CourtAdmissionErrorV2::Contract)
    }

    /// Forward a store-owned checked transition in the current rooted view.
    /// The immutable arithmetic fact may survive ordinary generation advances,
    /// but scope/intent/registered inputs must still resolve identically. A5's
    /// actual store capability must recheck current case existence, intent and
    /// its opaque transition epoch (including ABA after undo); this wrapper is
    /// not current-row authority by itself. Closure can legitimately occur after
    /// the funding admission cutoff; existing-case clocks/history are resolved
    /// separately and never inferred from that new-intake expiry.
    pub fn apply_bound_transition<T>(
        &mut self,
        bound: &AdmittedCheckedFactV2<'_, S::AdmittedCase, T>,
        transition: S::CheckedTransition,
    ) -> Result<CourtAdmissionUndoV2<S::UndoRecord>, CourtAdmissionErrorV2<V::Error, S::Error>> {
        let root = self.view.root();
        let checked_root = bound.root();
        let admitted = bound.admitted_case();
        if root.scope != checked_root.scope || root.generation < checked_root.generation || root.now < checked_root.now {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        if self.view.resolve_committed(&admitted.intent, &admitted.result).map_err(CourtAdmissionErrorV2::View)? != admitted.resolved
            || self.view.root() != root
        {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        self.apply_transition(admitted, transition)
    }

    pub fn apply_transition(
        &mut self,
        admitted: &AdmittedCourtV2<S::AdmittedCase>,
        transition: S::CheckedTransition,
    ) -> Result<CourtAdmissionUndoV2<S::UndoRecord>, CourtAdmissionErrorV2<V::Error, S::Error>> {
        if !Arc::ptr_eq(&self.owner, &admitted.owner) {
            return Err(CourtAdmissionErrorV2::ForeignCapability);
        }
        if self.view.root().scope != admitted.intent.scope {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        // A checked transition is the store's opaque associated capability, not
        // a public enum claiming that a Panel/Miner is guilty or paid.
        let store = self.store.apply_transition(&admitted.store, transition).map_err(CourtAdmissionErrorV2::Store)?;
        Ok(CourtAdmissionUndoV2 { owner: self.owner.clone(), store })
    }
    pub fn rollback(&mut self, undo: CourtAdmissionUndoV2<S::UndoRecord>) -> Result<(), CourtAdmissionErrorV2<V::Error, S::Error>> {
        if !Arc::ptr_eq(&self.owner, &undo.owner) {
            return Err(CourtAdmissionErrorV2::ForeignCapability);
        }
        // A6 must be able to undo an old branch after switching rooted views.
        self.store.rollback(undo.store).map_err(CourtAdmissionErrorV2::Store)
    }
}

impl<S: CommittedCourtRecoveryStoreV2, V: CourtAdmissionViewV2> AtomicAdmissionCoordinatorV2<S, V> {
    /// Reissue a committed case under THIS coordinator's fresh owner identity.
    /// Neither this method nor its store port performs new intake, reserves
    /// money/capacity or reconstructs a native grant from a saved DTO. Existing
    /// Court/service clocks are resolved independently of funding intake expiry.
    pub fn recover_committed_case(
        &self,
        result: MinerResultCommit,
        miner_key: &[u8; PUBKEY_BYTES],
        miner_signature: &[u8; SIGNATURE_BYTES],
        lookup_intent: AdmissionIntent,
    ) -> Result<AdmittedCourtV2<S::AdmittedCase>, CourtAdmissionErrorV2<V::Error, S::Error>> {
        result.authenticate(miner_key, miner_signature).map_err(CourtAdmissionErrorV2::Contract)?;
        let root = self.view.root();
        if result.scope != root.scope
            || lookup_intent.scope != root.scope
            || lookup_intent.case != result.case_id()
            || lookup_intent.result != result.id()
        {
            return Err(CourtAdmissionErrorV2::Binding);
        }
        let resolved = self.view.resolve_committed(&lookup_intent, &result).map_err(CourtAdmissionErrorV2::View)?;
        resolved.context.check(ClaimForm::PositiveResultV2, &result).map_err(CourtAdmissionErrorV2::Contract)?;
        let service = resolved.service;
        // Structural links only. service.validate(result, root.now) is a NEW
        // intake predicate and would incorrectly expire existing obligations.
        if service.version != CONTRACT_VERSION || service.revision != CONTRACT_REVISION {
            return Err(CourtAdmissionErrorV2::Contract(ContractError::Version));
        }
        if resolved.intent != lookup_intent
            || service.case != result.case_id()
            || service.id() != result.service_contract
            || service.funding_consent != lookup_intent.funding_consent
            || lookup_intent.expiry > service.deadline
        {
            return Err(CourtAdmissionErrorV2::Binding);
        }
        if service.budget.retention_until < service.deadline || service.budget.witness_attempts == 0 {
            return Err(CourtAdmissionErrorV2::Contract(ContractError::Budget));
        }
        if self.view.root() != root {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        let store = CommittedCourtRecoveryStoreV2::recover_committed_case(&self.store, lookup_intent, root)
            .map_err(CourtAdmissionErrorV2::Store)?;
        if self.view.root() != root {
            return Err(CourtAdmissionErrorV2::StaleRoot);
        }
        Ok(AdmittedCourtV2 { owner: self.owner.clone(), intent: lookup_intent, root, result, resolved, store })
    }
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
