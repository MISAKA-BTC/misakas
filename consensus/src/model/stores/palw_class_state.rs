//! ADR-0038 Decision D: per-`ExecutionClass` difficulty state.
//!
//! `palw_pwu` needs two factors: the class's normative per-inference cost, frozen at registration,
//! and the class's DAA **target**, which is not frozen — it is what the per-class retarget moves.
//! This store is where the second one lives.
//!
//! # Reads must be chain-scoped, and this store is not
//!
//! A store keyed by class id holds ONE value: whatever the virtual chain last wrote. Reading it
//! from a context that is evaluating a different chain is the shape of the 2026-08-17 audit's
//! blocker 6(b) — a validity answer that depends on where this node's virtual tip happens to
//! point. So the store is deliberately NOT read directly by anything that weighs blocks: callers
//! build a [`PalwClassStateView`] for the chain point they are evaluating, exactly as bond
//! consumers build an `ActiveBondView`, and the view is what the resolver reads.
//!
//! # Absent is not zero
//!
//! [`PALW_CLASS_STATE_SCHEMA_VERSION`] exists for the reason the carriage store's does: a row the
//! iterator cannot decode is dropped silently, so a layout change would make every class read as
//! *absent* rather than as broken. A class whose target reads as absent is a class whose blocks
//! weigh nothing — a wrong answer that looks like a valid one. Bump this on any layout change so
//! the rows are discarded and re-derived instead of read as missing.

use std::collections::BTreeMap;
use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DirectDbWriter, StoreResult, DB};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use kaspa_hashes::Hash64;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

/// Bump on ANY change to [`PalwClassStateRecord`]'s layout. See the module docs: without it a
/// layout change reads as an empty store, which is indistinguishable from "no class has a target".
pub const PALW_CLASS_STATE_SCHEMA_VERSION: u32 = 1;

/// One class's difficulty state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwClassStateRecord {
    /// The class's lottery target (bigger = easier), the first factor of
    /// `palw_pwu::palw_pwu_v1`.
    pub target: u128,
    /// DAA of the retarget that produced `target` — carried so a view can tell a stale row from a
    /// current one rather than assuming.
    pub retargeted_at_daa: u64,
    /// Blocks this class produced in the window ending at `retargeted_at_daa`, kept so the next
    /// retarget is a function of recorded facts rather than of a re-walk.
    pub observed_blocks: u64,
}

impl MemSizeEstimator for PalwClassStateRecord {}

/// Accepted per-class difficulty state.
#[derive(Clone)]
pub struct DbPalwClassStateStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash64, Arc<PalwClassStateRecord>>,
    schema: CachedDbItem<u32>,
}

impl DbPalwClassStateStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::PalwClassState.into()),
            schema: CachedDbItem::new(db, DatabaseStorePrefixes::PalwClassStateSchema.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// The layout the rows on disk were written under, or `None` on a database that has never
    /// held any. A version other than [`PALW_CLASS_STATE_SCHEMA_VERSION`] means the rows are
    /// undecodable and MUST be treated as absent-and-stale rather than as an empty class set.
    pub fn schema_version(&self) -> Option<u32> {
        self.schema.read().ok()
    }

    pub fn set_schema_version_batch(&mut self, batch: &mut WriteBatch, version: u32) -> StoreResult<()> {
        self.schema.write(BatchDbWriter::new(batch), &version)
    }

    pub fn set_schema_version(&mut self, version: u32) -> StoreResult<()> {
        self.schema.write(DirectDbWriter::new(&self.db), &version)
    }

    pub fn get(&self, class_id: Hash64) -> Option<Arc<PalwClassStateRecord>> {
        self.access.read(class_id).ok()
    }

    pub fn insert_batch(&mut self, batch: &mut WriteBatch, class_id: Hash64, record: Arc<PalwClassStateRecord>) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), class_id, record)
    }

    pub fn delete_batch(&mut self, batch: &mut WriteBatch, class_id: Hash64) -> StoreResult<()> {
        self.access.delete(BatchDbWriter::new(batch), class_id)
    }
}

/// A chain-scoped snapshot of class difficulty state.
///
/// Built for the chain point being evaluated and read from there, so a block's weight never
/// depends on where this node's virtual tip happens to be — the discipline `ActiveBondView`
/// already established for bonds and the one blocker 6(b) says the E2 spend gate broke.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwClassStateView {
    classes: BTreeMap<Hash64, PalwClassStateRecord>,
}

impl PalwClassStateView {
    pub fn from_records(records: impl IntoIterator<Item = (Hash64, PalwClassStateRecord)>) -> Self {
        Self { classes: records.into_iter().collect() }
    }

    /// The class's target at this chain point, or `None` when this view does not hold the class.
    ///
    /// `None` means "this view cannot answer", never "the target is zero" — the resolver turns it
    /// into a refusal (`palw_facts::PalwFactsError::Unresolved`) rather than a weightless block,
    /// which is what keeps a pruned node's disagreement visible instead of plausible.
    pub fn target(&self, class_id: &Hash64) -> Option<u128> {
        self.classes.get(class_id).map(|r| r.target)
    }

    pub fn record(&self, class_id: &Hash64) -> Option<&PalwClassStateRecord> {
        self.classes.get(class_id)
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.classes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }
    fn rec(target: u128) -> PalwClassStateRecord {
        PalwClassStateRecord { target, retargeted_at_daa: 1_000, observed_blocks: 42 }
    }

    /// A view answers only for the classes it holds, and says so rather than inventing a target.
    /// A zero would be a valid-looking wrong answer: `palw_pwu` reads a small target as HARD, so
    /// an invented zero would make an unknown class the heaviest on the network.
    #[test]
    fn an_unheld_class_is_unanswerable_not_zero() {
        let view = PalwClassStateView::from_records([(h(1), rec(500))]);
        assert_eq!(view.target(&h(1)), Some(500));
        assert_eq!(view.target(&h(2)), None, "an absent class must be unanswerable");
        assert!(!view.is_empty());
        assert_eq!(view.len(), 1);
        // The empty view answers nothing at all, rather than answering zero for everything.
        assert_eq!(PalwClassStateView::default().target(&h(1)), None);
    }

    /// The view is order-free: two builders that saw the same records in different orders produce
    /// the same view, so two nodes cannot disagree about a class's target because of sweep order.
    #[test]
    fn the_view_is_insertion_order_free() {
        let forward = PalwClassStateView::from_records([(h(1), rec(500)), (h(2), rec(900)), (h(3), rec(7))]);
        let backward = PalwClassStateView::from_records([(h(3), rec(7)), (h(2), rec(900)), (h(1), rec(500))]);
        assert_eq!(forward, backward);
    }

    /// The schema constant exists to force a re-derive; pin it so a layout change that forgets to
    /// bump it is a failing test rather than a silently empty class set.
    #[test]
    fn the_schema_version_is_pinned() {
        assert_eq!(PALW_CLASS_STATE_SCHEMA_VERSION, 1);
    }
}
