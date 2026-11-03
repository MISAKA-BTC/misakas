//! Typed, bounded readers for RFC-0044 aux151. Missing and malformed rows are
//! distinct. Read/shape/link success is NOT native acceptance, local arithmetic
//! validation, a live coin lock or Gate C. The actual fold/VP owners supply those
//! capabilities using the same current state generation and accepted prefix.
use crate::palw_state_v2::PalwChainStateV2;
use misaka_palw_positive_contract::{
    ContractError,
    identity::*,
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::*,
};

pub mod assignment;
pub mod restore;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositiveRowErrorV2 {
    MissingKernelRoute,
    MissingRequiredRow,
    WrongRowKind,
    InconsistentLinks,
    Contract(ContractError),
}
impl From<ContractError> for PositiveRowErrorV2 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

/// Borrowed actual state; this reader cannot mutate, install or pay any row.
pub struct PositiveRowsV2<'a> {
    state: &'a PalwChainStateV2,
}
impl<'a> PositiveRowsV2<'a> {
    pub fn new(state: &'a PalwChainStateV2) -> Self {
        Self { state }
    }

    pub fn read(&self, key: NativeRowKey) -> Result<Option<NativeRowValue>, PositiveRowErrorV2> {
        let route = self.state.kernel_route_state_v1().ok_or(PositiveRowErrorV2::MissingKernelRoute)?;
        let key = key.encode();
        route
            .aux
            .get(&(KERNEL_AUX_TABLE, key.clone()))
            .map(|bytes| NativeRowValue::decode(&key, bytes).map_err(PositiveRowErrorV2::from))
            .transpose()
    }
    fn required(&self, key: NativeRowKey) -> Result<NativeRowBody, PositiveRowErrorV2> {
        Ok(self.read(key)?.ok_or(PositiveRowErrorV2::MissingRequiredRow)?.body)
    }

    /// Resolves the signed literal bundle, authenticates its signatures at the
    /// original admission clock and checks all local identity links. Origin
    /// ancestry/accepted-prefix, registered expectations, source supply, live
    /// UTXO/capital, replay and drawn approval remain independently mandatory.
    /// Expiry of new admission does not make an existing funding lock disappear.
    pub fn bundle(&self, claim: ClaimId) -> Result<PositiveRowBundleV2, PositiveRowErrorV2> {
        let NativeRowBody::ClaimIndex(index) = self.required(NativeRowKey::ClaimIndex(claim))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::Result(result) = self.required(NativeRowKey::Result(index.result))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::Case(case) = self.required(NativeRowKey::Case(index.case))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        let NativeRowBody::Funding(funding) = self.required(NativeRowKey::Funding(index.case))? else {
            return Err(PositiveRowErrorV2::WrongRowKind);
        };
        validate_positive_bundle_v2(&index, &result, &case, &funding)?;
        Ok(PositiveRowBundleV2 { index, result, case, funding })
    }
}

/// Structural/authenticated DTO bundle only. Public fields cannot mint credit
/// or a spend. Never accept one of these from a peer as an executed native view.
pub struct PositiveRowBundleV2 {
    pub index: ClaimIndexRow,
    pub result: ResultRow,
    pub case: CaseRow,
    pub funding: FundingRow,
}

pub fn validate_positive_bundle_v2(
    index: &ClaimIndexRow,
    result: &ResultRow,
    case: &CaseRow,
    funding: &FundingRow,
) -> Result<(), PositiveRowErrorV2> {
    validate_bundle(index, result, case, funding).map_err(PositiveRowErrorV2::from)
}

// Compile-time type checks prevent truncation if native consensus aliases change.
fn native_hash64_from_block(hash: crate::BlockHash) -> NativeHash64 {
    NativeHash64(hash.as_bytes())
}
fn native_hash64_from_transaction(hash: crate::TransactionId) -> NativeHash64 {
    NativeHash64(hash.as_bytes())
}
