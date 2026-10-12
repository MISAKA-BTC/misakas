//! **Encoder-decoders as data** (`ENCDEC_FROM_SPEC_V1`, Phase 1 of `docs/design/palw/tir/frontend-as-data-v1.md` §3).
//!
//! What `lower/encdec.rs` hard-wired per family — the config reader for T5/mT5, BART, mBART, Marian and Pegasus, and
//! their tensor-name tables — is now an adapter of kind `encdec` (`adapters/{t5,bart,mbart,marian,pegasus}.json`
//! over the mixins `encdec-frame` and `mixin-bart-lineage`). The Rust reader (`parse_encdec`) remains as the ORACLE:
//! on every fixture and every real config, and on single-key mutants of them, an adapter reads the same
//! `EncDecSpec` — names included — or refuses where the Rust reader read it without looking (stricter, never looser).
//! The lowering and the float references are untouched, so everything `tests/encdec.rs` proves about the five
//! families holds for any family these files describe.

use misaka_palw_tir_lower::hf_schema::{AdapterChoice, AdapterSource, Level, ReadOptions, TensorIndex, read_encdec};
use misaka_palw_tir_lower::lower::encdec::{EncDecSpec, parse_encdec};
use misaka_palw_tir_lower::model::{FeatureStatus, ReportResult, analyze};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// The fixtures of the families an adapter reads and Rust never did: they are held by their own tests (`whisper.rs`, `encdec.rs`).
const DATA_ONLY: [&str; 3] = ["whisper", "t5_encoder", "longt5"];

/// Every encoder-decoder config the crate carries: the tiny fixtures (`hf-encdec/*`) and the published configs
/// (`configs/encdec/*.json`), as `(name, text)`.
fn configs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(tests_dir().join("fixtures/hf-encdec")).unwrap().flatten().map(|e| e.path()).collect();
    dirs.sort();
    for d in dirs.into_iter().filter(|d| d.join("config.json").exists()) {
        let n = d.file_name().unwrap().to_string_lossy().to_string();
        // Families written as data only have no Rust reader to be the oracle of (`whisper`, `t5-encoder`).
        if DATA_ONLY.contains(&n.as_str()) {
            continue;
        }
        out.push((format!("fixture {n}"), std::fs::read_to_string(d.join("config.json")).unwrap()));
    }
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(tests_dir().join("configs/encdec")).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
    files.sort();
    for p in files {
        let n = p.file_stem().unwrap().to_string_lossy().to_string();
        if n.starts_with("whisper") || n.contains("encoder") || n.starts_with("long-t5") {
            continue;
        }
        out.push((format!("real {n}"), std::fs::read_to_string(&p).unwrap()));
    }
    assert!(out.len() >= 12, "{} encoder-decoder configs", out.len());
    out
}

fn by_adapter(cfg: &Value) -> Result<(EncDecSpec, AdapterSource), String> {
    read_encdec(cfg, &ReadOptions::default()).map(|r| (r.spec, r.adapter)).map_err(|f| f.error.to_string())
}

/// The built-in adapter that must claim each architecture.
fn expected_adapter(arch: &str) -> &'static str {
    match arch {
        "T5ForConditionalGeneration" | "MT5ForConditionalGeneration" => "t5",
        "BartForConditionalGeneration" => "bart",
        "MBartForConditionalGeneration" => "mbart",
        "MarianMTModel" => "marian",
        "PegasusForConditionalGeneration" => "pegasus",
        other => panic!("no adapter expected for {other}"),
    }
}

fn arch_of(cfg: &Value) -> String {
    cfg["architectures"][0].as_str().unwrap().to_string()
}

#[test]
fn every_family_adapter_reads_what_the_rust_reader_read() {
    let mut seen = BTreeSet::new();
    for (name, text) in configs() {
        let cfg: Value = serde_json::from_str(&text).unwrap();
        let rust = parse_encdec(&text).unwrap_or_else(|e| panic!("{name}: the Rust reader: {e}"));
        let (spec, source) = by_adapter(&cfg).unwrap_or_else(|e| panic!("{name}: the adapter: {e}"));
        assert_eq!(spec, rust, "{name}: the adapter and the Rust reader disagree");
        let want = expected_adapter(&arch_of(&cfg));
        assert!(matches!(&source, AdapterSource::BuiltIn { id, .. } if id == want), "{name}: {source:?}");
        seen.insert(want);
    }
    assert_eq!(seen.len(), 5, "every family of the pack is exercised: {seen:?}");
}

#[test]
fn the_t5_adapter_reads_what_the_rust_reader_read() {
    // T5 v1.0 (ReLU, tied and scaled) and v1.1 (gated GELU, untied, unscaled): the gate/up names differ with `gated`.
    for name in ["t5", "t5_gated"] {
        let text = std::fs::read_to_string(tests_dir().join("fixtures/hf-encdec").join(name).join("config.json")).unwrap();
        let cfg: Value = serde_json::from_str(&text).unwrap();
        let (spec, _) = by_adapter(&cfg).unwrap();
        assert_eq!(spec, parse_encdec(&text).unwrap(), "{name}");
        assert_eq!(spec.gated, name == "t5_gated");
        assert_eq!(spec.names.enc.ffn_gate.is_some(), spec.gated, "{name}");
        assert_eq!(spec.names.enc.ffn_up, if spec.gated { "layer.1.DenseReluDense.wi_1" } else { "layer.1.DenseReluDense.wi" });
        assert_eq!(spec.names.dec.ffn_down, "layer.2.DenseReluDense.wo");
        assert!((spec.head_scale == 1.0) == (name == "t5_gated"), "{name}: v1.1 does not scale the decoder output");
    }
    // A published config with most keys missing (t5-small has no feed_forward_proj, no tie_word_embeddings, no
    // relative_attention_max_distance): the class defaults are assumed and reported.
    let text = std::fs::read_to_string(tests_dir().join("configs/encdec/t5-small.json")).unwrap();
    let cfg: Value = serde_json::from_str(&text).unwrap();
    let r = read_encdec(&cfg, &ReadOptions::default()).unwrap();
    assert_eq!(r.spec, parse_encdec(&text).unwrap());
    assert!(r.assumed_defaults.iter().any(|k| k == "feed_forward_proj") && r.assumed_defaults.iter().any(|k| k == "relative_attention_max_distance"), "{:?}", r.assumed_defaults);
}

/// What the two readers said about one configuration.
enum Outcome {
    Same,
    /// The adapter refuses where the Rust reader read the configuration without looking (a value no Hugging Face
    /// class runs: a type that is no number, a head count that does not divide the width, …).
    Stricter,
    Differ(String),
}

fn outcome(name: &str, text: &str, cfg: &Value) -> Outcome {
    let rust = catch_unwind(AssertUnwindSafe(|| parse_encdec(text)));
    let adapter = match catch_unwind(AssertUnwindSafe(|| by_adapter(cfg))) {
        Ok(a) => a,
        Err(_) => return Outcome::Differ(format!("{name}: the adapter PANICS")),
    };
    match (rust, adapter) {
        (Ok(Ok(r)), Ok((a, _))) if r == a => Outcome::Same,
        (Ok(Ok(r)), Ok((a, _))) => Outcome::Differ(format!("{name}: read differently\n  rust    {r:?}\n  adapter {a:?}")),
        // Both refuse.
        (Ok(Err(_)), Err(_)) => Outcome::Same,
        // The adapter refuses where the Rust reader read (or panicked on) the value: stricter.
        (Ok(Ok(_)), Err(_)) | (Err(_), Err(_)) | (Err(_), Ok(_)) => Outcome::Stricter,
        (Ok(Err(e)), Ok(_)) => Outcome::Differ(format!("{name}: the adapter reads what the Rust reader refused ({e})")),
    }
}

/// Every key of every config deleted, nulled, flipped, nudged or rewritten: the adapter and the Rust reader either
/// read the same spec or both refuse, and the adapter is never LOOSER. (It may be stricter: the Rust reader looked
/// only at the keys it needs and ignored the rest.)
#[test]
fn single_key_mutants_agree_or_the_adapter_is_stricter() {
    let variants: Vec<(&str, Value)> = vec![
        ("null", Value::Null),
        ("zero", json!(0)),
        ("one", json!(1)),
        ("two", json!(2)),
        ("minus one", json!(-1)),
        ("half", json!(0.5)),
        ("big", json!(1_000_000_000_000u64)),
        ("true", json!(true)),
        ("false", json!(false)),
        ("text", json!("x")),
        ("empty list", json!([])),
        ("gated", json!("gated-gelu")),
        ("relu", json!("relu")),
    ];
    let (mut same, mut stricter, mut bad) = (0usize, 0usize, Vec::new());
    for (name, text) in configs() {
        let cfg: Value = serde_json::from_str(&text).unwrap();
        let keys: Vec<String> = cfg.as_object().unwrap().keys().cloned().collect();
        for k in &keys {
            // Deleting the key.
            let mut muts: Vec<(String, Value)> = Vec::new();
            let mut del = cfg.clone();
            del.as_object_mut().unwrap().remove(k);
            muts.push((format!("{name}: delete {k}"), del));
            for (vn, v) in &variants {
                let mut m = cfg.clone();
                m[k.as_str()] = v.clone();
                muts.push((format!("{name}: {k} = {vn}"), m));
            }
            for (label, m) in muts {
                match outcome(&label, &m.to_string(), &m) {
                    Outcome::Same => same += 1,
                    Outcome::Stricter => stricter += 1,
                    Outcome::Differ(why) => bad.push(why),
                }
            }
        }
    }
    eprintln!("{same} mutants read the same, {stricter} the adapter refuses where the Rust reader did not");
    assert!(bad.is_empty(), "{} mutants on which the adapter and the Rust reader disagree:\n{}", bad.len(), bad.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
    assert!(same > 500, "{same}");
}

#[test]
fn an_unknown_key_is_refused_never_ignored() {
    // The Rust reader looked only at the keys it needs; an adapter accounts for every key (a key nobody
    // accounts for might change the math).
    let text = std::fs::read_to_string(tests_dir().join("fixtures/hf-encdec/bart/config.json")).unwrap();
    let mut cfg: Value = serde_json::from_str(&text).unwrap();
    cfg["a_key_nobody_has_heard_of"] = json!(3);
    let e = by_adapter(&cfg).unwrap_err();
    assert!(e.contains("a_key_nobody_has_heard_of"), "{e}");
    assert!(parse_encdec(&cfg.to_string()).is_ok(), "the Rust reader never looked");
}

#[test]
fn marian_refuses_what_the_rust_reader_refused() {
    let text = std::fs::read_to_string(tests_dir().join("fixtures/hf-encdec/marian/config.json")).unwrap();
    let mut cfg: Value = serde_json::from_str(&text).unwrap();
    cfg["share_encoder_decoder_embeddings"] = json!(false);
    assert!(by_adapter(&cfg).unwrap_err().contains("separate encoder and decoder embeddings"));
    let mut cfg: Value = serde_json::from_str(&text).unwrap();
    cfg["decoder_vocab_size"] = json!(65);
    assert!(by_adapter(&cfg).unwrap_err().contains("decoder vocabulary"));
}

/// An adapter the CALLER writes is read like a built-in one: a new family whose tensors are laid out like BART's but
/// whose positions are sinusoidal and whose norms are pre-norm needs no Rust — here, Pegasus rewritten from
/// the BART lineage mixin in six lines.
#[test]
fn a_user_adapter_adds_an_encoder_decoder_family_with_data_only() {
    let text = std::fs::read_to_string(tests_dir().join("fixtures/hf-encdec/pegasus/config.json")).unwrap();
    let cfg: Value = serde_json::from_str(&text).unwrap();
    let adapter = json!({
        "format": "misaka.palw.model-adapter.v1", "id": "my-pegasus", "kind": "encdec", "extends": ["mixin-bart-lineage"],
        "match": {"architectures": ["PegasusForConditionalGeneration"]},
        "vars": [{"name": "learned_positions", "value": false}, {"name": "pre_norm_stack", "value": true}],
    })
    .to_string();
    let r = read_encdec(&cfg, &ReadOptions { adapter: AdapterChoice::Text(adapter) }).unwrap();
    assert!(matches!(&r.adapter, AdapterSource::UserFile { id, .. } if id == "my-pegasus"), "{:?}", r.adapter);
    assert_eq!(r.spec, parse_encdec(&text).unwrap());
    // `--adapter none`: an encoder-decoder has no standard template.
    assert!(read_encdec(&cfg, &ReadOptions { adapter: AdapterChoice::None }).unwrap_err().error.to_string().contains("no standard template"));
    // A decoder adapter is not an encoder-decoder adapter.
    let e = read_encdec(&cfg, &ReadOptions { adapter: AdapterChoice::BuiltIn("llama".into()) }).unwrap_err();
    assert!(e.error.to_string().contains("not `encdec`"), "{}", e.error);
}

/// `check-architecture` on an encoder-decoder: Level B, the features listed, the tensors of the checkpoint all
/// accounted for by one of the two stages, nothing new asked of the protocol.
#[test]
fn the_report_for_an_encoder_decoder_is_level_b_with_its_features() {
    for name in ["t5", "t5_gated", "bart", "mbart", "marian", "pegasus"] {
        let dir = tests_dir().join("fixtures/hf-encdec").join(name);
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
        let tensors = TensorIndex::from_checkpoint_path(&dir).expect("tensor names");
        let r = analyze(&cfg, Some(&tensors), &ReadOptions::default());
        assert_eq!(r.result, ReportResult::Lowerable, "{name}: {}", r.render());
        assert_eq!(r.level, Level::B, "{name}");
        assert!(r.unread_tensors.is_empty() && r.weight_errors.is_empty(), "{name}: {:?} {:?}", r.unread_tensors, r.weight_errors);
        assert!(!r.new_consensus_primitive_required && !r.new_court_kernel_required);
        let ids: Vec<&str> = r.features.iter().map(|f| f.id.as_str()).collect();
        for want in ["ENCDEC_FROM_SPEC_V1", "ATTN_CROSS_ENCDEC_V1", "EMBED_TOKEN_V1", "OUTPUT_LOGITS_V1"] {
            assert!(ids.contains(&want), "{name}: {want} missing from {ids:?}");
        }
        assert!(r.features.iter().all(|f| f.status == FeatureStatus::Supported), "{name}: {:#?}", r.features);
        let text = r.render();
        assert!(text.contains("level           B") && text.contains("No new consensus primitive required"), "{name}: {text}");
    }
    // The positions are named by what they are.
    let ids_of = |name: &str| -> Vec<String> {
        let dir = tests_dir().join("fixtures/hf-encdec").join(name);
        let cfg: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
        analyze(&cfg, None, &ReadOptions::default()).features.into_iter().map(|f| f.id).collect()
    };
    assert!(ids_of("t5").contains(&"POS_RELATIVE_BIAS_V1".to_string()));
    assert!(ids_of("bart").contains(&"EMBED_POSITION_LEARNED_V1".to_string()));
    assert!(ids_of("marian").contains(&"POS_SINUSOID_V1".to_string()));
    // A checkpoint tensor neither stage reads is refused (the BitNet check), an encoder-decoder or not.
    let dir = tests_dir().join("fixtures/hf-encdec/bart");
    let cfg: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
    let mut tensors = TensorIndex::from_checkpoint_path(&dir).unwrap();
    tensors
        .tensors
        .insert("model.decoder.layers.0.self_attn.rotary_gain.weight".into(), misaka_palw_tir_lower::hf_schema::TensorEntry { dtype: "BF16".into(), shape: Some(vec![4]) });
    let r = analyze(&cfg, Some(&tensors), &ReadOptions::default());
    assert_eq!(r.level, Level::C, "{}", r.render());
    assert_eq!(r.unread_tensors, vec!["model.decoder.layers.0.self_attn.rotary_gain.weight".to_string()]);
}

#[test]
fn an_encoder_decoder_no_adapter_claims_is_level_c_and_says_so() {
    let mut cfg = json!({"architectures": ["SomeNewForConditionalGeneration"], "model_type": "some_new", "is_encoder_decoder": true, "vocab_size": 8});
    let r = analyze(&cfg, None, &ReadOptions::default());
    assert_eq!(r.level, Level::C);
    assert!(matches!(r.result, ReportResult::NotLowerable { .. }), "{:?}", r.result);
    assert!(r.missing.iter().any(|m| m.what.contains("SomeNewForConditionalGeneration")), "{:?}", r.missing);
    // An older hub config of T5 does not carry `is_encoder_decoder`: the architecture is what routes it.
    cfg = serde_json::from_str(&std::fs::read_to_string(tests_dir().join("configs/encdec/t5-small.json")).unwrap()).unwrap();
    cfg.as_object_mut().unwrap().remove("is_encoder_decoder");
    assert!(misaka_palw_tir_lower::hf_schema::is_encoder_decoder(&cfg));
    assert_eq!(analyze(&cfg, None, &ReadOptions::default()).level, Level::B);
}
