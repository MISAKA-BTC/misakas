//! **FR-09, DeepSeek sparse attention (`ATTN_TOKEN_INDEXER_V1`) against its Hugging Face fixture.** The tiny
//! DeepSeek-V3.2 (`tests/fixtures/fr09/deepseek_v32`, `index_topk` 4 over sequences of 10 to 24 tokens, so the
//! selection really selects): the float reference against `transformers`' logits, the integer program against the float
//! reference, and admission. The selection's tie rule (lowest index of the window) is the IR's; transformers' `topk`
//! leaves its tie order unspecified.

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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fr09/deepseek_v32")
}

fn config() -> String {
    std::fs::read_to_string(dir().join("config.json")).expect("config")
}

#[test]
fn deepseek_v32_float_matches_hf() {
    let spec = parse_config_str(&config()).expect("the deepseek-v32 adapter reads its fixture");
    let prog = hl::build_program(&spec).expect("hl");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "checkpoint tensors the program never reads: {unused:?}");
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir().join("logits.json")).unwrap()).unwrap();
    let tokens: Vec<usize> = meta["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let rows = |key: &str| -> Vec<Vec<f64>> {
        meta[key].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()).collect()
    };
    let got = Session::new(&prog, &params).run(&tokens).expect("run");
    let diff = |want: &[Vec<f64>]| -> Vec<f64> {
        got.iter().zip(want).map(|(g, w)| g.iter().zip(w).map(|(a, b)| (*a as f64 - b).abs()).fold(0.0, f64::max)).collect()
    };
    // 1. transformers' own model with the IR's tie rule (the one line `topk` replaced by a stable descending sort):
    //    the float reference equals it at every position — including those where the selection really selects.
    let want = rows("logits_full");
    let scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let d = diff(&want);
    eprintln!("deepseek_v32: {} positions vs HF (lowest-index ties): max|Δ| {:.2e} (scale {scale:.1})", got.len(), d.iter().cloned().fold(0.0, f64::max));
    assert!(d.iter().all(|x| *x <= 1e-4 * scale), "float vs HF with the IR's tie rule: {d:?}");
    // 2. `torch.topk`'s own tie order differs from the IR's exactly where a k-th place is tied: here the last position
    //    (seven tokens score exactly zero for four places). The earlier positions, selecting among distinct scores or
    //    all of them, agree with both.
    let d = diff(&rows("logits_full_torch_topk"));
    eprintln!("deepseek_v32: vs torch.topk's tie order: {d:?}");
    assert!(d[..9].iter().all(|x| *x <= 1e-4 * scale), "positions before the tie must agree with torch.topk too: {d:?}");
    assert!(d[9] > 1e-2 * scale, "the documented tie at the last position moved nothing ({:.2e})", d[9]);
}

/// One lowered and calibrated model: its float logits and its integer logits on the same sequences.
struct Run {
    float: Vec<Vec<Vec<f32>>>,
    int: Vec<Vec<Vec<f64>>>,
}

/// The integer program (config → HL → TIR → calibration → artifact) and the float reference on `eval`, calibrated on
/// random sequences of 32 tokens.
fn run_on(cfg: &str, opts: &LowerOpts, eval: &[Vec<usize>]) -> Result<Run, String> {
    let prep = fidelity::prepare(cfg, opts).map_err(|e| format!("prepare: {e}"))?;
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
        .map(|s| fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("integer run: {e}"))?;
    Ok(Run { float, int })
}

/// The fixture's configuration with `index_topk` set.
fn config_with_topk(k: usize) -> String {
    let mut v: serde_json::Value = serde_json::from_str(&config()).unwrap();
    v["index_topk"] = serde_json::json!(k);
    v.to_string()
}

fn show(name: &str, m: &fidelity::Metrics) {
    eprintln!(
        "{name}: top-1 {:.3}  KL {:.5} (max {:.4})  ppl {:.3} -> {:.3} ({:+.2}%)  [{} positions]",
        m.top1_agreement, m.kl_mean, m.kl_max, m.ppl_float, m.ppl_int, m.ppl_delta * 100.0, m.positions
    );
}

#[test]
fn deepseek_v32_integer_follows_float() {
    let eval = fidelity::random_sequences(64, 3, 24, 1234);
    let r = run_on(&config(), &LowerOpts::default(), &eval).expect("integer run");
    let m = fidelity::compare(&r.float, &r.int, &eval);
    show("deepseek_v32 (top 4 of up to 24)", &m);
    assert!(m.top1_agreement >= 0.9, "top-1 {}", m.top1_agreement);
    assert!(m.kl_mean <= 0.01, "KL {}", m.kl_mean);
}

/// **The integer program selects**: against a model of the same weights whose indexer keeps every token the logits
/// differ (the fixture's selection is not vacuous), and the integer program follows the float reference of ITS OWN
/// selection far more closely than the float reference of the dense model — at every `index_topk`.
#[test]
fn the_integer_program_applies_the_selection() {
    let eval = fidelity::random_sequences(64, 3, 24, 99);
    let dense = run_on(&config_with_topk(1000), &LowerOpts::default(), &eval).expect("dense");
    for k in [1usize, 2, 3, 4, 6, 10] {
        let r = run_on(&config_with_topk(k), &LowerOpts::default(), &eval).expect("sparse");
        let own = fidelity::compare(&r.float, &r.int, &eval);
        let against_dense = fidelity::compare(&dense.float, &r.int, &eval);
        show(&format!("index_topk {k:>2}: integer vs its float"), &own);
        show(&format!("index_topk {k:>2}: integer vs the DENSE float"), &against_dense);
        assert!(own.top1_agreement >= 0.9 && own.kl_mean <= 0.01, "k {k}: top-1 {} KL {}", own.top1_agreement, own.kl_mean);
        if k <= 4 {
            assert!(against_dense.kl_mean > 5.0 * own.kl_mean, "k {k}: the selection moved nothing (KL {} against dense, {} against its own)", against_dense.kl_mean, own.kl_mean);
        }
    }
}

/// The integer program against transformers' own logits (with the IR's tie rule) on the fixture's ten tokens: the last
/// position — where the tie moves torch's choice — included.
#[test]
fn deepseek_v32_integer_matches_hf_on_the_fixture_tokens() {
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir().join("logits.json")).unwrap()).unwrap();
    let tokens: Vec<usize> = meta["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let want: Vec<Vec<f64>> =
        meta["logits_full"].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()).collect();
    let eval = vec![tokens];
    let r = run_on(&config(), &LowerOpts::default(), &eval).expect("integer run");
    let hf = vec![want.iter().map(|r| r.iter().map(|x| *x as f32).collect::<Vec<f32>>()).collect::<Vec<_>>()];
    let m = fidelity::compare(&hf, &r.int, &eval);
    show("deepseek_v32 integer vs transformers (10 tokens)", &m);
    assert!(m.top1_agreement >= 0.9, "top-1 {}", m.top1_agreement);
    assert!(m.kl_mean <= 0.01, "KL {}", m.kl_mean);
}

/// Admission (`tir_admit_v1`, spec 04b §10.3) of the fixture and of the real DeepSeek-V3.2 shape: 61 layers, 128 heads,
/// an indexer of 64 heads × 128 over the top 2,048 tokens. The selection's threshold is one committed lane pair whose
/// cone is `B/4` reductions over `H` (at most 16, 04b §9.5.1); every terminal tile fits the legacy court's ceilings.
#[test]
fn deepseek_v32_is_admitted() {
    use misaka_palw_tir::admit::tir_admit_program_v1;
    use misaka_palw_tir_lower::admission::default_inputs;
    use misaka_palw_tir_lower::lower::lower;
    let report = |name: &str, cfg: &str, window: Option<u32>| {
        let spec = parse_config_str(cfg).unwrap_or_else(|e| panic!("{name}: {e}"));
        let hl = hl::build_program(&spec).unwrap_or_else(|e| panic!("{name}: {e}"));
        let lw = lower(&hl, &LowerOpts { max_window: window, ..LowerOpts::default() }).unwrap_or_else(|e| panic!("{name}: lower: {e}"));
        let a = tir_admit_program_v1(&lw.program, &default_inputs()).unwrap_or_else(|e| panic!("{name}: REFUSED {e}"));
        let worst = a.cones.iter().max_by_key(|c| c.terminal().macs).expect("a cone");
        let dissected = a.cones.iter().filter(|c| !c.h_reductions.is_empty()).count();
        let deepest = a.cones.iter().map(|c| c.h_reductions.len()).max().unwrap_or(0);
        let nodes = lw.program.blocks.iter().map(|b| b.nodes.len()).max().unwrap_or(0);
        eprintln!(
            "{name} (window {window:?}): ADMITTED — largest block {nodes} nodes, {} cones ({dissected} dissected over H, at most {deepest} reductions over H in one cone), worst terminal {} MACs ({}:{}), position {} MACs, cone work {}",
            a.cones.len(),
            worst.terminal().macs,
            worst.block,
            worst.node,
            a.position.cost.macs,
            a.cone_work
        );
        assert!(nodes <= 512 && deepest <= 16);
        (a.cone_work, a.position.cost.macs)
    };
    report("the tiny fixture", &config(), None);
    let real = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real/deepseek-v3.2-exp.json")).expect("config");
    for w in [1u32 << 13, 1 << 16] {
        let (work, macs) = report("DeepSeek-V3.2-Exp", &real, Some(w));
        assert!(macs <= 1 << 40, "{macs} MACs a position");
        assert!(work <= 1 << 16, "admission's own work {work} past testnet-12's 65,536");
    }
}

/// **Freeze criterion 4 on the DSA program**: the reference evaluator, the independent second implementation
/// (`ref2`, written from 04b alone, fed the canonical bytes) and the typed backend that ships on nodes give the same
/// logits and the same value at every commit point — the threshold's two lanes included — at every position, with
/// the selection selecting (`index_topk` 4 over 16 and 24 positions).
#[test]
fn the_dsa_program_is_the_same_on_all_three_implementations() {
    for k in [4usize, 1, 3] {
        let prep = fidelity::prepare(&config_with_topk(k), &LowerOpts::default()).expect("prepare");
        let ck = Checkpoint::open(&dir()).expect("checkpoint");
        let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
        let loader = Resident(Arc::new(params));
        let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24, 11);
        let quiet = |_: usize, _: usize| {};
        let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
        let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
        let eval = fidelity::random_sequences(prep.hl.vocab, 2, 24, 97);
        let n = three_ways(&prep.lowered.program, &mat.params, &eval).unwrap_or_else(|e| panic!("index_topk {k}: {e}"));
        eprintln!("index_topk {k}: {n} positions, logits and every commit equal on reference, ref2 and exec");
    }
}

/// **The streaming conversion of the DSA model is the whole conversion, byte for byte** (RFC-0002 Part II): the
/// indexer's projections are row-wise params like any other, and its narrowings (score, query, key) are small fills.
#[test]
fn the_streamed_conversion_of_the_dsa_model_is_the_whole_one() {
    use misaka_palw_tir_artifact::chunks::{ChunkStore, write_container_v1_chunked};
    use misaka_palw_tir_lower::float_ref::stream::Streamed;
    use misaka_palw_tir_lower::lower::{ChunkSink, StreamOpts, materialise_stream};
    use misaka_palw_tir_lower::artifact;
    let prep = fidelity::prepare(&config(), &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let loader = Streamed::new(&prep.hl, &prep.binding, &ck);
    let calib = fidelity::random_sequences(prep.hl.vocab, 3, 16, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let policy = QuantPolicy::default();
    let tmp = std::env::temp_dir().join(format!("tir-stream-dsa-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let meta = serde_json::json!({"fixture": "deepseek_v32"});
    let tok = [3u8; 64];
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &policy, &quiet).expect("materialise");
    let old_path = tmp.join("old.palwtir");
    artifact::write(&old_path, &prep.lowered.program, &mat.params, tok, meta.clone()).unwrap();
    let old = std::fs::read(&old_path).unwrap();
    for (label, opts) in [("rows1", StreamOpts { defer_min_elems: 0, block_elems: 1 }), ("never", StreamOpts { defer_min_elems: usize::MAX, block_elems: 8 })] {
        let store = ChunkStore::open(&tmp.join(format!("chunks-{label}"))).unwrap();
        let mut sink = ChunkSink::new(&store);
        materialise_stream(&prep.lowered, &prep.hl, &loader, &stats, &policy, &opts, &mut sink, &quiet).unwrap_or_else(|e| panic!("materialise_stream[{label}]: {e}"));
        let path = tmp.join(format!("new-{label}.palwtir"));
        write_container_v1_chunked(&path, &prep.lowered.program, Vec::new(), tok, meta.to_string(), &store, &sink.artifact).unwrap();
        assert!(std::fs::read(&path).unwrap() == old, "the streamed container ({label}) differs from the whole one");
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
