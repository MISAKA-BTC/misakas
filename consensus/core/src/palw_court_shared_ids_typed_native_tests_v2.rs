// Conditional production dispatcher controls with a prepared retained-context
// fixture. No production original installer, approval or public activation is supplied.
#[test]
fn shared_ids_typed_dispatch_preserves_gates_semantic_refusal_and_exact_replay() {
    use super::super::palw_court_service_lifecycle_fold_v1::{
        SharedCourtOpEffectV1 as Effect, apply_shared_court_wire_op_v1 as dispatch,
    };
    use crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1 as Op;
    use crate::palw_tir_public_source_v1::complete_ids_owned_v2::{borrowed_observed_v2, reset_borrowed_observed_v2};
    let fx = palw_tir_public_frame_fixture_v1(None, 4);
    // Existing helper installs only a trusted native-context fixture; it does
    // not inject the unadopted MaterialIntake storage schedule.
    let (state, params, mut extras, claim, _) = material_intake_world(&fx);
    // Explicit local test mirror only; no public Params/preset is changed.
    extras.tir_public_conviction_active = true;
    let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
    let op = Op::Ids { claim, rows_root: rows, ids: fx.execution.generated.clone() };
    let at = at(&fx.frame, 1);
    let mut dormant = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    assert!(matches!(dispatch(&mut dormant, &at, &op), Err(PalwStateV2Error::ObjectiveOffenceDormant)));
    let active = params.with_legacy_public_filer_from_daa(Some(0)).with_court_outsider_lane_from_daa(Some(0));
    let mut b = TransitionBuilder::try_new(&state, &active, false, false, false, false, &extras).unwrap();
    assert!(matches!(dispatch(&mut b, &at, &op), Ok(Effect::Stored(true))));
    let root = b.state.state_root();
    let entries = b.entries.len();
    reset_borrowed_observed_v2();
    assert!(matches!(dispatch(&mut b, &at, &op), Ok(Effect::Stored(false))));
    assert_eq!(
        borrowed_observed_v2(),
        (2, 2),
        "exact replay reauthenticates both incoming and existing stored IDs through typed validation"
    );
    assert_eq!(b.state.state_root(), root);
    assert_eq!(b.entries.len(), entries);
    let mut changed = fx.execution.generated.clone();
    changed[0] ^= 1;
    assert!(matches!(
        dispatch(&mut b, &at, &Op::Ids { claim, rows_root: rows, ids: changed }),
        Err(PalwStateV2Error::CarriageInconsistent(_))
    ));
    assert_eq!(b.state.state_root(), root);
    assert_eq!(b.entries.len(), entries);
}
#[test]
fn shared_ids_typed_actual_dispatch_hash_stage_capacity_retry_and_row_recheck() {
    const CHILD: &str = "MISAKA_SHARED_IDS_TYPED_QUOTA_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let full = format!("{}::shared_ids_typed_actual_dispatch_hash_stage_capacity_retry_and_row_recheck", module_path!());
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
    use super::super::palw_court_service_lifecycle_fold_v1::{
        SharedCourtOpEffectV1 as Effect, apply_shared_court_wire_op_v1 as dispatch,
    };
    use crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1 as Op;
    use crate::palw_native_resource_v1::{
        NativeDecodeErrorV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
    };
    use crate::palw_tir_public_source_v1::complete_ids_owned_v2::{borrowed_observed_v2, reset_borrowed_observed_v2};
    let fx = palw_tir_public_frame_fixture_v1(None, 4);
    let (state, params, mut extras, claim, _) = material_intake_world(&fx);
    // Explicit local test mirror only; no public Params/preset is changed.
    extras.tir_public_conviction_active = true;
    let params = params.with_legacy_public_filer_from_daa(Some(0)).with_court_outsider_lane_from_daa(Some(0));
    let rows = crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
    let op = Op::Ids { claim, rows_root: rows, ids: fx.execution.generated.clone() };
    let point = at(&fx.frame, 1);
    const LOCAL_BYTES: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LOCAL_BYTES, database_bytes: 0 }));
    let run = |available: u64| {
        let block = reserve_native_requests_v1(LOCAL_BYTES - available).unwrap();
        reset_borrowed_observed_v2();
        let mut b = match TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras) {
            Ok(b) => b,
            Err(e) => {
                drop(block);
                return (Err(e), (0, 0), true);
            }
        };
        let root = b.state.state_root();
        let entries = b.entries.len();
        let result = dispatch(&mut b, &point, &op);
        let unchanged = if result.is_err() { b.state.state_root() == root && b.entries.len() == entries } else { true };
        let observed = borrowed_observed_v2();
        drop(b);
        drop(block);
        (result, observed, unchanged)
    };
    assert!(matches!(run(LOCAL_BYTES).0, Ok(Effect::Stored(true))));
    let (mut low, mut high) = (0, LOCAL_BYTES);
    while low < high {
        let middle = low + (high - low) / 2;
        let (_, observed, _) = run(middle);
        if observed.1 > 0 { high = middle } else { low = middle + 1 }
    }
    assert!(low > 0);
    let (result, observed, unchanged) = run(low - 1);
    assert_eq!(observed, (1, 0), "capacity rejection must reach actual shared-IDs hash admission, not just builder/checkpoint");
    assert!(matches!(result, Err(PalwStateV2Error::NativeResource(_))));
    assert!(unchanged);
    assert!(
        matches!(run(LOCAL_BYTES).0, Ok(Effect::Stored(true))),
        "same exact dispatcher input retries after local capacity returns"
    );
    // A replay's stored-row validator must independently preserve native quota
    // refusal. Initial and exact-replay calls otherwise both run at ample quota.
    let mut b = TransitionBuilder::try_new(&state, &params, false, false, false, false, &extras).unwrap();
    dispatch(&mut b, &point, &op).unwrap();
    let row = match b.state.staged_court_row_v1(&Key::CourtSharedIds(claim)).unwrap() {
        Row::CourtSharedIds(row) => row,
        _ => panic!("stored IDs"),
    };
    // Request only the currently free real demand, leaving b's live owners intact.
    let (mut available, mut high) = (0, LOCAL_BYTES);
    while available < high {
        let middle = available + (high - available + 1) / 2;
        match reserve_native_requests_v1(middle) {
            Ok(lease) => {
                drop(lease);
                available = middle
            }
            Err(_) => high = middle - 1,
        }
    }
    let block = reserve_native_requests_v1(available).unwrap();
    assert!(matches!(row.check_context_classified_v2(&b.state), Err(NativeDecodeErrorV1::Resource(_))));
    drop(block);
    row.check_context_classified_v2(&b.state).unwrap();
    assert!(matches!(dispatch(&mut b, &point, &op), Ok(Effect::Stored(false))));
}
