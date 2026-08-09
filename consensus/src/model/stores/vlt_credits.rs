//! MISAKA Verified LLM Token-Weighted BFT: the per-epoch **verified-compute credit**
//! accumulator (`X_i(epoch)`).
//!
//! [`DbVltCreditStore`] holds one [`VltEpochCredits`] per epoch (key = `u64` epoch). Without it,
//! every virtual-state commit would re-walk `vlt_credit_window_blue_score` of selected chain —
//! `credit_window_epochs + credit_delay_epochs` epochs deep — re-verifying an ML-DSA-87
//! executor signature plus a whole verifier committee's signatures for every certificate in that
//! window, just to rebuild a `C_i(E)` sum whose old terms did not change. This store is what
//! reduces that to a walk of the unfinalized tail.
//!
//! **Only finalized epochs are written.** [`vlt_epoch_finalized`] requires burial past both the
//! challenge window (no challenge can still zero one of the epoch's certificates) and the reorg
//! horizon (no branch under consideration can still carry different certificates for it). Below
//! that depth every branch shares the same history, which is what makes a single cached value
//! sound on the sink path *and* while the reorg gate scores a candidate branch — a cache keyed by
//! epoch alone would otherwise be branch-confused, which is a consensus split, not a slow path.
//!
//! Inert wherever the VLT fence is: no row is written below
//! `DnsParams::vlt.vlt_activation_daa_score` (`u64::MAX` on every shipped preset), so this store
//! stays empty on today's networks.
//!
//! `VltEpochCredits` is count-estimable only, so the store uses an **untracked (`Count`)** cache
//! policy — never `tracked_bytes`, which would call `estimate_mem_bytes` and panic (see
//! [`super::epoch_accumulator`]).

use std::sync::Arc;

use kaspa_consensus_core::vlt::VltEpochCredits;
use kaspa_core::info;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

use super::U64Key;

/// The derivation rules the rows in this store were produced under.
///
/// Bump this whenever a change alters what the credit walk would produce for an epoch it has
/// already recorded. Write-once and derived is a dangerous pair: a bug in the derivation is not
/// corrected by fixing the bug, because the wrong answer is already on disk and marked final, and
/// every later read prefers it to the truth. The version is the only way back.
///
/// * 1 — original.
/// * 2 — the credit walk bounded a certificate's phase-1 commitment by the certificate floor
///   rather than by its own dependency horizon, so in the steady state every certificate resolved
///   to "commitment missing" and the epochs were sealed as empty. Rows written under rule 1 record
///   a walk that could not see what it was judging.
/// * 3 — the verifier committee's candidate pool moved from the credit walk to the capability
///   store, and is now bound to the beacon anchor's own chain history. Rows written under rule 2
///   drew that pool from whatever declarations the walk happened to hold, which empties as the walk
///   floor rises past a declaration that is still in force — so they record certificates as
///   unverified that a committee-aware walk credits.
///
/// The rule is the *resolution* semantics, not the row layout: bump this whenever a change alters
/// what the walk would decide about an epoch it has already recorded, because write-once means the
/// old decision outlives the code that made it.
pub const VLT_CREDITS_SCHEMA_VERSION: u32 = 3;

/// Per-epoch verified-compute credit store, keyed by `u64` epoch. Write-once per epoch: a row
/// appears only after the epoch is finalized, and a finalized epoch never changes.
#[derive(Clone)]
pub struct DbVltCreditStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, VltEpochCredits>,
    version: CachedDbItem<u32>,
}

impl DbVltCreditStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::VltCredits.into()),
            version: CachedDbItem::new(db, DatabaseStorePrefixes::VltCreditsSchemaVersion.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Drop every row not derived under the current rules, so they are rebuilt from the chain.
    ///
    /// Called once at startup. Only the *derived* accumulator is discarded — the blocks, bonds,
    /// commitments, certificates, verdicts and challenges every row is computed from stay exactly
    /// where they are, so this is a recomputation, not a resync.
    ///
    /// An absent version marker with rows present means a database written before versioning
    /// existed, which is version 1 by definition. An absent marker with no rows is a fresh
    /// database and simply gets stamped.
    pub fn reindex_if_stale(&mut self) -> Result<(), StoreError> {
        let stored = match self.version.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored == Some(VLT_CREDITS_SCHEMA_VERSION) {
            return Ok(());
        }
        let had_rows = self.access.iterator().next().is_some();
        if had_rows {
            info!(
                "[vlt-credit] accumulator rows were derived under rules v{} and this build derives v{VLT_CREDITS_SCHEMA_VERSION}; \
                 discarding them so they are recomputed from the chain (no blocks or overlay transactions are affected)",
                stored.unwrap_or(1)
            );
            self.access.delete_all(DirectDbWriter::new(&self.db))?;
        }
        self.version.write(DirectDbWriter::new(&self.db), &VLT_CREDITS_SCHEMA_VERSION)
    }

    /// `epoch`'s finalized credits, or `StoreError::KeyNotFound` if it is not finalized yet
    /// (every epoch while the VLT fence is inert).
    pub fn get(&self, epoch: u64) -> Result<VltEpochCredits, StoreError> {
        self.access.read(epoch.into())
    }

    pub fn has(&self, epoch: u64) -> Result<bool, StoreError> {
        self.access.has(epoch.into())
    }

    /// Persist a finalized epoch's credits into `batch`.
    ///
    /// The caller must only call this for an epoch [`vlt_epoch_finalized`] accepts. Writing a
    /// live epoch would cache a value that a later challenge or reorg could still change, and
    /// every subsequent read would silently prefer the stale row over the truth.
    ///
    /// [`vlt_epoch_finalized`]: kaspa_consensus_core::vlt::vlt_epoch_finalized
    pub fn set_batch(&self, batch: &mut WriteBatch, epoch: u64, credits: VltEpochCredits) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), epoch.into(), credits)
    }

    /// Direct (non-batched) write — tests / diagnostics only.
    pub fn set(&self, epoch: u64, credits: VltEpochCredits) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), epoch.into(), credits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;

    fn row(validator: u8, x: u128) -> VltEpochCredits {
        VltEpochCredits::from_unordered([(Hash64::from_bytes([validator; 64]), x)])
    }

    /// The rows are derived AND write-once, which is the pair that makes a derivation bug
    /// permanent: fixing the walk does not fix the answer already recorded as final. The version
    /// is the only way back, so it has to actually drop the old rows — and it must not drop
    /// anything on an ordinary restart, or every startup would pay a full re-walk.
    #[test]
    fn a_rules_change_discards_rows_derived_under_the_old_ones() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbVltCreditStore::new(db.clone(), CachePolicy::Count(16));

        // A fresh database is stamped without discarding anything.
        store.reindex_if_stale().unwrap();
        store.set(7, row(1, 500)).unwrap();
        assert!(store.has(7).unwrap());

        // Same rules ⇒ untouched. A restart must not throw away the work the store exists to save.
        store.reindex_if_stale().unwrap();
        assert_eq!(store.get(7).unwrap(), row(1, 500));

        // Now simulate a database written under the previous rules.
        store.version.write(DirectDbWriter::new(&db), &(VLT_CREDITS_SCHEMA_VERSION - 1)).unwrap();
        let mut reopened = DbVltCreditStore::new(db.clone(), CachePolicy::Count(16));
        reopened.reindex_if_stale().unwrap();
        assert!(!reopened.has(7).unwrap(), "a row derived under superseded rules must not survive to be read as final");
        // And the marker is advanced, so the next start is a no-op rather than a second wipe.
        assert_eq!(reopened.version.read().unwrap(), VLT_CREDITS_SCHEMA_VERSION);
        reopened.set(7, row(2, 900)).unwrap();
        reopened.reindex_if_stale().unwrap();
        assert_eq!(reopened.get(7).unwrap(), row(2, 900));
    }

    /// A database written before versioning existed has rows and no marker. That is version 1 by
    /// definition, not a fresh database — treating it as fresh would keep exactly the rows the
    /// version exists to discard.
    #[test]
    fn rows_without_a_version_marker_are_treated_as_the_original_rules() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbVltCreditStore::new(db.clone(), CachePolicy::Count(16));
        store.set(3, row(1, 100)).unwrap();

        let mut reopened = DbVltCreditStore::new(db.clone(), CachePolicy::Count(16));
        reopened.reindex_if_stale().unwrap();
        assert!(!reopened.has(3).unwrap());
    }
}
