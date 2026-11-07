//! **`MIXER_MOA_V1`** (FR-06, JetMoE): the float reference equals transformers' `JetMoeForCausalLM` (mixture of attention and of
//! MLP experts, the shared tiled K/V, the `[D]` biases), the court reproduces every node of the lowered program. The integer-versus-float
//! numbers are `tests/fidelity_tiny.rs::jetmoe`; the three implementations' identity is `tests/three_way.rs` (every fixture).

mod common;

use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{hf_weights, hl, parse_config_str};
use std::path::Path;
use std::sync::Arc;

fn dir(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name)
}

/// The float reference against transformers' logits on the fixture's weights (every checkpoint tensor read).
fn matches_hf(name: &str) {
    let d = dir(name);
    let cfg = std::fs::read_to_string(d.join("config.json")).expect("config");
    let spec = parse_config_str(&cfg).unwrap_or_else(|e| panic!("{name}: {e}"));
    let prog = hl::build_program(&spec).expect("hl");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&d).expect("checkpoint");
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "{name}: every checkpoint tensor is read: {unused:?}");
    let meta: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(d.join("logits.json")).unwrap()).unwrap();
    let tokens: Vec<usize> = meta["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let got = Session::new(&prog, &params).run(&tokens).expect("run");
    let want = meta["logits_full"].as_array().unwrap();
    let (mut worst, mut scale) = (0f64, 1f64);
    for (g, w) in got.iter().zip(want) {
        for (a, b) in g.iter().zip(w.as_array().unwrap()) {
            let b = b.as_f64().unwrap();
            worst = worst.max((*a as f64 - b).abs());
            scale = scale.max(b.abs());
        }
    }
    assert!(worst <= 1e-4 * scale, "{name}: max |dlogit| {worst:e} against transformers (scale {scale})");
}

/// The court reproduces every node of every occurrence of the lowered program from the committed leaves, and every primitive the
/// program uses is evaluated by it.
fn court(name: &str) -> misaka_palw_tir_lower::fidelity::Prepared {
    let d = dir(name);
    let cfg = std::fs::read_to_string(d.join("config.json")).expect("config");
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&d).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let loader = Resident(Arc::new(params));
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24, 11);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let tokens: Vec<u32> = fidelity::random_sequences(prep.hl.vocab, 1, 12, 5)[0].iter().map(|t| *t as u32).collect();
    let p = &prep.lowered.program;
    let last = tokens.len() as u32 - 1;
    let r = common::court_coverage(p, &mat.params, &tokens, &[0, 3, last], &[4]).unwrap_or_else(|e| panic!("{name}: {e}"));
    let used: std::collections::BTreeSet<&str> = p.blocks.iter().flat_map(|b| b.nodes.iter().map(|n| n.prim.name())).collect();
    for u in &used {
        assert!(r.primitives.get(u).copied().unwrap_or(0) > 0, "{name}: primitive {u} is used but the court never evaluated it");
    }
    assert!(r.commits > 0);
    prep
}

#[test]
fn jetmoe_matches_its_hf_fixture() {
    matches_hf("jetmoe");
}

#[test]
fn the_court_reproduces_every_node_of_the_jetmoe_program() {
    let prep = court("jetmoe");
    // The attention is the mixture: the program holds the expert-linear gathers and the exact weighted sum.
    assert!(prep.spec.features().iter().any(|f| f.id.0 == "MIXER_MOA_V1"));
}
