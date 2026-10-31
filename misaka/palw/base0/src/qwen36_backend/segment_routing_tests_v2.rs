//! Real tiny Q36 capture, recurrence restore and segment replay with independent
//! reader/layer router bindings. Fixture plan ownership is not native approval.
use super::*;
use crate::fp_recompute::{Qwen36RecomputeKernelsV1, base0_fp_recompute_state_at_covered_v1};
use kaspa_consensus_core::{palw_reader_router_v2::*, palw_step::*, palw_step_refute::KDESC_Q36_ROUTER_TOPK};
use std::sync::Arc;

#[test]
fn segment_routing_q36_selected_reader_plan_replays_from_real_recurrence_checkpoint() {
    let (artifact, mut profile) = crate::fuzz_qwen36::tiny_class_v8_for_tests(2, 4, 32);
    // Fixture-only duplicate routers, retaining graph references and node order.
    for nodes in [&mut profile.gdn_nodes, &mut profile.attn_nodes] {
        let first = nodes.iter().position(|n| n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)).unwrap();
        let insert = first + 1;
        let router = nodes[first].clone();
        for node in nodes.iter_mut() {
            for input in &mut node.input_refs {
                if *input < PALW_STEP_INPUT_SENTINEL_MIN && usize::from(*input) >= insert {
                    *input += 1;
                }
            }
        }
        nodes.insert(insert, router);
    }
    profile.validate_shape().unwrap();
    let artifact = Arc::new(artifact);
    let historical = Qwen36Engine::new(&artifact).plan_from_profile(&profile).unwrap();
    let mut bindings = Vec::new();
    for reader_slot in 0..profile.global_node_count() {
        let (reader, layer) = profile.resolve_node_slot(reader_slot).unwrap();
        if !needs_reader_router_v2(reader) {
            continue;
        }
        let routers: Vec<_> = (0..reader_slot)
            .filter(|slot| {
                let (node, li) = profile.resolve_node_slot(*slot).unwrap();
                li == layer && node.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)
            })
            .collect();
        assert_eq!(routers.len(), 2);
        // A combine reader must name the immutable selected (first) router;
        // only non-combine readers are retargeted to the independent one.
        let combine =
            reader.kernel_semantics_id == kernel_semantics_id_v1(kaspa_consensus_core::palw_step_refute::KDESC_Q36_MOE_COMBINE);
        bindings.push(PalwReaderRouterBindingV2 {
            reader_slot,
            router_slot: routers[if combine { 0 } else { layer.unwrap_or(0) as usize % 2 }],
        });
    }
    let declaration = PalwReaderRouterDeclarationV2 {
        version: 2,
        class_id: profile.shape_profile_id(),
        artifact_root: Hash64::from_u64_word(44),
        bindings,
    };
    let native = historical.bind_reader_router_v2(&profile, &declaration, Hash64::from_u64_word(7)).unwrap();
    let mut backend = Qwen36Backend::with_class_profile(
        artifact.clone(),
        "Q36-segment-routing-fixture",
        (17, 4),
        profile.clone(),
        b"misaka-palw-test".to_vec(),
    );
    backend.plan = Some(native.plan);
    backend.native_routing_plan_lease_v2 = native.lease;
    let plan = backend.plan.as_ref().unwrap();
    let (job, prompt) = crate::produce::base0_rc_job_v1(
        &profile,
        Hash64::from_u64_word(0x534547),
        artifact.shape.vocab,
        17,
        4,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    );
    let run = qwen36_execute_for_attempt_v1(&artifact, &profile, plan, &job, &prompt).unwrap();
    let ids: Vec<_> = prompt.iter().map(|id| *id as u32).collect();
    let mut kernels = Qwen36RecomputeKernelsV1::new(&artifact, plan);
    let state = base0_fp_recompute_state_at_covered_v1(
        &profile,
        &job,
        &ids,
        &run.generated_token_ids,
        16,
        &mut kernels,
        backend.prompt_ids_form,
    )
    .unwrap();
    let mut anchor = crate::fp_interval::base0_checkpoint_operands_v1(&run.binding, &[], &run.checkpoints.leaves, 16).unwrap();
    assert_eq!(state.state_chunks_root, anchor.leaf.state_chunks_root);
    anchor.chunks = state.chunks;
    let capture = crate::produce::base0_material_encode_v1(&run).unwrap();
    let retention = crate::produce::base0_material_decode_any_v1(&capture).unwrap();
    let (seats, segment) = (3u16..=33)
        .find_map(|seats| {
            let k = kaspa_consensus_core::palw_verification_v2::palw_segment_count_v2(seats);
            (0..k)
                .find(|segment| {
                    let (start, _) = kaspa_consensus_core::palw_verification_v2::palw_segment_leaf_range_v2(
                        run.binding.step_leaf_count,
                        k,
                        *segment,
                    )
                    .unwrap();
                    let c = canonical_step_coordinates(&profile, &job, start).unwrap();
                    crate::fp_interval::Base0FpWindowV1::step_of_coordinate_v1(job.declared_prefill_tokens, c.call_index, c.position)
                        > 16
                })
                .map(|segment| (seats, segment))
        })
        .unwrap();
    let opening = crate::segment_opening::base0_segment_opening_v2(
        &retention,
        seats,
        segment,
        crate::segment_opening::Base0SegmentAnchorV1::Given(anchor),
        Some(&profile),
        backend.step_ladder_cap(),
    )
    .unwrap()
    .encode_v2()
    .unwrap();
    let claim = kaspa_consensus_core::palw_segment_resume_v1::PalwSegmentClaimV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        seat_count: seats,
        segment_index: segment,
    };
    let replay = backend.replay_segment_from_checkpoint_v1(&job, &prompt, &opening, claim).unwrap();
    assert!(replay.matches);
    assert!(replay.calls_replayed > 0);
    let mut wrong = job.clone();
    wrong.exact_decode_tokens += 1;
    assert!(
        backend.replay_segment_from_checkpoint_v1(&wrong, &prompt, &opening, claim).is_err(),
        "opening cannot substitute a count even when the original owner is None"
    );
}
