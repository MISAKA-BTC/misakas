//! **An image classifier's class is a pipeline class, judged** (HFX 2026-10-10).
//!
//! Before: a `…ForImageClassification` repository's tower (ViT) was not read at all (no adapter), and a convolutional network's
//! classifier was ignored; either ended at `PIPELINE_CLASS_UNDECLARED`. Now the preflight reads the classifier (`VisionOut::Classify`,
//! `CnnHead`), declares the one-stage pipeline over the job's canonical image shape-only
//! (`model::route::lower_image_class_shape_v1`) and judges it as the dormant `Head` profile's IMAGE task:
//! `FENCE_NOT_ARMED(palw_task_heads_v1)` at every height, the admission the profile would run recorded beside it
//! (`HEAD_PROFILE_HYPOTHETICAL: …`), and the media-pipeline kernel route (K2-TIR-v3) reported, never merged.

use misaka_palw_sdk::preflight::{Depth, Options, Report, StageStatus, run};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/hf-vis").join(name)
}

fn judged(height: u64) -> Options {
    Options { depth: Depth::Shape, network: Some("testnet-12".into()), height: Some(height), pipeline_admission: true, ..Options::default() }
}

fn head_fence(r: &Report) -> bool {
    r.blockers().iter().any(|b| b.code == "FENCE_NOT_ARMED" && b.arg.as_deref() == Some("palw_task_heads_v1"))
}

#[test]
fn an_image_classifier_is_declared_and_judged_as_the_dormant_head_profiles_image_task() {
    for (name, kind) in [("vit_cls", "image:vision"), ("resnet_cls", "image:cnn"), ("convnext_cls", "image:cnn"), ("mobilenet_v2_cls", "image:cnn")] {
        let r = run(&fixture(name), &judged(9_000)).expect("preflight");
        let codes: Vec<_> = r.blockers().iter().map(|b| (b.code.clone(), b.arg.clone())).collect();
        assert!(r.blockers().iter().all(|b| b.code != "PIPELINE_CLASS_UNDECLARED"), "{name}: {codes:?}");
        let p = r.pipeline.as_ref().unwrap_or_else(|| panic!("{name}: the declared class is reported: {codes:?}"));
        assert_eq!(p.kind, kind, "{name}");
        assert!(head_fence(&r), "{name}: {codes:?}");
        assert!(!r.registrable(), "{name}: a dormant profile is never registrable");
        assert_ne!(r.verdict.register.status, StageStatus::Unknown, "{name}");
        let hypo: Vec<&String> = r.notes.iter().filter(|n| n.starts_with("HEAD_PROFILE_HYPOTHETICAL")).collect();
        assert_eq!(hypo.len(), 1, "{name}: {:?}", r.notes);
        eprintln!("{name}: {} | register blockers {codes:?}", hypo[0]);
        eprintln!("{name}: gate detail {:?}", r.admission.as_ref().map(|a| (&a.gate, &a.gate_detail)));
        // The media-pipeline kernel route is reported beside it.
        let k = r.kernel.as_ref().unwrap_or_else(|| panic!("{name}: no kernel route"));
        assert!(k.kernel.starts_with("K2-TIR-v3"), "{name}: {k:?}");
        assert_eq!(k.shipped, "KERNEL_NOT_ACTIVE", "{name}: {k:?}");
        eprintln!("{name}: kernel {k:?}");
    }
}

#[test]
fn an_image_backbone_is_still_not_declared_from_headers() {
    // The same ViT without its classifier is an image BACKBONE: an Embedding-profile class over an image slot, which this build does not declare.
    let r = run(&fixture("vit"), &judged(9_000)).expect("preflight");
    let undeclared = r.blockers().into_iter().find(|b| b.code == "PIPELINE_CLASS_UNDECLARED");
    assert!(undeclared.is_some(), "{:?}", r.blockers().iter().map(|b| &b.code).collect::<Vec<_>>());
}
