//! Explicit local native capacity admission, independent of consensus roots.
//! Encoded bytes and a sampled RSS are not native restore/DB upper bounds.
use crate::BlockHash;
use std::sync::OnceLock;
pub(crate) mod typed;
pub use typed::{NativeCoverageV1, NativeFieldInventoryV1, NativeHeapV1};
pub(crate) use typed::{native_enum, native_struct, native_tuple_struct, native_unit, native_unsupported};
mod demand;
mod generated;
mod inventory;
mod kernel_ledger;
mod ledger_expansion;
pub(crate) use demand::reserve_native_requests_v1;
pub use ledger_expansion::{ReservedKernelLedgerV1, inventory_ledger_expansion_v1, preflight_kernel_served_row_v1};
pub(crate) use ledger_expansion::{ledger_cache_clone_v1, reserve_ledger_expansion_v1};
#[cfg(test)]
mod aux_registry_tests;
pub(crate) mod wire;
mod wire_budget;
mod wire_generated;
mod wire_kernel_aux;
pub use demand::{
    NativeDemandCheckerV1, NativeDemandLeaseV1, admit_native_receive_v1, arm_native_logical_limits_v1,
    native_logical_admission_enabled_v1, reserve_native_install_in_database_v1, reserve_native_install_v1, reserve_native_snapshot_v1,
    reserve_native_wire_v1,
};
pub use wire::{
    NativeWirePreflightV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, NativeWireValuePreflightV1,
    preflight_native_carriage_v1, preflight_native_snapshot_v1, preflight_native_value_v1,
};
mod database_quota;
mod quota;
pub use database_quota::{NativeHardDatabaseQuotaV1, actual_hard_database_quota_v1};
pub use inventory::{NativeSnapshotInventoryV1, NativeSnapshotOperationV1, inventory_accepted_state_v1};
pub use quota::{NativeHardMemoryQuotaV1, actual_hard_memory_quota_v1};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSnapshotResourceLimitsV1 {
    pub memory_bytes: u64,
    pub database_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSnapshotResourceBoundV1 {
    pub version: u16,
    pub point: BlockHash,
    /// Whole simultaneous carriage/restore/index/cache/batch materialization.
    pub memory_upper_bytes: u64,
    /// Whole installation including WAL, retained rows and compaction copies.
    pub database_upper_bytes: u64,
}
static LIMITS: OnceLock<Option<NativeSnapshotResourceLimitsV1>> = OnceLock::new();
pub fn arm_native_resource_limits_v1(limits: Option<NativeSnapshotResourceLimitsV1>) {
    let _ = LIMITS.set(limits);
}
pub fn native_resource_admission_enabled_v1() -> bool {
    LIMITS.get().is_some_and(Option::is_some)
}
pub fn admit_native_snapshot_v1(point: BlockHash, bound: Option<NativeSnapshotResourceBoundV1>) -> Result<(), String> {
    admit_native_snapshot_with_limits_v1(point, *LIMITS.get().unwrap_or(&None), bound)
}
pub fn admit_native_snapshot_with_limits_v1(
    point: BlockHash,
    limits: Option<NativeSnapshotResourceLimitsV1>,
    bound: Option<NativeSnapshotResourceBoundV1>,
) -> Result<(), String> {
    let Some(limits) = limits else { return Ok(()) };
    let bound = bound.ok_or("native RAM/DB structural bound unavailable for explicit local capability")?;
    if bound.version != 1 || bound.point != point || bound.memory_upper_bytes == 0 {
        return Err("native RAM/DB bound version, point or coverage is invalid".into());
    }
    if bound.memory_upper_bytes == u64::MAX || bound.database_upper_bytes == u64::MAX {
        return Err("native RAM/DB structural bound is saturated".into());
    }
    if bound.memory_upper_bytes > limits.memory_bytes || bound.database_upper_bytes > limits.database_bytes {
        return Err(format!(
            "native snapshot exceeds local capacity: RAM {} / {}, DB {} / {}",
            bound.memory_upper_bytes, limits.memory_bytes, bound.database_upper_bytes, limits.database_bytes
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_unknown_bound_refusal_preserves_legacy_and_checks_point_and_both_axes() {
        let point = BlockHash::from_u64_word(1);
        let limits = Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 4096, database_bytes: 8192 });
        assert!(admit_native_snapshot_with_limits_v1(point, None, None).is_ok());
        assert!(admit_native_snapshot_with_limits_v1(point, limits, None).is_err());
        let good = NativeSnapshotResourceBoundV1 { version: 1, point, memory_upper_bytes: 4096, database_upper_bytes: 8192 };
        assert!(admit_native_snapshot_with_limits_v1(point, limits, Some(good)).is_ok());
        for invalid in [
            NativeSnapshotResourceBoundV1 { point: BlockHash::from_u64_word(2), ..good },
            NativeSnapshotResourceBoundV1 { memory_upper_bytes: 4097, ..good },
            NativeSnapshotResourceBoundV1 { database_upper_bytes: 8193, ..good },
            NativeSnapshotResourceBoundV1 { database_upper_bytes: u64::MAX, ..good },
            NativeSnapshotResourceBoundV1 { version: 2, ..good },
        ] {
            assert!(admit_native_snapshot_with_limits_v1(point, limits, Some(invalid)).is_err());
        }
    }
}
