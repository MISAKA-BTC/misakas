//! A private MOE2 award's lookup keys in its own block body. The carrier
//! points to the winning right and reservation that the block owner must
//! authenticate from branch state before settlement.
use super::settlement::Moe2ExecSlot;
use crate::{
    Hash64,
    palw_bond_budget_v1::{PalwBudgetOriginV1, PalwBudgetVectorV1},
    palw_state_v2::PalwChainStateV2,
    palw_tb1::exec_close_carrier::{ExecCloseCarrier, MAX_CONTEXT_BYTES},
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::Transaction,
};

const TAG: &[u8; 8] = b"MOE2REF1";
const CONTEXT_TAG: &[u8; 8] = b"MOE2REF2";
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
    Context,
    ExecutionMismatch,
    ClaimAlreadySettled,
    AttemptAlreadySettled,
    MissingReservation,
    ReservationOrigin,
    ReservationSlotMismatch,
    ReservationWindowNotOpen { opens_at: u64 },
    ReservationWindowExpired { expires_at: u64 },
    ReservationUnavailable,
}

impl Moe2AwardReferenceError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing private MOE2 award reference in the candidate",
            Self::Multiple => "multiple private MOE2 award references in the candidate",
            Self::Encoding => "invalid private MOE2 award reference encoding",
            Self::Context => "private MOE2 award context does not open its EXEC reservation",
            Self::ExecutionMismatch => "private MOE2 award differs from its candidate reference",
            Self::ClaimAlreadySettled => "private MOE2 claim has already settled on this branch",
            Self::AttemptAlreadySettled => "private MOE2 attempt has already settled on this branch",
            Self::MissingReservation => "private MOE2 reservation is absent from the selected parent",
            Self::ReservationOrigin => "private MOE2 reservation has no bound EXEC close and slot",
            Self::ReservationSlotMismatch => "private MOE2 reservation names a different EXEC close or slot",
            Self::ReservationWindowNotOpen { .. } => "private MOE2 reservation window has not opened",
            Self::ReservationWindowExpired { .. } => "private MOE2 reservation window has expired",
            Self::ReservationUnavailable => "private MOE2 reservation is no longer available at this DAA",
        }
    }
}

impl std::fmt::Display for Moe2AwardReferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())?;
        match self {
            Self::ReservationWindowNotOpen { opens_at } => write!(f, " (opens at DAA {opens_at})"),
            Self::ReservationWindowExpired { expires_at } => write!(f, " (expired at DAA {expires_at})"),
            _ => Ok(()),
        }
    }
}

impl std::error::Error for Moe2AwardReferenceError {}

impl Moe2AwardReference {
    pub fn for_slot(claim: Hash64, attempt: [u8; 32], slot: &Moe2ExecSlot) -> Self {
        Self { claim, attempt, close: slot.close, slot: slot.slot, reservation: slot.reservation_key() }
    }

    /// Check every lookup key against the authenticated Original and slot.
    pub fn check_execution_slot(&self, claim: Hash64, attempt: [u8; 32], slot: &Moe2ExecSlot) -> Result<(), Moe2AwardReferenceError> {
        if *self != Self::for_slot(claim, attempt, slot) {
            return Err(Moe2AwardReferenceError::ExecutionMismatch);
        }
        Ok(())
    }

    /// Check the selected parent's replay indexes before loading archived
    /// evidence. The numerical key is checked once the EXEC input is resolved.
    pub fn check_parent_unspent(&self, parent: &PalwChainStateV2) -> Result<(), Moe2AwardReferenceError> {
        if parent.private_moe2_exec_settlement(&self.claim).is_some() {
            return Err(Moe2AwardReferenceError::ClaimAlreadySettled);
        }
        if parent.private_moe2_attempt_work(&self.attempt).is_some() {
            return Err(Moe2AwardReferenceError::AttemptAlreadySettled);
        }
        Ok(())
    }

    /// Resolve the reference against the selected parent's committed reservation.
    /// The reservation admits no claim or winner; Gate C remains a separate read.
    pub fn check_parent_reservation(&self, parent: &PalwChainStateV2, daa: u64) -> Result<(), Moe2AwardReferenceError> {
        let row = parent
            .bond_budget()
            .and_then(|budget| budget.claim_row(&self.reservation))
            .ok_or(Moe2AwardReferenceError::MissingReservation)?;
        let PalwBudgetOriginV1::Moe2ExecBound { expires_at, close, slot } = row.origin else {
            return Err(Moe2AwardReferenceError::ReservationOrigin);
        };
        if (close, slot) != (self.close, self.slot) {
            return Err(Moe2AwardReferenceError::ReservationSlotMismatch);
        }
        if daa < row.accepted_daa {
            return Err(Moe2AwardReferenceError::ReservationWindowNotOpen { opens_at: row.accepted_daa });
        }
        if daa >= expires_at {
            return Err(Moe2AwardReferenceError::ReservationWindowExpired { expires_at });
        }
        if !row.open || !row.live || row.reserved.claims != 1 || row.consumed != PalwBudgetVectorV1::ZERO {
            return Err(Moe2AwardReferenceError::ReservationUnavailable);
        }
        Ok(())
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
        Ok(Self::read_fields(payload))
    }

    fn read_fields(payload: &[u8]) -> Self {
        Self {
            claim: Hash64::from_bytes(payload[8..72].try_into().unwrap()),
            attempt: payload[72..104].try_into().unwrap(),
            close: Hash64::from_bytes(payload[104..168].try_into().unwrap()),
            slot: u64::from_be_bytes(payload[168..176].try_into().unwrap()),
            reservation: Hash64::from_bytes(payload[176..LENGTH].try_into().unwrap()),
        }
    }

    /// The original registration context can come with the candidate when
    /// the close's body is no longer available. V1 keeps its archive lookup.
    pub fn encode_with_registration(&self, context: &[u8]) -> Result<Vec<u8>, Moe2AwardReferenceError> {
        if context.is_empty() || context.len() > MAX_CONTEXT_BYTES {
            return Err(Moe2AwardReferenceError::Encoding);
        }
        let mut payload = self.encode().to_vec();
        payload[..CONTEXT_TAG.len()].copy_from_slice(CONTEXT_TAG);
        payload.extend_from_slice(context);
        Ok(payload)
    }

    /// Use the candidate's transactions, in isolation from mergeset carriers.
    pub fn from_transactions<'a>(txs: impl IntoIterator<Item = &'a Transaction>) -> Result<Self, Moe2AwardReferenceError> {
        Ok(Moe2AwardSource::from_transactions(txs)?.reference)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2AwardSource<'a> {
    pub reference: Moe2AwardReference,
    context: Option<&'a [u8]>,
}

impl<'a> Moe2AwardSource<'a> {
    pub fn archived(reference: Moe2AwardReference) -> Self {
        Self { reference, context: None }
    }

    pub fn from_transactions(txs: impl IntoIterator<Item = &'a Transaction>) -> Result<Self, Moe2AwardReferenceError> {
        use Moe2AwardReferenceError::*;
        let mut candidates = txs.into_iter().filter(|tx| {
            tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && (tx.payload.starts_with(TAG) || tx.payload.starts_with(CONTEXT_TAG))
        });
        let payload = &candidates.next().ok_or(Missing)?.payload;
        if candidates.next().is_some() {
            return Err(Multiple);
        }
        if payload.starts_with(TAG) {
            return Moe2AwardReference::decode(payload).map(Self::archived);
        }
        let context = payload.get(LENGTH..).filter(|bytes| !bytes.is_empty() && bytes.len() <= MAX_CONTEXT_BYTES).ok_or(Encoding)?;
        Ok(Self { reference: Moe2AwardReference::read_fields(payload), context: Some(context) })
    }

    pub(crate) fn registration_context(&self) -> Option<&'a [u8]> {
        self.context
    }

    /// Caller first checks the reservation in the selected parent. An invalid
    /// supplied preimage is refused without retrying through the close archive.
    pub fn open_registration(&self, close_daa: u64) -> Result<Option<(Hash64, Moe2ExecSlot)>, Moe2AwardReferenceError> {
        let r = self.reference;
        self.context
            .map(|context| {
                ExecCloseCarrier::open_reserved_context(r.close, close_daa, r.slot, r.reservation, context)
                    .map_err(|_| Moe2AwardReferenceError::Context)
            })
            .transpose()
    }
}
