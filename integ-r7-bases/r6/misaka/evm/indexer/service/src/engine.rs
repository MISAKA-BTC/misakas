//! The reconcile + backfill driver — the service's only piece with real
//! correctness logic, and the one verified offline.
//!
//! [`sync_once`] performs one indexing pass: snapshot the node head, reconcile
//! the local chain against the node's (reusing the audited pure planner
//! [`plan_reconcile`](misaka_evm_indexer_core::plan_reconcile) over a bounded,
//! finality-capped window), detach orphaned blocks, then walk forward fetching
//! each canonical block + its logs, decoding them with the audited core decoder
//! ([`decode_log`](misaka_evm_indexer_core::event::decode_log)) and applying
//! them through the [`IndexStore`]. The poll loop in `main` calls it repeatedly.
//!
//! Everything here is generic over [`NodeRpc`] + [`IndexStore`], so the tests at
//! the bottom drive the FULL loop — clean extend, reorg revert+reapply, bounded
//! catch-up, finality enforcement, malformed-log tolerance — against a fake
//! in-memory node and the in-memory store, with no live node or database.

use misaka_evm_indexer_core::event::{DecodedEvent, decode_log};
use misaka_evm_indexer_core::{BlockId, IndexedBlock, LocatedTransfer, plan_reconcile};

use crate::node::{NodeRpc, RpcError};
use crate::store::IndexStore;
use std::collections::HashMap;

/// What one [`sync_once`] pass did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncOutcome {
    /// Blocks detached on a reorg this pass.
    pub reverted: usize,
    /// Canonical blocks (re)applied this pass.
    pub applied: usize,
    /// Normalized transfer rows produced this pass.
    pub transfers: usize,
    /// Recognized-but-malformed logs skipped this pass (recorded, never expanded).
    pub malformed: usize,
    /// The store's canonical head height after the pass.
    pub new_head: Option<u64>,
    /// `true` when the pass reached the node's head; `false` if it stopped at the
    /// per-pass block cap and the loop should call again immediately.
    pub caught_up: bool,
}

/// A failure during a sync pass.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error("store: {0}")]
    Store(String),
    /// The local chain and the node disagree DEEPER than the finality window — a
    /// reorg that would have to rewrite a finalized block. The indexer refuses to
    /// touch finalized history and surfaces this for an operator (it implies the
    /// node it is following reorged past finality, or the indexer is pointed at a
    /// different chain). `window_floor` is the deepest height examined.
    #[error("reorg deeper than finality window (floor height {window_floor}); refusing to rewrite finalized blocks")]
    ReorgDeeperThanFinality { window_floor: u64 },
}

/// Run one indexing pass. `finality_depth` blocks below the node head are treated
/// as immutable (reverts are refused past it); `max_blocks` bounds how many
/// blocks a single pass applies so the poll loop makes steady, bounded progress.
pub async fn sync_once<N, S>(node: &N, store: &mut S, finality_depth: u64, max_blocks: u64) -> Result<SyncOutcome, EngineError>
where
    N: NodeRpc + Sync,
    S: IndexStore + Send,
{
    let node_head = node.block_number().await?;
    let local_head = store.head().await.map_err(store_err)?;

    // 1. Reconcile: decide what to detach and where to resume.
    let (revert, apply_from) = match &local_head {
        None => (Vec::new(), 0),
        Some(head) => reconcile(node, store, head.number, node_head, finality_depth).await?,
    };

    let mut outcome = SyncOutcome::default();

    // 2. Detach orphaned local blocks (planner returns them newest-first).
    for hash in &revert {
        store.revert_block(hash).await.map_err(store_err)?;
        outcome.reverted += 1;
    }

    // 3. (Re)apply forward, bounded by the node head and the per-pass cap.
    let ceiling = node_head.min(apply_from.saturating_add(max_blocks.saturating_sub(1)));
    if apply_from <= node_head {
        for n in apply_from..=ceiling {
            let Some(nb) = node.get_block(n).await? else {
                // The node head moved/raced below `n`; stop and let the next pass
                // re-reconcile against the fresh head.
                break;
            };
            let block = IndexedBlock {
                rpc_hash: nb.rpc_hash,
                // Standard eth-rpc exposes one block id; mirror it as the L1 hash.
                l1_hash: nb.rpc_hash,
                number: nb.number,
                parent_hash: nb.parent_hash,
                canonical: true,
                finalized: false,
            };
            let logs = node.get_logs(n, n).await?;
            let mut transfers = Vec::new();
            for log in &logs {
                // Defensive: only attribute logs the node reports under THIS block
                // hash (guards a get_block/get_logs reorg race).
                if log.block_hash != block.rpc_hash {
                    continue;
                }
                match decode_log(log.address, &log.topics, &log.data) {
                    Ok(Some(DecodedEvent::Transfers(ts))) => {
                        for transfer in ts {
                            transfers.push(LocatedTransfer {
                                block_number: log.block_number,
                                block_hash: log.block_hash,
                                tx_hash: log.tx_hash,
                                tx_index: log.tx_index,
                                log_index: log.log_index,
                                transfer,
                            });
                        }
                    }
                    // URI / unrecognized Transfer-shape: no normalized row.
                    Ok(Some(_)) | Ok(None) => {}
                    // Recognized but malformed (e.g. oversized batch): record, skip.
                    Err(_) => outcome.malformed += 1,
                }
            }
            outcome.transfers += transfers.len();
            store.apply_block(&block, &transfers).await.map_err(store_err)?;
            outcome.applied += 1;
        }
    }

    // 4. Advance finality: every block `finality_depth` below the node head is
    // immutable.
    if node_head >= finality_depth {
        store.set_finalized(node_head - finality_depth).await.map_err(store_err)?;
    }

    outcome.new_head = store.head().await.map_err(store_err)?.map(|b| b.number);
    outcome.caught_up = ceiling >= node_head || apply_from > node_head;
    Ok(outcome)
}

/// Build the bounded reconcile window and run the pure planner over it. Returns
/// `(revert_hashes_newest_first, apply_from)` or [`EngineError::ReorgDeeperThanFinality`]
/// if no common ancestor exists within the finality window.
async fn reconcile<N, S>(
    node: &N,
    store: &S,
    local_head_num: u64,
    node_head: u64,
    finality_depth: u64,
) -> Result<(Vec<[u8; 32]>, u64), EngineError>
where
    N: NodeRpc + Sync,
    S: IndexStore + Send,
{
    // Never examine (or revert) below the finality floor.
    let floor = local_head_num.saturating_sub(finality_depth);
    let mut local_map: HashMap<u64, BlockId> = HashMap::new();
    let mut node_map: HashMap<u64, BlockId> = HashMap::new();
    let mut local_head_id = None;
    for n in floor..=local_head_num {
        if let Some(b) = store.canonical_block_at(n).await.map_err(store_err)? {
            let id = BlockId { number: n, rpc_hash: b.rpc_hash };
            if n == local_head_num {
                local_head_id = Some(id);
            }
            local_map.insert(n, id);
        }
        if let Some(b) = node.get_block(n).await? {
            node_map.insert(n, b.block_id());
        }
    }

    let plan = plan_reconcile(local_head_id, |n| local_map.get(&n).copied(), |n| node_map.get(&n).copied());

    // No common ancestor at or above the floor → the divergence reaches finalized
    // history. (A real ancestor at height h yields apply_from = h + 1 > floor.)
    if floor > 0 && plan.apply_from <= floor {
        return Err(EngineError::ReorgDeeperThanFinality { window_floor: floor });
    }
    // `node_head` only bounds forward application (step 3); reconcile decides what
    // to revert and the resume height regardless of how far ahead the node is.
    let _ = node_head;
    Ok((plan.revert, plan.apply_from))
}

fn store_err<E: std::fmt::Display>(e: E) -> EngineError {
    EngineError::Store(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{NodeBlock, NodeLog};
    use crate::store::MemIndexStore;
    use alloy_primitives::U256;
    use async_trait::async_trait;

    /// `keccak256("Transfer(address,address,uint256)")` — ERC-20/721.
    const TRANSFER: &str = "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
    /// `keccak256("TransferSingle(address,address,address,uint256,uint256)")`.
    const TRANSFER_SINGLE: &str = "c3d58168c5ae7397731d063d5bbf3d657854427343f4c083240f7aacaa2d0f62";

    fn hx32(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        faster_hex::hex_decode(s.as_bytes(), &mut out).unwrap();
        out
    }
    fn addr_topic(a: u8) -> [u8; 32] {
        let mut t = [0u8; 32];
        t[12..].copy_from_slice(&[a; 20]);
        t
    }

    const TOK: [u8; 20] = [0x11; 20];

    /// A canonical ERC-20 Transfer log in block `n` (hash `bh`).
    fn erc20_log(n: u64, bh: [u8; 32], from: u8, to: u8, amount: u64, log_index: u32) -> NodeLog {
        let mut data = [0u8; 32];
        data[24..].copy_from_slice(&amount.to_be_bytes());
        NodeLog {
            address: TOK,
            topics: vec![hx32(TRANSFER), addr_topic(from), addr_topic(to)],
            data: data.to_vec(),
            block_number: n,
            block_hash: bh,
            tx_hash: [0xaa; 32],
            tx_index: 0,
            log_index,
        }
    }

    #[derive(Clone)]
    struct FakeBlock {
        rpc_hash: [u8; 32],
        parent: [u8; 32],
        logs: Vec<NodeLog>,
    }

    /// In-memory node: `chain[n]` is the canonical block at height `n`.
    struct FakeNode {
        chain: Vec<FakeBlock>,
    }

    impl FakeNode {
        fn block_at(n: u64, hash: u8, parent: u8, logs: Vec<NodeLog>) -> FakeBlock {
            let _ = n;
            FakeBlock { rpc_hash: [hash; 32], parent: [parent; 32], logs }
        }
    }

    #[async_trait]
    impl NodeRpc for FakeNode {
        async fn block_number(&self) -> Result<u64, RpcError> {
            Ok(self.chain.len() as u64 - 1)
        }
        async fn get_block(&self, number: u64) -> Result<Option<NodeBlock>, RpcError> {
            Ok(self.chain.get(number as usize).map(|b| NodeBlock { number, rpc_hash: b.rpc_hash, parent_hash: b.parent }))
        }
        async fn get_logs(&self, from: u64, to: u64) -> Result<Vec<NodeLog>, RpcError> {
            let mut out = Vec::new();
            for n in from..=to {
                if let Some(b) = self.chain.get(n as usize) {
                    out.extend(b.logs.iter().cloned());
                }
            }
            Ok(out)
        }
    }

    // a generous cap so single-pass tests reach the head.
    const ALL: u64 = 1_000;

    #[tokio::test]
    async fn clean_extend_from_empty_indexes_all_blocks() {
        // 0: mint 100 → A(0xA1); 1: A→B 40; 2: empty.
        let node = FakeNode {
            chain: vec![
                FakeNode::block_at(0, 0x00, 0x00, vec![erc20_log(0, [0x00; 32], 0x00, 0xA1, 100, 0)]),
                FakeNode::block_at(1, 0x01, 0x00, vec![erc20_log(1, [0x01; 32], 0xA1, 0xB2, 40, 0)]),
                FakeNode::block_at(2, 0x02, 0x01, vec![]),
            ],
        };
        let mut store = MemIndexStore::new();
        let out = sync_once(&node, &mut store, 10, ALL).await.unwrap();
        assert_eq!(out.applied, 3);
        assert_eq!(out.reverted, 0);
        assert_eq!(out.transfers, 2);
        assert_eq!(out.new_head, Some(2));
        assert!(out.caught_up);
        assert_eq!(store.erc20_balance(TOK, [0xA1; 20]).await.unwrap(), U256::from(60u64));
        assert_eq!(store.erc20_balance(TOK, [0xB2; 20]).await.unwrap(), U256::from(40u64));
    }

    #[tokio::test]
    async fn reorg_reverts_and_reapplies_with_correct_balances() {
        // First sync the original chain: 0,1,2 with A→B 40 at block 2.
        let mut store = MemIndexStore::new();
        let original = FakeNode {
            chain: vec![
                FakeNode::block_at(0, 0x00, 0x00, vec![erc20_log(0, [0x00; 32], 0x00, 0xA1, 100, 0)]),
                FakeNode::block_at(1, 0x01, 0x00, vec![]),
                FakeNode::block_at(2, 0x02, 0x01, vec![erc20_log(2, [0x02; 32], 0xA1, 0xB2, 40, 0)]),
            ],
        };
        sync_once(&original, &mut store, 10, ALL).await.unwrap();
        assert_eq!(store.erc20_balance(TOK, [0xB2; 20]).await.unwrap(), U256::from(40u64));

        // Node reorgs height 2 to a competing block 2' (hash 0x12) that instead
        // sends A→C 10, and extends to height 3.
        let reorged = FakeNode {
            chain: vec![
                FakeNode::block_at(0, 0x00, 0x00, vec![erc20_log(0, [0x00; 32], 0x00, 0xA1, 100, 0)]),
                FakeNode::block_at(1, 0x01, 0x00, vec![]),
                FakeNode::block_at(2, 0x12, 0x01, vec![erc20_log(2, [0x12; 32], 0xA1, 0xC3, 10, 0)]),
                FakeNode::block_at(3, 0x13, 0x12, vec![]),
            ],
        };
        let out = sync_once(&reorged, &mut store, 10, ALL).await.unwrap();
        assert_eq!(out.reverted, 1, "old block 2 detached");
        assert_eq!(out.applied, 2, "block 2' and 3 applied");
        assert_eq!(out.new_head, Some(3));
        // B's 40 was undone; C got 10; A is 100 - 10 = 90.
        assert_eq!(store.erc20_balance(TOK, [0xB2; 20]).await.unwrap(), U256::ZERO);
        assert_eq!(store.erc20_balance(TOK, [0xC3; 20]).await.unwrap(), U256::from(10u64));
        assert_eq!(store.erc20_balance(TOK, [0xA1; 20]).await.unwrap(), U256::from(90u64));
    }

    #[tokio::test]
    async fn bounded_catch_up_takes_multiple_passes() {
        let chain: Vec<FakeBlock> = (0..=5).map(|n| FakeNode::block_at(n, n as u8, n.saturating_sub(1) as u8, vec![])).collect();
        let node = FakeNode { chain };
        let mut store = MemIndexStore::new();

        // Cap at 2 blocks/pass: heights 0,1 then 2,3 then 4,5.
        let p1 = sync_once(&node, &mut store, 10, 2).await.unwrap();
        assert_eq!((p1.applied, p1.new_head, p1.caught_up), (2, Some(1), false));
        let p2 = sync_once(&node, &mut store, 10, 2).await.unwrap();
        assert_eq!((p2.applied, p2.new_head, p2.caught_up), (2, Some(3), false));
        let p3 = sync_once(&node, &mut store, 10, 2).await.unwrap();
        assert_eq!((p3.applied, p3.new_head, p3.caught_up), (2, Some(5), true));
        // Idempotent: a further pass at the head does nothing.
        let p4 = sync_once(&node, &mut store, 10, 2).await.unwrap();
        assert_eq!((p4.applied, p4.reverted, p4.caught_up), (0, 0, true));
    }

    #[tokio::test]
    async fn reorg_deeper_than_finality_is_refused() {
        // Index a 6-block chain.
        let mut store = MemIndexStore::new();
        let original =
            FakeNode { chain: (0..=5).map(|n| FakeNode::block_at(n, n as u8, n.saturating_sub(1) as u8, vec![])).collect() };
        sync_once(&original, &mut store, 2, ALL).await.unwrap();

        // Node now disagrees all the way down at height 1 (finalized: head 5,
        // depth 2 → floor 3). The competing chain shares nothing within the window.
        let deep = FakeNode { chain: (0..=5).map(|n| FakeNode::block_at(n, (n + 100) as u8, (n + 99) as u8, vec![])).collect() };
        let err = sync_once(&deep, &mut store, 2, ALL).await.unwrap_err();
        assert!(matches!(err, EngineError::ReorgDeeperThanFinality { window_floor: 3 }), "got {err:?}");
    }

    #[tokio::test]
    async fn malformed_log_is_counted_not_fatal() {
        // A TransferSingle topic with too-short data → DecodeError (malformed),
        // alongside a valid ERC-20 transfer in the same block.
        let bad = NodeLog {
            address: [0x55; 20],
            topics: vec![hx32(TRANSFER_SINGLE), addr_topic(0x0e), addr_topic(0xAA), addr_topic(0xBB)],
            data: vec![0u8; 10], // expects 64 bytes
            block_number: 0,
            block_hash: [0x00; 32],
            tx_hash: [0xaa; 32],
            tx_index: 0,
            log_index: 0,
        };
        let node =
            FakeNode { chain: vec![FakeNode::block_at(0, 0x00, 0x00, vec![bad, erc20_log(0, [0x00; 32], 0x00, 0xA1, 100, 1)])] };
        let mut store = MemIndexStore::new();
        let out = sync_once(&node, &mut store, 10, ALL).await.unwrap();
        assert_eq!(out.malformed, 1);
        assert_eq!(out.transfers, 1, "the valid transfer still landed");
        assert_eq!(out.applied, 1, "block applied despite the malformed log");
        assert_eq!(store.erc20_balance(TOK, [0xA1; 20]).await.unwrap(), U256::from(100u64));
    }

    #[tokio::test]
    async fn finality_marks_blocks_below_window() {
        let node = FakeNode { chain: (0..=5).map(|n| FakeNode::block_at(n, n as u8, n.saturating_sub(1) as u8, vec![])).collect() };
        let mut store = MemIndexStore::new();
        sync_once(&node, &mut store, 2, ALL).await.unwrap();
        // head 5, depth 2 → finalized up to 3.
        assert!(store.canonical_block_at(3).await.unwrap().unwrap().finalized);
        assert!(!store.canonical_block_at(4).await.unwrap().unwrap().finalized);
    }
}
