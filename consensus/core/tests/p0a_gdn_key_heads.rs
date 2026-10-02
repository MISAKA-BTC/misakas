//! **P0a: the GDN key-head count is derived, and the held map that reads it spells it**
//! (`docs/design/palw/tir/phase-f-integration.md` §3.3, on `tir/phase-f`).
//!
//! What this file holds, before any fence or engine is involved:
//!
//! * profile version 3 and `palw_gdn_key_heads_v1`: the count at 2/4, 16/32 and 16/48 heads, and
//!   the shapes version 3 refuses;
//! * the maps: gdn v3 and the held composition v5 are new ids spelled as their parts;
//! * **v5 is v4 for every consensus reader — and only under a version-3 profile.** A V3 profile on v5
//!   is held, tiled, laid out, counted and priced exactly as the same graph at V1 on v4; a V1 or V2
//!   profile that names v5 is judged as a map this tree does not know, which is how a build before
//!   P0a judges it — so no registration an old build can see is judged differently by a new one;
//! * the court's slice reads the one derivation, and at 16/32 value head 20 slices key head 4.

use kaspa_consensus_core::palw_context_ladder::{palw_class_ladder_rules_for_court_v1, palw_qwen36_context_row_profile_v8};
use kaspa_consensus_core::palw_qwen36_profile::{
    PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_profile_v7, qwen36_profile_v8,
};
use kaspa_consensus_core::palw_state_chunk_map::{
    PALW_GDN_STATE_CHUNK_MAP_NAME_V2, PALW_GDN_STATE_CHUNK_MAP_NAME_V3, PALW_TILED_KV_STATE_CHUNK_MAP_NAME_V4,
    gdn_state_chunk_map_id_v2, gdn_state_chunk_map_id_v3, hybrid_state_chunk_map_id_v4, hybrid_state_chunk_map_id_v5,
    palw_hybrid_state_chunk_map_name_v5, palw_map_addresses_history_tiles_v1, palw_map_is_held_v4, palw_profile_is_held_v4,
    palw_profile_spells_key_heads_v1, palw_state_chunk_count_at_v1, palw_state_layout_v4,
};
use kaspa_consensus_core::palw_step::{
    PALW_STEP_OBJECT_VERSION_V1, PALW_STEP_OBJECT_VERSION_V2, PALW_STEP_OBJECT_VERSION_V3, PalwShapeProfileV3, PalwStepOutLenV1,
    palw_gdn_key_heads_v1,
};
use kaspa_consensus_core::palw_step_refute::{KDESC_Q36_GDN_STEP, qwen36_gdn_slice_v1};
use kaspa_hashes::Hash64;

/// The tiny hybrid geometry at `k` key and `v` value heads of dimension `hd` — Phase F §3.3's
/// fixtures (head dim 4, hidden 64, four layers at interval 4, tiny experts) and the dev fixture's
/// 2/4 at head dim 8.
fn tiny(k: u16, v: u16, hd: u32, hidden: u32) -> PalwQwen36GeometryV1 {
    PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: hidden,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: k,
        gdn_v_heads: v,
        gdn_head_dim: hd,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 32,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    }
}

fn geometries() -> Vec<(&'static str, PalwQwen36GeometryV1, u32)> {
    vec![
        ("dev fixture 2/4", tiny(2, 4, 8, 32), 2),
        ("16/32", tiny(16, 32, 4, 64), 16),
        ("16/48", tiny(16, 48, 4, 64), 16),
        ("Qwen3.6-35B-A3B @512", qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B }), 16),
    ]
}

#[test]
fn the_new_maps_are_new_ids_spelled_as_their_parts() {
    assert_ne!(gdn_state_chunk_map_id_v3(), gdn_state_chunk_map_id_v2());
    assert_ne!(hybrid_state_chunk_map_id_v5(), hybrid_state_chunk_map_id_v4());
    assert_ne!(gdn_state_chunk_map_id_v3(), hybrid_state_chunk_map_id_v5());
    let name = palw_hybrid_state_chunk_map_name_v5();
    assert!(name.contains(&format!("attn={PALW_TILED_KV_STATE_CHUNK_MAP_NAME_V4}/")), "{name}");
    assert!(name.contains(&format!("gdn={PALW_GDN_STATE_CHUNK_MAP_NAME_V3}/")), "{name}");
    // The v3 gather names the key-head count and how it is had; v2's names one head count.
    assert!(PALW_GDN_STATE_CHUNK_MAP_NAME_V3.contains("conv-head-gather=[q:(h%kh)*k,k:kh*k+(h%kh)*k,v:2*kh*k+h*v]"));
    assert!(PALW_GDN_STATE_CHUNK_MAP_NAME_V3.contains("window-row=(2*kh*k+heads*v)*4/kh=width(gdn.ref0)/k"));
    assert!(PALW_GDN_STATE_CHUNK_MAP_NAME_V2.contains("conv-head-gather=[q:h*k,k:heads*k+h*k,v:2*heads*k+h*v]"));
    // By id alone the held maps are ADR-0103's two; v5 is held only through a V3 profile.
    assert!(palw_map_is_held_v4(&hybrid_state_chunk_map_id_v4()));
    assert!(!palw_map_is_held_v4(&hybrid_state_chunk_map_id_v5()));
}

#[test]
fn graph_v8_derives_the_key_head_count_at_every_ratio() {
    for (name, g, want) in geometries() {
        let v8 = qwen36_profile_v8(g).unwrap_or_else(|e| panic!("{name}: graph-v8 projects: {e}"));
        assert_eq!(v8.version, PALW_STEP_OBJECT_VERSION_V3, "{name}");
        assert_eq!(v8.state_chunk_map_id, hybrid_state_chunk_map_id_v5(), "{name}");
        assert_eq!(palw_gdn_key_heads_v1(&v8).expect("derives"), want, "{name}");
        assert_eq!(v8.gdn_heads, g.gdn_v_heads, "{name}: gdn_heads stays the VALUE head count");
        assert!(palw_profile_is_held_v4(&v8) && palw_profile_spells_key_heads_v1(&v8), "{name}");
        // graph-v7 is the same graph at V1 on v4, node for node.
        let v7 = qwen36_profile_v7(g).expect("graph-v7 projects");
        assert_eq!((v7.version, v7.state_chunk_map_id), (PALW_STEP_OBJECT_VERSION_V1, hybrid_state_chunk_map_id_v4()));
        let tables = |p: &PalwShapeProfileV3| (p.pre_nodes.clone(), p.gdn_nodes.clone(), p.attn_nodes.clone(), p.post_nodes.clone());
        assert_eq!(tables(&v7), tables(&v8), "{name}: the graph is graph-v7's");
        assert_ne!(v7.shape_profile_id(), v8.shape_profile_id(), "{name}: and the class is another");
    }
    assert_eq!(palw_qwen36_context_row_profile_v8(512).expect("the 512 row"), qwen36_profile_v8(geometries()[3].1).unwrap());
}

/// **A V3 profile on v5 is v4's class for every consensus reader**: held, tiled, laid out, counted
/// and priced as the same graph at V1 on v4. Only the id differs, and with it every leaf the id is
/// bound into.
#[test]
fn v5_under_version_3_is_v4_for_every_consensus_reader() {
    for (name, g, _) in geometries() {
        let v8 = qwen36_profile_v8(g).expect("graph-v8");
        let v7 = qwen36_profile_v7(g).expect("graph-v7");
        assert_eq!(palw_map_addresses_history_tiles_v1(&v8), palw_map_addresses_history_tiles_v1(&v7), "{name}");
        assert!(palw_map_addresses_history_tiles_v1(&v8), "{name}: the held composition tiles the history");
        for positions in [1u32, 2, 3, 15, 16, 17, 31, 32] {
            let a = palw_state_layout_v4(&v8, positions).expect("v5 lays out");
            let b = palw_state_layout_v4(&v7, positions).expect("v4 lays out");
            assert_eq!(a, b, "{name} @{positions}");
            assert!(!a.gdn_layers.is_empty() || positions % 16 != 0, "{name} @{positions}: the recurrence rides its spacing");
            assert_eq!(
                palw_state_chunk_count_at_v1(&v8, positions),
                palw_state_chunk_count_at_v1(&v7, positions),
                "{name} @{positions}"
            );
        }
        let ladder = 1u64 << 26;
        let rules = |p: &PalwShapeProfileV3| {
            palw_class_ladder_rules_for_court_v1(p, None, ladder)
                .map(|r| (r.ladder, r.canonical_footprint_floor, format!("{:?}", r.cost_shape)))
        };
        assert_eq!(rules(&v8), rules(&v7), "{name}: priced as v4");
        assert_eq!(
            kaspa_consensus_core::palw_step::worst_case_step_leaf_count_capped_v1(&v8, ladder).ok(),
            kaspa_consensus_core::palw_step::worst_case_step_leaf_count_capped_v1(&v7, ladder).ok(),
            "{name}: the same step space"
        );
    }
}

/// **A V1 or V2 profile that names v5 is judged as an unknown map — as the build before P0a judges
/// it.** The comparison is against a map id nobody spells, on every reader the held regime moves.
#[test]
fn v5_under_another_version_is_a_map_this_tree_does_not_know() {
    let unknown = Hash64::from_bytes([0x5Au8; 64]);
    for (name, g, _) in geometries() {
        for version in [PALW_STEP_OBJECT_VERSION_V1, PALW_STEP_OBJECT_VERSION_V2] {
            let mut named = qwen36_profile_v7(g).expect("graph-v7");
            named.version = version;
            named.state_chunk_map_id = hybrid_state_chunk_map_id_v5();
            let mut stranger = named.clone();
            stranger.state_chunk_map_id = unknown;
            assert!(!palw_profile_is_held_v4(&named) && !palw_profile_spells_key_heads_v1(&named), "{name} v{version}");
            assert_eq!(
                palw_map_addresses_history_tiles_v1(&named),
                palw_map_addresses_history_tiles_v1(&stranger),
                "{name} v{version}"
            );
            for positions in [1u32, 16, 32] {
                assert_eq!(
                    palw_state_chunk_count_at_v1(&named, positions),
                    palw_state_chunk_count_at_v1(&stranger, positions),
                    "{name} v{version} @{positions}"
                );
            }
            assert_eq!(
                named.validate_shape().map_err(|e| e.to_string()),
                stranger.validate_shape().map_err(|e| e.to_string()),
                "{name} v{version}: the shape rules read it as any unknown id"
            );
            let ladder = 1u64 << 26;
            let rules = |p: &PalwShapeProfileV3| {
                palw_class_ladder_rules_for_court_v1(p, None, ladder)
                    .map(|r| (r.ladder, r.canonical_footprint_floor, format!("{:?}", r.cost_shape)))
            };
            assert_eq!(rules(&named), rules(&stranger), "{name} v{version}: priced as any unknown id");
        }
    }
}

#[test]
fn version_3_refuses_what_it_does_not_mean() {
    let g = tiny(16, 32, 4, 64);
    let v8 = qwen36_profile_v8(g).expect("graph-v8");
    // On v4 — a map whose gather is written over one head count.
    let mut on_v4 = v8.clone();
    on_v4.state_chunk_map_id = hybrid_state_chunk_map_id_v4();
    assert!(on_v4.validate_shape().unwrap_err().to_string().contains("spells its key-head count"));
    // On no map: the genesis-anchored hybrid (graph-v3's shape) is fine at version 3.
    let mut unmapped = v8.clone();
    unmapped.state_chunk_map_id = Hash64::default();
    unmapped.n_ctx = 8;
    unmapped.validate_shape().expect("a V3 hybrid with no map");
    // A conv row that is not the window `2·kh·k + heads·v`.
    let mut wrong_conv = v8.clone();
    let step = kaspa_consensus_core::palw_step::kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let node = wrong_conv.gdn_nodes.iter().position(|n| n.kernel_semantics_id == step).expect("the recurrence node");
    let conv_ref = wrong_conv.gdn_nodes[node].input_refs[1] as usize;
    wrong_conv.gdn_nodes[conv_ref].out_len = PalwStepOutLenV1::Fixed { elements: (2 * 32 + 32) * 4 };
    assert!(palw_gdn_key_heads_v1(&wrong_conv).unwrap_err().to_string().contains("convolution row"), "the v2 window at 16/32");
    // Value heads that do not group over the key heads.
    let mut ragged = v8.clone();
    ragged.gdn_heads = 24;
    assert!(palw_gdn_key_heads_v1(&ragged).is_err());
    // No recurrence at all: a dense row at version 3 has no key-head count to derive.
    let mut dense = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5(
        kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 32,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        },
    )
    .expect("a dense graph-v5 row");
    dense.validate_shape().expect("at version 1");
    dense.version = PALW_STEP_OBJECT_VERSION_V3;
    assert!(dense.validate_shape().unwrap_err().to_string().contains("no GDN layer"));
}

/// **The court's slice reads the same derivation** — at 16/32, value head 20 reads key head
/// `20 % 16 = 4` and the value block is the conv row's tail past `2·16·4`.
#[test]
fn the_courts_gdn_slice_reads_the_one_derivation() {
    let v8 = qwen36_profile_v8(tiny(16, 32, 4, 64)).expect("graph-v8");
    let step = kaspa_consensus_core::palw_step::kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let node = v8.gdn_nodes.iter().find(|n| n.kernel_semantics_id == step).expect("the recurrence node");
    let width = |ordinal: usize| match v8.gdn_nodes[node.input_refs[ordinal] as usize].out_len {
        PalwStepOutLenV1::Fixed { elements } => elements as u64,
        PalwStepOutLenV1::KvScaled { .. } => unreachable!(),
    };
    let widths: Vec<u64> = (0..5).map(width).collect();
    assert_eq!(widths[0], 16 * 4, "the key row is sixteen key heads");
    assert_eq!(widths[1], 2 * 16 * 4 + 32 * 4, "the window is 2·kh·k + heads·v");
    for (vh, kh) in [(3u32, 3u64), (15, 15), (16, 0), (20, 4), (31, 15)] {
        let slices = qwen36_gdn_slice_v1(&v8, node, &widths, vh).expect("a head slice");
        assert_eq!(slices[0], (kh * 4, kh * 4 + 4), "head {vh} reads key head {kh}");
        assert_eq!(slices[2], (kh * 4, kh * 4 + 4), "and the query of key head {kh}");
        let v_block = 2 * 16 * 4;
        assert_eq!(slices[1], (v_block + vh as u64 * 4, v_block + (vh as u64 + 1) * 4), "head {vh}'s value lanes");
    }
}
