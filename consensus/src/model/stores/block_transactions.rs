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
        const NORMAL_SIG_SIZE: usize = 66;
        let (inputs, outputs) = self.0.iter().fold((0, 0), |(ins, outs), tx| (ins + tx.inputs.len(), outs + tx.outputs.len()));
        // TODO: consider tracking transactions by bytes accurately (preferably by saving the size in a field)
        // We avoid zooming in another level and counting exact bytes for sigs and scripts for performance reasons.
        // Outliers with longer signatures are rare enough and their size is eventually bounded by mempool standards
        // or in the worst case by max block mass.
        // A similar argument holds for spk within outputs, but in this case the constant is already counted through the SmallVec used within.
        inputs * (size_of::<TransactionInput>() + NORMAL_SIG_SIZE)
            + outputs * size_of::<TransactionOutput>()
            + self.0.len() * size_of::<Transaction>()
            + size_of::<Vec<Transaction>>()
            + size_of::<Self>()
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
