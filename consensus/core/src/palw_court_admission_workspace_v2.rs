//! Construction envelope for the existing active-capacity ingress. The raw
//! scheduler, pool classification, ordering and fee rules remain authoritative.
use super::super::native_writers::NativeWriteQuoteV1;
use super::*;

fn add(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_add(b).ok_or_else(|| {
        PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<PalwCourtPoolV1>(
            "court admission cardinality",
        ))
    })
}
fn growing_peak<T>(q: &mut NativeWriteQuoteV1, n: u64) -> Result<(), PalwStateV2Error> {
    // The existing typed Vec envelope covers capacity up to 2*max(n,min).
    // Hold both the old and replacement allocation during a possible growth.
    q.grown::<T>(n)?.grown::<T>(n)?;
    Ok(())
}
pub(super) fn quote(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim: Hash64,
    challenger: PalwBondKeyV2,
    now: u64,
) -> Result<NativeWriteQuoteV1, PalwStateV2Error> {
    let mut q = NativeWriteQuoteV1::new("court admission pools, schedule and input copies");
    let Some(record) = state.legacy_disputes.get(&claim).filter(|r| r.live.contains_key(&challenger)) else { return Ok(q) };
    q.copy(record)?;
    // Existing rows return before pool/schedule construction, including tickets.
    if record.courts.contains_key(&challenger) {
        return Ok(q);
    }
    q.member::<PalwBondKeyV2, PalwReservedCourtV1>(1)?;
    if court_ingress_schedule_at_v1(params, now).is_some()
        && let Some(bond) = state.bonds.get(&challenger)
    {
        // charge_court_ingress_fee -> slash_bond clones before its writer.
        q.copy(bond)?;
    }
    let mut tickets = [0u64; 2];
    let mut guaranteed = [0u64; 2];
    let mut best = [0u64; 2];
    let mut reserve_sessions = 0u64;
    for record in state.legacy_disputes.values() {
        for row in record.courts.values() {
            let pool = usize::from(!row.reserve);
            match row.stage {
                PalwCourtStageV1::Ticket { .. } => tickets[pool] = add(tickets[pool], 1)?,
                PalwCourtStageV1::Queued { .. } if row.guaranteed => guaranteed[pool] = add(guaranteed[pool], 1)?,
                PalwCourtStageV1::Queued { .. } => best[pool] = add(best[pool], 1)?,
                PalwCourtStageV1::Running { .. } if row.reserve => reserve_sessions = add(reserve_sessions, 1)?,
                PalwCourtStageV1::Running { .. } => {}
            }
        }
    }
    q.member::<Hash64, ()>(reserve_sessions)?;
    let sessions = state.court_sessions.len() as u64;
    let mut releases = [0u64; 2];
    for pool in 0..2 {
        // Any native session can be assigned to either pool. This avoids an
        // allocating classification merely to quote its classification set.
        releases[pool] = add(tickets[pool], sessions)?;
        growing_peak::<u64>(&mut q, releases[pool])?;
        growing_peak::<PalwCourtQueuedV1>(&mut q, guaranteed[pool])?;
        growing_peak::<PalwCourtQueuedV1>(&mut q, best[pool])?;
    }
    // Only one selected pool is cloned. Admit its type-sized maximum; both
    // original pools above remain live while that clone and scheduler run.
    let queue = add(guaranteed[0].max(guaranteed[1]), 1)?;
    q.exact::<PalwCourtQueuedV1>(guaranteed[0].max(guaranteed[1]))?;
    growing_peak::<PalwCourtQueuedV1>(&mut q, queue)?;
    let release = releases[0].max(releases[1]);
    q.exact::<u64>(release)?;
    // palw_court_schedule: releases heap plus <= queue.len() usable free
    // slots; ordered (index,&entry) and a full-size stable-sort scratch; grants.
    growing_peak::<std::cmp::Reverse<u64>>(&mut q, add(release, queue)?)?;
    growing_peak::<(usize, &PalwCourtQueuedV1)>(&mut q, queue)?;
    q.exact::<(usize, &PalwCourtQueuedV1)>(queue)?;
    q.exact::<u64>(queue)?;
    Ok(q)
}
