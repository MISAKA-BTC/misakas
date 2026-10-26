//! **Lane ROLES: the Panel=1 slice class's root is attested public from `palw_kernel_route_panel_one_v1`, and only that root.**
//! Below the fence a `RegisterClass` of `wide128_v1(7)` is refused (no attestation: byte-identical to before); at or past it the same
//! object registers; a different synthetic root (`wide128_v1(8)`) is still refused. No test attestation hook is used here — run alone
//! (`--exact`), since the hook is process-wide and other tests attest `wide128_v1(7)` through it.
use super::*;

const H: u64 = 60;

fn config_panel_one_at(h: u64) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = kernel_config();
    let mut params = config.params.clone();
    params.palw_kernel_route_panel_one_v1 = Some(ForkActivation::new(h));
    params.sync_palw_g14_gates_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        unreachable!("testnet-12 is ConsensusV2")
    };
    let mut bundle = bundle.clone();
    bundle.state = bundle.state.clone().with_test_only_kernel_panel_one_external_gain_bound_v1(Some(100_000_000));
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(node_bundle) = &mut params.palw_consensus_mode {
        *node_bundle = bundle.clone();
    }
    assert_eq!(bundle.state.kernel_route_panel_one_from_daa(), Some(h));
    (Config::new(params), bundle, premine, floats)
}

fn register_of(seed: u64) -> (K, ParamCommitmentsV1) {
    let fx = wide128_v1(seed);
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&fx.program.encode()), MAX_POSITIONS).unwrap();
    let pc = ParamCommitmentsV1::of(&fx.params);
    (K::RegisterClass { descriptor: d.digest(), program_bytes: fx.program.encode(), plan, param_commitments: pc.clone() }, pc)
}

#[tokio::test]
async fn roles_slice_root_is_attested_only_past_the_panel_one_fence() {
    let (reg7, pc7) = register_of(7);
    assert_eq!(pc7.root(), kaspa_consensus_core::palw_kernel_route_v1::PALW_T12_KERNEL_SLICE_ATTESTED_ROOT_V1, "the pinned root is wide128_v1(7)'s");
    let mut net = Net::over_cfg(config_panel_one_at(H), TestConsensus::new);
    assert!(net.daa() + 3 < H, "the world starts below the fence");

    // Below the fence: refused, nothing registers.
    let o = net.route(1, &reg7);
    net.send(vec![(1, o)]).await;
    assert!(net.daa() < H);
    assert!(net.ledger().classes.is_empty(), "below the fence the slice root is not attested");

    // Past the fence: the same object registers; another synthetic root does not.
    net.beat_to(H + 1).await;
    let (reg8, _) = register_of(8);
    let o = net.route(1, &reg8);
    net.send(vec![(1, o)]).await;
    assert!(net.ledger().classes.is_empty(), "a root other than the pinned one is still refused");
    let o = net.route(1, &reg7);
    net.send(vec![(1, o)]).await;
    let classes = net.ledger().classes;
    assert_eq!(classes.len(), 1, "the pinned root registers past the fence");
    assert_eq!(classes.values().next().unwrap().param_commitments.root(), pc7.root());
}
