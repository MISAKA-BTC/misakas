//! **GGUF checkpoints end to end** on files written in numpy per the GGUF and ggml block
//! specifications (`tools/gen_gguf_fixtures.py`): the file is parsed here, its metadata mapped to
//! the Hugging Face config of the model it was converted from, its block formats unpacked to their
//! stored integers, and the projections lowered from those integers.
//!
//! For each fixture: the mapped config equals the one the model was built with; every GGUF tensor
//! is read; the float reference (the dequantised blocks) matches `transformers` with the same
//! weights substituted; the program passes the range analysis; every per-group scale is exact at
//! its row's unit; and the integer program agrees with the float reference — beside the same
//! weights through the W8 path.

use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_lower::fidelity::{self, Metrics};
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::gguf::GgufModel;
use misaka_palw_tir_lower::lower::{self, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gguf").join(name)
}

/// The mapped config against the one the fixture was built with, key by key.
fn same_config(mapped: &serde_json::Value, hf: &serde_json::Value) -> Result<(), String> {
    let num = |v: &serde_json::Value| v.as_f64();
    for k in [
        "hidden_size",
        "intermediate_size",
        "num_hidden_layers",
        "num_attention_heads",
        "num_key_value_heads",
        "head_dim",
        "rms_norm_eps",
        "vocab_size",
        "sliding_window",
        "attn_logit_softcapping",
        "final_logit_softcapping",
        "query_pre_attn_scalar",
    ] {
        let (a, b) = (mapped.get(k), hf.get(k));
        match (a, b) {
            (Some(a), Some(b)) if num(a) == num(b) => {}
            (None, None) => {}
            (None, Some(b)) if b.is_null() => {}
            (Some(a), Some(b)) if a.is_null() && b.is_null() => {}
            _ => return Err(format!("config `{k}`: mapped {a:?}, built with {b:?}")),
        }
    }
    let theta = |c: &serde_json::Value| c.get("rope_theta").or_else(|| c.pointer("/rope_parameters/rope_theta")).and_then(|v| v.as_f64());
    if theta(mapped).unwrap_or(10000.0) != theta(hf).unwrap_or(10000.0) {
        return Err(format!("rope theta: mapped {:?}, built with {:?}", theta(mapped), theta(hf)));
    }
    // `tie_word_embeddings` is left out of transformers' diff dict at the class default; a wrong
    // tie shows in the logits check.
    Ok(())
}

struct Outcome {
    types: String,
    hf_max_abs: f64,
    hf_scale: f64,
    quantised: usize,
    inexact: usize,
    exact: Metrics,
    w8: Metrics,
}

fn run(name: &str) -> Result<Outcome, String> {
    let dir = fixture_dir(name);
    let model = GgufModel::open(&dir.join("model.gguf")).map_err(|e| format!("open: {e}"))?;
    let hf_cfg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("hf_config.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    same_config(&model.config, &hf_cfg)?;
    let spec = model.spec().map_err(|e| format!("spec: {e}"))?;
    let prep = fidelity::prepare_spec(spec, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let quantised = prep.lowered.program.params.iter().filter(|p| p.name.ends_with(".qa")).count();
    analyze_ranges(&prep.lowered.program).map_err(|e| format!("range analysis refuses the program: {e}"))?;
    let (params, unused) = ParamStore::from_source(&prep.hl, &prep.binding, &model).map_err(|e| e.to_string())?;
    if !unused.is_empty() {
        return Err(format!("GGUF tensors the program never reads: {unused:?}"));
    }
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
    let measure = |lw: &lower::Lowered| -> Result<(Metrics, usize), String> {
        let mat = materialise(lw, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(|e| format!("materialise: {e}"))?;
        let il: Vec<Vec<Vec<f64>>> = eval
            .iter()
            .map(|s| fidelity::int_logits(&lw.program, &mat.params, s, mat.logits_scale, &|_| {}))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("integer run: {e}"))?;
        Ok((fidelity::compare(&fl, &il, &eval), mat.quant_inexact))
    };
    let (exact, inexact) = measure(&prep.lowered)?;
    let w8 = lower::lower(&prep.hl, &LowerOpts::default()).map_err(|e| format!("W8 lowering: {e}"))?;
    let (w8m, _) = measure(&w8)?;
    let types = meta["types"].as_object().map(|m| {
        let mut v: Vec<String> = m.values().filter_map(|t| t.as_str()).map(str::to_string).collect();
        v.sort();
        v.dedup();
        v.join(",")
    });
    Ok(Outcome { types: types.unwrap_or_default(), hf_max_abs, hf_scale, quantised, inexact, exact, w8: w8m })
}

fn check(name: &str) {
    if !fixture_dir(name).join("model.gguf").exists() {
        eprintln!("fixture {name}: missing, skipped");
        return;
    }
    match run(name) {
        Ok(o) => {
            eprintln!(
                "{name:>20} [{}]: HF max|Δ| {:.1e} (scale {:.1}); {} projections from integers, inexact scales {}; exact top-1 {:.3} KL {:.5}; W8 top-1 {:.3} KL {:.5}",
                o.types,
                o.hf_max_abs,
                o.hf_scale,
                o.quantised,
                o.inexact,
                o.exact.top1_agreement,
                o.exact.kl_mean,
                o.w8.top1_agreement,
                o.w8.kl_mean
            );
            assert!(o.hf_max_abs <= 1e-4 * o.hf_scale, "{name}: float reference vs HF {:.3e}", o.hf_max_abs);
            assert!(o.quantised > 0, "{name}: no projection lowered from its integers");
            assert_eq!(o.inexact, 0, "{name}: stored scales rounded at their row's unit");
            assert!(o.exact.top1_agreement >= 0.9, "{name}: top-1 {}", o.exact.top1_agreement);
            assert!(o.exact.kl_mean <= 0.01, "{name}: KL {}", o.exact.kl_mean);
        }
        Err(e) => panic!("fixture {name}: {e}"),
    }
}

macro_rules! gguf {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            check(stringify!($name));
        }
    )*};
}

gguf!(gguf_llama_q4_k_m, gguf_qwen2_q5_k_m, gguf_qwen3_q8_0, gguf_gemma_q4_0, gguf_gemma2_q6_k, gguf_mistral_mix);

/// Hostile headers are errors, never panics or unbounded allocations.
#[test]
fn a_malformed_gguf_is_refused() {
    use misaka_palw_tir_lower::gguf::GgufFile;
    let d = std::env::temp_dir().join(format!("tir-gguf-bad-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join("bad.gguf");
    let mut v = b"GGUF".to_vec();
    v.extend(3u32.to_le_bytes());
    v.extend(1u64.to_le_bytes());
    v.extend(1u64.to_le_bytes());
    // A key whose length claims 2^40 bytes.
    v.extend((1u64 << 40).to_le_bytes());
    std::fs::write(&p, &v).unwrap();
    assert!(GgufFile::open(&p).is_err());
    std::fs::write(&p, b"GGUF\x01\x00\x00\x00").unwrap();
    assert!(GgufFile::open(&p).is_err());
    std::fs::write(&p, b"NOPE").unwrap();
    assert!(GgufFile::open(&p).is_err());
    let _ = std::fs::remove_dir_all(d);
}
