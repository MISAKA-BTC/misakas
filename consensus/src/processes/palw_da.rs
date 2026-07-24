//! DA-01 consensus crypto/context seam.
//!
//! Core owns canonical bytes and the deterministic state machine. This module supplies the real
//! ML-DSA-87 verifier and a fork-local provider-bond view, so no global/tip-relative read can decide
//! a challenge on one branch using another branch's state.

use borsh::BorshDeserialize;
use kaspa_consensus_core::palw::ProviderBondView;
use kaspa_consensus_core::palw::da::{
    PALW_RECEIPT_DA_OBJECT_VERSION_V2, PalwDaChallengeV1, PalwDaError, PalwDaPolicyV1, PalwDaResponseV1, PalwDaStateV1,
    PalwDaTimeoutEvidenceV1, PalwDaUndoV1, PalwProviderSessionAuthorizationV1, PalwReceiptDaObjectV1, palw_receipt_da_commitment,
    verify_palw_provider_session_authorization, verify_palw_receipt_da_object,
};
use kaspa_consensus_core::palw::{
    PalwProviderBondMutation, PalwProviderBondRecord, PalwProviderBondStatus, PalwPublicLeafV1, effective_provider_bond_status,
};
use kaspa_hashes::Hash64;
use kaspa_txscript::verify_mldsa87_with_context;
use misaka_palw::receipt_v3::{
    ComputeReceiptV3, ReceiptV3Expectations, ReceiptV3SubmissionRef, SignedEnvelopeV3, VerifyAndMatchReceiptV3Error,
    credential_id_from_verifying_key, verify_and_match_receipts_v3,
};

/// Public/value-network DA object. The receipt bodies and envelopes are exactly the node-owned
/// `misaka-palw::receipt_v3` Borsh/canonical bytes; this wrapper only binds them to the on-chain leaf,
/// provider bonds, owner-authorized receipt keys and order-independent k=2 verdict.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, BorshDeserialize)]
pub struct PalwReceiptDaObjectV2 {
    pub version: u16,
    pub network_id: Hash64,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub provider_a_bond: kaspa_consensus_core::tx::TransactionOutpoint,
    pub provider_b_bond: kaspa_consensus_core::tx::TransactionOutpoint,
    pub receipt_a: ComputeReceiptV3,
    pub envelope_a: SignedEnvelopeV3,
    pub receipt_b: ComputeReceiptV3,
    pub envelope_b: SignedEnvelopeV3,
    pub session_authorization_a: PalwProviderSessionAuthorizationV1,
    pub session_authorization_b: PalwProviderSessionAuthorizationV1,
    pub matched_pair_id: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwDaOverlayEffect {
    Challenge(PalwDaChallengeV1),
    Response(PalwDaResponseV1),
    Timeout(PalwDaTimeoutEvidenceV1),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwDaProcessError {
    UnhandledSubnet(u8),
    Decode,
    MissingBond,
    Core(PalwDaError),
    ReceiptV3(VerifyAndMatchReceiptV3Error),
}

pub fn palw_receipt_da_object_v2_bytes(object: &PalwReceiptDaObjectV2) -> Result<Vec<u8>, PalwDaProcessError> {
    if object.version != PALW_RECEIPT_DA_OBJECT_VERSION_V2 {
        return Err(PalwDaError::UnsupportedVersion(object.version).into());
    }
    let bytes = borsh::to_vec(object).map_err(|_| PalwDaProcessError::Decode)?;
    palw_receipt_da_commitment(object.version, &bytes)?;
    Ok(bytes)
}

pub fn decode_canonical_palw_receipt_da_object_v2(bytes: &[u8]) -> Result<PalwReceiptDaObjectV2, PalwDaProcessError> {
    let object = PalwReceiptDaObjectV2::try_from_slice(bytes).map_err(|_| PalwDaProcessError::Decode)?;
    if palw_receipt_da_object_v2_bytes(&object)? != bytes {
        return Err(PalwDaError::NonCanonicalObject.into());
    }
    Ok(object)
}

#[allow(clippy::too_many_arguments)]
pub fn verify_receipt_da_object_v2_with_consensus_crypto(
    network_id: u32,
    genesis_network_id: Hash64,
    leaf: &PalwPublicLeafV1,
    provider_a: &PalwProviderBondRecord,
    provider_b: &PalwProviderBondRecord,
    pov_daa_score: u64,
    current_epoch: u64,
    object_bytes: &[u8],
) -> Result<PalwReceiptDaObjectV2, PalwDaProcessError> {
    let object = decode_canonical_palw_receipt_da_object_v2(object_bytes)?;
    if leaf.receipt_da_object_version != PALW_RECEIPT_DA_OBJECT_VERSION_V2
        || object.network_id != genesis_network_id
        || object.batch_id != leaf.batch_id
        || object.leaf_index != leaf.leaf_index
        || object.provider_a_bond != leaf.provider_a_bond
        || object.provider_b_bond != leaf.provider_b_bond
        || provider_a.bond_outpoint != object.provider_a_bond
        || provider_b.bond_outpoint != object.provider_b_bond
        || object.provider_a_bond == object.provider_b_bond
    {
        return Err(PalwDaError::ObjectBinding.into());
    }
    let commitment = palw_receipt_da_commitment(object.version, object_bytes)?;
    if commitment.root != leaf.receipt_da_root {
        return Err(PalwDaError::WrongProof.into());
    }
    if commitment.object_len != leaf.receipt_da_object_len || commitment.chunk_count != leaf.receipt_da_chunk_count {
        return Err(PalwDaError::ChunkMetadata.into());
    }
    if !matches!(effective_provider_bond_status(provider_a, pov_daa_score), PalwProviderBondStatus::Active)
        || !matches!(effective_provider_bond_status(provider_b, pov_daa_score), PalwProviderBondStatus::Active)
    {
        return Err(PalwDaError::ProviderBond.into());
    }
    let mut crypto = verify_signature;
    verify_palw_provider_session_authorization(
        network_id,
        object.provider_a_bond,
        &object.session_authorization_a.session_public_key,
        leaf.receipt_v3_issued_epoch,
        leaf.receipt_v3_expires_epoch,
        provider_a,
        &object.session_authorization_a,
        &mut crypto,
    )?;
    verify_palw_provider_session_authorization(
        network_id,
        object.provider_b_bond,
        &object.session_authorization_b.session_public_key,
        leaf.receipt_v3_issued_epoch,
        leaf.receipt_v3_expires_epoch,
        provider_b,
        &object.session_authorization_b,
        &mut crypto,
    )?;

    let expected = |slot, session_public_key: &[u8]| ReceiptV3Expectations {
        network_id: genesis_network_id,
        compute_set_id: leaf.receipt_v3_compute_set_id,
        job_challenge: leaf.receipt_v3_job_challenge,
        replica_slot: slot,
        issued_epoch: leaf.receipt_v3_issued_epoch,
        expires_epoch: leaf.receipt_v3_expires_epoch,
        current_epoch,
        registered_credential_id: credential_id_from_verifying_key(session_public_key),
    };
    let expected_a = expected(0, &object.session_authorization_a.session_public_key);
    let expected_b = expected(1, &object.session_authorization_b.session_public_key);
    let matched = verify_and_match_receipts_v3(
        ReceiptV3SubmissionRef {
            receipt: &object.receipt_a,
            envelope: &object.envelope_a,
            verifying_key: &object.session_authorization_a.session_public_key,
            expected: &expected_a,
        },
        ReceiptV3SubmissionRef {
            receipt: &object.receipt_b,
            envelope: &object.envelope_b,
            verifying_key: &object.session_authorization_b.session_public_key,
            expected: &expected_b,
        },
    )
    .map_err(PalwDaProcessError::ReceiptV3)?;
    if matched.pair_id() != object.matched_pair_id || matched.pair_id() != leaf.private_match_commitment {
        return Err(PalwDaError::MatchCommitment.into());
    }
    Ok(object)
}

impl From<PalwDaError> for PalwDaProcessError {
    fn from(value: PalwDaError) -> Self {
        Self::Core(value)
    }
}

pub fn parse_palw_da_effect(subnetwork_byte: u8, payload: &[u8]) -> Result<PalwDaOverlayEffect, PalwDaProcessError> {
    match subnetwork_byte {
        // 0x39 is deliberately absent: ADR-0040 reserves it for cross-fork slashing evidence.
        0x3a => PalwDaChallengeV1::try_from_slice(payload).map(PalwDaOverlayEffect::Challenge),
        0x3b => PalwDaResponseV1::try_from_slice(payload).map(PalwDaOverlayEffect::Response),
        0x3c => PalwDaTimeoutEvidenceV1::try_from_slice(payload).map(PalwDaOverlayEffect::Timeout),
        byte => return Err(PalwDaProcessError::UnhandledSubnet(byte)),
    }
    .map_err(|_| PalwDaProcessError::Decode)
}

#[inline]
fn verify_signature(public_key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    matches!(verify_mldsa87_with_context(public_key, message, signature, context), Ok(true))
}

/// Consensus ML-DSA-87 verifier in the injection shape used by the search-snapshot artifacts
/// (`(public_key, message, signature, context) -> bool`).
pub(crate) fn consensus_mldsa_verify(public_key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    verify_signature(public_key, message, signature, context)
}

#[allow(clippy::too_many_arguments)]
pub fn verify_receipt_da_object_with_consensus_crypto(
    network_id: u32,
    leaf: &PalwPublicLeafV1,
    provider_a: &PalwProviderBondRecord,
    provider_b: &PalwProviderBondRecord,
    pov_daa_score: u64,
    object_bytes: &[u8],
) -> Result<PalwReceiptDaObjectV1, PalwDaProcessError> {
    verify_palw_receipt_da_object(network_id, leaf, provider_a, provider_b, pov_daa_score, object_bytes, verify_signature)
        .map_err(Into::into)
}

pub struct PalwDaApplyContext<'a> {
    pub network_id: u32,
    pub current_daa_score: u64,
    pub current_epoch: u64,
    pub policy: &'a PalwDaPolicyV1,
    pub provider_bonds: &'a ProviderBondView,
}

/// Apply one already-isolation-valid DA transaction to a fork-local state. Timeout is the only event
/// that emits a provider-registry mutation. The returned undo is exact and must be retained by a
/// staging caller until its enclosing block is committed.
pub fn apply_palw_da_effect(
    state: &mut PalwDaStateV1,
    effect: PalwDaOverlayEffect,
    context: &PalwDaApplyContext<'_>,
) -> Result<(Option<PalwProviderBondMutation>, PalwDaUndoV1), PalwDaProcessError> {
    match effect {
        PalwDaOverlayEffect::Challenge(challenge) => {
            let challenger = context.provider_bonds.get(&challenge.challenger_bond).ok_or(PalwDaProcessError::MissingBond)?;
            let undo = state.apply_challenge(
                challenge,
                challenger,
                context.network_id,
                context.current_daa_score,
                context.current_epoch,
                context.policy,
                verify_signature,
            )?;
            Ok((None, undo))
        }
        PalwDaOverlayEffect::Response(response) => {
            let provider = context.provider_bonds.get(&response.provider_bond).ok_or(PalwDaProcessError::MissingBond)?;
            let undo = state.apply_response(response, provider, context.network_id, context.current_daa_score, verify_signature)?;
            Ok((None, undo))
        }
        PalwDaOverlayEffect::Timeout(evidence) => {
            let (mutation, undo) = state.apply_timeout_evidence(evidence, context.network_id, context.current_daa_score)?;
            Ok((Some(mutation), undo))
        }
    }
}

/// Stateless transport/object identity helper used by the P2P consumer before inserting an object
/// into a content-addressed cache.
pub fn verified_object_root(object: &PalwReceiptDaObjectV1) -> Result<(Hash64, Vec<u8>), PalwDaProcessError> {
    let bytes = kaspa_consensus_core::palw::da::palw_receipt_da_object_bytes(object)?;
    let root = kaspa_consensus_core::palw::da::palw_receipt_da_commitment(object.version, &bytes)?.root;
    Ok((root, bytes))
}
