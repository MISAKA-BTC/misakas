//! **G14 lane closure in the fold** ([`crate::palw_g14_lane_closure_v1`]): the kernel route's signer liability locks (G-K1) and the EXEC
//! REAL root's V2 signer charge (G-X1). A child module of `palw_state_v2`, as the kernel route and EXEC folds are, so it writes only
//! through their journaled writers (`write_kernel_row`, `slash_bond`, the covering-signer funnel). Every function is reached only from
//! a lane fold that runs past that lane's own (dormant) fence; aux 143 rows exist only there.
use super::*;
use crate::palw_g14_lane_closure_v1::*;
use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::LedgerEventV1;

/// Every signer lock row of the route, `(key, row)`, in key order (seat first).
fn signer_locks_v1(route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1) -> Vec<(Vec<u8>, PalwKernelSignerLockRowV1)> {
    route
        .aux
        .range((PALW_G14_LANE_CLOSURE_TABLE_V1, vec![PALW_KERNEL_SIGNER_LOCK_PREFIX_V1])..)
        .take_while(|((table, key), _)| {
            *table == PALW_G14_LANE_CLOSURE_TABLE_V1 && key.first() == Some(&PALW_KERNEL_SIGNER_LOCK_PREFIX_V1)
        })
        .filter_map(|((_, key), bytes)| borsh::from_slice::<PalwKernelSignerLockRowV1>(bytes).ok().map(|row| (key.clone(), row)))
        .collect()
}

/// **G-K1: lock a counting seat's signer liability** before its passing receipt is counted. `Ok(false)`: the seat is not Active or
/// its FREE collateral (collateral less V2's committed ledger, which already holds its other locks) cannot back the lock — the receipt
/// is then not counted (a `Valid` no collateral stands behind is worth nothing, the 2026-09-23 audit's rule for the Panel route). A
/// seat already locked on the claim keeps its one lock.
pub(super) fn lock_kernel_signer_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    seat: &PalwBondKeyV2,
    claim: &Digest,
    claim_reward: u64,
    quorum: u8,
) -> Result<bool, PalwStateV2Error> {
    let key = palw_kernel_signer_lock_key_v1(&palw_kernel_bond_id_v1(seat), claim);
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(false) };
    if route.aux.contains_key(&(PALW_G14_LANE_CLOSURE_TABLE_V1, key.clone())) {
        return Ok(true);
    }
    let amount = palw_kernel_signer_lock_amount_v1(claim_reward, quorum);
    let Some(record) = builder.state.bonds.get(seat) else { return Ok(false) };
    if !matches!(record.status, PalwBondStatusV2::Active) {
        return Ok(false);
    }
    let free = u128::from(record.collateral).saturating_sub(builder.committed_at(seat, ctx.daa_score));
    if free < u128::from(amount) {
        return Ok(false);
    }
    let row = PalwKernelSignerLockRowV1 {
        version: PALW_G14_LANE_CLOSURE_ROW_VERSION_V1,
        claim: *claim,
        seat: *seat,
        amount,
        locked_daa: ctx.daa_score,
    };
    builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, Some(borsh::to_vec(&row).expect("a lock row serializes")))?;
    Ok(true)
}

/// Charge every lock of `claim`: the row goes first (it leaves V2's committed ledger), then the seat's real bond is slashed by the
/// lock (clamped to what the bond still holds — another lane may have drained it; the conviction itself never depends on it). The
/// slash is burned (no payout: a signer's liability is a court charge on its own collateral, as RFC-0007 T6's). Once: the row is the
/// marker.
fn charge_signer_locks_of_claim_v1(builder: &mut TransitionBuilder<'_>, claim: &Digest) -> Result<(), PalwStateV2Error> {
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let locks: Vec<(Vec<u8>, PalwKernelSignerLockRowV1)> =
        signer_locks_v1(route).into_iter().filter(|(_, row)| row.claim == *claim).collect();
    for (key, row) in locks {
        builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
        if builder.state.bonds.contains_key(&row.seat) {
            builder.slash_bond(row.seat, u128::from(row.amount))?;
        }
    }
    Ok(())
}

/// **G-K1 at a conviction** (after every kernel settlement batch — an object's arm or the closing tick): each `Convicted` claim's
/// signer locks are charged, pre- or post-Final alike. A no-op on a batch with no conviction or a claim no seat covered.
pub(super) fn charge_kernel_signers_on_conviction_v1(
    builder: &mut TransitionBuilder<'_>,
    events: &[LedgerEventV1],
) -> Result<(), PalwStateV2Error> {
    for event in events {
        if let LedgerEventV1::Convicted { claim, .. } = event {
            charge_signer_locks_of_claim_v1(builder, claim)?;
        }
    }
    Ok(())
}

/// **G-K4: a response to a segmented claim's demand stood** — the first block that carried one is recorded (once), so pruning never
/// passes the block bodies its served parts live in while the claim is convictable.
pub(super) fn note_served_material_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    claim: &Digest,
) -> Result<(), PalwStateV2Error> {
    if !ledger.claims.get(claim).is_some_and(|row| matches!(row.body, misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { .. })) {
        return Ok(());
    }
    let key = palw_served_material_key_v1(claim);
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    if route.aux.contains_key(&(PALW_G14_LANE_CLOSURE_TABLE_V1, key.clone())) {
        return Ok(());
    }
    let row = PalwServedMaterialRowV1 {
        version: PALW_G14_LANE_CLOSURE_ROW_VERSION_V1,
        claim: *claim,
        first_blue_score: ctx.blue_score,
        first_daa: ctx.daa_score,
    };
    builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, Some(borsh::to_vec(&row).expect("a material row serializes")))
}

/// **G-K1 / G-K4 at the closing tick**: each lock whose claim can no longer be convicted is released (a timeout, a missing row, or past
/// `liability_until` unconvicted); a lock whose claim stands convicted is charged (the safety net of the conviction batch). A served
/// material row is released when its claim is decided (convicted) or can no longer be convicted. Read from the ledger's own claim
/// rows, so the sweep needs no ledger load and runs whatever else the route is doing.
pub(super) fn sweep_kernel_signer_locks_v1(builder: &mut TransitionBuilder<'_>, now_daa: u64) -> Result<(), PalwStateV2Error> {
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let claim_row = |claim: &Digest| {
        route
            .rows
            .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, claim.to_vec()))
            .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes).ok())
    };
    let materials: Vec<Vec<u8>> = route
        .aux
        .range((PALW_G14_LANE_CLOSURE_TABLE_V1, vec![PALW_SERVED_MATERIAL_PREFIX_V1])..)
        .take_while(|((table, key), _)| {
            *table == PALW_G14_LANE_CLOSURE_TABLE_V1 && key.first() == Some(&PALW_SERVED_MATERIAL_PREFIX_V1)
        })
        .filter_map(|((_, key), bytes)| borsh::from_slice::<PalwServedMaterialRowV1>(bytes).ok().map(|row| (key.clone(), row)))
        .filter(|(_, row)| {
            palw_kernel_signer_lock_fate_v1(claim_row(&row.claim).as_ref(), now_daa) != PalwKernelSignerLockFateV1::Hold
        })
        .map(|(key, _)| key)
        .collect();
    let locks = signer_locks_v1(route);
    for key in materials {
        builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
    }
    if locks.is_empty() {
        return Ok(());
    }
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let mut charge = Vec::new();
    let mut release = Vec::new();
    for (key, row) in locks {
        let claim_row = route
            .rows
            .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, row.claim.to_vec()))
            .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes).ok());
        match palw_kernel_signer_lock_fate_v1(claim_row.as_ref(), now_daa) {
            PalwKernelSignerLockFateV1::Hold => {}
            PalwKernelSignerLockFateV1::Release => release.push(key),
            PalwKernelSignerLockFateV1::Charge => charge.push(row.claim),
        }
    }
    for key in release {
        builder.write_kernel_row(PALW_G14_LANE_CLOSURE_TABLE_V1, key, None)?;
    }
    charge.dedup();
    for claim in charge {
        charge_signer_locks_of_claim_v1(builder, &claim)?;
    }
    Ok(())
}

/// **G-X1: the REAL root's own licensed piece is proven false** — every V2 seat that signed `Valid` on the REAL claim (it holds a
/// slashable lock on it) is charged S4 on that lock through the covering-signer funnel (lock first; once per seat and claim across
/// every route: the false-Valid offence key). The producer is not charged a second time here: its prefix claim's kernel reservation
/// is the one charge for the one lie (I8). A later executor's slice is not something the REAL claim's V2 seats signed — they licensed
/// the claim before any slice existed — so its falsity charges that executor's kernel claim and that claim's own receipt signers
/// (G-K1), never these seats.
pub(super) fn charge_exec_root_signers_on_proven_false_v1(
    builder: &mut TransitionBuilder<'_>,
    root_id: Hash64,
    now_daa: u64,
) -> Result<(), PalwStateV2Error> {
    let covering: Vec<PalwBondKeyV2> =
        builder.state.slashable_locks.keys().filter(|(_, claim)| *claim == root_id).map(|(seat, _)| *seat).collect();
    if covering.is_empty() {
        return Ok(());
    }
    let conv = builder.open_conviction_v1(&covering, now_daa);
    let g = builder.claim_g_v1(&root_id).map(|gains| gains.g()).unwrap_or(0);
    builder.charge_covering_signers_v1(&conv, root_id, &covering, g)
}
