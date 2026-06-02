//! kaspa-pq ADR-0018 "本格版" (PoS-v2 economics, Phase 1): the per-epoch
//! **accumulator** and its per-block **quality-pool** input store.
//!
//! [`DbEpochAccumulatorStore`] holds one [`EpochTally`] per epoch (key = `u64`
//! epoch), recomputed deterministically from the selected-chain bounded window
//! at each virtual-state commit (`VirtualStateProcessor::update_epoch_accumulator`,
//! mirroring the `update_dns_state` recompute precedent — reorg-safe with no
//! incremental delta). Live (non-finalized) epochs are overwritten each commit;
//! once an epoch finalizes its tally is immutable and the recompute skips it.
//!
//! [`DbBlockQualityPoolStore`] holds the per-block validator quality sub-pool
//! ([`split_validator_pool`]`.1`, key = `BlockHash`) that the accumulator sums —
//! persisted because the per-block `validator_pool` is not cheaply re-derivable
//! from a historical block (it needs that block's mergeset reward data). It is
//! the exact per-block sibling of [`super::rewarded_epochs`] and is pruned
//! alongside it.
//!
//! Both stores are **inert on every current network**: a write happens only past
//! `DnsParams::pos_v2_activation_daa_score` (`u64::MAX` everywhere today), so no
//! row is ever written and the recompute returns before touching the DB.
//!
//! Both values (`EpochTally`, `u64`) are unit-/count-estimable only, so the
//! stores use an **untracked (`Count`)** cache policy — never `tracked_bytes`,
//! which would call `estimate_mem_bytes` and panic (the validator-attestation
//! `virtual-processor` crash; see [`super::rewarded_epochs`]).
//!
//! [`split_validator_pool`]: kaspa_consensus_core::dns_finality::split_validator_pool

use std::sync::Arc;

use kaspa_consensus_core::dns_finality::EpochTally;
use kaspa_consensus_core::{BlockHash, BlockHasher};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

use super::U64Key;

/// Per-epoch [`EpochTally`] accumulator store (ADR-0018 "本格版" Phase 1), keyed by
/// `u64` epoch. Recomputed/overwritten per virtual-state commit while an epoch is
/// live; immutable once finalized.
#[derive(Clone)]
pub struct DbEpochAccumulatorStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, EpochTally>,
}

impl DbEpochAccumulatorStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::EpochAccumulator.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// The tally for `epoch`, or `StoreError::KeyNotFound` if the epoch has not
    /// been accumulated yet (every epoch while the overlay is dormant).
    pub fn get(&self, epoch: u64) -> Result<EpochTally, StoreError> {
        self.access.read(epoch.into())
    }

    pub fn has(&self, epoch: u64) -> Result<bool, StoreError> {
        self.access.has(epoch.into())
    }

    /// Write (overwrite) `epoch`'s tally into `batch`. Overwrite — not append —
    /// because a live epoch is recomputed each commit; the caller never calls
    /// this for an already-finalized epoch.
    pub fn set_batch(&self, batch: &mut WriteBatch, epoch: u64, tally: EpochTally) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), epoch.into(), tally)
    }

    /// Direct (non-batched) write — tests / diagnostics only.
    pub fn set(&self, epoch: u64, tally: EpochTally) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), epoch.into(), tally)
    }
}

/// Per-block validator quality sub-pool store (ADR-0018 "本格版" Phase 1), keyed by
/// `BlockHash`. The per-block recompute input the accumulator sums. Append-once
/// per chain block (mirrors [`super::rewarded_epochs::DbRewardedEpochsStore`]);
/// deleted on prune.
#[derive(Clone)]
pub struct DbBlockQualityPoolStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, u64, BlockHasher>,
}

impl DbBlockQualityPoolStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::BlockValidatorQualityPool.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn get(&self, hash: BlockHash) -> Result<u64, StoreError> {
        self.access.read(hash)
    }

    /// Append this block's quality sub-pool. Each chain block writes once (the
    /// commit path is append-only per block), so a re-insert is rejected.
    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, quality_subpool: u64) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, quality_subpool)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

/// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4) — per-block **cumulative security-reserve balance**
/// store, keyed by `BlockHash`. `balance_after(block) = balance_after(selected_parent) +
/// slashing-reserve accrual − reserve drip` (the recurrence is applied in the virtual processor).
/// Keyed by block hash — like [`DbBlockQualityPoolStore`] / [`super::rewarded_epochs`] — so the
/// finalizing coinbase reads the **selected parent's** committed balance for the per-epoch drip
/// (construction == validation, reorg-safe, no lagging singleton) and the recurrence only ever
/// touches the immediate (recent, never-pruned) parent. Written only past
/// `pos_v2_activation_daa_score` (a balance of 0 is the default, never stored), so it is inert on
/// every current network. Deleted on prune.
#[derive(Clone)]
pub struct DbReserveBalanceStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlockHash, u64, BlockHasher>,
}

impl DbReserveBalanceStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::ReserveBalance.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// The cumulative reserve balance after `hash`, or `StoreError::KeyNotFound` when the block
    /// stored no balance (balance 0 — every block while the v2 economics are dormant). Callers
    /// map the absent case to `0`.
    pub fn get(&self, hash: BlockHash) -> Result<u64, StoreError> {
        self.access.read(hash)
    }

    /// Persist the balance after `hash`. Each chain block writes once (append-only commit path).
    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: BlockHash, balance: u64) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, balance)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: BlockHash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;

    fn tally(stake: u128, payload: u8, included_stake: u64, pool: u128, finalized: bool) -> EpochTally {
        EpochTally {
            expected_stake: stake,
            included: vec![(Hash64::from_bytes([payload; 64]), included_stake)],
            quality_pool_accrued: pool,
            finalized,
        }
    }

    /// The per-epoch accumulator round-trips a full `EpochTally` (incl. the u128
    /// stake/pool fields through bincode) and a live epoch is **overwritten** on a
    /// subsequent recompute (e.g. when it finalizes).
    #[test]
    fn epoch_accumulator_set_get_overwrite() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbEpochAccumulatorStore::new(db, CachePolicy::Count(16));

        // Absent before the first write.
        assert!(store.get(7).is_err());
        assert!(!store.has(7).unwrap());

        let live = tally(300, 0xA1, 100, 12, false);
        store.set(7, live.clone()).unwrap();
        assert_eq!(store.get(7).unwrap(), live);
        assert!(store.has(7).unwrap());

        // Overwrite the same epoch (recompute → finalized): the latest value wins.
        let finalized = tally(300, 0xA1, 100, 12, true);
        store.set(7, finalized.clone()).unwrap();
        assert_eq!(store.get(7).unwrap(), finalized);
        // A different epoch is independent.
        assert!(store.get(8).is_err());
    }

    /// The per-block quality-pool store inserts/reads/deletes a `u64` and rejects a
    /// re-insert on the same block hash (per-block append-once).
    #[test]
    fn block_quality_pool_insert_get_delete() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbBlockQualityPoolStore::new(db.clone(), CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x11; 64]);

        assert!(store.get(h).is_err());
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, h, 4_242).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get(h).unwrap(), 4_242);

        // Second insert on the same key is rejected (each chain block writes once).
        let mut batch = WriteBatch::default();
        assert!(store.insert_batch(&mut batch, h, 1).is_err());

        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, h).unwrap();
        db.write(batch).unwrap();
        assert!(store.get(h).is_err());
    }

    /// The per-block reserve-balance store inserts/reads/deletes a `u64`; an absent block reads as
    /// `KeyNotFound` (callers map to balance 0).
    #[test]
    fn reserve_balance_insert_get_delete() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbReserveBalanceStore::new(db.clone(), CachePolicy::Count(16));
        let h = Hash64::from_bytes([0x22; 64]);

        assert!(store.get(h).is_err());
        let mut batch = WriteBatch::default();
        store.insert_batch(&mut batch, h, 9_000_000).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get(h).unwrap(), 9_000_000);

        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, h).unwrap();
        db.write(batch).unwrap();
        assert!(store.get(h).is_err());
    }
}
