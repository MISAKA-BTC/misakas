//! DA-01 producer and operator constructors.
//!
//! This is the LLM/miner side of the consensus-core receipt object. It signs both owner→session
//! authorizations and both execution receipts, derives the private match commitment from the full
//! signed receipt hashes, and returns the exact bytes/root that must be bound into the public leaf.
//! Object-v1 deliberately signs ZERO in each embedded legacy `receipt_da_root`; the outer leaf root
//! commits to the complete object and avoids an impossible signature/root self-reference.

use kaspa_consensus_core::palw::da::{
    PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT, PALW_DA_CHALLENGE_VERSION_V1, PALW_DA_MAX_SESSION_EPOCHS,
    PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT, PALW_DA_RESPONSE_VERSION_V1, PALW_DA_TIMEOUT_EVIDENCE_VERSION_V1,
    PALW_PROVIDER_SESSION_AUTH_VERSION_V1, PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT, PALW_RECEIPT_DA_OBJECT_VERSION_V2,
    PalwDaChallengeV1, PalwDaError, PalwDaResponseV1, PalwDaTimeoutEvidenceV1, PalwProviderSessionAuthorizationV1,
    PalwReceiptDaCommitmentV1, palw_receipt_da_chunk_proof, palw_receipt_da_commitment, palw_receipt_da_object_version,
};
use kaspa_consensus_core::{palw::PalwPublicLeafV1, tx::TransactionOutpoint};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw::receipt_v3::{
    ComputeReceiptV3, ImplementationTelemetryV3, MLDSA87_ALGORITHM_ID, MatchProjectionV2, PALW_RECEIPT_V3_MLDSA87_CONTEXT,
    RECEIPT_V3_VERSION, ReceiptV3Expectations, ReceiptV3SubmissionRef, SignedEnvelopeV3, VerifyAndMatchReceiptV3Error,
    credential_id_from_verifying_key, execution_nullifier_v3, verify_and_match_receipts_v3,
};
use thiserror::Error;

pub const DA_CHALLENGE_SUBNETWORK_BYTE: u8 = 0x3a;
pub const DA_RESPONSE_SUBNETWORK_BYTE: u8 = 0x3b;
pub const DA_TIMEOUT_SUBNETWORK_BYTE: u8 = 0x3c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDaReceiptSemantics {
    pub job_nullifier: Hash64,
    pub job_set_commitment: Hash64,
    pub model_profile_id: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_id: u16,
    pub quantum_count: u16,
    pub output_commitment: Hash64,
    pub canonical_gemm_trace_root: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub route_root: Hash64,
    pub state_root: Hash64,
    pub canonical_compute_units: u64,
    pub token_count: u64,
    pub stop_reason: u8,
    pub completed_at_epoch: u64,
}

pub struct PalwDaProviderSigner<'a> {
    pub provider_bond: TransactionOutpoint,
    pub owner_key: &'a ValidatorKey,
    pub session_key: &'a ValidatorKey,
    pub valid_from_epoch: u64,
    pub valid_until_epoch: u64,
    pub authorization_nonce: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDaProducerArtifact {
    pub object: PalwReceiptDaObjectV2Wire,
    pub object_bytes: Vec<u8>,
    pub commitment: PalwReceiptDaCommitmentV1,
    pub private_match_commitment: Hash64,
}

/// Lightweight Object-v2 wire mirror for offline producer/operator tooling.
///
/// Full semantic admission remains in `kaspa-consensus`; this type only gives tools that already
/// depend on the miner crate a strict Borsh decoder without pulling the node, RocksDB, or virtual
/// processor into the operator binary. A consensus cross-crate test pins this layout byte-for-byte
/// against the authoritative `processes::palw_da::PalwReceiptDaObjectV2` fixture.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwReceiptDaObjectV2Wire {
    pub version: u16,
    pub network_id: Hash64,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    pub provider_a_bond: TransactionOutpoint,
    pub provider_b_bond: TransactionOutpoint,
    pub receipt_a: ComputeReceiptV3,
    pub envelope_a: SignedEnvelopeV3,
    pub receipt_b: ComputeReceiptV3,
    pub envelope_b: SignedEnvelopeV3,
    pub session_authorization_a: PalwProviderSessionAuthorizationV1,
    pub session_authorization_b: PalwProviderSessionAuthorizationV1,
    pub matched_pair_id: Hash64,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PalwDaProducerError {
    #[error("the two DA providers must use distinct bond outpoints")]
    DuplicateProvider,
    #[error("provider session authorization is out of range or exceeds 64 epochs")]
    SessionEpochRange,
    #[error("provider session authorization nonce must be nonzero")]
    ZeroAuthorizationNonce,
    #[error("the candidate public leaf does not match the signed DA object")]
    LeafMismatch,
    #[error("DA response deadline overflows u64")]
    DeadlineOverflow,
    #[error(transparent)]
    Core(#[from] PalwDaError),
    #[error("borsh serialization failed")]
    Encode,
    #[error("receipt v3 verification/match failed: {0:?}")]
    ReceiptV3(VerifyAndMatchReceiptV3Error),
}

pub fn palw_receipt_da_object_v2_wire_bytes(object: &PalwReceiptDaObjectV2Wire) -> Result<Vec<u8>, PalwDaProducerError> {
    if object.version != PALW_RECEIPT_DA_OBJECT_VERSION_V2 {
        return Err(PalwDaError::UnsupportedVersion(object.version).into());
    }
    let bytes = borsh::to_vec(object).map_err(|_| PalwDaProducerError::Encode)?;
    palw_receipt_da_commitment(object.version, &bytes)?;
    Ok(bytes)
}

pub fn decode_canonical_palw_receipt_da_object_v2_wire(bytes: &[u8]) -> Result<PalwReceiptDaObjectV2Wire, PalwDaProducerError> {
    let object = borsh::from_slice::<PalwReceiptDaObjectV2Wire>(bytes).map_err(|_| PalwDaError::NonCanonicalObject)?;
    if palw_receipt_da_object_v2_wire_bytes(&object)? != bytes {
        return Err(PalwDaError::NonCanonicalObject.into());
    }
    Ok(object)
}

fn signed_session_authorization(
    network_id: u32,
    provider: &PalwDaProviderSigner<'_>,
    completed_at_epoch: u64,
) -> Result<PalwProviderSessionAuthorizationV1, PalwDaProducerError> {
    if provider.valid_from_epoch > provider.valid_until_epoch
        || provider.valid_until_epoch.saturating_sub(provider.valid_from_epoch) > PALW_DA_MAX_SESSION_EPOCHS
        || !(provider.valid_from_epoch..=provider.valid_until_epoch).contains(&completed_at_epoch)
    {
        return Err(PalwDaProducerError::SessionEpochRange);
    }
    if provider.authorization_nonce == Hash64::default() {
        return Err(PalwDaProducerError::ZeroAuthorizationNonce);
    }
    let mut authorization = PalwProviderSessionAuthorizationV1 {
        version: PALW_PROVIDER_SESSION_AUTH_VERSION_V1,
        network_id,
        provider_bond: provider.provider_bond,
        owner_public_key: provider.owner_key.public_key().to_vec(),
        session_public_key: provider.session_key.public_key().to_vec(),
        valid_from_epoch: provider.valid_from_epoch,
        valid_until_epoch: provider.valid_until_epoch,
        authorization_nonce: provider.authorization_nonce,
        signature: vec![],
    };
    authorization.signature = provider
        .owner_key
        .sign_with_context(authorization.signing_hash().as_byte_slice(), PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT)
        .to_vec();
    Ok(authorization)
}

fn first_32(hash: &Hash64) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&hash.as_byte_slice()[..32]);
    bytes
}

fn signed_receipt_v3(
    genesis_network_id: Hash64,
    provider: &PalwDaProviderSigner<'_>,
    fields: PalwDaReceiptSemantics,
    replica_slot: u8,
) -> (ComputeReceiptV3, SignedEnvelopeV3) {
    let credential = credential_id_from_verifying_key(provider.session_key.public_key());
    let projection = MatchProjectionV2 {
        compute_set_id: fields.job_set_commitment,
        job_challenge: fields.job_nullifier,
        output_commitment: fields.output_commitment,
        schedule_root: fields.operation_schedule_commitment,
        execution_root: fields.canonical_gemm_trace_root,
        route_root: fields.route_root,
        state_root: fields.state_root,
        canonical_compute_units: fields.canonical_compute_units,
        token_count: fields.token_count,
        stop_reason: fields.stop_reason,
    };
    let issued_epoch = fields.completed_at_epoch;
    let expires_epoch = provider.valid_until_epoch;
    let receipt = ComputeReceiptV3 {
        receipt_version: RECEIPT_V3_VERSION,
        network_id: genesis_network_id,
        execution_nullifier: execution_nullifier_v3(
            &genesis_network_id,
            &projection.compute_set_id,
            &projection.job_challenge,
            &credential,
            replica_slot,
            issued_epoch,
        ),
        projection,
        telemetry: ImplementationTelemetryV3 {
            runtime_class_id: first_32(&fields.runtime_class_id),
            runtime_manifest_hash: first_32(&fields.model_profile_id),
        },
        worker_credential_id: credential,
        replica_slot,
        issued_epoch,
        expires_epoch,
    };
    let body_digest = receipt.signing_digest();
    let envelope = SignedEnvelopeV3 {
        body_digest,
        algorithm: MLDSA87_ALGORITHM_ID,
        signer_credential_id: credential,
        signature: provider.session_key.sign_with_context(body_digest.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT).to_vec(),
    };
    (receipt, envelope)
}

#[allow(clippy::too_many_arguments)]
pub fn build_signed_receipt_da_object(
    network_id: u32,
    genesis_network_id: Hash64,
    batch_id: Hash64,
    leaf_index: u32,
    fields: PalwDaReceiptSemantics,
    provider_a: &PalwDaProviderSigner<'_>,
    provider_b: &PalwDaProviderSigner<'_>,
) -> Result<PalwDaProducerArtifact, PalwDaProducerError> {
    if provider_a.provider_bond == provider_b.provider_bond {
        return Err(PalwDaProducerError::DuplicateProvider);
    }
    let session_authorization_a = signed_session_authorization(network_id, provider_a, fields.completed_at_epoch)?;
    let session_authorization_b = signed_session_authorization(network_id, provider_b, fields.completed_at_epoch)?;
    let (receipt_a, envelope_a) = signed_receipt_v3(genesis_network_id, provider_a, fields, 0);
    let (receipt_b, envelope_b) = signed_receipt_v3(genesis_network_id, provider_b, fields, 1);
    let expected = |slot, provider: &PalwDaProviderSigner<'_>| ReceiptV3Expectations {
        network_id: genesis_network_id,
        compute_set_id: fields.job_set_commitment,
        job_challenge: fields.job_nullifier,
        replica_slot: slot,
        issued_epoch: fields.completed_at_epoch,
        expires_epoch: provider.valid_until_epoch,
        current_epoch: fields.completed_at_epoch,
        registered_credential_id: credential_id_from_verifying_key(provider.session_key.public_key()),
    };
    let matched = verify_and_match_receipts_v3(
        ReceiptV3SubmissionRef {
            receipt: &receipt_a,
            envelope: &envelope_a,
            verifying_key: provider_a.session_key.public_key(),
            expected: &expected(0, provider_a),
        },
        ReceiptV3SubmissionRef {
            receipt: &receipt_b,
            envelope: &envelope_b,
            verifying_key: provider_b.session_key.public_key(),
            expected: &expected(1, provider_b),
        },
    )
    .map_err(PalwDaProducerError::ReceiptV3)?;
    let match_commitment = matched.pair_id();
    let object = PalwReceiptDaObjectV2Wire {
        version: PALW_RECEIPT_DA_OBJECT_VERSION_V2,
        network_id: genesis_network_id,
        batch_id,
        leaf_index,
        provider_a_bond: provider_a.provider_bond,
        provider_b_bond: provider_b.provider_bond,
        receipt_a,
        envelope_a,
        receipt_b,
        envelope_b,
        session_authorization_a,
        session_authorization_b,
        matched_pair_id: match_commitment,
    };
    let object_bytes = palw_receipt_da_object_v2_wire_bytes(&object)?;
    let commitment = palw_receipt_da_commitment(object.version, &object_bytes)?;
    Ok(PalwDaProducerArtifact { object, object_bytes, commitment, private_match_commitment: match_commitment })
}

impl PalwDaProducerArtifact {
    /// Bind the candidate leaf before manifest/chunk publication. This is intentionally consuming:
    /// callers cannot accidentally keep publishing the pre-DA zero-root candidate as the final leaf.
    pub fn bind_leaf(&self, mut leaf: PalwPublicLeafV1) -> Result<PalwPublicLeafV1, PalwDaProducerError> {
        let receipt = &self.object.receipt_a;
        if leaf.batch_id != self.object.batch_id
            || leaf.leaf_index != self.object.leaf_index
            || leaf.provider_a_bond != self.object.provider_a_bond
            || leaf.provider_b_bond != self.object.provider_b_bond
            || leaf.job_nullifier != receipt.projection.job_challenge
        {
            return Err(PalwDaProducerError::LeafMismatch);
        }
        leaf.private_match_commitment = self.private_match_commitment;
        leaf.receipt_da_object_version = self.commitment.object_version;
        leaf.receipt_da_root = self.commitment.root;
        leaf.receipt_da_object_len = self.commitment.object_len;
        leaf.receipt_da_chunk_count = self.commitment.chunk_count;
        leaf.receipt_v3_compute_set_id = receipt.projection.compute_set_id;
        leaf.receipt_v3_job_challenge = receipt.projection.job_challenge;
        leaf.receipt_v3_issued_epoch = receipt.issued_epoch;
        leaf.receipt_v3_expires_epoch = receipt.expires_epoch;
        Ok(leaf)
    }
}

pub fn build_signed_da_challenge(
    network_id: u32,
    obligation_id: Hash64,
    challenge_epoch: u64,
    opened_daa_score: u64,
    response_window_daa: u64,
    challenger_bond: TransactionOutpoint,
    challenger_owner_key: &ValidatorKey,
    challenge_nonce: Hash64,
) -> Result<PalwDaChallengeV1, PalwDaProducerError> {
    let response_deadline_daa_score =
        opened_daa_score.checked_add(response_window_daa).ok_or(PalwDaProducerError::DeadlineOverflow)?;
    let mut challenge = PalwDaChallengeV1 {
        version: PALW_DA_CHALLENGE_VERSION_V1,
        network_id,
        obligation_id,
        challenge_epoch,
        opened_daa_score,
        response_deadline_daa_score,
        challenger_bond,
        challenger_owner_public_key: challenger_owner_key.public_key().to_vec(),
        challenge_nonce,
        signature: vec![],
    };
    challenge.signature = challenger_owner_key
        .sign_with_context(challenge.signing_hash().as_byte_slice(), PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT)
        .to_vec();
    Ok(challenge)
}

pub fn build_signed_da_response(
    network_id: u32,
    challenge_id: Hash64,
    provider_bond: TransactionOutpoint,
    provider_owner_key: &ValidatorKey,
    object_bytes: &[u8],
    chunk_index: u16,
) -> Result<PalwDaResponseV1, PalwDaProducerError> {
    let object_version = palw_receipt_da_object_version(object_bytes)?;
    let chunk_proof = palw_receipt_da_chunk_proof(object_version, object_bytes, chunk_index)?;
    let mut response = PalwDaResponseV1 {
        version: PALW_DA_RESPONSE_VERSION_V1,
        network_id,
        challenge_id,
        provider_bond,
        provider_owner_public_key: provider_owner_key.public_key().to_vec(),
        chunk_proof,
        signature: vec![],
    };
    response.signature =
        provider_owner_key.sign_with_context(response.signing_hash().as_byte_slice(), PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT).to_vec();
    Ok(response)
}

pub fn build_da_timeout_evidence(
    network_id: u32,
    challenge_id: Hash64,
    provider_bond: TransactionOutpoint,
) -> PalwDaTimeoutEvidenceV1 {
    PalwDaTimeoutEvidenceV1 { version: PALW_DA_TIMEOUT_EVIDENCE_VERSION_V1, network_id, challenge_id, provider_bond }
}

pub fn encode_da_challenge(challenge: &PalwDaChallengeV1) -> Result<(u8, Vec<u8>), PalwDaProducerError> {
    Ok((DA_CHALLENGE_SUBNETWORK_BYTE, borsh::to_vec(challenge).map_err(|_| PalwDaProducerError::Encode)?))
}

pub fn encode_da_response(response: &PalwDaResponseV1) -> Result<(u8, Vec<u8>), PalwDaProducerError> {
    Ok((DA_RESPONSE_SUBNETWORK_BYTE, borsh::to_vec(response).map_err(|_| PalwDaProducerError::Encode)?))
}

pub fn encode_da_timeout(evidence: &PalwDaTimeoutEvidenceV1) -> Result<(u8, Vec<u8>), PalwDaProducerError> {
    Ok((DA_TIMEOUT_SUBNETWORK_BYTE, borsh::to_vec(evidence).map_err(|_| PalwDaProducerError::Encode)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw::da::{PALW_DA_MAX_ONCHAIN_RESPONSE_BYTES, verify_palw_receipt_da_chunk};
    use kaspa_txscript::verify_mldsa87_with_context;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn fields() -> PalwDaReceiptSemantics {
        PalwDaReceiptSemantics {
            job_nullifier: h(1),
            job_set_commitment: h(2),
            model_profile_id: h(3),
            runtime_class_id: h(4),
            shape_id: 7,
            quantum_count: 2,
            output_commitment: h(5),
            canonical_gemm_trace_root: h(6),
            operation_schedule_commitment: h(7),
            route_root: h(8),
            state_root: h(9),
            canonical_compute_units: 123_456,
            token_count: 65,
            stop_reason: 1,
            completed_at_epoch: 9,
        }
    }

    #[test]
    fn producer_signs_canonical_object_and_operational_payloads() {
        let owner_a = ValidatorKey::from_seed([1; 32]);
        let session_a = ValidatorKey::from_seed([2; 32]);
        let owner_b = ValidatorKey::from_seed([3; 32]);
        let session_b = ValidatorKey::from_seed([4; 32]);
        let provider_a = PalwDaProviderSigner {
            provider_bond: TransactionOutpoint::new(h(0xa1), 0),
            owner_key: &owner_a,
            session_key: &session_a,
            valid_from_epoch: 8,
            valid_until_epoch: 10,
            authorization_nonce: h(0xb1),
        };
        let provider_b = PalwDaProviderSigner {
            provider_bond: TransactionOutpoint::new(h(0xa2), 0),
            owner_key: &owner_b,
            session_key: &session_b,
            valid_from_epoch: 8,
            valid_until_epoch: 10,
            authorization_nonce: h(0xb2),
        };
        let artifact = build_signed_receipt_da_object(111, h(0xc0), h(0xc1), 3, fields(), &provider_a, &provider_b).unwrap();
        assert_eq!(artifact.object.version, PALW_RECEIPT_DA_OBJECT_VERSION_V2);
        assert_eq!(artifact.object.receipt_a.network_id, h(0xc0));
        assert_eq!(artifact.object.receipt_b.network_id, h(0xc0));
        assert!(matches!(
            verify_mldsa87_with_context(
                &artifact.object.session_authorization_a.session_public_key,
                artifact.object.receipt_a.signing_digest().as_byte_slice(),
                &artifact.object.envelope_a.signature,
                PALW_RECEIPT_V3_MLDSA87_CONTEXT,
            ),
            Ok(true)
        ));
        assert!(matches!(
            verify_mldsa87_with_context(
                &artifact.object.session_authorization_a.owner_public_key,
                artifact.object.session_authorization_a.signing_hash().as_byte_slice(),
                &artifact.object.session_authorization_a.signature,
                PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
            ),
            Ok(true)
        ));

        let challenge = build_signed_da_challenge(111, h(0xd1), 9, 1_000, 200, provider_b.provider_bond, &owner_a, h(0xd2)).unwrap();
        assert!(matches!(
            verify_mldsa87_with_context(
                &challenge.challenger_owner_public_key,
                challenge.signing_hash().as_byte_slice(),
                &challenge.signature,
                PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT,
            ),
            Ok(true)
        ));
        let response =
            build_signed_da_response(111, challenge.challenge_id(), provider_a.provider_bond, &owner_a, &artifact.object_bytes, 0)
                .unwrap();
        assert_eq!(response.chunk_proof.object_version, PALW_RECEIPT_DA_OBJECT_VERSION_V2);
        assert!(matches!(
            verify_mldsa87_with_context(
                &response.provider_owner_public_key,
                response.signing_hash().as_byte_slice(),
                &response.signature,
                PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT,
            ),
            Ok(true)
        ));
        verify_palw_receipt_da_chunk(&artifact.commitment.root, &response.chunk_proof).unwrap();
        let (_, response_bytes) = encode_da_response(&response).unwrap();
        assert!(response_bytes.len() <= PALW_DA_MAX_ONCHAIN_RESPONSE_BYTES);
        assert_eq!(encode_da_challenge(&challenge).unwrap().0, 0x3a);
        assert_eq!(
            encode_da_timeout(&build_da_timeout_evidence(111, challenge.challenge_id(), provider_a.provider_bond)).unwrap().0,
            0x3c
        );
    }
}
