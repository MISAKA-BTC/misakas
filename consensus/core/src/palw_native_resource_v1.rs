//! Explicit local native capacity admission, independent of consensus roots.
//! Encoded bytes and a sampled RSS are not native restore/DB upper bounds.
use crate::BlockHash;
use std::sync::OnceLock;
pub(crate) mod typed;
pub use typed::{NativeCoverageV1, NativeFieldInventoryV1, NativeHeapV1, btree_node_envelope_bytes_v1};
pub(crate) use typed::{native_enum, native_struct, native_tuple_struct, native_unit};
mod demand;
mod restore;
mod restore_workspace;
mod retained;
pub use restore::restore_native_carriage_v1;
/// Reserve the existing audited root serialization scratch for a trusted
/// canonical frame length. This does not authorize unknown typed decoding.
pub fn reserve_native_root_workspace_v1(canonical_bytes: u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    let requests = restore_workspace::native_restore_root_workspace_requests_v1(canonical_bytes)?;
    reserve_native_requests_v1(requests)
}
pub(crate) use retained::{typed_value_requests_v1 as native_typed_value_requests_v1, value_requests_v1 as native_value_requests_v1};
mod transition;
pub use retained::{
    NativeDecodeErrorV1, NativeOwnerMutationErrorV1, NativeRetainedV1, NativeStateMutationErrorV1, native_delta_replay_requests_v1,
    retain_native_carriage_decode_classified_v1, retain_native_carriage_decode_v1,
    retain_native_carriage_decode_with_preflight_error_v1, retain_native_value_decode_classified_v1, retain_native_value_decode_v1,
};
pub(crate) use transition::NativeTransitionReservationV1;
pub type NativeStateDeltaV1 = NativeRetainedV1<crate::palw_state_v2::PalwStateDeltaV2>;
pub type NativeChainStateV1 = NativeRetainedV1<crate::palw_state_v2::PalwChainStateV2>;
pub type NativeStateCarriageV1 = NativeRetainedV1<crate::palw_state_v2::PalwStateCarriageV2>;
mod generated;
mod inventory;
mod kernel_ledger;
pub(crate) mod positive_rows;
mod kernel_material;
mod kernel_salted;
mod kernel_segment_assembly;
mod positive_assignment;
mod positive_certification;
pub mod positive_journal;
pub(crate) mod positive_rows;
pub use kernel_material::{
    native_kernel_parameter_auth_requests_v4, native_kernel_parameter_tensor_requests_v4, native_kernel_position_auth_requests_v4,
    native_kernel_segment_auth_requests_v4,
};
pub use kernel_segment_assembly::native_kernel_segment_assembly_requests_v4;
mod ledger_expansion;
// RFC-0006: the kernel ledger's consensus mutations, admitted from the real typed values (replaces `DerefMut`).
mod kernel_ledger_mutation;
pub use kernel_ledger_mutation::{PALW_KERNEL_LEDGER_APPLY_MEMBERSHIP_CEILING_V1, PALW_KERNEL_LEDGER_EVENTS_PER_ROW_CEILING_V1};
mod source_local;
mod tir;
pub use demand::{reserve_native_pending_requests_v1, reserve_native_requests_v1};
pub(crate) use ledger_expansion::source_tir_program_shape_v1;
pub(crate) use ledger_expansion::{NativeTirProgramShapeV1, standalone_tir_program_shape_v1};
pub use ledger_expansion::{ReservedKernelLedgerV1, inventory_ledger_expansion_v1, preflight_kernel_served_row_v1};
pub(crate) use ledger_expansion::{ledger_cache_clone_v1, reserve_ledger_expansion_v1};
pub use source_local::{
    LocalScanWorkBudgetV1, LocalSourceProgramOwnerV1, ScannedSourceProgramLocalV1, SourceProgramLocalErrorV1,
    SourceProgramLocalLimitsV1, SourceProgramLocalQuoteV1, require_source_program_local_profile_v1, scan_source_program_local_v1,
};
pub(crate) use tir::{NativeTirShardWorkspaceV1, TirNativeRequestsV1, reserve_native_tir_shard_workspace_v1};
pub use tir::{decode_native_tir_program_v1, growing_vec_request_capacity_v1};
pub(crate) use typed::BTreeNodeEnvelopeV1;
#[cfg(test)]
mod aux_registry_tests;
pub(crate) mod wire;
mod wire_budget;
mod wire_generated;
mod wire_jobs;
mod wire_kernel_aux;
#[cfg(test)]
mod wire_manual_tests;
pub use demand::{
    NativeDemandCheckerV1, NativeDemandLeaseV1, admit_native_receive_v1, arm_native_logical_limits_v1,
    native_declared_logical_limits_v1, native_logical_admission_enabled_v1, reserve_native_install_in_database_v1,
    reserve_native_install_v1, reserve_native_snapshot_v1, reserve_native_wire_v1,
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
    fn scoped_context_aux_preflight_uses_exact_writer_keys_and_rejects_unknown_shapes() {
        use crate::palw_claim_local_context_nodes_v2::{ComponentRootV2, ComponentSpecV2, ContextNodeV2, RowValueV2};
        fn scan(key: &[u8], bytes: &[u8]) -> std::io::Result<NativeWireShapeV1> {
            let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
            let shape = super::wire_kernel_aux::scan_native_aux_row_v1(144, key, &mut reader)?;
            assert_eq!(reader.remaining(), 0);
            Ok(shape)
        }
        let mut key = [0u8; 65];
        let body = borsh::to_vec(&vec![7u8; 17]).unwrap();
        key[0] = b'B';
        assert_eq!(scan(&key, &body).unwrap().heap, 34);
        assert!(scan(&key[..64], &body).is_err());
        key[0] = b'N';
        let node = ContextNodeV2::Leaf { key: vec![9; 8], value: RowValueV2 { data: crate::Hash64::default(), bytes: 17 } };
        assert_eq!(scan(&key, &borsh::to_vec(&node).unwrap()).unwrap().heap, 8);
        assert!(scan(&key, &[2]).is_err());
        let spec = ComponentSpecV2 { owner: 8, table: 1, schema: 1, key_bytes: 8, value_bytes: 17 };
        let root = ComponentRootV2::empty(spec).unwrap();
        let root_wire = borsh::to_vec(&root).unwrap();
        let mut ordinary = [0u8; 18];
        ordinary[0] = b'I';
        ordinary[1..].copy_from_slice(&borsh::to_vec(&spec).unwrap());
        scan(&ordinary, &root_wire).unwrap();
        let mut scoped = [0u8; 84];
        scoped[0] = b'I';
        scoped[1..3].copy_from_slice(&2u16.to_le_bytes());
        scoped[67..].copy_from_slice(&ordinary[1..]);
        scan(&scoped, &root_wire).unwrap();
        scoped[1..3].copy_from_slice(&3u16.to_le_bytes());
        assert!(scan(&scoped, &root_wire).is_err());
        key[0] = b'A';
        let archive = borsh::to_vec(&(2u16, vec![root], Vec::<RowValueV2>::new())).unwrap();
        scan(&key, &archive).unwrap();
        assert!(scan(&key, &9u16.to_le_bytes()).is_err());
        key[0] = b'Z';
        assert!(scan(&key, &body).is_err());
    }

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
