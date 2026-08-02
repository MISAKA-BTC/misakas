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
//! **Fork-relative snapshot chain** (`block → PalwPcpbChainStateV1`, prefix 71): static-audit
//! finding C-01. The epoch-keyed history above is idempotent-by-VALUE — its own contract says the
//! value is "a function of the chain that first closed this epoch" — so two forks that both close
//! epoch `e` write DIFFERENT commitments under one key and whichever committed last answers for
//! every reader, including clause 12 validating a block on the other fork. That made leaf acceptance
//! depend on fork receive order. Clause 12 now walks this block-keyed chain back from the CANDIDATE's
//! selected parent, so the answer is a function of the candidate's own past.
//!
//! The epoch-keyed history keeps its job as the BURIED half: the walk falls through to it only after
//! running off the retained block rows (pruning deleted them), i.e. for epochs below the pruning
//! point, where the value is final on every honest node and no fork can differ. That is also what
//! keeps the pruning-snapshot carry meaningful — a joiner answers pruned-epoch leaves from the
//! carried rows, and everything above its boundary from the chain it built itself.
//!
//! All three epoch/anchor-keyed stores are bounded: swept by the pruning/writer pass at the
//! snapshot-history window (`palw_provider_snapshot_history_window_epochs` — the beacon window + the
//! snapshot lag `k`); the block-keyed chain is deleted per block by the pruning pass, like the beacon
//! state rows it mirrors. Reads outside the retained window return `None` and every PCPB caller
//! treats `None` as REJECT (fail-closed) — substituting a live value for an unresolvable epoch is
//! precisely the grindable state D3-b forbids.

use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::palw::{PalwPcpbChainStateV1, PalwProviderSnapshotEntry, PalwSnapshotCommitment};
use kaspa_consensus_core::{BlockHash, BlockHasher};
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

/// The PCPB context column families: three selected-chain-reconciled rows keyed by epoch/anchor,
/// plus the block-keyed fork-relative snapshot chain (prefix 71).
#[derive(Clone)]
pub struct DbPalwPcpbStore {
    snapshot_history: CachedDbAccess<U64Key, Arc<PalwSnapshotCommitment>>,
    /// The entry set each commitment was built from — a PRODUCER aid (see the prefix-70 doc). Not
    /// consensus data: nothing verifies against it, and its absence costs production help for that
    /// epoch, never verification.
    snapshot_entries: CachedDbAccess<U64Key, Arc<PalwProviderSnapshotEntries>>,
    acommit: CachedDbAccess<PalwACommitKey, Arc<u64>>,
    /// Static-audit finding C-01 — the fork-relative snapshot history, keyed by chain block.
    chain: CachedDbAccess<BlockHash, Arc<PalwPcpbChainStateV1>, BlockHasher>,
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
            acommit: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwACommitRegistry.into()),
            chain: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwPcpbChainState.into()),
        }
    }

    // ---- fork-relative snapshot chain (static-audit C-01) ----

    /// `block`'s row in the fork-relative provider-snapshot history, or `None` when the block has
    /// none — which the walk reads as "this history is truncated here by pruning", never as "the
    /// epoch was never closed". See [`kaspa_consensus_core::palw::PalwForkRelativeOutcome`].
    pub fn chain_state(&self, block: BlockHash) -> Result<Option<PalwPcpbChainStateV1>, StoreError> {
        Ok(self.chain.read(block).optional()?.map(|state| (*state).clone()))
    }

    pub fn set_chain_state_batch(
        &self,
        batch: &mut WriteBatch,
        block: BlockHash,
        state: PalwPcpbChainStateV1,
    ) -> Result<(), StoreError> {
        self.chain.write(BatchDbWriter::new(batch), block, Arc::new(state))
    }

    pub fn delete_chain_state_batch(&self, batch: &mut WriteBatch, block: BlockHash) -> Result<(), StoreError> {
        self.chain.delete(BatchDbWriter::new(batch), block)
    }

    // ---- per-epoch provider snapshot history (the BURIED half) ----

    /// The bond-weighted provider snapshot commitment of `epoch`, or `None` when the epoch is
    /// outside the retained window (or predates activation). **`None` is fail-closed by contract**:
    /// a clause-12/13 check that cannot resolve the epoch its leaf anchored to must refuse the
    /// leaf, never substitute a current snapshot.
    ///
    /// Static-audit C-01: clause 12 consults this only after its fork-relative walk ran off the
    /// retained block rows, i.e. for epochs buried under the pruning point, where every honest node
    /// holds the same final value. Above the pruning point the walk always answers first, so a live
    /// fork can no longer read a sibling's commitment out of this shared key.
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

    /// Static-audit C-01 — the block-keyed chain: absent → `None` (which the walk reads as the
    /// pruning boundary, never as an answer), set → read back, deleted by the pruning pass.
    ///
    /// The contrast with the test above is the whole point of the pair. Two forks closing epoch 7
    /// share ONE epoch-keyed row and the second write erases the first; here they hold two rows and
    /// neither can be reached from the other's history.
    #[test]
    fn pcpb_chain_rows_are_block_keyed_and_sibling_disjoint() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwPcpbStore::new(db.clone(), CachePolicy::Empty);
        let (left, right) = (h(0x11), h(0x12));

        assert_eq!(store.chain_state(left).unwrap(), None, "a rowless block is the boundary signal");

        let row = |closed: Vec<(u64, PalwSnapshotCommitment)>, prev: Option<Hash64>| PalwPcpbChainStateV1 {
            version: 1,
            closed_snapshots: closed,
            prev_closer: prev,
        };
        let mut batch = WriteBatch::default();
        store.set_chain_state_batch(&mut batch, left, row(vec![(7, commitment(0xAA, 100))], Some(h(0x10)))).unwrap();
        store.set_chain_state_batch(&mut batch, right, row(vec![(7, commitment(0xBB, 400))], Some(h(0x10)))).unwrap();
        db.write(batch).unwrap();

        assert_eq!(store.chain_state(left).unwrap().unwrap().closed_snapshots, vec![(7, commitment(0xAA, 100))]);
        assert_eq!(store.chain_state(right).unwrap().unwrap().closed_snapshots, vec![(7, commitment(0xBB, 400))]);
        assert_eq!(store.chain_state(left).unwrap().unwrap().prev_closer, Some(h(0x10)));

        // Pruning removes the row per block, which is what turns a deep walk into `Buried`.
        let mut batch = WriteBatch::default();
        store.delete_chain_state_batch(&mut batch, left).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.chain_state(left).unwrap(), None);
        assert!(store.chain_state(right).unwrap().is_some(), "pruning one block leaves its sibling alone");
    }
}
