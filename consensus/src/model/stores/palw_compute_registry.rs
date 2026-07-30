//! # Compute Set registry stores (ADR-MA §21.1/§21.2)
//!
//! Two storage tiers, mirroring the batch-overlay design:
//!
//! * **Content-addressed record stores** — descriptor / policy / plan / activation-certificate
//!   records keyed by their DERIVED ids. Write-once by construction: the key is the keyed
//!   BLAKE2b-512 of the canonical bytes, so one key names one byte string forever; a divergent
//!   write under an existing key is a local-corruption fault, surfaced loudly (§7.2). These
//!   stores are fork-INDEPENDENT (a record is the same bytes on every fork that carries it) and
//!   are never rewritten by reorgs — only the VIEW moves.
//! * **Block-keyed fork-local view** — `view(B) = view(SP(B)) ⊕ Δ(mergeset(B))`, the
//!   [`PalwComputeRegistryViewV1`] a block's past determines. Same lifecycle as
//!   `DbPalwOverlayViewStore`: written atomically with the block commit, naturally reorg-
//!   reversible because a losing branch's rows are simply never read again (§21.2
//!   `fork-local application / reorg reversible`).
//!
//! Historical-resolution contract (§21.4): a source header names exact record ids; those ids
//! resolve HERE, never against current values — so nothing in these stores may be pruned while
//! any un-pruned block still references it (§21.3).

use std::sync::Arc;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHasher;
use kaspa_consensus_core::palw_compute_set::{
    ComputeSetRegistryError, PalwComputeRegistryViewV1, PalwComputeSetActivationCertificateV1, PalwComputeSetDescriptorV2,
    PalwComputeSetPolicyV1, PalwModelAllocationPlanV1,
};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResultExt;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter, StoreError};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use rocksdb::WriteBatch;

/// Reader surface for the record tiers (the §21.4 historical resolver runs on this).
pub trait PalwComputeRegistryStoreReader {
    fn descriptor(&self, compute_set_id: Hash64) -> Result<Option<Arc<PalwComputeSetDescriptorV2>>, StoreError>;
    fn policy(&self, policy_id: Hash64) -> Result<Option<Arc<PalwComputeSetPolicyV1>>, StoreError>;
    fn plan(&self, plan_id: Hash64) -> Result<Option<Arc<PalwModelAllocationPlanV1>>, StoreError>;
    fn certificate(&self, compute_set_id: Hash64) -> Result<Option<Arc<PalwComputeSetActivationCertificateV1>>, StoreError>;
    fn view(&self, block: BlockHash) -> Result<Option<Arc<PalwComputeRegistryViewV1>>, StoreError>;
}

#[derive(Clone)]
pub struct DbPalwComputeRegistryStore {
    db: Arc<DB>,
    descriptors: CachedDbAccess<Hash64, Arc<PalwComputeSetDescriptorV2>, BlockHasher>,
    policies: CachedDbAccess<Hash64, Arc<PalwComputeSetPolicyV1>, BlockHasher>,
    plans: CachedDbAccess<Hash64, Arc<PalwModelAllocationPlanV1>, BlockHasher>,
    certificates: CachedDbAccess<Hash64, Arc<PalwComputeSetActivationCertificateV1>, BlockHasher>,
    views: CachedDbAccess<BlockHash, Arc<PalwComputeRegistryViewV1>, BlockHasher>,
}

impl DbPalwComputeRegistryStore {
    pub fn new(db: Arc<DB>, record_cache: CachePolicy, view_cache: CachePolicy) -> Self {
        Self {
            descriptors: CachedDbAccess::new(Arc::clone(&db), record_cache, DatabaseStorePrefixes::PalwComputeSetDescriptor.into()),
            policies: CachedDbAccess::new(Arc::clone(&db), record_cache, DatabaseStorePrefixes::PalwComputeSetPolicy.into()),
            plans: CachedDbAccess::new(Arc::clone(&db), record_cache, DatabaseStorePrefixes::PalwAllocationPlan.into()),
            certificates: CachedDbAccess::new(
                Arc::clone(&db),
                record_cache,
                DatabaseStorePrefixes::PalwComputeSetCertificate.into(),
            ),
            views: CachedDbAccess::new(Arc::clone(&db), view_cache, DatabaseStorePrefixes::PalwComputeRegistryView.into()),
            db,
        }
    }

    pub fn clone_with_new_cache(&self, record_cache: CachePolicy, view_cache: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), record_cache, view_cache)
    }

    /// §7.2 write-once insert of a descriptor under its DERIVED id. Present + byte-identical is
    /// the idempotent no-op; present + divergent is a hard error (the id is a content hash, so
    /// divergence means local corruption or a hashing bug — never a legal state).
    pub fn insert_descriptor_batch(
        &self,
        batch: &mut WriteBatch,
        compute_set_id: Hash64,
        descriptor: &PalwComputeSetDescriptorV2,
    ) -> Result<(), ComputeSetRegistryError> {
        if let Some(existing) = self.descriptor(compute_set_id).map_err(store_fault)? {
            if existing.as_ref() == descriptor {
                return Ok(());
            }
            return Err(ComputeSetRegistryError::DescriptorDiverged(compute_set_id));
        }
        self.descriptors.write(BatchDbWriter::new(batch), compute_set_id, Arc::new(descriptor.clone())).map_err(store_fault)
    }

    /// Write-once insert of a policy record under its `policy_id` (same contract as descriptors).
    pub fn insert_policy_batch(
        &self,
        batch: &mut WriteBatch,
        policy_id: Hash64,
        policy: &PalwComputeSetPolicyV1,
    ) -> Result<(), ComputeSetRegistryError> {
        if let Some(existing) = self.policy(policy_id).map_err(store_fault)? {
            if existing.as_ref() == policy {
                return Ok(());
            }
            return Err(ComputeSetRegistryError::PolicySequenceDiverged { set: policy.compute_set_id, sequence: policy.policy_sequence });
        }
        self.policies.write(BatchDbWriter::new(batch), policy_id, Arc::new(policy.clone())).map_err(store_fault)
    }

    /// Write-once insert of an allocation plan under its `plan_id`.
    pub fn insert_plan_batch(
        &self,
        batch: &mut WriteBatch,
        plan_id: Hash64,
        plan: &PalwModelAllocationPlanV1,
    ) -> Result<(), ComputeSetRegistryError> {
        if let Some(existing) = self.plan(plan_id).map_err(store_fault)? {
            if existing.as_ref() == plan {
                return Ok(());
            }
            return Err(ComputeSetRegistryError::PlanSequenceDiverged(plan.sequence));
        }
        self.plans.write(BatchDbWriter::new(batch), plan_id, Arc::new(plan.clone())).map_err(store_fault)
    }

    /// Write-once insert of an activation certificate, keyed by the set it certifies.
    pub fn insert_certificate_batch(
        &self,
        batch: &mut WriteBatch,
        certificate: &PalwComputeSetActivationCertificateV1,
    ) -> Result<(), ComputeSetRegistryError> {
        if let Some(existing) = self.certificate(certificate.compute_set_id).map_err(store_fault)? {
            if existing.as_ref() == certificate {
                return Ok(());
            }
            // Two DIFFERENT quorum certificates for one set: re-certification is representable
            // by design (§17.3), later certificates supersede — but through the view fold, not a
            // silent overwrite here. Surface it.
            return Err(ComputeSetRegistryError::CertificateDiverged(certificate.compute_set_id));
        }
        self.certificates.write(BatchDbWriter::new(batch), certificate.compute_set_id, Arc::new(certificate.clone())).map_err(store_fault)
    }

    /// Write `block`'s carried registry view (atomic with the block commit).
    pub fn set_view_batch(
        &self,
        batch: &mut WriteBatch,
        block: BlockHash,
        view: Arc<PalwComputeRegistryViewV1>,
    ) -> Result<(), StoreError> {
        self.views.write(BatchDbWriter::new(batch), block, view)
    }

    /// Direct (non-batch) view write — diagnostics / tests.
    pub fn set_view(&self, block: BlockHash, view: Arc<PalwComputeRegistryViewV1>) -> Result<(), StoreError> {
        self.views.write(DirectDbWriter::new(&self.db), block, view)
    }

    pub fn delete_view_batch(&self, batch: &mut WriteBatch, block: BlockHash) -> Result<(), StoreError> {
        self.views.delete(BatchDbWriter::new(batch), block)
    }
}

#[inline]
fn store_fault(error: StoreError) -> ComputeSetRegistryError {
    // The registry is consensus-load-bearing once active; an unreadable/unwritable store is a
    // local fault, not an alternative semantic outcome. Callers on the commit path fail-stop on
    // this variant (the `palw_overlay_view_fail_stop` pattern).
    ComputeSetRegistryError::RegistryStoreFault(error.to_string())
}

impl PalwComputeRegistryStoreReader for DbPalwComputeRegistryStore {
    fn descriptor(&self, compute_set_id: Hash64) -> Result<Option<Arc<PalwComputeSetDescriptorV2>>, StoreError> {
        self.descriptors.read(compute_set_id).optional()
    }

    fn policy(&self, policy_id: Hash64) -> Result<Option<Arc<PalwComputeSetPolicyV1>>, StoreError> {
        self.policies.read(policy_id).optional()
    }

    fn plan(&self, plan_id: Hash64) -> Result<Option<Arc<PalwModelAllocationPlanV1>>, StoreError> {
        self.plans.read(plan_id).optional()
    }

    fn certificate(&self, compute_set_id: Hash64) -> Result<Option<Arc<PalwComputeSetActivationCertificateV1>>, StoreError> {
        self.certificates.read(compute_set_id).optional()
    }

    fn view(&self, block: BlockHash) -> Result<Option<Arc<PalwComputeRegistryViewV1>>, StoreError> {
        self.views.read(block).optional()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_compute_set::{
        ComputeSetState, PALW_COMPUTE_SET_DESCRIPTOR_VERSION, PALW_COMPUTE_SET_POLICY_VERSION,
    };
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn descriptor() -> PalwComputeSetDescriptorV2 {
        PalwComputeSetDescriptorV2 {
            version: PALW_COMPUTE_SET_DESCRIPTOR_VERSION,
            compute_vm_id: h(1),
            model_family_id: h(2),
            model_artifact_root: h(3),
            model_manifest_root: h(4),
            tokenizer_root: h(5),
            chat_template_root: h(6),
            preprocessing_root: h(7),
            decode_policy_root: h(8),
            semantic_program_root: h(9),
            shape_table_root: h(10),
            shape_cost_table_root: h(11),
            arithmetic_rules_root: h(12),
            overflow_budget_root: h(13),
            lut_root: h(14),
            trace_policy_root: h(15),
            checkpoint_policy_root: h(16),
            conformance_vector_root: h(17),
            modality_mask: 1,
            resource_limits_root: h(18),
        }
    }

    /// Write-once semantics on the content tiers + view roundtrip (§21.2).
    #[test]
    fn registry_store_write_once_and_view_roundtrip() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let store = DbPalwComputeRegistryStore::new(db, CachePolicy::Count(16), CachePolicy::Count(16));

        let d = descriptor();
        let set_id = d.compute_set_id();
        let mut batch = WriteBatch::default();
        store.insert_descriptor_batch(&mut batch, set_id, &d).unwrap();
        store.db.write(std::mem::take(&mut batch)).unwrap();
        assert_eq!(store.descriptor(set_id).unwrap().unwrap().as_ref(), &d);

        // Idempotent re-insert; divergent bytes under the same key are a hard error.
        let mut batch = WriteBatch::default();
        store.insert_descriptor_batch(&mut batch, set_id, &d).unwrap();
        let mut divergent = d.clone();
        divergent.lut_root = h(0x99);
        assert!(matches!(
            store.insert_descriptor_batch(&mut batch, set_id, &divergent),
            Err(ComputeSetRegistryError::DescriptorDiverged(_))
        ));

        // Policy tier.
        let policy = PalwComputeSetPolicyV1 {
            version: PALW_COMPUTE_SET_POLICY_VERSION,
            compute_set_id: set_id,
            policy_sequence: 1,
            effective_from_daa: 100,
            state: ComputeSetState::Proposed,
            no_new_jobs_from_daa: None,
            retired_from_daa: None,
            compute_work_scale: 0,
            weight_factor_bps: 0,
            min_leaf_bond_sompi: 0,
            job_timeout_daa: 600,
            receipt_retention_daa: 86_400,
            auditor_capacity_threshold: 0,
            premium_pi_bps: 0,
            max_prompt_tokens: 4096,
            max_output_tokens: 4096,
            allowed_shape_set_root: h(0x30),
        };
        let policy_id = policy.policy_id();
        let mut batch = WriteBatch::default();
        store.insert_policy_batch(&mut batch, policy_id, &policy).unwrap();
        store.db.write(batch).unwrap();
        assert_eq!(store.policy(policy_id).unwrap().unwrap().as_ref(), &policy);

        // View tier: block-keyed, absent → None, roundtrip, delete.
        let block = h(0x40);
        assert!(store.view(block).unwrap().is_none());
        let mut view = PalwComputeRegistryViewV1::new();
        view.sets.insert(set_id, Default::default());
        store.set_view(block, Arc::new(view.clone())).unwrap();
        assert_eq!(store.view(block).unwrap().unwrap().as_ref(), &view);
        let mut batch = WriteBatch::default();
        store.delete_view_batch(&mut batch, block).unwrap();
        store.db.write(batch).unwrap();
        assert!(store.view(block).unwrap().is_none());
    }
}
