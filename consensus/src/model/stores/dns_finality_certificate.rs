//! MISAKA VLT PR 4 (§7.2): the per-epoch **DNS finality certificate** store.
//!
//! One [`DnsFinalityCertificate`] per target epoch (key = `u64` epoch), written the first time
//! that epoch's precommit quorum counts on the selected chain and never rewritten. Unlike the
//! frozen voting snapshots this is NOT re-derivable state: the votes it certifies live in a
//! sliding window, and once the window moves past the epoch the certificate is the only thing
//! left that can prove — signatures and all — that the epoch finalized. It is the evidence a
//! §12 checkpoint package ships and a fresh IBD verifies, so there is deliberately no
//! schema-reindex here: discarding rows would discard proof, not cache.
//!
//! Empty below the VLT weight fence (the precommit round does not exist there), so this store
//! stays empty on every shipped preset.

use std::sync::Arc;

use kaspa_consensus_core::dns_finality::DnsFinalityCertificate;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;

use super::U64Key;

/// Per-epoch finality-certificate store, keyed by `u64` target epoch. Write-once per epoch.
#[derive(Clone)]
pub struct DbDnsFinalityCertificateStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, DnsFinalityCertificate>,
}

impl DbDnsFinalityCertificateStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::DnsFinalityCertificates.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// The certificate for `epoch`, or `KeyNotFound` if that epoch never certified.
    pub fn get(&self, epoch: u64) -> Result<DnsFinalityCertificate, StoreError> {
        self.access.read(epoch.into())
    }

    pub fn has(&self, epoch: u64) -> Result<bool, StoreError> {
        self.access.has(epoch.into())
    }

    /// Persist `epoch`'s certificate into `batch`. The caller enforces write-once (`has` first):
    /// a certificate is evidence, and evidence is not revised.
    pub fn set_batch(&self, batch: &mut WriteBatch, epoch: u64, cert: DnsFinalityCertificate) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), epoch.into(), cert)
    }

    /// Direct write — tests only.
    pub fn set(&self, epoch: u64, cert: DnsFinalityCertificate) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), epoch.into(), cert)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::dns_finality::{DnsFinalityCertificate, WeightedSignature};
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash64;

    /// A certificate must survive the store byte-exactly — the signatures inside it are the §12
    /// proof a checkpoint consumer verifies, and a lossy round-trip is a forged history.
    #[test]
    fn certificate_store_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbDnsFinalityCertificateStore::new(db.clone(), CachePolicy::Count(8));
        assert!(store.get(5).is_err());

        let cert = DnsFinalityCertificate {
            version: 1,
            epoch: 5,
            round: 0,
            source_anchor: Hash64::from_bytes([1; 64]),
            target_anchor: Hash64::from_bytes([2; 64]),
            target_anchor_daa: 480,
            snapshot_root: Hash64::from_bytes([3; 64]),
            validator_set_root: Hash64::from_bytes([4; 64]),
            total_weight: 1_000_000_000,
            quorum_weight: 666_666_667,
            signed_weight: 800_000_000,
            precommit_signatures: vec![WeightedSignature {
                validator_id: Hash64::from_bytes([5; 64]),
                bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([6; 64]), 0),
                signed_weight: 400_000_000,
                locked_epoch: 4,
                locked_hash: Hash64::from_bytes([7; 64]),
                snapshot_commitment: Hash64::from_bytes([8; 64]),
                signature: vec![0xab; 4627],
            }],
        };
        let mut batch = WriteBatch::default();
        store.set_batch(&mut batch, 5, cert.clone()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(store.clone_with_new_cache(CachePolicy::Count(8)).get(5).unwrap(), cert);
        assert!(store.has(5).unwrap() && !store.has(6).unwrap());
    }
}
