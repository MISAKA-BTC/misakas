//! **RFC-0023 stage 1 on the node (2026-10-18)**: `palw_unresolved_bound_v1` armed at DAA 20 beside Gate C (both dormant on every
//! preset; armed here without their validation, as every kernel-route harness does). Attempt claims open before and after the fence
//! (they enter the unresolved set); the node runs past the fence; then (a) a real restart over the same database and (b) a fresh node
//! replaying the whole chain from genesis (the IBD path) reach the same sink, state root, kernel rows, per-block delta roots and the
//! same resolved prefix at the tip.
use super::*;

fn stage1_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (cfg, bundle, premine, floats) = kernel_config();
    let mut params = cfg.params.clone();
    params.palw_model_eligibility_v1 = Some(ForkActivation::new(0));
    params.palw_independently_work_certified_v1 = Some(ForkActivation::new(0));
    params.palw_unresolved_bound_v1 = Some(ForkActivation::new(20));
    let mut tmp = params.clone();
    tmp.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle);
    tmp.sync_palw_rfc0018_v1();
    tmp.sync_palw_unresolved_bound_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = tmp.palw_consensus_mode else { unreachable!() };
    params.sync_palw_rfc0018_v1();
    params.sync_palw_unresolved_bound_v1();
    assert_eq!(bundle.state.unresolved_bound_from_daa(), Some(20), "the stage-1 mirror is on the bundle");
    (Config::new(params), bundle, premine, floats)
}

fn resolved_through(net: &Net) -> u64 {
    let state = net.chain.tip_state().1;
    let params = net.chain.vp().palw_state_params_v2.clone().expect("ConsensusV2");
    kaspa_consensus_core::palw_unresolved_bound_v1::palw_resolved_through_v1(&state, &params, net.daa())
}

#[tokio::test]
async fn rfc23s1_node_restart_and_genesis_replay_agree_on_the_resolved_prefix_across_the_fence() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(stage1_config(), |c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let ttpb = net.ttpb();
    let mut opened = 0;
    for target in [8u64, 24, 40] {
        net.beat_to(target).await;
        let _ = net.chain.attempt(1, ttpb, Vec::new(), &|_| true).await;
        opened += net.chain.tip_state().1.unresolved_first_blue_score_v1().is_some() as u32;
    }
    assert!(opened > 0, "an attempt claim entered the unresolved set");
    net.beat_to(160).await;
    let (sink, root, prefix) = (net.chain.sink(), net.chain.tip_state().1.state_root(), resolved_through(&net));
    println!("rfc23s1/node: tip daa {}, resolved prefix {prefix}, open {:?}", net.daa(), net.chain.tip_state().1.unresolved_first_blue_score_v1());
    let net = net.restart(db.clone());
    assert_eq!((net.chain.sink(), net.chain.tip_state().1.state_root()), (sink, root), "the restarted node's tip off disk");
    assert_eq!(resolved_through(&net), prefix, "the same resolved prefix after a restart");
    let replay = net.replay().await;
    net.assert_same(&replay, "RFC-0023 stage 1: a genesis replay across the fence");
    let other = net.on_chain(replay);
    assert_eq!(resolved_through(&other), prefix, "the same resolved prefix on the replaying node");
}
