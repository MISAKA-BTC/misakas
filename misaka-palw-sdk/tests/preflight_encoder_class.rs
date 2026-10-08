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

/// **A span head over a token-type table is refused by name** (`ENC_PAIR_SEGMENTS_V1`): BERT's QA head reads `question ‖ sep ‖ context`
/// as two segments, and this build's program adds type row 0 to every position — a different function, so no class is declared for it,
/// not even hypothetically. RoBERTa's (one type row) and DistilBERT's (none) are declared (above).
#[test]
fn a_span_head_over_token_types_needs_pair_segments_by_name() {
    let dir = with_tokenizer(&fixture("hf-heads/bert_qa"), "bert-qa");
    let r = run(&dir, &judged(9_000)).expect("preflight");
    let b = r.blockers().into_iter().find(|b| b.code == "ARCH_NEEDS_FEATURE").cloned();
    assert_eq!(b.and_then(|b| b.arg), Some("ENC_PAIR_SEGMENTS_V1".to_string()), "{:?}", r.blockers());
    assert!(!r.registrable());
    assert!(r.notes.iter().all(|n| !n.starts_with("HEAD_PROFILE_HYPOTHETICAL")), "{:?}", r.notes);
    for rel in ["hf-heads/roberta_qa", "hf-heads/bert_tokcls"] {
        let r = run(&with_tokenizer(&fixture(rel), "no-segments"), &judged(9_000)).expect("preflight");
        assert!(r.blockers().iter().all(|b| b.arg.as_deref() != Some("ENC_PAIR_SEGMENTS_V1")), "{rel}");
    }
}
