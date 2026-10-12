//! §10.3 / §10.5 — the storage-agnostic indexer engine: block-context models, a
//! [`TransferStore`] trait the backends (PostgreSQL / RocksDB, later slices)
//! implement, and an in-memory reference impl ([`MemStore`]) used by tests.
//!
//! The store owns BOTH the append-only transfer rows and the materialized
//! [`Balances`](crate::balance::Balances): `apply_block` attaches a canonical
//! block (insert rows + fold balances), `revert_block` detaches one on a reorg
//! (mark rows removed + inverse balance delta), and re-applying a previously
//! reverted block re-attaches it. All three are idempotent so the engine can
//! replay safely after a reconnect.

use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;

use alloy_primitives::U256;

use crate::balance::Balances;
use crate::event::TokenTransfer;

/// A `blocks` row (§10.3). `rpc_hash` is the eth-rpc block id (the key clients
/// see); `l1_hash` is the underlying L1 block hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedBlock {
    pub rpc_hash: [u8; 32],
    pub l1_hash: [u8; 32],
    pub number: u64,
    pub parent_hash: [u8; 32],
    pub canonical: bool,
    pub finalized: bool,
}

/// A transfer with its on-chain location — a `token_transfers` row (§10.3)
/// before the `canonical`/`removed` flags (which the store owns).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocatedTransfer {
    pub block_number: u64,
    pub block_hash: [u8; 32],
    pub tx_hash: [u8; 32],
    pub tx_index: u32,
    pub log_index: u32,
    pub transfer: TokenTransfer,
}

/// The persistence seam every backend implements. Write methods are idempotent
/// (§10.5: safe to replay after a reconnect); reads serve the query API (§10.6).
pub trait TransferStore {
    type Error: std::fmt::Debug;

    /// Attach a canonical block and its transfers (insert rows + fold balances).
    /// Re-applying a previously reverted block re-attaches it; a block already
    /// canonical is a no-op.
    fn apply_block(&mut self, block: &IndexedBlock, transfers: &[LocatedTransfer]) -> Result<(), Self::Error>;

    /// Detach a block on a reorg: flag its rows removed and apply the inverse
    /// balance delta (transfers reverted in reverse order). A no-op if the block
    /// is unknown or already non-canonical.
    fn revert_block(&mut self, block_hash: &[u8; 32]) -> Result<(), Self::Error>;

    /// Mark every block with `number <= up_to_number` finalized (immutable, §10.5).
    fn set_finalized(&mut self, up_to_number: u64) -> Result<(), Self::Error>;

    /// The current canonical head (highest-number canonical block), if any.
    fn head(&self) -> Result<Option<IndexedBlock>, Self::Error>;

    /// A block by its rpc hash.
    fn block(&self, block_hash: &[u8; 32]) -> Result<Option<IndexedBlock>, Self::Error>;

    /// The canonical block at a height, if any (§10.6 `getBlockByNumber`; the
    /// reconcile planner's `local_at(n)`). At most one block per height is
    /// canonical — a reorg detaches the old one before attaching the new.
    fn canonical_block_at(&self, number: u64) -> Result<Option<IndexedBlock>, Self::Error>;

    fn erc20_balance(&self, token: [u8; 20], owner: [u8; 20]) -> Result<U256, Self::Error>;
    fn erc721_owner(&self, collection: [u8; 20], token_id: U256) -> Result<Option<[u8; 20]>, Self::Error>;
    fn erc1155_balance(&self, collection: [u8; 20], token_id: U256, owner: [u8; 20]) -> Result<U256, Self::Error>;
}

/// In-memory reference [`TransferStore`] — the test/oracle backend (the
/// PostgreSQL / RocksDB backends in later slices must match its semantics).
#[derive(Debug, Default)]
pub struct MemStore {
    /// rpc_hash → block (kept across reorg with `canonical` toggled, for audit).
    blocks: HashMap<[u8; 32], IndexedBlock>,
    /// number → rpc_hashes seen at that height (canonical + detached).
    by_number: BTreeMap<u64, Vec<[u8; 32]>>,
    /// rpc_hash → its transfer rows (kept across reorg, `removed` toggled).
    transfers: HashMap<[u8; 32], Vec<StoredTransfer>>,
    balances: Balances,
}

#[derive(Debug, Clone)]
struct StoredTransfer {
    located: LocatedTransfer,
    removed: bool,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn token_transfers(stored: &[StoredTransfer]) -> Vec<TokenTransfer> {
        stored.iter().map(|s| s.located.transfer.clone()).collect()
    }
}

impl TransferStore for MemStore {
    type Error = Infallible;

    fn apply_block(&mut self, block: &IndexedBlock, transfers: &[LocatedTransfer]) -> Result<(), Self::Error> {
        match self.blocks.get(&block.rpc_hash) {
            // Already attached → idempotent no-op.
            Some(b) if b.canonical => return Ok(()),
            // Re-attaching a previously detached block: re-fold its stored rows.
            Some(_) => {
                let stored = self.transfers.get(&block.rpc_hash).cloned().unwrap_or_default();
                self.balances.apply_block(&Self::token_transfers(&stored));
                if let Some(rows) = self.transfers.get_mut(&block.rpc_hash) {
                    for r in rows {
                        r.removed = false;
                    }
                }
            }
            // New block.
            None => {
                self.balances.apply_block(&transfers.iter().map(|t| t.transfer.clone()).collect::<Vec<_>>());
                let rows = transfers.iter().cloned().map(|located| StoredTransfer { located, removed: false }).collect();
                self.transfers.insert(block.rpc_hash, rows);
                self.by_number.entry(block.number).or_default().push(block.rpc_hash);
            }
        }
        let mut b = block.clone();
        b.canonical = true;
        self.blocks.insert(block.rpc_hash, b);
        Ok(())
    }

    fn revert_block(&mut self, block_hash: &[u8; 32]) -> Result<(), Self::Error> {
        let Some(block) = self.blocks.get_mut(block_hash) else { return Ok(()) };
        if !block.canonical {
            return Ok(()); // already detached
        }
        block.canonical = false;
        if let Some(rows) = self.transfers.get(block_hash) {
            // Inverse delta: revert the block's transfers in reverse order.
            self.balances.revert_block(&Self::token_transfers(rows));
        }
        if let Some(rows) = self.transfers.get_mut(block_hash) {
            for r in rows {
                r.removed = true;
            }
        }
        Ok(())
    }

    fn set_finalized(&mut self, up_to_number: u64) -> Result<(), Self::Error> {
        for b in self.blocks.values_mut() {
            if b.number <= up_to_number && b.canonical {
                b.finalized = true;
            }
        }
        Ok(())
    }

    fn head(&self) -> Result<Option<IndexedBlock>, Self::Error> {
        // Highest-number canonical block. Iterate heights downward.
        for (_, hashes) in self.by_number.iter().rev() {
            if let Some(b) = hashes.iter().filter_map(|h| self.blocks.get(h)).find(|b| b.canonical) {
                return Ok(Some(b.clone()));
            }
        }
        Ok(None)
    }

    fn block(&self, block_hash: &[u8; 32]) -> Result<Option<IndexedBlock>, Self::Error> {
        Ok(self.blocks.get(block_hash).cloned())
    }

    fn canonical_block_at(&self, number: u64) -> Result<Option<IndexedBlock>, Self::Error> {
        Ok(self
            .by_number
            .get(&number)
            .and_then(|hashes| hashes.iter().filter_map(|h| self.blocks.get(h)).find(|b| b.canonical))
            .cloned())
    }

    fn erc20_balance(&self, token: [u8; 20], owner: [u8; 20]) -> Result<U256, Self::Error> {
        Ok(self.balances.erc20_balance(token, owner))
    }

    fn erc721_owner(&self, collection: [u8; 20], token_id: U256) -> Result<Option<[u8; 20]>, Self::Error> {
        Ok(self.balances.erc721_owner(collection, token_id))
    }

    fn erc1155_balance(&self, collection: [u8; 20], token_id: U256, owner: [u8; 20]) -> Result<U256, Self::Error> {
        Ok(self.balances.erc1155_balance(collection, token_id, owner))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TokenStandard;

    const A: [u8; 20] = [0xAA; 20];
    const B: [u8; 20] = [0xBB; 20];
    const TOK: [u8; 20] = [0x11; 20];

    fn block(number: u64, hash: u8, parent: u8) -> IndexedBlock {
        IndexedBlock {
            rpc_hash: [hash; 32],
            l1_hash: [hash; 32],
            number,
            parent_hash: [parent; 32],
            canonical: true,
            finalized: false,
        }
    }
    fn erc20_xfer(hash: u8, log_index: u32, from: [u8; 20], to: [u8; 20], amount: u64) -> LocatedTransfer {
        LocatedTransfer {
            block_number: 0,
            block_hash: [hash; 32],
            tx_hash: [0x01; 32],
            tx_index: 0,
            log_index,
            transfer: TokenTransfer {
                standard: TokenStandard::Erc20,
                token: TOK,
                operator: None,
                from,
                to,
                token_id: None,
                amount: U256::from(amount),
            },
        }
    }

    #[test]
    fn apply_blocks_tracks_head_and_balances() {
        let mut s = MemStore::new();
        s.apply_block(&block(1, 0x01, 0x00), &[erc20_xfer(0x01, 0, ZERO_ADDR, A, 100)]).unwrap();
        s.apply_block(&block(2, 0x02, 0x01), &[erc20_xfer(0x02, 0, A, B, 40)]).unwrap();
        assert_eq!(s.head().unwrap().unwrap().number, 2);
        assert_eq!(s.erc20_balance(TOK, A).unwrap(), U256::from(60u64));
        assert_eq!(s.erc20_balance(TOK, B).unwrap(), U256::from(40u64));

        // Idempotent re-apply of block 2 changes nothing.
        s.apply_block(&block(2, 0x02, 0x01), &[erc20_xfer(0x02, 0, A, B, 40)]).unwrap();
        assert_eq!(s.erc20_balance(TOK, B).unwrap(), U256::from(40u64));
    }

    #[test]
    fn revert_detaches_block_and_undoes_balances_then_reattach() {
        let mut s = MemStore::new();
        s.apply_block(&block(1, 0x01, 0x00), &[erc20_xfer(0x01, 0, ZERO_ADDR, A, 100)]).unwrap();
        s.apply_block(&block(2, 0x02, 0x01), &[erc20_xfer(0x02, 0, A, B, 40)]).unwrap();

        // Reorg: detach block 2.
        s.revert_block(&[0x02; 32]).unwrap();
        assert_eq!(s.erc20_balance(TOK, A).unwrap(), U256::from(100u64), "B's 40 returns to A");
        assert_eq!(s.erc20_balance(TOK, B).unwrap(), U256::ZERO);
        assert_eq!(s.head().unwrap().unwrap().number, 1, "head falls back to block 1");
        assert!(!s.block(&[0x02; 32]).unwrap().unwrap().canonical);
        // Idempotent re-revert is a no-op.
        s.revert_block(&[0x02; 32]).unwrap();
        assert_eq!(s.erc20_balance(TOK, A).unwrap(), U256::from(100u64));

        // A competing block 2' attaches instead.
        s.apply_block(&block(2, 0x12, 0x01), &[erc20_xfer(0x12, 0, A, B, 10)]).unwrap();
        assert_eq!(s.erc20_balance(TOK, B).unwrap(), U256::from(10u64));
        assert_eq!(s.head().unwrap().unwrap().rpc_hash, [0x12; 32]);

        // Re-attaching the original block 2 re-folds its stored rows exactly.
        s.apply_block(&block(2, 0x02, 0x01), &[]).unwrap();
        assert!(s.block(&[0x02; 32]).unwrap().unwrap().canonical);
        assert_eq!(s.erc20_balance(TOK, B).unwrap(), U256::from(50u64), "10 (2') + 40 (re-attached 2)");
    }

    #[test]
    fn canonical_block_at_tracks_reorg() {
        let mut s = MemStore::new();
        s.apply_block(&block(1, 0x01, 0x00), &[]).unwrap();
        s.apply_block(&block(2, 0x02, 0x01), &[]).unwrap();
        assert_eq!(s.canonical_block_at(2).unwrap().unwrap().rpc_hash, [0x02; 32]);
        assert!(s.canonical_block_at(3).unwrap().is_none(), "no block at unindexed height");

        // Reorg height 2 to a competing block: only the new one is canonical-at(2).
        s.revert_block(&[0x02; 32]).unwrap();
        assert!(s.canonical_block_at(2).unwrap().is_none(), "detached block not returned");
        s.apply_block(&block(2, 0x12, 0x01), &[]).unwrap();
        assert_eq!(s.canonical_block_at(2).unwrap().unwrap().rpc_hash, [0x12; 32]);
    }

    #[test]
    fn set_finalized_marks_blocks_up_to_height() {
        let mut s = MemStore::new();
        s.apply_block(&block(1, 0x01, 0x00), &[]).unwrap();
        s.apply_block(&block(2, 0x02, 0x01), &[]).unwrap();
        s.apply_block(&block(3, 0x03, 0x02), &[]).unwrap();
        s.set_finalized(2).unwrap();
        assert!(s.block(&[0x01; 32]).unwrap().unwrap().finalized);
        assert!(s.block(&[0x02; 32]).unwrap().unwrap().finalized);
        assert!(!s.block(&[0x03; 32]).unwrap().unwrap().finalized);
    }

    const ZERO_ADDR: [u8; 20] = [0u8; 20];
}
