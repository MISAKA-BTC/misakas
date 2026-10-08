//! **A pipeline class is judged, or a blocker is named; the preflight never says `unknown` with exit 0** (COV-P4, 2026-10-08, a fix
//! routed from H1's `flan-t5-small` run).
//!
//! `misaka model preflight hf://google/flan-t5-small@0fc9ddf7… --depth shape --max-context 512` lowered the encoder–decoder to its two
//! RFC-0003 programs, admitted each alone, and printed that the class's rules "are not judged here": register and mine `unknown`, exit
//! code 0. Now the class is declared shape-only (`preflight::pipeline`) and asked of the node's own generative gate
//! (`palw_gen_registration_preflight_at_v1`) at the judged height, and a route this build cannot declare is the blocker
//! `PIPELINE_CLASS_UNDECLARED`.
//!
//! The fixtures are the lowerer's tiny encoder–decoders (`tests/fixtures/hf-encdec`) and a flan-t5-small-shaped header-only checkpoint
//! made here from the model's configuration and the tensor names of the T5 family.

use misaka_palw_sdk::preflight::{Blocker, Depth, Options, Report, StageStatus, run};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(rel)
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("preflight-pipeline-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch");
    d
}

/// A copy of a fixture directory with a tokenizer beside it (the fixtures carry none), every safetensors file cut after its header.
fn header_only_copy(src: &Path, tag: &str) -> PathBuf {
    let dst = scratch(tag);
    std::fs::write(dst.join("tokenizer.json"), "{}").expect("tokenizer");
    for e in std::fs::read_dir(src).expect("fixture").flatten() {
        let name = e.file_name();
        let bytes = std::fs::read(e.path()).expect("read");
        let bytes = if name.to_string_lossy().ends_with(".safetensors") {
            let n = u64::from_le_bytes(bytes[..8].try_into().expect("8")) as usize;
            bytes[..8 + n].to_vec()
        } else {
            bytes
        };
        std::fs::write(dst.join(&name), bytes).expect("write");
    }
    dst
}

fn judged(depth: Depth, height: Option<u64>, max_context: Option<u32>) -> Options {
    Options { depth, network: Some("testnet-12".into()), height, max_context, pipeline_admission: true, ..Options::default() }
}

fn code_of(b: &Blocker) -> String {
    match &b.arg {
        Some(a) => format!("{}({a})", b.code),
        None => b.code.clone(),
    }
}

fn codes(r: &Report) -> Vec<String> {
    r.blockers().iter().map(|b| code_of(b)).collect()
}

/// **The invariant**: at the shape depth, no stage of a report is `unknown`; and a model is `registrable()` only when every stage is
/// `ok` — a report with a blocker, or one that could not judge a stage, exits non-zero.
fn never_unknown_with_exit_zero(r: &Report) {
    assert_eq!(r.depth.requested, Depth::Shape);
    if r.registrable() {
        for (name, v) in [("convert", &r.verdict.convert), ("register", &r.verdict.register), ("mine", &r.verdict.mine)] {
            assert_eq!(
                v.status,
                StageStatus::Ok,
                "registrable() with {name} {:?}: {}",
                v.status,
                v.unknown_because.clone().unwrap_or_default()
            );
        }
    }
}

/// A flan-t5-small-shaped header-only checkpoint: the configuration of `google/flan-t5-small` (revision 0fc9ddf7…) and the tensors the
/// T5 v1.1 family stores — gated-gelu feed-forwards (`wi_0`, `wi_1`), an untied head, relative-position bias in the first block of each
/// stack — as a safetensors file holding only its header.
fn flan_t5_small_headers(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    let config = json!({
        "architectures": ["T5ForConditionalGeneration"], "d_ff": 1024, "d_kv": 64, "d_model": 512, "decoder_start_token_id": 0,
        "dense_act_fn": "gelu_new", "dropout_rate": 0.1, "eos_token_id": 1, "feed_forward_proj": "gated-gelu", "initializer_factor": 1.0,
        "is_encoder_decoder": true, "is_gated_act": true, "layer_norm_epsilon": 1e-06, "model_type": "t5", "n_positions": 512,
        "num_decoder_layers": 8, "num_heads": 6, "num_layers": 8, "pad_token_id": 0, "relative_attention_max_distance": 128,
        "relative_attention_num_buckets": 32, "tie_word_embeddings": false, "vocab_size": 32128
    });
    std::fs::write(dir.join("config.json"), config.to_string()).expect("config");
    std::fs::write(dir.join("tokenizer.json"), "{}").expect("tokenizer");
    let (d, ff, inner, heads, vocab, layers) = (512u64, 1024u64, 384u64, 6u64, 32128u64, 8usize);
    let mut t: Vec<(String, Vec<u64>)> = vec![("shared.weight".into(), vec![vocab, d]), ("lm_head.weight".into(), vec![vocab, d])];
    for (stack, cross) in [("encoder", false), ("decoder", true)] {
        for b in 0..layers {
            let p = format!("{stack}.block.{b}.layer");
            for m in ["q", "k", "v"] {
                t.push((format!("{p}.0.SelfAttention.{m}.weight"), vec![inner, d]));
            }
            t.push((format!("{p}.0.SelfAttention.o.weight"), vec![d, inner]));
            if b == 0 {
                t.push((format!("{p}.0.SelfAttention.relative_attention_bias.weight"), vec![32, heads]));
            }
            t.push((format!("{p}.0.layer_norm.weight"), vec![d]));
            let ffn = if cross {
                for m in ["q", "k", "v"] {
                    t.push((format!("{p}.1.EncDecAttention.{m}.weight"), vec![inner, d]));
                }
                t.push((format!("{p}.1.EncDecAttention.o.weight"), vec![d, inner]));
                t.push((format!("{p}.1.layer_norm.weight"), vec![d]));
                2
            } else {
                1
            };
            t.push((format!("{p}.{ffn}.DenseReluDense.wi_0.weight"), vec![ff, d]));
            t.push((format!("{p}.{ffn}.DenseReluDense.wi_1.weight"), vec![ff, d]));
            t.push((format!("{p}.{ffn}.DenseReluDense.wo.weight"), vec![d, ff]));
            t.push((format!("{p}.{ffn}.layer_norm.weight"), vec![d]));
        }
        t.push((format!("{stack}.final_layer_norm.weight"), vec![d]));
    }
    let mut header = serde_json::Map::new();
    let mut at = 0u64;
    for (name, shape) in &t {
        let bytes = shape.iter().product::<u64>() * 4;
        header.insert(name.clone(), json!({"dtype": "F32", "shape": shape, "data_offsets": [at, at + bytes]}));
        at += bytes;
    }
    let text = serde_json::to_vec(&Value::Object(header)).expect("header");
    let mut file = (text.len() as u64).to_le_bytes().to_vec();
    file.extend(text);
    std::fs::write(dir.join("model.safetensors"), file).expect("header-only safetensors");
    dir
}

/// **The measured verdict of `google/flan-t5-small` at 512 positions** (2026-10-08), pinned so that a change of the verdict is seen: the
/// encoder stage's terminal close sizes past the generative close-sizing work cap (2^26, censored at cap+1) under every layout the
/// search offers — the same family of refusal the vision towers met, whose sizing needs the tile-class twin. The model is not
/// registrable and the command exits non-zero, by a code the chain would give.
#[test]
fn flan_t5_small_at_512_is_refused_by_the_close_sizing_cap_and_exits_nonzero() {
    let dir = flan_t5_small_headers("flan-pin");
    let r = run(&dir, &judged(Depth::Shape, Some(9_000), Some(512))).expect("preflight");
    assert_eq!(r.verdict.register.status, StageStatus::Blocked, "{:?}", codes(&r));
    assert_eq!(codes(&r), vec!["CLOSE_SIZE_OVER_CAP(generative close sizing work)".to_string()]);
    assert!(!r.registrable());
}

/// The H1 failure, reproduced on the model's headers: before the fix `register` and `mine` were `unknown` ("no program to judge") and
/// the exit code 0. Now the class is judged by the generative admission at the height: a verdict (ok or a named blocker) in every stage.
#[test]
fn a_flan_t5_small_class_is_judged_by_the_generative_admission_not_left_unknown() {
    let dir = flan_t5_small_headers("flan");
    // The ruleset where every scheduled fence is in force (DAA 9,000) and today's (6,900): both are judged, neither is `unknown`.
    for height in [9_000u64, 6_900] {
        let r = run(&dir, &judged(Depth::Shape, Some(height), Some(512))).expect("preflight");
        assert_eq!(r.depth.reached, Depth::Shape, "DAA {height}: the shape depth was reached");
        assert_eq!(r.verdict.convert.status, StageStatus::Ok, "DAA {height}: {:?}", codes(&r));
        assert_ne!(r.verdict.register.status, StageStatus::Unknown, "DAA {height}: register must be judged");
        assert_ne!(r.verdict.mine.status, StageStatus::Unknown, "DAA {height}: mine must be judged");
        let p = r.pipeline.as_ref().expect("a pipeline class report");
        assert_eq!((p.kind.as_str(), p.profile.as_str(), p.source_len, p.target_len), ("encdec", "Text", 512, 512));
        assert_eq!(p.stages.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["encoder", "decoder"]);
        let ad = r.admission.as_ref().expect("an admission block");
        assert!(ad.verdict.starts_with("pipeline class"), "{}", ad.verdict);
        // Whatever the verdict, it is the chain's: an admitted class says so, a refusal carries the chain's own code.
        match r.verdict.register.status {
            StageStatus::Ok => assert_eq!(ad.gate, "admitted", "DAA {height}"),
            _ => {
                assert!(
                    r.verdict.register.blockers.iter().all(|b| b.code != "PIPELINE_CLASS_UNDECLARED"),
                    "DAA {height}: {:?}",
                    codes(&r)
                );
                assert!(
                    r.verdict.register.blockers.iter().all(|b| b.evidence.iter().any(|e| e.starts_with("on-chain code "))),
                    "DAA {height}: a refusal names the chain's own code: {:?}",
                    r.verdict.register.blockers
                );
            }
        }
        // The seat is sized whether or not the class is admitted (the weights and the stages' own working set).
        assert!(r.seat.is_some(), "DAA {height}");
        assert!(!r.registrable() || r.verdict.register.status == StageStatus::Ok);
        never_unknown_with_exit_zero(&r);
    }
}

/// The class a model declares is a pipeline class judged at the height the caller names: below `palw_gen_v1` (DAA 5,300 on testnet-12)
/// it is the fence that blocks, by name, and the model is not registrable.
#[test]
fn below_the_generative_fence_the_registration_waits_for_it_by_name() {
    let dir = header_only_copy(&fixture("hf-encdec/t5"), "fence");
    let r = run(&dir, &judged(Depth::Shape, Some(100), Some(32))).expect("preflight");
    assert_eq!(r.verdict.register.status, StageStatus::Blocked, "{:?}", codes(&r));
    assert!(codes(&r).contains(&"FENCE_NOT_ARMED(palw_gen_v1)".to_string()), "{:?}", codes(&r));
    assert!(!r.registrable());
    never_unknown_with_exit_zero(&r);
}

/// The toy T5 of the lowerer's fixtures (real tiny weights, its tokenizer a stub): declared at 32 source and 32 target positions, judged
/// at DAA 9,000 — admitted, the seat holds it, the model is registrable, and the class is the Text profile of a source and a forced
/// decoder-start prefix.
#[test]
fn a_tiny_t5_is_admitted_as_a_text_class_with_a_source_and_registrable() {
    let dir = header_only_copy(&fixture("hf-encdec/t5"), "tiny");
    let r = run(&dir, &judged(Depth::Shape, Some(9_000), Some(32))).expect("preflight");
    assert_eq!(r.verdict.convert.status, StageStatus::Ok, "{:?}", codes(&r));
    assert_eq!(r.verdict.register.status, StageStatus::Ok, "{:?}", codes(&r));
    assert_eq!(r.verdict.mine.status, StageStatus::Ok, "{:?}", codes(&r));
    assert!(r.registrable());
    assert_eq!(r.admission.as_ref().map(|a| a.gate.as_str()), Some("admitted"));
    let p = r.pipeline.as_ref().expect("pipeline");
    assert!(p.source_token_floor >= 1);
    assert!(p.conventions.iter().any(|c| c.contains("placeholder")), "the shape-only conventions are said");
    assert!(r.seat.is_some(), "the seat is sized");
    // The JSON form carries the pipeline block.
    let j: Value = serde_json::from_str(&r.to_json()).expect("json");
    assert_eq!(j["pipeline"]["profile"], "Text");
    never_unknown_with_exit_zero(&r);
}

/// A route whose class this build cannot declare shape-only is a blocker of its own — Whisper's fixed feature-frame source has no job
/// binding in the protocol — and the model is not registrable: never `unknown` with exit 0.
#[test]
fn a_class_this_build_cannot_declare_is_a_named_blocker_and_exits_nonzero() {
    let dir = header_only_copy(&fixture("hf-encdec/whisper"), "whisper");
    let r = run(&dir, &judged(Depth::Shape, Some(9_000), Some(32))).expect("preflight");
    let c = codes(&r);
    if r.verdict.convert.status == StageStatus::Ok {
        assert_eq!(r.verdict.register.status, StageStatus::Blocked, "{c:?}");
        assert!(c.iter().any(|x| x.starts_with("PIPELINE_CLASS_UNDECLARED(")), "{c:?}");
        assert!(r.verdict.register.blockers.iter().any(|b| b.evidence.iter().any(|e| e.contains("feature-frame"))));
    }
    assert!(!r.registrable(), "{c:?}");
    never_unknown_with_exit_zero(&r);
}

/// `Options::default()` is unchanged: the corpus preflight pins (`tests/golden/corpus_preflight_v1.json`, hashed by the RFC-0011
/// coverage audit) record a pipeline class's register and mine as `unknown`, and this test keeps that record true. The command-line
/// tools and the census turn the judgment on; and even with it off, a shape-depth report with an `unknown` stage is not registrable.
#[test]
fn the_default_options_keep_the_pinned_legacy_verdict_but_it_is_never_registrable() {
    let dir = header_only_copy(&fixture("hf-encdec/t5"), "legacy");
    let opts = Options { depth: Depth::Shape, network: Some("testnet-12".into()), max_context: Some(128), ..Options::default() };
    assert!(!opts.pipeline_admission);
    let r = run(&dir, &opts).expect("preflight");
    assert_eq!(r.verdict.convert.status, StageStatus::Ok);
    assert_eq!(r.verdict.register.status, StageStatus::Unknown);
    assert_eq!(r.verdict.mine.status, StageStatus::Unknown);
    assert!(!r.registrable(), "unknown at the shape depth is not an exit code of 0");
    // At the headers depth `unknown` is by design and the exit code is unchanged.
    let h = run(&dir, &Options { depth: Depth::Headers, ..opts }).expect("preflight");
    assert_eq!(h.verdict.convert.status, StageStatus::Ok);
    assert!(h.registrable());
}

// ---- the census reads the same verdict -----------------------------------------------------------------------------------------------

/// A fetched-repository directory (`tools/hf_census/fetch.py`'s layout) for the census: the configuration and the safetensors header of
/// `model_dir`, the tokenizer a stub, the Hub's inventory sizes the files' own.
fn fetched_store(model_dir: &Path, tag: &str) -> (misaka_palw_sdk::census::store::Fetched, misaka_palw_sdk::census::ListingV1) {
    use misaka_palw_sdk::census::store::{FETCH_SCHEMA_V1, FetchV1, Fetched, FileV1, InfoV1, ItemV1};
    use std::io::Write;
    let dir = scratch(&format!("store-{tag}"));
    std::fs::create_dir_all(dir.join("f")).expect("f");
    std::fs::create_dir_all(dir.join("h")).expect("h");
    let config = std::fs::read(model_dir.join("config.json")).expect("config");
    std::fs::write(dir.join("f/config.json"), &config).expect("stored config");
    std::fs::write(dir.join("f/tokenizer.json"), b"{}").expect("stored tokenizer");
    let st = std::fs::read(model_dir.join("model.safetensors")).expect("weights");
    let n = u64::from_le_bytes(st[..8].try_into().expect("8")) as usize;
    let header = &st[..8 + n];
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(header).expect("gzip");
    std::fs::write(dir.join("h/model.safetensors.hdr.gz"), gz.finish().expect("gz")).expect("stored header");
    // The Hub's size of the weights: the header and the data it declares.
    let declared: u64 = {
        let v: Value = serde_json::from_slice(&header[8..]).expect("header json");
        v.as_object()
            .expect("object")
            .iter()
            .filter(|(k, _)| *k != "__metadata__")
            .map(|(_, t)| t["data_offsets"][1].as_u64().unwrap_or(0))
            .max()
            .unwrap_or(0)
    };
    let item = |path: &str, kind: &str, store: &str, header_len: Option<u64>| ItemV1 {
        path: path.into(),
        kind: kind.into(),
        status: "ok".into(),
        store: Some(store.into()),
        header_len,
        ..Default::default()
    };
    let fetch = FetchV1 {
        schema: FETCH_SCHEMA_V1.into(),
        repo: "o/t5".into(),
        revision: "r".repeat(40),
        info: InfoV1 { status: "ok".into(), ..Default::default() },
        inventory: vec![
            FileV1 { path: "config.json".into(), size: config.len() as u64, ..Default::default() },
            FileV1 { path: "model.safetensors".into(), size: 8 + n as u64 + declared, ..Default::default() },
            FileV1 { path: "tokenizer.json".into(), size: 2, ..Default::default() },
        ],
        items: vec![
            item("config.json", "file", "f/config.json", None),
            item("tokenizer.json", "file", "f/tokenizer.json", None),
            item("model.safetensors", "st_header", "h/model.safetensors.hdr.gz", Some(8 + n as u64)),
        ],
        base: None,
    };
    let listing = misaka_palw_sdk::census::ListingV1 {
        id: "o/t5".into(),
        oid: "0".into(),
        sha: Some("r".repeat(40)),
        pipeline_tag: Some("text2text-generation".into()),
        gated: Value::Bool(false),
        config: serde_json::from_slice(&config).expect("config json"),
        siblings: vec!["config.json".into(), "model.safetensors".into(), "tokenizer.json".into()],
        ..Default::default()
    };
    (Fetched { dir, fetch }, listing)
}

/// **The census's `admit` gate for an encoder–decoder is the pipeline admission's verdict, not `NOT_RUN_PIPELINE_ADMISSION`** — the code
/// that left the T5 family `UNTESTED` for a reason that was the tool's. A tiny T5 declared at 32 positions is shape-ready; a flan-t5-small
/// at 512 is refused by the chain's own close-sizing code, a `RESOURCE_REFUSED` outcome; neither is `NOT_RUN`.
#[test]
fn the_census_admit_gate_reads_the_pipeline_verdict_for_an_encoder_decoder() {
    use misaka_palw_sdk::census::codes::{Gate, GateStatus};
    use misaka_palw_sdk::census::gates::{CensusContext, ContextRule, evaluate};
    use misaka_palw_sdk::census::{RightsPolicy, onboarding::GapClassV1};
    let gate = |r: &misaka_palw_sdk::census::CensusRowV1, g: Gate| r.technical.iter().find(|x| x.gate == g).cloned().expect("gate");

    // The tiny T5: shape-ready at the ruleset where every scheduled fence is in force (the census default height).
    let tiny = header_only_copy(&fixture("hf-encdec/t5"), "census-tiny");
    let (fx, l) = fetched_store(&tiny, "tiny");
    let ctx = CensusContext::new("p4", "testnet-12", None, RightsPolicy::None, "test").with_context_rule(ContextRule::Fixed(32));
    let row = evaluate(&l, Some(&fx), &ctx);
    assert_eq!(gate(&row, Gate::Lower).status, GateStatus::Pass, "{:?}", gate(&row, Gate::Lower));
    let admit = gate(&row, Gate::Admit);
    assert_eq!((admit.status, admit.blocking.clone()), (GateStatus::Pass, None), "{admit:?}");
    assert!(row.shape_ready, "a tiny T5 is shape-ready under the pipeline admission");
    assert!(!row.registration_ready, "shape-ready is not registered");

    // flan-t5-small at 512: refused by the chain's code, a resource outcome — not a gate that was not run.
    let flan = flan_t5_small_headers("census-flan");
    let (fx, l) = fetched_store(&flan, "flan");
    let ctx = CensusContext::new("p4", "testnet-12", None, RightsPolicy::None, "test").with_context_rule(ContextRule::Fixed(512));
    let row = evaluate(&l, Some(&fx), &ctx);
    assert_eq!(gate(&row, Gate::Lower).status, GateStatus::Pass, "{:?}", gate(&row, Gate::Lower));
    let admit = gate(&row, Gate::Admit);
    assert_eq!(admit.status, GateStatus::Fail, "{admit:?}");
    assert_eq!(admit.blocking.as_deref(), Some("CLOSE_TOO_LARGE"), "{admit:?}");
    assert_eq!(admit.class, Some(GapClassV1::ResourceRefused));
    assert!(!row.shape_ready);
    assert!(row.technical.iter().all(|g| g.blocking.as_deref() != Some("NOT_RUN_PIPELINE_ADMISSION")));
}
