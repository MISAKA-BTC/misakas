use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use rocksdb::WriteBatch;

/// `job_nullifier`s paid to a `ReplicaPalw` provider pair by this chain block's coinbase.
///
/// # Storage model
///
/// Unlike the removed `PalwBatchViewV1::job_nullifiers` field, this set is a bounded, block-keyed
/// reward delta:
///
/// | | deleted `job_nullifiers` | this store |
/// |---|---|---|
/// | carried forward | cloned into every descendant's view | block-keyed, never cloned |
/// | who writes an entry | any leaf-chunk transaction | a paid, accepted algo-4 block |
/// | bound | attacker-chosen expiry | `mergeset_size_limit` per block, an existing consensus bound |
/// | readers | none (its bool fed a `continue`) | [`crate::pipeline::virtual_processor`]'s reward dedup |
/// | lifetime | payload-defined | deleted by the pruning processor with other per-block rows |
///
/// The per-block row is the same shape as `rewarded_epochs.rs` (ADR-0009 Addendum B §B.3(c)) and is
/// there for the same reason: the set is PATH-DEPENDENT (whether a block pays a nullifier depends on
/// what its ancestors paid), so it cannot be re-derived from the block alone. Re-deriving it from
/// `acceptance_data_store` + `headers_store` would mean an O(mergeset) header scan for every block in
/// the walk, on the hot path of every block — which is why the delta is recorded instead.
///
/// # Bound
///
/// A block's row has at most one entry per algo-4 source in its mergeset, and the mergeset is
/// consensus-bounded. Each entry additionally costs an accepted algo-4 block that passed every
/// clause check — the network's own rate limit, not a free write.
///
/// # Activation
///
/// Rows are written on PALW networks when accepted algo-4 sources produce
/// `WorkRewardClass::ReplicaPalw`.
pub type PalwPaidWorkIds = Vec<Hash64>;

pub trait PalwPaidWorkStoreReader {
    fn get(&self, hash: BlockHash) -> Result<Arc<PalwPaidWorkIds>, StoreError>;
}

pub trait PalwPaidWorkStore: PalwPaidWorkStoreReader {
    fn insert(&self, hash: BlockHash, ids: Arc<PalwPaidWorkIds>) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of the `PalwPaidWorkStore` trait.
#[derive(Clone)]
pub struct DbPalwPaidWorkStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, Arc<PalwPaidWorkIds>, BlockHasher>,
}

impl DbPalwPaidWorkStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwPaidWork.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, ids: Arc<PalwPaidWorkIds>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, ids)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl PalwPaidWorkStoreReader for DbPalwPaidWorkStore {
    fn get(&self, hash: BlockHash) -> Result<Arc<PalwPaidWorkIds>, StoreError> {
        self.access.read(hash)
    }
}

impl PalwPaidWorkStore for DbPalwPaidWorkStore {
    fn insert(&self, hash: BlockHash, ids: Arc<PalwPaidWorkIds>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, ids)?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    /// Happy path under the PRODUCTION cache mode: a non-empty paid-work list inserts, reads back and
    /// deletes, and a re-insert on the same block hash is REJECTED. The rejection matters: a chain
    /// block writes its reward row exactly once, and a silent overwrite would let a re-processed block
    /// erase the very dedup evidence its descendants read.
    #[test]
    fn insert_get_delete_and_reject_rewrite() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwPaidWorkStore::new(db, CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x11; 64]);
        let ids: Arc<PalwPaidWorkIds> = Arc::new(vec![Hash64::from_bytes([0xaa; 64]), Hash64::from_bytes([0xbb; 64])]);

        assert!(store.get(h).is_err());
        store.insert(h, ids.clone()).unwrap();
        assert_eq!(store.get(h).unwrap().as_slice(), ids.as_slice());
        assert!(store.insert(h, ids.clone()).is_err(), "each chain block writes its paid-work row once");
        store.delete(h).unwrap();
        assert!(store.get(h).is_err());
    }
}
