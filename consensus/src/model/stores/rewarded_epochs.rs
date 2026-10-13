use std::sync::Arc;

use super::overlay_absent_cache::{OverlayAbsentCache, OverlayCommitTicket};

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{BlockHasher, tx::TransactionOutpoint};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

/// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.3(c)): per-chain-block store of
/// the `(bond_outpoint, epoch)` pairs that the block rewarded in its coinbase
/// validator fan-out (ADR-0013).
///
/// Written at `commit_utxo_state` for non-empty UTXO-valid chain block lists.
/// Empty lists are absent rows, including while the overlay is dormant.
/// A descendant block's coinbase reward computation reads its ancestors' lists
/// within a bounded window to enforce cross-block `(bond, epoch)` reward
/// uniqueness — so each pair is rewarded at most once on the selected chain.
/// Deleted by the pruning processor alongside the other per-block stores.
///
/// The lists are path-dependent (which pairs a block rewards depends on what
/// its ancestors already rewarded), so unlike the bond set they cannot be
/// re-derived from the block alone — hence they are stored rather than
/// recomputed (ADR-0009 Addendum B §B.3(c), "Design S").
pub type RewardedEpochKeys = Vec<(TransactionOutpoint, u64)>;

pub trait RewardedEpochsStoreReader {
    fn get(&self, hash: BlockHash) -> Result<Arc<RewardedEpochKeys>, StoreError>;
}

pub trait RewardedEpochsStore: RewardedEpochsStoreReader {
    fn insert(&self, hash: BlockHash, keys: Arc<RewardedEpochKeys>) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of the `RewardedEpochsStore` trait.
#[derive(Clone)]
pub struct DbRewardedEpochsStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, Arc<RewardedEpochKeys>, BlockHasher>,
    absent: OverlayAbsentCache<BlockHash>,
}

impl DbRewardedEpochsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::RewardedEpochs.into()),
            absent: OverlayAbsentCache::for_policy(cache_policy),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        let mut cloned = Self::new(Arc::clone(&self.db), cache_policy);
        cloned.absent = self.absent.clone_for_policy(cache_policy);
        cloned
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, keys: Arc<RewardedEpochKeys>) -> Result<(), StoreError> {
        self.absent.mutate_batch(&hash, || {
            if self.access.has(hash)? {
                return Err(StoreError::KeyAlreadyExists(hash.to_string()));
            }
            self.access.write(BatchDbWriter::new(batch), hash, keys)
        })
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.absent.mutate_batch(&hash, || self.access.delete(BatchDbWriter::new(batch), hash))
    }

    pub(crate) fn insert_batch_tracked(
        &self,
        batch: &mut WriteBatch,
        hash: BlockHash,
        keys: Arc<RewardedEpochKeys>,
    ) -> Result<OverlayCommitTicket<BlockHash>, StoreError> {
        let (_, ticket) = self.absent.mutate_batch_tracked(&hash, || {
            if self.access.has(hash)? {
                return Err(StoreError::KeyAlreadyExists(hash.to_string()));
            }
            self.access.write(BatchDbWriter::new(batch), hash, keys)
        })?;
        Ok(ticket)
    }

    pub(crate) fn delete_batch_tracked(
        &self,
        batch: &mut WriteBatch,
        hash: BlockHash,
    ) -> Result<OverlayCommitTicket<BlockHash>, StoreError> {
        let (_, ticket) = self.absent.mutate_batch_tracked(&hash, || self.access.delete(BatchDbWriter::new(batch), hash))?;
        Ok(ticket)
    }
}

impl RewardedEpochsStoreReader for DbRewardedEpochsStore {
    fn get(&self, hash: BlockHash) -> Result<Arc<RewardedEpochKeys>, StoreError> {
        self.absent.read(hash, || self.access.read(hash))
    }
}

impl RewardedEpochsStore for DbRewardedEpochsStore {
    fn insert(&self, hash: BlockHash, keys: Arc<RewardedEpochKeys>) -> Result<(), StoreError> {
        self.absent.mutate(&hash, || {
            if self.access.has(hash)? {
                return Err(StoreError::KeyAlreadyExists(hash.to_string()));
            }
            self.access.write(DirectDbWriter::new(&self.db), hash, keys)
        })
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.absent.mutate(&hash, || self.access.delete(DirectDbWriter::new(&self.db), hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;
    use kaspa_utils::mem_size::MemSizeEstimator;

    fn rewarded(n: u32) -> Arc<RewardedEpochKeys> {
        Arc::new((0..n).map(|i| (TransactionOutpoint::new(Hash64::from_bytes([i as u8; 64]), i), i as u64)).collect())
    }

    /// Happy path under the PRODUCTION cache mode (untracked `Count`, see
    /// `consensus::storage`): a non-empty reward list inserts, reads back, and
    /// deletes, and a re-insert on the same block hash is rejected (per-block
    /// append-only).
    #[test]
    fn insert_get_delete_under_count_policy() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbRewardedEpochsStore::new(db, CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x11; 64]);
        let keys = rewarded(3);

        assert!(store.get(h).is_err());
        store.insert(h, keys.clone()).unwrap();
        assert_eq!(store.get(h).unwrap().as_slice(), keys.as_slice());
        // A second insert on the same key is rejected (each chain block writes once).
        assert!(store.insert(h, keys.clone()).is_err());
        store.delete(h).unwrap();
        assert!(store.get(h).is_err());
    }

    #[test]
    fn absence_batch_commit_clone_restore_and_reopen() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbRewardedEpochsStore::new(db.clone(), CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x33; 64]);
        assert!(store.get(h).is_err());
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, h, rewarded(2)).unwrap();
        let sibling = store.clone_with_new_cache(CachePolicy::Count(16));
        // An independently cached reader sees the DB before batch commit.
        assert!(sibling.get(h).is_err());
        db.write(batch).unwrap();
        assert_eq!(sibling.get(h).unwrap().len(), 2);
        assert_eq!(store.get(h).unwrap().len(), 2);
        store.delete(h).unwrap();
        assert!(store.get(h).is_err());
        store.insert(h, rewarded(3)).unwrap();
        assert_eq!(store.get(h).unwrap().len(), 3);
        let reopened = DbRewardedEpochsStore::new(db.clone(), CachePolicy::Count(16));
        assert_eq!(reopened.get(h).unwrap().len(), 3);
        let other = Hash64::from_bytes([0x44; 64]);
        assert!(store.get(other).is_err());
        let writer = store.clone_with_new_cache(CachePolicy::Empty);
        writer.insert(other, rewarded(1)).unwrap();
        assert_eq!(store.get(other).unwrap().len(), 1);
    }

    #[test]
    fn tracked_batch_reverse_commit_and_abandon_use_real_db() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbRewardedEpochsStore::new(db.clone(), CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x37; 64]);
        assert!(store.get(h).is_err());
        let mut insert = WriteBatch::default();
        let insert_ticket = store.insert_batch_tracked(&mut insert, h, rewarded(2)).unwrap();
        let mut delete = WriteBatch::default();
        let delete_ticket = store.delete_batch_tracked(&mut delete, h).unwrap();
        let reader = store.clone_with_new_cache(CachePolicy::Count(16));
        assert!(reader.get(h).is_err());
        db.write(delete).unwrap();
        delete_ticket.committed();
        assert!(reader.get(h).is_err());
        db.write(insert).unwrap();
        insert_ticket.committed();
        assert_eq!(reader.get(h).unwrap().len(), 2);
        assert_eq!(store.get(h).unwrap().len(), 2);
        let mut abandoned = WriteBatch::default();
        let abandoned_ticket = store.delete_batch_tracked(&mut abandoned, h).unwrap();
        drop(abandoned);
        drop(abandoned_ticket);
        // An abandoned delete cannot turn this committed row into cached absence.
        assert_eq!(store.clone_with_new_cache(CachePolicy::Count(16)).get(h).unwrap().len(), 2);
    }

    /// `RewardedEpochKeys` is a `Vec`, so it is UNIT-estimable (its length).
    #[test]
    fn value_is_unit_estimable() {
        assert_eq!(rewarded(4).estimate_mem_units(), 4);
    }

    /// Regression guard for the validator-attestation `virtual-processor` crash
    /// (`utils/src/mem_size.rs:.. not implemented`): the `Vec` value has NO byte
    /// estimation, so this store must use an untracked / units cache policy —
    /// never `tracked_bytes`, which sizes values via `estimate_mem_bytes` and
    /// would panic on the first non-empty reward write (exactly what happened in
    /// production). Pinning the missing byte estimation makes any future switch
    /// to a bytes-tracked policy fail loudly in unit tests instead of on-chain.
    #[test]
    #[should_panic(expected = "not implemented")]
    fn value_has_no_byte_estimation() {
        let _ = rewarded(1).estimate_mem_bytes();
    }
}
