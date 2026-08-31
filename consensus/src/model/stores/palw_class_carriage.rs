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
//! # The one place this index is NOT complete, said plainly
//!
//! **A node that joins by a pruned sync gets the class state and none of these rows.** The only
//! writer is the chain-candidate accept path, and a pruning-point IBD never walks the blocks
//! below the pruning point: `import_pruning_point_palw_state` brings the class table over
//! wholesale and touches nothing here. Such a node therefore holds classes whose declarations it
//! does not have, and — with the ADR-0067 arm armed — REFUSES to serve them, which reads as "this
//! node cannot serve the registered class" rather than as the missing index it is.
//!
//! That refusal is the safe direction (it never serves a class it cannot prove), and it is a real
//! gap: on a pruned-sync fleet only the nodes that watched a registration go by can judge its
//! class, and judging is what decides quorums. Closing it means either carrying the accepted
//! carriages in the pruning-point sidecar beside `PalwStateCarriageV2`, or serving a row from a
//! peer on demand — which needs no trust, because `profile.shape_profile_id() == class_id` makes
//! the bytes self-authenticating. Neither is built; the ADR records it as open.

use std::sync::Arc;

use kaspa_database::prelude::{CachePolicy, CachedDbAccess, CachedDbItem, DB, DirectDbWriter, StoreError, StoreResult};
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
    /// doc's argument (the id is the profile's hash).
    pub fn insert(&mut self, class_id: Hash64, record: PalwClassCarriageRecord) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), class_id, Arc::new(record))
    }
}
