//! ADR-0038 Decision D: per-`ExecutionClass` difficulty state.
//!
//! This store holds a class's **remembered** facts: when it last had a commitment credited, and its
//! status in the ADR-0028 ladder.
//!
//! # The DAA target is NOT here, and that is the design
//!
//! Schema 2 carried `target`, `retargeted_at_daa` and `observed_blocks`. Nothing ever wrote them —
//! the store's only insert site rewrites `last_credited_daa` on an already-existing row — and
//! nothing outside this file's own tests ever read `target`. Keeping them would have handed the
//! ADR-0039 W4′ weight wiring a ready-to-use, SINK-RELATIVE answer to a question that must be
//! chain-scoped, and a plausible wrong answer is worse than a missing one.
//!
//! A store keyed by class id holds ONE value: whatever the virtual chain last wrote. A block's
//! weight derived from it depends on where THIS node's virtual tip happens to point — the 2026-08-17
//! audit's blocker 6(b), and the same defect that had to be removed from the audit-bond spend gate
//! and from the class credit mark. The target's one legal source is a fold over the BLOCK'S OWN
//! selected-parent chain (`palw_class_daa::fold_class_target_v1`). Removing the fields makes that
//! structural instead of a comment asking for it: there is no target here to read by mistake.
//!
//! # Reads must still be chain-scoped
//!
//! The same argument applies to what remains. `last_credited_daa` gates a mint and `status` is the
//! emergency stop, so callers build a [`PalwClassStateView`] for the chain point they are
//! evaluating, exactly as bond consumers build an `ActiveBondView`.
//!
//! # Absent is not zero
//!
//! [`PALW_CLASS_STATE_SCHEMA_VERSION`] exists for the reason the carriage store's does: a row the
//! iterator cannot decode is dropped silently, so a layout change would make every class read as
//! *absent* rather than as broken. Absent must therefore fail closed at every consumer — which is
//! why [`PalwClassStateView::is_frozen`] answers `true` for a class it does not hold. Bump this on
//! any layout change so the rows are discarded and re-derived instead of read as missing.

use std::collections::BTreeMap;
use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, CachedDbItem, DB, DirectDbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

/// Bump on ANY change to [`PalwClassStateRecord`]'s layout. See the module docs: without it a
/// layout change reads as an empty store, which is indistinguishable from "no class has a target".
pub const PALW_CLASS_STATE_SCHEMA_VERSION: u32 = 3;

/// One class's difficulty state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwClassStateRecord {
    /// DAA at which this class last had a commitment CREDITED, or `None` if never.
    ///
    /// ADR-0033 §4e bounds an attacker's pre-unbonding gain as
    /// `base(C) × (unbonding / min_credit_interval + 1)` — the inequality ASSUMES one credited job
    /// per `min_credit_interval_daa`. That assumption cannot be checked from the credit walk: the
    /// walk spans `w_challenge` backward and a commitment crosses `w_challenge` AFTER acceptance,
    /// so previous credits are outside it by construction (audit B4). It has to be remembered, and
    /// this is where.
    ///
    /// `None` means "never credited", which is the permissive-and-correct start: the first credit
    /// on a class has no predecessor to be too close to.
    pub last_credited_daa: Option<u64>,
    /// The class's status in the ADR-0028 ladder — the source `class_frozen` reads.
    ///
    /// Stored as the discriminant rather than the enum so a reordering of
    /// [`PalwClassStatusV3`](kaspa_consensus_core::palw_dispute::PalwClassStatusV3) cannot silently
    /// renumber what pruning-surviving bytes mean; the enum's own docs demand exactly that. Read
    /// back through [`PalwClassStateView::status`], which refuses an unknown discriminant rather
    /// than defaulting it to `Active`.
    pub status_discriminant: u8,
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

    /// Drop every row when the on-disk layout is not the one this build reads.
    ///
    /// Mirrors `DbPalwCarriageStore::reindex_if_stale` and exists for the same reason: an
    /// undecodable row is dropped SILENTLY by the iterator, so a layout change would make every
    /// class read as absent rather than as broken. Absent is not neutral here — `is_frozen` and
    /// `credit_interval_elapsed` both fail closed on it, which is safe, but it would silently
    /// disable a live class rather than tell the operator why.
    pub fn reindex_if_stale(&mut self) -> Result<(), StoreError> {
        let stored = match self.schema.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored == Some(PALW_CLASS_STATE_SCHEMA_VERSION) {
            return Ok(());
        }
        if self.access.iterator().next().is_some() {
            kaspa_core::info!(
                "[palw-class-state] rows were written under layout v{} and this build reads \
                 v{PALW_CLASS_STATE_SCHEMA_VERSION}; discarding them so they are re-derived",
                stored.unwrap_or(1)
            );
            self.access.delete_all(DirectDbWriter::new(&self.db))?;
        }
        self.schema.write(DirectDbWriter::new(&self.db), &PALW_CLASS_STATE_SCHEMA_VERSION)
    }

    /// Every stored `(class_id, record)`, for building a [`PalwClassStateView`].
    pub fn iterator(&self) -> impl Iterator<Item = Result<(Hash64, Arc<PalwClassStateRecord>), Box<dyn std::error::Error>>> + '_ {
        self.access.iterator().map(|res| {
            res.map(|(key, record)| {
                let mut bytes = [0u8; 64];
                bytes.copy_from_slice(&key);
                (Hash64::from_bytes(bytes), record)
            })
        })
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

    /// The class's ladder status at this chain point, or `None` when this view cannot answer —
    /// either it does not hold the class, or the stored discriminant is not one this build knows.
    ///
    /// An unknown discriminant is deliberately NOT defaulted. Defaulting it to `Active` would let a
    /// forward-incompatible row re-open a frozen class, which is the one transition ADR-0028's
    /// status machine refuses outright.
    pub fn status(&self, class_id: &Hash64) -> Option<kaspa_consensus_core::palw_dispute::PalwClassStatusV3> {
        use kaspa_consensus_core::palw_dispute::PalwClassStatusV3 as S;
        match self.classes.get(class_id)?.status_discriminant {
            0 => Some(S::Inactive),
            1 => Some(S::Probation),
            2 => Some(S::Active),
            3 => Some(S::Frozen),
            4 => Some(S::Deprecated),
            _ => None,
        }
    }

    /// Is this class frozen, as far as this chain point can tell?
    ///
    /// `true` for a class this view cannot answer for, because the emergency stop must fail CLOSED:
    /// a node that cannot establish a class is running must not draw panels for it or mint on it.
    /// That is the opposite of the `frozen: false` this used to be hardcoded to at the live panel
    /// site, where the emergency stop existed as a type and could never fire (audit §3.4).
    pub fn is_frozen(&self, class_id: &Hash64) -> bool {
        !matches!(self.status(class_id), Some(kaspa_consensus_core::palw_dispute::PalwClassStatusV3::Active))
    }

    /// DAA of this class's last credited commitment, or `None` if it has never been credited or
    /// this view does not hold the class. See [`PalwClassStateRecord::last_credited_daa`].
    pub fn last_credited_daa(&self, class_id: &Hash64) -> Option<u64> {
        self.classes.get(class_id)?.last_credited_daa
    }

    /// Is a commitment accepted at `accepted_daa` far enough past this class's last credit to be
    /// creditable under `min_credit_interval_daa`?
    ///
    /// This is the predicate ADR-0033 §4e's `jobs` term assumes and nothing enforced. A class that
    /// has never been credited passes; a class this view does not hold does NOT, for the same
    /// fail-closed reason `is_frozen` refuses.
    pub fn credit_interval_elapsed(&self, class_id: &Hash64, accepted_daa: u64, min_credit_interval_daa: u64) -> bool {
        let Some(record) = self.classes.get(class_id) else { return false };
        match record.last_credited_daa {
            None => true,
            Some(last) => accepted_daa.saturating_sub(last) >= min_credit_interval_daa,
        }
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
    fn rec() -> PalwClassStateRecord {
        PalwClassStateRecord { last_credited_daa: None, status_discriminant: 2 } // Active
    }
    fn credited(daa: u64) -> PalwClassStateRecord {
        PalwClassStateRecord { last_credited_daa: Some(daa), status_discriminant: 2 }
    }

    /// A view answers only for the classes it holds, and every unanswered question fails CLOSED.
    ///
    /// This test used to be about `target`, which this store no longer carries — the target is a fold
    /// over the block's own chain, not a row (see the module header). What remains has the same
    /// hazard shape and the opposite safe direction: an absent class must read as FROZEN and as
    /// not-yet-creditable, never as Active with no credit history.
    #[test]
    fn an_unheld_class_is_unanswerable_and_fails_closed() {
        let view = PalwClassStateView::from_records([(h(1), credited(900))]);
        assert_eq!(view.last_credited_daa(&h(1)), Some(900));
        assert_eq!(view.status(&h(1)), Some(kaspa_consensus_core::palw_dispute::PalwClassStatusV3::Active));
        assert!(!view.is_frozen(&h(1)));

        assert_eq!(view.last_credited_daa(&h(2)), None, "an absent class must be unanswerable");
        assert_eq!(view.status(&h(2)), None);
        assert!(view.is_frozen(&h(2)), "an absent class is frozen — the fail-closed direction");
        assert!(!view.is_empty());
        assert_eq!(view.len(), 1);
        // The empty view freezes everything rather than admitting everything.
        assert!(PalwClassStateView::default().is_frozen(&h(1)));
        assert_eq!(PalwClassStateView::default().status(&h(1)), None);
    }

    /// The view is order-free: two builders that saw the same records in different orders produce
    /// the same view, so two nodes cannot disagree about a class's target because of sweep order.
    #[test]
    fn the_view_is_insertion_order_free() {
        let forward = PalwClassStateView::from_records([(h(1), credited(500)), (h(2), credited(900)), (h(3), rec())]);
        let backward = PalwClassStateView::from_records([(h(3), rec()), (h(2), credited(900)), (h(1), credited(500))]);
        assert_eq!(forward, backward);
    }

    /// The schema constant exists to force a re-derive; pin it so a layout change that forgets to
    /// bump it is a failing test rather than a silently empty class set.
    #[test]
    fn the_schema_version_is_pinned() {
        assert_eq!(PALW_CLASS_STATE_SCHEMA_VERSION, 3);
    }

    /// The emergency stop fails CLOSED, in both directions a node can be ignorant.
    ///
    /// `class_frozen` was hardcoded `false` at the live panel site, so the stop existed as a type
    /// and could never fire. Now it is read from chain state — and a class this view cannot answer
    /// for reads as frozen, because a node that cannot establish a class is running must not draw
    /// panels for it or mint on it.
    #[test]
    fn an_unanswerable_class_reads_as_frozen() {
        use kaspa_consensus_core::palw_dispute::PalwClassStatusV3 as S;
        let active = PalwClassStateView::from_records([(h(1), rec())]);
        assert_eq!(active.status(&h(1)), Some(S::Active));
        assert!(!active.is_frozen(&h(1)), "an Active class is not frozen");

        // A class this view does not hold.
        assert_eq!(active.status(&h(2)), None);
        assert!(active.is_frozen(&h(2)), "an unheld class must fail closed");

        // Every non-Active status is frozen for panel/mint purposes, including the ones that are
        // not literally `Frozen`: Probation is not a partial pass (ADR-0028's six-path gate).
        for (discriminant, status) in [(0u8, S::Inactive), (1, S::Probation), (3, S::Frozen), (4, S::Deprecated)] {
            let mut r = rec();
            r.status_discriminant = discriminant;
            let view = PalwClassStateView::from_records([(h(1), r)]);
            assert_eq!(view.status(&h(1)), Some(status));
            assert!(view.is_frozen(&h(1)), "{status:?} must not draw panels or mint");
        }

        // An unknown discriminant is unanswerable, NOT defaulted to Active — defaulting would let a
        // forward-incompatible row re-open a frozen class.
        let mut unknown = rec();
        unknown.status_discriminant = 200;
        let view = PalwClassStateView::from_records([(h(1), unknown)]);
        assert_eq!(view.status(&h(1)), None);
        assert!(view.is_frozen(&h(1)));
    }

    /// ADR-0033 §4e assumes one credited job per `min_credit_interval_daa`; this is the predicate
    /// that makes the assumption true, and it cannot be derived from the credit walk (audit B4).
    #[test]
    fn the_credit_interval_is_measured_against_remembered_state() {
        // Never credited: the first credit has no predecessor to be too close to.
        let fresh = PalwClassStateView::from_records([(h(1), rec())]);
        assert!(fresh.credit_interval_elapsed(&h(1), 0, 100));
        assert_eq!(fresh.last_credited_daa(&h(1)), None);

        let mut credited = rec();
        credited.last_credited_daa = Some(1_000);
        let view = PalwClassStateView::from_records([(h(1), credited)]);
        assert!(!view.credit_interval_elapsed(&h(1), 1_099, 100), "one short of the interval is too soon");
        assert!(view.credit_interval_elapsed(&h(1), 1_100, 100), "exactly the interval is enough");
        assert!(view.credit_interval_elapsed(&h(1), 5_000, 100));
        // A point of view BEHIND the last credit (an ordinary reorg state) is not elapsed.
        assert!(!view.credit_interval_elapsed(&h(1), 900, 100));

        // A class this view does not hold fails closed, like the freeze check.
        assert!(!view.credit_interval_elapsed(&h(2), 9_999, 100));
    }
}
