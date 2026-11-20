//! RFC-0064 reward quote for the MOE2 cohort. The caller supplies the market
//! at the canonical settlement position and the limits bound at admission.
pub mod settlement;

use super::{PALW_MODEL_SUPPLY_UNITS_V1, PalwModelMarketV1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2RewardSplit {
    pub purchase: u64,
    pub liquidity: u64,
    pub workers: u64,
}

impl Moe2RewardSplit {
    pub fn additional(reward: u64) -> Self {
        let purchase = (u128::from(reward) * 3 / 10) as u64;
        let liquidity = reward / 5;
        Self { purchase, liquidity, workers: reward - purchase - liquidity }
    }
}

/// Separate limits on the reward, the BILI spent on each market leg and the
/// Position quantities it moves. The minimum output is in purchased Positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2QuantityLimits {
    pub max_reward: u64,
    pub max_purchase_sompi: u64,
    pub min_out: u64,
    pub max_purchase_units: u64,
    pub max_lp_sompi: u64,
    pub max_lp_units: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2MarketQuote {
    pub split: Moe2RewardSplit,
    pub purchased_units: u64,
    pub liquidity_units: u64,
    pub retired_units: u64,
    pub after: PalwModelMarketV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moe2QuoteRefusal {
    InvalidMarket,
    LiquidityDust,
    OutsideQuote,
    Overflow,
}

/// Both market legs are projected from the same state. In particular, a failed
/// LP leg never installs the intermediate buy or releases a worker entitlement.
pub fn quote_additional_reward(
    market: &PalwModelMarketV1,
    reward: u64,
    limits: Moe2QuantityLimits,
) -> Result<Moe2MarketQuote, Moe2QuoteRefusal> {
    use Moe2QuoteRefusal::*;
    let inventory = market.position_units;
    if !market.is_open()
        || market.closed_to_buys
        || market.curve_x() == 0
        || inventory == 0
        || inventory > PALW_MODEL_SUPPLY_UNITS_V1
        || market.retired_units > PALW_MODEL_SUPPLY_UNITS_V1 - inventory
        || market.msk_reserve < market.seed_sompi
    {
        return Err(InvalidMarket);
    }
    let split = Moe2RewardSplit::additional(reward);
    if reward > limits.max_reward || split.purchase > limits.max_purchase_sompi || split.liquidity > limits.max_lp_sompi {
        return Err(OutsideQuote);
    }
    let x1 = market.curve_x() + u128::from(split.purchase);
    let purchased_units = (u128::from(split.purchase) * u128::from(inventory) / x1) as u64;
    let remaining = inventory - purchased_units;
    let liquidity_units = (u128::from(split.liquidity) * u128::from(remaining) / x1) as u64;
    if (split.purchase > 0 && purchased_units == 0) || (split.liquidity > 0 && liquidity_units == 0) {
        return Err(LiquidityDust);
    }
    if purchased_units < limits.min_out || purchased_units > limits.max_purchase_units || liquidity_units > limits.max_lp_units {
        return Err(OutsideQuote);
    }
    let retired_units = purchased_units.checked_sub(liquidity_units).ok_or(InvalidMarket)?;
    let reserve = market.msk_reserve.checked_add(split.purchase).and_then(|r| r.checked_add(split.liquidity)).ok_or(Overflow)?;
    let after = PalwModelMarketV1 {
        msk_reserve: reserve,
        position_units: inventory - retired_units,
        retired_units: market.retired_units.checked_add(retired_units).ok_or(Overflow)?,
        buyback_sompi: market.buyback_sompi.checked_add(split.purchase).ok_or(Overflow)?,
        ..*market
    };
    Ok(Moe2MarketQuote { split, purchased_units, liquidity_units, retired_units, after })
}

/// Economic state created at registration for the isolated MOE2 cohort. The
/// model ID, root and owner come from the same canonical native registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2RegisteredModel {
    pub model: crate::Hash64,
    pub artifact_root: crate::Hash64,
    pub registration_daa: u64,
    pub owner: Option<crate::palw_state_v2::PalwBondKeyV2>,
    pub allocation: Moe2ModelAllocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moe2ModelAllocation {
    Core,
    Additional(PalwModelMarketV1),
}

impl Moe2RegisteredModel {
    pub fn at_registration(
        state: &crate::palw_state_v2::PalwChainStateV2,
        catalog: &crate::palw_model_tier_v1::PalwModelTierMirrorV1,
        model: crate::Hash64,
        registration_daa: u64,
    ) -> Option<Self> {
        let class = state.class(&model)?;
        if class.registered_daa != registration_daa || !catalog.active_at(registration_daa) {
            return None;
        }
        let tier = crate::palw_model_tier_v1::palw_model_tier_of_v1(
            Some(catalog),
            &model,
            &class.artifact_root,
            class.registrant_bond.is_none(),
            registration_daa,
        );
        let allocation = if tier.is_approved() {
            Moe2ModelAllocation::Core
        } else {
            Moe2ModelAllocation::Additional(PalwModelMarketV1::open_virtual_v2(
                registration_daa,
                super::PALW_MODEL_MARKET_VIRTUAL_SOMPI_V2,
            ))
        };
        Some(Self { model, artifact_root: class.artifact_root, registration_daa, owner: class.registrant_bond, allocation })
    }
}

/// The two residual plans for jobs without delegated prefix work. The selected
/// plan is bound at admission. Additional plans need their own role allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moe2WorkerPlan {
    Miner,
    MinerAndPanel20,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moe2RewardAllocation {
    pub miner: u64,
    pub panel_pool: u64,
    pub market: Option<Moe2MarketQuote>,
}

impl Moe2ModelAllocation {
    pub fn quote_reward(
        &self,
        reward: u64,
        plan: Moe2WorkerPlan,
        limits: Moe2QuantityLimits,
    ) -> Result<Moe2RewardAllocation, Moe2QuoteRefusal> {
        if reward > limits.max_reward {
            return Err(Moe2QuoteRefusal::OutsideQuote);
        }
        let Self::Additional(market) = self else {
            return Ok(Moe2RewardAllocation { miner: reward, panel_pool: 0, market: None });
        };
        let quote = quote_additional_reward(market, reward, limits)?;
        let workers = quote.split.workers;
        let panel_pool = match plan {
            Moe2WorkerPlan::Miner => 0,
            Moe2WorkerPlan::MinerAndPanel20 => workers / 5,
        };
        Ok(Moe2RewardAllocation { miner: workers - panel_pool, panel_pool, market: Some(quote) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) const LIMITS: Moe2QuantityLimits = Moe2QuantityLimits {
        max_reward: u64::MAX,
        max_purchase_sompi: u64::MAX,
        min_out: 0,
        max_purchase_units: u64::MAX,
        max_lp_sompi: u64::MAX,
        max_lp_units: u64::MAX,
    };

    pub(super) fn market() -> PalwModelMarketV1 {
        // The worked RFC example has 1,000 real BILI, no virtual BILI and 10,000 units in the curve.
        let mut market = PalwModelMarketV1::seed_v1(1, 1_000, Default::default());
        market.position_units = 10_000;
        market
    }

    #[test]
    fn canonical_purchase_lp_and_residual() {
        let old = market();
        let quote = quote_additional_reward(&old, 1_000, LIMITS).unwrap();
        assert_eq!(quote.split, Moe2RewardSplit { purchase: 300, liquidity: 200, workers: 500 });
        assert_eq!((quote.purchased_units, quote.liquidity_units, quote.retired_units), (2307, 1183, 1124));
        assert_eq!((quote.after.msk_reserve, quote.after.position_units), (1500, 8876));
        assert_eq!(quote.after.seed_sompi, old.seed_sompi);
        assert_eq!(quote.after.retired_units, old.retired_units + 1124);
        assert_eq!(quote.after.registrant_paid_sompi, old.registrant_paid_sompi);
    }

    #[test]
    fn worker_plan_applies_to_residual_of_additional_only() {
        let plan = Moe2WorkerPlan::MinerAndPanel20;
        let core = Moe2ModelAllocation::Core.quote_reward(1000, plan, LIMITS).unwrap();
        assert_eq!(core, Moe2RewardAllocation { miner: 1000, panel_pool: 0, market: None });
        let additional = Moe2ModelAllocation::Additional(market()).quote_reward(1000, plan, LIMITS).unwrap();
        assert_eq!((additional.miner, additional.panel_pool), (400, 100));
        let quote = additional.market.unwrap();
        assert_eq!(quote.split.purchase + quote.split.liquidity + additional.miner + additional.panel_pool, 1000);
    }

    #[test]
    fn dust_bounds_and_rounding() {
        let mut m = market();
        assert_eq!(quote_additional_reward(&m, 3, LIMITS).unwrap().after, m);
        let only_buy = quote_additional_reward(&m, 4, LIMITS).unwrap();
        assert_eq!(only_buy.split.liquidity, 0);
        assert_eq!(only_buy.retired_units, only_buy.purchased_units);
        assert_eq!(
            quote_additional_reward(&m, 1000, Moe2QuantityLimits { min_out: 2308, ..LIMITS }),
            Err(Moe2QuoteRefusal::OutsideQuote)
        );
        let exact = Moe2QuantityLimits { max_purchase_sompi: 300, max_lp_sompi: 200, ..LIMITS };
        assert!(quote_additional_reward(&m, 1000, exact).is_ok());
        for short in [Moe2QuantityLimits { max_purchase_sompi: 299, ..exact }, Moe2QuantityLimits { max_lp_sompi: 199, ..exact }] {
            assert_eq!(quote_additional_reward(&m, 1000, short), Err(Moe2QuoteRefusal::OutsideQuote));
        }
        m.position_units = 2;
        assert_eq!(quote_additional_reward(&m, 10_000, LIMITS), Err(Moe2QuoteRefusal::LiquidityDust));
        m.position_units = PALW_MODEL_SUPPLY_UNITS_V1;
        m.msk_reserve = u64::MAX;
        assert_eq!(quote_additional_reward(&m, u64::MAX, LIMITS), Err(Moe2QuoteRefusal::Overflow));
    }
}
