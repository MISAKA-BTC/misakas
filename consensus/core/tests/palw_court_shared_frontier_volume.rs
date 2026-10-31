//! Codec/geometry lower bound, not measured model or network performance.
//! Applies ONLY if all canonical bottom StepNode branches may be requested
//! and must be served with the existing public frontier encoding.
#![allow(warnings)]
use kaspa_consensus_core::palw_tir_court_v1::{
    PALW_TIR_STEP_NODE_DEPTH_V1, palw_tir_step_node_frontier_v1, palw_tir_step_tree_height_v1, palw_tir_step_tree_width_v1,
};

#[test]
fn all_bottom_frontiers_partition_the_original_leaf_nodes() {
    for leaves in [2_u64, 3, 1024, 1025, 65_537, 2_000_000] {
        let height = palw_tir_step_tree_height_v1(leaves);
        let remainder = height % PALW_TIR_STEP_NODE_DEPTH_V1;
        let bottom = if remainder == 0 { PALW_TIR_STEP_NODE_DEPTH_V1 } else { remainder };
        let width = palw_tir_step_tree_width_v1(leaves, bottom).unwrap();
        let mut next = 0;
        for index in 0..width {
            let (level, first, end) = palw_tir_step_node_frontier_v1(leaves, bottom, index).unwrap();
            assert_eq!(level, 0);
            assert_eq!(first, next, "no overlap or gap, including a partial last node");
            assert!(end > first);
            next = end;
        }
        assert_eq!(next, leaves);
    }
}

#[test]
fn existing_frontier_codec_carries_each_distinct_hash_at_full_width() {
    use kaspa_consensus_core::{Hash64, palw_tir_public_source_v1::PalwTirPublicResponseV1};
    // Shape-only fixture: not an authenticated answer or a model-computation result.
    for count in [1_usize, 2, 1024] {
        let response = PalwTirPublicResponseV1::StepNode { frontier: vec![Hash64::default(); count], siblings: vec![] };
        let encoded = borsh::to_vec(&response).unwrap();
        // Variant byte, two Vec length prefixes, then every 64-byte frontier hash.
        assert_eq!(encoded.len(), 1 + 4 + count * 64 + 4);
    }
    // Conditional scenario, NOT an approved capacity or a ctx-to-leaf mapping.
    let leaves = 2_000_000_u64;
    let hash_bytes = leaves * 64;
    let scenario_bytes_per_daa = 12_500_u64;
    let daa_for_hashes_alone = hash_bytes.div_ceil(scenario_bytes_per_daa);
    assert_eq!((hash_bytes, daa_for_hashes_alone), (128_000_000, 10_240));
    assert!(daa_for_hashes_alone > 10_000);
}
