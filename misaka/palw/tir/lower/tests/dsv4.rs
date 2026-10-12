//! **FR-10, the DeepSeek-V4 set against its Hugging Face fixture** (`RESIDUAL_MHC_SINKHORN_V1`, `ATTN_Q_LOWRANK_V1`,
//! `ATTN_KV_SHARED_ROTATED_V1`, `ATTN_OUT_GROUPED_LOWRANK_V1`, `ATTN_COMPRESSED_KV_V1`, `ATTN_ENTRY_INDEXER_V1`,
//! `MLP_MOE_ROUTER_HASH_V1`, `MLP_MOE_ROUTER_SQRTSOFTPLUS_V1`, `MLP_GLU_LIMITED_V1`). The tiny DeepSeek-V4
//! (`tests/fixtures/dsv4/deepseek_v4`, made by `tools/corpus/gen_fixtures.py`: two streams, a CSA layer with windows of two and its
//! indexer keeping 4 of up to 5 entries over ten positions, an HCA layer with windows of four, a hash-routed and a top-k MoE): the
//! float reference against `transformers`' logits, the integer program against the float reference, admission, the three
//! implementations and the court.

mod common;

use common::three_ways;
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ReadOptions};
use misaka_palw_tir_lower::quantfmt::QuantRegistry;
use misaka_palw_tir_lower::{fidelity, hf_weights, hl};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The adapter that reads the fixture: the built-in `deepseek-v4`, or — to iterate on the adapter without a rebuild — the file `$PALW_ADAPTER`.
fn read_opts() -> ReadOptions {
    match std::env::var("PALW_ADAPTER") {
        Ok(p) => ReadOptions { adapter: AdapterChoice::Text(std::fs::read_to_string(p).expect("PALW_ADAPTER")) },
        Err(_) => ReadOptions::default(),
    }
}

fn parse_config_str(cfg: &str) -> Result<misaka_palw_tir_lower::spec::ModelSpec, misaka_palw_tir_lower::LowerError> {
    misaka_palw_tir_lower::hf_config::parse_config_str_read(cfg, &read_opts(), QuantRegistry::builtin())
}

fn prepare(cfg: &str, opts: &LowerOpts) -> Result<fidelity::Prepared, misaka_palw_tir_lower::LowerError> {
    fidelity::prepare_read(cfg, &read_opts(), opts, QuantRegistry::builtin())
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dsv4/deepseek_v4")
}

fn config() -> String {
    std::fs::read_to_string(dir().join("config.json")).expect("config")
}

fn reference() -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir().join("logits.json")).unwrap()).unwrap()
}

fn tokens_of(meta: &serde_json::Value) -> Vec<usize> {
    meta["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect()
}

fn rows(meta: &serde_json::Value, key: &str) -> Vec<Vec<f64>> {
    meta[key].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()).collect()
}

fn show(name: &str, m: &fidelity::Metrics) {
    eprintln!(
        "{name}: top-1 {:.3}  KL {:.5} (max {:.4})  ppl {:.3} -> {:.3} ({:+.2}%)  [{} positions]",
        m.top1_agreement, m.kl_mean, m.kl_max, m.ppl_float, m.ppl_int, m.ppl_delta * 100.0, m.positions
    );
}

/// One lowered and calibrated model: its float logits and its integer logits on the same sequences.
struct Run {
    float: Vec<Vec<Vec<f32>>>,
    int: Vec<Vec<Vec<f64>>>,
}

fn run_on(cfg: &str, eval: &[Vec<usize>]) -> Result<Run, String> {
    let prep = prepare(cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    analyze_ranges(&prep.lowered.program).map_err(|e| format!("range analysis refuses the program: {e}"))?;
    let ck = Checkpoint::open(&dir()).map_err(|e| e.to_string())?;
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
    let loader = Resident(Arc::new(params));
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
    let float = fidelity::float_logits(&prep.hl, &loader, eval, &quiet).map_err(|e| e.to_string())?;
    let int: Vec<Vec<Vec<f64>>> = eval
        .iter()
        .map(|s| fidelity::int_logits_exec(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("integer run: {e}"))?;
    Ok(Run { float, int })
}

/// The float reference equals `transformers` (float32, bf16-exact weights) at every position.
#[test]
fn dsv4_float_matches_hf() {
    let spec = parse_config_str(&config()).expect("the dsv4-text adapter reads its fixture");
    let prog = hl::build_program(&spec).expect("hl");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "checkpoint tensors the program never reads: {unused:?}");
    let meta = reference();
    let want = rows(&meta, "logits_full");
    let got = Session::new(&prog, &params).run(&tokens_of(&meta)).expect("run");
    let scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let d: Vec<f64> = got.iter().zip(&want).map(|(g, w)| g.iter().zip(w).map(|(a, b)| (*a as f64 - b).abs()).fold(0.0, f64::max)).collect();
    eprintln!("dsv4_text: {} positions vs HF: max|Δ| {:.2e} (scale {scale:.1})", got.len(), d.iter().cloned().fold(0.0, f64::max));
    assert!(d.iter().all(|x| *x <= 1e-4 * scale), "float vs HF: {d:?}");
}

#[test]
fn dsv4_integer_follows_float() {
    let eval = fidelity::random_sequences(64, 3, 24, 1234);
    let r = run_on(&config(), &eval).expect("integer run");
    let m = fidelity::compare(&r.float, &r.int, &eval);
    show("dsv4_text", &m);
    assert!(m.top1_agreement >= 0.9, "top-1 {}", m.top1_agreement);
    assert!(m.kl_mean <= 0.01, "KL {}", m.kl_mean);
}

/// Per-site errors of the fixture (a debugging aid): `PALW_PREFIX=L0. cargo test … -- --ignored --nocapture`.
#[test]
#[ignore]
fn site_errors_of_dsv4() {
    let prep = prepare(&config(), &LowerOpts::default()).unwrap();
    let ck = Checkpoint::open(&dir()).unwrap();
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
    let params = Arc::new(params);
    let loader = Resident(params.clone());
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
    let policy = QuantPolicy::default();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &policy, &quiet).unwrap();
    let seq = fidelity::random_sequences(prep.hl.vocab, 1, 12, 1234).remove(0);
    let errs = fidelity::site_errors(&prep, &params, &stats, &policy, &mat, &seq).unwrap();
    for e in errs.iter().filter(|e| std::env::var("PALW_PREFIX").map(|p| e.key.starts_with(&p)).unwrap_or(true)).take(60) {
        eprintln!("{:>40}  rel {:.5}  max|Δ| {:.4e}  |f|max {:.4e}", e.key, e.rel_l2, e.max_abs, e.float_absmax);
    }
}

fn materialised(cfg: &str, calib_seed: u64) -> (fidelity::Prepared, misaka_palw_tir_lower::lower::Materialised) {
    let prep = prepare(cfg, &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let loader = Resident(Arc::new(params));
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24, calib_seed);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    (prep, mat)
}

/// **The integer program against transformers' own logits** on the fixture's ten tokens.
#[test]
fn dsv4_integer_matches_hf_on_the_fixture_tokens() {
    let meta = reference();
    let eval = vec![tokens_of(&meta)];
    let r = run_on(&config(), &eval).expect("integer run");
    let hf = vec![rows(&meta, "logits_full").iter().map(|r| r.iter().map(|x| *x as f32).collect::<Vec<f32>>()).collect::<Vec<_>>()];
    let m = fidelity::compare(&hf, &r.int, &eval);
    show("dsv4 integer vs transformers (10 tokens)", &m);
    assert!(m.top1_agreement >= 0.9, "top-1 {}", m.top1_agreement);
    assert!(m.kl_mean <= 0.01, "KL {}", m.kl_mean);
}

/// **Freeze criterion 4 on the DeepSeek-V4 program**: the reference evaluator, the independent second implementation (`ref2`) and the
/// typed backend that ships on nodes give the same logits and the same value at every commit point, at every position — the
/// mixing weights, the Sinkhorn divisions, the window pools, the entry selection and the joint softmax included.
#[test]
fn the_dsv4_program_is_the_same_on_all_three_implementations() {
    let (prep, mat) = materialised(&config(), 11);
    let eval = fidelity::random_sequences(prep.hl.vocab, 2, 24, 97);
    let n = three_ways(&prep.lowered.program, &mat.params, &eval).unwrap_or_else(|e| panic!("{e}"));
    eprintln!("dsv4: {n} positions, logits and every commit equal on reference, ref2 and exec");
}

/// **The court on the DeepSeek-V4 program**: every node of every occurrence is reproduced by the court's demand evaluator from the
/// committed leaves alone, at positions before and after the sliding window fills, from a checkpoint every position and every third.
#[test]
fn the_court_replays_every_node_of_the_dsv4_program() {
    let (prep, mat) = materialised(&config(), 11);
    let tokens: Vec<u32> = fidelity::random_sequences(prep.hl.vocab, 1, 10, 5).remove(0).into_iter().map(|t| t as u32).collect();
    let last = tokens.len() as u32 - 1;
    let r = common::court_coverage(&prep.lowered.program, &mat.params, &tokens, &[0, 2, last], &[1, 3]).unwrap_or_else(|e| panic!("the court: {e}"));
    eprintln!("DeepSeek-V4 court: {} commit points, {} nodes, {} elements, primitives {:?}", r.commits, r.nodes, r.elements, r.primitives.keys().collect::<Vec<_>>());
    assert!(r.commits > 0);
    for p in ["Compare", "Select", "IntExp", "IntLn", "IntRsqrt", "ReduceSum", "ReduceMax", "TopK", "Concat", "Slice", "Transpose"] {
        assert!(r.primitives.contains_key(p), "the court never evaluated a `{p}` of the DeepSeek-V4 program");
    }
}

/// **`RESIDUAL_MHC_SINKHORN_V1`'s `comb` is doubly stochastic**: after the Sinkhorn iterations of the float reference every column
/// sums to one and every row very nearly so (twenty iterations: to 1e-5), whatever the logits — the manifold the mapping
/// constrains the residual to.
#[test]
fn the_sinkhorn_projection_is_doubly_stochastic() {
    use misaka_palw_tir_lower::float_ref::sinkhorn;
    let mut x = 12345u64;
    let mut rnd = || {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((x >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 2.0
    };
    for hc in [2usize, 4, 8] {
        for _ in 0..8 {
            let logits: Vec<f64> = (0..hc * hc).map(|_| rnd()).collect();
            let c = sinkhorn(&logits, hc, 20, 1e-6);
            for k in 0..hc {
                let col: f64 = (0..hc).map(|j| c[j * hc + k]).sum();
                let row: f64 = c[k * hc..(k + 1) * hc].iter().sum();
                assert!((col - 1.0).abs() < 1e-5, "hc {hc}: column {k} sums to {col}");
                assert!((row - 1.0).abs() < 1e-3, "hc {hc}: row {k} sums to {row}");
            }
        }
    }
}

/// **`MLP_MOE_ROUTER_HASH_V1`**: in a `hash_moe` layer the experts of a token are its row of the frozen table whatever the gate says
/// (the gate only weights them); the float reference's selection IS the table's row, position by position.
#[test]
fn a_hash_layer_routes_by_the_frozen_table() {
    use misaka_palw_tir_lower::weights::TensorSource;
    let prep = prepare(&config(), &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let table = ck.load("model.layers.0.ffn.gate.tid2eid").expect("the hash layer's table");
    let width = table.shape[1];
    let meta = reference();
    let tokens = tokens_of(&meta);
    let mut sess = Session::new(&prep.hl, &params).with_trace();
    let mut checked = 0;
    for t in &tokens {
        sess.step(*t).expect("step");
        let tr = sess.trace.clone().unwrap_or_default();
        // The occurrence of model layer 0's FFN site.
        let (_, ids) = tr
            .iter()
            .find(|(k, _)| k.ends_with(".moe.route") && prep.hl.model_layer(k.trim_start_matches('L').split('.').next().and_then(|n| n.parse().ok()).unwrap_or(99)) == 0)
            .expect("layer 0 routes");
        let want: Vec<f32> = table.data[t * width..(t + 1) * width].to_vec();
        let mut got = ids.clone();
        let mut w = want.clone();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        w.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(got, w, "token {t}: the hash layer's experts are its table row");
        checked += 1;
    }
    assert_eq!(checked, tokens.len());
}

/// **The real DeepSeek-V4-Flash shape is admitted** (`tir_admit_v1`): 43 layers of 4 streams, 64 heads of 512, the 128-window and the
/// compressed entries, the lightning indexer over 512 of them, 256 experts; no block past NF-12's 512 nodes. (The config is written from
/// the transformers config class: fields the hub file may carry differently are listed in `hf-coverage.md`.)
#[test]
fn the_real_deepseek_v4_shape_is_admitted() {
    use misaka_palw_tir::admit::tir_admit_program_v1;
    use misaka_palw_tir_lower::admission::default_inputs;
    use misaka_palw_tir_lower::lower::lower;
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real/deepseek-v4-flash.json")).expect("config");
    // A class declared at 8,192 positions: the entry stores hold `positions / ratio` rows (at the published million positions a store is
    // 2^27 lanes written by a one-hot select at every position — a class of that length is not what admission is asked here).
    let mut v: serde_json::Value = serde_json::from_str(&cfg).unwrap();
    v["max_position_embeddings"] = serde_json::json!(8192);
    let spec = parse_config_str(&v.to_string()).expect("spec");
    let hl = hl::build_program(&spec).expect("hl");
    for w in [1u32 << 13] {
        let lw = lower(&hl, &LowerOpts { max_window: Some(w), ..LowerOpts::default() }).expect("lower");
        let a = tir_admit_program_v1(&lw.program, &default_inputs()).unwrap_or_else(|e| panic!("REFUSED {e}"));
        let nodes = lw.program.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        eprintln!("DeepSeek-V4-Flash (window {w}): ADMITTED — largest block {nodes} nodes, {} cones, position {} MACs, cone work {}", a.cones.len(), a.position.cost.macs, a.cone_work);
        assert!(nodes <= 512, "largest block {nodes} nodes");
        assert!(a.cone_work <= 1 << 16, "admission's own work {} past testnet-12's 65,536", a.cone_work);
    }
}
