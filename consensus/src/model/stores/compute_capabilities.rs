//! MISAKA Verified LLM Token-Weighted BFT: accepted **capability declarations**, keyed by the
//! transaction that declared them.
//!
//! # Why this is a store and not a walk
//!
//! The verifier committee for a certificate is drawn from the validators that had declared the
//! job's `(model, runtime)` profile *at the sortition beacon*. Those declarations were collected by
//! the credit walk, which spans `vlt_credit_window_blue_score` — minutes. A declaration is valid
//! for `max_capability_validity_blocks` — a day of blocks, some three orders of magnitude longer.
//!
//! So a perfectly valid declaration falls out of the walk while it is still in force, the candidate
//! pool for that profile empties, `select_verifiers` draws from nothing, and every honest verdict
//! belongs to no committee. The certificate then reads as **unverified** — not refuted, not early,
//! simply uncountable — and it stays that way. A devnet reproduced exactly that: five certificates
//! credited at sink 1899 and were unverified at 1999, the epoch the walk floor rose past a
//! declaration made at DAA 1348.
//!
//! Deepening the walk cannot fix it. A window that covered `max_capability_validity_blocks` would
//! re-read a day of blocks on every virtual commit. The declaration has to outlive the walk, which
//! means it has to be stored — exactly like the [`super::stake_bonds`] records it is filtered
//! against, and queried the same way: at a pinned DAA, so two branches asking about the same
//! beacon get the same pool.
//!
//! # Reorg discipline
//!
//! Written when a chain block that accepted the declaration joins the selected chain, deleted when
//! it leaves. A declaration is a fact with no state machine — unlike a bond, which can be slashed
//! or unbonded — so there is nothing to revert beyond its existence.

use std::sync::Arc;

use kaspa_consensus_core::dns_finality::ComputeCapabilityRecord;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

/// The record layout AND the derivation rules these rows were written under.
///
/// A row is borsh-encoded, so changing the record's fields makes every existing row undecodable —
/// and `CachedDbAccess`'s iterator drops a row it cannot decode without a word, so the store reads
/// as *empty* rather than as broken. An empty capability store is indistinguishable from "nobody
/// declared": the committee is drawn from nothing, every honest verdict belongs to no committee,
/// and the certificate reads as unverified. That is the same failure this store was introduced to
/// fix, re-entered through the back door of a schema change.
///
/// * 1 — initial.
/// * 2 — added `declaration_block`, so the candidate pool can be bound to the beacon's own chain
///   history rather than argued from DAA.
pub const CAPABILITY_SCHEMA_VERSION: u32 = 2;

/// Accepted capability declarations, keyed by declaring transaction.
#[derive(Clone)]
pub struct DbComputeCapabilityStore {
    db: Arc<DB>,
    access: CachedDbAccess<TransactionId, Arc<ComputeCapabilityRecord>>,
    /// The layout/rules marker. Also gates the sweep: a version change must re-sweep, because the
    /// rows it would have relied on are gone.
    schema: CachedDbItem<u32>,
    /// Set once the store has been filled from history. Its absence on a database that already has
    /// a chain means the rows for blocks accepted before this store existed were never written —
    /// and an empty pool is indistinguishable from "nobody declared", which is exactly the failure
    /// the store was introduced to end.
    backfilled: CachedDbItem<u32>,
}

impl DbComputeCapabilityStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::ComputeCapabilities.into()),
            schema: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::ComputeCapabilitiesSchema.into()),
            backfilled: CachedDbItem::new(db, DatabaseStorePrefixes::ComputeCapabilitiesBackfilled.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(
        &mut self,
        batch: &mut WriteBatch,
        tx_id: TransactionId,
        record: Arc<ComputeCapabilityRecord>,
    ) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), tx_id, record)
    }

    /// Immediate, unbatched write — used by the history sweep, whose rows must be readable by the
    /// rest of the same commit.
    pub fn insert_direct(&mut self, tx_id: TransactionId, record: Arc<ComputeCapabilityRecord>) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), tx_id, record)
    }

    pub fn delete_batch(&mut self, batch: &mut WriteBatch, tx_id: TransactionId) -> StoreResult<()> {
        self.access.delete(BatchDbWriter::new(batch), tx_id)
    }

    /// Drop rows written under a superseded layout, and force the sweep to run again.
    ///
    /// Called before any read. Undecodable rows would otherwise be dropped silently by the
    /// iterator and read as an empty store, which is a live-looking answer that is simply wrong.
    pub fn reindex_if_stale(&mut self) -> Result<(), StoreError> {
        let stored = match self.schema.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored == Some(CAPABILITY_SCHEMA_VERSION) {
            return Ok(());
        }
        if self.access.iterator().next().is_some() {
            kaspa_core::info!(
                "[capability-store] rows were written under layout v{} and this build reads v{CAPABILITY_SCHEMA_VERSION}; \
                 discarding them and re-sweeping history",
                stored.unwrap_or(1)
            );
            self.access.delete_all(DirectDbWriter::new(&self.db))?;
        }
        // The sweep must run again: whatever it wrote is what was just discarded.
        self.backfilled.write(DirectDbWriter::new(&self.db), &0u32)?;
        self.schema.write(DirectDbWriter::new(&self.db), &CAPABILITY_SCHEMA_VERSION)
    }

    /// Whether history has already been swept into this store.
    pub fn is_backfilled(&self) -> bool {
        matches!(self.backfilled.read(), Ok(1))
    }

    /// Mark the sweep done. Called once the caller has written every declaration a live pool could
    /// still need — see `backfill_compute_capabilities`.
    pub fn mark_backfilled(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        self.backfilled.write(BatchDbWriter::new(batch), &1u32)
    }

    /// Direct (non-batched) mark, for a sweep that ran outside a commit batch.
    pub fn mark_backfilled_direct(&mut self) -> Result<(), StoreError> {
        self.backfilled.write(DirectDbWriter::new(&self.db), &1u32)
    }

    /// Every stored declaration.
    ///
    /// Whole-store iteration, like the bond set the candidates are filtered against: the pool is a
    /// function of every live declaration, and the set is bounded by the validator count rather
    /// than by chain length, since a renewal replaces its predecessor in
    /// `capability_candidate_pool` (it keeps the longest-lived entry per validator).
    pub fn all(&self) -> Vec<ComputeCapabilityRecord> {
        self.access.iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect()
    }
}
