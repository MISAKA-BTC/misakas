//! Real registered program/layout copies through the actual planning and worker
//! helpers. The OwnerCase fixture asserts no model replay or accepted claim.
use super::super::{cone_owned_consumer_tests, harvest};
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::*;
fn run_child(name: &str) -> bool {
    const CHILD: &str = "MISAKA_CONE_INPUTS_OWNED_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(name) {
        return true;
    }
    let full = format!("{}::{name}", module_path!());
    assert!(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
            .env(CHILD, name)
            .status()
            .unwrap()
            .success()
    );
    false
}
#[test]
fn tir_cone_inputs_owned_planning_qminus1_q_preserves_case_and_copies_exactly() {
    if !run_child("tir_cone_inputs_owned_planning_qminus1_q_preserves_case_and_copies_exactly") {
        return;
    }
    let (_, _, _, _, registered, layout) = harvest::tests::fixture_v2();
    let (mut case, _host) = cone_owned_consumer_tests::case();
    case.prompt = vec![1, 6, 3];
    const LIMIT: u64 = 1 << 28;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let q = OwnedConeInputsV2::requests_v2(&registered, &layout, &case.prompt).unwrap();
    assert!(q > 0 && q < LIMIT);
    let block = reserve_native_requests_v1(LIMIT - (q - 1)).unwrap();
    STAGES.with(|s| s.set((0, 0)));
    let mut refusal = None;
    let failed = prepare_for_case_v2(&registered, &layout, &case, &mut refusal);
    assert!(failed.is_none());
    assert!(matches!(refusal, Some(NativeDecodeErrorV1::Resource(_))));
    assert_eq!(STAGES.with(std::cell::Cell::get), (1, 0), "no copy before admitted input quote");
    assert!(!case.done && case.demands == 0 && case.last_move.is_none() && case.harvested.is_empty());
    assert!(case.units.is_empty() && case.ids.is_none());
    drop(block);
    let exact = reserve_native_requests_v1(LIMIT - q).unwrap();
    let mut refusal = None;
    let owned = prepare_for_case_v2(&registered, &layout, &case, &mut refusal).unwrap();
    assert!(refusal.is_none());
    assert_eq!(STAGES.with(std::cell::Cell::get), (2, 1));
    assert_eq!(owned.registered, registered);
    assert_eq!(owned.layout, layout);
    assert_eq!(owned.prompt, case.prompt);
    assert!(
        std::sync::Arc::ptr_eq(&owned.registered.program, &registered.program),
        "shared program identity is retained, not copied or substituted"
    );
    assert!(reserve_native_requests_v1(1).is_err());
    assert!(!case.done && case.demands == 0 && case.last_move.is_none());
    drop(owned);
    assert!(reserve_native_requests_v1(q).is_ok());
    drop(exact);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
#[test]
fn tir_cone_inputs_owned_cancel_retains_boxed_inputs_and_program_until_final_borrow() {
    if !run_child("tir_cone_inputs_owned_cancel_retains_boxed_inputs_and_program_until_final_borrow") {
        return;
    }
    let (_, _, _, _, registered, layout) = harvest::tests::fixture_v2();
    let (mut case, _host) = cone_owned_consumer_tests::case();
    case.prompt = vec![1, 6, 3];
    const LIMIT: u64 = 1 << 28;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let class = registered.facts.class_id;
    let layout_scalars = (layout.version, layout.max_context, layout.checkpoint_interval, layout.h_tile);
    let commit_tiles: [u32; 3] = layout.commit_tiles.as_slice().try_into().unwrap();
    let state_tiles: [u32; 1] = layout.state_tiles.as_slice().try_into().unwrap();
    let mut refusal = None;
    let inputs = prepare_for_case_v2(&registered, &layout, &case, &mut refusal).unwrap();
    let source = NativeRetainedV1::try_from_clone_v1(&vec![4u8]).unwrap();
    drop(registered);
    drop(layout);
    drop(case);
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap().block_on(async move {
        let (started, wait_started) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel();
        let task = tokio::spawn(run_owned_inputs_v2(source, inputs, move |source, inputs| {
            assert_eq!(source.as_slice(), &[4]);
            assert_eq!(inputs.registered.facts.class_id, class);
            started.send(()).unwrap();
            wait_release.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            assert_eq!(
                (inputs.layout.version, inputs.layout.max_context, inputs.layout.checkpoint_interval, inputs.layout.h_tile),
                layout_scalars
            );
            assert_eq!(inputs.layout.commit_tiles.as_slice(), &commit_tiles);
            assert_eq!(inputs.layout.state_tiles.as_slice(), &state_tiles);
            inputs.registered.check_program_v1().unwrap();
            assert_eq!(inputs.prompt.as_slice(), &[1, 6, 3]);
            assert!(reserve_native_requests_v1(LIMIT).is_err());
        }));
        wait_started.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(reserve_native_requests_v1(LIMIT).is_err());
        release.send(()).unwrap();
        let mut released = false;
        for _ in 0..100 {
            if let Ok(lease) = reserve_native_requests_v1(LIMIT) {
                drop(lease);
                released = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(released, "actual last worker releases source, copied payload and outer owner requests");
    });
}
