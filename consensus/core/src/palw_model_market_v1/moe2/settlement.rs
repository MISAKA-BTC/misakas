//! Local execution of an already funded and authorized MOE2 settlement.
//! This module has no network object decoder or public activation. A caller
//! operating a versioned native store owns the transaction and fork boundary.
use super::{
    BASIS_POINTS, Moe2ModelAllocation, Moe2PriceLimits, Moe2QuantityLimits, Moe2QuoteRefusal, Moe2RegisteredModel,
    Moe2RewardAllocation, Moe2RewardSplit, Moe2WorkerPlan, PurchaseCurve,
};
use crate::Hash64;
use crate::palw_bond_budget_v1::{
    PALW_BUDGET_TABLE_CLAIMS_V1, PalwBondBudgetPolicyV1, PalwBondBudgetStateV1, PalwBudgetAskV1, PalwBudgetClaimRowV1,
    PalwBudgetDimV1, PalwBudgetOriginV1, PalwBudgetRefusalV1, PalwBudgetWriteV1,
};
use crate::palw_model_market_v1::PalwModelMarketV1;
use crate::palw_settlement_liability_v1::{
    PalwSettlementLiabilityRefusalV1, PalwSettlementTermsSourcedV1, palw_settlement_liability_sourced_v1,
};
use crate::palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2};
use std::collections::{BTreeMap, BTreeSet};

struct SettlementHashWriter<'a>(&'a mut blake2b_simd::State);

impl borsh::io::Write for SettlementHashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> borsh::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> borsh::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2Position {
    pub point: PalwBlockContextV2,
    pub ordinal: u32,
}

impl Moe2Position {
    /// DAA can stay equal across successive selected blocks. Only the native
    /// blue score advances strictly. Ordinals order settlements within one block.
    fn follows(self, prior: Self) -> bool {
        if self.point.block == prior.point.block {
            self.point == prior.point && self.ordinal > prior.ordinal
        } else {
            self.point.blue_score > prior.point.blue_score && self.point.daa_score >= prior.point.daa_score
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2SettlementTerms {
    pub claim: Hash64,
    pub funding_bond: PalwBondKeyV2,
    pub model: Hash64,
    pub artifact_root: Hash64,
    pub policy: Hash64,
    pub quote_anchor: Hash64,
    pub valid_from: u64,
    pub deadline: u64,
    pub limits: Moe2QuantityLimits,
    pub plan: Moe2WorkerPlan,
    pub price_limits: Moe2PriceLimits,
}

/// The finite terms an EXEC owner can fix before the Original or mining claim exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2PreDrawQuote {
    pub funding_bond: PalwBondKeyV2,
    pub model: Hash64,
    pub artifact_root: Hash64,
    pub policy: Hash64,
    pub quote_anchor: Hash64,
    pub valid_from: u64,
    pub deadline: u64,
    pub limits: Moe2QuantityLimits,
    pub plan: Moe2WorkerPlan,
    pub price_limits: Moe2PriceLimits,
}

/// Signed pre-draw slot as resolved by the private EXEC close reader. The
/// block owner authenticates the registration, its close, and the winning
/// claim's membership in this slot before invoking the accounting fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub struct Moe2ExecSlot {
    pub registration: [u8; 64],
    pub slot: u64,
    pub close: Hash64,
    pub close_daa: u64,
    pub quote: Moe2PreDrawQuote,
}

impl Moe2ExecSlot {
    pub const CONSENT_DOMAIN: &'static [u8] = b"MISAKA/private/FOW1/MOE2/quote-consent/v1";

    /// The signed deadline is inclusive. The first later block retires the slot.
    pub fn budget_origin(&self) -> Result<PalwBudgetOriginV1, Moe2SettlementError> {
        let expires_at = self.quote.deadline.checked_add(1).ok_or(Moe2SettlementError::OutsideWindow)?;
        Ok(PalwBudgetOriginV1::Moe2Exec { expires_at })
    }

    /// Consume capacity for this authenticated slot. The owning batch discards
    /// all scratch writes if any slot is refused.
    pub fn reserve_budget(
        &self,
        budget: &mut PalwBondBudgetStateV1,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        ask: PalwBudgetAskV1,
        writes: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<u64, Moe2SettlementError> {
        use Moe2SettlementError::*;
        if ask.reward_sompi != self.quote.limits.max_reward || self.quote.policy != budget.header.policy_digest {
            return Err(ReservationMismatch);
        }
        let key = self.reservation_key();
        let granted = budget
            .reserve_claim_v1(
                policy,
                capital,
                key,
                self.quote.funding_bond,
                Some(self.quote.model),
                self.close_daa,
                ask,
                self.budget_origin()?,
                writes,
            )
            .map_err(Budget)?;
        if granted.reward_sompi != ask.reward_sompi {
            return Err(ReservationMismatch);
        }
        budget
            .claim_row(&key)
            .and_then(|r| r.model)
            .filter(|(id, _)| *id == self.quote.model)
            .map(|(_, epoch)| epoch)
            .ok_or(ReservationMismatch)
    }

    pub fn consent_ref(registration: [u8; 64], slot: u64) -> [u8; 64] {
        use sha3::{Digest, Sha3_512};
        Sha3_512::new()
            .chain_update(Self::CONSENT_DOMAIN)
            .chain_update(registration)
            .chain_update(slot.to_be_bytes())
            .finalize()
            .into()
    }

    /// A native reservation belongs to this exact close and quote. The funder's
    /// consent is still identified by the registration and slot before close.
    pub fn reservation_key(&self) -> Hash64 {
        Self::reservation_key_for_digest(self.slot_digest())
    }

    fn reservation_key_for_digest(signed_slot: Hash64) -> Hash64 {
        kaspa_hashes::blake2b_512_keyed(b"misaka-palw/private/moe2/native-exec-reservation/v1", signed_slot.as_byte_slice())
    }

    fn slot_digest(&self) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/private/moe2/exec-admission/v1").to_state();
        borsh::BorshSerialize::serialize(self, &mut SettlementHashWriter(&mut h)).expect("fixed-size EXEC slot serializes");
        Hash64::from_bytes(*h.finalize().as_array())
    }

    fn binding(&self) -> (Hash64, Hash64) {
        (Hash64::from_bytes(Self::consent_ref(self.registration, self.slot)), self.slot_digest())
    }

    fn undo_digest(&self, undo: &Moe2SettlementUndo) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/private/moe2/exec-undo/v1").to_state();
        borsh::BorshSerialize::serialize(self, &mut SettlementHashWriter(&mut h)).expect("fixed-size EXEC slot serializes");
        h.update(undo.digest().as_byte_slice());
        Hash64::from_bytes(*h.finalize().as_array())
    }
}

impl Moe2PreDrawQuote {
    /// Price a hypothetical reward against the selected quote anchor's post-state.
    /// The caller authenticates the state root at its pinned preparation header.
    pub fn preview_at_anchor(
        &self,
        config: &crate::config::params::Params,
        state: &crate::palw_state_v2::PalwChainStateV2,
        reward: u64,
    ) -> Result<Moe2RewardAllocation, Moe2SettlementError> {
        let point = state.last_point().ok_or(Moe2SettlementError::UnknownClaimContext)?;
        self.preview_at_position(config, state, state, point.daa_score, reward)
    }

    /// Reprice the same signed offer at a later position. The caller supplies the
    /// authenticated quote-anchor state and the pre-state of the execution block.
    pub fn preview_at_position(
        &self,
        config: &crate::config::params::Params,
        anchor: &crate::palw_state_v2::PalwChainStateV2,
        current: &crate::palw_state_v2::PalwChainStateV2,
        execution_daa: u64,
        reward: u64,
    ) -> Result<Moe2RewardAllocation, Moe2SettlementError> {
        use Moe2SettlementError::*;
        let a = anchor.last_point().ok_or(UnknownClaimContext)?;
        let b = current.last_point().ok_or(UnknownClaimContext)?;
        if a.block != self.quote_anchor || a.blue_score > b.blue_score || a.daa_score > b.daa_score || b.daa_score > execution_daa {
            return Err(UnknownClaimContext);
        }
        if self.deadline == u64::MAX
            || !(self.valid_from..=self.deadline).contains(&a.daa_score)
            || !(self.valid_from..=self.deadline).contains(&execution_daa)
        {
            return Err(OutsideWindow);
        }
        if u128::from(self.price_limits.max_output_slippage_bps) > BASIS_POINTS {
            return Err(Quote(Moe2QuoteRefusal::OutsideQuote));
        }
        let reference = self.allocation_at(config, anchor, a.daa_score)?;
        let allocation = self.allocation_at(config, current, execution_daa)?;
        let split = allocation.quote_reward(reward, self.plan, self.limits).map_err(Quote)?;
        match (reference, allocation, split.market) {
            (Moe2ModelAllocation::Additional(original), Moe2ModelAllocation::Additional(market), Some(q))
                if original.opened_daa == market.opened_daa && original.virtual_sompi == market.virtual_sompi =>
            {
                self.price_limits.check(PurchaseCurve::from_market(&original).map_err(Quote)?, &market, &q).map_err(Quote)?;
            }
            (Moe2ModelAllocation::Core, Moe2ModelAllocation::Core, None) => {}
            _ => return Err(UnknownRegistration),
        }
        Ok(split)
    }

    /// Size the funded slot at its selected EXEC close, before any draw. The
    /// caller reserves against the native budget in frozen slot order.
    pub fn budget_ask_at(
        &self,
        config: &crate::config::params::Params,
        state: &crate::palw_state_v2::PalwChainStateV2,
        daa: u64,
    ) -> Result<(u64, crate::palw_bond_budget_v1::PalwBudgetAskV1), Moe2SettlementError> {
        use crate::palw_mode_v2::PalwConsensusMode;
        use Moe2SettlementError::UnknownClaimContext;
        let from = config.private_moe2_registration_from().ok_or(UnknownClaimContext)?;
        let PalwConsensusMode::ConsensusV2(bundle) = &config.palw_consensus_mode else { return Err(UnknownClaimContext) };
        self.budget_ask_at_state(&bundle.state, from, state, daa)
    }

    pub(crate) fn budget_ask_at_state(
        &self,
        params: &PalwStateParamsV2,
        from: u64,
        state: &PalwChainStateV2,
        daa: u64,
    ) -> Result<(u64, PalwBudgetAskV1), Moe2SettlementError> {
        use crate::palw_bond_budget_v1::palw_bond_budget_export_cap_v1;
        use crate::palw_state_v2::palw_bond_may_take_work_v2;
        use Moe2SettlementError::*;
        if self.deadline == u64::MAX || !(self.valid_from..=self.deadline).contains(&daa) {
            return Err(OutsideWindow);
        }
        self.allocation_at_state(params, from, state, daa)?;
        let policy = &params.bond_budget().ok_or(ReservationMismatch)?.policy;
        let capital = state
            .bond(&self.funding_bond)
            .filter(|bond| palw_bond_may_take_work_v2(bond, params.min_collateral_sompi()))
            .ok_or(ReservationMismatch)?
            .collateral;
        let reward_sompi = self.limits.max_reward;
        let rate = u128::from(palw_bond_budget_export_cap_v1(policy, 1000));
        let liability_sompi = match (reward_sompi, rate) {
            (0, _) => 0,
            (_, 0) => return Err(ReservationMismatch),
            _ => (u128::from(reward_sompi) * 1000).div_ceil(rate),
        };
        Ok((capital, PalwBudgetAskV1 { block_units: 0, final_weight: 0, reward_sompi, liability_sompi }))
    }

    pub(crate) fn allocation_at(
        &self,
        config: &crate::config::params::Params,
        state: &crate::palw_state_v2::PalwChainStateV2,
        daa: u64,
    ) -> Result<Moe2ModelAllocation, Moe2SettlementError> {
        use crate::palw_mode_v2::PalwConsensusMode;
        use Moe2SettlementError::UnknownClaimContext;
        let from = config.private_moe2_registration_from().ok_or(UnknownClaimContext)?;
        let PalwConsensusMode::ConsensusV2(bundle) = &config.palw_consensus_mode else { return Err(UnknownClaimContext) };
        self.allocation_at_state(&bundle.state, from, state, daa)
    }

    fn allocation_at_state(
        &self,
        params: &PalwStateParamsV2,
        from: u64,
        state: &PalwChainStateV2,
        daa: u64,
    ) -> Result<Moe2ModelAllocation, Moe2SettlementError> {
        use crate::palw_model_tier_v1::palw_model_tier_of_v1;
        use Moe2SettlementError::*;
        let catalog = params.model_tier_v1().filter(|c| c.active_at(daa)).ok_or(UnknownRegistration)?;
        let class = state.class(&self.model).ok_or(UnknownRegistration)?;
        if self.valid_from < from
            || class.artifact_root != self.artifact_root
            || self.valid_from < class.registered_daa
            || params.bond_budget().is_none_or(|b| !b.active_at(daa) || b.policy.digest() != self.policy)
            || state.bond_budget().is_none_or(|b| b.header.policy_digest != self.policy)
            || state.bond(&self.funding_bond).is_none()
        {
            return Err(UnknownClaimContext);
        }
        let approved =
            palw_model_tier_of_v1(Some(catalog), &self.model, &self.artifact_root, class.registrant_bond.is_none(), daa).is_approved();
        match (approved, state.model_market(&self.model)) {
            (true, None) if daa >= from => Ok(Moe2ModelAllocation::Core),
            (false, Some(market)) if class.registered_daa >= from => Ok(Moe2ModelAllocation::Additional(*market)),
            _ => Err(UnknownRegistration),
        }
    }

    /// The owning settlement fold supplies the independently authenticated claim.
    /// Quoting alone creates no claim, reservation, or payment authority.
    pub fn for_claim(self, claim: Hash64) -> Moe2SettlementTerms {
        let Self { funding_bond, model, artifact_root, policy, quote_anchor, valid_from, deadline, limits, plan, price_limits } = self;
        Moe2SettlementTerms {
            claim,
            funding_bond,
            model,
            artifact_root,
            policy,
            quote_anchor,
            valid_from,
            deadline,
            limits,
            plan,
            price_limits,
        }
    }
}

impl Moe2SettlementTerms {
    /// Private accounting key for a reservation made under these pre-draw terms.
    /// Distinct from both the original claim ID and any public wire identifier.
    pub fn reservation_key(&self) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/private/moe2/reward-reservation/v1").to_state();
        h.update(&borsh::to_vec(self).expect("fixed-size MOE2 terms serialize"));
        Hash64::from_bytes(*h.finalize().as_array())
    }
}

/// Bounded private store record for disconnecting a settlement. The block
/// owner may discard it after the rollback horizon. Its digest in the settled
/// entry binds the exact admission and the history of both LP indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2SettlementUndo {
    terms: Moe2SettlementTerms,
    reference: Option<PurchaseCurve>,
    before_model: Moe2RegisteredModel,
    before_budget: PalwBudgetClaimRowV1,
    draw: u64,
    position: Moe2Position,
    previous_position: Option<Moe2Position>,
    had_model_lp: bool,
    had_root_lp: bool,
}

impl Moe2SettlementUndo {
    pub(crate) fn point(&self) -> PalwBlockContextV2 {
        self.position.point
    }

    pub(crate) fn claim(&self) -> Hash64 {
        self.terms.claim
    }

    pub(crate) fn model(&self) -> Hash64 {
        self.terms.model
    }

    pub(crate) fn market_before(&self) -> Option<PalwModelMarketV1> {
        match self.before_model.allocation {
            Moe2ModelAllocation::Core => None,
            Moe2ModelAllocation::Additional(market) => Some(market),
        }
    }

    pub(crate) fn exec_debit(&self, slot: &Moe2ExecSlot) -> Result<PalwBudgetWriteV1, Moe2SettlementError> {
        let after = self.after_budget(true)?;
        Ok(self.debit(slot.reservation_key(), &after))
    }

    fn after_budget(&self, exec: bool) -> Result<PalwBudgetClaimRowV1, Moe2SettlementError> {
        if exec {
            return self.before_budget.after_exec_slot_reward(self.draw).map_err(|_| Moe2SettlementError::UndoMismatch);
        }
        let mut after = self.before_budget;
        after.consumed.reward_sompi = after.consumed.reward_sompi.checked_add(self.draw).ok_or(Moe2SettlementError::UndoMismatch)?;
        Ok(after)
    }

    fn debit(&self, key: Hash64, after: &PalwBudgetClaimRowV1) -> PalwBudgetWriteV1 {
        PalwBudgetWriteV1::Row {
            table: PALW_BUDGET_TABLE_CLAIMS_V1,
            key: borsh::to_vec(&key).expect("budget key serializes"),
            old: Some(borsh::to_vec(&self.before_budget).expect("budget row serializes")),
            new: Some(borsh::to_vec(after).expect("budget row serializes")),
        }
    }

    fn digest(&self) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/private/moe2/settlement-undo/v1").to_state();
        borsh::BorshSerialize::serialize(self, &mut SettlementHashWriter(&mut h)).expect("fixed-size settlement undo serializes");
        Hash64::from_bytes(*h.finalize().as_array())
    }
}

/// Accounting history. The recorded quantities have already entered the market.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct LockedLiquidityReceipt {
    pub claim: Hash64,
    pub policy: Hash64,
    pub model: Hash64,
    pub artifact_root: Hash64,
    pub position: Moe2Position,
    pub before: PalwModelMarketV1,
    pub after: PalwModelMarketV1,
    pub purchase: u64,
    pub liquidity: u64,
    pub purchased_units: u64,
    pub liquidity_units: u64,
    pub retired_units: u64,
}

/// The owning fold sources these terms and worker releases from this claim's
/// funding and vesting rows at the canonical settlement position.
#[derive(Clone, Copy, Debug)]
pub struct Moe2SettlementFunding {
    pub claim: Hash64,
    pub position: Moe2Position,
    pub liability: PalwSettlementTermsSourcedV1,
    pub miner_now: u64,
    pub panel_now: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2SettledReward {
    pub allocation: Moe2RewardAllocation,
    pub miner_now: u64,
    pub panel_now: u64,
    pub vest_until: Option<u64>,
}

/// Settlement data retained by the isolated native state even if its
/// accounting checkpoint is unavailable. Every row is inserted once.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2NativeExecSettlement {
    pub claim: Hash64,
    pub position: Moe2Position,
    pub model: Hash64,
    pub artifact_root: Hash64,
    pub reservation: Hash64,
    pub signed_slot: Hash64,
    pub reward: Moe2SettledReward,
    pub permanent_liquidity: Option<LockedLiquidityReceipt>,
}

impl Moe2NativeExecSettlement {
    pub(crate) fn has_matching_reservation_key(&self) -> bool {
        Moe2ExecSlot::reservation_key_for_digest(self.signed_slot) == self.reservation
    }

    pub(crate) fn total_reward(&self) -> Option<u64> {
        let allocation = &self.reward.allocation;
        let market = allocation.market.map_or(Some(0), |q| q.split.purchase.checked_add(q.split.liquidity))?;
        allocation.miner.checked_add(allocation.panel_pool)?.checked_add(market)
    }

    pub(crate) fn has_consistent_reward(&self) -> bool {
        let Moe2RewardAllocation { miner, panel_pool, market } = self.reward.allocation;
        let Some(workers) = miner.checked_add(panel_pool) else { return false };
        if self.reward.miner_now > miner
            || self.reward.panel_now > panel_pool
            || ((self.reward.miner_now, self.reward.panel_now) != (miner, panel_pool) && self.reward.vest_until.is_none())
            || (panel_pool != 0 && (market.is_none() || panel_pool != workers / 5))
            || market.is_some_and(|q| {
                q.split.workers != workers || self.total_reward().is_none_or(|total| q.split != Moe2RewardSplit::additional(total))
            })
        {
            return false;
        }
        match (self.reward.allocation.market, self.permanent_liquidity) {
            (Some(q), Some(lp)) => {
                q.split.liquidity > 0
                    && (lp.claim, lp.model, lp.artifact_root, lp.position)
                        == (self.claim, self.model, self.artifact_root, self.position)
                    && (lp.after, lp.purchase, lp.liquidity, lp.purchased_units, lp.liquidity_units, lp.retired_units)
                        == (q.after, q.split.purchase, q.split.liquidity, q.purchased_units, q.liquidity_units, q.retired_units)
            }
            (market, None) => market.is_none_or(|q| q.split.liquidity == 0),
            (None, Some(_)) => false,
        }
    }

    pub(crate) fn from_settlement(
        slot: &Moe2ExecSlot,
        book: &Moe2SettlementBook,
        undo: &Moe2SettlementUndo,
        reward: Moe2SettledReward,
    ) -> Self {
        Self {
            claim: undo.claim(),
            position: undo.position,
            model: slot.quote.model,
            artifact_root: slot.quote.artifact_root,
            reservation: slot.reservation_key(),
            signed_slot: slot.slot_digest(),
            reward,
            permanent_liquidity: book.receipt(&undo.claim()).copied(),
        }
    }
}

/// Release from an independently authorized mining claim. Its owner authenticates
/// the reward, worker plan, vesting and obligation horizons before accounting.
#[derive(Clone, Copy, Debug)]
pub struct Moe2WorkerRelease {
    pub reward: u64,
    pub miner_now: u64,
    pub panel_now: u64,
    pub liability_until: u64,
    pub signed_hard_deadline: u64,
    pub material_ttl_until: u64,
    pub vest_until: Option<u64>,
    pub retire_not_before: u64,
}

impl Moe2SettlementFunding {
    fn check(&self, allocation: Moe2RewardAllocation) -> Result<Moe2SettledReward, Moe2SettlementError> {
        let t = &self.liability;
        let market_export = allocation.market.map_or(0, |q| q.split.purchase + q.split.liquidity);
        if t.draw != t.payment
            || self.miner_now > allocation.miner
            || self.panel_now > allocation.panel_pool
            || t.paid_now != market_export + self.miner_now + self.panel_now
            || (t.paid_now < t.payment && t.vest_until.is_none())
        {
            return Err(Moe2SettlementError::Liability(PalwSettlementLiabilityRefusalV1::Malformed));
        }
        palw_settlement_liability_sourced_v1(t).map_err(Moe2SettlementError::Liability)?;
        Ok(Moe2SettledReward { allocation, miner_now: self.miner_now, panel_now: self.panel_now, vest_until: t.vest_until })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Moe2SettlementError {
    UnknownRegistration,
    DuplicateRegistration,
    PermanentLiquidityHistory,
    UnknownClaimContext,
    AlreadyAdmitted,
    AlreadySettled,
    WorkAlreadyConsumed,
    ReservationMismatch,
    WorkerOutputsMismatch,
    PositionOutOfOrder,
    UndoMismatch,
    CheckpointMismatch,
    OutsideWindow,
    Budget(PalwBudgetRefusalV1),
    Quote(Moe2QuoteRefusal),
    Liability(PalwSettlementLiabilityRefusalV1),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2SettlementBook {
    models: BTreeMap<Hash64, Moe2RegisteredModel>,
    admitted: BTreeMap<Hash64, (Moe2SettlementTerms, Option<PurchaseCurve>)>,
    settled: BTreeMap<Hash64, (Moe2SettledReward, Hash64)>,
    receipts: BTreeMap<Hash64, LockedLiquidityReceipt>,
    permanent_lp_models: BTreeSet<Hash64>,
    permanent_lp_roots: BTreeSet<Hash64>,
    last_settlement: Option<Moe2Position>,
    exec_claims: BTreeMap<Hash64, (Hash64, Hash64)>,
    assigned_slots: BTreeSet<Hash64>,
}

/// Market projection changes installed at one native block. The branch owner
/// journals this alongside the native delta and reverses settlements and claim
/// admissions before removing the market projection.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Moe2MarketBlockUndo {
    point: PalwBlockContextV2,
    parent: Option<PalwBlockContextV2>,
    changes: BTreeMap<Hash64, (Option<Moe2RegisteredModel>, Moe2RegisteredModel)>,
}

impl Moe2SettlementBook {
    pub(crate) fn native_award_scratch_bytes() -> Result<u64, crate::palw_native_resource_v1::NativeCoverageV1> {
        use crate::palw_native_resource_v1::{TirNativeRequestsV1, typed::BTreeNodeEnvelopeV1 as Node};
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Node<Hash64, Moe2RegisteredModel>>(1)?;
        q.exact_v1::<Node<Hash64, (Moe2SettlementTerms, Option<PurchaseCurve>)>>(1)?;
        q.exact_v1::<Node<Hash64, (Moe2SettledReward, Hash64)>>(1)?;
        q.exact_v1::<Node<Hash64, LockedLiquidityReceipt>>(1)?;
        q.exact_v1::<Node<Hash64, ()>>(3)?;
        q.exact_v1::<Node<Hash64, (Hash64, Hash64)>>(1)?;
        Ok(q.bytes_v1())
    }

    /// Scratch projection for one award in the native fold. The native
    /// reservation, settlement and work-use rows own its durable history.
    pub(crate) fn for_native_award(model: Moe2RegisteredModel, prior_in_block: Option<Moe2Position>) -> Self {
        Self { models: BTreeMap::from([(model.model, model)]), last_settlement: prior_in_block, ..Self::default() }
    }

    /// Commit a private store snapshot to its companion native state root. The
    /// owner retains this commitment independently of the checkpoint bytes.
    fn checkpoint_digest(domain: &[u8], native_root: Hash64, bytes: &[u8]) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
        h.update(native_root.as_byte_slice());
        h.update(bytes);
        Hash64::from_bytes(*h.finalize().as_array())
    }

    pub fn checkpoint(&self, native_root: Hash64) -> (Hash64, Vec<u8>) {
        if self.exec_claims.is_empty() {
            let rows = (
                &self.models,
                &self.admitted,
                &self.settled,
                &self.receipts,
                &self.permanent_lp_models,
                &self.permanent_lp_roots,
                &self.last_settlement,
            );
            let bytes = borsh::to_vec(&rows).expect("MOE2 legacy store rows serialize");
            return (Self::checkpoint_digest(b"misaka-palw/private/moe2/store/v1", native_root, &bytes), bytes);
        }
        let bytes = borsh::to_vec(self).expect("MOE2 EXEC store rows serialize");
        (Self::checkpoint_digest(b"misaka-palw/private/moe2/store/v2", native_root, &bytes), bytes)
    }

    /// The byte budget and committed root come from the owner, never the snapshot sender.
    pub fn from_checkpoint(
        native_root: Hash64,
        expected: Hash64,
        bytes: &[u8],
        byte_budget: usize,
    ) -> Result<Self, Moe2SettlementError> {
        use Moe2SettlementError::CheckpointMismatch;
        if bytes.len() > byte_budget {
            return Err(CheckpointMismatch);
        }
        if Self::checkpoint_digest(b"misaka-palw/private/moe2/store/v2", native_root, bytes) == expected {
            let book: Self = borsh::from_slice(bytes).map_err(|_| CheckpointMismatch)?;
            if book.exec_claims.is_empty()
                || book.exec_claims.len() != book.assigned_slots.len()
                || book.exec_claims.values().map(|(key, _)| *key).collect::<BTreeSet<_>>() != book.assigned_slots
            {
                return Err(CheckpointMismatch);
            }
            return Ok(book);
        }
        if Self::checkpoint_digest(b"misaka-palw/private/moe2/store/v1", native_root, bytes) != expected {
            return Err(CheckpointMismatch);
        }
        let (models, admitted, settled, receipts, permanent_lp_models, permanent_lp_roots, last_settlement) =
            borsh::from_slice(bytes).map_err(|_| CheckpointMismatch)?;
        Ok(Self { models, admitted, settled, receipts, permanent_lp_models, permanent_lp_roots, last_settlement, ..Self::default() })
    }

    /// Apply new registrations and native repricing of existing pairs in one batch. The
    /// owning fold commits this projection together with that block's class
    /// delta. A second registration in a block cannot leave the first half-installed.
    pub fn register_block(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        catalog: &crate::palw_model_tier_v1::PalwModelTierMirrorV1,
        delta: &crate::palw_state_v2::PalwStateDeltaV2,
    ) -> Result<Moe2MarketBlockUndo, Moe2SettlementError> {
        use crate::palw_state_v2::PalwDeltaEntryV2;
        use Moe2SettlementError::*;
        if state.last_point() != Some(&delta.point) {
            return Err(UnknownClaimContext);
        }
        let parent = delta
            .entries
            .iter()
            .find_map(|entry| match entry {
                PalwDeltaEntryV2::LastPoint { old, new } if new.as_ref() == Some(&delta.point) => Some(*old),
                _ => None,
            })
            .ok_or(UnknownClaimContext)?;
        let mut registered = BTreeMap::new();
        for entry in &delta.entries {
            let PalwDeltaEntryV2::Class { key, old: None, new: Some(class) } = entry else {
                continue;
            };
            let model =
                Moe2RegisteredModel::at_registration(state, catalog, *key, delta.point.daa_score).ok_or(UnknownRegistration)?;
            if model.artifact_root != class.artifact_root || model.owner != class.registrant_bond {
                return Err(UnknownRegistration);
            }
            if model.allocation == Moe2ModelAllocation::Core {
                self.check_core_designation(*key, model.artifact_root)?;
            }
            if self.models.contains_key(key) || registered.insert(*key, model).is_some() {
                return Err(DuplicateRegistration);
            }
        }
        // Later entries in the registration block can fund or trade the new
        // pair. Begin the book at the actual native post-state, checking that
        // the native delta opened the prescribed market before its first move.
        let mut unopened: BTreeSet<_> = registered
            .iter()
            .filter_map(|(&key, row)| matches!(row.allocation, Moe2ModelAllocation::Additional(_)).then_some(key))
            .collect();
        let mut seen_classes = BTreeSet::new();
        for entry in &delta.entries {
            if let PalwDeltaEntryV2::Class { key, old: None, new: Some(_) } = entry {
                seen_classes.insert(*key);
            }
            let PalwDeltaEntryV2::ModelMarket { key, old, new } = entry else { continue };
            // Keep previously registered pairs at the canonical execution price
            // when ordinary trades appear in later blocks. Skip legacy pairs.
            let model = match registered.entry(*key) {
                std::collections::btree_map::Entry::Occupied(row) => row.into_mut(),
                std::collections::btree_map::Entry::Vacant(row) => {
                    let Some(prior) = self.models.get(key) else { continue };
                    row.insert(*prior)
                }
            };
            let Moe2ModelAllocation::Additional(current) = &mut model.allocation else { return Err(UnknownRegistration) };
            if unopened.remove(key) {
                if !seen_classes.contains(key) || old.is_some() || new.as_ref() != Some(current) {
                    return Err(UnknownRegistration);
                }
            } else {
                if old.as_ref() != Some(current) {
                    return Err(UnknownRegistration);
                }
                *current = new.ok_or(UnknownRegistration)?;
            }
        }
        if !unopened.is_empty() {
            return Err(UnknownRegistration);
        }
        for (key, model) in &registered {
            let expected = match &model.allocation {
                Moe2ModelAllocation::Additional(current) => Some(current),
                Moe2ModelAllocation::Core => None,
            };
            if state.model_market(key) != expected {
                return Err(UnknownRegistration);
            }
        }
        let changes = registered.iter().map(|(key, after)| (*key, (self.models.get(key).copied(), *after))).collect();
        self.models.append(&mut registered);
        Ok(Moe2MarketBlockUndo { point: delta.point, parent, changes })
    }

    /// Restore market rows after the native block has been reversed. The
    /// disconnected point comes from the block owner, independently of the journal.
    pub fn revert_market_block(
        &mut self,
        disconnected: &PalwBlockContextV2,
        parent_state: &crate::palw_state_v2::PalwChainStateV2,
        undo: &Moe2MarketBlockUndo,
    ) -> Result<(), Moe2SettlementError> {
        use Moe2SettlementError::UndoMismatch;
        if disconnected != &undo.point
            || parent_state.last_point() != undo.parent.as_ref()
            || self.last_settlement.is_some_and(|last| last.point.blue_score >= undo.point.blue_score)
        {
            return Err(UndoMismatch);
        }
        for (key, (before, after)) in &undo.changes {
            if self.models.get(key) != Some(after) {
                return Err(UndoMismatch);
            }
            let restored_market = before.as_ref().and_then(|row| match &row.allocation {
                Moe2ModelAllocation::Additional(market) => Some(market),
                Moe2ModelAllocation::Core => None,
            });
            if parent_state.model_market(key) != restored_market {
                return Err(UndoMismatch);
            }
            if let Some(before) = before {
                if parent_state.class(key).is_none_or(|class| {
                    (class.artifact_root, class.registrant_bond, class.registered_daa)
                        != (before.artifact_root, before.owner, before.registration_daa)
                }) {
                    return Err(UndoMismatch);
                }
            } else if parent_state.class(key).is_some()
                || self.admitted.values().any(|(terms, _)| &terms.model == key)
                || self.permanent_lp_models.contains(key)
            {
                return Err(UndoMismatch);
            }
        }
        for (key, (before, _)) in &undo.changes {
            if let Some(before) = before {
                self.models.insert(*key, *before);
            } else {
                self.models.remove(key);
            }
        }
        Ok(())
    }

    pub fn model(&self, model: &Hash64) -> Option<&Moe2RegisteredModel> {
        self.models.get(model)
    }
    pub(crate) fn admitted_terms(&self, claim: &Hash64) -> Option<&Moe2SettlementTerms> {
        self.admitted.get(claim).map(|(terms, _)| terms)
    }
    pub fn entitlement(&self, claim: &Hash64) -> Option<&Moe2SettledReward> {
        self.settled.get(claim).map(|(reward, _)| reward)
    }
    pub fn receipt(&self, claim: &Hash64) -> Option<&LockedLiquidityReceipt> {
        self.receipts.get(claim)
    }

    /// A model ID and all aliases of a root share the permanent-LP exclusion.
    pub fn has_permanent_lp(&self, model: Hash64, artifact_root: Hash64) -> bool {
        self.permanent_lp_models.contains(&model) || self.permanent_lp_roots.contains(&artifact_root)
    }

    /// The same history check applies to registrations and to annual catalog designations.
    pub fn check_core_designation(&self, model: Hash64, artifact_root: Hash64) -> Result<(), Moe2SettlementError> {
        if self.has_permanent_lp(model, artifact_root) { Err(Moe2SettlementError::PermanentLiquidityHistory) } else { Ok(()) }
    }

    /// Bind terms before the draw. Neither a failed quote nor a retry can
    /// replace the policy, change the owner market, or extend the deadline.
    /// Resolve the signed anchor in the owning fold's canonical historical
    /// view. Keep that price even when admission and execution see later trades.
    pub fn admit(
        &mut self,
        terms: Moe2SettlementTerms,
        resolve: impl FnOnce(Hash64, Hash64) -> Option<Moe2RegisteredModel>,
    ) -> Result<(), Moe2SettlementError> {
        use Moe2SettlementError::*;
        let model = self.models.get(&terms.model).ok_or(UnknownRegistration)?;
        if terms.artifact_root != model.artifact_root
            || terms.valid_from < model.registration_daa
            || terms.deadline < terms.valid_from
            || terms.deadline == u64::MAX
        {
            return Err(UnknownClaimContext);
        }
        if self.admitted.contains_key(&terms.claim) || self.settled.contains_key(&terms.claim) {
            return Err(AlreadyAdmitted);
        }
        let reference = if let Moe2ModelAllocation::Additional(current) = model.allocation {
            if u128::from(terms.price_limits.max_output_slippage_bps) > BASIS_POINTS {
                return Err(Quote(Moe2QuoteRefusal::OutsideQuote));
            }
            let historical = resolve(terms.quote_anchor, terms.model).ok_or(UnknownClaimContext)?;
            let Moe2ModelAllocation::Additional(market) = historical.allocation else { return Err(UnknownClaimContext) };
            if (historical.model, historical.artifact_root, historical.registration_daa, historical.owner)
                != (model.model, model.artifact_root, model.registration_daa, model.owner)
                || market.opened_daa != current.opened_daa
                || market.virtual_sompi != current.virtual_sompi
            {
                return Err(UnknownClaimContext);
            }
            Some(PurchaseCurve::from_market(&market).map_err(Quote)?)
        } else {
            None
        };
        self.admitted.insert(terms.claim, (terms, reference));
        Ok(())
    }

    /// Bind the block owner's authenticated new-profile claim to the one slot
    /// reserved at EXEC close. No cash moves at claim admission.
    pub fn admit_exec_slot(
        &mut self,
        claim: Hash64,
        slot: &Moe2ExecSlot,
        budget: &PalwBondBudgetStateV1,
        resolve: impl FnOnce(Hash64, Hash64) -> Option<Moe2RegisteredModel>,
    ) -> Result<(), Moe2SettlementError> {
        use Moe2SettlementError::*;
        let binding = slot.binding();
        if self.assigned_slots.contains(&binding.0) {
            return Err(AlreadyAdmitted);
        }
        let row = budget.claim_row(&slot.reservation_key()).ok_or(ReservationMismatch)?;
        if !(slot.quote.valid_from..=slot.quote.deadline).contains(&slot.close_daa)
            || budget.header.policy_digest != slot.quote.policy
            || row.accepted_daa != slot.close_daa
            || (row.origin != PalwBudgetOriginV1::Attempt && row.origin != slot.budget_origin()?)
            || !row.live
            || !row.open
            || row.model.map(|(model, _)| model) != Some(slot.quote.model)
            || row.bond != slot.quote.funding_bond
            || row.reserved.reward_sompi != slot.quote.limits.max_reward
            || row.reserved.claims != 1
            || row.consumed != crate::palw_bond_budget_v1::PalwBudgetVectorV1::ZERO
        {
            return Err(ReservationMismatch);
        }
        self.admit(slot.quote.for_claim(claim), resolve)?;
        self.assigned_slots.insert(binding.0);
        self.exec_claims.insert(claim, binding);
        Ok(())
    }

    pub(crate) fn has_exec_binding(&self, claim: Hash64, slot: &Moe2ExecSlot) -> bool {
        self.exec_claims.get(&claim) == Some(&slot.binding())
    }

    /// Disconnect the admission only after disconnecting its settlement.
    pub fn revert_exec_admission(&mut self, claim: Hash64, slot: &Moe2ExecSlot) -> Result<(), Moe2SettlementError> {
        let binding = slot.binding();
        if self.exec_claims.get(&claim) != Some(&binding) || !self.admitted.contains_key(&claim) {
            return Err(Moe2SettlementError::UndoMismatch);
        }
        self.admitted.remove(&claim);
        self.exec_claims.remove(&claim);
        self.assigned_slots.remove(&binding.0);
        Ok(())
    }

    /// Install a canonical accounting transition. Funding, the maturity decision and
    /// authentication of the admission terms and quote anchor precede this kernel.
    pub fn settle(
        &mut self,
        claim: Hash64,
        position: Moe2Position,
        anchor: Hash64,
        funding: &Moe2SettlementFunding,
        budget: &mut PalwBondBudgetStateV1,
        journal: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<Moe2SettledReward, Moe2SettlementError> {
        self.settle_recorded(claim, position, anchor, funding, budget, journal).map(|(reward, _)| reward)
    }

    /// Return the block-owned rollback record together with the funded result.
    pub fn settle_recorded(
        &mut self,
        claim: Hash64,
        position: Moe2Position,
        anchor: Hash64,
        funding: &Moe2SettlementFunding,
        budget: &mut PalwBondBudgetStateV1,
        journal: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        self.settle_from_reservation(claim, position, anchor, funding, budget, journal, None)
    }

    /// Use the original signed slot without creating a second reservation.
    /// The ordinary claim-keyed route retains its original acceptance clock.
    pub(crate) fn settle_from_reservation(
        &mut self,
        claim: Hash64,
        position: Moe2Position,
        anchor: Hash64,
        funding: &Moe2SettlementFunding,
        budget: &mut PalwBondBudgetStateV1,
        journal: &mut Vec<PalwBudgetWriteV1>,
        slot: Option<&Moe2ExecSlot>,
    ) -> Result<(Moe2SettledReward, Moe2SettlementUndo), Moe2SettlementError> {
        use Moe2SettlementError::*;
        if self.settled.contains_key(&claim) {
            return Err(AlreadySettled);
        }
        let (terms, reference) = *self.admitted.get(&claim).ok_or(UnknownClaimContext)?;
        if self.exec_claims.get(&claim).copied() != slot.map(Moe2ExecSlot::binding) {
            return Err(ReservationMismatch);
        }
        if funding.claim != claim || funding.position != position || funding.liability.now != position.point.daa_score {
            return Err(UnknownClaimContext);
        }
        if self.last_settlement.is_some_and(|last| !position.follows(last)) {
            return Err(PositionOutOfOrder);
        }
        if !(terms.valid_from..=terms.deadline).contains(&position.point.daa_score) {
            return Err(OutsideWindow);
        }
        let model = self.models.get_mut(&terms.model).ok_or(UnknownRegistration)?;
        if model.artifact_root != terms.artifact_root || anchor != terms.quote_anchor || terms.valid_from < model.registration_daa {
            return Err(UnknownClaimContext);
        }
        let allocation = model.allocation.quote_reward(funding.liability.payment, terms.plan, terms.limits).map_err(Quote)?;
        let payment = funding.check(allocation)?;
        if let (Moe2ModelAllocation::Additional(before), Some(q)) = (model.allocation, allocation.market) {
            terms.price_limits.check(reference.ok_or(UnknownClaimContext)?, &before, &q).map_err(Quote)?;
        }

        // Only the original model-bound mining reservation can fund this move.
        // All fallible quote and liability checks precede the single journaled debit.
        let key = slot.map_or_else(|| terms.reservation_key(), Moe2ExecSlot::reservation_key);
        let row = budget.claim_row(&key).ok_or(ReservationMismatch)?;
        let liability = &funding.liability;
        if terms.policy != budget.header.policy_digest
            || row.bond != terms.funding_bond
            || row.model.map(|(id, _)| id) != Some(terms.model)
            || !row.open
            || !row.live
            || row.accepted_daa < model.registration_daa
            || match slot {
                None => row.origin != PalwBudgetOriginV1::Attempt || row.accepted_daa > terms.valid_from,
                Some(slot) => {
                    slot.quote.for_claim(claim) != terms
                        || slot.close_daa != row.accepted_daa
                        || !(terms.valid_from..=position.point.daa_score).contains(&slot.close_daa)
                        || row.reserved.reward_sompi != terms.limits.max_reward
                        || (row.origin != PalwBudgetOriginV1::Attempt
                            && (row.origin != slot.budget_origin()? || liability.signed_hard_deadline != Ok(terms.deadline)))
                }
            }
            || liability.reserved_reward != Ok(row.reserved.reward_sompi)
            || liability.consumed_reward != Ok(row.consumed.reward_sompi)
            || liability.liability_sompi != Ok(row.liability_sompi)
        {
            return Err(ReservationMismatch);
        }
        let undo = Moe2SettlementUndo {
            terms,
            reference,
            before_model: *model,
            before_budget: *row,
            draw: liability.draw,
            position,
            previous_position: self.last_settlement,
            had_model_lp: self.permanent_lp_models.contains(&terms.model),
            had_root_lp: self.permanent_lp_roots.contains(&terms.artifact_root),
        };
        if slot.is_some() {
            budget.consume_exec_slot_reward(&key, liability.draw, journal).map_err(|_| ReservationMismatch)?;
        } else {
            budget
                .consume(&key, PalwBudgetDimV1::Reward, liability.draw.into(), true, journal)
                .ok_or(ReservationMismatch)?
                .map_err(|_| ReservationMismatch)?;
        }
        if let (Moe2ModelAllocation::Additional(before), Some(q)) = (model.allocation, allocation.market) {
            if q.split.liquidity > 0 {
                let receipt = LockedLiquidityReceipt {
                    claim: terms.claim,
                    policy: terms.policy,
                    model: terms.model,
                    artifact_root: terms.artifact_root,
                    position,
                    before,
                    after: q.after,
                    purchase: q.split.purchase,
                    liquidity: q.split.liquidity,
                    purchased_units: q.purchased_units,
                    liquidity_units: q.liquidity_units,
                    retired_units: q.retired_units,
                };
                self.permanent_lp_models.insert(receipt.model);
                self.permanent_lp_roots.insert(receipt.artifact_root);
                self.receipts.insert(terms.claim, receipt);
            }
            model.allocation = Moe2ModelAllocation::Additional(q.after);
        }
        self.admitted.remove(&claim);
        self.settled.insert(claim, (payment, slot.map_or_else(|| undo.digest(), |slot| slot.undo_digest(&undo))));
        self.last_settlement = Some(position);
        Ok((payment, undo))
    }

    /// Disconnect the last settlement in reverse canonical order. A sibling's
    /// journal or a later trade/debit refuses before changing either ledger.
    /// Reconnection uses the original admission and the ordinary settle path.
    pub fn revert_settlement(
        &mut self,
        undo: &Moe2SettlementUndo,
        budget: &mut PalwBondBudgetStateV1,
        journal: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), Moe2SettlementError> {
        self.revert_from_reservation(undo, budget, journal, None)
    }

    pub fn revert_exec_settlement(
        &mut self,
        slot: &Moe2ExecSlot,
        undo: &Moe2SettlementUndo,
        budget: &mut PalwBondBudgetStateV1,
        journal: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), Moe2SettlementError> {
        self.revert_from_reservation(undo, budget, journal, Some(slot))
    }

    fn revert_from_reservation(
        &mut self,
        undo: &Moe2SettlementUndo,
        budget: &mut PalwBondBudgetStateV1,
        journal: &mut Vec<PalwBudgetWriteV1>,
        slot: Option<&Moe2ExecSlot>,
    ) -> Result<(), Moe2SettlementError> {
        use Moe2SettlementError::*;
        let terms = undo.terms;
        let (reward, digest) = self.settled.get(&terms.claim).ok_or(UndoMismatch)?;
        if self.last_settlement != Some(undo.position) || *digest != slot.map_or_else(|| undo.digest(), |slot| slot.undo_digest(undo))
        {
            return Err(UndoMismatch);
        }
        let mut after_model = undo.before_model;
        if let Some(q) = reward.allocation.market {
            after_model.allocation = Moe2ModelAllocation::Additional(q.after);
        }
        if self.model(&terms.model) != Some(&after_model) {
            return Err(UndoMismatch);
        }
        let key = slot.map_or_else(|| terms.reservation_key(), Moe2ExecSlot::reservation_key);
        let after_row = undo.after_budget(slot.is_some())?;
        if budget.claim_row(&key) != Some(&after_row) {
            return Err(ReservationMismatch);
        }
        if undo.draw != 0 || slot.is_some() {
            let PalwBudgetWriteV1::Row { table, key, old, new } = undo.debit(key, &after_row) else { unreachable!() };
            budget.apply_row(table, &key, &old, &new, true).map_err(|_| ReservationMismatch)?;
            journal.push(PalwBudgetWriteV1::Row { table, key, old: new, new: old });
        }
        self.models.insert(terms.model, undo.before_model);
        if self.receipts.remove(&terms.claim).is_some() {
            if !undo.had_model_lp {
                self.permanent_lp_models.remove(&terms.model);
            }
            if !undo.had_root_lp {
                self.permanent_lp_roots.remove(&terms.artifact_root);
            }
        }
        self.settled.remove(&terms.claim);
        self.admitted.insert(terms.claim, (terms, undo.reference));
        self.last_settlement = undo.previous_position;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{LIMITS, market};
    use super::*;
    use crate::palw_bond_budget_v1::{PalwBondBudgetPolicyV1, PalwBudgetVectorV1};
    fn h(id: u64) -> Hash64 {
        Hash64::from_u64_word(id)
    }

    fn funding(claim: Hash64, position: Moe2Position, reward: u64) -> Moe2SettlementFunding {
        let split = super::super::Moe2RewardSplit::additional(reward);
        Moe2SettlementFunding {
            claim,
            position,
            miner_now: split.workers - split.workers / 5,
            panel_now: split.workers / 5,
            liability: PalwSettlementTermsSourcedV1 {
                reserved_reward: Ok(reward),
                consumed_reward: Ok(0),
                draw: reward,
                payment: reward,
                paid_now: reward,
                exported_before: Ok(0),
                export_cap: Ok(reward),
                liability_sompi: Ok(2000),
                bond_collateral: Ok(3000),
                other_live_liability: Ok(1000),
                bond_locked_until: Ok(5),
                liability_until: Ok(5),
                signed_hard_deadline: Ok(8),
                material_ttl_until: Ok(8),
                vest_until: None,
                retire_not_before: Ok(8),
                now: position.point.daa_score,
            },
        }
    }

    fn settle_fixture(
        book: &mut Moe2SettlementBook,
        claim: Hash64,
        position: Moe2Position,
        anchor: Hash64,
        funding: &Moe2SettlementFunding,
    ) -> Result<Moe2SettledReward, Moe2SettlementError> {
        let policy = PalwBondBudgetPolicyV1::unapproved_probe_v1();
        let mut budget = PalwBondBudgetStateV1::new(0, &policy);
        let mut journal = Vec::new();
        if let Some((terms, _)) = book.admitted.get(&claim) {
            let reserved = PalwBudgetVectorV1 { reward_sompi: funding.liability.payment, ..Default::default() };
            budget
                .reserve_liable(
                    &policy,
                    3000,
                    terms.reservation_key(),
                    terms.funding_bond,
                    Some((terms.model, 0)),
                    terms.valid_from,
                    reserved,
                    2000,
                    PalwBudgetOriginV1::Attempt,
                    true,
                    &mut journal,
                )
                .unwrap();
        }
        book.settle(claim, position, anchor, funding, &mut budget, &mut journal)
    }

    #[test]
    fn anchored_slippage_and_current_price_impact_hold_both_legs_and_worker_reward() {
        let model = Moe2RegisteredModel {
            model: h(1),
            artifact_root: h(2),
            registration_daa: 1,
            owner: None,
            allocation: Moe2ModelAllocation::Additional(market()),
        };
        let mut book = Moe2SettlementBook::default();
        book.models.insert(model.model, model);
        let anchor = |id, line| (id == h(5) && line == model.model).then_some(model);
        let terms = Moe2SettlementTerms {
            claim: h(3),
            funding_bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Default::default(), 7)),
            model: model.model,
            artifact_root: model.artifact_root,
            policy: PalwBondBudgetPolicyV1::unapproved_probe_v1().digest(),
            quote_anchor: h(5),
            valid_from: 2,
            deadline: 4,
            limits: LIMITS,
            plan: Moe2WorkerPlan::MinerAndPanel20,
            price_limits: Moe2PriceLimits { max_output_slippage_bps: 0, max_purchase_impact_bps: 3004 },
        };
        assert_eq!(book.admit(terms, |_, _| None), Err(Moe2SettlementError::UnknownClaimContext));
        assert_eq!(
            book.admit(terms, |_, _| Some(Moe2RegisteredModel { artifact_root: h(90), ..model })),
            Err(Moe2SettlementError::UnknownClaimContext)
        );
        let different_curve = PalwModelMarketV1 { virtual_sompi: 1, ..market() };
        assert_eq!(
            book.admit(terms, |_, _| Some(Moe2RegisteredModel {
                allocation: Moe2ModelAllocation::Additional(different_curve),
                ..model
            })),
            Err(Moe2SettlementError::UnknownClaimContext)
        );
        let mut core_book = Moe2SettlementBook::default();
        core_book.models.insert(model.model, Moe2RegisteredModel { allocation: Moe2ModelAllocation::Core, ..model });
        core_book.admit(terms, |_, _| panic!("core has no market quote")).unwrap();
        let outside = Err(Moe2SettlementError::Quote(Moe2QuoteRefusal::OutsideQuote));
        let limits = |slippage, impact| Moe2PriceLimits { max_output_slippage_bps: slippage, max_purchase_impact_bps: impact };
        assert_eq!(book.admit(Moe2SettlementTerms { price_limits: limits(10_001, 3004), ..terms }, anchor), outside);
        let at = Moe2Position { point: PalwBlockContextV2 { block: h(20), blue_score: 10, daa_score: 3, subsidy: 1000 }, ordinal: 1 };
        let short = Moe2SettlementTerms { claim: h(10), price_limits: limits(0, 3003), ..terms };
        book.admit(short, anchor).unwrap();
        let settle = |book: &mut Moe2SettlementBook, claim, at| settle_fixture(book, claim, at, h(5), &funding(claim, at, 1000));
        assert_eq!(settle(&mut book, short.claim, at), Err(Moe2SettlementError::Quote(Moe2QuoteRefusal::OutsideQuote)));
        assert_eq!(book.model(&model.model), Some(&model));
        assert!(book.receipt(&short.claim).is_none() && book.entitlement(&short.claim).is_none());
        book.admit(terms, anchor).unwrap();
        let first = settle(&mut book, terms.claim, at).unwrap().allocation.market.unwrap();
        assert_eq!(first.purchased_units, 2307);
        // Both subsequent claims reference the earlier market. Re-reading the
        // current market as a fresh anchor would incorrectly admit the first.
        let next = Moe2Position { ordinal: 2, ..at };
        for (id, bps) in [(11, 3589), (12, 3590)] {
            let claim = h(id);
            book.admit(Moe2SettlementTerms { claim, price_limits: limits(bps, 3004), ..terms }, anchor).unwrap();
            let result = settle(&mut book, claim, next);
            if id == 11 {
                assert_eq!(result, Err(Moe2SettlementError::Quote(Moe2QuoteRefusal::OutsideQuote)));
                assert_eq!(book.model(&model.model).unwrap().allocation, Moe2ModelAllocation::Additional(first.after));
                assert!(book.receipt(&claim).is_none() && book.entitlement(&claim).is_none());
            } else {
                assert_eq!(result.unwrap().allocation.market.unwrap().purchased_units, 1479);
                assert!(book.receipt(&claim).is_some() && book.entitlement(&claim).is_some());
            }
        }
    }

    #[test]
    fn exec_slot_debits_once_and_reconnects_after_the_quote_anchor() {
        let policy = PalwBondBudgetPolicyV1::unapproved_probe_v1();
        let mut budget = PalwBondBudgetStateV1::new(0, &policy);
        let mut book = Moe2SettlementBook::default();
        let model = Moe2RegisteredModel {
            model: h(1),
            artifact_root: h(90),
            registration_daa: 1,
            owner: None,
            allocation: Moe2ModelAllocation::Additional(market()),
        };
        book.models.insert(model.model, model);
        let slot = Moe2ExecSlot {
            registration: [5; 64],
            slot: 9,
            close: h(3),
            close_daa: 3,
            quote: Moe2PreDrawQuote {
                funding_bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Default::default(), 1)),
                model: model.model,
                artifact_root: model.artifact_root,
                policy: policy.digest(),
                quote_anchor: h(2),
                valid_from: 2,
                deadline: 5,
                limits: Moe2QuantityLimits { max_reward: 1000, ..LIMITS },
                plan: Moe2WorkerPlan::MinerAndPanel20,
                price_limits: Moe2PriceLimits { max_output_slippage_bps: 0, max_purchase_impact_bps: 10_000 },
            },
        };
        let at = Moe2Position { point: PalwBlockContextV2 { block: h(4), blue_score: 10, daa_score: 4, subsidy: 1000 }, ordinal: 1 };
        let claim = h(10);
        let other = h(11);
        let mut writes = Vec::new();
        budget
            .reserve_liable(
                &policy,
                3000,
                slot.reservation_key(),
                slot.quote.funding_bond,
                Some((model.model, 0)),
                3,
                PalwBudgetVectorV1 { claims: 1, reward_sompi: 1000, ..Default::default() },
                2000,
                PalwBudgetOriginV1::Attempt,
                true,
                &mut writes,
            )
            .unwrap();
        // The budget row itself enforces one use even for a zero or partial draw.
        for draw in [0, 800] {
            let mut replay = budget.clone();
            let mut journal = Vec::new();
            replay.consume_exec_slot_reward(&slot.reservation_key(), draw, &mut journal).unwrap();
            assert_eq!(journal.len(), 1);
            let after = replay.clone();
            assert!(replay.consume_exec_slot_reward(&slot.reservation_key(), 1, &mut journal).is_err());
            assert_eq!(replay, after);
            assert_eq!(journal.len(), 1);
        }
        let before_admission = book.clone();
        book.admit_exec_slot(claim, &slot, &budget, |_, _| Some(model)).unwrap();
        assert_eq!(book.admit_exec_slot(other, &slot, &budget, |_, _| Some(model)), Err(Moe2SettlementError::AlreadyAdmitted));
        book.revert_exec_admission(claim, &slot).unwrap();
        assert_eq!(book, before_admission);
        book.admit_exec_slot(claim, &slot, &budget, |_, _| Some(model)).unwrap();
        book.admit(slot.quote.for_claim(other), |_, _| Some(model)).unwrap();
        let parent = (book.clone(), budget.clone());
        let pay = |id, pos| {
            let mut f = funding(id, pos, 800);
            f.liability.reserved_reward = Ok(1000);
            f
        };
        let other_slot = Moe2ExecSlot { slot: 10, ..slot };
        for source in [None, Some(&other_slot), Some(&Moe2ExecSlot { close_daa: 2, ..slot })] {
            let count = writes.len();
            assert_eq!(
                book.settle_from_reservation(claim, at, h(2), &pay(claim, at), &mut budget, &mut writes, source),
                Err(Moe2SettlementError::ReservationMismatch)
            );
            assert_eq!((&book, &budget, writes.len()), (&parent.0, &parent.1, count));
        }
        let (result, undo) =
            book.settle_from_reservation(claim, at, h(2), &pay(claim, at), &mut budget, &mut writes, Some(&slot)).unwrap();
        let receipt = book.receipt(&claim).unwrap();
        assert_eq!((receipt.purchase, receipt.liquidity, result.miner_now + result.panel_now), (240, 160, 400));
        let consumed = budget.claim_row(&slot.reservation_key()).unwrap().consumed;
        assert_eq!((consumed.claims, consumed.reward_sompi), (1, 800));
        let checkpoint = book.checkpoint(h(99));
        book = Moe2SettlementBook::from_checkpoint(h(99), checkpoint.0, &checkpoint.1, checkpoint.1.len()).unwrap();
        let child = (book.clone(), budget.clone());
        let later = Moe2Position { ordinal: 2, ..at };
        let count = writes.len();
        assert!(book.settle_from_reservation(other, later, h(2), &pay(other, later), &mut budget, &mut writes, Some(&slot)).is_err());
        assert!(book.revert_settlement(&undo, &mut budget, &mut writes).is_err());
        assert!(book.revert_exec_settlement(&Moe2ExecSlot { close: h(42), ..slot }, &undo, &mut budget, &mut writes).is_err());
        assert_eq!((&book, &budget, writes.len()), (&child.0, &child.1, count));
        book.revert_exec_settlement(&slot, &undo, &mut budget, &mut writes).unwrap();
        assert_eq!((&book, &budget), (&parent.0, &parent.1));
        book.settle_from_reservation(claim, at, h(2), &pay(claim, at), &mut budget, &mut writes, Some(&slot)).unwrap();
        assert_eq!((book, budget), child);
    }

    #[test]
    fn recorded_settlements_unwind_in_order_and_preserve_prior_alias_liquidity() {
        let policy = PalwBondBudgetPolicyV1::unapproved_probe_v1();
        let mut budget = PalwBondBudgetStateV1::new(0, &policy);
        let mut book = Moe2SettlementBook::default();
        let mut journal = Vec::new();
        let at = Moe2Position { point: PalwBlockContextV2 { block: h(20), blue_score: 10, daa_score: 3, subsidy: 1000 }, ordinal: 1 };
        let mut history = Vec::new();
        // Two aliases share a root. A third settlement reuses the first model.
        for (i, model_id) in [1, 2, 1].into_iter().enumerate() {
            let model = *book.models.entry(h(model_id)).or_insert(Moe2RegisteredModel {
                model: h(model_id),
                artifact_root: h(90),
                registration_daa: 1,
                owner: None,
                allocation: Moe2ModelAllocation::Additional(market()),
            });
            let terms = Moe2SettlementTerms {
                claim: h(100 + i as u64),
                model: model.model,
                artifact_root: model.artifact_root,
                funding_bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Default::default(), i as u32)),
                policy: policy.digest(),
                quote_anchor: h(70 + i as u64),
                valid_from: 2,
                deadline: 5,
                limits: LIMITS,
                plan: Moe2WorkerPlan::MinerAndPanel20,
                price_limits: Moe2PriceLimits { max_output_slippage_bps: 0, max_purchase_impact_bps: 10_000 },
            };
            book.admit(terms, |_, _| Some(model)).unwrap();
            budget
                .reserve_liable(
                    &policy,
                    3000,
                    terms.reservation_key(),
                    terms.funding_bond,
                    Some((model.model, 0)),
                    terms.valid_from,
                    PalwBudgetVectorV1 { reward_sompi: 1000, ..Default::default() },
                    2000,
                    PalwBudgetOriginV1::Attempt,
                    true,
                    &mut journal,
                )
                .unwrap();
            let position = Moe2Position { ordinal: i as u32 + 1, ..at };
            let (_, undo) = book
                .settle_recorded(
                    terms.claim,
                    position,
                    terms.quote_anchor,
                    &funding(terms.claim, position, 1000),
                    &mut budget,
                    &mut journal,
                )
                .unwrap();
            history.push(undo);
        }
        let child = (book.clone(), budget.clone());
        let j = journal.len();
        let mut changed = history[2];
        changed.had_root_lp = false;
        for invalid in [&changed, &history[0]] {
            assert_eq!(book.revert_settlement(invalid, &mut budget, &mut journal), Err(Moe2SettlementError::UndoMismatch));
        }
        assert_eq!((&book, &budget, journal.len()), (&child.0, &child.1, j));
        // A changed budget row must refuse even when the market matches.
        let mut closed = budget.clone();
        closed.close(&history[2].terms.reservation_key(), &mut Vec::new());
        let before = closed.clone();
        assert_eq!(book.revert_settlement(&history[2], &mut closed, &mut journal), Err(Moe2SettlementError::ReservationMismatch));
        assert_eq!((&book, &closed, journal.len()), (&child.0, &before, j));
        for i in (0..history.len()).rev() {
            let undo = history[i];
            book.revert_settlement(&undo, &mut budget, &mut journal).unwrap();
            assert_eq!(budget.claim_row(&undo.terms.reservation_key()).unwrap().consumed.reward_sompi, 0);
            assert!(book.entitlement(&undo.terms.claim).is_none() && book.receipt(&undo.terms.claim).is_none());
            assert_eq!(book.permanent_lp_roots.contains(&h(90)), i > 0);
            assert_eq!(book.permanent_lp_models.contains(&h(1)), i > 0);
            assert_eq!(book.permanent_lp_models.contains(&h(2)), i > 1);
        }
        for undo in history {
            let t = undo.terms;
            let (_, again) = book
                .settle_recorded(
                    t.claim,
                    undo.position,
                    t.quote_anchor,
                    &funding(t.claim, undo.position, 1000),
                    &mut budget,
                    &mut journal,
                )
                .unwrap();
            assert_eq!(again, undo);
        }
        assert_eq!((book, budget), child);
    }

    #[test]
    fn settlement_is_atomic_across_dust_order_retries_and_alternate_forks() {
        let model = Moe2RegisteredModel {
            model: h(1),
            artifact_root: h(2),
            registration_daa: 1,
            owner: None,
            allocation: Moe2ModelAllocation::Additional(market()),
        };
        let terms = Moe2SettlementTerms {
            claim: h(3),
            funding_bond: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Default::default(), 7)),
            model: model.model,
            artifact_root: model.artifact_root,
            policy: PalwBondBudgetPolicyV1::unapproved_probe_v1().digest(),
            quote_anchor: h(5),
            valid_from: 2,
            deadline: 4,
            limits: LIMITS,
            plan: Moe2WorkerPlan::MinerAndPanel20,
            price_limits: Moe2PriceLimits { max_output_slippage_bps: 0, max_purchase_impact_bps: 10_000 },
        };
        let mut parent = Moe2SettlementBook::default();
        parent.models.insert(model.model, model);
        let anchor = |id, line| (id == h(5) && line == model.model).then_some(model);
        let before = parent.clone();
        assert_eq!(
            parent.admit(Moe2SettlementTerms { deadline: u64::MAX, ..terms }, anchor),
            Err(Moe2SettlementError::UnknownClaimContext)
        );
        assert_eq!(parent, before);
        parent.admit(terms, anchor).unwrap();
        let mut child = parent.clone();
        let block = PalwBlockContextV2 { block: h(20), blue_score: 10, daa_score: 3, subsidy: 1000 };
        let at = Moe2Position { point: block, ordinal: 1 };
        let funds = funding(terms.claim, at, 1000);
        assert_eq!(settle_fixture(&mut child, terms.claim, at, h(6), &funds), Err(Moe2SettlementError::UnknownClaimContext));
        let high_min = Moe2SettlementTerms { claim: h(11), limits: Moe2QuantityLimits { min_out: 2308, ..LIMITS }, ..terms };
        child.admit(high_min, anchor).unwrap();
        assert_eq!(
            child.admit(Moe2SettlementTerms { claim: high_min.claim, ..terms }, anchor),
            Err(Moe2SettlementError::AlreadyAdmitted)
        );
        assert_eq!(
            settle_fixture(&mut child, high_min.claim, at, h(5), &funding(high_min.claim, at, 1000)),
            Err(Moe2SettlementError::Quote(Moe2QuoteRefusal::OutsideQuote))
        );
        assert_eq!(child.model(&model.model), Some(&model));
        assert!(child.entitlement(&h(3)).is_none());
        let mut held = funds;
        held.miner_now = 0;
        held.panel_now = 0;
        held.liability.vest_until = Some(6);
        held.liability.exported_before = Ok(100);
        held.liability.export_cap = Ok(599);
        // The buy and permanent LP together export 500 even with both workers vested.
        held.liability.paid_now = 300;
        assert_eq!(
            settle_fixture(&mut child, terms.claim, at, h(5), &held),
            Err(Moe2SettlementError::Liability(PalwSettlementLiabilityRefusalV1::Malformed))
        );
        held.liability.paid_now = 500;
        assert_eq!(
            settle_fixture(&mut child, terms.claim, at, h(5), &held),
            Err(Moe2SettlementError::Liability(PalwSettlementLiabilityRefusalV1::ExportBeyondCap))
        );
        assert!(child.receipt(&terms.claim).is_none() && child.entitlement(&terms.claim).is_none());
        assert_eq!(child.model(&model.model), Some(&model));
        held.liability.export_cap = Ok(600);
        let mut vested_branch = child.clone();
        let vested = settle_fixture(&mut vested_branch, terms.claim, at, h(5), &held).unwrap();
        assert_eq!((vested.miner_now, vested.panel_now, vested.vest_until), (0, 0, Some(6)));
        assert_eq!(vested_branch.entitlement(&terms.claim), Some(&vested));
        let pay = settle_fixture(&mut child, terms.claim, at, h(5), &funds).unwrap();
        assert_eq!((pay.allocation.miner, pay.allocation.panel_pool), (400, 100));
        assert_eq!(child.receipt(&h(3)).unwrap().liquidity_units, 1183);
        assert_eq!(child.check_core_designation(h(7), model.artifact_root), Err(Moe2SettlementError::PermanentLiquidityHistory));
        assert_eq!(child.check_core_designation(model.model, h(8)), Err(Moe2SettlementError::PermanentLiquidityHistory));
        assert!(parent.check_core_designation(h(7), model.artifact_root).is_ok());
        assert_eq!(settle_fixture(&mut child, terms.claim, at, h(5), &funds), Err(Moe2SettlementError::AlreadySettled));
        let second = Moe2SettlementTerms { claim: h(9), quote_anchor: h(6), ..terms };
        let later = *child.model(&model.model).unwrap();
        child.admit(second, |id, line| (id == h(6) && line == model.model).then_some(later)).unwrap();
        assert_eq!(
            settle_fixture(&mut child, second.claim, at, h(6), &funding(second.claim, at, 1000)),
            Err(Moe2SettlementError::PositionOutOfOrder)
        );
        let late = Moe2Position { point: PalwBlockContextV2 { block: h(21), blue_score: 11, daa_score: 5, ..block }, ordinal: 0 };
        assert_eq!(
            settle_fixture(&mut child, second.claim, late, h(6), &funding(second.claim, late, 1000)),
            Err(Moe2SettlementError::OutsideWindow)
        );
        let successor = Moe2Position { point: PalwBlockContextV2 { block: h(22), blue_score: 11, ..block }, ordinal: 0 };
        let sibling = Moe2Position { point: PalwBlockContextV2 { block: h(23), ..block }, ordinal: 2 };
        assert_eq!(
            settle_fixture(&mut child, second.claim, sibling, h(6), &funding(second.claim, sibling, 1000)),
            Err(Moe2SettlementError::PositionOutOfOrder)
        );
        assert_eq!(
            settle_fixture(&mut child, second.claim, successor, h(6), &funding(second.claim, sibling, 1000)),
            Err(Moe2SettlementError::UnknownClaimContext)
        );
        settle_fixture(&mut child, second.claim, successor, h(6), &funding(second.claim, successor, 1000)).unwrap();
        assert_eq!(child.receipt(&second.claim).unwrap().position, successor);
        assert!(parent.entitlement(&h(3)).is_none());
        let dust = settle_fixture(&mut parent, terms.claim, at, h(5), &funding(terms.claim, at, 3)).unwrap();
        assert_eq!((dust.allocation.miner, dust.allocation.panel_pool), (3, 0));
        assert!(parent.receipt(&h(3)).is_none() && !parent.has_permanent_lp(model.model, model.artifact_root));
    }
}
