struct ContinuationSourceCaseV2 {
    state: PalwChainStateV2,
    control: Control,
    offset: u64,
}
fn continuation_source_cases_v2(f: &SourceThreeFixture) -> Vec<ContinuationSourceCaseV2> {
    use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
    let frame = &f.fx.frame;
    let network = f.extras.tir_public_network_domain_v1.unwrap();
    let installed = source_three_installed(f);
    let producer = frame.custody_v1().pledge_v1().producer;
    let capture = source_three_capture(f, producer, &f.producer_key);
    let mut b = TransitionBuilder::try_new(&installed, &f.params, false, false, false, false, &f.extras).unwrap();
    apply_control_v1(&mut b, &at(frame, 1), network, &capture).unwrap();
    let captured = b.state.clone();
    let ready = source_three_control(
        frame,
        producer,
        T::Preparation { original_accepted_daa: frame.plan_v1().terms.accepted_daa },
        A::Preparation(Owned::PreparationReady { descriptor: source_three_descriptor(frame) }),
        0,
        &f.producer_key,
        network,
    );
    apply_control_v1(&mut b, &at(frame, 2), network, &ready).unwrap();
    let prepared = b.state.clone();
    let open_at = frame.plan_v1().preparation_until_daa - frame.plan_v1().terms.accepted_daa;
    let open = source_three_control(
        frame,
        outsider(),
        T::OwnerOpen { owner: outsider() },
        A::Open,
        owner_terms_v1(frame).unwrap().plan.rent_sompi,
        &f.owner_key,
        network,
    );
    let mut b = TransitionBuilder::try_new(&prepared, &f.params, false, false, false, false, &f.extras).unwrap();
    apply_control_v1(&mut b, &at(frame, open_at), network, &open).unwrap();
    let opened = b.state.clone();
    let demand = source_three_control(
        frame,
        outsider(),
        T::Acquisition { owner: outsider(), reserved_daa: at(frame, open_at).daa_score },
        A::Acquisition(Owned::Demand { request: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 0 } }),
        0,
        &f.owner_key,
        network,
    );
    vec![
        ContinuationSourceCaseV2 { state: installed, control: capture, offset: 1 },
        ContinuationSourceCaseV2 { state: captured, control: ready, offset: 2 },
        ContinuationSourceCaseV2 { state: opened, control: demand, offset: open_at + 1 },
    ]
}
#[test]
fn continuation_source_owned_signed_preparation_capture_ready_quota_rollback_retry() {
    continuation_source_child_v2("continuation_source_owned_signed_preparation_capture_ready_quota_rollback_retry", 0)
}
#[test]
fn continuation_source_owned_signed_owner_demand_quota_rollback_retry() {
    continuation_source_child_v2("continuation_source_owned_signed_owner_demand_quota_rollback_retry", 1)
}
#[test]
fn continuation_source_owned_direct_last_borrow_and_semantic_release() {
    continuation_source_child_v2("continuation_source_owned_direct_last_borrow_and_semantic_release", 2)
}
fn continuation_source_child_v2(name: &str, mode: u8) {
    const CHILD: &str = "MISAKA_CONTINUATION_SOURCE_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        continuation_source_worker_v2(mode)
    } else {
        let full = format!("{}::{name}", module_path!());
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, name)
                .status()
                .unwrap()
                .success()
        );
    }
}
fn continuation_source_worker_v2(mode: u8) {
    use crate::palw_native_resource_v1::*;
    use crate::palw_tir_public_state_v1::*;
    let f = source_three_fixture();
    let cases = continuation_source_cases_v2(&f);
    let frame = &f.fx.frame;
    let network = f.extras.tir_public_network_domain_v1.unwrap();
    // Actual native signatures and all action preconditions must work before
    // local capacity is armed; no alternate verifier supplies acceptance.
    for case in &cases {
        let mut b = TransitionBuilder::try_new(&case.state, &f.params, false, false, false, false, &f.extras).unwrap();
        reset_continuation_source_stages_v2();
        apply_control_v1(&mut b, &at(frame, case.offset), network, &case.control).unwrap();
        assert_eq!(continuation_source_stages_v2(), [1, 1, 1]);
    }
    const LIMIT: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    if mode < 2 {
        let selected = if mode == 0 { &cases[..2] } else { &cases[2..] };
        for case in selected {
            let run = |available: u64| {
                let blocker = reserve_native_requests_v1(LIMIT - available).unwrap();
                reset_continuation_source_stages_v2();
                let result = match TransitionBuilder::try_new(&case.state, &f.params, false, false, false, false, &f.extras) {
                    Err(e) => Err(e),
                    Ok(mut b) => {
                        let root = b.state.state_root();
                        let entries = b.entries.len();
                        let result = apply_control_v1(&mut b, &at(frame, case.offset), network, &case.control);
                        if result.is_err() {
                            assert_eq!(b.state.state_root(), root);
                            assert_eq!(b.entries.len(), entries);
                        }
                        result
                    }
                };
                let observed = continuation_source_stages_v2();
                drop(blocker);
                (result, observed)
            };
            let (ok, s) = run(LIMIT);
            ok.unwrap();
            assert_eq!(s, [1, 1, 1]);
            let (mut lo, mut hi) = (0, LIMIT);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let (_, s) = run(mid);
                if s[1] > 0 { hi = mid } else { lo = mid + 1 }
            }
            assert!(lo > 0);
            let (err, s) = run(lo - 1);
            assert_eq!(s, [1, 0, 0], "must target Source after real signature/checkpoint, not early builder refusal");
            assert!(matches!(err, Err(PalwStateV2Error::NativeResource(_))));
            run(LIMIT).0.unwrap();
        }
    } else {
        let q = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
            &f.fx.record,
            &f.fx.layout,
            frame.header_v1(),
        )
        .unwrap();
        assert!(q > 0 && q < LIMIT);
        let direct = |case: &ContinuationSourceCaseV2, source: Hash64| match (&case.control.target, &case.control.action) {
            (T::Preparation { .. }, A::Preparation(action)) => palw_tir_public_preparation_transition_native_v2(
                &case.state,
                frame.preparation_key_v1(),
                source,
                0,
                case.control.signer,
                at(frame, case.offset).daa_score,
                action,
                frame.descriptor_v1().maximum_leaves,
            ),
            (T::Acquisition { owner, reserved_daa }, A::Acquisition(action)) => palw_tir_public_transition_native_v2(
                &case.state,
                owner_key(frame, *owner, *reserved_daa).unwrap(),
                source,
                0,
                case.control.signer,
                at(frame, case.offset).daa_score,
                action,
                frame.descriptor_v1().maximum_leaves,
                None,
            ),
            _ => unreachable!(),
        };
        thread_local! {static SEEN:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
        fn final_borrow(source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1) {
            assert_ne!(source.source_id_v1(), Hash64::default());
            assert!(reserve_native_requests_v1(1).is_err());
            SEEN.with(|v| v.set(true));
        }
        for case in &cases {
            let blocker = reserve_native_requests_v1(LIMIT - (q - 1)).unwrap();
            reset_continuation_source_stages_v2();
            assert!(matches!(direct(case, case.control.source_id), Err(PalwTirPublicSourceErrorV2::Resource(_))));
            assert_eq!(continuation_source_stages_v2(), [1, 0, 0]);
            drop(blocker);
            let blocker = reserve_native_requests_v1(LIMIT - q).unwrap();
            observe_continuation_source_borrow_v2(Some(final_borrow));
            SEEN.with(|v| v.set(false));
            reset_continuation_source_stages_v2();
            direct(case, case.control.source_id).unwrap();
            assert!(SEEN.with(std::cell::Cell::get));
            assert_eq!(continuation_source_stages_v2(), [1, 1, 1]);
            assert!(reserve_native_requests_v1(q).is_ok());
            assert!(matches!(direct(case, Hash64::default()), Err(PalwTirPublicSourceErrorV2::Decode(_))));
            assert!(reserve_native_requests_v1(q).is_ok());
            observe_continuation_source_borrow_v2(None);
            drop(blocker);
            direct(case, case.control.source_id).unwrap();
        }
    }
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}

#[test]
fn public_row_owned_source_api_actual_accepted_row_quota_semantic_and_retry() {
    const CHILD: &str = "MISAKA_PUBLIC_ROW_OWNED_SOURCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::public_row_owned_source_api_actual_accepted_row_quota_semantic_and_retry", module_path!());
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    use crate::palw_native_resource_v1::*;
    use crate::palw_tir_public_state_v1::*;
    let f = source_three_fixture();
    let cases = continuation_source_cases_v2(&f);
    let case = &cases[2];
    let frame = &f.fx.frame;
    let T::Acquisition { owner, reserved_daa } = case.control.target else { panic!("actual accepted acquisition") };
    let key = owner_key(frame, owner, reserved_daa).unwrap();
    let Some(Row::TirAcquisition(row)) = case.state.staged_court_row_v1(&key) else { panic!("actual signed Open row") };
    let max = frame.descriptor_v1().maximum_leaves;
    let get = || palw_tir_public_source_for_row_owned_v2(&case.state, row, max);
    assert_eq!(get().unwrap().source_id_v1(), frame.plan_v1().source_id);
    const LIMIT: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let q = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
        &f.fx.record,
        &f.fx.layout,
        frame.header_v1(),
    )
    .unwrap();
    assert!(q > 0 && q < LIMIT);
    let blocker = reserve_native_requests_v1(LIMIT - (q - 1)).unwrap();
    reset_continuation_source_stages_v2();
    assert!(matches!(get(), Err(NativeDecodeErrorV1::Resource(_))));
    assert_eq!(continuation_source_stages_v2(), [1, 0, 0]);
    assert_eq!(palw_tir_public_source_for_row_v1(&case.state, row, max).unwrap().source_id_v1(), frame.plan_v1().source_id);
    drop(blocker);
    let blocker = reserve_native_requests_v1(LIMIT - q).unwrap();
    let held = get().unwrap();
    assert_eq!(held.source_id_v1(), frame.plan_v1().source_id);
    assert!(reserve_native_requests_v1(1).is_err());
    drop(held);
    assert!(reserve_native_requests_v1(q).is_ok());
    drop(blocker);
    assert!(matches!(palw_tir_public_source_for_row_owned_v2(&case.state, row, max - 1), Err(NativeDecodeErrorV1::Decode(_))));
    let mut wrong = row.as_ref().clone();
    wrong.descriptor.source_id = Hash64::default();
    assert!(matches!(palw_tir_public_source_for_row_owned_v2(&case.state, &wrong, max), Err(NativeDecodeErrorV1::Decode(_))));
    assert_eq!(get().unwrap().source_id_v1(), frame.plan_v1().source_id);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
