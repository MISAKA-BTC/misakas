//! **FR-12, Gemma-3n (`RESIDUAL_ALTUP_V1`, `RESIDUAL_LAUREL_V1`, `FFN_ACTIVATION_SPARSITY_V1`) against its Hugging Face fixture.**
//! The tiny Gemma-3n text model (`tests/fixtures/altup/gemma3n_text`: four AltUp streams, LAuReL of rank 4, per-layer inputs,
//! two KV-sharing layers, the Gaussian top-k at sparsity 0.95 in two layers): the float reference against `transformers`'
//! logits, the integer program against the float reference, admission, the three implementations and the court.

mod common;

use common::three_ways;
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{fidelity, hf_weights, hl, parse_config_str};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/altup/gemma3n_text")
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
    let prep = fidelity::prepare(cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
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
fn gemma3n_float_matches_hf() {
    let spec = parse_config_str(&config()).expect("the gemma3n-text adapter reads its fixture");
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
    eprintln!("gemma3n_text: {} positions vs HF: max|Δ| {:.2e} (scale {scale:.1})", got.len(), d.iter().cloned().fold(0.0, f64::max));
    assert!(d.iter().all(|x| *x <= 1e-4 * scale), "float vs HF: {d:?}");
}

#[test]
fn gemma3n_integer_follows_float() {
    let eval = fidelity::random_sequences(64, 3, 24, 1234);
    let r = run_on(&config(), &eval).expect("integer run");
    let m = fidelity::compare(&r.float, &r.int, &eval);
    show("gemma3n_text", &m);
    assert!(m.top1_agreement >= 0.9, "top-1 {}", m.top1_agreement);
    assert!(m.kl_mean <= 0.01, "KL {}", m.kl_mean);
}

/// Per-site errors of the fixture (a debugging aid): `PALW_PREFIX=L0. cargo test … -- --ignored --nocapture`.
#[test]
#[ignore]
fn site_errors_of_gemma3n() {
    let prep = fidelity::prepare(&config(), &LowerOpts::default()).unwrap();
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
    let prep = fidelity::prepare(cfg, &LowerOpts::default()).expect("prepare");
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
fn gemma3n_integer_matches_hf_on_the_fixture_tokens() {
    let meta = reference();
    let eval = vec![tokens_of(&meta)];
    let r = run_on(&config(), &eval).expect("integer run");
    let hf = vec![rows(&meta, "logits_full").iter().map(|r| r.iter().map(|x| *x as f32).collect::<Vec<f32>>()).collect::<Vec<_>>()];
    let m = fidelity::compare(&hf, &r.int, &eval);
    show("gemma3n integer vs transformers (10 tokens)", &m);
    assert!(m.top1_agreement >= 0.9, "top-1 {}", m.top1_agreement);
    assert!(m.kl_mean <= 0.01, "KL {}", m.kl_mean);
}

/// **Freeze criterion 4 on the AltUp program**: the reference evaluator, the independent second implementation (`ref2`) and the
/// typed backend that ships on nodes give the same logits and the same value at every commit point, at every position — the
/// predictions, the magnitude matches (two means, a ratio, an integer square root) and the Gaussian cutoff included.
#[test]
fn the_altup_program_is_the_same_on_all_three_implementations() {
    let (prep, mat) = materialised(&config(), 11);
    let eval = fidelity::random_sequences(prep.hl.vocab, 2, 24, 97);
    let n = three_ways(&prep.lowered.program, &mat.params, &eval).unwrap_or_else(|e| panic!("{e}"));
    eprintln!("gemma3n: {n} positions, logits and every commit equal on reference, ref2 and exec");
}

/// **The court on the AltUp program**: every node of every occurrence is reproduced by the court's demand evaluator from the
/// committed leaves alone, at positions before and after the sliding window fills, from a checkpoint every position and every third.
#[test]
fn the_court_replays_every_node_of_the_altup_program() {
    let (prep, mat) = materialised(&config(), 11);
    let tokens: Vec<u32> = fidelity::random_sequences(prep.hl.vocab, 1, 10, 5).remove(0).into_iter().map(|t| t as u32).collect();
    let last = tokens.len() as u32 - 1;
    let r = common::court_coverage(&prep.lowered.program, &mat.params, &tokens, &[0, 2, last], &[1, 3]).unwrap_or_else(|e| panic!("the court: {e}"));
    eprintln!("AltUp court: {} commit points, {} nodes, {} elements, primitives {:?}", r.commits, r.nodes, r.elements, r.primitives.keys().collect::<Vec<_>>());
    assert!(r.commits > 0);
    for p in ["Compare", "Select", "IntRsqrt", "Log2Floor", "ReduceSum", "Concat", "Slice"] {
        assert!(r.primitives.contains_key(p), "the court never evaluated a `{p}` of the AltUp program");
    }
}

/// **`FFN_ACTIVATION_SPARSITY_V1` keeps what lies above `mean + z·std`**: in the float reference the sparse layers' gate rows
/// are exactly `relu(g − (mean + z·std))` of the row before them, about 5 % of a row survives at sparsity 0.95, and the integer
/// program reproduces the survivors (their error against the float value is a small part of the row).
#[test]
fn a_gaussian_top_k_keeps_what_lies_above_mean_plus_z_std() {
    let prep = fidelity::prepare(&config(), &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let params = Arc::new(params);
    let z = misaka_palw_tir_lower::detmath::norm_inv_cdf(0.95).expect("z");
    assert!((z - 1.6448536269514722).abs() < 1e-9, "Φ⁻¹(0.95) = {z}");
    let mut sess = Session::new(&prep.hl, &params).with_trace();
    let seq = fidelity::random_sequences(prep.hl.vocab, 1, 12, 1234).remove(0);
    let (mut rows_seen, mut kept, mut total) = (0usize, 0usize, 0usize);
    for t in &seq {
        sess.step(*t).expect("step");
        let tr = sess.trace.clone().unwrap_or_default();
        for (k, out) in tr.iter().filter(|(k, _)| k.ends_with(".mlp.sparse")) {
            let g = &tr[&k.replace(".mlp.sparse", ".mlp.gate")];
            // `L<occurrence>.mlp.sparse`: the model layer of the occurrence decides — a dense layer of a model that sparsifies others passes
            // its row through (one block for every layer: the sparsity is data).
            let occ: usize = k.trim_start_matches('L').split('.').next().and_then(|n| n.parse().ok()).expect("occurrence");
            let sparse_layer = prep.hl.model_layer(occ) < 2;
            if !sparse_layer {
                assert!(out.iter().zip(g).all(|(o, x)| o == x), "{k}: a dense layer passes its row through");
                continue;
            }
            let n = g.len() as f64;
            let mean = g.iter().map(|x| *x as f64).sum::<f64>() / n;
            let std = (g.iter().map(|x| (*x as f64 - mean).powi(2)).sum::<f64>() / n).sqrt();
            for (o, x) in out.iter().zip(g) {
                let want = (*x as f64 - (mean + std * (z as f32) as f64)).max(0.0);
                assert!((*o as f64 - want).abs() <= 1e-5 * (1.0 + want.abs()), "{k}: {o} vs {want}");
            }
            rows_seen += 1;
            kept += out.iter().filter(|x| **x > 0.0).count();
            total += out.len();
        }
    }
    // The tiny fixture sparsifies two of its six layers: two rows a position.
    assert_eq!(rows_seen, 2 * seq.len(), "the sparse layers of the fixture are layers 0 and 1");
    let frac = kept as f64 / total as f64;
    eprintln!("Gaussian top-k at 0.95: {kept} of {total} lanes survive ({frac:.3})");
    assert!(frac > 0.01 && frac < 0.15, "survivor fraction {frac}");
    // The integer program: per-site error of the sparse rows.
    let loader = Resident(params.clone());
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let policy = QuantPolicy::default();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &policy, &quiet).expect("materialise");
    let errs = fidelity::site_errors(&prep, &params, &stats, &policy, &mat, &seq).expect("site errors");
    let sparse: Vec<_> = errs.iter().filter(|e| e.key.ends_with(".mlp.sparse") && prep.hl.model_layer(e.key.trim_start_matches('L').split('.').next().and_then(|n| n.parse().ok()).unwrap_or(99)) < 2).collect();
    assert_eq!(sparse.len(), 2, "{sparse:?}");
    for e in &sparse {
        eprintln!("{}: rel {:.4} (|f|max {:.3})", e.key, e.rel_l2, e.float_absmax);
        assert!(e.rel_l2 < 0.05, "{}: the integer cutoff moved the survivors by {}", e.key, e.rel_l2);
    }
}

/// **The real Gemma-3n-E4B text decoder is admitted** (`tir_admit_v1`): 35 layers of 2,048 lanes with four streams, the carry of
/// `(K + 1)·D = 10,240` lanes, per-layer inputs, the 15 KV-sharing layers and the ten sparse layers; no block past NF-12's 512 nodes.
#[test]
fn the_real_gemma3n_e4b_text_decoder_is_admitted() {
    use misaka_palw_tir::admit::tir_admit_program_v1;
    use misaka_palw_tir_lower::admission::default_inputs;
    use misaka_palw_tir_lower::lower::lower;
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real/gemma-3n-e4b-text.json")).expect("config");
    let spec = parse_config_str(&cfg).expect("spec");
    let hl = hl::build_program(&spec).expect("hl");
    for w in [1u32 << 13] {
        let lw = lower(&hl, &LowerOpts { max_window: Some(w), ..LowerOpts::default() }).expect("lower");
        let a = tir_admit_program_v1(&lw.program, &default_inputs()).unwrap_or_else(|e| panic!("REFUSED {e}"));
        let nodes = lw.program.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        eprintln!("Gemma-3n-E4B (window {w}): ADMITTED — largest block {nodes} nodes, {} cones, position {} MACs, cone work {}", a.cones.len(), a.position.cost.macs, a.cone_work);
        assert!(nodes <= 512, "largest block {nodes} nodes");
        assert!(a.cone_work <= 1 << 16, "admission's own work {} past testnet-12's 65,536", a.cone_work);
    }
}
