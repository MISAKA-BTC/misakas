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

/// Accepted capability declarations, keyed by declaring transaction.
#[derive(Clone)]
pub struct DbComputeCapabilityStore {
    db: Arc<DB>,
    access: CachedDbAccess<TransactionId, Arc<ComputeCapabilityRecord>>,
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
