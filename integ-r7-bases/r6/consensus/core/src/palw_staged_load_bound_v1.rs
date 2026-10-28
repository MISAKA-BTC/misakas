//! Inclusion-pressure bound for the staged public verifier route WITHOUT a consensus cap.
//!
//! A per-block cap on staged moves would be a censorship lever: a coalition that fills the cap
//! with its own cheap moves would lock an outside owner out of every block. The route instead
//! bounds load by what each party pays and owns:
//!
//! * every staged move is an ordinary fee-paying transaction (mass and fee rules unchanged);
//! * every owned row is prepaid by its own reserver (`plan.deposit_sompi` + rent), its moves and
//!   carrier bytes are capped by that reserver's own plan, and
//!   `PalwStagedPublicCourtConfigV1::maximum_owned_rows_per_reserver` limits rows per bond, so a
//!   coalition can neither spend nor occupy an outsider's allowance or move budget;
//! * every move window carries `prices.inclusion_margin_daa` per required move, and source
//!   demands carry a full response span measured from the ACCEPTING block.
//!
//! The only remaining lever is fee competition for block space. To make an outsider miss one
//! window the coalition must keep the outsider's carrier out of every block for the whole
//! margin, i.e. outbid it on all block mass for `inclusion_margin_daa` DAA. Its cost is at least
//! `margin × mass_per_daa × outsider_feerate`; the outsider's own cost to stay included is at
//! most `moves × carrier_mass × outsider_feerate`. These are pure accounting helpers; the
//! measured values below are placeholders until measured on the target network and are read by
//! no consensus rule.

/// Placeholder (MEASURE): block mass the network admits per DAA at the target block rate.
pub const PALW_STAGED_MEASURED_BLOCK_MASS_PER_DAA_V1: u64 = 500_000;
/// Placeholder (MEASURE): the largest staged carrier's transaction mass in grams.
pub const PALW_STAGED_MEASURED_CARRIER_MASS_V1: u64 = 100_000;
/// Placeholder (MEASURE): fee rate an outside owner is prepared to pay, sompi per gram.
pub const PALW_STAGED_OUTSIDER_FEERATE_HEADROOM_V1: u64 = 10;

/// What a coalition must at least spend to delay one outsider move past its inclusion margin.
pub fn palw_staged_coalition_delay_cost_v1(inclusion_margin_daa: u64, block_mass_per_daa: u64, outsider_feerate: u64) -> u128 {
    u128::from(inclusion_margin_daa) * u128::from(block_mass_per_daa) * u128::from(outsider_feerate)
}

/// What the outsider spends to carry every move of its prepaid plan at the same fee rate.
pub fn palw_staged_outsider_inclusion_cost_v1(maximum_moves: u64, carrier_mass: u64, outsider_feerate: u64) -> u128 {
    u128::from(maximum_moves) * u128::from(carrier_mass) * u128::from(outsider_feerate)
}

/// The asymmetry the route relies on: delaying ONE move costs the coalition more than the
/// outsider pays for its WHOLE plan. False means the margin or fee headroom must be raised.
pub fn palw_staged_inclusion_pressure_holds_v1(
    maximum_moves: u64,
    inclusion_margin_daa: u64,
    block_mass_per_daa: u64,
    carrier_mass: u64,
    outsider_feerate: u64,
) -> bool {
    palw_staged_coalition_delay_cost_v1(inclusion_margin_daa, block_mass_per_daa, outsider_feerate)
        > palw_staged_outsider_inclusion_cost_v1(maximum_moves, carrier_mass, outsider_feerate)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delay_cost_scales_with_margin_and_capacity_not_with_any_cap() {
        let margin = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().prices.inclusion_margin_daa;
        let cost = palw_staged_coalition_delay_cost_v1(margin, PALW_STAGED_MEASURED_BLOCK_MASS_PER_DAA_V1, 10);
        assert_eq!(cost, u128::from(margin) * 500_000 * 10);
        assert!(palw_staged_coalition_delay_cost_v1(margin + 1, 500_000, 10) > cost);
        // A plan whose whole inclusion cost exceeds one margin of block space does not hold:
        // the bound is reported, never manufactured.
        assert!(palw_staged_inclusion_pressure_holds_v1(8, margin, 500_000, 100_000, 10));
        assert!(!palw_staged_inclusion_pressure_holds_v1(1_000, margin, 500_000, 100_000, 10));
    }
}
