//! **`ROPE_REVERSED_V1`** (FR-35): nanochat's `rotate_half` rotates by −θ, the inverse of the usual rotation. A rope
//! spec can say so (`RopeFreqs::reverse`, the adapter's `{"$rope": {…, "reverse": true}}`): the frequencies are stored
//! negated, so the float reference and the lowering's two-level cos/sin tables follow with no other change.
//!
//! No transformers: the tiny `llama` config with seeded synthetic weights. Equality with nanochat's reference is the
//! corpus lane's fixture; this file pins what is local to the crate.

use misaka_palw_tir_lower::fidelity::prepare_spec;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::spec::{Mixer, ModelSpec, Position};
use misaka_palw_tir_lower::{hl, parse_config_str};
use std::path::Path;

const TOKENS: [usize; 6] = [3, 17, 42, 5, 17, 9];

fn spec(reversed: bool) -> ModelSpec {
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/tiny/llama.json")).unwrap();
    let mut s = parse_config_str(&cfg).unwrap();
    let mut n = 0;
    for l in &mut s.layers {
        if let Mixer::Attention(a) = &mut l.mixer
            && let Position::Rope(r) = &mut a.position
        {
            if reversed {
                r.freqs = r.freqs.clone().reverse().unwrap();
            }
            n += 1;
        }
    }
    assert!(n > 0, "the tiny llama rotates");
    s
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

#[test]
fn a_reversed_rope_is_the_inverse_rotation_and_lowers() {
    let (fwd, rev) = (spec(false), spec(true));
    assert!(rev.features().iter().any(|f| f.id.0 == "ROPE_REVERSED_V1"));
    assert!(!fwd.features().iter().any(|f| f.id.0 == "ROPE_REVERSED_V1"));
    let (pf, pr) = (hl::build_program(&fwd).unwrap(), hl::build_program(&rev).unwrap());
    // Same params, same weights: the logits differ from position 1 on (position 0 is rotated by angle 0 either way).
    let params = ParamStore::synthetic(&pf, 11);
    let a = Session::new(&pf, &params).run(&TOKENS).unwrap();
    let b = Session::new(&pr, &params).run(&TOKENS).unwrap();
    assert!(max_diff(&a[0], &b[0]) < 1e-5, "position 0 is rotated by angle 0");
    let late = (1..TOKENS.len()).map(|p| max_diff(&a[p], &b[p])).fold(0.0, f32::max);
    assert!(late > 1e-4, "rotating by −θ is a different function ({late})");
    // The lowerer builds the same program shape (the tables carry the sign), and refuses nothing.
    let (lf, lr) = (prepare_spec(fwd, &LowerOpts::default()).unwrap(), prepare_spec(rev, &LowerOpts::default()).unwrap());
    assert_eq!(lf.lowered.program.blocks.len(), lr.lowered.program.blocks.len());
    assert_eq!(lf.lowered.program.params.len(), lr.lowered.program.params.len());
}
