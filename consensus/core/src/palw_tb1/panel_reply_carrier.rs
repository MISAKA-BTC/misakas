//! Signed PANEL replies in a lifecycle transaction. The caller supplies the
//! draw's seat count and verifies the selected duty and its block commitment.
use crate::{Hash64, api::ConsensusApi, block::Block, subnets::SUBNETWORK_ID_PALW_LIFECYCLE};
use libcrux_ml_dsa::ml_dsa_65::{self, MLDSA65Signature, MLDSA65VerificationKey};
use sha3::{Digest, Sha3_512};
use std::io;

use super::qf1_original_carrier::qf1_marker;

const TAG: &[u8; 8] = b"TB1RPL01";
pub const PANEL_REPLY_KEY_BYTES: usize = 1952;
pub const PANEL_REPLY_SIGNATURE_BYTES: usize = 3309;
const REPLY_BYTES: usize = PANEL_REPLY_KEY_BYTES + PANEL_REPLY_SIGNATURE_BYTES;
pub const REPLAY_SIGNATURE_DOMAIN: &[u8] = b"MISAKA/private/TB1/QF1/complete-replay/v1";

pub fn actor_id(key: &MLDSA65VerificationKey) -> Hash64 {
    Hash64::from_bytes(
        Sha3_512::new().chain_update(b"MISAKA/private/TB1/PANEL/actor/v1").chain_update(key.as_slice()).finalize().into(),
    )
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn replay_message(duty: &[u8; 64], actor: Hash64) -> [u8; 64] {
    Sha3_512::new().chain_update(duty).chain_update(actor.as_bytes()).finalize().into()
}

pub fn panel_reply_commitment(
    duty: &[u8; 64],
    selected: &[Hash64],
    replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)],
) -> io::Result<[u8; 64]> {
    if selected.is_empty() || replies.len() != selected.len() {
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "incomplete local PANEL replies"));
    }
    let mut marker = Sha3_512::new().chain_update(b"MISAKA/private/TB1/PANEL/reply-set/v1").chain_update(duty);
    for (&seat, &(key, signature)) in selected.iter().zip(replies) {
        let actor = actor_id(key);
        if actor != seat {
            return Err(invalid("PANEL replies outside draw order"));
        }
        ml_dsa_65::verify(key, &replay_message(duty, actor), REPLAY_SIGNATURE_DOMAIN, signature)
            .map_err(|_| invalid("invalid local PANEL reply"))?;
        marker.update(actor.as_bytes());
        marker.update(signature.as_slice());
    }
    Ok(marker.finalize().into())
}

/// View of a duty reconstructed from the authenticated PANEL close and seed.
/// The caller owns the authenticated draw. The reply payload supplies only signatures.
pub struct PanelReplayDuty<'a> {
    pub chain: [u8; 64],
    pub original: Hash64,
    pub original_block: Hash64,
    pub certificate: Hash64,
    pub deadline: u64,
    pub duty: &'a [u8; 64],
    pub selected: &'a [Hash64],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedPanelApproval {
    original: Hash64,
    original_block: Hash64,
    accepting_block: Hash64,
    selected_parent: Hash64,
    pub(crate) deadline: u64,
}

impl PublishedPanelApproval {
    pub fn original(&self) -> Hash64 {
        self.original
    }

    pub fn original_block(&self) -> Hash64 {
        self.original_block
    }

    pub fn accepting_block(&self) -> Hash64 {
        self.accepting_block
    }

    pub fn selected_parent(&self) -> Hash64 {
        self.selected_parent
    }
}

impl PanelReplayDuty<'_> {
    pub fn verify_published(&self, native: &dyn ConsensusApi, tip: Hash64, block: Hash64) -> io::Result<PublishedPanelApproval> {
        let source = native.get_block(block).map_err(io::Error::other)?;
        let parent = native.get_ghostdag_data(block).map_err(io::Error::other)?.selected_parent;
        if source.hash() != block || !native.is_chain_ancestor_of(block, tip).map_err(io::Error::other)? {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        let approval = self.verify_candidate(native, parent, &source)?;
        let marker = qf1_marker(&source).ok_or_else(|| invalid("missing PANEL reply commitment"))?;
        if !native.palw_coinbase_commits_to(self.chain, block, &marker).map_err(io::Error::other)? {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        Ok(PublishedPanelApproval { selected_parent: tip, ..approval })
    }

    /// Validate a reply set in a candidate's own body. The caller pins the
    /// candidate's selected parent using native consensus.
    pub fn verify_candidate(
        &self,
        native: &dyn ConsensusApi,
        selected_parent: Hash64,
        candidate: &Block,
    ) -> io::Result<PublishedPanelApproval> {
        if candidate.header.daa_score >= self.deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        if !native.is_chain_ancestor_of(self.original_block, selected_parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(self.certificate, selected_parent).map_err(io::Error::other)?
        {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        let raw = panel_replies_from_block(candidate, self.selected.len())
            .map_err(|error| invalid(&format!("PANEL reply carrier: {error:?}")))?;
        let replies: Vec<_> = raw
            .chunks_exact(REPLY_BYTES)
            .map(|row| {
                let (key, signature) = row.split_at(PANEL_REPLY_KEY_BYTES);
                (MLDSA65VerificationKey::new(key.try_into().unwrap()), MLDSA65Signature::new(signature.try_into().unwrap()))
            })
            .collect();
        let rows: Vec<_> = replies.iter().map(|(key, signature)| (key, signature)).collect();
        if qf1_marker(candidate) != Some(panel_reply_commitment(self.duty, self.selected, &rows)?) {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        Ok(PublishedPanelApproval {
            original: self.original,
            original_block: self.original_block,
            accepting_block: candidate.hash(),
            selected_parent,
            deadline: self.deadline,
        })
    }
}

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
