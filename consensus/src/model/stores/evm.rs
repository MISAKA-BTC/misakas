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
    decode_log_posting_loc, encode_log_posting_loc, log_posting_bucket, CanonicalEvmHeads, EvmBlockReceipts, EvmExecutionHeader,
    EvmExecutionPayload, EvmRawTx, EvmStateCheckpointV1, EvmStateDiffV2, EvmStateSnapshot, EvmTraceReplayBodyV1, EvmTxLocations,
    LogPostingKind, LogPostingLoc,
};
use kaspa_hashes::EvmH256;
use kaspa_consensus_core::{BlockHash, BlockHasher};
use kaspa_database::prelude::{
    BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DbSetAccess, DirectDbWriter, StoreError, StoreResult, DB,
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
// type. CANONICAL-DRIVEN: the map is written ONLY from the selected chain at
// virtual commit (`update_evm_canonical_number_map`) — attached blocks claim
// their number via `write_batch`, detached blocks release it via
// `delete_if_matches_batch`. It is NEVER written at per-block result-commit,
// because a UTXO-valid sink-search loser (validated by
// `calculate_utxo_state_relatively` but not selected) would otherwise overwrite
// the canonical row and shadow that number until the next commit. The READER
// still re-validates `is_chain_block(hash) && header.evm_number == n` as a
// backstop, so any stale row reads as absent (the `get_evm_tx_receipt`
// canonical pattern). RPC index only — never part of any commitment.
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

    /// Attach pass: the canonical (selected-chain) block claims `evm_number`.
    /// An upsert — a reorg's new canonical block at a number overwrites the
    /// prior one; only ever called from the virtual-commit canonical pass.
    pub fn write_batch(&self, batch: &mut WriteBatch, evm_number: u64, l1_hash: BlockHash) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), evm_number_key(evm_number), l1_hash)
    }

    /// Detach pass: release the row for `evm_number` ONLY if it still points to
    /// `expected` (the detached chain block). A number already re-claimed by a
    /// newer canonical block is left intact. Reads the current row first — safe
    /// because detach runs before attach within the same virtual-commit batch.
    pub fn delete_if_matches_batch(&self, batch: &mut WriteBatch, evm_number: u64, expected: BlockHash) -> Result<(), StoreError> {
        let key = evm_number_key(evm_number);
        match self.access.read(key) {
            Ok(h) if h == expected => self.access.delete(BatchDbWriter::new(batch), key),
            Ok(_) => Ok(()),
            Err(StoreError::KeyNotFound(_)) => Ok(()),
            Err(e) => Err(e),
        }
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
// EvmRawTransaction store (prefix 217, audit R-2) — tx_hash → raw EIP-2718 bytes
// (+ the payload block that carried it). Populated at body commit for every tx
// in a block's payload, so `eth_getTransactionByHash`/receipt resolve the raw tx
// by hash WITHOUT the bounded `EvmTxLocations.included_in` scan (which evicts
// past 16 inclusions). RPC index only — never part of any commitment.
// ---------------------------------------------------------------------------

pub trait EvmRawTxStoreReader {
    /// The raw-tx record for an EVM tx hash (absent = never seen in a payload).
    fn get(&self, tx_hash: EvmH256) -> Result<Option<EvmRawTx>, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmRawTxStore {
    access: CachedDbAccess<EvmH256, EvmRawTx>,
}

impl DbEvmRawTxStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmRawTransaction.into()) }
    }

    /// Upsert the raw bytes of a tx into the caller's batch (a tx's bytes are
    /// immutable under re-processing, so a re-write is a harmless no-op-equivalent).
    pub fn write_batch(&self, batch: &mut WriteBatch, tx_hash: EvmH256, raw: Vec<u8>, payload_block: BlockHash) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), tx_hash, EvmRawTx { raw, payload_block })
    }

    /// Reclaim a tx's row (used by pruning of the carrying payload block).
    pub fn delete_batch(&self, batch: &mut WriteBatch, tx_hash: EvmH256) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), tx_hash)
    }
}

impl EvmRawTxStoreReader for DbEvmRawTxStore {
    fn get(&self, tx_hash: EvmH256) -> Result<Option<EvmRawTx>, StoreError> {
        match self.access.read(tx_hash) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// EvmTraceReplay store (prefix 219, design §11) — the per-accepting-block replay
// plan for `debug_traceTransaction`, keyed by the accepting L1 `BlockHash`.
// Mirrors the receipts store (no-overwrite, prunable). RPC/replay data only —
// never part of any commitment.
// ---------------------------------------------------------------------------

pub trait EvmTraceReplayStoreReader {
    /// The replay body for an accepting block, or `None` if no trace was recorded
    /// (pre-activation, non-EVM, or pruned).
    fn get(&self, hash: BlockHash) -> Result<Option<EvmTraceReplayBodyV1>, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmTraceReplayStore {
    access: CachedDbAccess<BlockHash, EvmTraceReplayBodyV1, BlockHasher>,
}

impl DbEvmTraceReplayStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmTraceReplay.into()) }
    }

    /// Insert the replay body for an accepting block. Refuses to overwrite (the
    /// no-replay backstop: a block's EVM result — and thus its replay plan — is
    /// computed exactly once, never re-executed).
    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, body: EvmTraceReplayBodyV1) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, body)
    }

    /// Reclaim an accepting block's replay body (pruning of the buried block).
    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl EvmTraceReplayStoreReader for DbEvmTraceReplayStore {
    fn get(&self, hash: BlockHash) -> Result<Option<EvmTraceReplayBodyV1>, StoreError> {
        match self.access.read(hash) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

// ---------------------------------------------------------------------------
// §12 archive — state diff (prefix 220), checkpoint (prefix 221), and the
// content-addressed code store (prefix 222). All RPC/archive data only; keyed by
// the canonical `BlockHash` (diff/checkpoint) or `code_hash` (code). The diff and
// checkpoint stores refuse overwrite (a block's archive form is computed once); the
// code store is content-addressed so a re-write is the identical bytes (upsert).
// ---------------------------------------------------------------------------

pub trait EvmStateDiffStoreReader {
    fn get(&self, hash: BlockHash) -> Result<Option<EvmStateDiffV2>, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmStateDiffStore {
    access: CachedDbAccess<BlockHash, EvmStateDiffV2, BlockHasher>,
}

impl DbEvmStateDiffStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmStateDiffV2.into()) }
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, diff: EvmStateDiffV2) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, diff)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl EvmStateDiffStoreReader for DbEvmStateDiffStore {
    fn get(&self, hash: BlockHash) -> Result<Option<EvmStateDiffV2>, StoreError> {
        match self.access.read(hash) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

pub trait EvmStateCheckpointStoreReader {
    fn get(&self, hash: BlockHash) -> Result<Option<EvmStateCheckpointV1>, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmStateCheckpointStore {
    access: CachedDbAccess<BlockHash, EvmStateCheckpointV1, BlockHasher>,
}

impl DbEvmStateCheckpointStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmStateCheckpoint.into()) }
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, checkpoint: EvmStateCheckpointV1) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, checkpoint)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl EvmStateCheckpointStoreReader for DbEvmStateCheckpointStore {
    fn get(&self, hash: BlockHash) -> Result<Option<EvmStateCheckpointV1>, StoreError> {
        match self.access.read(hash) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

pub trait EvmCodeStoreReader {
    /// The code bytes for a `code_hash` (absent = never stored).
    fn get(&self, code_hash: EvmH256) -> Result<Option<Vec<u8>>, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmCodeStore {
    access: CachedDbAccess<EvmH256, Vec<u8>>,
}

impl DbEvmCodeStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmCode.into()) }
    }

    /// Content-addressed upsert: `code_hash = keccak256(code)`, so a re-write is the
    /// identical bytes (idempotent, no overwrite guard needed).
    pub fn write_batch(&self, batch: &mut WriteBatch, code_hash: EvmH256, code: Vec<u8>) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), code_hash, code)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, code_hash: EvmH256) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), code_hash)
    }
}

impl EvmCodeStoreReader for DbEvmCodeStore {
    fn get(&self, code_hash: EvmH256) -> Result<Option<Vec<u8>>, StoreError> {
        match self.access.read(code_hash) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// EvmLogs posting index (prefix 205, design §8) — a secondary log index for
// fast long-range `eth_getLogs`. A `DbSetAccess` set: bucket = `kind || selector`
// (address / topicN), member = `LogPostingLoc` bytes (number-be || l1_hash || tx
// || log). Written for every UTXO-valid block (side branches included), so the
// query MUST canonical-filter each member's `l1_hash` against the `evm_number`
// map. RPC index only — never part of any commitment.
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct DbEvmLogIndexStore {
    access: DbSetAccess<Vec<u8>, Vec<u8>>,
    /// Singleton (fixed zero key) — the index completeness floor.
    floor: CachedDbAccess<EvmH256, u64>,
}

impl DbEvmLogIndexStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            access: DbSetAccess::new(db.clone(), DatabaseStorePrefixes::EvmLogs.into()),
            floor: CachedDbAccess::new(db, CachePolicy::Empty, DatabaseStorePrefixes::EvmLogIndexMeta.into()),
        }
    }

    /// The lowest `evm_number` from which the posting index is complete (`None`
    /// until the writer has indexed any block). The query may trust the index
    /// only for `from_number >= floor`; below it, fall back to the canonical scan.
    pub fn indexed_floor(&self) -> Option<u64> {
        match self.floor.read(EvmH256::from_bytes([0u8; 32])) {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(_) => None,
        }
    }

    /// Lower the floor to `n` if the index now covers a lower block (set-once for
    /// forward processing; a backfill lowers it). Idempotent. NOTE: the floor store
    /// uses `CachePolicy::Empty`, so this guard's `indexed_floor()` reads only
    /// COMMITTED state — a caching policy here would instead surface this same
    /// batch's uncommitted write (benign for the monotone min, but a real change).
    pub fn set_floor_batch(&self, batch: &mut WriteBatch, n: u64) -> Result<(), StoreError> {
        if self.indexed_floor().map_or(true, |cur| n < cur) {
            self.floor.write(BatchDbWriter::new(batch), EvmH256::from_bytes([0u8; 32]), n)?;
        }
        Ok(())
    }

    /// Add one posting (`kind`+`selector` bucket → `loc`) to the caller's batch.
    pub fn write_posting_batch(
        &self,
        batch: &mut WriteBatch,
        kind: LogPostingKind,
        selector: &[u8],
        loc: &LogPostingLoc,
    ) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), log_posting_bucket(kind, selector), encode_log_posting_loc(loc))
    }

    /// Iterate the postings of a `(kind, selector)` bucket in ascending block
    /// order (block-global `logIndex` order within a block). Malformed members —
    /// never written by us — are skipped.
    pub fn bucket_locs(&self, kind: LogPostingKind, selector: &[u8]) -> impl Iterator<Item = LogPostingLoc> + '_ {
        self.access.bucket_iterator(log_posting_bucket(kind, selector)).filter_map(|r| r.ok().and_then(|m| decode_log_posting_loc(&m)))
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

    /// Canonical-index fix: the `evm_number → L1 hash` map is canonical-driven
    /// at virtual commit. `write_batch` claims a number for the attached chain
    /// block; `delete_if_matches_batch` releases it on detach ONLY if the row is
    /// still the detached block's (a number re-claimed by a newer canonical
    /// block survives), so a sink-search loser can never shadow the canonical row.
    #[test]
    fn evm_number_store_canonical_claim_and_conditional_release() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbEvmNumberStore::new(db.clone(), CachePolicy::Empty);

        // Attach: number 5 → block A.
        let mut batch = WriteBatch::default();
        store.write_batch(&mut batch, 5, bh(0xAA)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get(5).unwrap(), Some(bh(0xAA)));

        // Detach a block that does NOT own the row (number 5 still points to A):
        // releasing B is a no-op — guards against deleting a re-claimed number.
        let mut batch = WriteBatch::default();
        store.delete_if_matches_batch(&mut batch, 5, bh(0xBB)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get(5).unwrap(), Some(bh(0xAA)), "mismatched detach must not delete");

        // Reorg A→B at number 5: detach A (matches → released) and attach B in
        // the same batch — the batch applies delete then put, so the claim wins.
        let mut batch = WriteBatch::default();
        store.delete_if_matches_batch(&mut batch, 5, bh(0xAA)).unwrap();
        store.write_batch(&mut batch, 5, bh(0xBB)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get(5).unwrap(), Some(bh(0xBB)), "attach re-claims the number after detach");

        // Detach with no re-attach (the chain shrank at this number): fully released.
        let mut batch = WriteBatch::default();
        store.delete_if_matches_batch(&mut batch, 5, bh(0xBB)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get(5).unwrap(), None, "released number reads as absent");
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

    /// audit R-2: the raw-tx store maps `tx_hash → raw bytes (+ payload block)`
    /// so getTransactionByHash/receipt resolve by hash (no bounded included_in
    /// scan). Round-trips, reads absent, and reclaims on delete (pruning path).
    #[test]
    fn evm_raw_tx_store_roundtrip_and_delete() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbEvmRawTxStore::new(db.clone(), CachePolicy::Empty);
        let th = EvmH256::from_bytes([0x7Au8; 32]);
        let raw = vec![0x02u8, 0xDE, 0xAD, 0xBE, 0xEF];

        let mut batch = WriteBatch::default();
        store.write_batch(&mut batch, th, raw.clone(), bh(3)).unwrap();
        db.write(batch).unwrap();
        let got = store.get(th).unwrap().expect("present");
        assert_eq!(got.raw, raw, "raw bytes round-trip by hash");
        assert_eq!(got.payload_block, bh(3), "carrying payload block recorded");

        // An unknown hash reads as absent (KeyNotFound → None).
        assert!(store.get(EvmH256::from_bytes([0x01u8; 32])).unwrap().is_none());

        // delete_batch reclaims the row (the pruning path).
        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, th).unwrap();
        db.write(batch).unwrap();
        assert!(store.get(th).unwrap().is_none(), "deleted row reads as absent");
    }

    /// §8: the log posting index — postings written under address/topic buckets
    /// are range-scanned in ascending block order; distinct selectors are isolated.
    #[test]
    fn evm_log_index_postings_scan_in_block_order() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbEvmLogIndexStore::new(db.clone());
        let addr_a = [0xAAu8; 20];
        let addr_b = [0xBBu8; 20];
        let topic = [0xCCu8; 32];

        let mut batch = WriteBatch::default();
        // address A: logs in blocks 7, 5, 6 — written OUT of order, same selector.
        for (n, tx, li) in [(7u64, 0u32, 0u32), (5, 1, 0), (6, 0, 2)] {
            let loc = LogPostingLoc { evm_number: n, l1_hash: bh(n as u8), tx_index: tx, in_receipt_log_index: li };
            store.write_posting_batch(&mut batch, LogPostingKind::Address, &addr_a, &loc).unwrap();
            store.write_posting_batch(&mut batch, LogPostingKind::Topic0, &topic, &loc).unwrap();
        }
        // address B: one log in block 5.
        store
            .write_posting_batch(&mut batch, LogPostingKind::Address, &addr_b, &LogPostingLoc { evm_number: 5, l1_hash: bh(5), tx_index: 0, in_receipt_log_index: 0 })
            .unwrap();
        db.write(batch).unwrap();

        // A bucket scan returns address A's postings sorted by block (5,6,7),
        // regardless of write order.
        let a: Vec<u64> = store.bucket_locs(LogPostingKind::Address, &addr_a).map(|l| l.evm_number).collect();
        assert_eq!(a, vec![5, 6, 7]);
        // address B is isolated in its own bucket.
        let b: Vec<u64> = store.bucket_locs(LogPostingKind::Address, &addr_b).map(|l| l.evm_number).collect();
        assert_eq!(b, vec![5]);
        // the topic0 bucket carries the same three postings.
        assert_eq!(store.bucket_locs(LogPostingKind::Topic0, &topic).count(), 3);
        // an unseen selector is empty.
        assert_eq!(store.bucket_locs(LogPostingKind::Address, &[0x00u8; 20]).count(), 0);
    }

    /// §8: the index completeness floor — unset until the writer runs, set-once
    /// for forward processing (later blocks don't raise it), lowered by a backfill.
    #[test]
    fn evm_log_index_floor_set_once_and_lowered_by_backfill() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbEvmLogIndexStore::new(db.clone());
        assert_eq!(store.indexed_floor(), None, "unset until the writer indexes a block");

        let mut b = WriteBatch::default();
        store.set_floor_batch(&mut b, 10).unwrap();
        db.write(b).unwrap();
        assert_eq!(store.indexed_floor(), Some(10));

        let mut b = WriteBatch::default();
        store.set_floor_batch(&mut b, 11).unwrap();
        db.write(b).unwrap();
        assert_eq!(store.indexed_floor(), Some(10), "a later (higher) block must not raise the floor");

        let mut b = WriteBatch::default();
        store.set_floor_batch(&mut b, 3).unwrap();
        db.write(b).unwrap();
        assert_eq!(store.indexed_floor(), Some(3), "a backfill of an older block lowers the floor");
    }
}
