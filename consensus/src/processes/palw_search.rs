//! Search-availability consensus crypto/context seam (subnet bytes 0x3d-0x3f).
//!
//! Core owns the canonical wire codec and the deterministic state machine
//! (`consensus-core palw/search_snapshot.rs`); this module supplies the real ML-DSA-87 verifier
//! and a fork-local provider-bond view, so no global/tip-relative read can decide a challenge on
//! one branch using another branch's state. Scheduler authorization is the bonded registry — the
//! same authority anchor DA challengers use — never a node-local allowlist: dispatch outcomes must
//! be identical on every node for the same accepted transaction.

use kaspa_consensus_core::palw::search_snapshot::{
    PalwSearchAvailabilityStateV1, PalwSearchAvailabilityUndoV1, PalwSearchChallengeTxV1, PalwSearchResponseTxV1,
    PalwSearchSnapshotError, PalwSearchTimeoutTxV1,
};
use kaspa_consensus_core::palw::{ProviderBondView, is_provider_bond_active_at};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

use super::palw_da::consensus_mldsa_verify;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwSearchOverlayEffect {
    Challenge(Box<PalwSearchChallengeTxV1>),
    Response(Box<PalwSearchResponseTxV1>),
    Timeout(Box<PalwSearchTimeoutTxV1>),
}

impl PalwSearchOverlayEffect {
    /// The DA object root this effect targets.
    pub fn object_root(&self) -> Hash64 {
        match self {
            Self::Challenge(tx) => tx.object_root,
            Self::Response(tx) => tx.object_root,
            Self::Timeout(tx) => tx.object_root,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwSearchProcessError {
    UnhandledSubnet(u8),
    Decode,
    Core(PalwSearchSnapshotError),
}

impl From<PalwSearchSnapshotError> for PalwSearchProcessError {
    fn from(value: PalwSearchSnapshotError) -> Self {
        Self::Core(value)
    }
}

/// Parse one accepted 0x3d-0x3f payload with the canonical fail-closed codec.
pub fn parse_palw_search_effect(subnetwork_byte: u8, payload: &[u8]) -> Result<PalwSearchOverlayEffect, PalwSearchProcessError> {
    match subnetwork_byte {
        0x3d => PalwSearchChallengeTxV1::decode_strict(payload).map(|tx| PalwSearchOverlayEffect::Challenge(Box::new(tx))),
        0x3e => PalwSearchResponseTxV1::decode_strict(payload).map(|tx| PalwSearchOverlayEffect::Response(Box::new(tx))),
        0x3f => PalwSearchTimeoutTxV1::decode_strict(payload).map(|tx| PalwSearchOverlayEffect::Timeout(Box::new(tx))),
        byte => return Err(PalwSearchProcessError::UnhandledSubnet(byte)),
    }
    .map_err(|_| PalwSearchProcessError::Decode)
}

pub struct PalwSearchApplyContext<'a> {
    pub network_id: u32,
    pub genesis_hash: Hash64,
    pub current_daa_score: u64,
    /// Fork-local provider bonds exactly as-of the block's selected parent: both the challenger /
    /// reporter authorization anchor and the bonded scheduler registry for registrations.
    pub provider_bonds: &'a ProviderBondView,
}

/// Apply one already-isolation-valid search transaction to a fork-local state. Timeout is the only
/// event that emits a provider-registry mutation (the slashed scheduler bond). Undos come back in
/// application order; the staging caller reverts in reverse if it must unwind.
pub fn apply_palw_search_effect(
    state: &mut PalwSearchAvailabilityStateV1,
    effect: &PalwSearchOverlayEffect,
    context: &PalwSearchApplyContext<'_>,
) -> Result<(Option<TransactionOutpoint>, Vec<PalwSearchAvailabilityUndoV1>), PalwSearchProcessError> {
    let bond_owner_is_active = |outpoint: &TransactionOutpoint, key: &[u8]| {
        context
            .provider_bonds
            .get(outpoint)
            .is_some_and(|record| record.owner_public_key == key && is_provider_bond_active_at(record, context.current_daa_score))
    };
    match effect {
        PalwSearchOverlayEffect::Challenge(tx) => {
            let undos = state.apply_challenge_tx(
                tx,
                context.network_id,
                &context.genesis_hash,
                context.current_daa_score,
                bond_owner_is_active,
                consensus_mldsa_verify,
                |wanted: &TransactionOutpoint| context.provider_bonds.get(wanted).cloned(),
            )?;
            Ok((None, undos))
        }
        PalwSearchOverlayEffect::Response(tx) => {
            let undo = state.apply_response_tx(tx, context.network_id, context.current_daa_score)?;
            Ok((None, vec![undo]))
        }
        PalwSearchOverlayEffect::Timeout(tx) => {
            let (slashed_scheduler_bond, undo) = state.apply_timeout_tx(
                tx,
                context.network_id,
                context.current_daa_score,
                bond_owner_is_active,
                consensus_mldsa_verify,
            )?;
            Ok((Some(slashed_scheduler_bond), vec![undo]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw::PalwProviderBondRecord;
    use kaspa_consensus_core::palw::da::{PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1, palw_receipt_da_chunk_proof};
    use kaspa_consensus_core::palw::search_snapshot::{
        PALW_SEARCH_ANCHOR_MLDSA87_CONTEXT, PALW_SEARCH_ASSIGNMENT_MLDSA87_CONTEXT, PALW_SEARCH_ASSIGNMENT_VERSION_V1,
        PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT, PALW_SEARCH_CHALLENGE_RESPONSE_WINDOW_DAA, PALW_SEARCH_SNAPSHOT_VERSION_V1,
        PALW_SEARCH_TIMEOUT_MLDSA87_CONTEXT, PALW_SEARCH_TX_VERSION_V1, PalwSearchAssignmentV1, PalwSearchJobSpecV1,
        PalwSearchObligationStatusV1, PalwSearchOutcomeV1, PalwSearchProviderPolicyV1, PalwSearchSnapshotAnchorV1,
        PalwSearchSnapshotV1, PalwSignedSearchAnchorV1, normalize_query_v1, scheduler_key_id,
    };
    use kaspa_consensus_core::palw::validate_palw_overlay_payload;
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;
    use sha2::{Digest, Sha256};

    const NETWORK_ID: u32 = 111;

    fn genesis() -> Hash64 {
        Hash64::from_bytes([7; 64])
    }

    fn sha256(bytes: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher.finalize().into()
    }

    fn sign(keypair: &mldsa::MLDSA87KeyPair, message: &[u8], context: &[u8]) -> Vec<u8> {
        mldsa::sign(&keypair.signing_key, message, context, [0; 32]).unwrap().as_ref().to_vec()
    }

    fn bond_record(outpoint: TransactionOutpoint, owner_public_key: Vec<u8>) -> PalwProviderBondRecord {
        PalwProviderBondRecord {
            version: 1,
            bond_outpoint: outpoint,
            owner_pubkey_hash: Hash64::default(),
            owner_public_key,
            operator_group_id: Hash64::default(),
            runtime_classes: Vec::new(),
            capacity_by_shape: Vec::new(),
            reward_key_root: Hash64::default(),
            amount_sompi: 1_000_000,
            activation_daa_score: 1_000,
            created_daa_score: 1_000,
            unbond_delay_epochs: 4,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        }
    }

    fn signed_assignment(scheduler: &mldsa::MLDSA87KeyPair, scheduler_bond: TransactionOutpoint) -> PalwSearchAssignmentV1 {
        let mut assignment = PalwSearchAssignmentV1 {
            version: PALW_SEARCH_ASSIGNMENT_VERSION_V1,
            network_id: NETWORK_ID,
            genesis_hash: genesis(),
            ruleset_id: "palw-search-v1".into(),
            normalized_query: normalize_query_v1("量子コンピュータ  とは"),
            provider: PalwSearchProviderPolicyV1 {
                provider_id: "searxng".into(),
                policy_id: Hash64::from_bytes([9; 64]),
                region: "jp".into(),
                language: "ja-JP".into(),
                safe_search: 1,
            },
            max_results: 8,
            freshness_window_millis: 600_000,
            valid_from_daa_score: 9_000,
            valid_until_daa_score: 30_000,
            scheduler_bond,
            scheduler_public_key: scheduler.verification_key.as_ref().to_vec(),
            signature: Vec::new(),
        };
        assignment.signature = sign(scheduler, &assignment.signing_bytes().unwrap(), PALW_SEARCH_ASSIGNMENT_MLDSA87_CONTEXT);
        assignment.verify_signature(consensus_mldsa_verify).unwrap();
        assignment
    }

    fn snapshot_for(assignment: &PalwSearchAssignmentV1) -> PalwSearchSnapshotV1 {
        let original_query = "量子コンピュータ  とは".to_string();
        let normalized_query = normalize_query_v1(&original_query);
        let snapshot = PalwSearchSnapshotV1 {
            version: PALW_SEARCH_SNAPSHOT_VERSION_V1,
            network_id: NETWORK_ID,
            genesis_hash: genesis(),
            ruleset_id: assignment.ruleset_id.clone(),
            assignment_id: assignment.assignment_id().unwrap(),
            original_query_sha256: sha256(original_query.as_bytes()),
            normalized_query_sha256: sha256(normalized_query.as_bytes()),
            original_query,
            normalized_query,
            provider: assignment.provider.clone(),
            retrieval_unix_millis: 1_784_800_000_000,
            retrieval_daa_score: 10_000,
            freshness_deadline_millis: 1_784_800_600_000,
            outcome: PalwSearchOutcomeV1::EmptyResults,
            results: Vec::new(),
            bodies: Vec::new(),
        };
        snapshot.validate().unwrap();
        snapshot
    }

    /// The complete production-shaped on-chain vertical through the dispatch seam, with REAL
    /// ML-DSA-87 keys and the REAL consensus verifier at every gate:
    /// scheduler-signed assignment + anchor (JobSpec) → bond-authorized registering challenge
    /// (register + challenge atomically) → self-authorizing chunk-proof response → plain
    /// re-challenge → post-deadline timeout naming the SCHEDULER bond as the slash target →
    /// reverse-order revert to the empty state. Every payload also passes the 0x3d-0x3f isolation
    /// validators exactly as an accepted transaction must.
    #[test]
    fn search_dispatch_vertical_runs_with_real_mldsa_and_bonded_registry() {
        let scheduler = mldsa::generate_key_pair([0xA1; 32]);
        let challenger = mldsa::generate_key_pair([0xB2; 32]);
        let reporter = mldsa::generate_key_pair([0xC3; 32]);
        let scheduler_bond = TransactionOutpoint::new(Hash64::from_bytes([0xA5; 64]), 0);
        let challenger_bond = TransactionOutpoint::new(Hash64::from_bytes([0xB5; 64]), 0);
        let reporter_bond = TransactionOutpoint::new(Hash64::from_bytes([0xC5; 64]), 0);

        let assignment = signed_assignment(&scheduler, scheduler_bond);
        let snapshot = snapshot_for(&assignment);
        let object_bytes = snapshot.encode().unwrap();
        let commitment = snapshot.da_commitment().unwrap();
        let anchor = PalwSearchSnapshotAnchorV1 {
            assignment_id: assignment.assignment_id().unwrap(),
            snapshot_digest: snapshot.digest().unwrap(),
            object_root: commitment.root,
            object_len: commitment.object_len,
            chunk_count: commitment.chunk_count,
            availability_deadline_daa_score: 20_000,
        };
        let registration = PalwSearchJobSpecV1 {
            signed_anchor: PalwSignedSearchAnchorV1 {
                anchor,
                scheduler_public_key: assignment.scheduler_public_key.clone(),
                signature: sign(&scheduler, anchor.signing_hash().as_byte_slice(), PALW_SEARCH_ANCHOR_MLDSA87_CONTEXT),
            },
            assignment: assignment.clone(),
        };
        registration.verify(consensus_mldsa_verify).unwrap();

        let view = ProviderBondView::from_records([
            (scheduler_bond, bond_record(scheduler_bond, scheduler.verification_key.as_ref().to_vec())),
            (challenger_bond, bond_record(challenger_bond, challenger.verification_key.as_ref().to_vec())),
            (reporter_bond, bond_record(reporter_bond, reporter.verification_key.as_ref().to_vec())),
        ]);
        let context = |daa: u64| PalwSearchApplyContext {
            network_id: NETWORK_ID,
            genesis_hash: genesis(),
            current_daa_score: daa,
            provider_bonds: &view,
        };

        // Registering challenge: wire round-trip, isolation admission, parse, atomic apply.
        let mut registering = kaspa_consensus_core::palw::search_snapshot::PalwSearchChallengeTxV1 {
            version: PALW_SEARCH_TX_VERSION_V1,
            network_id: NETWORK_ID,
            object_root: anchor.object_root,
            chunk_index: 0,
            challenger_bond,
            challenger_public_key: challenger.verification_key.as_ref().to_vec(),
            registration: Some(registration.clone()),
            signature: Vec::new(),
        };
        registering.signature = sign(&challenger, registering.signing_hash().as_byte_slice(), PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT);
        let registering_bytes = registering.encode().unwrap();
        validate_palw_overlay_payload(0x3d, &registering_bytes).unwrap();
        let effect = parse_palw_search_effect(0x3d, &registering_bytes).unwrap();
        assert_eq!(effect.object_root(), anchor.object_root);

        let mut state = PalwSearchAvailabilityStateV1::default();
        let mut undos = Vec::new();
        let (mutation, applied) = apply_palw_search_effect(&mut state, &effect, &context(10_100)).unwrap();
        assert!(mutation.is_none());
        assert_eq!(applied.len(), 2, "register + challenge in one atomic accepted tx");
        undos.extend(applied);
        let obligation = state.obligations[&anchor.object_root];
        assert_eq!(obligation.scheduler_bond, scheduler_bond);
        assert_eq!(obligation.scheduler_key_id, scheduler_key_id(&assignment.scheduler_public_key));
        assert!(matches!(obligation.status, PalwSearchObligationStatusV1::Challenged { chunk_index: 0, .. }));

        // Self-authorizing response with the REAL version-3 chunk proof over the anchored bytes.
        let response = kaspa_consensus_core::palw::search_snapshot::PalwSearchResponseTxV1 {
            version: PALW_SEARCH_TX_VERSION_V1,
            network_id: NETWORK_ID,
            object_root: anchor.object_root,
            proof: palw_receipt_da_chunk_proof(PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1, &object_bytes, 0).unwrap(),
        };
        let response_bytes = response.encode().unwrap();
        validate_palw_overlay_payload(0x3e, &response_bytes).unwrap();
        let effect = parse_palw_search_effect(0x3e, &response_bytes).unwrap();
        let (mutation, applied) = apply_palw_search_effect(&mut state, &effect, &context(10_300)).unwrap();
        assert!(mutation.is_none());
        undos.extend(applied);
        assert_eq!(state.obligations[&anchor.object_root].status, PalwSearchObligationStatusV1::Active);

        // Plain re-challenge (obligation now exists; no attachment).
        let mut plain = registering.clone();
        plain.registration = None;
        plain.signature = sign(&challenger, plain.signing_hash().as_byte_slice(), PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT);
        let plain_bytes = plain.encode().unwrap();
        validate_palw_overlay_payload(0x3d, &plain_bytes).unwrap();
        let effect = parse_palw_search_effect(0x3d, &plain_bytes).unwrap();
        let (_, applied) = apply_palw_search_effect(&mut state, &effect, &context(11_000)).unwrap();
        assert_eq!(applied.len(), 1);
        undos.extend(applied);

        // Post-deadline timeout: the slash names the SCHEDULER bond recorded at registration.
        let mut timeout = kaspa_consensus_core::palw::search_snapshot::PalwSearchTimeoutTxV1 {
            version: PALW_SEARCH_TX_VERSION_V1,
            network_id: NETWORK_ID,
            object_root: anchor.object_root,
            reporter_bond,
            reporter_public_key: reporter.verification_key.as_ref().to_vec(),
            signature: Vec::new(),
        };
        timeout.signature = sign(&reporter, timeout.signing_hash().as_byte_slice(), PALW_SEARCH_TIMEOUT_MLDSA87_CONTEXT);
        let timeout_bytes = timeout.encode().unwrap();
        validate_palw_overlay_payload(0x3f, &timeout_bytes).unwrap();
        let effect = parse_palw_search_effect(0x3f, &timeout_bytes).unwrap();
        let timeout_daa = 11_000 + PALW_SEARCH_CHALLENGE_RESPONSE_WINDOW_DAA + 1;
        let (mutation, applied) = apply_palw_search_effect(&mut state, &effect, &context(timeout_daa)).unwrap();
        assert_eq!(mutation, Some(scheduler_bond), "the economic slash target is the anchoring scheduler's bond");
        undos.extend(applied);
        assert!(matches!(state.obligations[&anchor.object_root].status, PalwSearchObligationStatusV1::Slashed { .. }));
        // The block-delta + void sweep the staging layer runs on this mutation:
        state.record_block_slash(scheduler_bond).unwrap();
        assert_eq!(state.block_slashed_schedulers, vec![scheduler_bond]);
        assert!(state.void_by_scheduler_bond(scheduler_bond, timeout_daa).is_empty(), "sole obligation is already slashed");

        // Reverse-order revert restores the exact empty state (the reorg contract).
        state.begin_child_block();
        for undo in undos.into_iter().rev() {
            state.revert(undo).unwrap();
        }
        assert_eq!(state, PalwSearchAvailabilityStateV1::default());
    }

    /// Every consensus gate refuses through the SAME seam the dispatcher uses, with real crypto:
    /// unknown/slashed/unbonding scheduler bond, wrong owner key, foreign network/genesis, tampered
    /// signature, inactive challenger bond — and no failed gate leaves any state residue.
    #[test]
    fn search_dispatch_gates_are_fail_closed_through_the_seam() {
        let scheduler = mldsa::generate_key_pair([0xA1; 32]);
        let challenger = mldsa::generate_key_pair([0xB2; 32]);
        let scheduler_bond = TransactionOutpoint::new(Hash64::from_bytes([0xA5; 64]), 0);
        let challenger_bond = TransactionOutpoint::new(Hash64::from_bytes([0xB5; 64]), 0);
        let assignment = signed_assignment(&scheduler, scheduler_bond);
        let snapshot = snapshot_for(&assignment);
        let commitment = snapshot.da_commitment().unwrap();
        let anchor = PalwSearchSnapshotAnchorV1 {
            assignment_id: assignment.assignment_id().unwrap(),
            snapshot_digest: snapshot.digest().unwrap(),
            object_root: commitment.root,
            object_len: commitment.object_len,
            chunk_count: commitment.chunk_count,
            availability_deadline_daa_score: 20_000,
        };
        let registration = PalwSearchJobSpecV1 {
            signed_anchor: PalwSignedSearchAnchorV1 {
                anchor,
                scheduler_public_key: assignment.scheduler_public_key.clone(),
                signature: sign(&scheduler, anchor.signing_hash().as_byte_slice(), PALW_SEARCH_ANCHOR_MLDSA87_CONTEXT),
            },
            assignment: assignment.clone(),
        };
        let mut registering = kaspa_consensus_core::palw::search_snapshot::PalwSearchChallengeTxV1 {
            version: PALW_SEARCH_TX_VERSION_V1,
            network_id: NETWORK_ID,
            object_root: anchor.object_root,
            chunk_index: 0,
            challenger_bond,
            challenger_public_key: challenger.verification_key.as_ref().to_vec(),
            registration: Some(registration),
            signature: Vec::new(),
        };
        registering.signature = sign(&challenger, registering.signing_hash().as_byte_slice(), PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT);
        let effect = parse_palw_search_effect(0x3d, &registering.encode().unwrap()).unwrap();

        let healthy_scheduler = bond_record(scheduler_bond, scheduler.verification_key.as_ref().to_vec());
        let healthy_challenger = bond_record(challenger_bond, challenger.verification_key.as_ref().to_vec());
        let refused_views = [
            // Scheduler bond unknown to the registry.
            ProviderBondView::from_records([(challenger_bond, healthy_challenger.clone())]),
            // Scheduler bond slashed.
            ProviderBondView::from_records([
                (scheduler_bond, PalwProviderBondRecord { slashed_at_daa_score: Some(9_000), ..healthy_scheduler.clone() }),
                (challenger_bond, healthy_challenger.clone()),
            ]),
            // Scheduler bond exiting (pending unbond).
            ProviderBondView::from_records([
                (scheduler_bond, PalwProviderBondRecord { unbond_request_daa_score: Some(9_000), ..healthy_scheduler.clone() }),
                (challenger_bond, healthy_challenger.clone()),
            ]),
            // Scheduler bond owned by a different key.
            ProviderBondView::from_records([
                (scheduler_bond, PalwProviderBondRecord { owner_public_key: vec![0xEE; 32], ..healthy_scheduler.clone() }),
                (challenger_bond, healthy_challenger.clone()),
            ]),
            // Challenger bond missing entirely.
            ProviderBondView::from_records([(scheduler_bond, healthy_scheduler.clone())]),
        ];
        for view in &refused_views {
            let mut state = PalwSearchAvailabilityStateV1::default();
            let context = PalwSearchApplyContext {
                network_id: NETWORK_ID,
                genesis_hash: genesis(),
                current_daa_score: 10_100,
                provider_bonds: view,
            };
            assert!(apply_palw_search_effect(&mut state, &effect, &context).is_err());
            assert_eq!(state, PalwSearchAvailabilityStateV1::default(), "refused gate must leave no residue");
        }

        let healthy_view = ProviderBondView::from_records([
            (scheduler_bond, healthy_scheduler.clone()),
            (challenger_bond, healthy_challenger.clone()),
        ]);
        // Foreign network id and foreign genesis are refused.
        let mut state = PalwSearchAvailabilityStateV1::default();
        let foreign_network = PalwSearchApplyContext {
            network_id: NETWORK_ID + 1,
            genesis_hash: genesis(),
            current_daa_score: 10_100,
            provider_bonds: &healthy_view,
        };
        assert!(apply_palw_search_effect(&mut state, &effect, &foreign_network).is_err());
        let foreign_genesis = PalwSearchApplyContext {
            network_id: NETWORK_ID,
            genesis_hash: Hash64::from_bytes([0x77; 64]),
            current_daa_score: 10_100,
            provider_bonds: &healthy_view,
        };
        assert!(apply_palw_search_effect(&mut state, &effect, &foreign_genesis).is_err());
        // A tampered challenge signature is refused by the REAL verifier.
        let mut tampered = registering.clone();
        tampered.signature[0] ^= 1;
        let tampered_effect = parse_palw_search_effect(0x3d, &tampered.encode().unwrap()).unwrap();
        let context = PalwSearchApplyContext {
            network_id: NETWORK_ID,
            genesis_hash: genesis(),
            current_daa_score: 10_100,
            provider_bonds: &healthy_view,
        };
        assert!(apply_palw_search_effect(&mut state, &tampered_effect, &context).is_err());
        // And a tampered ANCHOR signature inside the registration is refused.
        let mut bad_anchor = registering.clone();
        if let Some(registration) = bad_anchor.registration.as_mut() {
            registration.signed_anchor.signature[0] ^= 1;
        }
        bad_anchor.signature = sign(&challenger, bad_anchor.signing_hash().as_byte_slice(), PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT);
        let bad_anchor_effect = parse_palw_search_effect(0x3d, &bad_anchor.encode().unwrap()).unwrap();
        assert!(apply_palw_search_effect(&mut state, &bad_anchor_effect, &context).is_err());
        assert_eq!(state, PalwSearchAvailabilityStateV1::default());

        // Unhandled subnet bytes and undecodable payloads never become effects.
        assert!(matches!(parse_palw_search_effect(0x3c, &[]), Err(PalwSearchProcessError::UnhandledSubnet(0x3c))));
        assert!(matches!(parse_palw_search_effect(0x3d, &[0xFF; 4]), Err(PalwSearchProcessError::Decode)));
    }
}
