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
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

use super::U64Key;

/// Per-epoch verified-compute credit store, keyed by `u64` epoch. Write-once per epoch: a row
/// appears only after the epoch is finalized, and a finalized epoch never changes.
#[derive(Clone)]
pub struct DbVltCreditStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, VltEpochCredits>,
}

impl DbVltCreditStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::VltCredits.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
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
