use super::*;
use crate::palw_native_resource_v1::*;
use crate::palw_tir_public_state_v1::legacy_original_v4;

fn original_fixture_v4() -> crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
    crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 4).frame.original_v1().clone()
}
#[test]
fn legacy_original_owner_streamed_claim_identity_preserves_canonical_bytes() {
    let original = original_fixture_v4();
    assert_eq!(legacy_original_v4::claim_id_v4(&original).unwrap(), crate::palw_freeprompt_v3::fp_claim_id_v3(&original));
    // Standalone wire/identity geometry, not approval or registration of a job.
    let mut expanded = original;
    expanded.job.executor_pubkey = vec![71; 70_000];
    assert_eq!(legacy_original_v4::claim_id_v4(&expanded).unwrap(), crate::palw_freeprompt_v3::fp_claim_id_v3(&expanded));
}
#[test]
fn legacy_original_owner_native_cold_read_exact_quota_lifetime_and_retry() {
    const CHILD: &str = "MISAKA_LEGACY_ORIGINAL_OWNER_CHILD_V4";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::legacy_original_owner_native_cold_read_exact_quota_lifetime_and_retry", module_path!());
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
    let original = original_fixture_v4();
    let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(&original);
    let bytes = borsh::to_vec(&original).unwrap();
    let q = preflight_native_value_v1::<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>(&bytes[..], bytes.len() as u64)
        .unwrap()
        .logical_requests_v1();
    assert!(q > 0);
    let mut state = PalwChainStateV2::genesis();
    // Original accepted bytes are a storage input fixture. This does not mine
    // a claim, create a legacy ClaimBundle, approve credit or fund acquisition.
    state.fp_public_jobs.insert(claim, bytes.clone());
    let root = state.state_root();
    assert_eq!(*state.staged_original_job_native_v4(claim).unwrap().unwrap(), original);
    const LIMIT: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(LIMIT - (q - 1)).unwrap();
    assert!(matches!(state.staged_original_job_native_v4(claim), Err(NativeDecodeErrorV1::Resource(_))));
    assert_eq!(state.state_root(), root);
    assert_eq!(state.staged_original_job_v1(claim).unwrap().unwrap(), original, "compatibility reader remains unchanged");
    drop(blocker);
    let blocker = reserve_native_requests_v1(LIMIT - q).unwrap();
    let owned = state.staged_original_job_native_v4(claim).unwrap().unwrap();
    assert_eq!(*owned, original);
    assert!(reserve_native_requests_v1(1).is_err(), "decode owner holds exact request through last borrow");
    assert_eq!(legacy_original_v4::claim_id_v4(&owned).unwrap(), claim);
    assert_eq!(state.state_root(), root);
    drop(owned);
    assert!(reserve_native_requests_v1(q).is_ok());
    drop(blocker);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
    let retried = state.staged_original_job_native_v4(claim).unwrap().unwrap();
    assert_eq!(borsh::to_vec(&*retried).unwrap(), bytes);
    drop(retried);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
#[test]
fn legacy_original_owner_missing_and_malformed_are_distinct_without_state_change() {
    let mut state = PalwChainStateV2::genesis();
    let claim = Hash64::from_u64_word(0x44);
    assert!(state.staged_original_job_native_v4(claim).unwrap().is_none());
    for malformed in [vec![], vec![255; 8], {
        let mut bytes = borsh::to_vec(&original_fixture_v4()).unwrap();
        bytes.push(1);
        bytes
    }] {
        state.fp_public_jobs.insert(claim, malformed);
        let root = state.state_root();
        assert!(matches!(state.staged_original_job_native_v4(claim), Err(NativeDecodeErrorV1::Decode(_))));
        assert_eq!(state.state_root(), root);
        assert!(state.staged_original_job_v1(claim).is_err());
    }
}
