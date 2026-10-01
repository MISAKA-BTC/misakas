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

/// A Level A reading is a guess about a class no adapter has claimed: rope pairing, norm placement and MLP gating
/// are class code, not configuration (FR-26). The label says so until a reference check has passed.
#[test]
fn level_a_is_labelled_unconfirmed_until_a_reference_check_passes() {
    let dir = root().join("tests/fixtures/hf/llama");
    let mut cfg = json(&dir.join("config.json"));
    let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    cfg["architectures"] = serde_json::json!(["SomeBrandNewForCausalLM"]);
    let r = analyze(&cfg, Some(&tensors), &ReadOptions::default());
    assert_eq!(r.level, Level::A, "{}", r.render());
    assert_eq!(r.level_label, "A (unconfirmed)");
    let text = r.render();
    assert!(text.contains("level           A (unconfirmed)") && text.contains("class code, not configuration"), "{text}");
    assert!(text.contains("LOWERABLE (Level A (unconfirmed))"), "{text}");
    let v = serde_json::to_value(&r).expect("json");
    assert_eq!((v["level"].as_str(), v["level_label"].as_str(), v["reference_confirmed"].as_bool()), (Some("A"), Some("A (unconfirmed)"), Some(false)));
    // A reference check on the same weights lifts the label; no other level was ever unconfirmed.
    let c = r.confirm_reference();
    assert_eq!(c.level_label, "A");
    assert!(!c.render().contains("unconfirmed"), "{}", c.render());
    let b = analyze(&json(&dir.join("config.json")), Some(&tensors), &ReadOptions::default());
    assert_eq!((b.level, b.level_label.as_str()), (Level::B, "B"));
}

/// The tensor index, when given, is run through the weight binding: a tensor no param reads is a feature the
/// reading does not know (this is the check that caught Arcee's gated MLP and BitNet's sub-norms), and a shape
/// the graph does not accept is refused with the numbers (FR-26). Neither is "lowerable".
#[test]
fn an_unread_tensor_or_a_wrong_shape_in_the_index_refuses_the_model() {
    use misaka_palw_tir_lower::hf_schema::TensorEntry;
    let dir = root().join("tests/fixtures/hf/llama");
    let cfg = json(&dir.join("config.json"));
    let clean = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
    let ok = analyze(&cfg, Some(&clean), &ReadOptions::default());
    assert_eq!(ok.result, ReportResult::Lowerable, "{}", ok.render());
    assert!(ok.unread_tensors.is_empty() && ok.weight_errors.is_empty());
    // BitNet's two sub-norm tensors next to the llama ones.
    let mut extra = clean.clone();
    let name = "model.layers.0.self_attn.attn_sub_norm.weight";
    extra.tensors.insert(name.to_string(), TensorEntry { dtype: "BF16".into(), shape: Some(vec![32]) });
    for adapter in [AdapterChoice::Auto, AdapterChoice::None] {
        let r = analyze(&cfg, Some(&extra), &ReadOptions { adapter });
        assert_eq!(r.level, Level::C, "{}", r.render());
        assert_eq!(r.unread_tensors, vec![name.to_string()]);
        assert!(matches!(r.result, ReportResult::NotLowerable { .. }), "{:?}", r.result);
        let text = r.render();
        assert!(text.contains(&format!("unread tensor: {name}")) && text.contains("NOT_LOWERABLE"), "{text}");
    }
    // A projection of the wrong shape.
    let mut wrong = clean.clone();
    let q = "model.layers.0.self_attn.q_proj.weight";
    let want = wrong.shape(q).expect("the fixture has it").to_vec();
    wrong.tensors.get_mut(q).unwrap().shape = Some(vec![want[0] - 1, want[1]]);
    let r = analyze(&cfg, Some(&wrong), &ReadOptions::default());
    assert_eq!(r.level, Level::C);
    assert!(r.weight_errors.iter().any(|e| e.contains("q.w") && e.contains("graph needs")), "{:?}", r.weight_errors);
    // Names only (an index file, no headers): a missing tensor is still found.
    let names = TensorIndex::from_names(clean.names().filter(|n| *n != "model.norm.weight").map(str::to_string));
    let r = analyze(&cfg, Some(&names), &ReadOptions::default());
    assert!(r.weight_errors.iter().any(|e| e.contains("missing tensor")), "{:?}", r.weight_errors);
}

/// FR-25: an architecture the build refuses on purpose (`adapters/refusals.json`) can be read anyway by an
/// adapter the CALLER supplies, which passes the same validation as any adapter; the report says so.
#[test]
fn a_user_adapter_may_override_a_built_in_refusal_and_the_report_says_so() {
    let dir = root().join("tests/fixtures/fr01/granitemoehybrid");
    let cfg = json(&dir.join("config.json"));
    let index: Value = json(&dir.join("tensors.json"));
    let tensors = TensorIndex::from_shapes(index.as_object().unwrap().iter().map(|(n, e)| {
        (n.clone(), e["shape"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as usize).collect::<Vec<usize>>())
    }));
    // By default the class is refused, by the feature it lacks.
    let by_default = analyze(&cfg, Some(&tensors), &ReadOptions::default());
    assert!(matches!(by_default.result, ReportResult::NotLowerable { .. }) && by_default.overrides_refusal.is_none(), "{}", by_default.render());
    assert!(by_default.missing.iter().any(|m| m.what == "MIXER_LAYER_PATTERN_HYBRID_V1"), "{:?}", by_default.missing);
    // A built-in adapter chosen by id does not override a refusal either: the data must not contradict itself.
    let forced = analyze(&cfg, Some(&tensors), &ReadOptions { adapter: AdapterChoice::BuiltIn("granitemoe".into()) });
    assert!(forced.overrides_refusal.is_none() && matches!(forced.result, ReportResult::NotLowerable { .. }));
    // The adapter the caller wrote overrides it, with the same validation as any adapter, and the report says so.
    let text = std::fs::read_to_string(dir.join("adapter.json")).unwrap();
    let r = analyze(&cfg, Some(&tensors), &ReadOptions { adapter: AdapterChoice::Text(text) });
    assert_eq!(r.result, ReportResult::Lowerable, "{}", r.render());
    assert_eq!(r.level, Level::B);
    let why = r.overrides_refusal.as_deref().expect("the override is recorded");
    assert!(why.contains("GraniteMoeHybridForCausalLM") && why.contains("not modelled"), "{why}");
    let out = r.render();
    assert!(out.contains("user adapter overrides built-in refusal: GraniteMoeHybridForCausalLM"), "{out}");
    assert!(serde_json::to_value(&r).unwrap()["overrides_refusal"].is_string());
    // The override cannot launder a feature the vocabulary lacks: Gemma-3n's AltUp has no field to say it in.
    let g = json(&root().join("tests/configs/real/gemma-3n-e4b.json"));
    let std_like = serde_json::json!({
        "format": "misaka.palw.model-adapter.v1", "id": "gemma3n-wishful", "extends": ["standard-decoder"],
    })
    .to_string();
    let refused = analyze(&g, None, &ReadOptions { adapter: AdapterChoice::Text(std_like) });
    assert!(matches!(refused.result, ReportResult::NotLowerable { .. }), "{}", refused.render());
}
