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
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet)
        .map_err(|e| format!("materialise: {e}"))?;
    // The artifact round-trips through the file format with its digest.
    let tmp = std::env::temp_dir().join(format!("palw-tir-{name}-{}.palwtir", std::process::id()));
    let digest =
        artifact::write(&tmp, &prep.lowered.program, &mat.params, [0u8; 64], serde_json::json!({})).map_err(|e| e.to_string())?;
    let (c, back) = artifact::read(&tmp, &prep.lowered.program).map_err(|e| e.to_string())?;
    let file = misaka_palw_tir_artifact::file_digest_v1(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    let hex: String = file.iter().map(|b| format!("{b:02x}")).collect();
    if hex != digest || back.tensors != mat.params.tensors || c.program != prep.lowered.program {
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
    paligemma_vlm,
    mistral3_vlm,
    phi3_longrope,
    olmo,
    olmo2,
    olmo3,
    glm,
    glm4,
    ministral,
    ministral3,
    gemma4_kvshare,
    cohere,
    cohere2,
    bitnet,
    apertus,
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

macro_rules! moe {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            check(stringify!($name), 0.85, 0.02);
        }
    )*};
}

moe!(mixtral, qwen2_moe, qwen3_moe, olmoe, granitemoe, gpt_oss, deepseek_v2, deepseek_v2_lite, deepseek_v3, glm4_moe, phimoe, llama4, llama4_vlm, gemma4);

macro_rules! recurrent {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            check(stringify!($name), 0.8, 0.03);
        }
    )*};
}

recurrent!(qwen3_next, qwen3_5, qwen3_5_moe, qwen3_5_vlm, mamba, falcon_mamba, mamba2, jamba, rwkv, lfm2, kimi_linear, zamba2, nemotron_h, nemotron_h_latent, falcon_h1, falcon_h1_norm, falcon_h1_gate);

// Qwen4-Exp's generic features (hyper-connection streams, hashed n-gram per-layer embeddings, sparse block attention,
// delta nets at any ratio, a routed MoE): the named acceptance matrix is `tests/qwen4_exp.rs`; here every fixture
// holds the same bar as the recurrent families.
recurrent!(
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

/// Per-site errors of one fixture (debugging aid): `PALW_SITES=qwen3_5 cargo test … -- --ignored`.
#[test]
#[ignore]
fn site_errors_of_one_fixture() {
    let name = std::env::var("PALW_SITES").unwrap_or_else(|_| "llama".into());
    let dir = fixture_dir(&name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap();
    let ck = Checkpoint::open(&dir).unwrap();
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
    for e in errs.iter().filter(|e| std::env::var("PALW_PREFIX").map(|p| e.key.starts_with(&p)).unwrap_or(true)).take(40) {
        eprintln!("{:>32}  rel {:.5}  max|Δ| {:.4e}  |f|max {:.4e}", e.key, e.rel_l2, e.max_abs, e.float_absmax);
    }
}

/// The calibration-length rule (freeze-v1 §5.2): a recurrent program refuses a calibration shorter
/// than its evaluated context; an attention-only one is not bound by it.
#[test]
fn a_recurrent_program_is_calibrated_as_long_as_its_context() {
    let hl_of = |name: &str| {
        let cfg = std::fs::read_to_string(fixture_dir(name).join("config.json")).expect("config");
        let spec = misaka_palw_tir_lower::parse_config_str(&cfg).expect("spec");
        misaka_palw_tir_lower::hl::build_program(&spec).expect("hl")
    };
    let short = vec![vec![1usize; 32]; 4];
    for recurrent in ["mamba", "falcon_mamba", "mamba2", "jamba", "qwen3_5", "qwen3_next", "rwkv", "lfm2", "kimi_linear", "zamba2", "nemotron_h", "falcon_h1"] {
        let hl = hl_of(recurrent);
        assert!(fidelity::check_calibration_length(&hl, &short, 64).is_err(), "{recurrent}");
        assert_eq!(fidelity::check_calibration_length(&hl, &short, 32), Ok(Some(32)), "{recurrent}");
    }
    assert_eq!(fidelity::check_calibration_length(&hl_of("llama"), &short, 4096), Ok(None));
}

/// A head tied to the embedding reads the table the gather reads: ONE `i16` param. (Until
/// 2026-09-28 the table set looked at a gather's FIRST input — the token — so it was empty, and
/// every tied head declared a second, `i8` copy of the table; Mamba-370m's top-1 fell from 0.97
/// to 0.74 on that alone.)
#[test]
fn a_tied_head_reads_the_gathered_tables_i16_codes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<_> = std::fs::read_dir(&root).expect("fixtures").map(|e| e.expect("entry").path()).collect();
    names.sort();
    let mut tied = Vec::new();
    for dir in names {
        let Ok(cfg) = std::fs::read_to_string(dir.join("config.json")) else { continue };
        let prep = fidelity::prepare(&cfg, &LowerOpts::default()).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        if !prep.spec.head.tied {
            continue;
        }
        let tables: Vec<_> = prep
            .lowered
            .program
            .params
            .iter()
            .filter(|p| p.name.starts_with("embed.table"))
            .map(|p| (p.name.clone(), p.dtype))
            .collect();
        assert_eq!(tables, vec![("embed.table".to_string(), misaka_palw_tir::DType::I16)], "{}", dir.display());
        tied.push(dir.file_name().unwrap_or_default().to_string_lossy().to_string());
    }
    eprintln!("tied heads on the table's i16 codes: {}", tied.join(" "));
    assert!(tied.len() >= 5, "only {} tied fixtures: {tied:?}", tied.len());
}

/// A selective scan's gated output reaches its projection on the `i32` rail. The scan's value is
/// wide, the product with the gate's code is exact in `i64`, and it is narrowed once. Mamba-370m's
/// layer-29 product reached 4.5x its calibrated absmax late in a 4,096-token document, and a
/// 16-bit site saturated there: the drift. No 16-bit code remains between the scan and `out_proj`.
#[test]
fn a_scans_gated_output_is_carried_wide_into_its_projection() {
    use misaka_palw_tir_lower::lower::Base;
    for name in ["mamba", "falcon_mamba", "jamba"] {
        let cfg = std::fs::read_to_string(fixture_dir(name).join("config.json")).expect("config");
        let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
        let wide: Vec<_> = prep
            .lowered
            .site_nodes
            .iter()
            .filter(|(_, (s, _, _))| s == "mamba.gated" || s == "mamba.scan")
            .map(|((b, n), (s, k, _))| (*b, *n, s.clone(), k.clone()))
            .collect();
        assert!(wide.len() >= 2, "{name}: {wide:?}");
        for (b, n, s, k) in &wide {
            assert!(matches!(k.base, Base::Site { wide: true, split: 0, .. }) && k.factor == 256.0, "{name} {s}: {k:?}");
            let dt = prep.lowered.program.blocks[*b as usize].nodes[*n as usize].out.dtype;
            assert_eq!(dt, misaka_palw_tir::DType::I32, "{name} {s}");
        }
    }
}
