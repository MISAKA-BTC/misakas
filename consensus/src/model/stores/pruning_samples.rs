use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

pub trait PruningSamplesStoreReader {
    fn pruning_sample_from_pov(&self, hash: BlockHash) -> Result<BlockHash, StoreError>;
}

pub trait PruningSamplesStore: PruningSamplesStoreReader {
    // This is append only
    fn insert(&self, hash: BlockHash, pruning_sample_from_pov: BlockHash) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `PruningSamplesStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbPruningSamplesStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, BlockHash, BlockHasher>,
}

impl DbPruningSamplesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PruningSamples.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, pruning_sample_from_pov: BlockHash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, pruning_sample_from_pov)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl PruningSamplesStoreReader for DbPruningSamplesStore {
    fn pruning_sample_from_pov(&self, hash: BlockHash) -> Result<BlockHash, StoreError> {
        self.access.read(hash)
    }
}

impl PruningSamplesStore for DbPruningSamplesStore {
    fn insert(&self, hash: BlockHash, pruning_sample_from_pov: BlockHash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, pruning_sample_from_pov)?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
