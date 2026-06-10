//! kaspa-pq Selected-Parent EVM Lane (ADR-0020) consensus stores (design §11.1).
//! All keyed by `BlockHash` (an EVM result is an append-only function of the
//! block, design §2.1) except the canonical-heads singleton. The lazy
//! chain-context hook (P3 2/2) writes these inside the same `commit_utxo_state`
//! batch as the UTXO diff, so an EVM result and its UTXO side-effects commit
//! atomically. `insert_batch` refuses to overwrite an existing key — a backstop
//! for the no-replay rule (a block's result is computed once, never re-executed).
//!
//! Reusing the reserved prefixes `EvmHeader` (201), `EvmStateDiff` (206) and
//! `EvmCanonicalHeads` (209). Cache policies are caller-supplied; the store
//! values all implement a real `MemSizeEstimator`, so any policy is safe.

use kaspa_consensus_core::evm::{CanonicalEvmHeads, EvmExecutionHeader, EvmStateSnapshot};
use kaspa_consensus_core::{BlockHash, BlockHasher};
use kaspa_database::prelude::{
    BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DirectDbWriter, StoreError, StoreResult, DB,
};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// EvmExecutionHeader store (prefix 201) — the committed per-block EVM header.
// ---------------------------------------------------------------------------

pub trait EvmHeaderStoreReader {
    fn get(&self, hash: BlockHash) -> Result<EvmExecutionHeader, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

pub trait EvmHeaderStore: EvmHeaderStoreReader {
    fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, header: EvmExecutionHeader) -> Result<(), StoreError>;
    fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError>;
}

#[derive(Clone)]
pub struct DbEvmHeaderStore {
    access: CachedDbAccess<BlockHash, EvmExecutionHeader, BlockHasher>,
}

impl DbEvmHeaderStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmHeader.into()) }
    }
}

impl EvmHeaderStoreReader for DbEvmHeaderStore {
    fn get(&self, hash: BlockHash) -> Result<EvmExecutionHeader, StoreError> {
        self.access.read(hash)
    }
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

impl EvmHeaderStore for DbEvmHeaderStore {
    fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, header: EvmExecutionHeader) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, header)
    }
    fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

// ---------------------------------------------------------------------------
// EvmStateSnapshot store (prefix 206) — full EVM state per block, to seed the
// executor for the block's selected children.
// ---------------------------------------------------------------------------

pub trait EvmStateStoreReader {
    fn get(&self, hash: BlockHash) -> Result<EvmStateSnapshot, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

pub trait EvmStateStore: EvmStateStoreReader {
    fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, snapshot: EvmStateSnapshot) -> Result<(), StoreError>;
    fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError>;
}

#[derive(Clone)]
pub struct DbEvmStateStore {
    access: CachedDbAccess<BlockHash, EvmStateSnapshot, BlockHasher>,
}

impl DbEvmStateStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmStateDiff.into()) }
    }
}

impl EvmStateStoreReader for DbEvmStateStore {
    fn get(&self, hash: BlockHash) -> Result<EvmStateSnapshot, StoreError> {
        self.access.read(hash)
    }
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

impl EvmStateStore for DbEvmStateStore {
    fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, snapshot: EvmStateSnapshot) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, snapshot)
    }
    fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

// ---------------------------------------------------------------------------
// CanonicalEvmHeads singleton (prefix 209) — latest / safe / finalized pointers,
// updated on each virtual-state commit (mirrors `DbDnsStateStore`).
// ---------------------------------------------------------------------------

pub trait EvmCanonicalHeadsStoreReader {
    fn get(&self) -> StoreResult<CanonicalEvmHeads>;
}

pub trait EvmCanonicalHeadsStore: EvmCanonicalHeadsStoreReader {
    fn set(&mut self, heads: CanonicalEvmHeads) -> StoreResult<()>;
}

#[derive(Clone)]
pub struct DbEvmCanonicalHeadsStore {
    db: Arc<DB>,
    access: CachedDbItem<CanonicalEvmHeads>,
}

impl DbEvmCanonicalHeadsStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::EvmCanonicalHeads.into()) }
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, heads: CanonicalEvmHeads) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &heads)
    }
}

impl EvmCanonicalHeadsStoreReader for DbEvmCanonicalHeadsStore {
    fn get(&self) -> StoreResult<CanonicalEvmHeads> {
        self.access.read()
    }
}

impl EvmCanonicalHeadsStore for DbEvmCanonicalHeadsStore {
    fn set(&mut self, heads: CanonicalEvmHeads) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &heads)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{EvmAccountSnapshot, EvmU256};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::{EvmH256, Hash64};

    fn bh(b: u8) -> BlockHash {
        Hash64::from_bytes([b; 64])
    }

    #[test]
    fn evm_stores_roundtrip_and_no_replay_guard() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        // Header store: insert, read back, and refuse re-insert (no-replay backstop).
        let hdr_store = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);
        let header = EvmExecutionHeader { evm_number: 7, gas_used: 21_000, ..Default::default() };
        let mut batch = WriteBatch::default();
        hdr_store.insert_batch(&mut batch, bh(1), header.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(hdr_store.get(bh(1)).unwrap(), header);
        let mut batch = WriteBatch::default();
        assert!(matches!(hdr_store.insert_batch(&mut batch, bh(1), header.clone()), Err(StoreError::KeyAlreadyExists(_))));

        // State-snapshot store: round-trips a Vec-valued snapshot.
        let state_store = DbEvmStateStore::new(db.clone(), CachePolicy::Empty);
        let snap = EvmStateSnapshot {
            accounts: vec![EvmAccountSnapshot {
                address: Default::default(),
                nonce: 1,
                balance: EvmU256::from(123u64),
                code_hash: EvmH256::from_bytes([9; 32]),
                code: vec![1, 2, 3],
                storage: vec![(EvmU256::from(1u64), EvmU256::from(2u64))],
            }],
        };
        let mut batch = WriteBatch::default();
        state_store.insert_batch(&mut batch, bh(1), snap.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(state_store.get(bh(1)).unwrap(), snap);

        // Canonical heads singleton: absent → set → read.
        let mut heads_store = DbEvmCanonicalHeadsStore::new(db.clone());
        assert!(heads_store.get().is_err());
        let heads = CanonicalEvmHeads { latest: bh(3), safe: bh(2), finalized: bh(1) };
        heads_store.set(heads).unwrap();
        assert_eq!(heads_store.get().unwrap(), heads);
    }
}
