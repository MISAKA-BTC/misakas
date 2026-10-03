//! **`ATTN_CROSS_V1`** (FR-21, Mllama's vision cross-attention): the text stage reads `rows` rows of the projected vision states (a
//! declared input; the tower is not computed) through its cross-attention layers. `tools/gen_mllama_cross_fixture.py` ran
//! transformers' `MllamaForConditionalGeneration` over the tiny checkpoint with seeded random states (`cross.json`).

use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::spec::CrossStatesSpec;
use misaka_palw_tir_lower::weights::Checkpoint;
use misaka_palw_tir_lower::{hf_weights, hl, parse_config_str};
use std::path::Path;

pub fn dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf/mllama")
}

fn cross() -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir().join("cross.json")).expect("cross.json")).unwrap()
}

fn states_of(c: &serde_json::Value) -> Vec<Vec<f32>> {
    c["cross_states"].as_array().unwrap().iter().map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect()).collect()
}

/// The float reference with the states bound equals transformers over the whole prompt, every checkpoint tensor read (the cross
/// layers' included), and differs from the text-only stage (the layers matter).
#[test]
fn mllama_with_vision_states_matches_its_hf_fixture() {
    let c = cross();
    let cfg = std::fs::read_to_string(dir().join("config.json")).expect("config");
    let mut spec = parse_config_str(&cfg).expect("mllama reads");
    let rows = c["rows"].as_u64().unwrap() as usize;
    spec.cross_states = Some(CrossStatesSpec { rows });
    let prog = hl::build_program(&spec).expect("hl");
    assert_eq!(prog.schedule.len(), spec.layers.len(), "the cross layer runs");
    let binding = hf_weights::bind(&spec, &prog).expect("bind");
    let ck = Checkpoint::open(&dir()).expect("checkpoint");
    let (params, unused) = ParamStore::from_source(&prog, &binding, &ck).expect("params");
    assert!(unused.is_empty(), "the cross layer's tensors are read: {unused:?}");
    let tokens: Vec<usize> = c["tokens"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let mut s = Session::new(&prog, &params);
    s.cross_states = Some(states_of(&c));
    let got = s.run(&tokens).expect("run");
    let want = c["logits_full"].as_array().unwrap();
    let (mut worst, mut scale) = (0f64, 1f64);
    for (g, w) in got.iter().zip(want) {
        for (a, b) in g.iter().zip(w.as_array().unwrap()) {
            let b = b.as_f64().unwrap();
            worst = worst.max((*a as f64 - b).abs());
            scale = scale.max(b.abs());
        }
    }
    assert!(worst <= 1e-4 * scale, "max |dlogit| {worst:e} against transformers (scale {scale})");
    // The text-only stage is another function: the states change the logits.
    spec.cross_states = None;
    let p0 = hl::build_program(&spec).expect("hl text-only");
    let b0 = hf_weights::bind(&spec, &p0).expect("bind");
    let (params0, _) = ParamStore::from_source(&p0, &b0, &ck).expect("params");
    let text = Session::new(&p0, &params0).run(&tokens).expect("run");
    let moved = got.iter().zip(&text).any(|(a, b)| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-3));
    assert!(moved, "the cross layer changed nothing");
}
