//! kaspa-pq Phase 10 (ADR-0009): singleton store for the per-anchor
//! [`DnsState`] (work/stake depth, last DNS-confirmed anchor, rollout
//! stage). Mirrors the [`super::headers_selected_tip`] singleton pattern
//! — one `CachedDbItem` keyed by [`DatabaseStorePrefixes::DnsState`].
//!
//! Written by the virtual processor on each virtual-state commit
//! (PR-10.6/10.7) and read by the `getDnsConfirmation` RPC (PR-10.14).
//! Before the first write, [`DnsStateStoreReader::get`] returns
//! `StoreError::KeyNotFound`, which callers map to "overlay dormant".

use kaspa_consensus_core::dns_finality::DnsState;
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
}
