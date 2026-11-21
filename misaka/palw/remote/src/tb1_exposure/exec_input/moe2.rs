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
    palw_bond_budget_v1::{PalwBudgetAskV1, PalwBudgetOriginV1, palw_bond_budget_export_cap_v1},
    palw_mode_v2::PalwConsensusMode,
    palw_model_tier_v1::palw_model_tier_of_v1,
    palw_state_v2::{PalwChainStateV2, palw_bond_may_take_work_v2},
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
        config: &Params,
        state: &PalwChainStateV2,
        reward: u64,
    ) -> io::Result<kaspa_consensus_core::palw_model_market_v1::moe2::Moe2RewardAllocation> {
        let network = palw_network_domain_v2_for(config.net.to_string().as_bytes(), Some(config.genesis.hash));
        if self.network != network || state.state_root() != self.state_root {
            return Err(invalid("MOE2 preview state or network differs from signed preparation"));
        }
        self.terms.preview_at_anchor(config, state, reward).map_err(|_| invalid("MOE2 preview outside signed quote or market policy"))
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
        let quote =
            self.entries.get(&slot).and_then(|entry| entry.terms.moe2_quote).ok_or_else(|| invalid("no MOE2 quote at EXEC slot"))?;
        let header = native.get_header(self.preparation).map_err(io::Error::other)?;
        let state_root = state_root_of_pinned_header_v1(&header, self.preparation).map_err(io::Error::other)?;
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC quote view moved"));
        }
        Ok(ClosedMoe2Quote { terms: quote, state_root, network: Hash64::from_bytes(self.rounds.exec.chain_id) })
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
        let catalog = bundle.state.model_tier_v1().filter(|c| c.active_at(daa)).ok_or_else(|| invalid("missing MOE2 catalog"))?;
        let mut budget = state.bond_budget().ok_or_else(|| invalid("missing native MOE2 budget"))?.clone();
        if budget.header.policy_digest != policy.digest() {
            return Err(invalid("native budget policy differs from MOE2 configuration"));
        }
        // The engine quotes export per 1,000 sompi held, including its protocol ceiling.
        let rate = u128::from(palw_bond_budget_export_cap_v1(policy, 1000));
        let mut result = BTreeMap::new();
        for (&slot, entry) in &self.entries {
            let Some(quote) = entry.terms.moe2_quote else { continue };
            let class = state.class(&quote.model).ok_or_else(|| invalid("missing MOE2 model at close"))?;
            let core = palw_model_tier_of_v1(Some(catalog), &quote.model, &quote.artifact_root, class.registrant_bond.is_none(), daa)
                .is_approved();
            if quote.policy != budget.header.policy_digest
                || class.artifact_root != quote.artifact_root
                || core == state.model_market(&quote.model).is_some()
                || (!core && class.registered_daa < from)
            {
                return Err(invalid("MOE2 slot differs from close policy or model cohort"));
            }
            let bond = state
                .bond(&quote.funding_bond)
                .filter(|b| palw_bond_may_take_work_v2(b, bundle.state.min_collateral_sompi()))
                .ok_or_else(|| invalid("MOE2 funding bond unavailable at close"))?;
            let reward = quote.limits.max_reward;
            let liability = match (reward, rate) {
                (0, _) => 0,
                (_, 0) => return Err(invalid("MOE2 budget cannot export the signed reward")),
                _ => (u128::from(reward) * 1000).div_ceil(rate),
            };
            let consent = Hash64::from_bytes(consent_ref(reg_key(&entry.context), slot));
            let ask = PalwBudgetAskV1 { block_units: 0, final_weight: 0, reward_sompi: reward, liability_sompi: liability };
            let grant = budget
                .reserve_claim_v1(
                    policy,
                    bond.collateral,
                    consent,
                    quote.funding_bond,
                    Some(quote.model),
                    daa,
                    ask,
                    PalwBudgetOriginV1::Attempt,
                    &mut Vec::new(),
                )
                .map_err(|err| io::Error::other(err.to_string()))?;
            if grant.reward_sompi != reward {
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
                    reward_sompi: reward,
                    collateral_sompi: bond.collateral,
                    minimum_liability_sompi: liability,
                    allocation_epoch: epoch,
                },
            );
        }
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::WouldBlock, "EXEC budget view moved"));
        }
        Ok(result)
    }
}
