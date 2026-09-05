//! **`palw-job-replay` — one committed job, re-executed here, every commitment printed.**
//!
//! An LLM-class block commits its job by construction: the anchor is a pure function of the
//! block's own position, and the prompt, the logits and the selected tokens all follow from it
//! deterministically — that is the class contract ("class" = the set of machines that produce
//! bit-identical traces). This tool is that contract made checkable on any machine holding the
//! artifact: give it the anchor a block committed and it re-derives the prompt, re-runs the
//! inference on THIS machine's CPU, and prints the roots, the material digest and the tokens.
//!
//! Two machines printing the same `material_sha256` for the same anchor have reproduced each
//! other's execution to the byte — full integer logits included, not just the argmax. Handed the
//! producer's retained `.material` file via `--material`, it additionally runs the seat's own
//! `verify_material` against the replay's roots, which is exactly the check a panel seat signs on.
//!
//! ```text
//! palw-job-replay --network testnet-11 --artifact qwen25-1.5b-a16.palwart \
//!                 --anchor <128-hex> [--material <claim>.material] [--label host-a]
//! ```
//!
//! The class is not an input: the artifact resolves against the network's own genesis
//! registrations, so the pairing under test is the one the chain pays for — a file that matches
//! no registered class is a refusal, not a fallback.

use std::path::PathBuf;
use std::str::FromStr;

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::qwen25_a16_backend::qwen25_a16_material_decode_v1;
use misaka_palw_base0::qwen36_backend::qwen36_material_decode_v1;
use misaka_palw_sdk::PalwClassSdk;
use serde_json::json;
use sha2::{Digest, Sha256};

fn die(message: String) -> ! {
    eprintln!("palw-job-replay: {message}");
    std::process::exit(1)
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()).collect()
}

fn h64(s: &str) -> Option<Hash64> {
    let b = hex_bytes(s)?;
    <[u8; 64]>::try_from(b.as_slice()).ok().map(Hash64::from_bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let network = flag("--network").unwrap_or_else(|| "testnet-11".to_string());
    let artifact_path = flag("--artifact").unwrap_or_else(|| die("--artifact <file> is required".into()));
    let anchor_hex = flag("--anchor").unwrap_or_else(|| die("--anchor <128-hex> is required".into()));
    let material_path = flag("--material");
    let label = flag("--label").unwrap_or_else(|| std::env::var("HOSTNAME").unwrap_or_else(|_| "unlabeled".to_string()));

    let anchor = h64(&anchor_hex).unwrap_or_else(|| die(format!("--anchor {anchor_hex} is not a 128-hex Hash64")));

    // The network's own terms: its court, and the classes its GENESIS registered. The domain and
    // ruleset are main's shipped params — the same place consensus reads them.
    let network_id = NetworkId::from_str(&network).unwrap_or_else(|e| die(format!("--network {network}: {e}")));
    let params: Params = network_id.into();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        die(format!("{network_id} has no PALW V2 bundle, so it has no classes to replay"));
    };
    let sdk = PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), network_id.to_string().into_bytes());

    let artifact_bytes = std::fs::read(&artifact_path).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let artifact_sha256 = sha256_hex(&artifact_bytes);
    drop(artifact_bytes);
    let holding = sdk.load_artifact(&PathBuf::from(&artifact_path)).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let holdings = [holding];

    // The class under test is the ARTIFACT's own pairing — never "the first class that resolves",
    // because the floor resolves with no file at all and would shadow every model class. The
    // artifact names its class through the SDK's pairing view; the genesis then has to agree on
    // both the class id and the root, so the execution below is the one the chain pays for.
    let paired: Vec<(Hash64, Hash64)> = sdk
        .pairings(&holdings[0])
        .into_iter()
        .filter_map(|(entry, result)| result.ok().map(|root| (entry.class_id(), root)))
        .collect();
    let mut resolved = None;
    for object in bundle.genesis_objects.iter() {
        let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } = object else { continue };
        if !paired.iter().any(|(pc, pr)| pc == class_id && pr == artifact_root) {
            continue;
        }
        match sdk.resolve(*class_id, *artifact_root, &holdings) {
            Ok(backend) => {
                resolved = Some((*class_id, *artifact_root, backend));
                break;
            }
            Err(e) => die(format!("the genesis registers this artifact's class {class_id} but it does not resolve: {e}")),
        }
    }
    let Some((class_id, artifact_root, backend)) = resolved else {
        die(format!("{artifact_path} pairs with no class the {network_id} genesis registered"));
    };

    let (job, prompt) = backend.job_for_anchor(anchor).unwrap_or_else(|e| die(format!("job_for_anchor: {e}")));
    let started = std::time::Instant::now();
    let outcome = backend.execute(&job, &prompt).unwrap_or_else(|e| die(format!("execute: {e}")));
    let elapsed = started.elapsed();

    // The generated tokens, read back from the material through the same decoder every consumer
    // uses — never from a private field, so what is printed is what a seat would see.
    let (generated, logits_rows, logits_row_len) = if let Some(run) = qwen25_a16_material_decode_v1(&outcome.material) {
        let row_len = run.logits_rows.first().map(|r| r.len()).unwrap_or(0);
        (run.generated, run.logits_rows.len(), row_len)
    } else if let Some(run) = qwen36_material_decode_v1(&outcome.material) {
        (run.generated, 0, 0)
    } else {
        (Vec::new(), 0, 0)
    };

    // `--material`: the producer's retained bytes for this claim, verified the way a seat verifies
    // them — against the replay's own roots and the block-derived anchor.
    let mut material_file = json!(null);
    if let Some(path) = material_path {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| die(format!("{path}: {e}")));
        let verdict = backend.verify_material(
            &bytes,
            PalwClaimRootsV1 { execution_root: outcome.execution_root, trace_root: outcome.trace_root, anchor },
        );
        material_file = json!({
            "path": path,
            "sha256": sha256_hex(&bytes),
            "byte_identical_to_replay": bytes == outcome.material,
            "seat_verdict_against_replay_roots": match verdict {
                PalwMaterialVerdictV1::Matches => "Matches",
                PalwMaterialVerdictV1::Mismatch => "Mismatch",
                PalwMaterialVerdictV1::Unverifiable => "Unverifiable",
            },
        });
    }

    let report = json!({
        "label": label,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "network": network_id.to_string(),
        "model_id": backend.model_id(),
        "class_id": class_id.to_string(),
        "artifact_root": artifact_root.to_string(),
        "artifact_sha256": artifact_sha256,
        "anchor": anchor.to_string(),
        "prompt_ids": prompt,
        "generated_ids": generated,
        "logits_rows": logits_rows,
        "logits_row_len": logits_row_len,
        "trace_root": outcome.trace_root.to_string(),
        "output_root": outcome.output_root.to_string(),
        "execution_root": outcome.execution_root.to_string(),
        "trace_manifest_root": outcome.trace_manifest_root.to_string(),
        "trace_chunk_count": outcome.trace_chunk_count,
        "material_sha256": sha256_hex(&outcome.material),
        "material_len": outcome.material.len(),
        "inference_ms": elapsed.as_millis() as u64,
        "material_file": material_file,
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
