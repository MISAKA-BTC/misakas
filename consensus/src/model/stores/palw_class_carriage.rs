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
//! validated, never a re-encoding. Append-only because the read side is EXISTENCE-GATED — a
//! serving node first asks current state whether the class exists (and is not frozen), and only
//! then reads the declaration here. A row left behind by a reorged-out registration is therefore
//! inert: the state gate refuses before the row is ever consulted, and a re-registration of the
//! same class id would carry the same profile (the id IS the profile's hash) so overwriting is
//! idempotent by construction.

use std::sync::Arc;

use kaspa_database::prelude::{CachePolicy, CachedDbAccess, CachedDbItem, DB, DirectDbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

/// Bump on ANY change to [`PalwClassCarriageRecord`]'s layout — see the registry prefix docs: an
/// undecodable row reads as absent, and absence refuses service (fail-closed), but a silently
/// empty store after a layout change would read as "nothing was ever registered".
pub const PALW_CLASS_CARRIAGE_SCHEMA_VERSION: u32 = 1;

/// One accepted registration's declaration, as delivered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwClassCarriageRecord {
    /// The DAA of the chain block whose lifecycle filter accepted the registration.
    pub registered_daa: u64,
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
    /// doc's argument (the id is the profile's hash).
    pub fn insert(&mut self, class_id: Hash64, record: PalwClassCarriageRecord) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), class_id, Arc::new(record))
    }
}
