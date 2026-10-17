//! **RFC08 08-B / ADR-0184 §1 in the fold: hold a native claim's fee from Final to its liability horizon, release it, or refund it on
//! a post-Final conviction** (dormant `palw_native_liability_horizon_v1`; kernel route aux 144). A child module of `palw_state_v2`
//! like the trap fold: it reads the builder and writes only through the state's journaled writers; the rows are
//! [`crate::palw_native_liability_hold_v1`]'s. Below the fence nothing here is reached (the families' Final arms take their old path
//! byte for byte), and a claim committed below the fence never takes the new rule.
use super::*;
use crate::palw_native_liability_hold_v1::{
    PALW_NATIVE_HELD_FEE_PREFIX_V1, PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1, PALW_NATIVE_LIABILITY_HOLD_TABLE_V1,
    PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1, PalwNativeHeldFeeRowV1, palw_native_held_fee_key_v1,
};

fn refused(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::BondBudgetRefused(why.to_string())
}

/// **Does the hold apply to a claim committed at `committed_daa`?** Only past the fence, and only for claims committed at or past it.
pub(super) fn native_hold_applies_v1(params: &PalwStateParamsV2, committed_daa: u64) -> bool {
    params.native_liability_horizon_active_at(committed_daa)
}

/// **The claim's liability horizon**: its family's signed deadline (which bounds the Final floor) + `window_court` — the formula
/// RFC08 08-A admits the claim under (`native_claim_hard_deadline_v1`), so the hold ends exactly where the admission's bound does.
pub(super) fn native_horizon_v1(params: &PalwStateParamsV2, signed_deadline: u64) -> u64 {
    super::palw_settlement_liability_fold_v1::native_claim_hard_deadline_v1(params, signed_deadline)
}

/// The held fee of `(family, job)`, if any.
pub(super) fn held_native_fee_v1(state: &PalwChainStateV2, family: u8, job: &[u8; 64]) -> Option<PalwNativeHeldFeeRowV1> {
    state
        .kernel_route
        .as_ref()?
        .aux_row::<PalwNativeHeldFeeRowV1>(PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, &palw_native_held_fee_key_v1(family, job))
        .filter(|row| row.version == PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1 && row.family == family && row.job == *job)
}

/// **Is `(family, job)` still accusable after its Final at `now`?** A held fee whose horizon has not passed.
pub(super) fn native_accusable_after_final_v1(state: &PalwChainStateV2, family: u8, job: &[u8; 64], now: u64) -> Option<PalwNativeHeldFeeRowV1> {
    held_native_fee_v1(state, family, job).filter(|row| now <= row.horizon_daa)
}

/// **Hold a Final's fee** (instead of queueing it): one row per `(family, job)`; a second hold of the same job is refused.
pub(super) fn hold_native_fee_v1(builder: &mut TransitionBuilder<'_>, row: &PalwNativeHeldFeeRowV1) -> Result<(), PalwStateV2Error> {
    if held_native_fee_v1(&builder.state, row.family, &row.job).is_some() {
        return Err(refused("the native claim's fee is already held"));
    }
    if row.horizon_daa < row.final_daa {
        return Err(refused("a native claim's liability horizon precedes its Final"));
    }
    builder.write_kernel_row_encoded_v1(PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, &palw_native_held_fee_key_v1(row.family, &row.job), Some(row))
}

fn delete_v1(builder: &mut TransitionBuilder<'_>, row: &PalwNativeHeldFeeRowV1) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row_encoded_v1::<PalwNativeHeldFeeRowV1>(
        PALW_NATIVE_LIABILITY_HOLD_TABLE_V1,
        &palw_native_held_fee_key_v1(row.family, &row.job),
        None,
    )
}

/// **Release one held fee** (the row first, so it is released once whatever follows): exactly the writes the Final would have
/// made — the native income paid (finished), the payout row queued, the producer's budget reservation closed.
pub(super) fn release_native_fee_v1(builder: &mut TransitionBuilder<'_>, row: &PalwNativeHeldFeeRowV1, now: u64) -> Result<(), PalwStateV2Error> {
    delete_v1(builder, row)?;
    let queue = Hash64::from_bytes(row.queue);
    if builder.state.pending_payouts.contains_key(&queue) {
        return Err(refused("the held native fee's payout queue already exists"));
    }
    builder.pay_native_recipient_v1(Hash64::from_bytes(row.income_root), row.producer, row.income_kind, now, row.amount, queue, true)?;
    builder.write_payout(queue, Some(PalwPayoutV2 { payload: Hash64::from_bytes(row.payload), amount: row.amount }))?;
    builder.bond_budget_close_v1(&Hash64::from_bytes(row.budget), now)?;
    Ok(())
}

/// The queue a refunded held fee is paid to the requester through (distinct from the producer's queue).
pub(super) fn native_refund_queue_v1(row: &PalwNativeHeldFeeRowV1) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
        b"misaka-palw/native/held-fee-refund/v1",
        &(row.family, row.job, row.claim, row.refund_payload),
    ))
}

/// **Refund a held fee on a post-Final conviction or default** (ADR-0184 §1): the row is deleted, the unpaid fee is queued to the
/// requester, the producer's native income is cancelled and its reservation closed. The slash itself is the family's (the same
/// collectible reserve a pre-Final conviction takes). Returns the refunded row.
pub(super) fn refund_native_fee_v1(
    builder: &mut TransitionBuilder<'_>,
    family: u8,
    job: &[u8; 64],
    now: u64,
) -> Result<PalwNativeHeldFeeRowV1, PalwStateV2Error> {
    let row = native_accusable_after_final_v1(&builder.state, family, job, now)
        .ok_or_else(|| refused("no held native fee inside its liability horizon"))?;
    delete_v1(builder, &row)?;
    let queue = native_refund_queue_v1(&row);
    if builder.state.pending_payouts.contains_key(&queue) {
        return Err(refused("the held native fee's refund queue already exists"));
    }
    builder.cancel_native_recipient_v1(Hash64::from_bytes(row.income_root), row.producer, row.income_kind)?;
    builder.write_payout(queue, Some(PalwPayoutV2 { payload: Hash64::from_bytes(row.refund_payload), amount: row.amount }))?;
    builder.bond_budget_close_v1(&Hash64::from_bytes(row.budget), now)?;
    Ok(row)
}

/// **The closing tick's sweep** (a no-op below the fence): held fees whose horizon has passed and whose job has no open dispute
/// (`open(builder, row)`: the family's own pending demands or proof owners) are released, at most
/// [`PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1`] a block, in key order. A held fee with an open dispute waits for its verdict.
pub(super) fn release_held_native_fees_v1(
    builder: &mut TransitionBuilder<'_>,
    now: u64,
    open: impl Fn(&TransitionBuilder<'_>, &PalwNativeHeldFeeRowV1) -> bool,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.native_liability_horizon_active_at(now) {
        return Ok(());
    }
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let table = PALW_NATIVE_LIABILITY_HOLD_TABLE_V1;
    let range = (table, vec![PALW_NATIVE_HELD_FEE_PREFIX_V1])..(table, vec![PALW_NATIVE_HELD_FEE_PREFIX_V1 + 1]);
    let mut due: Vec<PalwNativeHeldFeeRowV1> = Vec::new();
    for ((_, key), bytes) in route.aux.range(range) {
        if due.len() >= PALW_NATIVE_LIABILITY_RELEASES_PER_BLOCK_V1 {
            break;
        }
        let Ok(row) = borsh::from_slice::<PalwNativeHeldFeeRowV1>(bytes) else { continue };
        if row.version != PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1
            || *key != palw_native_held_fee_key_v1(row.family, &row.job)
            || row.horizon_daa >= now
        {
            continue;
        }
        due.push(row);
    }
    for row in due {
        if open(builder, &row) {
            continue;
        }
        release_native_fee_v1(builder, &row, now)?;
    }
    Ok(())
}
