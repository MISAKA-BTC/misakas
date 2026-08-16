//! `palw-record-verify` — third-party verification of an emitted capability record, holding
//! NOTHING but the record file. This is the matcher's first move at Stage 0: decode the
//! canonical Borsh capability, re-derive every identity the JSON view claims, check every
//! ready proof against the committed root, re-derive the signing message, and verify the
//! ML-DSA-87 signature under the routing context with the record's own public key — the same
//! `verify_mldsa87_with_context` the virtual processor uses. Exit 0 = every claim held.

use kaspa_consensus_core::dns_finality::validator_id_from_pubkey;
use kaspa_consensus_core::palw_routing::{
    PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT, PalwVerifierCapabilityV1, verifier_capability_message_v1, verify_ready_binding_v1,
};
use kaspa_txscript::verify_mldsa87_with_context;
use misaka_palw_reexecutor::{CAPABILITY_RECORD_SCHEMA_V1, CapabilityRecordV1, hex64, parse_hash64};

fn die(msg: String) -> ! {
    eprintln!("[palw-record-verify] FAIL: {msg}");
    std::process::exit(1);
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| die("usage: palw-record-verify <capability-record.json>".into()));
    let record: CapabilityRecordV1 =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| die(format!("read {path}: {e}"))))
            .unwrap_or_else(|e| die(format!("record json: {e}")));
    if record.schema != CAPABILITY_RECORD_SCHEMA_V1 {
        die(format!("unknown schema {:?}", record.schema));
    }

    // The canonical payload is the Borsh capability; the JSON view must MATCH it, never
    // extend it — a matcher that trusted a JSON field the Borsh does not carry would be
    // trusting the emitter twice.
    let capability: PalwVerifierCapabilityV1 = {
        let mut bytes = vec![0u8; record.capability_borsh_hex.len() / 2];
        faster_hex::hex_decode(record.capability_borsh_hex.as_bytes(), &mut bytes)
            .unwrap_or_else(|e| die(format!("capability hex: {e}")));
        borsh::from_slice(&bytes).unwrap_or_else(|e| die(format!("capability borsh: {e}")))
    };
    capability.validate().unwrap_or_else(|e| die(format!("capability does not validate: {e}")));
    let checks: [(&str, bool); 8] = [
        ("capability_id", record.capability_id == hex64(&capability.capability_id())),
        ("verifier_id", record.verifier_id == hex64(&capability.verifier_id)),
        ("family_version", record.family_version == capability.family_version),
        ("max_model_band", record.max_model_band == format!("{:?}", capability.max_model_band)),
        ("ready_binding_root", record.ready_binding_root == hex64(&capability.ready_binding_root)),
        ("availability_expiry_daa", record.availability_expiry_daa == capability.availability_expiry_daa),
        ("capability_nonce", record.capability_nonce == capability.capability_nonce),
        ("execution_family", record.execution_family == format!("{:?}", capability.execution_family)),
    ];
    for (name, held) in checks {
        if !held {
            die(format!("JSON view disagrees with the Borsh capability on {name}"));
        }
    }

    // The public key must BE the verifier: validator_id = H(pubkey), the consensus rule.
    let mut pubkey = vec![0u8; record.verifier_public_key.len() / 2];
    faster_hex::hex_decode(record.verifier_public_key.as_bytes(), &mut pubkey).unwrap_or_else(|e| die(format!("public key hex: {e}")));
    if validator_id_from_pubkey(&pubkey) != capability.verifier_id {
        die("the published public key does not hash to the capability's verifier_id".into());
    }

    // Every ready proof must verify against the committed root, and the signing message must
    // re-derive from the signed struct exactly as published.
    for entry in &record.ready_bindings {
        let binding_id = parse_hash64(&entry.binding_id).unwrap_or_else(|e| die(e));
        let proof = entry.proof.to_proof().unwrap_or_else(|e| die(e));
        if !verify_ready_binding_v1(&capability.ready_binding_root, &binding_id, &proof) {
            die(format!("ready proof for {} does not verify", &entry.binding_id[..16]));
        }
    }
    let message = verifier_capability_message_v1(record.network_id.as_bytes(), &capability);
    if record.signing_message != faster_hex::hex_string(message.as_bytes().as_slice()) {
        die("the published signing message is not the one the capability derives".into());
    }
    match verify_mldsa87_with_context(
        &pubkey,
        message.as_bytes().as_slice(),
        &capability.signature,
        PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT,
    ) {
        Ok(true) => {}
        other => die(format!("ML-DSA-87 signature verification failed: {other:?}")),
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "misaka.palw-record-verify.v1",
            "verified": true,
            "capability_id": record.capability_id,
            "verifier_id": record.verifier_id,
            "nonce": record.capability_nonce,
            "ready_bindings": record.ready_bindings.len(),
            "expires_daa": record.availability_expiry_daa,
        }))
        .expect("serializable")
    );
}
