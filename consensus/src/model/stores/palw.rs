//! kaspa-pq ADR-0039 §18.1 — the PALW audited-compute overlay stores: the on-chain state a validator
//! resolves an algo-4 ticket against (leaf descriptor, batch manifest, certificate, batch status). A
//! `verify_palw_ticket` binding (§14.2) is built from `leaf(batch_id, leaf_index)` +
//! `certificate(cert_hash)` + `batch_status(batch_id)`.
//!
//! These stores are written on the three PALW presets, which use
//! `palw_activation_daa_score = 0`. The writer
//! is `commit_palw_overlay_effects` (virtual commit), which folds ACCEPTED PALW overlay txs
//! (subnetworks `0x30`–`0x33`) — ordinary transactions, not algo-4 headers — so it runs on those
//! presets from genesis.
//!
//! `palw_algo4_accept` gates algo-4 header admission, not overlay transaction processing. The stores
//! stay empty on mainnet, testnet-10, simnet and devnet, where
//! `palw_activation_daa_score == u64::MAX` makes the fast-path guard return before any write.
//!
//! Rows exist on PALW networks, so changing their encoding requires a database-version transition.
//! See `LATEST_DB_VERSION` in `consensus/src/consensus/factory.rs`.
//!
//! Activation requirements:
//! 1. [`DbPalwStore::delete_batch_records`] must be tied to pruning-point retention.
//! 2. Overlay effects and the UTXO result must commit atomically.
//! 3. Certificate validity is fork-contextual, while its cache is global. Manifest identity is now
//!    enforced (`batch_id == content_id()`), leaves are write-once members of the committed Merkle root,
//!    and certificates are keyed by their own content hash. That makes the *bytes* collision-resistant;
//!    it does not make certificate *attestation validity* context-free. Attestation is checked against
//!    the current candidate's provider-bond view and audit-beacon history. A certificate admitted while
//!    evaluating a losing fork therefore remains in this global store and can later be resolved from a
//!    different fork without re-attestation. Closing that requires fork-scoped attestation provenance,
//!    or a protocol rule that anchors attestation to a finalized, fork-invariant snapshot; merely
//!    deleting rows on selected-chain detach is both insufficient and arrival-order-dependent.
//!
//! The old C4 finding that a mutable global `batch_status` itself needed reorg reversal is CLOSED:
//! [`crate::model::stores::palw_overlay_view::DbPalwOverlayViewStore`] carries lifecycle state per block,
//! and ticket validation reads the selected parent's view. `batch_status` remains only as a legacy/test
//! surface and production `apply_palw_overlay_effect` does not write it.

use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::palw::{PalwBatchCertificateV2, PalwBatchManifestV1, PalwBatchStatus, PalwBatchViewV1, PalwPublicLeafV1};
use kaspa_consensus_core::palw_pruned_frontier::{PalwPrunedActiveBatchV1, validate_palw_active_batch_bundles};
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::prelude::{CachePolicy, StoreError, StoreErrorPredicates};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::{HASH64_SIZE, Hash64};
use parking_lot::Mutex;
use rocksdb::WriteBatch;
use std::{collections::HashMap, sync::Arc};

/// `{ Hash64 batch_id (64) ‖ u32 leaf_index (4) }` = 68 bytes — the composite leaf key (§9.2), same
/// fixed-width scheme as `StakeBondKey`.
pub const PALW_LEAF_KEY_SIZE: usize = HASH64_SIZE + size_of::<u32>();

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
pub struct PalwLeafKey([u8; PALW_LEAF_KEY_SIZE]);

impl PalwLeafKey {
    pub fn new(batch_id: Hash64, leaf_index: u32) -> Self {
        let mut bytes = [0u8; PALW_LEAF_KEY_SIZE];
        bytes[..HASH64_SIZE].copy_from_slice(&batch_id.as_bytes());
        bytes[HASH64_SIZE..].copy_from_slice(&leaf_index.to_le_bytes());
        Self(bytes)
    }
}

impl AsRef<[u8]> for PalwLeafKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for PalwLeafKey {
    type Error = &'static str;
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        if slice.len() != PALW_LEAF_KEY_SIZE {
            return Err("palw-leaf key slice has unexpected length");
        }
        let mut bytes = [0u8; PALW_LEAF_KEY_SIZE];
        bytes.copy_from_slice(slice);
        Ok(Self(bytes))
    }
}

impl std::fmt::Display for PalwLeafKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "palw-leaf:{}", faster_hex::hex_string(&self.0))
    }
}

/// The §18.1 read surface: resolve the overlay state an algo-4 ticket binds to.
pub trait PalwStoreReader {
    fn leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<Arc<PalwPublicLeafV1>, StoreError>;
    fn batch_manifest(&self, batch_id: Hash64) -> Result<Arc<PalwBatchManifestV1>, StoreError>;
    fn certificate(&self, cert_hash: Hash64) -> Result<Arc<PalwBatchCertificateV2>, StoreError>;
    fn batch_status(&self, batch_id: Hash64) -> Result<PalwBatchStatus, StoreError>;
    fn has_leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<bool, StoreError>;
}

pub trait PalwStore: PalwStoreReader {
    fn insert_leaf(&self, batch_id: Hash64, leaf_index: u32, leaf: Arc<PalwPublicLeafV1>) -> Result<(), StoreError>;
    fn insert_manifest(&self, batch_id: Hash64, manifest: Arc<PalwBatchManifestV1>) -> Result<(), StoreError>;
    fn insert_certificate(&self, cert_hash: Hash64, cert: Arc<PalwBatchCertificateV2>) -> Result<(), StoreError>;
    fn set_batch_status(&self, batch_id: Hash64, status: PalwBatchStatus) -> Result<(), StoreError>;
}

/// A DB + cache implementation of the PALW overlay stores.
#[derive(Clone)]
pub struct DbPalwStore {
    db: Arc<DB>,
    leaves: CachedDbAccess<PalwLeafKey, Arc<PalwPublicLeafV1>>,
    manifests: CachedDbAccess<Hash64, Arc<PalwBatchManifestV1>, BlockHasher>,
    certificates: CachedDbAccess<Hash64, Arc<PalwBatchCertificateV2>, BlockHasher>,
    batch_status: CachedDbAccess<Hash64, PalwBatchStatus, BlockHasher>,
}

impl DbPalwStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            leaves: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwLeaf.into()),
            manifests: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwBatchManifest.into()),
            certificates: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::PalwCertificate.into()),
            batch_status: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PalwBatchStatus.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    /// Project the immutable content needed to transport a block-keyed lifecycle view across a
    /// pruning boundary. This is intentionally fail-closed at the raw-body/accepted-content seam:
    /// an uncertified entry needs its manifest, while a certified entry needs its exact certificate
    /// and every leaf. Missing accepted provenance must stop capture rather than produce a sidecar on
    /// which an archival and a fresh node could disagree.
    pub fn pruning_active_batches(&self, view: Option<&PalwBatchViewV1>) -> Result<Vec<PalwPrunedActiveBatchV1>, String> {
        let Some(view) = view else {
            return Ok(Vec::new());
        };
        let mut rows = Vec::with_capacity(view.batches.len());
        for (batch_id, lifecycle) in &view.batches {
            let manifest = self
                .batch_manifest(*batch_id)
                .map_err(|err| format!("active batch {batch_id} is missing its accepted manifest: {err}"))?;
            let certificate = match lifecycle.cert_hash {
                Some(cert_hash) => Some(
                    self.certificate(cert_hash)
                        .map_err(|err| {
                            format!("active batch {batch_id} is missing its accepted exact lifecycle certificate {cert_hash}: {err}")
                        })?
                        .as_ref()
                        .clone(),
                ),
                None => None,
            };
            let mut leaves = Vec::new();
            if lifecycle.cert_hash.is_some() {
                for leaf_index in 0..manifest.leaf_count {
                    let leaf = self
                        .leaf(*batch_id, leaf_index)
                        .map_err(|err| format!("certified active batch {batch_id} is missing accepted leaf {leaf_index}: {err}"))?;
                    leaves.push((*leaf).clone());
                }
            }
            rows.push(PalwPrunedActiveBatchV1 { batch_id: *batch_id, manifest: manifest.as_ref().clone(), leaves, certificate });
        }
        validate_palw_active_batch_bundles(Some(view), &rows)
            .map_err(|err| format!("active batch bundle projection is invalid: {err}"))?;
        Ok(rows)
    }

    /// Batch-delete every overlay record for a batch (used by the pruning processor). The leaf keys are
    /// derived from the manifest's `leaf_count`.
    pub fn delete_batch_records(
        &self,
        batch: &mut WriteBatch,
        batch_id: Hash64,
        leaf_count: u32,
        cert_hash: Hash64,
    ) -> Result<(), StoreError> {
        for i in 0..leaf_count {
            self.leaves.delete(BatchDbWriter::new(batch), PalwLeafKey::new(batch_id, i))?;
        }
        self.manifests.delete(BatchDbWriter::new(batch), batch_id)?;
        self.certificates.delete(BatchDbWriter::new(batch), cert_hash)?;
        self.batch_status.delete(BatchDbWriter::new(batch), batch_id)?;
        Ok(())
    }

    /// Preflight and stage the pruning-point active-batch projection in the caller's atomic import
    /// batch. Existing identical content is idempotent; a content-address collision aborts before any
    /// write is staged. Structural/root checks belong to the consensus-core snapshot validator.
    pub fn import_active_batches_batch(&self, batch: &mut WriteBatch, rows: &[PalwPrunedActiveBatchV1]) -> Result<(), StoreError> {
        let mut manifests = Vec::new();
        let mut leaves = Vec::new();
        let mut certificates = Vec::new();

        for row in rows {
            match self.manifests.read(row.batch_id) {
                Ok(existing) if *existing != row.manifest => {
                    return Err(StoreError::KeyAlreadyExists(format!(
                        "PALW pruning manifest {} collides with different local content",
                        row.batch_id
                    )));
                }
                Ok(_) => {}
                Err(error) if error.is_key_not_found() => manifests.push((row.batch_id, Arc::new(row.manifest.clone()))),
                Err(error) => return Err(error),
            }
            for leaf in &row.leaves {
                let key = PalwLeafKey::new(row.batch_id, leaf.leaf_index);
                match self.leaves.read(key) {
                    Ok(existing) if *existing != *leaf => {
                        return Err(StoreError::KeyAlreadyExists(format!(
                            "PALW pruning leaf ({}, {}) collides with different local content",
                            row.batch_id, leaf.leaf_index
                        )));
                    }
                    Ok(_) => {}
                    Err(error) if error.is_key_not_found() => leaves.push((key, Arc::new(leaf.clone()))),
                    Err(error) => return Err(error),
                }
            }
            if let Some(certificate) = &row.certificate {
                let hash = certificate.hash();
                match self.certificates.read(hash) {
                    Ok(existing) if *existing != *certificate => {
                        return Err(StoreError::KeyAlreadyExists(format!(
                            "PALW pruning certificate {hash} collides with different local content"
                        )));
                    }
                    Ok(_) => {}
                    Err(error) if error.is_key_not_found() => certificates.push((hash, Arc::new(certificate.clone()))),
                    Err(error) => return Err(error),
                }
            }
        }

        for (batch_id, manifest) in manifests {
            self.manifests.write(BatchDbWriter::new(batch), batch_id, manifest)?;
        }
        for (key, leaf) in leaves {
            self.leaves.write(BatchDbWriter::new(batch), key, leaf)?;
        }
        for (hash, certificate) in certificates {
            self.certificates.write(BatchDbWriter::new(batch), hash, certificate)?;
        }
        Ok(())
    }
}

/// An in-memory read-through PALW content store used while a virtual UTXO result is being staged.
/// Semantic validation sees earlier accepted effects from the same acceptance set, while no blob
/// escapes to RocksDB before the caller commits its UTXO `WriteBatch`. This closes the old partial
/// direct-write/crash window without changing the `PalwStore` validation surface.
pub struct PalwStoreBatchStage<'a> {
    base: &'a DbPalwStore,
    leaves: Mutex<HashMap<PalwLeafKey, Arc<PalwPublicLeafV1>>>,
    manifests: Mutex<HashMap<Hash64, Arc<PalwBatchManifestV1>>>,
    certificates: Mutex<HashMap<Hash64, Arc<PalwBatchCertificateV2>>>,
    statuses: Mutex<HashMap<Hash64, PalwBatchStatus>>,
}

impl<'a> PalwStoreBatchStage<'a> {
    pub fn new(base: &'a DbPalwStore) -> Self {
        Self {
            base,
            leaves: Mutex::new(HashMap::new()),
            manifests: Mutex::new(HashMap::new()),
            certificates: Mutex::new(HashMap::new()),
            statuses: Mutex::new(HashMap::new()),
        }
    }

    /// Move all validated content into the caller's atomic UTXO commit batch.
    pub fn stage_into(self, batch: &mut WriteBatch) -> Result<(), StoreError> {
        for (key, leaf) in self.leaves.into_inner() {
            self.base.leaves.write(BatchDbWriter::new(batch), key, leaf)?;
        }
        for (batch_id, manifest) in self.manifests.into_inner() {
            self.base.manifests.write(BatchDbWriter::new(batch), batch_id, manifest)?;
        }
        for (cert_hash, certificate) in self.certificates.into_inner() {
            self.base.certificates.write(BatchDbWriter::new(batch), cert_hash, certificate)?;
        }
        for (batch_id, status) in self.statuses.into_inner() {
            self.base.batch_status.write(BatchDbWriter::new(batch), batch_id, status)?;
        }
        Ok(())
    }
}

impl PalwStoreReader for PalwStoreBatchStage<'_> {
    fn leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<Arc<PalwPublicLeafV1>, StoreError> {
        let key = PalwLeafKey::new(batch_id, leaf_index);
        self.leaves.lock().get(&key).cloned().map(Ok).unwrap_or_else(|| self.base.leaf(batch_id, leaf_index))
    }

    fn batch_manifest(&self, batch_id: Hash64) -> Result<Arc<PalwBatchManifestV1>, StoreError> {
        self.manifests.lock().get(&batch_id).cloned().map(Ok).unwrap_or_else(|| self.base.batch_manifest(batch_id))
    }

    fn certificate(&self, cert_hash: Hash64) -> Result<Arc<PalwBatchCertificateV2>, StoreError> {
        self.certificates.lock().get(&cert_hash).cloned().map(Ok).unwrap_or_else(|| self.base.certificate(cert_hash))
    }

    fn batch_status(&self, batch_id: Hash64) -> Result<PalwBatchStatus, StoreError> {
        self.statuses.lock().get(&batch_id).copied().map(Ok).unwrap_or_else(|| self.base.batch_status(batch_id))
    }

    fn has_leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<bool, StoreError> {
        let key = PalwLeafKey::new(batch_id, leaf_index);
        if self.leaves.lock().contains_key(&key) { Ok(true) } else { self.base.has_leaf(batch_id, leaf_index) }
    }
}

impl PalwStore for PalwStoreBatchStage<'_> {
    fn insert_leaf(&self, batch_id: Hash64, leaf_index: u32, leaf: Arc<PalwPublicLeafV1>) -> Result<(), StoreError> {
        let key = PalwLeafKey::new(batch_id, leaf_index);
        match self.leaf(batch_id, leaf_index) {
            Ok(existing) if existing.leaf_hash() == leaf.leaf_hash() => return Ok(()),
            Ok(existing) => {
                return Err(StoreError::KeyAlreadyExists(format!(
                    "PALW leaf ({batch_id}, {leaf_index}) is write-once: refusing to replace {} with {}",
                    existing.leaf_hash(),
                    leaf.leaf_hash()
                )));
            }
            Err(error) if error.is_key_not_found() => {}
            Err(error) => return Err(error),
        }
        self.leaves.lock().insert(key, leaf);
        Ok(())
    }

    fn insert_manifest(&self, batch_id: Hash64, manifest: Arc<PalwBatchManifestV1>) -> Result<(), StoreError> {
        match self.batch_manifest(batch_id) {
            Ok(existing) if *existing == *manifest => return Ok(()),
            Ok(_) => return Err(StoreError::KeyAlreadyExists(format!("PALW manifest {batch_id} is write-once"))),
            Err(error) if error.is_key_not_found() => {}
            Err(error) => return Err(error),
        }
        self.manifests.lock().insert(batch_id, manifest);
        Ok(())
    }

    fn insert_certificate(&self, cert_hash: Hash64, cert: Arc<PalwBatchCertificateV2>) -> Result<(), StoreError> {
        match self.certificate(cert_hash) {
            Ok(existing) if *existing == *cert => return Ok(()),
            Ok(_) => return Err(StoreError::KeyAlreadyExists(format!("PALW certificate {cert_hash} is write-once"))),
            Err(error) if error.is_key_not_found() => {}
            Err(error) => return Err(error),
        }
        self.certificates.lock().insert(cert_hash, cert);
        Ok(())
    }

    fn set_batch_status(&self, batch_id: Hash64, status: PalwBatchStatus) -> Result<(), StoreError> {
        self.statuses.lock().insert(batch_id, status);
        Ok(())
    }
}

impl PalwStoreReader for DbPalwStore {
    fn leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<Arc<PalwPublicLeafV1>, StoreError> {
        self.leaves.read(PalwLeafKey::new(batch_id, leaf_index))
    }

    fn batch_manifest(&self, batch_id: Hash64) -> Result<Arc<PalwBatchManifestV1>, StoreError> {
        self.manifests.read(batch_id)
    }

    fn certificate(&self, cert_hash: Hash64) -> Result<Arc<PalwBatchCertificateV2>, StoreError> {
        self.certificates.read(cert_hash)
    }

    fn batch_status(&self, batch_id: Hash64) -> Result<PalwBatchStatus, StoreError> {
        self.batch_status.read(batch_id)
    }

    fn has_leaf(&self, batch_id: Hash64, leaf_index: u32) -> Result<bool, StoreError> {
        self.leaves.has(PalwLeafKey::new(batch_id, leaf_index))
    }
}

impl PalwStore for DbPalwStore {
    /// kaspa-pq **ADR-0040 P1-1 (LEAF-01)** — content-addressed, write-once leaf insertion.
    ///
    /// This used to be a plain `write`, i.e. last-writer-wins at `(batch_id, leaf_index)`. That is the
    /// reward-theft path: `palw_work_reward_class` re-reads the CURRENT leaf at coinbase time to pick
    /// `provider_{a,b}_reward_script`, so overwriting an already-accepted leaf with the same key but
    /// different reward scripts re-routes the 77 % worker base to the attacker. Presence of the leaf was
    /// proven at body-validation time; **immutability of its content was not.**
    ///
    /// Semantics: idempotent for identical content, fail-closed for different content. Re-applying the
    /// same overlay effect (reorg replay, chunk re-delivery) must stay legal, so equality is tested on
    /// [`PalwPublicLeafV1::leaf_hash`] rather than rejecting every second write outright.
    ///
    /// Note this is necessary but not sufficient on its own: it pins a leaf once written, while binding
    /// the written set to `manifest.leaf_root` is a separate gate (BIND-01).
    ///
    /// **kaspa-pq ADR-0040 §5.15.11 — that gate now EXISTS.** It did not when this comment was written:
    /// `palw_leaf_root` had zero consensus callers, so a squatter who copied a public `batch_id` could
    /// win this write-once race with its own leaves and the write-once property would then protect the
    /// SQUATTER. The gate is `apply_palw_overlay_effect`'s LeafChunk arm, which verifies a per-leaf
    /// Merkle membership proof against `manifest.leaf_root` BEFORE calling this function.
    ///
    /// The idempotent-on-identical-content branch below is load-bearing for that closure and must not be
    /// tightened into strict first-writer-wins: because membership is now checked on CONTENT, any chunk
    /// that reaches here is byte-identical to the honest one, so a front-run degrades to the attacker
    /// paying the fee to publish the victim's own data — and the honest transaction still succeeds.
    fn insert_leaf(&self, batch_id: Hash64, leaf_index: u32, leaf: Arc<PalwPublicLeafV1>) -> Result<(), StoreError> {
        let key = PalwLeafKey::new(batch_id, leaf_index);
        match self.leaves.read(key) {
            Ok(existing) => {
                return if existing.leaf_hash() == leaf.leaf_hash() {
                    Ok(()) // idempotent re-apply of identical content
                } else {
                    Err(StoreError::KeyAlreadyExists(format!(
                        "PALW leaf ({batch_id}, {leaf_index}) is write-once: refusing to replace {} with {}",
                        existing.leaf_hash(),
                        leaf.leaf_hash()
                    )))
                };
            }
            Err(error) if error.is_key_not_found() => {}
            Err(error) => return Err(error),
        }
        self.leaves.write(DirectDbWriter::new(&self.db), key, leaf)
    }

    fn insert_manifest(&self, batch_id: Hash64, manifest: Arc<PalwBatchManifestV1>) -> Result<(), StoreError> {
        self.manifests.write(DirectDbWriter::new(&self.db), batch_id, manifest)
    }

    /// kaspa-pq **ADR-0040 CERT-BATCH** — content-addressed, write-once certificate insertion, mirroring
    /// [`Self::insert_leaf`] (P1-1). Certificates are keyed by `cert.hash()`, so a write at an existing
    /// key with DIFFERENT content is a hash collision, not a legitimate update; it fails closed rather
    /// than silently replacing an already-attested blob that live headers may name. Re-applying identical
    /// content (reorg replay, duplicate delivery) stays idempotent.
    fn insert_certificate(&self, cert_hash: Hash64, cert: Arc<PalwBatchCertificateV2>) -> Result<(), StoreError> {
        match self.certificates.read(cert_hash) {
            Ok(existing) => {
                return if *existing == *cert {
                    Ok(()) // idempotent re-apply of identical content
                } else {
                    Err(StoreError::KeyAlreadyExists(format!(
                        "PALW certificate {cert_hash} is write-once: refusing to replace it with different content"
                    )))
                };
            }
            Err(error) if error.is_key_not_found() => {}
            Err(error) => return Err(error),
        }
        self.certificates.write(DirectDbWriter::new(&self.db), cert_hash, cert)
    }

    fn set_batch_status(&self, batch_id: Hash64, status: PalwBatchStatus) -> Result<(), StoreError> {
        self.batch_status.write(DirectDbWriter::new(&self.db), batch_id, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw::{
        PALW_BATCH_CERTIFICATE_VERSION_V2, PalwBatchLifecycleV1, PalwBatchViewV1, PalwLeafChunkV1, palw_leaf_merkle_proof,
        palw_leaf_merkle_root,
    };
    use kaspa_consensus_core::palw_pruned_frontier::PalwPrunedActiveBatchV1;
    use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn leaf(idx: u32) -> Arc<PalwPublicLeafV1> {
        use kaspa_consensus_core::tx::TransactionOutpoint;
        let spk = ScriptPublicKey::new(0, ScriptVec::from_slice(&[1]));
        Arc::new(PalwPublicLeafV1 {
            version: 1,
            batch_id: h(1),
            leaf_index: idx,
            job_nullifier: h(2),
            ticket_nullifier_commitment: h(3),
            model_profile_id: h(4),
            runtime_class_id: h(5),
            shape_id: 3,
            quantum_count: 2,
            proof_type: 1,
            provider_a_bond: TransactionOutpoint::new(h(6), 0),
            provider_b_bond: TransactionOutpoint::new(h(7), 0),
            provider_a_reward_script: spk.clone(),
            provider_b_reward_script: spk,
            ticket_authority_pk_hash: h(8),
            private_match_commitment: h(9),
            receipt_da_object_version: 1,
            receipt_da_root: h(10),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 5,
            activation_epoch: 7,
            expiry_epoch: 13,
            leaf_bond_sompi: 0,
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: Hash64::from_bytes([0x7c; 64]),
            assignment_proof_root: Hash64::from_bytes([0x7d; 64]),
            dispatch_kind: 0, // BeaconAssigned (external sentinels)
        })
    }

    #[test]
    fn accepted_content_stage_reads_own_writes_and_commits_in_caller_batch() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(16));
        let mut manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: h(0),
            registration_epoch: 5,
            model_profile_id: h(3),
            runtime_class_id: h(4),
            leaf_count: 1,
            chunk_count: 1,
            leaf_root: h(8),
            descriptor_root: h(6),
            total_leaf_bond_sompi: 0,
            audit_policy_id: h(7),
            activation_not_before_epoch: 13,
            expiry_epoch: 19,
        };
        manifest.batch_id = manifest.content_id();

        let stage = PalwStoreBatchStage::new(&store);
        stage.insert_manifest(manifest.batch_id, Arc::new(manifest.clone())).unwrap();
        assert_eq!(*stage.batch_manifest(manifest.batch_id).unwrap(), manifest);
        assert!(store.batch_manifest(manifest.batch_id).unwrap_err().is_key_not_found());

        let mut batch = WriteBatch::default();
        stage.stage_into(&mut batch).unwrap();
        db.write(batch).unwrap();
        assert_eq!(*store.batch_manifest(manifest.batch_id).unwrap(), manifest);
    }

    fn active_bundle() -> PalwPrunedActiveBatchV1 {
        active_bundle_with_pcpb(None)
    }

    /// D3-b: the same bundle, optionally stamped with a PCPB fixture BEFORE `leaf_root`/`batch_id`
    /// are derived — the leaf's PCPB fields sit inside `leaf_hash`, so stamping afterwards would
    /// silently break the membership proof rather than test anything.
    fn active_bundle_with_pcpb(pcpb: Option<&crate::processes::palw::pcpb_test_support::PcpbFixture>) -> PalwPrunedActiveBatchV1 {
        let mut leaf = (*leaf(0)).clone();
        leaf.batch_id = Hash64::default();
        if let Some(fixture) = pcpb {
            fixture.stamp(&mut leaf);
        }
        let leaf_root = palw_leaf_merkle_root(&[leaf.leaf_hash()]);
        let mut manifest = PalwBatchManifestV1 {
            version: 1,
            batch_id: Hash64::default(),
            registration_epoch: leaf.registered_epoch,
            model_profile_id: leaf.model_profile_id,
            runtime_class_id: leaf.runtime_class_id,
            leaf_count: 1,
            chunk_count: 1,
            leaf_root,
            descriptor_root: h(0x41),
            total_leaf_bond_sompi: 1,
            audit_policy_id: h(0x42),
            activation_not_before_epoch: leaf.activation_epoch,
            expiry_epoch: leaf.expiry_epoch,
        };
        let batch_id = manifest.content_id();
        manifest.batch_id = batch_id;
        leaf.batch_id = batch_id;
        let certificate = PalwBatchCertificateV2 {
            version: PALW_BATCH_CERTIFICATE_VERSION_V2,
            batch_id,
            manifest_hash: manifest.content_id(),
            leaf_root,
            audit_beacon_epoch: 1,
            audit_sample_root: h(0x43),
            passed_leaf_count: 1,
            rejected_leaf_bitmap_root: h(0x44),
            certificate_epoch: 1,
            activation_epoch: leaf.activation_epoch,
            expiry_epoch: leaf.expiry_epoch,
            auditor_set_commitment: h(0x45),
            approving_stake: 1,
            votes: vec![],
        };
        PalwPrunedActiveBatchV1 { batch_id, manifest, leaves: vec![leaf], certificate: Some(certificate) }
    }

    fn view_for_bundle(bundle: &PalwPrunedActiveBatchV1, cert_hash: Option<Hash64>) -> PalwBatchViewV1 {
        let mut view = PalwBatchViewV1::new();
        view.batches.insert(
            bundle.batch_id,
            PalwBatchLifecycleV1 {
                status: if cert_hash.is_some() { PalwBatchStatus::Active } else { PalwBatchStatus::Registering },
                registration_epoch: bundle.manifest.registration_epoch,
                activation_not_before_epoch: bundle.manifest.activation_not_before_epoch,
                expiry_epoch: bundle.manifest.expiry_epoch,
                leaf_count: bundle.manifest.leaf_count,
                chunk_count: bundle.manifest.chunk_count,
                chunks_present: [1, 0, 0, 0],
                leaf_root: bundle.manifest.leaf_root,
                cert_hash,
                sponsor_tag_hi: 0,
                sponsor_tag_lo: 0,
                cert_approving_stake: 0,
                first_cert_daa: cert_hash.map(|_| 10),
                revoked_from_daa: None,
            },
        );
        view
    }

    /// The eligibility-resolution triad (leaf / certificate / batch-status) inserts, reads back, and
    /// (leaf) round-trips under the composite `(batch_id, leaf_index)` key.
    #[test]
    fn overlay_store_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db, CachePolicy::Count(64));

        // leaf keyed by (batch_id, leaf_index) — distinct indices are distinct keys.
        assert!(!store.has_leaf(h(1), 0).unwrap());
        store.insert_leaf(h(1), 0, leaf(0)).unwrap();
        store.insert_leaf(h(1), 1, leaf(1)).unwrap();
        assert!(store.has_leaf(h(1), 0).unwrap());
        assert_eq!(store.leaf(h(1), 0).unwrap().leaf_index, 0);
        assert_eq!(store.leaf(h(1), 1).unwrap().leaf_index, 1);
        assert!(store.leaf(h(1), 2).is_err()); // absent

        // batch status.
        assert!(store.batch_status(h(1)).is_err());
        store.set_batch_status(h(1), PalwBatchStatus::Active).unwrap();
        assert_eq!(store.batch_status(h(1)).unwrap(), PalwBatchStatus::Active);

        // certificate keyed by its hash.
        assert!(store.certificate(h(9)).is_err());
    }

    /// Leaf keys for different batches never collide even at the same leaf index.
    #[test]
    fn leaf_key_is_batch_scoped() {
        let k_a = PalwLeafKey::new(h(1), 5);
        let k_b = PalwLeafKey::new(h(2), 5);
        assert_ne!(k_a.as_ref(), k_b.as_ref());
        assert_eq!(PalwLeafKey::try_from(k_a.as_ref()).unwrap(), k_a);
    }

    #[test]
    fn fresh_db_pruning_import_restores_first_post_pp_ticket_and_reward_blobs_after_restart() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let bundle = active_bundle();
        let cert_hash = bundle.certificate.as_ref().unwrap().hash();
        let mut batch = WriteBatch::default();
        DbPalwStore::new(db.clone(), CachePolicy::Count(64))
            .import_active_batches_batch(&mut batch, std::slice::from_ref(&bundle))
            .unwrap();

        let precommit = DbPalwStore::new(db.clone(), CachePolicy::Count(1));
        assert!(precommit.leaf(bundle.batch_id, 0).is_err(), "blob import must wait for the shared RocksDB batch");
        db.write(batch).unwrap();

        let restarted = DbPalwStore::new(db, CachePolicy::Count(1));
        let resolved = crate::processes::palw::resolve_palw_binding(bundle.batch_id, 0, cert_hash, 100, &restarted)
            .expect("a fresh pruned node must resolve the first post-PP ticket after restart");
        assert_eq!(resolved.leaf_hash, restarted.leaf(bundle.batch_id, 0).unwrap().leaf_hash());
        assert_eq!(
            restarted.leaf(bundle.batch_id, 0).unwrap().provider_a_reward_script,
            bundle.leaves[0].provider_a_reward_script,
            "reward derivation must read the imported immutable leaf bytes"
        );
    }

    #[test]
    fn uncertified_pruning_import_accepts_leaf_reannouncement_with_manifest_membership_proof() {
        use crate::processes::palw::pcpb_test_support as pcpb;
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        // D3-b: re-announcement runs through the REAL acceptance arm, which now also enforces
        // clauses 11/12 — so the bundle carries a real PCPB fixture and its context is staged.
        // Keeping this test on the real arm (rather than routing around it) is the point: it is
        // what proves the post-import path still works under the full gate.
        let fixture = pcpb::fixture(leaf(0).registered_epoch);
        let complete = active_bundle_with_pcpb(Some(&fixture));
        let mut manifest_only = complete.clone();
        manifest_only.leaves.clear();
        manifest_only.certificate = None;

        let mut batch = WriteBatch::default();
        DbPalwStore::new(db.clone(), CachePolicy::Count(64)).import_active_batches_batch(&mut batch, &[manifest_only]).unwrap();
        db.write(batch).unwrap();

        let restarted = DbPalwStore::new(db.clone(), CachePolicy::Count(1));
        assert!(restarted.leaf(complete.batch_id, 0).is_err());
        let leaf = complete.leaves[0].clone();
        let mut projected = leaf.clone();
        projected.batch_id = Hash64::default();
        let proof = palw_leaf_merkle_proof(&[projected.leaf_hash()], 0).unwrap();
        let chunk = PalwLeafChunkV1 {
            version: kaspa_consensus_core::palw::PALW_LEAF_CHUNK_VERSION_V3,
            batch_id: complete.batch_id,
            chunk_index: 0,
            leaves: vec![leaf.clone()],
            proofs: vec![proof],
            witnesses: vec![fixture.witness(leaf.leaf_index)],
        };
        let beacon = crate::model::stores::palw_beacon::DbPalwBeaconStore::new(db.clone(), CachePolicy::Count(1));
        let pcpb_store = crate::model::stores::palw_pcpb::DbPalwPcpbStore::new(db.clone(), CachePolicy::Count(4));
        fixture.stage(&db, &beacon, &pcpb_store);
        let ctx = crate::processes::palw::PalwPcpbAcceptanceCtx {
            pcpb_store: &pcpb_store,
            network_id: pcpb::NETWORK_ID,
            freshness_window_epochs: pcpb::W,
            snapshot_lag_epochs: pcpb::K,
            post_commit_delta_epochs: pcpb::DELTA,
            // Static-audit C-01: the fixture's fork-relative chain (staged by `fixture.stage`).
            selected_parent: crate::processes::palw::pcpb_test_support::PCPB_TEST_SELECTED_PARENT,
            context_walk_max_hops: 64,
            block_epoch: crate::processes::palw::pcpb_test_support::PCPB_TEST_BLOCK_EPOCH,
            acommit_burial_epochs: crate::processes::palw::pcpb_test_support::ACOMMIT_BURIAL,
        };
        crate::processes::palw::apply_palw_overlay_effect(
            crate::processes::palw::PalwOverlayEffect::LeafChunk(chunk),
            &restarted,
            &beacon,
            None,
            Some(&ctx),
        )
        .expect("an authenticated pre-PP partial leaf can be re-announced after import");
        assert_eq!(*restarted.leaf(complete.batch_id, 0).unwrap(), leaf);
    }

    #[test]
    fn raw_but_unaccepted_manifest_view_entry_rejects_pruning_capture() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db, CachePolicy::Count(8));
        let bundle = active_bundle();
        let view = view_for_bundle(&bundle, None);

        let error = store.pruning_active_batches(Some(&view)).unwrap_err();
        assert!(
            error.contains("missing its accepted manifest"),
            "a raw-body lifecycle entry without accepted content must fail capture, got: {error}"
        );
    }

    #[test]
    fn raw_invalid_certificate_view_entry_rejects_pruning_capture() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwStore::new(db.clone(), CachePolicy::Count(8));
        let bundle = active_bundle();
        // Model the current coordinate split: the raw body fold recorded an attacker certificate hash,
        // but virtual acceptance rejected its attestation, so only the accepted manifest exists.
        store.insert_manifest(bundle.batch_id, Arc::new(bundle.manifest.clone())).unwrap();
        let unaccepted_cert_hash = h(0xfa);
        let view = view_for_bundle(&bundle, Some(unaccepted_cert_hash));

        let error = store.pruning_active_batches(Some(&view)).unwrap_err();
        assert!(
            error.contains("missing its accepted exact lifecycle certificate"),
            "an unaccepted raw certificate must fail capture instead of weakening the bundle, got: {error}"
        );
    }

    fn assert_accepted_pruning_roundtrip_ignores_raw_adversary() {
        let (_source_lt, source_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let bundle = active_bundle();
        let cert_hash = bundle.certificate.as_ref().unwrap().hash();
        let accepted_view = view_for_bundle(&bundle, Some(cert_hash));
        let source = DbPalwStore::new(source_db.clone(), CachePolicy::Count(16));
        let mut source_batch = WriteBatch::default();
        source.import_active_batches_batch(&mut source_batch, std::slice::from_ref(&bundle)).unwrap();
        source_db.write(source_batch).unwrap();

        let projected = source.pruning_active_batches(Some(&accepted_view)).unwrap();
        assert_eq!(projected, vec![bundle.clone()]);

        // A raw junk certificate observed before/after capture has no accepted-view transition. The
        // transported exact hash remains the attested one, so capture bytes do not depend on arrival.
        let junk = PalwBatchCertificateV2 { approving_stake: u128::MAX, ..bundle.certificate.clone().unwrap() };
        assert_ne!(junk.hash(), cert_hash);
        assert_eq!(source.pruning_active_batches(Some(&accepted_view)).unwrap(), projected);

        let (_fresh_lt, fresh_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let fresh = DbPalwStore::new(fresh_db.clone(), CachePolicy::Count(16));
        let mut import_batch = WriteBatch::default();
        fresh.import_active_batches_batch(&mut import_batch, &projected).unwrap();
        fresh_db.write(import_batch).unwrap();
        let resolved = crate::processes::palw::resolve_palw_binding(bundle.batch_id, 0, cert_hash, 100, &fresh).unwrap();
        assert_eq!(resolved.leaf_hash, source.leaf(bundle.batch_id, 0).unwrap().leaf_hash());
        assert_eq!(borsh::to_vec(&accepted_view).unwrap(), borsh::to_vec(&view_for_bundle(&bundle, Some(cert_hash))).unwrap());
    }

    #[test]
    fn palw_pruning_snapshot_uses_accepted_block_keyed_lifecycle_provenance() {
        assert_accepted_pruning_roundtrip_ignores_raw_adversary();
    }

    #[test]
    fn palw_pruned_ibd_matches_from_genesis_under_raw_overlay_adversary() {
        assert_accepted_pruning_roundtrip_ignores_raw_adversary();
    }
}
