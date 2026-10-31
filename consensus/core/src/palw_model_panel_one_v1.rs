//! Claim-local transition to the G14 single-seat model route.
//!
//! The migration is selected by original claim acceptance, never the height of a
//! later receipt, redraw, restart or dispute. Historical claims retain their old
//! panel. Model identity and tier do not select a different verification route.
//! This policy does not certify a claim: Gate C and objective adjudication remain
//! separate requirements of the caller.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_panel_v2::PalwPanelDrawPolicyV1;
use crate::palw_panel_v2::{PalwPanelParamsV2, PalwPanelV2Error};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwPanelSeatV2};

/// A separate activation identity: the already-fired kernel-slice fence is not
/// retroactively reinterpreted as a migration of every model.
pub const PALW_MODEL_PANEL_ONE_RULE_DOMAIN_V1: &[u8] = b"misaka-palw/model-panel-one/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwModelClaimRouteV1 {
    HistoricalPanel,
    G14PanelOne,
}

/// Reuse G14's commit-then-draw randomness. A missing or pre-commit anchor is
/// unavailable randomness, never permission to use the producer's old anchor.
pub fn model_panel_one_seed_v1(state: &PalwChainStateV2, claim: &Hash64, accepted_daa: u64, span_daa: u64) -> Option<Hash64> {
    let anchor = state.round_seed_anchor()?;
    crate::palw_kernel_route_v1::palw_kernel_panel_one_anchor_after_commit_v1(accepted_daa, span_daa, Some(anchor))
        .then(|| crate::palw_kernel_route_v1::palw_kernel_panel_one_seed_v1(&claim.as_bytes(), anchor))
}

/// One global bonded draw, as on the G14 kernel route. A model's review,
/// readiness count, tier and publisher do not filter this population. The
/// selected bond still has to approve this particular claim with its own copy;
/// an unavailable copy supplies no approval or consensus credit.
pub fn derive_model_panel_one_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    seed: Hash64,
    minimum_collateral: u64,
    registered_by_daa: Option<u64>,
    policy: PalwPanelDrawPolicyV1,
) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
    let claim = state.claim(claim_id).ok_or(PalwPanelV2Error::MissingClaim(*claim_id))?;
    let producer = state.bond(&claim.bond).ok_or(PalwPanelV2Error::SeatBondMissing(claim.bond))?;
    let lock =
        policy.valid_lock.ok_or(PalwPanelV2Error::InvalidParams("G14 single-seat draw requires the claim's full signer lock"))?;
    let floor = policy.economy.map_or(minimum_collateral, |economy| economy.panel_floor_sompi.max(minimum_collateral));
    let mut best: Option<(u128, PalwBondKeyV2)> = None;
    for (key, bond) in state.bonds_iter() {
        if *key == claim.bond
            || bond.operator_id == producer.operator_id
            || bond.pubkey == producer.pubkey
            || !crate::palw_state_v2::palw_bond_may_take_work_v2(bond, floor)
            || registered_by_daa.is_some_and(|by| bond.registered_daa > by)
            || crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(state, key)
            || !lock.admits(state, key)
        {
            continue;
        }
        let weight = u128::from((bond.collateral / crate::constants::SOMPI_PER_KASPA).clamp(1, 1_000_000));
        let ticket = crate::palw_kernel_route_v1::palw_kernel_interim_key_v1(&seed, &bond.operator_id, weight);
        // One winner: min(op -> min(bond)) is just min(bond). The same
        // operator's bonds use the same random numerator, so no dedup map or
        // population-sized allocation is needed for a one-seat draw.
        best = Some(best.map_or((ticket, *key), |held| held.min((ticket, *key))));
    }
    let (_, bond) = best.ok_or(PalwPanelV2Error::InsufficientEligibleBonds { needed: 1, available: 0 })?;
    let operator_id = state.bond(&bond).ok_or(PalwPanelV2Error::SeatBondMissing(bond))?.operator_id;
    Ok(vec![PalwPanelSeatV2 { bond, operator_id }])
}

/// Resolve once from the claim's ORIGINAL acceptance, including for a redraw.
/// An absent migration preserves all historical rules byte for byte.
pub fn model_claim_route_v1(from_daa: Option<u64>, accepted_daa: u64) -> PalwModelClaimRouteV1 {
    match from_daa {
        Some(from) if from != u64::MAX && accepted_daa >= from => PalwModelClaimRouteV1::G14PanelOne,
        _ => PalwModelClaimRouteV1::HistoricalPanel,
    }
}

/// The panel for this claim, independently of its model or its coefficient.
/// There is no fall back to a larger historical panel after migration.
pub fn model_claim_panel_v1(
    from_daa: Option<u64>,
    accepted_daa: u64,
    historical: &PalwPanelParamsV2,
) -> Result<PalwPanelParamsV2, PalwPanelV2Error> {
    match model_claim_route_v1(from_daa, accepted_daa) {
        PalwModelClaimRouteV1::HistoricalPanel => Ok(historical.clone()),
        PalwModelClaimRouteV1::G14PanelOne => PalwPanelParamsV2::new(1, 1, historical.anchor_delay()),
    }
}

/// A counted whole-claim approval is one basis on the new route. Old segmented
/// claims still owe the historical two independent attestations. Callers must
/// establish the whole-claim scope before counting this basis.
pub fn model_claim_required_basis_v1(from_daa: Option<u64>, accepted_daa: u64, historical: u8) -> u8 {
    match model_claim_route_v1(from_daa, accepted_daa) {
        PalwModelClaimRouteV1::HistoricalPanel => historical,
        PalwModelClaimRouteV1::G14PanelOne => 1,
    }
}

/// Price the sole liable signer against the whole gain. Historical two-signer
/// pricing cannot be reused for one signature, and extra copies of that same
/// receipt must never divide this lock.
pub fn model_claim_seat_lock_v1(
    from_daa: Option<u64>,
    accepted_daa: u64,
    g_res: u128,
    escrowed_reward: u64,
    buyback_bound: u64,
    historical_basis: u8,
) -> u128 {
    use crate::palw_economic_safety_v1::palw_seat_lock_required_v2;
    use crate::palw_state_v2::{PALW_RCORE_ESCROW_LOCK_PERMILLE_V1, PALW_RCORE_VESTING_ROWS_LANDED_V1, palw_rcore_lock_v1};
    if model_claim_route_v1(from_daa, accepted_daa) == PalwModelClaimRouteV1::HistoricalPanel {
        return palw_rcore_lock_v1(g_res, escrowed_reward, buyback_bound, historical_basis);
    }
    if PALW_RCORE_VESTING_ROWS_LANDED_V1 {
        let cap = crate::palw_model_market_v1::palw_model_buyback_slice_v1(escrowed_reward);
        let gain = g_res.saturating_sub(u128::from(buyback_bound)).saturating_add(u128::from(cap));
        let escrow =
            u128::from(escrowed_reward.saturating_sub(cap)).saturating_mul(PALW_RCORE_ESCROW_LOCK_PERMILLE_V1).div_ceil(1_000);
        palw_seat_lock_required_v2(gain, 1).saturating_add(escrow)
    } else {
        palw_seat_lock_required_v2(g_res.saturating_sub(u128::from(buyback_bound)).saturating_add(u128::from(escrowed_reward)), 1)
    }
}

impl Params {
    /// Single-seat approval never replaces either public adjudication or the
    /// independently drawn Gate C approvals. Require the complete dependency
    /// chain before the migration, including on an isolated deployment.
    pub fn validate_palw_model_panel_one_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_model_panel_one_v1_fence() else { return Ok(()) };
        let dependencies = [
            ("palw_kernel_route_panel_one_v1", self.palw_kernel_route_panel_one_v1),
            ("palw_rcore_plus", self.palw_rcore_plus),
            ("palw_execution_lane", self.palw_execution_lane.map(|lane| lane.activation)),
            ("palw_legacy_public_filer_v1", self.palw_legacy_public_filer_v1),
            ("palw_legacy_held_da_v2", self.palw_legacy_held_da_v2),
            ("palw_court_capacity_v1", self.palw_court_capacity_v1.map(|f| f.activation)),
            ("palw_g14_route_required_v1", self.palw_g14_route_required_v1),
            ("palw_independently_work_certified_v1", self.palw_independently_work_certified_v1),
        ];
        if dependencies.iter().any(|(_, fence)| fence.is_none_or(|f| f == ForkActivation::never() || f.daa_score() > at.daa_score())) {
            return Err(PalwModeV2Error::Invalid(
                "palw_model_panel_one_v1 requires the G14 single-seat route, public filer, held DA, court capacity, route gate and Gate C at or below its activation",
            ));
        }
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) if bundle.state.model_panel_one_from_daa() == Some(at.daa_score()) => {
                let span = self.palw_execution_lane.expect("dependency checked").schedule_span_daa_at(at.daa_score());
                if span == 0 || span > bundle.state.window_bind() {
                    return Err(PalwModeV2Error::Invalid("G14 post-commit seed span must fit the claim bind window"));
                }
                Ok(())
            }
            _ => Err(PalwModeV2Error::Invalid("palw_model_panel_one_v1 requires its synchronized ConsensusV2 mirror")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_acceptance_controls_panels_across_the_migration() {
        let old = PalwPanelParamsV2::new(5, 3, 2).unwrap();
        for accepted in [0, 98, 99] {
            let p = model_claim_panel_v1(Some(100), accepted, &old).unwrap();
            assert_eq!((p.seat_count(), p.quorum(), p.anchor_delay()), (5, 3, 2));
            assert_eq!(model_claim_required_basis_v1(Some(100), accepted, 2), 2);
        }
        for accepted in [100, 101, u64::MAX] {
            let p = model_claim_panel_v1(Some(100), accepted, &old).unwrap();
            assert_eq!((p.seat_count(), p.quorum(), p.anchor_delay()), (1, 1, 2));
            assert_eq!(model_claim_required_basis_v1(Some(100), accepted, 2), 1);
        }
    }

    #[test]
    fn dormant_migration_preserves_each_historical_panel() {
        for (seats, quorum) in [(3, 2), (5, 3), (7, 4)] {
            let old = PalwPanelParamsV2::new(seats, quorum, 2).unwrap();
            for activation in [None, Some(u64::MAX)] {
                let p = model_claim_panel_v1(activation, u64::MAX, &old).unwrap();
                assert_eq!((p.seat_count(), p.quorum()), (seats, quorum));
            }
        }
    }

    #[test]
    fn single_seat_migration_cannot_be_armed_without_public_court_and_gate_c() {
        let mut params = crate::config::params::TESTNET11_PARAMS.clone();
        params.validate_palw_model_panel_one_v1().unwrap();
        params.palw_model_panel_one_v1 = Some(ForkActivation::new(20_000));
        assert!(params.validate_palw_model_panel_one_v1().is_err());
        params.palw_model_panel_one_v1 = Some(ForkActivation::never());
        params.validate_palw_model_panel_one_v1().unwrap();
    }

    #[test]
    fn t12_can_arm_the_migration_only_with_its_synchronized_mirror() {
        let mut p = crate::config::params::palw_t12_shipped_params();
        p.palw_model_panel_one_v1 = Some(ForkActivation::new(20_000));
        assert!(p.validate_palw_model_panel_one_v1().is_err());
        p.sync_palw_g14_gates_v1();
        p.validate_palw_model_panel_one_v1().unwrap();
    }

    #[test]
    fn a_future_migration_keeps_the_live_handshake_and_kernel_policy_identity() {
        let old = crate::config::params::palw_t12_shipped_params();
        let mut scheduled = old.clone();
        scheduled.palw_model_panel_one_v1 = Some(ForkActivation::new(20_000));
        scheduled.sync_palw_g14_gates_v1();
        assert_eq!(old.consensus_identity_id(), scheduled.consensus_identity_id());
        assert_ne!(old.consensus_params_id(), scheduled.consensus_params_id());
        assert_ne!(old.consensus_schedule_id(), scheduled.consensus_schedule_id());
    }

    #[test]
    fn one_signer_carries_the_whole_lock_and_duplicate_basis_cannot_reduce_it() {
        let old = model_claim_seat_lock_v1(Some(100), 99, 10_000, 1_000, 50, 2);
        let one = model_claim_seat_lock_v1(Some(100), 100, 10_000, 1_000, 50, 1);
        // Independent ceilings and the integer margin may differ by two units.
        assert!(one > old);
        assert!(one.saturating_add(3) >= old.saturating_mul(2));
        for basis in [0, 2, 3, u8::MAX] {
            assert_eq!(model_claim_seat_lock_v1(Some(100), 100, 10_000, 1_000, 50, basis), one);
        }
        assert_eq!(model_claim_seat_lock_v1(None, 100, 10_000, 1_000, 50, 2), old);
    }
}
