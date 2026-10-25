//! Actual pending responder case and the same production Source decision.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::*;
#[test]
fn tir_source_worker_owned_refused_read_preserves_responder_case_and_starts_no_work() {
    const CHILD: &str = "MISAKA_TIR_SOURCE_RESPONDER_READ_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::tir_source_worker_owned_refused_read_preserves_responder_case_and_starts_no_work", module_path!());
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
    // Local pending metadata only: no accepted claim, completed answer or
    // model result is claimed by this disposition control.
    let mut case = ResponseCaseV1 {
        claim: Hash64::from_u64_word(1),
        allocation_id: Default::default(),
        source_id: Default::default(),
        owner: PalwBondKeyV2(Default::default()),
        acquisition: Default::default(),
        reserved_daa: 0,
        demand_id: Default::default(),
        turn: 0,
        request: PalwTirPublicRequestV1::StepLeaf { index: 0 },
        respond_until_daa: 10,
        body: None,
        failed: false,
        last_move: Some(7),
    };
    const LIMIT: u64 = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let bytes = vec![9u8; 64];
    let q = super::super::tir_public_owner::source_owned::test_clone_requests_v2(&bytes, LIMIT);
    assert!(q > 0);
    let blocker = reserve_native_requests_v1(LIMIT - (q - 1)).unwrap();
    let mut refusal = None;
    let mut work_started = 0;
    let read = NativeRetainedV1::try_from_clone_v1(&bytes).map_err(NativeDecodeErrorV1::Resource);
    if let Some(_owner) = super::super::tir_public_owner::source_owned::defer_source_read_v2(&mut refusal, read) {
        work_started += 1;
        case.failed = true;
    }
    assert_eq!(work_started, 0);
    assert!(!case.failed);
    assert!(case.body.is_none());
    assert_eq!(case.last_move, Some(7));
    assert!(matches!(refusal, Some(NativeDecodeErrorV1::Resource(_))));
    drop(blocker);
    let mut refusal = None;
    assert!(
        super::super::tir_public_owner::source_owned::defer_source_read_v2::<Vec<u8>>(
            &mut refusal,
            Err(NativeDecodeErrorV1::Decode("semantic source identity refusal".into()))
        )
        .is_none()
    );
    assert!(!case.failed);
    assert!(case.body.is_none());
    assert!(matches!(refusal, Some(NativeDecodeErrorV1::Decode(_))));
    let mut refusal = None;
    let owner = super::super::tir_public_owner::source_owned::defer_source_read_v2(
        &mut refusal,
        NativeRetainedV1::try_from_clone_v1(&bytes).map_err(NativeDecodeErrorV1::Resource),
    )
    .unwrap();
    assert_eq!(owner.len(), 64);
    assert!(refusal.is_none());
    drop(owner);
    assert!(!case.failed);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
