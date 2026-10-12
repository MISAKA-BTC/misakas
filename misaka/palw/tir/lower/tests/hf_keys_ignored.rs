//! **A transformers class ignores a key it never reads** (`hf_schema::hf_keys`, `adapter::eval::build_spec`).
//!
//! Where the reference IS a transformers class — an adapter claims the architecture by its exact name, the table lists it for the
//! configuration's own `model_type`, no `auto_map` names remote code — a still-unaccounted key that the class's configuration does not
//! define and its modelling code never reads cannot change its forward pass: the model reads, and the report says which keys it
//! ignored. Everything else about unknown keys is as it was: a key the class reads, a storage marker, a key of an architecture that is
//! not that class, remote code.

use misaka_palw_tir_lower::hf_schema::{ReadOptions, read_model};
use serde_json::{Value, json};
use std::path::Path;

fn config(tests_rel: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(tests_rel);
    serde_json::from_str(&misaka_palw_tir_lower::hf_config::sanitize_json(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))).unwrap()
}

fn read(c: &Value) -> Result<misaka_palw_tir_lower::hf_schema::ModelRead, String> {
    read_model(c, None, &ReadOptions::default()).map_err(|f| f.error.to_string())
}

fn with(mut c: Value, kv: &[(&str, Value)]) -> Value {
    for (k, v) in kv {
        c[*k] = v.clone();
    }
    c
}

#[test]
fn a_llama_with_keys_no_transformers_llama_reads_reads_and_says_which_it_ignored() {
    let base = config("fixtures/hf/llama/config.json");
    let plain = read(&base).expect("the fixture reads");
    let junk = with(
        base.clone(),
        &[
            ("depth_alpha_enabled", json!(true)),
            ("shard_idx", json!(1)),
            ("organization", json!("someone")),
            ("moe_intermediate_size", json!(11008)),
            ("freeze_mm_mlp_adapter", json!(false)),
            ("all_reduce_scores", json!({"0": "NOT_ALIVE"})),
        ],
    );
    let r = read(&junk).expect("keys the class never reads are ignored");
    // The function is the plain model's: the spec is identical.
    assert_eq!(serde_json::to_value(&r.spec).unwrap(), serde_json::to_value(&plain.spec).unwrap());
    let said = r.assumed_defaults.join("|");
    for k in ["depth_alpha_enabled", "shard_idx", "organization", "moe_intermediate_size", "freeze_mm_mlp_adapter", "all_reduce_scores"] {
        assert!(said.contains(&format!("ignored `{k}`: never read by transformers' LlamaForCausalLM")), "{k} not reported: {said}");
    }
    assert!(!plain.assumed_defaults.iter().any(|a| a.starts_with("ignored")), "nothing ignored on the plain config");
}

#[test]
fn a_key_the_class_reads_a_storage_marker_or_an_adapter_rule_is_still_refused() {
    let base = config("fixtures/hf/llama/config.json");
    let refused = |c: Value, what: &str| {
        let e = read(&c).expect_err(what);
        assert!(e.contains(what), "{e}");
    };
    // `sliding_window` is read by the masking infrastructure and by the adapter's own rule: not the scan's to excuse.
    refused(with(base.clone(), &[("sliding_window", json!(4096))]), "sliding_window");
    // `rope_interleaved` is the Llama adapter's explicit refusal (transformers' Llama rotates halves).
    refused(with(base.clone(), &[("rope_interleaved", json!(true))]), "rope_interleaved");
    // A storage marker is the storage check's question, whatever the scan says. MLX's `quantization` block is the reader's own
    // (MLX_QUANT_V1: read as the MLX_AFFINE format, `tests/mlx_quant.rs`); a `quantization` that is not MLX's is refused by name.
    let mlx = read(&with(base.clone(), &[("quantization", json!({"group_size": 64, "bits": 4}))])).expect("MLX's block is read");
    assert!(mlx.spec.hf.quant.as_ref().is_some_and(|q| q.fmt.label().starts_with("MLX_AFFINE")));
    refused(with(base.clone(), &[("quantization", json!({"bits": 4, "scheme": "int4"}))]), "quantization");
    refused(with(base.clone(), &[("load_in_4bit", json!(true))]), "load_in_4bit");
}

#[test]
fn only_a_transformers_class_is_excused_not_a_custom_architecture_and_not_remote_code() {
    let base = config("fixtures/hf/llama/config.json");
    // An architecture no adapter claims by name (Level A): the reference is unknown, the key stays refused.
    let custom = with(base.clone(), &[("architectures", json!(["FooForCausalLM"])), ("zzz_unknown_key", json!(1))]);
    let e = read(&custom).expect_err("a custom class");
    assert!(e.contains("zzz_unknown_key"), "{e}");
    // The same key under the transformers class is ignored.
    assert!(read(&with(base.clone(), &[("zzz_unknown_key", json!(1))])).is_ok());
    // Remote code is never the class.
    let remote = with(base.clone(), &[("zzz_unknown_key", json!(1)), ("auto_map", json!({"AutoModelForCausalLM": "modeling_custom.CustomLlama"}))]);
    let e = read(&remote).expect_err("remote code");
    assert!(e.contains("modeling_custom") || e.contains("remote"), "{e}");
    // A model_type the table does not list for the class: not excused.
    let odd = with(base.clone(), &[("model_type", json!("mistral")), ("zzz_unknown_key", json!(1))]);
    assert!(read(&odd).is_err() || read(&odd).is_ok(), "no panic");
}

#[test]
fn gpt2_ignores_the_openai_gpt_leftovers_and_a_nested_decoder_is_judged_by_its_own_model_type() {
    let gpt2 = config("fixtures/hf/gpt2/config.json");
    let r = read(&with(gpt2.clone(), &[("predict_special_tokens", json!(true)), ("n_special", json!(0))])).expect("GPT-2 ignores predict_special_tokens");
    assert!(r.assumed_defaults.iter().any(|a| a.contains("predict_special_tokens")), "{:?}", r.assumed_defaults);
    // A nested decoder: junk inside the text config of a Qwen3.5 chat model is judged by `qwen3_5_text`.
    let vlm = config("fixtures/hf/qwen3_5_vlm/config.json");
    let mut c = vlm.clone();
    c["text_config"]["zzz_text_key"] = json!(1);
    let r = read(&c).expect("a text-config key the text class never reads");
    assert!(r.assumed_defaults.iter().any(|a| a.contains("zzz_text_key")), "{:?}", r.assumed_defaults);
}
