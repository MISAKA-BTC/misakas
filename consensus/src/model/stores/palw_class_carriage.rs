//! **ADR-0067: the chain-registered class index — what graph the chain registered under an id.**
//!
//! Consensus state retains a class's ECONOMIC facts (`PalwClassStateV2`: root, slash, status,
//! registrant) and deliberately drops the admission carriage once the gate has judged it. That is
//! right for consensus — the carriage is an input to admission, not a fact the transition needs
//! again — and exactly wrong for a node that wants to SERVE the class: execution-from-declaration
//! (ADR-0067 Decision 2) needs the declaration.
//!
//! So this store keeps what the wire delivered, verbatim: per class id, the Borsh bytes of the
//! `PalwClassAdmissionCarriageV2` the ACCEPTED registration carried, written where the lifecycle
//! filter accepts the object. IBD replays the same filter over the same chain blocks, so a
//! syncing node builds the same index without a separate backfill walk.
//!
//! # Why verbatim bytes, and why append-only
//!
//! Verbatim for the carriage store's own reason: a reader must decode exactly what admission
//! validated, never a re-encoding. Append-only because the read side is GATED on current state —
//! a serving node asks whether the class exists, is not frozen, and names the artifact root THIS
//! row was registered under, and only then reads the declaration. The root check is what makes
//! append-only safe: `class_id` hashes the profile ALONE, so a registration that lost a reorg and
//! the one that won can share a key while differing in the weights they name and the canonical
//! job that prices them — and the canonical job is not covered by the id at all.
//!
//! # Pruning-point declaration transport
//!
//! Protocol 107 carries snapshot-bound accepted declarations alongside PALW
//! state. The importer validates the complete offered set before mutation and
//! stages the tip, servable snapshot, declarations and incomplete-import marker
//! in one durable recovery batch. The store guard remains held until commit;
//! staged rows never populate a positive cache. A joined node can serve the same
//! authenticated declarations without replaying a pruned registration block.
//!
//! A profile id alone does not authenticate its canonical tokenizer/runtime/job.
//! Complete transport requires retained semantic identity or exact binary-known
//! genesis identity, plus the retained PWU and fused-attention geometry. A legacy
//! post-genesis class lacking that authority is refused as unsupported rather
//! than supplying an invented canonical identity. Protocol 106's historical
//! sparse compatibility path does not prove complete declaration coverage.

use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DB, DirectDbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

/// Bump on ANY change to [`PalwClassCarriageRecord`]'s layout — see the registry prefix docs: an
/// undecodable row reads as absent, and absence refuses service (fail-closed), but a silently
/// empty store after a layout change would read as "nothing was ever registered".
pub const PALW_CLASS_CARRIAGE_SCHEMA_VERSION: u32 = 2;

/// One accepted registration's declaration, as delivered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwClassCarriageRecord {
    /// The DAA of the chain block whose lifecycle filter accepted the registration.
    pub registered_daa: u64,
    /// **The artifact root this registration named.** `class_id` hashes the PROFILE only, so two
    /// registrations of one graph — a reorged-out one and the live one — share a key while
    /// differing in the weights they name and in the canonical job that prices them. The reader
    /// pins this against the class the chain currently holds, which is what makes an append-only
    /// store safe under reorg: a stale row is refused rather than served.
    pub artifact_root: Hash64,
    /// `PalwClassAdmissionCarriageV2`, Borsh, verbatim.
    pub carriage: Vec<u8>,
}

impl MemSizeEstimator for PalwClassCarriageRecord {}

/// The chain-registered class declarations this node has accepted.
#[derive(Clone)]
pub struct DbPalwClassCarriageStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash64, Arc<PalwClassCarriageRecord>>,
    schema: CachedDbItem<u32>,
}

impl DbPalwClassCarriageStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: db.clone(),
            access: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwClassCarriages.into()),
            schema: CachedDbItem::new(db, DatabaseStorePrefixes::PalwClassCarriagesSchema.into()),
        }
    }

    /// Drop every row written under another layout, so they re-derive (on re-acceptance or a
    /// future backfill walk) instead of reading as absent.
    pub fn reindex_if_stale(&mut self) -> StoreResult<()> {
        let stored = match self.schema.read() {
            Ok(version) => version,
            Err(StoreError::KeyNotFound(_)) => 0,
            Err(err) => return Err(err),
        };
        if stored == PALW_CLASS_CARRIAGE_SCHEMA_VERSION {
            return Ok(());
        }
        self.access.delete_all(DirectDbWriter::new(&self.db))?;
        self.schema.write(DirectDbWriter::new(&self.db), &PALW_CLASS_CARRIAGE_SCHEMA_VERSION)
    }

    /// The declaration the chain accepted for `class_id`, if this node saw it.
    pub fn get(&self, class_id: Hash64) -> Option<Arc<PalwClassCarriageRecord>> {
        self.access.read(class_id).ok()
    }

    /// Record an accepted registration's declaration. Idempotent for one class id by the module
    /// doc's argument: service additionally checks the current root, registration
    /// position and canonical semantic authority against the consensus snapshot.
    pub fn insert(&mut self, class_id: Hash64, record: PalwClassCarriageRecord) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), class_id, Arc::new(record))
    }

    /// Genesis and pruning-snapshot declarations share their PALW commit. Do not publish
    /// a positive cache row before the containing RocksDB batch commits.
    /// The caller retains its store write guard through that commit.
    pub(crate) fn insert_batch(
        &mut self,
        batch: &mut rocksdb::WriteBatch,
        class_id: Hash64,
        record: PalwClassCarriageRecord,
    ) -> StoreResult<()> {
        self.access.write_many_without_cache(BatchDbWriter::new(batch), &mut std::iter::once((class_id, Arc::new(record))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};

    #[test]
    fn genesis_batch_carriage_is_not_visible_before_commit_and_survives_fresh_cache() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwClassCarriageStore::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();
        let id = Hash64::from_u64_word(1);
        let record = PalwClassCarriageRecord { registered_daa: 0, artifact_root: Hash64::from_u64_word(2), carriage: vec![3, 4] };
        let mut abandoned = rocksdb::WriteBatch::default();
        store.insert_batch(&mut abandoned, id, record.clone()).unwrap();
        assert!(store.get(id).is_none(), "staged declarations cannot authorize service");
        drop(abandoned);
        assert!(DbPalwClassCarriageStore::new(db.clone(), CachePolicy::Count(16)).get(id).is_none());
        let mut committed = rocksdb::WriteBatch::default();
        store.insert_batch(&mut committed, id, record.clone()).unwrap();
        db.write(committed).unwrap();
        assert_eq!(*store.get(id).unwrap(), record);
        assert_eq!(*DbPalwClassCarriageStore::new(db, CachePolicy::Count(16)).get(id).unwrap(), record);
    }
}
