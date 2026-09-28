//! **Gate 2a end to end on the tiny HF fixtures**: config → HL → TIR program → calibration →
//! integer artifact → the `misaka-palw-tir` reference evaluator, against the float reference
//! (which itself matches `transformers` to ~1e-6 on these fixtures, `tests/hf_fixtures.rs`).
//!
//! The fixtures have random weights, so the numbers say that the lowering is RIGHT (a wrong
//! scale, a swapped half or a misrouted head drops agreement to chance), not how well a trained
//! model quantises — that is the real-checkpoint run's job.

use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{artifact, fidelity};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name)
}

fn run(name: &str) -> Result<fidelity::Metrics, String> {
    let dir = fixture_dir(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?;
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    analyze_ranges(&prep.lowered.program).map_err(|e| format!("range analysis refuses the program: {e}"))?;
    let ck = Checkpoint::open(&dir).map_err(|e| e.to_string())?;
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
    let loader = Resident(Arc::new(params));
    let vocab = prep.hl.vocab;
    // A learned position table bounds the sequence (HF indexes past it).
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(vocab, 6, 32.min(max_len), 7);
    let eval = fidelity::random_sequences(vocab, 3, 24.min(max_len), 1234);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
    // The artifact round-trips through the file format with its digest.
    let tmp = std::env::temp_dir().join(format!("palw-tir-{name}-{}.art", std::process::id()));
    let digest = artifact::write(&tmp, &prep.lowered.program, &mat.params, serde_json::json!({})).map_err(|e| e.to_string())?;
    let (h, back) = artifact::read(&tmp, &prep.lowered.program).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    if h.digest != digest || back.tensors != mat.params.tensors {
        return Err("artifact round trip".into());
    }
    let fl = fidelity::float_logits(&prep.hl, &loader, &eval, &quiet).map_err(|e| e.to_string())?;
    let il: Vec<Vec<Vec<f64>>> = eval
        .iter()
        .map(|s| fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("integer run: {e}"))?;
    Ok(fidelity::compare(&fl, &il, &eval))
}

fn check(name: &str, min_top1: f64, max_kl: f64) {
    if !fixture_dir(name).join("model.safetensors").exists() {
        eprintln!("fixture {name}: missing, skipped");
        return;
    }
    match run(name) {
        Ok(m) => {
            eprintln!(
                "fixture {name:>18}: top-1 {:.3}  KL {:.5} (max {:.4})  ppl {:.3} → {:.3} ({:+.2}%)  [{} positions]",
                m.top1_agreement,
                m.kl_mean,
                m.kl_max,
                m.ppl_float,
                m.ppl_int,
                m.ppl_delta * 100.0,
                m.positions
            );
            assert!(m.top1_agreement >= min_top1, "{name}: top-1 {} < {min_top1}", m.top1_agreement);
            assert!(m.kl_mean <= max_kl, "{name}: KL {} > {max_kl}", m.kl_mean);
        }
        Err(e) => panic!("fixture {name}: {e}"),
    }
}

macro_rules! dense {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            check(stringify!($name), 0.9, 0.01);
        }
    )*};
}

dense!(
    llama,
    llama_linear_tied,
    mistral_window,
    qwen2_sliding,
    qwen2_dynamic,
    qwen3_yarn,
    smollm3,
    granite,
    gemma,
    gemma2,
    gemma3,
    gemma3_vlm,
    llava,
    mistral3_vlm,
    phi3_longrope,
    olmo,
    olmo2,
    cohere,
    cohere2,
    stablelm,
    stablelm_parallel,
    starcoder2,
    exaone4,
    nemotron,
    phi,
    gpt2,
    gpt_neo,
    gpt_neox,
    gpt_neox_seq,
    gptj,
    falcon_new,
    falcon_mq,
    falcon_alibi,
    bloom,
    mpt,
    opt,
    opt_postln_proj,
    gpt_bigcode,
    gpt_bigcode_mha,
);
