//! MISAKA VLT PR 2 (§5): the per-epoch **frozen voting snapshot** store.
//!
//! [`DbVltVotingSnapshotStore`] holds one sealed [`VltVotingSnapshot`] per wall epoch (key =
//! `u64` epoch) — the complete denominator a vote for that epoch is weighed against, with the
//! two roots ([`VltVotingSnapshot::snapshot_root`], [`VltVotingSnapshot::validator_set_root`])
//! the vote's signed [`kaspa_consensus_core::vlt::vote_snapshot_commitment`] binds.
//!
//! **Write-once per epoch, frozen at the boundary.** The virtual processor derives the row at
//! the first recompute of each wall epoch, pinned at a canonical lag-buried anchor, and never
//! overwrites it — that is "the validator set and its weights are fixed within an epoch"
//! expressed as store discipline. The row is *derived* state: any node re-derives the identical
//! bytes from the chain at the same pin, which is what lets the credit walk verify a vote's
//! commitment without trusting this store, and lets a fresh IBD converge to the same rows.
//!
//! Inert below the VLT shadow fence (no row is ever written), so this store stays empty on every
//! shipped preset.

use std::sync::Arc;

use kaspa_consensus_core::vlt::VltVotingSnapshot;
use kaspa_core::info;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

use super::U64Key;

/// The derivation rules the frozen rows were produced under. Write-once and derived is the same
/// dangerous pair [`super::vlt_credits::VLT_CREDITS_SCHEMA_VERSION`] documents: a derivation bug
/// is not fixed by fixing the code, because the wrong bytes are already on disk marked final.
///
/// * 1 — original (PR 2).
pub const VLT_VOTING_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Per-epoch frozen voting-snapshot store, keyed by `u64` wall epoch.
#[derive(Clone)]
pub struct DbVltVotingSnapshotStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, VltVotingSnapshot>,
    version: CachedDbItem<u32>,
}

impl DbVltVotingSnapshotStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::VltVotingSnapshots.into()),
            version: CachedDbItem::new(db, DatabaseStorePrefixes::VltVotingSnapshotsSchemaVersion.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Drop every row not derived under the current rules so they are re-frozen from the chain.
    /// Called once at startup; an ordinary restart is a no-op.
    pub fn reindex_if_stale(&mut self) -> Result<(), StoreError> {
        let stored = match self.version.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored == Some(VLT_VOTING_SNAPSHOT_SCHEMA_VERSION) {
            return Ok(());
        }
        if self.access.iterator().next().is_some() {
            info!(
                "[vlt-voting-snapshot] frozen rows were derived under rules v{} and this build derives \
                 v{VLT_VOTING_SNAPSHOT_SCHEMA_VERSION}; discarding them so they are re-frozen from the chain",
                stored.unwrap_or(1)
            );
            self.access.delete_all(DirectDbWriter::new(&self.db))?;
        }
        self.version.write(DirectDbWriter::new(&self.db), &VLT_VOTING_SNAPSHOT_SCHEMA_VERSION)
    }

    /// The frozen snapshot for `epoch`, or `KeyNotFound` if that epoch never froze one (every
    /// epoch below the shadow fence, and any epoch this node skipped over in one IBD commit).
    pub fn get(&self, epoch: u64) -> Result<VltVotingSnapshot, StoreError> {
        self.access.read(epoch.into())
    }

    pub fn has(&self, epoch: u64) -> Result<bool, StoreError> {
        self.access.has(epoch.into())
    }

    /// Freeze `epoch`'s snapshot into `batch`. The caller enforces write-once (freeze only when
    /// [`Self::has`] is false) and must never freeze a snapshot whose resolution is incomplete —
    /// both would seal a local accident of timing or storage into "the" denominator.
    pub fn set_batch(&self, batch: &mut WriteBatch, epoch: u64, snapshot: VltVotingSnapshot) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), epoch.into(), snapshot)
    }

    /// Direct (non-batched) write — the sign-path's lazy freeze and tests.
    pub fn set(&self, epoch: u64, snapshot: VltVotingSnapshot) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), epoch.into(), snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_consensus_core::vlt::{VLT_VOTING_SNAPSHOT_VERSION_V1, VltValidatorWeight, VltVotingSnapshot};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;

    fn sealed(epoch: u64, weight: u128) -> VltVotingSnapshot {
        VltVotingSnapshot {
            version: VLT_VOTING_SNAPSHOT_VERSION_V1,
            source_finalized_anchor: Hash64::from_bytes([0x11; 64]),
            source_anchor_daa: 1_000,
            snapshot_epoch: epoch.saturating_sub(2),
            activation_epoch: epoch,
            model_table_hash: Hash64::from_bytes([0x22; 64]),
            capability_set_root: Hash64::from_bytes([0x33; 64]),
            validator_set_root: Hash64::default(),
            credit_table_root: Hash64::from_bytes([0x44; 64]),
            snapshot_root: Hash64::default(),
            validators: vec![VltValidatorWeight {
                validator_id: Hash64::from_bytes([0x55; 64]),
                consensus_key: vec![0x66; 16],
                bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([0x77; 64]), 0),
                raw_recent_compute: weight * 2,
                bond_cap: weight,
                effective_weight: weight,
            }],
            total_weight: 0,
            quorum_weight: 0,
            resolution_complete: true,
        }
        .seal()
    }

    /// A frozen row must survive the store byte-exactly (u128 weights, key bytes, roots) and be
    /// readable through a fresh cache — the restart the freeze exists to survive.
    #[test]
    fn voting_snapshot_store_roundtrip_and_reindex() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbVltVotingSnapshotStore::new(db.clone(), CachePolicy::Count(8));
        store.reindex_if_stale().unwrap();

        assert!(store.get(7).is_err());
        let row = sealed(7, 340_282_366_920_938_463_463u128);
        let mut batch = WriteBatch::default();
        store.set_batch(&mut batch, 7, row.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.clone_with_new_cache(CachePolicy::Count(8)).get(7).unwrap(), row);

        // Same rules ⇒ a restart keeps the rows; older rules ⇒ they are discarded for re-freezing.
        store.reindex_if_stale().unwrap();
        assert!(store.has(7).unwrap());
        store.version.write(DirectDbWriter::new(&db), &0).unwrap();
        let mut reopened = DbVltVotingSnapshotStore::new(db.clone(), CachePolicy::Count(8));
        reopened.reindex_if_stale().unwrap();
        assert!(!reopened.has(7).unwrap(), "rows frozen under superseded rules must not be read as final");
    }
}
