//! Source controls for the checkpoint terminals (RFC-0006 consumer v2). Written for the consolidated validation phase; not run here
//! and not acceptance results.
use super::*;
use crate::palw_step_leg::{PalwStepLegBuilderV1, step_opening_v1};
use crate::palw_step_refute::tests as fixtures;

const MAX: u64 = leg::PALW_STEP_LEG_MAX_LEAVES;

/// The fixture's GatedDeltaNet node (pre slot 0, then the recurrence table's six nodes; the core is the sixth).
const GDN_SLOT: u32 = 6;

/// The step tile at `coord` of the fixture's honest rows (position ordinals: prefill 0, 1, then decode call 1).
fn tile(rows: &[Vec<Vec<u32>>], coord: PalwStepCoordinateV1) -> leg::PalwStepTileLeafV1 {
    let ord = if coord.call_index == 0 { coord.position as usize } else { 2 };
    let row = &rows[ord][coord.node_slot as usize];
    let start = coord.tile_index as usize * 16;
    let end = (start + 16).min(row.len());
    leg::PalwStepTileLeafV1 {
        version: leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        coord,
        value_count: (end - start) as u32,
        values_le: row[start..end].iter().flat_map(|v| v.to_le_bytes()).collect(),
    }
}

/// The honest recurrence state after every one of the fixture's three positions — the long form, from the genesis zero state.
fn honest_state(profile: &crate::palw_step::PalwShapeProfileV3, rows: &[Vec<Vec<u32>>]) -> Vec<Vec<u32>> {
    let heads = profile.gdn_heads as usize;
    let mut inputs = Vec::new();
    for row in rows.iter().take(3) {
        inputs.extend([row[1].clone(), row[2].clone(), row[3].clone(), row[4][..heads].to_vec(), row[5][..heads].to_vec()]);
    }
    gdn_core_anchored_replay_v1(profile, None, &inputs, DotStructure::Step16Epr4).expect("the long form replays").state
}

/// **A solo gdn v2 class with ONE checkpoint after decode call 1**, its delta state the honest replay (or a forged one), its
/// convolution windows zero. Returns the transition accusation of that checkpoint, assembled the way a holder assembles it.
fn transition_fixture(forge: bool) -> PalwGdnTransitionRefutationV2 {
    let (mut b, _, rows) = fixtures::honest_execution();
    b.shape_profile.state_chunk_map_id = maps::gdn_state_chunk_map_id_v2();
    b.state_chunk_map_id = b.shape_profile.state_chunk_map_id;
    b.job_context.shape_profile_id = b.shape_profile.shape_profile_id();
    b.checkpoint_profile.checkpoint_interval = 1;
    // The step leg again, under the class the binding now names (a step leaf binds the profile hash).
    let mut builder = PalwStepLegBuilderV1::new(b.job_context.clone(), b.shape_profile.clone()).unwrap();
    for index in 0..builder.expected_main_leaves() {
        let coord = crate::palw_step::canonical_step_coordinates(&b.shape_profile, &b.job_context, index).unwrap();
        let t = tile(&rows, coord);
        let values: Vec<u32> = t.values_le.chunks_exact(4).map(|q| u32::from_le_bytes([q[0], q[1], q[2], q[3]])).collect();
        builder.push_step_tile(coord, &values).unwrap();
    }
    let material = builder.finish().unwrap();
    b.step_leaf_count = material.leaf_count;
    b.step_merkle_root = material.merkle_root;
    // The checkpoint: delta chunks from the honest (or forged) state, convolution windows zero.
    let geometry = PalwGdnStateGeometryV2::of(&b.shape_profile).unwrap();
    let mut state = honest_state(&b.shape_profile, &rows);
    if forge {
        state[1][7] ^= 1 << 12;
    }
    let chunks: Vec<Vec<u8>> = (0..geometry.chunk_count())
        .map(|index| {
            let entry = geometry.entry(index).unwrap();
            match entry.kind {
                crate::palw_gdn_state_map_v2::PalwGdnStateChunkKindV2::Delta => {
                    let k = geometry.k_dim as usize;
                    let from = entry.row_start as usize * k;
                    let to = from + entry.row_count as usize * k;
                    state[entry.head as usize][from..to].iter().flat_map(|w| w.to_le_bytes()).collect()
                }
                crate::palw_gdn_state_map_v2::PalwGdnStateChunkKindV2::Conv => vec![0u8; entry.byte_len() as usize],
            }
        })
        .collect();
    let hashes: Vec<Hash64> =
        chunks.iter().enumerate().map(|(i, c)| leg::state_chunk_leaf_hash_v1(&b.state_chunk_map_id, i as u32, c)).collect();
    let context_hash = b.job_context.context_hash();
    let leaf = leg::PalwCheckpointLeafV2 {
        version: leg::PALW_STEP_LEG_OBJECT_VERSION_V1,
        checkpoint_index: 0,
        covered_decode_call: 1,
        prev_checkpoint_leaf_hash: leg::checkpoint_genesis_prev_v2(&context_hash),
        state_chunk_count: chunks.len() as u32,
        state_chunks_root: leg::state_chunks_root_v1(&hashes).unwrap(),
    };
    let leaf_hash = leg::checkpoint_leaf_hash_v2(&context_hash, &b.checkpoint_profile.profile_hash(), &b.state_chunk_map_id, &leaf);
    let opening = step_opening_v1(&[leaf_hash], 0).unwrap();
    b.checkpoint_count = 1;
    b.checkpoint_merkle_root = leg::step_opening_root_capped_v1(1, &opening, MAX).unwrap();
    fixtures::rebind_committed_root(&mut b);
    let leaf_hashes = [leaf_hash];
    let capture = super::super::capture::PalwGdnCaptureV2 {
        binding: &b,
        step_leaf_hashes: &material.leaf_hashes,
        checkpoint_leaf_hashes: &leaf_hashes,
        max_leaves: MAX,
    };
    gdn_transition_refutation_from_capture_v2(&capture, GDN_SLOT, 0, 0, &|coord| Some(tile(&rows, coord)), &|index| {
        (index == 0).then(|| (leaf.clone(), chunks.clone()))
    })
    .expect("the holder assembles the transition evidence")
}

fn check(r: &PalwGdnTransitionRefutationV2) -> Result<PalwGdnCheckpointVerdictV2, PalwGdnCourtErrorV2> {
    check_gdn_transition_refutation_v2(
        r,
        r.binding.committed_execution_root,
        r.binding.shape_profile.shape_profile_id(),
        &PalwNoWeightsV1,
        MAX,
    )
}

/// **Honest long-versus-anchored parity at the checkpoint itself**: an honest first checkpoint IS the genesis replay over its window
/// (no fault), and a forged delta word is convicted as a `DeltaState` mismatch at exactly that head/row/lane.
#[test]
fn an_honest_first_checkpoint_is_its_genesis_replay_and_a_forged_one_convicts() {
    let honest = transition_fixture(false);
    assert!(honest.predecessor.is_none(), "the first checkpoint stands on the genesis zero state");
    assert!(matches!(check(&honest), Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))));
    let forged = transition_fixture(true);
    let verdict = check(&forged).expect("a forged checkpoint state convicts");
    assert_eq!(verdict.mismatch, PalwGdnMismatchV2::DeltaState { head: 1, row: 0, lane: 7 });
    assert_eq!(verdict.verdict.fault, PalwStepFaultV1::ComputationMismatch { value_index: 256 + 7 });
}

/// **Changed / foreign roots, wrong index / cadence / head, truncated and duplicated chunks**: each is refused as evidence (never
/// a conviction, never an acquittal by default).
#[test]
fn malformed_transition_evidence_is_refused_not_adjudicated() {
    let r = transition_fixture(true);
    let refused = |r: &PalwGdnTransitionRefutationV2| matches!(check(r), Err(PalwGdnCourtErrorV2::Evidence(e)) if e != PalwStepRefuteError::NoFaultFound);
    let mut foreign = r.clone();
    foreign.binding.checkpoint_merkle_root = Hash64::from_bytes([77; 64]);
    assert!(check(&foreign).is_err(), "a foreign checkpoint root no longer pins the binding to the claim");
    let mut changed = r.clone();
    changed.accused.chunks[0].bytes[0] ^= 1;
    assert!(refused(&changed), "changed chunk bytes are not members of the checkpoint");
    let mut wrong_cadence = r.clone();
    wrong_cadence.accused.leaf.covered_decode_call = 2;
    assert!(refused(&wrong_cadence), "a covered counter that is not the index's cadence");
    let mut wrong_index = r.clone();
    wrong_index.accused.leaf.checkpoint_index = 1;
    assert!(check(&wrong_index).is_err(), "an index past the committed checkpoint leg");
    let mut truncated = r.clone();
    truncated.accused.chunks.pop();
    assert!(refused(&truncated), "a missing selected chunk");
    let mut duplicated = r.clone();
    duplicated.accused.chunks.push(duplicated.accused.chunks[0].clone());
    assert!(refused(&duplicated), "a duplicated chunk");
    let mut wrong_head = r.clone();
    wrong_head.head = 1;
    assert!(refused(&wrong_head), "the fused core's selection is every head: head must be 0");
    let mut wrong_layer = r.clone();
    wrong_layer.node_slot = 0;
    assert!(check(&wrong_layer).is_err(), "the embedding slot is not a recurrence layer's");
    let mut stray_predecessor = r.clone();
    stray_predecessor.predecessor = Some(r.accused.clone());
    assert!(refused(&stray_predecessor), "the first checkpoint carries no predecessor");
    let mut short_inputs = r.clone();
    short_inputs.inputs.pop();
    assert!(refused(&short_inputs), "the window rows are exactly the canonical set");
    assert!(
        check_gdn_transition_refutation_v2(&r, Hash64::default(), r.binding.shape_profile.shape_profile_id(), &PalwNoWeightsV1, MAX)
            .is_err(),
        "another claim's execution root"
    );
}

/// **A count that is not the registered layout's is the executor's shape fault**, decided from the authenticated leaf alone.
#[test]
fn a_checkpoint_whose_count_is_not_the_layout_is_a_shape_fault() {
    let mut r = transition_fixture(false);
    // Re-commit the leaf with one extra (empty) chunk in its declared count, opening and root re-derived so it authenticates.
    let mut leaf = r.accused.leaf.clone();
    leaf.state_chunk_count += 1;
    let context_hash = r.binding.job_context.context_hash();
    let leaf_hash = leg::checkpoint_leaf_hash_v2(
        &context_hash,
        &r.binding.checkpoint_profile.profile_hash(),
        &r.binding.state_chunk_map_id,
        &leaf,
    );
    let opening = step_opening_v1(&[leaf_hash], 0).unwrap();
    r.binding.checkpoint_merkle_root = leg::step_opening_root_capped_v1(1, &opening, MAX).unwrap();
    fixtures::rebind_committed_root(&mut r.binding);
    r.accused.leaf = leaf;
    r.accused.opening = opening;
    let verdict = check(&r).expect("the shape fault convicts");
    assert!(matches!(verdict.mismatch, PalwGdnMismatchV2::CheckpointShape { declared: 5, derived: 4 }));
}

/// The convolution-window terminal serves only a layer whose convolution is the Q36 node: the float fixture has none, and is
/// refused by name rather than compared against rows it does not commit.
#[test]
fn the_conv_window_terminal_refuses_a_layer_without_a_q36_convolution() {
    let r = transition_fixture(false);
    let conv = PalwGdnConvWindowRefutationV2 {
        version: PALW_GDN_CHECKPOINT_EVIDENCE_VERSION_V2,
        binding: r.binding.clone(),
        conv_slot: 1,
        head: 0,
        accused: r.accused.clone(),
        inputs: Vec::new(),
    };
    assert!(
        check_gdn_conv_window_refutation_v2(
            &conv,
            conv.binding.committed_execution_root,
            conv.binding.shape_profile.shape_profile_id(),
            MAX
        )
        .is_err()
    );
}

/// **Flags-off historical bytes**: the v2 count helper answers nothing for every historical map the legacy dispatcher owns, so the
/// round-4 structural verdicts are untouched below consumer v2.
#[test]
fn the_v2_checkpoint_count_reads_only_the_maps_consumer_v2_owns() {
    let r = transition_fixture(false);
    let mut p = r.binding.shape_profile.clone();
    assert_eq!(super::super::gdn_checkpoint_chunk_count_v2(&p, 3), Some(4));
    for map in [
        maps::hybrid_state_chunk_map_id_v1(),
        maps::hybrid_state_chunk_map_id_v2(),
        maps::hybrid_state_chunk_map_id_v3(),
        maps::integer_kv_state_chunk_map_id_v2(),
        maps::gdn_state_chunk_map_id_v3(),
        Hash64::default(),
    ] {
        p.state_chunk_map_id = map;
        assert_eq!(super::super::gdn_checkpoint_chunk_count_v2(&p, 3), None, "{map}");
    }
}

/// **The held route, consensus half**: the demands are exactly the accused witness's chunks, as held-DA `StateChunk` units of the
/// accused checkpoint; a witness rebuilt from held-DA answers carrying the producer's bytes and paths is the assembled witness, and
/// the accusation built on it convicts identically. Answers that are short, of another unit, or under two different checkpoint
/// leaves are refused.
#[test]
fn the_held_da_route_rebuilds_the_same_transition_accusation() {
    use crate::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnChunkOpeningV1};
    use crate::palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1};
    let r = transition_fixture(true);
    let demands = gdn_transition_demands_v2(&r.binding, GDN_SLOT, 0, 0).expect("the transition's units");
    assert_eq!(demands.len(), r.accused.chunks.len());
    for (d, c) in demands.iter().zip(&r.accused.chunks) {
        assert_eq!(*d, PalwHeldMissingV1::StateChunk { checkpoint: 0, chunk: c.chunk_index });
    }
    let anchor = PalwAttnCheckpointAnchorV1 { leaf: r.accused.leaf.clone(), opening: r.accused.opening.clone() };
    let answers: Vec<PalwHeldDisclosureV1> = r
        .accused
        .chunks
        .iter()
        .map(|c| PalwHeldDisclosureV1::StateChunk {
            anchor: anchor.clone(),
            chunk: PalwAttnChunkOpeningV1 { chunk_index: c.chunk_index, chunk_bytes: c.bytes.clone(), siblings: c.siblings.clone() },
        })
        .collect();
    let rebuilt = gdn_checkpoint_witness_from_disclosures_v2(&demands, &answers).expect("the answers rebuild the witness");
    assert_eq!(rebuilt, r.accused);
    let mut via = r.clone();
    via.accused = rebuilt;
    assert_eq!(check(&via).expect("convicts").mismatch, check(&r).expect("convicts").mismatch);
    // Refusals.
    assert!(gdn_checkpoint_witness_from_disclosures_v2(&demands[..demands.len() - 1], &answers).is_err(), "one answer per unit");
    let mut other_unit = demands.clone();
    other_unit[0] = PalwHeldMissingV1::StateChunk { checkpoint: 0, chunk: u32::MAX };
    assert!(gdn_checkpoint_witness_from_disclosures_v2(&other_unit, &answers).is_err(), "an answer of another unit");
    if answers.len() > 1 {
        let mut split = answers.clone();
        if let PalwHeldDisclosureV1::StateChunk { anchor, .. } = &mut split[1] {
            anchor.leaf.state_chunk_count += 1;
        }
        assert!(gdn_checkpoint_witness_from_disclosures_v2(&demands, &split).is_err(), "answers under two different leaves");
    }
}
