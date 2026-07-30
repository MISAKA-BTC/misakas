//! Singleton store for the complete, checksummed PALW pruning-point frontier.
//!
//! This uses the fresh `PalwPrunedFrontier` prefix reserved by the original scaffold. That scaffold
//! had no production writer, so no released datadir contains its legacy `(BlockHash,
//! PalwPrunedFrontierV1)` tuple. The first producer writes [`PalwPruningPointSnapshotV1`] directly.
//! DB version 14 resets every `<= 13` datadir. Version 14 adds the active manifest/leaf/certificate
//! projection to the singleton, making first-post-PP ticket/reward reads self-contained on a fresh
//! pruned node; a v13 partial snapshot must never be reused.
//!
//! The frontier uses its own singleton because `PruningPointOverlaySnapshot` is bincode-persisted and
//! positional. Appending a field to that wrapper would make pre-upgrade values unreadable. A dedicated
//! prefix has no legacy bytes to misinterpret; before the first write, `get` returns `KeyNotFound`.
//! The frontier is not included in `overlay_commitment_root`.
//!
use kaspa_consensus_core::palw_pruned_frontier::PalwPruningPointSnapshotV1;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;
use std::sync::Arc;

pub type PalwPrunedFrontierEntry = PalwPruningPointSnapshotV1;

pub trait PalwPrunedFrontierStoreReader {
    fn get(&self) -> StoreResult<PalwPrunedFrontierEntry>;
}

pub trait PalwPrunedFrontierStore: PalwPrunedFrontierStoreReader {
    fn set(&mut self, entry: PalwPrunedFrontierEntry) -> StoreResult<()>;
}

#[derive(Clone)]
pub struct DbPalwPrunedFrontierStore {
    db: Arc<DB>,
    access: CachedDbItem<PalwPrunedFrontierEntry>,
}

impl DbPalwPrunedFrontierStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::PalwPrunedFrontier.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, entry: PalwPrunedFrontierEntry) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &entry)
    }
}

impl PalwPrunedFrontierStoreReader for DbPalwPrunedFrontierStore {
    fn get(&self) -> StoreResult<PalwPrunedFrontierEntry> {
        self.access.read()
    }
}

impl PalwPrunedFrontierStore for DbPalwPrunedFrontierStore {
    fn set(&mut self, entry: PalwPrunedFrontierEntry) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw::PalwPrunedFrontierV1,
        palw_pruned_frontier::{PALW_PRUNING_SNAPSHOT_VERSION, PalwPruningPointSnapshotPayloadV1},
    };
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{ConnBuilder, StoreError};
    use kaspa_hashes::Hash64;

    fn snapshot(pp: Hash64, daa: u64) -> PalwPruningPointSnapshotV1 {
        PalwPruningPointSnapshotV1::new(PalwPruningPointSnapshotPayloadV1 {
            version: PALW_PRUNING_SNAPSHOT_VERSION,
            pruning_point: pp,
            pruning_point_daa_score: daa,
            paid_work_window_daa: 0,
            frontier: PalwPrunedFrontierV1::default(),
            beacon_accumulator: None,
            spam_accumulator: None,
            da_snapshot: None,
            search_availability_snapshot: None,
            compute_registry_snapshot: None,
            active_batches: vec![],
            provider_bonds: vec![],
            paid_work: vec![],
        })
    }

    /// The fresh prefix is absent before first capture. Complete snapshots round-trip and a later
    /// pruning point atomically replaces the singleton (the expected reorg/catch-up behavior).
    #[test]
    fn pruned_frontier_singleton_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwPrunedFrontierStore::new(db);
        assert!(matches!(store.get(), Err(StoreError::KeyNotFound(_))), "empty before first write");

        let first = snapshot(Hash64::from_bytes([0x42; 64]), 900);
        store.set(first.clone()).unwrap();
        assert_eq!(store.get().unwrap(), first);

        let replacement = snapshot(Hash64::from_bytes([0x43; 64]), 1_000);
        store.set(replacement.clone()).unwrap();
        assert_eq!(store.get().unwrap(), replacement);
    }

    /// `set_batch` participates in the same RocksDB commit as the pruning-point pointer. A crash can
    /// therefore observe either both old values or both new values, never a new pp with a stale PALW
    /// frontier.
    #[test]
    fn pruned_frontier_batch_write_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwPrunedFrontierStore::new(db.clone());
        let value = snapshot(Hash64::from_bytes([0x51; 64]), 700);
        let mut batch = WriteBatch::default();
        store.set_batch(&mut batch, value.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get().unwrap(), value);
    }
}
