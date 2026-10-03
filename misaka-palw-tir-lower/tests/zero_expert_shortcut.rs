//! **`MLP_MOE_ZERO_EXPERT_V1`** and **`FFN_SHORTCUT_MOE_V1`** (FR-08, LongCat-Flash): the float reference equals transformers'
//! `LongcatFlashForCausalLM` (MLA with the two LoRA scales, a softmax router over experts + identity experts with a selection bias, the
//! MoE of sub-layer 0 added to the output of sub-layer 1), the court reproduces every node, and the side value rides a carry at the
//! residual's scale. The integer-versus-float numbers are `tests/hf_fixtures.rs` and the corpus harness; identity of the three
//! implementations is `tests/three_way.rs`.

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
fn longcat_flash_matches_its_hf_fixture() {
    matches_hf("longcat_flash");
}

#[test]
fn the_court_reproduces_every_node_of_the_longcat_program() {
    let prep = court("longcat_flash");
    let ids: Vec<&str> = prep.spec.features().iter().map(|f| f.id.0).collect();
    assert!(ids.contains(&"MLP_MOE_ZERO_EXPERT_V1") && ids.contains(&"FFN_SHORTCUT_MOE_V1"), "{ids:?}");
}

/// The shortcut MoE is a carry of its own at the residual's scale: one `i32` carry past `h`, written by the layers that produce it
/// and passed through by the others.
#[test]
fn the_shortcut_moe_rides_a_residual_scale_carry() {
    let cfg = std::fs::read_to_string(dir("longcat_flash").join("config.json")).expect("config");
    let spec = parse_config_str(&cfg).expect("reads");
    let prog = hl::build_program(&spec).expect("hl");
    assert_eq!(prog.carries.len(), 2, "h and the side carry");
    assert!(prog.carries[1].resid && prog.carries[1].shape == vec![spec.hidden_size]);
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).expect("prepare");
    let p = &prep.lowered.program;
    for b in p.blocks.iter().skip(1) {
        assert_eq!(b.carry_in.len(), 2);
        assert!(b.carry_in.iter().all(|t| t.dtype == misaka_palw_tir::DType::I32), "both carries ride the i32 rail");
    }
}

/// A MoE that consumes a side value no layer produces is refused by name.
#[test]
fn a_consumer_without_a_producer_is_refused_by_name() {
    use misaka_palw_tir_lower::spec::{Ffn, ShortcutSide};
    let cfg = std::fs::read_to_string(dir("longcat_flash").join("config.json")).expect("config");
    let mut spec = parse_config_str(&cfg).expect("reads");
    for l in &mut spec.layers {
        if let Ffn::MlpShortcut(sc) = &mut l.ffn {
            sc.side = ShortcutSide::Consume;
        }
    }
    let e = hl::build_program(&spec).err().expect("refused").to_string();
    assert!(e.contains("FFN_SHORTCUT_MOE_V1"), "{e}");
}
