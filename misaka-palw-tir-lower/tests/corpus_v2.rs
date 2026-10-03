//! **The architecture corpus v2 harness** (lane H; `docs/design/palw/tir/corpus-v2.md`).
//!
//! One question, measured on 100 Hugging Face architectures: *can this model be added
//! permissionlessly — by data alone, with no core-developer action?* The harness plays the part of a
//! THIRD PARTY: it reads a tiny random-init model `transformers`/`diffusers` made (`tools/corpus`),
//! asks the generic frontend for its support **Level**
//!
//! * **A** — the standard keys and tensor names suffice (the reader's template alone gives the
//!   spec of the family's own adapter, or the family has none and the template reads it);
//! * **B** — a data adapter (`misaka.palw.model-adapter.v1`): a built-in file, or one this lane wrote
//!   in `tools/corpus/adapters/` and the harness feeds to the reader as a user-supplied file;
//! * **C** — a capability is missing (a feature, a primitive, or a route the infrastructure lacks),
//!
//! and, where the model reads, runs every later stage: lowering, `tir_admit_v1`, the float
//! reference against `transformers`' own logits, the integer program against the float reference,
//! reference ↔ ref2 ↔ exec bit identity, and the court property (every commit point of every
//! position reproduced from its opened leaves by the cone evaluator, on the reference evaluator and
//! on the typed backend).
//!
//! Nothing here edits the lowering crate: only data (adapters, descriptors, fixtures) and this test.
//!
//! Run (one cargo command at a time on this machine):
//!
//! ```text
//! export CARGO_TARGET_DIR=~/Downloads/MISAKA-wt-b/corpus-target CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 RUST_TEST_THREADS=2
//! cargo test -p misaka-palw-tir-lower --test corpus_v2                          # quick: manifest + read-only table
//! PALW_CORPUS_REPORT=/tmp/report.json cargo test -p misaka-palw-tir-lower --test corpus_v2 -- --ignored --nocapture
//! PALW_CORPUS_ONLY=llama,qwen2 PALW_CORPUS_STAGES=read …                         # a subset, read stage only
//! ```

use misaka_palw_tir as tir;
use misaka_palw_tir::{ConeEnv, Interpreter, RunState, Tensor};
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ModelRead, ReadFailure, ReadOptions, TensorIndex, read_model};
use misaka_palw_tir_lower::lower::{IntParams, LowerOpts, materialise};
use misaka_palw_tir_lower::model::{analyze, feature_info, ArchitectureReport, FeatureStatus};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::{Confidence, ModelSpec, Reference};
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{admission, fidelity};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

// ───────────────────────────────────────── the manifest ─────────────────────────────────────────

#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)]
struct Entry {
    id: String,
    category: String,
    route: String,
    hf_arch: String,
    model_type: String,
    builder: String,
    usage: String,
    share: Option<f64>,
    why: String,
    #[serde(default)]
    examples: Vec<String>,
    #[serde(default)]
    options: Value,
    #[serde(default)]
    real_config: Option<String>,
    #[serde(default = "yes")]
    tiny: bool,
    #[serde(default)]
    note: String,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug)]
struct Manifest {
    entries: Vec<Entry>,
    #[serde(default)]
    usage_weight: BTreeMap<String, f64>,
}

fn crate_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn corpus_dir() -> PathBuf {
    crate_dir().join("tools/corpus")
}

fn manifest() -> Manifest {
    let p = std::env::var("PALW_CORPUS_MANIFEST").map(PathBuf::from).unwrap_or_else(|_| corpus_dir().join("corpus_v2.json"));
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("manifest json")
}

/// The heavy fixtures (weights + HF reference outputs): `$PALW_CORPUS_FIXTURES`, else the lane's scratch.
fn fixtures_root() -> Option<PathBuf> {
    let p = std::env::var("PALW_CORPUS_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Downloads/MISAKA-wt-b/corpus-fixtures"));
    p.is_dir().then_some(p)
}

fn read_json(p: &Path) -> Option<Value> {
    let t = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&misaka_palw_tir_lower::hf_config::sanitize_json(&t)).ok()
}

/// A tensor index from a directory's safetensors headers, else from the committed light spec.
fn tensor_index(heavy: Option<&Path>, spec_dir: &Path) -> Option<TensorIndex> {
    if let Some(d) = heavy
        && let Ok(t) = TensorIndex::from_checkpoint_path(d)
        && !t.is_empty()
    {
        return Some(t);
    }
    let v = read_json(&spec_dir.join("tensors.json"))?;
    let o = v.as_object()?;
    if o.is_empty() {
        return None;
    }
    Some(TensorIndex::from_shapes(o.iter().map(|(k, e)| {
        let shape = e.get("shape").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as usize).collect()).unwrap_or_default();
        (k.clone(), shape)
    })))
}

// ───────────────────────────────────────── helpers ─────────────────────────────────────────

fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (l, r) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if let Some(d) = first_diff(l, r, &format!("{path}.{k}")) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: {} vs {} elements", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (l, r))| first_diff(l, r, &format!("{path}[{i}]")))
        }
        (Value::Number(l), Value::Number(r)) if l.as_f64() == r.as_f64() => None,
        _ if a == b => None,
        _ => Some(format!("{path}: {a} ≠ {b}")),
    }
}

/// The spec without what is informational (as `tests/adapters.rs`).
fn comparable(mut s: ModelSpec) -> Value {
    s.notes.clear();
    s.model_type.clear();
    s.families.clear();
    s.confidence = Confidence::Known;
    s.reference = Reference::Native;
    serde_json::to_value(&s).expect("json")
}

fn adapter_json(a: &misaka_palw_tir_lower::hf_schema::AdapterSource) -> Value {
    serde_json::to_value(a).unwrap_or(Value::Null)
}

fn short(s: &str, n: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= n { s } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

/// Which `Level` a report says, with the missing items.
fn report_json(r: &ArchitectureReport) -> Value {
    let features: Vec<Value> = r
        .features
        .iter()
        .map(|f| {
            json!({
                "id": f.id, "status": if f.status == FeatureStatus::Supported { "SUPPORTED" } else { "MISSING" },
                "capability": f.capability, "detail": f.detail, "layers": f.layers.len(),
            })
        })
        .collect();
    json!({
        "level": format!("{}", r.level),
        "adapter": adapter_json(&r.adapter),
        "features": features,
        "missing": r.missing.iter().map(|m| json!({"what": m.what, "why": m.why, "general_primitive": m.general_primitive})).collect::<Vec<_>>(),
        "unmapped_config_keys": r.unmapped_config_keys,
        "assumed_defaults": r.assumed_defaults,
        "new_primitive_required": r.new_consensus_primitive_required,
        "new_court_kernel_required": r.new_court_kernel_required,
        "result": match &r.result { misaka_palw_tir_lower::model::ReportResult::Lowerable => "LOWERABLE".to_string(), misaka_palw_tir_lower::model::ReportResult::NotLowerable { reason } => format!("NOT_LOWERABLE: {}", short(reason, 400)) },
    })
}

fn failure_json(f: &ReadFailure) -> Value {
    json!({
        "error": short(&f.error.to_string(), 600),
        "unmapped_config_keys": f.unmapped_config_keys,
        "missing": f.missing.iter().map(|m| json!({"what": m.what, "why": m.why, "general_primitive": m.general_primitive})).collect::<Vec<_>>(),
    })
}

/// Is every feature the spec uses supported (lowered, no protocol gap)?
fn all_supported(read: &ModelRead) -> (bool, Vec<String>) {
    let mut missing = Vec::new();
    for u in read.spec.features() {
        let ok = match feature_info(u.id.0) {
            Some(i) => matches!(i.lowering, misaka_palw_tir_lower::model::Lowering::Implemented) && matches!(i.protocol, misaka_palw_tir_lower::model::Requirement::None),
            None => false,
        };
        if !ok {
            missing.push(u.id.0.to_string());
        }
    }
    (missing.is_empty(), missing)
}

// ───────────────────────────────────────── the read stage ─────────────────────────────────────────

/// One way to read the model, in order of preference: the standard template alone (A), a built-in
/// adapter (B), a third-party adapter file (B). The first whose whole pipeline holds is the level.
struct Candidate {
    level: &'static str,
    via: String,
    read: ModelRead,
}

struct Reads {
    candidates: Vec<Candidate>,
    json: Value,
}

fn adapters_dir() -> PathBuf {
    std::env::var("PALW_CORPUS_ADAPTERS").map(PathBuf::from).unwrap_or_else(|_| corpus_dir().join("adapters"))
}

/// Read one config three ways (the built-in routing, the standard template alone, a third-party
/// adapter file) and decide the level:
/// A: the standard template alone reads it with nothing missing, and (when a built-in adapter claims
///    the class) to the very spec that adapter gives; B: a built-in or user adapter reads it with
///    nothing missing; C: neither.
fn read_stage(e: &Entry, cfg: &Value, tensors: Option<&TensorIndex>) -> Reads {
    let auto = read_model(cfg, tensors, &ReadOptions { adapter: AdapterChoice::Auto });
    // Level A reads with the built-in template; PALW_CORPUS_STANDARD=<file> reads with a CANDIDATE template
    // instead (a data file extending `standard-decoder`): the measurement of a Level A uplift.
    let standard: Option<String> = std::env::var("PALW_CORPUS_STANDARD").ok().and_then(|p| std::fs::read_to_string(p).ok());
    let none = read_model(
        cfg,
        tensors,
        &ReadOptions { adapter: standard.clone().map(AdapterChoice::Text).unwrap_or(AdapterChoice::None) },
    );
    let user_path = adapters_dir().join(format!("{}.json", e.id));
    let user_text = std::fs::read_to_string(&user_path).ok();
    // A built-in refusal (adapters/refusals.json) is checked BEFORE any adapter, so a third party's
    // adapter can never override it. The harness measures whether DATA could express the model, so
    // it reads such a configuration under a renamed architecture and says so in the report.
    let arch = cfg.get("architectures").and_then(|a| a.get(0)).and_then(Value::as_str).unwrap_or("").to_string();
    let refusal = misaka_palw_tir_lower::adapter::builtin::refusal_for(&arch);
    let bypass = refusal.is_some();
    // FR-25 (accepted): a user adapter may override a built-in refusal, and the report says so, naming the refusal.
    let refusal_note: Option<String> = refusal.as_ref().filter(|_| user_text.is_some()).map(|(missing, why)| {
        format!("user adapter overrides built-in refusal: {}{}", missing.join(", "), if why.is_empty() { String::new() } else { format!(" ({})", short(why, 160)) })
    });
    let user_cfg: Value = if bypass {
        let mut c = cfg.clone();
        c["architectures"] = json!([format!("User{arch}")]);
        c
    } else {
        cfg.clone()
    };
    let user = user_text.as_ref().map(|t| read_model(&user_cfg, tensors, &ReadOptions { adapter: AdapterChoice::Text(t.clone()) }));

    let mut j = serde_json::Map::new();
    let sum = |r: &Result<ModelRead, ReadFailure>| -> Value {
        match r {
            Ok(m) => {
                let (ok, missing) = all_supported(m);
                json!({"ok": true, "supported": ok, "missing": missing, "adapter": adapter_json(&m.adapter), "layers": m.spec.layers.len()})
            }
            Err(f) => json!({"ok": false, "failure": failure_json(f)}),
        }
    };
    j.insert("auto".into(), sum(&auto));
    j.insert("none".into(), sum(&none));
    if let Some(u) = &user {
        j.insert("user".into(), sum(u));
    }

    // Level A: the template alone, with the same spec as the claimed adapter's.
    let auto_claims = matches!(&auto, Ok(m) if !matches!(m.adapter, misaka_palw_tir_lower::hf_schema::AdapterSource::None));
    let mut a_same: Option<bool> = None;
    let mut a_diff: Option<String> = None;
    if let (Ok(a), Ok(n)) = (&auto, &none) {
        if auto_claims {
            let d = first_diff(&comparable(a.spec.clone()), &comparable(n.spec.clone()), "");
            a_same = Some(d.is_none());
            a_diff = d;
        } else {
            a_same = Some(true);
        }
    }
    // No adapter to compare with (the built-in route refused, e.g. because a key is unknown to it): a standard
    // `…ForCausalLM` class read by the candidate template stands on its own.
    if a_same.is_none() && auto.is_err() && none.is_ok() && arch.ends_with("ForCausalLM") {
        a_same = Some(true);
    }
    j.insert("refusal_bypassed_for_user_adapter".into(), json!(bypass && user_text.is_some()));
    j.insert("refusal_overridden".into(), json!(refusal_note));
    j.insert("a_same_as_adapter".into(), json!(a_same));
    j.insert("a_diff".into(), json!(a_diff));

    let none_ok = matches!(&none, Ok(m) if all_supported(m).0);
    let auto_ok = matches!(&auto, Ok(m) if all_supported(m).0);
    let user_ok = matches!(&user, Some(Ok(m)) if all_supported(m).0);

    // Every route that reads with nothing missing, best first; routes that give the same spec once.
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen: Vec<Value> = Vec::new();
    let mut push = |level: &'static str, via: String, m: ModelRead| {
        let key = comparable(m.spec.clone());
        if !seen.contains(&key) {
            seen.push(key);
            candidates.push(Candidate { level, via, read: m });
        }
    };
    if none_ok && a_same == Some(true) {
        push("A", "standard template (no adapter)".to_string(), none.ok().expect("none"));
    }
    if auto_ok {
        let m = auto.ok().expect("auto");
        let (level, via) = match &m.adapter {
            misaka_palw_tir_lower::hf_schema::AdapterSource::BuiltIn { id, .. } => ("B", format!("built-in adapter `{id}`")),
            _ => ("A", "standard template (no adapter)".to_string()),
        };
        push(level, via, m);
    }
    if user_ok {
        let m = user.expect("user").ok().expect("user read");
        let id = match &m.adapter {
            misaka_palw_tir_lower::hf_schema::AdapterSource::UserFile { id, .. } => id.clone(),
            _ => "?".into(),
        };
        let dir_name = adapters_dir().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        push(
            "B",
            match &refusal_note {
                Some(n) => format!("third-party adapter `{id}` (tools/corpus/{dir_name}); {n}"),
                None => format!("third-party adapter `{id}` (tools/corpus/{dir_name})"),
            },
            m,
        );
    }

    // The report for the best route: its features and MISSING items (Level C names them).
    let best_user = candidates.first().is_some_and(|c| c.via.starts_with("third-party"));
    let opts = if best_user {
        ReadOptions { adapter: AdapterChoice::Text(user_text.clone().unwrap_or_default()) }
    } else {
        ReadOptions { adapter: AdapterChoice::Auto }
    };
    let rep = analyze(if best_user { &user_cfg } else { cfg }, tensors, &opts);
    j.insert("report".into(), report_json(&rep));
    j.insert("claimed_level".into(), json!(candidates.first().map(|c| c.level).unwrap_or("C")));
    Reads { candidates, json: Value::Object(j) }
}

// ───────────────────────────────────────── the pipeline ─────────────────────────────────────────

struct Stage {
    ok: bool,
    ms: u128,
    data: Value,
}

fn stage<F: FnOnce() -> Result<Value, String>>(f: F) -> Stage {
    let t = Instant::now();
    match f() {
        Ok(v) => Stage { ok: true, ms: t.elapsed().as_millis(), data: v },
        Err(e) => Stage { ok: false, ms: t.elapsed().as_millis(), data: json!({"error": short(&e, 700)}) },
    }
}

fn stage_json(s: &Stage) -> Value {
    let mut v = s.data.clone();
    if let Some(o) = v.as_object_mut() {
        o.insert("ok".into(), json!(s.ok));
        o.insert("ms".into(), json!(s.ms as u64));
    }
    v
}

/// One commit: `(slot, block, layer, node, values)`.
type Commit = (u64, u8, Option<u32>, u16, Vec<i128>);

struct Collect(Vec<Commit>);
impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.push((v.slot as u64, v.block, v.layer.map(u32::from), v.node, v.data.to_i128s()));
        }
    }
}

fn ref2_dtype(d: tir::DType) -> misaka_palw_tir_ref2::DType {
    use misaka_palw_tir::DType as A;
    use misaka_palw_tir_ref2::DType as B;
    match d {
        A::I8 => B::I8,
        A::I16 => B::I16,
        A::I32 => B::I32,
        A::I64 => B::I64,
        A::I128 => B::I128,
        A::Idx => B::Idx,
    }
}

/// What the lowered program is made of: primitives, and the role of every commit point.
fn program_facts(p: &tir::TirProgramV1) -> Value {
    let mut prims: BTreeMap<&'static str, usize> = BTreeMap::new();
    let (mut nodes, mut commits) = (0usize, 0usize);
    let mut roles: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (bi, b) in p.blocks.iter().enumerate() {
        let hist_fed: BTreeSet<u16> = b
            .nodes
            .iter()
            .filter(|n| matches!(n.prim, tir::Prim::HistAppend { .. }))
            .flat_map(|n| n.inputs.iter().filter_map(|r| if let tir::Ref::Node(k) = r { Some(*k) } else { None }))
            .collect();
        let state_fed: BTreeSet<u16> = b
            .nodes
            .iter()
            .filter(|n| matches!(n.prim, tir::Prim::StateWrite { .. }))
            .flat_map(|n| n.inputs.iter().filter_map(|r| if let tir::Ref::Node(k) = r { Some(*k) } else { None }))
            .collect();
        for (ni, n) in b.nodes.iter().enumerate() {
            nodes += 1;
            *prims.entry(n.prim.name()).or_default() += 1;
            if !n.commit {
                continue;
            }
            commits += 1;
            let ni16 = ni as u16;
            let mut any = false;
            if bi as u8 == p.schedule.post && ni16 == p.logits {
                *roles.entry("logits").or_default() += 1;
                any = true;
            }
            if b.carry_out.contains(&ni16) {
                *roles.entry("carry_out").or_default() += 1;
                any = true;
            }
            if matches!(n.prim, tir::Prim::TopK { .. }) {
                *roles.entry("topk").or_default() += 1;
                any = true;
            }
            if hist_fed.contains(&ni16) {
                *roles.entry("hist_append_input").or_default() += 1;
                any = true;
            }
            if state_fed.contains(&ni16) {
                *roles.entry("state_write_input").or_default() += 1;
                any = true;
            }
            if matches!(n.prim, tir::Prim::HistAppend { .. } | tir::Prim::StateWrite { .. }) {
                *roles.entry("state_node").or_default() += 1;
                any = true;
            }
            if !any {
                *roles.entry("lowerer_cone_split").or_default() += 1;
            }
        }
    }
    json!({
        "blocks": p.blocks.len(), "layers": p.schedule.layers.len(), "nodes": nodes, "commit_points": commits,
        "params": p.params.len(), "states": p.states.len(),
        "prims": prims, "prim_kinds": prims.len(), "commit_roles": roles,
    })
}

/// Three-way equality (reference ↔ ref2 ↔ exec) of the logits and every commit point, and the court
/// property: every commit point reproduced by the cone evaluator from the other commits of its
/// occurrence, the carries, the params and the state the position started from — on the reference
/// evaluator and on the typed backend.
fn three_way_and_court(p: &tir::TirProgramV1, params: &IntParams, eval: &[Vec<usize>]) -> Result<(Value, Value), String> {
    let IntParams { tensors } = params;
    // ref2: its own decoding of the canonical bytes, its own tensors.
    let bytes = p.encode();
    let p2 = misaka_palw_tir_ref2::codec::decode_canonical(&bytes).map_err(|e| format!("ref2 refuses the program: {e:?}"))?;
    let mut params2 = misaka_palw_tir_ref2::eval::Params::new();
    for ((j, layer), t) in tensors {
        let d = &p2.params[*j as usize];
        let shape = d.shape.iter().map(|x| *x as u64).collect();
        let t2 = misaka_palw_tir_ref2::Tensor::from_le_bytes(d.dtype, shape, &t.le_bytes()).map_err(|e| format!("ref2 tensor: {e:?}"))?;
        if d.dtype != ref2_dtype(p.params[*j as usize].dtype) {
            return Err("ref2 and the reference disagree on a param's dtype".into());
        }
        params2.insert((*j, layer.map(u32::from)), t2);
    }
    // exec: the plan and borrowed little-endian params.
    let owned: Vec<((u16, Option<u16>), Vec<u8>)> = tensors.iter().map(|(k, t)| (*k, t.le_bytes())).collect();
    let plan = TirPlan::compile(p).map_err(|e| format!("exec plan: {e}"))?;
    let mut xparams = TirParams::new(&plan);
    for ((j, layer), b) in &owned {
        let data = ParamData::from_le_bytes(p.params[*j as usize].dtype, b).map_err(|e| e.to_string())?;
        xparams.insert(&plan, *j, *layer, data).map_err(|e| e.to_string())?;
    }

    let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
    let occurrences = p.occurrences();
    let bases = p.occurrence_slot_bases();
    let (mut positions, mut cones, mut cone_exec) = (0usize, 0usize, 0usize);
    let mut cone_by_prim_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
    for seq in eval {
        let mut st1 = RunState::default();
        let mut st2 = misaka_palw_tir_ref2::eval::initial_state(&p2);
        let mut exec = TirExecutor::new(&plan, &xparams).map_err(|e| e.to_string())?;
        for (pos, tok) in seq.iter().enumerate() {
            let before = st1.clone();
            let o1 = interp.step(params, &mut st1, *tok as u32).map_err(|e| format!("reference at {pos}: {e}"))?;
            let (o2, next) = misaka_palw_tir_ref2::eval::step(&p2, &params2, &st2, *tok as u64).map_err(|e| format!("ref2 at {pos}: {e:?}"))?;
            st2 = next;
            let mut sink = Collect(Vec::new());
            exec.step(*tok as u32, &mut sink).map_err(|e| format!("exec at {pos}: {e}"))?;
            let (_, xl) = exec.logits();
            let l1 = &o1.logits.data;
            if *l1 != o2.logits.data || *l1 != xl.to_i128s() {
                return Err(format!("three-way: position {pos}: the logits differ"));
            }
            let c1: Vec<Commit> = o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            let c2: Vec<Commit> = o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
            let mut c3 = sink.0;
            c3.sort_by_key(|c| c.0);
            if c1 != c2 {
                return Err(format!("three-way: position {pos}: reference and ref2 commit differently ({} vs {} commits)", c1.len(), c2.len()));
            }
            if c1 != c3 {
                return Err(format!("three-way: position {pos}: reference and exec commit differently ({} vs {} commits)", c1.len(), c3.len()));
            }
            positions += 1;

            // The court property.
            for (occ, (block, layer)) in occurrences.iter().enumerate() {
                let nn = p.blocks[*block as usize].nodes.len() as u32;
                let commits: BTreeMap<u16, Tensor> = o1
                    .commits
                    .iter()
                    .filter(|c| c.slot >= bases[occ] && c.slot < bases[occ] + nn && c.block == *block && c.layer == *layer)
                    .map(|c| (c.node, c.value.clone()))
                    .collect();
                let carry_in: BTreeMap<u8, Tensor> = if occ == 0 {
                    BTreeMap::new()
                } else {
                    let (pb, pl) = occurrences[occ - 1];
                    let prev = &p.blocks[pb as usize];
                    let pn = prev.nodes.len() as u32;
                    prev.carry_out
                        .iter()
                        .enumerate()
                        .map(|(k, n)| {
                            let v = o1
                                .commits
                                .iter()
                                .find(|c| c.block == pb && c.layer == pl && c.node == *n && c.slot >= bases[occ - 1] && c.slot < bases[occ - 1] + pn)
                                .ok_or_else(|| format!("no carry-out commit for occurrence {}", occ - 1));
                            v.map(|v| (k as u8, v.value.clone()))
                        })
                        .collect::<Result<_, _>>()?
                };
                let is_layer = layer.is_some();
                let fixed: BTreeMap<u16, Tensor> = p
                    .states
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.per_layer == is_layer && matches!(s.kind, tir::StateKind::Fixed { .. }))
                    .map(|(j, s)| {
                        let v = before
                            .fixed
                            .get(&(j as u16, *layer))
                            .cloned()
                            .unwrap_or_else(|| Tensor::zeros(s.dtype, &s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>()));
                        (j as u16, v)
                    })
                    .collect();
                let hist_prior: BTreeMap<u16, Vec<Tensor>> = p
                    .states
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.per_layer == is_layer && matches!(s.kind, tir::StateKind::Hist { .. }))
                    .map(|(j, _)| (j as u16, before.hist.get(&(j as u16, *layer)).map(|r| r.iter().cloned().collect()).unwrap_or_default()))
                    .collect();
                for (node, value) in &commits {
                    let mut supplied = commits.clone();
                    supplied.remove(node);
                    let env = ConeEnv { token: Some(*tok as u32), pos: before.pos, carry_in: carry_in.clone(), fixed: fixed.clone(), hist_prior: hist_prior.clone(), supplied };
                    let got = interp
                        .eval_cone(*block, *layer, *node, params, &env)
                        .map_err(|e| format!("court: block {block} layer {layer:?} node {node} at pos {}: the cone does not evaluate: {e}", before.pos))?;
                    if &got != value {
                        return Err(format!("court: block {block} layer {layer:?} node {node} at pos {}: the cone gives a different value", before.pos));
                    }
                    cones += 1;
                    let gx = misaka_palw_tir_exec::eval_cone(&plan, &xparams, *block, *layer, *node, &env)
                        .map_err(|e| format!("court (exec): block {block} layer {layer:?} node {node}: {e}"))?;
                    if &gx != value {
                        return Err(format!("court (exec): block {block} layer {layer:?} node {node} at pos {}: the cone gives a different value", before.pos));
                    }
                    cone_exec += 1;
                    *cone_by_prim_kind.entry(p.blocks[*block as usize].nodes[*node as usize].prim.name()).or_default() += 1;
                }
            }
        }
    }
    Ok((
        json!({"positions": positions, "sequences": eval.len(), "equal": true}),
        json!({"commit_points_replayed": cones, "replayed_on_exec": cone_exec, "all_equal": true, "by_terminal_primitive": cone_by_prim_kind}),
    ))
}

/// **A decoder with cross-attention layers over declared vision states** (`ATTN_CROSS_V1`): when the fixture holds `cross.json`
/// (transformers over the same checkpoint with seeded `cross_attention_states`; the tower is not run), the text stage is lowered
/// with those rows declared as an input and run as the two-stage program (stage 0's K/V stack, then the text stage): the float
/// reference against HF's logits, the integer program against the float one, the three implementations and the court on both
/// stages, and admission of both. The text-only stages above are unchanged.
fn run_cross(read: &ModelRead, dir: &Path, ck: &Checkpoint) -> BTreeMap<&'static str, Stage> {
    use misaka_palw_tir_lower::lower::cross::{self, STATES_PARAM, XKV_PARAM};
    use misaka_palw_tir_lower::lower::{IntTensor, lower};
    let mut st: BTreeMap<&'static str, Stage> = BTreeMap::new();
    // The heavy fixture dir's `cross.json`, else the repository's own (`tests/fixtures/hf/<id>/cross.json`, generated by
    // `tools/gen_mllama_cross_fixture.py`), which counts only beside the very same checkpoint bytes — the corpus reproduces from the repo.
    let in_repo = dir.file_name().map(|n| crate_dir().join("tests/fixtures/hf").join(n));
    let meta = read_json(&dir.join("cross.json")).or_else(|| {
        let r = in_repo.as_ref()?;
        let same = std::fs::read(r.join("model.safetensors")).ok()? == std::fs::read(dir.join("model.safetensors")).ok()?;
        same.then(|| read_json(&r.join("cross.json"))).flatten()
    });
    let Some(meta) = meta else { return st };
    let unit = 1.0 / 4096.0;
    let mut ctx: Option<(ModelSpec, Vec<usize>, Vec<Vec<f32>>, IntTensor)> = None;
    let s = stage(|| {
        let mut spec = read.spec.clone();
        let rows = meta["rows"].as_u64().ok_or("rows")? as usize;
        spec.cross_states = Some(misaka_palw_tir_lower::spec::CrossStatesSpec { rows });
        let d = spec.hidden_size;
        let ints: Vec<i32> = meta["cross_states"].as_array().ok_or("cross_states")?.iter().flat_map(|r| r.as_array().cloned().unwrap_or_default()).map(|x| (x.as_f64().unwrap_or(0.0) / unit).round() as i32).collect();
        let states: Vec<Vec<f32>> = ints.chunks(d).map(|r| r.iter().map(|q| (*q as f64 * unit) as f32).collect()).collect();
        let tokens: Vec<usize> = meta["tokens"].as_array().ok_or("tokens")?.iter().map(|t| t.as_u64().unwrap_or(0) as usize).collect();
        let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| e.to_string())?;
        let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).map_err(|e| e.to_string())?;
        let (params, unused) = ParamStore::from_source(&hl, &binding, ck).map_err(|e| e.to_string())?;
        if !unused.is_empty() {
            return Err(format!("checkpoint tensors the program never reads: {}", short(&format!("{unused:?}"), 300)));
        }
        let want: Vec<Vec<f64>> = meta["logits_full"].as_array().ok_or("logits")?.iter().map(|r| r.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default()).collect();
        let mut sess = Session::new(&hl, &params);
        sess.cross_states = Some(states.clone());
        let got = sess.run(&tokens).map_err(|e| format!("float reference: {e}"))?;
        let scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
        let max_abs = got.iter().zip(&want).flat_map(|(g, w)| g.iter().zip(w).map(|(a, b)| (*a as f64 - b).abs())).fold(0f64, f64::max);
        let rel = max_abs / scale;
        if rel > 1e-4 {
            return Err(format!("float reference with declared states vs HF: max|Δ|/scale {rel:.2e}"));
        }
        ctx = Some((spec, tokens, states, IntTensor::i32(vec![rows, d], ints)));
        Ok(json!({"positions": got.len(), "rows": rows, "max_rel_vs_hf": rel}))
    });
    let ok = s.ok;
    st.insert("cross_float_vs_hf", s);
    if !ok {
        return st;
    }
    let (spec, tokens, states, states_i) = ctx.expect("cross context");
    let mut stages_opt = None;
    let s = stage(|| {
        let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| e.to_string())?;
        let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).map_err(|e| e.to_string())?;
        let (params, _) = ParamStore::from_source(&hl, &binding, ck).map_err(|e| e.to_string())?;
        let d = spec.hidden_size;
        let rows = states.len();
        let mut stats: BTreeMap<String, misaka_palw_tir_lower::float_ref::SiteStat> = Default::default();
        let mut rng = 17u64;
        let mut runs: Vec<(Vec<usize>, Vec<Vec<f32>>)> = vec![(tokens.clone(), states.clone())];
        for sq in fidelity::random_sequences(hl.vocab, 4, 24, 11) {
            let r: Vec<Vec<f32>> = (0..rows)
                .map(|_| {
                    (0..d)
                        .map(|_| {
                            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                            (((rng >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0) as f32 * 1.2
                        })
                        .collect()
                })
                .collect();
            runs.push((sq, r));
        }
        for (sq, rs) in runs {
            let mut sess = Session::new(&hl, &params).with_site_stats();
            sess.cross_states = Some(rs);
            sess.run(&sq).map_err(|e| format!("calibration: {e}"))?;
            for (k, v) in sess.sites.take().unwrap_or_default() {
                stats.entry(k).or_default().merge(&v);
            }
        }
        let quiet = |_: usize, _: usize| {};
        let text = lower(&hl, &LowerOpts { max_window: Some(64), ..LowerOpts::default() }).map_err(|e| format!("lower the text stage: {e}"))?;
        let mat = materialise(&text, &hl, &Resident(Arc::new(params)), &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
        let (hl0, b0) = cross::hl_cross_kv(&spec).map_err(|e| e.to_string())?;
        let (p0, _) = ParamStore::from_source(&hl0, &b0, ck).map_err(|e| e.to_string())?;
        let kv = cross::lower_cross_kv(&hl0, &spec, unit).map_err(|e| format!("lower stage 0: {e}"))?;
        let mat0 = materialise(&kv, &hl0, &Resident(Arc::new(p0)), &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise stage 0: {e}"))?;
        let inputs = admission::default_inputs();
        for (name, p) in [("stage 0", &kv.program), ("text", &text.program)] {
            admission::admit(p, &inputs).map_err(|e| format!("{name}: tir_admit_v1 refuses: {e}"))?;
        }
        let with = |p: &tir::TirProgramV1, base: &IntParams, name: &str, t: IntTensor| -> Result<IntParams, String> {
            let j = p.params.iter().position(|d| d.name == name).ok_or_else(|| format!("no input `{name}`"))?;
            let mut q = base.clone();
            q.tensors.insert((j as u16, None), t);
            Ok(q)
        };
        let p0 = with(&kv.program, &mat0.params, STATES_PARAM, states_i.clone())?;
        let interp = Interpreter::new(&kv.program).map_err(|e| e.to_string())?;
        let out = interp.step(&p0, &mut RunState::default(), 0).map_err(|e| format!("stage 0: {e}"))?;
        let ck_ = cross::CrossKv::of(&spec).map_err(|e| e.to_string())?;
        let xkv = IntTensor::i16(vec![ck_.slots.len(), 2, ck_.rows, ck_.inner()], out.logits.data.iter().map(|v| *v as i16).collect());
        let pt = with(&text.program, &mat.params, XKV_PARAM, xkv)?;
        let facts = json!({"stage0_nodes": kv.program.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(), "text_nodes": text.program.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(), "cross_layers": ck_.slots.len()});
        stages_opt = Some((hl, mat.logits_scale, text, kv, p0, pt));
        Ok(facts)
    });
    let ok = s.ok;
    st.insert("cross_lower", s);
    if !ok {
        return st;
    }
    let (hl, logits_scale, text, kv, p0, pt) = stages_opt.expect("cross stages");
    let s = stage(|| {
        let params = ParamStore::from_source(&hl, &misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).map_err(|e| e.to_string())?, ck).map_err(|e| e.to_string())?.0;
        let mut sess = Session::new(&hl, &params);
        sess.cross_states = Some(states.clone());
        let fl = sess.run(&tokens).map_err(|e| e.to_string())?;
        let il = fidelity::int_logits(&text.program, &pt, &tokens, logits_scale, &|_| {}).map_err(|e| format!("integer run: {e}"))?;
        let m = fidelity::compare(&[fl], &[il], &[tokens.clone()]);
        let v = json!({"positions": m.positions, "top1": m.top1_agreement, "kl_mean": m.kl_mean, "kl_max": m.kl_max});
        if m.top1_agreement < 0.8 || m.kl_mean > 0.02 {
            return Err(format!("integer vs float with declared states: top-1 {:.3}, KL {:.5}", m.top1_agreement, m.kl_mean));
        }
        Ok(v)
    });
    let ok = s.ok;
    st.insert("cross_int_vs_float", s);
    if !ok {
        return st;
    }
    let s = stage(|| {
        let (tw0, ct0) = three_way_and_court(&kv.program, &p0, &[vec![0usize]]).map_err(|e| format!("stage 0: {e}"))?;
        let (tw1, ct1) = three_way_and_court(&text.program, &pt, &[tokens.clone()]).map_err(|e| format!("text stage: {e}"))?;
        Ok(json!({"stage0": tw0, "text": tw1, "court_stage0": ct0, "court_text": ct1}))
    });
    st.insert("cross_three_way_court", s);
    st
}


/// Per-class thresholds of integer-vs-float fidelity (as `tests/fidelity_tiny.rs`).
fn thresholds(category: &str) -> (f64, f64) {
    match category {
        "text/moe" => (0.85, 0.02),
        "text/hybrid" => (0.8, 0.03),
        _ => (0.9, 0.01),
    }
}

/// The decoder route: spec → HL → TIR → admission → HF logits → integer fidelity → three-way + court.
fn run_decoder(e: &Entry, read: &ModelRead, dir: &Path, full: bool) -> (BTreeMap<&'static str, Stage>, Option<&'static str>) {
    let mut st: BTreeMap<&'static str, Stage> = BTreeMap::new();
    let mut failed: Option<&'static str> = None;
    macro_rules! fail_if {
        ($name:literal, $s:expr) => {{
            let s: Stage = $s;
            let ok = s.ok;
            st.insert($name, s);
            if !ok {
                failed = Some($name);
                return (st, failed);
            }
        }};
    }
    let opts = LowerOpts::default();
    let mut prepared = None;
    fail_if!(
        "lower",
        stage(|| {
            let p = fidelity::prepare_spec(read.spec.clone(), &opts).map_err(|e| e.to_string())?;
            let mut v = program_facts(&p.lowered.program);
            v["digest"] = json!(misaka_palw_tir_lower::artifact::program_digest(&p.lowered.program));
            prepared = Some(p);
            Ok(v)
        })
    );
    let prep = prepared.expect("prepared");
    fail_if!(
        "admit",
        stage(|| {
            let inputs = admission::default_inputs();
            let v = admission::admit(&prep.lowered.program, &inputs);
            let j = admission::to_json(&prep.lowered.program, &inputs, &v);
            match v {
                Ok(_) => Ok(j),
                Err(e) => Err(format!("tir_admit_v1 refuses: {e}")),
            }
        })
    );
    if !full || !dir.join("model.safetensors").exists() && !dir.join("model.safetensors.index.json").exists() {
        st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": false, "note": "no weights: lowering and admission only"}) });
        return (st, failed);
    }
    let ck = match Checkpoint::open(dir) {
        Ok(c) => c,
        Err(err) => {
            st.insert("fixture", Stage { ok: false, ms: 0, data: json!({"error": err.to_string()}) });
            return (st, Some("fixture"));
        }
    };
    st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": true}) });
    let mut params_opt = None;
    fail_if!(
        "bind",
        stage(|| {
            let (params, unused) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
            if !unused.is_empty() {
                return Err(format!("checkpoint tensors the program never reads: {}", short(&format!("{unused:?}"), 300)));
            }
            params_opt = Some(params);
            Ok(json!({"unused_tensors": 0}))
        })
    );
    let params = params_opt.expect("params");
    // The float reference against transformers' own logits.
    let reference = read_json(&dir.join("reference.json"));
    match &reference {
        Some(meta) if meta.get("tokens").is_some() => {
            let s = stage(|| {
                let tokens: Vec<usize> = meta["tokens"].as_array().ok_or("tokens")?.iter().map(|t| t.as_u64().unwrap_or(0) as usize).collect();
                let (key, which) = if meta.get("logits_decode").is_some() { ("logits_decode", "decode") } else { ("logits_full", "full") };
                let want: Vec<Vec<f64>> = meta[key]
                    .as_array()
                    .ok_or("logits")?
                    .iter()
                    .map(|r| r.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default())
                    .collect();
                let got = Session::new(&prep.hl, &params).run(&tokens).map_err(|e| format!("float reference: {e}"))?;
                let scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
                let (mut max_abs, mut near_ties, mut top1_miss) = (0f64, 0usize, 0usize);
                for (p, (g, w)) in got.iter().zip(&want).enumerate() {
                    if g.len() != w.len() {
                        return Err(format!("position {p}: {} logits vs HF's {}", g.len(), w.len()));
                    }
                    let row = g.iter().zip(w).fold(0f64, |m, (a, b)| m.max((*a as f64 - b).abs()));
                    max_abs = max_abs.max(row);
                    let arg = |v: &[f64]| v.iter().enumerate().fold(0, |bi, (i, x)| if *x > v[bi] { i } else { bi });
                    let gw: Vec<f64> = g.iter().map(|x| *x as f64).collect();
                    if arg(&gw) != arg(w) {
                        let mut s = w.clone();
                        s.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
                        if s[0] - s[1] > 1e-3 * scale {
                            top1_miss += 1;
                        } else {
                            near_ties += 1;
                        }
                    }
                }
                let rel = max_abs / scale;
                if top1_miss > 0 || rel > 1e-4 {
                    return Err(format!("float reference vs HF ({which}): max|Δ|/scale {rel:.2e}, {top1_miss} argmax differences beyond a near-tie"));
                }
                // FR-24: a remote-code architecture's reference is the registrant's / the corpus's own file run through trust_remote_code,
                // not a transformers class: the stage passes and says what it was passed against.
                let remote = matches!(read.spec.reference, Reference::RemoteCode { .. });
                let mut j = json!({"positions": got.len(), "reference": which, "max_rel_vs_hf": rel, "near_ties": near_ties});
                if remote {
                    j["reference_unverified"] = json!(true);
                    j["reference_kind"] = json!("remote code (trust_remote_code, local files): the corpus's reconstruction of the repository's modelling file, not the repository's");
                }
                Ok(j)
            });
            let ok = s.ok;
            st.insert("float_vs_hf", s);
            if !ok {
                return (st, Some("float_vs_hf"));
            }
        }
        _ => {
            st.insert("float_vs_hf", Stage { ok: true, ms: 0, data: json!({"skipped": "no reference.json"}) });
        }
    }
    // Calibrate, materialise, integer vs float.
    let loader = Resident(Arc::new(params));
    let vocab = prep.hl.vocab;
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let quiet = |_: usize, _: usize| {};
    let mut mat_opt = None;
    fail_if!(
        "materialise",
        stage(|| {
            let calib = fidelity::random_sequences(vocab, 6, 32.min(max_len), 7);
            let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
            let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
            tir::interval::analyze_ranges(&prep.lowered.program).map_err(|e| format!("range analysis refuses the program: {e}"))?;
            let n = mat.params.tensors.len();
            mat_opt = Some(mat);
            Ok(json!({"tensors": n}))
        })
    );
    let mat = mat_opt.expect("materialised");
    let (min_top1, max_kl) = thresholds(&e.category);
    let s = stage(|| {
        let eval = fidelity::random_sequences(vocab, 3, 24.min(max_len), 1234);
        let fl = fidelity::float_logits(&prep.hl, &loader, &eval, &quiet).map_err(|e| e.to_string())?;
        let il: Vec<Vec<Vec<f64>>> = eval
            .iter()
            .map(|s| fidelity::int_logits_exec(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("integer run: {e}"))?;
        let m = fidelity::compare(&fl, &il, &eval);
        let v = json!({"positions": m.positions, "top1": m.top1_agreement, "kl_mean": m.kl_mean, "kl_max": m.kl_max, "ppl_delta": m.ppl_delta, "min_top1": min_top1, "max_kl": max_kl});
        if m.top1_agreement < min_top1 || m.kl_mean > max_kl {
            return Err(format!("integer vs float: top-1 {:.3} (min {min_top1}), KL {:.5} (max {max_kl})", m.top1_agreement, m.kl_mean));
        }
        Ok(v)
    });
    let ok = s.ok;
    st.insert("int_vs_float", s);
    if !ok {
        return (st, Some("int_vs_float"));
    }
    // Three-way identity and the court property.
    let eval = fidelity::random_sequences(vocab, 2, 12.min(max_len), 97);
    let mut court = None;
    let s = stage(|| {
        let (tw, ct) = three_way_and_court(&prep.lowered.program, &mat.params, &eval)?;
        court = Some(ct);
        Ok(tw)
    });
    let ok = s.ok;
    st.insert("three_way", s);
    if !ok {
        // a court failure inside the combined run is reported as the court stage
        let msg = st["three_way"].data["error"].as_str().unwrap_or("").to_string();
        if msg.starts_with("court") {
            st.insert("court", Stage { ok: false, ms: 0, data: json!({"error": msg}) });
            return (st, Some("court"));
        }
        return (st, Some("three_way"));
    }
    st.insert("court", Stage { ok: true, ms: 0, data: court.unwrap_or(Value::Null) });
    // `ATTN_CROSS_V1`: a model with cross-attention layers and a `cross.json` fixture also runs with its vision states declared.
    if read.spec.layers.iter().any(|l| matches!(l.mixer, misaka_palw_tir_lower::spec::Mixer::CrossAttention(_))) {
        for (k, v) in run_cross(read, dir, &ck) {
            let ok = v.ok;
            st.insert(k, v);
            if !ok {
                return (st, Some(k));
            }
        }
    }
    (st, failed)
}

// ───────────────────────────────────────── convention search ─────────────────────────────────────────

/// Config keys that cannot change a forward pass whatever their value (bookkeeping, dropout, token ids,
/// training-only losses). The synthesizer may mark an UNKNOWN key inert only if it is one of these by
/// name; any other unknown key is a feature or a human decision, and the search stops there.
fn benign_key(k: &str) -> bool {
    k.ends_with("_dropout")
        || k.ends_with("_pdrop")
        || k.ends_with("_token_id")
        || k.ends_with("_token_ids")
        || k.starts_with("initializer_")
        || k.starts_with("router_aux")
        || matches!(
            k,
            "use_cache" | "output_router_logits" | "output_attentions" | "output_hidden_states" | "pretraining_tp" | "attention_dropout" | "tie_word_embeddings_"
                | "sliding_window_pattern_" | "use_return_dict" | "return_dict" | "torchscript"
        )
}

/// **Convention search (adapter synthesis).** For a standard-looking `…ForCausalLM` class with no adapter:
/// start from the candidate Level A template (`a-candidates/standard-v2.json`), mark the benign unknown
/// keys inert, and enumerate the finite convention switches that are class code (today: the rotary
/// pairing). Each combination is DATA (an adapter text). The caller keeps the first whose pipeline holds.
/// Returns the candidates, and the unknown keys that are not benign (what a human or a feature must decide).
fn synthesize(cfg: &Value, tensors: Option<&TensorIndex>) -> (Vec<(String, String)>, Vec<String>) {
    let Ok(base_text) = std::fs::read_to_string(corpus_dir().join("a-candidates/standard-v2.json")) else { return (vec![], vec![]) };
    let Ok(base): Result<Value, _> = serde_json::from_str(&base_text) else { return (vec![], vec![]) };
    let unknown: Vec<String> = match read_model(cfg, tensors, &ReadOptions { adapter: AdapterChoice::Text(base_text.clone()) }) {
        Ok(_) => vec![],
        Err(f) => f.unmapped_config_keys,
    };
    let (benign, rest): (Vec<String>, Vec<String>) = unknown.into_iter().partition(|k| benign_key(k));
    if !rest.is_empty() {
        return (vec![], rest);
    }
    let mut out = Vec::new();
    for style in ["Half", "Interleaved"] {
        let mut vars = vec![];
        if style != "Half" {
            vars.push(json!({"name": "rope_style", "value": style}));
        }
        let over = json!({"id": format!("synthesized-{style}"), "config": {"inert": benign}, "vars": vars});
        let merged = misaka_palw_tir_lower::adapter::merge(base.clone(), over);
        out.push((format!("rope pairing {style}"), serde_json::to_string(&merged).unwrap_or_default()));
    }
    (out, vec![])
}

// ───────────────────────────────────────── other routes ─────────────────────────────────────────

fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let d: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    d / (na * nb)
}

fn rel_err(a: &[f64], b: &[f64]) -> f64 {
    let d: f64 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt();
    d / b.iter().map(|x| x * x).sum::<f64>().sqrt()
}


/// A bidirectional encoder (BERT family): lower with `lower::bidir`, admit the version-2 program,
/// the float reference and the integer program against the HF embedding (mean pooled, normalised).
fn run_bidir(read: &ModelRead, dir: &Path, full: bool) -> (BTreeMap<&'static str, Stage>, Option<&'static str>) {
    use misaka_palw_tir_lower::encoder;
    use misaka_palw_tir_lower::lower::bidir::{self, BidirCfg, Padded, Pooling};
    let mut st: BTreeMap<&'static str, Stage> = BTreeMap::new();
    macro_rules! fail_if {
        ($name:literal, $s:expr) => {{
            let s: Stage = $s;
            let ok = s.ok;
            st.insert($name, s);
            if !ok {
                return (st, Some($name));
            }
        }};
    }
    let reference = read_json(&dir.join("reference.json"));
    let lmax = reference.as_ref().and_then(|m| m["lmax"].as_u64()).unwrap_or(12) as u32;
    let pad = reference.as_ref().and_then(|m| m["pad"].as_u64()).unwrap_or(0) as u32;
    let cfg = BidirCfg { lmax, pooling: Pooling::Mean, normalize: true };
    let spec = &read.spec;
    let mut built = None;
    fail_if!(
        "lower",
        stage(|| {
            let hl = misaka_palw_tir_lower::hl::build_program(spec).map_err(|e| format!("hl: {e}"))?;
            let binding = misaka_palw_tir_lower::hf_weights::bind(spec, &hl).map_err(|e| format!("bind: {e}"))?;
            let lw = bidir::lower_bidir(&hl, spec, &cfg).map_err(|e| e.to_string())?;
            let p2 = encoder::bidir_v2(&lw, spec.vocab_size as u32, lmax).map_err(|e| e.to_string())?;
            let v = program_facts(&lw.program);
            built = Some((hl, binding, lw, p2));
            Ok(v)
        })
    );
    let (hl, binding, lw, p2) = built.expect("built");
    fail_if!(
        "admit",
        stage(|| {
            let inputs = admission::default_inputs();
            let pipe = encoder::bidir_pipeline(vec![], vec![], pad, lmax);
            tir::pipeline::validate_pipeline(&pipe, std::slice::from_ref(&p2)).map_err(|e| format!("pipeline normal form: {e}"))?;
            let a = tir::admit_v2::tir_admit_program_v2(&p2, &inputs).map_err(|e| format!("tir_admit_v2 refuses: {e}"))?;
            let pa = tir::admit_v2::tir_admit_pipeline_v1(&pipe.encode(), &[p2.encode()], &inputs, &tir::admit_v2::TirJobCeilingsV1::open_v1())
                .map_err(|e| format!("tir_admit_pipeline_v1 refuses: {e}"))?;
            Ok(json!({"cones": a.view.cones.len(), "job_macs": pa.job_cost.macs, "step_leaves": pa.job_step_leaves, "cone_work": pa.cone_work}))
        })
    );
    let ck = match Checkpoint::open(dir) {
        Ok(c) if full => c,
        _ => {
            st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": false}) });
            return (st, None);
        }
    };
    let Some(refm) = reference.filter(|m| m.get("sequences").is_some()) else {
        st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": false, "note": "no reference.json"}) });
        return (st, None);
    };
    st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": true}) });
    let seqs: Vec<(Padded, Vec<f64>)> = refm["sequences"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| {
            let ids: Vec<usize> = s["padded"].as_array()?.iter().map(|t| t.as_u64().unwrap_or(0) as usize).collect();
            let count = s["count"].as_u64()? as usize;
            let want: Vec<f64> = s["mean_normalized"].as_array()?.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect();
            Some((Padded { ids, count }, want))
        }).collect())
        .unwrap_or_default();
    let mut params_opt = None;
    fail_if!(
        "bind",
        stage(|| {
            let (params, unused) = ParamStore::from_source(&hl, &binding, &ck).map_err(|e| e.to_string())?;
            if !unused.is_empty() {
                return Err(format!("checkpoint tensors the program never reads: {}", short(&format!("{unused:?}"), 300)));
            }
            params_opt = Some(params);
            Ok(json!({"unused_tensors": 0}))
        })
    );
    let params_f = params_opt.expect("params");
    fail_if!(
        "float_vs_hf",
        stage(|| {
            let mut worst = 0f64;
            for (p, want) in &seqs {
                let got = bidir::float_forward(&hl, spec, &cfg, &params_f, p, None).map_err(|e| format!("float reference: {e}"))?;
                worst = worst.max(rel_err(&got, want));
            }
            if worst > 1e-4 {
                return Err(format!("float reference vs HF: rel {worst:.2e}"));
            }
            Ok(json!({"max_rel_vs_hf": worst, "sequences": seqs.len()}))
        })
    );
    let mut mat_opt = None;
    let loader = Resident(Arc::new(params_f));
    fail_if!(
        "materialise",
        stage(|| {
            let (cls, sep) = (seqs[0].0.ids[0], seqs[0].0.ids[seqs[0].0.count - 1]);
            let mut stats = BTreeMap::new();
            let pe = loader.0.clone();
            for (i, body) in fidelity::random_sequences(spec.vocab_size, 8, lmax as usize - 2, 11).into_iter().enumerate() {
                let n = 2 + (i % (lmax as usize - 2)) + 1;
                let mut ids: Vec<usize> = std::iter::once(cls).chain(body.into_iter().take(n - 2)).chain(std::iter::once(sep)).collect();
                let count = ids.len().min(lmax as usize);
                ids.truncate(lmax as usize);
                ids.resize(lmax as usize, pad as usize);
                bidir::float_forward(&hl, spec, &cfg, &pe, &Padded { ids, count }, Some(&mut stats)).map_err(|e| format!("calibration: {e}"))?;
            }
            let quiet = |_: usize, _: usize| {};
            let mat = materialise(&lw, &hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
            mat_opt = Some(mat);
            Ok(json!({}))
        })
    );
    let mat = mat_opt.expect("mat");
    fail_if!(
        "int_vs_hf",
        stage(|| {
            let params2 = encoder::lifted_params(&lw.program, &[bidir::IDS_PARAM, bidir::COUNT_PARAM], &mat.params);
            let interp = tir::interp_v2::InterpreterV2::new(&p2).map_err(|e| format!("interpreter v2: {e}"))?;
            let mut worst = 1f64;
            for (p, want) in &seqs {
                let mut inputs = tir::interp_v2::MapInputs::default();
                inputs.constant.insert(0, tir::Tensor::new(tir::DType::Idx, vec![lmax as usize], p.ids.iter().map(|t| *t as i128).collect()).map_err(|e| e.to_string())?);
                inputs.constant.insert(1, tir::Tensor::scalar(tir::DType::Idx, p.count as i128).map_err(|e| e.to_string())?);
                let run = interp.run_positions(&params2, &inputs, 1).map_err(|e| format!("v2 run: {e}"))?;
                let got: Vec<f64> = run[0].output.data.iter().map(|c| *c as f64 * mat.logits_scale).collect();
                worst = worst.min(cosine(&got, want));
            }
            if worst < 0.999 {
                return Err(format!("integer vs HF: cosine {worst:.5}"));
            }
            Ok(json!({"min_cosine_vs_hf": worst}))
        })
    );
    // The version-1 program (its two inputs are params) on the three implementations, and the court
    // property: one step per sequence computes the whole pooled vector.
    let mut court = None;
    let s = stage(|| {
        use misaka_palw_tir_lower::lower::IntTensor;
        let ids_j = lw.program.param_index(bidir::IDS_PARAM).ok_or("no ids param")?;
        let cnt_j = lw.program.param_index(bidir::COUNT_PARAM).ok_or("no count param")?;
        let (mut replayed, mut exec_replayed, mut positions) = (0u64, 0u64, 0u64);
        let mut by_prim: BTreeMap<String, u64> = BTreeMap::new();
        for (p, _) in &seqs {
            let mut ip = mat.params.clone();
            ip.tensors.insert((ids_j, None), IntTensor::idx(vec![lmax as usize], p.ids.iter().map(|t| *t as u32).collect()));
            ip.tensors.insert((cnt_j, None), IntTensor::idx(vec![], vec![p.count as u32]));
            let (tw, ct) = three_way_and_court(&lw.program, &ip, &[vec![0usize]])?;
            positions += tw["positions"].as_u64().unwrap_or(0);
            replayed += ct["commit_points_replayed"].as_u64().unwrap_or(0);
            exec_replayed += ct["replayed_on_exec"].as_u64().unwrap_or(0);
            for (k, v) in ct["by_terminal_primitive"].as_object().into_iter().flatten() {
                *by_prim.entry(k.clone()).or_default() += v.as_u64().unwrap_or(0);
            }
        }
        court = Some(json!({"commit_points_replayed": replayed, "replayed_on_exec": exec_replayed, "all_equal": true, "by_terminal_primitive": by_prim}));
        Ok(json!({"positions": positions, "sequences": seqs.len(), "equal": true}))
    });
    let ok = s.ok;
    st.insert("three_way", s);
    if !ok {
        let msg = st["three_way"].data["error"].as_str().unwrap_or("").to_string();
        if msg.starts_with("court") {
            st.insert("court", Stage { ok: false, ms: 0, data: json!({"error": msg}) });
            return (st, Some("court"));
        }
        return (st, Some("three_way"));
    }
    st.insert("court", Stage { ok: true, ms: 0, data: court.unwrap_or(Value::Null) });
    (st, None)
}

/// A causal encoder (CLIP's text tower): the decoder lowering with an Embedding output (the final row
/// at the end-of-text token); float and integer rows against HF's `text_embeds`, then the three
/// implementations and the court over the same token scan.
fn run_causal_encoder(read: &ModelRead, dir: &Path, full: bool) -> (BTreeMap<&'static str, Stage>, Option<&'static str>) {
    let mut st: BTreeMap<&'static str, Stage> = BTreeMap::new();
    macro_rules! fail_if {
        ($name:literal, $s:expr) => {{
            let s: Stage = $s;
            let ok = s.ok;
            st.insert($name, s);
            if !ok {
                return (st, Some($name));
            }
        }};
    }
    let ctx = read.spec.max_position_embeddings.unwrap_or(77) as u32;
    let opts = LowerOpts { max_window: Some(ctx), ..LowerOpts::default() };
    let mut prepared = None;
    fail_if!(
        "lower",
        stage(|| {
            let p = fidelity::prepare_spec(read.spec.clone(), &opts).map_err(|e| e.to_string())?;
            let v = program_facts(&p.lowered.program);
            prepared = Some(p);
            Ok(v)
        })
    );
    let prep = prepared.expect("prepared");
    fail_if!(
        "admit",
        stage(|| {
            let inputs = admission::default_inputs();
            let v = admission::admit(&prep.lowered.program, &inputs);
            let j = admission::to_json(&prep.lowered.program, &inputs, &v);
            v.map(|_| j).map_err(|e| format!("tir_admit_v1 refuses: {e}"))
        })
    );
    let reference = read_json(&dir.join("reference.json"));
    let ck = match Checkpoint::open(dir) {
        Ok(c) if full => c,
        _ => {
            st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": false}) });
            return (st, None);
        }
    };
    let Some(refm) = reference.filter(|m| m.get("sequences").is_some()) else {
        st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": false, "note": "no reference.json"}) });
        return (st, None);
    };
    st.insert("fixture", Stage { ok: true, ms: 0, data: json!({"available": true}) });
    let seqs: Vec<(Vec<usize>, Vec<f64>)> = refm["sequences"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| {
            let toks: Vec<usize> = s["tokens"].as_array()?.iter().map(|t| t.as_u64().unwrap_or(0) as usize).collect();
            let want: Vec<f64> = s["embeds"].as_array()?.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect();
            Some((toks, want))
        }).collect())
        .unwrap_or_default();
    if seqs.is_empty() {
        st.insert("float_vs_hf", Stage { ok: true, ms: 0, data: json!({"skipped": "the reference has no `embeds`"}) });
        return (st, None);
    }
    let mut params_opt = None;
    fail_if!(
        "bind",
        stage(|| {
            let (params, unused) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
            if !unused.is_empty() {
                return Err(format!("checkpoint tensors the program never reads: {}", short(&format!("{unused:?}"), 300)));
            }
            params_opt = Some(params);
            Ok(json!({"unused_tensors": 0}))
        })
    );
    let params = params_opt.expect("params");
    fail_if!(
        "float_vs_hf",
        stage(|| {
            let mut worst = 0f64;
            for (toks, want) in &seqs {
                let rows = Session::new(&prep.hl, &params).run(toks).map_err(|e| format!("float reference: {e}"))?;
                let last: Vec<f64> = rows.last().ok_or("no rows")?.iter().map(|x| *x as f64).collect();
                worst = worst.max(rel_err(&last, want));
            }
            if worst > 1e-4 {
                return Err(format!("float reference vs HF: rel {worst:.2e}"));
            }
            Ok(json!({"max_rel_vs_hf": worst, "sequences": seqs.len()}))
        })
    );
    let loader = Resident(Arc::new(params));
    let (bos, eos) = (seqs[0].0[0], *seqs[0].0.last().unwrap_or(&0));
    let quiet = |_: usize, _: usize| {};
    let mut mat_opt = None;
    fail_if!(
        "materialise",
        stage(|| {
            let calib: Vec<Vec<usize>> = fidelity::random_sequences(prep.hl.vocab.saturating_sub(2).max(2), 6, 12, 7)
                .into_iter()
                .map(|p| std::iter::once(bos).chain(p).chain(std::iter::once(eos)).collect())
                .collect();
            let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
            let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
            mat_opt = Some(mat);
            Ok(json!({}))
        })
    );
    let mat = mat_opt.expect("mat");
    fail_if!(
        "int_vs_hf",
        stage(|| {
            let mut worst = 1f64;
            for (toks, want) in &seqs {
                let rows = fidelity::int_logits(&prep.lowered.program, &mat.params, toks, mat.logits_scale, &|_| {}).map_err(|e| format!("integer run: {e}"))?;
                worst = worst.min(cosine(rows.last().ok_or("no rows")?, want));
            }
            if worst < 0.999 {
                return Err(format!("integer vs HF: cosine {worst:.5}"));
            }
            Ok(json!({"min_cosine_vs_hf": worst}))
        })
    );
    let eval: Vec<Vec<usize>> = seqs.iter().map(|(t, _)| t.clone()).collect();
    let mut court = None;
    let s = stage(|| {
        let (tw, ct) = three_way_and_court(&prep.lowered.program, &mat.params, &eval)?;
        court = Some(ct);
        Ok(tw)
    });
    let ok = s.ok;
    st.insert("three_way", s);
    if !ok {
        let msg = st["three_way"].data["error"].as_str().unwrap_or("").to_string();
        if msg.starts_with("court") {
            st.insert("court", Stage { ok: false, ms: 0, data: json!({"error": msg}) });
            return (st, Some("court"));
        }
        return (st, Some("three_way"));
    }
    st.insert("court", Stage { ok: true, ms: 0, data: court.unwrap_or(Value::Null) });
    (st, None)
}

/// The core routes that exist as Rust parsers (not data): encoder–decoders (`parse_encdec`) and
/// vision towers (`parse_vision`). Probed to the program and its admission.
fn probe_core_route(e: &Entry, cfg_text: &str, tensors: Option<&TensorIndex>) -> Value {
    use misaka_palw_tir_lower::lower::{encdec, vision};
    let inputs = admission::default_inputs();
    let admit = |p: &tir::TirProgramV1| -> Result<Value, String> {
        let v = admission::admit(p, &inputs);
        let j = admission::to_json(p, &inputs, &v);
        v.map(|_| j).map_err(|e| format!("tir_admit_v1 refuses: {e}"))
    };
    let r = catch_unwind(AssertUnwindSafe(|| -> Result<Value, String> {
        match e.route.as_str() {
            "encdec" => {
                let s = encdec::parse_encdec(cfg_text).map_err(|e| e.to_string())?;
                let names: BTreeSet<String> = tensors.map(|t| t.names().map(str::to_string).collect()).unwrap_or_default();
                let has = |n: &str| names.contains(n);
                let ((ehl, _), (dhl, _)) = encdec::hl_programs(&s, 16, &has).map_err(|e| format!("hl: {e}"))?;
                let enc = encdec::lower_encoder(&ehl, &s, 16).map_err(|e| format!("lower encoder: {e}"))?;
                let dec = encdec::lower_decoder(&dhl, &s, 16, 64).map_err(|e| format!("lower decoder: {e}"))?;
                let (a, b) = (admit(&enc.program)?, admit(&dec.program)?);
                // The data route (FR-18 phase 1): an adapter of kind `encdec` builds the same spec. Its spec must EQUAL the Rust
                // parser's (the oracle), and the same lowering must then lower and admit it.
                let adapter_route = (|| -> Value {
                    let cfgv: Value = match serde_json::from_str(cfg_text) {
                        Ok(v) => v,
                        Err(e) => return json!({"ok": false, "error": e.to_string()}),
                    };
                    match misaka_palw_tir_lower::hf_schema::read_encdec(&cfgv, &ReadOptions { adapter: AdapterChoice::Auto }) {
                        Err(f) => json!({"ok": false, "error": short(&f.error.to_string(), 300)}),
                        Ok(r) => {
                            let id = match &r.adapter {
                                misaka_palw_tir_lower::hf_schema::AdapterSource::BuiltIn { id, .. } => id.clone(),
                                other => format!("{other:?}"),
                            };
                            // Equal up to the last bits of a float: `1/sqrt(d)` built by the adapter's arithmetic and by `powf(-0.5)` differ by an ulp
                            // (reported in `ulp_only`); anything else is a real difference.
                            let (va, vb) = (serde_json::to_value(&s).unwrap_or(Value::Null), serde_json::to_value(&r.spec).unwrap_or(Value::Null));
                            fn near(a: &Value, b: &Value) -> bool {
                                match (a, b) {
                                    (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
                                        (Some(x), Some(y)) => x == y || (x - y).abs() <= 1e-12 * x.abs().max(y.abs()),
                                        _ => x == y,
                                    },
                                    (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| near(p, q)),
                                    (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, p)| y.get(k).is_some_and(|q| near(p, q))),
                                    _ => a == b,
                                }
                            }
                            let exact = r.spec == s;
                            let equal = exact || near(&va, &vb);
                            let low = (|| -> Result<(Value, Value), String> {
                                let ((ehl, _), (dhl, _)) = encdec::hl_programs(&r.spec, 16, &has).map_err(|e| format!("hl: {e}"))?;
                                let enc = encdec::lower_encoder(&ehl, &r.spec, 16).map_err(|e| format!("lower encoder: {e}"))?;
                                let dec = encdec::lower_decoder(&dhl, &r.spec, 16, 64).map_err(|e| format!("lower decoder: {e}"))?;
                                Ok((admit(&enc.program)?, admit(&dec.program)?))
                            })();
                            match low {
                                Ok((a, b)) => json!({"ok": equal && a["admitted"] == json!(true) && b["admitted"] == json!(true), "adapter": id, "spec_equals_rust_parser": equal, "ulp_only": equal && !exact, "spec_diff": if exact { Value::Null } else { json!(first_diff(&serde_json::to_value(&s).unwrap_or(Value::Null), &serde_json::to_value(&r.spec).unwrap_or(Value::Null), "")) }, "admit_encoder": a["admitted"], "admit_decoder": b["admitted"], "assumed_defaults": r.assumed_defaults}),
                                Err(m) => json!({"ok": false, "adapter": id, "spec_equals_rust_parser": equal, "error": short(&m, 300)}),
                            }
                        }
                    }
                })();
                Ok(json!({"parser": "lower::encdec::parse_encdec (Rust, per family)", "adapter_route": adapter_route, "encoder": program_facts(&enc.program), "decoder": program_facts(&dec.program), "admit_encoder": a["admitted"], "admit_decoder": b["admitted"]}))
            }
            "vision" => {
                let size = e.options.get("size").and_then(Value::as_u64).unwrap_or(28) as u32;
                let s = vision::parse_vision(cfg_text, Some((size, size)), None).map_err(|e| e.to_string())?;
                let (hl, _) = vision::hl_program(&s).map_err(|e| format!("hl: {e}"))?;
                let lw = vision::lower_vision(&hl, &s).map_err(|e| format!("lower: {e}"))?;
                let a = admit(&lw.program)?;
                Ok(json!({"parser": "lower::vision::parse_vision (Rust, per family)", "program": program_facts(&lw.program), "admit": a["admitted"]}))
            }
            _ => Err("no core route".into()),
        }
    }));
    match r {
        Ok(Ok(v)) => json!({"core_route": true, "detail": v}),
        Ok(Err(m)) => json!({"core_route": false, "error": short(&m, 400)}),
        Err(p) => json!({"core_route": false, "error": short(&p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default(), 300)}),
    }
}

/// **The data routes of the kinds the decoder pipeline does not run** (re-measure of `tir/generic`): a vision tower, a convolutional
/// network and an encoder-decoder (Whisper's feature-frame encoder and T5's encoder alone included) are read by an adapter of their
/// own kind (`read_vision`, `read_cnn`, `read_encdec`), lowered and ADMITTED as version-2 programs. The weights stages of these
/// routes (float against transformers, integer against it, the three implementations, the court) are the lowering crate's own tests
/// (`tests/{vision,cnn,encdec,whisper}.rs`); this probe is the read + lower + admit the harness measures every entry by.
fn probe_data_route(cfg: &Value, tensors: Option<&TensorIndex>, dir: Option<&Path>) -> Value {
    misaka_palw_tir_lower::model::route::probe_data_route(cfg, tensors, dir)
}

// ───────────────────────────────────────── per-entry driver ─────────────────────────────────────────

fn run_entry(e: &Entry, full: bool) -> Value {
    let t0 = Instant::now();
    let spec_dir = std::env::var("PALW_CORPUS_SPECS").map(PathBuf::from).unwrap_or_else(|_| corpus_dir().join("specs")).join(&e.id);
    let heavy = fixtures_root().map(|r| r.join(&e.id)).filter(|d| d.join("config.json").exists());
    let cfg_path = heavy.as_ref().map(|d| d.join("config.json")).filter(|p| p.exists()).unwrap_or_else(|| spec_dir.join("config.json"));
    let mut out = serde_json::Map::new();
    out.insert("id".into(), json!(e.id));
    out.insert("category".into(), json!(e.category));
    out.insert("route".into(), json!(e.route));
    out.insert("usage".into(), json!(e.usage));
    out.insert("share".into(), json!(e.share));
    out.insert("hf_arch".into(), json!(e.hf_arch));
    out.insert("model_type".into(), json!(e.model_type));
    out.insert("heavy_fixture".into(), json!(heavy.is_some()));
    let Some(cfg) = read_json(&cfg_path) else {
        out.insert("level".into(), json!("C"));
        out.insert("failed_stage".into(), json!("fixture"));
        out.insert("error".into(), json!(format!("no config at {}", cfg_path.display())));
        return Value::Object(out);
    };
    let tensors = tensor_index(heavy.as_deref(), &spec_dir);
    out.insert("tensors".into(), json!(tensors.as_ref().map(|t| t.len())));

    let reads = read_stage(e, &cfg, tensors.as_ref());
    out.insert("read".into(), reads.json.clone());
    let claimed = reads.json["claimed_level"].as_str().unwrap_or("C").to_string();
    out.insert("claimed_level".into(), json!(claimed));
    let mut stages = serde_json::Map::new();
    let mut failed: Option<String> = None;
    let mut level = "C".to_string();
    let mut via = "-".to_string();
    if matches!(e.route.as_str(), "encdec" | "vision") {
        let cfg_text = std::fs::read_to_string(&cfg_path).unwrap_or_default();
        out.insert("core".into(), probe_core_route(e, &misaka_palw_tir_lower::hf_config::sanitize_json(&cfg_text), tensors.as_ref()));
    }
    // The data routes of the kinds without a pipeline stage here: an adapter of the kind's own reader, lowered and admitted.
    let data_route = if matches!(e.route.as_str(), "encdec" | "vision" | "audio" | "diffusion") { Some(probe_data_route(&cfg, tensors.as_ref(), heavy.as_deref())) } else { None };
    if let Some(d) = &data_route {
        out.insert("data_route".into(), d.clone());
    }
    let data_ok = data_route.as_ref().and_then(|d| d["ok"].as_bool()).unwrap_or(false);
    let dir = heavy.as_deref().unwrap_or(Path::new("/nonexistent"));
    let encdec_adapter_ok = out.get("core").and_then(|c| c["detail"]["adapter_route"]["ok"].as_bool()).unwrap_or(false);
    let pipeline = matches!(e.route.as_str(), "decoder" | "vlm" | "encoder-bidir" | "encoder-causal");
    let mut refuted: Vec<Value> = Vec::new();
    if reads.candidates.is_empty() {
        failed = Some("read".into());
    } else if !pipeline {
        stages.insert("route".into(), json!({"ok": true, "note": format!("route `{}` has no pipeline stage in this harness yet (read stage only)", e.route)}));
        level = reads.candidates[0].level.to_string();
        via = reads.candidates[0].via.clone();
    }
    if reads.candidates.is_empty() && e.route == "encdec" && encdec_adapter_ok {
        // An adapter of kind `encdec` reads it, builds the Rust parser's spec, and the spec lowers and is admitted: Level B by data,
        // proven to LOWER AND ADMIT only (this harness has no weights stage for an encoder-decoder yet).
        let id = out["core"]["detail"]["adapter_route"]["adapter"].as_str().unwrap_or("?").to_string();
        stages.insert("route".into(), json!({"ok": true, "note": "encoder-decoder adapter route: spec equals the Rust parser's, lowered and admitted (no weights stage)"}));
        level = "B".to_string();
        via = format!("built-in encdec adapter `{id}` (lower + admit only)");
        failed = None;
    }
    if reads.candidates.is_empty() && level == "C" && data_ok {
        let d = data_route.as_ref().expect("data route");
        let n: usize = d["programs"].as_array().map(|a| a.len()).unwrap_or(0);
        stages.insert("route".into(), json!({"ok": true, "note": format!("data route: read by the adapter of its own kind, {n} program(s) lowered and admitted; weights stages are the lowering crate's tests")}));
        level = "B".to_string();
        via = format!("built-in {} adapter `{}` (read + lower + admit)", d["kind"].as_str().unwrap_or("?"), d["adapter"].as_str().unwrap_or("?"));
        failed = None;
    }
    if pipeline {
        for (ci, c) in reads.candidates.iter().enumerate() {
            let run = catch_unwind(AssertUnwindSafe(|| {
                match e.route.as_str() {
                    "encoder-bidir" => run_bidir(&c.read, dir, full),
                    "encoder-causal" => run_causal_encoder(&c.read, dir, full),
                    _ => run_decoder(e, &c.read, dir, full),
                }
            }));
            let (st, f) = match run {
                Ok(x) => x,
                Err(p) => {
                    let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                    let mut m = BTreeMap::new();
                    m.insert("panic", Stage { ok: false, ms: 0, data: json!({"error": short(&msg, 500)}) });
                    (m, Some("panic"))
                }
            };
            let this: serde_json::Map<String, Value> = st.iter().map(|(k, s)| ((*k).to_string(), stage_json(s))).collect();
            let last = ci + 1 == reads.candidates.len();
            if f.is_none() {
                stages = this;
                level = c.level.to_string();
                via = c.via.clone();
                failed = None;
                break;
            }
            let why = f.map(|n| this.get(n).and_then(|v| v["error"].as_str()).unwrap_or("").to_string()).unwrap_or_default();
            refuted.push(json!({"level": c.level, "via": c.via, "failed_stage": f, "error": short(&why, 400)}));
            if last {
                // no route reproduces the model: it is not supported (Level C), whatever the reader claimed
                stages = this;
                level = "C".to_string();
                via = format!("{} (refuted: {})", c.via, f.unwrap_or("?"));
                failed = f.map(str::to_string);
            }
        }
    }
    // Convention search: a decoder class nothing above could read or hold is given one more route, an
    // adapter SYNTHESIZED from the standard template and a finite set of conventions (data, not Rust).
    let mut synthesized: Value = Value::Null;
    if level == "C" && matches!(e.route.as_str(), "decoder") && e.hf_arch.ends_with("ForCausalLM") && std::env::var("PALW_CORPUS_NO_SYNTH").is_err() {
        let (cands, blocked_by) = synthesize(&cfg, tensors.as_ref());
        let mut tried = Vec::new();
        'search: for (what, text) in &cands {
            let Ok(read) = read_model(&cfg, tensors.as_ref(), &ReadOptions { adapter: AdapterChoice::Text(text.clone()) }) else { continue };
            if !all_supported(&read).0 {
                continue;
            }
            let run = catch_unwind(AssertUnwindSafe(|| run_decoder(e, &read, dir, full)));
            let (st, f) = match run {
                Ok(x) => x,
                Err(_) => continue,
            };
            let this: serde_json::Map<String, Value> = st.iter().map(|(k, s)| ((*k).to_string(), stage_json(s))).collect();
            tried.push(json!({"what": what, "failed_stage": f}));
            if f.is_none() {
                stages = this;
                level = "B".to_string();
                via = format!("synthesized adapter ({what})");
                failed = None;
                synthesized = json!({"what": what, "adapter": serde_json::from_str::<Value>(text).unwrap_or(Value::Null)});
                break 'search;
            }
        }
        if synthesized.is_null() {
            synthesized = json!({"tried": tried, "blocked_by_keys": blocked_by});
        }
    }
    out.insert("synthesized".into(), synthesized);
    // FR-26 (accepted): a Level A with no reference check is labelled "A (unconfirmed)"; the check is the float reference
    // against the transformers/diffusers class on the same weights (`float_vs_hf`).
    let confirmed = stages.get("float_vs_hf").and_then(|s| s["ok"].as_bool()).unwrap_or(false);
    // FR-24: a Level B whose float reference is remote code this harness cannot attest reads "B (reference unverified)": every stage that can run passed.
    let unverified_ref = stages.get("float_vs_hf").and_then(|s| s["reference_unverified"].as_bool()).unwrap_or(false);
    let level_label = if level == "A" && !confirmed {
        "A (unconfirmed)".to_string()
    } else if level == "B" && unverified_ref {
        "B (reference unverified)".to_string()
    } else {
        level.clone()
    };
    out.insert("level_label".into(), json!(level_label));
    out.insert("level".into(), json!(level));
    out.insert("via".into(), json!(via));
    out.insert("refuted_routes".into(), json!(refuted));
    out.insert("stages".into(), Value::Object(stages));
    out.insert("failed_stage".into(), json!(failed));
    out.insert("ms".into(), json!(t0.elapsed().as_millis() as u64));
    Value::Object(out)
}

fn selected(m: &Manifest) -> Vec<Entry> {
    let only: Option<BTreeSet<String>> = std::env::var("PALW_CORPUS_ONLY").ok().map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    m.entries.iter().filter(|e| only.as_ref().is_none_or(|o| o.contains(&e.id))).cloned().collect()
}

fn run_all(full: bool) -> Value {
    let m = manifest();
    let entries = selected(&m);
    let mut results = Vec::new();
    let partial = std::env::var("PALW_CORPUS_REPORT").ok().map(|p| PathBuf::from(format!("{p}.partial")));
    for (i, e) in entries.iter().enumerate() {
        let e2 = e.clone();
        let h = std::thread::Builder::new()
            .name(format!("corpus-{}", e.id))
            .stack_size(512 << 20)
            .spawn(move || run_entry(&e2, full))
            .expect("spawn");
        let r = h.join().unwrap_or_else(|_| json!({"id": e.id, "level": "C", "failed_stage": "panic"}));
        eprintln!(
            "[{:>3}/{}] {:<20} level {} {:<46} failed: {} ({} ms)",
            i + 1,
            entries.len(),
            e.id,
            r["level_label"].as_str().or(r["level"].as_str()).unwrap_or("?"),
            short(r["via"].as_str().unwrap_or(""), 46),
            r["failed_stage"].as_str().unwrap_or("-"),
            r["ms"].as_u64().unwrap_or(0)
        );
        results.push(r);
        if let Some(p) = &partial {
            let _ = std::fs::write(p, serde_json::to_vec(&json!({"entries": results})).unwrap_or_default());
        }
    }
    json!({
        "schema": "misaka.palw.corpus-report.v2",
        "transformers": "5.17.0",
        "entries": results,
        "usage_weight": m.usage_weight,
        "pack_hash": misaka_palw_tir_lower::adapter::builtin::pack_hash(),
        "full": full,
    })
}

fn write_report(v: &Value) {
    let path = std::env::var("PALW_CORPUS_REPORT").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(option_env!("CARGO_TARGET_TMPDIR").unwrap_or("/tmp")).join("corpus-report.json")
    });
    std::fs::write(&path, serde_json::to_vec_pretty(v).expect("json")).expect("write report");
    eprintln!("report: {}", path.display());
}

// ───────────────────────────────────────── the tests ─────────────────────────────────────────

#[test]
fn the_manifest_is_a_corpus_of_50_to_100_distinct_architectures() {
    if std::env::var("PALW_CORPUS_MANIFEST").is_ok() {
        return; // another manifest (the census) is not held to the corpus's shape
    }
    let m = manifest();
    let ids: BTreeSet<&str> = m.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids.len(), m.entries.len(), "duplicate ids");
    assert!((50..=100).contains(&m.entries.len()), "{} entries", m.entries.len());
    let cats: BTreeSet<&str> = m.entries.iter().map(|e| e.category.as_str()).collect();
    for want in ["text/dense", "text/moe", "text/hybrid", "encoder", "encdec", "vlm", "vision", "image-gen", "audio", "remote-code"] {
        assert!(cats.contains(want), "no entry in category {want}");
    }
    for e in &m.entries {
        assert!(m.usage_weight.contains_key(&e.usage), "{}: usage tier `{}`", e.id, e.usage);
        assert!(!e.why.is_empty(), "{}: no reason", e.id);
    }
}

/// The quick, weight-free run: every committed light spec (config + tensor names) read to a level.
#[test]
fn every_entry_reads_to_a_level_from_its_light_spec() {
    let r = run_all(false);
    write_report(&r);
    let entries = r["entries"].as_array().expect("entries");
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for x in entries {
        *counts.entry(x["level"].as_str().unwrap_or("?").to_string()).or_default() += 1;
        if x["claimed_level"] == "C" && x["route"] == "decoder" {
            // a refusal is never silent: it names a missing item or an unmapped key or gives an error
            let rep = &x["read"]["report"];
            let named = !rep["missing"].as_array().map(Vec::is_empty).unwrap_or(true)
                || !x["read"]["auto"]["failure"]["error"].is_null()
                || !x["read"]["auto"]["missing"].as_array().map(Vec::is_empty).unwrap_or(true);
            assert!(named, "{}: Level C without a named reason", x["id"]);
        }
    }
    eprintln!("levels: {counts:?}");
}

/// The full run (weights needed): `--ignored`.
#[test]
#[ignore]
fn corpus_v2_full() {
    let r = run_all(true);
    write_report(&r);
}

/// The court "knows the primitives and nothing about models" (spec 04b §10.4): no source file of
/// the IR, the evaluators or the court names an architecture.
#[test]
fn no_court_or_ir_source_names_a_model() {
    let root = crate_dir().join("..");
    let banned = [
        "llama", "qwen", "mistral", "gemma", "deepseek", "mamba", "rwkv", "mixtral", "falcon", "phi3", "bert", "t5", "whisper", "stable_diffusion",
        "gpt2", "gptneox", "olmo", "granite", "cohere", "glm", "kimi", "minimax", "jamba", "yolo",
    ];
    let dirs = [
        "misaka-palw-tir/src",
        "misaka-palw-tir-exec/src",
        "misaka-palw-tir-ref2/src",
        "consensus/core/src",
    ];
    let mut offenders: Vec<String> = Vec::new();
    let mut scanned = 0usize;
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(root.join(d)) else { continue };
        let mut stack: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        while let Some(p) = stack.pop() {
            if p.is_dir() {
                if let Ok(r) = std::fs::read_dir(&p) {
                    stack.extend(r.flatten().map(|e| e.path()));
                }
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // only the IR/court/exec files: in consensus/core only the palw_tir_* and palw_gen_* ones
            if d.starts_with("consensus") && !(name.starts_with("palw_tir_") || name.starts_with("palw_gen_court")) {
                continue;
            }
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            scanned += 1;
            // code and strings only: skip doc and line comments (a comment may cite a family as an example)
            // and test modules (a tripwire test lists the very words)
            let text = text.split("#[cfg(test)]").next().unwrap_or("").to_string();
            let code: String = text
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
                .to_ascii_lowercase();
            for b in banned {
                // word-ish match to avoid `bert` in `albert`-free code: require non-alphanumeric neighbours
                let mut from = 0;
                while let Some(i) = code[from..].find(b) {
                    let at = from + i;
                    let before = code[..at].chars().next_back().is_none_or(|c| !c.is_ascii_alphanumeric());
                    let after = code[at + b.len()..].chars().next().is_none_or(|c| !c.is_ascii_alphanumeric());
                    if before && after {
                        offenders.push(format!("{}: `{b}`", p.strip_prefix(&root).unwrap_or(&p).display()));
                        break;
                    }
                    from = at + b.len();
                }
            }
        }
    }
    eprintln!("scanned {scanned} IR/evaluator/court source files for model names");
    assert!(scanned > 20, "scanned only {scanned} files");
    assert!(offenders.is_empty(), "model-specific names in code that must be model-free: {offenders:?}");
}
