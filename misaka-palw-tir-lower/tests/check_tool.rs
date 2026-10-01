//! `palw-tir-check`'s report: verdicts, the weights check, and the refusal line.

use misaka_palw_tir_lower::report::{self, WeightsArg};
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn a_fixture_with_its_weights_checks_clean() {
    let dir = root().join("tests/fixtures/hf/qwen3_next");
    if !dir.exists() {
        eprintln!("SKIPPED: no fixture at {}", dir.display());
        return;
    }
    let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
    let c = report::check(&cfg, Some(WeightsArg::Files(&dir))).unwrap();
    let w = c.weights.as_ref().unwrap();
    assert!(w.errors.is_empty() && w.unused.is_empty(), "{:?} {:?}", w.errors, w.unused);
    let text = report::render(&c);
    assert!(text.contains("gated delta net 2 k-heads") && text.contains("LOWERABLE_TO_HL"), "{text}");
}

#[test]
fn weights_of_another_model_are_reported_not_accepted() {
    let cfg = std::fs::read_to_string(root().join("tests/fixtures/hf/llama/config.json"));
    let dir = root().join("tests/fixtures/hf/mistral_window");
    let (Ok(cfg), true) = (cfg, dir.exists()) else {
        eprintln!("SKIPPED: fixtures absent");
        return;
    };
    let c = report::check(&cfg, Some(WeightsArg::Files(&dir))).unwrap();
    assert!(!c.weights.as_ref().unwrap().errors.is_empty(), "biases and shapes differ");
    assert!(report::verdict(&c).starts_with("NOT_LOWERABLE(weights"));
}

#[test]
fn refusals_print_the_rfc_verdict() {
    let cfg = std::fs::read_to_string(root().join("tests/configs/real/gpt-oss-20b-mxfp4.json")).unwrap();
    let e = report::check(&cfg, None).err().unwrap();
    assert!(report::refusal(&e).starts_with("NOT_LOWERABLE(GptOssForCausalLM: pre-quantized"));
}

#[test]
fn every_real_config_renders() {
    for e in std::fs::read_dir(root().join("tests/configs/real")).unwrap().flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap();
        match report::check(&text, None) {
            Ok(c) => {
                let r = report::render(&c);
                assert!(r.contains("per-position estimates") && r.contains("schedule:"), "{}", e.path().display());
                assert!(c.cost.param_elems > 0 && c.cost.macs_at(1) > 0);
            }
            Err(err) => assert!(report::refusal(&err).starts_with("NOT_LOWERABLE("), "{}: {err}", e.path().display()),
        }
    }
}

#[test]
fn parameter_counts_of_known_checkpoints_are_right() {
    // Sanity of the param binding shapes and the cost walk against published sizes.
    for (name, lo, hi) in [
        ("llama-3.1-8b", 7.9e9, 8.1e9),
        ("mistral-7b-v0.1", 7.2e9, 7.3e9),
        ("qwen2.5-7b-instruct", 7.5e9, 7.7e9),
        ("gemma-2-9b", 9.1e9, 9.3e9),
        ("mixtral-8x7b-v0.1", 46.6e9, 46.8e9),
        ("deepseek-v3-bf16", 670e9, 673e9),
        ("gpt-oss-20b-bf16", 20.8e9, 21.0e9),
        ("mamba-130m-hf", 0.12e9, 0.14e9),
        ("qwen3-30b-a3b", 30.3e9, 30.6e9),
    ] {
        let text = std::fs::read_to_string(root().join(format!("tests/configs/real/{name}.json"))).unwrap();
        let c = report::check(&text, None).unwrap();
        let p = c.cost.param_elems as f64;
        assert!(p >= lo && p <= hi, "{name}: {p:.4e} params, expected [{lo:.3e}, {hi:.3e}]");
    }
}

/// A layer that runs as several blocks (Gemma-4's mixer and FFN halves; Qwen4-Exp's n-gram, mixer and FFN
/// blocks) is checked against the checkpoint's tensors of its MODEL layer, not of each block's occurrence
/// index: the weights check reports no phantom layer, and no tensor of the checkpoint is left unread.
#[test]
fn a_layer_run_as_several_blocks_checks_clean() {
    for name in ["gemma4", "qwen4_exp", "qwen4_qsa_r3"] {
        let dir = root().join("tests/fixtures/hf").join(name);
        let cfg = std::fs::read_to_string(dir.join("config.json")).unwrap();
        let c = report::check(&cfg, Some(WeightsArg::Files(&dir))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let w = c.weights.as_ref().unwrap();
        assert!(w.errors.is_empty() && w.unused.is_empty(), "{name}: {:?} {:?}", w.errors, w.unused);
        assert!(w.bound > 0, "{name}");
    }
}
