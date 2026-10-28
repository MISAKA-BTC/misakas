//! Snapshot-specific simultaneous native demand and explicit DB/allocator coverage.
use super::*;
use crate::palw_state_v2::{PalwChainStateV2, PalwStateCarriageV2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NativeSnapshotOperationV1 {
    Serve,
    Restore,
    Install,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeSnapshotInventoryV1 {
    pub version: u16,
    pub point: BlockHash,
    pub state_root: crate::Hash64,
    pub operation: NativeSnapshotOperationV1,
    pub canonical_bytes: u64,
    pub fields: Vec<NativeFieldInventoryV1>,
    pub native_requests: NativeCoverageV1,
    pub restoration_indices: NativeCoverageV1,
    pub simultaneous_copies: NativeCoverageV1,
    pub serialization_and_batch: NativeCoverageV1,
    pub root_scratch: NativeCoverageV1,
    pub allocator_physical: NativeCoverageV1,
    pub database_rows: NativeCoverageV1,
    pub database_history: NativeCoverageV1,
    pub database_wal: NativeCoverageV1,
    pub database_compaction: NativeCoverageV1,
}
impl NativeSnapshotInventoryV1 {
    pub fn logical_memory_v1(&self) -> NativeCoverageV1 {
        self.simultaneous_copies
            .clone()
            .add::<Self>(self.serialization_and_batch.clone(), "native peak")
            .add::<Self>(self.root_scratch.clone(), "native peak")
    }
    pub fn database_v1(&self) -> NativeCoverageV1 {
        self.database_rows
            .clone()
            .add::<Self>(self.database_history.clone(), "database installation")
            .add::<Self>(self.database_wal.clone(), "database installation")
            .add::<Self>(self.database_compaction.clone(), "database installation")
    }
    /// Refresh quota evidence at admission; never trust a cached/peer-supplied quota.
    pub fn physical_bound_v1(&self) -> Result<NativeSnapshotResourceBoundV1, NativeCoverageV1> {
        let logical = self.logical_memory_v1();
        let requested = logical.bytes().ok_or(logical)?;
        let quota = actual_hard_memory_quota_v1()?;
        if requested > quota.headroom_bytes() {
            return Err(NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>(
                "allocator.physical-memory",
                "typed requested allocations exceed currently observed kernel hard-quota headroom",
            ));
        }
        let database = self.database_v1();
        let database_upper_bytes = database.bytes().ok_or(database)?;
        Ok(NativeSnapshotResourceBoundV1 {
            version: 1,
            point: self.point,
            memory_upper_bytes: quota.limit_bytes(),
            database_upper_bytes,
        })
    }
}
fn product<T>(value: u64, count: u64, component: &str) -> NativeCoverageV1 {
    value
        .checked_mul(count)
        .map(|bytes| NativeCoverageV1::Bounded { bytes })
        .unwrap_or_else(|| NativeCoverageV1::overflow::<T>(component))
}
/// Called on an accepted state at capture/write time, not by decoding a peer's
/// advertised byte length. All reverse/due indices are already present here and
/// visited by their actual private types and cardinalities.
pub fn inventory_accepted_state_v1(
    state: &PalwChainStateV2,
    point: BlockHash,
    canonical_bytes: u64,
    operation: NativeSnapshotOperationV1,
) -> NativeSnapshotInventoryV1 {
    let fields = state.native_components_v1();
    let native_requests = state
        .native_heap_v1("PalwChainStateV2.all-primary-and-derived")
        .add::<PalwChainStateV2>(NativeCoverageV1::Bounded { bytes: std::mem::size_of::<PalwChainStateV2>() as u64 }, "native state");
    let restoration_indices = fields
        .iter()
        .filter(|field| RESTORE_INDEX_FIELDS_V1.iter().any(|name| field.component.ends_with(name)))
        .fold(NativeCoverageV1::zero(), |sum, field| sum.add::<PalwChainStateV2>(field.heap.clone(), "restoration indices"));
    // Clone of an accepted primary table cannot have more nested ownership or
    // BTree entries than the complete state, including its restored indices.
    // Account the live source, restored state, writer/carriage copies and cache.
    let copy_count = match operation {
        NativeSnapshotOperationV1::Serve => 3,
        NativeSnapshotOperationV1::Restore => 4,
        NativeSnapshotOperationV1::Install => 5,
    };
    let simultaneous_copies = native_requests
        .clone()
        .scale::<PalwChainStateV2>(copy_count, "state/carriage/restore/cache coexistence")
        .add::<PalwStateCarriageV2>(
            product::<PalwStateCarriageV2>(std::mem::size_of::<PalwStateCarriageV2>() as u64, copy_count, "carriage headers"),
            "coexisting carriage headers",
        );
    // Six known encoded-vector lifetimes: source record, served snapshot record,
    // two cache clones, Bincode temporary and RocksDB WriteBatch payload. This
    // term models those byte Vecs only, never native maps via a codec multiplier.
    let serialization_and_batch = product::<Vec<u8>>(canonical_bytes, 6, "encoded record/cache/batch Vecs")
        .add::<Vec<u8>>(NativeCoverageV1::Bounded { bytes: (8 << 20) + (128 << 10) }, "fixed chunk/deflate/input scratch");
    let root_scratch = fields
        .iter()
        .try_fold(0u64, |sum, field| sum.checked_add(field.entries))
        .map(|entries| {
            product::<crate::Hash64>(entries, 3 * std::mem::size_of::<crate::Hash64>() as u64, "root current/next/frontier vectors")
        })
        .unwrap_or_else(|| NativeCoverageV1::overflow::<crate::Hash64>("root entry counts"));
    let allocator_physical =
        actual_hard_memory_quota_v1().map(|quota| NativeCoverageV1::Bounded { bytes: quota.limit_bytes() }).unwrap_or_else(|why| why);
    let no_write = operation != NativeSnapshotOperationV1::Install;
    NativeSnapshotInventoryV1 {
        version: 1,
        point,
        state_root: state.state_root(),
        operation,
        canonical_bytes,
        fields,
        native_requests,
        restoration_indices,
        simultaneous_copies,
        serialization_and_batch,
        root_scratch,
        allocator_physical,
        database_rows: if no_write {
            NativeCoverageV1::zero()
        } else {
            product::<Vec<u8>>(canonical_bytes, 2, "tip and pruning singleton payloads")
        },
        database_history: if no_write {
            NativeCoverageV1::zero()
        } else {
            NativeCoverageV1::unsupported::<PalwChainStateV2>(
                "DbPalwStateV2Store::deltas",
                "existing per-block delta/history rows and archival retention have no declared finite whole-DB installation envelope",
            )
        },
        database_wal: if no_write {
            NativeCoverageV1::zero()
        } else {
            NativeCoverageV1::unsupported::<PalwChainStateV2>(
                "rocksdb::WriteBatch/WAL",
                "synchronized commit does not bound concurrently retained WAL/log files across all database column families",
            )
        },
        database_compaction: if no_write {
            NativeCoverageV1::zero()
        } else {
            NativeCoverageV1::unsupported::<PalwChainStateV2>(
                "rocksdb::DB::compaction",
                "input SST versions, concurrent compaction outputs and retained live files have no enforced DB hard-quota evidence provider",
            )
        },
    }
}
/// Every private reconstruction table is named; primary tables stay in the
/// overall request inventory. Exhaustive state visitor still covers new fields.
const RESTORE_INDEX_FIELDS_V1: &[&str] = &[
    ".exec_v2_job_use_deadlines",
    ".exec_v2_reward_payees",
    ".staged_holds",
    ".staged_due",
    ".staged_by_owner",
    ".improvement_due",
    ".improvement_heads_of",
    ".improvement_retiring",
    ".improvement_open_epochs",
    ".improvement_live_results",
    ".improvement_artifact_outputs",
    ".improvement_artifact_answers",
    ".improvement_artifact_deadlines",
    ".improvement_material_epochs",
    ".improvement_opt_in_expiries",
    ".improvement_licence_expiries",
    ".improvement_eval_claims",
    ".deadlines",
    ".unresolved",
    ".work_ids",
    ".open_courts_by_claim",
    ".court_deadlines",
    ".court_close_deadlines",
    ".courts_by_challenger",
    ".commitments_by_reporter",
    ".commitments_by_daa",
    ".vesting_order",
    ".vesting_unlatched",
    ".vesting_unlatched_by_settled",
    ".vesting_payees",
    ".da_deadlines",
    ".da_by_accuser",
    ".legacy_dispute_deadlines",
    ".legacy_disputes_by_reserver",
    ".capacity_weight_index",
];
