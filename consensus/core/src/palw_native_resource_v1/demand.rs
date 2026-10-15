//! Bounded prototype demand ledger for native operations. It accounts trusted
//! typed allocation requests; it never interprets that as physical allocator RSS.
use super::*;
use std::sync::{Arc, Mutex, OnceLock};
#[derive(Default)]
struct OutstandingV1 {
    memory: u64,
    database: u64,
}
pub struct NativeDemandCheckerV1 {
    limits: NativeSnapshotResourceLimitsV1,
    state: Mutex<OutstandingV1>,
}
pub struct NativeDemandLeaseV1 {
    checker: Arc<NativeDemandCheckerV1>,
    memory: u64,
    database: u64,
}
impl NativeDemandLeaseV1 {
    /// Transfer part of this already admitted memory reservation to another
    /// owner. The sum of both tickets remains unchanged; database reservation
    /// stays with the original owner. Refusal changes neither ticket nor ledger.
    pub fn split_memory_v1(&mut self, amount: u64) -> Result<Self, NativeCoverageV1> {
        let remaining = self.memory.checked_sub(amount).ok_or_else(|| {
            NativeCoverageV1::unsupported::<Self>("native owner ticket split", "transfer exceeds the owner's memory reservation")
        })?;
        let transferred = Self { checker: Arc::clone(&self.checker), memory: amount, database: 0 };
        self.memory = remaining;
        Ok(transferred)
    }
    /// The typed memory this ticket holds (RFC-0006 RFC06-1: an owner's admitted size bounds the scratch derived from it).
    pub(crate) fn memory_v1(&self) -> u64 {
        self.memory
    }
    /// Extend an existing owner before its next allocation. Reuse the same
    /// ticket so tracking an unbounded walk does not itself allocate a second
    /// collection whose ownership would need another reservation.
    pub fn grow_memory_v1(&mut self, additional: u64) -> Result<(), NativeCoverageV1> {
        let total = self.memory.checked_add(additional).ok_or_else(|| NativeCoverageV1::overflow::<Self>("growing native owner"))?;
        let mut extra = self.checker.reserve_requests_v1(additional, 0)?;
        self.memory = total;
        // The reservation is now owned by self. Dropping the temporary must
        // not return the transferred demand to the shared checker.
        extra.memory = 0;
        Ok(())
    }
}
impl NativeDemandCheckerV1 {
    pub fn new(limits: NativeSnapshotResourceLimitsV1) -> Arc<Self> {
        Arc::new(Self { limits, state: Mutex::new(OutstandingV1::default()) })
    }
    pub fn reserve(self: &Arc<Self>, inventory: &NativeSnapshotInventoryV1) -> Result<NativeDemandLeaseV1, NativeCoverageV1> {
        let memory = inventory.logical_memory_v1();
        let memory = memory.bytes().ok_or(memory)?;
        let database = inventory.database_v1();
        let database = database.bytes().ok_or(database)?;
        self.reserve_requests_v1(memory, database)
    }
    pub(crate) fn reserve_requests_v1(self: &Arc<Self>, memory: u64, database: u64) -> Result<NativeDemandLeaseV1, NativeCoverageV1> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| NativeCoverageV1::unsupported::<Self>("logical demand ledger", "poisoned reservation state"))?;
        let next_memory =
            state.memory.checked_add(memory).ok_or_else(|| NativeCoverageV1::overflow::<Self>("simultaneous logical RAM requests"))?;
        let next_database = state
            .database
            .checked_add(database)
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("simultaneous logical DB requests"))?;
        if native_resource_admission_enabled_v1() {
            let quota = actual_hard_memory_quota_v1()?;
            if next_memory > quota.headroom_bytes() {
                return Err(NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>(
                    "aggregate physical quota headroom",
                    "all outstanding typed requests exceed currently observed enforced quota headroom",
                ));
            }
        }
        if next_memory > self.limits.memory_bytes || next_database > self.limits.database_bytes {
            return Err(NativeCoverageV1::unsupported::<Self>(
                "logical native capacity",
                "trusted typed requests plus outstanding operations exceed declared logical capacity",
            ));
        }
        state.memory = next_memory;
        state.database = next_database;
        Ok(NativeDemandLeaseV1 { checker: Arc::clone(self), memory, database })
    }
    pub fn outstanding(&self) -> (u64, u64) {
        let state = self.state.lock().expect("native demand ledger lock");
        (state.memory, state.database)
    }
}
impl Drop for NativeDemandLeaseV1 {
    fn drop(&mut self) {
        if let Ok(mut state) = self.checker.state.lock() {
            state.memory = state.memory.saturating_sub(self.memory);
            state.database = state.database.saturating_sub(self.database);
        }
    }
}
static LOGICAL: OnceLock<Option<Arc<NativeDemandCheckerV1>>> = OnceLock::new();
pub fn arm_native_logical_limits_v1(limits: Option<NativeSnapshotResourceLimitsV1>) {
    let _ = LOGICAL.set(limits.map(NativeDemandCheckerV1::new));
}
/// Explicit local configuration only. This is not an observed free-memory or
/// filesystem quota and does not authenticate a caller's resource estimate.
pub fn native_declared_logical_limits_v1() -> Option<NativeSnapshotResourceLimitsV1> {
    LOGICAL.get().and_then(Option::as_ref).map(|checker| checker.limits)
}
pub fn native_logical_admission_enabled_v1() -> bool {
    LOGICAL.get().is_some_and(Option::is_some)
}
pub fn reserve_native_snapshot_v1(
    point: BlockHash,
    inventory: Result<NativeSnapshotInventoryV1, NativeCoverageV1>,
) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    if !native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1() {
        return Ok(None);
    }
    let inventory = inventory?;
    if inventory.version != 1 || inventory.point != point {
        return Err(NativeCoverageV1::unsupported::<NativeSnapshotInventoryV1>(
            "point binding",
            "inventory version/accepted point mismatch",
        ));
    }
    if native_resource_admission_enabled_v1() {
        let bound = inventory.physical_bound_v1()?;
        admit_native_snapshot_v1(point, Some(bound))
            .map_err(|why| NativeCoverageV1::unsupported::<NativeSnapshotInventoryV1>("physical native capacity", &why))?;
    }
    LOGICAL.get().and_then(Option::as_ref).map(|checker| checker.reserve(&inventory)).transpose()
}

/// Reserve installation from the validated, actually reconstructed native state.
/// A peer/local point label or a Serve inventory cannot authorize an installation.
/// Call after state/root validation and before the first store mutation, retaining
/// the returned lease until the synchronous batch commit has completed.
pub fn reserve_native_install_v1(
    point: BlockHash,
    state: &crate::palw_state_v2::PalwChainStateV2,
) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    reserve_native_install_in_database_v1(point, state, None)
}
pub fn reserve_native_install_in_database_v1(
    point: BlockHash,
    state: &crate::palw_state_v2::PalwChainStateV2,
    database_path: Option<&std::path::Path>,
) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    if !native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1() {
        return Ok(None);
    }
    struct Count(u64);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 =
                self.0.checked_add(bytes.len() as u64).ok_or_else(|| std::io::Error::other("native canonical byte count overflow"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // from_state deep-copies the primary tables before the counting writer can
    // determine their canonical length. Reserve that actual typed copy first;
    // the later installation inventory cannot retroactively cover allocation.
    let copy_bound = state.native_heap_v1("native installation carriage copy").add::<crate::palw_state_v2::PalwStateCarriageV2>(
        NativeCoverageV1::Bounded { bytes: std::mem::size_of::<crate::palw_state_v2::PalwStateCarriageV2>() as u64 },
        "native installation carriage owner",
    );
    let _source_copy_lease = reserve_native_requests_v1(copy_bound.bytes().ok_or(copy_bound)?)?;
    let carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(state);
    let mut count = Count(0);
    borsh::to_writer(&mut count, &carriage).map_err(|_| NativeCoverageV1::overflow::<Count>("canonical installation byte count"))?;
    let inventory = inventory_accepted_state_v1(state, point, count.0, NativeSnapshotOperationV1::Install);
    let memory = inventory.logical_memory_v1();
    let memory = memory.bytes().ok_or(memory)?;
    let path = database_path.ok_or_else(|| {
        NativeCoverageV1::unsupported::<NativeHardDatabaseQuotaV1>(
            "actual database layout",
            "no audited single-directory RocksDB factory provenance; external WAL/table files may escape project quota",
        )
    })?;
    let quota = actual_hard_database_quota_v1(path)?;
    // This is whole physical disk coverage, including existing history, WAL and
    // compaction files; it does not promise a write succeeds under quota pressure.
    let database = quota.limit_bytes();
    let minimum_write = count.0.checked_mul(2).ok_or_else(|| NativeCoverageV1::overflow::<Vec<u8>>("tip/pruning singleton writes"))?;
    if minimum_write > quota.headroom_bytes() {
        return Err(NativeCoverageV1::unsupported::<NativeHardDatabaseQuotaV1>(
            "actual database quota headroom",
            "even the two canonical singleton payloads exceed observed quota headroom",
        ));
    }
    if native_resource_admission_enabled_v1() {
        let memory_quota = actual_hard_memory_quota_v1()?;
        if memory > memory_quota.headroom_bytes() {
            return Err(NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>(
                "actual install memory headroom",
                "typed requests exceed observed enforced quota headroom",
            ));
        }
        admit_native_snapshot_v1(
            point,
            Some(NativeSnapshotResourceBoundV1 {
                version: 1,
                point,
                memory_upper_bytes: memory_quota.limit_bytes(),
                database_upper_bytes: database,
            }),
        )
        .map_err(|why| NativeCoverageV1::unsupported::<NativeHardDatabaseQuotaV1>("actual native RAM/DB hard quotas", &why))?;
    }
    LOGICAL.get().and_then(Option::as_ref).map(|checker| checker.reserve_requests_v1(memory, database)).transpose()
}
/// Guard for callers with no actual structural wire preflight. A peer-provided
/// byte length/resource estimate and local Serve metadata cannot authorize an
/// arbitrary incoming shape. Protocol 107 uses reserve_native_wire_v1 instead.
pub fn admit_native_receive_v1(point: BlockHash) -> Result<(), NativeCoverageV1> {
    if !native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1() {
        return Ok(());
    }
    Err(NativeCoverageV1::unsupported::<crate::palw_state_v2::PalwStateCarriageV2>(
        "untrusted Borsh snapshot receive/restore",
        &format!(
            "no authenticated preallocation shape for incoming point {point}; local Serve metadata cannot bound arbitrary peer vectors/maps"
        ),
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn split_transfers_only_owned_memory_and_each_drop_returns_its_share() {
        let checker =
            super::NativeDemandCheckerV1::new(super::NativeSnapshotResourceLimitsV1 { memory_bytes: 100, database_bytes: 100 });
        let mut source = checker.reserve_requests_v1(80, 20).unwrap();
        assert!(source.split_memory_v1(81).is_err());
        assert_eq!(checker.outstanding(), (80, 20));
        let mut output = source.split_memory_v1(50).unwrap();
        assert_eq!(checker.outstanding(), (80, 20));
        assert!(source.split_memory_v1(31).is_err());
        output.grow_memory_v1(20).unwrap();
        assert_eq!(checker.outstanding(), (100, 20));
        drop(source);
        assert_eq!(checker.outstanding(), (70, 0));
        drop(output);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    use super::*;
    #[test]
    fn growing_owner_is_atomic_and_returns_the_complete_ticket() {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 64, database_bytes: 0 });
        let mut owner = checker.reserve_requests_v1(16, 0).unwrap();
        owner.grow_memory_v1(32).unwrap();
        assert_eq!(checker.outstanding(), (48, 0));
        assert!(owner.grow_memory_v1(17).is_err());
        assert_eq!(checker.outstanding(), (48, 0), "failed growth leaves the old reservation live");
        let sibling = checker.reserve_requests_v1(16, 0).unwrap();
        assert!(owner.grow_memory_v1(1).is_err());
        drop(sibling);
        owner.grow_memory_v1(16).unwrap();
        assert_eq!(checker.outstanding(), (64, 0));
        drop(owner);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn outstanding_sibling_reservations_release_and_install_unknown_is_refused() {
        let state = crate::palw_state_v2::PalwChainStateV2::genesis();
        let point = BlockHash::from_u64_word(1);
        let inventory = inventory_accepted_state_v1(&state, point, 16, NativeSnapshotOperationV1::Serve);
        let request = inventory.logical_memory_v1().bytes().unwrap();
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: request * 2, database_bytes: 0 });
        let first = checker.reserve(&inventory).unwrap();
        let second = checker.reserve(&inventory).unwrap();
        assert!(checker.reserve(&inventory).is_err());
        drop(first);
        assert_eq!(checker.outstanding(), (request, 0));
        let replacement = checker.reserve(&inventory).unwrap();
        drop(second);
        drop(replacement);
        assert_eq!(checker.outstanding(), (0, 0));
        let install = inventory_accepted_state_v1(&state, point, 16, NativeSnapshotOperationV1::Install);
        assert!(matches!(install.database_v1(), NativeCoverageV1::Unsupported { .. }));
        assert!(checker.reserve(&install).is_err());
    }
}

pub fn reserve_native_wire_v1(
    point: BlockHash,
    preflight: &super::wire::NativeWirePreflightV1,
) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    if !native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1() {
        return Ok(None);
    }
    let memory = preflight.memory_requests_v1().map_err(|why| {
        NativeCoverageV1::unsupported::<super::wire::NativeWirePreflightV1>("actual incoming Borsh shape", &why.to_string())
    })?;
    if native_resource_admission_enabled_v1() {
        let quota = actual_hard_memory_quota_v1()?;
        if memory > quota.headroom_bytes() {
            return Err(NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>(
                "actual incoming physical quota",
                "requested allocations exceed observed enforced quota headroom",
            ));
        }
        admit_native_snapshot_v1(
            point,
            Some(NativeSnapshotResourceBoundV1 {
                version: 1,
                point,
                memory_upper_bytes: quota.limit_bytes(),
                database_upper_bytes: 0,
            }),
        )
        .map_err(|why| NativeCoverageV1::unsupported::<NativeHardMemoryQuotaV1>("actual incoming physical quota", &why))?;
    }
    LOGICAL.get().and_then(Option::as_ref).map(|checker| checker.reserve_requests_v1(memory, 0)).transpose()
}

/// Logical request reservation for owner-audited native consumers. The caller
/// must derive requests from actual typed objects or an actual codec preflight.
/// Allocation request admission for owner-audited typed callers. A caller must
/// establish its actual schema bound before construction and retain the lease
/// through every owning cache/consumer; raw encoded lengths are not that bound.
pub fn reserve_native_requests_v1(memory: u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    if !native_logical_admission_enabled_v1() && !native_resource_admission_enabled_v1() {
        return Ok(None);
    }
    let checker = LOGICAL.get().and_then(Option::as_ref).ok_or_else(|| {
        NativeCoverageV1::unsupported::<NativeDemandCheckerV1>("native consumer reservation", "capacity checker is not armed")
    })?;
    checker.reserve_requests_v1(memory, 0).map(Some)
}
