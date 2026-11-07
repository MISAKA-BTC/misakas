//! Run in the consolidated verification phase only. Global admission settings
//! are isolated in child processes; no parallel fixture inherits a zero quota.
use super::*;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};

#[test]
fn local_projection_capacity_is_retryable_for_cold_and_warm_ledgers() {
    let worker = format!("{}::projection_capacity_worker", module_path!().split_once("::").unwrap().1);
    for case in ["full-cold", "full-warm", "native9-cold", "native9-warm"] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &worker, "--nocapture", "--test-threads=1"])
            .env("MISAKA_KERNEL_PROJECTION_CAPACITY_CHILD", case)
            .status()
            .unwrap();
        assert!(status.success(), "isolated {case} projection counterexample failed: {status}");
    }
}

fn projection_fixture(native9: bool, warm: bool) -> PalwKernelRouteStateV1 {
    let policy = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
    let mut route = PalwKernelRouteStateV1::new(policy, None, LedgerScalarsV1::default());
    if native9 {
        let mut ledger = route.ledger_checked_v1().unwrap();
        ledger.try_sync_bond_v1([3; 64], 1_000).unwrap();
        route.rows = ledger.to_rows();
        // This is only the operational projection's already-admitted-row seam.
        // The marker deliberately is not a full class registration and provides
        // no evidence of admission, execution, credit, or snapshot acceptance.
        route.rows.insert((misaka_palw_kernel::rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9, vec![4; 64]), vec![3; 64]);
        let projection = route.ledger_native_projection_checked_v9().unwrap();
        assert_eq!(projection.native_candidate_count_v9(), 1);
        if warm {
            route.ledger_cache.set_native_projection(projection);
        }
    } else {
        let ledger = route.ledger_checked_v1().unwrap();
        if warm {
            route.ledger_cache.set(ledger.as_rebuilt_from_v1(&route.rows));
        }
    }
    route
}

#[test]
fn projection_capacity_worker() {
    let Ok(case) = std::env::var("MISAKA_KERNEL_PROJECTION_CAPACITY_CHILD") else { return };
    let native9 = case.starts_with("native9-");
    let route = projection_fixture(native9, case.ends_with("-warm"));
    let before = borsh::to_vec(&route).unwrap();
    let roots = (route.ledger_root(), route.aux_root());
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    let failure = if native9 { route.ledger_native_projection_checked_v9() } else { route.ledger_checked_v1() }.unwrap_err();
    assert!(matches!(&failure, NativeDecodeErrorV1::Resource(_)), "local quota must retain its cause: {failure}");
    assert!(matches!(
        kernel_ledger_projection_state_error_v1(failure, crate::palw_state_v2::PalwStateV2Error::KernelRouteRefused),
        crate::palw_state_v2::PalwStateV2Error::NativeResource(_)
    ));
    if !native9 {
        assert!(matches!(route.ledger_native_projection_checked_v9(), Err(NativeDecodeErrorV1::Resource(_))));
    }
    assert_eq!(borsh::to_vec(&route).unwrap(), before, "a refusal cannot change persisted rows/header/aux");
    assert_eq!((route.ledger_root(), route.aux_root()), roots);
}

#[test]
fn malformed_scope_keeps_its_semantic_verdict_with_or_without_a_cache() {
    for (native9, warm) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut route = projection_fixture(native9, warm);
        route.aux.insert((PALW_KERNEL_ROUTE_TABLE_COMMITMENT_SCOPE_V1, Vec::new()), vec![1]);
        let before = borsh::to_vec(&route).unwrap();
        let failure = if native9 { route.ledger_native_projection_checked_v9() } else { route.ledger_checked_v1() }.unwrap_err();
        assert!(matches!(&failure, NativeDecodeErrorV1::Decode(_)));
        assert!(matches!(
            kernel_ledger_projection_state_error_v1(failure, crate::palw_state_v2::PalwStateV2Error::KernelRouteRefused),
            crate::palw_state_v2::PalwStateV2Error::KernelRouteRefused(_)
        ));
        assert_eq!(borsh::to_vec(&route).unwrap(), before);
    }
}

#[test]
fn native_projection_reinjects_current_scope_after_cache_fill_and_reversion() {
    for warm in [false, true] {
        let mut route = projection_fixture(true, warm);
        let historical_root = route.ledger_root();
        assert_eq!(route.ledger_native_projection_checked_v9().unwrap().commitment_scope_from_daa, None);
        route.aux.insert((PALW_KERNEL_ROUTE_TABLE_COMMITMENT_SCOPE_V1, Vec::new()), borsh::to_vec(&77u64).unwrap());
        assert_eq!(route.ledger_native_projection_checked_v9().unwrap().commitment_scope_from_daa, Some(77));
        route.aux.remove(&(PALW_KERNEL_ROUTE_TABLE_COMMITMENT_SCOPE_V1, Vec::new()));
        assert_eq!(route.ledger_native_projection_checked_v9().unwrap().commitment_scope_from_daa, None);
        assert_eq!(route.ledger_root(), historical_root, "aux consumer policy does not rewrite historical ledger roots");
    }
}
