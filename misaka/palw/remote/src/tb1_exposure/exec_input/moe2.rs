//! Private quote consent. The owner's RegKey covers all economic terms and the
//! job. The funder then authorizes that key and allocation slot. Neither ID
//! depends on a later ticket or claim.
use super::*;
use crate::tb1_exposure::Tb1HolderFreeze;
use kaspa_consensus_core::palw_state_proof_v1::{
    PALW_PROOF_LABEL_BONDS, PALW_PROOF_LABEL_CLASSES, state_root_of_pinned_header_v1, verify_bond_v1, verify_class_v1,
};
use libcrux_ml_dsa::ml_dsa_87::{self, MLDSA87VerificationKey};

pub const QUOTE_DOMAIN: &[u8] = b"MISAKA/private/FOW1/MOE2/quote-consent/v1";

pub fn consent_ref(reg_key: [u8; 64], slot: u64) -> [u8; 64] {
    Sha3_512::new().chain_update(QUOTE_DOMAIN).chain_update(reg_key).chain_update(slot.to_be_bytes()).finalize().into()
}

pub(super) fn verify_consent(
    native: &dyn ConsensusApi,
    preparation: Hash64,
    row: &SignedExecInput<'_>,
    key: [u8; 64],
    quote: &Moe2PreDrawQuote,
    consent: &MLDSA87Signature,
) -> io::Result<()> {
    let authorization = consent_ref(key, row.slot);
    if authorization != row.funding_ref
        || native.get_ghostdag_data(preparation).map_err(io::Error::other)?.selected_parent != quote.quote_anchor
    {
        return Err(invalid("MOE2 quote differs from signed slot or preparation parent"));
    }
    let (header, bonds) = native.palw_state_proof_v1(preparation, PALW_PROOF_LABEL_BONDS).map_err(io::Error::other)?;
    let root = state_root_of_pinned_header_v1(&header, preparation).map_err(io::Error::other)?;
    if !(quote.valid_from..=quote.deadline).contains(&header.daa_score) {
        return Err(invalid("MOE2 quote outside original window at preparation"));
    }
    let bond = verify_bond_v1(&bonds, root, &quote.funding_bond).map_err(io::Error::other)?;
    let vk = MLDSA87VerificationKey::new(bond.pubkey.as_slice().try_into().map_err(|_| invalid("invalid MOE2 bond key"))?);
    ml_dsa_87::verify(&vk, &authorization, QUOTE_DOMAIN, consent).map_err(|_| invalid("invalid MOE2 funder consent"))?;
    let (_, classes) = native.palw_state_proof_v1(preparation, PALW_PROOF_LABEL_CLASSES).map_err(io::Error::other)?;
    let class = verify_class_v1(&classes, root, &quote.model).map_err(io::Error::other)?;
    if class.artifact_root != quote.artifact_root || quote.valid_from < class.registered_daa {
        return Err(invalid("MOE2 quote differs from registered model at preparation"));
    }
    Ok(())
}

/// Historical quote read from a selected close and the state root committed by
/// its preparation. Current settlement authorization must be checked separately.
pub struct ClosedMoe2Quote {
    terms: Moe2PreDrawQuote,
    state_root: Hash64,
    network: Hash64,
}

impl ClosedMoe2Quote {
    pub fn terms(&self) -> Moe2PreDrawQuote {
        self.terms
    }

    pub fn preview(
        &self,
        config: &kaspa_consensus_core::config::params::Params,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        reward: u64,
    ) -> io::Result<kaspa_consensus_core::palw_model_market_v1::moe2::Moe2RewardAllocation> {
        let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.net.to_string().as_bytes(),
            Some(config.genesis.hash),
        );
        if self.network != network || state.state_root() != self.state_root {
            return Err(invalid("MOE2 preview state or network differs from signed preparation"));
        }
        self.terms.preview_at_anchor(config, state, reward).map_err(|_| invalid("MOE2 preview outside signed quote or market policy"))
    }
}

impl ExecInputBatch<'_> {
    /// Recover the dual-signed terms from the exact selected EXEC close. This
    /// historical read does not release a seed, debit a bond, or grant mining credit.
    pub fn read_closed_moe2_quote(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
        slot: u64,
    ) -> io::Result<ClosedMoe2Quote> {
        let tip = native.get_sink();
        self.check_close(native, close, closure)?;
        Tb1HolderFreeze::resolve(native, close, closure, q)?;
        if !native.is_chain_ancestor_of(close, tip).map_err(io::Error::other)? {
            return Err(invalid("detached EXEC quote close"));
        }
        let quote =
            self.entries.get(&slot).and_then(|entry| entry.terms.moe2_quote).ok_or_else(|| invalid("no MOE2 quote at EXEC slot"))?;
        let header = native.get_header(self.preparation).map_err(io::Error::other)?;
        let state_root = state_root_of_pinned_header_v1(&header, self.preparation).map_err(io::Error::other)?;
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC quote view moved"));
        }
        Ok(ClosedMoe2Quote { terms: quote, state_root, network: Hash64::from_bytes(self.rounds.exec.chain_id) })
    }
}
