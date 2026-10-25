//! Caller-side construction admission for the real vesting → payout queue path.
//! The construction owner holds this conservative typed envelope until journal
//! and state output tickets are split. No payment/maturity rule is duplicated.
use super::*;
use crate::palw_vesting_v1::{PalwVestingLegV1, PalwVestingMoveV1};

fn grown<T>(q: &mut native_writers::NativeWriteQuoteV1, n: u64) -> Result<(), PalwStateV2Error> {
    if n != 0 {
        // Push/extend may hold old and new buffers together. Four actual slots
        // per final element (minimum eight) bounds that pinned Vec strategy.
        let slots = n.checked_mul(4).ok_or_else(|| {
            native_writers::native_err_v1(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Vec<T>>(
                "vesting workspace growth",
            ))
        })?;
        q.exact::<T>(slots.max(8))?;
    }
    Ok(())
}

pub(super) fn quote(state: &PalwChainStateV2) -> Result<native_writers::NativeWriteQuoteV1, PalwStateV2Error> {
    let mut q = native_writers::NativeWriteQuoteV1::new("vesting maturity caller workspace");
    let daa = state.vesting_unlatched.len() as u64;
    let counted = state.vesting_unlatched_by_settled.len() as u64;
    let candidates = daa.checked_add(counted).ok_or_else(|| {
        native_writers::native_err_v1(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<Vec<(u64, Hash64)>>(
            "vesting candidates",
        ))
    })?;
    grown::<(u64, Hash64)>(&mut q, candidates)?;
    grown::<(u64, Hash64)>(&mut q, daa)?;
    grown::<(u64, Hash64)>(&mut q, counted)?;
    let moves = (state.vesting.len() as u64).checked_add(state.reporter_rewards.len() as u64).ok_or_else(|| {
        native_writers::native_err_v1(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<PalwVestingMoveV1>("vesting moves"))
    })?;
    grown::<PalwVestingMoveV1>(&mut q, moves)?;
    // Every row is counted, including the first non-fitting move and unlatched
    // rows. This avoids allocating/reimplementing the planner to discover Q.
    for row in state.vesting.values() {
        let legs = row.seats.len().checked_add(2).ok_or_else(|| {
            native_writers::native_err_v1(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<PalwVestingLegV1>(
                "vesting row legs",
            ))
        })? as u64;
        grown::<PalwVestingLegV1>(&mut q, legs)?;
        q.exact::<PalwVestingLegV1>(legs)?; // Moved note's copy, before its journal call.
        q.member::<Hash64, ()>(legs)?.member::<Hash64, ()>(legs)?; // next keys + created keys
        q.copy(row)?; // latch/payment/removal caller clone (sequential, not per writer).
        q.member::<PalwBondKeyV2, PalwPayoutV2>(row.seats.len() as u64)?;
    }
    for _ in state.reporter_rewards.values() {
        // Reporter moves use vec![leg], and its note separately clones that leg.
        q.exact::<PalwVestingLegV1>(2)?.member::<Hash64, ()>(2)?;
    }
    Ok(q)
}

impl TransitionBuilder<'_> {
    pub(super) fn reserve_vesting_workspace_v2(&mut self) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        self.admit_native_write_v1(quote(&self.state)?)
    }
}

impl TransitionBuilder<'_> {
    pub(super) fn reserve_activation_pool_flush_workspace_v2(&mut self) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let mut q = native_writers::NativeWriteQuoteV1::new("activation pool flush caller workspace");
        let n = self.state.activation_pool_scheduled.len() as u64;
        // Many classes may share one payout key. The drain width therefore
        // does NOT bound the number of scheduled rows collected or consumed.
        grown::<((Hash64, Hash64), u64)>(&mut q, n)?;
        q.member::<Hash64, ()>(n)?.member::<Hash64, u64>(n)?;
        // The scheduled map's native ordering groups a class's payees. Quote
        // one actual caller pool clone per class, with no allocated dedup set.
        let mut previous = None;
        for (class, _) in self.state.activation_pool_scheduled.keys() {
            if previous != Some(*class) {
                if let Some(pool) = self.state.activation_pools.get(class) {
                    q.copy(pool)?;
                }
                previous = Some(*class);
            }
        }
        self.admit_native_write_v1(q)
    }
}
