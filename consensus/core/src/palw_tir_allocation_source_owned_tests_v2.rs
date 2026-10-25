#[test]
fn allocation_source_owned_actual_derivation_quota_semantic_retry_and_release() {
    const CHILD: &str = "MISAKA_ALLOCATION_SOURCE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::allocation_source_owned_actual_derivation_quota_semantic_retry_and_release", module_path!());
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
    use crate::palw_tir_public_allocation_v1::*;
    let f = preparation_sources_fixture_v2();
    let b = TransitionBuilder::try_new(&f.state, &f.params, false, false, false, false, &f.extras).unwrap();
    let (expected, native, _, geometry) = original_plan_v1(&b, f.ctx.daa_score, &f.fp.commitment, &f.envelope.header).unwrap();
    drop(b);
    let raw = || {
        derive_tir_public_allocation_v1(
            &geometry,
            &f.fp.commitment,
            &f.envelope.header,
            &f.fx.record,
            &f.fx.layout,
            None,
            None,
            native,
        )
    };
    let typed = || {
        derive_tir_public_allocation_native_v2(
            &geometry,
            &f.fp.commitment,
            &f.envelope.header,
            &f.fx.record,
            &f.fx.layout,
            None,
            None,
            native,
        )
    };
    let assert_plan = |value: PalwTirPublicAllocationQuoteV1| match value {
        PalwTirPublicAllocationQuoteV1::Complete(plan) => assert_eq!(*plan, expected),
        _ => panic!("actual fixture must first have a complete native allocation"),
    };
    assert_plan(raw().unwrap());
    assert_plan(typed().unwrap());
    const LOCAL_BYTES: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LOCAL_BYTES, database_bytes: 0 }));
    let q = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
        &f.fx.record,
        &f.fx.layout,
        &f.envelope.header,
    )
    .unwrap();
    assert!(q > 0 && q < LOCAL_BYTES);
    let blocker = reserve_native_requests_v1(LOCAL_BYTES - (q - 1)).unwrap();
    reset_allocation_source_stages_v2();
    assert!(matches!(typed(), Err(NativeDecodeErrorV1::Resource(_))));
    assert_eq!(allocation_source_stages_v2(), [1, 0, 0]);
    // The legacy raw/String API deliberately keeps its original admission
    // semantics, rather than Stringifying new local Resource errors.
    assert_plan(raw().unwrap());
    drop(blocker);
    let blocker = reserve_native_requests_v1(LOCAL_BYTES - q).unwrap();
    thread_local! { static OBSERVED:std::cell::Cell<bool>=const{std::cell::Cell::new(false)}; }
    fn final_borrow(source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1) {
        assert_ne!(source.source_id_v1(), Hash64::default());
        assert!(reserve_native_requests_v1(1).is_err(), "actual Source lease is still live at common derivation finish");
        OBSERVED.with(|v| v.set(true));
    }
    observe_allocation_source_borrow_v2(Some(final_borrow));
    reset_allocation_source_stages_v2();
    assert_plan(typed().unwrap());
    assert!(OBSERVED.with(std::cell::Cell::get));
    assert_eq!(allocation_source_stages_v2(), [1, 1, 1]);
    observe_allocation_source_borrow_v2(None);
    let recovered = reserve_native_requests_v1(q).unwrap();
    drop(recovered);
    drop(blocker);
    check_tir_public_allocation_retained_v2(
        &expected,
        &geometry,
        &f.fp.commitment,
        &f.envelope.header,
        &f.fx.record,
        &f.fx.layout,
        None,
        None,
        native,
    )
    .unwrap();
    let mut wrong = f.envelope.header.clone();
    wrong.full_logits_trace_root = Hash64::default();
    assert!(matches!(
        derive_tir_public_allocation_native_v2(&geometry, &f.fp.commitment, &wrong, &f.fx.record, &f.fx.layout, None, None, native),
        Err(NativeDecodeErrorV1::Decode(_))
    ));
    let mut changed = expected.clone();
    changed.source_id = Hash64::default();
    assert!(matches!(
        check_tir_public_allocation_retained_v2(
            &changed,
            &geometry,
            &f.fp.commitment,
            &f.envelope.header,
            &f.fx.record,
            &f.fx.layout,
            None,
            None,
            native
        ),
        Err(NativeDecodeErrorV1::Decode(_))
    ));
    assert_plan(typed().unwrap());
    assert!(reserve_native_requests_v1(LOCAL_BYTES).is_ok());
}
