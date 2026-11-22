//! Private quote consent. The owner's RegKey covers all economic terms and the
//! job. The funder then authorizes that key and allocation slot. Neither ID
//! depends on a later ticket or claim.
use super::*;
use crate::tb1_exposure::Tb1HolderFreeze;
use kaspa_consensus_core::palw_state_proof_v1::{
    PALW_PROOF_LABEL_BONDS, PALW_PROOF_LABEL_CLASSES, state_root_of_pinned_header_v1, verify_bond_v1, verify_class_v1,
};
use kaspa_consensus_core::{
    config::params::Params,
    palw_attempt_v2::palw_network_domain_v2_for,
    palw_bond_budget_v1::PalwBudgetOriginV1,
    palw_mode_v2::PalwConsensusMode,
    palw_state_v2::{PalwChainStateV2, PalwStateDeltaV2, apply_delta_v2},
};
use libcrux_ml_dsa::ml_dsa_87::{self, MLDSA87VerificationKey};

use kaspa_consensus_core::palw_model_market_v1::moe2::settlement::Moe2ExecSlot;

pub const QUOTE_DOMAIN: &[u8] = Moe2ExecSlot::CONSENT_DOMAIN;

pub fn consent_ref(reg_key: [u8; 64], slot: u64) -> [u8; 64] {
    Moe2ExecSlot::consent_ref(reg_key, slot)
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
    slot: Moe2ExecSlot,
    state_root: Hash64,
    network: Hash64,
}

impl ClosedMoe2Quote {
    pub fn terms(&self) -> Moe2PreDrawQuote {
        self.slot.quote
    }

    pub fn exec_slot(&self) -> &Moe2ExecSlot {
        &self.slot
    }

    pub fn preview(
        &self,
        config: &Params,
        state: &PalwChainStateV2,
        reward: u64,
    ) -> io::Result<kaspa_consensus_core::palw_model_market_v1::moe2::Moe2RewardAllocation> {
        let network = palw_network_domain_v2_for(config.net.to_string().as_bytes(), Some(config.genesis.hash));
        if self.network != network || state.state_root() != self.state_root {
            return Err(invalid("MOE2 preview state or network differs from signed preparation"));
        }
        self.slot
            .quote
            .preview_at_anchor(config, state, reward)
            .map_err(|_| invalid("MOE2 preview outside signed quote or market policy"))
    }

    /// Recheck the signed economic terms against the pre-state committed by
    /// the current selected tip. Reading a cached close alone is insufficient
    /// after a reorganization, deadline, policy change or price movement.
    pub fn preview_for_selected_tip(
        &self,
        native: &dyn ConsensusApi,
        config: &Params,
        anchor_state: &PalwChainStateV2,
        execution_parent: &PalwChainStateV2,
        reward: u64,
    ) -> io::Result<kaspa_consensus_core::palw_model_market_v1::moe2::Moe2RewardAllocation> {
        let tip = native.get_sink();
        let header = native.get_header(tip).map_err(io::Error::other)?;
        let parent = native.get_ghostdag_data(tip).map_err(io::Error::other)?.selected_parent;
        let root = state_root_of_pinned_header_v1(&header, tip).map_err(io::Error::other)?;
        let network = palw_network_domain_v2_for(config.net.to_string().as_bytes(), Some(config.genesis.hash));
        if self.network != network
            || anchor_state.state_root() != self.state_root
            || execution_parent.last_point().map(|p| p.block) != Some(parent)
            || execution_parent.state_root() != root
            || !native.is_chain_ancestor_of(self.slot.close, parent).map_err(io::Error::other)?
        {
            return Err(invalid("MOE2 quote differs from selected execution history"));
        }
        let result = self.slot.quote.preview_at_position(config, anchor_state, execution_parent, header.daa_score, reward);
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "MOE2 execution view moved"));
        }
        result.map_err(|err| match err {
            kaspa_consensus_core::palw_model_market_v1::moe2::settlement::Moe2SettlementError::OutsideWindow => {
                io::Error::new(io::ErrorKind::TimedOut, "MOE2 quote outside its original window")
            }
            _ => invalid("MOE2 reward outside original market terms or active rules"),
        })
    }
}

/// Historical monetary capacity for one dual-signed slot in the complete close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2BudgetPreview {
    pub consent: Hash64,
    pub reward_sompi: u64,
    pub collateral_sompi: u64,
    pub minimum_liability_sompi: u128,
    pub allocation_epoch: u64,
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
        self.check_quote_close(native, close, closure, q, tip)?;
        let entry = self.entries.get(&slot).ok_or_else(|| invalid("unknown EXEC slot"))?;
        let quote = entry.terms.moe2_quote.ok_or_else(|| invalid("no MOE2 quote at EXEC slot"))?;
        let close_daa = native.get_header(close).map_err(io::Error::other)?.daa_score;
        let slot = Moe2ExecSlot { registration: reg_key(&entry.context), slot, close, close_daa, quote };
        let header = native.get_header(self.preparation).map_err(io::Error::other)?;
        let state_root = state_root_of_pinned_header_v1(&header, self.preparation).map_err(io::Error::other)?;
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC quote view moved"));
        }
        Ok(ClosedMoe2Quote { slot, state_root, network: Hash64::from_bytes(self.rounds.exec.chain_id) })
    }
    fn check_quote_close(
        &self,
        native: &dyn ConsensusApi,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
        tip: Hash64,
    ) -> io::Result<()> {
        self.check_close(native, close, closure)?;
        Tb1HolderFreeze::resolve(native, close, closure, q)?;
        if !native.is_chain_ancestor_of(close, tip).map_err(io::Error::other)? {
            return Err(invalid("detached EXEC quote close"));
        }
        Ok(())
    }

    /// Price the collateral for full export of each signed reward ceiling. Replay the
    /// ordinary budget engine in frozen slot order against the parent state committed
    /// by the close. Earlier slots compete for the same bond and model budgets.
    /// The scratch reservations confer no authority to lock funds or publish shares.
    pub fn preview_closed_moe2_budget(
        &self,
        native: &dyn ConsensusApi,
        config: &Params,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
        state: &PalwChainStateV2,
    ) -> io::Result<BTreeMap<u64, Moe2BudgetPreview>> {
        self.close_budget(native, config, close, closure, q, state, None)
    }

    /// Append the complete signed slot set to the private block owner's close
    /// delta. Earlier writes in the same block compete for the same budget.
    /// The caller owns the ordinary close delta and commits it with the block.
    pub fn reserve_closed_moe2_budget(
        &self,
        native: &dyn ConsensusApi,
        config: &Params,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
        parent: &PalwChainStateV2,
        delta: &mut PalwStateDeltaV2,
    ) -> io::Result<BTreeMap<u64, Moe2BudgetPreview>> {
        self.close_budget(native, config, delta.point.block, closure, q, parent, Some(delta))
    }

    fn close_budget(
        &self,
        native: &dyn ConsensusApi,
        config: &Params,
        close: Hash64,
        closure: &Tb1MiningClosure,
        q: &Tb1Qualification<'_>,
        state: &PalwChainStateV2,
        delta: Option<&mut PalwStateDeltaV2>,
    ) -> io::Result<BTreeMap<u64, Moe2BudgetPreview>> {
        let tip = native.get_sink();
        self.check_quote_close(native, close, closure, q, tip)?;
        let header = native.get_header(close).map_err(io::Error::other)?;
        let parent = native.get_ghostdag_data(close).map_err(io::Error::other)?.selected_parent;
        let network = palw_network_domain_v2_for(config.net.to_string().as_bytes(), Some(config.genesis.hash));
        let from = config.private_moe2_registration_from().ok_or_else(|| invalid("MOE2 private budget unavailable"))?;
        let daa = header.daa_score;
        if network.as_bytes() != self.rounds.exec.chain_id
            || daa < from
            || state.last_point().map(|p| p.block) != Some(parent)
            || state.state_root() != state_root_of_pinned_header_v1(&header, close).map_err(io::Error::other)?
        {
            return Err(invalid("MOE2 capacity requires the original network and close parent state"));
        }
        let PalwConsensusMode::ConsensusV2(bundle) = &config.palw_consensus_mode else { return Err(invalid("missing native budget")) };
        let policy = &bundle
            .state
            .bond_budget()
            .filter(|_| bundle.state.bond_budget_active_at(daa))
            .ok_or_else(|| invalid("inactive MOE2 budget at close"))?
            .policy;
        bundle.state.model_tier_v1().filter(|c| c.active_at(daa)).ok_or_else(|| invalid("missing MOE2 catalog"))?;
        let post;
        let state = if let Some(d) = delta.as_deref() {
            if d.point.daa_score != daa || d.point.blue_score != header.blue_score {
                return Err(invalid("MOE2 reservation differs from close position"));
            }
            post = apply_delta_v2(state, d, &bundle.state).map_err(io::Error::other)?;
            &post
        } else {
            state
        };
        let mut writes = Vec::new();
        let mut budget = state.bond_budget().ok_or_else(|| invalid("missing native MOE2 budget"))?.clone();
        if budget.header.policy_digest != policy.digest() {
            return Err(invalid("native budget policy differs from MOE2 configuration"));
        }
        let mut result = BTreeMap::new();
        for (&slot, entry) in &self.entries {
            let Some(quote) = entry.terms.moe2_quote else { continue };
            let (capital, ask) = quote.budget_ask_at(config, state, daa).map_err(|reason| {
                io::Error::new(io::ErrorKind::InvalidData, format!("MOE2 slot cannot reserve under its signed terms: {reason:?}"))
            })?;
            let consent = Hash64::from_bytes(consent_ref(reg_key(&entry.context), slot));
            let grant = budget
                .reserve_claim_v1(
                    policy,
                    capital,
                    consent,
                    quote.funding_bond,
                    Some(quote.model),
                    daa,
                    ask,
                    PalwBudgetOriginV1::Attempt,
                    &mut writes,
                )
                .map_err(|err| io::Error::other(err.to_string()))?;
            if grant.reward_sompi != ask.reward_sompi {
                return Err(invalid("native budget clipped a signed MOE2 reward ceiling"));
            }
            let (_, epoch) = budget
                .claim_row(&consent)
                .and_then(|r| r.model)
                .filter(|(id, _)| *id == quote.model)
                .ok_or_else(|| invalid("missing native model allocation"))?;
            result.insert(
                slot,
                Moe2BudgetPreview {
                    consent,
                    reward_sompi: ask.reward_sompi,
                    collateral_sompi: capital,
                    minimum_liability_sompi: ask.liability_sompi,
                    allocation_epoch: epoch,
                },
            );
        }
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC budget view moved"));
        }
        if let Some(delta) = delta {
            delta.entries.extend(writes.into_iter().map(Into::into));
        }
        Ok(result)
    }
}
