//! Signed share-release preimages in a lifecycle transaction of the published
//! batch block. Qualification, signatures and acceptance timing are verified
//! independently by the reader of that block's branch.
use super::{
    Tb1Field,
    releases::{Tb1HolderRelease, Tb1ReleaseContext},
    signatures::release_message,
};
use crate::{block::Block, subnets::SUBNETWORK_ID_PALW_LIFECYCLE};
use libcrux_ml_dsa::ml_dsa_65::MLDSA65Signature;

const TAG: &[u8; 8] = b"TB1REL01";
pub(super) const SIGNATURE_BYTES: usize = 3309;
const SHARE_BYTES: usize = 4 + 66;
const MIN_MESSAGE_BYTES: usize = 4 + 1 + 64 + 64 + 4 + 4;

pub struct PublishedRelease<'a> {
    pub row: Tb1HolderRelease<'a>,
    pub signature: &'a MLDSA65Signature,
}

pub struct RetainedRelease<'a> {
    context: Tb1ReleaseContext<'a>,
    holder_slot: usize,
    shares: Vec<(usize, Tb1Field)>,
    signature: MLDSA65Signature,
}

impl RetainedRelease<'_> {
    pub fn view(&self) -> PublishedRelease<'_> {
        PublishedRelease {
            row: Tb1HolderRelease { context: self.context, holder_slot: self.holder_slot, shares: &self.shares },
            signature: &self.signature,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseBatchCarrierError {
    Missing,
    Multiple,
    Limit,
    Encoding,
}

pub fn encode_release_batch(
    rows: &[PublishedRelease<'_>],
    max_records: usize,
    max_message_bytes: usize,
) -> Result<Vec<u8>, ReleaseBatchCarrierError> {
    use ReleaseBatchCarrierError::{Encoding, Limit};
    if rows.is_empty() || rows.len() > max_records {
        return Err(Limit);
    }
    let mut payload = TAG.to_vec();
    payload.extend_from_slice(&u32::try_from(rows.len()).map_err(|_| Limit)?.to_be_bytes());
    for signed in rows {
        let message = release_message(&signed.row, max_message_bytes).ok_or(Encoding)?;
        payload.extend_from_slice(&u32::try_from(message.len()).map_err(|_| Limit)?.to_be_bytes());
        payload.extend_from_slice(&message);
        payload.extend_from_slice(signed.signature.as_slice());
    }
    Ok(payload)
}

pub fn release_batch_from_block(
    block: &Block,
    max_records: usize,
    max_message_bytes: usize,
) -> Result<Vec<RetainedRelease<'_>>, ReleaseBatchCarrierError> {
    use ReleaseBatchCarrierError::{Missing, Multiple};
    let mut carriers =
        block.transactions.iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(TAG));
    let carrier = carriers.next().ok_or(Missing)?;
    if carriers.next().is_some() {
        return Err(Multiple);
    }
    decode_release_batch(&carrier.payload, max_records, max_message_bytes)
}

pub fn decode_release_batch(
    payload: &[u8],
    max_records: usize,
    max_message_bytes: usize,
) -> Result<Vec<RetainedRelease<'_>>, ReleaseBatchCarrierError> {
    use ReleaseBatchCarrierError::{Encoding, Limit};
    let mut bytes = payload.strip_prefix(TAG).ok_or(Encoding)?;
    let count = take_count(&mut bytes)?;
    if count == 0 || count > max_records || count > bytes.len() / (4 + MIN_MESSAGE_BYTES + SIGNATURE_BYTES) {
        return Err(Limit);
    }
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let len = take_count(&mut bytes)?;
        if !(MIN_MESSAGE_BYTES..=max_message_bytes).contains(&len) {
            return Err(Limit);
        }
        let message = take_bytes(&mut bytes, len)?;
        let signature = MLDSA65Signature::new(*take(&mut bytes)?);
        rows.push(decode_release(message, signature)?);
    }
    if !bytes.is_empty() {
        return Err(Encoding);
    }
    Ok(rows)
}

fn decode_release(mut message: &[u8], signature: MLDSA65Signature) -> Result<RetainedRelease<'_>, ReleaseBatchCarrierError> {
    use ReleaseBatchCarrierError::Encoding;
    let round_len = take_count(&mut message)?;
    if round_len == 0 {
        return Err(Encoding);
    }
    let round_ref = take_bytes(&mut message, round_len)?;
    let context = Tb1ReleaseContext { round_ref, qual_root: *take(&mut message)?, freeze_id: *take(&mut message)? };
    let holder_slot = take_count(&mut message)?;
    let count = take_count(&mut message)?;
    if count.checked_mul(SHARE_BYTES) != Some(message.len()) {
        return Err(Encoding);
    }
    let mut shares = Vec::with_capacity(count);
    for _ in 0..count {
        let slot = take_count(&mut message)?;
        if shares.last().is_some_and(|(prior, _)| *prior >= slot) {
            return Err(Encoding);
        }
        let value = Tb1Field::from_bytes(take(&mut message)?).ok_or(Encoding)?;
        shares.push((slot, value));
    }
    Ok(RetainedRelease { context, holder_slot, shares, signature })
}

fn take_count(input: &mut &[u8]) -> Result<usize, ReleaseBatchCarrierError> {
    usize::try_from(u32::from_be_bytes(*take(input)?)).map_err(|_| ReleaseBatchCarrierError::Limit)
}

fn take<'a, const N: usize>(input: &mut &'a [u8]) -> Result<&'a [u8; N], ReleaseBatchCarrierError> {
    let (field, tail) = input.split_first_chunk::<N>().ok_or(ReleaseBatchCarrierError::Encoding)?;
    *input = tail;
    Ok(field)
}

fn take_bytes<'a>(input: &mut &'a [u8], len: usize) -> Result<&'a [u8], ReleaseBatchCarrierError> {
    let (field, tail) = input.split_at_checked(len).ok_or(ReleaseBatchCarrierError::Encoding)?;
    *input = tail;
    Ok(field)
}
