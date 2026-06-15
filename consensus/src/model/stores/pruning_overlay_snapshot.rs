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
}

impl DbPruningPointOverlaySnapshotStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::PruningPointOverlaySnapshot.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, snapshot: PruningPointOverlaySnapshot) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &snapshot)
    }
}

impl PruningPointOverlaySnapshotStoreReader for DbPruningPointOverlaySnapshotStore {
    fn get(&self) -> StoreResult<PruningPointOverlaySnapshot> {
        self.access.read()
    }
}

impl PruningPointOverlaySnapshotStore for DbPruningPointOverlaySnapshotStore {
    fn set(&mut self, snapshot: PruningPointOverlaySnapshot) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &snapshot)
    }
}
