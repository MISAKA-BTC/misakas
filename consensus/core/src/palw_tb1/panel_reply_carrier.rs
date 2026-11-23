//! Signed PANEL replies in a lifecycle transaction. The caller supplies the
//! draw's seat count and verifies the selected duty and its block commitment.
use crate::{
    Hash64,
    api::ConsensusApi,
    block::Block,
    palw_panel_v2::{
        PALW_DRAW_WEIGHT_MAX_MSK_V1, palw_draw_key_cmp_v1, palw_draw_neg_log2_q64_v1, palw_draw_ticket_u64_v1,
        palw_panel_stake_ticket_v1,
    },
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
};
use libcrux_ml_dsa::ml_dsa_65::{self, MLDSA65Signature, MLDSA65VerificationKey};
use sha3::{Digest, Sha3_512};
use std::io;

use super::{
    freeze::Tb1MiningClosure, publication::VerifiedTb1Publication, qf1_original_carrier::qf1_marker, releases::Tb1BeaconSeed,
};

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

pub fn replay_policy_id(seats: u32) -> Hash64 {
    Hash64::from_bytes(
        Sha3_512::new()
            .chain_update(b"MISAKA/private/TB1/PANEL/stake-ticket/full-replay/v1")
            .chain_update(seats.to_be_bytes())
            .finalize()
            .into(),
    )
}

/// Inputs from the authenticated PANEL cut. The owner resolves the eligible
/// Original and the roster committed by the close before constructing this view.
pub struct PanelReplayCut<'a> {
    pub closure: &'a Tb1MiningClosure,
    pub close: Hash64,
    pub freeze_id: [u8; 64],
    pub original: Hash64,
    pub original_block: Hash64,
    pub deadline: u64,
    pub candidates: &'a [(Hash64, u64)],
    pub seats: u32,
}

impl PanelReplayCut<'_> {
    pub fn draw_for_candidate(&self, publication: &VerifiedTb1Publication) -> io::Result<PanelReplayDuty> {
        let (certificate, seed) = publication.panel_seed(self.close, self.closure, self.freeze_id)?;
        self.draw(certificate, seed)
    }

    /// Draw from a seed already checked by the branch publication reader.
    pub fn draw(&self, certificate: Hash64, seed: Tb1BeaconSeed) -> io::Result<PanelReplayDuty> {
        let seats = self.seats as usize;
        if seats == 0
            || seats > self.candidates.len()
            || replay_policy_id(self.seats).as_bytes() != self.closure.policy_root
            || self.candidates.iter().any(|(_, weight)| *weight == 0 || *weight > PALW_DRAW_WEIGHT_MAX_MSK_V1)
            || self.candidates.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err(invalid("different PANEL policy or population"));
        }
        let anchor = Hash64::from_bytes(
            Sha3_512::new()
                .chain_update(b"MISAKA/private/TB1/PANEL/draw-anchor/v1")
                .chain_update(self.freeze_id)
                .chain_update(seed.as_bytes())
                .finalize()
                .into(),
        );
        let mut ranked: Vec<_> = self
            .candidates
            .iter()
            .map(|(id, weight)| {
                let ticket = palw_panel_stake_ticket_v1(anchor, &self.original, id);
                (palw_draw_neg_log2_q64_v1(palw_draw_ticket_u64_v1(&ticket)), *weight, *id)
            })
            .collect();
        ranked.sort_unstable_by(|a, b| palw_draw_key_cmp_v1(a.0, a.1, &a.2, b.0, b.1, &b.2));
        let selected = ranked.into_iter().take(seats).map(|(_, _, id)| id).collect();
        let duty = Sha3_512::new()
            .chain_update(REPLAY_SIGNATURE_DOMAIN)
            .chain_update(self.freeze_id)
            .chain_update(self.original.as_bytes())
            .chain_update(self.original_block.as_bytes())
            .chain_update(certificate.as_bytes())
            .chain_update(anchor.as_bytes())
            .finalize()
            .into();
        Ok(PanelReplayDuty {
            chain: self.closure.round.chain_id,
            original: self.original,
            original_block: self.original_block,
            certificate,
            deadline: self.deadline,
            selected,
            duty,
        })
    }
}

pub fn panel_reply_commitment(
    duty: &[u8; 64],
    selected: &[Hash64],
    replies: &[(&MLDSA65VerificationKey, &MLDSA65Signature)],
) -> io::Result<[u8; 64]> {
    let mut seats = selected.iter();
    let mut marker = Sha3_512::new().chain_update(b"MISAKA/private/TB1/PANEL/reply-set/v1").chain_update(duty);
    for &(key, signature) in replies {
        let actor = actor_id(key);
        if !seats.any(|&seat| seat == actor) {
            return Err(invalid("PANEL replies outside draw order"));
        }
        ml_dsa_65::verify(key, &replay_message(duty, actor), REPLAY_SIGNATURE_DOMAIN, signature)
            .map_err(|_| invalid("invalid local PANEL reply"))?;
        marker.update(actor.as_bytes());
        marker.update(signature.as_slice());
    }
    if selected.is_empty() || replies.len() != selected.len() {
        return Err(io::Error::new(io::ErrorKind::WouldBlock, "incomplete local PANEL replies"));
    }
    Ok(marker.finalize().into())
}

/// Duty reconstructed by the owner from the authenticated PANEL close and seed.
/// The reply payload supplies signatures. It cannot install a duty.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PanelReplayDuty {
    pub chain: [u8; 64],
    pub original: Hash64,
    pub original_block: Hash64,
    pub certificate: Hash64,
    pub deadline: u64,
    pub duty: [u8; 64],
    pub selected: Vec<Hash64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedPanelApproval {
    original: Hash64,
    original_block: Hash64,
    accepting_block: Hash64,
    selected_parent: Hash64,
    pub(crate) deadline: u64,
    pub(crate) certificate: Hash64,
    pub(crate) duty: [u8; 64],
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

impl PanelReplayDuty {
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
        if !native.is_chain_ancestor_of(self.original_block, selected_parent).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(self.certificate, selected_parent).map_err(io::Error::other)?
        {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        self.verify_bound_replies(selected_parent, candidate)
    }

    /// Verify a body against the draw already stored by its selected parent.
    pub fn verify_bound_replies(&self, selected_parent: Hash64, candidate: &Block) -> io::Result<PublishedPanelApproval> {
        if candidate.header.daa_score >= self.deadline {
            return Err(io::ErrorKind::TimedOut.into());
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
        if qf1_marker(candidate) != Some(panel_reply_commitment(&self.duty, &self.selected, &rows)?) {
            return Err(invalid("local PANEL replies outside committed branch"));
        }
        Ok(PublishedPanelApproval {
            original: self.original,
            original_block: self.original_block,
            accepting_block: candidate.hash(),
            selected_parent,
            deadline: self.deadline,
            certificate: self.certificate,
            duty: self.duty,
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
