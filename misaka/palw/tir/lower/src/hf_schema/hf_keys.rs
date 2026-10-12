//! **The configuration keys a transformers architecture reads** (`data/hf-config-keys-v1.json`, derived from the installed
//! transformers' own source by `tools/gen_hf_config_keys.py`).
//!
//! A `config.json` key the reader does not model is refused: "a key that might change the math is refused, never ignored". That is
//! the right default for an architecture nobody has identified, and too blunt for one that IS a transformers class. The reference of
//! `LlamaForCausalLM` is `transformers`' Llama; `LlamaConfig` loads a key it does not define as a passive attribute, and
//! `modeling_llama.py` reads only what it reads. A key in neither set — a training tool's bookkeeping (`depth_alpha_enabled`,
//! `shard_idx`, `organization`), another architecture's leftover (`moe_intermediate_size` in a Llama config, `predict_special_tokens`
//! in a GPT-2's) — cannot change the reference's forward pass. This module says which of the unread keys those are.
//!
//! What it proves, and what it does not:
//!
//! * a key is **never read** by the class iff it is in none of: the configuration class's fields (`__init__` parameters,
//!   `attribute_map`, sub-configs, annotations), the keys its modelling files read (`config.x`, `getattr(config, "x")`,
//!   `hasattr`, `config.get("x")`, `config["x"]`), and the keys the shared infrastructure reads (`shared_reads`: rope utils, masking,
//!   cache, the base configuration). A key the reader does not model but the class reads stays refused;
//! * it applies only where the reference IS the class: the architecture is one the table lists for the configuration's own
//!   `model_type`, the configuration names no `auto_map` (remote code is never the class), and an adapter claimed the architecture by
//!   its exact name — see `adapter::eval::build_spec`;
//! * the scan is static: a key read through a computed name (`getattr(config, name)`) is invisible to it. A storage-format marker is
//!   therefore never inert whatever the scan says ([`is_storage_marker`]): the weights' form is a different question from the math.
//!
//! The table records the transformers version it was derived from ([`transformers_version`]); a key's verdict is by that version.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const TABLE_JSON: &str = include_str!("data/hf-config-keys-v1.json");

struct Model {
    classes: BTreeSet<String>,
    keys: BTreeSet<String>,
}

struct Table {
    transformers: String,
    shared: BTreeSet<String>,
    models: BTreeMap<String, Model>,
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| {
        let v: serde_json::Value = serde_json::from_str(TABLE_JSON).unwrap_or(serde_json::Value::Null);
        let set = |x: &serde_json::Value| -> BTreeSet<String> {
            x.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()).unwrap_or_default()
        };
        let models = v["models"]
            .as_object()
            .map(|m| m.iter().map(|(k, e)| (k.clone(), Model { classes: set(&e["classes"]), keys: set(&e["keys"]) })).collect())
            .unwrap_or_default();
        Table { transformers: v["transformers"].as_str().unwrap_or("").to_string(), shared: set(&v["shared_reads"]), models }
    })
}

/// The transformers version the table was derived from.
pub fn transformers_version() -> &'static str {
    &table().transformers
}

/// A key that marks how the weights are stored (quantisation, packing): never inert — the form of the weights is the storage
/// check's to judge, and a static scan of the modelling code does not speak for it.
pub fn is_storage_marker(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["quant", "bnb", "awq", "gptq", "mlx", "gguf", "torchao", "hqq", "fp8", "fp4", "int4", "int8", "nf4", "bits", "bit", "load_in", "exl2", "exl3"]
        .iter()
        .any(|m| k.contains(m))
}

/// Whether the table knows `model_type` and, when `arch` is given, lists that class as one of its modelling classes.
pub fn is_transformers_class(arch: Option<&str>, model_type: &str) -> bool {
    table().models.get(model_type).is_some_and(|m| arch.is_none_or(|a| m.classes.contains(a)))
}

/// Of the unread top-level `keys` of a configuration whose reference is the transformers class `arch` of `model_type`
/// (`arch = None` for a nested decoder whose wrapper class matched), those the class provably never reads. Empty when the
/// table does not know the class.
pub fn never_read(arch: Option<&str>, model_type: &str, keys: &[String]) -> Vec<String> {
    let t = table();
    let Some(m) = t.models.get(model_type) else { return Vec::new() };
    if let Some(a) = arch
        && !m.classes.contains(a)
    {
        return Vec::new();
    }
    keys.iter()
        .filter(|k| !k.contains('.') && !t.shared.contains(*k) && !m.keys.contains(*k) && !is_storage_marker(k))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_key_no_transformers_llama_reads_is_never_read_and_one_it_reads_is_not() {
        let junk = k(&["depth_alpha_enabled", "shard_idx", "organization", "moe_intermediate_size", "predict_special_tokens", "freeze_mm_mlp_adapter"]);
        assert_eq!(never_read(Some("LlamaForCausalLM"), "llama", &junk), junk);
        // What LlamaConfig defines or llama's modelling reads stays: head_dim, sliding_window (shared masking), rope_theta, mlp_bias.
        let read = k(&["head_dim", "sliding_window", "rope_theta", "mlp_bias", "attention_bias", "pretraining_tp", "hidden_act"]);
        assert!(never_read(Some("LlamaForCausalLM"), "llama", &read).is_empty(), "{:?}", never_read(Some("LlamaForCausalLM"), "llama", &read));
    }

    #[test]
    fn the_verdict_is_per_class_phi3_reads_a_key_llama_ignores_and_gpt2_reads_its_activation() {
        assert!(never_read(Some("Phi3ForCausalLM"), "phi3", &k(&["original_max_position_embeddings"])).is_empty());
        assert!(never_read(Some("GPT2LMHeadModel"), "gpt2", &k(&["activation_function"])).is_empty());
        assert_eq!(never_read(Some("LlamaForCausalLM"), "llama", &k(&["activation_function"])), k(&["activation_function"]));
        assert_eq!(never_read(Some("GPT2LMHeadModel"), "gpt2", &k(&["predict_special_tokens"])), k(&["predict_special_tokens"]));
    }

    #[test]
    fn it_speaks_only_for_a_transformers_class_of_that_model_type() {
        let junk = k(&["zzz_unknown_key"]);
        // A class name the table does not list for the model type (a custom class named like a Llama), an unknown model type, and a
        // nested path are not the class's to excuse.
        assert!(never_read(Some("MyLlamaForCausalLM"), "llama", &junk).is_empty());
        assert!(never_read(Some("LlamaForCausalLM"), "not_a_model", &junk).is_empty());
        assert!(never_read(Some("LlamaForCausalLM"), "llama", &k(&["text_config.zzz"])).is_empty());
        assert!(is_transformers_class(Some("LlamaForCausalLM"), "llama") && !is_transformers_class(Some("Foo"), "llama"));
        // A nested decoder (no class check) is judged by its own model type.
        assert_eq!(never_read(None, "llama", &junk), junk);
    }

    #[test]
    fn a_storage_format_marker_is_never_inert_whatever_the_scan_says() {
        for key in ["quantization", "quantization_config", "load_in_4bit", "bnb_4bit_quant_type", "bits", "awq_version", "mlx_group", "gguf_file"] {
            assert!(is_storage_marker(key), "{key}");
            assert!(never_read(Some("LlamaForCausalLM"), "llama", &k(&[key])).is_empty() || !is_storage_marker(key), "{key}");
        }
        assert!(!is_storage_marker("hidden_size") && !is_storage_marker("depth_alpha_enabled"));
        assert!(never_read(Some("Qwen2ForCausalLM"), "qwen2", &k(&["quantization"])).is_empty());
        assert!(!transformers_version().is_empty());
    }
}
