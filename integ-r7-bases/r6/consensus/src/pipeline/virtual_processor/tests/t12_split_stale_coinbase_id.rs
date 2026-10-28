//! **The 2026-09-26 testnet-12 split at DAA 198, reproduced on three consensus instances.**
//!
//! b6 (169.58.232.113) produced `0e118a39…` from a block template its heartbeat miner's request had
//! cached. The mining cache retargeted the coinbase to the producer (`modify_block_template`: new
//! payload, cached `Transaction::id` untouched) and the producer submitted the in-memory block, so
//! b6 stored the coinbase under `44da7820…` — the id of the heartbeat's coinbase bytes — while every
//! peer rebuilt the transaction from the relayed bytes and stored `a70ead4e…`. The block itself was
//! valid everywhere: its own `utxo_commitment` does not cover its own coinbase, and
//! `hash_merkle_root` hashes bytes, never the cached id. The first chain block to merge it
//! (`60878caf…`, built by the majority) committed the coinbase output at `(a70ead4e…, 0)`; b6
//! computed `(44da7820…, 0)`, found `22df6da0…` instead of `6570108d…`, disqualified it and extended
//! its own branch — whose first block, `c8e772df…`, carried `22df6da0…` and was refused by every
//! other node.
//!
//! Replayed here on `producer` (b6: receives the block as the in-memory object its own lane built),
//! `peer` (a majority node: receives it the way P2P hands it over) and `newcomer` (a node that has
//! seen only the shared history, `x` and b6's own merge of it). The network is the mainnet preset
//! because nothing here is PALW-specific: the defect is in the upstream mining path, and it became
//! reachable when an in-process producer skipped the byte round trip that recomputes ids. The fix
//! this file pins is the one `FlowContext::submit_rpc_block` now applies —
//! `Block::with_current_tx_ids` where an in-process block enters consensus — beside
//! `modify_block_template`'s own `finalize`.
use super::*;
use kaspa_consensus_core::blockstatus::BlockStatus;

/// The block as a peer receives it: every transaction rebuilt from its bytes, so every id is the
/// bytes' id — which is what the P2P and RPC conversions do (`Transaction::new*` finalizes).
fn off_the_wire(block: &Block) -> Block {
    let transactions = block
        .transactions
        .iter()
        .map(|tx| {
            Transaction::new_with_mass(
                tx.version,
                tx.inputs.clone(),
                tx.outputs.clone(),
                tx.lock_time,
                tx.subnetwork_id.clone(),
                tx.gas,
                tx.payload.clone(),
                tx.mass(),
            )
        })
        .collect();
    Block::new(block.header.as_ref().clone(), transactions).with_evm_payload(block.evm_payload.clone())
}

/// What b6's producer submitted: a template built for ANOTHER miner (the heartbeat miner asked
/// first, so its template is the cached one), retargeted to the producer exactly as
/// `BlockTemplateBuilder::modify_block_template` retargeted it before this fix — the payload
/// rewritten through the same consensus call, every current-miner output moved, the merkle root
/// recomputed, and the cached coinbase id left as it was. (The mining crate's
/// `a_template_served_from_another_miners_cache_carries_its_own_coinbase_id` pins the function.)
fn producer_block_from_a_heartbeat_template(ctx: &mut TestContext, heartbeat: &MinerData, producer: &MinerData) -> Block {
    ctx.simulated_time += ctx.consensus.params().target_time_per_block();
    let mut template = ctx
        .consensus
        .build_block_template(heartbeat.clone(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .unwrap();
    let coinbase = &mut template.block.transactions[0];
    coinbase.payload = ctx.consensus.modify_coinbase_payload(coinbase.payload.clone(), producer).unwrap();
    for &index in &template.coinbase_miner_script_output_indices {
        coinbase.outputs[index].script_public_key = producer.script_public_key.clone();
    }
    template.block.header.hash_merkle_root = ctx.consensus.calc_transaction_hash_merkle_root(&template.block.transactions);
    template.block.header.timestamp = ctx.simulated_time;
    template.block.header.nonce = ctx.simulated_time;
    template.block.header.finalize();
    template.block.to_immutable()
}

/// A plain block from `ctx`'s own template, built on its own sink.
fn block_from_own_template(ctx: &mut TestContext, miner: MinerData) -> Block {
    ctx.simulated_time += ctx.consensus.params().target_time_per_block();
    let mut template =
        ctx.consensus.build_block_template(miner, Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard).unwrap();
    template.block.header.timestamp = ctx.simulated_time;
    template.block.header.nonce = ctx.simulated_time;
    template.block.header.finalize();
    template.block.to_immutable()
}

async fn insert(ctx: &TestContext, block: Block) -> BlockStatus {
    ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap()
}

fn stored_coinbase(ctx: &TestContext, hash: BlockHash) -> Transaction {
    ctx.consensus.get_block(hash).unwrap().transactions[0].clone()
}

struct Replay {
    producer: TestContext,
    peer: TestContext,
    newcomer: TestContext,
    /// b6's block (`0e118a39…`).
    x: BlockHash,
    /// The majority's chain block that merges it (`60878caf…`).
    y: BlockHash,
    /// b6's own next block (`c8e772df…`).
    z: BlockHash,
}

async fn replay(submit_as_built: bool) -> Replay {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let mut producer = TestContext::new(TestConsensus::new(&config));
    let mut peer = TestContext::new(TestConsensus::new(&config));
    let newcomer = TestContext::new(TestConsensus::new(&config));

    // A short shared history, built by the peer and delivered to the others off the wire.
    for _ in 0..3 {
        let block = block_from_own_template(&mut peer, new_miner_data());
        assert_eq!(insert(&peer, block.clone()).await, BlockStatus::StatusUTXOValid);
        for other in [&producer, &newcomer] {
            assert_eq!(insert(other, off_the_wire(&block)).await, BlockStatus::StatusUTXOValid);
        }
    }
    producer.simulated_time = peer.simulated_time;

    // b6's block: built from the heartbeat miner's cached template, retargeted to the producer.
    let (heartbeat, producer_address) = (new_miner_data(), new_miner_data());
    let x_as_built = producer_block_from_a_heartbeat_template(&mut producer, &heartbeat, &producer_address);
    let x = x_as_built.hash();
    assert!(!x_as_built.transactions[0].id_is_current(), "the fixture must carry the stale id b6 carried");
    let x_on_producer = if submit_as_built { x_as_built.clone() } else { x_as_built.clone().with_current_tx_ids().0 };
    assert_eq!(x_on_producer.hash(), x, "re-deriving the ids never moves the block hash: the merkle root hashes bytes");
    // Valid on EVERY node: nothing in the block's own validation reads its own coinbase's id.
    assert_eq!(insert(&producer, x_on_producer).await, BlockStatus::StatusUTXOValid);
    for other in [&peer, &newcomer] {
        assert_eq!(insert(other, off_the_wire(&x_as_built)).await, BlockStatus::StatusUTXOValid);
    }
    peer.simulated_time = producer.simulated_time;

    // The majority's next chain block merges x; b6 receives it as its only candidate.
    let y_block = block_from_own_template(&mut peer, new_miner_data());
    let y = y_block.hash();
    assert_eq!(insert(&peer, y_block.clone()).await, BlockStatus::StatusUTXOValid);
    insert(&producer, off_the_wire(&y_block)).await;

    // b6's own next block, on whatever sink b6 now has.
    producer.simulated_time = peer.simulated_time;
    let z_block = block_from_own_template(&mut producer, producer_address);
    let z = z_block.hash();
    assert_eq!(insert(&producer, z_block.clone()).await, BlockStatus::StatusUTXOValid);
    insert(&peer, off_the_wire(&z_block)).await;

    // The newcomer meets b6's branch first where it can (z on x alone), then the majority's.
    let order = if z_block.header.direct_parents().contains(&y) { [&y_block, &z_block] } else { [&z_block, &y_block] };
    for block in order {
        insert(&newcomer, off_the_wire(block)).await;
    }
    Replay { producer, peer, newcomer, x, y, z }
}

/// **Before the fix: the same bytes, two stored ids, two chains — and the network is the side the
/// bytes agree with.**
#[tokio::test]
async fn t12_split_a_stale_coinbase_id_submitted_in_process_forks_its_producer_off_the_network() {
    let Replay { producer, peer, newcomer, x, y, z } = replay(true).await;

    let (on_producer, on_peer) = (stored_coinbase(&producer, x), stored_coinbase(&peer, x));
    assert_ne!(on_producer.id(), on_peer.id(), "b6 stored 44da7820… for 0e118a39's coinbase; every other node a70ead4e…");
    assert!(!on_producer.id_is_current(), "and it is persisted: the producer's store serves an id its bytes do not hash to");
    assert!(on_peer.id_is_current());
    assert_eq!(stored_coinbase(&newcomer, x).id(), on_peer.id(), "a node that never saw the in-memory object agrees with the peer");

    // The majority's merge of x is UTXO-invalid on the producer alone (b6 on 60878caf…)…
    assert_eq!(peer.consensus.block_status(y), BlockStatus::StatusUTXOValid);
    assert_eq!(
        producer.consensus.block_status(y),
        BlockStatus::StatusDisqualifiedFromChain,
        "the producer computes a different utxo_commitment for the block that merges x"
    );
    assert_eq!(producer.consensus.get_sink(), z, "and extends its own branch");

    // …and the producer's own merge of x is UTXO-invalid for anyone who computes from the bytes
    // (c8e772df… on the majority; a joining node included).
    assert_eq!(newcomer.consensus.block_status(z), BlockStatus::StatusDisqualifiedFromChain);
    assert_eq!(newcomer.consensus.block_status(y), BlockStatus::StatusUTXOValid);
    assert_ne!(peer.consensus.block_status(z), BlockStatus::StatusUTXOValid);
    assert_eq!(peer.consensus.get_sink(), y, "the network keeps its chain");
    assert_eq!(newcomer.consensus.get_sink(), y, "and a joining node follows the network, never the producer's branch");
}

/// **After the fix: re-deriving the ids where an in-process block enters consensus keeps one chain.**
#[tokio::test]
async fn t12_split_ids_rederived_at_submission_keep_the_producer_on_the_network() {
    let Replay { producer, peer, newcomer, x, y, z } = replay(false).await;

    let id = stored_coinbase(&peer, x).id();
    for (node, name) in [(&producer, "producer"), (&peer, "peer"), (&newcomer, "newcomer")] {
        assert_eq!(stored_coinbase(node, x).id(), id, "{name}: one block, one coinbase id");
        assert_eq!(node.consensus.block_status(y), BlockStatus::StatusUTXOValid, "{name}: the majority's merge of x is valid");
        assert_eq!(node.consensus.block_status(z), BlockStatus::StatusUTXOValid, "{name}: the producer's next block is valid");
        assert_eq!(node.consensus.get_sink(), z, "{name}: one chain");
    }
}
