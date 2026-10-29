//! ADR-0207 ordinary verifier fee rows; aux 142 prefixes 4 and 5.
use borsh::{BorshDeserialize, BorshSerialize};
pub use misaka_palw_kernel::verifier_fee_consent_v1::VerifierFeeConsentV1;
pub const TABLE: u8 = 142;
pub const CONSENT: u8 = 4;
pub const HELD: u8 = 5;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HeldVerifierFeeV1 {
    pub terms: VerifierFeeConsentV1,
    pub amount: u64,
    pub convicted_daa: Option<u64>,
}
pub fn fee_key(prefix: u8, claim: &[u8; 64], verifier: &[u8; 64], slot: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(137);
    key.push(prefix);
    key.extend_from_slice(claim);
    key.extend_from_slice(verifier);
    key.extend_from_slice(&slot.to_le_bytes());
    key
}

/// Exact slot identity shared by ordinary and paid-trap recipient R reservations.
pub fn verifier_fee_income_context_v1(claim: [u8; 64], verifier: [u8; 64], slot: u64) -> crate::Hash64 {
    crate::Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/VERIFIER-FEE-INCOME/V1", &(claim, verifier, slot)))
}
