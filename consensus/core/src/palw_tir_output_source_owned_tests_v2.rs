// Native original fixtures; not a public model campaign or an approval source.
#[test]
fn output_source_owned_preserves_native_frame_class_layout_and_root_refusals() {
    let fx = palw_tir_public_frame_fixture_v1(None, 4);
    let frame = &fx.frame;
    let (state, params, extras) = world(&fx, frame);
    let retained = super::output_source_owned_v2::source_v2(&state, frame).unwrap();
    let legacy = frame.source_v1(&fx.record, &fx.layout, frame.descriptor_v1().maximum_leaves).unwrap();
    assert_eq!(retained.source_id_v1(), legacy.source_id_v1());
    assert_eq!(retained.header_v1(), legacy.header_v1());
    assert_eq!(retained.decode_count_v1(), legacy.decode_count_v1());
    assert_eq!(retained.vocab_v1(), legacy.vocab_v1());
    for mode in 0..4 {
        let mut bad = state.clone();
        let class = frame.original_v1().job.class_id;
        match mode {
            0 => {
                bad.tir_classes.remove(&class);
            }
            1 => bad.tir_classes.get_mut(&class).unwrap().layout_digest = Hash64::default(),
            2 => bad.tir_classes.get_mut(&class).unwrap().graph_ir_root = Hash64::default(),
            _ => {
                bad.staged_courts.remove(&Key::TirLayout(class));
            }
        }
        let b = TransitionBuilder::try_new(&bad, &params, false, false, false, false, &extras).unwrap();
        assert!(matches!(source(&b, frame), Err(PalwStateV2Error::CarriageInconsistent(_))));
        assert!(
            matches!(super::output_source_owned_v2::source_v2(&bad, frame), Err(PalwStateV2Error::CarriageInconsistent(_))),
            "same malformed native class/layout/root is semantic, not local quota"
        );
    }
    // Existing native constructor still refuses a substituted execution root.
    assert!(matches!(
        crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            &fx.record,
            &fx.layout,
            frame.header_v1(),
            Hash64::default(),
            frame.original_v1().trace_root,
            frame.descriptor_v1().maximum_leaves
        ),
        Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))
    ));
}
#[test]
fn output_source_owned_actual_source_stage_quota_retry_and_owner_lifetime() {
    const CHILD: &str = "MISAKA_OUTPUT_SOURCE_OWNER_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let full = format!("{}::output_source_owned_actual_source_stage_quota_retry_and_owner_lifetime", module_path!());
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
    use super::output_source_owned_v2::{observed_v2, reset_observed_v2, source_v2};
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1};
    let fx = palw_tir_public_frame_fixture_v1(None, 4);
    let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
    let lying_fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(4, h(0xBAD));
    let frame = credited_frame(&lying_fx);
    let (state, params, extras) = world(&lying_fx, &frame);
    let evidence = Evidence::Output { rows_root: rows, ids: fx.execution.generated.clone() };
    let initial = state.state_root();
    const LOCAL_BYTES: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LOCAL_BYTES, database_bytes: 0 }));
    let full = reserve_native_requests_v1(LOCAL_BYTES).unwrap();
    reset_observed_v2();
    assert!(matches!(source_v2(&state, &frame), Err(PalwStateV2Error::NativeResource(_))));
    assert_eq!(observed_v2(), (1, 0));
    drop(full);
    let owner = source_v2(&state, &frame).unwrap();
    let (mut free, mut high) = (0, LOCAL_BYTES);
    while free < high {
        let middle = free + (high - free + 1) / 2;
        match reserve_native_requests_v1(middle) {
            Ok(ticket) => {
                drop(ticket);
                free = middle
            }
            Err(_) => high = middle - 1,
        }
    }
    assert!(free < LOCAL_BYTES, "actual retained source owner keeps native construction demand");
    let remainder = reserve_native_requests_v1(free).unwrap();
    assert!(reserve_native_requests_v1(1).is_err());
    assert_eq!(owner.source_id_v1(), frame.plan_v1().source_id);
    drop(owner);
    assert!(reserve_native_requests_v1(LOCAL_BYTES - free).is_ok(), "source drop returns its actual owned quota");
    drop(remainder);
    let run = |available: u64| {
        let block = reserve_native_requests_v1(LOCAL_BYTES - available).unwrap();
        reset_observed_v2();
        let result = palw_tir_public_staged_conviction_preflight_v2(
            &state,
            &params,
            &extras,
            at(&frame, 1).daa_score,
            outsider(),
            frame.plan_v1().original_claim,
            &evidence,
        );
        let observed = observed_v2();
        drop(block);
        (result, observed)
    };
    assert!(run(LOCAL_BYTES).0.is_ok());
    let (mut low, mut high) = (0, LOCAL_BYTES);
    while low < high {
        let middle = low + (high - low) / 2;
        let (_, observed) = run(middle);
        if observed.1 > 0 { high = middle } else { low = middle + 1 }
    }
    assert!(low > 0);
    let (result, observed) = run(low - 1);
    assert_eq!(
        observed,
        (1, 0),
        "actual Output preflight must reach the source constructor quota stage; earlier builder/ID errors do not satisfy this control"
    );
    assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
    assert!(run(LOCAL_BYTES).0.is_ok());
    assert_eq!(state.state_root(), initial, "capacity and dry runs publish no claim or economic effects");
}
