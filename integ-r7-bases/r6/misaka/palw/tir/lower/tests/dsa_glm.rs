//! **FR-09 for GLM-MoE-DSA: the ONE token indexer of DeepSeek-V3.2, its rotary interleaved — data, no code** (COV-P1P2,
//! 2026-10-08; adapter `glm-moe-dsa`, which extends `deepseek-v32`).
//!
//! The fixture (`tests/fixtures/fr09/glm_moe_dsa`) is a tiny random-init `GlmMoeDsaForCausalLM` written offline by
//! `tools/gen_glm_dsa_fixture.py` (transformers 5.17, the shape of the DeepSeek-V3.2 fixture, `index_topk` 4 over ten tokens), its
//! logits stored twice: with the IR's tie rule (lowest index — transformers' `topk` line replaced by a stable descending sort) and
//! with `torch.topk`'s own unpinned order. The real configurations (`tests/configs/real/glm-5.json`, `glm-5.3.json`: Hub configs,
//! read as data) say what the adapter reads and what it refuses by name.
mod common;

use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::{ArchSpec, Mixer};
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{LowerError, fidelity, hf_weights, hl, parse_config_str};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fr09/glm_moe_dsa")
}

fn config() -> String {
    std::fs::read_to_string(dir().join("config.json")).expect("config")
}

fn real(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real").join(name)).expect("a real config")
}

fn indexer(spec: &ArchSpec) -> serde_json::Value {
    let Mixer::Mla(m) = &spec.layers[0].mixer else { panic!("an MLA layer") };
    serde_json::to_value(m.indexer.as_ref().expect("an indexer")).unwrap()
}

#[test]
fn glm_moe_dsa_float_matches_hf_with_the_irs_tie_rule() {
    let spec = parse_config_str(&config()).expect("the glm-moe-dsa adapter reads its fixture");
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
    let want = rows("logits_full");
    let scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let d = diff(&want);
    eprintln!("glm_moe_dsa: vs HF (lowest-index ties): max|Δ| {:.2e} (scale {scale:.1})", d.iter().cloned().fold(0.0, f64::max));
    assert!(d.iter().all(|x| *x <= 1e-4 * scale), "float vs HF with the IR's tie rule: {d:?}");
    // torch.topk's own order differs exactly where a k-th place is tied (the last position of this fixture: ReLU zeros tie), and
    // nowhere else.
    let d = diff(&rows("logits_full_torch_topk"));
    eprintln!("glm_moe_dsa: vs torch.topk's tie order: {d:?}");
    let tied: Vec<usize> = (0..d.len()).filter(|i| d[*i] > 1e-2 * scale).collect();
    assert_eq!(tied, vec![d.len() - 1], "the documented tie moved elsewhere: {d:?}");
    assert!(d[..d.len() - 1].iter().all(|x| *x <= 1e-4 * scale), "{d:?}");
}

#[test]
fn glm_moe_dsa_integer_follows_float() {
    let prep = fidelity::prepare(&config(), &LowerOpts::default()).expect("prepare");
    misaka_palw_tir::interval::analyze_ranges(&prep.lowered.program).expect("the range analysis proves the program");
    let ck = Checkpoint::open(&dir()).unwrap();
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap();
    let loader = Resident(Arc::new(params));
    let quiet = |_: usize, _: usize| {};
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32, 7);
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
    let eval = fidelity::random_sequences(64, 3, 24, 1234);
    let float = fidelity::float_logits(&prep.hl, &loader, &eval, &quiet).unwrap();
    let int: Vec<Vec<Vec<f64>>> = eval
        .iter()
        .map(|s| fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}))
        .collect::<Result<_, _>>()
        .unwrap();
    let m = fidelity::compare(&float, &int, &eval);
    eprintln!("glm_moe_dsa integer: top-1 {:.3} KL {:.5}", m.top1_agreement, m.kl_mean);
    assert!(m.top1_agreement >= 0.9 && m.kl_mean <= 0.01, "top-1 {} KL {}", m.top1_agreement, m.kl_mean);
}

/// The two families share the indexer: the same spec in every field but the rotary's pairing (data).
#[test]
fn deepseek_v32_and_glm_moe_dsa_share_one_indexer_but_the_rope_pairing() {
    let glm = parse_config_str(&config()).unwrap();
    let ds = parse_config_str(
        &std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fr09/deepseek_v32/config.json")).unwrap(),
    )
    .unwrap();
    let (mut a, mut b) = (indexer(&glm), indexer(&ds));
    assert_eq!(a["rope"]["style"], "Interleaved");
    assert_eq!(b["rope"]["style"], "Half");
    a["rope"]["style"] = serde_json::json!("-");
    b["rope"]["style"] = serde_json::json!("-");
    assert_eq!(a, b, "the indexers differ in more than the rotary pairing");
}

#[test]
fn what_glm_moe_dsa_refuses_by_name() {
    let refused = |cfg: &str, needle: &str| match parse_config_str(cfg) {
        Err(LowerError::NotLowerable(m)) => assert!(m.contains(needle), "`{needle}` not in {m}"),
        other => panic!("expected a refusal naming {needle}: {other:?}"),
    };
    // Cross-layer index sharing, in each of transformers' spellings.
    let mut v: serde_json::Value = serde_json::from_str(&config()).unwrap();
    v["indexer_types"] = serde_json::json!(["full", "shared"]);
    refused(&v.to_string(), "ATTN_TOKEN_INDEXER_SHARED_V1");
    let mut v: serde_json::Value = serde_json::from_str(&config()).unwrap();
    v.as_object_mut().unwrap().remove("indexer_types");
    v["index_topk_freq"] = serde_json::json!(2);
    v["index_skip_topk_offset"] = serde_json::json!(1);
    refused(&v.to_string(), "ATTN_TOKEN_INDEXER_SHARED_V1");
    // A configuration that claims a half-split rotary: transformers interleaves anyway — another model.
    let mut v: serde_json::Value = serde_json::from_str(&config()).unwrap();
    v["rope_interleave"] = serde_json::json!(false);
    refused(&v.to_string(), "rope_interleave");
    // GLM-5.3 shares its top-k across layers (freq 4, offset 3): refused by name, not misread.
    refused(&real("glm-5.3.json"), "ATTN_TOKEN_INDEXER_SHARED_V1");
}

#[test]
fn glm_5_reads_as_deepseek_v32_with_interleaved_rotaries() {
    let spec = parse_config_str(&real("glm-5.json")).expect("GLM-5: every layer's indexer is its own");
    assert_eq!(spec.num_layers(), 78);
    let ix = indexer(&spec);
    assert_eq!((ix["heads"].as_u64(), ix["head_dim"].as_u64(), ix["topk"].as_u64()), (Some(32), Some(128), Some(2048)));
    assert_eq!(ix["rope"]["style"], "Interleaved");
    // DeepSeek-V3.2 (the released one): its FP8 checkpoint's `scale_fmt: ue8m0` is a key the FP8_BLOCK descriptor does not read —
    // refused by name (a quantisation decision, not DSA); the same configuration in bf16 reads, half-split indexer, YaRN rotary.
    match parse_config_str(&real("deepseek-v3.2.json")) {
        Err(LowerError::NotLowerable(m)) => assert!(m.contains("scale_fmt") && m.contains("FP8_BLOCK"), "{m}"),
        other => panic!("DeepSeek-V3.2 FP8: {other:?}"),
    }
    let mut v: serde_json::Value = serde_json::from_str(&real("deepseek-v3.2.json")).unwrap();
    v.as_object_mut().unwrap().remove("quantization_config");
    let ds = parse_config_str(&v.to_string()).expect("DeepSeek-V3.2 in bf16");
    assert_eq!(indexer(&ds)["rope"]["style"], "Half");
}
