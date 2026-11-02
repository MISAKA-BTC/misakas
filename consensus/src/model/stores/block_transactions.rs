use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::tx::{TransactionInput, TransactionOutput};
use kaspa_consensus_core::{BlockHasher, tx::Transaction};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub trait BlockTransactionsStoreReader {
    fn get(&self, hash: BlockHash) -> Result<kaspa_consensus_core::block::BlockTransactionsV3, StoreError>;
}

pub trait BlockTransactionsStore: BlockTransactionsStoreReader {
    // This is append only
    fn insert(&self, hash: BlockHash, transactions: kaspa_consensus_core::block::BlockTransactionsV3) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

#[derive(Clone, Serialize, Deserialize)]
struct BlockBody(kaspa_consensus_core::block::BlockTransactionsV3);

impl MemSizeEstimator for BlockBody {
    fn estimate_mem_bytes(&self) -> usize {
        // This is the immutable allocation retained by a cache entry, not a
        // serialized-length estimate. PQ P2PKH witnesses are normally 7,226 B,
        // and their 69 B output scripts spill the 35 B inline SmallVec.
        let mut bytes = size_of::<Self>()
            .saturating_add(size_of::<Vec<Transaction>>())
            .saturating_add(2 * size_of::<usize>()) // Arc strong/weak counters
            .saturating_add(self.0.capacity().saturating_mul(size_of::<Transaction>()));
        for tx in self.0.iter() {
            bytes = bytes
                .saturating_add(tx.inputs.capacity().saturating_mul(size_of::<TransactionInput>()))
                .saturating_add(tx.outputs.capacity().saturating_mul(size_of::<TransactionOutput>()))
                .saturating_add(tx.payload.capacity());
            for input in &tx.inputs {
                bytes = bytes.saturating_add(input.signature_script.capacity());
            }
            for output in &tx.outputs {
                bytes = bytes.saturating_add(output.script_public_key.heap_capacity_bytes());
            }
        }
        bytes
    }
}

/// A DB + cache implementation of `BlockTransactionsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbBlockTransactionsStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, BlockBody, BlockHasher>,
}

impl DbBlockTransactionsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::BlockTransactions.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }

    pub fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        hash: BlockHash,
        transactions: kaspa_consensus_core::block::BlockTransactionsV3,
    ) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, BlockBody(transactions))?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl BlockTransactionsStoreReader for DbBlockTransactionsStore {
    fn get(&self, hash: BlockHash) -> Result<kaspa_consensus_core::block::BlockTransactionsV3, StoreError> {
        Ok(self.access.read(hash)?.0)
    }
}

impl BlockTransactionsStore for DbBlockTransactionsStore {
    fn insert(&self, hash: BlockHash, transactions: kaspa_consensus_core::block::BlockTransactionsV3) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, BlockBody(transactions))?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}

#[cfg(test)]
#[path = "block_transactions/retained_body_v3_tests.rs"]
mod retained_body_v3_tests;

#[cfg(test)]
#[path = "block_transactions/capacity_weight_tests.rs"]
mod capacity_weight_tests;
