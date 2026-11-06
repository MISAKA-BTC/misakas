//! Thin A5 input reader over the actual native aux151 typed reader.
//!
//! Successful decoding, signed links and snapshot membership are DATA facts.
//! This module deliberately cannot construct native admission, local replay,
//! live funding, ancestry or drawn-P approval capabilities. No legacy S/K/V
//! lookup is attempted when a Positive row is missing or malformed.

use kaspa_consensus_core::{
    Hash64,
    palw_positive_rows_v2::{PositiveRowBundleV2, PositiveRowErrorV2, PositiveRowsV2},
    palw_state_v2::PalwChainStateV2,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::ClaimId,
    native::NativeRowKey,
    ports::{CreditGate, CreditUse},
    rows::{CoverageRow, NativeRowBody, NativeRowValue, WitnessRow, validate_witness_bundle},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthenticationStage {
    ResultAndFunding,
    Witness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditRowFailure {
    MissingKernelRoute,
    MissingRow(NativeRowKey),
    MalformedRow { key: NativeRowKey, cause: ContractError },
    WrongRowKind(NativeRowKey),
    BundleLinks(ContractError),
    Authentication { stage: AuthenticationStage, cause: ContractError },
    Core(PositiveRowErrorV2),
}

/// These owners have not yet supplied the native associated private-field
/// capabilities. Their serialized references/flags cannot replace them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingNativeOwner {
    AcceptedOrigin,
    AcceptedPrefixAndImportGeneration,
    RegisteredExpectations,
    LocalReplay,
    LiveFundingAndCapital,
    DependenciesAndExports,
    CurrentCourtAndResources,
    ActualPDrawCertified,
}

pub const MISSING_NATIVE_OWNERS: [MissingNativeOwner; 8] = [
    MissingNativeOwner::AcceptedOrigin,
    MissingNativeOwner::AcceptedPrefixAndImportGeneration,
    MissingNativeOwner::RegisteredExpectations,
    MissingNativeOwner::LocalReplay,
    MissingNativeOwner::LiveFundingAndCapital,
    MissingNativeOwner::DependenciesAndExports,
    MissingNativeOwner::CurrentCourtAndResources,
    MissingNativeOwner::ActualPDrawCertified,
];

/// Bound to the actual borrowed state read, with no public DTO constructor or
/// deserializer. This remains an authenticated DATA bundle, not authority.
pub struct SignedCreditInputs<'a> {
    state: &'a PalwChainStateV2,
    bundle: PositiveRowBundleV2,
    witness: Option<(WitnessRow, CoverageRow)>,
}
impl SignedCreditInputs<'_> {
    pub fn bundle(&self) -> &PositiveRowBundleV2 {
        &self.bundle
    }
    pub fn witness_and_coverage(&self) -> Option<(&WitnessRow, &CoverageRow)> {
        self.witness.as_ref().map(|(witness, coverage)| (witness, coverage))
    }
    /// The source root identifies this data read. It does not establish origin
    /// acceptance, a replayed calculation, current locks or certification.
    pub fn source_state_root(&self) -> Hash64 {
        self.state.state_root()
    }
    pub fn missing_native_owners(&self) -> &'static [MissingNativeOwner] {
        &MISSING_NATIVE_OWNERS
    }
}

pub struct PositiveCreditRows<'a> {
    state: &'a PalwChainStateV2,
}
impl<'a> PositiveCreditRows<'a> {
    pub fn new(state: &'a PalwChainStateV2) -> Self {
        Self { state }
    }

    pub fn read(&self, key: NativeRowKey) -> Result<Option<NativeRowValue>, CreditRowFailure> {
        PositiveRowsV2::new(self.state).read(key).map_err(|error| classify_read_error(key, error))
    }

    fn required(&self, key: NativeRowKey) -> Result<NativeRowBody, CreditRowFailure> {
        Ok(self.read(key)?.ok_or(CreditRowFailure::MissingRow(key))?.body)
    }

    /// Core `bundle` authenticates the literal Result/funding signatures at
    /// their original admission time. On failure, read-only probes distinguish
    /// an absent/malformed component from a cross-row link/authentication fault.
    pub fn resolve(&self, claim: ClaimId) -> Result<SignedCreditInputs<'a>, CreditRowFailure> {
        let bundle = match PositiveRowsV2::new(self.state).bundle(claim) {
            Ok(bundle) => bundle,
            Err(error) => return Err(self.diagnose_bundle_failure(claim, error)),
        };
        let witness = if let Some(id) = bundle.case.positive_witness {
            let witness_key = NativeRowKey::Witness(id);
            let coverage_key = NativeRowKey::Coverage(id);
            let NativeRowBody::Witness(witness) = self.required(witness_key)? else {
                return Err(CreditRowFailure::WrongRowKind(witness_key));
            };
            let NativeRowBody::Coverage(coverage) = self.required(coverage_key)? else {
                return Err(CreditRowFailure::WrongRowKind(coverage_key));
            };
            validate_witness_bundle(&bundle.result, &bundle.case, &witness, &coverage)
                .map_err(|error| classify_bundle_error(AuthenticationStage::Witness, error))?;
            Some((witness, coverage))
        } else {
            None
        };
        Ok(SignedCreditInputs { state: self.state, bundle, witness })
    }

    fn diagnose_bundle_failure(&self, claim: ClaimId, error: PositiveRowErrorV2) -> CreditRowFailure {
        let index_key = NativeRowKey::ClaimIndex(claim);
        let index = match self.required(index_key) {
            Ok(NativeRowBody::ClaimIndex(index)) => index,
            Ok(_) => return CreditRowFailure::WrongRowKind(index_key),
            Err(error) => return error,
        };
        for key in [NativeRowKey::Result(index.result), NativeRowKey::Case(index.case), NativeRowKey::Funding(index.case)] {
            if let Err(error) = self.required(key) {
                return error;
            }
        }
        // All individual typed rows are present and shape-valid. The core
        // failure therefore belongs to the signed cross-row bundle boundary.
        match error {
            PositiveRowErrorV2::Contract(error) => classify_bundle_error(AuthenticationStage::ResultAndFunding, error),
            other => CreditRowFailure::Core(other),
        }
    }
}

fn classify_read_error(key: NativeRowKey, error: PositiveRowErrorV2) -> CreditRowFailure {
    match error {
        PositiveRowErrorV2::MissingKernelRoute => CreditRowFailure::MissingKernelRoute,
        PositiveRowErrorV2::MissingRequiredRow => CreditRowFailure::MissingRow(key),
        PositiveRowErrorV2::WrongRowKind => CreditRowFailure::WrongRowKind(key),
        PositiveRowErrorV2::Contract(cause) => CreditRowFailure::MalformedRow { key, cause },
        other => CreditRowFailure::Core(other),
    }
}

fn classify_bundle_error(stage: AuthenticationStage, cause: ContractError) -> CreditRowFailure {
    if cause == ContractError::Signature {
        CreditRowFailure::Authentication { stage, cause }
    } else {
        CreditRowFailure::BundleLinks(cause)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRowCreditDenial {
    Rows(CreditRowFailure),
    MissingOwner(MissingNativeOwner),
}

/// Native-row input boundary only. There is intentionally no success type:
/// every consumer gets the same current read failure or typed missing owner.
/// An authenticated DTO bundle or a P statement signature is not enough.
pub struct NativeRowCreditGate<'a> {
    rows: PositiveCreditRows<'a>,
}
impl<'a> NativeRowCreditGate<'a> {
    pub fn new(state: &'a PalwChainStateV2) -> Self {
        Self { rows: PositiveCreditRows::new(state) }
    }
}
impl CreditGate for NativeRowCreditGate<'_> {
    type RootedClaim = ClaimId;
    type Authorization = std::convert::Infallible;
    type Denial = NativeRowCreditDenial;
    fn authorize(&self, claim: &ClaimId, usage: CreditUse) -> Result<Self::Authorization, Self::Denial> {
        match usage {
            CreditUse::Reward
            | CreditUse::WorkReceipt
            | CreditUse::DagWeight
            | CreditUse::RuleE
            | CreditUse::Settlement
            | CreditUse::PrefixReuse
            | CreditUse::Dependency
            | CreditUse::WorkerPayment => {}
        }
        let inputs = self.rows.resolve(*claim).map_err(NativeRowCreditDenial::Rows)?;
        Err(NativeRowCreditDenial::MissingOwner(inputs.missing_native_owners()[0]))
    }
}

#[cfg(test)]
mod tests;
