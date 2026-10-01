//! **Pre-quantised checkpoints (GPTQ, AWQ) end to end** on fixtures quantised in numpy per each
//! format's specification (`tools/gen_quant_fixtures.py`): the packed tensors are read, their
//! dequantisation is the float reference (checked against `transformers` with the same weights
//! substituted), and the projections lower from the stored integers — no re-quantisation.
//!
//! For each fixture: every checkpoint tensor is read; the float reference matches HF; the program
//! passes the range analysis; every per-group scale is exact at its row's unit; the integer
//! program agrees with the float reference (top-1, KL); and the same weights through the W8 path
//! (re-quantised per row) are measured beside it.

use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_lower::fidelity::{self, Metrics};
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{self, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-quant").join(name)
}

struct Outcome {
    hf_max_abs: f64,
    hf_scale: f64,
    quantised: usize,
    exact: Metrics,
    w8: Metrics,
    inexact: usize,
    bytes_q: usize,
    bytes_w8: usize,
}

fn run(name: &str) -> Result<Outcome, String> {
    let dir = fixture_dir(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?;
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let quantised = prep.lowered.program.params.iter().filter(|p| p.name.ends_with(".qa")).count();
    if quantised == 0 {
        return Err("no projection lowered from its integers".into());
    }
    analyze_ranges(&prep.lowered.program).map_err(|e| format!("range analysis refuses the program: {e}"))?;
    let ck = Checkpoint::open(&dir).map_err(|e| e.to_string())?;
    let (params, unused) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
    if !unused.is_empty() {
        return Err(format!("checkpoint tensors the program never reads: {unused:?}"));
    }
    // The float reference (the dequantised weights) against transformers with the same weights.
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("logits.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let tokens: Vec<usize> = meta["tokens"].as_array().ok_or("tokens")?.iter().map(|t| t.as_u64().unwrap_or(0) as usize).collect();
    let want: Vec<Vec<f64>> = meta["logits_full"]
        .as_array()
        .ok_or("logits")?
        .iter()
        .map(|r| r.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default())
        .collect();
    let got = Session::new(&prep.hl, &params).run(&tokens).map_err(|e| format!("float run: {e}"))?;
    let hf_scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let mut hf_max_abs = 0f64;
    for (g, w) in got.iter().zip(&want) {
        for (a, b) in g.iter().zip(w) {
            hf_max_abs = hf_max_abs.max((*a as f64 - b).abs());
        }
    }
    let loader = Resident(Arc::new(params));
    let vocab = prep.hl.vocab;
    let calib = fidelity::random_sequences(vocab, 6, 32, 7);
    let eval = fidelity::random_sequences(vocab, 3, 24, 1234);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let fl = fidelity::float_logits(&prep.hl, &loader, &eval, &quiet).map_err(|e| e.to_string())?;
    let measure = |lw: &lower::Lowered| -> Result<(Metrics, usize, usize), String> {
        let mat = materialise(lw, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
        let il: Vec<Vec<Vec<f64>>> = eval
            .iter()
            .map(|s| fidelity::int_logits(&lw.program, &mat.params, s, mat.logits_scale, &|_| {}))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("integer run: {e}"))?;
        Ok((fidelity::compare(&fl, &il, &eval), mat.quant_inexact, mat.params.bytes()))
    };
    let (exact, inexact, bytes_q) = measure(&prep.lowered)?;
    // The same weights through the W8 path: re-quantised per row, the float twin's lowering.
    let w8 = lower::lower(&prep.hl, &LowerOpts::default()).map_err(|e| format!("W8 lowering: {e}"))?;
    let (w8m, _, bytes_w8) = measure(&w8)?;
    Ok(Outcome { hf_max_abs, hf_scale, quantised, exact, w8: w8m, inexact, bytes_q, bytes_w8 })
}

fn check(name: &str) {
    if !fixture_dir(name).join("model.safetensors").exists() {
        eprintln!("fixture {name}: missing, skipped");
        return;
    }
    match run(name) {
        Ok(o) => {
            eprintln!(
                "{name:>22}: HF max|Δ| {:.1e} (scale {:.1}); {} projections from integers, inexact scales {}; exact top-1 {:.3} KL {:.5}; W8 top-1 {:.3} KL {:.5}; params {} B vs W8 {} B",
                o.hf_max_abs,
                o.hf_scale,
                o.quantised,
                o.inexact,
                o.exact.top1_agreement,
                o.exact.kl_mean,
                o.w8.top1_agreement,
                o.w8.kl_mean,
                o.bytes_q,
                o.bytes_w8
            );
            assert!(o.hf_max_abs <= 1e-4 * o.hf_scale, "{name}: float reference vs HF {:.3e}", o.hf_max_abs);
            assert_eq!(o.inexact, 0, "{name}: stored scales rounded at their row's unit");
            assert!(o.exact.top1_agreement >= 0.9, "{name}: top-1 {}", o.exact.top1_agreement);
            assert!(o.exact.kl_mean <= 0.01, "{name}: KL {}", o.exact.kl_mean);
        }
        Err(e) => panic!("fixture {name}: {e}"),
    }
}

macro_rules! quantised {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            check(stringify!($name));
        }
    )*};
}

quantised!(
    gptq_b4_g32,
    gptq_b4_g64_act,
    gptq_b4_g128_act_asym,
    gptq_b8_g128,
    gptq_b8_g32_asym_act,
    gptq_b4_perrow,
    gptq_v2_b4_g64,
    gptq_b2_g32,
    awq_g32,
    awq_g64,
    awq_g128,
    gptq_qwen3moe_b4_g32_act,
    awq_mixtral_g64,
);

/// A checkpoint whose config says GPTQ but whose projection is stored in float is a clear error,
/// not a silent float lowering.
#[test]
fn a_float_tensor_under_a_quantised_config_is_refused() {
    use misaka_palw_tir_lower::weights::{MapSource, Resolver, Tensor, eval_src};
    let dir = fixture_dir("gptq_b4_g32");
    if !dir.join("config.json").exists() {
        return;
    }
    let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap();
    let pi = prep.hl.params.iter().position(|p| p.name == "attn.q.w").expect("attn.q.w");
    let mut m = MapSource::default();
    m.0.insert("model.layers.0.self_attn.q_proj.weight".into(), Tensor::new(vec![128, 128], vec![0.0; 128 * 128]));
    let r = Resolver::new(&m, &[]);
    let e = eval_src(&prep.binding.srcs[pi], &r, Some(0), &Default::default()).unwrap_err();
    assert!(e.to_string().contains("stored in float"), "{e}");
}
