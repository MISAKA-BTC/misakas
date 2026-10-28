//! Actual non-genesis held transition, with native metadata-targeted quota.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
use kaspa_consensus_core::palw_step_refute::{PalwStepRefuteError, memory_v2::gdn_check_transition_demands_borrowed_v3};

fn answered_transition() -> (World, PalwGdnHeldCaseV1) {
    let w = world_with_fault_at_checkpoint(0, false, 1);
    let mut case =
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, &w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
            .unwrap()
            .unwrap();
    assert_eq!(w.profile.n_ctx, 384);
    assert_eq!(w.positions, 32);
    assert_eq!(case.index, w.index);
    assert!(matches!(case.candidates[case.at], Base0GdnCheckpointCourtV2::Transition { head: 0, .. }));
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    case.demanded = Some((0, 110));
    (w, case)
}

#[test]
fn actual_transition_borrowed_check_matches_native_heads_and_preserves_semantic_order() {
    let (w, case) = answered_transition();
    let Base0GdnCheckpointCourtV2::Transition { node_slot, .. } = case.candidates[case.at] else { panic!("transition") };
    for head in [0, u32::from(w.profile.gdn_heads) - 1] {
        let native = gdn_transition_demands_v2(&w.forged.binding, node_slot, head, case.index).unwrap();
        assert_eq!(native.len(), 1, "actual tiny head has dd=1: this does not claim multi-block reordering coverage");
        gdn_check_transition_demands_borrowed_v3(&w.forged.binding, node_slot, head, case.index, &native).unwrap();
        let PalwHeldMissingV1::StateChunk { checkpoint, chunk } = native[0] else { panic!("state") };
        let wrongs: [Vec<PalwHeldMissingV1>; 5] = [
            vec![],
            vec![native[0], native[0]],
            vec![PalwHeldMissingV1::PromptIdsTile { tile: 0 }],
            vec![PalwHeldMissingV1::StateChunk { checkpoint: checkpoint + 1, chunk }],
            vec![PalwHeldMissingV1::StateChunk { checkpoint, chunk: chunk + 1 }],
        ];
        for wrong in wrongs {
            assert!(matches!(
                gdn_check_transition_demands_borrowed_v3(&w.forged.binding, node_slot, head, case.index, &wrong),
                Err(PalwGdnCourtErrorV2::Evidence(_))
            ));
        }
        let other =
            gdn_transition_demands_v2(&w.forged.binding, node_slot, (head + 1) % u32::from(w.profile.gdn_heads), case.index).unwrap();
        assert!(gdn_check_transition_demands_borrowed_v3(&w.forged.binding, node_slot, head, case.index, &other).is_err());
    }
    // The original collector evaluates native site/checkpoint/head before its
    // caller can compare lengths. The borrowed check preserves that ordering.
    for (slot, head) in [(u32::MAX, 0), (node_slot, u32::MAX)] {
        assert!(matches!(
            gdn_transition_demands_v2(&w.forged.binding, slot, head, case.index),
            Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::Unadjudicable))
        ));
        assert!(matches!(
            gdn_check_transition_demands_borrowed_v3(&w.forged.binding, slot, head, case.index, &[]),
            Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::Unadjudicable))
        ));
    }
}

#[test]
fn actual_transition_metadata_quota_target_and_retained_caller_retry_do_not_advance_or_sign() {
    const CHILD: &str = "MISAKA_GDN_TRANSITION_DEMAND_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name =
            format!("{}::actual_transition_metadata_quota_target_and_retained_caller_retry_do_not_advance_or_sign", module_path!());
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
    let (w, mut case) = answered_transition();
    let Base0GdnCheckpointCourtV2::Transition { node_slot, head } = case.candidates[case.at] else { panic!("transition") };
    let binding_before = borsh::to_vec(&w.forged.binding).unwrap();
    assert!(
        (0..w.profile.layer_count).any(|l| w.profile.layer_kind(l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::Attention)
    );
    assert!(
        (0..w.profile.layer_count).any(|l| w.profile.layer_kind(l) == kaspa_consensus_core::palw_step::PalwLayerKindV1::GatedDeltaNet)
    );
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
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    // This direct API has no Base0 leg, answer batch, prefix replay, copied
    // witness or rank cache. Its first allocation admission is metadata itself.
    assert!(matches!(
        gdn_check_transition_demands_borrowed_v3(&w.forged.binding, node_slot, head, case.index, &case.demands),
        Err(PalwGdnCourtErrorV2::Resource(_))
    ));
    assert!(matches!(
        gdn_check_transition_demands_borrowed_v3(&w.forged.binding, u32::MAX, head, case.index, &[]),
        Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::Unadjudicable))
    ));
    assert_eq!(borsh::to_vec(&w.forged.binding).unwrap(), binding_before);
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
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
        "whole-caller refusal may be earlier and does not substitute for the direct metadata control"
    );
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    assert_eq!(signs.get(), 0);
    drop(blocker);
    gdn_check_transition_demands_borrowed_v3(&w.forged.binding, node_slot, head, case.index, &case.demands).unwrap();
    let original = case.demands[0];
    let PalwHeldMissingV1::StateChunk { checkpoint, chunk } = original else { panic!("state") };
    case.demands[0] = PalwHeldMissingV1::StateChunk { checkpoint: checkpoint + 1, chunk };
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    assert!(matches!(step(&mut case), Err(PalwGdnHeldErrorV2::Operation(_))));
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    assert_eq!(signs.get(), 0);
    case.demands[0] = original;
    let next = step(&mut case).unwrap();
    assert!(matches!(next, PalwGdnHeldNextRetainedV2::Accuse { .. }));
    assert_eq!(signs.get(), 1);
    drop(next);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
}
