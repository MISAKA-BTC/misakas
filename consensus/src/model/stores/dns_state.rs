//! kaspa-pq Phase 10 (ADR-0009): singleton store for the per-anchor
//! [`DnsState`] (work/stake depth, last DNS-confirmed anchor, rollout
//! stage). Mirrors the [`super::headers_selected_tip`] singleton pattern
//! — one `CachedDbItem` keyed by [`DatabaseStorePrefixes::DnsState`].
//!
//! Written by the virtual processor on each virtual-state commit
//! (PR-10.6/10.7) and read by the `getDnsConfirmation` RPC (PR-10.14).
//! Before the first write, [`DnsStateStoreReader::get`] returns
//! `StoreError::KeyNotFound`, which callers map to "overlay dormant".
//!
//! MISAKA VLT PR 1: also home to the [`VltActivationRecord`] singleton
//! ([`DatabaseStorePrefixes::VltActivation`]) — same pattern, written in
//! the same per-epoch recompute batch as `DnsState`. A separate row
//! rather than a `DnsState` field because `DnsState` is a live per-anchor
//! gauge rewritten every epoch, while the record is a state machine whose
//! whole value is *not* moving unless a transition happened — and because
//! appending to `DnsState`'s borsh layout would invalidate every deployed
//! row for a field that changes a handful of times per network lifetime.

use kaspa_consensus_core::dns_finality::DnsState;
use kaspa_consensus_core::vlt::VltActivationRecord;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Reader API for `DnsStateStore`.
pub trait DnsStateStoreReader {
    fn get(&self) -> StoreResult<DnsState>;
}

pub trait DnsStateStore: DnsStateStoreReader {
    fn set(&mut self, state: DnsState) -> StoreResult<()>;
}

/// A DB + cache implementation of the `DnsStateStore` trait.
#[derive(Clone)]
pub struct DbDnsStateStore {
    db: Arc<DB>,
    access: CachedDbItem<DnsState>,
}

impl DbDnsStateStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::DnsState.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, state: DnsState) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &state)
    }
}

impl DnsStateStoreReader for DbDnsStateStore {
    fn get(&self) -> StoreResult<DnsState> {
        self.access.read()
    }
}

impl DnsStateStore for DbDnsStateStore {
    fn set(&mut self, state: DnsState) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &state)
    }
}

/// Reader API for `VltActivationStore`.
pub trait VltActivationStoreReader {
    /// `StoreError::KeyNotFound` before the first write — i.e. the weight fence has never been
    /// crossed on this consensus, which callers treat as "no record" rather than an error.
    fn get(&self) -> StoreResult<VltActivationRecord>;
}

pub trait VltActivationStore: VltActivationStoreReader {
    fn set(&mut self, record: VltActivationRecord) -> StoreResult<()>;
}

/// A DB + cache implementation of the `VltActivationStore` trait.
#[derive(Clone)]
pub struct DbVltActivationStore {
    db: Arc<DB>,
    access: CachedDbItem<VltActivationRecord>,
}

impl DbVltActivationStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::VltActivation.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, record: VltActivationRecord) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &record)
    }
}

impl VltActivationStoreReader for DbVltActivationStore {
    fn get(&self) -> StoreResult<VltActivationRecord> {
        self.access.read()
    }
}

impl VltActivationStore for DbVltActivationStore {
    fn set(&mut self, record: VltActivationRecord) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::BlueWorkType;
    use kaspa_consensus_core::dns_finality::{DnsHealth, DnsRolloutStage, StakeScore};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;

    fn fixture() -> DnsState {
        DnsState {
            selected_chain_anchor: Hash64::from_bytes([0x11; 64]),
            anchor_daa_score: 123_456,
            work_depth: BlueWorkType::from_u64(9_999_999),
            // > 2^64 to exercise the u128 StakeScore through bincode.
            stake_depth: StakeScore(123_456_789_012_345_678_901u128),
            last_dns_confirmed_anchor: Hash64::from_bytes([0x22; 64]),
            last_dns_confirmed_anchor_daa_score: 123_000,
            rollout_stage: DnsRolloutStage::Active,
            validator_set_commitment: Hash64::from_bytes([0x33; 64]),
            health: DnsHealth::DegradedStakeQualityLow,
        }
    }

    #[test]
    fn dns_state_store_roundtrip_direct_and_batch() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbDnsStateStore::new(db.clone());

        // Before the first write the singleton is absent.
        assert!(store.get().is_err());

        // Direct write/read round-trips the full struct (incl. u128 + BlueWorkType).
        let s = fixture();
        store.set(s.clone()).unwrap();
        assert_eq!(store.get().unwrap(), s);

        // Batch write overwrites the singleton.
        let mut s2 = s.clone();
        s2.anchor_daa_score = 999;
        s2.stake_depth = StakeScore(0);
        s2.rollout_stage = DnsRolloutStage::Bootstrap;
        s2.health = DnsHealth::Active;
        let mut batch = WriteBatch::default();
        store.set_batch(&mut batch, s2.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.get().unwrap(), s2);
    }

    /// The record is what a restart resumes the §6 activation machine from, so the row has to
    /// survive the store byte-exactly — including the `u128` weights and every enum arm. A fresh
    /// cache over the same DB (the restart shape) must read the same record back.
    #[test]
    fn vlt_activation_store_roundtrip_and_reopen() {
        use kaspa_consensus_core::vlt::{PersistedVltActivationState, VltActivationRecord};

        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbVltActivationStore::new(db.clone());

        // No record until the weight fence has been crossed once.
        assert!(store.get().is_err());

        let scheduled = VltActivationRecord {
            state: PersistedVltActivationState::ActivationScheduled,
            source_anchor: Hash64::from_bytes([0x44; 64]),
            snapshot_epoch: 41,
            snapshot_root: Hash64::from_bytes([0x55; 64]),
            scheduled_at_epoch: 46,
            activation_epoch: 47,
            // > 2^64 to exercise the u128 weights through the serializer.
            total_weight: 340_282_366_920_938_463_463u128,
            quorum_weight: 226_854_911_280_625_642_309u128,
            ..VltActivationRecord::awaiting()
        };
        store.set(scheduled.clone()).unwrap();
        assert_eq!(store.get().unwrap(), scheduled);

        // Batch write (the recompute path) + a fresh cache over the same DB (the restart path).
        let mut active = scheduled.clone();
        active.state = PersistedVltActivationState::Active;
        let mut batch = WriteBatch::default();
        store.set_batch(&mut batch, active.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.clone_with_new_cache().get().unwrap(), active);
    }
}
