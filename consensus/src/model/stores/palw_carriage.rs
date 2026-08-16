//! MISAKA PALW chain carriage (ADR-0029 Stage 1): accepted **carriage objects**, keyed by the
//! transaction that carried them.
//!
//! # An index with no consensus reader yet
//!
//! Stage 1 gives PALW objects dedicated subnetwork ids (band 0x40-0x45), stateless admission
//! validation, and this store — and deliberately nothing more. NOTHING in consensus rules reads
//! these rows: the credit gate, duty classification and offense grounding are Stage 2, exactly as
//! the capability store landed before its committee-draw consumer did. What the store buys today
//! is the ADR-0029 promise that every consensus-running node indexes the carried objects
//! identically — the precondition for ever grounding an objective offense in them.
//!
//! # Row contents
//!
//! `(kind byte, acceptance DAA, Borsh body bytes)` — the payload **verbatim**, never a re-encoded
//! object, so a Stage-2 reader decodes exactly what admission validated. The acceptance DAA is
//! the object's protocol time (ADR-0029 §4), the same clock the capability rows carry.
//!
//! # Reorg discipline
//!
//! Written when a chain block that accepted the carrier joins the selected chain, deleted when it
//! leaves — the [`super::compute_capabilities`] walk verbatim. A carriage row is a fact with no
//! state machine (dedup and adjudication are the reader's business, ADR-0029 §2), so there is
//! nothing to revert beyond its existence.

use std::sync::Arc;

use kaspa_consensus_core::palw_carriage::PalwCarriageRecord;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

/// The record layout these rows were written under.
///
/// A row is written through the serde path of `CachedDbAccess`, so changing
/// [`PalwCarriageRecord`]'s fields makes every existing row undecodable — and the iterator drops
/// a row it cannot decode without a word, so the store reads as *empty* rather than as broken. An
/// empty carriage index is indistinguishable from "nothing was carried", which would quietly
/// un-ground whatever Stage 2 builds on it. Bump this on any layout change so the rows are
/// discarded for re-sweeping instead of read as absent.
pub const PALW_CARRIAGE_SCHEMA_VERSION: u32 = 1;

/// Accepted PALW carriage objects, keyed by carrying transaction.
#[derive(Clone)]
pub struct DbPalwCarriageStore {
    db: Arc<DB>,
    access: CachedDbAccess<TransactionId, Arc<PalwCarriageRecord>>,
    /// The layout marker. Also gates the sweep: a version change must re-sweep, because the rows
    /// it would have relied on are gone.
    schema: CachedDbItem<u32>,
    /// Set once the store has been filled from history. Its absence on a database that already
    /// has a chain means carriers accepted before this store existed were never written.
    backfilled: CachedDbItem<u32>,
}

impl DbPalwCarriageStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::PalwCarriages.into()),
            schema: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::PalwCarriagesSchema.into()),
            backfilled: CachedDbItem::new(db, DatabaseStorePrefixes::PalwCarriagesBackfilled.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&mut self, batch: &mut WriteBatch, tx_id: TransactionId, record: Arc<PalwCarriageRecord>) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), tx_id, record)
    }

    pub fn delete_batch(&mut self, batch: &mut WriteBatch, tx_id: TransactionId) -> StoreResult<()> {
        self.access.delete(BatchDbWriter::new(batch), tx_id)
    }

    /// Whether a carrier's row is present (the Stage-2 reader's existence probe; tests today).
    pub fn has(&self, tx_id: TransactionId) -> StoreResult<bool> {
        self.access.has(tx_id)
    }

    /// One carrier's row.
    pub fn get(&self, tx_id: TransactionId) -> StoreResult<Arc<PalwCarriageRecord>> {
        self.access.read(tx_id)
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
        if stored == Some(PALW_CARRIAGE_SCHEMA_VERSION) {
            return Ok(());
        }
        if self.access.iterator().next().is_some() {
            kaspa_core::info!(
                "[palw-carriage-store] rows were written under layout v{} and this build reads v{PALW_CARRIAGE_SCHEMA_VERSION}; \
                 discarding them and re-sweeping history",
                stored.unwrap_or(1)
            );
            self.access.delete_all(DirectDbWriter::new(&self.db))?;
        }
        // The sweep must run again: whatever it wrote is what was just discarded.
        self.backfilled.write(DirectDbWriter::new(&self.db), &0u32)?;
        self.schema.write(DirectDbWriter::new(&self.db), &PALW_CARRIAGE_SCHEMA_VERSION)
    }

    /// Whether history has already been swept into this store.
    pub fn is_backfilled(&self) -> bool {
        matches!(self.backfilled.read(), Ok(1))
    }

    /// Direct (non-batched) mark, mirroring the capability sweep's: set only after every insert
    /// of the sweep is staged, so a crash mid-sweep leaves the marker unset and the next start
    /// sweeps again — idempotently, since a row keyed by its own transaction id rewrites to the
    /// same value.
    pub fn mark_backfilled_direct(&mut self) -> Result<(), StoreError> {
        self.backfilled.write(DirectDbWriter::new(&self.db), &1u32)
    }

    /// Every stored carriage row. Whole-store iteration like the capability pool — bounded by the
    /// walk horizon and the reorg discipline, not by chain length.
    pub fn all(&self) -> Vec<(TransactionId, PalwCarriageRecord)> {
        self.access
            .iterator()
            .filter_map(|r| r.ok().and_then(|(key, rec)| TransactionId::try_from(&key[..]).ok().map(|id| (id, (*rec).clone()))))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_KIND_ATTESTATION;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use rocksdb::WriteBatch;

    fn record(v: u8) -> PalwCarriageRecord {
        PalwCarriageRecord { kind: PALW_CARRIAGE_KIND_ATTESTATION, accepted_daa_score: 1_000 + v as u64, body: vec![v; 32] }
    }

    /// A carriage on a branch that loses must not stay in the store: rows keyed by carrying tx id
    /// are what makes the revert a plain delete (the capability-store discipline). Exercised at
    /// store level because that is where the capability walk's own coverage lives; the walk in
    /// the virtual processor drives exactly these two mutations.
    #[test]
    fn a_carriage_that_leaves_the_selected_chain_is_deleted() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwCarriageStore::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();
        assert!(!store.is_backfilled(), "a fresh store has not been swept");

        let (a, b) = (TransactionId::from_bytes([0xAA; 64]), TransactionId::from_bytes([0xBB; 64]));
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, a, Arc::new(record(1))).unwrap();
        store.insert_batch(&mut batch, b, Arc::new(record(2))).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.all().len(), 2);
        assert!(store.has(a).unwrap());
        assert_eq!(*store.get(b).unwrap(), record(2), "accepted → readable, bytes verbatim");

        // `b`'s block leaves the selected chain.
        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, b).unwrap();
        db.write(batch).unwrap();
        assert!(!store.has(b).unwrap(), "reverted → gone");
        let left = store.all();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0], (a, record(1)));

        // Re-inclusion restores it — keyed by carrying tx, so this is an overwrite, not a dup.
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, b, Arc::new(record(2))).unwrap();
        store.insert_batch(&mut batch, b, Arc::new(record(2))).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.all().len(), 2, "re-inclusion must not duplicate the row");

        // Deleting one that was never there is not an error: the revert re-derives from
        // acceptance data and may name a carrier this node filtered out when it was applied.
        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, TransactionId::from_bytes([0xCC; 64])).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.all().len(), 2);
    }
}
