//! ADR-0045 D3-b (docs/palw-pcpb-leaf-v2-wiring-design.md §2) — the two PCPB context stores.
//!
//! **Provider snapshot history** (`epoch → PalwSnapshotCommitment`, prefix 68): the clause-0
//! "independently resolved" side of `palw_dispatch_evidence_valid`. Written as each epoch closes
//! along the selected chain — inside the provider-bond registry reconciliation
//! (`stage_palw_provider_bond_mutations`), which is the ONLY coordinate where "the registry as of
//! this chain block" is a well-defined pure function of the selected chain (the beacon writer runs
//! per accepted chain block BEFORE the registry reconciles the same chain path, so deriving there
//! would read a registry from the previous virtual commit and nodes batching different numbers of
//! chain blocks per commit would derive different roots — a clause-0 consensus split).
//!
//! **A-commit registry** (`a_commit → accepted epoch`, prefix 69): the self-serial ordering anchor.
//! First-accept-wins along the selected chain; a reorg re-derives rows exactly like the snapshot
//! history (the reconciliation replays the new chain's accepted anchors). Clause 12 requires
//! equality with the leaf's declared `a_commit_epoch` — BOTH directions of epoch-grinding die on
//! that equality, which is why the row's epoch is the ACCEPTING block's consensus-derived epoch and
//! never a declared field.
//!
//! Both stores are bounded: swept by the pruning/writer pass at the snapshot-history window
//! (`palw_provider_snapshot_history_window_epochs` — the beacon window + the snapshot lag `k`).
//! Reads outside the retained window return `None` and every PCPB caller treats `None` as REJECT
//! (fail-closed) — substituting a live value for an unresolvable epoch is precisely the grindable
//! state D3-b forbids.

use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::palw::{PalwProviderSnapshotEntry, PalwSnapshotCommitment};
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResultExt;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, StoreError};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::{HASH64_SIZE, Hash64};
use rocksdb::WriteBatch;

use super::U64Key;

/// Fixed-width `Hash64` key for the A-commit registry (same scheme as `PalwLeafKey`).
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
pub struct PalwACommitKey([u8; HASH64_SIZE]);

impl PalwACommitKey {
    pub fn new(a_commit: &Hash64) -> Self {
        Self(a_commit.as_bytes())
    }
}

impl AsRef<[u8]> for PalwACommitKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for PalwACommitKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Hash64::from_bytes(self.0).fmt(f)
    }
}

impl From<Hash64> for PalwACommitKey {
    fn from(h: Hash64) -> Self {
        Self::new(&h)
    }
}

/// The two PCPB context column families. Direct (non-block-keyed) rows, selected-chain reconciled.
#[derive(Clone)]
pub struct DbPalwPcpbStore {
    snapshot_history: CachedDbAccess<U64Key, Arc<PalwSnapshotCommitment>>,
    /// The entry set each commitment was built from — a PRODUCER aid (see the prefix-70 doc). Not
    /// consensus data: nothing verifies against it, and its absence costs production help for that
    /// epoch, never verification.
    snapshot_entries: CachedDbAccess<U64Key, Arc<PalwProviderSnapshotEntries>>,
    acommit: CachedDbAccess<PalwACommitKey, Arc<u64>>,
}

/// Newtype so the entry vector can carry the `MemSizeEstimator` the cached store requires.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwProviderSnapshotEntries(pub Vec<PalwProviderSnapshotEntry>);

impl kaspa_utils::mem_size::MemSizeEstimator for PalwProviderSnapshotEntries {}

impl DbPalwPcpbStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            snapshot_history: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwProviderSnapshotHistory.into()),
            snapshot_entries: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwProviderSnapshotEntries.into()),
            acommit: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwACommitRegistry.into()),
        }
    }

    // ---- per-epoch provider snapshot history ----

    /// The bond-weighted provider snapshot commitment of `epoch`, or `None` when the epoch is
    /// outside the retained window (or predates activation). **`None` is fail-closed by contract**:
    /// a clause-12/13 check that cannot resolve the epoch its leaf anchored to must refuse the
    /// leaf, never substitute a current snapshot.
    pub fn snapshot_at(&self, epoch: u64) -> Result<Option<PalwSnapshotCommitment>, StoreError> {
        Ok(self.snapshot_history.read(epoch.into()).optional()?.map(|c| *c))
    }

    /// Record `epoch`'s snapshot commitment, atomically with the registry reconciliation that
    /// derived it. Idempotent-by-value and reorg-overwritable for the same reason the beacon seed
    /// history is: the per-epoch value is a function of "the selected chain that closed this
    /// epoch", so the new chain's boundary derivation is the correct one.
    pub fn set_snapshot_batch(
        &self,
        batch: &mut WriteBatch,
        epoch: u64,
        commitment: PalwSnapshotCommitment,
    ) -> Result<(), StoreError> {
        self.snapshot_history.write(BatchDbWriter::new(batch), epoch.into(), Arc::new(commitment))
    }

    pub fn delete_snapshot_batch(&self, batch: &mut WriteBatch, epoch: u64) -> Result<(), StoreError> {
        self.snapshot_entries.delete(BatchDbWriter::new(batch), epoch.into())?;
        self.snapshot_history.delete(BatchDbWriter::new(batch), epoch.into())
    }

    /// The canonical entry set of `epoch`, for producers assembling membership witnesses. `None`
    /// when the epoch is outside the window OR when this node imported the boundary from a pruning
    /// snapshot (which carries commitments, not entries) — both are "cannot help you produce", never
    /// "cannot verify".
    pub fn snapshot_entries_at(&self, epoch: u64) -> Result<Option<Vec<PalwProviderSnapshotEntry>>, StoreError> {
        Ok(self.snapshot_entries.read(epoch.into()).optional()?.map(|rows| rows.0.clone()))
    }

    pub fn set_snapshot_entries_batch(
        &self,
        batch: &mut WriteBatch,
        epoch: u64,
        entries: Vec<PalwProviderSnapshotEntry>,
    ) -> Result<(), StoreError> {
        self.snapshot_entries.write(BatchDbWriter::new(batch), epoch.into(), Arc::new(PalwProviderSnapshotEntries(entries)))
    }

    /// Every retained `(epoch, commitment)` row in ascending epoch order — the pruning-snapshot
    /// carry, the sweep, and the bounded-window audit read this.
    pub fn snapshot_history(&self) -> Result<Vec<(u64, PalwSnapshotCommitment)>, StoreError> {
        let mut rows = Vec::new();
        for entry in self.snapshot_history.iterator() {
            let (key, commitment) = entry.map_err(|err| StoreError::DataInconsistency(format!("snapshot history scan: {err}")))?;
            let bytes: [u8; 8] =
                key.as_ref().try_into().map_err(|_| StoreError::DataInconsistency("snapshot history key width".into()))?;
            // `U64Key` is little-endian, so raw key order is NOT epoch order — hence the explicit sort.
            rows.push((u64::from_le_bytes(bytes), *commitment));
        }
        rows.sort_unstable_by_key(|(epoch, _)| *epoch);
        Ok(rows)
    }

    // ---- A-commit registry ----

    /// The epoch `a_commit` was first accepted on the selected chain, or `None` if it never was (or
    /// its row left the retained window). `None` is fail-closed: the self arm rejects.
    pub fn acommit_epoch(&self, a_commit: &Hash64) -> Result<Option<u64>, StoreError> {
        Ok(self.acommit.read(PalwACommitKey::new(a_commit)).optional()?.map(|epoch| *epoch))
    }

    pub fn set_acommit_batch(&self, batch: &mut WriteBatch, a_commit: &Hash64, epoch: u64) -> Result<(), StoreError> {
        self.acommit.write(BatchDbWriter::new(batch), PalwACommitKey::new(a_commit), Arc::new(epoch))
    }

    pub fn delete_acommit_batch(&self, batch: &mut WriteBatch, a_commit: &Hash64) -> Result<(), StoreError> {
        self.acommit.delete(BatchDbWriter::new(batch), PalwACommitKey::new(a_commit))
    }

    /// Every retained `(a_commit, epoch)` row, unordered — the sweep and the pruning carry read
    /// this. Bounded by the sweep window (rows older than the snapshot-history floor are deleted),
    /// so the scan is over live-window anchors only, not chain history.
    pub fn acommit_rows(&self) -> Result<Vec<(Hash64, u64)>, StoreError> {
        let mut rows = Vec::new();
        for entry in self.acommit.iterator() {
            let (key, epoch) = entry.map_err(|err| StoreError::DataInconsistency(format!("a-commit registry scan: {err}")))?;
            let bytes: [u8; HASH64_SIZE] =
                key.as_ref().try_into().map_err(|_| StoreError::DataInconsistency("a-commit registry key width".into()))?;
            rows.push((Hash64::from_bytes(bytes), *epoch));
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn commitment(b: u8, total: u128) -> PalwSnapshotCommitment {
        PalwSnapshotCommitment { snapshot_root: h(b), assignment_root: h(b.wrapping_add(1)), total_bond: total, provider_count: 3 }
    }

    /// D3-b — the snapshot history answers past epochs, is overwritable by a reorg re-derivation,
    /// and falls closed outside the window; the A-commit registry round-trips, unwinds, and falls
    /// closed for unregistered anchors. Mirrors the D3-a beacon-seed-history contract.
    #[test]
    fn pcpb_context_stores_round_trip_sweep_and_fall_closed() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwPcpbStore::new(db.clone(), CachePolicy::Empty);

        // Unwritten epochs / anchors are `None` — PCPB fails closed rather than substituting.
        assert_eq!(store.snapshot_at(5).unwrap(), None);
        assert_eq!(store.acommit_epoch(&h(0xAA)).unwrap(), None);

        let mut batch = WriteBatch::default();
        for (epoch, c) in [(5u64, commitment(0x55, 100)), (6, commitment(0x66, 150)), (7, commitment(0x77, 90))] {
            store.set_snapshot_batch(&mut batch, epoch, c).unwrap();
        }
        store.set_acommit_batch(&mut batch, &h(0xAA), 6).unwrap();
        store.set_acommit_batch(&mut batch, &h(0xAB), 7).unwrap();
        db.write(batch).unwrap();

        assert_eq!(store.snapshot_at(5).unwrap(), Some(commitment(0x55, 100)));
        assert_eq!(store.snapshot_at(7).unwrap(), Some(commitment(0x77, 90)));
        assert_eq!(
            store.snapshot_history().unwrap(),
            vec![(5, commitment(0x55, 100)), (6, commitment(0x66, 150)), (7, commitment(0x77, 90))]
        );
        assert_eq!(store.acommit_epoch(&h(0xAA)).unwrap(), Some(6));

        // Reorg: the new selected chain re-closes epoch 7 with a different registry state, and its
        // derivation is the correct one (same overwrite semantics as the beacon seed history).
        let mut batch = WriteBatch::default();
        store.set_snapshot_batch(&mut batch, 7, commitment(0x7E, 120)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.snapshot_at(7).unwrap(), Some(commitment(0x7E, 120)));

        // A reorg that drops the anchor's accepting block unwinds the registry row entirely — the
        // anchor is simply "never accepted" on the new chain until some block re-includes it.
        let mut batch = WriteBatch::default();
        store.delete_acommit_batch(&mut batch, &h(0xAB)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.acommit_epoch(&h(0xAB)).unwrap(), None);

        // Sweeping the window floor removes older rows and leaves the rest intact — swept reads
        // fall closed, never stale.
        let mut batch = WriteBatch::default();
        store.delete_snapshot_batch(&mut batch, 5).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.snapshot_at(5).unwrap(), None, "swept epochs fall closed, not stale");
        assert_eq!(store.snapshot_history().unwrap(), vec![(6, commitment(0x66, 150)), (7, commitment(0x7E, 120))]);
        assert_eq!(store.acommit_rows().unwrap(), vec![(h(0xAA), 6)]);
    }
}
