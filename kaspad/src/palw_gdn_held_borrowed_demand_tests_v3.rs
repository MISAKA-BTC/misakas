//! Actual graph-v8 held caller; its complete-head conv chunk count is dc=1.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
use kaspa_consensus_core::palw_step_refute::memory_v2::{gdn_check_conv_demands_borrowed_v3, gdn_conv_window_demands_v2};

fn answered_conv_case(w: &World) -> PalwGdnHeldCaseV1 {
    let mut case =
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
            .unwrap()
            .unwrap();
    let at = case.candidates.iter().position(|c| matches!(c, Base0GdnCheckpointCourtV2::ConvWindow { head: 0, .. })).unwrap();
    load_candidate_v1(&mut case, at, &w.forged.view(), &mut || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap();
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    case.demanded = Some((0, 110));
    case
}

#[test]
fn actual_borrowed_demand_check_matches_native_first_last_head_and_rejects_foreign_units() {
    let w = world_with_fault(0, true);
    let case = answered_conv_case(&w);
    let Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, .. } = case.candidates[case.at] else { panic!("conv") };
    for head in [0, u32::from(w.profile.gdn_heads) - 1] {
        let native = gdn_conv_window_demands_v2(&w.forged.binding, conv_slot, head, case.index).unwrap();
        assert_eq!(native.len(), 1, "actual complete-head fixture: dc=1, not a multi-block reorder test");
        gdn_check_conv_demands_borrowed_v3(&w.forged.binding, conv_slot, head, case.index, &native).unwrap();
        let PalwHeldMissingV1::StateChunk { checkpoint, chunk } = native[0] else { panic!("state") };
        for wrong in [
            Vec::new(),
            vec![native[0], native[0]],
            vec![PalwHeldMissingV1::StateChunk { checkpoint: checkpoint + 1, chunk }],
            vec![PalwHeldMissingV1::StateChunk { checkpoint, chunk: chunk + 1 }],
        ] {
            assert!(matches!(
                gdn_check_conv_demands_borrowed_v3(&w.forged.binding, conv_slot, head, case.index, &wrong),
                Err(PalwGdnCourtErrorV2::Evidence(_))
            ));
        }
        let other =
            gdn_conv_window_demands_v2(&w.forged.binding, conv_slot, (head + 1) % u32::from(w.profile.gdn_heads), case.index).unwrap();
        assert!(gdn_check_conv_demands_borrowed_v3(&w.forged.binding, conv_slot, head, case.index, &other).is_err());
    }
}

#[test]
fn actual_retained_caller_refuses_mismatch_or_capacity_without_state_or_signing_then_retries() {
    const CHILD: &str = "MISAKA_GDN_BORROWED_DEMAND_CALLER_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name =
            format!("{}::actual_retained_caller_refuses_mismatch_or_capacity_without_state_or_signing_then_retries", module_path!());
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
    let w = world_with_fault(0, true);
    let mut case = answered_conv_case(&w);
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
    let original = case.demands[0];
    let PalwHeldMissingV1::StateChunk { checkpoint, chunk } = original else { panic!("state") };
    case.demands[0] = PalwHeldMissingV1::StateChunk { checkpoint: checkpoint + 1, chunk };
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
    assert!(step(&mut case).is_err(), "actual retained caller must not accuse with foreign demands");
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    assert_eq!(signs.get(), 0);
    case.demands[0] = original;
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    assert!(
        matches!(step(&mut case), Err(PalwGdnHeldErrorV2::Resource(_))),
        "caller control may refuse at an earlier leg/batch; it is not the metadata-targeted quota proof"
    );
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    assert_eq!(signs.get(), 0);
    drop(blocker);
    let next = step(&mut case).unwrap();
    assert!(matches!(next, PalwGdnHeldNextRetainedV2::Accuse { .. }));
    assert_eq!(signs.get(), 1);
    drop(next);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
}
