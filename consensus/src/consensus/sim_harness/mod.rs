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
//! Scope: the DNS overlay vertical (bond -> attestation -> confirmed anchor). The PALW vertical and
//! the EVM feature are deliberately out of scope here.
//!
//! Scenarios:
//! * [`three_node_smoke`] — three nodes, no cut: convergence, one full overlay lifecycle,
//!   determinism.
//! * [`partition_stalemate`] — a healed network partition that does NOT reconverge.
//!
//! On `partition_stalemate` and what it asserts: with the code as it stands today, a healed
//! partition between a high-work branch carrying no attestations and a lower-work branch holding the
//! DNS-confirmed anchor is a PERMANENT STALEMATE, and that is the DESIGNED consequence — the reorg
//! gate (`check_dns_reorg_rule`) has no partition-downgrade path, so every candidate from the
//! work-heavy branch is a `DominanceViolation` forever. The test therefore pins the stalemate as the
//! expected outcome. Should upstream ever implement such a downgrade path, this test must be
//! INVERTED (expect reconvergence) and kept as its regression test — it must not be deleted, and it
//! must not be "fixed" by relaxing the asserts.

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
use kaspa_utils::sim::{PartitionMode, PartitionWindow, Simulation, Topology};

use crate::config::Config;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use actors::{AnchorProbeActor, MinerActor, MinerCfg, ObserverActor, Payout, ValidatorActor, ValidatorCfg};
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
///
/// `require_anchor_attestation` is the one DNS knob a scenario chooses. The DEVNET preset inherited
/// here leaves it `false` (legacy semantics: depth alone confirms), which the smoke run keeps. A
/// scenario that forks a branch carrying NO validator must pass `true` — the testnet-22 semantics —
/// otherwise the validator-less branch keeps advancing its own confirmed anchor unopposed, which is
/// not what the live network does.
fn sim_config(require_anchor_attestation: bool) -> Arc<Config> {
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
                dns.require_anchor_attestation = require_anchor_attestation;
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
    confirmed_anchor_daa_score: u64,
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
        confirmed_anchor_daa_score: dns_state.last_dns_confirmed_anchor_daa_score,
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
    let config = sim_config(false);
    let genesis = config.genesis.hash;
    let vseed = validator_seed(seed);

    let nodes: [Arc<SimNode>; 3] = std::array::from_fn(|_| Arc::new(SimNode::new(config.clone())));

    let mut sim: Simulation<SimMsg> = Simulation::with_start_time(LINK_DELAY_MS, config.genesis.timestamp);
    sim.set_topology(topology);
    let miner_cfg = MinerCfg {
        id: MINER_ID,
        start_delay_ms: BLOCK_INTERVAL_MS,
        interval_ms: BLOCK_INTERVAL_MS,
        mine_end: u64::MAX,
        max_blocks: NUM_BLOCKS,
        payout: Payout::ValidatorFirstN(FUNDING_BLOCKS),
        assert_confirmed_on_start: false,
    };
    sim.register(MINER_ID, Box::new(MinerActor::new(nodes[0].clone(), vseed, miner_cfg)));
    // `harvest_cap: 2` (bond + one shard) keeps the smoke scenario at exactly one attestation.
    let validator_cfg = ValidatorCfg { id: VALIDATOR_ID, miner_id: MINER_ID, harvest_cap: 2 };
    sim.register(VALIDATOR_ID, Box::new(ValidatorActor::new(nodes[1].clone(), config.clone(), vseed, validator_cfg)));
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

// ---------------------------------------------------------------------------------------------
// Scenario: a network partition that the DNS reorg gate turns into a permanent stalemate.
// ---------------------------------------------------------------------------------------------

/// Simulated node ids of the partition scenario.
const A_MINER_ID: u64 = 0;
const B_MINER_ID: u64 = 1;
const B_VALIDATOR_ID: u64 = 2;
const A_OBSERVER_ID: u64 = 3;
/// A send-nothing probe (see [`AnchorProbeActor`]); it takes part in no partition group.
const ANCHOR_PROBE_ID: u64 = 4;

/// Virtual ms of shared prefix: branch B mines alone, bonds, attests and confirms an anchor before
/// anything forks. `assert_confirmed_on_start` on miner A turns "the prefix was too short" into an
/// immediate, self-describing failure instead of a puzzling end-of-run assert.
const PREFIX_MS: u64 = 40_000;
/// Virtual time (offset from start) at which the cut heals and the deferred blocks burst through.
/// The 10s cut length is a WALL-CLOCK budget decision, not a semantic one: post-heal blocks carry
/// huge merge sets, so the test's real runtime grows superlinearly with the cut (measured: 20s cut
/// -> 149s, 15s -> 76s, 10s -> 37s). 10s still leaves branch A 25 blocks against branch B's 10.
const HEAL_MS: u64 = 50_000;
/// Virtual time (offset from start) at which both miners stop, so the run drains naturally. The
/// 5s tail after the heal is what makes the two sides actually meet: each side hears the other's
/// burst and keeps mining, and branch B keeps attesting on its own branch.
const RUN_END_MS: u64 = 55_000;
/// Branch A's mining interval — 2.5x branch B's, so A's branch wins on accumulated blue work by a
/// wide margin. The whole point of the scenario is that winning on work is NOT enough.
const A_INTERVAL_MS: u64 = 400;

/// Everything the partition asserts compare — and the whole of what the determinism assert
/// re-compares on a second run.
#[derive(Debug, PartialEq, Eq)]
struct PartitionOutcome {
    /// Miner A's node (the high-work, validator-less branch).
    a: NodeOutcome,
    /// Miner B's node (the branch carrying the validator, hence the DNS-confirmed anchors).
    b: NodeOutcome,
    validator_b: NodeOutcome,
    observer_a: NodeOutcome,
    /// Blue work of BOTH sinks as read from node B's ghostdag store — node B is the one that has
    /// seen both branches, so it is the only node that can compare them.
    a_sink_blue_work_on_b: BlueWorkType,
    b_sink_blue_work_on_b: BlueWorkType,
    /// Whether node B has A's sink at all. Distinguishes "the gate refused a reorg it saw" from
    /// "the blocks never arrived", which would make the stalemate assert vacuous.
    b_knows_a_sink: bool,
    /// Node B's confirmed-anchor DAA score sampled at heal time (before the burst arrives).
    b_anchor_daa_at_heal: u64,
}

/// Runs the partition scenario: a `HEAL_MS - PREFIX_MS` cut between {miner A, observer A} and
/// {miner B, validator B}, laid over a shared prefix on which the DNS overlay already confirmed an
/// anchor.
///
/// Note on the window bounds: the simulation clock starts at the GENESIS TIMESTAMP, so every
/// absolute virtual time here is `genesis_ts + offset`. A window built from bare offsets would sit
/// entirely in the past and never cut anything.
fn run_partition(seed: u64) -> PartitionOutcome {
    // `require_anchor_attestation: true` is load-bearing, not decoration: with it, branch A — which
    // has no validator — cannot advance its own confirmed anchor past the fork, so A's latch stays
    // frozen on the shared prefix and A's own gate waves its blocks through. With `false`, branch A
    // would confirm anchors unopposed and the scenario would stop resembling the live network.
    let config = sim_config(true);
    let genesis = config.genesis.hash;
    let start = config.genesis.timestamp;
    let vseed = validator_seed(seed);

    let nodes: [Arc<SimNode>; 4] = std::array::from_fn(|_| Arc::new(SimNode::new(config.clone())));

    let topology = Topology::new(LINK_DELAY_MS).with_partition(PartitionWindow {
        start: start + PREFIX_MS,
        end: start + HEAL_MS,
        groups: vec![[A_MINER_ID, A_OBSERVER_ID].into_iter().collect(), [B_MINER_ID, B_VALIDATOR_ID].into_iter().collect()],
        // Nothing is lost: every block sent across the cut is delivered at heal, in send order, so
        // parents precede children and no node is left with orphans.
        mode: PartitionMode::DelayUntilHeal,
    });

    let mut sim: Simulation<SimMsg> = Simulation::with_start_time(LINK_DELAY_MS, start);
    sim.set_topology(topology);

    // Branch A: fast, and pays its coinbases to nobody the overlay knows.
    sim.register(
        A_MINER_ID,
        Box::new(MinerActor::new(
            nodes[0].clone(),
            vseed,
            MinerCfg {
                id: A_MINER_ID,
                start_delay_ms: PREFIX_MS,
                interval_ms: A_INTERVAL_MS,
                mine_end: start + RUN_END_MS,
                max_blocks: u64::MAX,
                payout: Payout::Never,
                assert_confirmed_on_start: true,
            },
        )),
    );
    // Branch B: slower, but pays the validator on every block so it can keep attesting forever.
    sim.register(
        B_MINER_ID,
        Box::new(MinerActor::new(
            nodes[1].clone(),
            vseed,
            MinerCfg {
                id: B_MINER_ID,
                start_delay_ms: BLOCK_INTERVAL_MS,
                interval_ms: BLOCK_INTERVAL_MS,
                mine_end: start + RUN_END_MS,
                max_blocks: u64::MAX,
                payout: Payout::ValidatorAlways,
                assert_confirmed_on_start: false,
            },
        )),
    );
    sim.register(
        B_VALIDATOR_ID,
        Box::new(ValidatorActor::new(
            nodes[2].clone(),
            config.clone(),
            vseed,
            ValidatorCfg { id: B_VALIDATOR_ID, miner_id: B_MINER_ID, harvest_cap: usize::MAX },
        )),
    );
    sim.register(A_OBSERVER_ID, Box::new(ObserverActor::new(nodes[3].clone())));

    let anchor_at_heal = Arc::new(std::sync::Mutex::new(None));
    sim.register(ANCHOR_PROBE_ID, Box::new(AnchorProbeActor::new(nodes[1].clone(), HEAL_MS, anchor_at_heal.clone())));

    sim.run(u64::MAX - 1);

    let a = node_outcome(&nodes[0], genesis);
    let b = node_outcome(&nodes[1], genesis);
    let validator_b = node_outcome(&nodes[2], genesis);
    let observer_a = node_outcome(&nodes[3], genesis);
    let ghostdag_b = &nodes[1].consensus.storage.ghostdag_store;
    let outcome = PartitionOutcome {
        a_sink_blue_work_on_b: ghostdag_b.get_blue_work(a.sink).expect("node B knows branch A's sink after the heal"),
        b_sink_blue_work_on_b: ghostdag_b.get_blue_work(b.sink).expect("node B knows its own sink"),
        b_knows_a_sink: nodes[1].consensus.get_block_status(a.sink).is_some_and(|s| s.is_valid()),
        b_anchor_daa_at_heal: anchor_at_heal.lock().unwrap().expect("the probe fired at heal time"),
        a,
        b,
        validator_b,
        observer_a,
    };
    for node in &nodes {
        node.shutdown();
    }
    outcome
}

/// Index of the first block on which the two chains disagree — i.e. the length of the shared prefix.
fn fork_index(a_chain: &[BlockHash], b_chain: &[BlockHash]) -> usize {
    a_chain.iter().zip(b_chain).position(|(x, y)| x != y).unwrap_or_else(|| a_chain.len().min(b_chain.len()))
}

/// A partition heals, but the chains do not: branch A wins on work, branch B holds the DNS-confirmed
/// anchor, and the DNS reorg gate refuses to let A's branch displace it. Both sides keep their own
/// sink for good.
///
/// **This test pins CURRENT behaviour, and current behaviour is the designed outcome**: the reorg
/// gate has no partition-downgrade path, so a heal between a high-work branch with no attestations
/// and a lower-work branch holding the confirmed anchor is a permanent stalemate (a
/// `DominanceViolation` on every candidate). Should upstream ever implement such a downgrade path,
/// this test must be INVERTED (expect convergence) and kept as the regression test for it — not
/// deleted.
///
/// Diagnostics: run with `--nocapture` and read node B's `[dns-reorg-gate]` warnings — they are the
/// direct evidence that B saw branch A and refused it. The asserts deliberately do not count log
/// lines; they pin the same fact from the outside (B knows A's sink, B's sink stays off A's chain,
/// and A's sink carries strictly more blue work).
#[test]
fn partition_stalemate() {
    kaspa_core::log::try_init_logger("info");
    let first = run_partition(11);
    let a_chain = &first.a.chain;
    let b_chain = &first.b.chain;
    let fork = fork_index(a_chain, b_chain);
    assert!(fork > 0, "the two branches must share a prefix");
    assert!(fork < a_chain.len() && fork < b_chain.len(), "the two branches must actually diverge");

    // (a) branch A keeps its own sink, and its observer agrees with it.
    assert_eq!(Some(&first.a.sink), a_chain.last(), "node A's sink must be the tip of node A's own chain");
    assert!(!b_chain.contains(&first.a.sink), "node B must NOT have adopted branch A's sink");
    assert_eq!(first.observer_a.sink, first.a.sink, "the observer on branch A must agree with miner A");
    assert_eq!(first.observer_a.chain, first.a.chain, "the observer on branch A must agree with miner A's chain");

    // (b) the stalemate is a REFUSAL, not a delivery failure: node B has branch A's sink, has more
    // work available on it (c), and still did not move. If the gate ever stops firing, GHOSTDAG
    // moves node B's sink onto branch A and this assert fails — which is exactly the canary.
    assert!(!a_chain.contains(&first.b.sink), "node A must NOT have adopted branch B's sink");
    assert!(first.b_knows_a_sink, "node B must have received branch A's sink (otherwise the stalemate is vacuous)");

    // (c) branch A is the strictly heavier branch, judged by the node that has seen both.
    assert!(
        first.a_sink_blue_work_on_b > first.b_sink_blue_work_on_b,
        "branch A must carry strictly more blue work ({} vs {})",
        first.a_sink_blue_work_on_b,
        first.b_sink_blue_work_on_b
    );

    // (d) the two sides latched different anchors: B's confirmed anchor advanced onto its own
    // post-fork branch, A's stayed frozen on the shared prefix (no validator ever attested on A).
    let b_anchor_pos = b_chain.iter().position(|h| *h == first.b.confirmed_anchor).expect("node B's anchor is on node B's chain");
    assert!(b_anchor_pos >= fork, "node B's confirmed anchor must have moved past the fork (pos {b_anchor_pos}, fork {fork})");
    let a_anchor_pos = a_chain.iter().position(|h| *h == first.a.confirmed_anchor).expect("node A's anchor is on node A's chain");
    assert!(a_anchor_pos < fork, "node A's confirmed anchor must stay on the shared prefix (pos {a_anchor_pos}, fork {fork})");
    assert!(
        first.b.confirmed_anchor_daa_score > first.a.confirmed_anchor_daa_score,
        "node B's anchor must be strictly ahead of node A's frozen one ({} vs {})",
        first.b.confirmed_anchor_daa_score,
        first.a.confirmed_anchor_daa_score
    );
    assert_eq!(first.validator_b.sink, first.b.sink, "the validator must stay on its own miner's branch");

    // (d') the overlay is still LIVE on branch B after the heal: the validator keeps attesting and
    // B's confirmed anchor keeps advancing — the stalemate freezes the reorg, not the branch.
    assert!(
        first.b.confirmed_anchor_daa_score > first.b_anchor_daa_at_heal,
        "node B's confirmed anchor must keep advancing after the heal ({} at heal, {} at the end)",
        first.b_anchor_daa_at_heal,
        first.b.confirmed_anchor_daa_score
    );

    // (e) determinism: virtual time only, so the whole two-branch outcome is reproducible.
    let second = run_partition(11);
    assert_eq!(first, second, "the same seed must reproduce the same partitioned run");
}
