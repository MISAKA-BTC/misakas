//! **A checkpoint that spells a LayerNorm's gain and bias `gamma` / `beta` is read as transformers reads it.**
//!
//! BERT-lineage checkpoints exported before the rename (and many TF conversions still on the Hub) store `…LayerNorm.gamma` and
//! `…LayerNorm.beta`. transformers renames them on load for every model (its `legacy` checkpoint conversion, `LayerNorm.gamma →
//! LayerNorm.weight`, `LayerNorm.beta → LayerNorm.bias`); the binder asked for `LayerNorm.weight` only and refused the checkpoint as
//! `TENSOR_MISSING embed.norm.gain`.
//!
//! The test rewrites a real fixture's safetensors header with the old spellings (the data is untouched) and requires
//! * every parameter binds and every tensor of the checkpoint is read (`check_weights`),
//! * the float reference gives **the same bytes** as the original spelling,
//! * a name the rule does not cover (`LayerNorm.weights`, a prefix) is not renamed.

use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::lower::bidir::{self, BidirCfg, Padded, Pooling};
use misaka_palw_tir_lower::weights::{Checkpoint, check_weights, legacy_layer_norm_name};
use std::path::{Path, PathBuf};

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-enc").join(name)
}

/// A copy of `src` (`config.json`, `model.safetensors`) whose LayerNorm tensors carry the pre-rename names.
fn legacy_copy(src: &Path, tag: &str) -> (PathBuf, usize) {
    let dst = std::env::temp_dir().join(format!("tir-legacy-ln-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dst).unwrap();
    std::fs::copy(src.join("config.json"), dst.join("config.json")).unwrap();
    let bytes = std::fs::read(src.join("model.safetensors")).unwrap();
    let n = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
    let header: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&bytes[8..8 + n]).unwrap();
    let mut renamed = serde_json::Map::new();
    let mut count = 0;
    for (k, v) in header {
        let new = k
            .strip_suffix("LayerNorm.weight")
            .map(|p| format!("{p}LayerNorm.gamma"))
            .or_else(|| k.strip_suffix("LayerNorm.bias").map(|p| format!("{p}LayerNorm.beta")));
        if new.is_some() {
            count += 1;
        }
        renamed.insert(new.unwrap_or(k), v);
    }
    let mut h = serde_json::to_vec(&renamed).unwrap();
    while h.len() % 8 != 0 {
        h.push(b' ');
    }
    let mut out = (h.len() as u64).to_le_bytes().to_vec();
    out.extend(&h);
    out.extend(&bytes[8 + n..]);
    std::fs::write(dst.join("model.safetensors"), out).unwrap();
    (dst, count)
}

fn forward(dir: &Path) -> (Vec<f64>, usize, usize) {
    let cfg_text = std::fs::read_to_string(dir.join("config.json")).unwrap();
    let spec = misaka_palw_tir_lower::parse_config_str(&cfg_text).unwrap();
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).unwrap();
    let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).unwrap();
    let ck = Checkpoint::open(dir).unwrap();
    let rep = check_weights(&hl, &binding, &ck);
    let (params, _) = ParamStore::from_source(&hl, &binding, &ck).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let ids = vec![2usize, 5, 9, 11, 3, 0, 0, 0, 0, 0, 0, 0];
    let out = bidir::float_forward(
        &hl,
        &spec,
        &BidirCfg { lmax: 12, pooling: Pooling::Mean, normalize: true },
        &params,
        &Padded { ids, count: 5 },
        None,
    )
    .unwrap();
    (out, rep.errors.len(), rep.unused.len())
}

#[test]
fn a_gamma_beta_checkpoint_binds_and_computes_the_same_bytes_as_its_weight_bias_twin() {
    let src = fixture_dir("bert");
    let (legacy, renamed) = legacy_copy(&src, "bert");
    assert!(renamed >= 6, "the fixture has LayerNorms to rename: {renamed}");
    // The original spelling binds; the legacy one does not without the rule — and does with it.
    let (want, errs, unused) = forward(&src);
    assert_eq!((errs, unused), (0, 0));
    let (got, errs, unused) = forward(&legacy);
    assert_eq!((errs, unused), (0, 0), "every tensor of the legacy checkpoint is read");
    assert_eq!(got, want, "gamma/beta are weight/bias: the same bytes");
    let _ = std::fs::remove_dir_all(legacy);
}

#[test]
fn the_rule_renames_a_layer_norm_leaf_and_nothing_else() {
    assert_eq!(legacy_layer_norm_name("embeddings.LayerNorm.weight").as_deref(), Some("embeddings.LayerNorm.gamma"));
    assert_eq!(legacy_layer_norm_name("encoder.layer.0.output.LayerNorm.bias").as_deref(), Some("encoder.layer.0.output.LayerNorm.beta"));
    for not in ["embeddings.LayerNorm.weights", "LayerNorm.weight.x", "layer_norm.weight", "embeddings.norm.weight", "weight", ""] {
        assert_eq!(legacy_layer_norm_name(not), None, "{not}");
    }
}
