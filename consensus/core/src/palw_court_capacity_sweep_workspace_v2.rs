//! Typed caller scratch for the existing capacity settlement/ticket sweep.
use super::super::native_writers::NativeWriteQuoteV1;
use super::*;

fn grown<T>(q: &mut NativeWriteQuoteV1, n: u64) -> Result<(), PalwStateV2Error> {
    if n > 0 {
        let slots = n.checked_mul(4).ok_or_else(|| {
            PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Vec<T>>(
                "court capacity sweep Vec growth",
            ))
        })?;
        q.exact::<T>(slots.max(8))?;
    }
    Ok(())
}
fn add(n: &mut u64, v: u64) -> Result<(), PalwStateV2Error> {
    *n = n.checked_add(v).ok_or_else(|| {
        PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<PalwCourtPoolV1>(
            "court capacity sweep cardinality",
        ))
    })?;
    Ok(())
}
pub(super) fn quote(state: &PalwChainStateV2) -> Result<NativeWriteQuoteV1, PalwStateV2Error> {
    let mut q = NativeWriteQuoteV1::new("court capacity settlement and ticket caller workspace");
    let mut rows = 0;
    let mut tickets = [0u64; 2];
    let mut guaranteed = [0u64; 2];
    let mut best = [0u64; 2];
    for record in state.legacy_disputes.values() {
        if !record.courts.is_empty() {
            q.copy(record)?;
        }
        add(&mut rows, record.courts.len() as u64)?;
        for (reserver, row) in &record.courts {
            // Expiry occupancy settlement calls slash_bond, whose incoming
            // registry clone exists before its keyed writer can quote it.
            if let Some(bond) = state.bonds.get(reserver) {
                q.copy(bond)?;
            }
            let pool = usize::from(!row.reserve);
            match row.stage {
                PalwCourtStageV1::Ticket { .. } => add(&mut tickets[pool], 1)?,
                PalwCourtStageV1::Queued { .. } if row.guaranteed => add(&mut guaranteed[pool], 1)?,
                PalwCourtStageV1::Queued { .. } => add(&mut best[pool], 1)?,
                PalwCourtStageV1::Running { .. } => {}
            }
        }
    }
    grown::<(Hash64, PalwBondKeyV2, PalwReservedCourtV1)>(&mut q, rows)?;
    q.member::<Hash64, ()>(rows)?; // reserve_sessions constructed by court_pools
    for pool in 0..2 {
        // Every actual session can belong to either pool; count each once per
        // possible pool rather than clone the reserve classification to quote.
        let mut releases = tickets[pool];
        add(&mut releases, state.court_sessions.len() as u64)?;
        grown::<u64>(&mut q, releases)?;
        grown::<PalwCourtQueuedV1>(&mut q, guaranteed[pool])?;
        grown::<PalwCourtQueuedV1>(&mut q, best[pool])?;
    }
    // The reserve queue merges its two buffers, and the final grant collector
    // can take every queued row. Each original source buffer remains covered.
    let mut merged = guaranteed[0];
    add(&mut merged, best[0])?;
    grown::<PalwCourtQueuedV1>(&mut q, merged)?;
    grown::<PalwCourtQueuedV1>(&mut q, rows)?;
    Ok(q)
}
