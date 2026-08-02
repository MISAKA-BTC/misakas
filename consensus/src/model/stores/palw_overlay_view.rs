use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::palw::PalwBatchViewV1;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResultExt;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter, StoreError};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

/// kaspa-pq ADR-0039 PALW (§18.2 / C5 option B) — the block-keyed fork-local batch-lifecycle overlay
/// view. Each block carries `view(B) = view(SP(B)) ⊕ Δ(mergeset(B))`: a child clones its selected
/// parent's [`PalwBatchViewV1`], applies its own mergeset's accepted overlay-tx deltas, retains the
/// still-referenceable set, and persists the result. The algo-4 ticket check resolves batch/leaf/cert
/// facts against `view(selected_parent)` instead of the tip-global `DbPalwStore`, which is the
/// past-relative coordinate the C4/C5 panels require (the tip-global read is a consensus split). Same
/// shape + lifecycle as `DbPalwNullifierStore` (block-keyed, seeded from the selected parent).
///
/// This store is written for every block on the three PALW presets, which use
/// `palw_activation_daa_score = 0`. The
/// builder `commit_palw_overlay_view` is called unconditionally from the body commit
/// (`pipeline/body_processor/processor.rs`), and its only fast-path guard tests
/// `palw_activation_daa_score == u64::MAX` — which never fires at 0.
///
/// `palw_algo4_accept` gates algo-4 header admission, not persistence of the overlay view.
///
/// The store is genuinely empty only on mainnet / testnet-10 / simnet / devnet
/// (`palw_activation_daa_score == u64::MAX`), where the ticket check stays byte-identical.
///
/// Rows exist on PALW networks, so changing [`PalwBatchViewV1`] requires a database-version
/// transition. See `LATEST_DB_VERSION` in `consensus/src/consensus/factory.rs`.
#[derive(Clone)]
pub struct DbPalwOverlayViewStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, Arc<PalwBatchViewV1>, BlockHasher>,
}

impl DbPalwOverlayViewStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwOverlayView.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// The block's carried batch view, or `None` if absent (genesis / a pre-activation parent → the
    /// builder seeds an empty view).
    pub fn view(&self, block: BlockHash) -> Result<Option<Arc<PalwBatchViewV1>>, StoreError> {
        self.access.read(block).optional()
    }

    /// Write `block`'s carried view into the commit `WriteBatch` (atomic with the block commit).
    pub fn set_batch(&self, batch: &mut WriteBatch, block: BlockHash, view: Arc<PalwBatchViewV1>) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), block, view)
    }

    /// Direct (non-batch) write — diagnostics / tests.
    pub fn set(&self, block: BlockHash, view: Arc<PalwBatchViewV1>) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), block, view)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, block: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw::{PalwBatchManifestV1, PalwBatchStatus};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    /// Absent → None; a carried view (a batch registered via the pure delta) round-trips; batch write +
    /// delete work.
    #[test]
    fn overlay_view_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwOverlayViewStore::new(db, CachePolicy::Count(16));
        let block = h(0x21);
        assert!(store.view(block).unwrap().is_none());

        let mut m = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0),
            registration_epoch: 5,
            model_profile_id: h(3),
            runtime_class_id: h(4),
            leaf_count: 100,
            chunk_count: 2,
            leaf_root: h(8),
            descriptor_root: h(6),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(7),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        m.batch_id = m.content_id();
        let mut view = PalwBatchViewV1::new();
        assert!(view.apply_manifest(&m, 5, 256, 64, 2, 6, 6, 0, 1_024, kaspa_consensus_core::palw::PalwManifestSponsorshipV1::INERT));
        store.set(block, Arc::new(view.clone())).unwrap();

        let got = store.view(block).unwrap().unwrap();
        assert_eq!(*got, view);
        assert_eq!(got.entry(&m.batch_id).unwrap().status, PalwBatchStatus::Registering);

        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, block).unwrap();
        store.db.write(batch).unwrap();
        assert!(store.view(block).unwrap().is_none());
    }
}
