//! Read-only PALW audit-round facts shared by consensus and operator tooling.
//!
//! A certificate producer must not reconstruct its committee or sampled leaf set from a collection of
//! loosely-related tip-global queries.  Both are consensus facts at one selected-parent coordinate and
//! one declared `audit_beacon_epoch`.  This module defines that snapshot and delegates every derivation
//! to the same PALW primitives used by certificate verification.

use std::collections::HashSet;

use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};

use crate::{
    BlockHash,
    palw::{
        PalwBatchLifecycleV1, PalwBatchManifestV1, PalwBatchStatus, PalwCredentialStake, PalwProviderBondRecord, PalwPublicLeafV1,
        ProviderBondView, palw_audit_sample_root, palw_deterministic_sample, select_weighted_auditor_committee,
    },
    tx::TransactionOutpoint,
};

/// Hard work/response bound for the operator audit-facts surface.
///
/// Certificate verification still derives against the complete point-of-view provider registry. If
/// that registry grows beyond this cap, exporting a complete selection-relevant, omission-detecting
/// snapshot is refused loudly instead of turning one public RPC request into an unbounded registry
/// scan. Raising this is a public-operator/RPC capacity decision, not something a caller may override.
pub const MAX_PALW_AUDIT_FACT_PROVIDER_RECORDS: usize = 1_024;

/// The pure committee/sample derivations for one frozen PALW audit round.
///
/// `provider_bonds` is the complete selection-relevant provider view frozen at
/// `snapshot_daa_score`, in canonical outpoint order — including pending/unbonding/slashed rows that
/// existed by the snapshot plus any later-created row named by a batch leaf. The latter exception is
/// necessary because the verifier resolves producer credential/operator exclusions from the current
/// raw view before active-set filtering. Unreferenced rows created after the snapshot cannot be active
/// there and are omitted; post-snapshot unbond/slash stamps are rolled back. This projection is stable
/// across harmless tip advance while remaining exactly selection-equivalent to the verifier's current
/// view. `selected_auditors` is the credential-aggregated, stake-weighted slate. Its stake sum is
/// deliberately the verifier's quorum denominator: one selected auditor that withholds a vote remains
/// in `selected_total_stake`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwAuditSelectionFacts {
    pub provider_bonds: Vec<PalwProviderBondRecord>,
    /// Canonically parallel to `selected_auditors`; these aggregate weights drive both the draw and quorum.
    pub selected_credential_stakes: Vec<PalwCredentialStake>,
    pub selected_auditors: Vec<PalwProviderBondRecord>,
    pub selected_total_stake: u128,
    pub auditor_set_commitment: Hash64,
    /// Selection order used by `palw_audit_sample_root`; callers must not sort this list.
    pub sampled_leaf_indices: Vec<u32>,
    pub audit_sample_root: Hash64,
}

/// Everything an operator needs to construct and independently check a certificate round at one sink.
///
/// The sink is part of the result because this is intentionally a snapshot, not a promise about a
/// later block. A submitter must refetch if the sink changes before inclusion. No method producing this
/// value writes a store or invents missing manifest/leaf provenance. `inclusion_epoch` is the proposed
/// carrier-block epoch derived at that sink. Consensus ultimately binds the certificate to the epoch of
/// the block that actually carries its transaction, not to a later block that accepts that carrier from
/// its mergeset; crossing an epoch before carriage requires refreshing and rebuilding the envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PalwAuditRoundFacts {
    pub network_id: u32,
    pub sink: BlockHash,
    pub sink_daa_score: u64,
    /// Proposed carrier-block epoch for the certificate transaction.
    pub inclusion_epoch: u64,
    pub batch_id: Hash64,
    pub manifest_hash: Hash64,
    pub manifest: PalwBatchManifestV1,
    pub lifecycle: PalwBatchLifecycleV1,
    pub leaves: Vec<PalwPublicLeafV1>,
    pub audit_beacon_epoch: u64,
    /// `R_(audit_beacon_epoch - 1)`, resolved from the sink's selected-parent history.
    pub previous_epoch_seed: Hash64,
    pub snapshot_daa_score: u64,
    pub inclusion_window_epochs: u64,
    pub committee_size: u16,
    pub sample_size: u16,
    pub quorum_num: u16,
    pub quorum_den: u16,
    pub selection: PalwAuditSelectionFacts,
}

/// Why an operator snapshot cannot be assembled at the requested sink/round.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwAuditFactsError {
    #[error("PALW is not enabled on this network")]
    Disabled,
    #[error("PALW batch {0} has no on-chain manifest")]
    BatchNotFound(Hash64),
    #[error("the sink has no PALW overlay view")]
    OverlayViewUnavailable,
    #[error("PALW batch {0} is not present in the sink overlay view")]
    BatchNotInSinkView(Hash64),
    #[error("PALW batch is not auditable in lifecycle state {0:?}")]
    BatchNotAuditable(PalwBatchStatus),
    #[error("PALW leaf ({batch_id}, {leaf_index}) is absent")]
    LeafMissing { batch_id: Hash64, leaf_index: u32 },
    #[error("audit beacon epoch {audit_epoch} is outside the batch audit interval [{registration_epoch}, {activation_epoch})")]
    AuditEpochOutOfRange { audit_epoch: u64, registration_epoch: u64, activation_epoch: u64 },
    #[error("audit beacon epoch {audit_epoch} is outside the certificate inclusion window at epoch {inclusion_epoch}")]
    OutsideInclusionWindow { audit_epoch: u64, inclusion_epoch: u64 },
    #[error("R_(audit_beacon_epoch - 1) is unavailable for audit epoch {0}")]
    AuditSeedUnavailable(u64),
    #[error("PALW provider set exceeds the audit-facts bound of {max} records")]
    ProviderSetTooLarge { max: usize },
    #[error("selected auditor bond {0} disappeared from the provider view")]
    SelectedBondMissing(TransactionOutpoint),
    #[error("PALW state-store read failed: {0}")]
    Store(String),
}

fn cmp_outpoint(a: &TransactionOutpoint, b: &TransactionOutpoint) -> std::cmp::Ordering {
    a.transaction_id.as_byte_slice().cmp(b.transaction_id.as_byte_slice()).then(a.index.cmp(&b.index))
}

/// Project the current append-only registry back to the exact selection-relevant audit snapshot.
///
/// Registry rows are never physically removed on unbond/slash, so a row created by the snapshot and
/// its status at the snapshot are reconstructible from the retained DAA stamps. A post-snapshot row is
/// inactive and irrelevant unless a batch leaf names it: producer exclusions deliberately resolve raw
/// referenced rows regardless of status, so those rows must remain to match the verifier even if an
/// operator precommitted a future provider-bond outpoint in the leaf.
pub fn project_palw_audit_provider_records(
    provider_bond_view: &ProviderBondView,
    snapshot_daa_score: u64,
    leaves: &[PalwPublicLeafV1],
) -> Vec<PalwProviderBondRecord> {
    let referenced: HashSet<_> = leaves.iter().flat_map(|leaf| [leaf.provider_a_bond, leaf.provider_b_bond]).collect();
    let mut records: Vec<_> = provider_bond_view
        .records()
        .into_iter()
        .filter(|record| record.created_daa_score <= snapshot_daa_score || referenced.contains(&record.bond_outpoint))
        .map(|mut record| {
            if record.unbond_request_daa_score.is_some_and(|daa| daa > snapshot_daa_score) {
                record.unbond_request_daa_score = None;
            }
            if record.slashed_at_daa_score.is_some_and(|daa| daa > snapshot_daa_score) {
                record.slashed_at_daa_score = None;
            }
            record
        })
        .collect();
    records.sort_by(|a, b| cmp_outpoint(&a.bond_outpoint, &b.bond_outpoint));
    records
}

/// Derive the exact committee and leaf sample that `verify_certificate_attestation` will derive.
///
/// The producer-provider exclusions intentionally inspect the raw view regardless of current status,
/// matching the verifier: if a leaf names a known provider credential/operator group, that identity may
/// not audit its own batch. Committee selection and active-set filtering then evaluate the frozen
/// `snapshot_daa_score`.
pub fn derive_palw_audit_selection(
    previous_epoch_seed: &Hash64,
    batch_id: &Hash64,
    provider_bond_view: &ProviderBondView,
    snapshot_daa_score: u64,
    leaves: &[PalwPublicLeafV1],
    committee_size: usize,
    sample_size: u32,
) -> Result<PalwAuditSelectionFacts, PalwAuditFactsError> {
    let provider_bonds = project_palw_audit_provider_records(provider_bond_view, snapshot_daa_score, leaves);
    let frozen_provider_bond_view =
        ProviderBondView::from_records(provider_bonds.iter().cloned().map(|record| (record.bond_outpoint, record)));
    let mut excluded_credentials = HashSet::new();
    let mut excluded_operator_groups = HashSet::new();
    for leaf in leaves {
        for outpoint in [&leaf.provider_a_bond, &leaf.provider_b_bond] {
            if let Some(record) = frozen_provider_bond_view.get(outpoint) {
                excluded_credentials.insert(record.owner_pubkey_hash);
                excluded_operator_groups.insert(record.operator_group_id);
            }
        }
    }

    let (selected_credential_stakes, auditor_set_commitment) = select_weighted_auditor_committee(
        previous_epoch_seed,
        batch_id,
        &frozen_provider_bond_view,
        snapshot_daa_score,
        &excluded_credentials,
        &excluded_operator_groups,
        committee_size,
    );

    let mut selected_auditors = Vec::with_capacity(selected_credential_stakes.len());
    let selected_total_stake = selected_credential_stakes.iter().fold(0u128, |total, member| total.saturating_add(member.weight));
    for member in &selected_credential_stakes {
        let outpoint = member.representative;
        let record = frozen_provider_bond_view.get(&outpoint).ok_or(PalwAuditFactsError::SelectedBondMissing(outpoint))?.clone();
        selected_auditors.push(record);
    }

    let sampled_leaf_indices = palw_deterministic_sample(previous_epoch_seed, batch_id, leaves.len() as u32, sample_size);
    let sampled_da_roots: Vec<_> = sampled_leaf_indices.iter().map(|&index| leaves[index as usize].receipt_da_root).collect();
    let audit_sample_root = palw_audit_sample_root(&sampled_da_roots);

    Ok(PalwAuditSelectionFacts {
        provider_bonds,
        selected_credential_stakes,
        selected_auditors,
        selected_total_stake,
        auditor_set_commitment,
        sampled_leaf_indices,
        audit_sample_root,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw::{PALW_PAYLOAD_VERSION_V1, PalwProviderBondStatus, effective_provider_bond_status},
        tx::{ScriptPublicKey, ScriptVec},
    };

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn op(byte: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(h(byte), 0)
    }

    fn bond(byte: u8, credential: u8, group: u8, amount: u64) -> PalwProviderBondRecord {
        PalwProviderBondRecord {
            version: PALW_PAYLOAD_VERSION_V1,
            bond_outpoint: op(byte),
            owner_pubkey_hash: h(credential),
            owner_public_key: vec![byte; 2_592],
            operator_group_id: h(group),
            runtime_classes: vec![h(0x70)],
            capacity_by_shape: vec![(1, 1)],
            reward_key_root: h(0x71),
            amount_sompi: amount,
            activation_daa_score: 10,
            created_daa_score: 10,
            unbond_delay_epochs: 6,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        }
    }

    fn leaf(index: u32, provider_a: TransactionOutpoint, provider_b: TransactionOutpoint, da: u8) -> PalwPublicLeafV1 {
        let spk = ScriptPublicKey::new(0, ScriptVec::from_slice(&[1]));
        PalwPublicLeafV1 {
            version: 1,
            batch_id: h(0x40),
            leaf_index: index,
            job_nullifier: h(0x41 + index as u8),
            ticket_nullifier_commitment: h(0x51 + index as u8),
            model_profile_id: h(0x60),
            runtime_class_id: h(0x61),
            shape_id: 1,
            quantum_count: 2,
            proof_type: 1,
            provider_a_bond: provider_a,
            provider_b_bond: provider_b,
            provider_a_reward_script: spk.clone(),
            provider_b_reward_script: spk,
            ticket_authority_pk_hash: h(0x62),
            private_match_commitment: h(0x63),
            receipt_da_object_version: 1,
            receipt_da_root: h(da),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 2,
            activation_epoch: 5,
            expiry_epoch: 9,
            leaf_bond_sompi: 1,
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: h(0x7c),
            assignment_proof_root: h(0x7d),
            dispatch_kind: 0, // BeaconAssigned (external sentinels)
        }
    }

    #[test]
    fn selection_facts_match_consensus_primitives_and_exclude_batch_providers() {
        let producer_a = bond(1, 1, 1, 100);
        let producer_b = bond(2, 2, 2, 100);
        // Two outpoints share credential 3; selection must credential-aggregate them and expose only
        // the canonical representative selected by `select_auditor_committee`.
        let auditor_3a = bond(3, 3, 3, 40);
        let auditor_3b = bond(4, 3, 3, 60);
        let auditor_4 = bond(5, 4, 4, 90);
        let future = PalwProviderBondRecord { activation_daa_score: 1_000, ..bond(6, 6, 6, 500) };
        assert_eq!(effective_provider_bond_status(&future, 100), PalwProviderBondStatus::Pending);

        let view = ProviderBondView::from_records(
            [producer_a.clone(), producer_b.clone(), auditor_3a.clone(), auditor_3b.clone(), auditor_4.clone(), future]
                .into_iter()
                .map(|record| (record.bond_outpoint, record)),
        );
        let leaves = vec![
            leaf(0, producer_a.bond_outpoint, producer_b.bond_outpoint, 0xa0),
            leaf(1, producer_a.bond_outpoint, producer_b.bond_outpoint, 0xa1),
        ];
        let facts = derive_palw_audit_selection(&h(0x99), &h(0x40), &view, 100, &leaves, 2, 2).unwrap();

        assert_eq!(facts.selected_auditors.len(), 2);
        assert!(facts.selected_auditors.iter().all(|record| record.owner_pubkey_hash == h(3) || record.owner_pubkey_hash == h(4)));
        assert!(facts.selected_auditors.iter().all(|record| record.owner_pubkey_hash != h(1) && record.owner_pubkey_hash != h(2)));
        assert_eq!(facts.sampled_leaf_indices.len(), 2);
        let expected_roots: Vec<_> = facts.sampled_leaf_indices.iter().map(|&index| leaves[index as usize].receipt_da_root).collect();
        assert_eq!(facts.audit_sample_root, palw_audit_sample_root(&expected_roots));
        assert_eq!(facts.provider_bonds.len(), 6, "the frozen view retains already-created pending rows");
        assert_eq!(facts.selected_total_stake, 190, "quorum uses credential aggregates 100 + 90, not representatives 40 + 90");
        assert!(facts.selected_credential_stakes.iter().any(|member| member.credential == h(3) && member.weight == 100));
    }

    #[test]
    fn inactive_producer_row_still_excludes_its_active_operator_sibling() {
        let inactive_producer = PalwProviderBondRecord { slashed_at_daa_score: Some(50), ..bond(1, 1, 7, 100) };
        let active_operator_sibling = bond(2, 2, 7, 100);
        let producer_b = bond(3, 3, 9, 100);
        let independent_auditor = bond(4, 4, 10, 100);
        let view = ProviderBondView::from_records(
            [inactive_producer.clone(), active_operator_sibling.clone(), producer_b.clone(), independent_auditor.clone()]
                .into_iter()
                .map(|record| (record.bond_outpoint, record)),
        );
        let leaves = vec![leaf(0, inactive_producer.bond_outpoint, producer_b.bond_outpoint, 0xa0)];
        let facts = derive_palw_audit_selection(&h(0x99), &h(0x40), &view, 100, &leaves, 4, 1).unwrap();

        assert_eq!(facts.provider_bonds.len(), 4, "the inactive producer row is part of the re-derivable snapshot");
        assert_eq!(facts.selected_auditors.len(), 1);
        assert_eq!(facts.selected_auditors[0].bond_outpoint, independent_auditor.bond_outpoint);
        assert!(
            facts.selected_auditors.iter().all(|record| record.bond_outpoint != active_operator_sibling.bond_outpoint),
            "the active same-operator sibling must remain excluded even though the referenced producer is inactive"
        );
    }

    #[test]
    fn provider_projection_rewinds_post_snapshot_mutations_and_drops_irrelevant_future_rows() {
        let pre_snapshot_unbond = PalwProviderBondRecord { unbond_request_daa_score: Some(99), ..bond(1, 1, 1, 100) };
        let post_snapshot_unbond = PalwProviderBondRecord { unbond_request_daa_score: Some(101), ..bond(2, 2, 2, 100) };
        let post_snapshot_slash = PalwProviderBondRecord { slashed_at_daa_score: Some(101), ..bond(3, 3, 3, 100) };
        let future = PalwProviderBondRecord { created_daa_score: 101, activation_daa_score: 101, ..bond(4, 4, 4, 100) };
        let view = ProviderBondView::from_records(
            [pre_snapshot_unbond, post_snapshot_unbond, post_snapshot_slash, future]
                .into_iter()
                .map(|record| (record.bond_outpoint, record)),
        );

        let projected = project_palw_audit_provider_records(&view, 100, &[]);
        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].unbond_request_daa_score, Some(99), "a pre-snapshot mutation is retained");
        assert_eq!(projected[1].unbond_request_daa_score, None, "a later unbond is rewound");
        assert_eq!(projected[2].slashed_at_daa_score, None, "a later slash is rewound");
    }

    #[test]
    fn referenced_future_provider_row_still_drives_verifier_operator_exclusion() {
        let future_producer = PalwProviderBondRecord { created_daa_score: 150, activation_daa_score: 150, ..bond(1, 1, 7, 100) };
        let active_operator_sibling = bond(2, 2, 7, 100);
        let producer_b = bond(3, 3, 9, 100);
        let independent_auditor = bond(4, 4, 10, 100);
        let view = ProviderBondView::from_records(
            [future_producer.clone(), active_operator_sibling.clone(), producer_b.clone(), independent_auditor.clone()]
                .into_iter()
                .map(|record| (record.bond_outpoint, record)),
        );
        let leaves = vec![leaf(0, future_producer.bond_outpoint, producer_b.bond_outpoint, 0xa0)];
        let facts = derive_palw_audit_selection(&h(0x99), &h(0x40), &view, 100, &leaves, 4, 1).unwrap();

        assert!(facts.provider_bonds.iter().any(|record| record.bond_outpoint == future_producer.bond_outpoint));
        assert_eq!(facts.selected_auditors.len(), 1);
        assert_eq!(facts.selected_auditors[0].bond_outpoint, independent_auditor.bond_outpoint);
        assert!(facts.selected_auditors.iter().all(|record| record.bond_outpoint != active_operator_sibling.bond_outpoint));
    }

    #[test]
    fn unreferenced_future_provider_row_does_not_change_frozen_round() {
        let producer_a = bond(1, 1, 1, 100);
        let producer_b = bond(2, 2, 2, 100);
        let auditor_a = bond(3, 3, 3, 100);
        let auditor_b = bond(4, 4, 4, 100);
        let future = PalwProviderBondRecord { created_daa_score: 150, activation_daa_score: 150, ..bond(5, 5, 5, 500) };
        let leaves = vec![leaf(0, producer_a.bond_outpoint, producer_b.bond_outpoint, 0xa0)];
        let base_records = [producer_a, producer_b, auditor_a, auditor_b];
        let base = ProviderBondView::from_records(base_records.clone().into_iter().map(|record| (record.bond_outpoint, record)));
        let advanced =
            ProviderBondView::from_records(base_records.into_iter().chain([future]).map(|record| (record.bond_outpoint, record)));

        let before = derive_palw_audit_selection(&h(0x99), &h(0x40), &base, 100, &leaves, 2, 1).unwrap();
        let after = derive_palw_audit_selection(&h(0x99), &h(0x40), &advanced, 100, &leaves, 2, 1).unwrap();
        assert_eq!(after, before);
    }
}
