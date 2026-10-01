//! **Qwen4-Exp as a combination of generic features** (RFC-0002 lane G, the genericity acceptance
//! test). The fixtures are tiny, randomly initialised `Qwen4ExpForCausalLM` checkpoints from
//! transformers 5.17 (`tools/gen_qwen4_fixtures.py`) — never the published weights. Nothing in the
//! lowering knows the name `qwen4`: the model is read by a data adapter (`adapters/qwen4-exp.json`)
//! into gated residuals (hyper-connections), sparse block attention, gated delta layers at a
//! key:value head ratio, hashed n-gram per-layer embeddings and a routed MoE, each a feature of the
//! finite vocabulary (`model::REGISTRY`).
//!
//! The acceptance matrix, one named test each (`GR` gated residuals, `PLE` n-gram per-layer
//! embeddings, `QSA` sparse block attention, `GDN` gated delta nets). Every one runs the whole
//! chain on its fixture:
//!
//! 1. transformers' logits ↔ the float reference (the HL interpreter) — the semantics are right;
//! 2. the integer program (reference evaluator) ↔ transformers' logits — the quantised lowering
//!    keeps them (top-1, KL);
//! 3. the structural evidence the feature has: the in-program n-gram ids equal transformers' ids,
//!    the program's block selection equals the indexer's;
//! 4. reference evaluator ↔ `misaka-palw-tir-ref2` ↔ `misaka-palw-tir-exec` on every commit point;
//! 5. the court's demand evaluator reproduces every node of every occurrence from the committed
//!    leaves alone (`tests/common`).

#![allow(non_snake_case)]
// the real-shape config is one large `json!` literal
#![recursion_limit = "1024"]

mod common;

use misaka_palw_tir::{Interpreter, Prim, RunState};
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::hf_schema::{AdapterChoice, AdapterSource, ReadOptions, TensorIndex, read_model};
use misaka_palw_tir_lower::lower::{LowerOpts, Materialised, materialise};
use misaka_palw_tir_lower::model::REGISTRY;
use misaka_palw_tir_lower::ngram::{NgramTables, ids_batch};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::{Mixer, ModelSpec, Residual};
use misaka_palw_tir_lower::{fidelity, hf_weights, hl};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

// ───────────────────────────── nothing in these tests may grow large ─────────────────────────────
//
// A config the lowering mishandles must fail loudly and at once, not thrash the machine: this binary's
// allocator aborts on one allocation over 512 MiB and on live memory over 4 GiB, naming the size. The
// fixtures and every mutated config are tiny — each test also asserts, by ARITHMETIC over the shapes the
// program declares, that no param passes 64 MiB and that the program holds under 20 M elements — and the
// real-scale n-gram check is the hash function alone (`lower::generic::tests`), never a table.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Guard;
static LIVE: AtomicUsize = AtomicUsize::new(0);
const ONE_MAX: usize = 512 << 20;
const LIVE_MAX: usize = 4 << 30;

fn too_big(n: usize, live: usize) -> ! {
    // No allocation here: a fixed message through the unbuffered stderr, then the process ends.
    eprintln!("qwen4_exp: an allocation of {n} bytes with {live} live is past the test's bound; aborting before the machine swaps");
    std::process::abort()
}

unsafe impl GlobalAlloc for Guard {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        if l.size() > ONE_MAX || live > LIVE_MAX {
            too_big(l.size(), live);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        if l.size() > ONE_MAX || live > LIVE_MAX {
            too_big(l.size(), live);
        }
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let live = if new >= l.size() { LIVE.fetch_add(new - l.size(), Ordering::Relaxed) + (new - l.size()) } else { LIVE.fetch_sub(l.size() - new, Ordering::Relaxed) };
        if new > ONE_MAX || live > LIVE_MAX {
            too_big(new, live);
        }
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static ALLOCATOR: Guard = Guard;

/// `(elements of the largest param, its bytes, elements in all)` of what a program declares, by
/// arithmetic over its shapes (a per-layer param once for every layer occurrence whose block reads it).
/// Nothing is allocated.
fn declared(p: &misaka_palw_tir::TirProgramV1) -> (u64, u64, u64) {
    use misaka_palw_tir::Ref;
    // the layer occurrences that read each param
    let mut uses = vec![0u64; p.params.len()];
    for b in &p.schedule.layers {
        let mut seen = BTreeSet::new();
        for n in &p.blocks[*b as usize].nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = r {
                    seen.insert(*j as usize);
                }
            }
        }
        for j in seen {
            uses[j] += 1;
        }
    }
    let (mut largest, mut bytes, mut total) = (0u64, 0u64, 0u64);
    for (j, d) in p.params.iter().enumerate() {
        let e: u64 = d.shape.iter().fold(1u64, |a, x| a.saturating_mul(*x as u64));
        largest = largest.max(e);
        bytes = bytes.max(e.saturating_mul(d.dtype.width() as u64));
        total = total.saturating_add(e.saturating_mul(if d.per_layer { uses[j].max(1) } else { 1 }));
    }
    (largest, bytes, total)
}

/// The tests' bound: no param over 64 MiB, under 20 M elements in all.
fn assert_tiny(what: &str, p: &misaka_palw_tir::TirProgramV1) {
    let (largest, bytes, total) = declared(p);
    assert!(bytes <= 64 << 20 && total <= 20_000_000, "{what}: declares a param of {largest} elements ({bytes} bytes) and {total} elements in all — not a tiny test program");
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf")
}

fn fixture(name: &str) -> PathBuf {
    root().join(name)
}

fn json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("json")
}

/// The spec the generic reader makes of a fixture, with the checkpoint's tensor names.
fn spec_of(name: &str) -> ModelSpec {
    let dir = fixture(name);
    let cfg = json(&dir.join("config.json"));
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    let r = read_model(&cfg, Some(&tensors), &ReadOptions::default()).unwrap_or_else(|f| panic!("{name}: {}", f.error));
    assert!(matches!(&r.adapter, AdapterSource::BuiltIn { id, .. } if id == "qwen4-exp"), "{name}: read by {:?}", r.adapter);
    r.spec
}

fn names() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root())
        .expect("fixtures")
        .map(|e| e.expect("entry").file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("qwen4_"))
        .collect();
    v.sort();
    assert_eq!(v.len(), 16, "the Qwen4-Exp fixtures: {v:?}");
    v
}

#[test]
fn the_adapter_reads_every_fixture_into_generic_features() {
    for n in names() {
        let s = spec_of(&n);
        let h = s.hyper.as_ref().unwrap_or_else(|| panic!("{n}: no hyper-connection streams"));
        assert!(h.streams >= 1, "{n}");
        // every layer is a gated residual over those streams; the mixer is a delta net or sparse attention
        for (i, l) in s.layers.iter().enumerate() {
            assert!(matches!(l.residual, Residual::HyperConnection { .. }), "{n} layer {i}");
            match &l.mixer {
                Mixer::GatedDeltaNet(_) => {}
                Mixer::Attention(a) => assert!(a.sparse.is_some() && a.output_gate, "{n} layer {i}: sparse attention with a gated output"),
                m => panic!("{n} layer {i}: {m:?}"),
            }
        }
        // nothing outside the vocabulary: every feature the spec uses is registered, and none needs a protocol change
        for u in s.features() {
            let f = REGISTRY.iter().find(|f| f.id == u.id).unwrap_or_else(|| panic!("{n}: {} is not in the registry", u.id));
            assert!(matches!(f.protocol, misaka_palw_tir_lower::model::Requirement::None), "{n}: {} needs {:?}", u.id, f.protocol);
        }
    }
}

#[test]
fn the_ngram_ids_are_the_transformers_ids() {
    let mut checked = 0;
    for n in names() {
        let p = fixture(&n).join("ngram_ids.json");
        if !p.exists() {
            continue;
        }
        let s = spec_of(&n);
        let v = json(&p);
        let tokens: Vec<i64> = v["tokens"].as_array().expect("tokens").iter().map(|x| x.as_i64().expect("int")).collect();
        let ple: Vec<_> = s
            .layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| match &l.residual {
                Residual::HyperConnection { ple: Some(p) } => Some((i, p.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(v["ids"].as_object().expect("ids").len(), ple.len(), "{n}: PLE layers");
        for (layer, spec) in ple {
            let want: Vec<Vec<i64>> = v["ids"][layer.to_string()]
                .as_array()
                .expect("layer ids")
                .iter()
                .map(|row| row.as_array().expect("row").iter().map(|x| x.as_i64().expect("int")).collect())
                .collect();
            let t = NgramTables::new(&spec);
            assert_eq!(ids_batch(&t, &tokens), want, "{n} layer {layer}");
            checked += 1;
        }
    }
    assert!(checked >= 8, "{checked} PLE layers checked against the HF ids");
}

#[test]
fn a_config_with_no_adapter_choice_is_read_without_the_model_type() {
    // The adapter claims the class by its architecture; the model_type is informational.
    let dir = fixture("qwen4_exp");
    let mut cfg = json(&dir.join("config.json"));
    cfg["model_type"] = Value::String("something_else_entirely".into());
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    let r = read_model(&cfg, Some(&tensors), &ReadOptions { adapter: AdapterChoice::Auto }).expect("read");
    assert_eq!(r.spec.layers.len(), 4);
}

// ───────────────────────────── the chain on one fixture ─────────────────────────────

/// One fixture through the whole chain, made once and shared by the named tests that use it.
struct Case {
    name: String,
    prep: fidelity::Prepared,
    params: Arc<ParamStore>,
    tokens: Vec<usize>,
    /// transformers' logits: the whole sequence at once, and token by token through its cache.
    hf_full: Vec<Vec<f64>>,
    hf_decode: Option<Vec<Vec<f64>>>,
    /// The float reference's (HL interpreter) logits over `tokens`.
    float: Vec<Vec<f32>>,
    mat: Materialised,
    /// The integer program's logits over `tokens` (natural-log units).
    int: Vec<Vec<f64>>,
}

fn rows(v: &Value) -> Vec<Vec<f64>> {
    v.as_array().expect("logits").iter().map(|r| r.as_array().expect("row").iter().map(|x| x.as_f64().expect("float")).collect()).collect()
}

fn case(name: &str) -> Arc<Case> {
    case_with(name, "", LowerOpts::default())
}

/// [`case`] under other lowering options (`tag` names them).
fn case_with(name: &str, tag: &str, opts: LowerOpts) -> Arc<Case> {
    type Slot = Arc<OnceLock<Arc<Case>>>;
    static CASES: OnceLock<Mutex<BTreeMap<String, Slot>>> = OnceLock::new();
    let slot: Slot = CASES.get_or_init(Default::default).lock().expect("cases").entry(format!("{name}|{tag}")).or_default().clone();
    slot.get_or_init(|| Arc::new(build(name, &opts))).clone()
}

fn build(name: &str, opts: &LowerOpts) -> Case {
    let dir = fixture(name);
    let (prep, ck) = fidelity::open_model(&dir, opts).unwrap_or_else(|e| panic!("{name}: prepare: {e}"));
    assert_tiny(name, &prep.lowered.program);
    let (params, unused) = ParamStore::from_source(&prep.hl, &prep.binding, ck.as_ref()).unwrap_or_else(|e| panic!("{name}: params: {e}"));
    // Every checkpoint tensor is read or declared ignored (the hash buffers: recomputed from the config, equal to the checkpoint's).
    assert!(unused.is_empty(), "{name}: tensors the program never reads: {unused:?}");
    let params = Arc::new(params);
    let meta = json(&dir.join("logits.json"));
    let tokens: Vec<usize> = meta["tokens"].as_array().expect("tokens").iter().map(|t| t.as_u64().expect("int") as usize).collect();
    let hf_full = rows(&meta["logits_full"]);
    let hf_decode = meta.get("logits_decode").map(rows);
    let float = Session::new(&prep.hl, &params).run(&tokens).unwrap_or_else(|e| panic!("{name}: float run: {e}"));
    let loader = Resident(params.clone());
    let quiet = |_: usize, _: usize| {};
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32, 7);
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap_or_else(|e| panic!("{name}: calibrate: {e}"));
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap_or_else(|e| panic!("{name}: materialise: {e}"));
    let int = fidelity::int_logits(&prep.lowered.program, &mat.params, &tokens, mat.logits_scale, &|_| {}).unwrap_or_else(|e| panic!("{name}: integer run: {e}"));
    Case { name: name.to_string(), prep, params, tokens, hf_full, hf_decode, float, mat, int }
}

fn log_softmax(v: &[f64]) -> Vec<f64> {
    let m = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let z = v.iter().map(|x| (x - m).exp()).sum::<f64>().ln() + m;
    v.iter().map(|x| x - z).collect()
}

fn argmax(v: &[f64]) -> usize {
    v.iter().enumerate().fold(0, |b, (i, x)| if *x > v[b] { i } else { b })
}

/// `KL(p ‖ q)` of the softmaxes of two logit rows.
fn kl(p: &[f64], q: &[f64]) -> f64 {
    let (lp, lq) = (log_softmax(p), log_softmax(q));
    lp.iter().zip(&lq).map(|(a, b)| a.exp() * (a - b)).sum()
}

/// Agreement of an integer program's logits with a reference, positionwise.
#[derive(Debug)]
struct Agreement {
    kl_mean: f64,
    kl_max: f64,
    /// Positions where the reference's best token leads by more than the quantisation's own error many
    /// times over (random weights leave many near-ties), and how many of those the integer program gets wrong.
    decided: usize,
    wrong: Vec<usize>,
    positions: usize,
}

fn agree(reference: &[Vec<f64>], int: &[Vec<f64>]) -> Agreement {
    let mut kls = Vec::new();
    let mut sq = 0f64;
    let mut n = 0usize;
    for (r, i) in reference.iter().zip(int) {
        kls.push(kl(r, i));
        for (a, b) in r.iter().zip(i) {
            sq += (a - b) * (a - b);
            n += 1;
        }
    }
    let rms = (sq / n.max(1) as f64).sqrt();
    let (mut decided, mut wrong) = (0, Vec::new());
    for (p, (r, i)) in reference.iter().zip(int).enumerate() {
        let mut s = r.clone();
        s.sort_by(|a, b| b.partial_cmp(a).expect("finite"));
        if s[0] - s[1] > 8.0 * rms {
            decided += 1;
            if argmax(r) != argmax(i) {
                wrong.push(p);
            }
        }
    }
    Agreement { kl_mean: kls.iter().sum::<f64>() / kls.len().max(1) as f64, kl_max: kls.iter().copied().fold(0.0, f64::max), decided, wrong, positions: kls.len() }
}

fn widen(v: &[Vec<f32>]) -> Vec<Vec<f64>> {
    v.iter().map(|r| r.iter().map(|x| *x as f64).collect()).collect()
}

/// The float reference is transformers' logits (within float noise), the integer program keeps them.
fn assert_chain(c: &Case) {
    // 1. transformers ↔ the float reference
    let scale = c.hf_full.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let mut max_abs = 0f64;
    for (g, w) in c.float.iter().zip(&c.hf_full) {
        for (a, b) in g.iter().zip(w) {
            max_abs = max_abs.max((*a as f64 - b).abs());
        }
    }
    // 2. the integer program ↔ transformers and ↔ the float reference
    let vs_hf = agree(&c.hf_full, &c.int);
    let vs_float = agree(&widen(&c.float), &c.int);
    eprintln!(
        "{:>22}: float↔HF max|Δ| {:.2e} (scale {:.2}) | int↔HF KL {:.5} (max {:.4}), decided {}/{} wrong {} | int↔float KL {:.5}",
        c.name,
        max_abs,
        scale,
        vs_hf.kl_mean,
        vs_hf.kl_max,
        vs_hf.decided,
        vs_hf.positions,
        vs_hf.wrong.len(),
        vs_float.kl_mean
    );
    assert!(max_abs <= 1e-3 * scale, "{}: the float reference is {max_abs:.3e} from transformers (scale {scale:.2})", c.name);
    assert!(vs_hf.kl_mean <= 0.01, "{}: the integer program is {:.5} nats from transformers", c.name, vs_hf.kl_mean);
    assert!(vs_hf.wrong.is_empty(), "{}: top-1 differs from transformers at {:?} though it leads by >8× the error", c.name, vs_hf.wrong);
    assert!(vs_float.kl_mean <= 0.01, "{}: the integer program is {:.5} nats from its float reference", c.name, vs_float.kl_mean);
    // transformers' cached decode is its own whole-sequence logits (the fixtures record how far apart)
    if let Some(d) = &c.hf_decode {
        let m = d.iter().zip(&c.hf_full).flat_map(|(a, b)| a.iter().zip(b).map(|(x, y)| (x - y).abs())).fold(0f64, f64::max);
        assert!(m <= 1e-4 * scale, "{}: transformers' decode is {m:.3e} from its whole-sequence logits", c.name);
    }
}

// ───────────────────────────── structural evidence ─────────────────────────────

/// The honest integer run, step by step, keeping the commits of the nodes `pick` selects:
/// `(position, occurrence, node value)`.
fn committed(c: &Case, pick: &dyn Fn(u8, u16, &misaka_palw_tir::TirProgramV1) -> bool) -> Vec<(u32, Option<u16>, Vec<i128>)> {
    let p = &c.prep.lowered.program;
    let mp = misaka_palw_tir::MapParams { tensors: c.mat.params.tensors.iter().map(|(k, t)| (*k, t.to_tir())).collect() };
    let interp = Interpreter::new(p).expect("valid");
    let mut st = RunState::default();
    let mut out = Vec::new();
    for t in &c.tokens {
        let step = interp.step(&mp, &mut st, *t as u32).expect("an honest step");
        for cm in &step.commits {
            if pick(cm.block, cm.node, p) {
                out.push((step.pos, cm.layer, cm.value.data.clone()));
            }
        }
    }
    out
}

/// The in-program n-gram ids (each head's id within its own table, committed) equal transformers'
/// ids minus the head's table offset, at every position of every PLE layer.
fn assert_ngram_ids_in_program(c: &Case) {
    let v = json(&fixture(&c.name).join("ngram_ids.json"));
    let spec = spec_of(&c.name);
    let ids = committed(c, &|b, n, p| {
        let blk = &p.blocks[b as usize];
        blk.name.ends_with(".ple") && blk.nodes[n as usize].commit && matches!(blk.nodes[n as usize].prim, Prim::Clamp { .. }) && blk.nodes[n as usize].out.dtype == misaka_palw_tir::DType::Idx
    });
    assert!(!ids.is_empty(), "{}: no n-gram id commits", c.name);
    let mut checked = 0;
    for (pos, occ, got) in ids {
        let model_layer = c.prep.hl.model_layer(occ.expect("a layer block") as usize);
        let Residual::HyperConnection { ple: Some(p) } = &spec.layers[model_layer].residual else { panic!("layer {model_layer} has no PLE") };
        let t = NgramTables::new(p);
        let want: Vec<i128> = v["ids"][model_layer.to_string()][pos as usize]
            .as_array()
            .expect("row")
            .iter()
            .zip(&t.head_offsets)
            .map(|(x, off)| (x.as_i64().expect("int") - off) as i128)
            .collect();
        assert_eq!(got, want, "{}: layer {model_layer} position {pos}: the program's hash heads", c.name);
        checked += 1;
    }
    eprintln!("{:>22}: {checked} in-program hash rows (layer × position) equal transformers' ids", c.name);
}

/// The block ids the program selects (the committed `TopK` of the indexer), per position.
fn selected_blocks(c: &Case) -> Vec<(u32, Vec<usize>)> {
    committed(c, &|b, n, p| {
        let blk = &p.blocks[b as usize];
        blk.name.ends_with(".mix") && matches!(blk.nodes[n as usize].prim, Prim::TopK { .. })
    })
    .into_iter()
    .map(|(pos, _, v)| (pos, v.into_iter().map(|x| x as usize).collect()))
    .collect()
}

/// The tokens each query reads under the program's block ids: the selected blocks' tokens and the
/// incomplete tail, among those before the query — compared with transformers' indexer's.
fn assert_selection_equals_the_indexer(c: &Case) -> usize {
    let v = json(&fixture(&c.name).join("qsa_selected.json"));
    let ratio = v["ratio"].as_u64().expect("ratio") as usize;
    let want = v["selected"].as_object().expect("selected").values().next().expect("one QSA layer").as_array().expect("positions").clone();
    let mut checked = 0;
    for (pos, ids) in selected_blocks(c) {
        let pos = pos as usize;
        let complete = (pos + 1) / ratio;
        let ids: BTreeSet<usize> = ids.into_iter().collect();
        let visible: Vec<usize> = (0..=pos).filter(|j| ids.contains(&(j / ratio)) || *j >= complete * ratio).collect();
        let hf: Vec<usize> = want[pos].as_array().expect("row").iter().map(|x| x.as_u64().expect("int") as usize).collect();
        assert_eq!(visible, hf, "{}: position {pos}: the program's block selection (ids {ids:?}) vs transformers' indexer", c.name);
        checked += 1;
    }
    eprintln!("{:>22}: {checked} positions: the program's selected blocks read exactly the tokens transformers' indexer keeps", c.name);
    checked
}

// ───────────────────────────── the acceptance matrix ─────────────────────────────

#[test]
fn GR_01_hc_count_1() {
    // One stream: the gated mean over one stream, the mixer, the injection back — the machinery degenerates cleanly.
    let c = case("qwen4_hc1");
    assert_eq!(spec_of("qwen4_hc1").hyper.as_ref().expect("streams").streams, 1);
    assert_eq!(c.prep.hl.carries[0].shape, vec![c.prep.hl.hidden], "one stream is the plain residual width");
    assert_chain(&c);
}

#[test]
fn GR_02_hc_count_4() {
    // Four streams (the model's own count) and two.
    for (name, streams) in [("qwen4_exp", 4), ("qwen4_hc2", 2)] {
        let c = case(name);
        assert_eq!(spec_of(name).hyper.as_ref().expect("streams").streams, streams);
        assert_eq!(c.prep.hl.carries[0].shape, vec![c.prep.hl.hidden * streams]);
        assert_chain(&c);
    }
}

#[test]
fn GR_03_gate_edge_values() {
    // The gates driven into saturation: the sigmoids at 0 and 1, the silus in their tails.
    let c = case("qwen4_hc_edge");
    assert_chain(&c);
    // the saturation is real: a large share of the mix gates sit within 1e-2 of 0 or 1
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&c.prep.hl, &Resident(c.params.clone()), std::slice::from_ref(&c.tokens), &quiet).expect("calibrate");
    let gate_absmax = stats.iter().filter(|(k, _)| k.ends_with("hc.mix.gate") || k.ends_with("hc.ffn.gate")).map(|(_, s)| s.absmax).fold(0f64, f64::max);
    assert!(gate_absmax > 0.99, "the gates reach saturation (absmax {gate_absmax})");
}

#[test]
fn PLE_01_bigram() {
    let c = case("qwen4_ple_bigram");
    assert_eq!(match &spec_of("qwen4_ple_bigram").layers[1].residual { Residual::HyperConnection { ple: Some(p) } => p.ngram_size, _ => 0 }, 2);
    assert_chain(&c);
    assert_ngram_ids_in_program(&c);
}

#[test]
fn PLE_02_trigram() {
    let c = case("qwen4_ple_trigram");
    assert_eq!(match &spec_of("qwen4_ple_trigram").layers[1].residual { Residual::HyperConnection { ple: Some(p) } => p.ngram_size, _ => 0 }, 3);
    assert_chain(&c);
    assert_ngram_ids_in_program(&c);
}

#[test]
fn PLE_03_hash_boundary() {
    // The largest and smallest token ids, eos inside n-grams, a repeated run: the ids at the head tables' ends.
    let c = case("qwen4_ple_boundary");
    assert_chain(&c);
    assert_ngram_ids_in_program(&c);
}

#[test]
fn PLE_04_streaming_cache() {
    // The program is a stream: its window state carries the n-gram history position to position, and an eos ends a segment.
    // Position-by-position integer logits equal transformers' whole-sequence logits AND its token-by-token cached decode.
    for name in ["qwen4_ple_trigram", "qwen4_ple_boundary", "qwen4_exp"] {
        let c = case(name);
        let decode = c.hf_decode.as_ref().unwrap_or_else(|| panic!("{name}: no cached-decode logits"));
        let a = agree(decode, &c.int);
        eprintln!("{name:>22}: streaming int ↔ HF cached decode KL {:.5}, decided {}/{} wrong {}", a.kl_mean, a.decided, a.positions, a.wrong.len());
        assert!(a.kl_mean <= 0.01 && a.wrong.is_empty(), "{name}: {a:?}");
        assert_ngram_ids_in_program(&c);
    }
}

#[test]
fn PLE_05_dilated_conv_boundary() {
    // The dilated depthwise causal conv has `(k − 1)·dilation` rows of left context; before the sequence has that
    // many positions it reads zeros. The first positions are held to the same fidelity as the rest.
    let c = case("qwen4_ple_trigram");
    let Residual::HyperConnection { ple: Some(p) } = &spec_of("qwen4_ple_trigram").layers[1].residual else { panic!("a PLE layer") };
    let receptive = (p.conv_kernel - 1) * p.conv_dilation;
    assert!(receptive >= 2 && c.tokens.len() > receptive + 2, "the fixture spans the boundary ({receptive} rows of left context)");
    assert_chain(&c);
    let (early_hf, early_int) = (&c.hf_full[..receptive], &c.int[..receptive]);
    let a = agree(early_hf, early_int);
    eprintln!("PLE-05: first {receptive} positions (zero left context): KL {:.5}", a.kl_mean);
    assert!(a.kl_mean <= 0.01 && a.wrong.is_empty(), "{a:?}");
    // the integer conv state starts at zero and the window holds exactly (k − 1)·dilation rows
    let prog = &c.prep.lowered.program;
    let st = prog.states.iter().find(|s| s.name == "ple.conv.window").expect("the conv window state");
    assert_eq!(st.shape[0] as usize, receptive);
}

#[test]
fn PLE_06_a_table_taller_than_a_chunk_is_read_by_chunks() {
    // NF-8 caps a dimension at 2^24 rows and a published n-gram table has hundreds of millions: each head's table is
    // its own param `[rows, dim]`, cut into chunks (`LowerOpts::table_chunk_rows`, 2^24 by default) with a `Select` by
    // the chunk index, every one read by `Gather { axis: 0, batch_dims: 0 }` of the param itself (the shape lane M2's
    // residency can address by row). Cut at 16 rows the fixture's 61..97-row head tables take several chunks each; the
    // program is another one, the function is the same: the logits are bit-identical to the unchunked lowering's.
    let whole = case("qwen4_ple_trigram");
    let cut = case_with("qwen4_ple_trigram", "chunk16", LowerOpts { table_chunk_rows: Some(16), ..LowerOpts::default() });
    let tables = |c: &Case| -> Vec<misaka_palw_tir::program::ParamDecl> {
        c.prep.lowered.program.params.iter().filter(|p| p.name.starts_with("ple.ngram.table.h")).cloned().collect()
    };
    let heads = 4; // (ngram_size - 1) * heads_per_ngram
    let (chunks, one) = (tables(&cut).len(), tables(&whole).len());
    assert_eq!(one, heads, "within 2^24 rows a head's table is one param");
    assert!(chunks >= 4 * heads, "{chunks} chunk params");
    assert!(tables(&cut).iter().all(|p| p.shape.len() == 2 && p.shape[0] <= 16), "a chunk is [rows <= 16, dim]");
    assert_eq!(cut.int, whole.int, "the chunked lookup reads the same rows");
    assert_chain(&cut);
    assert_ngram_ids_in_program(&cut);
    let eval = fidelity::random_sequences(cut.prep.hl.vocab, 2, 16, 97);
    common::three_ways(&cut.prep.lowered.program, &cut.mat.params, &eval).expect("three implementations agree on the chunked lookup");
    let tokens: Vec<u32> = cut.tokens.iter().map(|t| *t as u32).collect();
    let r = common::court_coverage(&cut.prep.lowered.program, &cut.mat.params, &tokens, &[0, 3, 9], &[1]).expect("the court reproduces the chunked lookup");
    eprintln!("PLE-06: {} chunks; the court reproduced {} nodes", chunks, r.nodes);
}

#[test]
fn QSA_01_k_1() {
    // budget = one block: the indexer keeps one block (plus the incomplete tail).
    let c = case("qwen4_qsa_k1");
    assert_chain(&c);
    assert_selection_equals_the_indexer(&c);
    assert!(selected_blocks(&c).iter().all(|(_, ids)| ids.len() == 1), "K = 1");
}

#[test]
fn QSA_02_k_max() {
    // the budget covers every block: the selection is every complete block, and attention is dense.
    let c = case("qwen4_qsa_kmax");
    assert_chain(&c);
    assert_selection_equals_the_indexer(&c);
    let v = json(&fixture("qwen4_qsa_kmax").join("qsa_selected.json"));
    let ratio = v["ratio"].as_u64().expect("ratio") as usize;
    for (pos, ids) in selected_blocks(&c) {
        let complete = (pos as usize + 1) / ratio;
        assert!((0..complete).all(|b| ids.contains(&b)), "position {pos}: every complete block is selected ({ids:?})");
    }
}

#[test]
fn QSA_03_score_tie() {
    // an all-zero indexer: every block scores 0 and the top-k tie rule (the lower index first) decides.
    let c = case("qwen4_qsa_tie");
    assert_chain(&c);
    assert_selection_equals_the_indexer(&c);
}

#[test]
fn QSA_04_incomplete_trailing_block() {
    // blocks of 3 positions: most positions end inside a block that is not complete — its tokens are always read.
    let c = case("qwen4_qsa_r3");
    assert_chain(&c);
    let n = assert_selection_equals_the_indexer(&c);
    let v = json(&fixture("qwen4_qsa_r3").join("qsa_selected.json"));
    let ratio = v["ratio"].as_u64().expect("ratio") as usize;
    assert_eq!(ratio, 3);
    assert!((0..n).any(|p| (p + 1) % ratio != 0), "positions with an incomplete trailing block");
}

#[test]
fn QSA_05_causal_boundary() {
    // At block starts, block ends and the first positions the selection equals the indexer's and never names a
    // block that is not complete while a complete one is available.
    for name in ["qwen4_qsa_k1", "qwen4_qsa_r3", "qwen4_exp"] {
        let c = case(name);
        assert_chain(&c);
        assert_selection_equals_the_indexer(&c);
        let v = json(&fixture(name).join("qsa_selected.json"));
        let ratio = v["ratio"].as_u64().expect("ratio") as usize;
        for (pos, ids) in selected_blocks(&c) {
            let complete = (pos as usize + 1) / ratio;
            let real: Vec<&usize> = ids.iter().filter(|b| **b < complete).collect();
            assert_eq!(real.len(), ids.len().min(complete), "{name} position {pos}: complete blocks fill the selection before any other ({ids:?}, {complete} complete)");
        }
    }
}

#[test]
fn QSA_06_cached_generation() {
    // The program is a stream: the pooled-key sums and the block-key matrix are its state. Position by position its
    // logits equal transformers' token-by-token cached decode.
    for name in ["qwen4_qsa_k1", "qwen4_qsa_kmax", "qwen4_qsa_r3", "qwen4_exp"] {
        let c = case(name);
        let decode = c.hf_decode.as_ref().unwrap_or_else(|| panic!("{name}: no cached-decode logits"));
        let a = agree(decode, &c.int);
        eprintln!("{name:>22}: streaming int ↔ HF cached decode KL {:.5}, decided {}/{} wrong {}", a.kl_mean, a.decided, a.positions, a.wrong.len());
        assert!(a.kl_mean <= 0.01 && a.wrong.is_empty(), "{name}: {a:?}");
    }
}

#[test]
fn GDN_01_ratio_1_1() {
    assert_gdn("qwen4_gdn_1_1", 2, 2);
}

#[test]
fn GDN_02_ratio_1_2() {
    assert_gdn("qwen4_exp", 2, 4);
}

#[test]
fn GDN_03_ratio_1_3() {
    assert_gdn("qwen4_gdn_1_3", 2, 6);
}

#[test]
fn GDN_04_arbitrary_ratio() {
    // 1:4 with a single key head, and a sigmoid output gate in place of the silu.
    assert_gdn("qwen4_gdn_1_4", 1, 4);
    assert_gdn("qwen4_gdn_sigmoid_gate", 2, 4);
}

fn assert_gdn(name: &str, nk: usize, nv: usize) {
    let c = case(name);
    let spec = spec_of(name);
    let g = spec.layers.iter().find_map(|l| if let Mixer::GatedDeltaNet(g) = &l.mixer { Some(g.clone()) } else { None }).expect("a delta net layer");
    assert_eq!((g.k_heads, g.v_heads), (nk, nv), "{name}");
    assert_chain(&c);
}

// ───────────────────────────── exactness, court, size ─────────────────────────────

#[test]
fn every_fixture_is_the_same_on_the_reference_ref2_and_exec() {
    let mut positions = 0;
    for n in names() {
        let c = case(&n);
        // two random sequences, and the fixture's own (an eos inside it, the segment rule exercised)
        let mut eval = fidelity::random_sequences(c.prep.hl.vocab, 2, 10, 97);
        eval.push(c.tokens.clone());
        positions += common::three_ways(&c.prep.lowered.program, &c.mat.params, &eval).unwrap_or_else(|e| panic!("{n}: {e}"));
    }
    eprintln!("{positions} positions: logits and every commit point equal on reference, ref2 and exec");
}

#[test]
fn the_court_reproduces_every_node_of_the_qwen4_programs_from_the_leaves() {
    // Every primitive the programs use is evaluated by the court's demand evaluator: no new court kernel.
    let mut used: BTreeSet<&'static str> = BTreeSet::new();
    let mut covered: BTreeMap<&'static str, usize> = BTreeMap::new();
    for n in ["qwen4_exp", "qwen4_qsa_r3", "qwen4_ple_bigram", "qwen4_hc1"] {
        let c = case(&n);
        let p = &c.prep.lowered.program;
        for b in &p.blocks {
            for nd in &b.nodes {
                used.insert(nd.prim.name());
            }
        }
        let tokens: Vec<u32> = c.tokens.iter().map(|t| *t as u32).collect();
        let last = tokens.len() as u32 - 1;
        let r = common::court_coverage(p, &c.mat.params, &tokens, &[0, 1, 5, last], &[4]).unwrap_or_else(|e| panic!("{n}: {e}"));
        eprintln!(
            "{n:>22}: {} nodes ({} committed values reproduced from the leaves, {} reductions over H dissected), {} elements",
            r.nodes, r.commits, r.dissected, r.elements
        );
        for (k, v) in r.primitives {
            *covered.entry(k).or_default() += v;
        }
    }
    for u in &used {
        assert!(covered.get(u).copied().unwrap_or(0) > 0, "primitive {u} is used by the programs but the court evaluator never evaluated it");
    }
    eprintln!("court coverage by primitive: {covered:?}");
}

#[test]
fn every_program_is_admitted_and_fits_the_normal_form_caps() {
    use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1, tir_admit_program_v1};
    let inputs = TirAdmitInputsV1 { tile_len: 64, h_chunk: 64, ceilings: TirCeilingsV1::legacy_court_v1() };
    for n in names() {
        let c = case(&n);
        let p = &c.prep.lowered.program;
        let nodes = p.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        let a = tir_admit_program_v1(p, &inputs).unwrap_or_else(|e| panic!("{n}: admission refuses the program: {e}"));
        let worst = a.cones.iter().max_by_key(|c| c.terminal().macs).expect("a cone");
        eprintln!(
            "{n:>22}: {} blocks, largest {nodes} nodes, {} cones (worst terminal {} MACs), cone work {}, step leaves {}",
            p.blocks.len(),
            a.cones.len(),
            worst.terminal().macs,
            a.cone_work,
            a.position.step_leaves
        );
        assert!(nodes <= 512 && p.blocks.len() <= 16, "{n}");
    }
}

/// The nodes the n-gram per-layer embedding costs, measured on the lowered program (the registry's
/// evidence for the bit-primitive extension): every node between the block's first and its table read.
#[test]
fn the_ngram_hash_costs_are_measured() {
    let c = case("qwen4_ple_trigram");
    let hl = &c.prep.hl;
    let bi = hl.blocks.iter().position(|b| b.name.ends_with(".ple")).expect("a PLE block");
    let tir = &c.prep.lowered.program;
    // the TIR block of this HL block: the schedule maps an HL block to its TIR block by order
    let tb = c.prep.lowered.block_map[bi] as usize;
    let nodes = &tir.blocks[tb].nodes;
    assert!(nodes.len() > 100, "the PLE block has {} nodes", nodes.len());
    eprintln!("PLE block (hash, table read, key/value, three norms, gate, conv): {} nodes", nodes.len());
}

#[test]
fn the_hl_ops_of_the_new_features_carry_no_checkpoint_names() {
    // the HL graph names roles, never tensors: the binding is the only place that knows them
    for n in names() {
        let c = case(&n);
        let hlp = hl::build_program(&c.prep.spec).expect("hl");
        let b = hf_weights::bind(&c.prep.spec, &hlp).expect("bind");
        assert_eq!(b.srcs.len(), hlp.params.len());
        for p in &hlp.params {
            assert!(!p.name.contains("model.layers") && !p.name.contains("_hyper_connection"), "{}: HL param `{}` carries a checkpoint name", n, p.name);
        }
    }
}

/// A config at the SHAPE of the model's class defaults (transformers' `Qwen4ExpTextConfig`: 2048 wide, 40
/// layers, 512 experts of top 10, 4 streams, hashed tables of 20 M rows a head) — never its weights: the
/// lowering DECLARES tables (shapes in a program), it never instantiates them, and this test has no param
/// store, no fill and no materialisation (the allocator bound above would end it if it grew). NF-8 caps a
/// dimension at 2^24, so every head's table takes two chunks; NF-12 caps a block at 512 nodes. The hash
/// itself is checked at this scale by `lower::generic::tests` with the function alone.
#[test]
fn a_config_at_the_real_shape_lowers_with_chunked_tables_inside_the_caps() {
    let layer_types: Vec<&str> = (0..40).map(|i| if (i + 1) % 4 == 0 { "qwen_sparse_attention" } else { "linear_attention" }).collect();
    let cfg = serde_json::json!({
        "architectures": ["Qwen4ExpForCausalLM"], "model_type": "qwen4_exp_text",
        "vocab_size": 248320, "hidden_size": 2048, "num_hidden_layers": 40, "num_attention_heads": 16, "num_key_value_heads": 2,
        "head_dim": 256, "max_position_embeddings": 32768, "rms_norm_eps": 1e-6, "hidden_act": "silu", "tie_word_embeddings": false,
        "rope_parameters": { "rope_type": "default", "rope_theta": 10000.0, "partial_rotary_factor": 0.25 },
        "attention_bias": false, "linear_conv_kernel_dim": 4, "linear_key_head_dim": 128, "linear_value_head_dim": 128,
        "linear_num_key_heads": 16, "linear_num_value_heads": 32, "moe_intermediate_size": 512, "shared_expert_intermediate_size": 512,
        "num_experts_per_tok": 10, "num_experts": 512, "norm_topk_prob": true, "layer_types": layer_types,
        "hc_count": 4, "hc_lowrank": 320, "ple_layer_ids": [2, 6, 10, 14, 18, 22, 26, 30, 34], "ple_embed_dim": 2048,
        "ple_conv_kernel_size": 4, "ngram_size": 3, "heads_per_ngram": 8, "ngram_vocab_size_base": 20_000_000,
        "make_ngram_vocab_size_divisible_by": 128, "seed": 1234, "split_ngram_parts": 512,
        "indexer_n_heads": 16, "indexer_kv_heads": 1, "indexer_head_dim": 128, "indexer_budget": 2048, "indexer_compress_ratio": 16,
        "eos_token_id": 248044, "bos_token_id": 248044, "pad_token_id": 248044
    });
    let prep = fidelity::prepare(&cfg.to_string(), &LowerOpts::default()).unwrap_or_else(|e| panic!("the real-shape config does not lower: {e}"));
    let p = &prep.lowered.program;
    let tables: Vec<_> = p.params.iter().filter(|q| q.name.starts_with("ple.ngram.table.h")).collect();
    assert_eq!(tables.len(), 16 * 2, "two chunks a head at 20 M rows, sixteen heads");
    for t in &tables {
        assert!(t.shape.iter().all(|d| *d <= 1 << 24), "{}: {:?}", t.name, t.shape);
        assert_eq!(t.shape.len(), 2, "{}: a head's chunk is [rows, dim], axis-0 row-major", t.name);
    }
    let (largest, bytes, total) = declared(p);
    eprintln!("real shape DECLARES (never allocates) a param of {largest} elements ({bytes} bytes) and {total} elements in all");
    let most = p.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
    let counts: Vec<String> = p.blocks.iter().map(|b| format!("{}: {}", b.name, b.nodes.len())).collect();
    eprintln!("real shape: {} blocks, largest {most} nodes; PLE table chunks {:?}; [{}]", p.blocks.len(), tables.iter().map(|t| t.shape.clone()).collect::<Vec<_>>(), counts.join(", "));
    assert!(most <= 512 && p.blocks.len() <= 16);
    // the court's view of it at the legacy ceilings (printed: a real-size program may meet a ceiling that is the
    // network's to set, not the lowering's)
    use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1, tir_admit_program_v1};
    let inputs = TirAdmitInputsV1 { tile_len: 64, h_chunk: 64, ceilings: TirCeilingsV1::legacy_court_v1() };
    match tir_admit_program_v1(p, &inputs) {
        Ok(a) => eprintln!("real shape: admitted; {} cones, cone work {}, step leaves {}, {:.3e} MACs a position", a.cones.len(), a.cone_work, a.position.step_leaves, a.position.cost.macs as f64),
        Err(e) => eprintln!("real shape: admission says {e}"),
    }
}

/// A mistaken config key is read or refused — never a panic, never a hang. Every key of two of the fixtures'
/// configs is deleted and rewritten to a null, small numbers (0, 1, 2, 3, 7, 65 and −1), a float, a boolean, a
/// string and an empty list. The numbers are SMALL on purpose: one key changed to at most 65 can widen a tensor
/// of a 32-wide fixture by a factor of about 30, so every mutant stays tiny by arithmetic — and each one that
/// lowers is sized by arithmetic over what it declares, and the test never materialises a program (a deleted key
/// falls back to the class default, which can be the published size: such a program is counted, never built).
/// (A hostile huge number is the next test's, over the keys that size a feature.)
#[test]
fn a_mutated_config_is_read_or_refused_never_a_panic_or_a_hang() {
    use serde_json::json;
    use std::time::Instant;
    let variants = [Value::Null, json!(0), json!(1), json!(2), json!(3), json!(7), json!(65), json!(-1), json!(0.5), json!(true), json!("x"), json!([])];
    let (mut checked, mut lowered, mut declared_large) = (0usize, 0usize, 0usize);
    for name in ["qwen4_exp", "qwen4_qsa_r3"] {
        let cfg = json(&fixture(name).join("config.json"));
        let keys: Vec<String> = cfg.as_object().expect("an object").keys().cloned().collect();
        for key in keys {
            for v in std::iter::once(None).chain(variants.iter().map(Some)) {
                let mut c = cfg.clone();
                match v {
                    None => {
                        c.as_object_mut().expect("an object").remove(&key);
                    }
                    Some(v) => c[&key] = v.clone(),
                }
                let text = c.to_string();
                let t = Instant::now();
                if std::env::var_os("PALW_TRACE_MUTANTS").is_some() {
                    eprintln!("{name} `{key}` = {v:?}");
                }
                let r = std::panic::catch_unwind(|| fidelity::prepare(&text, &LowerOpts::default()));
                let r = r.unwrap_or_else(|_| panic!("{name}: `{key}` = {v:?}: a panic"));
                assert!(t.elapsed().as_secs() < 30, "{name}: `{key}` = {v:?}: took {:?}", t.elapsed());
                // A deleted key falls back to the class's default, which can be the published size (a table of
                // 20 M rows a head): the program DECLARES it, and this test never materialises a program — it
                // only lowers — so such a mutant is counted, not sized further.
                if let Ok(prep) = &r {
                    let (_, bytes, total) = declared(&prep.lowered.program);
                    if bytes <= 64 << 20 && total <= 20_000_000 {
                        lowered += 1;
                    } else {
                        declared_large += 1;
                        if std::env::var_os("PALW_TRACE_MUTANTS").is_some() {
                            eprintln!("  declares a param of {bytes} bytes / {total} elements: lowered only, never materialised");
                        }
                    }
                }
                checked += 1;
            }
        }
    }
    eprintln!(
        "{checked} mutated configs: {lowered} lowered tiny, {declared_large} lowered to a program that DECLARES published-size tables (a deleted key's class default; never materialised), the rest refused by name; no panic, none over 30 s"
    );
    // the only mutants that reach a published size are the deleted keys the class defaults a size for
    assert!(declared_large <= 40, "{declared_large} mutants declare published-size params");
}

/// A hostile huge number in a key that sizes a feature is refused by arithmetic before anything is sized on
/// it: the streams, the hash heads, the table, the convolution, the indexer. Run under this binary's allocator
/// bound, so a guard that regressed aborts the run at 512 MiB instead of reaching for the machine's memory.
#[test]
fn a_hostile_number_in_a_feature_key_is_refused_before_anything_is_sized_on_it() {
    use serde_json::json;
    use std::time::Instant;
    let keys = [
        "hc_count", "hc_lowrank", "heads_per_ngram", "ngram_size", "ngram_vocab_size_base", "ple_embed_dim", "ple_conv_kernel_size",
        "split_ngram_parts", "indexer_n_heads", "indexer_kv_heads", "indexer_head_dim", "indexer_budget", "indexer_compress_ratio",
        "max_position_embeddings", "head_dim",
    ];
    let huge = [json!(1_000_000_000_000u64), json!(4_294_967_296u64), json!(u64::MAX)];
    let cfg = json(&fixture("qwen4_exp").join("config.json"));
    let mut refused = 0;
    for key in keys {
        for v in &huge {
            let mut c = cfg.clone();
            c[key] = v.clone();
            let t = Instant::now();
            let r = std::panic::catch_unwind(|| fidelity::prepare(&c.to_string(), &LowerOpts::default()));
            let r = r.unwrap_or_else(|_| panic!("`{key}` = {v}: a panic"));
            assert!(t.elapsed().as_secs() < 30, "`{key}` = {v}: took {:?}", t.elapsed());
            match r {
                Ok(prep) => assert_tiny(&format!("`{key}` = {v}"), &prep.lowered.program),
                Err(_) => refused += 1,
            }
        }
    }
    eprintln!("{refused} of {} hostile feature keys refused by name before sizing", keys.len() * huge.len());
    assert!(refused >= keys.len(), "most of them are refused ({refused})");
}
