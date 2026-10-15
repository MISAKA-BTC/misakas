//! kaspa-pq ADR-0022: singleton store for the [`PruningPointOverlaySnapshot`] —
//! the DNS/PoS-v2 [`OverlaySnapshot`] taken as-of the current pruning point.
//!
//! Captured at pruning-advance (in the pruning processor), *before* the below-pp
//! overlay rows (`rewarded_epochs` / `block_quality_pool` / `reserve_balance`) are
//! deleted, so the historical overlay state survives pruning. Two consumers:
//!   * **serving** — a peer streams it during another node's headers-proof IBD;
//!   * **`compute_overlay_snapshot`** — when its selected-chain walk reaches the
//!     pruning point it appends this snapshot's below-pp window (the walk cannot
//!     traverse below the pruning point — there is no reachability there after a
//!     prune or a pruned-IBD import), so post-pruning blocks reproduce the
//!     committed `overlay_commitment_root` (`c == v`).
//!
//! Mirrors the [`super::dns_state`] singleton pattern. Before the first write
//! [`PruningPointOverlaySnapshotStoreReader::get`] returns `KeyNotFound`.

use super::overlay_absent_cache::{OverlayAbsentCache, OverlayCommitTicket};
use kaspa_consensus_core::dns_finality::PruningPointOverlaySnapshot;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;
use std::sync::Arc;

pub trait PruningPointOverlaySnapshotStoreReader {
    fn get(&self) -> StoreResult<PruningPointOverlaySnapshot>;
}

pub trait PruningPointOverlaySnapshotStore: PruningPointOverlaySnapshotStoreReader {
    fn set(&mut self, snapshot: PruningPointOverlaySnapshot) -> StoreResult<()>;
}

#[derive(Clone)]
pub struct DbPruningPointOverlaySnapshotStore {
    db: Arc<DB>,
    access: CachedDbItem<PruningPointOverlaySnapshot>,
    absent: OverlayAbsentCache<()>,
}

impl DbPruningPointOverlaySnapshotStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbItem::new(db, DatabaseStorePrefixes::PruningPointOverlaySnapshot.into()),
            absent: OverlayAbsentCache::new(1),
        }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        let mut cloned = Self::new(Arc::clone(&self.db));
        cloned.absent = self.absent.clone_with_new_cache(1);
        cloned
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, snapshot: PruningPointOverlaySnapshot) -> StoreResult<()> {
        self.absent.mutate_batch(&(), || self.access.write(BatchDbWriter::new(batch), &snapshot))
    }

    pub(crate) fn set_batch_tracked(
        &mut self,
        batch: &mut WriteBatch,
        snapshot: PruningPointOverlaySnapshot,
    ) -> StoreResult<OverlayCommitTicket<()>> {
        let (_, ticket) = self.absent.mutate_batch_tracked(&(), || self.access.write(BatchDbWriter::new(batch), &snapshot))?;
        Ok(ticket)
    }
}

impl PruningPointOverlaySnapshotStoreReader for DbPruningPointOverlaySnapshotStore {
    fn get(&self) -> StoreResult<PruningPointOverlaySnapshot> {
        self.absent.read((), || self.access.read())
    }
}

impl PruningPointOverlaySnapshotStore for DbPruningPointOverlaySnapshotStore {
    fn set(&mut self, snapshot: PruningPointOverlaySnapshot) -> StoreResult<()> {
        self.absent.mutate(&(), || self.access.write(DirectDbWriter::new(&self.db), &snapshot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::dns_finality::OverlaySnapshot;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    use kaspa_hashes::Hash64;

    #[test]
    fn singleton_absence_queued_commit_clone_and_reopen() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPruningPointOverlaySnapshotStore::new(db.clone());
        assert!(store.get().is_err());
        let snapshot = PruningPointOverlaySnapshot {
            pruning_point: Hash64::from_bytes([0x45; 64]),
            snapshot: OverlaySnapshot { bonds: vec![], reserve_balance: 42, window: vec![] },
        };
        let mut batch = WriteBatch::default();
        store.set_batch(&mut batch, snapshot.clone()).unwrap();
        let sibling = store.clone_with_new_cache();
        assert!(sibling.get().is_err());
        db.write(batch).unwrap();
        assert_eq!(sibling.get().unwrap(), snapshot);
        assert_eq!(store.get().unwrap(), snapshot);
        assert_eq!(DbPruningPointOverlaySnapshotStore::new(db).get().unwrap(), snapshot);
    }

    #[test]
    fn singleton_direct_write_invalidates_independently_cached_absence() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPruningPointOverlaySnapshotStore::new(db);
        assert!(store.get().is_err());
        let mut writer = store.clone_with_new_cache();
        let snapshot = PruningPointOverlaySnapshot {
            pruning_point: Hash64::from_bytes([0x46; 64]),
            snapshot: OverlaySnapshot { bonds: vec![], reserve_balance: 99, window: vec![] },
        };
        writer.set(snapshot.clone()).unwrap();
        assert_eq!(store.get().unwrap(), snapshot);
    }

    #[test]
    fn singleton_tracked_commit_abandon_and_reverse_overwrite_db_values() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPruningPointOverlaySnapshotStore::new(db.clone());
        let first = PruningPointOverlaySnapshot {
            pruning_point: Hash64::from_bytes([0x47; 64]),
            snapshot: OverlaySnapshot { bonds: vec![], reserve_balance: 42, window: vec![] },
        };
        let second = PruningPointOverlaySnapshot {
            pruning_point: Hash64::from_bytes([0x48; 64]),
            snapshot: OverlaySnapshot { bonds: vec![], reserve_balance: 99, window: vec![] },
        };
        let mut batch1 = WriteBatch::default();
        let ticket1 = store.set_batch_tracked(&mut batch1, first.clone()).unwrap();
        let mut batch2 = WriteBatch::default();
        let ticket2 = store.set_batch_tracked(&mut batch2, second.clone()).unwrap();
        let reader = store.clone_with_new_cache();
        assert!(reader.get().is_err());
        db.write(batch2).unwrap();
        ticket2.committed();
        assert_eq!(reader.get().unwrap(), second);
        db.write(batch1).unwrap();
        ticket1.committed();
        // Use an empty positive cache: this ticket mechanism owns negative
        // coherence, not the generic item's optimistic positive-cache ordering.
        assert_eq!(store.clone_with_new_cache().get().unwrap(), first);
        let mut abandoned = WriteBatch::default();
        let ticket = store.set_batch_tracked(&mut abandoned, second).unwrap();
        drop(abandoned);
        drop(ticket);
        assert_eq!(store.clone_with_new_cache().get().unwrap(), first);
    }
}
