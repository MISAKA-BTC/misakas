//! Bridge a real, independently signed Qwen integer-inference receipt pair into
//! the testnet-200 PALW lifecycle.
//!
//! `verify-and-derive` performs all cryptographic checks before it emits any
//! ticket material:
//! * reconstruct and verify both canonical Receipt-v3 bodies and ML-DSA-87
//!   envelopes with the node-owned verifier;
//! * require distinct workers/slots and an exact k=2 projection match;
//! * require byte-identical worker-result JSON, recompute the token commitment,
//!   and bind the model/tables/engine identities;
//! * derive the private ticket nullifier from the verified pair plus the local
//!   ticket-authority seed, so the mintable ticket cannot exist independently of
//!   the real inference proof.
//!
//! The raw nullifier is written only to a new mode-0600 file and is never logged.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::str::FromStr;

use kaspa_consensus_core::palw::{PALW_AUTHORIZATION_DOMAIN, ticket_nullifier_commitment};
use kaspa_hashes::{Hash64, ZERO_HASH64, blake2b_512_keyed};
use kaspa_pq_validator_core::{TicketSecretStore, ValidatorKey, load_validator_seed};
use misaka_palw::receipt_v3::{
    ComputeReceiptV3, ImplementationTelemetryV3, MatchProjectionV2, ReceiptV3Expectations, ReceiptV3SubmissionRef, SignedEnvelopeV3,
    credential_id_from_verifying_key, output_commitment_v3, verify_and_match_receipts_v3,
};
use serde::Deserialize;
use serde_json::json;

const FILE_COMMITMENT_DOMAIN: &[u8] = b"misaka-palw-real-provider/file-v1";
const MODEL_PROFILE_DOMAIN: &[u8] = b"misaka-palw-real-provider/model-profile-v1";
const PROOF_COMMITMENT_DOMAIN: &[u8] = b"misaka-palw-real-provider/proof-v1";
const TICKET_NULLIFIER_DOMAIN: &[u8] = b"misaka-palw-real-provider/ticket-v1";

fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("palw-real-provider: error: {}", msg.as_ref());
    exit(1);
}

fn parse_flags(args: &[String]) -> HashMap<String, String> {
    let mut flags = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let Some(name) = arg.strip_prefix("--") else {
            die(format!("unexpected argument '{arg}' (expected --name value)"));
        };
        let value = args.get(i + 1).cloned().unwrap_or_else(|| die(format!("flag --{name} needs a value")));
        if flags.insert(name.to_string(), value).is_some() {
            die(format!("duplicate flag --{name}"));
        }
        i += 2;
    }
    flags
}

fn required<'a>(flags: &'a HashMap<String, String>, name: &str) -> &'a str {
    flags.get(name).map(String::as_str).unwrap_or_else(|| die(format!("missing required --{name}")))
}

fn read_regular(path: &Path, label: &str) -> Vec<u8> {
    let metadata = fs::symlink_metadata(path).unwrap_or_else(|e| die(format!("cannot stat {label} '{}': {e}", path.display())));
    if !metadata.file_type().is_file() {
        die(format!("{label} '{}' is not a regular file", path.display()));
    }
    fs::read(path).unwrap_or_else(|e| die(format!("cannot read {label} '{}': {e}", path.display())))
}

fn require_private_regular_file(path: &Path, label: &str) {
    let metadata = fs::symlink_metadata(path).unwrap_or_else(|e| die(format!("cannot stat {label} '{}': {e}", path.display())));
    if !metadata.file_type().is_file() {
        die(format!("{label} '{}' is not a regular file (symlink/device/fifo refused)", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            die(format!("{label} '{}' is group/world-accessible (mode {mode:o}); chmod 600 it", path.display()));
        }
    }
}

fn write_new(path: &Path, bytes: &[u8], private: bool) {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(if private { 0o600 } else { 0o644 });
    }
    let mut file = options.open(path).unwrap_or_else(|e| die(format!("cannot create new output '{}': {e}", path.display())));
    file.write_all(bytes).unwrap_or_else(|e| die(format!("cannot write '{}': {e}", path.display())));
    file.sync_all().unwrap_or_else(|e| die(format!("cannot fsync '{}': {e}", path.display())));
}

fn parse_hash(value: &str, field: &str) -> Hash64 {
    Hash64::from_str(value).unwrap_or_else(|e| die(format!("{field} is not a 128-hex Hash64: {e:?}")))
}

fn parse_fixed<const N: usize>(value: &str, field: &str) -> [u8; N] {
    let bytes = hex::decode(value).unwrap_or_else(|e| die(format!("{field} is not hex: {e}")));
    bytes.try_into().unwrap_or_else(|v: Vec<u8>| die(format!("{field} has {} bytes; expected {N}", v.len())))
}

fn file_commitment(bytes: &[u8]) -> Hash64 {
    blake2b_512_keyed(FILE_COMMITMENT_DOMAIN, bytes)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct ProjectionJson {
    compute_set_id: String,
    job_challenge: String,
    output_commitment: String,
    schedule_root: String,
    execution_root: String,
    route_root: String,
    state_root: String,
    canonical_compute_units: u64,
    token_count: u64,
    stop_reason: u8,
}

impl ProjectionJson {
    fn canonical(&self, label: &str) -> MatchProjectionV2 {
        MatchProjectionV2 {
            compute_set_id: parse_hash(&self.compute_set_id, &format!("{label}.compute_set_id")),
            job_challenge: parse_hash(&self.job_challenge, &format!("{label}.job_challenge")),
            output_commitment: parse_hash(&self.output_commitment, &format!("{label}.output_commitment")),
            schedule_root: parse_hash(&self.schedule_root, &format!("{label}.schedule_root")),
            execution_root: parse_hash(&self.execution_root, &format!("{label}.execution_root")),
            route_root: parse_hash(&self.route_root, &format!("{label}.route_root")),
            state_root: parse_hash(&self.state_root, &format!("{label}.state_root")),
            canonical_compute_units: self.canonical_compute_units,
            token_count: self.token_count,
            stop_reason: self.stop_reason,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct TelemetryJson {
    runtime_class_id: String,
    runtime_manifest_hash: String,
}

impl TelemetryJson {
    fn canonical(&self, label: &str) -> ImplementationTelemetryV3 {
        ImplementationTelemetryV3 {
            runtime_class_id: parse_fixed(&self.runtime_class_id, &format!("{label}.runtime_class_id")),
            runtime_manifest_hash: parse_fixed(&self.runtime_manifest_hash, &format!("{label}.runtime_manifest_hash")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EnvelopeJson {
    body_digest: String,
    algorithm: u8,
    signer_credential_id: String,
    signature: String,
}

impl EnvelopeJson {
    fn canonical(&self, label: &str) -> SignedEnvelopeV3 {
        SignedEnvelopeV3 {
            body_digest: parse_hash(&self.body_digest, &format!("{label}.body_digest")),
            algorithm: self.algorithm,
            signer_credential_id: parse_hash(&self.signer_credential_id, &format!("{label}.signer_credential_id")),
            signature: hex::decode(&self.signature).unwrap_or_else(|e| die(format!("{label}.signature is not hex: {e}"))),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ArtifactsJson {
    engine_blake2b256: String,
    model_blake2b256: String,
    ruleset_id: String,
    tables_blake2b256: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ReceiptJson {
    schema: String,
    class: String,
    receipt_version: u16,
    network_id: String,
    execution_nullifier: String,
    projection: ProjectionJson,
    telemetry: TelemetryJson,
    worker_credential_id: String,
    replica_slot: u8,
    issued_epoch: u64,
    expires_epoch: u64,
    envelope: EnvelopeJson,
    verifying_key: String,
    receipt_id: String,
    worker_label: String,
    prompt_tokens: u64,
    output_tokens: u64,
    engine_seconds: f64,
    timestamp_millis: u64,
    artifacts: ArtifactsJson,
}

struct VerifiedReceipt {
    json: ReceiptJson,
    body: ComputeReceiptV3,
    envelope: SignedEnvelopeV3,
    verifying_key: Vec<u8>,
}

fn parse_receipt(bytes: &[u8], label: &str, expected_slot: u8) -> VerifiedReceipt {
    let json: ReceiptJson = serde_json::from_slice(bytes).unwrap_or_else(|e| die(format!("{label} is not valid receipt JSON: {e}")));
    if json.schema != "palw.integer-receipt/v3-envelope" || json.class != "canonical_integer_v0" {
        die(format!("{label} has unsupported schema/class: {}/{}", json.schema, json.class));
    }
    if json.replica_slot != expected_slot {
        die(format!("{label} replica_slot={} but expected {expected_slot}", json.replica_slot));
    }
    if !json.engine_seconds.is_finite() || json.engine_seconds <= 0.0 {
        die(format!("{label}.engine_seconds must prove a positive finite execution duration"));
    }
    let body = ComputeReceiptV3 {
        receipt_version: json.receipt_version,
        network_id: parse_hash(&json.network_id, &format!("{label}.network_id")),
        projection: json.projection.canonical(&format!("{label}.projection")),
        telemetry: json.telemetry.canonical(&format!("{label}.telemetry")),
        worker_credential_id: parse_hash(&json.worker_credential_id, &format!("{label}.worker_credential_id")),
        replica_slot: json.replica_slot,
        execution_nullifier: parse_hash(&json.execution_nullifier, &format!("{label}.execution_nullifier")),
        issued_epoch: json.issued_epoch,
        expires_epoch: json.expires_epoch,
    };
    let envelope = json.envelope.canonical(&format!("{label}.envelope"));
    let verifying_key = hex::decode(&json.verifying_key).unwrap_or_else(|e| die(format!("{label}.verifying_key is not hex: {e}")));
    let claimed_receipt_id = parse_hash(&json.receipt_id, &format!("{label}.receipt_id"));
    if body.receipt_id() != claimed_receipt_id {
        die(format!("{label}.receipt_id does not match the canonical body"));
    }
    VerifiedReceipt { json, body, envelope, verifying_key }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct WorkerResultJson {
    canonical_compute_units: u64,
    compute_set_id: String,
    execution_root: String,
    prompt_token_count: u64,
    route_root: String,
    schedule_root: String,
    state_root: String,
    stop_reason: u8,
    telemetry: TelemetryJson,
    tokens: Vec<u32>,
}

fn result_projection(result: &WorkerResultJson, challenge: Hash64) -> MatchProjectionV2 {
    MatchProjectionV2 {
        compute_set_id: parse_hash(&result.compute_set_id, "result.compute_set_id"),
        job_challenge: challenge,
        output_commitment: output_commitment_v3(&result.tokens, &challenge),
        schedule_root: parse_hash(&result.schedule_root, "result.schedule_root"),
        execution_root: parse_hash(&result.execution_root, "result.execution_root"),
        route_root: parse_hash(&result.route_root, "result.route_root"),
        state_root: parse_hash(&result.state_root, "result.state_root"),
        canonical_compute_units: result.canonical_compute_units,
        token_count: result.prompt_token_count.saturating_add(result.tokens.len() as u64),
        stop_reason: result.stop_reason,
    }
}

fn authority_pk_hash(seed: [u8; 32]) -> Hash64 {
    let key = ValidatorKey::from_seed(seed);
    blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, key.public_key())
}

fn model_profile_id(artifacts: &ArtifactsJson) -> Hash64 {
    let model = parse_fixed::<32>(&artifacts.model_blake2b256, "artifacts.model_blake2b256");
    let tables = parse_fixed::<32>(&artifacts.tables_blake2b256, "artifacts.tables_blake2b256");
    let engine = parse_fixed::<32>(&artifacts.engine_blake2b256, "artifacts.engine_blake2b256");
    let rules = artifacts.ruleset_id.as_bytes();
    let mut preimage = Vec::with_capacity(4 + rules.len() + 96);
    preimage.extend_from_slice(&(rules.len() as u32).to_le_bytes());
    preimage.extend_from_slice(rules);
    preimage.extend_from_slice(&model);
    preimage.extend_from_slice(&tables);
    preimage.extend_from_slice(&engine);
    blake2b_512_keyed(MODEL_PROFILE_DOMAIN, &preimage)
}

fn cmd_verify_and_derive(args: &[String]) {
    let flags = parse_flags(args);
    let receipt_a_path = PathBuf::from(required(&flags, "receipt-a"));
    let receipt_b_path = PathBuf::from(required(&flags, "receipt-b"));
    let result_a_path = PathBuf::from(required(&flags, "result-a"));
    let result_b_path = PathBuf::from(required(&flags, "result-b"));
    let authority_path = PathBuf::from(required(&flags, "authority-key"));
    let nullifier_out = PathBuf::from(required(&flags, "nullifier-out"));
    let proof_out = PathBuf::from(required(&flags, "proof-out"));

    let receipt_a_bytes = read_regular(&receipt_a_path, "--receipt-a");
    let receipt_b_bytes = read_regular(&receipt_b_path, "--receipt-b");
    let result_a_bytes = read_regular(&result_a_path, "--result-a");
    let result_b_bytes = read_regular(&result_b_path, "--result-b");
    if result_a_bytes != result_b_bytes {
        die("the two worker-result files are not byte-identical");
    }

    let a = parse_receipt(&receipt_a_bytes, "receipt-a", 0);
    let b = parse_receipt(&receipt_b_bytes, "receipt-b", 1);
    if a.json.artifacts != b.json.artifacts {
        die("receipt A/B artifact identities differ");
    }
    if a.body.telemetry != b.body.telemetry {
        die("receipt A/B runtime telemetry differs");
    }

    let expected = |receipt: &VerifiedReceipt| ReceiptV3Expectations {
        network_id: receipt.body.network_id,
        compute_set_id: receipt.body.projection.compute_set_id,
        job_challenge: receipt.body.projection.job_challenge,
        replica_slot: receipt.body.replica_slot,
        issued_epoch: receipt.body.issued_epoch,
        expires_epoch: receipt.body.expires_epoch,
        current_epoch: receipt.body.issued_epoch,
        registered_credential_id: credential_id_from_verifying_key(&receipt.verifying_key),
    };
    let expected_a = expected(&a);
    let expected_b = expected(&b);
    let matched = verify_and_match_receipts_v3(
        ReceiptV3SubmissionRef { receipt: &a.body, envelope: &a.envelope, verifying_key: &a.verifying_key, expected: &expected_a },
        ReceiptV3SubmissionRef { receipt: &b.body, envelope: &b.envelope, verifying_key: &b.verifying_key, expected: &expected_b },
    )
    .unwrap_or_else(|e| die(format!("Receipt-v3 k=2 cryptographic verification failed: {e:?}")));

    let result: WorkerResultJson =
        serde_json::from_slice(&result_a_bytes).unwrap_or_else(|e| die(format!("worker-result JSON is invalid: {e}")));
    if result.telemetry.canonical("result.telemetry") != a.body.telemetry {
        die("worker-result telemetry does not match the signed receipts");
    }
    let recomputed_projection = result_projection(&result, a.body.projection.job_challenge);
    if let Some(field) = recomputed_projection.first_mismatch(&a.body.projection) {
        die(format!("worker-result does not reproduce signed projection field '{field}'"));
    }
    if result.prompt_token_count != a.json.prompt_tokens
        || a.json.prompt_tokens.saturating_add(a.json.output_tokens) != a.body.projection.token_count
        || b.json.prompt_tokens != a.json.prompt_tokens
        || b.json.output_tokens != a.json.output_tokens
    {
        die("prompt/output token accounting does not match the signed Receipt-v3 token_count");
    }

    require_private_regular_file(&authority_path, "--authority-key");
    let mut seed = load_validator_seed(authority_path.to_str().unwrap_or_else(|| die("--authority-key path is not valid UTF-8")))
        .unwrap_or_else(|e| die(format!("cannot load ticket authority seed '{}': {e}", authority_path.display())));
    let authority_hash = authority_pk_hash(seed);
    let model_profile = model_profile_id(&a.json.artifacts);
    let receipt_a_file_commitment = file_commitment(&receipt_a_bytes);
    let receipt_b_file_commitment = file_commitment(&receipt_b_bytes);
    let result_file_commitment = file_commitment(&result_a_bytes);
    let mut proof_preimage = Vec::new();
    for hash in [
        matched.pair_id(),
        a.body.receipt_id(),
        b.body.receipt_id(),
        a.body.projection.digest(),
        model_profile,
        receipt_a_file_commitment,
        receipt_b_file_commitment,
        result_file_commitment,
    ] {
        proof_preimage.extend_from_slice(hash.as_byte_slice());
    }
    let proof_commitment = blake2b_512_keyed(PROOF_COMMITMENT_DOMAIN, &proof_preimage);
    let mut ticket_preimage = Vec::with_capacity(32 + 128);
    ticket_preimage.extend_from_slice(&seed);
    ticket_preimage.extend_from_slice(proof_commitment.as_byte_slice());
    ticket_preimage.extend_from_slice(matched.pair_id().as_byte_slice());
    let nullifier = blake2b_512_keyed(TICKET_NULLIFIER_DOMAIN, &ticket_preimage);
    seed.fill(0);
    std::hint::black_box(&seed);
    let nullifier_commitment = ticket_nullifier_commitment(&nullifier);

    write_new(&nullifier_out, format!("{nullifier}\n").as_bytes(), true);
    let p = &a.body.projection;
    let proof = json!({
        "schema": "misaka.palw.real-provider-proof/v1",
        "verification": {
            "receipt_v3_a": true,
            "receipt_v3_b": true,
            "mldsa87_a": true,
            "mldsa87_b": true,
            "distinct_workers": true,
            "distinct_replica_slots": true,
            "exact_k2_projection_match": true,
            "worker_results_byte_identical": true,
            "output_token_commitment_recomputed": true
        },
        "external_receipt_network_id": a.body.network_id.to_string(),
        "external_pair_id": matched.pair_id().to_string(),
        "external_receipt_a_id": a.body.receipt_id().to_string(),
        "external_receipt_b_id": b.body.receipt_id().to_string(),
        "workers": [
            {
                "slot": 0,
                "label": a.json.worker_label,
                "credential_id": a.body.worker_credential_id.to_string(),
                "execution_nullifier": a.body.execution_nullifier.to_string(),
                "engine_seconds": a.json.engine_seconds,
                "timestamp_millis": a.json.timestamp_millis
            },
            {
                "slot": 1,
                "label": b.json.worker_label,
                "credential_id": b.body.worker_credential_id.to_string(),
                "execution_nullifier": b.body.execution_nullifier.to_string(),
                "engine_seconds": b.json.engine_seconds,
                "timestamp_millis": b.json.timestamp_millis
            }
        ],
        "projection": {
            "compute_set_id": p.compute_set_id.to_string(),
            "job_challenge": p.job_challenge.to_string(),
            "output_commitment": p.output_commitment.to_string(),
            "schedule_root": p.schedule_root.to_string(),
            "execution_root": p.execution_root.to_string(),
            "route_root": p.route_root.to_string(),
            "state_root": p.state_root.to_string(),
            "canonical_compute_units": p.canonical_compute_units,
            "token_count": p.token_count,
            "stop_reason": p.stop_reason,
            "prompt_token_count": a.json.prompt_tokens,
            "output_token_count": a.json.output_tokens
        },
        "runtime": {
            "class": a.json.class,
            "ruleset_id": a.json.artifacts.ruleset_id,
            "runtime_class_id_blake2b256": a.json.telemetry.runtime_class_id,
            "runtime_manifest_hash_blake2b256": a.json.telemetry.runtime_manifest_hash,
            "model_blake2b256": a.json.artifacts.model_blake2b256,
            "tables_blake2b256": a.json.artifacts.tables_blake2b256,
            "engine_blake2b256": a.json.artifacts.engine_blake2b256,
            "model_profile_id": model_profile.to_string()
        },
        "source_file_commitments": {
            "receipt_a": receipt_a_file_commitment.to_string(),
            "receipt_b": receipt_b_file_commitment.to_string(),
            "worker_result": result_file_commitment.to_string()
        },
        "proof_commitment": proof_commitment.to_string(),
        "ticket_binding": {
            "authority_pk_hash": authority_hash.to_string(),
            "nullifier_commitment": nullifier_commitment.to_string(),
            "derivation": "authority-secret + verified-proof-commitment + verified-k2-pair-id"
        }
    });
    let mut proof_bytes = serde_json::to_vec_pretty(&proof).unwrap_or_else(|e| die(format!("cannot serialize proof JSON: {e}")));
    proof_bytes.push(b'\n');
    write_new(&proof_out, &proof_bytes, false);

    println!("verification: receipt-v3+mldsa87+k2+tokens PASS");
    println!("external_pair_id: {}", matched.pair_id());
    println!("external_receipt_a_id: {}", a.body.receipt_id());
    println!("external_receipt_b_id: {}", b.body.receipt_id());
    println!("compute_set_id: {}", p.compute_set_id);
    println!("job_challenge: {}", p.job_challenge);
    println!("output_commitment: {}", p.output_commitment);
    println!("schedule_root: {}", p.schedule_root);
    println!("execution_root: {}", p.execution_root);
    println!("route_root: {}", p.route_root);
    println!("state_root: {}", p.state_root);
    println!("canonical_compute_units: {}", p.canonical_compute_units);
    println!("token_count: {}", p.token_count);
    println!("stop_reason: {}", p.stop_reason);
    println!("prompt_token_count: {}", a.json.prompt_tokens);
    println!("output_token_count: {}", a.json.output_tokens);
    println!("model_profile_id: {model_profile}");
    println!("proof_commitment: {proof_commitment}");
    println!("ticket_nullifier_commitment: {nullifier_commitment}");
    println!("ticket_authority_pk_hash: {authority_hash}");
    println!("proof_file: {}", proof_out.display());
}

fn cmd_store_add(args: &[String]) {
    let flags = parse_flags(args);
    let authority_path = PathBuf::from(required(&flags, "authority-key"));
    require_private_regular_file(&authority_path, "--authority-key");
    let mut seed = load_validator_seed(authority_path.to_str().unwrap_or_else(|| die("--authority-key path is not valid UTF-8")))
        .unwrap_or_else(|e| die(format!("cannot load ticket authority seed '{}': {e}", authority_path.display())));
    let authority_hash = authority_pk_hash(seed);
    seed.fill(0);
    std::hint::black_box(&seed);

    let batch_id = parse_hash(required(&flags, "batch-id"), "--batch-id");
    if batch_id == ZERO_HASH64 {
        die("--batch-id must be the nonzero content-derived manifest id");
    }
    let leaf_index: u32 = required(&flags, "leaf-index").parse().unwrap_or_else(|e| die(format!("--leaf-index is not a u32: {e}")));
    let nullifier_path = PathBuf::from(required(&flags, "nullifier-file"));
    require_private_regular_file(&nullifier_path, "--nullifier-file");
    let raw = fs::read_to_string(&nullifier_path)
        .unwrap_or_else(|e| die(format!("cannot read --nullifier-file '{}': {e}", nullifier_path.display())));
    let nullifier = parse_hash(raw.trim(), "--nullifier-file contents");
    let secret_file = PathBuf::from(required(&flags, "secret-file"));
    let mut store = TicketSecretStore::load_or_empty(secret_file, authority_hash).unwrap_or_else(|e| die(e));
    store.record_and_flush(batch_id, leaf_index, nullifier).unwrap_or_else(|e| die(e));
    eprintln!("palw-real-provider: recorded verified-inference ticket for batch_id={batch_id}, leaf_index={leaf_index}");
}

fn usage() {
    eprintln!(
        "palw-real-provider\n\
         usage:\n  \
         palw-real-provider verify-and-derive \\\n\
           --receipt-a <json> --receipt-b <json> --result-a <json> --result-b <json> \\\n\
           --authority-key <0600 seed> --nullifier-out <new 0600 file> --proof-out <new json>\n  \
         palw-real-provider store-add --authority-key <0600 seed> --secret-file <store.json> \\\n\
           --batch-id <128hex> --leaf-index <u32> --nullifier-file <0600 file>"
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rest = if args.len() > 2 { &args[2..] } else { &[] };
    match args.get(1).map(String::as_str) {
        Some("verify-and-derive") => cmd_verify_and_derive(rest),
        Some("store-add") => cmd_store_add(rest),
        _ => {
            usage();
            exit(2);
        }
    }
}
