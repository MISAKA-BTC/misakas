//! Offline DA-01 operator tooling. No RPC or mutable node state is touched here.

use borsh::BorshDeserialize;
use clap::Parser;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw::da::{
    PALW_DA_MAX_OBJECT_BYTES, PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT, PALW_RECEIPT_DA_OBJECT_VERSION_V1,
    PALW_RECEIPT_DA_OBJECT_VERSION_V2, palw_receipt_da_chunk_proof, palw_receipt_da_commitment, palw_receipt_da_object_bytes,
    palw_receipt_da_object_version,
};
use kaspa_consensus_core::palw::{da::PalwReceiptDaObjectV1, validate_palw_overlay_payload};
use kaspa_hashes::blake2b_512_keyed;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed, parse_stake_bond_ref};
use kaspa_txscript::verify_mldsa87_with_context;
use misaka_palw::receipt_v3::{
    ReceiptV3Expectations, ReceiptV3SubmissionRef, credential_id_from_verifying_key, verify_and_match_receipts_v3,
};
use misaka_palw_miner::da::{
    PalwDaProviderSigner, PalwDaReceiptSemantics, PalwReceiptDaObjectV2Wire, build_da_timeout_evidence, build_signed_da_challenge,
    build_signed_da_response, build_signed_receipt_da_object, decode_canonical_palw_receipt_da_object_v2_wire, encode_da_challenge,
    encode_da_response, encode_da_timeout,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{parse_hash64, write_new_payload};

#[derive(Parser, Debug)]
pub struct DaInspectArgs {
    /// Canonical Borsh `PalwReceiptDaObjectV1` or public Header-v4 `PalwReceiptDaObjectV2`.
    #[arg(long)]
    object_file: PathBuf,
    /// Optional fixed chunk index whose Merkle proof should be exported.
    #[arg(long, requires = "proof_out")]
    chunk_index: Option<u16>,
    /// New file receiving Borsh `PalwReceiptDaChunkProofV1`; requires --chunk-index.
    #[arg(long, requires = "chunk_index")]
    proof_out: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct DaChallengePayloadArgs {
    /// Consensus PALW network-domain u32 (must match the node's configured `palw_network_id`).
    #[arg(long)]
    network_id: u32,
    #[arg(long, value_parser = parse_hash64)]
    obligation_id: Hash64,
    #[arg(long)]
    challenge_epoch: u64,
    #[arg(long)]
    opened_daa_score: u64,
    #[arg(long, default_value_t = 200)]
    response_window_daa: u64,
    /// Active challenger provider bond, `txid:index`.
    #[arg(long)]
    challenger_bond: String,
    /// ML-DSA-87 seed for the challenger bond owner.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    owner_key: String,
    #[arg(long, value_parser = parse_hash64)]
    challenge_nonce: Hash64,
    /// New file receiving the canonical 0x3a payload.
    #[arg(long)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
pub struct DaResponsePayloadArgs {
    #[arg(long)]
    network_id: u32,
    #[arg(long, value_parser = parse_hash64)]
    challenge_id: Hash64,
    /// Challenged provider bond, `txid:index`.
    #[arg(long)]
    provider_bond: String,
    /// ML-DSA-87 seed for the challenged provider bond owner (not the hot session key).
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    owner_key: String,
    #[arg(long)]
    object_file: PathBuf,
    #[arg(long)]
    chunk_index: u16,
    /// New file receiving the canonical 0x3b payload.
    #[arg(long)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
pub struct DaObjectBuildArgs {
    /// Consensus PALW network-domain u32.
    #[arg(long)]
    network_id: u32,
    /// Header-v4 genesis/network identity embedded into Receipt-v3 bodies.
    #[arg(long, value_parser = parse_hash64)]
    genesis_network_id: Hash64,
    /// Batch id the object binds. Author-time leaves use the all-zero id (the manifest restamps the
    /// leaf's batch_id later; the DA object stays at the author-time id it was committed under).
    #[arg(long, value_parser = parse_hash64, default_value_t = Hash64::default())]
    batch_id: Hash64,
    #[arg(long)]
    leaf_index: u32,
    /// Provider A bond `txid:index` (must equal the leaf's provider_a_bond).
    #[arg(long)]
    provider_a_bond: String,
    /// ML-DSA-87 owner seed for provider A (signs A's session authorization).
    #[arg(long)]
    provider_a_owner_key: String,
    /// Provider B bond `txid:index` (must equal the leaf's provider_b_bond).
    #[arg(long)]
    provider_b_bond: String,
    /// ML-DSA-87 owner seed for provider B.
    #[arg(long)]
    provider_b_owner_key: String,
    /// Session validity window (epochs) covering `--completed-at-epoch`.
    #[arg(long)]
    valid_from_epoch: u64,
    #[arg(long)]
    valid_until_epoch: u64,
    #[arg(long)]
    completed_at_epoch: u64,
    // ---- leaf semantics the object must echo (bind_leaf equality; keep in sync with the leaf) ----
    #[arg(long, value_parser = parse_hash64)]
    job_nullifier: Hash64,
    #[arg(long, value_parser = parse_hash64)]
    job_set_commitment: Hash64,
    #[arg(long, value_parser = parse_hash64)]
    model_profile_id: Hash64,
    #[arg(long, value_parser = parse_hash64)]
    runtime_class_id: Hash64,
    #[arg(long)]
    shape_id: u16,
    #[arg(long)]
    quantum_count: u16,
    #[arg(long, value_parser = parse_hash64)]
    output_commitment: Hash64,
    #[arg(long, value_parser = parse_hash64)]
    canonical_gemm_trace_root: Hash64,
    #[arg(long, value_parser = parse_hash64)]
    operation_schedule_commitment: Hash64,
    /// Canonical MoE routing root (zero for dense/mock work).
    #[arg(long, value_parser = parse_hash64, default_value_t = Hash64::default())]
    route_root: Hash64,
    /// Canonical recurrent/checkpoint state root (zero when not applicable).
    #[arg(long, value_parser = parse_hash64, default_value_t = Hash64::default())]
    state_root: Hash64,
    /// Exact semantic compute units from the provider receipt. Defaults to the
    /// legacy leaf quantum count for compatibility with wiring-only batches.
    #[arg(long)]
    canonical_compute_units: Option<u64>,
    /// Total prompt + output tokens committed by Receipt v3. Defaults to the
    /// legacy leaf quantum count for compatibility with wiring-only batches.
    #[arg(long)]
    token_count: Option<u64>,
    /// Canonical Receipt-v3 stop-reason tag.
    #[arg(long, default_value_t = 0)]
    stop_reason: u8,
    /// New file receiving the canonical Borsh `PalwReceiptDaObjectV1` (the DA blob to serve).
    #[arg(long)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
pub struct DaTimeoutPayloadArgs {
    #[arg(long)]
    network_id: u32,
    #[arg(long, value_parser = parse_hash64)]
    challenge_id: Hash64,
    /// Provider bond named by the expired challenge, `txid:index`.
    #[arg(long)]
    provider_bond: String,
    /// New file receiving the canonical 0x3c payload.
    #[arg(long)]
    out: PathBuf,
}

fn read_bounded_object(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| format!("cannot stat DA object '{}': {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("DA object '{}' is not a regular file", path.display()));
    }
    let len = usize::try_from(metadata.len()).map_err(|_| "DA object length does not fit usize".to_string())?;
    if len == 0 || len > PALW_DA_MAX_OBJECT_BYTES {
        return Err(format!("DA object is {len} bytes; required range is 1..={PALW_DA_MAX_OBJECT_BYTES}"));
    }
    fs::read(path).map_err(|error| format!("cannot read DA object '{}': {error}", path.display()))
}

enum CanonicalDaObject {
    V1(PalwReceiptDaObjectV1),
    V2(PalwReceiptDaObjectV2Wire),
}

fn verify_v2_object_crypto(object: &PalwReceiptDaObjectV2Wire) -> Result<(), String> {
    for (label, bond, receipt, authorization) in [
        ("A", object.provider_a_bond, &object.receipt_a, &object.session_authorization_a),
        ("B", object.provider_b_bond, &object.receipt_b, &object.session_authorization_b),
    ] {
        if authorization.provider_bond != bond || authorization.session_public_key.is_empty() {
            return Err(format!("provider {label} session authorization does not bind the DA object's bond/session"));
        }
        if !matches!(
            verify_mldsa87_with_context(
                &authorization.owner_public_key,
                authorization.signing_hash().as_byte_slice(),
                &authorization.signature,
                PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
            ),
            Ok(true)
        ) {
            return Err(format!("provider {label} owner→session ML-DSA-87 authorization is invalid"));
        }
        if receipt.worker_credential_id != credential_id_from_verifying_key(&authorization.session_public_key) {
            return Err(format!("provider {label} receipt credential does not bind its authorized session key"));
        }
    }
    let expected = |receipt: &misaka_palw::receipt_v3::ComputeReceiptV3, session_key: &[u8]| ReceiptV3Expectations {
        network_id: object.network_id,
        compute_set_id: receipt.projection.compute_set_id,
        job_challenge: receipt.projection.job_challenge,
        replica_slot: receipt.replica_slot,
        issued_epoch: receipt.issued_epoch,
        expires_epoch: receipt.expires_epoch,
        current_epoch: receipt.issued_epoch,
        registered_credential_id: credential_id_from_verifying_key(session_key),
    };
    let expected_a = expected(&object.receipt_a, &object.session_authorization_a.session_public_key);
    let expected_b = expected(&object.receipt_b, &object.session_authorization_b.session_public_key);
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
    .map_err(|error| format!("Receipt-v3 ML-DSA-87/k=2 verification failed: {error:?}"))?;
    if matched.pair_id() != object.matched_pair_id {
        return Err("Receipt-v3 matched pair id does not equal the DA object commitment".to_string());
    }
    Ok(())
}

impl CanonicalDaObject {
    fn version(&self) -> u16 {
        match self {
            Self::V1(object) => object.version,
            Self::V2(object) => object.version,
        }
    }
}

fn decode_canonical_object(path: &Path) -> Result<(CanonicalDaObject, Vec<u8>), String> {
    let bytes = read_bounded_object(path)?;
    let version = palw_receipt_da_object_version(&bytes).map_err(|error| format!("invalid DA object: {error}"))?;
    let object = match version {
        PALW_RECEIPT_DA_OBJECT_VERSION_V1 => {
            let object =
                PalwReceiptDaObjectV1::try_from_slice(&bytes).map_err(|_| "DA object is not canonical Borsh object-v1".to_string())?;
            let canonical = palw_receipt_da_object_bytes(&object).map_err(|error| format!("invalid DA object: {error}"))?;
            if canonical != bytes {
                return Err("DA object-v1 has a non-canonical/trailing byte representation".to_string());
            }
            CanonicalDaObject::V1(object)
        }
        PALW_RECEIPT_DA_OBJECT_VERSION_V2 => {
            let object = decode_canonical_palw_receipt_da_object_v2_wire(&bytes)
                .map_err(|error| format!("DA object is not canonical Borsh object-v2: {error:?}"))?;
            CanonicalDaObject::V2(object)
        }
        _ => unreachable!("version helper admits only supported versions"),
    };
    Ok((object, bytes))
}

pub(crate) fn load_key(path: &str) -> Result<ValidatorKey, String> {
    let mut seed = load_validator_seed(path)?;
    let key = ValidatorKey::from_seed(seed);
    seed.fill(0);
    std::hint::black_box(&seed);
    Ok(key)
}

pub(crate) fn write_da_payload(path: &Path, subnetwork_byte: u8, payload: &[u8]) -> Result<(), String> {
    validate_palw_overlay_payload(subnetwork_byte, payload)
        .map_err(|error| format!("built 0x{subnetwork_byte:02x} payload failed consensus stateless validation: {error}"))?;
    write_new_payload(path, payload)
}

pub fn da_inspect(args: DaInspectArgs) -> Result<(), String> {
    let (object, bytes) = decode_canonical_object(&args.object_file)?;
    let object_version = object.version();
    let commitment =
        palw_receipt_da_commitment(object_version, &bytes).map_err(|error| format!("cannot commit DA object: {error}"))?;
    println!("object_version: {object_version}");
    println!("object_root: {}", commitment.root);
    println!("object_bytes: {}", commitment.object_len);
    println!("chunk_count: {}", commitment.chunk_count);
    match &object {
        CanonicalDaObject::V1(object) => {
            println!("network_id: {}", object.network_id);
            println!("batch_id: {}", object.batch_id);
            println!("leaf_index: {}", object.leaf_index);
            println!("provider_a_bond: {}", object.receipt_a.provider_bond);
            println!("provider_b_bond: {}", object.receipt_b.provider_bond);
            println!(
                "embedded_receipt_roots_zero: {}",
                object.receipt_a.receipt_da_root == Hash64::default() && object.receipt_b.receipt_da_root == Hash64::default()
            );
        }
        CanonicalDaObject::V2(object) => {
            verify_v2_object_crypto(object)?;
            let projection = &object.receipt_a.projection;
            println!("network_id: {}", object.network_id);
            println!("batch_id: {}", object.batch_id);
            println!("leaf_index: {}", object.leaf_index);
            println!("provider_a_bond: {}", object.provider_a_bond);
            println!("provider_b_bond: {}", object.provider_b_bond);
            println!("receipt_schema: receipt-v3");
            println!("matched_pair_id: {}", object.matched_pair_id);
            println!("receipt_v3_crypto_verified: true");
            println!("receipt_v3_compute_set_id: {}", projection.compute_set_id);
            println!("receipt_v3_job_challenge: {}", projection.job_challenge);
            println!("receipt_v3_output_commitment: {}", projection.output_commitment);
            println!("receipt_v3_schedule_root: {}", projection.schedule_root);
            println!("receipt_v3_execution_root: {}", projection.execution_root);
            println!("receipt_v3_route_root: {}", projection.route_root);
            println!("receipt_v3_state_root: {}", projection.state_root);
            println!("receipt_v3_canonical_compute_units: {}", projection.canonical_compute_units);
            println!("receipt_v3_token_count: {}", projection.token_count);
            println!("receipt_v3_stop_reason: {}", projection.stop_reason);
            println!("receipt_v3_replica_a_slot: {}", object.receipt_a.replica_slot);
            println!("receipt_v3_replica_b_slot: {}", object.receipt_b.replica_slot);
            println!("provider_a_session_valid_from_epoch: {}", object.session_authorization_a.valid_from_epoch);
            println!("provider_a_session_valid_until_epoch: {}", object.session_authorization_a.valid_until_epoch);
            println!("provider_b_session_valid_from_epoch: {}", object.session_authorization_b.valid_from_epoch);
            println!("provider_b_session_valid_until_epoch: {}", object.session_authorization_b.valid_until_epoch);
        }
    }

    if let (Some(chunk_index), Some(proof_out)) = (args.chunk_index, args.proof_out) {
        let proof = palw_receipt_da_chunk_proof(object_version, &bytes, chunk_index)
            .map_err(|error| format!("cannot build chunk proof: {error}"))?;
        let proof_bytes = borsh::to_vec(&proof).map_err(|_| "cannot encode chunk proof".to_string())?;
        write_new_payload(&proof_out, &proof_bytes)?;
        println!("proof_file: {}", proof_out.display());
        println!("proof_chunk_index: {chunk_index}");
    }
    Ok(())
}

pub fn da_challenge_payload(args: DaChallengePayloadArgs) -> Result<(), String> {
    let owner_key = load_key(&args.owner_key)?;
    let challenger_bond = parse_stake_bond_ref(&args.challenger_bond)?;
    let challenge = build_signed_da_challenge(
        args.network_id,
        args.obligation_id,
        args.challenge_epoch,
        args.opened_daa_score,
        args.response_window_daa,
        challenger_bond,
        &owner_key,
        args.challenge_nonce,
    )
    .map_err(|error| format!("cannot build DA challenge: {error}"))?;
    let (subnetwork, payload) = encode_da_challenge(&challenge).map_err(|error| error.to_string())?;
    write_da_payload(&args.out, subnetwork, &payload)?;
    println!("payload_kind: da-challenge");
    println!("subnetwork_byte: 0x{subnetwork:02x}");
    println!("challenge_id: {}", challenge.challenge_id());
    println!("response_deadline_daa_score: {}", challenge.response_deadline_daa_score);
    println!("payload_file: {}", args.out.display());
    Ok(())
}

pub fn da_response_payload(args: DaResponsePayloadArgs) -> Result<(), String> {
    let owner_key = load_key(&args.owner_key)?;
    let provider_bond = parse_stake_bond_ref(&args.provider_bond)?;
    let (_, object_bytes) = decode_canonical_object(&args.object_file)?;
    let response =
        build_signed_da_response(args.network_id, args.challenge_id, provider_bond, &owner_key, &object_bytes, args.chunk_index)
            .map_err(|error| format!("cannot build DA response: {error}"))?;
    let (subnetwork, payload) = encode_da_response(&response).map_err(|error| error.to_string())?;
    write_da_payload(&args.out, subnetwork, &payload)?;
    println!("payload_kind: da-response");
    println!("subnetwork_byte: 0x{subnetwork:02x}");
    println!("response_id: {}", response.response_id());
    println!("chunk_index: {}", response.chunk_proof.chunk_index);
    println!("payload_file: {}", args.out.display());
    Ok(())
}

/// Derive a deterministic per-provider session key from the owner key. The session key only signs
/// the receipt INSIDE the DA object; DA obligation satisfaction checks the Merkle chunk proof, not
/// the receipt signature, so a reproducible derivation (no extra key file) is sufficient and lets a
/// later re-build reproduce byte-identical object bytes.
fn derive_session_key(owner: &ValidatorKey) -> ValidatorKey {
    let digest = blake2b_512_keyed(b"palw-da-mock-session-v1", owner.public_key());
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&digest.as_bytes()[..32]);
    ValidatorKey::from_seed(seed)
}

pub fn da_object_build(args: DaObjectBuildArgs) -> Result<(), String> {
    let owner_a = load_key(&args.provider_a_owner_key)?;
    let owner_b = load_key(&args.provider_b_owner_key)?;
    let session_a = derive_session_key(&owner_a);
    let session_b = derive_session_key(&owner_b);
    let bond_a = parse_stake_bond_ref(&args.provider_a_bond)?;
    let bond_b = parse_stake_bond_ref(&args.provider_b_bond)?;

    // The authorization nonce must be nonzero; derive one deterministically per provider+leaf.
    let nonce =
        |tag: &[u8]| -> Hash64 { blake2b_512_keyed(b"palw-da-mock-auth-nonce-v1", &[tag, &args.leaf_index.to_le_bytes()].concat()) };
    let signer_a = PalwDaProviderSigner {
        provider_bond: bond_a,
        owner_key: &owner_a,
        session_key: &session_a,
        valid_from_epoch: args.valid_from_epoch,
        valid_until_epoch: args.valid_until_epoch,
        authorization_nonce: nonce(b"a"),
    };
    let signer_b = PalwDaProviderSigner {
        provider_bond: bond_b,
        owner_key: &owner_b,
        session_key: &session_b,
        valid_from_epoch: args.valid_from_epoch,
        valid_until_epoch: args.valid_until_epoch,
        authorization_nonce: nonce(b"b"),
    };
    let fields = PalwDaReceiptSemantics {
        job_nullifier: args.job_nullifier,
        job_set_commitment: args.job_set_commitment,
        model_profile_id: args.model_profile_id,
        runtime_class_id: args.runtime_class_id,
        shape_id: args.shape_id,
        quantum_count: args.quantum_count,
        output_commitment: args.output_commitment,
        canonical_gemm_trace_root: args.canonical_gemm_trace_root,
        operation_schedule_commitment: args.operation_schedule_commitment,
        route_root: args.route_root,
        state_root: args.state_root,
        canonical_compute_units: args.canonical_compute_units.unwrap_or(u64::from(args.quantum_count)),
        token_count: args.token_count.unwrap_or(u64::from(args.quantum_count)),
        stop_reason: args.stop_reason,
        completed_at_epoch: args.completed_at_epoch,
    };
    let artifact = build_signed_receipt_da_object(
        args.network_id,
        args.genesis_network_id,
        args.batch_id,
        args.leaf_index,
        fields,
        &signer_a,
        &signer_b,
    )
    .map_err(|error| format!("cannot build receipt DA object: {error}"))?;

    write_new_payload(&args.out, &artifact.object_bytes)?;
    // These are exactly the fields the leaf must carry (PalwDaProducerArtifact::bind_leaf). The
    // harness copies them into the author-time leaf so register_leaf_obligations sees a real,
    // provable DA commitment and a da-response chunk proof can satisfy the obligation.
    println!("payload_kind: da-object");
    println!("object_version: {}", artifact.commitment.object_version);
    println!("receipt_da_root: {}", artifact.commitment.root);
    println!("receipt_da_object_len: {}", artifact.commitment.object_len);
    println!("receipt_da_chunk_count: {}", artifact.commitment.chunk_count);
    println!("private_match_commitment: {}", artifact.private_match_commitment);
    println!("receipt_v3_compute_set_id: {}", artifact.object.receipt_a.projection.compute_set_id);
    println!("receipt_v3_job_challenge: {}", artifact.object.receipt_a.projection.job_challenge);
    println!("receipt_v3_issued_epoch: {}", artifact.object.receipt_a.issued_epoch);
    println!("receipt_v3_expires_epoch: {}", artifact.object.receipt_a.expires_epoch);
    println!("object_file: {}", args.out.display());
    Ok(())
}

pub fn da_timeout_payload(args: DaTimeoutPayloadArgs) -> Result<(), String> {
    let provider_bond = parse_stake_bond_ref(&args.provider_bond)?;
    let evidence = build_da_timeout_evidence(args.network_id, args.challenge_id, provider_bond);
    let (subnetwork, payload) = encode_da_timeout(&evidence).map_err(|error| error.to_string())?;
    write_da_payload(&args.out, subnetwork, &payload)?;
    println!("payload_kind: da-timeout");
    println!("subnetwork_byte: 0x{subnetwork:02x}");
    println!("evidence_id: {}", evidence.evidence_id());
    println!("payload_file: {}", args.out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use kaspa_consensus_core::palw::da::{PALW_RECEIPT_DA_OBJECT_VERSION_V2, PalwReceiptDaChunkProofV1, verify_palw_receipt_da_chunk};
    use kaspa_consensus_core::tx::TransactionOutpoint;

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn canonical_v2_object_bytes() -> Vec<u8> {
        let owner_a = ValidatorKey::from_seed([1; 32]);
        let session_a = ValidatorKey::from_seed([2; 32]);
        let owner_b = ValidatorKey::from_seed([3; 32]);
        let session_b = ValidatorKey::from_seed([4; 32]);
        let provider_a = PalwDaProviderSigner {
            provider_bond: TransactionOutpoint::new(h(30), 0),
            owner_key: &owner_a,
            session_key: &session_a,
            valid_from_epoch: 5,
            valid_until_epoch: 20,
            authorization_nonce: h(50),
        };
        let provider_b = PalwDaProviderSigner {
            provider_bond: TransactionOutpoint::new(h(31), 1),
            owner_key: &owner_b,
            session_key: &session_b,
            valid_from_epoch: 5,
            valid_until_epoch: 20,
            authorization_nonce: h(60),
        };
        let fields = PalwDaReceiptSemantics {
            job_nullifier: h(2),
            job_set_commitment: h(1),
            model_profile_id: h(11),
            runtime_class_id: h(12),
            shape_id: 7,
            quantum_count: 2,
            output_commitment: h(3),
            canonical_gemm_trace_root: h(5),
            operation_schedule_commitment: h(4),
            route_root: h(6),
            state_root: h(7),
            canonical_compute_units: 8,
            token_count: 9,
            stop_reason: 0,
            completed_at_epoch: 5,
        };
        build_signed_receipt_da_object(200, h(10), h(40), 7, fields, &provider_a, &provider_b).unwrap().object_bytes
    }

    #[test]
    fn da_operator_subcommands_have_strict_required_shapes() {
        let hash = "11".repeat(64);
        let bond = format!("{hash}:0");
        assert!(DaInspectArgs::try_parse_from(["da-inspect", "--object-file", "object.borsh"]).is_ok());
        assert!(DaInspectArgs::try_parse_from(["da-inspect", "--object-file", "object.borsh", "--chunk-index", "0"]).is_err());
        assert!(
            DaTimeoutPayloadArgs::try_parse_from([
                "da-timeout",
                "--network-id",
                "111",
                "--challenge-id",
                &hash,
                "--provider-bond",
                &bond,
                "--out",
                "timeout.borsh",
            ])
            .is_ok()
        );
    }

    #[test]
    fn da_inspect_exports_object_v2_domain_chunk_proof() {
        let temp = tempfile::tempdir().unwrap();
        let object_path = temp.path().join("object-v2.palwda");
        let proof_path = temp.path().join("chunk.proof.borsh");
        let bytes = canonical_v2_object_bytes();
        fs::write(&object_path, &bytes).unwrap();

        let (decoded, decoded_bytes) = decode_canonical_object(&object_path).unwrap();
        assert!(matches!(decoded, CanonicalDaObject::V2(_)));
        assert_eq!(decoded_bytes, bytes);
        da_inspect(DaInspectArgs { object_file: object_path, chunk_index: Some(0), proof_out: Some(proof_path.clone()) }).unwrap();

        let proof = PalwReceiptDaChunkProofV1::try_from_slice(&fs::read(proof_path).unwrap()).unwrap();
        assert_eq!(proof.object_version, PALW_RECEIPT_DA_OBJECT_VERSION_V2);
        let commitment = palw_receipt_da_commitment(PALW_RECEIPT_DA_OBJECT_VERSION_V2, &bytes).unwrap();
        verify_palw_receipt_da_chunk(&commitment.root, &proof).unwrap();
    }
}
