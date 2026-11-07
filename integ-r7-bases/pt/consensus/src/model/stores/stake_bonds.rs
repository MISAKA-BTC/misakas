//! kaspa-pq Phase 10 (ADR-0009): keyed store for [`StakeBondRecord`]s,
//! keyed by the [`TransactionOutpoint`] that created the bond. Backs the
//! `StakeScore` aggregation (PR-10.6/10.7) and the bond-existence /
//! bond-active stateful tx checks (PR-10.9).
//!
//! Mirrors the [`super::utxo_set`] outpoint-keyed pattern, but with a
//! fixed-width 68-byte key (`Hash64` txid ‖ `u32` index, no trailing-zero
//! trimming) so the iterator decode is a plain `TryFrom`. Records are
//! *mutated* in place as a bond moves `Pending → Active → Unbonding →
//! Slashed`, so write methods take `&mut self` and the store is held under
//! an `RwLock` (matching `utxo_set`'s cache-consistency convention).

use kaspa_consensus_core::{
    dns_finality::StakeBondRecord,
    tx::{TransactionIndexType, TransactionOutpoint},
};
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::prelude::{CachePolicy, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::{HASH64_SIZE, Hash64};
use rocksdb::WriteBatch;
use std::{error::Error, sync::Arc};

/// `{ Hash64 txid (64) ‖ u32 index (4) }` = 68 bytes (PR-9.5c widened the
/// txid from 32 to 64 bytes).
pub const STAKE_BOND_KEY_SIZE: usize = HASH64_SIZE + size_of::<TransactionIndexType>();

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct StakeBondKey([u8; STAKE_BOND_KEY_SIZE]);

impl AsRef<[u8]> for StakeBondKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for StakeBondKey {
    type Error = &'static str;
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        if slice.len() != STAKE_BOND_KEY_SIZE {
            return Err("stake-bond key slice has unexpected length");
        }
        let mut bytes = [0u8; STAKE_BOND_KEY_SIZE];
        bytes.copy_from_slice(slice);
        Ok(Self(bytes))
    }
}

impl From<TransactionOutpoint> for StakeBondKey {
    fn from(outpoint: TransactionOutpoint) -> Self {
        let mut bytes = [0u8; STAKE_BOND_KEY_SIZE];
        bytes[..HASH64_SIZE].copy_from_slice(&outpoint.transaction_id.as_bytes());
        bytes[HASH64_SIZE..].copy_from_slice(&outpoint.index.to_le_bytes());
        Self(bytes)
    }
}

impl From<StakeBondKey> for TransactionOutpoint {
    fn from(k: StakeBondKey) -> Self {
        let transaction_id = Hash64::from_slice(&k.0[..HASH64_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; size_of::<TransactionIndexType>()]>::try_from(&k.0[HASH64_SIZE..]).expect("index size is exact"),
        );
        Self::new(transaction_id, index)
    }
}

impl std::fmt::Display for StakeBondKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let outpoint: TransactionOutpoint = (*self).into();
        outpoint.fmt(f)
    }
}

pub trait StakeBondsStoreReader {
    fn get(&self, outpoint: &TransactionOutpoint) -> Result<Arc<StakeBondRecord>, StoreError>;
    fn has(&self, outpoint: &TransactionOutpoint) -> Result<bool, StoreError>;
    /// Iterates every persisted bond record. Used by the StakeScore
    /// aggregation to enumerate the active validator set.
    fn iterator(&self) -> Box<dyn Iterator<Item = Result<(TransactionOutpoint, Arc<StakeBondRecord>), Box<dyn Error>>> + '_>;
}

pub trait StakeBondsStore: StakeBondsStoreReader {
    /// Inserts or overwrites the record for `outpoint` (a bond status
    /// transition rewrites the same key).
    fn insert(&mut self, outpoint: TransactionOutpoint, record: Arc<StakeBondRecord>) -> Result<(), StoreError>;
    fn delete(&mut self, outpoint: TransactionOutpoint) -> Result<(), StoreError>;
}

/// A DB + cache implementation of the `StakeBondsStore` trait.
#[derive(Clone)]
pub struct DbStakeBondsStore {
    db: Arc<DB>,
    access: CachedDbAccess<StakeBondKey, Arc<StakeBondRecord>>,
}

impl DbStakeBondsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::StakeBonds.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(
        &mut self,
        batch: &mut WriteBatch,
        outpoint: TransactionOutpoint,
        record: Arc<StakeBondRecord>,
    ) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), outpoint.into(), record)
    }

    pub fn delete_batch(&mut self, batch: &mut WriteBatch, outpoint: TransactionOutpoint) -> StoreResult<()> {
        self.access.delete(BatchDbWriter::new(batch), outpoint.into())
    }
}

impl StakeBondsStoreReader for DbStakeBondsStore {
    fn get(&self, outpoint: &TransactionOutpoint) -> Result<Arc<StakeBondRecord>, StoreError> {
        self.access.read((*outpoint).into())
    }

    fn has(&self, outpoint: &TransactionOutpoint) -> Result<bool, StoreError> {
        self.access.has((*outpoint).into())
    }

    fn iterator(&self) -> Box<dyn Iterator<Item = Result<(TransactionOutpoint, Arc<StakeBondRecord>), Box<dyn Error>>> + '_> {
        Box::new(self.access.iterator().map(|res| match res {
            Ok((key_bytes, record)) => {
                let key = StakeBondKey::try_from(key_bytes.as_ref())?;
                Ok((key.into(), record))
            }
            Err(e) => Err(e),
        }))
    }
}

impl StakeBondsStore for DbStakeBondsStore {
    fn insert(&mut self, outpoint: TransactionOutpoint, record: Arc<StakeBondRecord>) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), outpoint.into(), record)
    }

    fn delete(&mut self, outpoint: TransactionOutpoint) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), outpoint.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::dns_finality::BondStatus;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn outpoint(b: u8, idx: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([b; 64]), idx)
    }

    fn record(op: TransactionOutpoint, amount: u64, status: BondStatus) -> Arc<StakeBondRecord> {
        Arc::new(StakeBondRecord {
            version: 1,
            bond_outpoint: op,
            owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
            validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
            validator_pubkey: vec![0xcc; 2592],
            amount,
            activation_daa_score: 100,
            created_daa_score: 100,
            unbonding_period_blocks: 1000,
            owner_reward_spk_payload: [0xdd; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status,
        })
    }

    #[test]
    fn stake_bonds_store_crud_iterator_and_key_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbStakeBondsStore::new(db.clone(), CachePolicy::Count(16));

        // A high outpoint index exercises the 4-byte LE index half of the key.
        let op1 = outpoint(0x01, 0);
        let op2 = outpoint(0x02, 4_000_000_007);

        assert!(!store.has(&op1).unwrap());
        assert!(store.get(&op1).is_err());

        store.insert(op1, record(op1, 1_000, BondStatus::Active)).unwrap();
        store.insert(op2, record(op2, 2_000, BondStatus::Pending)).unwrap();
        assert!(store.has(&op1).unwrap());
        assert_eq!(store.get(&op1).unwrap().amount, 1_000);
        assert_eq!(store.get(&op2).unwrap().status, BondStatus::Pending);

        // Overwrite is a status transition on the same key.
        store.insert(op1, record(op1, 1_000, BondStatus::Slashed)).unwrap();
        assert_eq!(store.get(&op1).unwrap().status, BondStatus::Slashed);

        // Iterator yields both, round-tripping each outpoint through the key codec.
        let seen: Vec<TransactionOutpoint> = store.iterator().map(|r| r.unwrap().0).collect();
        assert_eq!(seen.len(), 2);
        assert!(seen.contains(&op1) && seen.contains(&op2));

        // Batch delete removes only the targeted key.
        let mut batch = WriteBatch::default();
        store.delete_batch(&mut batch, op1).unwrap();
        db.write(batch).unwrap();
        assert!(!store.has(&op1).unwrap());
        assert!(store.has(&op2).unwrap());
    }
}
