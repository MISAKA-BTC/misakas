//! **Court capacity in the fold** (audit G-07; `crate::palw_court_capacity_v1` for the design) — the per-block court budget, court
//! admission through the challenger's LG14-A reservation, the deadline-aware queue and its tickets, and the occupancy settlement. A
//! child module of `palw_state_v2`; every row it writes lives in the LG14-A dispute record (`PalwDisputeClaimV1::courts`) and goes
//! through that table's one journaled writer (delta 200), so restart, reorg, IBD and pruned import carry it unchanged.
//!
//! Below `palw_court_capacity_v1` — every block of every shipped network — every function here returns at once: `Legacy` for the
//! admission (the caller keeps int-12's concurrency cap), no charge, no sweep, no settlement.

use super::*;
use crate::palw_court_capacity_v1::*;

#[path = "palw_court_capacity_sweep_workspace_v2.rs"]
mod sweep_workspace;
#[path = "palw_court_admission_workspace_v2.rs"]
mod admission_workspace;
#[path = "palw_legacy_court_ingress_workspace_v2.rs"]
mod legacy_ingress_workspace;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CourtCapacityRefused(why.into())
}

/// The refusal code of a court-demanding claim whose promised retention is below its court guarantee's.
pub const PALW_COURT_RETENTION_BELOW_GUARANTEE_CODE_V1: &str = "COURT_RETENTION_BELOW_GUARANTEE";

/// Court admission's refusal while the prosecution's own row is QUEUED (it opens with its ticket).
pub const PALW_COURT_REFUSAL_QUEUED_V1: &str = "the prosecution is queued: it opens with its ticket";
/// Court admission's refusal when the challenger has no live dispute reservation on the claim yet.
pub const PALW_COURT_REFUSAL_NO_RESERVATION_V1: &str =
    "court admission past palw_court_capacity_v1 needs the challenger's own live dispute reservation on the claim (its claim-fixed \
     hard deadline and Final hold)";
/// Court admission's refusal when the challenger already runs a session on the claim.
pub const PALW_COURT_REFUSAL_RUNNING_V1: &str = "the reserver already runs a court session on this claim";
/// Court admission's refusal when the prosecution's ticket lapsed (its row is dropped at the next sweep; a later filing queues anew).
pub const PALW_COURT_REFUSAL_TICKET_EXPIRED_V1: &str = "the prosecution's court ticket has expired";

/// **Lane g14-court-capacity-r2: does a court-capacity refusal ask the filer to WAIT** (its row is queued, its ticket lapsed and
/// will be dropped, its reservation is not landed yet, or this block's court budget is spent) rather than to give up? A filer that
/// retries on these never files an object the fold refuses for good, and never abandons a prosecution the court will still serve.
pub fn palw_court_refusal_waits_v1(error: &PalwStateV2Error) -> bool {
    match error {
        PalwStateV2Error::CourtCapacityRefused(why) => {
            why.starts_with(PALW_COURT_REFUSAL_QUEUED_V1)
                || why.starts_with(PALW_COURT_REFUSAL_TICKET_EXPIRED_V1)
                || why.starts_with(PALW_COURT_REFUSAL_NO_RESERVATION_V1)
                || why.starts_with("the block's court budget")
        }
        _ => false,
    }
}

/// **Lane g14-court-capacity-r2: what `reserver`'s prosecution of `claim` should do now** — read off its court row, for a node's
/// filer: `None` below the fence (int-12's cap decides); else whether to file the accusation now (no row yet: it opens or queues; or
/// a live ticket), to wait (queued), or to stand aside (its session runs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCourtProsecutionStepV1 {
    /// File (or re-file) the accusation now.
    File,
    /// Queued: file nothing until the row holds a ticket.
    Wait,
    /// The session runs: the court's moves, not a filing.
    Running,
}

pub fn palw_court_prosecution_step_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim: &Hash64,
    reserver: &PalwBondKeyV2,
    now: u64,
) -> Option<PalwCourtProsecutionStepV1> {
    params.court_capacity_at_v1(now)?;
    let row = state.legacy_disputes.get(claim).and_then(|record| record.courts.get(reserver));
    Some(match row.map(|row| row.stage) {
        None => PalwCourtProsecutionStepV1::File,
        Some(PalwCourtStageV1::Ticket { expires_daa, .. }) if now <= expires_daa => PalwCourtProsecutionStepV1::File,
        Some(PalwCourtStageV1::Ticket { .. }) | Some(PalwCourtStageV1::Queued { .. }) => PalwCourtProsecutionStepV1::Wait,
        Some(PalwCourtStageV1::Running { .. }) => PalwCourtProsecutionStepV1::Running,
    })
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
    let Some((cost, one_move)) = crate::palw_court_transport_v1::palw_court_accounted_op_v1(
        &builder.state, builder.params, ctx.daa_score, object,
    ).map_err(refused)? else { return Ok(()) };
    let policy = builder.params.court_capacity_at_v1(ctx.daa_score)
        .ok_or_else(|| refused("court operation has no active capacity budget"))?;
    let budget = if one_move { policy.one_move_budget } else { policy.block_budget };
    let already = if one_move { builder.court_one_move_spent } else { builder.court_cost_spent };
    let spent = already.checked_add(cost).ok_or_else(|| refused("the block's court budget accumulation overflows"))?;
    if !spent.fits_within(&budget) {
        return Err(refused(format!(
            "the block's court budget {budget:?} ({}) cannot take this operation ({cost:?}; {already:?} already spent)",
            if one_move { "one-move accusations" } else { "court sessions" }
        )));
    }
    if one_move {
        builder.court_one_move_spent = spent;
    } else {
        builder.court_cost_spent = spent;
    }
    Ok(())
}

// ---- round 2: court-demanding claims and the bounded admission -------------------------------------------------------------------

/// **Is `claim` court-demanding** (lane g14-court-capacity-r2): a V2 Attempt of a class with fused-attention leaves (its lies are
/// adjudicated only through a court session), accepted at or past the court-capacity fence. Its first court row is its reserve row,
/// and the bounded admission counts it.
pub(super) fn palw_court_demanding_claim_v1(state: &PalwChainStateV2, from_daa: u64, claim: &PalwClaimStateV2) -> bool {
    matches!(claim.source, PalwClaimSourceV2::Attempt)
        && claim.accepted_daa >= from_daa
        && state.classes.get(&claim.class_id).is_some_and(|class| class.fused_attention)
}

/// **The bounded admission** (round 2): a new court-demanding claim (a fused-attention V2 Attempt whose trace retention is
/// `trace_retention_daa`) is admitted at `now` only while its hard-deadline bucket (`⌊hard / W⌋`) holds fewer than `M` in-flight
/// court-demanding claims — not voided, hard deadline not passed. `Ok` below the fence and for a class with no fused leaf. A pure read
/// of state (the producer's pre-check asks the same function).
pub(super) fn check_court_claim_admission_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
    now: u64,
    trace_retention_daa: u64,
    span_daa: u64,
) -> Result<(), PalwStateV2Error> {
    let Some(mirror) = params.court_capacity_v1().filter(|m| m.active_at(now)) else { return Ok(()) };
    // **The retention a court-demanding claim promises** (user decision 2026-10-17): a fused-attention attempt must promise at least
    // the court-demanding retention, or it is not reward-bearing (option (b)'s refusal); a class that is not court-demanding may not
    // take the longer canonical value (one value per class: no second lottery draw).
    let fused = state.classes.get(class_id).is_some_and(|class| class.fused_attention);
    // RFC-0018 Gate C (user decision 2026-10-17): both canonical values carry the same certification extension (0 below its fence).
    let gate_c = crate::palw_rfc0018_v1::palw_gate_c_retention_extension_daa_v1(params, now);
    let court_retention = palw_court_demanding_retention_daa_v1(params, now).map_or(0, |r| r.saturating_add(gate_c));
    let min_retention = crate::palw_producer_v2::palw_min_trace_retention_daa_v1(params).saturating_add(gate_c);
    let promised = trace_retention_daa.saturating_sub(now);
    if fused && promised < court_retention {
        return Err(PalwStateV2Error::ClassNotRewardable {
            class: *class_id,
            code: PALW_COURT_RETENTION_BELOW_GUARANTEE_CODE_V1,
            why: format!(
                "palw_court_capacity_v1: a court-demanding (fused-attention) claim promises {promised} DAA of trace retention, below the \
                 {court_retention} its court guarantee needs (4W + W_disclose from its licence): no reward, weight or consensus credit"
            ),
        });
    }
    if !fused && court_retention > min_retention && promised == court_retention {
        return Err(PalwStateV2Error::ClassNotRewardable {
            class: *class_id,
            code: PALW_COURT_RETENTION_BELOW_GUARANTEE_CODE_V1,
            why: "palw_court_capacity_v1: only a court-demanding class promises the court retention (one canonical value per class)"
                .to_string(),
        });
    }
    if !fused {
        return Ok(());
    }
    let policy = mirror.policy;
    let turn = params.turn_deadline_daa();
    let width = policy.admission_bucket_daa(turn, params.window_court());
    // RFC-0016 V8 (user decision 2026-10-17): past the elastic-capacity fence the bound is also the attested court capacity, derived
    // per class (`claims_per_span × bucket / span`), the minimum of the two.
    let cap = match super::palw_rfc0016_fold_v1::palw_rfc0016_court_bucket_bound_v1(state, params, class_id, now, width, span_daa) {
        Some(attested) => policy.admission_claims_per_bucket(turn).min(attested),
        None => policy.admission_claims_per_bucket(turn),
    };
    let w_disclose = palw_da_disclose_window_daa_v1(params);
    // G14 U06 (`palw_court_clock_v2`): a claim accepted past the fence is provisioned for its MAXIMAL dispute, `a + 10,000` from
    // acceptance (RFC-0026 §3.1); `now` is the new claim's acceptance. Below it: the old formula.
    let bucket = if params.court_clock_v2_from_daa().is_some_and(|from| now >= from) {
        crate::palw_court_clock_v2::palw_court_clock_v2_extended_hard(now) / width
    } else {
        trace_retention_daa.saturating_sub(w_disclose) / width
    };
    // G14 lane r6-d1-release: a funded original with an open liability holds its court reserve too (`check_funded_court_reserve_v1`).
    let funded = palw_funded_court_reserve_claims_v1(state);
    let admitted = state
        .claims
        .iter()
        .filter(|(id, claim)| {
            (palw_court_demanding_claim_v1(state, mirror.from_daa, claim) || (claim.accepted_daa >= mirror.from_daa && funded.contains(*id)))
                && !matches!(claim.phase, PalwClaimPhaseV2::Voided { .. })
                && {
                    let hard = crate::palw_court_clock_v2::palw_claim_court_reserve_hard_v2(params, claim, w_disclose);
                    hard >= now && hard / width == bucket
                }
        })
        .count() as u64;
    if admitted >= cap {
        return Err(PalwStateV2Error::CourtClaimAdmissionFull { class: *class_id, bucket, admitted, cap });
    }
    Ok(())
}

/// **G14 lane r6-d1-release (r5-original-admission open 4): the claims a funded original admission reserved court capacity for** —
/// every funded escrow row with an open, unsettled liability. Empty below `palw_tir_public_conviction_v1` (no funded row exists).
pub(super) fn palw_funded_court_reserve_claims_v1(state: &PalwChainStateV2) -> std::collections::BTreeSet<Hash64> {
    funded_court_reserve_claims_borrowed_v2(state).collect()
}

fn funded_court_reserve_claims_borrowed_v2(state: &PalwChainStateV2) -> impl Iterator<Item = Hash64> + '_ {
    state.funded_escrows_v1().values().filter(|row| row.liability_open && row.settled_by.is_none()).map(|row| row.claim)
}

/// **G14 lane r6-d1-release: the court-capacity reservation of a funded original admission** (when `palw_court_capacity_v1` is in
/// force at `now`; `Ok` and nothing reserved below it). The capacity design's per-claim reserve is the bounded admission's bucket
/// slot plus the claim's ONE reserve court row (`court_reserve_used`); a queue row (`PalwReservedCourtV1`) can only exist beside a
/// live challenger reservation, which no admission has. So the funded claim (already written by the install, `claim_id`) is counted
/// in its hard-deadline bucket exactly like a court-demanding claim, refused when the bucket is full, and — once admitted — counted
/// by every later admission ([`palw_funded_court_reserve_claims_v1`]) and given the reserve pool for its first court row.
pub(super) fn check_funded_court_reserve_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: Hash64,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    let Some(mirror) = params.court_capacity_v1().filter(|m| m.active_at(now)) else { return Ok(()) };
    let policy = mirror.policy;
    let turn = params.turn_deadline_daa();
    let width = policy.admission_bucket_daa(turn, params.window_court());
    if width == 0 {
        return Err(refused("court capacity bucket width is zero"));
    }
    let w_disclose = palw_da_disclose_window_daa_v1(params);
    let claim = state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
    let bucket = crate::palw_court_clock_v2::palw_claim_court_reserve_hard_v2(params, claim, w_disclose) / width;
    let funded = palw_funded_court_reserve_claims_v1(state);
    let admitted = state
        .claims
        .iter()
        .filter(|(id, other)| {
            **id != claim_id
                && (palw_court_demanding_claim_v1(state, mirror.from_daa, other)
                    || (other.accepted_daa >= mirror.from_daa && funded.contains(*id)))
                && !matches!(other.phase, PalwClaimPhaseV2::Voided { .. })
                && {
                    let hard = crate::palw_court_clock_v2::palw_claim_court_reserve_hard_v2(params, other, w_disclose);
                    hard >= now && hard / width == bucket
                }
        })
        .count() as u64;
    let cap = policy.admission_claims_per_bucket(turn);
    if admitted >= cap {
        return Err(PalwStateV2Error::CourtClaimAdmissionFull { class: claim.class_id, bucket, admitted, cap });
    }
    Ok(())
}

/// **One pool's occupancy** (round 2): its occupants' worst-case releases, its outstanding tickets, its queue — guaranteed entries
/// and best-effort ones apart.
#[derive(Default)]
struct PalwCourtPoolV1 {
    releases: Vec<u64>,
    running: u64,
    tickets: u64,
    guaranteed: Vec<PalwCourtQueuedV1>,
    best_effort: Vec<PalwCourtQueuedV1>,
}

impl PalwCourtPoolV1 {
    fn occupied(&self) -> u64 {
        self.running + self.tickets
    }
    fn queued(&self) -> usize {
        self.guaranteed.len() + self.best_effort.len()
    }
}

/// **Both pools' occupancy, from state alone** (round 2): every open court session occupies a slot until its backstop
/// (`deadline_daa`) — a reserve row's session on the reserve pool, every other session on the spare pool, whoever opened it; a ticket
/// occupies one from its grant for its ticket window plus the session it may open, on its row's pool.
fn court_pools_v1(state: &PalwChainStateV2, ticket_window: u64) -> (PalwCourtPoolV1, PalwCourtPoolV1) {
    let mut reserve = PalwCourtPoolV1::default();
    let mut spare = PalwCourtPoolV1::default();
    let mut reserve_sessions = std::collections::BTreeSet::new();
    for (claim, record) in &state.legacy_disputes {
        for (reserver, row) in &record.courts {
            let pool = if row.reserve { &mut reserve } else { &mut spare };
            match row.stage {
                PalwCourtStageV1::Ticket { granted_daa, .. } => {
                    pool.tickets += 1;
                    pool.releases.push(granted_daa.saturating_add(ticket_window).saturating_add(row.service_daa));
                }
                PalwCourtStageV1::Queued { enqueued_daa } => {
                    let entry = PalwCourtQueuedV1 {
                        key: PalwCourtQueueKeyV1 {
                            deadline_daa: record.hard_deadline_daa,
                            enqueued_daa,
                            claim: *claim,
                            reserver: *reserver,
                        },
                        service_daa: row.service_daa,
                    };
                    if row.guaranteed { pool.guaranteed.push(entry) } else { pool.best_effort.push(entry) }
                }
                PalwCourtStageV1::Running { session, .. } => {
                    if row.reserve {
                        reserve_sessions.insert(session);
                    }
                }
            }
        }
    }
    for (id, session) in state.court_sessions.iter() {
        let pool = if reserve_sessions.contains(id) { &mut reserve } else { &mut spare };
        pool.running += 1;
        pool.releases.push(session.deadline_daa);
    }
    (reserve, spare)
}

/// **G14 F03 (lane g14p/court-owner): how many FOLLOW-ON sessions the unchanged network cap may hold** — a follow-on is a session on
/// a claim that already has a running session. `⌊(cap − 1) / 2⌋`: at least `cap − ⌊(cap − 1) / 2⌋ ≥ 1` slots stay for first
/// sessions (one per distinct claim), so stacking decoy dissections on one claim can never fill the cap. A pure function of the cap.
pub fn palw_court_outsider_lane_followon_cap_v1(cap: u64) -> u64 {
    cap.saturating_sub(1) / 2
}

/// **The legacy concurrency cap** (int-12, audit A M-1), and past `palw_court_outsider_lane_v1` its claim-diverse split (G14 F03).
///
/// Below the fence, byte for byte the old rule: `open >= cap` refuses with `CourtSessionsAtCapacity`. Past it the cap is the SAME
/// number (total concurrency, hence the per-block slot's rung guarantee, is unchanged) but follow-on sessions — a second or later
/// session on a claim that already has one running — may hold at most [`palw_court_outsider_lane_followon_cap_v1`] of it. So a
/// coalition that stacks Sybil dissections on its own decoy claim (G-07 residual) leaves room for every other claim's first
/// session: an outsider pursuing a different (lying) claim opens. The rule reads no owner, opener order or identity — only the set
/// of claims with running sessions — so it is not a first-come exclusive occupancy (ADR-0199): every claim's first session competes
/// for the first-session room on equal terms, and that room can be exhausted only by `≥ cap − F` DISTINCT claims under dispute,
/// each a licensed claim whose decoy accusation costs its accuser the claim's reserve when it fails (residual, see the lane notes).
pub(super) fn legacy_court_cap_admits_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: Hash64,
    opener: PalwBondKeyV2,
    direct_evidence: bool,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    let open_sessions = state.court_sessions.len() as u64;
    let max_sessions = palw_max_concurrent_court_sessions_v1(params.turn_deadline_daa());
    let at_capacity = || PalwStateV2Error::CourtSessionsAtCapacity {
        open: open_sessions,
        max: max_sessions,
        turn_deadline_daa: params.turn_deadline_daa(),
        per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,
    };
    if open_sessions >= max_sessions {
        return Err(at_capacity());
    }
    // Round 2 (requirement): a DIRECT accusation that already carries its evidence (the IR / generative cone accusations, whose
    // proof is the whole refutation) is never refused by the follow-on share — only by the overall cap above, exactly as before
    // the fence. So nothing admitted below the fence on that route is refused past it.
    if direct_evidence || !params.court_outsider_lane_active_v1(now) {
        return Ok(());
    }
    if court_outsider_lane_is_followon_v1(state, claim_id, opener)
        && court_outsider_lane_followons_v1(state) >= palw_court_outsider_lane_followon_cap_v1(max_sessions)
    {
        return Err(at_capacity());
    }
    Ok(())
}

/// **Round 2 (risk 2): the claim's own side.** The bonds the chain records as the claim's accused/colluding side: its producer
/// (`claim.bond`) and the bonds of its panel's seats (`panels[claim].seats[].bond`). Rooted facts only — the claim and panel rows of
/// the chain state. A missing claim or panel contributes nothing.
///
/// **audit B-07 (fix-b): and every bond the state LINKS to that side** — one whose recorded operator id (derived from its key at
/// registration) or payout payload (where its rewards are paid) equals the producer's or a seat's. Such a bond is the same
/// operator / funding destination, so it is not an outsider and cannot take the outside-first slot. A Sybil that shares neither is
/// indistinguishable from an honest outsider on chain; that residual is bounded only by cost (the floor collateral, the failed
/// accusation's charge and, once adopted, the paid ingress).
fn court_outsider_lane_inside_v1(state: &PalwChainStateV2, claim_id: Hash64, bond: &PalwBondKeyV2) -> bool {
    if court_outsider_side_borrowed_v2(state, claim_id).any(|key| key == *bond) {
        return true;
    }
    let Some(record) = state.bonds.get(bond) else { return false };
    court_outsider_side_borrowed_v2(state, claim_id)
        .filter_map(|key| state.bonds.get(&key))
        .any(|s| s.operator_id == record.operator_id || s.payout_payload == record.payout_payload)
}

fn court_outsider_side_borrowed_v2(state: &PalwChainStateV2, claim: Hash64) -> impl Iterator<Item = PalwBondKeyV2> + '_ {
    state.claims.get(&claim).map(|row| row.bond).into_iter()
        .chain(state.panels.get(&claim).into_iter().flat_map(|panel| panel.seats.iter().map(|seat| seat.bond)))
}

/// A session opened by `opener` on `claim_id` is a FOLLOW-ON iff the claim already has a running session opened from the same side
/// (inside = producer/seat, outside = any other bond). So a colluder (seat or producer) holding the claim's first session does not
/// turn the outsider into a follow-on: the outsider's session is the outside side's first and counts against the first-session room.
/// The record read is `PalwCourtSessionStateV2::challenger_bond` — the opener the state records.
pub(super) fn court_outsider_lane_is_followon_v1(state: &PalwChainStateV2, claim_id: Hash64, opener: PalwBondKeyV2) -> bool {
    let inside = court_outsider_lane_inside_v1(state, claim_id, &opener);
    state
        .court_sessions
        .values()
        .any(|s| s.claim == claim_id && court_outsider_lane_inside_v1(state, claim_id, &s.challenger_bond) == inside)
}

/// Follow-on sessions now running: per claim, every session beyond the first of each side (at most two firsts per claim: one inside,
/// one outside). Consistent with [`court_outsider_lane_is_followon_v1`].
pub(super) fn court_outsider_lane_followons_v1(state: &PalwChainStateV2) -> u64 {
    let mut firsts = std::collections::BTreeSet::new();
    let mut followons = 0u64;
    for session in state.court_sessions.values() {
        let side = court_outsider_lane_inside_v1(state, session.claim, &session.challenger_bond);
        if !firsts.insert((session.claim, side)) {
            followons += 1;
        }
    }
    followons
}

// ---- lane r5-f03-ingress (G14 item 3): the paid public ingress and the per-claim outside reservation --------------------------
//
// Dormant unless `palw_court_outsider_lane_v1` is in force AND the ingress fee schedule resolves
// (`crate::palw_court_paid_ingress_v1::palw_court_ingress_schedule_v1`, typed-missing in every non-test build). With the schedule
// missing every function below reduces to the court-owner lane's rule above, byte for byte.

/// The paid ingress's schedule at `now`: `Some` only past the outsider lane fence and with an adopted (test-injected) schedule.
pub(super) fn court_ingress_schedule_at_v1(
    params: &PalwStateParamsV2,
    now: u64,
) -> Option<crate::palw_court_paid_ingress_v1::PalwCourtIngressScheduleV1> {
    if !params.court_outsider_lane_active_v1(now) {
        return None;
    }
    crate::palw_court_paid_ingress_v1::palw_court_ingress_schedule_v1().ok()
}

/// **The once-only court ingress fee the fold burns from a session's opener at `now`** — `None` while the paid ingress is dormant
/// (below `palw_court_outsider_lane_v1`, or the schedule typed-missing: every shipped build). For a node's filer, which pays it
/// only when its operator opted in (`--palw-court-paid-ingress`).
pub fn palw_court_ingress_fee_due_v1(params: &PalwStateParamsV2, now: u64) -> Option<u64> {
    court_ingress_schedule_at_v1(params, now).map(|schedule| schedule.fee_sompi_per_session)
}

/// **In-flight claims that the ingress counts** at `now`: accepted at or past the lane fence, still in a reservable phase (not
/// Final, not voided), hard deadline not passed. Ordered by `(accepted_daa, claim id)` — the admission order, which every node
/// derives alike. Rooted facts only (the claim rows and the params' clock).
fn court_ingress_inflight_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, now: u64) -> Vec<(u64, Hash64)> {
    if params.court_outsider_lane_from_daa().is_none() { return Vec::new(); }
    let mut inflight: Vec<_> = court_ingress_inflight_borrowed_v2(state, params, now).collect();
    inflight.sort_unstable();
    inflight
}

fn court_ingress_inflight_borrowed_v2<'a>(
    state: &'a PalwChainStateV2, params: &'a PalwStateParamsV2, now: u64,
) -> impl Iterator<Item = (u64, Hash64)> + 'a {
    let from = params.court_outsider_lane_from_daa();
    let w_disclose = palw_da_disclose_window_daa_v1(params);
    state
        .claims
        .iter()
        .filter(move |(_, claim)| {
            from.is_some_and(|at| claim.accepted_daa >= at)
                && crate::palw_legacy_public_filer_v1::palw_dispute_phase_reservable_v1(&claim.phase, false)
                && crate::palw_court_clock_v2::palw_claim_court_reserve_hard_v2(params, claim, w_disclose) >= now
        })
        .map(|(id, claim)| (claim.accepted_daa, *id))
}

/// **The claims that hold an outside-side reservation at `now`** (ADR-0192 advance reservation): the first
/// `R = ⌊cap / 2⌋` in-flight claims in admission order. With the admission coordinator calling
/// [`palw_court_ingress_claim_admits_v1`] no more than `R` are ever in flight, so every admitted claim holds one.
pub(super) fn court_ingress_reserved_claims_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2, now: u64) -> Vec<Hash64> {
    let cap = palw_max_concurrent_court_sessions_v1(params.turn_deadline_daa());
    let mut inflight = court_ingress_inflight_v1(state, params, now);
    inflight.truncate(crate::palw_court_paid_ingress_v1::palw_court_ingress_reserved_claims_cap_v1(cap) as usize);
    inflight.into_iter().map(|(_, id)| id).collect()
}

/// Does `claim_id` have a running OUTSIDE-side session (opener neither its producer nor a panel seat)? That session holds the
/// claim's reservation.
pub(super) fn court_ingress_outside_running_v1(state: &PalwChainStateV2, claim_id: Hash64) -> bool {
    state
        .court_sessions
        .values()
        .any(|s| s.claim == claim_id && !court_outsider_lane_inside_v1(state, claim_id, &s.challenger_bond))
}

/// **The admission coordinator's call at ORIGINAL admission** (G14 item 3, court-owner risk 1): may a new claim (of `class_id`)
/// be admitted at `now` with its outside reservation? `Ok` when the paid ingress is dormant (fence or schedule missing). Past it, a
/// new claim is admitted only while fewer than `R = ⌊cap / 2⌋` claims are in flight — so a coalition filling the first-session
/// room with its OWN distinct claims cannot take the target claim's reservation: each claim's reservation is its own, and the
/// count of claims is what is bounded (the round-2 bounded-admission pattern of `palw_court_capacity_v1`, structural `R`). A
/// refusal is `CourtClaimAdmissionFull` (no claim, no reward — liveness degrades, safety does not). A pure read of state.
pub fn palw_court_ingress_claim_admits_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    court_ingress_claim_admits_excluding_v1(state, params, class_id, now, None)
}

/// [`palw_court_ingress_claim_admits_v1`] for an admission path that has ALREADY written the new claim's row before the check
/// (the RFC-0035 A funded install): the claim itself is not counted among the in-flight claims it competes with. `None` is the
/// ordinary pre-write check (the attempt fold and the producer's pre-check). rfcgap-c (RFC-0014 2026-10-19 E / RFC-0026 §3.2 F03):
/// the coordinator call the r5-f03-ingress lane exposed.
pub(super) fn court_ingress_claim_admits_excluding_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    class_id: &Hash64,
    now: u64,
    exclude: Option<Hash64>,
) -> Result<(), PalwStateV2Error> {
    if court_ingress_schedule_at_v1(params, now).is_none() {
        return Ok(());
    }
    let cap = palw_max_concurrent_court_sessions_v1(params.turn_deadline_daa());
    let reserved_cap = crate::palw_court_paid_ingress_v1::palw_court_ingress_reserved_claims_cap_v1(cap);
    let admitted =
        court_ingress_inflight_v1(state, params, now).into_iter().filter(|(_, id)| Some(*id) != exclude).count() as u64;
    if admitted >= reserved_cap {
        return Err(PalwStateV2Error::CourtClaimAdmissionFull { class: *class_id, bucket: 0, admitted, cap: reserved_cap });
    }
    Ok(())
}

/// **The concurrency cap under the paid ingress** (past the lane fence, schedule adopted). The overall cap is unchanged. A claim's
/// FIRST outside-side session opens on its claim's own reservation (it never competes with any other session). Every other
/// session competes for the non-reserved room `cap − #reserved claims` (a reserved claim's slot counts as taken whether or not its
/// outside session runs), and then keeps the court-owner lane's follow-on share (`legacy_court_cap_admits_v1`).
///
/// Design decision (flagged): a direct evidence-carrying accusation stays exempt from the follow-on share (court-owner round 2), but
/// NOT from the reserved room — otherwise cone accusations could fill the reserved slots and the per-claim guarantee would not hold.
pub(super) fn paid_court_cap_admits_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: Hash64,
    opener: PalwBondKeyV2,
    direct_evidence: bool,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    let open_sessions = state.court_sessions.len() as u64;
    let max_sessions = palw_max_concurrent_court_sessions_v1(params.turn_deadline_daa());
    let at_capacity = || PalwStateV2Error::CourtSessionsAtCapacity {
        open: open_sessions,
        max: max_sessions,
        turn_deadline_daa: params.turn_deadline_daa(),
        per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,
    };
    if open_sessions >= max_sessions {
        return Err(at_capacity());
    }
    let reserved = court_ingress_reserved_claims_v1(state, params, now);
    let outside = !court_outsider_lane_inside_v1(state, claim_id, &opener);
    if outside && reserved.contains(&claim_id) && !court_ingress_outside_running_v1(state, claim_id) {
        // The claim's own reservation: no other session can hold it.
        return Ok(());
    }
    let reserved_in_use = reserved.iter().filter(|c| court_ingress_outside_running_v1(state, **c)).count() as u64;
    let non_reserved_open = open_sessions.saturating_sub(reserved_in_use);
    if non_reserved_open.saturating_add(reserved.len() as u64) >= max_sessions {
        return Err(at_capacity());
    }
    legacy_court_cap_admits_v1(state, params, claim_id, opener, direct_evidence, now)
}

/// **Court admission through the paid ingress** — what the session-opening sites call (`CourtOpened`, the named-leaf dissection).
/// Below the lane fence or with the schedule missing: exactly `court_admission_v1` then, on `Legacy`, the court-owner lane's cap.
/// Past both: the same, with [`paid_court_cap_admits_v1`] in place of the legacy cap, and the ingress fee burned ONCE from the
/// opener — at the session's admission on the legacy path; past `palw_court_capacity_v1` when its court row is created (the
/// deadline-aware queue then decides, queued honest prosecution first, G-07), never again when its ticket opens it. A refused
/// admission writes nothing (the fold's object is atomic), so a refused try is never charged.
pub(super) fn court_ingress_admission_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    opener: PalwBondKeyV2,
    direct_evidence: bool,
    session_window: u64,
) -> Result<PalwCourtAdmissionV1, PalwStateV2Error> {
    let now = ctx.daa_score;
    let schedule = court_ingress_schedule_at_v1(builder.params, now);
    let had_row = builder.state.legacy_disputes.get(&claim_id).is_some_and(|record| record.courts.contains_key(&opener));
    let admission = court_admission_v1(builder, ctx, claim_id, opener, session_window)?;
    let charge = match admission {
        PalwCourtAdmissionV1::Legacy => {
            builder.reserve_legacy_court_ingress_workspace_v2(opener, direct_evidence, now)?;
            if schedule.is_some() {
                paid_court_cap_admits_v1(&builder.state, builder.params, claim_id, opener, direct_evidence, now)?;
            } else {
                legacy_court_cap_admits_v1(&builder.state, builder.params, claim_id, opener, direct_evidence, now)?;
            }
            schedule.is_some()
        }
        PalwCourtAdmissionV1::Open | PalwCourtAdmissionV1::Queued => schedule.is_some() && !had_row,
    };
    if charge && let Some(schedule) = schedule {
        charge_court_ingress_fee_v1(builder, opener, schedule.fee_sompi_per_session)?;
    }
    Ok(admission)
}

/// Burn the once-only ingress fee from `opener`'s bond. Never clamps: a bond that cannot pay it in full is refused.
fn charge_court_ingress_fee_v1(builder: &mut TransitionBuilder<'_>, opener: PalwBondKeyV2, fee: u64) -> Result<(), PalwStateV2Error> {
    if fee == 0 {
        return Ok(());
    }
    let collateral = builder.state.bonds.get(&opener).ok_or(PalwStateV2Error::MissingBond(opener))?.collateral;
    if collateral < fee {
        return Err(refused(format!("the court ingress fee {fee} exceeds the opener's collateral {collateral}")));
    }
    builder.slash_bond(opener, u128::from(fee)).map(|_| ())
}

/// **Court admission** where a court session is about to open (`open_dissection_at_named_leaf_v1`, `CourtOpened`). Past the fence:
/// the challenger's own live LG14-A reservation on the claim (its claim-fixed hard deadline, its Final hold), an Active bond at or
/// above the floor, and the occupancy escrow (ρ × (ticket window + `session_window`) × weight × the claim's row count, round 2) on its
/// free half; a session opened now and held to `session_window` must still end by the claim's hard deadline (lateness, not capacity).
/// Then (round 2):
/// * its own ticket opens (its guarantee was proven, or its best effort granted, when it queued);
/// * the claim's FIRST court row — on a court-demanding claim — is its reserve row: it opens on a free reserve slot when nobody waits
///   for one, else it queues on the reserve pool; **never refused for capacity** (the bounded admission is its guarantee);
/// * any other row is a spare row: it opens on a free spare slot when nobody waits for one; else it queues GUARANTEED when the
///   worst-case service accounting shows that its session, and that of every guaranteed spare entry already queued, still ends by its
///   claim's hard deadline — and otherwise BEST-EFFORT, behind every guaranteed entry. **Never refused for capacity.**
pub(super) fn court_admission_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    challenger: PalwBondKeyV2,
    session_window: u64,
) -> Result<PalwCourtAdmissionV1, PalwStateV2Error> {
    let now = ctx.daa_score;
    let Some(mirror) = builder.params.court_capacity_v1().filter(|m| m.active_at(now)) else { return Ok(PalwCourtAdmissionV1::Legacy) };
    let policy = mirror.policy;
    let record = builder
        .state
        .legacy_disputes
        .get(&claim_id)
        .filter(|record| record.live.contains_key(&challenger))
        .ok_or_else(|| refused(PALW_COURT_REFUSAL_NO_RESERVATION_V1))?;
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
    // An existing row: its own ticket opens; a queued or running prosecution files nothing more on this claim.
    if let Some(row) = record.courts.get(&challenger) {
        return match row.stage {
            PalwCourtStageV1::Ticket { expires_daa, .. } if now <= expires_daa => Ok(PalwCourtAdmissionV1::Open),
            PalwCourtStageV1::Ticket { .. } => Err(refused(PALW_COURT_REFUSAL_TICKET_EXPIRED_V1)),
            PalwCourtStageV1::Queued { .. } => Err(refused(PALW_COURT_REFUSAL_QUEUED_V1)),
            PalwCourtStageV1::Running { .. } => Err(refused(PALW_COURT_REFUSAL_RUNNING_V1)),
        };
    }
    if now.saturating_add(session_window) > record.hard_deadline_daa {
        return Err(refused(
            "a court session opened now could outlast the claim's hard deadline: its verdict must land inside it (the court term of \
             LG14-A's hard-deadline envelope)",
        ));
    }
    let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
    // G14 lane r6-d1-release: a funded original admitted under the capacity fence took its reserve at admission
    // (`check_funded_court_reserve_v1`); its first court row runs on it.
    let reserve = !record.court_reserve_used
        && (palw_court_demanding_claim_v1(&builder.state, mirror.from_daa, claim)
            || (claim.accepted_daa >= mirror.from_daa && funded_court_reserve_claims_borrowed_v2(&builder.state).any(|id| id == claim_id)));
    // Round 2 (limit 2): the j-th concurrent court row on one claim pays j × the occupancy rate.
    let weight = policy.weight().saturating_mul(1 + record.courts.len() as u64);
    let escrow = policy.fee(ticket_window.saturating_add(session_window), weight);
    if escrow > 0 {
        builder.check_accuser_room(challenger, escrow, "court occupancy escrow", now)?;
    }
    // All preceding admission checks borrow state only. Admit the clone, pool
    // construction and scheduler before their first allocation or row write.
    builder.reserve_court_admission_workspace_v2(claim_id, challenger, now)?;
    let mut record = builder.state.legacy_disputes.get(&claim_id).expect("checked reservation, unchanged by admission quote").clone();
    let (reserve_pool, spare_pool) = court_pools_v1(&builder.state, ticket_window);
    let (pool, slots) =
        if reserve { (reserve_pool, policy.reserve_slots(turn)) } else { (spare_pool, policy.spare_slots(turn)) };
    let row = |stage, guaranteed| PalwReservedCourtV1 {
        stage,
        escrow,
        rho_sompi: policy.rho_sompi,
        weight,
        service_daa: session_window,
        reserve,
        guaranteed,
        admitted_daa: now,
    };
    record.court_reserve_used |= reserve;
    if pool.occupied() < slots && pool.queued() == 0 {
        // The session opens in this object; `court_opened_v1` names it.
        record.courts.insert(challenger, row(PalwCourtStageV1::Running { session: ZERO_HASH64, started_daa: now }, true));
        builder.write_legacy_dispute(claim_id, Some(record))?;
        return Ok(PalwCourtAdmissionV1::Open);
    }
    let guaranteed = reserve || {
        let mut queue = pool.guaranteed.clone();
        queue.push(PalwCourtQueuedV1 {
            key: PalwCourtQueueKeyV1 { deadline_daa: record.hard_deadline_daa, enqueued_daa: now, claim: claim_id, reserver: challenger },
            service_daa: session_window,
        });
        let mut releases = pool.releases.clone();
        releases.sort_unstable();
        palw_court_schedule_v1(now, slots, &releases, &queue, ticket_window).is_ok()
    };
    record.courts.insert(challenger, row(PalwCourtStageV1::Queued { enqueued_daa: now }, guaranteed));
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
    builder.reserve_court_opened_workspace_v2(claim_id)?;
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
    builder.reserve_court_capacity_sweep_workspace_v2()?;
    let turn = builder.params.turn_deadline_daa();
    let ticket_window = policy.ticket_window(turn);
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
    // 3. Grants (round 2): each pool's free slots, earliest claim deadline first — the reserve queue on the reserve slots; on the
    //    spare slots every guaranteed entry before any best-effort one.
    let (reserve, spare) = court_pools_v1(&builder.state, ticket_window);
    let reserve_free = policy.reserve_slots(turn).saturating_sub(reserve.occupied());
    let spare_free = policy.spare_slots(turn).saturating_sub(spare.occupied());
    let mut grants: Vec<PalwCourtQueuedV1> = Vec::new();
    let mut reserve_queue = reserve.guaranteed;
    reserve_queue.extend(reserve.best_effort);
    reserve_queue.sort_by_key(|entry| entry.key);
    grants.extend(reserve_queue.into_iter().take(reserve_free as usize));
    let mut guaranteed = spare.guaranteed;
    guaranteed.sort_by_key(|entry| entry.key);
    let mut best_effort = spare.best_effort;
    best_effort.sort_by_key(|entry| entry.key);
    grants.extend(guaranteed.into_iter().chain(best_effort).take(spare_free as usize));
    for entry in grants {
        let Some(mut record) = builder.state.legacy_disputes.get(&entry.key.claim).cloned() else { continue };
        let Some(row) = record.courts.get_mut(&entry.key.reserver) else { continue };
        row.stage = PalwCourtStageV1::Ticket { granted_daa: now, expires_daa: now.saturating_add(ticket_window) };
        builder.write_legacy_dispute(entry.key.claim, Some(record))?;
    }
    Ok(())
}

/// **The unserved-claim rule** (round 2): does `record` — a claim whose reservations lapse at `now` — hold a live reservation whose
/// court row is still WAITING (queued or ticketed) though it was admitted at least one service window (ticket + session) before the
/// hard deadline? Then the court did not serve a prosecution it had time to serve, and the claim (pre-Final) is voided
/// `PanelUnavailable` instead of released: it earns nothing. `false` below the fence (no court row exists there).
pub(super) fn court_unserved_at_lapse_v1(params: &PalwStateParamsV2, record: &crate::palw_legacy_public_filer_v1::PalwDisputeClaimV1, now: u64) -> bool {
    let Some(policy) = params.court_capacity_at_v1(now) else { return false };
    let ticket_window = policy.ticket_window(params.turn_deadline_daa());
    record.courts.iter().any(|(reserver, row)| {
        record.live.contains_key(reserver)
            && row.waiting()
            && row.admitted_daa.saturating_add(ticket_window).saturating_add(row.service_daa) <= record.hard_deadline_daa
    })
}


impl TransitionBuilder<'_> {
    pub(super) fn reserve_legacy_court_ingress_workspace_v2(
        &mut self, opener: PalwBondKeyV2, direct_evidence: bool, now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() { return Ok(()); }
        let quote = legacy_ingress_workspace::quote(&self.state, self.params, opener, direct_evidence, now)?;
        self.admit_native_write_v1(quote)
    }

    pub(super) fn reserve_court_admission_workspace_v2(
        &mut self, claim: Hash64, challenger: PalwBondKeyV2, now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() { return Ok(()); }
        let quote = admission_workspace::quote(&self.state, self.params, claim, challenger, now)?;
        self.admit_native_write_v1(quote)
    }

    pub(super) fn reserve_court_opened_workspace_v2(&mut self, claim: Hash64) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() { return Ok(()); }
        let mut quote = native_writers::NativeWriteQuoteV1::new("court admitted session row handoff");
        if let Some(record) = self.state.legacy_disputes.get(&claim) { quote.copy(record)?; }
        self.admit_native_write_v1(quote)
    }

    pub(super) fn reserve_court_capacity_sweep_workspace_v2(&mut self) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() { return Ok(()); }
        self.admit_native_write_v1(sweep_workspace::quote(&self.state)?)
    }
}
