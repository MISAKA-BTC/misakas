//! Read-only historical-header census with owned typed-list capacity.
use crate::{
    Hash64,
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_state_v2::{PalwChainStateV2, PalwClaimSourceV2},
    palw_work_slice_v2::PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1,
};
use std::{mem::size_of, ops::Deref};

/// **RFC-0006 historical-header coverage census (read-only).** Before the EXEC origin migration may be armed, every
/// eligible legacy claim (an attempt claim with no recorded original clock) must have its original accepted header either
/// within the carriage budget above or covered by a separately bounded transport, and every original header must still be
/// readable somewhere before pruning destroys it. A node reports, from its own store, how its eligible history stands.
/// `unavailable` (header pruned/absent here) is not proof that no archive holds it; an archive node's census is what
/// proves coverage. Neither list authorizes anything.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct PalwExecOriginCoverageV1 {
    pub cap_bytes: u64,
    pub eligible: u64,
    pub within_cap: u64,
    pub max_bytes_seen: u64,
    /// `(claim, accepted block, encoded header bytes)` above the cap; at most `listed` entries are kept.
    pub oversized: Vec<(Hash64, Hash64, u64)>,
    /// `(claim, accepted block)` whose header this node cannot read; at most `listed` entries are kept.
    pub unavailable: Vec<(Hash64, Hash64)>,
    pub oversized_total: u64,
    pub unavailable_total: u64,
}

/// Holds the reservation until the complete census has been consumed. No
/// consuming extraction or Clone detaches the two typed result lists.
pub struct PalwExecOriginCoverageOwnedV1 {
    census: PalwExecOriginCoverageV1,
    _lease: Option<NativeDemandLeaseV1>,
}
impl Deref for PalwExecOriginCoverageOwnedV1 {
    type Target = PalwExecOriginCoverageV1;
    fn deref(&self) -> &Self::Target {
        &self.census
    }
}

/// Typed result storage only. The source state and header reader/cache need
/// their own owners. Empty lists do not allocate and require no lease.
fn list_requests(listed: usize) -> Result<u64, NativeCoverageV1> {
    if listed == 0 {
        return Ok(0);
    }
    let row = size_of::<(Hash64, Hash64, u64)>() + size_of::<(Hash64, Hash64)>();
    listed
        .checked_mul(row)
        .and_then(|n| n.checked_add(size_of::<PalwExecOriginCoverageOwnedV1>()))
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| NativeCoverageV1::overflow::<PalwExecOriginCoverageOwnedV1>("historical header census lists"))
}

/// Eligible: Attempt-sourced claims whose original clock is not yet recorded.
/// Admission runs before list allocation or any header read. The read callback
/// reports encoded size without requiring a second serialized header buffer.
pub fn palw_exec_origin_coverage_v1(
    state: &PalwChainStateV2,
    listed: usize,
    header_bytes: impl FnMut(Hash64) -> Option<u64>,
) -> Result<PalwExecOriginCoverageOwnedV1, NativeCoverageV1> {
    let mut header_bytes = header_bytes;
    match palw_exec_origin_coverage_checked_v2(state, listed, |block| Ok::<_, std::convert::Infallible>(header_bytes(block))) {
        Ok(value) => Ok(value),
        Err(NativeDecodeErrorV1::Resource(error)) => Err(error),
        Err(NativeDecodeErrorV1::Decode(never)) => match never {},
    }
}

/// Fallible historical reader: only Ok(None) contributes to unavailable.
/// A local capacity, malformed row, or storage failure aborts the observation.
pub fn palw_exec_origin_coverage_checked_v2<E>(
    state: &PalwChainStateV2,
    listed: usize,
    header_bytes: impl FnMut(Hash64) -> Result<Option<u64>, E>,
) -> Result<PalwExecOriginCoverageOwnedV1, NativeDecodeErrorV1<E>> {
    let origins = || {
        state.claims_iter().filter_map(|(id, claim)| {
            (matches!(claim.source, PalwClaimSourceV2::Attempt) && state.exec_v2_origin_v1(id).is_none())
                .then_some((*id, claim.accepted_block))
        })
    };
    census_from_origins_checked(origins, listed, header_bytes)
}

fn census_from_origins_checked<E, I: Iterator<Item = (Hash64, Hash64)>>(
    origins: impl Fn() -> I,
    listed: usize,
    mut header_bytes: impl FnMut(Hash64) -> Result<Option<u64>, E>,
) -> Result<PalwExecOriginCoverageOwnedV1, NativeDecodeErrorV1<E>> {
    let capacity = origins().take(listed).count();
    let request = list_requests(capacity).map_err(NativeDecodeErrorV1::Resource)?;
    let lease = if request == 0 { None } else { reserve_native_requests_v1(request).map_err(NativeDecodeErrorV1::Resource)? };
    let cap = PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1 as u64;
    let mut census = PalwExecOriginCoverageV1 { cap_bytes: cap, ..Default::default() };
    census.oversized.try_reserve_exact(capacity).map_err(|e| {
        NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<PalwExecOriginCoverageOwnedV1>(
            "historical oversized list",
            &e.to_string(),
        ))
    })?;
    census.unavailable.try_reserve_exact(capacity).map_err(|e| {
        NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<PalwExecOriginCoverageOwnedV1>(
            "historical unavailable list",
            &e.to_string(),
        ))
    })?;
    for (id, block) in origins() {
        census.eligible += 1;
        match header_bytes(block).map_err(NativeDecodeErrorV1::Decode)? {
            Some(bytes) => {
                census.max_bytes_seen = census.max_bytes_seen.max(bytes);
                if bytes <= cap {
                    census.within_cap += 1;
                } else {
                    census.oversized_total += 1;
                    if census.oversized.len() < capacity {
                        census.oversized.push((id, block, bytes));
                    }
                }
            }
            None => {
                census.unavailable_total += 1;
                if census.unavailable.len() < capacity {
                    census.unavailable.push((id, block));
                }
            }
        }
    }
    Ok(PalwExecOriginCoverageOwnedV1 { census, _lease: lease })
}

#[cfg(test)]
fn census_from_origins<I: Iterator<Item = (Hash64, Hash64)>>(
    origins: impl Fn() -> I,
    listed: usize,
    mut header_bytes: impl FnMut(Hash64) -> Option<u64>,
) -> Result<PalwExecOriginCoverageOwnedV1, NativeCoverageV1> {
    match census_from_origins_checked(origins, listed, |block| Ok::<_, std::convert::Infallible>(header_bytes(block))) {
        Ok(value) => Ok(value),
        Err(NativeDecodeErrorV1::Resource(error)) => Err(error),
        Err(NativeDecodeErrorV1::Decode(never)) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};

    #[test]
    fn historical_read_fault_aborts_instead_of_becoming_unavailable() {
        #[derive(Debug, PartialEq)]
        enum Fault {
            Capacity,
            Corrupt,
            Storage,
        }
        for fault in [Fault::Capacity, Fault::Corrupt, Fault::Storage] {
            let mut fault = Some(fault);
            let mut calls = 0;
            let result = census_from_origins_checked(
                || (0..3).map(|n| (Hash64::from_u64_word(n), Hash64::from_u64_word(n))),
                0,
                |_| {
                    calls += 1;
                    if calls == 1 { Ok(None) } else { Err(fault.take().unwrap()) }
                },
            );
            assert!(matches!(result, Err(NativeDecodeErrorV1::Decode(_))));
            assert_eq!(calls, 2, "stop at the failing read rather than report a partial census");
        }
    }

    #[test]
    fn empty_native_history_needs_no_reader_or_list_allocation() {
        let result =
            palw_exec_origin_coverage_v1(&PalwChainStateV2::genesis(), usize::MAX, |_| panic!("no historical claims")).unwrap();
        assert_eq!(result.eligible, 0);
        assert_eq!(result.oversized.capacity(), 0);
        assert_eq!(result.unavailable.capacity(), 0);
    }

    #[test]
    fn counts_continue_after_named_lists_are_full() {
        for listed in [0, 1, 2, usize::MAX] {
            let origins = (0..6).map(|n| (Hash64::from_u64_word(n), Hash64::from_u64_word(n)));
            let mut index = 0;
            let census = census_from_origins(
                || origins.clone(),
                listed,
                |_| {
                    index += 1;
                    match index % 3 {
                        0 => Some(PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1 as u64),
                        1 => None,
                        _ => Some(1 + PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1 as u64),
                    }
                },
            )
            .unwrap();
            assert_eq!((census.eligible, census.within_cap, census.oversized_total, census.unavailable_total), (6, 2, 2, 2));
            assert_eq!(census.oversized.len(), listed.min(2));
            assert_eq!(census.unavailable.len(), listed.min(2));
            assert_eq!(index, 6);
        }
        assert!(list_requests(usize::MAX).is_err());
    }

    #[test]
    fn census_capacity_child() {
        let Ok(case) = std::env::var("MISAKA_CENSUS_CAPACITY_CASE") else {
            return;
        };
        let empty = case == "empty";
        let listed = if case == "totals" { 0 } else { 2 };
        let count = if empty { 0 } else { 6 };
        let request = list_requests(listed.min(count)).unwrap();
        let memory = match case.as_str() {
            "zero" | "empty" | "totals" => 0,
            "below" => request - 1,
            _ => request,
        };
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        let origins = (0..count).map(|n| (Hash64::from_u64_word(n as u64), Hash64::from_u64_word(n as u64)));
        let mut reads = 0;
        let result = census_from_origins(
            || origins.clone(),
            listed,
            |_| {
                reads += 1;
                None
            },
        );
        if case == "zero" || case == "below" {
            assert!(result.is_err());
            assert_eq!(reads, 0, "refusal precedes header reads");
        } else {
            let census = result.unwrap();
            assert_eq!(reads, count);
            assert_eq!(census.unavailable_total, count as u64);
            assert_eq!(census.unavailable.len(), listed.min(count));
            if request > 0 {
                assert!(reserve_native_requests_v1(1).is_err(), "owner retains its ticket");
            }
            drop(census);
        }
        // A failed construction or dropped result must release its whole ticket.
        assert!(reserve_native_requests_v1(memory).is_ok());
    }

    #[test]
    fn census_capacity_boundaries_and_lifetime() {
        for case in ["zero", "below", "exact", "empty", "totals"] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "palw_exec_origin_coverage_v1::tests::census_capacity_child", "--nocapture"])
                .env("MISAKA_CENSUS_CAPACITY_CASE", case)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{case}: {} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"), "child test was not selected");
        }
    }
}
