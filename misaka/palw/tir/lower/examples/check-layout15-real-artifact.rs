//! Same bounded complete integer params, separate canonical programs, exact logits
//! at every fed position. No source fidelity, full context, native Final or cohort.
use misaka_palw_tir_exec::{NoSink, ParamData, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::layout_pass15::{LAYOUT_PASS_ID_V15, transpose_history_contractions_v15};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    schema: String,
    artifact_sha256: String,
    program_sha256: String,
    prompt: Vec<u32>,
    generated_count: usize,
    max_positions: u32,
    max_artifact_bytes: u64,
    max_position_tensor_bytes: u64,
    max_fixture_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutJob {
    schema: String,
    layout_pass_id: String,
    native_job_protocol_sha256: String,
    derived_program_sha256: String,
    max_sum_position_tensor_bytes_old_plus_new: u64,
    mandatory_exact_positions: usize,
    source_fidelity_full_context_final_or_population_credit: bool,
    #[serde(default)]
    score_token_ids: Vec<u32>,
}
fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}
fn hash_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut b = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    let mut bytes = vec![];
    std::fs::File::open(path)?.take((64 << 10) + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 64 << 10 {
        return Err("bounded JSON".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn payload(program: &misaka_palw_tir::TirProgramV1, positions: u32) -> u128 {
    program
        .occurrences()
        .iter()
        .map(|(b, _)| {
            program.blocks[*b as usize]
                .nodes
                .iter()
                .map(|n| n.out.resolve(positions as usize).iter().map(|n| *n as u128).product::<u128>() * n.out.dtype.width() as u128)
                .sum::<u128>()
        })
        .sum()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.len() != 3 {
        return Err("usage: check-layout15-real-artifact artifact native-job-protocol layout-job-protocol".into());
    }
    let job: Job = json(&a[1])?;
    let layout: LayoutJob = json(&a[2])?;
    if job.schema != "misaka.rfc02.actual-checkpoint-fixed-job-protocol.v1"
        || job.prompt.is_empty()
        || job.generated_count == 0
        || job.generated_count > 128
        || job.max_positions == 0
        || job.max_positions > 4096
        || job.max_artifact_bytes == 0
        || job.max_artifact_bytes > 1 << 30
        || job.max_position_tensor_bytes == 0
        || job.max_fixture_bytes == 0
        || layout.schema != "misaka.rfc02.layout15-bitexact-job-protocol.v1"
        || layout.layout_pass_id != LAYOUT_PASS_ID_V15
        || layout.source_fidelity_full_context_final_or_population_credit
        || layout.score_token_ids.len() > 8
        || layout.max_sum_position_tensor_bytes_old_plus_new == 0
        || layout.max_sum_position_tensor_bytes_old_plus_new > 1 << 30
        || job.prompt.len() + job.generated_count - 1 != layout.mandatory_exact_positions
        || layout.mandatory_exact_positions > job.max_positions as usize
        || hash_file(Path::new(&a[1]))? != layout.native_job_protocol_sha256
    {
        return Err("fixed scope or protocol pins".into());
    }
    let path = Path::new(&a[0]);
    if std::fs::metadata(path)?.len() > job.max_artifact_bytes || hash_file(path)? != job.artifact_sha256 {
        return Err("bounded artifact pin".into());
    }
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open_bounded(path, job.max_artifact_bytes, 4 << 20)?;
    if hash_bytes(&container.header.program) != job.program_sha256 {
        return Err("old program pin".into());
    }
    let original = misaka_palw_tir::TirProgramV1::decode_canonical(&container.header.program)?;
    let old_plan = TirPlan::compile(&original)?;
    let pass = transpose_history_contractions_v15(&original)?;
    let derived = pass.program.encode();
    if hash_bytes(&derived) != layout.derived_program_sha256 {
        return Err("new canonical program pin".into());
    }
    if job.prompt.iter().any(|t| *t >= pass.program.token_bound) {
        return Err("prompt vocabulary".into());
    }
    if layout.score_token_ids.iter().any(|t| *t >= pass.program.token_bound) {
        return Err("score id vocabulary".into());
    }
    let old_payload = payload(&original, job.max_positions);
    let new_payload = payload(&pass.program, job.max_positions);
    if old_payload > job.max_position_tensor_bytes as u128
        || old_payload + new_payload > layout.max_sum_position_tensor_bytes_old_plus_new as u128
    {
        return Err("combined position payload preallocation quote".into());
    }
    let plan = TirPlan::compile(&pass.program)?;
    let parameter_bytes = container.header.tensors.iter().map(|t| t.bytes as u128).sum::<u128>();
    if parameter_bytes > job.max_artifact_bytes as u128 {
        return Err("aggregate parameter byte preallocation quote".into());
    }
    let owned =
        container.header.tensors.iter().map(|t| container.read_tensor_bytes(t.param, t.layer)).collect::<Result<Vec<_>, _>>()?;
    let mut params = TirParams::new(&old_plan);
    for (t, bytes) in container.header.tensors.iter().zip(&owned) {
        params.insert(&old_plan, t.param, t.layer, ParamData::from_le_bytes(original.params[t.param as usize].dtype, bytes)?)?;
    }
    let started = std::time::Instant::now();
    let mut old = TirExecutor::new(&old_plan, &params)?;
    let mut new = TirExecutor::new(&plan, &params)?;
    let mut positions = 0usize;
    let mut step = |token: u32| -> Result<(), Box<dyn std::error::Error>> {
        old.step(token, &mut NoSink)?;
        new.step(token, &mut NoSink)?;
        if old.logits().1.to_i128s() != new.logits().1.to_i128s() {
            return Err(format!("exact logits mismatch at{positions}").into());
        }
        positions += 1;
        Ok(())
    };
    for t in &job.prompt {
        step(*t)?;
    }
    let mut generated = vec![];
    for i in 0..job.generated_count {
        let (_, values) = old.logits();
        let mut best = 0usize;
        for n in 1..values.len() {
            if values.get(n) > values.get(best) {
                best = n;
            }
        }
        generated.push(best as u32);
        if i + 1 < job.generated_count {
            old.step(best as u32, &mut NoSink)?;
            new.step(best as u32, &mut NoSink)?;
            if old.logits().1.to_i128s() != new.logits().1.to_i128s() {
                return Err(format!("exact logits mismatch at{positions}").into());
            }
            positions += 1;
        }
    }
    if positions != layout.mandatory_exact_positions || hash_file(path)? != job.artifact_sha256 {
        return Err("measured extent or after artifact identity".into());
    }
    let meta: serde_json::Value = serde_json::from_str(&container.header.meta)?;
    let scale = meta["logits_scale"].as_f64().filter(|v| v.is_finite() && *v > 0.0).ok_or("diagnostic source logit unit missing")?;
    let selected_scores = layout
        .score_token_ids
        .iter()
        .map(|id| {
            serde_json::json!({"token_id":id,
        "integer_raw":old.logits().1.get(*id as usize),"declared_float_units":old.logits().1.get(*id as usize) as f64*scale})
        })
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.layout15-real-artifact-bitexact.v1",
        "artifact_sha256":job.artifact_sha256,"old_program_sha256":job.program_sha256,"new_program_sha256":layout.derived_program_sha256,
        "native_job_protocol_sha256":layout.native_job_protocol_sha256,"layout_job_protocol_sha256":hash_file(Path::new(&a[2]))?,
        "positions":positions,"generated":generated,"all_position_logits_exact":true,"block_rewrites":pass.rewrites.len(),
        "original_program_nodes":original.occurrences().iter().map(|(b,_)|original.blocks[*b as usize].nodes.len()).sum::<usize>(),
        "new_program_nodes":pass.program.occurrences().iter().map(|(b,_)|pass.program.blocks[*b as usize].nodes.len()).sum::<usize>(),
        "old_position_payload_quote":old_payload,"new_position_payload_quote":new_payload,"elapsed_seconds":started.elapsed().as_secs_f64(),
        "complete_integer_parameter_bytes_loaded":parameter_bytes,"parameter_storage":"bounded owned byte arrays; same borrowed typed views in both executors",
        "last_position_selected_scores":selected_scores,"declared_logits_scale":scale,
        "artifact_before_after_sha256_match":true,"new_class_program_trace_inventory_binding_required":true,
        "native_final_source_fidelity_full_context_or_a6_a8_established":false})
    );
    Ok(())
}
