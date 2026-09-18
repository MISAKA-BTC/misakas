//! **Where the consensus clock stands, per block** — ADR-0142.
//!
//! Derived data in the same sense as the DAA score beside it: every node computes the same value
//! for the same block from the same stored headers, so it needs no commitment of its own. It is
//! stored rather than walked because the answer at a block is a function of its whole chain back to
//! the fence, and the header processor has to have it in constant time.
//!
//! The cursor advances at a block **iff that block's mergeset gave the heartbeat exemption** — so a
//! block that does not advance the clock leaves it exactly as its selected parent had it, which is
//! the invariant ADR-0142 exists for. `None` for a block below the fence, and for every block
//! before the first beat past it.

use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::palw_clock_cursor_v1::PalwClockCursorV1;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

pub trait PalwClockCursorStoreReader {
    /// The cursor as of `hash`. `Ok(None)` where the block carries none — below the fence, or
    /// before the first beat opened it. A missing ROW is the same answer as a `None` row: both mean
    /// "this block advanced no clock", and a node that has pruned the row has pruned the block.
    fn get_clock_cursor(&self, hash: BlockHash) -> Result<Option<PalwClockCursorV1>, StoreError>;
}

/// A DB + cache implementation, keyed by block hash like every other per-block derivation.
#[derive(Clone)]
pub struct DbPalwClockCursorStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, Arc<PalwClockCursorV1>, BlockHasher>,
}

impl DbPalwClockCursorStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwClockCursor.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Write the cursor for a block. `None` writes nothing, so a chain below the fence leaves this
    /// column family empty and costs a node that never arms the rule exactly nothing.
    ///
    /// Idempotent on the same value: header processing can re-enter for a block already staged, and
    /// refusing there would fail a re-org replay that is otherwise correct. A DIFFERENT value for a
    /// block already written is a bug and is refused.
    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, cursor: Option<PalwClockCursorV1>) -> Result<(), StoreError> {
        let Some(cursor) = cursor else { return Ok(()) };
        if let Some(existing) = self.get_clock_cursor(hash)? {
            if existing == cursor {
                return Ok(());
            }
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, Arc::new(cursor))?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl PalwClockCursorStoreReader for DbPalwClockCursorStore {
    fn get_clock_cursor(&self, hash: BlockHash) -> Result<Option<PalwClockCursorV1>, StoreError> {
        match self.access.read(hash) {
            Ok(cursor) => Ok(Some(*cursor)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
