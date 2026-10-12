//! **A learned pointwise activation (xIELU) and the sub-layer norms (BitNet)** — the court on the lowered programs and the
//! refusals that keep a learned activation from being silently another function.
//!
//! The fidelity numbers against transformers are `tests/hf_fixtures.rs` (float) and `tests/fidelity_tiny.rs` (integer); the three
//! implementations are `tests/three_way.rs`. Here: the court's demand evaluator reproduces every node of every occurrence of both
//! programs from the committed leaves (no new court kernel), xIELU is a table PER LAYER (each layer's four scalars make its own
//! 65,536 entries), and xIELU anywhere but a dense MLP is refused by name.

mod common;

use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::{Act, Ffn, Glu};
use misaka_palw_tir_lower::weights::Checkpoint;
use std::path::Path;
use std::sync::Arc;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name)
}

struct Case {
    prep: fidelity::Prepared,
    mat: misaka_palw_tir_lower::lower::Materialised,
    tokens: Vec<u32>,
}

fn case(name: &str) -> Case {
    let dir = fixture(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
    let ck = Checkpoint::open(&dir).expect("checkpoint");
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).expect("params");
    let loader = Resident(Arc::new(params));
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24, 11);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).expect("calibrate");
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).expect("materialise");
    let tokens: Vec<u32> = fidelity::random_sequences(prep.hl.vocab, 1, 12, 5)[0].iter().map(|t| *t as u32).collect();
    Case { prep, mat, tokens }
}

/// The court reproduces every node of every occurrence of both programs from the committed leaves, and every primitive each
/// program uses is evaluated by it.
#[test]
fn the_court_reproduces_every_node_of_the_xielu_and_sublayer_norm_programs() {
    for name in ["apertus", "bitnet"] {
        let c = case(name);
        let p = &c.prep.lowered.program;
        let last = c.tokens.len() as u32 - 1;
        let r = common::court_coverage(p, &c.mat.params, &c.tokens, &[0, 3, last], &[4]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let used: std::collections::BTreeSet<&str> = p.blocks.iter().flat_map(|b| b.nodes.iter().map(|n| n.prim.name())).collect();
        for u in &used {
            assert!(r.primitives.get(u).copied().unwrap_or(0) > 0, "{name}: primitive {u} is used but the court never evaluated it");
        }
        eprintln!("{name}: {} nodes, {} committed values reproduced from the leaves, {} primitives", r.nodes, r.commits, r.primitives.len());
        assert!(r.commits > 0);
    }
}

/// xIELU is one 65,536-entry table PER LAYER: the stacked table param has a row of entries for every layer, and two layers whose
/// scalars differ do not share one.
#[test]
fn xielu_is_a_table_per_layer_from_the_layers_own_scalars() {
    let c = case("apertus");
    let j = c.prep.lowered.program.params.iter().position(|d| d.name == "mlp.act.table").expect("the activation table param");
    let decl = &c.prep.lowered.program.params[j];
    assert!(decl.per_layer, "one table per layer");
    let t = c.mat.params.tensors.get(&(j as u16, Some(0))).expect("layer 0's table");
    let u = c.mat.params.tensors.get(&(j as u16, Some(1))).expect("layer 1's table");
    assert_eq!(t.len(), 65536);
    assert_ne!(t, u, "the layers' tables are made from their own alpha_p, alpha_n, beta and eps");
}

/// xIELU reads parameters of its layer: in a clamped SwiGLU, a head transform, a per-layer embedding gate, an attention output gate
/// or an expert MLP it would be another function, so each is refused by name (and the Level A template and every other reader still
/// refuse `hidden_act = "xielu"` outright: `tests/real_configs.rs`).
#[test]
fn xielu_anywhere_but_a_dense_mlp_is_refused_by_name() {
    let cfg = std::fs::read_to_string(fixture("apertus").join("config.json")).expect("config");
    let mut spec = misaka_palw_tir_lower::parse_config_str(&cfg).expect("apertus reads");
    // The clamped SwiGLU over a xIELU gate.
    for l in &mut spec.layers {
        if let Ffn::Mlp(m) = &mut l.ffn {
            m.gated = true;
            m.glu = Glu::ClampedSwiGlu { alpha: 1.0, limit: 7.0 };
        }
    }
    let e = misaka_palw_tir_lower::hl::build_program(&spec).unwrap_err().to_string();
    assert!(e.contains("xIELU in a clamped SwiGLU"), "{e}");
    // A gated MLP with the plain activation list, but a head transform that uses it.
    let mut spec = misaka_palw_tir_lower::parse_config_str(&cfg).expect("apertus reads");
    spec.head.transform = Some(misaka_palw_tir_lower::spec::HeadTransformSpec { bias: false, act: Act::Xielu, norm: misaka_palw_tir_lower::spec::NormSpec::rms(1e-5) });
    let e = misaka_palw_tir_lower::hl::build_program(&spec).unwrap_err().to_string();
    assert!(e.contains("xIELU in a head transform"), "{e}");
}
