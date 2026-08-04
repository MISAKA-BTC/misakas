//! A multi-node consensus smoke harness driven by the virtual-time simulator
//! (`kaspa_utils::sim`).
//!
//! Real `Consensus` instances (one per simulated node, each on its own temp DB) exchange blocks and
//! overlay txs as simulator messages over a `Topology`. Everything that would otherwise read the wall
//! clock is replaced by simulated time — block header timestamps come from `env.now()`, and the
//! simulation clock starts at the genesis timestamp so headers satisfy both the past-median and the
//! future-time header checks. That makes a run a pure function of its inputs: the same seed yields the
//! same sink, the same virtual chain and the same confirmed DNS anchor, which is what makes the
//! determinism assert a real canary rather than a tautology.
//!
//! Scope: the DNS overlay vertical (bond -> attestation -> confirmed anchor) over three nodes. The
//! PALW vertical and the EVM feature are deliberately out of scope here.

mod actors;
mod node;

use std::sync::Arc;

use kaspa_consensus_core::BlueWorkType;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::{ConfigBuilder, params::SIMNET_PARAMS};
use kaspa_consensus_core::dns_finality::DnsRolloutStage;
use kaspa_consensus_core::tx::Transaction;
use kaspa_consensus_core::{BlockHash, Hash64};
use kaspa_utils::sim::{Simulation, Topology};

use crate::config::Config;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use actors::{MinerActor, ObserverActor, ValidatorActor};
use node::SimNode;

/// The message type exchanged by the simulated nodes.
#[derive(Clone)]
pub(super) enum SimMsg {
    Block(Block),
    Tx(Transaction),
}

/// Simulated node ids.
const MINER_ID: u64 = 0;
const VALIDATOR_ID: u64 = 1;
const OBSERVER_ID: u64 = 2;

/// Blocks the miner produces per run. Sized for the DNS vertical: funding, maturity, the bond, the
/// blue-score burial before attesting, the shard and the confirmation tail — with margin.
const NUM_BLOCKS: u64 = 60;
/// Coinbases of the first this-many blocks pay the harness validator.
const FUNDING_BLOCKS: u64 = 6;
/// Virtual milliseconds between mining attempts.
const BLOCK_INTERVAL_MS: u64 = 1000;
/// Default link delay of the simulated network, in virtual milliseconds.
const LINK_DELAY_MS: u64 = 500;
/// Coinbase maturity used by the harness params.
const COINBASE_MATURITY: u64 = 2;
/// Blocks the validator lets bury its accepted bond before it attests (blue-score epochs must be
/// ready and the bond active).
const BOND_BURIAL_BLOCKS: u64 = 8;

/// Deterministically derives the harness validator's ML-DSA-87 seed from the run seed.
fn validator_seed(seed: u64) -> [u8; 32] {
    let mut out = [0x42u8; 32];
    out[..8].copy_from_slice(&seed.to_le_bytes());
    out
}

/// Harness params: SIMNET with PoW skipped, the `dns_v3_validator_drives_confirmed_anchor` DNS recipe
/// (so a single bonded validator plus one attested epoch confirms an anchor), and a short coinbase
/// maturity. PALW parameters are deliberately left at their preset values — the PALW vertical is not
/// part of this scenario and its fences must stay where the presets put them.
fn sim_config() -> Arc<Config> {
    Arc::new(
        ConfigBuilder::new(SIMNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| {
                p.coinbase_maturity = COINBASE_MATURITY;
                use kaspa_consensus_core::dns_finality::{STAKE_SCORE_SCALE, StakeScore};
                let mut dns = kaspa_consensus_core::config::params::DEVNET_PARAMS.dns_params.clone().unwrap();
                dns.dns_activation_daa_score = 0;
                dns.pos_v2_activation_daa_score = 0;
                dns.epoch_length_blocks = 2;
                dns.reward_uniqueness_window_blocks = 50;
                dns.max_reorg_horizon_blocks = 2;
                dns.attestation_epoch_length_blue_score = 3;
                dns.attestation_lag_blue_score = 2;
                dns.attestation_anchor_backoff_blue_score = 1;
                dns.stake_score_window_blue_score = 10_000;
                dns.required_work_depth = kaspa_consensus_core::BlueWorkType::ZERO;
                dns.required_stake_depth = StakeScore(STAKE_SCORE_SCALE / 2);
                // A single validator cannot attest every epoch; zero floors keep DnsHealth::Active
                // (the health thresholds have their own dedicated tests).
                dns.stake_event_quality_floor_bps = 0;
                dns.stake_censorship_floor_bps = 0;
                p.dns_params = Some(dns);
            })
            .build(),
    )
}

/// Per-node observable outcome of a run — everything the asserts compare across nodes and across
/// repeated runs.
#[derive(Debug, PartialEq, Eq)]
struct NodeOutcome {
    sink: BlockHash,
    chain: Vec<BlockHash>,
    confirmed_anchor: Hash64,
    rollout_stage: DnsRolloutStage,
    sink_blue_work: BlueWorkType,
    sink_blue_score: u64,
}

fn node_outcome(node: &SimNode, genesis: BlockHash) -> NodeOutcome {
    let sink = node.sink();
    let chain = node.consensus.get_virtual_chain_from_block(genesis, None).expect("virtual chain from genesis").added;
    let dns_state = node.dns_state();
    let sink_blue_work = node.consensus.storage.ghostdag_store.get_blue_work(sink).unwrap();
    let sink_blue_score = node.consensus.storage.headers_store.get_blue_score(sink).unwrap();
    NodeOutcome {
        sink,
        chain,
        confirmed_anchor: dns_state.last_dns_confirmed_anchor,
        rollout_stage: dns_state.rollout_stage,
        sink_blue_work,
        sink_blue_score,
    }
}

/// Runs one full simulation and returns the per-node outcomes, indexed by node id.
///
/// `topology` is a parameter so a later step can inject link delays / partition windows without
/// touching the actors; this step always passes a uniform-delay topology.
fn run_once(seed: u64, topology: Topology) -> [NodeOutcome; 3] {
    let config = sim_config();
    let genesis = config.genesis.hash;
    let vseed = validator_seed(seed);

    let nodes: [Arc<SimNode>; 3] = std::array::from_fn(|_| Arc::new(SimNode::new(config.clone())));

    let mut sim: Simulation<SimMsg> = Simulation::with_start_time(LINK_DELAY_MS, config.genesis.timestamp);
    sim.set_topology(topology);
    sim.register(MINER_ID, Box::new(MinerActor::new(nodes[0].clone(), vseed)));
    sim.register(VALIDATOR_ID, Box::new(ValidatorActor::new(nodes[1].clone(), config.clone(), vseed)));
    sim.register(OBSERVER_ID, Box::new(ObserverActor::new(nodes[2].clone())));
    // The event queue drains once every actor is idle; `until` is only a safety bound.
    sim.run(u64::MAX - 1);

    let outcomes = std::array::from_fn(|i| node_outcome(&nodes[i], genesis));
    for node in &nodes {
        node.shutdown();
    }
    outcomes
}

fn default_topology() -> Topology {
    Topology::new(LINK_DELAY_MS)
}

/// Three real consensus nodes, one virtual clock: they must converge on the same chain, drive the
/// DNS overlay to a confirmed anchor, and reproduce the run exactly on a re-run.
#[test]
fn three_node_smoke() {
    kaspa_core::log::try_init_logger("info");
    let first = run_once(7, default_topology());

    // (a) convergence: identical sink and identical virtual chain on all three nodes.
    for (id, outcome) in [(VALIDATOR_ID, &first[1]), (OBSERVER_ID, &first[2])] {
        assert_eq!(first[0].sink, outcome.sink, "node {id} must agree with the miner on the sink");
        assert_eq!(first[0].chain, outcome.chain, "node {id} must agree with the miner on the virtual chain");
    }
    assert_eq!(first[0].chain.len() as u64, NUM_BLOCKS, "every mined block must join the selected chain");

    // (c) the DNS overlay vertical completed, identically, on every node.
    for (id, outcome) in first.iter().enumerate() {
        assert_eq!(outcome.rollout_stage, DnsRolloutStage::Active, "node {id} must reach DnsRolloutStage::Active");
        assert_ne!(outcome.confirmed_anchor, Hash64::default(), "node {id} must have a confirmed DNS anchor");
    }

    // (b) determinism: the same seed reproduces the run exactly. Header timestamps are virtual, so
    // block hashes are a function of the simulated content only — this compares real hashes.
    let second = run_once(7, default_topology());
    assert_eq!(first, second, "the same seed must reproduce the same run");
}
