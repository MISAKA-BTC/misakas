//! `qi35-receipt-v3` — emit an ML-DSA-87 signed **Receipt v3** for a real qi35 execution.
//!
//! ## Why this exists
//!
//! The qi35 engine emits `palw.canonical-integer-receipt/v0`, whose authenticator is a
//! **keyed BLAKE2b-256 MAC**. A MAC is symmetric: only the key holder can check it, so it is
//! evidence to its own author and to nobody else. The PALW mint path consumes
//! `palw.integer-receipt/v3-envelope` — the same `class: canonical_integer_v0`, but authenticated
//! by an **ML-DSA-87 signature** over the canonical body, which anyone holding the verifying key
//! can check. That difference, not any missing measurement, is what kept real inference out of
//! the ticket path: the engine already computes every consensus-relevant quantity the v3
//! projection needs.
//!
//! This binary closes that gap for the qi35 class by mapping an engine execution onto
//! [`ComputeReceiptV3`] and signing it:
//!
//! | `MatchProjectionV2` field   | source                                              |
//! |-----------------------------|-----------------------------------------------------|
//! | `output_commitment`         | `output_commitment_v3(GEN token ids, job_challenge)` |
//! | `route_root`                | engine `ROOTS route=` (MoE routing trace)            |
//! | `state_root`                | engine `ROOTS state=` (recurrent state)              |
//! | `schedule_root`             | engine `ROOTS kv=` (KV/operation schedule)           |
//! | `execution_root`            | keyed digest over route‖kv‖state                     |
//! | `canonical_compute_units`   | operator-supplied semantic CU for the class          |
//! | `token_count` / `stop_reason` | prompt + GEN length / canonical stop tag           |
//!
//! **Placement is deliberate and temporary.** The designated producer is
//! `PalwRuntimePluginV1::execute -> ComputeReceiptV3`; this crate is where the desktop-side
//! tooling already lives, so the mapping can be exercised end to end before the plugin exists.
//! When the qi35 runtime plugin lands, it should take this mapping over verbatim and this binary
//! should go away.
//!
//! **What still gates a real-inference mint** (unchanged by this binary): `compute_set_id` must
//! name a Compute Set that is REGISTERED and ACTIVE on the target network. On testnet-20 the
//! registry band (0x40-0x44) is open from genesis, so that is a registration step, not a code
//! change.

use kaspa_hashes::blake2b_512_keyed;
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw::receipt_v3::{
    ComputeReceiptV3, ImplementationTelemetryV3, MLDSA87_ALGORITHM_ID, MatchProjectionV2,
    PALW_RECEIPT_V3_MLDSA87_CONTEXT, RECEIPT_V3_VERSION, SignedEnvelopeV3, credential_id_from_verifying_key,
    execution_nullifier_v3, output_commitment_v3,
};
use misaka_palw_bridge::chain::parse_hash64;
use misaka_palw_bridge::match_key::{bytes_hex, decode_hex, hash64_hex};
use serde_json::json;

/// Keyed digest binding the three engine roots into one execution root.
const QI35_EXECUTION_ROOT_DOMAIN: &[u8] = b"misaka-palw-qi35/execution-root-v1";
/// Domain for lifting a 32-byte engine root into the 64-byte `Hash64` the v3 projection uses.
const QI35_ROOT_LIFT_DOMAIN: &[u8] = b"misaka-palw-qi35/root-lift-v1";

/// The qi35 engine commits its execution roots as BLAKE2b-**256**; `MatchProjectionV2` is
/// `Hash64` (512-bit) throughout. Lift the narrow root with a domain-separated keyed hash rather
/// than zero-padding it: padding would make a 32-byte root and a 64-byte root that happens to end
/// in zeros indistinguishable, and it would put engine bytes into a field consensus reads as a
/// full-width digest. The lift is injective on the engine's own output and is stated here so a
/// replica reproduces it exactly.
fn lift_root(hex: &str, label: &str) -> Result<kaspa_hashes::Hash64, String> {
    let bytes = decode_hex(hex).map_err(|e| format!("{label}: {e}"))?;
    match bytes.len() {
        64 => Ok(parse_hash64(hex)?),
        32 => Ok(blake2b_512_keyed(QI35_ROOT_LIFT_DOMAIN, &bytes)),
        n => Err(format!("{label}: want a 32- or 64-byte root, got {n}")),
    }
}

struct Args {
    prompt_ids: Vec<u32>,
    output_ids: Vec<u32>,
    route: String,
    kv: String,
    state: String,
    job_challenge: String,
    compute_set_id: String,
    network_id: String,
    worker_key: String,
    replica_slot: u8,
    issued_epoch: u64,
    expires_epoch: u64,
    canonical_compute_units: u64,
    stop_reason: u8,
    runtime_class_id: String,
    runtime_manifest_hash: String,
    engine_blake2b256: String,
    model_blake2b256: String,
    tables_blake2b256: String,
    worker_label: String,
    /// Measured wall-clock of THIS worker's execution. The node's verifier requires a positive
    /// finite value — a receipt that claims no execution time proves no execution.
    engine_seconds: f64,
    timestamp_millis: u64,
    receipt_out: String,
    result_out: String,
}

fn ids_from(text: &str) -> Result<Vec<u32>, String> {
    text.split(',').filter(|s| !s.trim().is_empty()).map(|s| s.trim().parse::<u32>().map_err(|e| format!("token id {s:?}: {e}"))).collect()
}

fn parse_args() -> Result<Args, String> {
    let mut m: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let name = argv[i].strip_prefix("--").ok_or_else(|| format!("expected --flag, got {:?}", argv[i]))?;
        i += 1;
        let value = argv.get(i).cloned().ok_or_else(|| format!("--{name} needs a value"))?;
        m.insert(name.to_string(), value);
        i += 1;
    }
    let need = |k: &str| -> Result<String, String> { m.get(k).cloned().ok_or_else(|| format!("--{k} is required")) };
    let file_or = |k: &str| -> Result<String, String> {
        let v = need(k)?;
        if let Some(path) = v.strip_prefix('@') {
            std::fs::read_to_string(path).map(|s| s.trim().to_string()).map_err(|e| format!("read {path}: {e}"))
        } else {
            Ok(v)
        }
    };
    Ok(Args {
        prompt_ids: ids_from(&file_or("prompt-ids")?)?,
        output_ids: ids_from(&file_or("output-ids")?)?,
        route: need("route-root")?,
        kv: need("kv-root")?,
        state: need("state-root")?,
        job_challenge: need("job-challenge")?,
        compute_set_id: need("compute-set-id")?,
        network_id: need("network-id-hash")?,
        worker_key: need("worker-key")?,
        replica_slot: need("replica-slot")?.parse().map_err(|e| format!("--replica-slot: {e}"))?,
        issued_epoch: need("issued-epoch")?.parse().map_err(|e| format!("--issued-epoch: {e}"))?,
        expires_epoch: need("expires-epoch")?.parse().map_err(|e| format!("--expires-epoch: {e}"))?,
        canonical_compute_units: need("canonical-compute-units")?.parse().map_err(|e| format!("--canonical-compute-units: {e}"))?,
        stop_reason: m.get("stop-reason").map(|s| s.parse()).transpose().map_err(|e| format!("--stop-reason: {e}"))?.unwrap_or(0),
        runtime_class_id: need("runtime-class-id")?,
        runtime_manifest_hash: need("runtime-manifest-hash")?,
        engine_blake2b256: need("engine-blake2b256")?,
        model_blake2b256: need("model-blake2b256")?,
        tables_blake2b256: need("tables-blake2b256")?,
        worker_label: m.get("worker-label").cloned().unwrap_or_else(|| "qi35".into()),
        engine_seconds: need("engine-seconds")?.parse().map_err(|e| format!("--engine-seconds: {e}"))?,
        timestamp_millis: m
            .get("timestamp-millis")
            .map(|s| s.parse())
            .transpose()
            .map_err(|e| format!("--timestamp-millis: {e}"))?
            .unwrap_or(0),
        receipt_out: need("receipt-out")?,
        result_out: need("result-out")?,
    })
}

fn fixed32(hex: &str, label: &str) -> Result<[u8; 32], String> {
    let bytes = decode_hex(hex).map_err(|e| format!("{label}: {e}"))?;
    bytes.as_slice().try_into().map_err(|_| format!("{label}: want 32 bytes, got {}", bytes.len()))
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("qi35-receipt-v3: {e}");
            eprintln!(
                "\nusage: qi35-receipt-v3 --prompt-ids @file --output-ids @file \\\n  \
                 --route-root H --kv-root H --state-root H --job-challenge H128 \\\n  \
                 --compute-set-id H128 --network-id-hash H128 --worker-key <seed> --replica-slot 0|1 \\\n  \
                 --issued-epoch N --expires-epoch N --canonical-compute-units N [--stop-reason N] \\\n  \
                 --runtime-class-id H64 --runtime-manifest-hash H64 \\\n  \
                 --engine-blake2b256 H64 --model-blake2b256 H64 --tables-blake2b256 H64 \\\n  \
                 --engine-seconds F [--timestamp-millis N] [--worker-label S] \\\n  \
                 --receipt-out <json> --result-out <json>"
            );
            std::process::exit(2);
        }
    }
}

fn run() -> Result<(), String> {
    let a = parse_args()?;
    if a.replica_slot > 1 {
        return Err("replica_slot must be 0 or 1 (Receipt v3 accepts no others)".into());
    }
    if a.output_ids.is_empty() {
        return Err("output-ids is empty — there is no execution to attest".into());
    }
    if !(a.engine_seconds.is_finite() && a.engine_seconds > 0.0) {
        return Err("--engine-seconds must be a positive finite measurement of this run".into());
    }

    // The worker's ML-DSA-87 identity. THIS is what makes the receipt publicly checkable: the
    // credential is an unkeyed digest of the verifying key, and the signature is verifiable by
    // anyone holding that key — unlike the engine's symmetric MAC.
    let seed_hex = std::fs::read_to_string(&a.worker_key).map_err(|e| format!("read {}: {e}", a.worker_key))?;
    let seed: [u8; 32] = decode_hex(seed_hex.trim())?
        .as_slice()
        .try_into()
        .map_err(|_| format!("{}: want a 32-byte hex seed", a.worker_key))?;
    let worker = ValidatorKey::from_seed(seed);
    let verifying_key = worker.public_key().to_vec();
    let worker_credential_id = credential_id_from_verifying_key(&verifying_key);

    let network_id = parse_hash64(&a.network_id)?;
    let compute_set_id = parse_hash64(&a.compute_set_id)?;
    let job_challenge = parse_hash64(&a.job_challenge)?;

    // Engine roots. `execution_root` binds all three so a replica that agreed on the answer but
    // not on how it got there cannot pass the projection.
    let route_root = lift_root(&a.route, "route-root")?;
    let schedule_root = lift_root(&a.kv, "kv-root")?;
    let state_root = lift_root(&a.state, "state-root")?;
    let execution_root = {
        let mut preimage = Vec::with_capacity(192);
        for h in [&route_root, &schedule_root, &state_root] {
            preimage.extend_from_slice(h.as_byte_slice());
        }
        blake2b_512_keyed(QI35_EXECUTION_ROOT_DOMAIN, &preimage)
    };

    let projection = MatchProjectionV2 {
        compute_set_id,
        job_challenge,
        output_commitment: output_commitment_v3(&a.output_ids, &job_challenge),
        schedule_root,
        execution_root,
        route_root,
        state_root,
        canonical_compute_units: a.canonical_compute_units,
        // TOTAL committed tokens — prompt + output. The node's verifier re-derives it as
        // `prompt_token_count + tokens.len()` from the worker result, so counting only the
        // output here silently fails that cross-check.
        token_count: (a.prompt_ids.len() + a.output_ids.len()) as u64,
        stop_reason: a.stop_reason,
    };
    let telemetry = ImplementationTelemetryV3 {
        runtime_class_id: fixed32(&a.runtime_class_id, "runtime-class-id")?,
        runtime_manifest_hash: fixed32(&a.runtime_manifest_hash, "runtime-manifest-hash")?,
    };
    let body = ComputeReceiptV3 {
        receipt_version: RECEIPT_V3_VERSION,
        network_id,
        projection: projection.clone(),
        telemetry,
        worker_credential_id,
        replica_slot: a.replica_slot,
        execution_nullifier: execution_nullifier_v3(
            &network_id,
            &compute_set_id,
            &job_challenge,
            &worker_credential_id,
            a.replica_slot,
            a.issued_epoch,
        ),
        issued_epoch: a.issued_epoch,
        expires_epoch: a.expires_epoch,
    };

    let body_digest = body.signing_digest();
    let signature = worker.sign_with_context(body_digest.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT);
    let envelope = SignedEnvelopeV3 {
        body_digest,
        algorithm: MLDSA87_ALGORITHM_ID,
        signer_credential_id: worker_credential_id,
        signature: signature.to_vec(),
    };

    let receipt = json!({
        "schema": "palw.integer-receipt/v3-envelope",
        "class": "canonical_integer_v0",
        "receipt_version": body.receipt_version,
        "network_id": hash64_hex(&body.network_id),
        "execution_nullifier": hash64_hex(&body.execution_nullifier),
        "projection": {
            "compute_set_id": hash64_hex(&projection.compute_set_id),
            "job_challenge": hash64_hex(&projection.job_challenge),
            "output_commitment": hash64_hex(&projection.output_commitment),
            "schedule_root": hash64_hex(&projection.schedule_root),
            "execution_root": hash64_hex(&projection.execution_root),
            "route_root": hash64_hex(&projection.route_root),
            "state_root": hash64_hex(&projection.state_root),
            "canonical_compute_units": projection.canonical_compute_units,
            "token_count": projection.token_count,
            "stop_reason": projection.stop_reason,
        },
        "telemetry": {
            "runtime_class_id": bytes_hex(&body.telemetry.runtime_class_id),
            "runtime_manifest_hash": bytes_hex(&body.telemetry.runtime_manifest_hash),
        },
        "worker_credential_id": hash64_hex(&body.worker_credential_id),
        "replica_slot": body.replica_slot,
        "issued_epoch": body.issued_epoch,
        "expires_epoch": body.expires_epoch,
        "envelope": {
            "body_digest": hash64_hex(&envelope.body_digest),
            "algorithm": envelope.algorithm,
            "signer_credential_id": hash64_hex(&envelope.signer_credential_id),
            "signature": bytes_hex(&envelope.signature),
        },
        "verifying_key": bytes_hex(&verifying_key),
        "receipt_id": hash64_hex(&body.receipt_id()),
        "worker_label": a.worker_label,
        "prompt_tokens": a.prompt_ids.len() as u64,
        "output_tokens": a.output_ids.len() as u64,
        "engine_seconds": a.engine_seconds,
        "timestamp_millis": a.timestamp_millis,
        "artifacts": {
            "engine_blake2b256": a.engine_blake2b256,
            "model_blake2b256": a.model_blake2b256,
            "ruleset_id": "qi35-int-v1",
            "tables_blake2b256": a.tables_blake2b256,
        },
    });

    // The worker-result JSON must be BYTE-IDENTICAL between the two replicas, so it carries only
    // matched quantities — no per-worker identity, no timing.
    let result = json!({
        "canonical_compute_units": projection.canonical_compute_units,
        "compute_set_id": hash64_hex(&projection.compute_set_id),
        "execution_root": hash64_hex(&projection.execution_root),
        "prompt_token_count": a.prompt_ids.len() as u64,
        "route_root": hash64_hex(&projection.route_root),
        "schedule_root": hash64_hex(&projection.schedule_root),
        "state_root": hash64_hex(&projection.state_root),
        "stop_reason": projection.stop_reason,
        "telemetry": {
            "runtime_class_id": bytes_hex(&body.telemetry.runtime_class_id),
            "runtime_manifest_hash": bytes_hex(&body.telemetry.runtime_manifest_hash),
        },
        "tokens": a.output_ids,
    });

    std::fs::write(&a.receipt_out, serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?)
        .map_err(|e| format!("write {}: {e}", a.receipt_out))?;
    std::fs::write(&a.result_out, serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?)
        .map_err(|e| format!("write {}: {e}", a.result_out))?;
    println!("receipt_id: {}", hash64_hex(&body.receipt_id()));
    println!("worker_credential_id: {}", hash64_hex(&worker_credential_id));
    println!("replica_slot: {}", body.replica_slot);
    println!("receipt: {}", a.receipt_out);
    println!("result:  {}", a.result_out);
    Ok(())
}
