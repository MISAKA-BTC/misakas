//! Actual pending owner case/production decision; no accepted Source is forged.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::*;
#[test]
fn tir_source_worker_owned_refused_read_preserves_owner_case_and_starts_no_work() {
    const CHILD: &str = "MISAKA_TIR_SOURCE_OWNER_READ_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::tir_source_worker_owned_refused_read_preserves_owner_case_and_starts_no_work", module_path!());
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
    // Existing local case fixture's replay methods are never invoked and it
    // asserts no model result. Core separately authenticates actual row Source.
    let (mut case, _host) = super::cone_owned_consumer_tests::case();
    const LIMIT: u64 = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let bytes = vec![9u8; 64];
    let q = source_owned::test_clone_requests_v2(&bytes, LIMIT);
    assert!(q > 0);
    let blocker = reserve_native_requests_v1(LIMIT - (q - 1)).unwrap();
    let mut refusal = None;
    let mut work_started = 0;
    let read = NativeRetainedV1::try_from_clone_v1(&bytes).map_err(NativeDecodeErrorV1::Resource);
    if let Some(_owner) = source_owned::defer_source_read_v2(&mut refusal, read) {
        work_started += 1;
        case.done = true;
    }
    assert_eq!(work_started, 0);
    assert!(!case.done);
    assert_eq!(case.demands, 0);
    assert!(case.last_move.is_none());
    assert!(matches!(refusal, Some(NativeDecodeErrorV1::Resource(_))));
    drop(blocker);
    let mut refusal = None;
    assert!(
        source_owned::defer_source_read_v2::<Vec<u8>>(
            &mut refusal,
            Err(NativeDecodeErrorV1::Decode("semantic source identity refusal".into()))
        )
        .is_none()
    );
    assert!(!case.done);
    assert!(matches!(refusal, Some(NativeDecodeErrorV1::Decode(_))));
    let mut refusal = None;
    let owner = source_owned::defer_source_read_v2(
        &mut refusal,
        NativeRetainedV1::try_from_clone_v1(&bytes).map_err(NativeDecodeErrorV1::Resource),
    )
    .unwrap();
    assert_eq!(owner.len(), 64);
    assert!(refusal.is_none());
    drop(owner);
    assert!(!case.done);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
