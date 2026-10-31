//! Signature and amount preflight for the A0 acyclic FundingScope contract.
//!
//! No type here is a validated native transaction, composed UTXO, coin lock,
//! escrow spend authorization, or native admission. The actual native store
//! must require those capabilities and atomically commit them with all court
//! state. A missing native output/quote/store adapter MUST return an error.
use super::admission::{
    AdmittedCourtV2, AtomicAdmissionCoordinatorV2, CourtAdmissionErrorV2, CourtAdmissionRootV2, CourtAdmissionViewV2,
    PreparedCourtAdmissionV2,
};
use misaka_palw_positive_contract::{
    ContractError,
    funding::{FundingConsent, FundingPlanBasis},
    identity::{Digest, PUBKEY_BYTES, SIGNATURE_BYTES},
    messages::{AdmissionContext, ClaimForm, MinerResultCommit, MinerServiceContract},
    ports::AtomicCourtStore,
};
use std::sync::Arc;

/// A separate pre-consent projection, never the literal final signed objects.
/// Both self-referential link fields are unavailable to the quote resolver;
/// consent/scope IDs and the carrying transaction are not inputs either.
/// A resolver must not hash a final result/plan/service/consent/carrier ID back
/// into the basis. Canonical registered inputs and rooted policy preimages are
/// its only authority for deriving the resource, fee and capital schedule.
pub struct FundingBasisInputV2 {
    result_core: MinerResultCommit,
    service_core: MinerServiceContract,
}
impl FundingBasisInputV2 {
    fn project(result: &MinerResultCommit, service: &MinerServiceContract) -> Self {
        let mut result_core = result.clone();
        result_core.service_contract = Digest([0; 64]);
        let mut service_core = *service;
        service_core.funding_consent = Digest([0; 64]);
        Self { result_core, service_core }
    }
    pub fn result_core(&self) -> &MinerResultCommit {
        &self.result_core
    }
    pub fn service_core(&self) -> &MinerServiceContract {
        &self.service_core
    }
}

/// Data independently resolved from the registered job/program, adopted policy
/// and canonical schedule PREIMAGES. Peer-supplied digest-to-fee mappings are
/// forbidden. A resolver must recompute all digests/amounts from those preimages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootedFundingQuoteV2 {
    pub context: AdmissionContext,
    pub basis: FundingPlanBasis,
    /// Canonical source-service subtotal, distinct from total Court fees and
    /// capital. It must fit both the signed source budget and total fee quote.
    pub source_service_fee: u64,
}

/// Output facts resolved from the ACTUAL validated carrier. This data alone
/// proves neither spendability nor a lock. The transaction identity is derived
/// after signing; it is never fed into FundingBasisInputV2 or FundingScope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedFundingOutputV2 {
    pub carrier: Digest,
    pub index: u32,
    pub amount: u64,
}

pub trait RootedFundingViewV2 {
    type Error;
    /// Native implementations use their validated transaction/composed UTXO
    /// context, not a peer's claimed amount or arbitrary output DTO.
    type Carrier;
    fn root(&self) -> CourtAdmissionRootV2;
    fn resolve_quote(&self, input: &FundingBasisInputV2) -> Result<RootedFundingQuoteV2, Self::Error>;
    fn resolve_output(&self, carrier: &Self::Carrier, index: u32) -> Result<ResolvedFundingOutputV2, Self::Error>;
}

#[derive(Clone, Copy)]
pub struct SignedFundingInputsV2<'a> {
    pub result: &'a MinerResultCommit,
    pub service: &'a MinerServiceContract,
    pub consent: &'a FundingConsent,
    pub miner_key: &'a [u8; PUBKEY_BYTES],
    pub miner_signature: &'a [u8; SIGNATURE_BYTES],
    pub payer_key: &'a [u8; PUBKEY_BYTES],
    pub payer_signature: &'a [u8; SIGNATURE_BYTES],
}

#[derive(Debug, PartialEq, Eq)]
pub enum FundingPreflightErrorV2<E> {
    Contract(ContractError),
    View(E),
    Binding,
    OutputAmount,
    RootChanged,
    QuoteChanged,
    OutputChanged,
    ForeignCapability,
}

/// Both literal signatures, acyclic linkage, rooted quote and output amount
/// checked. No Deserialize/boolean constructor/native success conversion.
pub struct SignatureCheckedFundingPreflightV2 {
    owner: Arc<()>,
    root: CourtAdmissionRootV2,
    result: MinerResultCommit,
    service: MinerServiceContract,
    consent: FundingConsent,
    quote: RootedFundingQuoteV2,
    output: ResolvedFundingOutputV2,
    miner_key: [u8; PUBKEY_BYTES],
    miner_signature: [u8; SIGNATURE_BYTES],
    payer_key: [u8; PUBKEY_BYTES],
    payer_signature: [u8; SIGNATURE_BYTES],
}
impl SignatureCheckedFundingPreflightV2 {
    pub fn root(&self) -> CourtAdmissionRootV2 {
        self.root
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub fn service(&self) -> &MinerServiceContract {
        &self.service
    }
    pub fn consent(&self) -> &FundingConsent {
        &self.consent
    }
    pub fn basis(&self) -> &FundingPlanBasis {
        &self.quote.basis
    }
    pub fn quote(&self) -> RootedFundingQuoteV2 {
        self.quote
    }
    pub fn output(&self) -> ResolvedFundingOutputV2 {
        self.output
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
}

pub struct FundingPreflightCoordinatorV2<V> {
    owner: Arc<()>,
    view: V,
}
impl<V: RootedFundingViewV2> FundingPreflightCoordinatorV2<V> {
    pub fn new(view: V) -> Self {
        Self { owner: Arc::new(()), view }
    }
    pub fn view(&self) -> &V {
        &self.view
    }

    pub fn check(
        &self,
        inputs: SignedFundingInputsV2<'_>,
        carrier: &V::Carrier,
    ) -> Result<SignatureCheckedFundingPreflightV2, FundingPreflightErrorV2<V::Error>> {
        inputs.result.authenticate(inputs.miner_key, inputs.miner_signature).map_err(FundingPreflightErrorV2::Contract)?;
        let root = self.view.root();
        if inputs.result.scope != root.scope {
            return Err(FundingPreflightErrorV2::Binding);
        }
        let projected = FundingBasisInputV2::project(inputs.result, inputs.service);
        let quote = self.view.resolve_quote(&projected).map_err(FundingPreflightErrorV2::View)?;
        quote.context.check(ClaimForm::PositiveResultV2, inputs.result).map_err(FundingPreflightErrorV2::Contract)?;
        inputs
            .consent
            .authenticate(inputs.result, inputs.service, &quote.basis, root.now, inputs.payer_key, inputs.payer_signature)
            .map_err(FundingPreflightErrorV2::Contract)?;
        if quote.source_service_fee > inputs.service.budget.fee || quote.source_service_fee > quote.basis.required_fee {
            return Err(FundingPreflightErrorV2::Contract(ContractError::Budget));
        }
        let required = quote
            .basis
            .required_fee
            .checked_add(quote.basis.required_capital)
            .ok_or(FundingPreflightErrorV2::Contract(ContractError::Budget))?;
        let output = self.view.resolve_output(carrier, inputs.consent.funding_output_index).map_err(FundingPreflightErrorV2::View)?;
        if output.index != inputs.consent.funding_output_index {
            return Err(FundingPreflightErrorV2::Binding);
        }
        if output.amount < required || output.amount > inputs.consent.max_locked {
            return Err(FundingPreflightErrorV2::OutputAmount);
        }
        if self.view.root() != root {
            return Err(FundingPreflightErrorV2::RootChanged);
        }
        Ok(SignatureCheckedFundingPreflightV2 {
            owner: self.owner.clone(),
            root,
            result: inputs.result.clone(),
            service: *inputs.service,
            consent: inputs.consent.clone(),
            quote,
            output,
            miner_key: *inputs.miner_key,
            miner_signature: *inputs.miner_signature,
            payer_key: *inputs.payer_key,
            payer_signature: *inputs.payer_signature,
        })
    }

    /// Re-resolve from the current policy/schedule/resource and actual output
    /// before the native owner commits anything. A stale prepare or replaced
    /// carrier cannot be used as current UTXO/funding authority.
    pub fn revalidate(
        &self,
        checked: &SignatureCheckedFundingPreflightV2,
        carrier: &V::Carrier,
    ) -> Result<(), FundingPreflightErrorV2<V::Error>> {
        if !Arc::ptr_eq(&self.owner, &checked.owner) {
            return Err(FundingPreflightErrorV2::ForeignCapability);
        }
        let root = self.view.root();
        if root.scope != checked.root.scope || root.generation != checked.root.generation || root.now < checked.root.now {
            return Err(FundingPreflightErrorV2::RootChanged);
        }
        let fresh = self.check(checked.inputs(), carrier)?;
        if fresh.root.scope != checked.root.scope
            || fresh.root.generation != checked.root.generation
            || fresh.root.now < checked.root.now
        {
            return Err(FundingPreflightErrorV2::RootChanged);
        }
        if fresh.quote != checked.quote {
            return Err(FundingPreflightErrorV2::QuoteChanged);
        }
        if fresh.output != checked.output {
            return Err(FundingPreflightErrorV2::OutputChanged);
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FundingAtomicErrorV2<F, A, S> {
    Funding(FundingPreflightErrorV2<F>),
    Admission(CourtAdmissionErrorV2<A, S>),
    Binding,
}
/// Joint preparation, not admitted and not a native funding/lock receipt.
pub struct PreparedFundingPreflightAdmissionV2<P> {
    funding: SignatureCheckedFundingPreflightV2,
    admission: PreparedCourtAdmissionV2<P>,
}
/// Successful supplied-store admission plus signature/amount preflight. If the
/// store is private accounting this remains PRIVATE, never a native UTXO lock.
pub struct PreflightCheckedAdmissionV2<C> {
    funding: SignatureCheckedFundingPreflightV2,
    admitted: AdmittedCourtV2<C>,
}
impl<C> PreflightCheckedAdmissionV2<C> {
    pub fn funding(&self) -> &SignatureCheckedFundingPreflightV2 {
        &self.funding
    }
    pub fn admitted(&self) -> &AdmittedCourtV2<C> {
        &self.admitted
    }
    pub fn into_parts(self) -> (SignatureCheckedFundingPreflightV2, AdmittedCourtV2<C>) {
        (self.funding, self.admitted)
    }
}
impl<V: RootedFundingViewV2> FundingPreflightCoordinatorV2<V> {
    fn check_atomic_binding<S: AtomicCourtStore, A: CourtAdmissionViewV2>(
        &self,
        admission: &AtomicAdmissionCoordinatorV2<S, A>,
        prepared: &PreparedCourtAdmissionV2<S::PreparedAdmission>,
        checked: &SignatureCheckedFundingPreflightV2,
    ) -> Result<(), FundingAtomicErrorV2<V::Error, A::Error, S::Error>> {
        let root = self.view.root();
        let resolved = prepared.resolved();
        if root != admission.view().root()
            || root.scope != checked.root.scope
            || root.generation != checked.root.generation
            || root.now < checked.root.now
            || root.scope != prepared.root().scope
            || root.generation != prepared.root().generation
            || root.now < prepared.root().now
            || prepared.result() != checked.result()
            || resolved.service != *checked.service()
            || resolved.context != checked.quote.context
            || resolved.intent.scope != checked.root.scope
            || resolved.intent.case != checked.result.case_id()
            || resolved.intent.result != checked.result.id()
            || resolved.intent.resource_quote != checked.quote.basis.resource_quote
            || resolved.intent.funding_consent != checked.consent.id().0
            || resolved.intent.expiry > checked.consent.admission_expiry
        {
            return Err(FundingAtomicErrorV2::Binding);
        }
        Ok(())
    }

    pub fn prepare_atomic<S: AtomicCourtStore, A: CourtAdmissionViewV2>(
        &self,
        admission: &AtomicAdmissionCoordinatorV2<S, A>,
        inputs: SignedFundingInputsV2<'_>,
        carrier: &V::Carrier,
    ) -> Result<PreparedFundingPreflightAdmissionV2<S::PreparedAdmission>, FundingAtomicErrorV2<V::Error, A::Error, S::Error>> {
        let funding = self.check(inputs, carrier).map_err(FundingAtomicErrorV2::Funding)?;
        let prepared = admission
            .prepare(inputs.result.clone(), inputs.miner_key, inputs.miner_signature)
            .map_err(FundingAtomicErrorV2::Admission)?;
        self.check_atomic_binding(admission, &prepared, &funding)?;
        Ok(PreparedFundingPreflightAdmissionV2 { funding, admission: prepared })
    }

    pub fn commit_atomic<S: AtomicCourtStore, A: CourtAdmissionViewV2>(
        &self,
        admission: &mut AtomicAdmissionCoordinatorV2<S, A>,
        prepared: PreparedFundingPreflightAdmissionV2<S::PreparedAdmission>,
        carrier: &V::Carrier,
    ) -> Result<PreflightCheckedAdmissionV2<S::AdmittedCase>, FundingAtomicErrorV2<V::Error, A::Error, S::Error>> {
        self.revalidate(&prepared.funding, carrier).map_err(FundingAtomicErrorV2::Funding)?;
        self.check_atomic_binding(admission, &prepared.admission, &prepared.funding)?;
        // The native store must resolve this same actual carrier/outpoint from
        // its validated transaction context and require the composed UTXO/lock
        // evidence. Its single transaction owns every row/resource/coin effect;
        // a native-unassigned store error is preserved. No second ledger exists.
        let admitted = admission.commit(prepared.admission).map_err(FundingAtomicErrorV2::Admission)?;
        Ok(PreflightCheckedAdmissionV2 { funding: prepared.funding, admitted })
    }
}

#[cfg(test)]
#[path = "funding_tests.rs"]
mod tests;
