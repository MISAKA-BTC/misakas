//! Bounded empirical source-versus-integer diagnostic with a policy fixed before measurement.
//! Uses the existing mapped artifact executor and portable fit arithmetic. Reference producer
//! labels and source revision labels are informational; their authentication is outside this tool.
//! No source-logit gate implies complete-task quality, consensus equality, registration or Final.
use misaka_palw_sdk::runtime_pack::hfref::{self, HfReference};
use misaka_palw_tir_exec::node::TirArtifactV1;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Protocol {
    schema: String,
    repository: String,
    revision: String,
    artifact_sha256: String,
    source_manifest_sha256: String,
    tokens_sha256: String,
    task: String,
    context: u32,
    sequences: usize,
    positions: usize,
    vocab: usize,
    fed_tokens: Vec<Vec<usize>>,
    logits_scale: f64,
    top1_min: f64,
    kl_mean_max: f64,
    slope_min: f64,
    slope_max: f64,
    correlation_min: f64,
    max_abs: f64,
    rmse_max: f64,
}

fn hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut reader = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: check-source-checkpoint-fidelity <artifact> <hf-reference directory> <predeclared protocol.json>".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&args[2])?.take((64 << 10) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 << 10 {
        return Err("protocol byte limit".into());
    }
    let protocol: Protocol = serde_json::from_slice(&bytes)?;
    let finite = [
        protocol.logits_scale,
        protocol.top1_min,
        protocol.kl_mean_max,
        protocol.slope_min,
        protocol.slope_max,
        protocol.correlation_min,
        protocol.max_abs,
        protocol.rmse_max,
    ];
    let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
    if protocol.schema != "misaka.rfc02.independent-source-fidelity-protocol.v1"
        || !hex(&protocol.revision, 40)
        || !hex(&protocol.artifact_sha256, 64)
        || !hex(&protocol.source_manifest_sha256, 64)
        || !hex(&protocol.tokens_sha256, 64)
        || protocol.repository.is_empty()
        || protocol.task != "causal-next-token-logit-diagnostic"
        || protocol.context == 0
        || protocol.sequences == 0
        || protocol.positions == 0
        || protocol.sequences > 256
        || protocol.positions > 4096
        || protocol.vocab == 0
        || protocol.fed_tokens.len() != protocol.sequences
        || protocol.fed_tokens.iter().any(|s| {
            s.is_empty() || s.len() > protocol.context as usize || s.iter().any(|t| *t >= protocol.vocab || *t > u32::MAX as usize)
        })
        || protocol.fed_tokens.iter().try_fold(0usize, |n, s| n.checked_add(s.len())) != Some(protocol.positions)
        || protocol.positions.checked_mul(protocol.vocab).is_none_or(|n| n > 64 << 20)
        || finite.iter().any(|v| !v.is_finite())
        || protocol.logits_scale <= 0.0
        || !(0.0..=1.0).contains(&protocol.top1_min)
        || !(0.0..=1.0).contains(&protocol.correlation_min)
        || protocol.slope_min <= 0.0
        || protocol.slope_max < protocol.slope_min
        || protocol.kl_mean_max < 0.0
        || protocol.max_abs < 0.0
        || protocol.rmse_max < 0.0
    {
        return Err("invalid predeclared policy or coverage".into());
    }
    // Hash the exact artifact before mapping or execution. Provenance meta alone has no authority.
    let artifact_path = Path::new(&args[0]);
    if hash(artifact_path)? != protocol.artifact_sha256 {
        return Err("artifact byte identity".into());
    }
    let artifact = TirArtifactV1::open(artifact_path)?;
    let meta: serde_json::Value = serde_json::from_str(&artifact.container().header.meta)?;
    if protocol.context > artifact.plan().program.history_bound
        || protocol.vocab != artifact.plan().program.token_bound as usize
        || meta["logits_scale"].as_f64().is_none_or(|s| s.to_bits() != protocol.logits_scale.to_bits())
        || meta["math"] != "libm-v1"
    {
        return Err("artifact context, vocabulary or exact arithmetic units".into());
    }
    let reference_path = Path::new(&args[1]);
    let reference = HfReference::load(reference_path)?;
    if reference.vocab != protocol.vocab
        || reference.sequences.len() != protocol.sequences
        || reference.sequences.iter().map(|s| &s.tokens).ne(protocol.fed_tokens.iter())
        || reference.positions() != protocol.positions
        || reference.sequences.iter().any(|s| s.tokens.len() > protocol.context as usize)
        || reference.producer["source_manifest_sha256"] != protocol.source_manifest_sha256
        || reference.producer["token_file_sha256"] != protocol.tokens_sha256
        || reference.producer["repository"] != protocol.repository
        || reference.producer["revision"] != protocol.revision
        || reference.producer["reference_dtype"] != "float32"
        || reference.producer["source_before_and_after_sha256_match"] != true
    {
        return Err("source reference scope or byte pins differ from policy".into());
    }
    let started = std::time::Instant::now();
    let fit = hfref::measure_streamed_deterministic(&artifact, protocol.logits_scale, &reference)?;
    let metrics = [fit.slope, fit.intercept, fit.corr, fit.max_abs, fit.rmse, fit.top1, fit.kl_mean];
    let mut failures = Vec::new();
    if metrics.iter().any(|n| !n.is_finite()) {
        failures.push("non-finite fit");
    }
    if fit.top1 < protocol.top1_min {
        failures.push("top1");
    }
    if fit.kl_mean > protocol.kl_mean_max {
        failures.push("kl_mean");
    }
    if !(protocol.slope_min..=protocol.slope_max).contains(&fit.slope) {
        failures.push("slope");
    }
    if fit.corr < protocol.correlation_min {
        failures.push("correlation");
    }
    if fit.max_abs > protocol.max_abs {
        failures.push("max_abs");
    }
    if fit.rmse > protocol.rmse_max {
        failures.push("rmse");
    }
    if hash(artifact_path)? != protocol.artifact_sha256 {
        return Err("artifact changed during measurement".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "schema": "misaka.rfc02.independent-source-fidelity-result.v1",
            "repository": protocol.repository, "revision": protocol.revision, "task": protocol.task,
            "context": protocol.context, "sequences": protocol.sequences, "positions": protocol.positions,
            "artifact_sha256": protocol.artifact_sha256, "protocol_sha256": hash(Path::new(&args[2]))?,
            "reference_json_sha256": hash(&reference_path.join("hf-reference.json"))?,
            "reference_logits_sha256": hash(&reference_path.join("hf-reference.f32"))?,
        "storage": {"source": reference.producer["source_storage"], "source_files": reference.producer["source_files"], "integer": "PALWTIR1"},
            "arithmetic": {"reference": "Transformers float32 eager CPU", "integer": "frozen exact TIR25", "fit": "libm-v1"},
            "logits_scale": protocol.logits_scale,
            "metrics": {"slope": fit.slope, "intercept": fit.intercept, "correlation": fit.corr,
                "max_abs": fit.max_abs, "rmse": fit.rmse, "top1": fit.top1, "kl_mean": fit.kl_mean},
            "measurement_elapsed_ms": started.elapsed().as_millis(),
            "predeclared_diagnostic_fidelity_passed": failures.is_empty(), "failures": failures,
            "producer": reference.producer,
            "integer_reference_bit_equality_established_by_this_run": false,
            "complete_task_or_original_maximum_context_proven": false,
            "node_registration_public_conviction_final_or_redemption_proven": false,
            "a6_a8_cohort_acceptance_proven": false,
            "full_rfc02_complete": false
        })
    );
    if !failures.is_empty() {
        return Err("predeclared source diagnostic fidelity failed; result retained".into());
    }
    Ok(())
}
