//! **The 2026-09-26 testnet-12 IBD stall, on a real round lane through the real pipeline.**
//!
//! From DAA 316 no fresh node could finish IBD: every attempt died on `block has missing parents`
//! naming a round block, and every block named was one whose child in the lane sorts before it by
//! hash. ADR-0125 is why: a round block is never a selected parent and never blue, so a lane of round
//! blocks hanging from one anchor carries ONE blue work — the anchor's plus the anchor's own work —
//! and `(blue_work, hash)`, which upstream reads as a topological order, is hash order along the
//! lane. The syncer's header batches (`get_hashes_between`) and the syncing node's own body requests
//! (`get_missing_block_body_hashes`) were both built from it.
//!
//! Here a lane is mined whose every block sorts before its parent (the nonce is ground for it, so the
//! test does not depend on luck), and a fresh node is synced from the chain three ways: in the order
//! the un-upgraded fleet serves it (fails, as on testnet-12), in the order this build serves and
//! requests it (lands), and in the fleet's order through the syncing-side hold-back (lands). The
//! consensus order the merging block accepts the lane in is pinned as it is: this is a node fix.
use super::{TestContext, adr0125_config, adr0125_harness_bond, adr0125_round_block};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::topological_order::{is_parent_first, release_parent_first};
use std::ops::Deref;
use std::thread::JoinHandle;

/// A fresh node, synced by hand.
struct Syncee {
    consensus: TestConsensus,
    handles: Vec<JoinHandle<()>>,
}

impl Syncee {
    fn new(config: &Config) -> Self {
        let consensus = TestConsensus::new(config);
        let handles = consensus.init();
        Self { consensus, handles }
    }

    /// Headers, one at a time and each awaited: a block reaching the pipeline before its parent is
    /// refused deterministically, where a concurrent hand-over would only lose a race most times.
    async fn headers(&self, source: &TestConsensus, order: &[BlockHash]) -> Result<(), RuleError> {
        for hash in order {
            let header = source.get_header(*hash).expect("the source holds every header it lists");
            self.consensus.validate_and_insert_block(Block::from_header_arc(header)).virtual_state_task.await?;
        }
        Ok(())
    }

    /// Full blocks for headers this node already holds — the body phase.
    async fn bodies(&self, source: &TestConsensus, order: &[BlockHash]) -> Result<(), RuleError> {
        for hash in order {
            let block = source.get_block(*hash).expect("the source holds every body it lists");
            self.consensus.validate_and_insert_block(block).virtual_state_task.await?;
        }
        Ok(())
    }
}

impl Drop for Syncee {
    fn drop(&mut self) {
        self.consensus.shutdown(std::mem::take(&mut self.handles));
    }
}

/// The selected chain from genesis (exclusive) to the sink (inclusive).
fn selected_chain(source: &TestConsensus) -> Vec<BlockHash> {
    let genesis = source.params().genesis.hash;
    let mut chain = Vec::new();
    let mut current = source.get_sink();
    while current != genesis {
        chain.push(current);
        current = source.ghostdag_store().get_selected_parent(current).unwrap();
    }
    chain.reverse();
    chain
}

/// **What the fleet serves**: `antipast_hashes_between(genesis, sink)` as it stood before this fix —
/// each chain block's mergeset in consensus order, as is, and the sink last.
fn as_the_fleet_serves_it(source: &TestConsensus) -> Vec<BlockHash> {
    let genesis = source.params().genesis.hash;
    let store = source.ghostdag_store();
    let mut order = Vec::new();
    for chain_block in selected_chain(source) {
        let data = store.get_data(chain_block).unwrap();
        order.extend(data.consensus_ordered_mergeset(store.deref()).filter(|hash| *hash != genesis));
    }
    order.push(source.get_sink());
    order
}

fn parent_first(source: &TestConsensus, order: &[BlockHash]) -> bool {
    is_parent_first(order, |hash| *hash, |hash| source.get_header(*hash).unwrap().direct_parents().to_vec())
}

/// A round block of `round` that extends the lane from `parent` and sorts BEFORE it.
fn round_block_sorting_before(
    ctx: &TestContext,
    config: &Config,
    round: u64,
    payout: &kaspa_consensus_core::tx::ScriptPublicKey,
    parent: BlockHash,
) -> MutableBlock {
    for nonce in 0..256 {
        let block = adr0125_round_block(ctx, config, round, 0, payout.clone(), nonce);
        assert_eq!(block.header.direct_parents(), &[parent], "the lane extends itself from its tip alone");
        if block.header.hash < parent {
            return block;
        }
    }
    panic!("256 nonces without a hash below the parent's");
}

#[tokio::test]
async fn ibd_across_a_round_lane_whose_blocks_tie_on_blue_work() {
    let (config, bundle) = adr0125_config();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let sink0 = ctx.consensus.get_sink();
    let anchor = vp.ghostdag_store.get_selected_parent(sink0).unwrap();
    let genesis_ts = config.params.genesis.timestamp;
    let first_round = {
        let r = (vp.headers_store.get_timestamp(sink0).unwrap() - genesis_ts) / 1_000 + 2;
        r + r % 2
    };
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let payout = p2pkh_mldsa87_spk(state.bond(&adr0125_harness_bond()).expect("row 0 is registered").payout_payload.as_byte_slice());

    // The lane: r0 hangs from the anchor, each next one names only the previous one — and sorts
    // before it by hash, which is the order `(blue_work, hash)` then puts them in.
    const LANE: usize = 6;
    let mut lane: Vec<BlockHash> = Vec::with_capacity(LANE);
    let r0 = adr0125_round_block(&ctx, &config, first_round, 0, payout.clone(), 0);
    assert_eq!(r0.header.direct_parents(), &[anchor]);
    lane.push(r0.header.hash);
    ctx.consensus.validate_and_insert_block(r0.to_immutable()).virtual_state_task.await.expect("a signed round block is valid");
    for i in 1..LANE {
        let round = first_round + 2 * i as u64;
        let block = round_block_sorting_before(&ctx, &config, round, &payout, *lane.last().unwrap());
        lane.push(block.header.hash);
        ctx.consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.expect("valid");
    }

    // The root cause, on the real rule: one anchor, one blue work, the whole lane.
    let lane_work = vp.ghostdag_store.get_blue_work(lane[0]).unwrap();
    for hash in lane.iter() {
        assert_eq!(vp.ghostdag_store.get_selected_parent(*hash).unwrap(), anchor, "every lane block's selected parent is the anchor");
        assert_eq!(vp.ghostdag_store.get_blue_work(*hash).unwrap(), lane_work, "and so every lane block weighs the same");
    }

    // A chain block merges the lane, and the chain goes on.
    let last_round = first_round + 2 * (LANE as u64 - 1);
    ctx.simulated_time = ctx.simulated_time.max(genesis_ts + last_round * 1_000) + config.params.target_time_per_block();
    let merging = ctx.build_block_template(11, ctx.simulated_time);
    assert!(merging.block.header.direct_parents().contains(lane.last().unwrap()), "virtual offers the lane tip as a parent");
    let merging_hash = merging.block.header.hash;
    ctx.validate_and_insert_block(merging.block.to_immutable()).await.assert_valid_utxo_tip();
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let source = &ctx.consensus;
    let sink = source.get_sink();
    let genesis = config.params.genesis.hash;

    // CONSENSUS, pinned as it is: the merging block accepts the lane in (blue_work, hash) order —
    // here exactly backwards. Changing that order changes what a block accepts; it is not this fix.
    let merging_data = vp.ghostdag_store.get_data(merging_hash).unwrap();
    let acceptance_order: Vec<BlockHash> = merging_data
        .consensus_ordered_mergeset_without_selected_parent(vp.ghostdag_store.deref())
        .filter(|hash| lane.contains(hash))
        .collect();
    let mut lane_backwards = lane.clone();
    lane_backwards.reverse();
    assert_eq!(acceptance_order, lane_backwards, "the consensus order of a tied lane is hash order, untouched by this fix");

    // What the fleet serves is not parents-first; what this build serves is — the same blocks.
    let fleet_order = as_the_fleet_serves_it(source);
    assert!(!parent_first(source, &fleet_order), "the un-upgraded order sends a lane child before its parent");
    let (served, highest) = source.get_hashes_between(genesis, sink, 1 << 20).unwrap();
    assert_eq!(highest, sink);
    assert!(parent_first(source, &served), "get_hashes_between is parents-first");
    let mut fleet_sorted = fleet_order.clone();
    fleet_sorted.sort();
    let mut served_sorted = served.clone();
    served_sorted.sort();
    assert_eq!(fleet_sorted, served_sorted, "only the order changed");

    // 1. As on testnet-12: the fleet's order fails the header phase on a lane block.
    let before = Syncee::new(&config);
    match before.headers(source, &fleet_order).await {
        Err(RuleError::MissingParents(missing)) => {
            assert!(missing.iter().all(|parent| lane.contains(parent)), "the missing parent is a round block: {missing:?}")
        }
        other => panic!("the fleet's header order must fail with MissingParents, got {other:?}"),
    }

    // 2. This build, as a syncer and as a syncee: headers as served, then the node's own body list.
    let after = Syncee::new(&config);
    after.headers(source, &served).await.expect("parents-first headers all land");
    // Before the fix the body list was the fleet's order too: the phase dies on a lane block.
    let fleet_bodies: Vec<BlockHash> = fleet_order.iter().copied().filter(|hash| *hash != sink).collect();
    match after.bodies(source, &fleet_bodies).await {
        Err(RuleError::MissingParents(missing)) => assert!(missing.iter().all(|parent| lane.contains(parent)), "{missing:?}"),
        other => panic!("the fleet's body order must fail with MissingParents, got {other:?}"),
    }
    // The node's own list now: parents-first, and every body lands.
    let missing_bodies = after.consensus.get_missing_block_body_hashes(sink).unwrap();
    assert!(!missing_bodies.is_empty());
    assert!(parent_first(source, &missing_bodies), "get_missing_block_body_hashes is parents-first");
    assert_eq!(missing_bodies.last(), Some(&sink), "the list runs to the sink");
    after.bodies(source, &missing_bodies).await.expect("parents-first bodies all land");
    assert_eq!(after.consensus.get_sink(), sink, "the synced node's sink is the source's");
    assert_eq!(
        after.consensus.block_status(sink),
        BlockStatus::StatusUTXOValid,
        "and the state it computed is the one the sink's header commits to"
    );

    // 3. This build as a syncee of the UN-upgraded fleet: the fleet's order, in chunks that split
    //    the lane, through the hold-back — every header lands.
    let held_back = Syncee::new(&config);
    let mut held = Vec::new();
    for chunk in fleet_order.chunks(3) {
        let mut batch = std::mem::take(&mut held);
        batch.extend(chunk.iter().map(|hash| source.get_header(*hash).unwrap()));
        let (released, waiting) = release_parent_first(batch, |parent| held_back.consensus.get_block_status(parent).is_some());
        held = waiting;
        for header in released {
            held_back
                .consensus
                .validate_and_insert_block(Block::from_header_arc(header))
                .virtual_state_task
                .await
                .expect("a released header's parents are all in");
        }
    }
    assert!(held.is_empty(), "nothing waits once the stream is through");
    let missing_bodies = held_back.consensus.get_missing_block_body_hashes(sink).unwrap();
    held_back.bodies(source, &missing_bodies).await.expect("and the body phase lands");
    assert_eq!(held_back.consensus.get_sink(), sink);
    assert_eq!(held_back.consensus.block_status(sink), BlockStatus::StatusUTXOValid);
}
