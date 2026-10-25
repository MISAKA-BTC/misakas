//! Native profile384/Core-canonical prefill draw, not a hand-extended binding.
//! The ordinary fixture's real prefill draw has one recurrence checkpoint.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeHeapV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
use kaspa_consensus_core::palw_step_refute::memory_v2::{
    PalwGdnCaptureV2, PalwGdnCheckpointWitnessV2, PalwGdnStateChunkWitnessV2, gdn_predecessor_witness_request_v3,
    gdn_transition_refutation_with_accused_retained_v3,
};

fn second_recurrence_case() -> (World, PalwGdnHeldCaseV1) {
    let w = world_with_fault_at_checkpoint(0, false, 1);
    assert_eq!(w.profile.n_ctx, 384);
    let canonical = qwen36_held_canonical_v1(w.profile.n_ctx);
    assert_eq!(kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_canonical_v1(&w.profile, false), Some(canonical));
    assert_eq!(w.honest.binding.job_context.declared_prefill_tokens, canonical.0);
    assert_eq!(w.honest.binding.job_context.exact_decode_tokens, 1);
    let mut case =
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, &w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
            .unwrap()
            .unwrap();
    assert_eq!(case.index, w.index, "own replay must discover the actual second forged checkpoint");
    assert_eq!(w.positions, 32);
    assert!(matches!(case.candidates[case.at], Base0GdnCheckpointCourtV2::Transition { head: 0, .. }));
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    case.demanded = Some((0, 110));
    (w, case)
}

#[test]
fn native_second_recurrence_has_real_predecessor_root_and_court_convicts() {
    let (w, case) = second_recurrence_case();
    let Base0GdnCheckpointCourtV2::Transition { node_slot, head } = case.candidates[case.at] else { panic!("transition") };
    let answers: Vec<_> = case.answers.iter().map(|a| (**a.as_ref().unwrap()).clone()).collect();
    let built = base0_gdn_transition_evidence_from_disclosures_retained_v4(
        &w.forged.view(),
        node_slot,
        head,
        case.index,
        &case.demands,
        &answers,
        || w.backend.gdn_recompute_kernels_v2().unwrap(),
    )
    .unwrap();
    let legacy = misaka_palw_base0::gdn_evidence_v2::base0_gdn_transition_evidence_from_disclosures_v2(
        &w.forged.view(),
        node_slot,
        head,
        case.index,
        &case.demands,
        &answers,
        || w.backend.gdn_recompute_kernels_v2().unwrap(),
    )
    .unwrap();
    assert_eq!(
        borsh::to_vec(&*built).unwrap(),
        borsh::to_vec(&legacy).unwrap(),
        "ownership changes neither existing native witness bytes nor serialized refutation/root meaning"
    );
    let predecessor = built.predecessor.as_ref().expect("this runtime control requires Some, not the genesis path");
    let positions = palw_checkpoint_positions_at_v1(&w.profile, &w.forged.binding.job_context, predecessor.leaf.covered_decode_call);
    assert_eq!(positions, 16);
    assert_eq!(predecessor.leaf, w.honest.checkpoint_leaves[predecessor.leaf.checkpoint_index as usize]);
    let mut kernels = w.backend.gdn_recompute_kernels_v2().unwrap();
    let replay = misaka_palw_base0::fp_recompute::base0_fp_recompute_state_at_covered_v1(
        &w.profile,
        &w.forged.binding.job_context,
        &w.forged.prompt_token_ids,
        &w.forged.output_token_ids,
        predecessor.leaf.covered_decode_call,
        &mut kernels,
        w.forged.prompt_ids_form,
    )
    .unwrap();
    assert_eq!(replay.state_chunks_root, predecessor.leaf.state_chunks_root);
    assert!(
        terminal_found_fault_v1(check_gdn_transition_refutation_v2(
            &built,
            case.claim.execution_root,
            case.claim.shape_profile_id,
            &w.oracle,
            w.forged.max_leaves
        ))
        .unwrap()
    );
}

fn copied_accused_heap(witness: &PalwGdnCheckpointWitnessV2) -> u64 {
    (witness.chunks.capacity() * std::mem::size_of::<PalwGdnStateChunkWitnessV2>()) as u64
        + (witness.opening.siblings.capacity() * std::mem::size_of::<Hash64>()) as u64
        + witness.leaf.native_heap_v1("predecessor test accused input").bytes().unwrap()
        + witness
            .chunks
            .iter()
            .map(|c| (c.bytes.capacity() + c.siblings.capacity() * std::mem::size_of::<Hash64>()) as u64)
            .sum::<u64>()
}

fn free(ceiling: u64) -> u64 {
    let (mut lo, mut hi) = (0, ceiling);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if reserve_native_requests_v1(mid).is_ok() { lo = mid } else { hi = mid - 1 }
    }
    lo
}

#[test]
fn actual_borrowed_predecessor_build_admission_and_input_to_output_owner_lifetime() {
    const CHILD: &str = "MISAKA_GDN_PREDECESSOR_COPY_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::actual_borrowed_predecessor_build_admission_and_input_to_output_owner_lifetime", module_path!());
        let name = name.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let (w, mut case) = second_recurrence_case();
    let Base0GdnCheckpointCourtV2::Transition { node_slot, head } = case.candidates[case.at] else { panic!("transition") };
    let answers: Vec<_> = case.answers.iter().map(|a| (**a.as_ref().unwrap()).clone()).collect();
    let native = base0_gdn_transition_evidence_from_disclosures_retained_v4(
        &w.forged.view(),
        node_slot,
        head,
        case.index,
        &case.demands,
        &answers,
        || w.backend.gdn_recompute_kernels_v2().unwrap(),
    )
    .unwrap();
    let mut corrupted = (*native).clone();
    corrupted.predecessor.as_mut().unwrap().chunks[0].bytes[0] ^= 1;
    assert!(
        terminal_found_fault_v1(check_gdn_transition_refutation_v2(
            &corrupted,
            case.claim.execution_root,
            case.claim.shape_profile_id,
            &w.oracle,
            w.forged.max_leaves
        ))
        .is_err(),
        "unauthenticated predecessor bytes are not a producer conviction or an honest result"
    );
    drop(corrupted);
    let predecessor = native.predecessor.as_ref().expect("actual non-genesis witness").clone();
    let accused = native.accused.clone();
    drop(native);
    let positions = palw_checkpoint_positions_at_v1(&w.profile, &w.forged.binding.job_context, predecessor.leaf.covered_decode_call);
    let mut kernels = w.backend.gdn_recompute_kernels_v2().unwrap();
    let replay = misaka_palw_base0::fp_recompute::base0_fp_recompute_state_at_covered_v1(
        &w.profile,
        &w.forged.binding.job_context,
        &w.forged.prompt_token_ids,
        &w.forged.output_token_ids,
        predecessor.leaf.covered_decode_call,
        &mut kernels,
        w.forged.prompt_ids_form,
    )
    .unwrap();
    assert_eq!(replay.state_chunks_root, predecessor.leaf.state_chunks_root);
    let leg = misaka_palw_base0::gdn_evidence_v2::base0_gdn_leg_v2(&w.forged.view()).unwrap();
    let capture = PalwGdnCaptureV2 {
        binding: &w.forged.binding,
        step_leaf_hashes: &w.forged.tiles.leaves,
        checkpoint_leaf_hashes: &leg.leaf_hashes,
        max_leaves: w.forged.max_leaves,
    };
    let layer = w.profile.resolve_node_slot(node_slot).unwrap().1.unwrap();
    let before = borsh::to_vec(&replay.chunks).unwrap();
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    // The quote (including its own metadata admission) has already succeeded.
    // Only now fill the remaining budget: build itself, not an earlier leg or
    // metadata constructor, must refuse before predecessor copies/tree work.
    let request = gdn_predecessor_witness_request_v3(
        &capture,
        predecessor.leaf.checkpoint_index,
        positions,
        layer,
        Some(head),
        &predecessor.leaf,
        &replay.chunks,
    )
    .unwrap();
    let (persistent, workspace) = request.requests_v3();
    assert!(persistent > 0 && workspace > 0);
    let blocker = reserve_native_requests_v1(free(ceiling)).unwrap();
    assert!(matches!(request.build_v3(), Err(PalwGdnCourtErrorV2::Resource(_))));
    assert_eq!(borsh::to_vec(&replay.chunks).unwrap(), before);
    drop(blocker);
    let built = gdn_predecessor_witness_request_v3(
        &capture,
        predecessor.leaf.checkpoint_index,
        positions,
        layer,
        Some(head),
        &predecessor.leaf,
        &replay.chunks,
    )
    .unwrap()
    .build_v3()
    .unwrap();
    assert_eq!(*built, predecessor, "same native selected bytes/order/paths/root");
    assert_eq!(free(ceiling), ceiling - persistent, "metadata/workspace retire; copied witness retains its own request");
    drop(built);
    assert_eq!(free(ceiling), ceiling);
    // Native typed callback failure propagates immediately; no later input
    // construction or string-only side channel may replace Resource.
    let tile_reads = std::cell::Cell::new(0);
    let callback_error = kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<u8>(
        "predecessor callback control",
        "targeted local capacity refusal",
    );
    let result = gdn_transition_refutation_with_accused_retained_v3(
        &capture,
        node_slot,
        head,
        case.index,
        accused,
        &|_| {
            tile_reads.set(tile_reads.get() + 1);
            None
        },
        &|_| Err(PalwGdnCourtErrorV2::Resource(callback_error.clone())),
    );
    assert!(matches!(result, Err(PalwGdnCourtErrorV2::Resource(found)) if found == callback_error));
    assert_eq!(tile_reads.get(), 0);
    assert_eq!(free(ceiling), ceiling);
    // The actual retained Base0 builder transports this additional owner into
    // the native court and signed output admission, alongside accused-copy.
    let built = base0_gdn_transition_evidence_from_disclosures_retained_v4(
        &w.forged.view(),
        node_slot,
        head,
        case.index,
        &case.demands,
        &answers,
        || w.backend.gdn_recompute_kernels_v2().unwrap(),
    )
    .unwrap();
    let accused_bytes = copied_accused_heap(&built.accused);
    assert_eq!(
        free(ceiling),
        ceiling - persistent - accused_bytes,
        "the predecessor request is additional to the independently retained accused copy"
    );
    let signs = std::cell::Cell::new(0);
    let signer = |m: &[u8], c: &[u8]| {
        assert!(
            free(ceiling) <= ceiling - persistent - accused_bytes,
            "both independent input owners still span signing and admitted output"
        );
        signs.set(signs.get() + 1);
        sign(m, c)
    };
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2),
        network_domain: Hash64::from_u64_word(0xD0),
        openings: &w.openings,
        oracle: &w.oracle,
        sign: &signer,
    };
    assert!(
        terminal_found_fault_v1(check_gdn_transition_refutation_v2(
            &built,
            case.claim.execution_root,
            case.claim.shape_profile_id,
            &w.oracle,
            w.forged.max_leaves
        ))
        .unwrap()
    );
    drop(built);
    assert_eq!(free(ceiling), ceiling);
    let before_case = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    let step = |case: &mut PalwGdnHeldCaseV1| {
        palw_gdn_held_step_retained_v2(
            case,
            &w.forged.view(),
            || w.backend.gdn_recompute_kernels_v2().unwrap(),
            &[],
            100,
            20,
            &accuser,
        )
    };
    assert!(
        matches!(step(&mut case), Err(PalwGdnHeldErrorV2::Resource(_))),
        "whole-caller retry control may refuse earlier; direct build admission was targeted separately above"
    );
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before_case);
    assert_eq!(signs.get(), 0);
    drop(blocker);
    let object = step(&mut case).unwrap();
    assert!(matches!(object, PalwGdnHeldNextRetainedV2::Accuse { .. }));
    assert_eq!(signs.get(), 1);
    assert!(free(ceiling) < ceiling, "independent signed output survives input owner retirement");
    drop(object);
    assert_eq!(free(ceiling), ceiling);
}
