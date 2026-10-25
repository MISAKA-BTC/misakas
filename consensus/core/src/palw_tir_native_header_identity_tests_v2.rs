#[test]
fn native_header_identity_owned_copy_quota_order_semantic_and_release() {
    native_header_identity_child_v2("native_header_identity_owned_copy_quota_order_semantic_and_release", 0)
}
#[test]
fn native_header_identity_actual_paired_before_source_refusal_and_retry() {
    native_header_identity_child_v2("native_header_identity_actual_paired_before_source_refusal_and_retry", 1)
}
fn native_header_identity_child_v2(name: &str, mode: u8) {
    const CHILD: &str = "MISAKA_NATIVE_HEADER_IDENTITY_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        native_header_identity_worker_v2(mode)
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
fn native_header_identity_worker_v2(mode: u8) {
    use crate::palw_native_resource_v1::*;
    use crate::palw_tir_public_allocation_v1::*;
    let f = preparation_sources_fixture_v2();
    let mut extras = f.extras.clone();
    extras.tir_public_originals.insert(f.fp.claim_id(), f.envelope.clone());
    let b = TransitionBuilder::try_new(&f.state, &f.params, false, false, false, false, &extras).unwrap();
    let (expected, rules, _, geometry) = original_plan_v1(&b, f.ctx.daa_score, &f.fp.commitment, &f.envelope.header).unwrap();
    drop(b);
    let derive = |g: &crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1| {
        derive_tir_public_allocation_native_v2(g, &f.fp.commitment, &f.envelope.header, &f.fx.record, &f.fx.layout, None, None, rules)
    };
    const LIMIT: u64 = 256 << 20;
    // This helper deliberately returns zero while native admission is off.
    // Arm before asking for the actual quote used by try_from_clone_v1.
    let header_unarmed = native_value_requests_v1(&f.envelope.header).unwrap();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let header_q = native_value_requests_v1(&f.envelope.header).unwrap();
    let source_q = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
        &f.fx.record,
        &f.fx.layout,
        &f.envelope.header,
    )
    .unwrap();
    eprintln!(
        "native-header-identity quote: mode={mode} logical_limit={LIMIT} header_before_arm={header_unarmed} header_after_arm={header_q} source_after_arm={source_q}"
    );
    assert!(header_q > 0 && header_q < source_q && source_q < LIMIT);
    if mode == 0 {
        let blocker = reserve_native_requests_v1(LIMIT - (header_q - 1)).unwrap();
        reset_native_header_identity_stages_v2();
        reset_allocation_source_stages_v2();
        assert!(matches!(derive(&geometry), Err(NativeDecodeErrorV1::Resource(_))));
        assert_eq!(native_header_identity_stages_v2(), [1, 0, 0]);
        assert_eq!(allocation_source_stages_v2(), [0; 3]);
        drop(blocker);
        let blocker = reserve_native_requests_v1(LIMIT - header_q).unwrap();
        thread_local! {static SEEN:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
        fn observe(header: &crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1) {
            assert_ne!(header.job_context_v1().job_id, Hash64::default());
            assert!(
                reserve_native_requests_v1(1).is_err(),
                "actual deep-clone ticket holds all exact header quote during identity use"
            );
            SEEN.with(|v| v.set(true));
        }
        observe_native_header_identity_v2(Some(observe));
        reset_native_header_identity_stages_v2();
        reset_allocation_source_stages_v2();
        // Header identity succeeds and releases its own ticket before the much
        // larger Source independently refuses. This is not a plan-positive.
        assert!(matches!(derive(&geometry), Err(NativeDecodeErrorV1::Resource(_))));
        assert!(SEEN.with(std::cell::Cell::get));
        assert_eq!(native_header_identity_stages_v2(), [1, 1, 1]);
        assert_eq!(allocation_source_stages_v2(), [1, 0, 0]);
        let recovered = reserve_native_requests_v1(header_q).unwrap();
        drop(recovered);
        let mut wrong = geometry.clone();
        wrong.execution_root = Hash64::default();
        reset_allocation_source_stages_v2();
        assert!(matches!(derive(&wrong), Err(NativeDecodeErrorV1::Decode(_))));
        assert_eq!(allocation_source_stages_v2(), [0; 3]);
        assert!(reserve_native_requests_v1(header_q).is_ok());
        observe_native_header_identity_v2(None);
        drop(blocker);
        let PalwTirPublicAllocationQuoteV1::Complete(plan) = derive(&geometry).unwrap() else { panic!("fixture complete") };
        assert_eq!(*plan, expected);
    } else {
        let run = |available: u64| {
            let blocker = reserve_native_requests_v1(LIMIT - available).unwrap();
            reset_native_header_identity_stages_v2();
            reset_allocation_source_stages_v2();
            let result = match TransitionBuilder::try_new(&f.state, &f.params, false, false, false, false, &extras) {
                Err(e) => Err(e),
                Ok(b) => {
                    let root = b.state.state_root();
                    let entries = b.entries.len();
                    let result = prepare_paired_original_v1(&b, &f.ctx, &f.object, Some(&f.fp.commitment));
                    assert_eq!(b.state.state_root(), root);
                    assert_eq!(b.entries.len(), entries);
                    result.map(|p| assert_eq!(p.unwrap().frame_v1().plan_v1(), &expected))
                }
            };
            let stages = (native_header_identity_stages_v2(), allocation_source_stages_v2());
            drop(blocker);
            (result, stages)
        };
        let (ok, _) = run(LIMIT);
        ok.unwrap();
        let (mut lo, mut hi) = (0, LIMIT);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (_, s) = run(mid);
            if s.0[1] > 0 { hi = mid } else { lo = mid + 1 }
        }
        assert!(lo > 0);
        let (err, stages) = run(lo - 1);
        assert_eq!(stages, ([1, 0, 0], [0; 3]), "actual caller reaches header clone admission before Source");
        assert!(matches!(err, Err(PalwStateV2Error::NativeResource(_))));
        run(LIMIT).0.unwrap();
        assert_eq!(palw_tir_public_preclaim_quote_v1(&f.state, &f.params, &f.ctx, &extras, &f.object, &f.envelope).unwrap(), expected);
    }
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
