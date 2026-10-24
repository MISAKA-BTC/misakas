//! Real transition/conv native court quota, after successful retained assembly.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};

#[test]
fn actual_transition_and_conv_court_capacity_is_typed_retry_without_case_advance_or_signature() {
    const CHILD: &str = "MISAKA_HELD_GDN_COURT_RESOURCE_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name =
            format!("{}::actual_transition_and_conv_court_capacity_is_typed_retry_without_case_advance_or_signature", module_path!());
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
    // Both genuine worlds/cases/disclosures are admitted before quota arming.
    // Convolution uses its actual first conv candidate, not substituted evidence.
    let fixtures: Vec<_> = [false, true]
        .into_iter()
        .map(|conv| {
            let w = world_with_fault(0, conv);
            let mut case = palw_gdn_held_open_retained_v3(claim_of(&w.forged, &w), &w.forged.view(), || {
                w.backend.gdn_recompute_kernels_v2().unwrap()
            })
            .unwrap()
            .unwrap();
            if conv {
                let at =
                    case.candidates.iter().position(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { head: 0, .. })).unwrap();
                load_candidate_v1(&mut case, at, &w.forged.view(), &mut || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap();
            }
            let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
            note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
            case.demanded = Some((0, 110));
            (w, case)
        })
        .collect();
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    for (w, mut case) in fixtures {
        let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
        // Native reserve_checkpoint_workspace_v2 includes a byte Vec for the
        // existing 1 MiB chunk cap plus 256; this 1 MiB free budget cannot fit
        // that court request, while the actual small upstream assembly can.
        let blocker = reserve_native_requests_v1(ceiling - (1 << 20)).unwrap();
        let batch = super::super::answer_owner::complete_batch(&case.answers).unwrap();
        let evidence = match case.candidates[case.at] {
            Base0GdnCheckpointCourtV2::Transition { node_slot, head } => base0_gdn_transition_evidence_from_disclosures_retained_v4(
                &w.forged.view(),
                node_slot,
                head,
                case.index,
                &case.demands,
                &batch,
                || w.backend.gdn_recompute_kernels_v2().unwrap(),
            )
            .expect("the exact upstream transition construction fits this budget")
            .map_v4(|refutation| PalwMemoryRoutingEvidenceV2::GdnTransition { refutation }),
            Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, head } => base0_gdn_conv_window_evidence_from_disclosures_retained_v4(
                &w.forged.view(),
                conv_slot,
                head,
                case.index,
                &case.demands,
                &batch,
            )
            .expect("the exact upstream conv construction fits this budget")
            .map_v4(|refutation| PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation }),
        };
        let verdict = match &*evidence {
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
        assert!(
            matches!(verdict, Err(PalwGdnCourtErrorV2::Resource(_))),
            "actual native court, not earlier construction, refuses quota"
        );
        assert!(matches!(terminal_found_fault_v1(verdict), Err(PalwGdnHeldErrorV2::Resource(_))));
        drop(evidence);
        drop(batch);
        let signs = std::cell::Cell::new(0);
        let signer = |m: &[u8], c: &[u8]| {
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
        let result = palw_gdn_held_step_retained_v2(
            &mut case,
            &w.forged.view(),
            || w.backend.gdn_recompute_kernels_v2().unwrap(),
            &[],
            10_000,
            20,
            &accuser,
        );
        assert!(matches!(result, Err(PalwGdnHeldErrorV2::Resource(_))), "actual filer retains the native Resource variant");
        assert_eq!(signs.get(), 0);
        assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
        drop(blocker);
        let action = palw_gdn_held_step_retained_v2(
            &mut case,
            &w.forged.view(),
            || w.backend.gdn_recompute_kernels_v2().unwrap(),
            &[],
            10_000,
            20,
            &accuser,
        )
        .unwrap();
        assert!(matches!(action, PalwGdnHeldNextRetainedV2::Accuse { .. }));
        assert_eq!(signs.get(), 1);
        drop(action);
        assert!(reserve_native_requests_v1(ceiling).is_ok(), "retry retires temporary construction/court/action tickets");
    }
}
