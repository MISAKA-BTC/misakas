//! Own autoregressive integer jobs versus independently produced HF jobs.
//! Fixed lengths and thresholds precede execution. This tool does not interpret
//! text or establish task quality, native eligibility, Final or cohort coverage.
use misaka_palw_tir_exec::{NoSink, TirExecutor, node::TirArtifactV1};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Read, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    id: String,
    task: String,
    prompt_text: String,
    prompt_tokens: Vec<u32>,
    expected_answer: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Protocol {
    schema: String,
    repository: String,
    revision: String,
    source_manifest_sha256: String,
    artifact_sha256: String,
    tokenizer_sha256: String,
    context: u32,
    vocab: u32,
    max_source_bytes: u64,
    max_model_parameter_bytes: u64,
    max_total_positions: usize,
    max_new_tokens: usize,
    eos_token_ids: Vec<u32>,
    task_jobs: Vec<Job>,
    token_agreement_min: f64,
    exact_job_fraction_min: f64,
    source_and_integer_task_success_min: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceJob {
    id: String,
    task: String,
    prompt_tokens: Vec<u32>,
    generated_tokens: Vec<u32>,
    first_eos_index: Option<usize>,
    answer_text: String,
    predeclared_task_answer_passed: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    schema: String,
    protocol_sha256: String,
    repository: String,
    revision: String,
    source_manifest_sha256: String,
    tokenizer_sha256: String,
    context: u32,
    max_new_tokens: usize,
    vocab: u32,
    jobs: Vec<SourceJob>,
    producer: serde_json::Value,
    fixed_count_generated_after_eos: bool,
    complete_task_benchmark_or_maximum_context_proven: bool,
    native_final_or_a6_a8_proven: bool,
    full_rfc02_complete: bool,
}
fn hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        h.update(&buffer[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take((64 << 10) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 << 10 {
        return Err("generation JSON limit".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn validate(p: &Protocol) -> Result<(), &'static str> {
    let hex = |s: &str, n| s.len() == n && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if p.schema != "misaka.rfc02.greedy-task-protocol.v1"
        || p.repository.is_empty()
        || !hex(&p.revision, 40)
        || [&p.source_manifest_sha256, &p.artifact_sha256, &p.tokenizer_sha256].iter().any(|s| !hex(s, 64))
        || p.context == 0
        || p.context > 4096
        || p.vocab == 0
        || p.vocab > 1 << 20
        || p.max_source_bytes == 0
        || p.max_source_bytes > 16 << 30
        || p.max_model_parameter_bytes == 0
        || p.max_model_parameter_bytes > 32 << 30
        || p.max_total_positions == 0
        || p.max_total_positions > 4096
        || p.max_new_tokens == 0
        || p.max_new_tokens > 128
        || p.eos_token_ids.is_empty()
        || p.eos_token_ids.len() > 8
        || p.eos_token_ids.iter().any(|id| *id >= p.vocab)
        || p.eos_token_ids.iter().collect::<BTreeSet<_>>().len() != p.eos_token_ids.len()
        || p.task_jobs.is_empty()
        || p.task_jobs.len() > 64
        || [p.token_agreement_min, p.exact_job_fraction_min, p.source_and_integer_task_success_min]
            .iter()
            .any(|x| !x.is_finite() || *x <= 0.0 || *x > 1.0)
    {
        return Err("invalid fixed generation policy");
    }
    let mut ids = BTreeSet::new();
    let mut positions = 0usize;
    for j in &p.task_jobs {
        if j.id.is_empty()
            || j.id.len() > 64
            || !ids.insert(&j.id)
            || !matches!(j.task.as_str(), "arithmetic" | "translation" | "code")
            || j.prompt_text.is_empty()
            || j.prompt_text.chars().count() > 4096
            || j.expected_answer.is_empty()
            || j.expected_answer.chars().count() > 256
            || j.prompt_tokens.is_empty()
            || j.prompt_tokens.iter().any(|t| *t >= p.vocab)
        {
            return Err("invalid fixed generation job");
        }
        let count = j.prompt_tokens.len().checked_add(p.max_new_tokens - 1).ok_or("generation extent overflow")?;
        if count > p.context as usize {
            return Err("generation context extent");
        }
        positions = positions.checked_add(count).ok_or("generation total overflow")?;
        if positions > p.max_total_positions {
            return Err("generation total position budget");
        }
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: check-source-checkpoint-generation <artifact> <independent-source.json> <protocol.json>".into());
    }
    let p: Protocol = json(&args[2])?;
    validate(&p)?;
    let protocol_hash = hash(Path::new(&args[2]))?;
    let source: Source = json(&args[1])?;
    if source.schema != "misaka.rfc02.independent-greedy-source.v1"
        || source.protocol_sha256 != protocol_hash
        || source.repository != p.repository
        || source.revision != p.revision
        || source.source_manifest_sha256 != p.source_manifest_sha256
        || source.tokenizer_sha256 != p.tokenizer_sha256
        || source.context != p.context
        || source.vocab != p.vocab
        || source.max_new_tokens != p.max_new_tokens
        || source.jobs.len() != p.task_jobs.len()
        || !source.fixed_count_generated_after_eos
        || source.producer["source_before_and_after_sha256_match"] != true
        || source.producer["arithmetic"] != "float32 eager CPU"
        || source.producer["trust_remote_code"] != false
        || source.complete_task_benchmark_or_maximum_context_proven
        || source.native_final_or_a6_a8_proven
        || source.full_rfc02_complete
    {
        return Err("independent generation source scope/byte pins differ".into());
    }
    for (j, s) in p.task_jobs.iter().zip(&source.jobs) {
        if j.id != s.id
            || j.task != s.task
            || j.prompt_tokens != s.prompt_tokens
            || s.generated_tokens.len() != p.max_new_tokens
            || s.generated_tokens.iter().any(|id| *id >= p.vocab)
            || s.first_eos_index != s.generated_tokens.iter().position(|id| p.eos_token_ids.contains(id))
            || s.answer_text.chars().count() > 4096
            || (s.predeclared_task_answer_passed && s.first_eos_index.is_none())
        {
            return Err("independent generation source job differs".into());
        }
    }
    let artifact_path = Path::new(&args[0]);
    if hash(artifact_path)? != p.artifact_sha256 {
        return Err("generation artifact byte identity".into());
    }
    let artifact = TirArtifactV1::open(artifact_path)?;
    let meta: serde_json::Value = serde_json::from_str(&artifact.container().header.meta)?;
    // Historical pilot containers used a string; runtime-pack containers use
    // the converter's exact one-field MathModeV1 image. Both identify the same
    // portable provenance, and neither changes committed TIR arithmetic.
    let portable_math = meta["math"] == "libm-v1"
        || meta["math"].as_object().is_some_and(|m| m.len() == 1 && m.get("mode").is_some_and(|v| v == "libm-v1"));
    if artifact.plan().program.token_bound != p.vocab || artifact.plan().program.history_bound < p.context || !portable_math
    {
        return Err("generation artifact vocabulary/context/arithmetic".into());
    }
    let started = std::time::Instant::now();
    let (mut equal_tokens, mut exact_jobs) = (0usize, 0usize);
    let mut jobs = Vec::new();
    for (j, s) in p.task_jobs.iter().zip(&source.jobs) {
        let mut ex = TirExecutor::new(artifact.plan(), artifact.params())?;
        for token in &j.prompt_tokens {
            ex.step(*token, &mut NoSink)?;
        }
        let mut generated = Vec::with_capacity(p.max_new_tokens);
        for n in 0..p.max_new_tokens {
            let (_, logits) = ex.logits();
            if logits.len() != p.vocab as usize {
                return Err("generation logit extent".into());
            }
            let mut best = 0usize;
            for i in 1..logits.len() {
                if logits.get(i) > logits.get(best) {
                    best = i;
                }
            }
            generated.push(best as u32);
            if n + 1 < p.max_new_tokens {
                ex.step(best as u32, &mut NoSink)?;
            }
        }
        equal_tokens += generated.iter().zip(&s.generated_tokens).filter(|(a, b)| a == b).count();
        exact_jobs += usize::from(generated == s.generated_tokens);
        eprintln!("integer greedy job {}: {} tokens", j.id, generated.len());
        jobs.push(serde_json::json!({"id":j.id,"task":j.task,"prompt_tokens":j.prompt_tokens,
            "generated_tokens":generated,"source_generated_tokens":s.generated_tokens,
            "first_divergence_index":generated.iter().zip(&s.generated_tokens).position(|(a,b)| a != b)}));
    }
    if hash(artifact_path)? != p.artifact_sha256 || hash(Path::new(&args[2]))? != protocol_hash {
        return Err("generation artifact/protocol changed during execution".into());
    }
    let agreement = equal_tokens as f64 / (p.task_jobs.len() * p.max_new_tokens) as f64;
    let exact_fraction = exact_jobs as f64 / p.task_jobs.len() as f64;
    let passed = agreement >= p.token_agreement_min && exact_fraction >= p.exact_job_fraction_min;
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.independent-greedy-integer.v1",
        "protocol_sha256":protocol_hash,"artifact_sha256":p.artifact_sha256,
        "reference_json_sha256":hash(Path::new(&args[1]))?,"repository":p.repository,"revision":p.revision,
        "context":p.context,"max_new_tokens":p.max_new_tokens,"jobs":jobs,
        "token_agreement":agreement,"exact_job_fraction":exact_fraction,
        "predeclared_free_running_fidelity_passed":passed,"measurement_elapsed_ms":started.elapsed().as_millis(),
        "arithmetic":"frozen exact TIR25 typed CPU, unfused",
        "semantic_task_scoring_completed":false,"integer_reference_bit_equality_established_by_this_run":false,
        "complete_task_benchmark_or_maximum_context_proven":false,"native_final_or_a6_a8_proven":false,"full_rfc02_complete":false})
    );
    if !passed {
        return Err("predeclared free-running fidelity failed; result retained".into());
    }
    Ok(())
}
