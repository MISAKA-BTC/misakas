//! Signed PANEL replies in a lifecycle transaction. The caller supplies the
//! draw's seat count and verifies the selected duty and its block commitment.
use crate::{block::Block, subnets::SUBNETWORK_ID_PALW_LIFECYCLE};
use libcrux_ml_dsa::ml_dsa_65::{MLDSA65Signature, MLDSA65VerificationKey};

const TAG: &[u8; 8] = b"TB1RPL01";
pub const PANEL_REPLY_KEY_BYTES: usize = 1952;
pub const PANEL_REPLY_SIGNATURE_BYTES: usize = 3309;
const REPLY_BYTES: usize = PANEL_REPLY_KEY_BYTES + PANEL_REPLY_SIGNATURE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelReplyCarrierError {
    Missing,
    Multiple,
    Encoding,
}

pub fn encode_panel_replies(replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(TAG.len() + replies.len() * REPLY_BYTES);
    payload.extend_from_slice(TAG);
    for (key, signature) in replies {
        payload.extend_from_slice(key.as_slice());
        payload.extend_from_slice(signature.as_slice());
    }
    payload
}

pub fn decode_panel_replies(payload: &[u8], seats: usize) -> Result<&[u8], PanelReplyCarrierError> {
    use PanelReplyCarrierError::Encoding;
    let rows = payload.strip_prefix(TAG).ok_or(Encoding)?;
    if seats == 0 || seats.checked_mul(REPLY_BYTES) != Some(rows.len()) {
        return Err(Encoding);
    }
    Ok(rows)
}

pub fn panel_replies_from_block(block: &Block, seats: usize) -> Result<&[u8], PanelReplyCarrierError> {
    use PanelReplyCarrierError::{Missing, Multiple};
    let mut carriers =
        block.transactions.iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(TAG));
    let carrier = carriers.next().ok_or(Missing)?;
    if carriers.next().is_some() {
        return Err(Multiple);
    }
    decode_panel_replies(&carrier.payload, seats)
}
