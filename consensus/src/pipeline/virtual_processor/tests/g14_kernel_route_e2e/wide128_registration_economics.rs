//! **G14-FIX (coordinator 2026-10-27): does the Panel=1 slice class `wide128_v1(7)` register on the SHIPPED testnet-12 params with
//! the G14 list fired at 17,400, under the production 500 MSK external gain bound?** No harness, no `cfg(test)` seam: the shipped
//! params, the list armed through `palw_t12_arm_after_int13_at_v1`, the kernel route's production ledger policy (64 adjudications
//! per block, the E1 1,100 MSK collateral), the class's own prosecution bounds as `register_class` derives them, and the fold's
//! Panel=1 registration predicate (`panel_one_censorship_check_clocked_v1`) with X3's clock where the list arms it.
use super::*;
use kaspa_consensus_core::config::params::{PALW_T12_G14_17000_DAA_V1, PALW_T12_G14_17000_FENCES_V1, palw_t12_arm_after_int13_at_v1};
use kaspa_consensus_core::palw_kernel_route_panel_one_censorship_v1::{
    panel_one_censorship_check_clocked_v1, panel_one_censorship_cost_clocked_v1, panel_one_censorship_cost_v1,
};

#[test]
fn g14_fix_wide128_registers_under_the_production_gain_bound_on_shipped_params_at_17400() {
    let at = PALW_T12_G14_17000_DAA_V1;
    let mut params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    palw_t12_arm_after_int13_at_v1(&mut params, PALW_T12_G14_17000_FENCES_V1, Some(at));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        unreachable!("testnet-12 is ConsensusV2")
    };
    let state = &bundle.state;
    assert!(state.kernel_route_panel_one_governs_v1(at), "Panel=1 is in force at {at}");
    let clock_from = state.claim_hard_deadline_v1().map(|m| m.from_daa);
    let clocked = clock_from.is_some_and(|from| at >= from);

    // The route's production ledger policy (no cfg(test) adjudication cap).
    let policy = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Default::default(), Default::default());
    assert_eq!(policy.max_adjudications_per_block, 64);
    assert_eq!(policy.claim_collateral, 1_100 * 100_000_000, "E1 (ACT2 0c63ddf72)");

    // The class's bounds exactly as `register_class` derives them.
    let fx = wide128_v1(7);
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&fx.program.encode()), MAX_POSITIONS).unwrap();
    assert_eq!(
        ParamCommitmentsV1::of(&fx.params).root(),
        kaspa_consensus_core::palw_kernel_route_v1::PALW_T12_KERNEL_SLICE_ATTESTED_ROOT_V1,
        "wide128_v1(7), root 44c62870…960e"
    );
    let bounds = misaka_palw_kernel::gate::class_prosecution_bounds_v1(
        &d,
        &plan,
        &fx.program,
        &misaka_palw_kernel::public::ProfileMaterialV1::kernel_route(true),
        &policy.prosecution,
    )
    .expect("publicly prosecutable");
    let worst = bounds.max_court_work;

    let gain_bound = state.kernel_panel_one_external_gain_bound_v1();
    assert_eq!(gain_bound, Some(500 * 100_000_000), "the production bound, no test override");
    let cost = if clocked { panel_one_censorship_cost_clocked_v1(&policy, worst) } else { panel_one_censorship_cost_v1(&policy, worst) };
    let gain = if clocked { policy.claim_collateral as u128 } else { 0 } + policy.claim_reward as u128 + gain_bound.unwrap() as u128;
    eprintln!(
        "G14FIX-WIDE128 clock_from={clock_from:?} clocked={clocked} worst_court_work={worst} max_court_work_per_block={} \
         dismissal_fee={} cost={cost} gain={gain} ratio={:.3}",
        policy.max_court_work_per_block,
        policy.dismissal_fee_v1(worst),
        cost as f64 / gain as f64
    );
    assert_eq!(
        panel_one_censorship_check_clocked_v1(&policy, worst, gain_bound, clocked),
        Ok(()),
        "wide128_v1(7) must register on testnet-12 at {at}: cost {cost} vs gain {gain}"
    );
}
