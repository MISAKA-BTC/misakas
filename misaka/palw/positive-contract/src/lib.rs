//! A0 common contract, revision 1. No native event, court, payment or credit.
//! IDs and DTOs are not authenticated state capabilities. Implementations of
//! ports belong to A1--A6; independent acceptance tests belong to A7.
pub mod assignment;
pub mod certification;
pub mod funding;
pub mod identity;
pub mod journal;
pub mod messages;
pub mod native;
pub mod ports;
pub mod record;
pub mod reservations;
pub mod rows;
pub mod tir;
pub mod wire;

pub const CONTRACT_VERSION: u16 = 2;
pub const CONTRACT_REVISION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    Version,
    Scope,
    Identity,
    Signature,
    Range,
    Budget,
    NonCanonical,
    UnsupportedProfile,
    NativeAdapterUnassigned,
}

/// No public-network native tag/height is allocated by this shared API change.
pub fn require_native_activation() -> Result<std::convert::Infallible, ContractError> {
    Err(ContractError::NativeAdapterUnassigned)
}
