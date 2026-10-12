//! **`LowerOpts::gdn_core_wide`** (H1, the Huihui-Qwen3.5-9B fit diagnosis): the gated-delta core delivered on the wide rail's finer
//! grid. Three things are pinned here, on the tiny Qwen3.5 / Qwen3-Next fixtures:
//!   1. the option OFF is the default — the program and every materialised tensor are what `LowerOpts::default()` gives (so no
//!      registered class's artifact moves);
//!   2. the option ON changes only the materialised constants and tensors — the node graph (the program the court replays) is the
//!      same, byte for byte;
//!   3. the option ON still lowers, runs on the reference evaluator, and agrees with the float reference no worse than OFF beyond noise.

use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::fidelity;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name)
}

struct Out {
    program: Vec<u8>,
    tensors: Vec<Vec<u8>>,
    metrics: fidelity::Metrics,
}

fn run(name: &str, opts: &LowerOpts) -> Out {
    let dir = fixture_dir(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let prep = fidelity::prepare(&cfg, opts).expect("prepare");
    analyze_ranges(&prep.lowered.program).expect("range analysis");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let loader = Resident(Arc::new(params));
    let vocab = prep.hl.vocab;
    let calib = fidelity::random_sequences(vocab, 6, 32, 7);
    let eval = fidelity::random_sequences(vocab, 3, 24, 1234);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let fl = fidelity::float_logits(&prep.hl, &loader, &eval, &quiet).expect("float");
    let il: Vec<Vec<Vec<f64>>> = eval
        .iter()
        .map(|s| fidelity::int_logits(&prep.lowered.program, &mat.params, s, mat.logits_scale, &|_| {}).expect("integer run"))
        .collect();
    Out {
        program: prep.lowered.program.encode(),
        tensors: mat.params.tensors.iter().map(|t| format!("{t:?}").into_bytes()).collect(),
        metrics: fidelity::compare(&fl, &il, &eval),
    }
}

fn pin(name: &str) {
    if !fixture_dir(name).join("model.safetensors").exists() {
        eprintln!("fixture {name}: missing, skipped");
        return;
    }
    let default = run(name, &LowerOpts::default());
    let off = run(name, &LowerOpts { gdn_core_wide: false, ..LowerOpts::default() });
    let on = run(name, &LowerOpts { gdn_core_wide: true, ..LowerOpts::default() });
    assert!(default.program == off.program && default.tensors == off.tensors, "{name}: the option off is not the default");
    assert!(on.program == off.program, "{name}: the option changed the program (it must change only materialised constants)");
    assert!(on.tensors != off.tensors, "{name}: the option changed nothing (the fixture has no gated-delta layer?)");
    eprintln!(
        "{name}: off top-1 {:.3} KL {:.5} | on top-1 {:.3} KL {:.5}",
        off.metrics.top1_agreement, off.metrics.kl_mean, on.metrics.top1_agreement, on.metrics.kl_mean
    );
    assert!(on.metrics.top1_agreement >= 0.9, "{name}: top-1 {} with the option", on.metrics.top1_agreement);
    assert!(on.metrics.kl_mean <= off.metrics.kl_mean.max(0.002) * 1.5 + 0.002, "{name}: KL {} against {} without it", on.metrics.kl_mean, off.metrics.kl_mean);
}

#[test]
fn qwen3_5_the_option_is_off_by_default_and_changes_only_materialised_values() {
    pin("qwen3_5");
}

#[test]
fn qwen3_next_the_option_is_off_by_default_and_changes_only_materialised_values() {
    pin("qwen3_next");
}
