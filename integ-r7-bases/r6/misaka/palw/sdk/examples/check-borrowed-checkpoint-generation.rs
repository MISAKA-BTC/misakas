//! Predeclared EOS-terminated complete short tasks on the separately lowered128
//! graph and independently authenticated borrowed actual parameters. This
//! diagnostic does not replace native fixed-count jobs or earlier greedy trials.
use misaka_palw_kernel::{merkle3_stream::tensor_commitment_streamed_v3, trace::ParamCommitmentsV1};
use misaka_palw_tir_exec::{NoSink, ParamData, Slice, TirExecutor, TirParams, TirPlan, node::TirArtifactV1};
use misaka_palw_tir_lower::{
    detmath::{MathMode, set_mode},
    fidelity::prepare,
    float_ref::ParamStore,
    lower::{FillCtx, LowerOpts},
    quant::QuantPolicy,
};
use sha2::{Digest, Sha256};
use std::{borrow::Cow, collections::BTreeMap, io::Read, path::Path};
const ARTIFACT: &str = "435ed8fcddf68a4fc7dd22792dfe912fdbc3a9b4ab1a6803a711fcb1ff834a64";
const CONFIG: &str = "98d2ff8cc47488d08a2b0b3acf4eb99ef210779b42bd48605f6b8e36acdbf670";
const PC: &str = "a45721593036993c53eb29ad1526906ca190fcdf7d7272fbf7aa3f0b62d19ef1";
const OLD_PROGRAM: &str = "16a0de98fe52cdf47364392551b15bb85a7aad76acd8b14853b1803074bca8f1";
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
fn hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}
fn bounded(path: &Path, cap: u64) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut b = Vec::new();
    std::fs::File::open(path)?.take(cap + 1).read_to_end(&mut b)?;
    if b.len() as u64 > cap {
        return Err("bounded metadata input".into());
    }
    Ok(b)
}
fn borrow(s: Slice<'_>) -> Result<ParamData<'_>, &'static str> {
    Ok(match s {
        Slice::I8(v) => ParamData::I8(Cow::Borrowed(v)),
        Slice::I16(v) => ParamData::I16(Cow::Borrowed(v)),
        Slice::I32(v) => ParamData::I32(Cow::Borrowed(v)),
        Slice::I64(v) => ParamData::I64(Cow::Borrowed(v)),
        Slice::Idx(v) => ParamData::Idx(Cow::Borrowed(v)),
        Slice::I128(_) => return Err("i128 parameter forbidden"),
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.len() != 6 {
        return Err("usage: check-borrowed-checkpoint-generation <artifact> <PC.borsh> <config.json> <canonical128 program> <predeclared task protocol.json> <independent source tasks.json>".into());
    }
    let started = std::time::Instant::now();
    let path = Path::new(&a[0]);
    let context = 128u32;
    let protocol: serde_json::Value = serde_json::from_slice(&bounded(Path::new(&a[4]), 64 << 10)?)?;
    let protocol_sha = hash(Path::new(&a[4]))?;
    let source: serde_json::Value = serde_json::from_slice(&bounded(Path::new(&a[5]), 64 << 10)?)?;
    let source_sha = hash(Path::new(&a[5]))?;
    let jobs = protocol["task_jobs"].as_array().ok_or("task list")?;
    let cap = protocol["max_new_tokens"].as_u64().ok_or("output cap")?;
    let eos: Vec<u32> = serde_json::from_value(protocol["eos_token_ids"].clone())?;
    let total_cap = protocol["max_total_positions"].as_u64().ok_or("position cap")?;
    let gates = ["token_agreement_min", "exact_job_fraction_min", "source_and_integer_task_success_min"]
        .iter()
        .map(|k| protocol[*k].as_f64().filter(|v| v.is_finite() && *v > 0. && *v <= 1.).ok_or("finite task gates"))
        .collect::<Result<Vec<_>, _>>()?;
    if protocol["schema"] != "misaka.rfc02.borrowed-context-complete-task-protocol.v1"
        || protocol["artifact_sha256"] != ARTIFACT
        || protocol["parameter_map_sha256"] != PC
        || protocol["new_program_sha256"] != "245b31fc77e8ddc730c91acc5d92cf7c6b771348a5e27a81693a7a31d780bca2"
        || protocol["context"] != 128
        || protocol["vocab"] != 151936
        || cap != 32
        || total_cap == 0
        || total_cap > 4096
        || jobs.is_empty()
        || jobs.len() > 16
        || eos.is_empty()
        || eos.len() > 8
        || eos.iter().any(|x| *x >= 151936)
        || eos.iter().collect::<std::collections::BTreeSet<_>>().len() != eos.len()
    {
        return Err("task preallocation policy/pins".into());
    }
    let mut total = 0u64;
    let mut identities = std::collections::BTreeSet::new();
    let mut prompts = Vec::new();
    for j in jobs {
        let id = j["id"].as_str().filter(|x| !x.is_empty() && x.len() <= 64).ok_or("task id")?;
        let tokens: Vec<u32> = serde_json::from_value(j["prompt_tokens"].clone())?;
        let extent = (tokens.len() as u64).checked_add(cap - 1).ok_or("task extent overflow")?;
        total = total.checked_add(extent).ok_or("task total overflow")?;
        if !identities.insert(id) || tokens.is_empty() || tokens.iter().any(|x| *x >= 151936) || extent > 128 || total > total_cap {
            return Err("task prompt/context/total preallocation bound".into());
        }
        prompts.push(tokens);
    }
    let source_jobs = source["jobs"].as_array().ok_or("source task list")?;
    if source["schema"] != "misaka.rfc02.borrowed-context-task-source.v1"
        || source["protocol_sha256"] != protocol_sha
        || source["context"] != 128
        || source["new_program_sha256"] != protocol["new_program_sha256"]
        || source["repository"] != protocol["repository"]
        || source["revision"] != protocol["revision"]
        || source["producer"]["source_before_and_after_sha256_match"] != true
        || source["producer"]["arithmetic"] != "float32 eager CPU"
        || source["producer"]["trust_remote_code"] != false
        || source_jobs.len() != jobs.len()
    {
        return Err("independent source task pins/context".into());
    }
    for ((j, s), prompt) in jobs.iter().zip(source_jobs).zip(&prompts) {
        let generated: Vec<u32> = serde_json::from_value(s["generated_tokens"].clone())?;
        let first = generated.iter().position(|x| eos.contains(x));
        if s["id"] != j["id"]
            || s["task"] != j["task"]
            || s["prompt_tokens"] != j["prompt_tokens"]
            || generated.is_empty()
            || generated.len() > cap as usize
            || generated.iter().any(|x| *x >= 151936)
            || first.is_some_and(|i| i + 1 != generated.len())
            || first.is_none() && generated.len() != cap as usize
            || prompt.is_empty()
        {
            return Err("source EOS/task bound".into());
        }
    }
    if hash(path)? != ARTIFACT || hash(Path::new(&a[1]))? != PC || hash(Path::new(&a[2]))? != CONFIG {
        return Err("pinned source inputs".into());
    }
    let pc: ParamCommitmentsV1 = borsh::from_slice(&bounded(Path::new(&a[1]), 64 << 20)?)?;
    let config = String::from_utf8(bounded(Path::new(&a[2]), 64 << 10)?)?;
    let artifact = TirArtifactV1::open(path)?;
    let old = &artifact.plan().program;
    if hex(&Sha256::digest(old.encode())) != OLD_PROGRAM {
        return Err("exact source artifact program".into());
    }
    let meta: serde_json::Value = serde_json::from_str(&artifact.container().header.meta)?;
    // Match the identical pinned decimal using the same JSON decoder as the
    // historical protocol. Its Number parser rounds this decimal one ulp above
    // a separately computed powi literal; that parser difference is not a new
    // arithmetic policy or a fidelity tolerance.
    let pinned_scale: serde_json::Value = serde_json::from_str("5.960464477539063e-8")?;

    if meta["math"] != "libm-v1" || meta["logits_scale"] != pinned_scale {
        return Err(format!(
            "pinned artifact arithmetic metadata: math={} scale={} parsed={:?} expected={:?}",
            meta["math"],
            meta["logits_scale"],
            meta["logits_scale"].as_f64(),
            2f64.powi(-24)
        )
        .into());
    }
    set_mode(MathMode::LibmV1);
    let original = prepare(&config, &LowerOpts { max_window: Some(32), ..Default::default() })?;
    if original.lowered.program != *old {
        return Err(format!(
            "fresh32 canonical program differs from mapped32; fresh digest {}",
            hex(&Sha256::digest(original.lowered.program.encode()))
        )
        .into());
    }
    let longer = prepare(&config, &LowerOpts { max_window: Some(context), ..Default::default() })?;
    let p = &longer.lowered.program;
    let canonical = bounded(Path::new(&a[3]), 16 << 20)?;
    if canonical != p.encode() || hex(&Sha256::digest(&canonical)) != protocol["new_program_sha256"].as_str().ok_or("new graph pin")? {
        return Err("fresh canonical128 graph identity".into());
    }
    if p == old
        || p.params != old.params
        || p.schedule != old.schedule
        || p.version != old.version
        || p.token_bound != old.token_bound
        || p.history_bound != old.history_bound
        || p.logits_scheme_id != old.logits_scheme_id
    {
        return Err("context lowering is not a distinct graph with identical reusable parameter/schedule declarations".into());
    }
    if longer.lowered.row_params != original.lowered.row_params
        || longer.lowered.resid_sites != original.lowered.resid_sites
        || longer.lowered.logits_key != original.lowered.logits_key
        || longer.lowered.site_nodes != original.lowered.site_nodes
    {
        return Err("reusable parameter quantization/site recipes differ".into());
    }
    // Record every actual structural difference, not a history-bound rewrite.
    let _changes=old.blocks.iter().zip(&p.blocks).enumerate().map(|(b,(old,new))|serde_json::json!({"block":b,"nodes_equal":old.nodes==new.nodes,"carry_types_equal":old.carry_in==new.carry_in&&old.carry_out==new.carry_out})).collect::<Vec<_>>();
    let _old_states = old.states.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>();
    let _new_states = p.states.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>();
    let plan = TirPlan::compile(p)?;
    if plan.param_instances != artifact.plan().param_instances
        || pc.by_instance.len() != plan.param_instances.len()
        || plan.param_instances.iter().any(|k| !pc.by_instance.contains_key(k))
    {
        return Err("exact reusable parameter-instance set".into());
    }
    // Recompute context-sensitive positional tables from the pinned config under
    // portable math, and compare them with both graph fills and actual mapped bytes.
    let empty = ParamStore::default();
    let stats = BTreeMap::new();
    let quant_policy = QuantPolicy::default();
    let ctx32 = FillCtx::for_scales(&original.hl, &empty, None, "", &stats, 1., &quant_policy);
    let ctxnew = FillCtx::for_scales(&longer.hl, &empty, None, "", &stats, 1., &quant_policy);
    let mut rope_tables = 0;
    for (j, d) in p.params.iter().enumerate().filter(|(_, d)| d.name.contains("rope")) {
        let old_fill = (original.lowered.fills[j])(&ctx32)?;
        let new_fill = (longer.lowered.fills[j])(&ctxnew)?;
        if new_fill != old_fill || new_fill.dtype != d.dtype || new_fill.shape.iter().map(|v| *v as u32).collect::<Vec<_>>() != d.shape
        {
            return Err("positional table fill changes or mismatches declaration".into());
        }
        let actual = artifact.params().get(j as u16, None).ok_or("mapped positional table")?;
        if actual.len() != new_fill.len() || (0..actual.len()).any(|i| actual.get(i) != new_fill.get(i) as i128) {
            return Err("fresh positional table differs from mapped artifact".into());
        }
        rope_tables += 1;
    }
    if rope_tables == 0 {
        return Err("no independently reconstructed positional tables".into());
    }
    let mut params = TirParams::new(&plan);
    let mut raw_bytes = 0u128;
    for &(j, l) in &plan.param_instances {
        let d = &p.params[j as usize];
        let source = artifact.params().get(j, l).ok_or("mapped instance missing")?;
        let shape = d.shape.iter().map(|x| *x as usize).collect::<Vec<_>>();
        let width = d.dtype.width();
        let (root, _) = tensor_commitment_streamed_v3(d.dtype, &shape, 32 << 20, &mut |offset, out| {
            if !(offset as usize).is_multiple_of(width) || out.len() % width != 0 {
                return Err("raw source alignment".into());
            }
            let start = usize::try_from(offset).map_err(|e| e.to_string())? / width;
            let count = out.len() / width;
            misaka_palw_tir_exec::with_slice!(source,v=>{for(x,b)in v.get(start..start+count).ok_or("raw source range")?.iter().zip(out.chunks_exact_mut(width)){b.copy_from_slice(&x.to_le_bytes());}});
            raw_bytes += out.len() as u128;
            Ok(())
        })?;
        if pc.by_instance.get(&(j, l)) != Some(&root) {
            return Err(format!("actual borrowed source root {j}/{l:?}").into());
        }
        params.insert(&plan, j, l, borrow(source)?)?;
    }
    let mut rows = Vec::new();
    let (mut equal, mut denominator, mut exact) = (0usize, 0usize, 0usize);
    for ((j, src), prompt) in jobs.iter().zip(source_jobs).zip(&prompts) {
        let mut ex = TirExecutor::new(&plan, &params)?;
        for token in prompt {
            ex.step(*token, &mut NoSink)?;
        }
        let mut generated = Vec::new();
        for n in 0..cap {
            let (_, out) = ex.logits();
            if out.len() != 151936 {
                return Err("task output vocabulary".into());
            }
            let mut best = 0usize;
            for i in 1..out.len() {
                if out.get(i) > out.get(best) {
                    best = i;
                }
            }
            generated.push(best as u32);
            if eos.contains(&(best as u32)) {
                break;
            }
            if n + 1 < cap {
                ex.step(best as u32, &mut NoSink)?;
            }
        }
        let want: Vec<u32> = serde_json::from_value(src["generated_tokens"].clone())?;
        equal += generated.iter().zip(&want).filter(|(a, b)| a == b).count();
        denominator += generated.len().max(want.len());
        exact += usize::from(generated == want);
        let stop = generated.iter().position(|x| eos.contains(x));
        eprintln!("integer complete task {}: {} output ids, completed {}", j["id"], generated.len(), stop.is_some());
        rows.push(serde_json::json!({"id":j["id"],"task":j["task"],"prompt_tokens":prompt,
            "generated_tokens":generated,"first_eos_index":stop,"completed":stop.is_some()}));
    }
    if hash(path)? != ARTIFACT || hash(Path::new(&a[4]))? != protocol_sha || hash(Path::new(&a[5]))? != source_sha {
        return Err("source/artifact/task protocol changed during diagnostic".into());
    }
    let agreement = equal as f64 / denominator as f64;
    let fraction = exact as f64 / jobs.len() as f64;
    let passed = agreement >= gates[0] && fraction >= gates[1];
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.rfc02.borrowed-context-task-integer.v1",
        "protocol_sha256":protocol_sha,"reference_json_sha256":source_sha,"repository":protocol["repository"],
        "revision":protocol["revision"],"context":128,"new_program_sha256":protocol["new_program_sha256"],
        "artifact_sha256":ARTIFACT,"parameter_root":hex(&pc.root()),"all1636_borrowed_source_parameters_authenticated":true,
        "independently_reconstructed_rope_tables":rope_tables,"raw_parameter_bytes_read":raw_bytes,
        "reused_calibration_context":32,"new_full_artifact_created":false,"max_new_tokens":cap,"jobs":rows,
        "raw_id_agreement":agreement,"exact_job_fraction":fraction,"free_running_fidelity_passed":passed,
        "elapsed_ms":started.elapsed().as_millis(),"semantic_task_scoring_completed":false,
        "native_fixed_count_semantics_replaced":false,"full_benchmark_maxcontext_native_final_or_cohort_proven":false})
    );
    if !passed {
        return Err("predeclared complete-task raw fidelity failed; result retained".into());
    }
    Ok(())
}
