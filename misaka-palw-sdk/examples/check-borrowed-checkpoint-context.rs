//! Re-lower a pinned decoder for a distinct context/window and borrow its actual
//! mapped integer parameters after exact declaration, instance and streamed PC
//! authentication. A different graph/root is never represented as the old
//! artifact. Source fit is an optional separately predeclared bounded diagnostic.
use misaka_palw_kernel::{
    flat_scope9::flat_resource_scope_v9, merkle3_stream::tensor_commitment_streamed_v3, trace::ParamCommitmentsV1,
};
use misaka_palw_sdk::runtime_pack::hfref::HfReference;
use misaka_palw_tir_exec::{NoSink, ParamData, Slice, TirExecutor, TirParams, TirPlan, node::TirArtifactV1};
use misaka_palw_tir_lower::{
    detmath::{MathMode, set_mode},
    fidelity::prepare,
    float_ref::ParamStore,
    lower::{FillCtx, LowerOpts},
    quant::QuantPolicy,
};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};
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
fn softmax(row: &[f64]) -> Vec<f64> {
    let m = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut v = row.iter().map(|x| libm::exp(x - m)).collect::<Vec<_>>();
    let z = v.iter().sum::<f64>();
    v.iter_mut().for_each(|x| *x /= z);
    v
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.len() != 5 && a.len() != 6 {
        return Err("usage: check-borrowed-checkpoint-context <artifact> <PC.borsh> <config.json> <new context 33..4096> <fresh canonical program> OR <predeclared protocol.json> <HF-reference directory>".into());
    }
    let started = std::time::Instant::now();
    let path = Path::new(&a[0]);
    let context: u32 = a[3].parse()?;
    if !(33..=4096).contains(&context) {
        return Err("bounded distinct context required".into());
    }
    let policy = if a.len() == 6 {
        let v: serde_json::Value = serde_json::from_slice(&bounded(Path::new(&a[4]), 64 << 10)?)?;
        let fed: Vec<Vec<usize>> = serde_json::from_value(v["fed_tokens"].clone())?;
        let positions = fed.iter().try_fold(0usize, |n, row| n.checked_add(row.len())).ok_or("positions overflow")?;
        let names = ["top1_min", "kl_mean_max", "slope_min", "slope_max", "correlation_min", "max_abs", "rmse_max"];
        let gates = names
            .iter()
            .map(|k| v[*k].as_f64().filter(|x| x.is_finite()).ok_or("finite predeclared gates"))
            .collect::<Result<Vec<_>, _>>()?;
        let digest = |k: &str| {
            v[k].as_str().is_some_and(|x| x.len() == 64 && x.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
        };
        if v["schema"] != "misaka.rfc02.borrowed-context-source-fidelity-protocol.v1"
            || v["artifact_sha256"] != ARTIFACT
            || v["context"].as_u64() != Some(context as u64)
            || v["repository"] != "Qwen/Qwen2.5-1.5B-Instruct"
            || v["revision"] != "989aa7980e4cf806f80c7fef2b1adb7bc71aa306"
            || v["source_manifest_sha256"] != "0c45f44e969b7c226d3fb8444ac5dff91c8d98001127cc2dcae8749c251994a3"
            || !digest("new_program_sha256")
            || !digest("tokens_sha256")
            || fed.is_empty()
            || fed.len() > 256
            || positions == 0
            || positions > 4096
            || fed.iter().any(|r| r.is_empty() || r.len() > context as usize || r.iter().any(|t| *t >= 151936))
            || positions.checked_mul(151936).is_none_or(|n| n > 64 << 20)
            || v["positions"].as_u64() != Some(positions as u64)
            || !(0.0..=1.0).contains(&gates[0])
            || gates[1] < 0.
            || gates[2] <= 0.
            || gates[3] < gates[2]
            || !(0.0..=1.0).contains(&gates[4])
            || gates[5] < 0.
            || gates[6] < 0.
        {
            return Err("preallocation policy bounds and pins".into());
        }
        let header: serde_json::Value = serde_json::from_slice(&bounded(&Path::new(&a[5]).join("hf-reference.json"), 64 << 10)?)?;
        let header_fed: Vec<Vec<usize>> = serde_json::from_value(serde_json::Value::Array(
            header["sequences"].as_array().ok_or("reference sequences")?.iter().map(|r| r["tokens"].clone()).collect(),
        ))?;
        if header["schema"] != "misaka.palw.hf-reference.v1"
            || header["vocab"].as_u64() != Some(151936)
            || header["logits_file"] != "hf-reference.f32"
            || header_fed != fed
            || std::fs::metadata(Path::new(&a[5]).join("hf-reference.f32"))?.len() != positions as u64 * 151936 * 4
        {
            return Err("reference metadata/bytes before loading logits".into());
        }
        Some(v)
    } else {
        None
    };
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
    let logits_scale = pinned_scale.as_f64().ok_or("pinned decimal scale")?;
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
    let changes=old.blocks.iter().zip(&p.blocks).enumerate().map(|(b,(old,new))|serde_json::json!({"block":b,"nodes_equal":old.nodes==new.nodes,"carry_types_equal":old.carry_in==new.carry_in&&old.carry_out==new.carry_out})).collect::<Vec<_>>();
    let old_states = old.states.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>();
    let new_states = p.states.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>();
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
            if offset as usize % width != 0 || out.len() % width != 0 {
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
    let scope_result = flat_resource_scope_v9(p, context, None);
    let scope = match scope_result {
        Ok(s) => {
            serde_json::json!({"admitted":true,"scope_root":hex(&s.root()),"relations":s.relations.len(),"court_bytes":s.worst_court_bytes,"court_work":s.worst_court_work,"court_scratch_bytes":s.worst_court_scratch_bytes})
        }
        Err(e) => serde_json::json!({"admitted":false,"refusal":format!("{e:?}")}),
    };
    let program_bytes = p.encode();
    let program_sha = hex(&Sha256::digest(&program_bytes));
    let mut result = serde_json::json!({"schema":"misaka.rfc02.borrowed-checkpoint-context.v1","artifact_sha256":ARTIFACT,"config_sha256":CONFIG,"parameter_map_sha256":PC,"source_program_sha256":OLD_PROGRAM,"new_program_sha256":program_sha,"new_program_root":hex(&misaka_palw_kernel::public::program_root_v1(&program_bytes)),"old_context":32,"new_context":context,"lowering_options":{"old_max_window":32,"new_max_window":context,"math":"libm-v1","history_bound":p.history_bound},"block_changes":changes,"old_states":old_states,"new_states":new_states,"parameter_declarations_equal":true,"parameter_instances_equal":true,"schedule_equal":true,"consts_equal":p.consts==old.consts,"new_params_borrow_mapped_native_slices":true,"logits_scale_as_parsed":logits_scale,"logits_scale_pinned_decimal":"5.960464477539063e-8","reused_calibration_context":32,"fresh_context_recalibration_performed":false,"row_parameter_and_scale_site_recipes_equal":true,"independently_reconstructed_rope_tables":rope_tables,"each_borrowed_instance_matches_registered_root":true,"parameter_instances":pc.by_instance.len(),"raw_parameter_bytes_read":raw_bytes,"parameter_root":hex(&pc.root()),"scope9":scope,"new_full_artifact_created":false,"native_registration_or_final_proven":false,"a6_a8_cohort_credit":false,"full_rfc02_complete":false});
    if a.len() == 5 {
        std::fs::OpenOptions::new().write(true).create_new(true).open(&a[4])?.write_all(&program_bytes)?;
    } else {
        let protocol = policy.as_ref().ok_or("missing predeclared source policy")?;
        if protocol["schema"] != "misaka.rfc02.borrowed-context-source-fidelity-protocol.v1"
            || protocol["new_program_sha256"] != program_sha
            || protocol["artifact_sha256"] != ARTIFACT
            || protocol["context"].as_u64() != Some(context as u64)
        {
            return Err("predeclared new graph/context policy".into());
        }
        let reference = HfReference::load(Path::new(&a[5]))?;
        reference.validate()?;
        if reference.positions() > 4096
            || reference.vocab != p.token_bound as usize
            || reference.positions().checked_mul(reference.vocab).is_none_or(|n| n > 64 << 20)
            || reference.producer["source_manifest_sha256"] != protocol["source_manifest_sha256"]
            || reference.producer["token_file_sha256"] != protocol["tokens_sha256"]
            || reference.producer["repository"] != protocol["repository"]
            || reference.producer["revision"] != protocol["revision"]
            || reference.producer["reference_dtype"] != "float32"
            || reference.producer["source_before_and_after_sha256_match"] != true
        {
            return Err("reference scope/pins/bounds".into());
        }
        let fed: Vec<Vec<usize>> = serde_json::from_value(protocol["fed_tokens"].clone())?;
        if reference.sequences.iter().map(|s| &s.tokens).ne(fed.iter())
            || fed.is_empty()
            || fed.iter().any(|s| s.is_empty() || s.len() > context as usize || s.iter().any(|t| *t >= reference.vocab))
            || protocol["positions"].as_u64() != Some(reference.positions() as u64)
        {
            return Err("exact fed sequences/positions".into());
        }
        let names = ["top1_min", "kl_mean_max", "slope_min", "slope_max", "correlation_min", "max_abs", "rmse_max"];
        let gates = names
            .map(|k| protocol[k].as_f64().filter(|v| v.is_finite()).ok_or("finite predeclared gates"))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        if !(0.0..=1.0).contains(&gates[0])
            || !(0.0..=1.0).contains(&gates[4])
            || gates[1] < 0.
            || gates[2] <= 0.
            || gates[3] < gates[2]
            || gates[5] < 0.
            || gates[6] < 0.
        {
            return Err("invalid predeclared gates".into());
        }
        let (mut n, mut sx, mut sy, mut sxx, mut syy, mut sxy, mut square, mut maximum, mut kl) =
            (0., 0., 0., 0., 0., 0., 0., 0f64, 0.);
        let (mut agree, mut positions) = (0usize, 0usize);
        for seq in &reference.sequences {
            let mut ex = TirExecutor::new(&plan, &params)?;
            for (pos, t) in seq.tokens.iter().enumerate() {
                ex.step(*t as u32, &mut NoSink)?;
                let (_, out) = ex.logits();
                let row = out.to_i128s().into_iter().map(|v| v as f64 * logits_scale).collect::<Vec<_>>();
                let want =
                    seq.logits[pos * reference.vocab..(pos + 1) * reference.vocab].iter().map(|v| *v as f64).collect::<Vec<_>>();
                if row.len() != want.len() || row.iter().any(|v| !v.is_finite()) {
                    return Err("logits shape/nonfinite".into());
                }
                let (mut ix, mut iy) = (0, 0);
                for i in 0..row.len() {
                    let (x, y) = (row[i], want[i]);
                    n += 1.;
                    sx += x;
                    sy += y;
                    sxx += x * x;
                    syy += y * y;
                    sxy += x * y;
                    square += (x - y) * (x - y);
                    maximum = maximum.max((x - y).abs());
                    if x > row[ix] {
                        ix = i
                    }
                    if y > want[iy] {
                        iy = i
                    }
                }
                agree += usize::from(ix == iy);
                positions += 1;
                let (q, r) = (softmax(&row), softmax(&want));
                kl += r.iter().zip(q).filter(|(v, _)| **v > 0.).map(|(v, q)| v * libm::log(v / q.max(1e-300))).sum::<f64>();
            }
        }
        let (vx, vy) = (sxx - sx * sx / n, syy - sy * sy / n);
        let slope = (sxy - sx * sy / n) / vx;
        let correlation = (sxy - sx * sy / n) / (vx * vy).sqrt();
        let rmse = (square / n).sqrt();
        let top1 = agree as f64 / positions as f64;
        let mean_kl = kl / positions as f64;
        let metrics = [top1, mean_kl, slope, correlation, maximum, rmse];
        let mut failures = Vec::new();
        if metrics.iter().any(|v| !v.is_finite()) {
            failures.push("nonfinite")
        };
        if top1 < gates[0] {
            failures.push("top1")
        };
        if mean_kl > gates[1] {
            failures.push("kl_mean")
        };
        if slope < gates[2] || slope > gates[3] {
            failures.push("slope")
        };
        if correlation < gates[4] {
            failures.push("correlation")
        };
        if maximum > gates[5] {
            failures.push("max_abs")
        };
        if rmse > gates[6] {
            failures.push("rmse")
        };
        result["source_fidelity"] = serde_json::json!({"reference_json_sha256":hash(&Path::new(&a[5]).join("hf-reference.json"))?,"reference_logits_sha256":hash(&Path::new(&a[5]).join("hf-reference.f32"))?,"protocol_sha256":hash(Path::new(&a[4]))?,"sequences":fed.len(),"positions":positions,"top1":top1,"kl_mean":mean_kl,"slope":slope,"correlation":correlation,"max_abs":maximum,"rmse":rmse,"predeclared_gates_passed":failures.is_empty(),"failures":failures,"complete_task_or_original_maximum_context_proven":false});
    }
    if hash(path)? != ARTIFACT {
        return Err("artifact changed during context diagnostic".into());
    }
    result["elapsed_ms"] = serde_json::json!(started.elapsed().as_millis());
    println!("{result}");
    if result["source_fidelity"]["predeclared_gates_passed"] == false {
        return Err("predeclared context fidelity failed; measured result retained".into());
    }
    Ok(())
}
