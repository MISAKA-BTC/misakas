//! Selected-chain admission for complete PALW receipt DA objects.
//!
//! P2P chunk proofs establish availability of bytes under a root; they do not establish that those
//! bytes contain two owner-authorized, independently signed matching receipts. This module is the
//! mandatory publication seam: it freezes one virtual sink, resolves the leaf and provider records at
//! that snapshot, runs the complete V1/V2 verifier, and only then writes the content-addressed object.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use kaspa_consensus_core::palw::{
    PalwBatchViewV1, PalwProviderBondRecord, PalwPublicLeafV1,
    da::{
        PALW_DA_GC_MAX_DELETIONS_PER_CYCLE, PALW_DA_SERVICE_MAX_BYTES, PALW_DA_SERVICE_MAX_FETCH_TARGETS, PALW_DA_SERVICE_MAX_OBJECTS,
        PALW_RECEIPT_DA_OBJECT_VERSION_V1, PALW_RECEIPT_DA_OBJECT_VERSION_V2, PalwDaAdmissionError, PalwDaChallengeStatusV1,
        PalwDaFetchTargetV1, PalwDaObjectGcStatsV1, PalwDaObligationStatusV1, PalwDaServiceError, PalwDaServiceSnapshotV1,
        PalwDaServingObjectV1, PalwDaStateV1, PalwReceiptDaCommitmentV1, palw_receipt_da_commitment,
    },
    search_snapshot::{
        PALW_SEARCH_SNAPSHOT_MAX_FUTURE_DAA_SLACK, PALW_SEARCH_SNAPSHOT_RETENTION_DAA, PalwSearchAssignmentV1,
        PalwSearchSnapshotAdmittedV1, PalwSearchSnapshotV1, scheduler_key_id, snapshot_matches_assignment,
    },
};
use kaspa_database::prelude::StoreErrorPredicates;
use kaspa_hashes::Hash64;

use super::Consensus;
use crate::{
    model::stores::{
        headers::HeaderStoreReader,
        palw::PalwStoreReader,
        palw_da::{DbPalwDaStore, PalwDaStoreReader},
        palw_provider_bonds::PalwProviderBondsStoreReader,
        virtual_state::VirtualStateStoreReader,
    },
    processes::palw_da::{verify_receipt_da_object_v2_with_consensus_crypto, verify_receipt_da_object_with_consensus_crypto},
};

fn sink_view_has_live_da_leaf(view: Option<&PalwBatchViewV1>, batch_id: &Hash64, leaf_index: u32) -> bool {
    view.and_then(|view| view.entry(batch_id))
        .is_some_and(|lifecycle| leaf_index < lifecycle.leaf_count && !lifecycle.status.is_terminal())
}

fn retained_object_roots(state: &PalwDaStateV1, current_daa_score: u64) -> Result<BTreeSet<Hash64>, PalwDaServiceError> {
    if !state.validate_structure() {
        return Err(PalwDaServiceError::Inconsistent("selected-parent DA state failed structural validation".into()));
    }
    Ok(state
        .obligations
        .values()
        .filter(|obligation| current_daa_score <= obligation.retention_until_daa_score)
        .map(|obligation| obligation.object_root)
        .collect())
}

fn ensure_gc_sink_unchanged(captured_sink: Hash64, current_sink: Hash64) -> Result<(), PalwDaServiceError> {
    if captured_sink != current_sink {
        return Err(PalwDaServiceError::StaleSnapshot);
    }
    Ok(())
}

fn gc_delete_candidates(retained_roots: &BTreeSet<Hash64>, mut stored_roots: Vec<Hash64>) -> Vec<Hash64> {
    // Do not rely on a backend iterator's ordering. A deterministic prefix lets interrupted or
    // repeated sweeps make stable progress while bounding the virtual-lock critical section.
    stored_roots.sort_unstable();
    stored_roots.into_iter().filter(|root| !retained_roots.contains(root)).take(PALW_DA_GC_MAX_DELETIONS_PER_CYCLE).collect()
}

fn selected_parent_da_state(
    state: Option<Arc<PalwDaStateV1>>,
    selected_parent: Hash64,
    parent_daa_score: u64,
    activation_daa_score: u64,
    genesis: Hash64,
) -> Result<Arc<PalwDaStateV1>, PalwDaServiceError> {
    match state {
        Some(state) if state.validate_structure() => Ok(state),
        Some(_) => Err(PalwDaServiceError::Inconsistent(format!(
            "PALW DA selected-parent state {selected_parent} failed structural validation"
        ))),
        None if selected_parent == genesis || parent_daa_score < activation_daa_score => Ok(Arc::new(PalwDaStateV1::default())),
        None => Err(PalwDaServiceError::Inconsistent(format!(
            "PALW DA state is missing for active selected parent {selected_parent} at DAA {parent_daa_score}"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_palw_da_object_for_admission(
    network_id: u32,
    genesis_network_id: Hash64,
    sink_daa_score: u64,
    current_epoch: u64,
    leaf: &PalwPublicLeafV1,
    provider_a: &PalwProviderBondRecord,
    provider_b: &PalwProviderBondRecord,
    object_bytes: &[u8],
) -> Result<PalwReceiptDaCommitmentV1, PalwDaAdmissionError> {
    match leaf.receipt_da_object_version {
        PALW_RECEIPT_DA_OBJECT_VERSION_V1 => {
            verify_receipt_da_object_with_consensus_crypto(network_id, leaf, provider_a, provider_b, sink_daa_score, object_bytes)
                .map_err(|error| PalwDaAdmissionError::InvalidObject(format!("{error:?}")))?;
        }
        PALW_RECEIPT_DA_OBJECT_VERSION_V2 => {
            verify_receipt_da_object_v2_with_consensus_crypto(
                network_id,
                genesis_network_id,
                leaf,
                provider_a,
                provider_b,
                sink_daa_score,
                current_epoch,
                object_bytes,
            )
            .map_err(|error| PalwDaAdmissionError::InvalidObject(format!("{error:?}")))?;
        }
        version => return Err(PalwDaAdmissionError::UnsupportedObjectVersion(version)),
    }

    let commitment = palw_receipt_da_commitment(leaf.receipt_da_object_version, object_bytes)
        .map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
    if commitment.root != leaf.receipt_da_root
        || commitment.object_len != leaf.receipt_da_object_len
        || commitment.chunk_count != leaf.receipt_da_chunk_count
    {
        return Err(PalwDaAdmissionError::InvalidObject("object commitment does not match the selected-chain leaf".to_string()));
    }
    Ok(commitment)
}

/// Pure admission verifier for one canonical node-anchored search snapshot (DA object
/// version 3). No leaf or provider bond exists for this class yet: the snapshot is an input
/// artifact for future scheduler assignments, not a mint claim. What IS enforced, fail-closed:
/// strict canonical decode, network and genesis binding, and a retrieval DAA score that does
/// not lead the admitting sink by more than the pinned slack. Assignment-id resolution stays a
/// StopShip gate before any P2P or mint exposure.
/// Resolves the assignment side of admission. A zero `assignment_id` is the diagnostic
/// sentinel and must come WITHOUT an assignment; a non-zero id must come WITH the signed
/// assignment whose content-addressed id, scheduler signature, and every pinned field match.
pub(crate) fn resolve_palw_search_assignment(
    snapshot: &PalwSearchSnapshotV1,
    assignment_bytes: Option<&[u8]>,
    verify_signature: impl FnMut(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<Option<PalwSearchAssignmentV1>, PalwDaAdmissionError> {
    let zero_assignment = snapshot.assignment_id == Hash64::from_bytes([0; 64]);
    match (zero_assignment, assignment_bytes) {
        (true, None) => Ok(None),
        (true, Some(_)) => {
            Err(PalwDaAdmissionError::InvalidObject("diagnostic snapshot (zero assignment id) must not carry an assignment".into()))
        }
        (false, None) => {
            Err(PalwDaAdmissionError::InvalidObject("snapshot references an assignment but none was supplied".into()))
        }
        (false, Some(bytes)) => {
            let assignment = PalwSearchAssignmentV1::decode_strict(bytes)
                .map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
            assignment
                .verify_signature(verify_signature)
                .map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
            snapshot_matches_assignment(snapshot, &assignment)
                .map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
            Ok(Some(assignment))
        }
    }
}

pub(crate) fn verify_palw_search_snapshot_for_admission(
    network_id: u32,
    genesis_hash: Hash64,
    sink_daa_score: u64,
    object_bytes: &[u8],
) -> Result<(PalwSearchSnapshotV1, PalwReceiptDaCommitmentV1, Hash64), PalwDaAdmissionError> {
    let snapshot = PalwSearchSnapshotV1::decode_strict(object_bytes)
        .map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
    if snapshot.network_id != network_id {
        return Err(PalwDaAdmissionError::InvalidObject(format!(
            "snapshot network id {} does not match node network id {network_id}",
            snapshot.network_id
        )));
    }
    if snapshot.genesis_hash != genesis_hash {
        return Err(PalwDaAdmissionError::InvalidObject("snapshot genesis hash does not match this network".to_string()));
    }
    if snapshot.retrieval_daa_score > sink_daa_score.saturating_add(PALW_SEARCH_SNAPSHOT_MAX_FUTURE_DAA_SLACK) {
        return Err(PalwDaAdmissionError::InvalidObject(format!(
            "snapshot retrieval DAA score {} leads the sink {sink_daa_score} beyond the pinned slack",
            snapshot.retrieval_daa_score
        )));
    }
    let commitment =
        snapshot.da_commitment().map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
    let digest = snapshot.digest().map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
    Ok((snapshot, commitment, digest))
}

impl Consensus {
    pub(crate) fn palw_admit_da_object_impl(
        &self,
        batch_id: Hash64,
        leaf_index: u32,
        object_bytes: Arc<Vec<u8>>,
    ) -> Result<Hash64, PalwDaAdmissionError> {
        let params = &self.config.params;
        if params.palw_activation_daa_score == u64::MAX {
            return Err(PalwDaAdmissionError::Disabled);
        }

        // Virtual commit holds the matching write lock while changing the selected-chain sink and
        // provider registry. Keep this guard through every read, crypto decision, and durable insert,
        // so admission cannot splice a leaf from one sink to bond status from another.
        let virtual_read = self.virtual_stores.read();
        let virtual_state =
            virtual_read.state.get().map_err(|error| PalwDaAdmissionError::Store(format!("virtual state: {error:?}")))?;
        let sink = virtual_state.ghostdag_data.selected_parent;
        let sink_daa_score = self
            .storage
            .headers_store
            .get_daa_score(sink)
            .map_err(|error| PalwDaAdmissionError::Store(format!("sink header: {error:?}")))?;
        if sink_daa_score < params.palw_activation_daa_score {
            return Err(PalwDaAdmissionError::Disabled);
        }

        // A content-addressed leaf blob can outlive the fork which admitted it. Require the batch
        // and leaf coordinate to exist in the frozen sink's carried lifecycle view before consulting
        // that global immutable blob, otherwise a side-fork leaf could be mislabeled as selected-chain
        // admission. Terminal batches no longer have a live publication/retention obligation.
        let sink_view = self
            .storage
            .palw_overlay_view_store
            .view(sink)
            .map_err(|error| PalwDaAdmissionError::Store(format!("sink PALW view: {error:?}")))?;
        let leaf_is_in_sink_view = sink_view_has_live_da_leaf(sink_view.as_deref(), &batch_id, leaf_index);
        if !leaf_is_in_sink_view {
            return Err(PalwDaAdmissionError::LeafNotFound { batch_id, leaf_index });
        }

        let leaf = self.storage.palw_store.leaf(batch_id, leaf_index).map_err(|error| {
            if error.is_key_not_found() {
                PalwDaAdmissionError::LeafNotFound { batch_id, leaf_index }
            } else {
                PalwDaAdmissionError::Store(format!("leaf: {error:?}"))
            }
        })?;
        let provider_store = self.storage.palw_provider_bonds_store.read();
        let provider_a = provider_store.get(&leaf.provider_a_bond).map_err(|error| {
            if error.is_key_not_found() {
                PalwDaAdmissionError::ProviderNotFound(leaf.provider_a_bond)
            } else {
                PalwDaAdmissionError::Store(format!("provider A: {error:?}"))
            }
        })?;
        let provider_b = provider_store.get(&leaf.provider_b_bond).map_err(|error| {
            if error.is_key_not_found() {
                PalwDaAdmissionError::ProviderNotFound(leaf.provider_b_bond)
            } else {
                PalwDaAdmissionError::Store(format!("provider B: {error:?}"))
            }
        })?;

        let commitment = verify_palw_da_object_for_admission(
            params.net.suffix().unwrap_or(0),
            self.config.genesis.hash,
            sink_daa_score,
            sink_daa_score / params.palw_epoch_length_daa.max(1),
            &leaf,
            &provider_a,
            &provider_b,
            &object_bytes,
        )?;
        self.storage
            .palw_da_store
            .write()
            .insert_admitted_object(commitment.root, object_bytes)
            .map_err(|error| PalwDaAdmissionError::Store(format!("object store: {error:?}")))?;
        drop(provider_store);
        drop(virtual_read);
        Ok(commitment.root)
    }

    /// Admit one canonical node-anchored search snapshot against a single frozen virtual sink.
    /// Same locking discipline as receipt admission: the virtual read guard is held through the
    /// activation gate, the semantic verifier, and the durable insert, so a snapshot cannot bind
    /// a DAA score from one sink and be stored under another. Retention is
    /// `sink + PALW_SEARCH_SNAPSHOT_RETENTION_DAA`; expired rows are reclaimed by the
    /// snapshot-specific GC, never by receipt-obligation GC.
    pub(crate) fn palw_admit_search_snapshot_impl(
        &self,
        object_bytes: Arc<Vec<u8>>,
        assignment_bytes: Option<Arc<Vec<u8>>>,
    ) -> Result<PalwSearchSnapshotAdmittedV1, PalwDaAdmissionError> {
        let params = &self.config.params;
        if params.palw_activation_daa_score == u64::MAX {
            return Err(PalwDaAdmissionError::Disabled);
        }
        let virtual_read = self.virtual_stores.read();
        let virtual_state =
            virtual_read.state.get().map_err(|error| PalwDaAdmissionError::Store(format!("virtual state: {error:?}")))?;
        let sink = virtual_state.ghostdag_data.selected_parent;
        let sink_daa_score = self
            .storage
            .headers_store
            .get_daa_score(sink)
            .map_err(|error| PalwDaAdmissionError::Store(format!("sink header: {error:?}")))?;
        if sink_daa_score < params.palw_activation_daa_score {
            return Err(PalwDaAdmissionError::Disabled);
        }
        let (snapshot, commitment, digest) = verify_palw_search_snapshot_for_admission(
            params.net.suffix().unwrap_or(0),
            self.config.genesis.hash,
            sink_daa_score,
            &object_bytes,
        )?;
        let assignment = resolve_palw_search_assignment(
            &snapshot,
            assignment_bytes.as_deref().map(Vec::as_slice),
            crate::processes::palw_da::consensus_mldsa_verify,
        )?;
        if let Some(assignment) = assignment.as_ref() {
            // Node-local governance: only allowlisted scheduler keys may anchor
            // assignment-resolved snapshots. Empty allowlist = fail-closed.
            kaspa_consensus_core::palw::search_snapshot::enforce_scheduler_allowlist(
                &assignment.scheduler_public_key,
                &self.config.palw_search_scheduler_allowlist,
            )
            .map_err(|error| PalwDaAdmissionError::InvalidObject(error.to_string()))?;
        }
        let admitted = PalwSearchSnapshotAdmittedV1 {
            object_root: commitment.root,
            snapshot_digest: digest,
            object_len: commitment.object_len,
            chunk_count: commitment.chunk_count,
            admitted_daa_score: sink_daa_score,
            retention_until_daa_score: sink_daa_score.saturating_add(PALW_SEARCH_SNAPSHOT_RETENTION_DAA),
            assignment_resolved: assignment.is_some(),
            scheduler_key_id: assignment.as_ref().map(|assignment| scheduler_key_id(&assignment.scheduler_public_key)),
        };
        let stored = crate::model::stores::palw_da::PalwSearchSnapshotStoredV1 {
            admitted_daa_score: admitted.admitted_daa_score,
            retention_until_daa_score: admitted.retention_until_daa_score,
            snapshot_digest: digest,
            bytes: Arc::try_unwrap(object_bytes).unwrap_or_else(|shared| (*shared).clone()),
        };
        self.storage
            .palw_da_store
            .write()
            .insert_admitted_search_snapshot(admitted.object_root, Arc::new(stored))
            .map_err(|error| PalwDaAdmissionError::Store(format!("search snapshot store: {error:?}")))?;
        drop(virtual_read);
        Ok(admitted)
    }

    /// Reclaim search snapshots whose retention window ended before the current sink DAA score.
    /// Bounded per cycle; deterministic root order. Entirely disjoint from receipt-obligation GC:
    /// this sweep reads only the snapshot column's own retention metadata.
    pub(crate) fn palw_search_snapshot_gc_impl(&self) -> Result<Vec<Hash64>, PalwDaServiceError> {
        let params = &self.config.params;
        if params.palw_activation_daa_score == u64::MAX {
            return Err(PalwDaServiceError::Disabled);
        }
        let virtual_read = self.virtual_stores.read();
        let virtual_state =
            virtual_read.state.get().map_err(|error| PalwDaServiceError::Store(format!("virtual state: {error:?}")))?;
        let current_daa_score = self
            .storage
            .headers_store
            .get_daa_score(virtual_state.ghostdag_data.selected_parent)
            .map_err(|error| PalwDaServiceError::Store(format!("sink header: {error:?}")))?;
        let deleted = self
            .storage
            .palw_da_store
            .write()
            .delete_expired_search_snapshots(current_daa_score, PALW_DA_GC_MAX_DELETIONS_PER_CYCLE)
            .map_err(|error| PalwDaServiceError::Store(format!("search snapshot GC: {error:?}")))?;
        drop(virtual_read);
        Ok(deleted)
    }

    /// Build the bounded input to the local availability service from exactly one frozen virtual
    /// sink. The durable object table is never scanned: roots come exclusively from retained
    /// selected-chain obligations, which also makes restart rehydration remove stale side-fork data.
    pub(crate) fn palw_da_service_snapshot_impl(&self) -> Result<PalwDaServiceSnapshotV1, PalwDaServiceError> {
        let params = &self.config.params;
        if params.palw_activation_daa_score == u64::MAX || !params.palw_algo4_accept {
            return Err(PalwDaServiceError::Disabled);
        }

        let virtual_read = self.virtual_stores.read();
        let virtual_state =
            virtual_read.state.get().map_err(|error| PalwDaServiceError::Store(format!("virtual state: {error:?}")))?;
        let selected_parent = virtual_state.ghostdag_data.selected_parent;
        let current_daa_score = self
            .storage
            .headers_store
            .get_daa_score(selected_parent)
            .map_err(|error| PalwDaServiceError::Store(format!("selected-parent header: {error:?}")))?;
        let state = match self.storage.palw_da_store.read().state(selected_parent) {
            Ok(state) => Some(state),
            Err(error) if error.is_key_not_found() => None,
            Err(error) => return Err(PalwDaServiceError::Store(format!("selected-parent DA state: {error:?}"))),
        };
        let state = selected_parent_da_state(
            state,
            selected_parent,
            current_daa_score,
            params.palw_activation_daa_score,
            self.config.genesis.hash,
        )?;

        // First collapse the possibly many provider/sample obligations to one recovery target per
        // object root. Open challenges win over background obligations, then earlier deadlines win.
        let mut candidates: BTreeMap<Hash64, PalwDaFetchTargetV1> = BTreeMap::new();
        for (obligation_id, obligation) in &state.obligations {
            if current_daa_score > obligation.retention_until_daa_score {
                continue;
            }
            let (challenged, deadline_daa_score) = match obligation.status {
                PalwDaObligationStatusV1::Pending => (false, obligation.retention_until_daa_score),
                PalwDaObligationStatusV1::Challenged(challenge_id) => {
                    let challenge = state.challenges.get(&challenge_id).ok_or_else(|| {
                        PalwDaServiceError::Inconsistent(format!("obligation {obligation_id} names a missing challenge"))
                    })?;
                    if challenge.challenge.obligation_id != *obligation_id
                        || challenge.object_root != obligation.object_root
                        || challenge.chunk_index != obligation.chunk_index
                        || !matches!(challenge.status, PalwDaChallengeStatusV1::Open)
                    {
                        return Err(PalwDaServiceError::Inconsistent(format!(
                            "obligation {obligation_id} has a mismatched/non-open challenge"
                        )));
                    }
                    (true, challenge.challenge.response_deadline_daa_score)
                }
                PalwDaObligationStatusV1::Satisfied(_) | PalwDaObligationStatusV1::TimedOut(_) => {
                    // Still rehydrate already-admitted bytes through retention, but do not fetch on a
                    // terminal obligation solely to improve this node's cache.
                    (false, obligation.retention_until_daa_score)
                }
            };
            let target = PalwDaFetchTargetV1 {
                obligation_id: *obligation_id,
                batch_id: obligation.batch_id,
                leaf_index: obligation.leaf_index,
                object_root: obligation.object_root,
                object_len: obligation.object_len,
                chunk_count: obligation.chunk_count,
                required_chunk_index: obligation.chunk_index,
                deadline_daa_score,
                challenged,
            };
            candidates
                .entry(obligation.object_root)
                .and_modify(|existing| {
                    if (target.challenged && !existing.challenged)
                        || (target.challenged == existing.challenged && target.deadline_daa_score < existing.deadline_daa_score)
                    {
                        *existing = target.clone();
                    }
                })
                .or_insert(target);
        }

        let mut ordered = candidates.into_values().collect::<Vec<_>>();
        ordered.sort_by_key(|target| (!target.challenged, target.deadline_daa_score, target.object_root));

        let mut serving_objects = Vec::new();
        let mut fetch_targets = Vec::new();
        let mut total_bytes = 0usize;
        for target in ordered {
            if serving_objects.len() >= PALW_DA_SERVICE_MAX_OBJECTS && fetch_targets.len() >= PALW_DA_SERVICE_MAX_FETCH_TARGETS {
                break;
            }
            let leaf = self.storage.palw_store.leaf(target.batch_id, target.leaf_index).map_err(|error| {
                PalwDaServiceError::Inconsistent(format!(
                    "obligation leaf {}/{} is unavailable: {error:?}",
                    target.batch_id, target.leaf_index
                ))
            })?;
            // V1 remains readable by consensus for legacy closed-testnet data, but public availability
            // orchestration is intentionally Object-v2-only.
            if leaf.receipt_da_object_version != PALW_RECEIPT_DA_OBJECT_VERSION_V2 {
                continue;
            }
            if leaf.receipt_da_root != target.object_root
                || leaf.receipt_da_object_len != target.object_len
                || leaf.receipt_da_chunk_count != target.chunk_count
                || target.required_chunk_index >= target.chunk_count
            {
                return Err(PalwDaServiceError::Inconsistent(format!(
                    "obligation {} metadata does not match its selected-chain leaf",
                    target.obligation_id
                )));
            }

            match self.storage.palw_da_store.read().object(target.object_root) {
                Ok(bytes) => {
                    DbPalwDaStore::validate_object(target.object_root, &bytes).map_err(|error| {
                        PalwDaServiceError::Inconsistent(format!("durable object {}: {error:?}", target.object_root))
                    })?;
                    let version =
                        bytes.get(..2).and_then(|prefix| prefix.try_into().ok()).map(u16::from_le_bytes).ok_or_else(|| {
                            PalwDaServiceError::Inconsistent(format!("durable object {} has no version", target.object_root))
                        })?;
                    if version != PALW_RECEIPT_DA_OBJECT_VERSION_V2
                        || bytes.len() != target.object_len as usize
                        || total_bytes.saturating_add(bytes.len()) > PALW_DA_SERVICE_MAX_BYTES
                    {
                        if version != PALW_RECEIPT_DA_OBJECT_VERSION_V2 || bytes.len() != target.object_len as usize {
                            return Err(PalwDaServiceError::Inconsistent(format!(
                                "durable object {} metadata does not match Object-v2 obligation",
                                target.object_root
                            )));
                        }
                        continue;
                    }
                    if serving_objects.len() < PALW_DA_SERVICE_MAX_OBJECTS {
                        total_bytes += bytes.len();
                        serving_objects.push(PalwDaServingObjectV1 { object_root: target.object_root, bytes });
                    }
                }
                Err(error) if error.is_key_not_found() => {
                    if !matches!(
                        state.obligations[&target.obligation_id].status,
                        PalwDaObligationStatusV1::Satisfied(_) | PalwDaObligationStatusV1::TimedOut(_)
                    ) && fetch_targets.len() < PALW_DA_SERVICE_MAX_FETCH_TARGETS
                    {
                        fetch_targets.push(target);
                    }
                }
                Err(error) => return Err(PalwDaServiceError::Store(format!("durable object lookup: {error:?}"))),
            }
        }

        // P2P chunk serving for node-anchored search snapshots: admitted, retention-live rows are
        // served under the same object/byte caps as receipt objects, in deterministic root order.
        // Chunk proofs come from the version-parameterized DA-01 tree, so a version-3 proof built
        // over these exact bytes verifies against the anchored root as-is.
        if serving_objects.len() < PALW_DA_SERVICE_MAX_OBJECTS && total_bytes < PALW_DA_SERVICE_MAX_BYTES {
            let store = self.storage.palw_da_store.read();
            let mut roots = store
                .search_snapshot_roots()
                .map_err(|error| PalwDaServiceError::Store(format!("search snapshot roots: {error:?}")))?;
            roots.sort_unstable();
            for root in roots {
                if serving_objects.len() >= PALW_DA_SERVICE_MAX_OBJECTS {
                    break;
                }
                let stored = store
                    .search_snapshot(root)
                    .map_err(|error| PalwDaServiceError::Store(format!("search snapshot {root}: {error:?}")))?;
                if current_daa_score > stored.retention_until_daa_score {
                    continue;
                }
                if total_bytes.saturating_add(stored.bytes.len()) > PALW_DA_SERVICE_MAX_BYTES {
                    break;
                }
                total_bytes += stored.bytes.len();
                serving_objects.push(PalwDaServingObjectV1 { object_root: root, bytes: Arc::new(stored.bytes.clone()) });
            }
        }
        Ok(PalwDaServiceSnapshotV1 { selected_parent, current_daa_score, serving_objects, fetch_targets })
    }

    /// Sweep auxiliary object bytes without ever deriving liveness from the 64-object serving cache.
    /// The large prefix walk happens outside the virtual lock. A second sink check fences the atomic
    /// delete batch against reorgs; iterator failure or generation mismatch deletes zero rows.
    pub(crate) fn palw_da_gc_objects_impl(&self) -> Result<PalwDaObjectGcStatsV1, PalwDaServiceError> {
        let params = &self.config.params;
        if params.palw_activation_daa_score == u64::MAX || !params.palw_algo4_accept {
            return Err(PalwDaServiceError::Disabled);
        }

        let (selected_parent, retained_roots) = {
            let virtual_read = self.virtual_stores.read();
            let virtual_state =
                virtual_read.state.get().map_err(|error| PalwDaServiceError::Store(format!("virtual state: {error:?}")))?;
            let selected_parent = virtual_state.ghostdag_data.selected_parent;
            let current_daa_score = self
                .storage
                .headers_store
                .get_daa_score(selected_parent)
                .map_err(|error| PalwDaServiceError::Store(format!("selected-parent header: {error:?}")))?;
            let state = match self.storage.palw_da_store.read().state(selected_parent) {
                Ok(state) => Some(state),
                Err(error) if error.is_key_not_found() => None,
                Err(error) => return Err(PalwDaServiceError::Store(format!("selected-parent DA state: {error:?}"))),
            };
            let state = selected_parent_da_state(
                state,
                selected_parent,
                current_daa_score,
                params.palw_activation_daa_score,
                self.config.genesis.hash,
            )?;
            (selected_parent, retained_object_roots(&state, current_daa_score)?)
        };

        // This call fully collects (and validates) the prefix. Any iterator error returns before a
        // WriteBatch exists, which is the delete-zero guarantee for corrupt/failed scans.
        let stored_roots = self
            .storage
            .palw_da_store
            .read()
            .object_roots()
            .map_err(|error| PalwDaServiceError::Store(format!("object GC scan: {error:?}")))?;
        let scanned_objects = stored_roots.len();
        // Candidate derivation can touch every stored key, so keep it outside the virtual lock. The
        // deterministic cap bounds both the later WriteBatch and the reorg-fenced critical section.
        let deletions = gc_delete_candidates(&retained_roots, stored_roots);

        let virtual_read = self.virtual_stores.read();
        let current_virtual =
            virtual_read.state.get().map_err(|error| PalwDaServiceError::Store(format!("virtual state recheck: {error:?}")))?;
        let current_sink = current_virtual.ghostdag_data.selected_parent;
        ensure_gc_sink_unchanged(selected_parent, current_sink)?;
        self.storage
            .palw_da_store
            .write()
            .delete_objects_atomic(&deletions)
            .map_err(|error| PalwDaServiceError::Store(format!("object GC batch: {error:?}")))?;
        drop(virtual_read);

        Ok(PalwDaObjectGcStatsV1 {
            selected_parent,
            retained_roots: retained_roots.len(),
            scanned_objects,
            deleted_objects: deletions.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        dns_finality::validator_id_from_pubkey,
        palw::{
            PalwBatchLifecycleV1, PalwBatchStatus, PalwBatchViewV1, PalwProviderBondMutation, PalwProviderBondRecord,
            PalwProviderBondStatus, PalwPublicLeafV1, ProviderBondView,
            da::{
                PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT, PALW_PROVIDER_SESSION_AUTH_VERSION_V1, PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
                PALW_RECEIPT_DA_OBJECT_VERSION_V1, PALW_RECEIPT_DA_OBJECT_VERSION_V2, PalwBuriedBeaconV1, PalwDaObligationStatusV1,
                PalwDaPolicyV1, PalwProviderSessionAuthorizationV1, palw_receipt_da_chunk_proof,
            },
            effective_provider_bond_status, validate_palw_overlay_payload,
        },
        tx::{ScriptPublicKey, TransactionOutpoint},
    };
    use kaspa_hashes::blake2b_512_keyed;
    use kaspa_pq_validator_core::ValidatorKey;
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;
    use misaka_palw::receipt_v3::{
        ComputeReceiptV3, ImplementationTelemetryV3, MLDSA87_ALGORITHM_ID, MatchProjectionV2, PALW_RECEIPT_V3_MLDSA87_CONTEXT,
        RECEIPT_V3_VERSION, ReceiptV3Expectations, ReceiptV3SubmissionRef, SignedEnvelopeV3, credential_id_from_verifying_key,
        execution_nullifier_v3, output_commitment_v3, verify_and_match_receipts_v3,
    };
    use misaka_palw_miner::da::{
        build_da_timeout_evidence, build_signed_da_challenge, build_signed_da_response,
        decode_canonical_palw_receipt_da_object_v2_wire, encode_da_challenge, encode_da_response, encode_da_timeout,
        palw_receipt_da_object_v2_wire_bytes,
    };

    use crate::processes::palw_da::{
        PalwDaApplyContext, PalwDaOverlayEffect, PalwReceiptDaObjectV2, apply_palw_da_effect, palw_receipt_da_object_v2_bytes,
    };

    const NETWORK_ID: u32 = 110;
    const SINK_DAA_SCORE: u64 = 1_000;
    const CURRENT_EPOCH: u64 = 10;

    #[test]
    fn admission_requires_the_leaf_coordinate_in_a_live_sink_view() {
        let batch_id = h(0x61);
        assert!(!sink_view_has_live_da_leaf(None, &batch_id, 0));

        let mut view = PalwBatchViewV1::new();
        view.batches.insert(
            batch_id,
            PalwBatchLifecycleV1 {
                status: PalwBatchStatus::Committed,
                registration_epoch: 1,
                activation_not_before_epoch: 3,
                expiry_epoch: 9,
                leaf_count: 1,
                chunk_count: 1,
                chunks_present: [1, 0, 0, 0],
                leaf_root: h(0x62),
                cert_hash: None,
                cert_activation_epoch: 0,
                cert_expiry_epoch: 0,
                cert_approving_stake: 0,
                first_cert_daa: None,
                revoked_from_daa: None,
            },
        );
        assert!(sink_view_has_live_da_leaf(Some(&view), &batch_id, 0));
        assert!(!sink_view_has_live_da_leaf(Some(&view), &batch_id, 1));
        view.batches.get_mut(&batch_id).unwrap().status = PalwBatchStatus::Expired;
        assert!(!sink_view_has_live_da_leaf(Some(&view), &batch_id, 0));
    }

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn gc_obligation(id: Hash64, root: Hash64, retention_until_daa_score: u64) -> kaspa_consensus_core::palw::da::PalwDaObligationV1 {
        kaspa_consensus_core::palw::da::PalwDaObligationV1 {
            version: 1,
            obligation_id: id,
            batch_id: h(0x70),
            leaf_index: 0,
            leaf_hash: h(0x71),
            object_root: root,
            object_len: 1,
            chunk_count: 1,
            chunk_index: 0,
            provider_bond: TransactionOutpoint::new(h(0x72), 0),
            beacon_epoch: 0,
            beacon_anchor: h(0x73),
            created_daa_score: 1,
            retention_until_daa_score,
            status: PalwDaObligationStatusV1::Pending,
        }
    }

    #[test]
    fn gc_retained_roots_are_not_truncated_to_serving_cache_cap_and_reorg_deletes_zero() {
        let mut state = PalwDaStateV1::default();
        for index in 0..=PALW_DA_SERVICE_MAX_OBJECTS as u8 {
            let id = h(index.wrapping_add(0x80));
            state.obligations.insert(id, gc_obligation(id, h(index), 200));
        }
        let expired_id = h(0xf1);
        state.obligations.insert(expired_id, gc_obligation(expired_id, h(0xf2), 99));
        let retained = retained_object_roots(&state, 100).unwrap();
        assert_eq!(retained.len(), PALW_DA_SERVICE_MAX_OBJECTS + 1);
        assert!(retained.contains(&h(PALW_DA_SERVICE_MAX_OBJECTS as u8)));
        assert!(!retained.contains(&h(0xf2)));

        let sink = h(0x31);
        let stored = vec![h(0), h(PALW_DA_SERVICE_MAX_OBJECTS as u8), h(0xf2), h(0xfe)];
        assert_eq!(gc_delete_candidates(&retained, stored), vec![h(0xf2), h(0xfe)]);
        assert_eq!(ensure_gc_sink_unchanged(sink, h(0x32)), Err(PalwDaServiceError::StaleSnapshot));
    }

    #[test]
    fn service_snapshot_and_gc_reject_active_missing_or_corrupt_parent_state() {
        let selected_parent = h(0x41);
        let genesis = h(0x42);
        let missing = selected_parent_da_state(None, selected_parent, 100, 50, genesis);
        assert!(matches!(missing, Err(PalwDaServiceError::Inconsistent(_))));

        let corrupt = PalwDaStateV1 { version: 0, ..Default::default() };
        let corrupt = selected_parent_da_state(Some(Arc::new(corrupt)), selected_parent, 100, 50, genesis);
        assert!(matches!(corrupt, Err(PalwDaServiceError::Inconsistent(_))));

        assert!(selected_parent_da_state(None, genesis, 100, 50, genesis).is_ok());
        assert!(selected_parent_da_state(None, selected_parent, 49, 50, genesis).is_ok());
    }

    #[test]
    fn gc_deletion_plan_is_deterministic_and_capped() {
        fn indexed_hash(index: u64) -> Hash64 {
            let mut bytes = [0u8; 64];
            bytes[..8].copy_from_slice(&index.to_be_bytes());
            Hash64::from_bytes(bytes)
        }

        let retained = BTreeSet::from([indexed_hash(2)]);
        let mut stored = (0..PALW_DA_GC_MAX_DELETIONS_PER_CYCLE as u64 + 2).map(indexed_hash).collect::<Vec<_>>();
        stored.reverse();
        let deletions = gc_delete_candidates(&retained, stored);
        assert_eq!(deletions.len(), PALW_DA_GC_MAX_DELETIONS_PER_CYCLE);
        assert!(!deletions.contains(&indexed_hash(2)));
        assert!(deletions.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(deletions[0], indexed_hash(0));
    }

    fn projection() -> MatchProjectionV2 {
        MatchProjectionV2 {
            compute_set_id: h(0x31),
            job_challenge: h(0x32),
            output_commitment: output_commitment_v3(&[1, 2, 3], &h(0x32)),
            schedule_root: h(0x51),
            execution_root: h(0x52),
            route_root: h(0x53),
            state_root: h(0x54),
            canonical_compute_units: 1_234,
            token_count: 5,
            stop_reason: 0,
        }
    }

    fn signed_receipt(slot: u8, seed: u8, class: u8) -> (ComputeReceiptV3, SignedEnvelopeV3, Vec<u8>) {
        let keypair = mldsa::generate_key_pair([seed; 32]);
        let verifying_key = keypair.verification_key.as_ref().to_vec();
        let credential = credential_id_from_verifying_key(&verifying_key);
        let projection = projection();
        let receipt = ComputeReceiptV3 {
            receipt_version: RECEIPT_V3_VERSION,
            network_id: h(0x10),
            execution_nullifier: execution_nullifier_v3(
                &h(0x10),
                &projection.compute_set_id,
                &projection.job_challenge,
                &credential,
                slot,
                5,
            ),
            projection,
            telemetry: ImplementationTelemetryV3 { runtime_class_id: [class; 32], runtime_manifest_hash: [class.wrapping_add(1); 32] },
            worker_credential_id: credential,
            replica_slot: slot,
            issued_epoch: 5,
            expires_epoch: 20,
        };
        let body_digest = receipt.signing_digest();
        let signature = mldsa::sign(&keypair.signing_key, body_digest.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT, [0; 32])
            .expect("deterministic Receipt-v3 signing")
            .as_ref()
            .to_vec();
        let envelope = SignedEnvelopeV3 { body_digest, algorithm: MLDSA87_ALGORITHM_ID, signer_credential_id: credential, signature };
        (receipt, envelope, verifying_key)
    }

    fn provider_and_authorization(
        owner_seed: u8,
        bond: TransactionOutpoint,
        session_public_key: Vec<u8>,
        nonce: Hash64,
    ) -> (PalwProviderBondRecord, PalwProviderSessionAuthorizationV1) {
        let owner = mldsa::generate_key_pair([owner_seed; 32]);
        let owner_public_key = owner.verification_key.as_ref().to_vec();
        let record = PalwProviderBondRecord {
            version: 1,
            bond_outpoint: bond,
            owner_pubkey_hash: validator_id_from_pubkey(&owner_public_key),
            owner_public_key: owner_public_key.clone(),
            operator_group_id: h(owner_seed),
            runtime_classes: vec![],
            capacity_by_shape: vec![],
            reward_key_root: h(owner_seed.wrapping_add(1)),
            amount_sompi: 1_000_000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbond_delay_epochs: 10,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        };
        let mut authorization = PalwProviderSessionAuthorizationV1 {
            version: PALW_PROVIDER_SESSION_AUTH_VERSION_V1,
            network_id: NETWORK_ID,
            provider_bond: bond,
            owner_public_key,
            session_public_key,
            valid_from_epoch: 5,
            valid_until_epoch: 20,
            authorization_nonce: nonce,
            signature: vec![],
        };
        authorization.signature = mldsa::sign(
            &owner.signing_key,
            authorization.signing_hash().as_byte_slice(),
            PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
            [0; 32],
        )
        .expect("deterministic owner authorization signing")
        .as_ref()
        .to_vec();
        (record, authorization)
    }

    struct V2Fixture {
        leaf: PalwPublicLeafV1,
        provider_a: PalwProviderBondRecord,
        provider_b: PalwProviderBondRecord,
        object: PalwReceiptDaObjectV2,
        bytes: Vec<u8>,
    }

    fn restamp(leaf: &mut PalwPublicLeafV1, object: &PalwReceiptDaObjectV2) -> Vec<u8> {
        let bytes = palw_receipt_da_object_v2_bytes(object).unwrap();
        let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &bytes).unwrap();
        leaf.receipt_da_root = commitment.root;
        leaf.receipt_da_object_len = commitment.object_len;
        leaf.receipt_da_chunk_count = commitment.chunk_count;
        bytes
    }

    fn fixture() -> V2Fixture {
        let (receipt_a, envelope_a, key_a) = signed_receipt(0, 0x11, 1);
        let (receipt_b, envelope_b, key_b) = signed_receipt(1, 0x22, 2);
        let bond_a = TransactionOutpoint::new(h(0xc1), 0);
        let bond_b = TransactionOutpoint::new(h(0xc2), 1);
        let (provider_a, session_authorization_a) = provider_and_authorization(0xa1, bond_a, key_a.clone(), h(0xd1));
        let (provider_b, session_authorization_b) = provider_and_authorization(0xb2, bond_b, key_b.clone(), h(0xd2));
        let expected = |slot, key: &[u8]| ReceiptV3Expectations {
            network_id: h(0x10),
            compute_set_id: h(0x31),
            job_challenge: h(0x32),
            replica_slot: slot,
            issued_epoch: 5,
            expires_epoch: 20,
            current_epoch: CURRENT_EPOCH,
            registered_credential_id: credential_id_from_verifying_key(key),
        };
        let expected_a = expected(0, &key_a);
        let expected_b = expected(1, &key_b);
        let pair_id = verify_and_match_receipts_v3(
            ReceiptV3SubmissionRef { receipt: &receipt_a, envelope: &envelope_a, verifying_key: &key_a, expected: &expected_a },
            ReceiptV3SubmissionRef { receipt: &receipt_b, envelope: &envelope_b, verifying_key: &key_b, expected: &expected_b },
        )
        .unwrap()
        .pair_id();
        let object = PalwReceiptDaObjectV2 {
            version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
            network_id: h(0x10),
            batch_id: h(0x02),
            leaf_index: 7,
            provider_a_bond: bond_a,
            provider_b_bond: bond_b,
            receipt_a,
            envelope_a,
            receipt_b,
            envelope_b,
            session_authorization_a,
            session_authorization_b,
            matched_pair_id: pair_id,
        };
        let mut leaf = PalwPublicLeafV1 {
            version: 1,
            batch_id: object.batch_id,
            leaf_index: object.leaf_index,
            job_nullifier: h(0x32),
            ticket_nullifier_commitment: h(0x41),
            model_profile_id: h(0x42),
            runtime_class_id: h(0x43),
            shape_id: 1,
            quantum_count: 1,
            proof_type: 1,
            provider_a_bond: bond_a,
            provider_b_bond: bond_b,
            provider_a_reward_script: ScriptPublicKey::from_vec(0, vec![]),
            provider_b_reward_script: ScriptPublicKey::from_vec(0, vec![]),
            ticket_authority_pk_hash: h(0x44),
            private_match_commitment: pair_id,
            receipt_da_object_version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
            receipt_da_root: Hash64::default(),
            receipt_da_object_len: 0,
            receipt_da_chunk_count: 0,
            receipt_v3_compute_set_id: h(0x31),
            receipt_v3_job_challenge: h(0x32),
            receipt_v3_issued_epoch: 5,
            receipt_v3_expires_epoch: 20,
            registered_epoch: 6,
            activation_epoch: 10,
            expiry_epoch: 20,
            leaf_bond_sompi: 1_000,
        };
        let bytes = restamp(&mut leaf, &object);
        V2Fixture { leaf, provider_a, provider_b, object, bytes }
    }

    fn admit(fixture: &V2Fixture) -> Result<PalwReceiptDaCommitmentV1, PalwDaAdmissionError> {
        verify_palw_da_object_for_admission(
            NETWORK_ID,
            h(0x10),
            SINK_DAA_SCORE,
            CURRENT_EPOCH,
            &fixture.leaf,
            &fixture.provider_a,
            &fixture.provider_b,
            &fixture.bytes,
        )
    }

    #[test]
    fn public_da_admission_runs_full_v2_semantics_before_root_only_storage() {
        let valid = fixture();
        assert_eq!(admit(&valid).unwrap().root, valid.leaf.receipt_da_root);

        let mut wrong_bond = fixture();
        wrong_bond.object.provider_a_bond = TransactionOutpoint::new(h(0xee), 0);
        wrong_bond.bytes = restamp(&mut wrong_bond.leaf, &wrong_bond.object);
        assert!(matches!(admit(&wrong_bond), Err(PalwDaAdmissionError::InvalidObject(_))));

        let mut forged_authorization = fixture();
        forged_authorization.object.session_authorization_a.signature[0] ^= 1;
        forged_authorization.bytes = restamp(&mut forged_authorization.leaf, &forged_authorization.object);
        assert!(matches!(admit(&forged_authorization), Err(PalwDaAdmissionError::InvalidObject(_))));

        let mut forged_receipt = fixture();
        forged_receipt.object.envelope_b.signature[0] ^= 1;
        forged_receipt.bytes = restamp(&mut forged_receipt.leaf, &forged_receipt.object);
        let root_only = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &forged_receipt.bytes).unwrap();
        assert_eq!(root_only.root, forged_receipt.leaf.receipt_da_root, "root-only validation accepts the forged bytes");
        assert!(matches!(admit(&forged_receipt), Err(PalwDaAdmissionError::InvalidObject(_))));

        let mut wrong_pair = fixture();
        wrong_pair.object.matched_pair_id = blake2b_512_keyed(b"wrong-pair", b"not-the-verified-pair");
        wrong_pair.bytes = restamp(&mut wrong_pair.leaf, &wrong_pair.object);
        assert!(matches!(admit(&wrong_pair), Err(PalwDaAdmissionError::InvalidObject(_))));
    }

    #[test]
    fn header_v4_object_v2_response_satisfies_certificate_gate_and_timeout_reorg_is_exact() {
        let fixture = fixture();
        let commitment = admit(&fixture).unwrap();
        let policy = PalwDaPolicyV1::STRICT_TESTNET;
        let beacon = PalwBuriedBeaconV1 {
            epoch: CURRENT_EPOCH,
            seed: h(0x71),
            anchor_hash: h(0x72),
            anchor_daa_score: 100,
            observed_daa_score: 250,
        };
        let mut state = PalwDaStateV1::default();
        let (obligation_ids, registration_undo) =
            state.register_leaf_obligations(&fixture.leaf, commitment, &beacon, &policy, 300).unwrap();
        assert_eq!(obligation_ids.len(), 2);
        assert!(!state.certificate_allowed(&fixture.leaf.batch_id));

        let challenger_bond = TransactionOutpoint::new(h(0xc3), 2);
        let (challenger, _) = provider_and_authorization(
            0xc3,
            challenger_bond,
            fixture.object.session_authorization_a.session_public_key.clone(),
            h(0xd3),
        );
        let challenger_key = ValidatorKey::from_seed([0xc3; 32]);
        assert_eq!(challenger.owner_public_key, challenger_key.public_key());
        let provider_a_key = ValidatorKey::from_seed([0xa1; 32]);
        let provider_b_key = ValidatorKey::from_seed([0xb2; 32]);
        assert_eq!(fixture.provider_a.owner_public_key, provider_a_key.public_key());
        assert_eq!(fixture.provider_b.owner_public_key, provider_b_key.public_key());

        let mut provider_bonds = ProviderBondView::from_records([
            (fixture.provider_a.bond_outpoint, fixture.provider_a.clone()),
            (fixture.provider_b.bond_outpoint, fixture.provider_b.clone()),
            (challenger.bond_outpoint, challenger.clone()),
        ]);
        let registered_state = state.clone();
        let mut response_branch_undos = Vec::new();
        for (offset, obligation_id) in obligation_ids.iter().copied().enumerate() {
            let obligation = state.obligations[&obligation_id].clone();
            let opened_daa_score = 400 + offset as u64;
            let challenge = build_signed_da_challenge(
                NETWORK_ID,
                obligation_id,
                CURRENT_EPOCH,
                opened_daa_score,
                policy.response_window_daa,
                challenger.bond_outpoint,
                &challenger_key,
                h(0xe0 + offset as u8),
            )
            .unwrap();
            let (challenge_subnetwork, challenge_payload) = encode_da_challenge(&challenge).unwrap();
            assert_eq!(validate_palw_overlay_payload(challenge_subnetwork, &challenge_payload), Ok(()));
            let challenge_context = PalwDaApplyContext {
                network_id: NETWORK_ID,
                current_daa_score: opened_daa_score,
                current_epoch: CURRENT_EPOCH,
                policy: &policy,
                provider_bonds: &provider_bonds,
            };
            let (mutation, challenge_undo) =
                apply_palw_da_effect(&mut state, PalwDaOverlayEffect::Challenge(challenge.clone()), &challenge_context).unwrap();
            assert!(mutation.is_none());

            let (provider, owner_key) = if obligation.provider_bond == fixture.provider_a.bond_outpoint {
                (&fixture.provider_a, &provider_a_key)
            } else {
                (&fixture.provider_b, &provider_b_key)
            };
            let response = build_signed_da_response(
                NETWORK_ID,
                challenge.challenge_id(),
                provider.bond_outpoint,
                owner_key,
                &fixture.bytes,
                obligation.chunk_index,
            )
            .unwrap();
            assert_eq!(response.chunk_proof.object_version, PALW_RECEIPT_DA_OBJECT_VERSION_V2);
            let (response_subnetwork, response_payload) = encode_da_response(&response).unwrap();
            assert_eq!(validate_palw_overlay_payload(response_subnetwork, &response_payload), Ok(()));
            let response_context = PalwDaApplyContext {
                network_id: NETWORK_ID,
                current_daa_score: opened_daa_score + 1,
                current_epoch: CURRENT_EPOCH,
                policy: &policy,
                provider_bonds: &provider_bonds,
            };
            if offset == 0 {
                let mut wrong_domain = response.clone();
                wrong_domain.chunk_proof =
                    palw_receipt_da_chunk_proof(PALW_RECEIPT_DA_OBJECT_VERSION_V1, &fixture.bytes, obligation.chunk_index).unwrap();
                wrong_domain.signature = owner_key
                    .sign_with_context(wrong_domain.signing_hash().as_byte_slice(), PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT)
                    .to_vec();
                let wrong_payload = encode_da_response(&wrong_domain).unwrap().1;
                assert_eq!(validate_palw_overlay_payload(response_subnetwork, &wrong_payload), Ok(()));
                let before_wrong_domain = state.clone();
                assert!(apply_palw_da_effect(&mut state, PalwDaOverlayEffect::Response(wrong_domain), &response_context,).is_err());
                assert_eq!(state, before_wrong_domain, "a proof from the wrong object-version domain is rejected atomically");
            }
            let (mutation, response_undo) =
                apply_palw_da_effect(&mut state, PalwDaOverlayEffect::Response(response), &response_context).unwrap();
            assert!(mutation.is_none());
            assert!(matches!(state.obligations[&obligation_id].status, PalwDaObligationStatusV1::Satisfied(_)));
            response_branch_undos.push((challenge_undo, response_undo));
        }
        assert!(state.certificate_allowed(&fixture.leaf.batch_id));

        // A selected-chain detach replays the exact DA undos in reverse transaction/block order.
        for (challenge_undo, response_undo) in response_branch_undos.into_iter().rev() {
            state.revert(response_undo);
            state.revert(challenge_undo);
        }
        assert_eq!(state, registered_state);

        // Exercise the competing timeout branch with the same real challenge signature. The slash
        // registry mutation and DA snapshot undo must both return byte-for-byte to their parent view.
        let obligation = state.obligations[&obligation_ids[0]].clone();
        let opened_daa_score = 500;
        let challenge = build_signed_da_challenge(
            NETWORK_ID,
            obligation.obligation_id,
            CURRENT_EPOCH,
            opened_daa_score,
            policy.response_window_daa,
            challenger.bond_outpoint,
            &challenger_key,
            h(0xf0),
        )
        .unwrap();
        let challenge_context = PalwDaApplyContext {
            network_id: NETWORK_ID,
            current_daa_score: opened_daa_score,
            current_epoch: CURRENT_EPOCH,
            policy: &policy,
            provider_bonds: &provider_bonds,
        };
        let (_, challenge_undo) =
            apply_palw_da_effect(&mut state, PalwDaOverlayEffect::Challenge(challenge.clone()), &challenge_context).unwrap();
        let challenged_state = state.clone();
        let evidence = build_da_timeout_evidence(NETWORK_ID, challenge.challenge_id(), obligation.provider_bond);
        let (timeout_subnetwork, timeout_payload) = encode_da_timeout(&evidence).unwrap();
        assert_eq!(validate_palw_overlay_payload(timeout_subnetwork, &timeout_payload), Ok(()));
        let timeout_context = PalwDaApplyContext {
            network_id: NETWORK_ID,
            current_daa_score: challenge.response_deadline_daa_score + 1,
            current_epoch: CURRENT_EPOCH,
            policy: &policy,
            provider_bonds: &provider_bonds,
        };
        let (mutation, timeout_undo) =
            apply_palw_da_effect(&mut state, PalwDaOverlayEffect::Timeout(evidence), &timeout_context).unwrap();
        let mutation = mutation.expect("timeout emits one objective provider slash");
        assert_eq!(mutation, PalwProviderBondMutation::Slash(obligation.provider_bond, challenge.response_deadline_daa_score + 1));
        state.record_block_slash(obligation.provider_bond).unwrap();
        provider_bonds.apply(std::slice::from_ref(&mutation));
        assert_eq!(
            effective_provider_bond_status(
                provider_bonds.get(&obligation.provider_bond).unwrap(),
                challenge.response_deadline_daa_score + 1
            ),
            PalwProviderBondStatus::Slashed
        );
        provider_bonds.revert(std::slice::from_ref(&mutation));
        assert_eq!(provider_bonds.get(&obligation.provider_bond).unwrap().slashed_at_daa_score, None);
        state.revert(timeout_undo);
        assert_eq!(state, challenged_state);
        state.revert(challenge_undo);
        assert_eq!(state, registered_state);
        state.revert(registration_undo);
        assert_eq!(state, PalwDaStateV1::default());
    }

    #[test]
    fn receipt_da_v2_outer_schema_golden_matches_qwen_lifecycle_exporter() {
        let fixture = fixture();
        let operator_wire = decode_canonical_palw_receipt_da_object_v2_wire(&fixture.bytes).unwrap();
        assert_eq!(operator_wire.network_id, fixture.object.network_id);
        assert_eq!(operator_wire.provider_a_bond, fixture.object.provider_a_bond);
        assert_eq!(operator_wire.provider_b_bond, fixture.object.provider_b_bond);
        assert_eq!(palw_receipt_da_object_v2_wire_bytes(&operator_wire).unwrap(), fixture.bytes);
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../../../mil/palw/test-data/receipt_da_object_v2_golden_v1.json")).unwrap();
        assert_eq!(golden["schema"], "misaka.palw.receipt-da-object-v2-golden.v1");
        assert_eq!(golden["private_match_commitment"], format!("{}", fixture.object.matched_pair_id));
        let commitment = admit(&fixture).unwrap();
        assert_eq!(commitment.object_version, PALW_RECEIPT_DA_OBJECT_VERSION_V2);
        assert_eq!(u64::from(commitment.object_len), golden["object_len"].as_u64().unwrap());
        assert_eq!(commitment.object_len as usize, fixture.bytes.len());
        assert_eq!(format!("{}", commitment.root), golden["root"].as_str().unwrap());
    }

    mod search_snapshot_admission {
        use super::super::verify_palw_search_snapshot_for_admission;
        use kaspa_consensus_core::palw::da::{PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1, PalwDaAdmissionError};
        use kaspa_consensus_core::palw::search_snapshot::{
            PALW_SEARCH_SNAPSHOT_MAX_FUTURE_DAA_SLACK, PALW_SEARCH_SNAPSHOT_VERSION_V1, PalwSearchMediaTypeV1,
            PalwSearchOutcomeV1, PalwSearchProviderPolicyV1, PalwSearchResultV1, PalwSearchSnapshotV1, normalize_query_v1,
        };
        use kaspa_hashes::Hash64;

        const NETWORK_ID: u32 = 111;
        const SINK_DAA: u64 = 10_000;

        fn genesis() -> Hash64 {
            Hash64::from_bytes([0x42; 64])
        }

        fn sha256(bytes: &[u8]) -> [u8; 32] {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            hasher.finalize().into()
        }

        fn snapshot() -> PalwSearchSnapshotV1 {
            let original_query = "自家発行 LLM 検証".to_string();
            let normalized_query = normalize_query_v1(&original_query);
            PalwSearchSnapshotV1 {
                version: PALW_SEARCH_SNAPSHOT_VERSION_V1,
                network_id: NETWORK_ID,
                genesis_hash: genesis(),
                ruleset_id: "palw-search-v1".into(),
                assignment_id: Hash64::from_bytes([0; 64]),
                original_query_sha256: sha256(original_query.as_bytes()),
                normalized_query_sha256: sha256(normalized_query.as_bytes()),
                original_query,
                normalized_query,
                provider: PalwSearchProviderPolicyV1 {
                    provider_id: "searxng".into(),
                    policy_id: Hash64::from_bytes([0x99; 64]),
                    region: "jp".into(),
                    language: "ja-JP".into(),
                    safe_search: 1,
                },
                retrieval_unix_millis: 1_784_800_000_000,
                retrieval_daa_score: SINK_DAA,
                freshness_deadline_millis: 1_784_800_600_000,
                outcome: PalwSearchOutcomeV1::Ok,
                results: vec![PalwSearchResultV1 {
                    rank: 1,
                    media_type: PalwSearchMediaTypeV1::Web,
                    title: "検証側リプレイ".into(),
                    url: "https://example.org/replay".into(),
                    snippet: "同じ結果".into(),
                }],
                bodies: vec![],
            }
        }

        #[test]
        fn valid_snapshot_is_admitted_with_version3_commitment() {
            let snapshot = snapshot();
            let bytes = snapshot.encode().unwrap();
            let (decoded, commitment, digest) =
                verify_palw_search_snapshot_for_admission(NETWORK_ID, genesis(), SINK_DAA, &bytes).unwrap();
            assert_eq!(decoded, snapshot);
            assert_eq!(commitment.object_version, PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1);
            assert_eq!(commitment.object_len as usize, bytes.len());
            assert_eq!(digest, snapshot.digest().unwrap());
        }

        #[test]
        fn wrong_network_genesis_or_future_daa_is_rejected() {
            let bytes = snapshot().encode().unwrap();
            assert!(matches!(
                verify_palw_search_snapshot_for_admission(NETWORK_ID + 1, genesis(), SINK_DAA, &bytes),
                Err(PalwDaAdmissionError::InvalidObject(_))
            ));
            assert!(matches!(
                verify_palw_search_snapshot_for_admission(NETWORK_ID, Hash64::from_bytes([0x43; 64]), SINK_DAA, &bytes),
                Err(PalwDaAdmissionError::InvalidObject(_))
            ));
            let mut future = snapshot();
            future.retrieval_daa_score = SINK_DAA + PALW_SEARCH_SNAPSHOT_MAX_FUTURE_DAA_SLACK + 1;
            let future_bytes = future.encode().unwrap();
            assert!(matches!(
                verify_palw_search_snapshot_for_admission(NETWORK_ID, genesis(), SINK_DAA, &future_bytes),
                Err(PalwDaAdmissionError::InvalidObject(_))
            ));
            // Exactly at the slack boundary is admitted.
            let mut boundary = snapshot();
            boundary.retrieval_daa_score = SINK_DAA + PALW_SEARCH_SNAPSHOT_MAX_FUTURE_DAA_SLACK;
            let boundary_bytes = boundary.encode().unwrap();
            assert!(verify_palw_search_snapshot_for_admission(NETWORK_ID, genesis(), SINK_DAA, &boundary_bytes).is_ok());
        }

        #[test]
        fn assignment_resolution_binds_signature_id_and_pins() {
            use super::super::resolve_palw_search_assignment;
            use kaspa_consensus_core::palw::search_snapshot::{
                PALW_SEARCH_ASSIGNMENT_MLDSA87_CONTEXT, PALW_SEARCH_ASSIGNMENT_VERSION_V1, PalwSearchAssignmentV1,
            };
            use libcrux_ml_dsa::ml_dsa_87 as mldsa;
            let keypair = mldsa::generate_key_pair([0x21; 32]);
            let public_key = keypair.verification_key.as_ref().to_vec();
            let mut assignment = PalwSearchAssignmentV1 {
                version: PALW_SEARCH_ASSIGNMENT_VERSION_V1,
                network_id: NETWORK_ID,
                genesis_hash: genesis(),
                ruleset_id: "palw-search-v1".into(),
                normalized_query: "自家発行 LLM 検証".into(),
                provider: snapshot().provider,
                max_results: 4,
                freshness_window_millis: 600_000,
                valid_from_daa_score: SINK_DAA - 100,
                valid_until_daa_score: SINK_DAA + 100,
                scheduler_public_key: public_key,
                signature: vec![],
            };
            let signing = assignment.signing_bytes().unwrap();
            assignment.signature =
                mldsa::sign(&keypair.signing_key, &signing, PALW_SEARCH_ASSIGNMENT_MLDSA87_CONTEXT, [0; 32])
                    .expect("deterministic assignment signing")
                    .as_ref()
                    .to_vec();
            let assignment_bytes = assignment.encode().unwrap();

            let mut bound = snapshot();
            bound.assignment_id = assignment.assignment_id().unwrap();
            bound.freshness_deadline_millis = bound.retrieval_unix_millis + assignment.freshness_window_millis;
            let verify = crate::processes::palw_da::consensus_mldsa_verify;

            // Resolves with the real consensus ML-DSA verifier.
            let resolved = resolve_palw_search_assignment(&bound, Some(&assignment_bytes), verify).unwrap();
            assert_eq!(resolved.unwrap().assignment_id().unwrap(), bound.assignment_id);

            // Referenced-but-missing, unreferenced-but-supplied, tampered signature, broken pin.
            assert!(resolve_palw_search_assignment(&bound, None, verify).is_err());
            assert!(resolve_palw_search_assignment(&snapshot(), Some(&assignment_bytes), verify).is_err());
            let mut bad_signature = assignment.clone();
            bad_signature.signature[0] ^= 1;
            assert!(resolve_palw_search_assignment(&bound, Some(&bad_signature.encode().unwrap()), verify).is_err());
            let mut off_window = bound.clone();
            off_window.retrieval_daa_score = SINK_DAA + 101;
            assert!(resolve_palw_search_assignment(&off_window, Some(&assignment_bytes), verify).is_err());
            // Diagnostic zero-sentinel path stays available and assignment-free.
            assert!(resolve_palw_search_assignment(&snapshot(), None, verify).unwrap().is_none());
        }

        #[test]
        fn tampered_or_foreign_bytes_are_rejected() {
            let bytes = snapshot().encode().unwrap();
            for index in [0, 2, bytes.len() / 2, bytes.len() - 1] {
                let mut mutated = bytes.clone();
                mutated[index] ^= 0x01;
                let verdict = verify_palw_search_snapshot_for_admission(NETWORK_ID, genesis(), SINK_DAA, &mutated);
                if let Ok((decoded, _, digest)) = verdict {
                    // A mutation that still decodes must change the digest (bound to every byte).
                    assert_ne!(digest, snapshot().digest().unwrap(), "mutation at {index} kept the digest");
                    assert_ne!(decoded, snapshot());
                }
            }
            assert!(matches!(
                verify_palw_search_snapshot_for_admission(NETWORK_ID, genesis(), SINK_DAA, &[]),
                Err(PalwDaAdmissionError::InvalidObject(_))
            ));
        }
    }
}
