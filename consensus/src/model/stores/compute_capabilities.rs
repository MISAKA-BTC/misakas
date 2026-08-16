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

    /// Mark the sweep done — **in the caller's batch**, so the marker and the declarations it
    /// vouches for become durable in the same atomic write.
    ///
    /// There is deliberately no direct (non-batched) variant. A direct mark is durable the moment
    /// it returns, while the sweep's rows are still staged in an uncommitted `WriteBatch`: a crash
    /// in that window would leave a store that believes it has been swept and has none of the
    /// swept declarations, and the sweep never runs again — silently missing capabilities that
    /// gate VLT credit. Batched, the two are one write: a crash leaves both absent (the next start
    /// sweeps again, idempotently, since a declaration keyed by its own transaction id rewrites to
    /// the same value) or both present.
    pub fn mark_backfilled(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        self.backfilled.write(BatchDbWriter::new(batch), &1u32)
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::BlockHash;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;
    use rocksdb::WriteBatch;

    fn record(v: u8) -> ComputeCapabilityRecord {
        ComputeCapabilityRecord {
            declaration_block: BlockHash::from_bytes([v; 64]),
            validator_id: Hash64::from_bytes([v; 64]),
            bond_outpoint: TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes([v; 64]), 0),
            model_weights_hash: Hash64::from_bytes([1; 64]),
            runtime_hash: Hash64::from_bytes([2; 64]),
            runtime_class_id: Hash64::from_bytes([3; 64]),
            accepted_daa_score: 1_000,
            expiry_daa_score: 900_000,
        }
    }

    /// A declaration on a branch that loses must not stay in the store. The read side filters by
    /// ancestry, so a survivor cannot contaminate a committee — but a store that only ever grows
    /// accumulates every dead branch's declarations forever, and the delete is what bounds it.
    ///
    /// Exercised here rather than on a devnet because a live mesh will not produce the case: a
    /// validator declares once and renews `max_capability_validity_blocks` later, so no ordinary
    /// reorg removes a block containing a declaration.
    #[test]
    fn a_declaration_that_leaves_the_selected_chain_is_deleted() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbComputeCapabilityStore::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let (a, b) = (TransactionId::from_bytes([0xAA; 64]), TransactionId::from_bytes([0xBB; 64]));
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, a, Arc::new(record(1))).unwrap();
        store.insert_batch(&mut batch, b, Arc::new(record(2))).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.all().len(), 2);

        // `b`'s block leaves the selected chain.
        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, b).unwrap();
        db.write(batch).unwrap();
        let left = store.all();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].declaration_block, BlockHash::from_bytes([1; 64]));

        // Re-inclusion restores it — the same block can rejoin the selected chain, and the record
        // is keyed by its declaring transaction, so this is an overwrite rather than a duplicate.
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, b, Arc::new(record(2))).unwrap();
        store.insert_batch(&mut batch, b, Arc::new(record(2))).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.all().len(), 2, "re-inclusion must not duplicate the declaration");

        // Deleting one that was never there is not an error: the revert re-derives from acceptance
        // data and may name a declaration this node filtered out when it was applied.
        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, TransactionId::from_bytes([0xCC; 64])).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.all().len(), 2);
    }

    /// The marker and the declarations it vouches for become durable together.
    ///
    /// This store gates VLT credit, so a store that believes it has been swept and holds none of
    /// the swept declarations is a wrong-but-quiet consensus outcome, not a crash. A restart IS a
    /// fresh store over the same database, so that is how the durability is observed:
    /// `CachedDbItem::write` populates the in-memory cache immediately (the running process does
    /// not re-sweep), but the DB sees nothing until the batch is written.
    #[test]
    fn the_backfill_marker_is_not_durable_until_its_batch_is_written() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbComputeCapabilityStore::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let tx = kaspa_consensus_core::tx::TransactionId::from_bytes([0xAA; 64]);
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, tx, Arc::new(record(1))).unwrap();
        store.mark_backfilled(&mut batch).unwrap();

        assert!(store.is_backfilled(), "the sweeping process must not re-sweep within its own pass");
        {
            let restarted = DbComputeCapabilityStore::new(db.clone(), CachePolicy::Count(16));
            assert!(!restarted.is_backfilled(), "a crash before the batch write must leave the sweep undone");
            assert!(restarted.all().is_empty(), "and it must leave no declarations either — both or neither");
        }

        db.write(batch).unwrap();

        let restarted = DbComputeCapabilityStore::new(db, CachePolicy::Count(16));
        assert!(restarted.is_backfilled(), "the marker is durable once its batch is written");
        assert_eq!(restarted.all().len(), 1, "with the declarations it vouched for");
    }
}
