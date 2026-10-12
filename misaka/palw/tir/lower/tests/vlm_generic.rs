//! **The text decoder inside a multimodal `…ForConditionalGeneration` wrapper, by ONE data-driven route** (`adapters/vlm-generic.json`;
//! COV-P1P2, 2026-10-08, routed from H1's SmolVLM `ARCH_NEEDS_FEATURE(adapter)`).
//!
//! Fixtures are REAL configurations and safetensors HEADERS (no weight byte: each shard file is cut after its JSON header), fetched by
//! HTTP range from pinned revisions — `tests/fixtures/vlm-generic/<repo>/fixture.json` records the repository, revision and header
//! digests. The class such a read registers is text-only (partial-task coverage of the repository), never the full VLM.
use misaka_palw_tir_lower::hf_schema::{AdapterChoice, AdapterSource, HeaderSource, ReadOptions, TensorIndex, read_model};
use misaka_palw_tir_lower::spec::{ArchSpec, Mixer};
use misaka_palw_tir_lower::{LowerError, hf_weights, hl, weights};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vlm-generic").join(name)
}

fn config(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir(name).join("config.json")).unwrap()).unwrap()
}

/// Names and shapes from every header-only shard of the fixture.
fn index(name: &str) -> TensorIndex {
    let mut all = Vec::new();
    for e in std::fs::read_dir(dir(name)).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("safetensors") {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let n = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
        let v: Value = serde_json::from_slice(&bytes[8..8 + n]).unwrap();
        for (k, e) in v.as_object().unwrap() {
            if k == "__metadata__" {
                continue;
            }
            let shape: Vec<usize> = e["shape"].as_array().unwrap().iter().map(|x| x.as_u64().unwrap() as usize).collect();
            all.push((k.clone(), shape));
        }
    }
    TensorIndex::from_shapes(all)
}

/// The read, the HL program, and every param bound against the HEADERS: `(spec, unread tensors)`.
fn read_and_bind(cfg: &Value, idx: &TensorIndex, opts: &ReadOptions) -> (ArchSpec, AdapterSource, Vec<String>) {
    let read = read_model(cfg, Some(idx), opts).unwrap_or_else(|f| panic!("read: {f} ({:?})", f.missing));
    let prog = hl::build_program(&read.spec).expect("an HL program");
    let binding = hf_weights::bind(&read.spec, &prog).expect("a binding");
    let rep = weights::check_weights(&prog, &binding, &HeaderSource(idx));
    assert!(rep.errors.is_empty(), "binding errors: {:?}", rep.errors.iter().take(6).collect::<Vec<_>>());
    assert!(rep.bound > 0);
    (read.spec, read.adapter, rep.unused)
}

#[test]
fn smolvlm_idefics3_text_decoder_reads_by_the_generic_route_and_binds_every_tensor() {
    let cfg = config("smolvlm-256m-instruct");
    let idx = index("smolvlm-256m-instruct");
    assert_eq!(cfg["architectures"][0], "Idefics3ForConditionalGeneration");
    let (spec, adapter, unused) = read_and_bind(&cfg, &idx, &ReadOptions::default());
    let AdapterSource::BuiltIn { id, .. } = &adapter else { panic!("a built-in route: {adapter:?}") };
    assert_eq!(id, "vlm-generic(vlm-llama)", "the text model_type `llama` picks the vlm dispatch table's entry");
    assert_eq!(spec.num_layers(), 30);
    let Mixer::Attention(a) = &spec.layers[0].mixer else { panic!("attention") };
    assert_eq!((a.heads, a.kv_heads, a.head_dim), (9, 3, 64));
    assert!(!spec.head.tied, "Idefics3's wrapper says tie_word_embeddings false and stores lm_head.weight");
    // Every text tensor bound; the vision tower (197 tensors) and the connector unread by design, nothing else unread.
    assert!(unused.is_empty(), "unread tensors outside the declared non-text components: {unused:?}");
    let text: usize = idx.names().filter(|n| n.starts_with("model.text_model.") || *n == "lm_head.weight").count();
    let vision: usize = idx.names().filter(|n| n.starts_with("model.vision_model.") || n.starts_with("model.connector.")).count();
    assert_eq!((text, vision, idx.names().count()), (273, 198, 471));
    // The names the decoder reads sit under the prefix the index holds, found — not named — by the route.
    assert!(
        spec.hf.names.values().any(|n| n.starts_with("model.text_model.layers.")),
        "{:?}",
        spec.hf.names.values().take(3).collect::<Vec<_>>()
    );
}

#[test]
fn the_route_is_keyed_by_structure_not_by_the_wrapper_name() {
    // The same configuration under a wrapper name no adapter has ever seen, its language model at another attribute path:
    // read identically but for the prefix.
    let mut cfg = config("smolvlm-256m-instruct");
    cfg["architectures"] = serde_json::json!(["NeverSeenForConditionalGeneration"]);
    cfg["model_type"] = serde_json::json!("never_seen");
    let idx = index("smolvlm-256m-instruct");
    let moved = TensorIndex::from_shapes(
        idx.tensors.iter().map(|(n, e)| (n.replace("model.text_model.", "model.lm_body."), e.shape.clone().unwrap())),
    );
    // A wrapper that is NOT a transformers class gets no "never read by the class" evidence: the training-lineage keys its text_config
    // carries (`perceiver_config`, `qk_layer_norms`, …) stay refused, by name — fail-closed, never waved through as a known decoder.
    let err = read_model(&cfg, Some(&moved), &ReadOptions::default()).expect_err("an unknown wrapper's unread keys are refused");
    let msg = err.error.to_string();
    let lineage = [
        "_flash_attn_2_enabled",
        "neftune_noise_alpha",
        "perceiver_config",
        "pixel_shuffle_factor",
        "qk_layer_norms",
        "use_resampler",
    ];
    for k in lineage {
        assert!(msg.contains(&format!("text_config.{k}")), "{k}: {msg}");
    }
    for k in lineage {
        cfg["text_config"].as_object_mut().unwrap().remove(k);
    }
    let (a, _, _) = read_and_bind(&config("smolvlm-256m-instruct"), &idx, &ReadOptions::default());
    let (b, src, unused) = read_and_bind(&cfg, &moved, &ReadOptions::default());
    assert!(matches!(&src, AdapterSource::BuiltIn { id, .. } if id.starts_with("vlm-generic(")));
    assert!(unused.is_empty(), "{unused:?}");
    assert_eq!(serde_json::to_value(&a.layers).unwrap(), serde_json::to_value(&b.layers).unwrap());
    let renamed: Vec<String> = a.hf.names.values().map(|n| n.replace("model.text_model.", "model.lm_body.")).collect();
    assert_eq!(renamed, b.hf.names.values().cloned().collect::<Vec<_>>());
}

#[test]
fn two_candidate_decoders_are_refused_by_name_never_guessed() {
    let cfg = config("smolvlm-256m-instruct");
    let idx = index("smolvlm-256m-instruct");
    let mut names: Vec<(String, Vec<usize>)> = idx.tensors.iter().map(|(n, e)| (n.clone(), e.shape.clone().unwrap())).collect();
    names.push(("model.draft_model.embed_tokens.weight".into(), vec![49_280, 576]));
    let two = TensorIndex::from_shapes(names);
    let err = read_model(&cfg, Some(&two), &ReadOptions::default()).expect_err("ambiguous decoder");
    let LowerError::NotLowerable(msg) = &err.error else { panic!("{:?}", err.error) };
    assert!(msg.contains("`model.draft_model.`") && msg.contains("`model.text_model.`") && msg.contains("not identified"), "{msg}");
}

#[test]
fn an_unstated_tie_with_a_head_present_is_refused_and_with_no_head_is_tied() {
    let mut cfg = config("smolvlm-256m-instruct");
    cfg.as_object_mut().unwrap().remove("tie_word_embeddings");
    let idx = index("smolvlm-256m-instruct");
    let err = read_model(&cfg, Some(&idx), &ReadOptions::default()).expect_err("the class default is not known here");
    assert!(err.error.to_string().contains("does not state tie_word_embeddings"), "{}", err.error);
    // No head in the index: the only consistent reading is a tied head.
    let headless = TensorIndex::from_shapes(
        idx.tensors.iter().filter(|(n, _)| n.as_str() != "lm_head.weight").map(|(n, e)| (n.clone(), e.shape.clone().unwrap())),
    );
    let read = read_model(&cfg, Some(&headless), &ReadOptions::default()).expect("tied");
    assert!(read.spec.head.tied);
}

#[test]
fn a_wrapper_whose_text_model_type_no_decoder_claims_is_refused_by_name() {
    let mut cfg = config("smolvlm-256m-instruct");
    cfg["text_config"]["model_type"] = serde_json::json!("vllama_unmodelled");
    let err = read_model(&cfg, Some(&index("smolvlm-256m-instruct")), &ReadOptions::default()).expect_err("no decoder adapter");
    assert!(err.error.to_string().contains("`text_config.model_type` = `vllama_unmodelled`"), "{}", err.error);
    assert!(err.missing.iter().any(|m| m.what.contains("Idefics3ForConditionalGeneration")));
}

#[test]
fn huihui_qwen35_9b_generic_route_equals_the_named_vlm_route() {
    // Qwen3_5ForConditionalGeneration is named by `vlm` (→ vlm-qwen3-5); the generic route, asked of the same wrapper, must read the
    // same decoder: the same layers, head and tensor names (only its alias and non-text lists differ).
    let cfg = config("huihui-qwen3.5-9b");
    let idx = index("huihui-qwen3.5-9b");
    let (named, src, unused) = read_and_bind(&cfg, &idx, &ReadOptions::default());
    assert!(matches!(&src, AdapterSource::BuiltIn { id, .. } if id == "vlm-qwen3-5"), "{src:?}");
    assert!(unused.is_empty(), "{unused:?}");
    let generic = misaka_palw_tir_lower::adapter::builtin::find_wrapper_for("Qwen3_5ForConditionalGeneration", &cfg)
        .expect("a wrapper")
        .expect("a decoder adapter");
    assert_eq!(generic.id, "vlm-generic(vlm-qwen3-5)");
    let text = serde_json::to_string(&generic.value).unwrap();
    let (g, _, g_unused) = read_and_bind(&cfg, &idx, &ReadOptions { adapter: AdapterChoice::Text(text) });
    assert!(g_unused.is_empty(), "{g_unused:?}");
    assert_eq!(serde_json::to_value(&named.layers).unwrap(), serde_json::to_value(&g.layers).unwrap());
    assert_eq!(serde_json::to_value(&named.head).unwrap(), serde_json::to_value(&g.head).unwrap());
    assert_eq!(named.hf.names, g.hf.names);
    assert_eq!(named.num_layers(), 32);
    let gdn = named.layers.iter().filter(|l| matches!(l.mixer, Mixer::GatedDeltaNet(_))).count();
    assert_eq!((gdn, named.num_layers() - gdn), (24, 8), "Qwen3.5-9B: 24 Gated DeltaNet layers, 8 full-attention layers");
}

#[test]
fn a_causal_lm_or_a_named_wrapper_never_takes_the_generic_route() {
    let idx = index("smolvlm-256m-instruct");
    assert!(misaka_palw_tir_lower::adapter::builtin::find_wrapper_for("LlamaForCausalLM", &config("smolvlm-256m-instruct")).is_none());
    // A wrapper with no nested text_config (a flat VLM config) is not one either.
    let mut flat = config("smolvlm-256m-instruct");
    flat.as_object_mut().unwrap().remove("text_config");
    assert!(misaka_palw_tir_lower::adapter::builtin::find_wrapper_for("Idefics3ForConditionalGeneration", &flat).is_none());
    let _ = idx;
}
