//! Witness-copy admission and actual native-court/output lifetime controls.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeDecodeErrorV1, NativeHeapV1, NativeRetainedV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1,
    reserve_native_requests_v1,
};
use kaspa_consensus_core::palw_step_refute::memory_v2::{
    PalwGdnCheckpointWitnessV2, PalwGdnStateChunkWitnessV2, gdn_checkpoint_witness_from_disclosures_v2,
};
use misaka_palw_base0::gdn_evidence_v2::base0_gdn_checkpoint_witness_retained_v4;

fn fixture(conv: bool) -> (World, PalwGdnHeldCaseV1, Vec<PalwHeldDisclosureV1>) {
    let w = world_with_fault(0, conv);
    let mut case =
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, &w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
            .unwrap()
            .unwrap();
    if conv {
        let at = case.candidates.iter().position(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { head: 0, .. })).unwrap();
        load_candidate_v1(&mut case, at, &w.forged.view(), &mut || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap();
    }
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    let answers = case.answers.iter().map(|a| (**a.as_ref().unwrap()).clone()).collect();
    (w, case, answers)
}
fn child(key: &str, name: &str) -> bool {
    if std::env::var(key).as_deref() == Ok("1") {
        return true;
    }
    let name = name.split_once("::").unwrap().1;
    assert!(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(key, "1")
            .status()
            .unwrap()
            .success()
    );
    false
}
fn arm(ceiling: u64) {
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
}
// Read the isolated logical ledger through admitted probes; each successful
// probe retires immediately. This measures tickets, not physical allocation.
fn free(ceiling: u64) -> u64 {
    let (mut lo, mut hi) = (0, ceiling);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if reserve_native_requests_v1(mid).is_ok() {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}
fn copied_heap(witness: &PalwGdnCheckpointWitnessV2) -> u64 {
    (witness.chunks.capacity() * std::mem::size_of::<PalwGdnStateChunkWitnessV2>()) as u64
        + (witness.opening.siblings.capacity() * std::mem::size_of::<Hash64>()) as u64
        + witness
            .chunks
            .iter()
            .map(|c| (c.bytes.capacity() + c.siblings.capacity() * std::mem::size_of::<Hash64>()) as u64)
            .sum::<u64>()
        + witness.leaf.native_heap_v1("witness test leaf").bytes().unwrap()
}

#[test]
fn real_borrowed_witness_copy_refuses_quota_before_clone_and_retains_source_and_output() {
    if !child(
        "MISAKA_GDN_WITNESS_COPY_CHILD",
        &format!("{}::real_borrowed_witness_copy_refuses_quota_before_clone_and_retains_source_and_output", module_path!()),
    ) {
        return;
    }
    let (_w, case, answers) = fixture(false);
    let before = borsh::to_vec(&answers).unwrap();
    let native = gdn_checkpoint_witness_from_disclosures_v2(&case.demands, &answers).unwrap();
    let bytes = copied_heap(&native);
    let mut foreign = answers.clone();
    let PalwHeldDisclosureV1::StateChunk { anchor, .. } = &mut foreign[0] else { panic!("state answer") };
    anchor.leaf.checkpoint_index += 1;
    let ceiling = 256 << 20;
    arm(ceiling);
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    assert!(matches!(base0_gdn_checkpoint_witness_retained_v4(&case.demands, &answers), Err(NativeDecodeErrorV1::Resource(_))));
    assert!(
        matches!(base0_gdn_checkpoint_witness_retained_v4(&case.demands, &foreign), Err(NativeDecodeErrorV1::Decode(_))),
        "borrowed identity is checked before capacity/copy"
    );
    assert_eq!(borsh::to_vec(&answers).unwrap(), before);
    drop(blocker);
    let owned = base0_gdn_checkpoint_witness_retained_v4(&case.demands, &answers).unwrap();
    assert_eq!(*owned, native);
    assert_eq!(free(ceiling), ceiling - bytes, "exact admitted clone destinations stay live");
    let built = owned.try_build_v4(Ok).unwrap();
    let output = built
        .try_handoff_v4(|witness| {
            assert_eq!(free(ceiling), ceiling - bytes);
            NativeRetainedV1::try_from_clone_v1(&witness)
        })
        .unwrap();
    assert!(free(ceiling) < ceiling, "independently admitted output remains after input guard retires");
    assert_eq!(*output, native);
    drop(output);
    assert_eq!(free(ceiling), ceiling);
    // A consumed value's Drop observes the input ticket before the generic
    // handoff releases it on an error. Generic R alone guarantees no output.
    struct DropCheck {
        witness: PalwGdnCheckpointWitnessV2,
        ceiling: u64,
    }
    impl Drop for DropCheck {
        fn drop(&mut self) {
            assert!(!self.witness.chunks.is_empty());
            assert!(free(self.ceiling) < self.ceiling, "input value retires before its ticket");
        }
    }
    let owned = base0_gdn_checkpoint_witness_retained_v4(&case.demands, &answers).unwrap();
    let built = owned.try_build_v4(|witness| Ok(DropCheck { witness, ceiling })).unwrap();
    let refused: Result<(), &str> = built.try_handoff_v4(|_value| Err("caller withholds"));
    assert!(refused.is_err());
    assert_eq!(free(ceiling), ceiling);
}

#[test]
fn actual_transition_and_conv_builders_keep_witness_ticket_through_native_court_and_signed_handoff() {
    if !child(
        "MISAKA_GDN_WITNESS_NATIVE_CHILD",
        &format!(
            "{}::actual_transition_and_conv_builders_keep_witness_ticket_through_native_court_and_signed_handoff",
            module_path!()
        ),
    ) {
        return;
    }
    let fixtures: Vec<_> = [false, true].into_iter().map(fixture).collect();
    let ceiling = 256 << 20;
    arm(ceiling);
    for (w, mut case, answers) in fixtures {
        let native = gdn_checkpoint_witness_from_disclosures_v2(&case.demands, &answers).unwrap();
        let bytes = copied_heap(&native);
        let built = match case.candidates[case.at] {
            Base0GdnCheckpointCourtV2::Transition { node_slot, head } => base0_gdn_transition_evidence_from_disclosures_retained_v4(
                &w.forged.view(),
                node_slot,
                head,
                case.index,
                &case.demands,
                &answers,
                || w.backend.gdn_recompute_kernels_v2().unwrap(),
            )
            .unwrap()
            .map_v4(|refutation| PalwMemoryRoutingEvidenceV2::GdnTransition { refutation }),
            Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, head } => base0_gdn_conv_window_evidence_from_disclosures_retained_v4(
                &w.forged.view(),
                conv_slot,
                head,
                case.index,
                &case.demands,
                &answers,
            )
            .unwrap()
            .map_v4(|refutation| PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation }),
        };
        assert_eq!(free(ceiling), ceiling - bytes, "real builder retains only the named witness-copy request after leg retirement");
        let signs = std::cell::Cell::new(0);
        let signer = |m: &[u8], c: &[u8]| {
            assert!(free(ceiling) < ceiling - bytes, "output preadmission and witness guard overlap at the actual signer");
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
        let object = built
            .try_handoff_v4(|evidence| {
                assert_eq!(free(ceiling), ceiling - bytes);
                let verdict = match &evidence {
                    PalwMemoryRoutingEvidenceV2::GdnTransition { refutation } => check_gdn_transition_refutation_v2(
                        refutation,
                        case.claim.execution_root,
                        case.claim.shape_profile_id,
                        &w.oracle,
                        w.forged.max_leaves,
                    ),
                    PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation } => check_gdn_conv_window_refutation_v2(
                        refutation,
                        case.claim.execution_root,
                        case.claim.shape_profile_id,
                        w.forged.max_leaves,
                    ),
                    _ => unreachable!(),
                };
                assert!(terminal_found_fault_v1(verdict)?);
                assert_eq!(free(ceiling), ceiling - bytes, "court scratch retires, input guard remains");
                super::super::action_owner::accusation(&case.claim, evidence, &accuser)
            })
            .unwrap();
        assert_eq!(signs.get(), 1);
        assert!(free(ceiling) < ceiling, "signed output keeps its independent ticket");
        drop(object);
        assert_eq!(free(ceiling), ceiling);
        // Drive the actual retained filer with the same authenticated material.
        let action = palw_gdn_held_step_retained_v2(
            &mut case,
            &w.forged.view(),
            || w.backend.gdn_recompute_kernels_v2().unwrap(),
            &[],
            100,
            20,
            &accuser,
        )
        .unwrap();
        assert!(matches!(action, PalwGdnHeldNextRetainedV2::Accuse { .. }));
        assert_eq!(signs.get(), 2);
        drop(action);
        assert_eq!(free(ceiling), ceiling);
    }
}
