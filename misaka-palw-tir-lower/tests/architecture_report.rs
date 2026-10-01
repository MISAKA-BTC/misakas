//! **What `palw-class check-architecture` prints** (the feature report, `model::analyze`): the
//! model_type is informational, the features are each `SUPPORTED` or `MISSING` with the capability
//! that would close the gap, the support level says how the model was read (A: the standard keys,
//! B: a data adapter, C: a capability is missing), the adapter used is named (built-in file, a file
//! the caller supplied, none) and the protocol is asked for nothing new unless a feature needs it.
//! A model added with DATA only — an adapter file — goes through the same report and the same
//! lowering as a built-in one.

use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::hf_schema::{AdapterChoice, AdapterSource, Level, ReadOptions, TensorIndex};
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::model::{FeatureStatus, ReportResult, analyze};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).expect("json")
}

#[test]
fn qwen4_exp_is_level_b_with_every_feature_supported_and_nothing_new_required() {
    let dir = root().join("tests/fixtures/hf/qwen4_exp");
    let cfg = json(&dir.join("config.json"));
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    let r = analyze(&cfg, Some(&tensors), &ReadOptions::default());
    assert_eq!(r.level, Level::B);
    assert!(matches!(&r.adapter, AdapterSource::BuiltIn { id, .. } if id == "qwen4-exp"), "{:?}", r.adapter);
    assert_eq!(r.result, ReportResult::Lowerable);
    assert!(!r.new_consensus_primitive_required && !r.new_court_kernel_required);
    let ids: Vec<&str> = r.features.iter().map(|f| f.id.as_str()).collect();
    for want in ["RESIDUAL_GATED_HC_V1", "ATTN_SPARSE_BLOCK_V1", "EMBED_NGRAM_PLE_V1", "CONV_DEPTHWISE_CAUSAL_V1", "MIXER_GDN_V1"] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
    assert!(r.features.iter().all(|f| f.status == FeatureStatus::Supported), "{:#?}", r.features);
    let text = r.render();
    assert!(text.contains("informational only") && text.contains("level           B") && text.contains("built-in data file `qwen4-exp`"), "{text}");
    assert!(text.contains("No new consensus primitive required") && text.contains("No new court kernel required"), "{text}");
    assert!(text.contains("SUPPORTED") && !text.contains("MISSING"), "{text}");
    eprintln!("{text}");
    // the JSON a wrapper reads
    let v = serde_json::to_value(&r).expect("json");
    assert_eq!(v["level"], "B");
    assert_eq!(v["new_consensus_primitive_required"], false);
    assert_eq!(v["schema"], "misaka.palw.architecture-report.v1");
}

#[test]
fn the_model_type_selects_nothing() {
    // The adapter claims the class by its architecture; renaming the model_type changes the report's first line only.
    let dir = root().join("tests/fixtures/hf/qwen4_exp");
    let mut cfg = json(&dir.join("config.json"));
    let a = analyze(&cfg, None, &ReadOptions::default());
    cfg["model_type"] = Value::String("a_name_nobody_registered".into());
    let b = analyze(&cfg, None, &ReadOptions::default());
    assert_eq!((a.level, &a.adapter, &a.features, &a.result), (b.level, &b.adapter, &b.features, &b.result));
    assert_eq!(b.model_type.as_deref(), Some("a_name_nobody_registered"));
}

#[test]
fn a_standard_decoder_is_level_a_with_no_adapter_and_a_known_family_is_level_b() {
    let dir = root().join("tests/fixtures/hf/llama");
    let mut cfg = json(&dir.join("config.json"));
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    // Llama is a family the pack has an adapter for: Level B, the built-in file named.
    let known = analyze(&cfg, Some(&tensors), &ReadOptions::default());
    assert_eq!(known.level, Level::B);
    assert!(matches!(&known.adapter, AdapterSource::BuiltIn { id, .. } if id == "llama"), "{:?}", known.adapter);
    // A decoder class nobody has an adapter for, whose keys and tensor names are the standard ones: Level A.
    cfg["architectures"] = serde_json::json!(["SomeBrandNewForCausalLM"]);
    cfg["model_type"] = Value::String("some_brand_new".into());
    let r = analyze(&cfg, Some(&tensors), &ReadOptions::default());
    assert_eq!(r.level, Level::A, "{}", r.render());
    assert_eq!(r.adapter, AdapterSource::None);
    assert_eq!(r.result, ReportResult::Lowerable);
    assert!(r.render().contains("none (Level A"), "{}", r.render());
}

#[test]
fn a_model_the_vocabulary_lacks_features_for_is_level_c_and_says_what_is_missing() {
    let cfg = json(&root().join("tests/configs/real/gemma-3n-e4b.json"));
    let r = analyze(&cfg, None, &ReadOptions::default());
    assert_eq!(r.level, Level::C);
    assert!(matches!(r.result, ReportResult::NotLowerable { .. }), "{:?}", r.result);
    assert!(!r.missing.is_empty() || !r.features.is_empty(), "a Level C report names what it lacks");
    let text = r.render();
    assert!(text.contains("NOT_LOWERABLE") && text.contains("level           C"), "{text}");
}

#[test]
fn a_user_supplied_adapter_file_is_read_and_lowered_like_the_built_in_one() {
    // The same data, handed in as a file the caller wrote: the report names it as the user's, and the lowered
    // program is byte-for-byte the built-in adapter's.
    let dir = root().join("tests/fixtures/hf/qwen4_exp");
    let cfg_text = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let cfg: Value = serde_json::from_str(&cfg_text).expect("json");
    let file = root().join("adapters/qwen4-exp.json");
    let choice = AdapterChoice::parse_arg(file.to_str().expect("path")).expect("the adapter file is read");
    assert!(matches!(choice, AdapterChoice::Text(_)));
    let read = ReadOptions { adapter: choice };
    let r = analyze(&cfg, None, &read);
    assert!(matches!(&r.adapter, AdapterSource::UserFile { id, .. } if id == "qwen4-exp"), "{:?}", r.adapter);
    assert_eq!(r.level, Level::B);
    assert!(r.render().contains("user-supplied data file `qwen4-exp`"), "{}", r.render());
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let user = fidelity::prepare_read(&cfg_text, &read, &LowerOpts::default(), reg).expect("lowers through the user's file");
    let built = fidelity::prepare(&cfg_text, &LowerOpts::default()).expect("lowers through the built-in file");
    assert_eq!(user.lowered.program, built.lowered.program);
    // `builtin:<id>` and `none` are spellings too
    assert!(matches!(AdapterChoice::parse_arg("none"), Ok(AdapterChoice::None)));
    assert!(matches!(AdapterChoice::parse_arg("builtin:qwen4-exp"), Ok(AdapterChoice::BuiltIn(id)) if id == "qwen4-exp"));
    assert!(AdapterChoice::parse_arg("/no/such/adapter.json").is_err());
}

#[test]
fn without_an_adapter_a_class_the_standard_keys_do_not_cover_is_refused_not_guessed() {
    // `--adapter none` reads the standard keys only: Qwen4-Exp's hyper-connections are not among them.
    let cfg = json(&root().join("tests/fixtures/hf/qwen4_exp/config.json"));
    let r = analyze(&cfg, None, &ReadOptions { adapter: AdapterChoice::None });
    assert_eq!(r.level, Level::C);
    assert!(matches!(r.result, ReportResult::NotLowerable { .. }), "{:?}", r.result);
}
