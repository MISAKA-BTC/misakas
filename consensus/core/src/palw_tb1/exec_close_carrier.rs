//! Input evidence carried by an ordinary lifecycle transaction in an isolated
//! EXEC close block. The transaction does not change the lifecycle state. Its
//! evidence is consumed only with the close's freeze marker and checked rules.
use super::{
    exec_registration::{ExecInputSetBinding, ExecRegistration, ExecRegistrationError, SignedExecRegistration, registration_key},
    freeze::{Tb1MiningClosure, Tb1MiningPurpose},
};
use crate::{
    Hash64,
    block::Block,
    header::Header,
    palw_model_market_v1::moe2::settlement::{Moe2ExecSlot, Moe2PreDrawQuote},
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::Transaction,
};
use borsh::{BorshDeserialize, BorshSerialize};
use libcrux_ml_dsa::{ml_dsa_65::MLDSA65Signature, ml_dsa_87::MLDSA87Signature};
use std::collections::BTreeMap;

pub const EXEC_CLOSE_CARRIER_TAG: &[u8; 8] = b"TB1EXC01";
const OWNER_SIGNATURE_BYTES: usize = 3309;
const FUNDER_SIGNATURE_BYTES: usize = 4627;
pub(crate) const MAX_CONTEXT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecCloseCarrierError {
    Encoding,
    Limit,
    SlotOrder,
    Multiple,
    Missing,
    Position,
    Freeze,
    Registration(ExecRegistrationError),
}

struct Record<'a> {
    slot: u64,
    funding_ref: [u8; 64],
    context: &'a [u8],
    owner_signature: MLDSA65Signature,
    quote_consent: Option<MLDSA87Signature>,
}

pub struct ExecCloseCarrier<'a> {
    pub closure: Tb1MiningClosure,
    pub preparation: Hash64,
    pub qualified_rounds: Hash64,
    records: Vec<Record<'a>>,
}

fn take<'a, const N: usize>(bytes: &mut &'a [u8]) -> Result<[u8; N], ExecCloseCarrierError> {
    take_ref(bytes).copied()
}

fn carrier_payload(txs: &[Transaction]) -> Result<&[u8], ExecCloseCarrierError> {
    use ExecCloseCarrierError::*;
    let mut found =
        txs.iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(EXEC_CLOSE_CARRIER_TAG));
    let payload = &found.next().ok_or(Missing)?.payload;
    if found.next().is_some() {
        return Err(Multiple);
    }
    Ok(payload)
}

struct RecordBytes<'a> {
    slot: u64,
    funding_ref: [u8; 64],
    context: &'a [u8],
    owner_signature: &'a [u8; OWNER_SIGNATURE_BYTES],
    quote_consent: Option<&'a [u8; FUNDER_SIGNATURE_BYTES]>,
}

fn take_ref<'a, const N: usize>(bytes: &mut &'a [u8]) -> Result<&'a [u8; N], ExecCloseCarrierError> {
    let (value, tail) = bytes.split_first_chunk::<N>().ok_or(ExecCloseCarrierError::Encoding)?;
    *bytes = tail;
    Ok(value)
}

impl<'a> RecordBytes<'a> {
    fn read(bytes: &mut &'a [u8]) -> Result<Self, ExecCloseCarrierError> {
        use ExecCloseCarrierError::*;
        let slot = u64::from_be_bytes(take(bytes)?);
        let funding_ref = take(bytes)?;
        let len = usize::from(u16::from_be_bytes(take(bytes)?));
        if len == 0 || len > MAX_CONTEXT_BYTES {
            return Err(Limit);
        }
        let (context, tail) = bytes.split_at_checked(len).ok_or(Encoding)?;
        *bytes = tail;
        let owner_signature = take_ref(bytes)?;
        let quote_consent = match take::<1>(bytes)? {
            [0] => None,
            [1] => Some(take_ref(bytes)?),
            _ => return Err(Encoding),
        };
        Ok(Self { slot, funding_ref, context, owner_signature, quote_consent })
    }
}

impl<'a> ExecCloseCarrier<'a> {
    pub fn closure_from_block(block: &'a Block, max_bytes: usize) -> Result<Tb1MiningClosure, ExecCloseCarrierError> {
        Ok(Self::header_from_block(block, max_bytes)?.closure)
    }

    pub fn preparation_from_block(block: &'a Block, max_bytes: usize) -> Result<Hash64, ExecCloseCarrierError> {
        Ok(Self::header_from_block(block, max_bytes)?.preparation)
    }

    fn header_from_block(block: &'a Block, max_bytes: usize) -> Result<Self, ExecCloseCarrierError> {
        let payload = carrier_payload(&block.transactions)?;
        if payload.len() > max_bytes {
            return Err(ExecCloseCarrierError::Limit);
        }
        Ok(Self::read_prefix(payload, usize::from(u16::MAX))?.0)
    }

    /// Locate the single carrier in a body already checked against its header.
    /// Looking at the coinbase alone cannot authenticate the allocation set.
    pub fn from_block(block: &'a Block, max_records: usize, max_bytes: usize) -> Result<Self, ExecCloseCarrierError> {
        Self::decode(carrier_payload(&block.transactions)?, max_records, max_bytes)
    }

    pub fn decode(payload: &'a [u8], max_records: usize, max_bytes: usize) -> Result<Self, ExecCloseCarrierError> {
        use ExecCloseCarrierError::*;
        if payload.len() > max_bytes {
            return Err(Limit);
        }
        let (mut carrier, count, mut bytes) = Self::read_prefix(payload, max_records)?;
        // Slot, funding reference, context length, at least one context byte,
        // owner signature and consent tag must exist before allocating rows.
        if count > bytes.len() / (8 + 64 + 2 + 1 + 3309 + 1) {
            return Err(Encoding);
        }
        carrier.records.reserve_exact(count);
        let mut prior = None;
        for _ in 0..count {
            let row = RecordBytes::read(&mut bytes)?;
            if prior.is_some_and(|slot| slot >= row.slot) {
                return Err(SlotOrder);
            }
            prior = Some(row.slot);
            carrier.records.push(Record {
                slot: row.slot,
                funding_ref: row.funding_ref,
                context: row.context,
                owner_signature: MLDSA65Signature::new(*row.owner_signature),
                quote_consent: row.quote_consent.map(|consent| MLDSA87Signature::new(*consent)),
            });
        }
        if !bytes.is_empty() {
            return Err(Encoding);
        }
        Ok(carrier)
    }

    fn read_prefix(payload: &'a [u8], max_records: usize) -> Result<(Self, usize, &'a [u8]), ExecCloseCarrierError> {
        use ExecCloseCarrierError::*;
        let mut bytes = payload.strip_prefix(EXEC_CLOSE_CARRIER_TAG).ok_or(Encoding)?;
        let closure = Tb1MiningClosure::deserialize(&mut bytes).map_err(|_| Encoding)?;
        if closure.round.purpose != Tb1MiningPurpose::Exec {
            return Err(Encoding);
        }
        let preparation = Hash64::from_bytes(take(&mut bytes)?);
        let qualified_rounds = Hash64::from_bytes(take(&mut bytes)?);
        let count = usize::from(u16::from_be_bytes(take(&mut bytes)?));
        // Each row has the same minimum size even if its context and consent
        // were absent. Refuse implausible counts before allocating signatures.
        if count > max_records || count > bytes.len() / (8 + 64 + 2 + OWNER_SIGNATURE_BYTES + 1) {
            return Err(Limit);
        }
        Ok((Self { closure, preparation, qualified_rounds, records: Vec::new() }, count, bytes))
    }

    /// Recover the preimage of a reservation committed by a selected-parent
    /// state. The caller supplies the stored close payload after checking the row
    /// and the close's ancestry. The row already authenticates the signed
    /// registration and quote, so historic signatures need no second pass.
    pub fn read_reserved_slot(
        close: &Header,
        payload: &'a [u8],
        wanted_slot: u64,
        reservation: Hash64,
    ) -> Result<(Hash64, Moe2ExecSlot), ExecCloseCarrierError> {
        let (carrier, row) = Self::read_slot(payload, wanted_slot)?;
        let registration = Self::decode_slot(&carrier, &row)?;
        if row.quote_consent.is_none() || row.funding_ref != Moe2ExecSlot::consent_ref(registration.key, row.slot) {
            return Err(ExecCloseCarrierError::Registration(ExecRegistrationError::FundingReference));
        }
        Self::bind_reserved_registration(close.hash, close.daa_score, row.slot, reservation, registration)
    }

    /// Read the registration from the exact close already authenticated by
    /// the selected branch, including closes that made no mining reservation.
    pub(super) fn read_accepted_registration(
        block: &'a Block,
        receipt: &super::moe2_exec_close::Moe2ExecCloseReceipt,
        slot: u64,
    ) -> Result<(&'a [u8], ExecRegistration<'a>), ExecCloseCarrierError> {
        let (carrier, row) = Self::read_slot(carrier_payload(&block.transactions)?, slot)?;
        if receipt.block != block.hash()
            || receipt.preparation != carrier.preparation
            || receipt.freeze_id != carrier.closure.commitment(receipt.selected_parent)
        {
            return Err(ExecCloseCarrierError::Freeze);
        }
        Ok((row.context, Self::decode_slot(&carrier, &row)?))
    }

    fn decode_slot(carrier: &Self, row: &RecordBytes<'a>) -> Result<ExecRegistration<'a>, ExecCloseCarrierError> {
        let registration =
            ExecRegistration::decode(row.context, registration_key(row.context)).map_err(ExecCloseCarrierError::Registration)?;
        if (registration.preparation, registration.qualified_rounds) != (carrier.preparation, carrier.qualified_rounds) {
            return Err(ExecCloseCarrierError::Registration(ExecRegistrationError::Context));
        }
        Ok(registration)
    }

    fn read_slot(payload: &'a [u8], wanted_slot: u64) -> Result<(Self, RecordBytes<'a>), ExecCloseCarrierError> {
        use ExecCloseCarrierError::*;
        let (carrier, count, mut bytes) = Self::read_prefix(payload, usize::from(u16::MAX))?;
        let mut prior = None;
        let mut result = None;
        for _ in 0..count {
            let row = RecordBytes::read(&mut bytes)?;
            if prior.is_some_and(|slot| slot >= row.slot) {
                return Err(SlotOrder);
            }
            prior = Some(row.slot);
            if row.slot == wanted_slot {
                result = Some(row);
            }
        }
        if !bytes.is_empty() {
            return Err(Encoding);
        }
        Ok((carrier, result.ok_or(Missing)?))
    }

    /// Open the already committed reservation with the owner's original
    /// registration preimage. The native parent reservation, not this
    /// transport, is the authority that the close accepted the signatures.
    pub(crate) fn open_reserved_context(
        close: Hash64,
        close_daa: u64,
        slot: u64,
        reservation: Hash64,
        context: &[u8],
    ) -> Result<(Hash64, Moe2ExecSlot), ExecCloseCarrierError> {
        use ExecCloseCarrierError::*;
        if context.is_empty() || context.len() > MAX_CONTEXT_BYTES {
            return Err(Limit);
        }
        let registration = ExecRegistration::decode(context, registration_key(context)).map_err(Registration)?;
        Self::bind_reserved_registration(close, close_daa, slot, reservation, registration)
    }

    fn bind_reserved_registration(
        close: Hash64,
        close_daa: u64,
        slot: u64,
        reservation: Hash64,
        registration: ExecRegistration<'_>,
    ) -> Result<(Hash64, Moe2ExecSlot), ExecCloseCarrierError> {
        use ExecCloseCarrierError::Registration;
        let quote = registration.quote.ok_or(Registration(ExecRegistrationError::FundingReference))?;
        let resolved = Moe2ExecSlot { registration: registration.key, slot, close, close_daa, quote };
        if resolved.reservation_key() != reservation {
            return Err(Registration(ExecRegistrationError::Context));
        }
        Ok((registration.preparation, resolved))
    }

    pub fn encode(
        closure: &Tb1MiningClosure,
        preparation: Hash64,
        qualified_rounds: Hash64,
        records: &[SignedExecRegistration<'_>],
        max_records: usize,
        max_bytes: usize,
    ) -> Result<Vec<u8>, ExecCloseCarrierError> {
        use ExecCloseCarrierError::*;
        let count = u16::try_from(records.len()).map_err(|_| Limit)?;
        if records.len() > max_records {
            return Err(Limit);
        }
        if closure.round.purpose != Tb1MiningPurpose::Exec {
            return Err(Encoding);
        }
        if records.windows(2).any(|pair| pair[0].slot >= pair[1].slot) {
            return Err(SlotOrder);
        }
        let mut bytes = EXEC_CLOSE_CARRIER_TAG.to_vec();
        closure.serialize(&mut bytes).map_err(|_| Encoding)?;
        bytes.extend_from_slice(preparation.as_byte_slice());
        bytes.extend_from_slice(qualified_rounds.as_byte_slice());
        bytes.extend_from_slice(&count.to_be_bytes());
        for row in records {
            if row.context.is_empty() || row.context.len() > MAX_CONTEXT_BYTES {
                return Err(Limit);
            }
            let row_bytes = 8
                + 64
                + 2
                + row.context.len()
                + OWNER_SIGNATURE_BYTES
                + 1
                + usize::from(row.quote_consent.is_some()) * FUNDER_SIGNATURE_BYTES;
            if bytes.len().checked_add(row_bytes).is_none_or(|n| n > max_bytes) {
                return Err(Limit);
            }
            bytes.extend_from_slice(&row.slot.to_be_bytes());
            bytes.extend_from_slice(&row.funding_ref);
            bytes.extend_from_slice(&(row.context.len() as u16).to_be_bytes());
            bytes.extend_from_slice(row.context);
            bytes.extend_from_slice(row.owner_signature.as_slice());
            bytes.push(u8::from(row.quote_consent.is_some()));
            if let Some(consent) = row.quote_consent {
                bytes.extend_from_slice(consent.as_slice());
            }
        }
        if bytes.len() > max_bytes {
            return Err(Limit);
        }
        Ok(bytes)
    }

    /// Use the candidate's actual parent and its coinbase's freeze marker.
    /// The caller checks the preparation, qualified dealer roster, signed
    /// schedules and mining eligibility independently before a reservation.
    pub fn verify<'state>(
        &self,
        parent: Hash64,
        blue_score: u64,
        daa_score: u64,
        freeze_marker: [u8; 64],
        funder_at_preparation: impl FnMut(&Moe2PreDrawQuote) -> Option<&'state [u8]>,
    ) -> Result<BTreeMap<u64, ExecRegistration<'_>>, ExecCloseCarrierError> {
        use ExecCloseCarrierError::*;
        let round = &self.closure.round;
        if blue_score != round.close_blue_score
            || daa_score >= round.release_from_daa
            || round.release_from_daa >= round.deadline_exclusive_daa
        {
            return Err(Position);
        }
        if self.closure.commitment(parent) != freeze_marker {
            return Err(Freeze);
        }
        let witnesses: Vec<_> = self
            .records
            .iter()
            .map(|row| SignedExecRegistration {
                context: row.context,
                owner_signature: &row.owner_signature,
                slot: row.slot,
                funding_ref: row.funding_ref,
                quote_consent: row.quote_consent.as_ref(),
            })
            .collect();
        ExecInputSetBinding {
            preparation: self.preparation,
            qualified_rounds: self.qualified_rounds,
            consumer_set_root: self.closure.consumer_set_root,
        }
        .verify(&witnesses, self.records.len(), funder_at_preparation)
        .map_err(Registration)
    }
}
