//! Local execution of an already funded and authorized MOE2 settlement.
//! This module has no network object decoder or public activation. A caller
//! operating a versioned native store owns the transaction and fork boundary.
use super::{
    BASIS_POINTS, Moe2ModelAllocation, Moe2PriceLimits, Moe2QuantityLimits, Moe2QuoteRefusal, Moe2RegisteredModel,
    Moe2RewardAllocation, Moe2WorkerPlan, PurchaseCurve,
};
use crate::Hash64;
use crate::palw_model_market_v1::PalwModelMarketV1;
use crate::palw_settlement_liability_v1::{
    PalwSettlementLiabilityRefusalV1, PalwSettlementTermsSourcedV1, palw_settlement_liability_sourced_v1,
};
use crate::palw_state_v2::PalwBlockContextV2;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub price_limits: Moe2PriceLimits,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2SettledReward {
    pub allocation: Moe2RewardAllocation,
    pub miner_now: u64,
    pub panel_now: u64,
    pub vest_until: Option<u64>,
}

impl Moe2SettlementFunding {
    fn check(&self, allocation: Moe2RewardAllocation) -> Result<Moe2SettledReward, Moe2SettlementError> {
        let t = &self.liability;
        let market_export = allocation.market.map_or(0, |q| q.split.purchase + q.split.liquidity);
        if self.miner_now > allocation.miner
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
    PositionOutOfOrder,
    OutsideWindow,
    Quote(Moe2QuoteRefusal),
    Liability(PalwSettlementLiabilityRefusalV1),
}

#[derive(Clone, Debug, Default)]
pub struct Moe2SettlementBook {
    models: BTreeMap<Hash64, Moe2RegisteredModel>,
    admitted: BTreeMap<Hash64, (Moe2SettlementTerms, Option<PurchaseCurve>)>,
    settled: BTreeMap<Hash64, Moe2SettledReward>,
    receipts: BTreeMap<Hash64, LockedLiquidityReceipt>,
    permanent_lp_models: BTreeSet<Hash64>,
    permanent_lp_roots: BTreeSet<Hash64>,
    last_settlement: Option<Moe2Position>,
}

impl Moe2SettlementBook {
    /// Apply new registrations and native repricing of existing pairs in one batch. The
    /// owning fold commits this projection together with that block's class
    /// delta. A second registration in a block cannot leave the first half-installed.
    pub fn register_block(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        catalog: &crate::palw_model_tier_v1::PalwModelTierMirrorV1,
        delta: &crate::palw_state_v2::PalwStateDeltaV2,
    ) -> Result<(), Moe2SettlementError> {
        use crate::palw_state_v2::PalwDeltaEntryV2;
        use Moe2SettlementError::*;
        if state.last_point() != Some(&delta.point) {
            return Err(UnknownClaimContext);
        }
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
        self.models.append(&mut registered);
        Ok(())
    }

    pub fn model(&self, model: &Hash64) -> Option<&Moe2RegisteredModel> {
        self.models.get(model)
    }
    pub fn entitlement(&self, claim: &Hash64) -> Option<&Moe2SettledReward> {
        self.settled.get(claim)
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
        if terms.artifact_root != model.artifact_root || terms.valid_from < model.registration_daa || terms.deadline < terms.valid_from
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

    /// Install a canonical accounting transition. Funding, the maturity decision and
    /// authentication of the admission terms and quote anchor precede this kernel.
    pub fn settle(
        &mut self,
        claim: Hash64,
        position: Moe2Position,
        anchor: Hash64,
        funding: &Moe2SettlementFunding,
    ) -> Result<Moe2SettledReward, Moe2SettlementError> {
        use Moe2SettlementError::*;
        if self.settled.contains_key(&claim) {
            return Err(AlreadySettled);
        }
        let (terms, reference) = *self.admitted.get(&claim).ok_or(UnknownClaimContext)?;
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
        self.settled.insert(claim, payment);
        self.last_settlement = Some(position);
        Ok(payment)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{LIMITS, market};
    use super::*;
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
            model: model.model,
            artifact_root: model.artifact_root,
            policy: h(4),
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
        let settle = |book: &mut Moe2SettlementBook, claim, at| book.settle(claim, at, h(5), &funding(claim, at, 1000));
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
            price_limits: Moe2PriceLimits { max_output_slippage_bps: 0, max_purchase_impact_bps: 10_000 },
        };
        let mut parent = Moe2SettlementBook::default();
        parent.models.insert(model.model, model);
        let anchor = |id, line| (id == h(5) && line == model.model).then_some(model);
        parent.admit(terms, anchor).unwrap();
        let mut child = parent.clone();
        let block = PalwBlockContextV2 { block: h(20), blue_score: 10, daa_score: 3, subsidy: 1000 };
        let at = Moe2Position { point: block, ordinal: 1 };
        let funds = funding(terms.claim, at, 1000);
        assert_eq!(child.settle(terms.claim, at, h(6), &funds), Err(Moe2SettlementError::UnknownClaimContext));
        let high_min = Moe2SettlementTerms { claim: h(11), limits: Moe2QuantityLimits { min_out: 2308, ..LIMITS }, ..terms };
        child.admit(high_min, anchor).unwrap();
        assert_eq!(
            child.admit(Moe2SettlementTerms { claim: high_min.claim, ..terms }, anchor),
            Err(Moe2SettlementError::AlreadyAdmitted)
        );
        assert_eq!(
            child.settle(high_min.claim, at, h(5), &funding(high_min.claim, at, 1000)),
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
            child.settle(terms.claim, at, h(5), &held),
            Err(Moe2SettlementError::Liability(PalwSettlementLiabilityRefusalV1::Malformed))
        );
        held.liability.paid_now = 500;
        assert_eq!(
            child.settle(terms.claim, at, h(5), &held),
            Err(Moe2SettlementError::Liability(PalwSettlementLiabilityRefusalV1::ExportBeyondCap))
        );
        assert!(child.receipt(&terms.claim).is_none() && child.entitlement(&terms.claim).is_none());
        assert_eq!(child.model(&model.model), Some(&model));
        held.liability.export_cap = Ok(600);
        let mut vested_branch = child.clone();
        let vested = vested_branch.settle(terms.claim, at, h(5), &held).unwrap();
        assert_eq!((vested.miner_now, vested.panel_now, vested.vest_until), (0, 0, Some(6)));
        assert_eq!(vested_branch.entitlement(&terms.claim), Some(&vested));
        let pay = child.settle(terms.claim, at, h(5), &funds).unwrap();
        assert_eq!((pay.allocation.miner, pay.allocation.panel_pool), (400, 100));
        assert_eq!(child.receipt(&h(3)).unwrap().liquidity_units, 1183);
        assert_eq!(child.check_core_designation(h(7), model.artifact_root), Err(Moe2SettlementError::PermanentLiquidityHistory));
        assert_eq!(child.check_core_designation(model.model, h(8)), Err(Moe2SettlementError::PermanentLiquidityHistory));
        assert!(parent.check_core_designation(h(7), model.artifact_root).is_ok());
        assert_eq!(child.settle(terms.claim, at, h(5), &funds), Err(Moe2SettlementError::AlreadySettled));
        let second = Moe2SettlementTerms { claim: h(9), quote_anchor: h(6), ..terms };
        let later = *child.model(&model.model).unwrap();
        child.admit(second, |id, line| (id == h(6) && line == model.model).then_some(later)).unwrap();
        assert_eq!(
            child.settle(second.claim, at, h(6), &funding(second.claim, at, 1000)),
            Err(Moe2SettlementError::PositionOutOfOrder)
        );
        let late = Moe2Position { point: PalwBlockContextV2 { block: h(21), blue_score: 11, daa_score: 5, ..block }, ordinal: 0 };
        assert_eq!(
            child.settle(second.claim, late, h(6), &funding(second.claim, late, 1000)),
            Err(Moe2SettlementError::OutsideWindow)
        );
        let successor = Moe2Position { point: PalwBlockContextV2 { block: h(22), blue_score: 11, ..block }, ordinal: 0 };
        let sibling = Moe2Position { point: PalwBlockContextV2 { block: h(23), ..block }, ordinal: 2 };
        assert_eq!(
            child.settle(second.claim, sibling, h(6), &funding(second.claim, sibling, 1000)),
            Err(Moe2SettlementError::PositionOutOfOrder)
        );
        assert_eq!(
            child.settle(second.claim, successor, h(6), &funding(second.claim, sibling, 1000)),
            Err(Moe2SettlementError::UnknownClaimContext)
        );
        child.settle(second.claim, successor, h(6), &funding(second.claim, successor, 1000)).unwrap();
        assert_eq!(child.receipt(&second.claim).unwrap().position, successor);
        assert!(parent.entitlement(&h(3)).is_none());
        let dust = parent.settle(terms.claim, at, h(5), &funding(terms.claim, at, 3)).unwrap();
        assert_eq!((dust.allocation.miner, dust.allocation.panel_pool), (3, 0));
        assert!(parent.receipt(&h(3)).is_none() && !parent.has_permanent_lp(model.model, model.artifact_root));
    }
}
