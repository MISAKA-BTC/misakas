//! **`REFERENCE_REMOTE_CODE_V1`** (FR-24): a way to pin a remote-code reference — ChatGLM3, whose forward is the repository's `modeling_chatglm.py`, not a transformers class.
//!
//! The lowering follows the repository's source. What this crate can check is the lowering against a tiny model built by `tools/remote_reference.py` from a modelling file
//! (`tests/fixtures/remote/chatglm3/`, from the corpus's RECONSTRUCTION of the file, `tools/corpus/remote/chatglm3/`: the repository is not available offline), with the
//! sha256 of that file beside it (`pin.json`). The reference is therefore UNVERIFIED — the same program would pass against a file that differs from the repository's in a way
//! the reconstruction shares — and every verdict on this architecture says so (`LOWERABLE_UNVERIFIED`). Declared in an adapter by `remote_code_pin`; never attested.

mod common;

use common::three_ways;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ReadOptions, read_model};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::{Act, Ffn, Mixer, Position, Reference};
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{fidelity, parse_config_str};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MODULE: &str = "modeling_chatglm.ChatGLMForConditionalGeneration";

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/remote/chatglm3")
}

fn config() -> String {
    std::fs::read_to_string(dir().join("config.json")).unwrap()
}

#[test]
fn a_chatglm3_config_reads_as_a_partial_interleaved_multi_query_decoder_that_follows_remote_code() {
    let s = parse_config_str(&config()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.layers.len(), 3);
    let Mixer::Attention(a) = &s.layers[0].mixer else { panic!("{:?}", s.layers[0].mixer) };
    assert_eq!((a.heads, a.kv_heads, a.head_dim, a.v_head_dim), (4, 2, 8, 8));
    assert!(a.q_bias && a.k_bias && a.v_bias && !a.o_bias, "add_qkv_bias gives the fused projection a bias; `dense` has none");
    assert!((a.scale - 1.0 / 8f64.sqrt()).abs() < 1e-15, "the softmax scale is 1/sqrt(kv_channels)");
    let Position::Rope(r) = &a.position else { panic!() };
    assert_eq!((r.rotary_dim, r.style), (4, misaka_palw_tir_lower::rope::RopeStyle::Interleaved), "the first half of each head, adjacent pairs");
    assert!(matches!(&s.layers[0].ffn, Ffn::Mlp(m) if m.gated && m.act == Act::Silu && m.intermediate == 64));
    assert!(!s.head.tied && s.final_norm.is_some());
    assert_eq!(s.hf.names["attn.qkv"], "transformer.encoder.layers.{L}.self_attention.query_key_value");
    assert_eq!(s.hf.names["lm_head"], "transformer.output_layer");
    assert_eq!(s.reference, Reference::RemoteCode { module: MODULE.into(), pin: None });
    let ids: Vec<&str> = s.features().iter().map(|u| u.id.0).collect();
    assert!(ids.contains(&"REFERENCE_REMOTE_CODE_V1"), "{ids:?}");
}

#[test]
fn the_float_reference_follows_the_pinned_file_and_the_program_is_the_same_on_the_reference_ref2_and_exec() {
    let prep = fidelity::prepare(&config(), &LowerOpts::default()).unwrap_or_else(|e| panic!("{e}"));
    let ck = Checkpoint::open(&dir()).unwrap();
    let (params, unused) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).unwrap_or_else(|e| panic!("{e}"));
    assert!(unused.is_empty(), "every tensor of the checkpoint is read (the rotary table's inv_freq is a buffer, not a weight): {unused:?}");
    // The float reference against the logits the modelling file gave (UNVERIFIED: the file is the corpus's reconstruction).
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(dir().join("reference.json")).unwrap()).unwrap();
    let tokens: Vec<usize> = meta["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let want: Vec<Vec<f64>> = meta["logits_full"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect())
        .collect();
    let got = Session::new(&prep.hl, &params).run(&tokens).unwrap_or_else(|e| panic!("{e}"));
    let scale = want.iter().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let max_abs = got.iter().zip(&want).flat_map(|(g, w)| g.iter().zip(w).map(|(a, b)| (*a as f64 - b).abs())).fold(0f64, f64::max);
    assert!(max_abs <= 1e-4 * scale, "float reference vs the pinned file's logits: {max_abs:.3e} against a scale of {scale:.2}");
    // The integer program against the float one, and the court's three implementations.
    let loader = Resident(Arc::new(params));
    let quiet = |_: usize, _: usize| {};
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32, 7);
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).unwrap();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
    let eval = fidelity::random_sequences(prep.hl.vocab, 3, 24, 1234);
    let fl = fidelity::float_logits(&prep.hl, &loader, &eval, &quiet).unwrap();
    let il: Vec<Vec<Vec<f64>>> =
        eval.iter().map(|s| fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}).unwrap()).collect();
    let m = fidelity::compare(&fl, &il, &eval);
    assert!(m.top1_agreement >= 0.9 && m.kl_mean <= 0.02, "top-1 {} KL {}", m.top1_agreement, m.kl_mean);
    let eval = fidelity::random_sequences(prep.hl.vocab, 2, 16, 97);
    let n = three_ways(&prep.lowered.program, &mat.params, &eval).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(n, 32);
}

#[test]
fn a_remote_code_adapter_pins_the_file_its_lowering_follows_and_the_verdict_stays_unverified() {
    let hash = "4c0efa5309c7a8eae63bf957bf56286b040f236af1f09d2d22f9d8cdf6deb72f";
    let text = include_str!("../adapters/chatglm3.json");
    let with_pin = |pin: Value| -> String {
        let mut v: Value = serde_json::from_str(text).unwrap();
        v["id"] = "chatglm3-pinned".into();
        v["remote_code_pin"] = serde_json::json!({ MODULE: pin });
        v.to_string()
    };
    let cfg: Value = serde_json::from_str(&config()).unwrap();
    let read = |t: String| read_model(&cfg, None, &ReadOptions { adapter: AdapterChoice::Text(t) });
    // A 64-hex-character pin is carried into the spec, declared by the adapter and attested by no one.
    let r = read(with_pin(hash.into())).unwrap_or_else(|f| panic!("{}", f.error));
    assert_eq!(r.spec.reference, Reference::RemoteCode { module: MODULE.into(), pin: Some(hash.into()) });
    // The built-in adapter has no file to hash: unpinned.
    let r = read_model(&cfg, None, &ReadOptions::default()).unwrap_or_else(|f| panic!("{}", f.error));
    assert_eq!(r.spec.reference, Reference::RemoteCode { module: MODULE.into(), pin: None });
    // Anything else than a lowercase sha256 is refused, by name.
    for bad in [Value::from("4c0efa53"), Value::from(hash.to_uppercase()), Value::from(7)] {
        let e = read(with_pin(bad.clone())).err().unwrap_or_else(|| panic!("{bad} must be refused")).error.to_string();
        assert!(e.contains("remote_code_pin") && e.contains("sha256"), "{e}");
    }
    // A module the adapter does not name is not modelled, pin or no pin.
    let mut other = cfg.clone();
    other["auto_map"]["AutoModelForCausalLM"] = "modeling_other.Other".into();
    let e = read_model(&other, None, &ReadOptions::default()).err().unwrap().error.to_string();
    assert!(e.contains("modeling_other.Other") && e.contains("not modelled"), "{e}");
}

#[test]
fn a_chatglm3_config_without_its_auto_map_is_refused_because_it_exists_only_as_remote_code() {
    let mut cfg: Value = serde_json::from_str(&config()).unwrap();
    cfg.as_object_mut().unwrap().remove("auto_map");
    let e = parse_config_str(&cfg.to_string()).unwrap_err().to_string();
    assert!(e.contains("remote code"), "{e}");
}

#[test]
fn a_chatglm3_config_the_adapter_does_not_model_is_refused_by_name() {
    for (from, to, needle) in [
        ("\"apply_residual_connection_post_layernorm\": false", "\"apply_residual_connection_post_layernorm\": true", "apply_residual_connection_post_layernorm"),
        ("\"pre_seq_len\": null", "\"pre_seq_len\": 8", "pre_seq_len"),
        ("\"quantization_bit\": 0", "\"quantization_bit\": 4", "quantization_bit"),
    ] {
        let text = config().replace(from, to);
        assert_ne!(text, config(), "{from} is in the fixture's config");
        let e = parse_config_str(&text).unwrap_err().to_string();
        assert!(e.contains(needle), "{needle}: {e}");
    }
}
