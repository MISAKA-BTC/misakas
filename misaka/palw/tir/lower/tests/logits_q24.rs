//! **`LOGITS_Q24_V1`** (lowering version 2): a text program's `logits` are natural-log units × 2^24
//! in an `i32`, whatever the model — so a sampling temperature means the same for every class and a
//! reader never needs the pack's calibrated scale.
//!
//! * the materialised logits scale is exactly `2^−24`;
//! * the integer logits divided by `2^24` agree with transformers' float logits within the
//!   quantisation's own error (the fixtures' HF logits, `logits_full`);
//! * the narrowing is the head's last one: the program is the same one the calibrated-scale recipe
//!   lowered (`tests/golden_lowering.rs` holds its digest), only the `(m, s)` params differ;
//! * a model whose calibrated logits pass 120 natural-log units is refused, not clipped.

use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LOGITS_Q24_REFUSE_AT, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{LowerError, fidelity};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const Q24: f64 = 1.0 / 16_777_216.0;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name)
}

struct Run {
    logits_scale: f64,
    int: Vec<Vec<f64>>,
    hf: Vec<Vec<f32>>,
    nodes: usize,
}

fn run(name: &str) -> Run {
    let dir = fixture(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let loader = Resident(Arc::new(params));
    let quiet = |_: usize, _: usize| {};
    // A learned position table bounds the sequence.
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32.min(max_len), 7);
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("logits.json")).expect("logits.json")).expect("json");
    let tokens: Vec<usize> = meta["tokens"].as_array().expect("tokens").iter().map(|t| t.as_u64().expect("int") as usize).collect();
    let hf: Vec<Vec<f32>> =
        meta["logits_full"].as_array().expect("logits").iter().map(|r| r.as_array().expect("row").iter().map(|x| x.as_f64().expect("f") as f32).collect()).collect();
    let int = fidelity::int_logits(&prep.lowered.program, &mat.params, &tokens, mat.logits_scale, &|_| {}).expect("integer run");
    Run { logits_scale: mat.logits_scale, int, hf, nodes: prep.lowered.program.blocks.iter().map(|b| b.nodes.len()).sum() }
}

fn rms(v: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut n) = (0.0, 0usize);
    for x in v {
        s += x * x;
        n += 1;
    }
    (s / n.max(1) as f64).sqrt()
}

/// The fixtures span the heads the lowering has: a plain one, a tied one, a soft-capped one
/// (Gemma-2's `tanh` table), a scaled one (Granite's `logits_scaling`), an MoE, a recurrent one and
/// a learned-position one.
#[test]
fn the_logits_are_natural_log_units_times_two_to_the_24() {
    for name in ["llama", "llama_linear_tied", "gemma2", "granite", "qwen2_moe", "mamba", "gpt2", "gemma4"] {
        let r = run(name);
        assert_eq!(r.logits_scale, Q24, "{name}: the logits' unit is 2^-24");
        let err = rms(r.int.iter().zip(&r.hf).flat_map(|(a, b)| a.iter().zip(b).map(|(x, y)| x - *y as f64)));
        let mag = rms(r.hf.iter().flat_map(|row| row.iter().map(|x| *x as f64)));
        // Where transformers' best token leads its runner-up by more than the quantisation's error
        // many times over, the integer program picks it too (random weights leave many near-ties).
        let decided: Vec<(usize, &Vec<f64>, &Vec<f32>)> = r
            .int
            .iter()
            .zip(&r.hf)
            .enumerate()
            .filter(|(_, (_, b))| margin(b) > 8.0 * err)
            .map(|(p, (a, b))| (p, a, b))
            .collect();
        let wrong: Vec<usize> = decided.iter().filter(|(_, a, b)| argmax(a) != argmax_f32(b)).map(|(p, _, _)| *p).collect();
        eprintln!(
            "{name:>18}: |logits| rms {mag:.3}  rms error vs HF {err:.4} ({:.2}%)  decided positions {}/{} wrong {}  [{} nodes]",
            100.0 * err / mag,
            decided.len(),
            r.int.len(),
            wrong.len(),
            r.nodes
        );
        assert!(err <= 0.06 * mag + 0.01, "{name}: the Q24 logits are {err} (rms) from transformers' of magnitude {mag}");
        assert!(wrong.is_empty(), "{name}: positions {wrong:?} disagree with transformers' top-1 although it leads by more than 8× the error");
    }
}

/// The lead of the best logit over the runner-up.
fn margin(v: &[f32]) -> f64 {
    let mut s: Vec<f64> = v.iter().map(|x| *x as f64).collect();
    s.sort_by(|a, b| b.partial_cmp(a).expect("finite"));
    s[0] - s[1]
}

fn argmax(v: &[f64]) -> usize {
    v.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).expect("finite")).map(|(i, _)| i).unwrap_or(0)
}

fn argmax_f32(v: &[f32]) -> usize {
    v.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).expect("finite")).map(|(i, _)| i).unwrap_or(0)
}

/// A head scaled to logits of hundreds: the calibration sees a logit past the Q24 format's room
/// (`i32` at 2^−24 holds ±128) and the lowering says so instead of clipping.
#[test]
fn a_model_whose_calibrated_logits_pass_the_bound_is_refused() {
    let dir = fixture("llama");
    let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let mut spec = misaka_palw_tir_lower::parse_config_str(&cfg).expect("spec");
    spec.head.logit_scale = 5000.0;
    let prep = fidelity::prepare_spec(spec, &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let loader = Resident(Arc::new(params));
    let quiet = |_: usize, _: usize| {};
    let calib = fidelity::random_sequences(prep.hl.vocab, 6, 32, 7);
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    match materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet) {
        Err(LowerError::NotLowerable(m)) => assert!(m.contains("LOGITS_Q24_V1") && m.contains("128"), "{m}"),
        other => panic!("expected a NOT_LOWERABLE refusal, got {:?}", other.map(|m| m.logits_scale)),
    }
    assert!(LOGITS_Q24_REFUSE_AT < 128.0);
}
