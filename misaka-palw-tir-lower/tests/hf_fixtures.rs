//! **The HF fixture hook**: the float reference against logits produced by `transformers` itself.
//!
//! Each fixture under `tests/fixtures/hf/<name>/` holds the `config.json` transformers saved, the
//! BF16 weights, and the logits of a fixed token sequence (`tools/gen_hf_fixtures.py`, run offline
//! against a local transformers install; the version is recorded in each `logits.json`). A missing
//! fixture is skipped with a message, never failed: the fixtures are optional inputs.
//!
//! What a pass means: config parsed without refusals, every HL param bound, EVERY checkpoint
//! tensor consumed (an unread tensor is a feature we might be missing), and logits equal to HF's
//! within float tolerance with the same argmax at every position.

use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{hf_weights, hl, parse_config_str};
use std::path::{Path, PathBuf};

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name)
}

struct Outcome {
    max_abs: f64,
    scale: f64,
    positions: usize,
    reference: &'static str,
}

fn run_fixture(dir: &Path) -> Result<Outcome, String> {
    let cfg = std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?;
    let spec = parse_config_str(&cfg).map_err(|e| format!("config: {e}"))?;
    let prog = hl::build_program(&spec).map_err(|e| format!("hl: {e}"))?;
    let binding = hf_weights::bind(&spec, &prog).map_err(|e| format!("bind: {e}"))?;
    let ck = Checkpoint::open(dir).map_err(|e| format!("checkpoint: {e}"))?;
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).map_err(|e| format!("params: {e}"))?;
    if !unused.is_empty() {
        return Err(format!("checkpoint tensors the program never reads: {unused:?}"));
    }
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("logits.json")).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let tokens: Vec<usize> = meta["tokens"].as_array().ok_or("tokens")?.iter().map(|t| t.as_u64().unwrap_or(0) as usize).collect();
    let (key, reference) = if meta.get("logits_decode").is_some() { ("logits_decode", "decode") } else { ("logits_full", "full") };
    let want: Vec<Vec<f64>> = meta[key]
        .as_array()
        .ok_or("logits")?
        .iter()
        .map(|r| r.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default())
        .collect();
    let mut sess = Session::new(&prog, &params);
    let got = sess.run(&tokens).map_err(|e| format!("run: {e}"))?;
    let scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let mut max_abs = 0f64;
    for (p, (g, w)) in got.iter().zip(&want).enumerate() {
        if g.len() != w.len() {
            return Err(format!("position {p}: {} logits vs HF's {}", g.len(), w.len()));
        }
        let mut row_max = 0f64;
        for (a, b) in g.iter().zip(w) {
            row_max = row_max.max((*a as f64 - b).abs());
        }
        max_abs = max_abs.max(row_max);
        // Same argmax unless HF's top two are within the tolerance.
        let arg = |v: &[f64]| v.iter().enumerate().fold(0, |bi, (i, x)| if *x > v[bi] { i } else { bi });
        let gw: Vec<f64> = g.iter().map(|x| *x as f64).collect();
        let (ag, aw) = (arg(&gw), arg(w));
        if ag != aw {
            let mut sorted = w.clone();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
            if sorted[0] - sorted[1] > 1e-3 * scale {
                return Err(format!("position {p}: argmax {ag} vs HF {aw} (row max diff {row_max:.2e})"));
            }
        }
    }
    Ok(Outcome { max_abs, scale, positions: got.len(), reference })
}

/// Tolerance: f32 storage with different summation orders; 1e-4 of the logit scale is ~100×
/// the observed agreement and far below any semantic error (those are ≥ 1e-2).
const TOL: f64 = 1e-4;

fn check(name: &str) {
    let dir = fixture_dir(name);
    if !dir.join("logits.json").exists() {
        eprintln!("SKIPPED {name}: no HF fixture at {} (generate with tools/gen_hf_fixtures.py)", dir.display());
        return;
    }
    match run_fixture(&dir) {
        Ok(o) => {
            eprintln!("{name}: {} positions vs HF ({}) max|Δ| {:.2e} (scale {:.1})", o.positions, o.reference, o.max_abs, o.scale);
            assert!(o.max_abs <= TOL * o.scale, "{name}: max |Δlogit| {:.3e} exceeds {:.1e}·{:.1}", o.max_abs, TOL, o.scale);
        }
        Err(e) => panic!("{name}: {e}"),
    }
}

macro_rules! fixtures {
    ($($n:ident),* $(,)?) => {
        $( #[test] fn $n() { check(stringify!($n)); } )*
        /// Every fixture directory on disk has a test (a new fixture cannot be silently unchecked).
        #[test]
        fn every_fixture_on_disk_is_listed() {
            let known = [$(stringify!($n)),*];
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
            let Ok(rd) = std::fs::read_dir(&root) else {
                eprintln!("SKIPPED: no fixture directory {}", root.display());
                return;
            };
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                assert!(known.contains(&n.as_str()), "fixture `{n}` has no test in tests/hf_fixtures.rs");
            }
        }
    };
}

fixtures!(
    llama,
    llama_linear_tied,
    mistral_window,
    qwen2_dynamic,
    qwen2_sliding,
    qwen3_yarn,
    gemma,
    gemma2,
    gemma3,
    phi,
    phi3_longrope,
    gpt2,
    gpt_neo,
    gpt_neox,
    gpt_neox_seq,
    gptj,
    falcon_mq,
    falcon_new,
    falcon_alibi,
    stablelm,
    stablelm_parallel,
    starcoder2,
    gpt_bigcode,
    gpt_bigcode_mha,
    olmo,
    olmo2,
    olmo3,
    glm,
    glm4,
    ministral,
    cohere,
    cohere2,
    granite,
    bitnet,
    apertus,
    lfm2,
    kimi_linear,
    zamba2,
    nemotron_h,
    nemotron_h_latent,
    falcon_h1,
    falcon_h1_norm,
    falcon_h1_gate,
    nemotron,
    exaone4,
    smollm3,
    bloom,
    mpt,
    opt,
    opt_postln_proj,
    mixtral,
    qwen2_moe,
    qwen3_moe,
    olmoe,
    granitemoe,
    glm4_moe,
    phimoe,
    llama4,
    llama4_vlm,
    gemma4,
    gemma4_kvshare,
    ministral3,
    deepseek_v2,
    deepseek_v2_lite,
    deepseek_v3,
    gpt_oss,
    qwen3_next,
    qwen3_5,
    qwen3_5_moe,
    jamba,
    mamba,
    falcon_mamba,
    mamba2,
    rwkv,
    gemma3_vlm,
    qwen3_5_vlm,
    mistral3_vlm,
    llava,
    paligemma_vlm,
    qwen4_exp,
    qwen4_gdn_1_1,
    qwen4_gdn_1_3,
    qwen4_gdn_1_4,
    qwen4_gdn_sigmoid_gate,
    qwen4_hc1,
    qwen4_hc2,
    qwen4_hc_edge,
    qwen4_moe512,
    qwen4_ple_bigram,
    qwen4_ple_boundary,
    qwen4_ple_trigram,
    qwen4_qsa_k1,
    qwen4_qsa_kmax,
    qwen4_qsa_r3,
    qwen4_qsa_tie,
);
