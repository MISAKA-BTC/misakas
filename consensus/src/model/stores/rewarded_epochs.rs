use std::sync::Arc;

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
/// Written at `commit_utxo_state` for each UTXO-valid chain block (empty for
/// blocks that reward nothing, e.g. every block while the overlay is dormant).
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
}

impl DbRewardedEpochsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::RewardedEpochs.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, keys: Arc<RewardedEpochKeys>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, keys)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl RewardedEpochsStoreReader for DbRewardedEpochsStore {
    fn get(&self, hash: BlockHash) -> Result<Arc<RewardedEpochKeys>, StoreError> {
        self.access.read(hash)
    }
}

impl RewardedEpochsStore for DbRewardedEpochsStore {
    fn insert(&self, hash: BlockHash, keys: Arc<RewardedEpochKeys>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, keys)?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
