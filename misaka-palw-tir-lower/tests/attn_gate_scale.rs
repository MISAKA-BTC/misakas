//! **`ATTN_OUTPUT_GATE_SEPARATE_V1`** (FR-30) and **`ATTN_VALUE_SCALE_V1`** (FR-31): AfMoE gates the attention output by
//! `sigmoid(gate_proj(x))` per element, Laguna by `softplus(g_proj(x))` per head, MiMo-V2-Flash scales the values by a
//! constant. No transformers: the tiny `llama` config with seeded synthetic weights — the float reference against the
//! integer program (the three-way equality is the court's job), and that each flag is a different function. The
//! equality with the transformers classes is the corpus lane's fixtures (`afmoe`, `laguna`, `mimo_v2_flash`).

use misaka_palw_tir_lower::fidelity::{self, prepare_spec};
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::spec::{Act, Mixer, ModelSpec, SeparateGateSpec};
use misaka_palw_tir_lower::{hl, parse_config_str};
use std::path::Path;
use std::sync::Arc;

const TOKENS: [usize; 6] = [3, 17, 42, 5, 17, 9];

fn spec(gate: Option<SeparateGateSpec>, v_scale: f64) -> ModelSpec {
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/tiny/llama.json")).unwrap();
    let mut s = parse_config_str(&cfg).unwrap();
    for l in &mut s.layers {
        if let Mixer::Attention(a) = &mut l.mixer {
            a.gate = gate;
            a.v_scale = v_scale;
        }
    }
    if gate.is_some() {
        s.hf.names.insert("attn.gate".into(), "model.layers.{L}.self_attn.gate_proj".into());
    }
    s
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

/// The relative L2 error of the integer program's logits against the float reference's, on synthetic weights.
fn integer_vs_float(s: ModelSpec) -> f64 {
    let prep = prepare_spec(s, &LowerOpts::default()).unwrap_or_else(|e| panic!("{e}"));
    let loader = Resident(Arc::new(ParamStore::synthetic(&prep.hl, 5)));
    let seqs = fidelity::random_sequences(prep.hl.vocab, 3, 12, 7);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &seqs, &quiet).unwrap();
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).unwrap();
    let f = fidelity::float_logits(&prep.hl, &loader, &seqs[..1], &quiet).unwrap();
    let i = fidelity::int_logits(&prep.lowered.program, &mat.params, &seqs[0], mat.logits_scale, &|_| {}).unwrap();
    let (mut num, mut den) = (0f64, 0f64);
    for (fr, ir) in f[0].iter().zip(&i) {
        for (a, b) in fr.iter().zip(ir) {
            num += (*a as f64 - b) * (*a as f64 - b);
            den += (*a as f64) * (*a as f64);
        }
    }
    (num / den).sqrt()
}

#[test]
fn a_separate_gate_and_a_value_scale_are_features_that_lower() {
    let base = spec(None, 1.0);
    let pb = hl::build_program(&base).unwrap();
    let a = Session::new(&pb, &ParamStore::synthetic(&pb, 3)).run(&TOKENS).unwrap();
    for (name, s, feature) in [
        ("sigmoid per element", spec(Some(SeparateGateSpec { act: Act::Sigmoid, per_head: false }), 1.0), "ATTN_OUTPUT_GATE_SEPARATE_V1"),
        ("softplus per head", spec(Some(SeparateGateSpec { act: Act::Softplus, per_head: true }), 1.0), "ATTN_OUTPUT_GATE_SEPARATE_V1"),
        ("a value scale", spec(None, 0.707), "ATTN_VALUE_SCALE_V1"),
    ] {
        assert!(s.features().iter().any(|f| f.id.0 == feature) && !base.features().iter().any(|f| f.id.0 == feature), "{name}");
        let p = hl::build_program(&s).unwrap_or_else(|e| panic!("{name}: {e}"));
        p.validate().unwrap();
        // A different function than the plain attention, on the same (synthetic) weights where the params coincide.
        let b = Session::new(&p, &ParamStore::synthetic(&p, 3)).run(&TOKENS).unwrap();
        assert!((0..TOKENS.len()).any(|t| max_diff(&a[t], &b[t]) > 1e-4), "{name} changed nothing");
        // And the integer program follows the float reference.
        let rel = integer_vs_float(s);
        assert!(rel < 0.08, "{name}: integer vs float relative error {rel}");
    }
}

#[test]
fn the_fused_and_the_separate_gate_are_exclusive() {
    let mut s = spec(Some(SeparateGateSpec { act: Act::Sigmoid, per_head: false }), 1.0);
    for l in &mut s.layers {
        if let Mixer::Attention(a) = &mut l.mixer {
            a.output_gate = true;
        }
    }
    assert!(hl::build_program(&s).err().expect("refused").to_string().contains("ATTN_OUTPUT_GATE_SEPARATE_V1"));
}
