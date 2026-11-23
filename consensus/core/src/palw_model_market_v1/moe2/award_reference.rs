//! A private MOE2 award's lookup keys in its own block body. The carrier
//! points to the winning right and reservation that the block owner must
//! authenticate from branch state before settlement.
use super::settlement::Moe2ExecSlot;
use crate::{Hash64, subnets::SUBNETWORK_ID_PALW_LIFECYCLE, tx::Transaction};

const TAG: &[u8; 8] = b"MOE2REF1";
const LENGTH: usize = 8 + 64 + 32 + 64 + 8 + 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2AwardReference {
    pub claim: Hash64,
    pub attempt: [u8; 32],
    pub close: Hash64,
    pub slot: u64,
    pub reservation: Hash64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moe2AwardReferenceError {
    Missing,
    Multiple,
    Encoding,
}

impl Moe2AwardReferenceError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing private MOE2 award reference in the candidate",
            Self::Multiple => "multiple private MOE2 award references in the candidate",
            Self::Encoding => "invalid private MOE2 award reference encoding",
        }
    }
}

impl Moe2AwardReference {
    pub fn for_slot(claim: Hash64, attempt: [u8; 32], slot: &Moe2ExecSlot) -> Self {
        Self { claim, attempt, close: slot.close, slot: slot.slot, reservation: slot.reservation_key() }
    }

    pub fn encode(&self) -> [u8; LENGTH] {
        let mut out = [0; LENGTH];
        out[..8].copy_from_slice(TAG);
        out[8..72].copy_from_slice(self.claim.as_byte_slice());
        out[72..104].copy_from_slice(&self.attempt);
        out[104..168].copy_from_slice(self.close.as_byte_slice());
        out[168..176].copy_from_slice(&self.slot.to_be_bytes());
        out[176..].copy_from_slice(self.reservation.as_byte_slice());
        out
    }

    pub fn decode(payload: &[u8]) -> Result<Self, Moe2AwardReferenceError> {
        if payload.len() != LENGTH || !payload.starts_with(TAG) {
            return Err(Moe2AwardReferenceError::Encoding);
        }
        Ok(Self {
            claim: Hash64::from_bytes(payload[8..72].try_into().unwrap()),
            attempt: payload[72..104].try_into().unwrap(),
            close: Hash64::from_bytes(payload[104..168].try_into().unwrap()),
            slot: u64::from_be_bytes(payload[168..176].try_into().unwrap()),
            reservation: Hash64::from_bytes(payload[176..].try_into().unwrap()),
        })
    }

    /// Use the candidate's transactions, in isolation from mergeset carriers.
    pub fn from_transactions<'a>(txs: impl IntoIterator<Item = &'a Transaction>) -> Result<Self, Moe2AwardReferenceError> {
        let mut candidates =
            txs.into_iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(TAG));
        let candidate = candidates.next().ok_or(Moe2AwardReferenceError::Missing)?;
        if candidates.next().is_some() {
            return Err(Moe2AwardReferenceError::Multiple);
        }
        Self::decode(&candidate.payload)
    }
}
