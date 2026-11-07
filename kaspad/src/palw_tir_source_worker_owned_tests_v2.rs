//! Actual generic production handoff; payload is typed bytes, not a model result.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::*;

#[test]
fn public_worker_incomplete_handoff_keeps_submission_without_party_default() {
    for (why, kind) in [
        ("TIR owner replay worker failed: cancelled", PublicWorkerIncompleteV2::Stopped),
        ("source IO unavailable", PublicWorkerIncompleteV2::Stopped),
        ("TIR accepted body lost a rooted chunk", PublicWorkerIncompleteV2::WithheldBody),
        ("withheld body", PublicWorkerIncompleteV2::WithheldBody),
        ("TIR reassembled body does not reproduce its accepted checksum/head", PublicWorkerIncompleteV2::TamperedRecord),
        ("TIR accepted body has more chunks than its canonical split", PublicWorkerIncompleteV2::TamperedRecord),
        ("TIR rooted chunk substitutes demand/signer/head/offset", PublicWorkerIncompleteV2::DoubleSignature),
        ("double signature", PublicWorkerIncompleteV2::DoubleSignature),
    ] {
        let got = classify_worker_incomplete_v2(why);
        assert_eq!(got, kind);
        assert!(got.leaves_public_submission_v2());
        assert!(!got.attributes_party_default_v2());
        assert!(matches!(handoff_worker_result_v2::<u8>(Ok(Err(why.into()))), PublicWorkerHandoffV2::Remains(k) if k == kind));
    }
    assert!(matches!(rooted_chunk_handoff_v2::<u8>(None), PublicWorkerHandoffV2::Remains(PublicWorkerIncompleteV2::WithheldBody)));
    assert!(matches!(rooted_chunk_handoff_v2(Some(9u8)), PublicWorkerHandoffV2::Completed(9)));
    assert!(matches!(handoff_worker_result_v2::<u8>(Ok(Ok(4))), PublicWorkerHandoffV2::Completed(4)));
}
#[test]
fn tir_source_worker_owned_cancel_keeps_actual_reservation_until_worker_returns() {
    const CHILD: &str = "MISAKA_TIR_SOURCE_WORKER_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::tir_source_worker_owned_cancel_keeps_actual_reservation_until_worker_returns", module_path!());
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
    const LIMIT: u64 = 1 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let bytes = vec![7u8; 64];
    let q = test_clone_requests_v2(&bytes, LIMIT);
    assert!(q > 0 && q < LIMIT);
    let blocker = reserve_native_requests_v1(LIMIT - q).unwrap();
    let source = NativeRetainedV1::try_from_clone_v1(&bytes).unwrap();
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap().block_on(async move {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let task = tokio::spawn(run_owned_source_v2(source, move |payload| {
            assert_eq!(payload.len(), 64);
            assert!(reserve_native_requests_v1(1).is_err());
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            assert_eq!(payload[0], 7);
            assert!(reserve_native_requests_v1(1).is_err());
        }));
        started_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(reserve_native_requests_v1(1).is_err(), "cancelled waiter cannot release running worker owner");
        release_tx.send(()).unwrap();
        let mut released = false;
        for _ in 0..100 {
            if let Ok(owner) = reserve_native_requests_v1(q) {
                drop(owner);
                released = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(released, "completed actual worker releases native owner");
        let source = NativeRetainedV1::try_from_clone_v1(&bytes).unwrap();
        assert_eq!(
            run_owned_source_v2(source, |payload| {
                assert!(reserve_native_requests_v1(1).is_err());
                payload.len()
            })
            .await
            .unwrap(),
            64
        );
        assert!(reserve_native_requests_v1(q).is_ok());
        drop(blocker);
        assert!(reserve_native_requests_v1(LIMIT).is_ok());
    });
}
