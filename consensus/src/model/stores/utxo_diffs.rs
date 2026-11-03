use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHasher,
    tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry},
    utxo::utxo_diff::UtxoDiff,
};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

/// Store for holding the UTXO difference (delta) of a block relative to its selected parent.
/// Note that this data is lazy-computed only for blocks which are candidates to being chain
/// blocks. However, once the diff is computed, it is permanent. This store has a relation to
/// block status, such that if a block has status `StatusUTXOValid` then it is expected to have
/// utxo diff data as well as utxo multiset data and acceptance data.
pub trait UtxoDiffsStoreReader {
    fn get(&self, hash: BlockHash) -> Result<Arc<UtxoDiff>, StoreError>;
}

pub trait UtxoDiffsStore: UtxoDiffsStoreReader {
    fn insert(&self, hash: BlockHash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `UtxoDifferencesStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbUtxoDiffsStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, Arc<UtxoDiff>, BlockHasher>,
}

impl DbUtxoDiffsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoDiffs.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Validate and lend one persisted UTXO diff without loading either map
    /// into the ordinary cache. The bound is checked before scanning it.
    pub(crate) fn with_utxo_archive_v2<T>(
        &self,
        block: BlockHash,
        maximum_bytes: u64,
        visit: impl FnOnce(UtxoArchiveViewV2<'_>, u64) -> T,
    ) -> Result<T, StoreError> {
        let prefix = [u8::from(DatabaseStorePrefixes::UtxoDiffs)];
        let key = kaspa_database::prelude::DbKey::new(&prefix, block);
        let raw = self.db.get_pinned(&key)?.ok_or_else(|| StoreError::KeyNotFound(key))?;
        let bytes = raw.len() as u64;
        if bytes > maximum_bytes {
            return Err(StoreError::LocalResourceUnavailable("UTXO archive exceeds replay byte limit".into()));
        }
        Ok(visit(UtxoArchiveViewV2::new(&raw)?, bytes))
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, utxo_diff)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl UtxoDiffsStoreReader for DbUtxoDiffsStore {
    fn get(&self, hash: BlockHash) -> Result<Arc<UtxoDiff>, StoreError> {
        self.access.read(hash)
    }
}

impl UtxoDiffsStore for DbUtxoDiffsStore {
    fn insert(&self, hash: BlockHash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, utxo_diff)?;
        Ok(())
    }

    fn delete(&self, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}

pub(crate) struct UtxoArchiveViewV2<'a> {
    removed: &'a [u8],
}

pub(crate) struct ArchiveUtxoEntryV2<'a> {
    amount: u64,
    version: u16,
    pub(crate) script: &'a [u8],
    daa: u64,
    coinbase: bool,
}

impl ArchiveUtxoEntryV2<'_> {
    /// Call only after admitting the owned script and input slots.
    pub(crate) fn to_entry_v2(&self) -> UtxoEntry {
        UtxoEntry::new(self.amount, ScriptPublicKey::new(self.version, self.script.into()), self.daa, self.coinbase)
    }
}

impl<'a> UtxoArchiveViewV2<'a> {
    fn new(mut raw: &'a [u8]) -> Result<Self, StoreError> {
        visit_utxo_map_v2(&mut raw, |_, _| Ok(()))?;
        let removed = raw;
        visit_utxo_map_v2(&mut raw, |_, _| Ok(()))?;
        if !raw.is_empty() {
            return Err(malformed_utxo_archive());
        }
        Ok(Self { removed })
    }

    pub(crate) fn find_removed_v2(&self, point: TransactionOutpoint) -> Result<Option<ArchiveUtxoEntryV2<'a>>, StoreError> {
        let mut raw = self.removed;
        let mut found = None;
        visit_utxo_map_v2(&mut raw, |at, entry| {
            if at == point {
                if found.is_some() {
                    return Err(StoreError::DataInconsistency("duplicate removed outpoint in UTXO archive".into()));
                }
                found = Some(entry);
            }
            Ok(())
        })?;
        Ok(found)
    }
}

fn malformed_utxo_archive() -> StoreError {
    StoreError::DataInconsistency("malformed UTXO archive".into())
}

/// Read the existing fixed-int bincode map and borrow script bodies. A count
/// never allocates. Every iteration consumes one complete outpoint and entry.
fn visit_utxo_map_v2<'a>(
    raw: &mut &'a [u8],
    mut visit: impl FnMut(TransactionOutpoint, ArchiveUtxoEntryV2<'a>) -> Result<(), StoreError>,
) -> Result<(), StoreError> {
    fn take<const N: usize>(raw: &mut &[u8]) -> Result<[u8; N], StoreError> {
        let (head, tail) = raw.split_first_chunk::<N>().ok_or_else(malformed_utxo_archive)?;
        *raw = tail;
        Ok(*head)
    }

    let count = u64::from_le_bytes(take(raw)?);
    for _ in 0..count {
        if u64::from_le_bytes(take(raw)?) != 64 {
            return Err(malformed_utxo_archive());
        }
        let point = TransactionOutpoint::new(BlockHash::from_bytes(take(raw)?), u32::from_le_bytes(take(raw)?));
        let amount = u64::from_le_bytes(take(raw)?);
        let version = u16::from_le_bytes(take(raw)?);
        let len = usize::try_from(u64::from_le_bytes(take(raw)?)).map_err(|_| malformed_utxo_archive())?;
        let (script, tail) = raw.split_at_checked(len).ok_or_else(malformed_utxo_archive)?;
        *raw = tail;
        let daa = u64::from_le_bytes(take(raw)?);
        let coinbase = match take(raw)? {
            [0] => false,
            [1] => true,
            _ => return Err(malformed_utxo_archive()),
        };
        visit(point, ArchiveUtxoEntryV2 { amount, version, script, daa, coinbase })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_utxo_archive_reopens_and_reads_only_original_inputs() {
        let hash = |byte| BlockHash::from_bytes([byte; 64]);
        let added = TransactionOutpoint::new(hash(1), 0);
        let removed = TransactionOutpoint::new(hash(2), 1);
        let entry = UtxoEntry::new(400, ScriptPublicKey::new(3, [0x51; 200].as_slice().into()), 41, true);
        let mut diff = UtxoDiff::default();
        diff.add.insert(added, entry.clone());
        diff.remove.insert(removed, entry.clone());
        let raw = bincode::serialize(&diff).unwrap();
        let view = UtxoArchiveViewV2::new(&raw).unwrap();
        assert!(view.find_removed_v2(added).unwrap().is_none());
        assert_eq!(view.find_removed_v2(removed).unwrap().unwrap().to_entry_v2(), entry);
        assert!(UtxoArchiveViewV2::new(&raw[..raw.len() - 1]).is_err());
        let mut bad = raw.clone();
        *bad.last_mut().unwrap() = 2;
        assert!(UtxoArchiveViewV2::new(&bad).is_err());
        let duplicate = bincode::serialize(&(diff.add.clone(), vec![(removed, entry.clone()), (removed, entry.clone())])).unwrap();
        assert!(UtxoArchiveViewV2::new(&duplicate).unwrap().find_removed_v2(removed).is_err());

        let dir = kaspa_database::utils::get_kaspa_tempdir();
        let open = |create| {
            kaspa_database::prelude::ConnBuilder::default()
                .with_files_limit(10)
                .with_db_path(dir.path().to_path_buf())
                .with_create_if_missing(create)
                .build()
                .unwrap()
        };
        let block = hash(9);
        let store = DbUtxoDiffsStore::new(open(true), CachePolicy::Empty);
        store.insert(block, Arc::new(diff)).unwrap();
        drop(store);
        let store = DbUtxoDiffsStore::new(open(false), CachePolicy::Empty);
        assert!(store.with_utxo_archive_v2(block, raw.len() as u64 - 1, |_, _| ()).is_err());
        let restored = store
            .with_utxo_archive_v2(block, raw.len() as u64, |view, bytes| {
                assert_eq!(bytes, raw.len() as u64);
                view.find_removed_v2(removed).unwrap().unwrap().to_entry_v2()
            })
            .unwrap();
        assert_eq!(restored, entry);
    }
}
