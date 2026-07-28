use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::palw::PalwActiveNullifierSet;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

/// kaspa-pq ADR-0039 PALW (§15.2): per-block store of the `PalwActiveNullifierSet` — the
/// retention-windowed set of ticket nullifiers active in that block's past. A child's GHOSTDAG
/// duplicate-ticket dedup seeds from its selected parent's set here (first-seen kept, re-use recolored
/// red), so the window persists across the DAG without re-walking history. Deleted by the pruning
/// processor alongside the other per-block stores.
///
/// A row is written for every non-genesis block on the three PALW presets, which use
/// `palw_activation_daa_score = 0`: the writer at
/// `pipeline/header_processor/processor.rs` is guarded only by
/// `header.daa_score >= self.palw_activation_daa_score && ctx.hash != self.genesis.hash`.
///
/// The store is truly untouched only on mainnet / testnet-10 / simnet / devnet, where
/// `palw_activation_daa_score == u64::MAX` fails the guard. See `LATEST_DB_VERSION`
/// (`consensus/src/consensus/factory.rs`) for the format-break cutover.
pub trait PalwNullifierStoreReader {
    fn get(&self, hash: BlockHash) -> Result<Arc<PalwActiveNullifierSet>, StoreError>;
    fn has(&self, hash: BlockHash) -> Result<bool, StoreError>;
}

pub trait PalwNullifierStore: PalwNullifierStoreReader {
    fn insert(&self, hash: BlockHash, set: Arc<PalwActiveNullifierSet>) -> Result<(), StoreError>;
    fn delete(&self, hash: BlockHash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of the `PalwNullifierStore` trait.
#[derive(Clone)]
pub struct DbPalwNullifierStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, Arc<PalwActiveNullifierSet>, BlockHasher>,
}

impl DbPalwNullifierStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwNullifiers.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, set: Arc<PalwActiveNullifierSet>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, set)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl PalwNullifierStoreReader for DbPalwNullifierStore {
    fn get(&self, hash: BlockHash) -> Result<Arc<PalwActiveNullifierSet>, StoreError> {
        self.access.read(hash)
    }

    fn has(&self, hash: BlockHash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

impl PalwNullifierStore for DbPalwNullifierStore {
    fn insert(&self, hash: BlockHash, set: Arc<PalwActiveNullifierSet>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, set)?;
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
    use kaspa_hashes::Hash64;

    fn set(nfs: &[(u8, u64)]) -> Arc<PalwActiveNullifierSet> {
        let mut s = PalwActiveNullifierSet::new();
        for (b, daa) in nfs {
            s.insert(Hash64::from_bytes([*b; 64]), *daa);
        }
        Arc::new(s)
    }

    /// Happy path: a per-block active-nullifier set inserts, reads back byte-for-byte, and deletes; a
    /// re-insert on the same block hash is rejected (per-block append-only).
    #[test]
    fn insert_get_delete() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwNullifierStore::new(db, CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x11; 64]);
        let s = set(&[(1, 100), (2, 110)]);

        assert!(!store.has(h).unwrap());
        assert!(store.get(h).is_err());
        store.insert(h, s.clone()).unwrap();
        assert!(store.has(h).unwrap());
        assert_eq!(*store.get(h).unwrap(), *s);
        // second insert on the same key is rejected.
        assert!(store.insert(h, s.clone()).is_err());
        store.delete(h).unwrap();
        assert!(store.get(h).is_err());
    }

    /// The inert default (empty set) round-trips too — the only value ever written on a shipped preset.
    #[test]
    fn empty_inert_set_roundtrips() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwNullifierStore::new(db, CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x22; 64]);
        store.insert(h, Arc::new(PalwActiveNullifierSet::new())).unwrap();
        assert!(store.get(h).unwrap().is_empty());
    }
}
