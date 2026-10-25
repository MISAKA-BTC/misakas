//! The actual pre-object queue drain. Its key buffers precede the individually
//! admitted row writers; neither writer admission covers these earlier copies.
use super::*;

pub(in crate::palw_state_v2) fn reserve_consumed_queue_workspace_v2(
    builder: &mut TransitionBuilder<'_>,
) -> Result<(), PalwStateV2Error> {
    if !builder.native_writers_tracking_v1() {
        return Ok(());
    }
    let payouts = builder.state.pending_payouts.len().min(PALW_V2_MAX_PAYOUTS_PER_BLOCK);
    let settlements = builder.state.evm_settlements.len();
    let mut quote = native_writers::NativeWriteQuoteV1::new("pre-object consumed queue keys");
    quote.exact::<Hash64>(payouts as u64)?.exact::<u32>(settlements as u64)?;
    // The buffers are sequential, but this conservative phase sum stays in the
    // construction owner through ACK, journal consumption and output splitting.
    // Each row writer still admits its own mutations and journal slots.
    builder.admit_native_write_v1(quote)
}

fn keys<T>(count: usize) -> Result<Vec<T>, PalwStateV2Error> {
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|why| {
        PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<Vec<T>>(
            "pre-object consumed queue keys",
            &why.to_string(),
        ))
    })?;
    Ok(out)
}

pub(in crate::palw_state_v2) fn drain_consumed_queues_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    reserve_consumed_queue_workspace_v2(builder)?;
    let count = builder.state.pending_payouts.len().min(PALW_V2_MAX_PAYOUTS_PER_BLOCK);
    let mut payouts = keys::<Hash64>(count)?;
    payouts.extend(builder.state.pending_payouts.keys().copied().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK));
    for claim_id in payouts {
        builder.write_payout(claim_id, None)?;
    }
    // Dedicated checker fee/refund ACK reads only this exact consumed prefix;
    // principal refunds do not depend on an R engine being active.
    palw_source_verifier_economics_fold_v2::acknowledge_consumed_prefix(builder, ctx)?;
    // ADR-0089 D6: the parent's list was consumed by THIS block's EVM step,
    // before this block can enqueue its own settlements. Preserve key order.
    let mut settlements = keys::<u32>(builder.state.evm_settlements.len())?;
    settlements.extend(builder.state.evm_settlements.keys().copied());
    for seq in settlements {
        builder.write_evm_settlement(seq, None)?;
    }
    Ok(())
}
