//! Actual OwnerCase -> worker snapshot -> production blocking handoff. Public
//! authenticated identity material is not an accepted native claim fixture.
use super::super::{cone_owned_consumer_tests, harvest, source_owned};
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::*;
#[test]
fn tir_material_share_owned_case_cancel_keeps_unit_ids_and_pointer_owners() {
    const CHILD: &str = "MISAKA_TIR_MATERIAL_CASE_WORKER_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::tir_material_share_owned_case_cancel_keeps_unit_ids_and_pointer_owners", module_path!());
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
    const LIMIT: u64 = 1 << 30;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let (mut case, _host) = cone_owned_consumer_tests::case();
    // Typed public response authentication/complete-ID intake runs AFTER arming;
    // its output payload tickets are live, not merely shape-only Arc metadata.
    let (unit, ids) = harvest::tests::material();
    assert!(harvest::harvest_accepted_v2(&mut case, kaspa_consensus_core::Hash64::from_u64_word(1), || Ok(Some(unit))).unwrap());
    let case_membership_bytes = harvest::membership_requests_v2(&case).unwrap().1;
    assert!(harvest::harvest_accepted_v2(&mut case, kaspa_consensus_core::Hash64::from_u64_word(2), || Ok(Some(ids))).unwrap());
    let members = MaterialWorkerInputsV2::from_case(&case).unwrap();
    assert!(Arc::ptr_eq(case.ids.as_ref().unwrap(), members.ids.as_ref().unwrap()));
    assert!(Arc::ptr_eq(&case.units[0], &members.units[0]));
    let source = NativeRetainedV1::try_from_clone_v1(&vec![3u8]).unwrap();
    let available_before = available_v2(LIMIT);
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap().block_on(async move {
        let (started, wait_started) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel();
        let task = tokio::spawn(source_owned::run_owned_source_v2(source, move |source| {
            let members = members;
            assert_eq!(source.as_slice(), &[3]);
            assert_eq!(members.ids.as_ref().unwrap().ids_v1(), &[1, 2]);
            assert_eq!(members.units.len(), 1);
            started.send(()).unwrap();
            wait_release.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            // Final actual material borrow occurs after async cancellation and
            // destruction of the original case and its membership ticket.
            assert_eq!(members.ids.as_ref().unwrap().ids_v1(), &[1, 2]);
            assert!(matches!(
                members.units[0].request_v1(),
                kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepNode { .. }
            ));
            assert!(reserve_native_requests_v1(LIMIT).is_err());
        }));
        wait_started.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        drop(case);
        assert_eq!(
            available_v2(LIMIT),
            available_before + case_membership_bytes,
            "only the original case membership retires; copied worker membership remains charged"
        );
        assert!(reserve_native_requests_v1(LIMIT).is_err());
        release.send(()).unwrap();
        let mut released = false;
        for _ in 0..100 {
            if let Ok(ticket) = reserve_native_requests_v1(LIMIT) {
                drop(ticket);
                released = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(released, "last actual worker drops payload, Arc and copied member owners");
    });
}

fn available_v2(limit: u64) -> u64 {
    let (mut lo, mut hi) = (0, limit);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if let Ok(lease) = reserve_native_requests_v1(mid) {
            drop(lease);
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}
#[test]
fn tir_material_share_owned_publication_membership_and_arc_refusal_retry() {
    const CHILD: &str = "MISAKA_TIR_PUBLICATION_CAPACITY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::tir_material_share_owned_publication_membership_and_arc_refusal_retry", module_path!());
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
    const LIMIT: u64 = 1 << 30;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    for arc_stage in [false, true] {
        let (mut case, _host) = cone_owned_consumer_tests::case();
        let (unit1, ids1) = harvest::tests::material();
        drop(unit1);
        let (unit2, ids2) = harvest::tests::material();
        drop(unit2);
        let id = kaspa_consensus_core::Hash64::from_u64_word(10);
        let membership = harvest::membership_requests_v2(&case).unwrap().1;
        let arc =
            SharedMaterialV2::<kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1>::arc_requests_v2()
                .unwrap();
        let room = if arc_stage { membership + arc - 1 } else { membership - 1 };
        let available = available_v2(LIMIT);
        assert!(available > room);
        let block = reserve_native_requests_v1(available - room).unwrap();
        assert!(matches!(harvest::harvest_accepted_v2(&mut case, id, || Ok(Some(ids1))), Err(NativeDecodeErrorV1::Resource(_))));
        assert_eq!(case._harvest_membership.is_some(), arc_stage, "Arc-stage only follows actual admitted membership publication");
        assert!(case.units.is_empty() && case.ids.is_none() && case.harvested.is_empty() && !case.done);
        drop(block);
        assert!(harvest::harvest_accepted_v2(&mut case, id, || Ok(Some(ids2))).unwrap());
        let pointer = Arc::as_ptr(case.ids.as_ref().unwrap());
        assert!(!harvest::harvest_accepted_v2(&mut case, id, || panic!("no duplicate native read")).unwrap());
        assert_eq!(Arc::as_ptr(case.ids.as_ref().unwrap()), pointer);
        assert_eq!(case.ids.as_ref().unwrap().ids_v1(), &[1, 2]);
        assert!(!case.done);
        drop(case);
        assert_eq!(available_v2(LIMIT), LIMIT);
    }
}
