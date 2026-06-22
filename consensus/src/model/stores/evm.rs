//! kaspa-pq Selected-Parent EVM Lane (ADR-0020) consensus stores (design §11.1).
//! All keyed by `BlockHash` (an EVM result is an append-only function of the
//! block, design §2.1) except the canonical-heads singleton. The lazy
//! chain-context hook (P3 2/2) writes these inside the same `commit_utxo_state`
//! batch as the UTXO diff, so an EVM result and its UTXO side-effects commit
//! atomically. `insert_batch` refuses to overwrite an existing key — a backstop
//! for the no-replay rule (a block's result is computed once, never re-executed).
//!
//! Reusing the reserved prefixes `EvmHeader` (201), `EvmStateDiff` (206),
//! `EvmCanonicalHeads` (209) and `EvmPayload` (211). Cache policies are
//! caller-supplied; the store values all implement a real `MemSizeEstimator`,
//! so any policy is safe.

use kaspa_consensus_core::evm::{
    CanonicalEvmHeads, EvmBlockReceipts, EvmExecutionHeader, EvmExecutionPayload, EvmStateSnapshot, EvmTxLocations,
};
use kaspa_hashes::EvmH256;
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
// EvmExecutionPayload store (prefix 211) — each block's OWN payload, persisted
// at body validation (v0.4 §3.1). The virtual processor assembles
// `AcceptedEvmTxs(B)` by reading B's MERGESET blocks' payloads from here in
// canonical (sorted_mergeset) order. Unlike the result stores, re-insert is an
// idempotent no-op: the payload is immutable data committed by the header's
// `evm_payload_hash`, and a block body can legitimately be revalidated.
// ---------------------------------------------------------------------------

pub trait EvmPayloadStoreReader {
    fn get(&self, hash: BlockHash) -> Result<EvmExecutionPayload, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

pub trait EvmPayloadStore: EvmPayloadStoreReader {
    fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, payload: EvmExecutionPayload) -> Result<(), StoreError>;
    fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError>;
}

#[derive(Clone)]
pub struct DbEvmPayloadStore {
    access: CachedDbAccess<BlockHash, EvmExecutionPayload, BlockHasher>,
}

impl DbEvmPayloadStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmPayload.into()) }
    }
}

impl EvmPayloadStoreReader for DbEvmPayloadStore {
    fn get(&self, hash: BlockHash) -> Result<EvmExecutionPayload, StoreError> {
        self.access.read(hash)
    }
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

impl EvmPayloadStore for DbEvmPayloadStore {
    fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, payload: EvmExecutionPayload) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            // Idempotent: the payload is immutable per block (committed by
            // `evm_payload_hash`); a body revalidation must not fail here.
            return Ok(());
        }
        self.access.write(BatchDbWriter::new(batch), hash, payload)
    }
    fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

// ---------------------------------------------------------------------------
// EvmBlockReceipts store (prefix 203) — receipts of one ACCEPTING chain block.
// ---------------------------------------------------------------------------

pub trait EvmReceiptsStoreReader {
    fn get(&self, hash: BlockHash) -> Result<EvmBlockReceipts, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmReceiptsStore {
    access: CachedDbAccess<BlockHash, EvmBlockReceipts, BlockHasher>,
}

impl DbEvmReceiptsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmReceipts.into()) }
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, receipts: EvmBlockReceipts) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, receipts)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl EvmReceiptsStoreReader for DbEvmReceiptsStore {
    fn get(&self, hash: BlockHash) -> Result<EvmBlockReceipts, StoreError> {
        self.access.read(hash)
    }
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

// ---------------------------------------------------------------------------
// EvmTxLookup store (prefix 204) — tx hash → locations. UNGUARDED upsert: a
// row accretes entries as side branches / payload re-inclusions are seen.
// ---------------------------------------------------------------------------

pub trait EvmTxIndexStoreReader {
    fn get(&self, tx_hash: EvmH256) -> Result<EvmTxLocations, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmTxIndexStore {
    access: CachedDbAccess<EvmH256, EvmTxLocations>,
}

impl DbEvmTxIndexStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmTxLookup.into()) }
    }

    /// Read-or-default (absent row = a tx never seen).
    pub fn get_or_default(&self, tx_hash: EvmH256) -> Result<EvmTxLocations, StoreError> {
        match self.access.read(tx_hash) {
            Ok(row) => Ok(row),
            Err(StoreError::KeyNotFound(_)) => Ok(Default::default()),
            Err(e) => Err(e),
        }
    }

    /// Unguarded write (upsert) into the caller's batch.
    pub fn write_batch(&self, batch: &mut WriteBatch, tx_hash: EvmH256, row: EvmTxLocations) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), tx_hash, row)
    }
}

impl EvmTxIndexStoreReader for DbEvmTxIndexStore {
    fn get(&self, tx_hash: EvmH256) -> Result<EvmTxLocations, StoreError> {
        self.access.read(tx_hash)
    }
}

// ---------------------------------------------------------------------------
// EvmBlockHashMap store (prefix 210) — eth-rpc 32-byte block id → L1 BlockHash.
// The 32-byte id is the first 32 bytes of the 64-byte L1 hash (matches the
// truncation `eth_getTransactionReceipt` already exposes as `blockHash`), so
// `eth_getBlockByHash` can reverse a client-held 32-byte hash to the L1 block.
// Upsert (a given L1 block's first-32 is stable → effectively write-once, but
// tolerant of re-processing). RPC index only — never part of any commitment.
// ---------------------------------------------------------------------------

pub trait EvmBlockHashMapStoreReader {
    fn get(&self, rpc_hash: EvmH256) -> Result<Option<BlockHash>, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmBlockHashMapStore {
    access: CachedDbAccess<EvmH256, BlockHash>,
}

impl DbEvmBlockHashMapStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmBlockHashMap.into()) }
    }

    /// Unguarded upsert into the caller's batch.
    pub fn write_batch(&self, batch: &mut WriteBatch, rpc_hash: EvmH256, l1_hash: BlockHash) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), rpc_hash, l1_hash)
    }
}

impl EvmBlockHashMapStoreReader for DbEvmBlockHashMapStore {
    fn get(&self, rpc_hash: EvmH256) -> Result<Option<BlockHash>, StoreError> {
        match self.access.read(rpc_hash) {
            Ok(h) => Ok(Some(h)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// EvmNumberIndex store (prefix 213) — evm_number → L1 BlockHash (for
// `eth_getBlockByNumber` + `eth_getLogs` ranges). Keyed by the number encoded
// into a 32-byte key (right-aligned BE) so it reuses the proven `EvmH256` key
// type. Upsert: on a reorg the new canonical block at a number overwrites the
// old; the READER must re-validate `is_chain_block(hash) && header.evm_number == n`
// so a stale row reads as absent (the `get_evm_tx_receipt` canonical pattern).
// RPC index only — never part of any commitment.
// ---------------------------------------------------------------------------

/// Encode an `evm_number` as the 32-byte key of the number index (right-aligned BE).
#[inline]
fn evm_number_key(evm_number: u64) -> EvmH256 {
    let mut k = [0u8; 32];
    k[24..].copy_from_slice(&evm_number.to_be_bytes());
    EvmH256::from_bytes(k)
}

pub trait EvmNumberStoreReader {
    /// The (most-recently-written) L1 block hash for an `evm_number`. The caller
    /// MUST re-validate canonicality (`is_chain_block` + `header.evm_number`).
    fn get(&self, evm_number: u64) -> Result<Option<BlockHash>, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmNumberStore {
    access: CachedDbAccess<EvmH256, BlockHash>,
}

impl DbEvmNumberStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmNumberIndex.into()) }
    }

    /// Unguarded upsert into the caller's batch (the new canonical block at a
    /// number overwrites the prior one on a reorg).
    pub fn write_batch(&self, batch: &mut WriteBatch, evm_number: u64, l1_hash: BlockHash) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), evm_number_key(evm_number), l1_hash)
    }
}

impl EvmNumberStoreReader for DbEvmNumberStore {
    fn get(&self, evm_number: u64) -> Result<Option<BlockHash>, StoreError> {
        match self.access.read(evm_number_key(evm_number)) {
            Ok(h) => Ok(Some(h)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
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

        // Payload store: round-trips and re-insert is an idempotent no-op (the
        // payload is immutable data committed by `evm_payload_hash`).
        let payload_store = DbEvmPayloadStore::new(db.clone(), CachePolicy::Empty);
        let payload = EvmExecutionPayload { transactions: vec![vec![1, 2, 3]], ..Default::default() };
        let mut batch = WriteBatch::default();
        payload_store.insert_batch(&mut batch, bh(1), payload.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(payload_store.get(bh(1)).unwrap(), payload);
        let mut batch = WriteBatch::default();
        payload_store.insert_batch(&mut batch, bh(1), payload.clone()).unwrap();
        assert!(matches!(payload_store.get(bh(2)), Err(StoreError::KeyNotFound(_))), "absent payload reads as KeyNotFound (driver maps it to empty)");

        // Canonical heads singleton: absent → set → read.
        let mut heads_store = DbEvmCanonicalHeadsStore::new(db.clone());
        assert!(heads_store.get().is_err());
        let heads = CanonicalEvmHeads { latest: bh(3), safe: bh(2), finalized: bh(1) };
        heads_store.set(heads).unwrap();
        assert_eq!(heads_store.get().unwrap(), heads);
    }

    /// Audit H-01: the pruning processor reclaims per-block EVM state via
    /// `delete_batch`. Deleting a pruned block's rows must remove exactly that
    /// block's header/state/payload while a kept block (e.g. the pruning-point
    /// anchor) is untouched.
    #[test]
    fn evm_stores_delete_batch_reclaims_only_the_pruned_block() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let hdr = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);
        let state = DbEvmStateStore::new(db.clone(), CachePolicy::Empty);
        let payload = DbEvmPayloadStore::new(db.clone(), CachePolicy::Empty);

        let header = EvmExecutionHeader { evm_number: 7, gas_used: 21_000, ..Default::default() };
        let snap = EvmStateSnapshot { accounts: vec![] };
        let pl = EvmExecutionPayload { transactions: vec![vec![9, 9]], ..Default::default() };

        // Write two blocks: bh(1) will be "pruned", bh(2) is "kept".
        let mut batch = WriteBatch::default();
        for b in [bh(1), bh(2)] {
            hdr.insert_batch(&mut batch, b, header.clone()).unwrap();
            state.insert_batch(&mut batch, b, snap.clone()).unwrap();
            payload.insert_batch(&mut batch, b, pl.clone()).unwrap();
        }
        db.write(batch).unwrap();

        // Prune bh(1) (the exact set of deletes the pruning processor issues).
        let mut batch = WriteBatch::default();
        hdr.delete_batch(&mut batch, bh(1)).unwrap();
        state.delete_batch(&mut batch, bh(1)).unwrap();
        payload.delete_batch(&mut batch, bh(1)).unwrap();
        db.write(batch).unwrap();

        // bh(1) reclaimed across all three stores...
        assert!(hdr.get(bh(1)).is_err());
        assert!(state.get(bh(1)).is_err());
        assert!(matches!(payload.get(bh(1)), Err(StoreError::KeyNotFound(_))));
        // ...and bh(2) (the kept anchor) untouched.
        assert_eq!(hdr.get(bh(2)).unwrap(), header);
        assert_eq!(state.get(bh(2)).unwrap(), snap);
        assert_eq!(payload.get(bh(2)).unwrap(), pl);

        // Deleting an absent key is an idempotent no-op (inert on no-EVM blocks).
        let mut batch = WriteBatch::default();
        assert!(hdr.delete_batch(&mut batch, bh(9)).is_ok());
        assert!(state.delete_batch(&mut batch, bh(9)).is_ok());
        assert!(payload.delete_batch(&mut batch, bh(9)).is_ok());
    }
}
