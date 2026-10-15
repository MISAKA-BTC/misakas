//! Own autoregressive integer jobs versus independently produced HF jobs.
//! Publisher-template IDs, requested maxima and first-EOS contract precede execution. This tool does not interpret
//! text or establish task quality, native eligibility, Final or cohort coverage.
use misaka_palw_tir_exec::{NoSink, ParamData, TirExecutor, TirParams, TirPlan};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Read, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    id: String,
    task: String,
    prompt_text: String,
    user_text: String,
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
    tokenizer_config_sha256: String,
    generation_config_sha256: String,
    chat_template_sha256: String,
    decode_contract: String,
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
    actual_generated_length: usize,
    model_fed_tokens: Vec<u32>,
    actual_fed_length: usize,
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
    decode_contract: String,
    actual_generated_length: usize,
    requested_max_tokens_per_job: usize,
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
    if p.schema != "misaka.rfc02.greedy-task-protocol.v2"
        || p.decode_contract != "greedy-first-EOS-no-padding.v2"
        || [&p.tokenizer_config_sha256, &p.generation_config_sha256, &p.chat_template_sha256].iter().any(|s| !hex(s, 64))
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
            || j.user_text.is_empty()
            || j.user_text.len() > 4096
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
        return Err("usage: check-source-checkpoint-generation-v2 <artifact> <independent-source.json> <protocol.json>".into());
    }
    let p: Protocol = json(&args[2])?;
    validate(&p)?;
    let protocol_hash = hash(Path::new(&args[2]))?;
    let source: Source = json(&args[1])?;
    if source.schema != "misaka.rfc02.independent-greedy-source.v2"
        || source.protocol_sha256 != protocol_hash
        || source.repository != p.repository
        || source.revision != p.revision
        || source.source_manifest_sha256 != p.source_manifest_sha256
        || source.tokenizer_sha256 != p.tokenizer_sha256
        || source.context != p.context
        || source.vocab != p.vocab
        || source.max_new_tokens != p.max_new_tokens
        || source.jobs.len() != p.task_jobs.len()
        || source.fixed_count_generated_after_eos
        || source.decode_contract != p.decode_contract
        || source.requested_max_tokens_per_job != p.max_new_tokens
        || source.actual_generated_length != source.jobs.iter().map(|j| j.generated_tokens.len()).sum::<usize>()
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
            || s.generated_tokens.is_empty()
            || s.generated_tokens.len() > p.max_new_tokens
            || s.actual_generated_length != s.generated_tokens.len()
            || s.model_fed_tokens
                != j.prompt_tokens
                    .iter()
                    .chain(s.generated_tokens[..s.generated_tokens.len().saturating_sub(1)].iter())
                    .copied()
                    .collect::<Vec<_>>()
            || s.actual_fed_length != j.prompt_tokens.len() + s.generated_tokens.len().saturating_sub(1)
            || s.first_eos_index.is_some_and(|i| i + 1 != s.generated_tokens.len())
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
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open_bounded(artifact_path, 1 << 30, 4 << 20)?;
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&container.header.program)?;
    let plan = TirPlan::compile(&program)?;
    let parameter_bytes = container.header.tensors.iter().map(|t| t.bytes as u128).sum::<u128>();
    if parameter_bytes > 1 << 30 {
        return Err("aggregate integer parameter byte cap".into());
    }
    let max_positions = p.task_jobs.iter().map(|j| j.prompt_tokens.len() + p.max_new_tokens - 1).max().unwrap();
    let payload = program
        .occurrences()
        .iter()
        .map(|(b, _)| {
            program.blocks[*b as usize]
                .nodes
                .iter()
                .map(|n| n.out.resolve(max_positions).iter().map(|n| *n as u128).product::<u128>() * n.out.dtype.width() as u128)
                .sum::<u128>()
        })
        .sum::<u128>();
    if payload > 512 << 20 {
        return Err("position tensor payload preallocation quote".into());
    }
    let owned =
        container.header.tensors.iter().map(|t| container.read_tensor_bytes(t.param, t.layer)).collect::<Result<Vec<_>, _>>()?;
    let mut params = TirParams::new(&plan);
    for (t, bytes) in container.header.tensors.iter().zip(&owned) {
        params.insert(&plan, t.param, t.layer, ParamData::from_le_bytes(program.params[t.param as usize].dtype, bytes)?)?;
    }
    let meta: serde_json::Value = serde_json::from_str(&container.header.meta)?;
    // Historical pilot containers used a string; runtime-pack containers use
    // the converter's exact one-field MathModeV1 image. Both identify the same
    // portable provenance, and neither changes committed TIR arithmetic.
    let portable_math = meta["math"] == "libm-v1"
        || meta["math"].as_object().is_some_and(|m| m.len() == 1 && m.get("mode").is_some_and(|v| v == "libm-v1"));
    if plan.program.token_bound != p.vocab || plan.program.history_bound < p.context || !portable_math {
        return Err("generation artifact vocabulary/context/arithmetic".into());
    }
    let started = std::time::Instant::now();
    let (mut equal_tokens, mut exact_jobs, mut compared_slots) = (0usize, 0usize, 0usize);
    let mut jobs = Vec::new();
    for (j, s) in p.task_jobs.iter().zip(&source.jobs) {
        let mut ex = TirExecutor::new(&plan, &params)?;
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
            if p.eos_token_ids.contains(&(best as u32)) {
                break;
            }
            if n + 1 < p.max_new_tokens {
                ex.step(best as u32, &mut NoSink)?;
            }
        }
        compared_slots += generated.len().max(s.generated_tokens.len());
        equal_tokens += generated.iter().zip(&s.generated_tokens).filter(|(a, b)| a == b).count();
        exact_jobs += usize::from(generated == s.generated_tokens);
        eprintln!("integer greedy job {}: {} tokens", j.id, generated.len());
        jobs.push(serde_json::json!({"id":j.id,"task":j.task,"prompt_tokens":j.prompt_tokens,
            "generated_tokens":generated,"actual_generated_length":generated.len(),"source_generated_tokens":s.generated_tokens,
            "model_fed_tokens":j.prompt_tokens.iter().chain(generated[..generated.len()-1].iter()).copied().collect::<Vec<_>>(),
            "actual_fed_length":j.prompt_tokens.len()+generated.len()-1,
            "first_divergence_index":generated.iter().zip(&s.generated_tokens).position(|(a,b)| a != b)}));
    }
    if hash(artifact_path)? != p.artifact_sha256 || hash(Path::new(&args[2]))? != protocol_hash {
        return Err("generation artifact/protocol changed during execution".into());
    }
    let agreement = equal_tokens as f64 / compared_slots as f64;
    let exact_fraction = exact_jobs as f64 / p.task_jobs.len() as f64;
    let passed = agreement >= p.token_agreement_min && exact_fraction >= p.exact_job_fraction_min;
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.independent-greedy-integer.v2",
        "protocol_sha256":protocol_hash,"artifact_sha256":p.artifact_sha256,
        "reference_json_sha256":hash(Path::new(&args[1]))?,"repository":p.repository,"revision":p.revision,
        "context":p.context,"max_new_tokens":p.max_new_tokens,"jobs":jobs,
        "token_agreement":agreement,"exact_job_fraction":exact_fraction,
        "decode_contract":p.decode_contract,"requested_max_tokens_per_job":p.max_new_tokens,"actual_compared_output_slots":compared_slots,
        "predeclared_free_running_fidelity_passed":passed,"measurement_elapsed_ms":started.elapsed().as_millis(),
        "arithmetic":"frozen exact TIR25 typed CPU, unfused",
        "aggregate_integer_parameter_bytes":parameter_bytes,"max_position_payload_quote":payload,"parameter_storage":"bounded complete owned LE byte arrays borrowed by typed executor",
        "semantic_task_scoring_completed":false,"integer_reference_bit_equality_established_by_this_run":false,
        "complete_task_benchmark_or_maximum_context_proven":false,"native_final_or_a6_a8_proven":false,"full_rfc02_complete":false})
    );
    if !passed {
        return Err("predeclared free-running fidelity failed; result retained".into());
    }
    Ok(())
}
