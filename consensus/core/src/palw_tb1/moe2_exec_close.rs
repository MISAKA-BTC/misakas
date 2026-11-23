//! Candidate-branch validation of the dual-signed allocations in a private EXEC close.
use super::{
    exec_close_carrier::ExecCloseCarrier, exec_registration::ExecRegistrationProfile, freeze::Tb1MiningClosure,
    qualification::QualifiedExecPanel,
};
use crate::{
    Hash64,
    api::ConsensusApi,
    block::Block,
    config::params::Params,
    header::Header,
    palw_attempt_v2::palw_network_domain_v2_for,
    palw_model_market_v1::moe2::settlement::{Moe2ExecSlot, Moe2PreDrawQuote},
    palw_state_proof_v1::state_root_of_pinned_header_v1,
    palw_state_v2::{Moe2ExecCloseV1, PalwBlockContextV2, PalwChainStateV2},
};
use std::io;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub struct VerifiedMoe2ExecClose {
    close: Moe2ExecCloseV1,
    preparation: Hash64,
    first_quoted_profile: Option<ExecRegistrationProfile>,
    freeze_id: [u8; 64],
}

impl VerifiedMoe2ExecClose {
    pub fn quoted_slots(&self) -> &[Moe2ExecSlot] {
        &self.close.slots
    }

    pub fn block(&self) -> Hash64 {
        self.close.block
    }

    pub fn preparation(&self) -> Hash64 {
        self.preparation
    }

    pub fn freeze_id(&self) -> [u8; 64] {
        self.freeze_id
    }

    pub fn into_mining_reservation(self) -> io::Result<Moe2ExecCloseV1> {
        if let Some(profile) = self.first_quoted_profile {
            return Err(match profile {
                ExecRegistrationProfile::Qf1Local { .. } => invalid("local QF1 inference cannot reserve a mining reward"),
                ExecRegistrationProfile::TirByte3 => invalid("TIR input has no admitted FOW1 mining profile"),
            });
        }
        Ok(self.close)
    }
}

/// Validate the in-flight candidate's body with the selected parent already
/// resolved by the block owner. The callback checks the candidate coinbase
/// marker. Historical reads are confined to the branch before the close.
#[allow(clippy::too_many_arguments)]
pub fn verify_moe2_exec_close_candidate(
    native: &dyn ConsensusApi,
    config: &Params,
    point: &PalwBlockContextV2,
    block: &Block,
    selected_parent: Hash64,
    coinbase_commits_to: impl FnOnce(&Tb1MiningClosure, &[u8; 64]) -> io::Result<bool>,
    rounds: &QualifiedExecPanel,
    parent: &PalwChainStateV2,
    preparation_state: &PalwChainStateV2,
    max_records: usize,
    max_job_tokens: usize,
) -> io::Result<VerifiedMoe2ExecClose> {
    let close = point.block;
    if block.hash() != close || (block.header.daa_score, block.header.blue_score) != (point.daa_score, point.blue_score) {
        return Err(invalid("MOE2 reservation differs from close position"));
    }
    let private_from = check_close_parent(config, &block.header, close, selected_parent, parent, rounds)?;
    let byte_cap = usize::try_from(config.max_block_mass).map_err(|_| invalid("EXEC carrier budget overflow"))?;
    let evidence_error = |why| io::Error::new(io::ErrorKind::InvalidData, format!("invalid EXEC close carrier: {why:?}"));
    let carrier = ExecCloseCarrier::from_block(block, max_records, byte_cap).map_err(evidence_error)?;
    let closure = &carrier.closure;
    if carrier.qualified_rounds.as_bytes() != rounds.commitment()
        || closure.round.id() != rounds.exec().id()
        || closure.qual_root != rounds.exec_qual()
    {
        return Err(invalid("EXEC close carrier differs from its preparation"));
    }
    let freeze_id = closure.commitment(selected_parent);
    if !coinbase_commits_to(closure, &freeze_id)? {
        return Err(invalid("TB1 close block does not commit to the supplied closure"));
    }
    let preparation = native.get_header(carrier.preparation).map_err(io::Error::other)?;
    if preparation.blue_score >= closure.round.close_blue_score
        || preparation.daa_score >= closure.round.release_from_daa
        || !native.is_chain_ancestor_of(carrier.preparation, selected_parent).map_err(io::Error::other)?
        || !native
            .palw_coinbase_commits_to(closure.round.chain_id, carrier.preparation, &rounds.commitment())
            .map_err(io::Error::other)?
    {
        return Err(invalid("missing joint qualification before EXEC close"));
    }
    let preparation_parent = native.get_ghostdag_data(carrier.preparation).map_err(io::Error::other)?.selected_parent;
    let preparation_root = state_root_of_pinned_header_v1(&preparation, carrier.preparation).map_err(io::Error::other)?;
    if preparation_state.last_point().map(|p| p.block) != Some(preparation_parent)
        || preparation_state.state_root() != preparation_root
    {
        return Err(invalid("MOE2 preparation state differs from its committed parent"));
    }
    let rows = carrier
        .verify(selected_parent, point.blue_score, point.daa_score, freeze_id, |quote| {
            preparation_state.bond(&quote.funding_bond).map(|bond| bond.pubkey.as_slice())
        })
        .map_err(evidence_error)?;

    let mut slots = Vec::new();
    let mut first_quoted_profile = None;
    for (slot, row) in rows {
        if row.policy.as_bytes() != closure.policy_root
            || !(row.open_daa..row.deadline_exclusive_daa).contains(&point.daa_score)
            || rounds.panel().release_from_daa >= row.deadline_exclusive_daa
            || row.job_tokens as usize > max_job_tokens
            || !native.is_chain_ancestor_of(carrier.preparation, row.registration_anchor).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(row.registration_anchor, selected_parent).map_err(io::Error::other)?
        {
            return Err(invalid("EXEC input outside its registration history or policy"));
        }
        let Some(quote) = row.quote else { continue };
        if !(quote.valid_from..=quote.deadline).contains(&point.daa_score) {
            return Err(invalid("MOE2 quote outside original window at EXEC close"));
        }
        check_quote_window(&quote, preparation_parent, preparation.daa_score)?;
        let class = preparation_state.class(&quote.model).ok_or_else(|| invalid("missing MOE2 registered class"))?;
        check_registered_quote(&quote, class.artifact_root, class.registered_daa)?;
        slots.push(Moe2ExecSlot { registration: row.key, slot, close, close_daa: point.daa_score, quote });
        first_quoted_profile.get_or_insert(row.profile);
    }
    Ok(VerifiedMoe2ExecClose {
        preparation: carrier.preparation,
        close: Moe2ExecCloseV1 {
            selected_parent,
            parent_state_root: parent.state_root(),
            block: close,
            blue_score: point.blue_score,
            daa_score: point.daa_score,
            private_from,
            slots,
        },
        first_quoted_profile,
        freeze_id,
    })
}

pub fn check_close_parent(
    config: &Params,
    header: &Header,
    close: Hash64,
    selected_parent: Hash64,
    state: &PalwChainStateV2,
    rounds: &QualifiedExecPanel,
) -> io::Result<u64> {
    let network = palw_network_domain_v2_for(config.net.to_string().as_bytes(), Some(config.genesis.hash));
    let from = config.private_moe2_registration_from().ok_or_else(|| invalid("MOE2 private budget unavailable"))?;
    if network.as_bytes() != rounds.exec().chain_id
        || header.daa_score < from
        || state.last_point().map(|p| p.block) != Some(selected_parent)
        || state.state_root() != state_root_of_pinned_header_v1(header, close).map_err(io::Error::other)?
    {
        return Err(invalid("MOE2 capacity requires the original network and close parent state"));
    }
    Ok(from)
}

pub fn check_quote_window(quote: &Moe2PreDrawQuote, parent: Hash64, daa: u64) -> io::Result<()> {
    if parent != quote.quote_anchor {
        return Err(invalid("MOE2 quote differs from signed slot or preparation parent"));
    }
    if !(quote.valid_from..=quote.deadline).contains(&daa) {
        return Err(invalid("MOE2 quote outside original window at preparation"));
    }
    Ok(())
}

pub fn check_registered_quote(quote: &Moe2PreDrawQuote, artifact: Hash64, registered_daa: u64) -> io::Result<()> {
    if artifact != quote.artifact_root || quote.valid_from < registered_daa {
        return Err(invalid("MOE2 quote differs from registered model at preparation"));
    }
    Ok(())
}
