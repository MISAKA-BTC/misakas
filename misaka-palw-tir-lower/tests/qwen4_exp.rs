//! **Qwen4-Exp as a combination of generic features** (RFC-0002 lane G, the genericity acceptance
//! test). The fixtures are tiny, randomly initialised `Qwen4ExpForCausalLM` checkpoints from
//! transformers 5.17 (`tools/gen_qwen4_fixtures.py`) — never the published weights. Nothing in the
//! lowering knows the name `qwen4`: the model is read by a data adapter (`adapters/qwen4-exp.json`)
//! into gated residuals (hyper-connections), sparse block attention, gated delta layers at a
//! key:value head ratio, hashed n-gram per-layer embeddings and a routed MoE, each a feature of the
//! finite vocabulary (`model::REGISTRY`).
//!
//! Named tests (the acceptance matrix): GR-01..03, PLE-01..05, QSA-01..06, GDN-01..04.

use misaka_palw_tir_lower::hf_schema::{AdapterChoice, AdapterSource, ReadOptions, TensorIndex, read_model};
use misaka_palw_tir_lower::model::REGISTRY;
use misaka_palw_tir_lower::ngram::{NgramTables, ids_batch};
use misaka_palw_tir_lower::spec::{Mixer, ModelSpec, Residual};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-q4")
}

fn fixture(name: &str) -> PathBuf {
    root().join(name)
}

fn json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("json")
}

/// The spec the generic reader makes of a fixture, with the checkpoint's tensor names.
fn spec_of(name: &str) -> ModelSpec {
    let dir = fixture(name);
    let cfg = json(&dir.join("config.json"));
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    let r = read_model(&cfg, Some(&tensors), &ReadOptions::default()).unwrap_or_else(|f| panic!("{name}: {}", f.error));
    assert!(matches!(&r.adapter, AdapterSource::BuiltIn { id, .. } if id == "qwen4-exp"), "{name}: read by {:?}", r.adapter);
    r.spec
}

fn names() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root()).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    v.sort();
    v
}

#[test]
fn the_adapter_reads_every_fixture_into_generic_features() {
    for n in names() {
        let s = spec_of(&n);
        let h = s.hyper.as_ref().unwrap_or_else(|| panic!("{n}: no hyper-connection streams"));
        assert!(h.streams >= 1, "{n}");
        // every layer is a gated residual over those streams; the mixer is a delta net or sparse attention
        for (i, l) in s.layers.iter().enumerate() {
            assert!(matches!(l.residual, Residual::HyperConnection { .. }), "{n} layer {i}");
            match &l.mixer {
                Mixer::GatedDeltaNet(_) => {}
                Mixer::Attention(a) => assert!(a.sparse.is_some() && a.output_gate, "{n} layer {i}: sparse attention with a gated output"),
                m => panic!("{n} layer {i}: {m:?}"),
            }
        }
        // nothing outside the vocabulary: every feature the spec uses is registered, and none needs a protocol change
        for u in s.features() {
            let f = REGISTRY.iter().find(|f| f.id == u.id).unwrap_or_else(|| panic!("{n}: {} is not in the registry", u.id));
            assert!(matches!(f.protocol, misaka_palw_tir_lower::model::Requirement::None), "{n}: {} needs {:?}", u.id, f.protocol);
        }
    }
}

#[test]
fn the_ngram_ids_are_the_transformers_ids() {
    let mut checked = 0;
    for n in names() {
        let p = fixture(&n).join("ngram_ids.json");
        if !p.exists() {
            continue;
        }
        let s = spec_of(&n);
        let v = json(&p);
        let tokens: Vec<i64> = v["tokens"].as_array().expect("tokens").iter().map(|x| x.as_i64().expect("int")).collect();
        let ple: Vec<_> = s
            .layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| match &l.residual {
                Residual::HyperConnection { ple: Some(p) } => Some((i, p.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(v["ids"].as_object().expect("ids").len(), ple.len(), "{n}: PLE layers");
        for (layer, spec) in ple {
            let want: Vec<Vec<i64>> = v["ids"][layer.to_string()]
                .as_array()
                .expect("layer ids")
                .iter()
                .map(|row| row.as_array().expect("row").iter().map(|x| x.as_i64().expect("int")).collect())
                .collect();
            let t = NgramTables::new(&spec);
            assert_eq!(ids_batch(&t, &tokens), want, "{n} layer {layer}");
            checked += 1;
        }
    }
    assert!(checked >= 8, "{checked} PLE layers checked against the HF ids");
}

#[test]
fn a_config_with_no_adapter_choice_is_read_without_the_model_type() {
    // The adapter claims the class by its architecture; the model_type is informational.
    let dir = fixture("qwen4_exp");
    let mut cfg = json(&dir.join("config.json"));
    cfg["model_type"] = Value::String("something_else_entirely".into());
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    let r = read_model(&cfg, Some(&tensors), &ReadOptions { adapter: AdapterChoice::Auto }).expect("read");
    assert_eq!(r.spec.layers.len(), 4);
}
