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
    AccountCore, CanonicalEvmHeads, EvmAddress, EvmBlockReceipts, EvmCheckpointMeta, EvmColdSegmentManifest, EvmExecutionHeader,
    EvmExecutionPayload, EvmLatestStatePtr, EvmPruneCursor, EvmPruneSegment, EvmRawTx, EvmStateCheckpointV1, EvmStateCheckpointV2,
    EvmStateDiffV2, EvmStateSnapshot, EvmTraceReplayBodyV1, EvmTxLocations, EvmU256, FlatAccount, LogPostingKind, LogPostingLoc,
    decode_log_posting_loc, encode_log_posting_loc, log_posting_bucket,
};
use kaspa_consensus_core::{BlockHash, BlockHasher};
use kaspa_database::prelude::{
    BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DB, DbSetAccess, DirectDbWriter, StoreError, StoreResult,
};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::EvmH256;
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

impl DbEvmStateStore {
    /// `true` if any 206 snapshot row exists (peeks a single row). C-01 S9b-prune: used to skip the
    /// one-shot legacy bulk reclamation when the store is already empty.
    pub fn has_any(&self) -> Result<bool, StoreError> {
        Ok(!self.access.is_empty()?)
    }

    /// Enumerate all legacy per-block snapshots. This is intentionally exposed only for the
    /// one-shot C-01 migration: old databases can predate the content-addressed code store, while
    /// every 206 snapshot still contains the authoritative bytecode inline. Streaming the rows lets
    /// startup backfill prefix 222 without retaining the (potentially very large) snapshots in RAM.
    pub fn iter(&self) -> impl Iterator<Item = Result<(BlockHash, EvmStateSnapshot), StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, v)) => <[u8; 64]>::try_from(k.as_ref())
                .map(|b| (BlockHash::from_bytes(b), v))
                .map_err(|_| StoreError::DataInconsistency("EvmStateSnapshot key is not 64 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmStateSnapshot iterator: {e}"))),
        })
    }

    /// C-01 S9b-prune: ONE-SHOT bulk reclamation of the ENTIRE legacy 206 snapshot store (a
    /// `delete_range` over the prefix + a synchronous prefix-bounded `compact_range`). IRREVERSIBLE.
    /// Only sound when `--evm-retire-206` is effective (flat backend authoritative + shadow check on):
    /// the executor then seeds from the flat/reconstruct parent and a present 206 is merely a redundant
    /// byte-compare oracle, so dropping all 206 rows leaves the seed itself unchanged. The caller
    /// (`Consensus::evm_legacy_206_bulk_prune`) enforces that gate. Synchronous compaction of a large
    /// store can take a while.
    pub fn bulk_delete_all_and_compact(&self) -> Result<(), StoreError> {
        self.access.delete_all_and_compact()
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

    /// Drop the whole row. The row VECTORS are bounded, but the number of rows —
    /// one per unique tx hash ever seen — is not, which is the growth the pruner
    /// exists to stop.
    pub fn delete_batch(&self, batch: &mut WriteBatch, tx_hash: EvmH256) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), tx_hash)
    }

    /// Remove every location pointing at `block`, deleting the row once it is
    /// empty. Returns whether the row is now gone.
    ///
    /// An emptied row is deleted rather than kept: a row recording that a tx
    /// exists in no retained block is indistinguishable from a tx that was never
    /// seen, and keeping it would leave exactly the unbounded row count the
    /// pruner is here to bound.
    pub fn remove_block_locations_batch(
        &self,
        batch: &mut WriteBatch,
        tx_hash: EvmH256,
        block: BlockHash,
    ) -> Result<bool, StoreError> {
        let mut row = match self.access.read(tx_hash) {
            Ok(row) => row,
            Err(StoreError::KeyNotFound(_)) => return Ok(true),
            Err(e) => return Err(e),
        };
        row.included_in.retain(|b| *b != block);
        row.accepted_in.retain(|(b, _)| *b != block);
        if row.included_in.is_empty() && row.accepted_in.is_empty() {
            self.access.delete(BatchDbWriter::new(batch), tx_hash)?;
            return Ok(true);
        }
        self.access.write(BatchDbWriter::new(batch), tx_hash, row)?;
        Ok(false)
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

    /// Reclaim one mapping. Rows are small, but one per block forever is not: at
    /// 10 BPS "small and permanent" is tens of millions of rows a year.
    pub fn delete_batch(&self, batch: &mut WriteBatch, rpc_hash: EvmH256) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), rpc_hash)
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

    /// Retention pass: drop the row for `evm_number` outright.
    ///
    /// Distinct from `delete_if_matches_batch`, which is the reorg detach and must
    /// not disturb a number a newer canonical block has re-claimed. Pruning is
    /// deleting a number the node no longer serves at all, whoever owns it.
    pub fn delete_batch(&self, batch: &mut WriteBatch, evm_number: u64) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), evm_number_key(evm_number))
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
    pub fn write_batch(
        &self,
        batch: &mut WriteBatch,
        tx_hash: EvmH256,
        raw: Vec<u8>,
        payload_block: BlockHash,
    ) -> Result<(), StoreError> {
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

    /// Enumerate every retained diff. A mark root for the code GC: a reorg replays
    /// these, so the bytecode they name on BOTH sides has to survive.
    pub fn iter(&self) -> impl Iterator<Item = Result<(BlockHash, EvmStateDiffV2), StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, v)) => <[u8; 64]>::try_from(k.as_ref())
                .map(|b| (BlockHash::from_bytes(b), v))
                .map_err(|_| StoreError::DataInconsistency("EvmStateDiffV2 key is not 64 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmStateDiffV2 iterator: {e}"))),
        })
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

    /// Enumerate every retained legacy anchor. A code-GC mark root while a
    /// migrating database still has them.
    pub fn iter(&self) -> impl Iterator<Item = Result<(BlockHash, EvmStateCheckpointV1), StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, v)) => <[u8; 64]>::try_from(k.as_ref())
                .map(|b| (BlockHash::from_bytes(b), v))
                .map_err(|_| StoreError::DataInconsistency("EvmStateCheckpointV1 key is not 64 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmStateCheckpointV1 iterator: {e}"))),
        })
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
// C-01 state backend (design v0.1, Stage 1) — the flat LATEST-canonical state.
// `EvmFlatAccount` (234) holds one row per account in the current canonical state
// (point lookup O(1), full enumeration for the state-root recompute); the per-block
// `state_root` is indexed by `EvmBlockStateRoot` (232); `EvmLatestStatePtr` (231) is
// the canonical pointer the flat rows currently materialize. State data only — the
// committed `state_root` recomputed from these is byte-identical to the snapshot
// path (consensus-NEUTRAL). INERT until the writer/seed switch (later slices).
// ---------------------------------------------------------------------------

/// `EvmAddress → FlatAccount` (prefix 234): the flat latest-canonical EVM state.
#[derive(Clone)]
pub struct DbEvmFlatAccountStore {
    access: CachedDbAccess<EvmAddress, FlatAccount>,
}

impl DbEvmFlatAccountStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmFlatAccount.into()) }
    }

    /// The account at `address` in the current canonical state (`None` = absent).
    pub fn get(&self, address: EvmAddress) -> Result<Option<FlatAccount>, StoreError> {
        match self.access.read(address) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Upsert one account (latest canonical — a re-write replaces the prior value).
    pub fn write_batch(&self, batch: &mut WriteBatch, address: EvmAddress, account: FlatAccount) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), address, account)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, address: EvmAddress) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), address)
    }

    /// Enumerate every `(address, account)` in the current canonical state — the
    /// input to the keccak-MPT `state_root` recompute and to full materialization
    /// (IBD pruning-point snapshot). Address order is the RocksDB key order.
    pub fn iter(&self) -> impl Iterator<Item = Result<(EvmAddress, FlatAccount), StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, v)) => <[u8; 20]>::try_from(k.as_ref())
                .map(|b| (EvmAddress::from_bytes(b), v))
                .map_err(|_| StoreError::DataInconsistency("EvmFlatAccount key is not 20 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmFlatAccount iterator: {e}"))),
        })
    }
}

/// `BlockHash → state_root[32]` (prefix 232): O(1) committed-block state root.
#[derive(Clone)]
pub struct DbEvmBlockStateRootStore {
    access: CachedDbAccess<BlockHash, EvmH256, BlockHasher>,
}

impl DbEvmBlockStateRootStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmBlockStateRoot.into()) }
    }

    pub fn get(&self, block: BlockHash) -> Result<Option<EvmH256>, StoreError> {
        match self.access.read(block) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Upsert (a block's committed state root is stable; a re-write is identical bytes).
    pub fn write_batch(&self, batch: &mut WriteBatch, block: BlockHash, state_root: EvmH256) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), block, state_root)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, block: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), block)
    }
}

/// Singleton `EvmLatestStatePtr` (prefix 231): the canonical pointer the flat state
/// currently materializes.
pub struct DbEvmLatestStatePtrStore {
    access: CachedDbItem<EvmLatestStatePtr>,
}

impl DbEvmLatestStatePtrStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbItem::new(db, DatabaseStorePrefixes::EvmLatestStatePtr.into()) }
    }

    /// The current pointer (`None` = the flat state has not been initialized yet).
    pub fn get(&self) -> Result<Option<EvmLatestStatePtr>, StoreError> {
        match self.access.read() {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, ptr: EvmLatestStatePtr) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &ptr)
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
        if self.indexed_floor().is_none_or(|cur| n < cur) {
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

    /// Remove one posting from its bucket.
    ///
    /// The key layout is `kind || selector || evm_number || ...`, so the selector
    /// comes BEFORE the block number and "delete every posting below block N" is
    /// not a range. Deletion therefore has to name each posting, and the caller
    /// re-derives them from the block's receipts — the same derivation the writer
    /// used — rather than from a journal that would double the index's write cost.
    pub fn delete_posting_batch(
        &self,
        batch: &mut WriteBatch,
        kind: LogPostingKind,
        selector: &[u8],
        loc: &LogPostingLoc,
    ) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), log_posting_bucket(kind, selector), encode_log_posting_loc(loc))
    }

    /// The lowest `evm_number` this index can still ANSWER for, after pruning.
    ///
    /// Deliberately not `indexed_floor`. That one is a backfill watermark and
    /// MOVES DOWN as older blocks get indexed; this one moves UP as older blocks
    /// are reclaimed. One value cannot be both, and conflating them would let a
    /// prune pass advertise data it had just deleted.
    pub fn history_available_from(&self) -> Option<u64> {
        self.floor.read(EvmH256::from_bytes([1u8; 32])).ok()
    }

    /// Raise the availability floor. Monotone upward — it is a promise about what
    /// the node can still serve.
    pub fn set_history_available_from_batch(&self, batch: &mut WriteBatch, n: u64) -> Result<(), StoreError> {
        if self.history_available_from().is_none_or(|cur| n > cur) {
            self.floor.write(BatchDbWriter::new(batch), EvmH256::from_bytes([1u8; 32]), n)?;
        }
        Ok(())
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

    /// C-01 Stage 1: the flat-account store points-lookup + enumerates + deletes;
    /// the block→root index and the latest-state pointer round-trip.
    #[test]
    fn c01_flat_state_stores_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let roots = DbEvmBlockStateRootStore::new(db.clone(), CachePolicy::Empty);
        let mut ptr = DbEvmLatestStatePtrStore::new(db.clone());

        let addr = |b: u8| EvmAddress::from_bytes([b; 20]);
        let acct = |bal: u128| FlatAccount {
            core: kaspa_consensus_core::evm::AccountCore {
                nonce: 1,
                balance: EvmU256::from_u128(bal),
                code_hash: EvmH256::from_bytes([0; 32]),
            },
            storage: vec![(EvmU256::from_u128(1), EvmU256::from_u128(bal))],
        };

        let mut b = WriteBatch::default();
        flat.write_batch(&mut b, addr(0x01), acct(100)).unwrap();
        flat.write_batch(&mut b, addr(0x02), acct(200)).unwrap();
        flat.write_batch(&mut b, addr(0x03), acct(300)).unwrap();
        roots.write_batch(&mut b, bh(0x07), EvmH256::from_bytes([0x55; 32])).unwrap();
        ptr.set_batch(&mut b, EvmLatestStatePtr { canonical_head: bh(0x07), state_root: EvmH256::from_bytes([0x55; 32]) }).unwrap();
        db.write(b).unwrap();

        // point lookups
        assert_eq!(flat.get(addr(0x02)).unwrap(), Some(acct(200)));
        assert_eq!(flat.get(addr(0x09)).unwrap(), None);
        assert_eq!(roots.get(bh(0x07)).unwrap(), Some(EvmH256::from_bytes([0x55; 32])));
        assert_eq!(ptr.get().unwrap().unwrap().canonical_head, bh(0x07));

        // full enumeration (the state-root recompute input) sees every account.
        let mut all: Vec<_> = flat.iter().map(|r| r.unwrap()).collect();
        all.sort_by_key(|(a, _)| a.as_bytes());
        assert_eq!(all.len(), 3);
        assert_eq!(all.iter().map(|(a, _)| a.as_bytes()[0]).collect::<Vec<_>>(), vec![0x01, 0x02, 0x03]);

        // delete an account (self-destruct) reclaims exactly it.
        let mut b2 = WriteBatch::default();
        flat.delete_batch(&mut b2, addr(0x02)).unwrap();
        db.write(b2).unwrap();
        assert_eq!(flat.get(addr(0x02)).unwrap(), None);
        assert_eq!(flat.iter().count(), 2);
    }

    /// The legacy migration must stream every 206 row without loading the store into memory and
    /// must preserve the 64-byte block key exactly.
    #[test]
    fn c01_legacy_state_snapshots_iterate_for_backfill() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let states = DbEvmStateStore::new(db.clone(), CachePolicy::Empty);
        let snapshot = |tag: u8| EvmStateSnapshot {
            accounts: vec![EvmAccountSnapshot {
                address: EvmAddress::from_bytes([tag; 20]),
                nonce: tag as u64,
                balance: EvmU256::from_u128(tag as u128),
                code_hash: EvmH256::from_bytes([tag; 32]),
                code: vec![tag, tag.wrapping_add(1)],
                storage: vec![],
            }],
        };

        let mut batch = WriteBatch::default();
        states.insert_batch(&mut batch, bh(1), snapshot(1)).unwrap();
        states.insert_batch(&mut batch, bh(2), snapshot(2)).unwrap();
        db.write(batch).unwrap();

        let mut rows: Vec<_> = states.iter().map(|row| row.unwrap()).collect();
        rows.sort_by_key(|(hash, _)| hash.as_bytes()[0]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, bh(1));
        assert_eq!(rows[0].1, snapshot(1));
        assert_eq!(rows[1].0, bh(2));
        assert_eq!(rows[1].1, snapshot(2));
    }

    /// C-01 Stage 1 (S7, audit H-03): the flat point-lookup → `EvmAccountSnapshot`
    /// assembly that backs `get_evm_flat_account_at_head` — exercising the EOA
    /// empty-code branch (no code-store read) and the contract branch (code resolved
    /// by `code_hash` from the content-addressed store), plus the absent-account case.
    #[test]
    fn c01_flat_account_assembles_snapshot_with_code() {
        use kaspa_consensus_core::evm::{AccountCore, EVM_EMPTY_CODE_HASH};

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let flat = DbEvmFlatAccountStore::new(db.clone(), CachePolicy::Empty);
        let codes = DbEvmCodeStore::new(db.clone(), CachePolicy::Empty);

        // The exact assembly `get_evm_flat_account_at_head` performs (sans the ptr==sink gate):
        // flat row + code-by-hash (EOA ⇒ empty, no lookup) → snapshot.
        let assemble = |addr: EvmAddress| -> Option<EvmAccountSnapshot> {
            let flat_acct = flat.get(addr).unwrap()?;
            let code = if flat_acct.core.code_hash == EVM_EMPTY_CODE_HASH {
                Vec::new()
            } else {
                codes.get(flat_acct.core.code_hash).unwrap().unwrap_or_default()
            };
            Some(flat_acct.to_snapshot(addr, code))
        };

        let eoa = EvmAddress::from_bytes([0x11; 20]);
        let contract = EvmAddress::from_bytes([0x22; 20]);
        let code = vec![0x60u8, 0x80, 0x60, 0x40, 0x52]; // a few opcodes
        let code_hash = EvmH256::from_bytes([0xcd; 32]); // content-addressed key (not recomputed here)

        let eoa_flat = FlatAccount {
            core: AccountCore { nonce: 7, balance: EvmU256::from_u128(1_000), code_hash: EVM_EMPTY_CODE_HASH },
            storage: vec![],
        };
        let contract_flat = FlatAccount {
            core: AccountCore { nonce: 1, balance: EvmU256::from_u128(0), code_hash },
            storage: vec![(EvmU256::from_u128(3), EvmU256::from_u128(9))],
        };

        let mut b = WriteBatch::default();
        flat.write_batch(&mut b, eoa, eoa_flat.clone()).unwrap();
        flat.write_batch(&mut b, contract, contract_flat.clone()).unwrap();
        codes.write_batch(&mut b, code_hash, code.clone()).unwrap();
        db.write(b).unwrap();

        // EOA: empty code, no storage; the code store is NOT consulted for KECCAK_EMPTY.
        assert_eq!(
            assemble(eoa),
            Some(EvmAccountSnapshot {
                address: eoa,
                nonce: 7,
                balance: EvmU256::from_u128(1_000),
                code_hash: EVM_EMPTY_CODE_HASH,
                code: vec![],
                storage: vec![],
            })
        );
        // Contract: code resolved by hash, storage carried through.
        assert_eq!(
            assemble(contract),
            Some(EvmAccountSnapshot {
                address: contract,
                nonce: 1,
                balance: EvmU256::from_u128(0),
                code_hash,
                code,
                storage: vec![(EvmU256::from_u128(3), EvmU256::from_u128(9))],
            })
        );
        // Absent account ⇒ None (the AtHead(None) case).
        assert_eq!(assemble(EvmAddress::from_bytes([0x99; 20])), None);
        // Round-trip: re-deriving the flat row from the assembled snapshot is lossless.
        assert_eq!(FlatAccount::from_snapshot(&assemble(contract).unwrap()), contract_flat);
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
        assert!(
            matches!(payload_store.get(bh(2)), Err(StoreError::KeyNotFound(_))),
            "absent payload reads as KeyNotFound (driver maps it to empty)"
        );

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

    /// C-01 S9b-prune: the 206 store peeks emptiness (`has_any`) and bulk-reclaims the WHOLE store
    /// (`bulk_delete_all_and_compact`) in one shot, while a NEIGHBORING EVM store sharing the single
    /// column family by prefix (here the header store, prefix 201) is left completely untouched — the
    /// safety property the legacy-206 reclamation relies on (it must not collaterally delete other state).
    #[test]
    fn evm_state_store_bulk_reclaim_leaves_neighbors_intact() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let state = DbEvmStateStore::new(db.clone(), CachePolicy::Empty);
        let hdr = DbEvmHeaderStore::new(db.clone(), CachePolicy::Empty);

        // Empty store ⇒ has_any is false (the one-shot would skip).
        assert!(!state.has_any().unwrap());

        // Populate a few 206 snapshots AND a neighboring header row.
        let snap = EvmStateSnapshot { accounts: vec![] };
        let header = EvmExecutionHeader { evm_number: 7, gas_used: 21_000, ..Default::default() };
        let mut batch = WriteBatch::default();
        for b in [bh(1), bh(2), bh(3)] {
            state.insert_batch(&mut batch, b, snap.clone()).unwrap();
        }
        hdr.insert_batch(&mut batch, bh(1), header.clone()).unwrap();
        db.write(batch).unwrap();
        assert!(state.has_any().unwrap());

        // One-shot bulk reclaim of the entire 206 store.
        state.bulk_delete_all_and_compact().unwrap();
        assert!(!state.has_any().unwrap());
        for b in [bh(1), bh(2), bh(3)] {
            assert!(matches!(state.get(b), Err(StoreError::KeyNotFound(_))), "every 206 row reclaimed");
        }
        // The neighboring header store (prefix 201) is untouched.
        assert_eq!(hdr.get(bh(1)).unwrap(), header, "bulk 206 reclaim must not touch other EVM stores");

        // Idempotent: a second run on the now-empty store is a clean no-op.
        state.bulk_delete_all_and_compact().unwrap();
        assert!(!state.has_any().unwrap());
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
            .write_posting_batch(
                &mut batch,
                LogPostingKind::Address,
                &addr_b,
                &LogPostingLoc { evm_number: 5, l1_hash: bh(5), tx_index: 0, in_receipt_log_index: 0 },
            )
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

// ---------------------------------------------------------------------------
// §12.3 v2 — sparse compressed checkpoint anchors (prefix 223) and the cadence
// singleton (prefix 224).
//
// Deliberately a NEW prefix rather than a format change under 221. Borsh is not
// self-describing, so a v1 row and a v2 row are indistinguishable in place; a
// separate prefix lets a database written before this change stay readable —
// reconstruction consults v2 first, then v1 — while the segment pruner reclaims
// the v1 rows in the background. The alternative was a DB-version bump forcing
// every operator to resync, for RPC/archive data.
// ---------------------------------------------------------------------------

pub trait EvmStateCheckpointV2StoreReader {
    fn get(&self, hash: BlockHash) -> Result<Option<EvmStateCheckpointV2>, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

#[derive(Clone)]
pub struct DbEvmStateCheckpointV2Store {
    access: CachedDbAccess<BlockHash, EvmStateCheckpointV2, BlockHasher>,
}

impl DbEvmStateCheckpointV2Store {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmStateCheckpointV2.into()) }
    }

    /// Anchors are keyed by block, and a block is anchored at most once — an
    /// overwrite would mean the cadence fired twice for the same block, which is
    /// a logic error rather than something to paper over.
    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, checkpoint: EvmStateCheckpointV2) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, checkpoint)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<(BlockHash, EvmStateCheckpointV2), StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, v)) => <[u8; 64]>::try_from(k.as_ref())
                .map(|b| (BlockHash::from_bytes(b), v))
                .map_err(|_| StoreError::DataInconsistency("EvmStateCheckpointV2 key is not 64 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmStateCheckpointV2 iterator: {e}"))),
        })
    }
}

impl EvmStateCheckpointV2StoreReader for DbEvmStateCheckpointV2Store {
    fn get(&self, hash: BlockHash) -> Result<Option<EvmStateCheckpointV2>, StoreError> {
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

/// Singleton cadence state (prefix 224).
#[derive(Clone)]
pub struct DbEvmCheckpointMetaStore {
    access: CachedDbItem<EvmCheckpointMeta>,
}

impl DbEvmCheckpointMetaStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbItem::new(db, DatabaseStorePrefixes::EvmCheckpointMeta.into()) }
    }

    /// Absent reads as the default (no anchor yet) rather than an error: a fresh
    /// database has simply never written one.
    pub fn get(&self) -> Result<EvmCheckpointMeta, StoreError> {
        match self.access.read() {
            Ok(v) => Ok(v),
            Err(StoreError::KeyNotFound(_)) => Ok(EvmCheckpointMeta::default()),
            Err(e) => Err(e),
        }
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, meta: EvmCheckpointMeta) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &meta)
    }
}

// ---------------------------------------------------------------------------
// Segment pruner support stores.
// ---------------------------------------------------------------------------

/// Per-segment prune progress (prefix 225).
///
/// Persisted so a pass is resumable across restarts and so RPC can report an
/// availability floor. Keyed by the segment discriminant.
#[derive(Clone)]
pub struct DbEvmPruneCursorStore {
    access: CachedDbAccess<EvmH256, EvmPruneCursor>,
}

fn segment_key(segment: EvmPruneSegment) -> EvmH256 {
    let mut k = [0u8; 32];
    k[31] = segment as u8;
    EvmH256::from_bytes(k)
}

impl DbEvmPruneCursorStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbAccess::new(db, CachePolicy::Empty, DatabaseStorePrefixes::EvmPruneCursor.into()) }
    }

    /// Absent reads as a fresh cursor: a segment that has never been pruned has
    /// simply made no progress, which is not an error.
    pub fn get(&self, segment: EvmPruneSegment) -> Result<EvmPruneCursor, StoreError> {
        match self.access.read(segment_key(segment)) {
            Ok(c) => Ok(c),
            Err(StoreError::KeyNotFound(_)) => Ok(EvmPruneCursor::new()),
            Err(e) => Err(e),
        }
    }

    pub fn set_batch(&self, batch: &mut WriteBatch, segment: EvmPruneSegment, cursor: EvmPruneCursor) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), segment_key(segment), cursor)
    }
}

/// How many retained payload blocks still own a raw transaction (prefix 227).
///
/// 217 is keyed by tx hash while payloads are keyed by block, and the same tx can
/// appear in more than one payload. Deleting a raw tx when any single owning
/// block is pruned would break `eth_getTransactionByHash` for the others.
///
/// 204's location vectors cannot serve as this ledger: they are bounded and EVICT
/// older entries, so a tx in seventeen blocks has forgotten the first one. The
/// count is maintained in the same batch as the payload write, so it cannot drift
/// from what is actually stored.
#[derive(Clone)]
pub struct DbEvmRawTxOwnersStore {
    access: CachedDbAccess<EvmH256, u32>,
}

impl DbEvmRawTxOwnersStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmRawTxOwners.into()) }
    }

    pub fn get(&self, tx_hash: EvmH256) -> Result<u32, StoreError> {
        match self.access.read(tx_hash) {
            Ok(v) => Ok(v),
            Err(StoreError::KeyNotFound(_)) => Ok(0),
            Err(e) => Err(e),
        }
    }

    pub fn increment_batch(&self, batch: &mut WriteBatch, tx_hash: EvmH256) -> Result<u32, StoreError> {
        let next = self.get(tx_hash)?.saturating_add(1);
        self.access.write(BatchDbWriter::new(batch), tx_hash, next)?;
        Ok(next)
    }

    /// Returns the remaining owner count. At zero the row is removed and the
    /// caller may reclaim the raw transaction.
    ///
    /// Saturating rather than wrapping: an underflow would produce `u32::MAX`
    /// owners and pin a raw tx in the database forever, which is a worse failure
    /// than double-decrementing to zero and reclaiming a row that is rebuildable
    /// from the payload.
    pub fn decrement_batch(&self, batch: &mut WriteBatch, tx_hash: EvmH256) -> Result<u32, StoreError> {
        let next = self.get(tx_hash)?.saturating_sub(1);
        if next == 0 {
            self.access.delete(BatchDbWriter::new(batch), tx_hash)?;
        } else {
            self.access.write(BatchDbWriter::new(batch), tx_hash, next)?;
        }
        Ok(next)
    }
}

/// Bytecode a GC pass found unreachable, with the pass number that found it
/// (prefix 228).
///
/// Code entries are SHARED by every account, diff and anchor that references the
/// hash, so an unreachable verdict from a single pass is not enough: a concurrent
/// commit, a partially-written migration or a mark bug would delete code that
/// other retained state still needs, and unlike an index it is not rebuildable.
/// Quarantine turns "delete on one opinion" into "delete after several agree".
#[derive(Clone)]
pub struct DbEvmCodeQuarantineStore {
    access: CachedDbAccess<EvmH256, u64>,
}

impl DbEvmCodeQuarantineStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbAccess::new(db, CachePolicy::Empty, DatabaseStorePrefixes::EvmCodeQuarantine.into()) }
    }

    pub fn get(&self, code_hash: EvmH256) -> Result<Option<u64>, StoreError> {
        match self.access.read(code_hash) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_batch(&self, batch: &mut WriteBatch, code_hash: EvmH256, since_epoch: u64) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), code_hash, since_epoch)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, code_hash: EvmH256) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), code_hash)
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<(EvmH256, u64), StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, v)) => <[u8; 32]>::try_from(k.as_ref())
                .map(|b| (EvmH256::from_bytes(b), v))
                .map_err(|_| StoreError::DataInconsistency("EvmCodeQuarantine key is not 32 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmCodeQuarantine iterator: {e}"))),
        })
    }
}

impl DbEvmCodeStore {
    /// Enumerate every stored `code_hash`. The sweep half of the GC; values are
    /// skipped so a full pass does not pull every contract's bytecode into memory.
    pub fn iter_hashes(&self) -> impl Iterator<Item = Result<EvmH256, StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, _)) => <[u8; 32]>::try_from(k.as_ref())
                .map(EvmH256::from_bytes)
                .map_err(|_| StoreError::DataInconsistency("EvmCode key is not 32 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmCode iterator: {e}"))),
        })
    }
}

// ---------------------------------------------------------------------------
// C-01 Stage 2 — the SPLIT flat state: account core (230) + storage slots (233).
//
// Stage 1 put an account's whole storage vector in one row (234). Correct, and
// O(live state) rather than O(state x blocks), but every single-slot write had
// to decode, mutate, re-encode and rewrite the account's ENTIRE storage. A
// contract with 100k slots paid megabytes of write amplification, memtable
// pressure and SST churn to change one word — and RocksDB's temporary space
// during the resulting compaction is disk the node does not have while it is
// already short of it.
//
// Splitting makes a one-slot write one row. Zeroing a slot becomes a delete
// rather than a rewrite, and the slot rows of one account share an address
// prefix, so materializing an account is still one range scan.
// ---------------------------------------------------------------------------

/// `address | slot` — the 52-byte key of one storage slot.
///
/// Address first so an account's slots are contiguous and materializing one
/// account stays a single range scan. A fixed-size newtype rather than a `Vec`
/// so it satisfies the store's key bounds without heap traffic on every lookup.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlatSlotKey([u8; 52]);

impl FlatSlotKey {
    #[inline]
    pub fn new(address: EvmAddress, slot: EvmU256) -> Self {
        let mut k = [0u8; 52];
        k[..20].copy_from_slice(&address.as_bytes());
        k[20..].copy_from_slice(&slot.to_be_bytes());
        Self(k)
    }
}

impl AsRef<[u8]> for FlatSlotKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for FlatSlotKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Only ever used in store-error messages.
        write!(f, "{}", faster_hex::hex_string(&self.0))
    }
}

/// `address → AccountCore` (prefix 230): nonce, balance, code hash.
#[derive(Clone)]
pub struct DbEvmFlatAccountCoreStore {
    access: CachedDbAccess<EvmAddress, AccountCore>,
}

impl DbEvmFlatAccountCoreStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmFlatAccountCore.into()) }
    }

    pub fn get(&self, address: EvmAddress) -> Result<Option<AccountCore>, StoreError> {
        match self.access.read(address) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn write_batch(&self, batch: &mut WriteBatch, address: EvmAddress, core: AccountCore) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), address, core)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, address: EvmAddress) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), address)
    }

    pub fn iter(&self) -> impl Iterator<Item = Result<(EvmAddress, AccountCore), StoreError>> + '_ {
        self.access.iterator().map(|res| match res {
            Ok((k, v)) => <[u8; 20]>::try_from(k.as_ref())
                .map(|b| (EvmAddress::from_bytes(b), v))
                .map_err(|_| StoreError::DataInconsistency("EvmFlatAccountCore key is not 20 bytes".into())),
            Err(e) => Err(StoreError::DataInconsistency(format!("EvmFlatAccountCore iterator: {e}"))),
        })
    }

    pub fn is_empty(&self) -> Result<bool, StoreError> {
        self.access.is_empty()
    }
}

/// `address | slot → value` (prefix 233): one row per NON-ZERO storage slot.
///
/// Zero is absence, exactly as in the EVM: writing zero deletes the row rather
/// than storing it, so the store holds live storage and not its history.
#[derive(Clone)]
pub struct DbEvmFlatStorageStore {
    access: CachedDbAccess<FlatSlotKey, EvmU256>,
}

impl DbEvmFlatStorageStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EvmFlatStorageSlot.into()) }
    }

    pub fn get(&self, address: EvmAddress, slot: EvmU256) -> Result<Option<EvmU256>, StoreError> {
        match self.access.read(FlatSlotKey::new(address, slot)) {
            Ok(v) => Ok(Some(v)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write one slot. A zero value DELETES: the EVM has no distinction between
    /// "slot set to zero" and "slot never set", and storing zeros would make the
    /// store grow with writes instead of with live state.
    pub fn set_batch(&self, batch: &mut WriteBatch, address: EvmAddress, slot: EvmU256, value: EvmU256) -> Result<(), StoreError> {
        let key = FlatSlotKey::new(address, slot);
        if value == EvmU256::ZERO {
            self.access.delete(BatchDbWriter::new(batch), key)
        } else {
            self.access.write(BatchDbWriter::new(batch), key, value)
        }
    }

    /// Every non-zero slot of one account, ascending by slot. One range scan,
    /// which is what putting the address first in the key buys.
    pub fn account_slots(&self, address: EvmAddress) -> Result<Vec<(EvmU256, EvmU256)>, StoreError> {
        // `bucket` scopes the scan to this address, so the iterator yields only
        // this account's slots and the key suffix is the slot.
        let address_bytes = address.as_bytes();
        let mut out = Vec::new();
        for row in self.access.seek_iterator(Some(&address_bytes), None, usize::MAX, false) {
            let (key, value) = row.map_err(|e| StoreError::DataInconsistency(format!("EvmFlatStorageSlot iterator: {e}")))?;
            let Ok(slot_bytes) = <[u8; 32]>::try_from(key.as_ref()) else {
                return Err(StoreError::DataInconsistency("EvmFlatStorageSlot key suffix is not a 32-byte slot".into()));
            };
            out.push((EvmU256::from_be_bytes(slot_bytes), value));
        }
        Ok(out)
    }

    /// Drop every slot of an account — the destroyed-account path.
    pub fn delete_account_batch(&self, batch: &mut WriteBatch, address: EvmAddress) -> Result<u64, StoreError> {
        let mut deleted = 0;
        for (slot, _) in self.account_slots(address)? {
            self.access.delete(BatchDbWriter::new(batch), FlatSlotKey::new(address, slot))?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

/// The cold-segment manifest singleton (prefix 229).
///
/// In the database rather than derived by scanning a directory, so a node can
/// say what history it can serve WITHOUT touching a volume that may be slow,
/// remote or unmounted. A file missing from a directory the manifest lists is
/// then a detectable inconsistency rather than a silent narrowing of what the
/// node claims to have.
#[derive(Clone)]
pub struct DbEvmColdSegmentManifestStore {
    access: CachedDbItem<EvmColdSegmentManifest>,
}

impl DbEvmColdSegmentManifestStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbItem::new(db, DatabaseStorePrefixes::EvmColdSegmentManifest.into()) }
    }

    /// Absent reads as empty: a node that has exported nothing has an empty
    /// manifest, which is not an error.
    pub fn get(&self) -> Result<EvmColdSegmentManifest, StoreError> {
        match self.access.read() {
            Ok(v) => Ok(v),
            Err(StoreError::KeyNotFound(_)) => Ok(EvmColdSegmentManifest::new()),
            Err(e) => Err(e),
        }
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, manifest: EvmColdSegmentManifest) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &manifest)
    }
}
