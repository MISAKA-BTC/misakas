use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_muhash::MuHash;
use rocksdb::WriteBatch;
use std::sync::Arc;

// kaspa-pq: upstream Kaspa stored a `Uint3072` as the on-disk representation
// of the multiplicative-MuHash field and reconstructed `MuHash` from it on
// read. LtHash32_1024 has no compressed-field representation, so we just
// store the `MuHash` directly. The wire format is its serde encoding,
// which is the 4096-byte little-endian LtHash state (see
// `crypto/muhash/src/lib.rs`).

pub trait UtxoMultisetsStoreReader {
    fn get(&self, hash: BlockHash) -> Result<MuHash, StoreError>;
}

pub trait UtxoMultisetsStore: UtxoMultisetsStoreReader {
    fn insert(&self, hash: BlockHash, multiset: MuHash) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `DbUtxoMultisetsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbUtxoMultisetsStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, MuHash, BlockHasher>,
}

impl DbUtxoMultisetsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoMultisets.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, multiset: MuHash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.set_batch(batch, hash, multiset)
    }

    pub fn set_batch(&self, batch: &mut WriteBatch, hash: BlockHash, multiset: MuHash) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), hash, multiset)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl UtxoMultisetsStoreReader for DbUtxoMultisetsStore {
    fn get(&self, hash: BlockHash) -> Result<MuHash, StoreError> {
        self.access.read(hash)
    }
}

impl UtxoMultisetsStore for DbUtxoMultisetsStore {
    fn insert(&self, hash: BlockHash, multiset: MuHash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, multiset)?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
