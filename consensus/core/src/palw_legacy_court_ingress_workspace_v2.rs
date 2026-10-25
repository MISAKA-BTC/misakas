//! Only the Legacy branch of the real ingress: raw concurrency/claim selection
//! and its fee writer remain authoritative. No active-capacity double admission.
use super::super::native_writers::NativeWriteQuoteV1;
use super::*;

fn growing_peak<T>(q: &mut NativeWriteQuoteV1, count: u64) -> Result<(), PalwStateV2Error> {
    // Separate old/replacement Vec buffers, using the existing typed envelope.
    q.grown::<T>(count)?.grown::<T>(count)?;
    Ok(())
}

pub(super) fn quote(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    opener: PalwBondKeyV2,
    direct_evidence: bool,
    now: u64,
) -> Result<NativeWriteQuoteV1, PalwStateV2Error> {
    let mut q = NativeWriteQuoteV1::new("Legacy court ingress claim selection and fee input");
    let sessions = state.court_sessions.len() as u64;
    let cap = palw_max_concurrent_court_sessions_v1(params.turn_deadline_daa());
    // Both old cap functions refuse here before allocating or charging.
    if sessions >= cap {
        return Ok(q);
    }
    if let Some(schedule) = court_ingress_schedule_at_v1(params, now) {
        let inflight = court_ingress_inflight_borrowed_v2(state, params, now).count() as u64;
        let reserved = inflight.min(crate::palw_court_paid_ingress_v1::palw_court_ingress_reserved_claims_cap_v1(cap));
        growing_peak::<(u64, Hash64)>(&mut q, inflight)?;
        // Mapping may reuse the tuple buffer, but separate output is admitted
        // as well; no allocator-specific in-place collection is assumed.
        growing_peak::<Hash64>(&mut q, reserved)?;
        if schedule.fee_sompi_per_session > 0
            && let Some(bond) = state.bonds.get(&opener)
        {
            q.copy(bond)?; // slash_bond input exists before its keyed writer.
        }
    }
    if !direct_evidence && params.court_outsider_lane_active_v1(now) {
        // At most one first-side membership per actual running session.
        // Producer/seat and linked-operator classification now borrow inputs.
        q.member::<(Hash64, bool), ()>(sessions)?;
    }
    Ok(q)
}
