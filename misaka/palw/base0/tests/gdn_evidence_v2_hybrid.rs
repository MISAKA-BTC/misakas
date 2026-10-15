//! **RFC-0006 consumer v2 source control (written, not run here; not an acceptance result): the hybrid accuser's GDN evidence from
//! a real base0 capture.**
//!
//! The genesis hybrid shape — graph-v8: profile version 3 on the held composition whose recurrence half spells the key-head count
//! (v5), at the dev fixture's 2 key / 4 value heads — produces an honest capture; for EVERY recurrence step leaf the hybrid backend
//! assembles the anchored step evidence from that capture (`Qwen36Backend::gdn_step_evidence_v2`: the served tiles, the leg rebuilt
//! from the served leaves, the predecessor checkpoint recomputed by the accuser and authenticated against the committed leaf), and
//! the consensus court (`memory_v2::check_gdn_step_refutation_v2`) answers `NoFaultFound` on it — never an evidence refusal.
mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v8};
use kaspa_consensus_core::palw_step::{canonical_step_coordinates, kernel_semantics_id_v1};
use kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
use kaspa_consensus_core::palw_step_refute::memory_v2::{PalwGdnCourtErrorV2, check_gdn_step_refutation_v2, gdn_replay_start_v2};
use kaspa_consensus_core::palw_step_refute::{KDESC_Q36_GDN_STEP, PalwStepRefuteError};
use kaspa_hashes::Hash64;
use misaka_palw_base0::gdn_evidence_v2::Base0GdnEvidenceV2;

#[test]
fn every_recurrence_leaf_of_an_honest_held_v8_capture_yields_evidence_the_court_acquits() {
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let profile = qwen36_profile_v8(geometry).expect("graph-v8 (version 3, held v5) projects at the fixture geometry");
    let backend = qwen36_backend(&artifact, &profile, qwen36_held_canonical_v1(32)).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x6D_6E01)).expect("the anchor implies a job");
    let out = backend.execute(&palw_attempt_job_v1(canonical, true), &prompt).expect("the honest attempt runs");
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&out.material).expect("decodes");
    let binding = retention.binding().clone();

    // The production oracle: every inventory row proven against its own root.
    let inventory = misaka_palw_base0::inventory::qwen36_inventory_v1(&artifact, &profile).expect("the class yields an inventory");
    let openings: Vec<_> = (0..inventory.operands().len())
        .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
        .collect();
    let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root())
        .expect("every inventory row proves against its own root");

    let gdn_step = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let (mut judged, mut anchored) = (0u32, 0u32);
    for index in 0..binding.step_leaf_count {
        let coord = canonical_step_coordinates(&binding.shape_profile, &binding.job_context, index).expect("a main coordinate");
        let Some((node, Some(_))) = binding.shape_profile.resolve_node_slot(coord.node_slot) else { continue };
        if node.kernel_semantics_id != gdn_step {
            continue;
        }
        let start = gdn_replay_start_v2(&binding, &coord).expect("a held v5 recurrence leaf has a replay start");
        let evidence = backend.gdn_step_evidence_v2(&out.material, None, index).unwrap_or_else(|e| panic!("leaf {index}: {e}"));
        let Base0GdnEvidenceV2::Step(refutation) = evidence else {
            panic!("leaf {index}: an honest capture has no forged checkpoint");
        };
        assert_eq!(refutation.checkpoint.is_some(), start.checkpoint_index.is_some(), "leaf {index}: the anchor the court demands");
        if let (Some(witness), Some(at)) = (&refutation.checkpoint, start.checkpoint_index) {
            assert_eq!(witness.leaf.checkpoint_index, at, "leaf {index}: the exact predecessor checkpoint");
            anchored += 1;
        }
        let verdict = check_gdn_step_refutation_v2(
            &refutation,
            binding.committed_execution_root,
            profile.shape_profile_id(),
            &oracle,
            PALW_STEP_LEG_MAX_LEAVES,
        );
        assert!(
            matches!(verdict, Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))),
            "leaf {index}: an honest step is acquitted on its own evidence, got {verdict:?}"
        );
        judged += 1;
    }
    assert!(judged > 0, "the hybrid has recurrence step leaves");
    // Whether any leaf stands on a checkpoint depends on the derived recurrence spacing at this context; recorded, not assumed.
    let _ = anchored;
}

/// **The held-hybrid forged checkpoint, end to end** (the gap this route closes). A producer of the genesis held hybrid commits ONE
/// recurrence-carrying checkpoint whose selected delta chunk is not the job's state (its leg, checkpoint root and execution root
/// re-committed over the forged leaf, every later leaf re-chained). Then:
///
/// 1. the step evidence for a recurrence leaf standing on it names it `ForgedCheckpoint` (the accuser's own recompute disagrees);
/// 2. the accuser's demands are exactly the transition terminal's units, as held-DA `StateChunk` units of that checkpoint;
/// 3. the producer's answers (its own forged bytes and paths) pass the EXISTING held DA court's hash check
///    (`palw_held_da_check_disclosure_v1`) — no new object is involved;
/// 4. the transition evidence built from those answers convicts at consensus (`check_gdn_transition_refutation_v2`).
#[test]
fn a_forged_held_checkpoint_is_demanded_answered_and_convicted() {
    use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnChunkOpeningV1};
    use kaspa_consensus_core::palw_context_ladder::{palw_checkpoint_leaf_carries_recurrence_v1, palw_checkpoint_positions_at_v1};
    use kaspa_consensus_core::palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1, palw_held_da_check_disclosure_v1};
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use kaspa_consensus_core::palw_step_leg::{binding_commitment_root_v1, checkpoint_leaf_hash_v2, step_opening_capped_v1};
    use kaspa_consensus_core::palw_step_refute::memory_v2::{check_gdn_transition_refutation_v2, gdn_transition_demands_v2};
    use misaka_palw_base0::gdn_evidence_v2::{
        Base0GdnCheckpointCourtV2, base0_gdn_forged_checkpoint_demands_v2, base0_gdn_step_evidence_v2,
        base0_gdn_transition_evidence_from_disclosures_v2,
    };

    // The held v8 profile's recurrence spacing is one attention history tile (16 positions, the fused profile's), so the job must
    // checkpoint past it: at n_ctx 160 the canonical job is (19, 2), whose checkpoints cover positions 1..=19, within the fixture's
    // 32-position table (at n_ctx 32 the job is (3, 2) and its checkpoints cover 1..=3: none carries the recurrence).
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 160;
    let profile = qwen36_profile_v8(geometry).expect("graph-v8 projects");
    let backend = qwen36_backend(&artifact, &profile, qwen36_held_canonical_v1(160)).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x6D_6E02)).expect("a job");
    let out = backend.execute(&palw_attempt_job_v1(canonical, true), &prompt).expect("the honest attempt runs");
    let honest = backend.gdn_accuser_view_v2(&out.material, None).expect("the accuser's view of the served capture");
    let kernels = || backend.gdn_recompute_kernels_v2().expect("the registered plan");
    let (b, ctx) = (&honest.binding, &honest.binding.job_context);

    // The recurrence node and the first recurrence-carrying checkpoint.
    let gdn_step = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let node_slot = (0..u32::MAX)
        .take_while(|s| b.shape_profile.resolve_node_slot(*s).is_some())
        .find(|s| b.shape_profile.resolve_node_slot(*s).is_some_and(|(n, l)| l.is_some() && n.kernel_semantics_id == gdn_step))
        .expect("a recurrence step node");
    let (index, positions) = honest
        .checkpoint_leaves
        .iter()
        .map(|leaf| (leaf.checkpoint_index, palw_checkpoint_positions_at_v1(&b.shape_profile, ctx, leaf.covered_decode_call)))
        .find(|(_, positions)| palw_checkpoint_leaf_carries_recurrence_v1(&b.shape_profile, *positions))
        .unwrap_or_else(|| {
            let covered: Vec<(u32, u32)> = honest
                .checkpoint_leaves
                .iter()
                .map(|leaf| (leaf.checkpoint_index, palw_checkpoint_positions_at_v1(&b.shape_profile, ctx, leaf.covered_decode_call)))
                .collect();
            panic!(
                "the job reaches a recurrence-carrying checkpoint (spacing {}): (checkpoint, covered positions) = {covered:?}",
                kaspa_consensus_core::palw_context_ladder::palw_anchored_interval_for_profile_v1(&b.shape_profile)
            )
        });

    // The producer's forged state at that checkpoint: the honest chunks, one byte of the first demanded chunk flipped.
    let demands = gdn_transition_demands_v2(b, node_slot, 0, index).expect("the transition's units");
    let PalwHeldMissingV1::StateChunk { chunk: first, .. } = demands[0] else { panic!("state-chunk units") };
    let mut state = misaka_palw_base0::fp_recompute::base0_fp_recompute_state_at_covered_v1(
        &b.shape_profile,
        ctx,
        &honest.prompt_token_ids,
        &honest.output_token_ids,
        honest.checkpoint_leaves[index as usize].covered_decode_call,
        &mut kernels(),
        honest.prompt_ids_form,
    )
    .expect("the honest state");
    state.chunks[first as usize][0] ^= 0x01;
    let forged_root = misaka_palw_base0::fp_interval::base0_state_chunks_root_for_v1(&b.shape_profile, positions, &state.chunks)
        .expect("the forged state roots");

    // Re-commit the leg over the forged leaf (and re-chain every later leaf), then the binding.
    let mut forged = honest.clone();
    let (ctx_hash, profile_hash, map) =
        (ctx.context_hash(), forged.binding.checkpoint_profile.profile_hash(), forged.binding.state_chunk_map_id);
    forged.checkpoint_leaves[index as usize].state_chunks_root = forged_root;
    for j in index as usize + 1..forged.checkpoint_leaves.len() {
        let prev = checkpoint_leaf_hash_v2(&ctx_hash, &profile_hash, &map, &forged.checkpoint_leaves[j - 1]);
        forged.checkpoint_leaves[j].prev_checkpoint_leaf_hash = prev;
    }
    let leg = misaka_palw_base0::legs::Base0CheckpointCaptureV1::from_leaves_v1(
        ctx,
        &forged.binding.shape_profile,
        &forged.binding.checkpoint_profile,
        &forged.checkpoint_leaves,
    )
    .expect("the forged leg rebuilds");
    forged.binding.checkpoint_merkle_root = leg.merkle_root;
    forged.binding.committed_execution_root = binding_commitment_root_v1(&forged.binding);

    // 1. A recurrence leaf standing on the forged checkpoint is not adjudicated on it.
    let standing =
        (0..forged.binding.step_leaf_count).filter_map(|i| canonical_step_coordinates(&forged.binding.shape_profile, ctx, i)).find(
            |c| c.node_slot == node_slot && gdn_replay_start_v2(&forged.binding, c).is_ok_and(|s| s.checkpoint_index == Some(index)),
        );
    if let Some(coord) = standing {
        assert!(matches!(
            base0_gdn_step_evidence_v2(&forged.view(), coord, kernels),
            Ok(Base0GdnEvidenceV2::ForgedCheckpoint { index: at }) if at == index
        ));
    }

    // 2. The demands are the terminal's units.
    let asked = base0_gdn_forged_checkpoint_demands_v2(
        &forged.view(),
        Base0GdnCheckpointCourtV2::Transition { node_slot, head: 0 },
        index,
        kernels,
    )
    .expect("a forged checkpoint is demandable");
    assert_eq!(asked, demands);

    // 3. The producer's answers pass the existing held DA court.
    let opening = step_opening_capped_v1(&leg.leaf_hashes, u64::from(index), PALW_STEP_LEG_MAX_LEAVES).expect("an opening");
    let anchor = PalwAttnCheckpointAnchorV1 { leaf: forged.checkpoint_leaves[index as usize].clone(), opening };
    let answers: Vec<PalwHeldDisclosureV1> = demands
        .iter()
        .map(|d| {
            let PalwHeldMissingV1::StateChunk { chunk, .. } = *d else { unreachable!() };
            let siblings = kaspa_consensus_core::palw_state_chunk_map::palw_state_chunk_path_for_map_v1(
                &forged.binding.shape_profile,
                positions,
                &state.chunks,
                chunk,
            )
            .expect("the producer's path");
            PalwHeldDisclosureV1::StateChunk {
                anchor: anchor.clone(),
                chunk: PalwAttnChunkOpeningV1 { chunk_index: chunk, chunk_bytes: state.chunks[chunk as usize].clone(), siblings },
            }
        })
        .collect();
    for (d, a) in demands.iter().zip(&answers) {
        palw_held_da_check_disclosure_v1(
            &forged.binding.committed_execution_root,
            d,
            &forged.binding,
            a,
            PALW_STEP_LEG_MAX_LEAVES,
            PalwPromptIdsFormV1::MerkleV1,
        )
        .expect("the held DA court accepts the producer's own answer");
    }

    // 4. The evidence from the answers convicts.
    let refutation =
        base0_gdn_transition_evidence_from_disclosures_v2(&forged.view(), node_slot, 0, index, &demands, &answers, kernels)
            .expect("the transition evidence assembles from the answers");
    let inventory = misaka_palw_base0::inventory::qwen36_inventory_v1(&artifact, &profile).expect("an inventory");
    let openings: Vec<_> = (0..inventory.operands().len())
        .map(|i| kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), i as u32).unwrap())
        .collect();
    let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, inventory.root()).unwrap();
    let verdict = check_gdn_transition_refutation_v2(
        &refutation,
        forged.binding.committed_execution_root,
        profile.shape_profile_id(),
        &oracle,
        PALW_STEP_LEG_MAX_LEAVES,
    )
    .expect("the forged checkpoint convicts");
    assert!(matches!(
        verdict.mismatch,
        kaspa_consensus_core::palw_step_refute::memory_v2::PalwGdnMismatchV2::DeltaState { head: 0, .. }
    ));
}
