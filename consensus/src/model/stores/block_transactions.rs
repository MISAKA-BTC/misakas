use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::tx::{TransactionInput, TransactionOutput};
use kaspa_consensus_core::{BlockHasher, tx::Transaction};
use kaspa_consensus_core::{
    palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1},
    subnets::SUBNETWORK_ID_SIZE,
};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod tx_id_projection;

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

    /// Read stored cached IDs without cold-decoding a transaction body. A warm
    /// Arc cache hit is borrowed; a cold row remains pinned through both scans.
    /// This ticket covers only the newly owned IDs, not RocksDB pin/cache memory.
    pub fn rpc_transaction_ids_v1(
        &self,
        hash: BlockHash,
    ) -> Result<kaspa_consensus_core::block::RetainedTransactionIdsV1, StoreError> {
        use kaspa_consensus_core::block::RetainedTransactionIdsV1;
        use kaspa_core::memory_budget::{MemoryClass, global_memory_budget};
        let reserve = |bytes| {
            global_memory_budget()
                .try_reserve(MemoryClass::Snapshot, bytes)
                .map_err(|why| StoreError::LocalResourceUnavailable(why.to_string()))
        };
        if let Some(body) = self.access.read_from_cache(hash) {
            let bytes = tx_id_projection::id_reservation_bytes::<kaspa_consensus_core::tx::TransactionId>(body.0.len())
                .ok_or_else(|| StoreError::LocalResourceUnavailable("transaction IDs allocation overflow".into()))?;
            let mut ids = RetainedTransactionIdsV1 { ids: Vec::new(), memory: Some(reserve(bytes)?) };
            ids.ids.try_reserve_exact(body.0.len()).map_err(|why| StoreError::LocalResourceUnavailable(why.to_string()))?;
            ids.ids.extend(body.0.iter().map(Transaction::id));
            ids.memory.as_ref().unwrap().materialized();
            return Ok(ids);
        }
        let prefix: Vec<u8> = DatabaseStorePrefixes::BlockTransactions.into();
        let key = kaspa_database::prelude::DbKey::new(&prefix, hash);
        let bytes = self.db.get_pinned(&key)?.ok_or(StoreError::KeyNotFound(key))?;
        let ids =
            tx_id_projection::project_ids(&bytes, reserve, kaspa_consensus_core::tx::TransactionId::from_bytes).map_err(|why| {
                match why {
                    tx_id_projection::ProjectionError::Codec(error) => StoreError::DataInconsistency(error.to_string()),
                    tx_id_projection::ProjectionError::Capacity(error) => error,
                    tx_id_projection::ProjectionError::Allocation(error) => StoreError::LocalResourceUnavailable(error.to_string()),
                }
            })?;
        ids.lease.materialized();
        Ok(RetainedTransactionIdsV1 { ids: ids.ids, memory: Some(ids.lease) })
    }

    pub fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }

    /// Borrow the existing block body and decode only the accepted transaction
    /// after checking its typed allocation bound. Do not populate the cache.
    pub(crate) fn archive_transaction_v2(
        &self,
        block: BlockHash,
        index: u32,
        maximum_bytes: u64,
        maximum_heap: u64,
    ) -> Result<(ArchivedTransactionV2, u64), StoreError> {
        self.archive_transaction_with_limits_v2(block, index, maximum_bytes, maximum_heap)?
            .map_err(|why| StoreError::LocalResourceUnavailable(why.into()))
    }

    /// The inner error denotes a deterministic paid limit. A missing archive,
    /// bad framing or local allocator refusal still uses the outer error.
    pub(crate) fn archive_transaction_with_limits_v2(
        &self,
        block: BlockHash,
        index: u32,
        maximum_bytes: u64,
        maximum_heap: u64,
    ) -> Result<Result<(ArchivedTransactionV2, u64), &'static str>, StoreError> {
        let prefix = [u8::from(DatabaseStorePrefixes::BlockTransactions)];
        let key = kaspa_database::prelude::DbKey::new(&prefix, block);
        let raw = self.db.get_pinned(&key)?.ok_or_else(|| StoreError::KeyNotFound(key))?;
        let bytes = raw.len() as u64;
        if bytes > maximum_bytes {
            return Ok(Err("block archive exceeds replay byte limit"));
        }
        let (wire, heap) = archived_transaction_wire_v2(&raw, index)?
            .ok_or_else(|| StoreError::DataInconsistency("accepted transaction index outside archived block".into()))?;
        if heap > maximum_heap {
            return Ok(Err("archived transaction exceeds replay heap limit"));
        }
        let lease = reserve_native_requests_v1(heap)
            .map_err(|e| StoreError::LocalResourceUnavailable(e.to_string()))?
            .ok_or_else(|| StoreError::LocalResourceUnavailable("archive transaction allocator is not armed".into()))?;
        let transaction = bincode::deserialize(wire)?;
        Ok(Ok((ArchivedTransactionV2 { transaction, heap, lease }, bytes)))
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

pub(crate) struct ArchivedTransactionV2 {
    pub transaction: Transaction,
    pub heap: u64,
    pub lease: NativeDemandLeaseV1,
}

/// Scan the existing bincode BlockBody/Transaction representations. Counts
/// cannot allocate or advance past a complete field. Even when the selected
/// transaction comes first, validate the rest of the stored framing.
fn archived_transaction_wire_v2(raw: &[u8], index: u32) -> Result<Option<(&[u8], u64)>, StoreError> {
    fn invalid() -> StoreError {
        StoreError::DataInconsistency("malformed archived block transactions".into())
    }
    fn take<'a>(raw: &mut &'a [u8], count: usize) -> Result<&'a [u8], StoreError> {
        let (head, tail) = raw.split_at_checked(count).ok_or_else(invalid)?;
        *raw = tail;
        Ok(head)
    }
    fn count(raw: &mut &[u8]) -> Result<u64, StoreError> {
        Ok(u64::from_le_bytes(take(raw, 8)?.try_into().map_err(|_| invalid())?))
    }
    fn blob(raw: &mut &[u8]) -> Result<u64, StoreError> {
        let size = count(raw)?;
        take(raw, usize::try_from(size).map_err(|_| invalid())?)?;
        Ok(size)
    }
    fn hash(raw: &mut &[u8]) -> Result<(), StoreError> {
        if blob(raw)? != 64 {
            return Err(invalid());
        }
        Ok(())
    }
    fn grow(heap: &mut u64, count: u64, unit: usize) -> Result<(), StoreError> {
        *heap = count.checked_mul(unit as u64).and_then(|n| n.checked_mul(2)).and_then(|n| heap.checked_add(n)).ok_or_else(invalid)?;
        Ok(())
    }

    let mut cursor = raw;
    let transactions = count(&mut cursor)?;
    let mut selected = None;
    for position in 0..transactions {
        let start = cursor;
        take(&mut cursor, 2)?; // version
        let mut heap = size_of::<Transaction>() as u64;
        let inputs = count(&mut cursor)?;
        grow(&mut heap, inputs, size_of::<TransactionInput>())?;
        for _ in 0..inputs {
            hash(&mut cursor)?;
            take(&mut cursor, 4)?; // outpoint index
            grow(&mut heap, blob(&mut cursor)?, 1)?;
            take(&mut cursor, 8 + 1)?; // sequence and sigops
        }
        let outputs = count(&mut cursor)?;
        grow(&mut heap, outputs, size_of::<TransactionOutput>())?;
        for _ in 0..outputs {
            take(&mut cursor, 8 + 2)?; // amount and script version
            grow(&mut heap, blob(&mut cursor)?, 1)?;
        }
        take(&mut cursor, 8 + SUBNETWORK_ID_SIZE + 8)?; // lock, subnetwork and gas
        grow(&mut heap, blob(&mut cursor)?, 1)?;
        take(&mut cursor, 8)?; // mass
        hash(&mut cursor)?; // cached id; caller revalidates the actual transaction
        if position == u64::from(index) {
            selected = Some((&start[..start.len() - cursor.len()], heap));
        }
    }
    if !cursor.is_empty() {
        return Err(invalid());
    }
    Ok(selected)
}

#[cfg(test)]
#[path = "block_transactions/retained_body_v3_tests.rs"]
mod retained_body_v3_tests;

#[cfg(test)]
#[path = "block_transactions/capacity_weight_tests.rs"]
mod capacity_weight_tests;

#[cfg(test)]
#[path = "block_transactions/tx_id_projection_tests.rs"]
mod tx_id_projection_tests;
