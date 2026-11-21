use super::*;
#[path = "mas1_stop.rs"]
mod mas1_stop;
#[path = "moe2_reservation.rs"]
mod moe2_reservation;
use kaspa_consensus_core::{
    config::params::{
        PALW_T12_FULL_BOND_BUDGET_ENTRY, PALW_T12_FULL_GATE_C_ENTRY, PALW_T12_FULL_PROBABILISTIC_CONSTRAINTS_ENTRY,
        PALW_T12_G14_MODEL_BOND_ALLOCATION_ENTRY, PALW_T12_G14_MODEL_TIER_ENTRY,
    },
    palw_g14_gates_v1::{palw_isolated_g14_genesis_v1, palw_private_moe2_genesis},
    palw_model_market_v1::{PALW_MODEL_MARKET_VIRTUAL_SOMPI_V2, PalwModelMarketV1, moe2::settlement::Moe2SettlementBook},
};
use kaspa_rpc_core::RpcRawBlock;
use std::{fs::File, process::Command};

fn registration_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = t12_tir_config(0);
    let mut params = config.params;
    let base = params.genesis.clone();
    let isolation = Hash64::from_u64_word(64);
    let from = Some(ForkActivation::new(1));
    if std::env::var_os("MISAKA_PRIVATE_QF1_FULL").is_some() {
        // The launch harness predates the shipped range evaluator (t12 DAA 3600).
        // The first heartbeat can still have DAA zero when registration is priced.
        (kaspa_consensus_core::palw_tir_fence2_v1::PALW_T12_TIR_FENCE2_ENTRY.set)(&mut params, Some(ForkActivation::new(0)));
    }
    params.palw_model_market = from;
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    for fence in [
        PALW_T12_FULL_BOND_BUDGET_ENTRY,
        PALW_T12_FULL_PROBABILISTIC_CONSTRAINTS_ENTRY,
        PALW_T12_G14_MODEL_BOND_ALLOCATION_ENTRY,
        PALW_T12_G14_MODEL_TIER_ENTRY,
        PALW_T12_FULL_GATE_C_ENTRY,
    ] {
        (fence.set)(&mut params, from);
    }
    params.genesis = palw_isolated_g14_genesis_v1(&base, isolation).unwrap();
    assert_eq!(params.private_moe2_registration_from(), None);
    params.genesis = palw_private_moe2_genesis(&base, isolation).unwrap();
    assert_eq!(params.private_moe2_registration_from(), Some(1));
    params.validate_palw_v2().expect("the entire private ruleset validates");
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct RegistrationReplay {
    class_id: Hash64,
    artifact_root: Hash64,
    blocks: Vec<RpcRawBlock>,
    root: Hash64,
    fork: Vec<RpcRawBlock>,
    fork_root: Hash64,
    control: mas1_stop::ControlReplay,
    economics: moe2_reservation::Replay,
}

async fn heavier_fork(env: &Env, common: &Block) -> (Vec<Block>, Hash64) {
    let mut chain = t12_genesis_chain(&env.config, &env.bundle, &env.premine, &env.floats);
    arrive(&chain, common.clone(), "common parent").await;
    chain.ctx.simulated_time = common.header.timestamp;
    let mut blocks = Vec::new();
    for _ in 0..9 {
        blocks.push(chain.heartbeat(env.config.params.target_time_per_block(), Vec::new()).await);
    }
    (blocks, chain.tip_state().1.state_root())
}

/// Independently accept a signed registration, then reorganize onto a branch without it.
#[tokio::test]
async fn private_moe2_three_process_registration() {
    let mut env = Env::over(registration_config());
    let card = 2;
    let bond = env.chain.bonds[card];
    let (_, genesis) = env.chain.tip_state();
    let collateral = genesis.bond(&bond).unwrap().collateral;
    assert!(genesis.classes_iter().all(|(key, _)| genesis.model_market(key).is_none()));
    const PEER_INPUT: &str = "MISAKA_TEST_MOE2_REGISTRATION_BLOCKS";
    let peer_input = std::env::var_os(PEER_INPUT);
    let (class_id, artifact_root, blocks, expected, fork, fork_root, remote_control, remote_economics) =
        if let Some(ref path) = peer_input {
            let replay: RegistrationReplay = serde_json::from_reader(File::open(path).unwrap()).unwrap();
            let decode = |raw: Vec<RpcRawBlock>| raw.into_iter().map(|row| Block::try_from(row).unwrap()).collect::<Vec<_>>();
            let blocks = decode(replay.blocks);
            for block in &blocks {
                arrive(&env.chain, block.clone(), "MOE2 registration").await;
            }
            (
                replay.class_id,
                replay.artifact_root,
                blocks,
                Some(replay.root),
                decode(replay.fork),
                replay.fork_root,
                Some(replay.control),
                Some(replay.economics),
            )
        } else {
            let c = match std::env::var_os("MISAKA_PRIVATE_QF1_FULL").or_else(|| std::env::var_os("MISAKA_PRIVATE_TB1_TIR_ARTIFACT")) {
                Some(path) => {
                    let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(std::path::Path::new(&path)).unwrap();
                    Case {
                        class: artifact.class().unwrap(),
                        root: artifact.inventory_root().unwrap().0,
                        name: "held TIR".into(),
                        real_root: true,
                    }
                }
                None => case("moe-top2-shared"),
            };
            let m = mine_registration_on(env, &c, card).await;
            env = m.env;
            let (fork, fork_root) = heavier_fork(&env, &m.blocks[0]).await;
            (m.class_id, c.root, m.blocks, None, fork, fork_root, None, None)
        };
    let (tip, state) = env.chain.tip_state();
    assert_eq!(tip, blocks.last().unwrap().hash());
    let class = state.class(&class_id).expect("registered class");
    assert_eq!((class.artifact_root, class.registrant_bond), (artifact_root, Some(bond)));
    assert!(state.tir_class_v1(&class_id).is_some());
    let market = state.model_market(&class_id).expect("native market at registration");
    assert_eq!(market, &PalwModelMarketV1::open_virtual_v2(class.registered_daa, PALW_MODEL_MARKET_VIRTUAL_SOMPI_V2));
    let burn = collateral - state.bond(&bond).unwrap().collateral;
    assert_eq!(burn, kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1);
    let (root, delta) = env.chain.vp().palw_state_v2_store.read().delta_of(tip).unwrap();
    assert_eq!(root, state.state_root());
    let mut book = Moe2SettlementBook::default();
    book.register_block(&state, env.bundle.state.model_tier_v1().unwrap(), &delta).unwrap();
    assert_eq!(book.model(&class_id).unwrap().owner, Some(bond));
    println!(
        "pid={} tip={tip} root={root} class={class_id} artifact={artifact_root} daa={} burn={burn} market={market:?}",
        std::process::id(),
        class.registered_daa
    );
    // Run the two shallow reorg scenarios from the exact same registration prefix.
    // Concatenating the extensions instead moves the incumbent past the private
    // profile's shallow-reorg bound before the competing history even arrives.
    let (budget_observation, economics) = {
        let mut economic_env = Env::over(registration_config());
        for block in &blocks {
            arrive(&economic_env.chain, block.clone(), "MOE2 registration prefix").await;
        }
        economic_env.chain.ctx.simulated_time = blocks.last().unwrap().header.timestamp;
        let (_, economic_state) = economic_env.chain.tip_state();
        assert_eq!(economic_state.state_root(), root);
        let observed = moe2_reservation::exercise(&mut economic_env, &economic_state, book, class_id, remote_economics, &fork).await;
        let (economic_tip, rolled_back) = economic_env.chain.tip_state();
        assert_eq!(economic_tip, fork.last().unwrap().hash());
        assert_eq!(rolled_back.state_root(), fork_root);
        observed
    };
    let control = mas1_stop::exercise(&mut env, class_id, artifact_root, tip, remote_control.as_ref(), &fork).await;
    let stop_hash = Block::try_from(control.blocks[2].clone()).unwrap().hash();
    let (fork_tip, reverted) = env.chain.tip_state();
    assert_eq!(fork_tip, fork.last().unwrap().hash());
    assert_eq!(reverted.state_root(), fork_root);
    assert!(!env.chain.ctx.consensus.is_chain_ancestor_of(stop_hash, fork_tip).unwrap());
    assert!(reverted.class(&class_id).is_none() && reverted.model_market(&class_id).is_none());
    assert_eq!(reverted.bond(&bond).unwrap().collateral, collateral);
    println!("pid={} reverted_tip={fork_tip} reverted_root={fork_root} collateral_restored={burn}", std::process::id());
    if let Some(expected) = expected {
        assert_eq!(root, expected);
        let receipt = PathBuf::from(peer_input.unwrap()).with_file_name(format!("{}.json", std::process::id()));
        serde_json::to_writer(File::create(receipt).unwrap(), &(root, fork_root, budget_observation)).unwrap();
        return;
    }
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let path = dir.path().join("registration.json");
    let replay = RegistrationReplay {
        class_id,
        artifact_root,
        blocks: blocks.iter().map(RpcRawBlock::from).collect(),
        root,
        fork: fork.iter().map(RpcRawBlock::from).collect(),
        fork_root,
        control,
        economics,
    };
    serde_json::to_writer(File::create(&path).unwrap(), &replay).unwrap();
    let mut peers = Vec::new();
    for _ in 0..2 {
        let peer = Command::new(std::env::current_exe().unwrap())
            .args(["private_moe2_three_process_registration", "--test-threads=1", "--nocapture"])
            .env(PEER_INPUT, &path)
            .env_remove("MISAKA_PRIVATE_TB1_TIR_ARTIFACT")
            .spawn()
            .unwrap();
        peers.push(peer);
    }
    for mut peer in peers {
        assert!(peer.wait().unwrap().success());
        let receipt = File::open(dir.path().join(format!("{}.json", peer.id()))).unwrap();
        let roots: (Hash64, Hash64, moe2_reservation::Observation) = serde_json::from_reader(receipt).unwrap();
        assert_eq!(roots, (root, fork_root, budget_observation));
    }
}
