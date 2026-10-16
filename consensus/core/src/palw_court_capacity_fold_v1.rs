//! **Court capacity in the fold** (audit G-07; `crate::palw_court_capacity_v1` for the design) — the per-block court budget, court
//! admission through the challenger's LG14-A reservation, the deadline-aware queue and its tickets, and the occupancy settlement. A
//! child module of `palw_state_v2`; every row it writes lives in the LG14-A dispute record (`PalwDisputeClaimV1::courts`) and goes
//! through that table's one journaled writer (delta 200), so restart, reorg, IBD and pruned import carry it unchanged.
//!
//! Below `palw_court_capacity_v1` — every block of every shipped network — every function here returns at once: `Legacy` for the
//! admission (the caller keeps int-12's concurrency cap), no charge, no sweep, no settlement.

use super::*;
use crate::palw_court_capacity_v1::*;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CourtCapacityRefused(why.into())
}

/// What court admission decided for a session about to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwCourtAdmissionV1 {
    /// Below the fence: the caller's int-12 concurrency cap decides.
    Legacy,
    /// Open the session now (free capacity and nobody queued, or the challenger's own ticket).
    Open,
    /// The prosecution was queued under its worst-case guarantee; no session opens in this object.
    Queued,
}

/// **The per-block court budget**: each court operation is charged its deterministic cost before its arm runs (the heavy-prompt
/// pattern: a refused operation does not refund the budget it asked for); an operation past the block's budget is refused. The
/// acceptance walk holds the same budget over the objects it admits.
pub(super) fn charge_court_op_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
) -> Result<(), PalwStateV2Error> {
    let Some(policy) = builder.params.court_capacity_at_v1(ctx.daa_score) else { return Ok(()) };
    let Some(cost) = palw_court_op_cost_v1(&policy, &builder.state, object) else { return Ok(()) };
    let spent = builder.court_cost_spent.saturating_add(cost);
    if !spent.fits_within(&policy.block_budget) {
        return Err(refused(format!(
            "the block's court budget {:?} cannot take this operation ({cost:?}; {:?} already spent)",
            policy.block_budget, builder.court_cost_spent
        )));
    }
    builder.court_cost_spent = spent;
    Ok(())
}

/// **Every slot occupant's worst-case release, the tickets outstanding and the queue**, from state alone. Every open court session
/// occupies a slot until its backstop (`deadline_daa`), whoever opened it; a ticket occupies one from its grant for its ticket window
/// plus the session it may open.
fn court_occupancy_v1(state: &PalwChainStateV2, ticket_window: u64) -> (Vec<u64>, u64, Vec<PalwCourtQueuedV1>) {
    let mut releases: Vec<u64> = state.court_sessions.values().map(|session| session.deadline_daa).collect();
    let mut tickets = 0u64;
    let mut queue = Vec::new();
    for (claim, record) in &state.legacy_disputes {
        for (reserver, row) in &record.courts {
            match row.stage {
                PalwCourtStageV1::Ticket { granted_daa, .. } => {
                    tickets += 1;
                    releases.push(granted_daa.saturating_add(ticket_window).saturating_add(row.service_daa));
                }
                PalwCourtStageV1::Queued { enqueued_daa } => queue.push(PalwCourtQueuedV1 {
                    key: PalwCourtQueueKeyV1 {
                        deadline_daa: record.hard_deadline_daa,
                        enqueued_daa,
                        claim: *claim,
                        reserver: *reserver,
                    },
                    service_daa: row.service_daa,
                }),
                PalwCourtStageV1::Running { .. } => {}
            }
        }
    }
    (releases, tickets, queue)
}

/// **Court admission** where a court session is about to open (`open_dissection_at_named_leaf_v1`, `CourtOpened`). Past the fence:
/// the challenger's own live LG14-A reservation on the claim (its claim-fixed hard deadline, its Final hold), an Active bond at or
/// above the floor, and the occupancy escrow (ρ × (ticket window + `session_window`) × weight) on its free half. Then: its own ticket
/// opens (its guarantee was proven when it queued); free capacity with nobody queued opens — if a session opened now and held to
/// `session_window` still ends by the claim's hard deadline; otherwise the prosecution QUEUES — only if the worst-case service
/// accounting shows that its session, and that of every prosecution already queued, still ends by its claim's hard deadline.
pub(super) fn court_admission_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    challenger: PalwBondKeyV2,
    session_window: u64,
) -> Result<PalwCourtAdmissionV1, PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(policy) = builder.params.court_capacity_at_v1(now) else { return Ok(PalwCourtAdmissionV1::Legacy) };
    let mut record = builder
        .state
        .legacy_disputes
        .get(&claim_id)
        .filter(|record| record.live.contains_key(&challenger))
        .cloned()
        .ok_or_else(|| {
            refused(
                "court admission past palw_court_capacity_v1 needs the challenger's own live dispute reservation on the claim (its \
                 claim-fixed hard deadline and Final hold)",
            )
        })?;
    let bond = builder.state.bonds.get(&challenger).ok_or(PalwStateV2Error::MissingBond(challenger))?;
    if !matches!(bond.status, PalwBondStatusV2::Active) {
        return Err(PalwStateV2Error::BondNotActive(challenger));
    }
    let floor = builder.params.min_collateral_sompi();
    if bond.collateral < floor {
        return Err(PalwStateV2Error::BondBelowFloor { bond: challenger, collateral: bond.collateral, floor });
    }
    let turn = builder.params.turn_deadline_daa();
    let ticket_window = policy.ticket_window(turn);
    let capacity = policy.capacity_sessions(turn);
    // An existing row: its own ticket opens; a queued or running prosecution files nothing more on this claim.
    if let Some(row) = record.courts.get(&challenger) {
        return match row.stage {
            PalwCourtStageV1::Ticket { expires_daa, .. } if now <= expires_daa => Ok(PalwCourtAdmissionV1::Open),
            PalwCourtStageV1::Ticket { .. } => Err(refused("the prosecution's court ticket has expired")),
            PalwCourtStageV1::Queued { .. } => Err(refused("the prosecution is queued: it opens with its ticket")),
            PalwCourtStageV1::Running { .. } => Err(refused("the reserver already runs a court session on this claim")),
        };
    }
    if now.saturating_add(session_window) > record.hard_deadline_daa {
        return Err(refused(
            "a court session opened now could outlast the claim's hard deadline: its verdict must land inside it (the court term of \
             LG14-A's hard-deadline envelope)",
        ));
    }
    let weight = policy.weight();
    let escrow = policy.fee(ticket_window.saturating_add(session_window), weight);
    if escrow > 0 {
        builder.check_accuser_room(challenger, escrow, "court occupancy escrow", now)?;
    }
    let (mut releases, tickets, mut queue) = court_occupancy_v1(&builder.state, ticket_window);
    let occupied = builder.state.court_sessions.len() as u64 + tickets;
    let row = |stage| PalwReservedCourtV1 { stage, escrow, rho_sompi: policy.rho_sompi, weight, service_daa: session_window };
    if occupied < capacity && queue.is_empty() {
        // The session opens in this object; `court_opened_v1` names it.
        record.courts.insert(challenger, row(PalwCourtStageV1::Running { session: ZERO_HASH64, started_daa: now }));
        builder.write_legacy_dispute(claim_id, Some(record))?;
        return Ok(PalwCourtAdmissionV1::Open);
    }
    queue.push(PalwCourtQueuedV1 {
        key: PalwCourtQueueKeyV1 { deadline_daa: record.hard_deadline_daa, enqueued_daa: now, claim: claim_id, reserver: challenger },
        service_daa: session_window,
    });
    releases.sort_unstable();
    if let Err(position) = palw_court_schedule_v1(now, capacity, &releases, &queue, ticket_window) {
        return Err(refused(format!(
            "no deadline guarantee: with {occupied} of {capacity} court slots held and {} queued, the prosecution at position \
             {position} could not open before its claim's hard deadline",
            queue.len() - 1
        )));
    }
    record.courts.insert(challenger, row(PalwCourtStageV1::Queued { enqueued_daa: now }));
    builder.write_legacy_dispute(claim_id, Some(record))?;
    Ok(PalwCourtAdmissionV1::Queued)
}

/// **The session admitted for `challenger` opened as `session`**: its row runs, metered from its grant (a ticket) or its opening.
pub(super) fn court_opened_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    challenger: PalwBondKeyV2,
    session: Hash64,
) -> Result<(), PalwStateV2Error> {
    if builder.params.court_capacity_at_v1(ctx.daa_score).is_none() {
        return Ok(());
    }
    let Some(mut record) = builder.state.legacy_disputes.get(&claim_id).cloned() else { return Ok(()) };
    let Some(row) = record.courts.get_mut(&challenger) else { return Ok(()) };
    let started_daa = match row.stage {
        PalwCourtStageV1::Ticket { granted_daa, .. } => granted_daa,
        PalwCourtStageV1::Running { started_daa, .. } => started_daa,
        PalwCourtStageV1::Queued { .. } => return Err(refused("a queued prosecution opened without its ticket")),
    };
    row.stage = PalwCourtStageV1::Running { session, started_daa };
    builder.write_legacy_dispute(claim_id, Some(record))
}

/// **One court row settles at `now`**: the occupancy fee (ρ × held DAA × weight, never more than the escrow) is burned; on an
/// objective outcome against the producer (`convicted`: a conviction or a DA default) it is taken from the producer's collateral first
/// and only what the producer cannot cover falls on the reserver. The escrow leaves the A-6 ledger with the row (the caller drops it).
/// `held_until`: the DAA the occupancy is charged to (a session still open when its reservation lapses is charged to its backstop).
pub(super) fn settle_court_row_v1(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    reserver: PalwBondKeyV2,
    row: &PalwReservedCourtV1,
    held_until: u64,
    convicted: bool,
) -> Result<(), PalwStateV2Error> {
    let fee = row.fee_at(held_until);
    if fee == 0 {
        return Ok(());
    }
    let mut owed = fee;
    if convicted
        && let Some(producer) = builder.state.claims.get(claim_id).map(|claim| claim.bond)
        && builder.state.bonds.contains_key(&producer)
    {
        let paid = u128::from(builder.slash_bond(producer, fee)?);
        owed = owed.saturating_sub(paid);
    }
    if owed > 0 && builder.state.bonds.contains_key(&reserver) {
        builder.slash_bond(reserver, owed)?;
    }
    Ok(())
}

/// The DAA a row's occupancy is charged to when its reservation ends at `now` without the claim's court outcome: an open session to
/// its backstop (the slot stays held for it), anything else to `now`.
pub(super) fn court_row_held_until_v1(state: &PalwChainStateV2, row: &PalwReservedCourtV1, now: u64) -> u64 {
    match row.stage {
        PalwCourtStageV1::Running { session, .. } => state.court_sessions.get(&session).map_or(now, |s| s.deadline_daa.max(now)),
        _ => now,
    }
}

/// **The court-capacity sweep** (every block, right after the dispute-reservation sweep, in the fold and in the pre-object base, so
/// it runs before any object of the block): a running row whose session has ended while its reservation lives is settled to `now`
/// (no conviction voided the claim — a conviction or default would have released the reservation and settled there); an expired
/// ticket is settled for the DAA it held and dropped; then the capacity left free is granted, as tickets, to the queue in key order.
pub(super) fn sweep_court_capacity_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(policy) = builder.params.court_capacity_at_v1(now) else { return Ok(()) };
    let turn = builder.params.turn_deadline_daa();
    let ticket_window = policy.ticket_window(turn);
    let capacity = policy.capacity_sessions(turn);
    // 1–2. Ended sessions and expired tickets.
    let mut ended: Vec<(Hash64, PalwBondKeyV2, PalwReservedCourtV1)> = Vec::new();
    for (claim, record) in &builder.state.legacy_disputes {
        for (reserver, row) in &record.courts {
            let done = match row.stage {
                PalwCourtStageV1::Running { session, .. } => !builder.state.court_sessions.contains_key(&session),
                PalwCourtStageV1::Ticket { expires_daa, .. } => now > expires_daa,
                PalwCourtStageV1::Queued { .. } => false,
            };
            if done {
                ended.push((*claim, *reserver, *row));
            }
        }
    }
    for (claim, reserver, row) in ended {
        let held_until = match row.stage {
            PalwCourtStageV1::Ticket { expires_daa, .. } => expires_daa,
            _ => now,
        };
        settle_court_row_v1(builder, &claim, reserver, &row, held_until, false)?;
        if let Some(mut record) = builder.state.legacy_disputes.get(&claim).cloned() {
            record.courts.remove(&reserver);
            builder.write_legacy_dispute(claim, Some(record))?;
        }
    }
    // 3. Grants, earliest claim deadline first.
    let (_, tickets, mut queue) = court_occupancy_v1(&builder.state, ticket_window);
    let occupied = builder.state.court_sessions.len() as u64 + tickets;
    let mut free = capacity.saturating_sub(occupied);
    queue.sort_by_key(|entry| entry.key);
    for entry in queue {
        if free == 0 {
            break;
        }
        let Some(mut record) = builder.state.legacy_disputes.get(&entry.key.claim).cloned() else { continue };
        let Some(row) = record.courts.get_mut(&entry.key.reserver) else { continue };
        row.stage = PalwCourtStageV1::Ticket { granted_daa: now, expires_daa: now.saturating_add(ticket_window) };
        builder.write_legacy_dispute(entry.key.claim, Some(record))?;
        free -= 1;
    }
    Ok(())
}
