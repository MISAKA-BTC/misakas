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
impl NativeDemandCheckerV1 {
    pub fn new(limits: NativeSnapshotResourceLimitsV1) -> Arc<Self> {
        Arc::new(Self { limits, state: Mutex::new(OutstandingV1::default()) })
    }
    pub fn reserve(self: &Arc<Self>, inventory: &NativeSnapshotInventoryV1) -> Result<NativeDemandLeaseV1, NativeCoverageV1> {
        let memory = inventory.logical_memory_v1();
        let memory = memory.bytes().ok_or(memory)?;
        let database = inventory.database_v1();
        let database = database.bytes().ok_or(database)?;
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
    let carriage = crate::palw_state_v2::PalwStateCarriageV2::from_state(state);
    let mut count = Count(0);
    borsh::to_writer(&mut count, &carriage).map_err(|_| NativeCoverageV1::overflow::<Count>("canonical installation byte count"))?;
    reserve_native_snapshot_v1(point, Ok(inventory_accepted_state_v1(state, point, count.0, NativeSnapshotOperationV1::Install)))
}
/// Until a structural wire preflight is available, no peer-provided byte length
/// or resource inventory is a trusted receiver allocation bound. This refuses
/// even if a local Serve inventory for the advertised point exists.
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
    use super::*;
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
