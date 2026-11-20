//! Local execution of an already funded and authorized MOE2 settlement.
//! This module has no network object decoder or public activation. A caller
//! operating a versioned native store owns the transaction and fork boundary.
use super::{Moe2ModelAllocation, Moe2QuantityLimits, Moe2QuoteRefusal, Moe2RegisteredModel, Moe2RewardAllocation, Moe2WorkerPlan};
use crate::Hash64;
use crate::palw_model_market_v1::PalwModelMarketV1;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Moe2Position {
    pub daa: u64,
    pub ordinal: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2SettlementTerms {
    pub claim: Hash64,
    pub model: Hash64,
    pub artifact_root: Hash64,
    pub policy: Hash64,
    pub quote_anchor: Hash64,
    pub valid_from: u64,
    pub deadline: u64,
    pub limits: Moe2QuantityLimits,
    pub plan: Moe2WorkerPlan,
}

/// Accounting history. The recorded quantities have already entered the market.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub enum Moe2SettlementError {
    UnknownRegistration,
    DuplicateRegistration,
    UnknownClaimContext,
    AlreadyAdmitted,
    AlreadySettled,
    PositionOutOfOrder,
    OutsideWindow,
    Quote(Moe2QuoteRefusal),
}

#[derive(Clone, Debug, Default)]
pub struct Moe2SettlementBook {
    models: BTreeMap<Hash64, Moe2RegisteredModel>,
    admitted: BTreeMap<Hash64, Moe2SettlementTerms>,
    settled: BTreeMap<Hash64, Moe2RewardAllocation>,
    receipts: BTreeMap<Hash64, LockedLiquidityReceipt>,
    last_settlement: Option<Moe2Position>,
}

impl Moe2SettlementBook {
    /// In the registration transition, resolve the canonical model and install
    /// the resulting registration with its market (or core allocation) in one row.
    pub fn register(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        catalog: &crate::palw_model_tier_v1::PalwModelTierMirrorV1,
        model: Hash64,
        daa: u64,
    ) -> Result<(), Moe2SettlementError> {
        if self.models.contains_key(&model) {
            return Err(Moe2SettlementError::DuplicateRegistration);
        }
        let row = Moe2RegisteredModel::at_registration(state, catalog, model, daa).ok_or(Moe2SettlementError::UnknownRegistration)?;
        self.models.insert(model, row);
        Ok(())
    }

    pub fn model(&self, model: &Hash64) -> Option<&Moe2RegisteredModel> {
        self.models.get(model)
    }
    pub fn entitlement(&self, claim: &Hash64) -> Option<&Moe2RewardAllocation> {
        self.settled.get(claim)
    }
    pub fn receipt(&self, claim: &Hash64) -> Option<&LockedLiquidityReceipt> {
        self.receipts.get(claim)
    }

    /// A model ID and all aliases of a root share the permanent-LP exclusion.
    pub fn has_permanent_lp(&self, model: Hash64, artifact_root: Hash64) -> bool {
        self.receipts.values().any(|receipt| receipt.model == model || receipt.artifact_root == artifact_root)
    }

    /// Bind terms before the draw. Neither a failed quote nor a retry can
    /// replace the policy, change the owner market, or extend the deadline.
    pub fn admit(&mut self, terms: Moe2SettlementTerms) -> Result<(), Moe2SettlementError> {
        use Moe2SettlementError::*;
        let model = self.models.get(&terms.model).ok_or(UnknownRegistration)?;
        if terms.artifact_root != model.artifact_root || terms.valid_from < model.registration_daa || terms.deadline < terms.valid_from
        {
            return Err(UnknownClaimContext);
        }
        if self.admitted.contains_key(&terms.claim) || self.settled.contains_key(&terms.claim) {
            return Err(AlreadyAdmitted);
        }
        self.admitted.insert(terms.claim, terms);
        Ok(())
    }

    /// Install a canonical accounting transition. Funding, the maturity decision and
    /// authentication of the admission terms and quote anchor precede this kernel.
    pub fn settle(
        &mut self,
        claim: Hash64,
        position: Moe2Position,
        anchor: Hash64,
        reward: u64,
    ) -> Result<Moe2RewardAllocation, Moe2SettlementError> {
        use Moe2SettlementError::*;
        if self.settled.contains_key(&claim) {
            return Err(AlreadySettled);
        }
        let terms = *self.admitted.get(&claim).ok_or(UnknownClaimContext)?;
        if self.last_settlement.is_some_and(|last| position <= last) {
            return Err(PositionOutOfOrder);
        }
        if !(terms.valid_from..=terms.deadline).contains(&position.daa) {
            return Err(OutsideWindow);
        }
        let model = self.models.get_mut(&terms.model).ok_or(UnknownRegistration)?;
        if model.artifact_root != terms.artifact_root || anchor != terms.quote_anchor || terms.valid_from < model.registration_daa {
            return Err(UnknownClaimContext);
        }
        let allocation = model.allocation.quote_reward(reward, terms.plan, terms.limits).map_err(Quote)?;
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
                self.receipts.insert(terms.claim, receipt);
            }
            model.allocation = Moe2ModelAllocation::Additional(q.after);
        }
        self.admitted.remove(&claim);
        self.settled.insert(claim, allocation);
        self.last_settlement = Some(position);
        Ok(allocation)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{LIMITS, market};
    use super::*;
    fn h(id: u64) -> Hash64 {
        Hash64::from_u64_word(id)
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
            model: model.model,
            artifact_root: model.artifact_root,
            policy: h(4),
            quote_anchor: h(5),
            valid_from: 2,
            deadline: 4,
            limits: LIMITS,
            plan: Moe2WorkerPlan::MinerAndPanel20,
        };
        let mut parent = Moe2SettlementBook::default();
        parent.models.insert(model.model, model);
        parent.admit(terms).unwrap();
        let mut child = parent.clone();
        let at = Moe2Position { daa: 3, ordinal: 1 };
        assert_eq!(child.settle(terms.claim, at, h(6), 1000), Err(Moe2SettlementError::UnknownClaimContext));
        let high_min = Moe2SettlementTerms { claim: h(11), limits: Moe2QuantityLimits { min_out: 2308, ..LIMITS }, ..terms };
        child.admit(high_min).unwrap();
        assert_eq!(child.admit(Moe2SettlementTerms { claim: high_min.claim, ..terms }), Err(Moe2SettlementError::AlreadyAdmitted));
        assert_eq!(child.settle(high_min.claim, at, h(5), 1000), Err(Moe2SettlementError::Quote(Moe2QuoteRefusal::OutsideQuote)));
        assert_eq!(child.model(&model.model), Some(&model));
        assert!(child.entitlement(&h(3)).is_none());
        let pay = child.settle(terms.claim, at, h(5), 1000).unwrap();
        assert_eq!((pay.miner, pay.panel_pool), (400, 100));
        assert_eq!(child.receipt(&h(3)).unwrap().liquidity_units, 1183);
        assert!(child.has_permanent_lp(h(7), model.artifact_root));
        assert!(child.has_permanent_lp(model.model, h(8)));
        assert_eq!(child.settle(terms.claim, at, h(5), 1000), Err(Moe2SettlementError::AlreadySettled));
        let second = Moe2SettlementTerms { claim: h(9), ..terms };
        child.admit(second).unwrap();
        assert_eq!(child.settle(second.claim, at, h(5), 1000), Err(Moe2SettlementError::PositionOutOfOrder));
        assert_eq!(
            child.settle(second.claim, Moe2Position { daa: 5, ordinal: 0 }, h(5), 1000),
            Err(Moe2SettlementError::OutsideWindow)
        );
        assert!(parent.entitlement(&h(3)).is_none());
        let dust = parent.settle(terms.claim, at, h(5), 3).unwrap();
        assert_eq!((dust.miner, dust.panel_pool), (3, 0));
        assert!(parent.receipt(&h(3)).is_none() && !parent.has_permanent_lp(model.model, model.artifact_root));
    }
}
