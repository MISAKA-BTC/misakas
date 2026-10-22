//! Source controls for the consolidated validation phase; not acceptance results.
use super::*;
use crate::palw_step_leg::{PalwStepLegBuilderV1, checkpoint_empty_root_v2, step_opening_v1};
use crate::palw_step_refute::tests as fixtures;

fn mapped_binding(interval: u32) -> leg::PalwStepBindingV2 {
    let mut b = fixtures::skeleton_refutation().binding;
    b.shape_profile.state_chunk_map_id = maps::gdn_state_chunk_map_id_v2();
    b.state_chunk_map_id = b.shape_profile.state_chunk_map_id;
    b.job_context.shape_profile_id = b.shape_profile.shape_profile_id();
    b.job_context.exact_decode_tokens = 4;
    b.checkpoint_profile.checkpoint_interval = interval;
    b.checkpoint_count = 3 / interval;
    b
}

fn coord(call: u32, position: u32) -> PalwStepCoordinateV1 {
    PalwStepCoordinateV1 { call_index: call, node_slot: 6, position, tile_index: 0 }
}

fn anchored_fixture() -> (leg::PalwStepBindingV2, PalwGdnCheckpointWitnessV2) {
    let mut binding = mapped_binding(1);
    let geometry = PalwGdnStateGeometryV2::of(&binding.shape_profile).unwrap();
    let chunks: Vec<Vec<u8>> = (0..geometry.chunk_count())
        .map(|index| {
            let entry = geometry.entry(index).unwrap();
            std::iter::repeat_n((index as u32 + 1).to_le_bytes(), entry.byte_len() as usize / 4).flatten().collect()
        })
        .collect();
    let hashes: Vec<_> = chunks
        .iter()
        .enumerate()
        .map(|(index, bytes)| leg::state_chunk_leaf_hash_v1(&binding.state_chunk_map_id, index as u32, bytes))
        .collect();
    let leaf = leg::PalwCheckpointLeafV2 {
        version: leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        checkpoint_index: 0,
        covered_decode_call: 1,
        prev_checkpoint_leaf_hash: leg::checkpoint_genesis_prev_v2(&binding.job_context.context_hash()),
        state_chunk_count: chunks.len() as u32,
        state_chunks_root: leg::state_chunks_root_v1(&hashes).unwrap(),
    };
    let checkpoint_hash = leg::checkpoint_leaf_hash_v2(
        &binding.job_context.context_hash(),
        &binding.checkpoint_profile.profile_hash(),
        &binding.state_chunk_map_id,
        &leaf,
    );
    let tree = [checkpoint_hash, Hash64::from_bytes([9; 64]), Hash64::from_bytes([10; 64])];
    let opening = step_opening_v1(&tree, 0).unwrap();
    binding.checkpoint_merkle_root = leg::step_opening_root_capped_v1(3, &opening, leg::PALW_STEP_LEG_MAX_LEAVES).unwrap();
    let selected = geometry.delta_chunk_index(0, 1, 0).unwrap() as usize;
    let witness = PalwGdnStateChunkWitnessV2 {
        chunk_index: selected as u32,
        bytes: chunks[selected].clone(),
        siblings: leg::state_chunk_path_v1(&hashes, selected).unwrap(),
    };
    fixtures::rebind_committed_root(&mut binding);
    (binding, PalwGdnCheckpointWitnessV2 { leaf, opening, chunks: vec![witness] })
}

#[test]
fn recurrence_anchor_is_before_the_current_step_not_kvs_after_write_checkpoint() {
    let mut b = mapped_binding(1);
    let before = gdn_replay_start_v2(&b, &coord(2, 0)).unwrap();
    assert_eq!(before, PalwGdnReplayStartV2 { first_position: 3, checkpoint_index: Some(0), covered: Some(1) });
    assert_eq!(gdn_replay_start_v2(&b, &coord(1, 0)).unwrap().checkpoint_index, None);
    b.shape_profile.state_chunk_map_id = maps::hybrid_state_chunk_map_id_v3();
    b.state_chunk_map_id = b.shape_profile.state_chunk_map_id;
    b.job_context.declared_prefill_tokens = 32;
    let out = coord(0, 16);
    let recurrence = gdn_replay_start_v2(&b, &out).unwrap();
    assert_eq!(recurrence.first_position, 16);
    assert_eq!(recurrence.checkpoint_index, Some(15));
    assert_eq!(cadence::palw_checkpoint_covered_for_step_v1(&b.shape_profile, &b.job_context, 0, 16), Some(17));
    assert_eq!(gdn_replay_start_v2(&b, &coord(0, 0)).unwrap().checkpoint_index, None);
}

#[test]
fn selected_head_bytes_are_authenticated_and_no_extra_chunk_is_ignored() {
    let (b, witness) = anchored_fixture();
    let out = coord(2, 0);
    let start = gdn_replay_start_v2(&b, &out).unwrap();
    let verify = |w: &PalwGdnCheckpointWitnessV2| {
        authenticate_anchor_v2(&b, &out, start, Some(w), leg::PALW_STEP_LEG_MAX_LEAVES, Some(1))
            .map(|anchor| anchor.unwrap().delta_rows())
    };
    let rows = verify(&witness).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], vec![2; b.shape_profile.gdn_head_k_dim as usize * b.shape_profile.gdn_head_v_dim as usize]);
    let mut altered = witness.clone();
    altered.chunks[0].bytes[0] ^= 1;
    assert!(verify(&altered).is_err());
    let mut another_head = witness.clone();
    another_head.chunks[0].chunk_index -= 1;
    assert!(verify(&another_head).is_err());
    let mut duplicate = witness.clone();
    duplicate.chunks.push(duplicate.chunks[0].clone());
    assert!(verify(&duplicate).is_err());
    let mut truncated = witness.clone();
    truncated.chunks[0].bytes.pop();
    assert!(verify(&truncated).is_err());
    let mut future = witness.clone();
    future.leaf.covered_decode_call += 1;
    assert!(verify(&future).is_err());
    let mut foreign = b.clone();
    foreign.checkpoint_merkle_root = Hash64::from_bytes([88; 64]);
    assert!(authenticate_anchor_v2(&foreign, &out, start, Some(&witness), leg::PALW_STEP_LEG_MAX_LEAVES, Some(1)).is_err());
    assert!(authenticate_anchor_v2(&b, &out, start, None, leg::PALW_STEP_LEG_MAX_LEAVES, Some(1)).is_err());
}

#[test]
fn genesis_and_legacy_hybrid_missing_serializer_are_explicit() {
    let (mut b, witness) = anchored_fixture();
    let out = coord(0, 0);
    let start = gdn_replay_start_v2(&b, &out).unwrap();
    assert!(authenticate_anchor_v2(&b, &out, start, None, leg::PALW_STEP_LEG_MAX_LEAVES, Some(0)).unwrap().is_none());
    assert!(authenticate_anchor_v2(&b, &out, start, Some(&witness), leg::PALW_STEP_LEG_MAX_LEAVES, Some(0)).is_err());
    b.shape_profile.state_chunk_map_id = maps::hybrid_state_chunk_map_id_v1();
    assert!(gdn_replay_start_v2(&b, &out).is_err());
    b.shape_profile.state_chunk_map_id = maps::hybrid_state_chunk_map_id_v2();
    assert!(gdn_replay_start_v2(&b, &out).is_err());
}

pub(crate) fn genesis_evidence(fault: bool) -> PalwGdnStepRefutationV2 {
    let (mut b, _, mut rows) = fixtures::honest_execution();
    b.shape_profile.state_chunk_map_id = maps::gdn_state_chunk_map_id_v2();
    b.state_chunk_map_id = b.shape_profile.state_chunk_map_id;
    b.job_context.shape_profile_id = b.shape_profile.shape_profile_id();
    if fault {
        rows[2][6][3] ^= 1 << 20;
    }
    let mut builder = PalwStepLegBuilderV1::new(b.job_context.clone(), b.shape_profile.clone()).unwrap();
    for index in 0..builder.expected_main_leaves() {
        let coord = crate::palw_step::canonical_step_coordinates(&b.shape_profile, &b.job_context, index).unwrap();
        let ordinal = if coord.call_index == 0 { coord.position as usize } else { 2 };
        let row = &rows[ordinal][coord.node_slot as usize];
        let start = coord.tile_index as usize * 16;
        builder.push_step_tile(coord, &row[start..(start + 16).min(row.len())]).unwrap();
    }
    let material = builder.finish().unwrap();
    b.step_leaf_count = material.leaf_count;
    b.step_merkle_root = material.merkle_root;
    b.checkpoint_merkle_root = checkpoint_empty_root_v2(&b.job_context.context_hash());
    fixtures::rebind_committed_root(&mut b);
    PalwGdnStepRefutationV2 {
        version: PALW_GDN_STEP_EVIDENCE_VERSION_V2,
        step: fixtures::build_refutation(&b, &material, &rows, coord(1, 0)),
        checkpoint: None,
    }
}

#[test]
fn versioned_genesis_terminal_agrees_with_legacy_arithmetic_for_honest_and_committed_wrong_rows() {
    for wrong in [false, true] {
        let r = genesis_evidence(wrong);
        let old = check_execution_step_refutation_v1(&r.step, &PalwNoWeightsV1);
        let new = check_gdn_step_refutation_v2(
            &r,
            r.step.binding.committed_execution_root,
            r.step.binding.shape_profile.shape_profile_id(),
            &PalwNoWeightsV1,
            leg::PALW_STEP_LEG_MAX_LEAVES,
        );
        if wrong {
            assert_eq!(new.unwrap().fault, old.unwrap().fault);
        } else {
            assert_eq!(old, Err(PalwStepRefuteError::NoFaultFound));
            assert!(matches!(new, Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))));
        }
        assert!(
            check_gdn_step_refutation_v2(
                &r,
                Hash64::default(),
                r.step.binding.shape_profile.shape_profile_id(),
                &PalwNoWeightsV1,
                leg::PALW_STEP_LEG_MAX_LEAVES
            )
            .is_err()
        );
    }
}

#[test]
fn capture_builder_reuses_committed_tiles_and_preserves_the_genesis_terminal() {
    for wrong in [false, true] {
        let expected = genesis_evidence(wrong);
        let b = &expected.step.binding;
        let (_, _, mut rows) = fixtures::honest_execution();
        if wrong {
            rows[2][6][3] ^= 1 << 20;
        }
        let mut tiles = Vec::new();
        let mut hashes = Vec::new();
        let profile_id = b.shape_profile.shape_profile_id();
        for index in 0..b.step_leaf_count {
            let coordinate = crate::palw_step::canonical_step_coordinates(&b.shape_profile, &b.job_context, index).unwrap();
            let ordinal = if coordinate.call_index == 0 { coordinate.position as usize } else { 2 };
            let row = &rows[ordinal][coordinate.node_slot as usize];
            let first = coordinate.tile_index as usize * 16;
            let values = &row[first..(first + 16).min(row.len())];
            let leaf = leg::PalwStepTileLeafV1 {
                version: leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
                coord: coordinate,
                value_count: values.len() as u32,
                values_le: values.iter().flat_map(|v| v.to_le_bytes()).collect(),
            };
            hashes.push(leg::step_tile_leaf_hash_ctx_v1(&b.job_context, &profile_id, &leaf));
            tiles.push((index, leaf));
        }
        let target = expected.step.output_preimage.coord;
        let assemble = |tiles: &[(u64, leg::PalwStepTileLeafV1)], hashes: &[Hash64], chunks: &[Vec<u8>]| {
            gdn_refutation_from_leaves_v2(b, target, hashes, tiles, &[], &[], chunks, leg::PALW_STEP_LEG_MAX_LEAVES)
        };
        let assembled = assemble(&tiles, &hashes, &[]).unwrap();
        assert_eq!(*assembled, expected);
        assert!(assemble(&tiles, &hashes[..hashes.len() - 1], &[]).is_err());
        assert!(assemble(&tiles, &hashes, &[vec![0; 4]]).is_err());
        let mut foreign = hashes.clone();
        foreign[0] = Hash64::from_bytes([91; 64]);
        assert!(assemble(&tiles, &foreign, &[]).is_err());
        let mut duplicate = tiles.clone();
        duplicate.push(tiles.iter().find(|(i, _)| *i == expected.step.output_opening.leaf_index).unwrap().clone());
        assert!(assemble(&duplicate, &hashes, &[]).is_err());
        let mut altered = tiles.clone();
        altered.iter_mut().find(|(i, _)| *i == expected.step.output_opening.leaf_index).unwrap().1.values_le[0] ^= 1;
        assert!(assemble(&altered, &hashes, &[]).is_err());
        let verdict = check_gdn_step_refutation_v2(
            &assembled,
            b.committed_execution_root,
            profile_id,
            &PalwNoWeightsV1,
            leg::PALW_STEP_LEG_MAX_LEAVES,
        );
        assert_eq!(verdict.is_ok(), wrong);
    }
}
