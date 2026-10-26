//! Actual held answer reader/step; caller inputs and case are constructed before
//! the quota experiment. This tests authentication, not prefix/capture costs.
use super::*;
use kaspa_consensus_core::palw_held_da_v1::{palw_held_da_check_disclosure_v1, palw_held_da_state_chunk_auth_request_v2};
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeDecodeErrorV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
fn opened(w: &World) -> PalwGdnHeldCaseV1 {
    palw_gdn_held_open_v1(claim_of(&w.forged, w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap().unwrap()
}
#[test]
fn gdn_answer_auth_reader_preserves_invalid_skip_and_raw_disclosure_bytes() {
    for conv in [false, true] {
        let w = world_with_fault(0, conv);
        let case = opened(&w);
        let unit = case.demands[0];
        let good = producer_answer(&w, &case.claim, unit);
        let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure: source } = &good else { panic!("held carriage") };
        let mut foreign = good.clone();
        let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } = &mut foreign else { unreachable!() };
        disclosure.claim = Hash64::from_u64_word(0xBAD);
        let mut invalid = good.clone();
        let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } = &mut invalid else { unreachable!() };
        let PalwHeldDisclosureV1::StateChunk { chunk, .. } = &mut disclosure.disclosure else { panic!("actual GDN StateChunk") };
        chunk.chunk_bytes[0] ^= 1;
        assert!(
            palw_held_da_check_disclosure_v1(
                &case.claim.execution_root,
                &unit,
                &w.forged.binding,
                &disclosure.disclosure,
                w.forged.max_leaves,
                w.forged.prompt_ids_form
            )
            .is_err()
        );
        let read = |objects: &[PalwConsensusObjectV2]| {
            answer_on_chain_v1(objects, &case.claim, &w.forged.binding, &unit, w.forged.max_leaves, w.forged.prompt_ids_form)
        };
        assert!(read(&[foreign.clone(), invalid.clone()]).unwrap().is_none());
        let retained = read(&[foreign, invalid, good.clone()]).unwrap().unwrap();
        assert_eq!(borsh::to_vec(&*retained).unwrap(), borsh::to_vec(&source.disclosure).unwrap());
        palw_held_da_state_chunk_auth_request_v2(
            &case.claim.execution_root,
            &unit,
            &w.forged.binding,
            &source.disclosure,
            w.forged.max_leaves,
            w.forged.prompt_ids_form,
        )
        .unwrap()
        .check_v2()
        .unwrap();
    }
}
#[test]
fn gdn_answer_auth_actual_late_step_refuses_at_auth_before_copy_or_default_and_retries() {
    const CHILD: &str = "MISAKA_HELD_GDN_ANSWER_AUTH_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::gdn_answer_auth_actual_late_step_refuses_at_auth_before_copy_or_default_and_retries", module_path!());
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
    // Both existing native transition and convolution terminal fixtures. The
    // first real demanded disclosure supplies the exact authentication request.
    let scenarios: Vec<_> = [false, true]
        .into_iter()
        .map(|conv| {
            let w = world_with_fault(0, conv);
            let mut case = opened(&w);
            if conv {
                // The conv-only fixture has an honest transition. Select its
                // actual first conv terminal before deriving the answer set,
                // as the existing court/conv-owner controls do.
                let at = case
                    .candidates
                    .iter()
                    .position(|candidate| matches!(candidate, Base0GdnCheckpointCourtV2::ConvWindow { head: 0, .. }))
                    .expect("native convolution candidate");
                load_candidate_v1(&mut case, at, &w.forged.view(), &mut || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap();
            }
            assert!(
                match case.candidates[case.at] {
                    Base0GdnCheckpointCourtV2::Transition { head: 0, .. } => !conv,
                    Base0GdnCheckpointCourtV2::ConvWindow { head: 0, .. } => conv,
                    _ => false,
                },
                "quota/retry must exercise the fixture's actual faulty terminal"
            );
            let objects: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
            (w, case, objects)
        })
        .collect();
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    for (w, mut case, objects) in scenarios {
        let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } = &objects[0] else { panic!("held carriage") };
        let request = || {
            palw_held_da_state_chunk_auth_request_v2(
                &case.claim.execution_root,
                &case.demands[0],
                &w.forged.binding,
                &disclosure.disclosure,
                w.forged.max_leaves,
                w.forged.prompt_ids_form,
            )
            .unwrap()
        };
        let q = request().required_bytes_v2();
        assert!(q > 0 && q < ceiling);
        // A scalar-only authentication at Q succeeds; no retained answer copy
        // can mask this target-stage test. The workspace is released at return.
        {
            let blocker = reserve_native_requests_v1(ceiling - q).unwrap();
            request().check_v2().unwrap();
            let probe = reserve_native_requests_v1(q).unwrap();
            drop(probe);
            drop(blocker);
        }
        case.demanded = Some((0, 100));
        let prior = (case.at, case.demands.clone(), case.demanded);
        let calls = std::cell::Cell::new(0);
        let signer = |m: &[u8], c: &[u8]| {
            calls.set(calls.get() + 1);
            sign(m, c)
        };
        let accuser = PalwGdnHeldAccuserV1 {
            bond: bond(2),
            network_domain: Hash64::from_u64_word(0xD0),
            openings: &w.openings,
            oracle: &w.oracle,
            sign: &signer,
        };
        for available in [0, q - 1] {
            let blocker = reserve_native_requests_v1(ceiling - available).unwrap();
            assert!(matches!(
                palw_held_da_state_chunk_auth_request_v2(
                    &case.claim.execution_root,
                    &case.demands[0],
                    &w.forged.binding,
                    &disclosure.disclosure,
                    w.forged.max_leaves,
                    w.forged.prompt_ids_form
                )
                .unwrap()
                .check_v2(),
                Err(NativeDecodeErrorV1::Resource(_))
            ));
            let kernels = || w.backend.gdn_recompute_kernels_v2().unwrap();
            assert!(matches!(
                palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), kernels, &objects, 10000, 20, &accuser),
                Err(PalwGdnHeldErrorV2::Resource(_))
            ));
            assert_eq!((case.at, case.demands.clone(), case.demanded), prior);
            assert!(case.answers.iter().all(Option::is_none));
            assert_eq!(calls.get(), 0);
            drop(blocker);
        }
        let kernels = || w.backend.gdn_recompute_kernels_v2().unwrap();
        let retry = palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), kernels, &objects, 10000, 20, &accuser).unwrap();
        assert!(matches!(&retry, PalwGdnHeldNextRetainedV2::Accuse { .. }));
        assert_eq!(calls.get(), 1);
        assert!(case.answers.iter().all(Option::is_some));
        drop(retry);
        drop(accuser);
        drop(case);
        drop(w);
        assert!(reserve_native_requests_v1(ceiling).is_ok(), "all fixture/answer/court/output owners released");
    }
}
