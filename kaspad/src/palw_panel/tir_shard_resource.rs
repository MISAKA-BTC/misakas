//! Resource admission shared by cell seats, shard-only holders and public watchers.
//!
//! This is node policy. A refusal leaves the duty retryable and changes no consensus verdict.
//! The capture scratch envelope is the existing cell-seat policy, applied to every consumer;
//! it is not a measured worst-case RAM or court-cost certificate for an arbitrary class.

use std::sync::Arc;

use misaka_palw_sdk::lineages::tir::{KernelBackendV1, TirCaptureV1, tir_kernel_backend_v1};

use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryReservationKeyV1, PalwMemoryReservationV1};

use super::{PALW_TIR_SHARD_CELL_ROLE_V1, PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1};

pub(super) const PALW_TIR_SHARD_WATCH_ROLE_V1: &str = "tir-shard-watch";
pub(super) const PALW_TIR_SHARD_CLOSE_ROLE_V1: &str = "tir-shard-close";
pub(super) const PALW_TIR_SHARD_ROWS_ROLE_V1: &str = "tir-shard-rows";

/// Data and its reservation share one owner. Evicting a cache entry must not release a
/// reservation while an in-flight verifier/proof still owns an Arc to the same rows.
pub(in crate::palw_panel) struct ReservedRowsV1<T> {
    data: T,
    _memory: PalwMemoryReservationV1,
}

impl<T> ReservedRowsV1<T> {
    pub(in crate::palw_panel) fn new(data: T, memory: PalwMemoryReservationV1) -> Self {
        Self { data, _memory: memory }
    }
}

impl<T> std::ops::Deref for ReservedRowsV1<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.data
    }
}

/// Node-policy envelope for fetching and constructing one shard's held rows. It includes
/// the i128 MapParams construction, duplicate encoded rows/openings and native params,
/// per-leaf paths/collection overhead, and program/plan metadata. This estimate is held
/// for the cache's entire lifetime; it is not a measured RAM certificate or a consensus price.
/// Geometry and inventory spans are computed without allocating a list of every leaf.
pub(in crate::palw_panel) fn row_cache_need_v1(
    class: &kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1,
    s_l: u16,
    shard: u16,
) -> Result<u64, String> {
    use kaspa_consensus_core::palw_tir_artifact_v1::{
        palw_tir_instance_leaves_v1, palw_tir_inventory_leaf_count_v1, palw_tir_param_instances_v1, palw_tir_tensor_bytes_v1,
    };
    use kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_inventory_ranges_v1;
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(class).map_err(|e| e.to_string())?;
    let p = &space.program;
    palw_tir_inventory_leaf_count_v1(p).map_err(|e| e.to_string())?;
    let (parts, _) = misaka_palw_sdk::lineages::tir::tir_shard_geometry_over_v1(&space, s_l)?;
    let layers = parts.get(usize::from(shard)).cloned().ok_or_else(|| format!("shard {shard} of {s_l}"))?;
    let ranges = palw_tir_shard_inventory_ranges_v1(p, layers, shard == 0, shard + 1 == s_l);
    let (mut cursor, mut raw, mut leaves) = (0u64, 0u64, 0u64);
    for (j, instances) in palw_tir_param_instances_v1(p).iter().enumerate() {
        let count = palw_tir_instance_leaves_v1(p, j as u16);
        for _ in instances {
            let end = cursor.checked_add(count).ok_or("the row inventory size exceeds u64")?;
            if ranges.iter().any(|r| u64::from(r.start) <= cursor && end <= u64::from(r.end)) {
                raw = raw.checked_add(palw_tir_tensor_bytes_v1(p, j as u16)).ok_or("the row byte size exceeds u64")?;
                leaves = leaves.checked_add(count).ok_or("the row inventory size exceeds u64")?;
            }
            cursor = end;
        }
    }
    row_cache_envelope_v1(raw, leaves, class.program.len() as u64)
}

fn row_cache_envelope_v1(raw: u64, leaves: u64, program: u64) -> Result<u64, String> {
    raw.checked_mul(32)
        .and_then(|n| leaves.checked_mul(8192).and_then(|x| n.checked_add(x)))
        .and_then(|n| program.checked_mul(64).and_then(|x| n.checked_add(x)))
        .and_then(|n| n.checked_add(PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1))
        .ok_or_else(|| "the rows' construction size overflows the memory ledger".into())
}

pub(in crate::palw_panel) fn reserve_rows_v1(
    host: &Arc<PalwMemoryLedgerV1>,
    duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
    class: &kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1,
) -> Result<PalwMemoryReservationV1, String> {
    let place = duty.tir_shard.ok_or("the row fetch has no shard duty")?;
    let need = row_cache_need_v1(class, place.s_l, place.shard)?;
    let key = PalwMemoryReservationKeyV1 { role: PALW_TIR_SHARD_ROWS_ROLE_V1, class_id: duty.class_id, job: duty.claim_id };
    host.reserve(key, need).map_err(|r| format!("the rows' construction/cache is not free ({r})"))
}

pub(super) fn scratch_need_v1(capture_bytes: u64) -> Result<u64, String> {
    capture_bytes
        .checked_mul(3)
        .and_then(|n| n.checked_add(PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1))
        .ok_or_else(|| "the cells' scratch size overflows the memory ledger".to_string())
}

/// Count an already decoded capture without serializing another copy before admission.
/// Includes collection entries, lanes, prompt/generated ids and the encoded class program.
pub(super) fn capture_bytes_v1(capture: &TirCaptureV1) -> Result<u64, String> {
    let count = |n: usize| u64::try_from(n).map_err(|_| "the capture size exceeds u64".to_string());
    let mut bytes = count(capture.binding.class.program.len())?;
    for leaf in &capture.leaves {
        bytes =
            bytes.checked_add(count(leaf.values_le.len())?).and_then(|n| n.checked_add(128)).ok_or("the capture size exceeds u64")?;
    }
    for n in [capture.prompt.len(), capture.generated.len()] {
        bytes = bytes
            .checked_add(count(n)?.checked_mul(4).ok_or("the capture size exceeds u64")?)
            .ok_or("the capture size exceeds u64")?;
    }
    Ok(bytes)
}

pub(super) struct CellReservationV1 {
    // Keep the guards through verification/proof building, including error and unwind paths.
    _host: PalwMemoryReservationV1,
    _device: Option<PalwMemoryReservationV1>,
    pub backend: Box<dyn KernelBackendV1 + Send>,
}

pub(super) fn reserve_v1(
    host: &Arc<PalwMemoryLedgerV1>,
    key: PalwMemoryReservationKeyV1,
    capture_bytes: u64,
    device_bytes: u64,
    want_device: bool,
) -> Result<CellReservationV1, String> {
    reserve_with_backend_v1(host, key, capture_bytes, device_bytes, || tir_kernel_backend_v1(want_device))
}

fn reserve_with_backend_v1(
    host: &Arc<PalwMemoryLedgerV1>,
    key: PalwMemoryReservationKeyV1,
    capture_bytes: u64,
    device_bytes: u64,
    create_backend: impl FnOnce() -> (Box<dyn KernelBackendV1 + Send>, bool),
) -> Result<CellReservationV1, String> {
    let need = scratch_need_v1(capture_bytes)?;
    let held = host.reserve(key.clone(), need).map_err(|r| format!("the cells' scratch is not free ({r})"))?;
    // A device plug-in may allocate while constructing its backend: host admission comes first.
    let (mut backend, on_device) = create_backend();
    let mut device = None;
    if on_device {
        if let Some(capacity) = backend.device_capacity_bytes() {
            crate::palw_memory_ledger::arm_device_share_v1(0, capacity);
        }
        match crate::palw_memory_ledger::device_ledger_v1(0).map(|l| l.reserve(key, device_bytes)) {
            Some(Ok(held)) => device = Some(held),
            _ => backend = Box::new(misaka_palw_sdk::lineages::tir::CpuKernelBackendV1),
        }
    }
    Ok(CellReservationV1 { _host: held, _device: device, backend })
}

pub(super) fn key_v1(role: &'static str, duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2) -> PalwMemoryReservationKeyV1 {
    debug_assert!([PALW_TIR_SHARD_CELL_ROLE_V1, PALW_TIR_SHARD_WATCH_ROLE_V1, PALW_TIR_SHARD_CLOSE_ROLE_V1].contains(&role));
    PalwMemoryReservationKeyV1 { role, class_id: duty.class_id, job: duty.claim_id }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_memory_ledger::PalwMemoryPoolV1;
    use kaspa_consensus_core::Hash64;

    fn key(role: &'static str) -> PalwMemoryReservationKeyV1 {
        PalwMemoryReservationKeyV1 { role, class_id: Hash64::from_u64_word(7), job: Hash64::from_u64_word(9) }
    }
    fn pool(bytes: u64) -> Arc<PalwMemoryLedgerV1> {
        PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(bytes), || None)
    }

    #[test]
    fn cache_eviction_retains_memory_until_the_last_verifier_releases_the_rows() {
        let ledger = pool(4096);
        let held = ledger.reserve(key(PALW_TIR_SHARD_ROWS_ROLE_V1), 4096).unwrap();
        let cache = Arc::new(ReservedRowsV1::new(7u32, held));
        let verifier = cache.clone();
        drop(cache);
        assert_eq!(**verifier, 7);
        assert_eq!(ledger.reserved_bytes(), 4096);
        assert!(ledger.reserve(key(PALW_TIR_SHARD_ROWS_ROLE_V1), 1).is_err());
        drop(verifier);
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn row_profile_overflow_refuses_instead_of_saturating() {
        for (raw, leaves, program) in [(u64::MAX, 0, 0), (0, u64::MAX, 0), (0, 0, u64::MAX)] {
            assert!(row_cache_envelope_v1(raw, leaves, program).is_err());
        }
        assert_eq!(row_cache_envelope_v1(0, 0, 0).unwrap(), PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1);
    }

    #[test]
    fn seats_holders_watchers_and_proofs_share_the_same_outstanding_host_budget() {
        let need = scratch_need_v1(1024).unwrap();
        let ledger = pool(need * 2);
        let seat = reserve_v1(&ledger, key(PALW_TIR_SHARD_CELL_ROLE_V1), 1024, 0, false).unwrap();
        let watcher = reserve_v1(&ledger, key(PALW_TIR_SHARD_WATCH_ROLE_V1), 1024, 0, false).unwrap();
        assert_eq!(ledger.reserved_bytes(), need * 2);
        let refusal = reserve_v1(&ledger, key(PALW_TIR_SHARD_CLOSE_ROLE_V1), 1024, 0, false).err().unwrap();
        assert!(refusal.contains("scratch is not free"));
        assert!(refusal.contains(PALW_TIR_SHARD_WATCH_ROLE_V1));
        drop(watcher);
        let holder = reserve_v1(&ledger, key(PALW_TIR_SHARD_CELL_ROLE_V1), 1024, 0, false).unwrap();
        assert_eq!(ledger.reserved_bytes(), need * 2);
        drop((seat, holder));
        assert_eq!(ledger.reserved_bytes(), 0);
        assert!(reserve_v1(&ledger, key(PALW_TIR_SHARD_CLOSE_ROLE_V1), 1024, 0, false).is_ok());
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn a_refused_host_does_not_construct_a_device_or_cpu_backend() {
        let ledger = pool(0);
        let called = std::cell::Cell::new(false);
        let refused = reserve_with_backend_v1(&ledger, key(PALW_TIR_SHARD_WATCH_ROLE_V1), 0, 0, || {
            called.set(true);
            (Box::new(misaka_palw_sdk::lineages::tir::CpuKernelBackendV1), false)
        });
        assert!(refused.is_err());
        assert!(!called.get());
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn scratch_overflow_refuses_without_reserving_or_saturating() {
        let ledger = pool(u64::MAX);
        assert!(reserve_v1(&ledger, key(PALW_TIR_SHARD_WATCH_ROLE_V1), u64::MAX, 0, false).is_err());
        assert_eq!(ledger.reserved_bytes(), 0);
        let last = (u64::MAX - PALW_TIR_SHARD_CELL_SCRATCH_BYTES_V1) / 3;
        assert!(scratch_need_v1(last).is_ok());
        assert!(scratch_need_v1(last + 1).is_err());
    }

    #[test]
    fn verification_error_and_unwind_release_the_common_admission() {
        let need = scratch_need_v1(0).unwrap();
        let ledger = pool(need);
        let run = || -> Result<(), String> {
            let _held = reserve_v1(&ledger, key(PALW_TIR_SHARD_WATCH_ROLE_V1), 0, 0, false)?;
            Err("invalid authenticated material".into())
        };
        assert!(run().is_err());
        assert_eq!(ledger.reserved_bytes(), 0);
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = reserve_v1(&ledger, key(PALW_TIR_SHARD_CLOSE_ROLE_V1), 0, 0, false).unwrap();
            panic!("the proof builder failed");
        }));
        assert!(unwind.is_err());
        assert_eq!(ledger.reserved_bytes(), 0);
    }
}
