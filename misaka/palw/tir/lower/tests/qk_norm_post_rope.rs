//! **`ATTN_QK_NORM_POST_ROPE_V1`** (FR-02): Hunyuan normalises the per-head q and k AFTER the rotation, Qwen3
//! BEFORE it. A rotation preserves a head's L2 norm but a per-channel gain does not commute with it, so the
//! two orders are different functions — the order is part of the model, not a detail of the lowering.
//!
//! No transformers here: the tiny `qwen3_moe` config (per-head shared q/k norms and RoPE) with seeded synthetic
//! weights. The equality with transformers' Hunyuan classes is the corpus harness's job
//! (`PALW_CORPUS_ONLY=hunyuan_v1_dense`, `hunyuan_v1_moe`); this file pins what is local to the crate.

use misaka_palw_tir_lower::fidelity::prepare_spec;
use misaka_palw_tir_lower::float_ref::{ParamStore, Session};
use misaka_palw_tir_lower::lower::LowerOpts;
use misaka_palw_tir_lower::spec::{Mixer, ModelSpec};
use misaka_palw_tir_lower::{hl, parse_config_str};
use std::path::Path;

const TOKENS: [usize; 6] = [3, 17, 42, 5, 17, 9];

fn spec(after_rope: bool) -> ModelSpec {
    let cfg = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/tiny/qwen3_moe.json")).unwrap();
    let mut s = parse_config_str(&cfg).unwrap();
    let mut attention_layers = 0;
    for l in &mut s.layers {
        if let Mixer::Attention(a) = &mut l.mixer {
            assert!(a.qk_norm.is_some(), "the tiny qwen3_moe config has q/k norms");
            assert!(!a.qk_norm_after_rope, "Qwen3's order is the default");
            a.qk_norm_after_rope = after_rope;
            attention_layers += 1;
        }
    }
    assert!(attention_layers > 0);
    s
}

fn uses(s: &ModelSpec, id: &str) -> bool {
    s.features().iter().any(|f| f.id.0 == id)
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

#[test]
fn the_order_of_the_norm_and_the_rotation_is_part_of_the_function() {
    let (before, after) = (spec(false), spec(true));
    // The feature is reported for the post-rope order and only for it.
    assert!(uses(&after, "ATTN_QK_NORM_POST_ROPE_V1"));
    assert!(!uses(&before, "ATTN_QK_NORM_POST_ROPE_V1"));
    let (pb, pa) = (hl::build_program(&before).unwrap(), hl::build_program(&after).unwrap());
    pb.validate().unwrap();
    pa.validate().unwrap();
    // Only the order of the nodes moves: the same params, the same count of nodes.
    assert_eq!(pb.params, pa.params, "the flag moves nodes, never params");
    let nodes = |p: &hl::HlProgram| p.blocks.iter().map(|b| b.nodes.len()).sum::<usize>();
    assert_eq!(nodes(&pb), nodes(&pa));
    // The same weights give different logits — except at position 0, where the rotation is the identity.
    let params = ParamStore::synthetic(&pb, 11);
    let a = Session::new(&pb, &params).run(&TOKENS).unwrap();
    let b = Session::new(&pa, &params).run(&TOKENS).unwrap();
    assert!(max_diff(&a[0], &b[0]) < 1e-5, "position 0 is rotated by angle 0: both orders agree there ({})", max_diff(&a[0], &b[0]));
    let late = (1..TOKENS.len()).map(|p| max_diff(&a[p], &b[p])).fold(0.0, f32::max);
    assert!(late > 1e-4, "the two orders are the same function ({late})");
}

#[test]
fn the_post_rope_order_lowers() {
    // The lowerer sees the same nodes in the other order: it must not refuse or panic.
    let prep = prepare_spec(spec(true), &LowerOpts::default()).unwrap_or_else(|e| panic!("{e}"));
    assert!(!prep.lowered.program.blocks.is_empty());
}
