//! **`MIXER_KDA_V1`** (Kimi delta attention) and **`MIXER_MLA_NOPE_V1`** (FR-16): the gated delta rule with a forget gate per KEY CHANNEL, and
//! latent attention with no rotation. The equality with `transformers`' `KimiLinearForCausalLM` is the tiny fixture's (`fidelity_tiny::kimi_linear`,
//! `hf_fixtures::kimi_linear`, and the corpus entry `kimi_linear`); here the properties of the pieces that make it a feature and not a family:
//! the channel-wise decay reduces to the head-wise one when its channels agree, differs when they do not, the spec reads the hub's spellings
//! and refuses what it does not model by name.

use misaka_palw_tir_lower::float_ref::gated_delta;
use misaka_palw_tir_lower::spec::{HeadMap, Mixer};
use misaka_palw_tir_lower::{hl, parse_config_str};
use std::path::Path;

fn tiny() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/tiny/kimi_linear.json")).unwrap()
}

/// Deterministic pseudo-random values in `[-1, 1)`.
fn vals(n: usize, seed: u64) -> Vec<f32> {
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((x >> 33) as f32 / (1u64 << 30) as f32) - 1.0
        })
        .collect()
}

fn run(g: &[f32]) -> Vec<f32> {
    let (h, dk, dv) = (2usize, 4usize, 3usize);
    let mut s = vec![0f32; h * dk * dv];
    let mut out = Vec::new();
    for t in 0..5u64 {
        let (q, k, v) = (vals(h * dk, 1 + t), vals(h * dk, 20 + t), vals(h * dv, 40 + t));
        out.extend(gated_delta(&q, &k, &v, g, &[0.7, 0.4], &mut s, h, h, dk, dv, HeadMap::Group, 0.5));
    }
    out
}

#[test]
fn a_channel_wise_decay_with_equal_channels_is_the_head_wise_decay() {
    let per_head = [-0.3f32, -0.9];
    let per_channel: Vec<f32> = per_head.iter().flat_map(|g| [*g; 4]).collect();
    assert_eq!(run(&per_head), run(&per_channel));
}

#[test]
fn a_channel_wise_decay_forgets_each_key_channel_at_its_own_rate() {
    let per_head = [-0.3f32, -0.9];
    let mut per_channel: Vec<f32> = per_head.iter().flat_map(|g| [*g; 4]).collect();
    per_channel[2] = -2.5;
    per_channel[5] = -0.05;
    let (a, b) = (run(&per_head), run(&per_channel));
    assert!(a.iter().zip(&b).any(|(x, y)| (x - y).abs() > 1e-3), "one channel's rate must change the output");
    // The first position's output is read after the write of an empty state: the decay acts from the second position on.
    assert_eq!(a[..6], b[..6]);
}

#[test]
fn a_kimi_linear_config_reads_as_delta_attention_and_latent_attention_without_rotation() {
    let s = parse_config_str(&tiny()).unwrap();
    let kinds: Vec<&str> = s.layers.iter().map(|l| match &l.mixer {
        Mixer::Kda(_) => "kda",
        Mixer::Mla(m) if m.rope.is_none() => "mla-nope",
        _ => "other",
    }).collect();
    assert_eq!(kinds, ["kda", "kda", "mla-nope", "kda"]);
    let ids: Vec<&str> = s.features().iter().map(|u| u.id.0).collect();
    for want in ["MIXER_KDA_V1", "MIXER_MLA_NOPE_V1", "CONV_DEPTHWISE_CAUSAL_V1"] {
        assert!(ids.contains(&want), "{want} in {ids:?}");
    }
    let p = hl::build_program(&s).unwrap();
    let nodes: Vec<&hl::Op> = p.blocks.iter().flat_map(|b| b.nodes.iter().map(|n| &n.op)).collect();
    assert!(nodes.iter().any(|o| matches!(o, hl::Op::GatedDelta { channel_decay: true, .. })));
    assert!(!nodes.iter().any(|o| matches!(o, hl::Op::Rope { .. })), "no layer of this model rotates anything");
    // Three convolutions per delta layer, one tensor each (the hub's q_conv1d, k_conv1d, v_conv1d).
    for role in ["kda.q_conv.w", "kda.k_conv.w", "kda.v_conv.w", "kda.A", "kda.dt_bias"] {
        assert!(p.params.iter().any(|d| d.name == role), "{role}");
    }
}

#[test]
fn a_kimi_linear_config_that_rotates_the_latent_attention_or_changes_the_router_is_refused_by_name() {
    for (from, to, needle) in [
        ("\"model_type\": \"kimi_linear\"", "\"model_type\": \"kimi_linear\", \"mla_use_nope\": false", "mla_use_nope"),
        ("\"model_type\": \"kimi_linear\"", "\"model_type\": \"kimi_linear\", \"moe_router_activation_func\": \"softmax\"", "moe_router_activation_func"),
        ("\"model_type\": \"kimi_linear\"", "\"model_type\": \"kimi_linear\", \"moe_layer_freq\": 2", "moe_layer_freq"),
        ("\"model_type\": \"kimi_linear\"", "\"model_type\": \"kimi_linear\", \"num_nextn_predict_layers\": 1", "multi-token"),
    ] {
        let cfg = tiny().replace(from, to);
        match parse_config_str(&cfg) {
            Err(e) => assert!(e.to_string().contains(needle), "refused, but not by `{needle}`: {e}"),
            Ok(s) => panic!("{needle}: expected a refusal, got a spec of {} layers", s.layers.len()),
        }
    }
}

#[test]
fn a_kimi_linear_layer_list_in_the_hubs_spelling_decides_the_layer_types() {
    // No `layer_types`: `linear_attn_config`'s 1-based lists name them (layer 2 and 4 are full attention here), and the dense/sparse rule comes from
    // `first_k_dense_replace`.
    let mut v: serde_json::Value = serde_json::from_str(&tiny()).unwrap();
    let o = v.as_object_mut().unwrap();
    o.remove("layer_types");
    o.remove("mlp_layer_types");
    o.insert("first_k_dense_replace".into(), 2.into());
    o.insert(
        "linear_attn_config".into(),
        serde_json::json!({"kda_layers": [1, 3], "full_attn_layers": [2, 4], "num_heads": 4, "head_dim": 8, "short_conv_kernel_size": 4}),
    );
    let s = parse_config_str(&v.to_string()).unwrap();
    let kinds: Vec<bool> = s.layers.iter().map(|l| matches!(l.mixer, Mixer::Kda(_))).collect();
    assert_eq!(kinds, [true, false, true, false]);
    let dense: Vec<bool> = s.layers.iter().map(|l| matches!(l.ffn, misaka_palw_tir_lower::spec::Ffn::Mlp(_))).collect();
    assert_eq!(dense, [true, true, false, false]);
}
