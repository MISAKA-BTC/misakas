//! Search-availability fork-local state + pruning snapshot stores (ADR node-anchored-web-search-da).
//!
//! Mirrors the DA-01 store discipline exactly: consensus validity reads `state(selected_parent)`,
//! never a mutable tip singleton; an unchanged child commits a 64-byte anchor link instead of a
//! duplicated full row; the pruning singleton is tagged by its own snapshot type and captured in the
//! same batch as the pruning pointer.

use kaspa_consensus_core::palw::search_snapshot::{PalwSearchAvailabilityStateV1, PalwSearchPruningSnapshotV1};
use kaspa_consensus_core::{BlockHash, BlockHasher};
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DB, DbKey, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;
use std::sync::Arc;

pub trait PalwSearchAvailabilityStoreReader {
    fn state(&self, block: BlockHash) -> StoreResult<Arc<PalwSearchAvailabilityStateV1>>;
    fn pruning_snapshot(&self) -> StoreResult<PalwSearchPruningSnapshotV1>;
}

/// Anchor link for a chain block whose search-availability state is bit-identical to the full row
/// stored at `anchor` (same shape as `PalwDaStateLinkV1`; with an idle search state machine this
/// turns the dominant per-block write into 64 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PalwSearchStateLinkV1 {
    /// Block whose full state row carries this block's state. Always a full row, never a link.
    pub anchor: BlockHash,
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwSearchStateLinkV1 {
    fn estimate_mem_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
    }
}

#[derive(Clone)]
pub struct DbPalwSearchAvailabilityStore {
    db: Arc<DB>,
    states: CachedDbAccess<BlockHash, Arc<PalwSearchAvailabilityStateV1>, BlockHasher>,
    state_links: CachedDbAccess<BlockHash, PalwSearchStateLinkV1, BlockHasher>,
    snapshot: CachedDbItem<PalwSearchPruningSnapshotV1>,
}

impl DbPalwSearchAvailabilityStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            states: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwSearchAvailabilityStateByBlock.into()),
            state_links: CachedDbAccess::new(
                db.clone(),
                cache_policy,
                DatabaseStorePrefixes::PalwSearchAvailabilityStateLinkByBlock.into(),
            ),
            snapshot: CachedDbItem::new(db, DatabaseStorePrefixes::PalwSearchAvailabilityPruningSnapshot.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn set_state_batch(
        &mut self,
        batch: &mut WriteBatch,
        block: BlockHash,
        state: Arc<PalwSearchAvailabilityStateV1>,
    ) -> StoreResult<()> {
        self.states.write(BatchDbWriter::new(batch), block, state)
    }

    /// Record that `block`'s state is bit-identical to the full row at `anchor`. The caller must
    /// have verified equality against the exact stored anchor state.
    pub fn set_state_link_batch(&mut self, batch: &mut WriteBatch, block: BlockHash, anchor: BlockHash) -> StoreResult<()> {
        if anchor == block {
            return Err(StoreError::DataInconsistency("PALW search state link must not be self-referential".into()));
        }
        self.state_links.write(BatchDbWriter::new(batch), block, PalwSearchStateLinkV1 { anchor })
    }

    pub fn delete_state_batch(&mut self, batch: &mut WriteBatch, block: BlockHash) -> StoreResult<()> {
        self.states.delete(BatchDbWriter::new(batch), block)?;
        self.state_links.delete(BatchDbWriter::new(batch), block)
    }

    /// Resolve a block's state together with its anchor (the block owning the full row).
    /// `full → link → full(anchor)`, at most two reads; a dangling link is a hard inconsistency,
    /// and a plain miss keeps the original `KeyNotFound` semantics for callers.
    pub fn state_and_anchor(&self, block: BlockHash) -> StoreResult<(Arc<PalwSearchAvailabilityStateV1>, BlockHash)> {
        match self.states.read(block) {
            Ok(state) => Ok((state, block)),
            Err(StoreError::KeyNotFound(_)) => match self.state_links.read(block) {
                Ok(link) => match self.states.read(link.anchor) {
                    Ok(state) => Ok((state, link.anchor)),
                    Err(StoreError::KeyNotFound(_)) => Err(StoreError::DataInconsistency(format!(
                        "PALW search state link {block} -> {} is dangling",
                        link.anchor
                    ))),
                    Err(error) => Err(error),
                },
                Err(StoreError::KeyNotFound(_)) => Err(StoreError::KeyNotFound(DbKey::new(
                    &Vec::from(DatabaseStorePrefixes::PalwSearchAvailabilityStateByBlock),
                    block,
                ))),
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        }
    }

    pub fn set_pruning_snapshot_batch(&mut self, batch: &mut WriteBatch, snapshot: &PalwSearchPruningSnapshotV1) -> StoreResult<()> {
        if !snapshot.validate() {
            return Err(StoreError::DataInconsistency("invalid PALW search pruning snapshot".into()));
        }
        self.snapshot.write(BatchDbWriter::new(batch), snapshot)
    }
}

impl PalwSearchAvailabilityStoreReader for DbPalwSearchAvailabilityStore {
    fn state(&self, block: BlockHash) -> StoreResult<Arc<PalwSearchAvailabilityStateV1>> {
        self.state_and_anchor(block).map(|(state, _)| state)
    }

    fn pruning_snapshot(&self) -> StoreResult<PalwSearchPruningSnapshotV1> {
        self.snapshot.read()
    }
}
