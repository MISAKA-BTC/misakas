//! **R8-BUDGET hook H-1 on the real node (2026-10-11)**: a standalone kernel-route claim under the bond budget (ADR-0176) — the fence
//! `palw_bond_budget_v1` armed WITHOUT its validation (as `budget_e2e` arms it), under BOTH values of the undecided market-fee POLICY.
//! The claim's commitment reserves `Q` 1 and its liability on the producer's V2 bond (and, where the fee counts, its job escrow in
//! `R`); the actual escrow payment at Final consumes exactly that `R`; the Final closes the row and holds the bond through the claim's
//! inclusive liability horizon; a second node replays every row. A full open-claim cap refuses the next commitment (the object is
//! dropped, the block stands). Nothing here selects a policy value: both are run.
use super::*;
use kaspa_consensus_core::palw_bond_budget_v1::{
    PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1, PALW_BOND_BUDGET_POLICY_VERSION_V2, PALW_BUDGET_BLOCK_UNIT_V1, PalwBondBudgetFenceV1,
    PalwBondBudgetPolicyV1, PalwBudgetOriginV1, PalwMarketFeePolicyV1, PalwRoundRightsPolicyV1, PalwWeightValuePolicyV1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// A TEST policy (not a proposal): one unit is card 0's whole collateral; generous rates; `max_open` open claims per bond.
fn policy(unit: u64, market_fee: PalwMarketFeePolicyV1, max_open: u32) -> PalwBondBudgetPolicyV1 {
    PalwBondBudgetPolicyV1 {
        version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
        window_daa: 300,
        capital_unit_sompi: unit,
        rho: 1,
        claims_per_unit: 100,
        block_units_per_unit: 100 * PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_sompi: u64::MAX / 4,
        final_weight_per_unit: u64::MAX / 4,
        max_open_claims_per_bond: max_open,
        slice_rights_by_rho: false,
        round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 },
        liability_hold_daa: 7,
        export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
        weight_value: PalwWeightValuePolicyV1::Unknown,
        market_fee,
    }
}

/// [`kernel_config`] with the bond budget armed at DAA 1 under `market_fee` / `max_open`.
async fn budgeted(market_fee: PalwMarketFeePolicyV1, max_open: u32) -> World {
    let (config, base, premine, floats) = kernel_config();
    let mut params = config.params.clone();
    let unit = base
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            Obj::BondRegistered { collateral, .. } => Some(*collateral),
            _ => None,
        })
        .expect("a genesis card");
    params.palw_bond_budget_v1 =
        Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(1), policy: policy(unit, market_fee, max_open) });
    params.sync_palw_bond_budget_v1();
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fences; only this harness bypasses it");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
    let mut net = Net::over_cfg((Config::new(params), bundle, premine, floats), TestConsensus::new);
    net.beat_to(2).await;
    assert!(net.chain.tip_state().1.bond_budget().is_some(), "the engine booted at the fence");
    World::on(net).await
}

#[tokio::test]
async fn r8_h1_a_standalone_kernel_claim_reserves_consumes_and_closes_its_bond_budget_under_both_market_fee_policies() {
    kaspa_core::log::try_init_logger("warn");
    for market_fee in [PalwMarketFeePolicyV1::OutsideReward, PalwMarketFeePolicyV1::CountAgainstReward] {
        let mut w = budgeted(market_fee, 1_000).await;
        let reward = w.policy().claim_reward;
        let job = w.job().await;
        assert_eq!(w.net.ledger().job_escrows[&job.id()].amount, reward);
        let claim = w.claim(0, &job, false).await;
        let id = Hash64::from_bytes(claim.id);
        let state = w.net.chain.tip_state().1;
        let budget = state.bond_budget().expect("the engine");
        let row = *budget.claim_row(&id).expect("the commitment reserved on the producer's bond");
        assert_eq!(row.origin, PalwBudgetOriginV1::KernelRoute);
        assert_eq!(row.bond, w.net.bond(0));
        assert!(row.open);
        assert_eq!(row.reserved.claims, 1, "{market_fee:?}: one claim of Q");
        assert_eq!((row.reserved.block_units, row.reserved.final_weight), (0, 0), "a kernel claim mints no block and carries no weight");
        let expected_r = match market_fee {
            PalwMarketFeePolicyV1::OutsideReward => 0,
            PalwMarketFeePolicyV1::CountAgainstReward => reward,
        };
        assert_eq!(row.reserved.reward_sompi, expected_r, "{market_fee:?}: the escrow is reserved in R only where the fee counts");
        assert_eq!(row.liability_sompi, u128::from(w.net.ledger().claims[&claim.id].reserved), "the kernel reservation is its liability");
        assert_eq!(budget.bond_row(&w.net.bond(0)).unwrap().open_claims, 1);

        w.cover(&claim.id).await;
        let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&claim.id) else { panic!("covered") };
        w.net.beat_to(window_end_daa).await;
        let ClaimStateV1::Final { final_daa } = w.net.claim_state(&claim.id) else { panic!("Final") };
        assert_eq!(minted_and_owed(&w.net, &w.net.chain, 0), (0, reward), "the escrow pays the producer once");
        let state = w.net.chain.tip_state().1;
        let budget = state.bond_budget().unwrap();
        let row = *budget.claim_row(&id).unwrap();
        assert!(!row.open, "the Final closes the row");
        assert_eq!(row.consumed.reward_sompi, expected_r, "{market_fee:?}: the actual fee consumed exactly its reservation");
        let liability_until = w.net.ledger().claims[&claim.id].liability_until.expect("a Final claim's horizon");
        let bond = budget.bond_row(&w.net.bond(0)).unwrap();
        assert_eq!(bond.open_claims, 0);
        assert_eq!(bond.liability_hold_until, (final_daa + 7).max(liability_until + 1), "held through the inclusive horizon");
        assert!(budget.liability_holds(&w.net.bond(0), liability_until), "the last day still holds");
        assert!(row.reuse_not_before > w.net.daa(), "the window is not recovered early (D4)");
        let z = w.net.replay().await;
        w.net.assert_same(&z, "replay of the budgeted kernel claim");
        assert_eq!(z.tip_state().1.bond_budget(), state.bond_budget(), "a second node folds the same budget rows");
    }
}

#[tokio::test]
async fn r8_h1_a_full_open_claim_cap_refuses_the_next_kernel_commitment_and_the_block_stands() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = budgeted(PalwMarketFeePolicyV1::CountAgainstReward, 1).await;
    let first_job = w.job().await;
    let second_job = w.job().await;
    let first = w.claim(0, &first_job, false).await;
    let sink_before = w.net.chain.sink();
    let refused = w.claim_with(0, &second_job, false, Delivery::Direct).await;
    assert_ne!(w.net.chain.sink(), sink_before, "the chain advanced: the carrying block stands");
    assert!(!w.net.ledger().claims.contains_key(&refused.id), "the second commitment is refused at the full open-claim cap");
    let state = w.net.chain.tip_state().1;
    let budget = state.bond_budget().unwrap();
    assert!(budget.claim_row(&Hash64::from_bytes(refused.id)).is_none(), "a refused object writes no budget row");
    assert!(budget.claim_row(&Hash64::from_bytes(first.id)).is_some_and(|row| row.open));
    // A different producer is unaffected by card 0's cap.
    let other = w.claim(2, &second_job, false).await;
    assert!(w.net.chain.tip_state().1.bond_budget().unwrap().claim_row(&Hash64::from_bytes(other.id)).is_some());
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay with the refused commitment");
}
