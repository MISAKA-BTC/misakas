//! **A bidirectional encoder's class is a pipeline class, judged** (HFX 2026-10-08).
//!
//! Before: an embedding repository (BERT, RoBERTa, MPNet, …) lowered to a per-position program the IR court judged as if it were a
//! decoder, and the census stopped every such repository at `NOT_RUN_PIPELINE_ADMISSION` — its class, the one-stage RFC-0003 pipeline
//! over the padded token axis, was never declared. Now the preflight declares it shape-only (`model::route::lower_bidir_class_shape_v1`)
//! and asks the node's own generative gate:
//!
//! * an embedding is an `Embedding`-profile class under `palw_gen_v1` (armed on testnet-12 at DAA 5,300): admitted, or refused by a
//!   named code with numbers — never `unknown`;
//! * a head (a sequence classifier, a token classifier, a span QA head) is a task of the dormant `Head` profile: `FENCE_NOT_ARMED
//!   (palw_task_heads_v1)` at every height, with the admission the profile would run recorded beside it
//!   (`HEAD_PROFILE_HYPOTHETICAL: …`), and never `registrable()`.

use misaka_palw_sdk::preflight::{Depth, Options, Report, StageStatus, run};
use std::path::{Path, PathBuf};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(rel)
}

/// A copy of a fixture directory with a tokenizer beside it (the fixtures carry none).
fn with_tokenizer(src: &Path, tag: &str) -> PathBuf {
    let dst = std::env::temp_dir().join(format!("preflight-encoder-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(&dst).expect("scratch");
    std::fs::write(dst.join("tokenizer.json"), "{}").expect("tokenizer");
    for e in std::fs::read_dir(src).expect("fixture").flatten() {
        if e.path().is_dir() {
            let sub = dst.join(e.file_name());
            std::fs::create_dir_all(&sub).expect("dir");
            for f in std::fs::read_dir(e.path()).expect("sub").flatten() {
                std::fs::copy(f.path(), sub.join(f.file_name())).expect("copy");
            }
        } else {
            std::fs::copy(e.path(), dst.join(e.file_name())).expect("copy");
        }
    }
    dst
}

fn judged(height: u64) -> Options {
    Options {
        depth: Depth::Shape,
        network: Some("testnet-12".into()),
        height: Some(height),
        max_context: Some(12),
        pipeline_admission: true,
        ..Options::default()
    }
}

fn head_fence(r: &Report) -> bool {
    r.blockers().iter().any(|b| b.code == "FENCE_NOT_ARMED" && b.arg.as_deref() == Some("palw_task_heads_v1"))
}

#[test]
fn an_embedding_encoder_is_judged_as_an_embedding_class_and_never_left_unknown() {
    let dir = with_tokenizer(&fixture("hf-enc/bert"), "embed");
    let r = run(&dir, &judged(9_000)).expect("preflight");
    assert!(
        r.notes.iter().any(|n| n.contains("bidirectional encoder class (embedding head") && n.contains("RFC-0003 program")),
        "{:?}",
        r.notes
    );
    assert_ne!(r.verdict.register.status, StageStatus::Unknown, "{:?}", r.verdict.register.unknown_because);
    assert!(!head_fence(&r), "an embedding is the Embedding profile's, not the Head profile's");
    let p = r.pipeline.as_ref().expect("the declared class is reported");
    assert_eq!((p.kind.as_str(), p.profile.as_str()), ("encoder:embedding", "Embedding"));
    eprintln!(
        "bert embedding: register {:?}; blockers {:?}",
        r.verdict.register.status,
        r.blockers().iter().map(|b| &b.code).collect::<Vec<_>>()
    );
    // Below palw_gen_v1 (DAA 5,300 on testnet-12) the fence is the blocker, by name.
    let r = run(&dir, &judged(100)).expect("preflight");
    assert!(r.blockers().iter().any(|b| b.code == "FENCE_NOT_ARMED" && b.arg.as_deref() == Some("palw_gen_v1")));
    assert!(!r.registrable());
}

#[test]
fn a_head_is_the_dormant_head_profiles_and_its_hypothetical_admission_is_recorded() {
    for (rel, head) in [("hf-cls/bert_cls", "sequence"), ("hf-heads/bert_tokcls", "token"), ("hf-heads/distilbert_qa", "span_qa")] {
        let dir = with_tokenizer(&fixture(rel), head);
        let r = run(&dir, &judged(9_000)).expect("preflight");
        assert!(head_fence(&r), "{rel}: {:?}", r.blockers().iter().map(|b| (&b.code, &b.arg)).collect::<Vec<_>>());
        assert!(!r.registrable(), "{rel}: a dormant profile is never registrable");
        assert_ne!(r.verdict.register.status, StageStatus::Unknown, "{rel}");
        let p = r.pipeline.as_ref().expect("the declared class is reported");
        assert_eq!(p.kind, format!("encoder:{head}"), "{rel}");
        let hypo: Vec<&String> = r.notes.iter().filter(|n| n.starts_with("HEAD_PROFILE_HYPOTHETICAL")).collect();
        assert_eq!(hypo.len(), 1, "{rel}: {:?}", r.notes);
        eprintln!("{rel}: {}", hypo[0]);
    }
}

/// **A span head over a token-type table is declared with BERT-type pair segments** (`ENC_PAIR_SEGMENTS_V1`, HFX 2026-10-10): BERT's QA
/// head reads `question ‖ sep ‖ context` as two segments (token type 1 after the first separator). The class's program computes the
/// segment ids from the job's ids (an equality against the class's separator, a count of earlier separators, a gather of two type rows),
/// so a class is declared for it — hypothetically, behind the dormant `Head` profile — where before it was refused by name. RoBERTa's
/// (one type row) and DistilBERT's (none) have no segment ids and are declared without the feature.
#[test]
fn a_span_head_over_token_types_is_declared_with_pair_segments() {
    let dir = with_tokenizer(&fixture("hf-heads/bert_qa"), "bert-qa");
    let r = run(&dir, &judged(9_000)).expect("preflight");
    assert!(r.blockers().iter().all(|b| b.code != "ARCH_NEEDS_FEATURE"), "{:?}", r.blockers());
    assert!(r.notes.iter().any(|n| n.contains("pair segments (ENC_PAIR_SEGMENTS_V1)")), "{:?}", r.notes);
    assert!(head_fence(&r) && !r.registrable());
    assert_eq!(r.notes.iter().filter(|n| n.starts_with("HEAD_PROFILE_HYPOTHETICAL")).count(), 1, "{:?}", r.notes);
    for rel in ["hf-heads/roberta_qa", "hf-heads/distilbert_qa", "hf-heads/bert_tokcls"] {
        let r = run(&with_tokenizer(&fixture(rel), "no-segments"), &judged(9_000)).expect("preflight");
        assert!(r.blockers().iter().all(|b| b.arg.as_deref() != Some("ENC_PAIR_SEGMENTS_V1")), "{rel}");
        assert!(r.notes.iter().all(|n| !n.contains("pair segments")), "{rel}: {:?}", r.notes);
    }
}

/// **A bidirectional encoder's class reports its K2-TIR-v5 route beside the generative verdict** (HFX 2026-10-10; lane K2S,
/// `k2-real-scale.md` §12): the same program is judged by the v5 descriptor's plan, `check_plan_with_v1`, the per-prosecution gate and the
/// carrier. Shipped it is `KERNEL_NOT_ACTIVE` (the descriptor is implemented, never active); hypothetically armed it is `ELIGIBLE_AT`
/// for a small encoder at its declared context. Reported, never merged into the generative verdict (`register`/`mine` do not read it).
#[test]
fn every_encoder_class_reports_its_k2_tir_v5_route_beside_the_generative_verdict() {
    for rel in ["hf-enc/bert", "hf-cls/bert_cls", "hf-heads/bert_tokcls", "hf-heads/bert_qa", "hf-heads/distilbert_qa", "hf-heads/roberta_qa"] {
        let r = run(&with_tokenizer(&fixture(rel), "k2"), &judged(9_000)).expect("preflight");
        let k = r.kernel.as_ref().unwrap_or_else(|| panic!("{rel}: no kernel route"));
        eprintln!("{rel}: {k:?}");
        assert!(k.kernel.starts_with("K2-TIR-v5"), "{rel}: {k:?}");
        assert_eq!(k.shipped, "KERNEL_NOT_ACTIVE", "{rel}: {k:?}");
        assert_eq!(k.hypothetical, "ELIGIBLE_AT", "{rel}: {k:?}");
    }
}

/// Append an `I64` tensor named `name` (shape `[1, n]`, zeros) to the directory's `model.safetensors` — what an older checkpoint's
/// `embeddings.position_ids` buffer is.
fn with_i64_buffer(dir: &Path, name: &str, n: usize) {
    let path = dir.join("model.safetensors");
    let bytes = std::fs::read(&path).expect("model.safetensors");
    let hlen = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
    let mut header: serde_json::Value = serde_json::from_slice(&bytes[8..8 + hlen]).expect("header");
    let data = &bytes[8 + hlen..];
    let at = data.len();
    header[name] = serde_json::json!({"dtype": "I64", "shape": [1, n], "data_offsets": [at, at + 8 * n]});
    let header = serde_json::to_vec(&header).unwrap();
    let mut out = (header.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(&header);
    out.extend_from_slice(data);
    out.extend(std::iter::repeat_n(0u8, 8 * n));
    std::fs::write(&path, out).expect("rewrite");
}

/// **A module buffer is no weight of the class** (HFX 2026-10-10): older BERT-lineage checkpoints carry `embeddings.position_ids` (an
/// `I64` range). The tensor check skips it (a buffer, `weights::is_module_buffer`), but the storage check used to count it as a bound
/// tensor stored as a type no descriptor claims — `QUANT_NO_DESCRIPTOR(safetensors/I64)` on every such checkpoint, the MiniLM / MPNet
/// sentence-transformers among them. It is neither bound nor a blocker.
#[test]
fn an_i64_position_ids_buffer_is_not_a_quantisation_blocker() {
    let dir = with_tokenizer(&fixture("hf-enc/bert"), "pos-ids");
    with_i64_buffer(&dir, "embeddings.position_ids", 512);
    let r = run(&dir, &judged(9_000)).expect("preflight");
    assert!(
        r.blockers().iter().all(|b| b.code != "QUANT_NO_DESCRIPTOR"),
        "{:?}",
        r.blockers().iter().map(|b| (&b.code, &b.arg)).collect::<Vec<_>>()
    );
    assert_ne!(r.verdict.convert.status, StageStatus::Unknown);
    // …and a real weight stored as I64 still is one: a second, non-buffer I64 tensor the program reads would block. (The fixture binds
    // none, so the buffer is the only I64 tensor; its absence from the blockers is the whole claim.)
    let base = run(&with_tokenizer(&fixture("hf-enc/bert"), "pos-ids-base"), &judged(9_000)).expect("preflight");
    assert_eq!(
        r.blockers().iter().map(|b| b.code.clone()).collect::<Vec<_>>(),
        base.blockers().iter().map(|b| b.code.clone()).collect::<Vec<_>>(),
        "the buffer changes no verdict"
    );
}
