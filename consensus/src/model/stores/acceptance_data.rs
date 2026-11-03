use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::acceptance_data::AcceptanceData;
use kaspa_consensus_core::acceptance_data::AcceptedTxEntry;
use kaspa_consensus_core::acceptance_data::MergesetBlockAcceptanceData;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

pub trait AcceptanceDataStoreReader {
    fn get(&self, hash: BlockHash) -> Result<Arc<AcceptanceData>, StoreError>;
}

pub trait AcceptanceDataStore: AcceptanceDataStoreReader {
    fn insert(&self, hash: BlockHash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// Simple wrapper for implementing `MemSizeEstimator`
#[derive(Clone, Serialize, Deserialize)]
struct AcceptanceDataEntry(Arc<AcceptanceData>);

impl MemSizeEstimator for AcceptanceDataEntry {
    fn estimate_mem_bytes(&self) -> usize {
        self.0.iter().map(|l| l.accepted_transactions.len()).sum::<usize>() * size_of::<AcceptedTxEntry>()
            + self.0.len() * size_of::<MergesetBlockAcceptanceData>()
            + size_of::<AcceptanceData>()
            + size_of::<Self>()
    }
}

/// A DB + cache implementation of `DbAcceptanceDataStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbAcceptanceDataStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, AcceptanceDataEntry, BlockHasher>,
}

impl DbAcceptanceDataStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::AcceptanceData.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Resolve one entry from the persisted bincode representation without
    /// allocating any of its mergeset or transaction vectors.
    pub(crate) fn accepted_origin_at_bounded_v2(
        &self,
        accepting: BlockHash,
        ordinal: u32,
        maximum_bytes: u64,
    ) -> Result<Option<(BlockHash, AcceptedTxEntry, u64)>, StoreError> {
        let prefix = [u8::from(DatabaseStorePrefixes::AcceptanceData)];
        let key = kaspa_database::prelude::DbKey::new(&prefix, accepting);
        let bytes = self.db.get_pinned(&key)?.ok_or_else(|| StoreError::KeyNotFound(key))?;
        if bytes.len() as u64 > maximum_bytes {
            return Err(StoreError::LocalResourceUnavailable("accepted origin exceeds replay byte limit".into()));
        }
        Ok(accepted_origin_from_bytes_v2(&bytes, ordinal)?.map(|(block, entry)| (block, entry, bytes.len() as u64)))
    }

    /// Lend the canonical persisted record without allocating its vectors or
    /// seeding the ordinary acceptance cache during a paid archive replay.
    pub(crate) fn with_accepted_archive_v2<T>(
        &self,
        accepting: BlockHash,
        maximum_bytes: u64,
        visit: impl FnOnce(AcceptanceArchiveViewV2<'_>) -> T,
    ) -> Result<T, StoreError> {
        let prefix = [u8::from(DatabaseStorePrefixes::AcceptanceData)];
        let key = kaspa_database::prelude::DbKey::new(&prefix, accepting);
        let bytes = self.db.get_pinned(&key)?.ok_or_else(|| StoreError::KeyNotFound(key))?;
        if bytes.len() as u64 > maximum_bytes {
            return Err(StoreError::LocalResourceUnavailable("acceptance archive exceeds replay byte limit".into()));
        }
        Ok(visit(AcceptanceArchiveViewV2::new(&bytes)?))
    }

    pub fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        hash: BlockHash,
        acceptance_data: Arc<AcceptanceData>,
    ) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, AcceptanceDataEntry(acceptance_data))?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl AcceptanceDataStoreReader for DbAcceptanceDataStore {
    fn get(&self, hash: BlockHash) -> Result<Arc<AcceptanceData>, StoreError> {
        Ok(self.access.read(hash)?.0)
    }
}

impl AcceptanceDataStore for DbAcceptanceDataStore {
    fn insert(&self, hash: BlockHash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, AcceptanceDataEntry(acceptance_data))?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}

type ArchiveOriginV2 = (BlockHash, AcceptedTxEntry);

/// A canonical pinned bincode record. This view never owns a decoded Vec.
pub(crate) struct AcceptanceArchiveViewV2<'a>(&'a [u8]);

impl<'a> AcceptanceArchiveViewV2<'a> {
    fn new(raw: &'a [u8]) -> Result<Self, StoreError> {
        visit_acceptance_bytes_v2(raw, |_, _| false)?;
        Ok(Self(raw))
    }

    /// The archived input may refer to a transaction earlier in the same
    /// accepting prefix. The bound is exclusive and spans all mergeset blocks.
    pub(crate) fn find_before_v2(
        &self,
        before: u32,
        transaction: kaspa_consensus_core::TransactionId,
    ) -> Result<Option<ArchiveOriginV2>, StoreError> {
        let mut found = None;
        visit_acceptance_bytes_v2(self.0, |ordinal, origin| {
            if ordinal >= u64::from(before) {
                return true;
            }
            if origin.1.transaction_id == transaction {
                found = Some(origin);
                return true;
            }
            false
        })?;
        Ok(found)
    }
}

fn accepted_origin_from_bytes_v2(raw: &[u8], ordinal: u32) -> Result<Option<ArchiveOriginV2>, StoreError> {
    let mut found = None;
    visit_acceptance_bytes_v2(raw, |at, origin| {
        if at == u64::from(ordinal) {
            found = Some(origin);
        }
        false
    })?;
    Ok(found)
}

/// bincode v1 fixed-int sequence and byte-string framing. Both BlockHash
/// and TransactionId serialize as a length-prefixed Hash64. Only a validated
/// view uses early exit, so a bad tail cannot pass an ordinal lookup.
fn visit_acceptance_bytes_v2(mut raw: &[u8], mut stop: impl FnMut(u64, ArchiveOriginV2) -> bool) -> Result<(), StoreError> {
    fn invalid() -> StoreError {
        StoreError::DataInconsistency("malformed accepted origin record".into())
    }
    fn take<const N: usize>(raw: &mut &[u8]) -> Result<[u8; N], StoreError> {
        let (head, tail) = raw.split_first_chunk::<N>().ok_or_else(invalid)?;
        *raw = tail;
        Ok(*head)
    }
    fn hash(raw: &mut &[u8]) -> Result<BlockHash, StoreError> {
        if u64::from_le_bytes(take(raw)?) != 64 {
            return Err(invalid());
        }
        Ok(BlockHash::from_bytes(take(raw)?))
    }

    const ENTRY_BYTES: usize = 8 + 64 + 4;
    let count = u64::from_le_bytes(take(&mut raw)?);
    let mut ordinal = 0u64;
    for _ in 0..count {
        let block = hash(&mut raw)?;
        let entries = u64::from_le_bytes(take(&mut raw)?);
        let extent = usize::try_from(entries).ok().and_then(|n| n.checked_mul(ENTRY_BYTES)).ok_or_else(invalid)?;
        let (body, tail) = raw.split_at_checked(extent).ok_or_else(invalid)?;
        raw = tail;
        for mut entry in body.chunks_exact(ENTRY_BYTES) {
            let transaction_id = hash(&mut entry)?;
            let index_within_block = u32::from_le_bytes(take(&mut entry)?);
            if stop(ordinal, (block, AcceptedTxEntry { transaction_id, index_within_block })) {
                return Ok(());
            }
            ordinal = ordinal.checked_add(1).ok_or_else(invalid)?;
        }
    }
    if !raw.is_empty() {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_acceptance_lookup_matches_persisted_bincode_ordinals() {
        let block = |n| BlockHash::from_bytes([n; 64]);
        let body = AcceptanceDataEntry(Arc::new(vec![
            MergesetBlockAcceptanceData { block_hash: block(1), accepted_transactions: Vec::new() },
            MergesetBlockAcceptanceData {
                block_hash: block(2),
                accepted_transactions: vec![
                    AcceptedTxEntry { transaction_id: block(3), index_within_block: 1 },
                    AcceptedTxEntry { transaction_id: block(4), index_within_block: 7 },
                ],
            },
            MergesetBlockAcceptanceData {
                block_hash: block(5),
                accepted_transactions: vec![AcceptedTxEntry { transaction_id: block(6), index_within_block: 3 }],
            },
        ]));
        let raw = bincode::serialize(&body).unwrap();
        for (ordinal, carrier, tx, index) in [(0, 2, 3, 1), (1, 2, 4, 7), (2, 5, 6, 3)] {
            let (actual_carrier, actual) = accepted_origin_from_bytes_v2(&raw, ordinal).unwrap().unwrap();
            assert_eq!((actual_carrier, actual.transaction_id, actual.index_within_block), (block(carrier), block(tx), index));
        }
        assert!(accepted_origin_from_bytes_v2(&raw, 3).unwrap().is_none());
        let archive = AcceptanceArchiveViewV2::new(&raw).unwrap();
        assert!(archive.find_before_v2(1, block(4)).unwrap().is_none());
        let (creator, entry) = archive.find_before_v2(2, block(4)).unwrap().unwrap();
        assert_eq!((creator, entry.index_within_block), (block(2), 7));
        assert!(archive.find_before_v2(2, block(6)).unwrap().is_none());
        assert_eq!(archive.find_before_v2(3, block(6)).unwrap().unwrap().0, block(5));
        assert!(AcceptanceArchiveViewV2::new(&raw[..raw.len() - 1]).is_err());
        assert!(accepted_origin_from_bytes_v2(&raw[..raw.len() - 1], 0).is_err());
        let dir = kaspa_database::utils::get_kaspa_tempdir();
        let open = |create| {
            kaspa_database::prelude::ConnBuilder::default()
                .with_files_limit(10)
                .with_db_path(dir.path().to_path_buf())
                .with_create_if_missing(create)
                .build()
                .unwrap()
        };
        let accepting = block(9);
        let store = DbAcceptanceDataStore::new(open(true), CachePolicy::Empty);
        store.insert(accepting, body.0).unwrap();
        drop(store);
        let store = DbAcceptanceDataStore::new(open(false), CachePolicy::Empty);
        assert!(store.accepted_origin_at_bounded_v2(accepting, 2, raw.len() as u64 - 1).is_err());
        let (carrier, accepted, stored_bytes) = store.accepted_origin_at_bounded_v2(accepting, 2, raw.len() as u64).unwrap().unwrap();
        assert_eq!(
            (carrier, accepted.transaction_id, accepted.index_within_block, stored_bytes),
            (block(5), block(6), 3, raw.len() as u64)
        );

        assert!(store.with_accepted_archive_v2(accepting, raw.len() as u64 - 1, |_| ()).is_err());
        let earlier = store
            .with_accepted_archive_v2(accepting, raw.len() as u64, |view| view.find_before_v2(2, block(4)))
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!((earlier.0, earlier.1.index_within_block), (block(2), 7));

        let mut bad = raw;
        let first_entry_offset = 8 + 2 * (8 + 64 + 8);
        bad[first_entry_offset] = 63;
        assert!(accepted_origin_from_bytes_v2(&bad, 2).is_err());
    }
}
