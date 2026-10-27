//! **PALW user views (tooling lane, read-only): one consolidated document per question a user asks** — "where does my claim stand",
//! "what is the court doing", "what does my bond hold", "which fences are armed". RPC ops 225–228 serve them as versioned JSON; the
//! `misaka palw claim-detail | court-state | bond-detail | fences` commands print them.
//!
//! **Nothing here is a rule.** Every figure is read through the SAME function the fold or an existing read uses
//! ([`crate::palw_producer_v2::palw_claim_phase_deadline_v1`], [`crate::palw_court_clock_v2::palw_claim_court_hard_deadline_v2`],
//! [`PalwChainStateV2::palw_accusation_pending_v1`], [`crate::palw_receipt_pause_v2::palw_receipt_pause_row_v2`],
//! [`crate::palw_security_certification_v1::palw_claim_certification_status_v1`],
//! [`crate::palw_producer_v2::palw_bond_live_locks_v1`], [`crate::config::params::Params::palw_fences_v1`] …); no clock, deadline or
//! status is recomputed here by a formula of its own. A value the chain does not hold is `null`, never a guess; a read fault is named
//! (`"unavailable"` / an `error` string), never zero-filled. No rule reads these documents.

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimStateV2, PalwCourtSessionStateV2, PalwStateParamsV2,
};
use kaspa_hashes::Hash64;
use serde_json::{Value, json};

/// The documents' schema version (each carries `"version"`); a reader refuses a version it does not know.
pub const PALW_USER_VIEW_VERSION_V1: u32 = 1;

/// The node-local facts the next block's fold reads, which a pure read of the committed state cannot derive by itself: the DAA the
/// next block folds at, the raw second-clock depth there, and whether `palw_objective_offence` is armed there (the bond-lock read's
/// inputs, exactly as `getPalwClaims` passes them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwUserViewClockV1 {
    pub next_daa: u64,
    pub raw_depth: Option<u64>,
    pub objective_offence: bool,
}

/// `<txid>:<index>`.
pub fn palw_bond_key_text_v1(bond: &PalwBondKeyV2) -> String {
    format!("{}:{}", bond.0.transaction_id, bond.0.index)
}

/// A phase's name, its DAA and (for a void) its reason — the spelling `getPalwClaims` uses.
pub fn palw_claim_phase_text_v1(phase: &PalwClaimPhaseV2) -> (&'static str, u64, Option<String>) {
    match phase {
        PalwClaimPhaseV2::Provisional => ("provisional", 0, None),
        PalwClaimPhaseV2::PanelBound { bound_daa } => ("panel_bound", *bound_daa, None),
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => ("receipt_licensed", *licensed_daa, None),
        PalwClaimPhaseV2::Final { final_daa } => ("final", *final_daa, None),
        PalwClaimPhaseV2::Voided { voided_daa, reason } => ("voided", *voided_daa, Some(format!("{reason:?}"))),
        PalwClaimPhaseV2::DefaultDisputed { accused_daa, .. } => ("default_disputed", *accused_daa, None),
    }
}

fn opt_u64(v: Option<u64>) -> Value {
    v.map(Value::from).unwrap_or(Value::Null)
}

fn u128_text(v: u128) -> Value {
    Value::String(v.to_string())
}

fn activation_daa(a: Option<ForkActivation>) -> Value {
    match a {
        None => Value::Null,
        Some(a) if a.daa_score() == u64::MAX => Value::Null,
        Some(a) => Value::from(a.daa_score()),
    }
}

// ---- fences --------------------------------------------------------------------------------------------------------------------

/// The fence lists a user asks about by name, with the height constant each arms at (`None`: no height adopted — dormant).
fn palw_named_fence_lists_v1() -> Vec<(&'static str, Option<u64>, &'static [PalwPostLaunchFenceV1])> {
    use crate::config::params::*;
    vec![
        ("fullActivation", PALW_T12_FULL_ACTIVATION_DAA, PALW_T12_FULL_ACTIVATION_FENCES_V1),
        ("int13", PALW_T12_INT13_DAA, PALW_T12_INT13_FENCES_V1),
        ("postLaunchV1", Some(PALW_T12_POST_LAUNCH_FENCE_DAA), PALW_T12_POST_LAUNCH_FENCES_V1),
        ("postLaunchV2", None, PALW_T12_POST_LAUNCH_FENCES_V2),
        ("postLaunchV3", None, PALW_T12_POST_LAUNCH_FENCES_V3),
        ("postLaunchV4", None, PALW_T12_POST_LAUNCH_FENCES_V4),
        ("int11", None, PALW_T12_INT11_FENCES_V1),
        ("tirFlagDay", PALW_T12_TIR_FLAG_DAY_DAA, PALW_T12_TIR_FLAG_DAY_FENCES_V1),
        ("modelCourtWindow", PALW_T12_MODEL_COURT_WINDOW_DAA, PALW_T12_MODEL_COURT_WINDOW_FENCES_V1),
    ]
}

/// One fence as the node runs it: `activationDaa` (`null`: unscheduled), and `armed` — in force at `now_daa`
/// ([`ForkActivation::is_active`], the predicate every fold site reads).
fn fence_row(name: &str, activation: Option<ForkActivation>, now_daa: u64, lists: Vec<&'static str>) -> Value {
    json!({
        "name": name,
        "activationDaa": activation_daa(activation),
        "armed": activation.is_some_and(|a| a.is_active(now_daa)),
        "scheduled": activation.is_some_and(|a| a.daa_score() != u64::MAX),
        "lists": lists,
    })
}

/// **`getPalwFenceSchedule` (op 227): every top-level PALW fence of `params`, named** ([`Params::palw_fences_v1`], the exhaustive,
/// compiler-forced list), with its height and whether it is armed at `now_daa`, which named release lists carry it, and each list's
/// adopted height (`null`: not adopted — e.g. the full-activation list's `PALW_T12_FULL_ACTIVATION_DAA`, target `PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1`).
pub fn palw_fence_schedule_json_v1(params: &Params, now_daa: u64) -> Value {
    let lists = palw_named_fence_lists_v1();
    let fences: Vec<Value> = params
        .palw_fences_v1()
        .into_iter()
        .map(|(name, activation)| {
            let member: Vec<&'static str> =
                lists.iter().filter(|(_, _, entries)| entries.iter().any(|e| e.name == name)).map(|(list, _, _)| *list).collect();
            fence_row(name, activation, now_daa, member)
        })
        .collect();
    let all = params.palw_fences_v1();
    let list_docs: Vec<Value> = lists
        .iter()
        .map(|(list, height, entries)| {
            let members: Vec<Value> = entries
                .iter()
                .map(|e| {
                    let activation = all.iter().find(|(n, _)| *n == e.name).and_then(|(_, a)| *a);
                    json!({
                        "name": e.name,
                        "activationDaa": activation_daa(activation),
                        "armed": activation.is_some_and(|a| a.is_active(now_daa)),
                    })
                })
                .collect();
            let armed = members.iter().filter(|m| m["armed"] == Value::Bool(true)).count();
            json!({
                "list": list,
                "adoptedDaa": opt_u64(*height),
                "status": if height.is_some() { "adopted" } else { "not adopted" },
                "entries": members.len(),
                "armedEntries": armed,
                "members": members,
            })
        })
        .collect();
    let armed = fences.iter().filter(|f| f["armed"] == Value::Bool(true)).count();
    json!({
        "version": PALW_USER_VIEW_VERSION_V1,
        "kind": "palwFenceSchedule",
        "network": params.net.to_string(),
        "nowDaa": now_daa,
        "fenceCount": fences.len(),
        "armedCount": armed,
        "fullActivation": {
            "adoptedDaa": opt_u64(crate::config::params::PALW_T12_FULL_ACTIVATION_DAA),
            "targetDaa": crate::config::params::PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1,
            "targetNote": "intended height only; never read by any preset",
        },
        "lists": list_docs,
        "fences": fences,
    })
}

// ---- claim -----------------------------------------------------------------------------------------------------------------------

/// The court clocks of `claim` as the fold reads them ([`crate::palw_court_clock_v2`]): whether clock v2 governs it (T15), the hard
/// deadline every fold site reads, and — governed only — the initial-notice boundary (`a + 3,120`) and the extended hard deadline
/// (`a + 10,000`).
fn court_clocks_json(state: &PalwChainStateV2, params: &PalwStateParamsV2, claim_id: &Hash64, claim: &PalwClaimStateV2) -> Value {
    use crate::palw_court_clock_v2 as clock;
    let w_disclose = crate::palw_state_v2::palw_da_disclose_window_daa_v1(params);
    let record = state.legacy_dispute_v1(claim_id);
    let governs = clock::palw_court_clock_v2_governs(params, claim);
    let hard = clock::palw_claim_court_hard_deadline_v2(params, record, claim, w_disclose);
    let reserve_hard = clock::palw_claim_court_reserve_hard_v2(params, claim, w_disclose);
    json!({
        "clockV2Governs": governs,
        "acceptedDaa": claim.accepted_daa,
        "traceRetentionDaa": claim.trace_retention_daa,
        "wDisclose": w_disclose,
        "hardDeadlineDaa": hard,
        "reserveHardDeadlineDaa": reserve_hard,
        "noticeUntilDaa": if governs { Value::from(clock::palw_court_clock_v2_normal_hard(claim.accepted_daa)) } else { Value::Null },
        "extendedHardDeadlineDaa":
            if governs { Value::from(clock::palw_court_clock_v2_extended_hard(claim.accepted_daa)) } else { Value::Null },
        "extended": record.is_some(),
        "noticeWindowDaa": crate::palw_court_policy_v1::NOTICE_WINDOW_DAA,
        "disputeWindowDaa": crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA,
        "turnDeadlineDaa": params.turn_deadline_daa(),
        "windowCourt": params.window_court(),
    })
}

/// One court session: its backstop and the ladder's turn and per-turn deadline (the rung the sweep reads silence against).
pub fn palw_court_session_json_v1(session_id: &Hash64, session: &PalwCourtSessionStateV2, now_daa: u64) -> Value {
    let ladder = &session.ladder;
    let (lo, hi) = ladder.interval();
    json!({
        "sessionId": session_id.to_string(),
        "claimId": session.claim.to_string(),
        "challengerBond": palw_bond_key_text_v1(&session.challenger_bond),
        "openedDaa": session.opened_daa,
        "backstopDaa": session.deadline_daa,
        "backstopPassed": now_daa > session.deadline_daa,
        "ladder": {
            "round": ladder.round(),
            "turn": format!("{:?}", ladder.turn()),
            "turnDeadlineDaa": ladder.last_deadline_daa(),
            "interval": [lo, hi],
            "expectedMidpoint": opt_u64(ladder.expected_midpoint()),
            "terminalIndex": opt_u64(ladder.terminal_index()),
            "disclosures": ladder.disclosures.len(),
        },
        "dissectionPhase": session.dissection.is_some(),
    })
}

fn receipt_pause_json(state: &PalwChainStateV2, params: &PalwStateParamsV2, claim_id: &Hash64, claim: &PalwClaimStateV2) -> Value {
    let applies = crate::palw_receipt_pause_v2::palw_receipt_pause_applies_v2(params, claim);
    match crate::palw_receipt_pause_v2::palw_receipt_pause_row_v2(state, claim_id) {
        Ok(None) => json!({ "applies": applies, "row": false, "open": false, "openSinceDaa": null, "pausedTotalDaa": 0 }),
        Ok(Some(row)) => {
            let account = row.account();
            json!({
                "applies": applies,
                "row": true,
                "open": account.open_since_daa.is_some(),
                "openSinceDaa": opt_u64(account.open_since_daa),
                "pausedTotalDaa": account.paused_total_daa,
            })
        }
        Err(e) => json!({ "applies": applies, "error": e.to_string() }),
    }
}

fn dispute_json(state: &PalwChainStateV2, claim_id: &Hash64) -> Value {
    let Some(record) = state.legacy_dispute_v1(claim_id) else {
        return json!({ "record": false, "holds": false });
    };
    let live: Vec<Value> = record
        .live
        .iter()
        .map(|(reserver, row)| {
            json!({
                "reserver": palw_bond_key_text_v1(reserver),
                "reservedDaa": row.reserved_daa,
                "depositSompi": u128_text(row.deposit),
                "sessionsOpened": row.sessions_opened,
                "lanesJoined": row.lanes_joined,
                "courtCapacity": record.courts.get(reserver).map(|c| Value::String(format!("{c:?}"))).unwrap_or(Value::Null),
            })
        })
        .collect();
    json!({
        "record": true,
        "holds": state.palw_legacy_dispute_holds_v1(claim_id),
        "openedDaa": record.opened_daa,
        "hardDeadlineDaa": record.hard_deadline_daa,
        "live": live,
        "closedReservers": record.closed.iter().map(palw_bond_key_text_v1).collect::<Vec<_>>(),
        "dismissedHeld": record.dismissed_held.len(),
        "reacquiring": record.reacquiring.len(),
        "courtReserveUsed": record.court_reserve_used,
    })
}

fn da_sessions_json(state: &PalwChainStateV2, claim_id: &Hash64, now_daa: u64) -> Vec<Value> {
    state
        .da_sessions_of(claim_id)
        .map(|(accuser, session)| {
            json!({
                "accuser": palw_bond_key_text_v1(accuser),
                "openedDaa": session.opened_daa,
                "deadlineDaa": session.deadline_daa,
                "deadlinePassed": now_daa > session.deadline_daa,
                "accuserIsSeat": session.accuser_is_seat,
                "units": session.units.len(),
                "stage": format!("{:?}", session.stage),
                "exposureSompi": u128_text(session.exposure),
            })
        })
        .collect()
}

fn claim_row_json(row: &crate::palw_producer_v2::PalwClaimRowV1) -> Value {
    let (phase, phase_daa, void_reason) = palw_claim_phase_text_v1(&row.phase);
    json!({
        "claimId": row.claim_id.to_string(),
        "classId": row.class_id.to_string(),
        "executorBond": palw_bond_key_text_v1(&row.executor_bond),
        "freePrompt": row.free_prompt,
        "quanta": row.quanta,
        "quantaSpent": row.quanta_spent,
        "phase": phase,
        "phaseDaa": phase_daa,
        "voidReason": void_reason,
        "acceptedDaa": row.accepted_daa,
        "acceptedBlock": row.accepted_block.to_string(),
        "reboundDaa": opt_u64(row.rebound_daa),
        "boundDaa": opt_u64(row.bound_daa),
        "seats": row.seats.iter().map(palw_bond_key_text_v1).collect::<Vec<_>>(),
        "phaseDeadlineDaa": opt_u64(row.deadline_daa),
        "reservedSompi": u128_text(row.reserved),
        "committedSompi": row.committed.map(u128_text).unwrap_or(Value::Null),
        "reservationSplit": row.reservation_split.map(|s| Value::String(s.audit_json())).unwrap_or(Value::Null),
        "securityCertification": row.security_certification,
        "claimLocalSettlement": if row.claim_local_settlement.is_empty() { Value::Null } else { Value::String(row.claim_local_settlement.clone()) },
        "escrowedRewardSompi": row.escrowed_reward,
        "payoutPendingSompi": opt_u64(row.payout_pending),
        "workLeaves": row.work_leaves,
        "openCourts": row.open_courts,
        "execLane": row.exec_lane.as_ref().map(|l| json!({
            "stage": l.stage, "credit": l.credit, "span": l.span, "tickets": l.tickets, "ticketsSpent": l.tickets_spent,
            "firstRound": opt_u64(l.first_round), "lastRound": opt_u64(l.last_round),
        })).unwrap_or(Value::Null),
    })
}

/// **`getPalwClaimDetail` (op 225): one claim, everything a user follows it by** — the operator row `getPalwClaims` serves (phase, the
/// date the phase ends by itself, reservation, Gate C certification, claim-local settlement, payout, exec lane), whether an accusation
/// is pending (the one predicate every non-fraud end reads), the receipt-window pause row, the court clocks (notice / hard / extended),
/// the dispute record and its live reservations, the open DA and court sessions with their per-turn deadlines. `None` for a claim the
/// state does not hold (retired or never accepted).
pub fn palw_claim_detail_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: &Hash64,
    clock: &PalwUserViewClockV1,
    anchor_delay: Option<u64>,
) -> Option<Value> {
    let claim = state.claim(claim_id)?;
    let mut row = crate::palw_producer_v2::palw_claim_row_v1(state, params, claim_id)?;
    if let Some(delay) = anchor_delay {
        crate::palw_producer_v2::palw_claim_rows_bind_by_anchor_slot_v1(std::slice::from_mut(&mut row), params, delay);
    }
    let now = clock.next_daa;
    let sessions: Vec<Value> = state
        .court_sessions_iter()
        .filter(|(_, s)| s.claim == *claim_id)
        .map(|(id, s)| palw_court_session_json_v1(id, s, now))
        .collect();
    Some(json!({
        "version": PALW_USER_VIEW_VERSION_V1,
        "kind": "palwClaimDetail",
        "nowDaa": now,
        "tipDaa": state.last_point().map(|p| p.daa_score).unwrap_or(0),
        "claim": claim_row_json(&row),
        "gateCGoverns": crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, claim),
        "accusationPending": state.palw_accusation_pending_v1(claim_id),
        "receiptPause": receipt_pause_json(state, params, claim_id, claim),
        "courtClocks": court_clocks_json(state, params, claim_id, claim),
        "dispute": dispute_json(state, claim_id),
        "daSessions": da_sessions_json(state, claim_id, now),
        "courtSessions": sessions,
        "notes": [
            "phaseDeadlineDaa is the date the phase ends by itself (palw_claim_phase_deadline_v1); a court or accusation can hold it",
            "Final is not SecurityCertified: only a certified claim carries consensus credit (RFC-0018 Gate C)",
        ],
    }))
}

/// What `getPalwCourtState` (op 226) is asked about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCourtTargetV1 {
    Claim(Hash64),
    Session(Hash64),
}

/// **`getPalwCourtState` (op 226): the court around one claim, or one session** — every open court session with its ladder turn and
/// per-turn deadline, the DA sessions, the dispute record and its reservations, and the claim's court clocks. `None` when neither the
/// session nor the claim is held.
pub fn palw_court_state_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    target: PalwCourtTargetV1,
    clock: &PalwUserViewClockV1,
) -> Option<Value> {
    let now = clock.next_daa;
    let (claim_id, only_session) = match target {
        PalwCourtTargetV1::Claim(c) => (c, None),
        PalwCourtTargetV1::Session(s) => (state.court_session(&s)?.claim, Some(s)),
    };
    let sessions: Vec<Value> = state
        .court_sessions_iter()
        .filter(|(id, s)| s.claim == claim_id && only_session.is_none_or(|only| **id == only))
        .map(|(id, s)| palw_court_session_json_v1(id, s, now))
        .collect();
    let claim = state.claim(&claim_id);
    if claim.is_none() && sessions.is_empty() {
        return None;
    }
    Some(json!({
        "version": PALW_USER_VIEW_VERSION_V1,
        "kind": "palwCourtState",
        "nowDaa": now,
        "claimId": claim_id.to_string(),
        "claimHeld": claim.is_some(),
        "phase": claim.map(|c| palw_claim_phase_text_v1(&c.phase).0),
        "accusationPending": state.palw_accusation_pending_v1(&claim_id),
        "openCourts": state.open_courts_of(&claim_id),
        "courtSessions": sessions,
        "daSessions": da_sessions_json(state, &claim_id, now),
        "dispute": dispute_json(state, &claim_id),
        "courtClocks": claim.map(|c| court_clocks_json(state, params, &claim_id, c)).unwrap_or(Value::Null),
    }))
}

// ---- bond ------------------------------------------------------------------------------------------------------------------------

/// The cap on the claim rows a bond detail lists per role (the totals are counted whole).
pub const PALW_USER_VIEW_BOND_ROWS_V1: usize = 50;

/// **`getPalwBondDetail` (op 228): one bond** — its registry row (collateral net of slashes, status, classes), what it has reserved
/// (claims and registrations, the figures the exposure ceiling is measured against), its live slashable locks judged at the next block
/// ([`crate::palw_producer_v2::palw_bond_live_locks_v1`], the read `bond retire` needs), its live claims as executor and as seat, the
/// dispute reservations it holds and the court sessions it prosecutes. `None` for an outpoint the registry does not hold.
pub fn palw_bond_detail_json_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    bond: &PalwBondKeyV2,
    clock: &PalwUserViewClockV1,
) -> Option<Value> {
    let row = state.bond(bond)?;
    let summary = crate::palw_producer_v2::palw_bond_summary_v1(state, bond)?;
    let locks = crate::palw_producer_v2::palw_bond_live_locks_v1(
        state,
        params,
        bond,
        clock.next_daa,
        clock.raw_depth,
        clock.objective_offence,
        PALW_USER_VIEW_BOND_ROWS_V1,
    );
    let role_rows = |role| {
        let (rows, truncated) =
            crate::palw_producer_v2::palw_claim_rows_v1(state, params, bond, role, false, PALW_USER_VIEW_BOND_ROWS_V1);
        json!({
            "truncated": truncated,
            "rows": rows.iter().map(|r| {
                let (phase, phase_daa, _) = palw_claim_phase_text_v1(&r.phase);
                json!({
                    "claimId": r.claim_id.to_string(),
                    "phase": phase,
                    "phaseDaa": phase_daa,
                    "phaseDeadlineDaa": opt_u64(r.deadline_daa),
                    "reservedSompi": u128_text(r.reserved),
                    "securityCertification": r.security_certification,
                    "openCourts": r.open_courts,
                })
            }).collect::<Vec<_>>(),
        })
    };
    let prosecuting: Vec<Value> = state
        .court_sessions_iter()
        .filter(|(_, s)| s.challenger_bond == *bond)
        .map(|(id, s)| palw_court_session_json_v1(id, s, clock.next_daa))
        .collect();
    let (status, retiring_since) = match row.status {
        PalwBondStatusV2::Active => ("active", None),
        PalwBondStatusV2::Retiring { since_daa, .. } => ("retiring", Some(since_daa)),
    };
    Some(json!({
        "version": PALW_USER_VIEW_VERSION_V1,
        "kind": "palwBondDetail",
        "nowDaa": clock.next_daa,
        "bond": palw_bond_key_text_v1(bond),
        "operatorId": row.operator_id.to_string(),
        "status": status,
        "retiringSinceDaa": opt_u64(retiring_since),
        "registeredDaa": summary.registered_daa,
        "collateralSompi": summary.collateral,
        "slashedSompi": summary.slashed,
        "capableClasses": summary.capable_classes.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
        "reservedExposureSompi": u128_text(state.reserved_exposure(bond)),
        "registrationExposureSompi": u128_text(state.registration_exposure(bond)),
        "locks": {
            "atDaa": locks.at_daa,
            "liveCount": locks.live_count,
            "liveLockedSompi": u128_text(locks.live_locked),
            "daaClockRelease": locks.daa_clock_release,
            "anchorCountBound": opt_u64(locks.anchor_count_bound),
            "retireRefusedWhileLocked": locks.retire_refused_while_locked,
            "truncated": locks.truncated,
            "rows": locks.locks.iter().map(|l| json!({
                "claimId": l.claim_id.to_string(),
                "amountSompi": u128_text(l.amount),
                "expiryDaa": l.expiry_daa,
                "heldByAnchorCount": l.held_by_anchor_count,
            })).collect::<Vec<_>>(),
        },
        "claimsAsExecutor": role_rows(crate::palw_producer_v2::PalwClaimRoleV1::Executor),
        "claimsAsSeat": role_rows(crate::palw_producer_v2::PalwClaimRoleV1::Seat),
        "disputeReservations": state
            .palw_legacy_disputes_live_v1(Some(bond), PALW_USER_VIEW_BOND_ROWS_V1)
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        "prosecutingSessions": prosecuting,
    }))
}

// ---- typed-missing values ---------------------------------------------------------------------------------------------------------

/// **One value consensus holds typed-missing** (a fee schedule, a slash amount, a budget): no shipped build adopts it, so every object
/// that needs it is refused by the fold. `adopted` is read from the SAME accessor the fold reads, so the row turns `true` by itself
/// the day a reviewed change adopts the value; a builder that needs an un-adopted value refuses rather than guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTypedMissingV1 {
    pub name: &'static str,
    /// Where consensus reads it.
    pub accessor: &'static str,
    pub adopted: bool,
    /// The interactions that cannot proceed while it is missing.
    pub needed_by: &'static str,
}

/// Every typed-missing value a user-facing builder depends on, read from the accessors consensus reads (this build's).
pub fn palw_typed_missing_values_v1() -> Vec<PalwTypedMissingV1> {
    vec![
        PalwTypedMissingV1 {
            name: "funded original schedule",
            accessor: "palw_funded_original_coordinator_v1::palw_funded_original_schedule_v1",
            adopted: crate::palw_funded_original_coordinator_v1::palw_funded_original_schedule_v1(
                &crate::palw_state_v2::PalwTransitionExtrasV1::default(),
            )
            .is_ok(),
            needed_by: "funded original (TFO1 envelope, payer consent: fee / capital / retention / settlement profile ids)",
        },
        PalwTypedMissingV1 {
            name: "court paid-ingress schedule",
            accessor: "palw_court_paid_ingress_v1::palw_court_ingress_schedule_v1",
            adopted: crate::palw_court_paid_ingress_v1::palw_court_ingress_schedule_v1().is_ok(),
            needed_by: "court ingress fee (outsider lane session fee)",
        },
        PalwTypedMissingV1 {
            name: "material intake schedule",
            accessor: "palw_court_shared_material_intake_v1::material_intake_schedule_v1",
            adopted: crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().is_ok(),
            needed_by: "material intake parts (tag 255: sompi per byte, draft TTL, cover)",
        },
        PalwTypedMissingV1 {
            name: "D2 disclosure fee schedule",
            accessor: "palw_court_model_scope_routes_v1::readiness_disclosure::PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1",
            adopted: crate::palw_court_model_scope_routes_v1::readiness_disclosure::PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1.is_some(),
            needed_by: "D2 readiness demand (tag 231)",
        },
        PalwTypedMissingV1 {
            name: "D2 readiness draw cap",
            accessor: "palw_court_model_scope_routes_v1::readiness_root::PALW_D2_READINESS_DRAW_CAP_V1",
            adopted: crate::palw_court_model_scope_routes_v1::readiness_root::PALW_D2_READINESS_DRAW_CAP_V1.is_some(),
            needed_by: "D2 readiness root (tag 230)",
        },
        PalwTypedMissingV1 {
            name: "court service profile",
            accessor: "palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1",
            adopted: crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1.is_some(),
            needed_by: "shared court service binding (tag 178 supply / defaults)",
        },
        PalwTypedMissingV1 {
            name: "TIR position witness budget",
            accessor: "palw_tir_position_terminal_v2::PALW_TIR_POSITION_WITNESS_BUDGET_V2",
            adopted: crate::palw_tir_position_terminal_v2::PALW_TIR_POSITION_WITNESS_BUDGET_V2.is_some(),
            needed_by: "position terminal close (tags 201–202)",
        },
        PalwTypedMissingV1 {
            name: "TIR lookahead frontier bound",
            accessor: "palw_tir_position_terminal_v2::PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2",
            adopted: crate::palw_tir_position_terminal_v2::PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2.is_some(),
            needed_by: "lookahead disclose / choose (tags 203–204)",
        },
    ]
}

/// The register as JSON (`misaka palw not-adopted --json`).
pub fn palw_typed_missing_json_v1() -> Value {
    let rows: Vec<Value> = palw_typed_missing_values_v1()
        .iter()
        .map(|m| {
            json!({
                "name": m.name,
                "accessor": m.accessor,
                "status": if m.adopted { "adopted" } else { "not adopted" },
                "neededBy": m.needed_by,
            })
        })
        .collect();
    json!({ "version": PALW_USER_VIEW_VERSION_V1, "kind": "palwTypedMissing", "values": rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::palw_t12_shipped_params;

    #[test]
    fn fence_schedule_names_every_fence_and_marks_none_armed_on_dormant_preset() {
        let params = palw_t12_shipped_params();
        let doc = palw_fence_schedule_json_v1(&params, 0);
        assert_eq!(doc["version"], 1);
        assert_eq!(doc["fenceCount"].as_u64().unwrap() as usize, params.palw_fences_v1().len());
        // The full-activation height is not adopted on any shipped preset: it is reported as such, never as a number.
        assert!(doc["fullActivation"]["adoptedDaa"].is_null());
        let full = doc["lists"].as_array().unwrap().iter().find(|l| l["list"] == "fullActivation").unwrap();
        assert_eq!(full["status"], "not adopted");
        assert_eq!(full["entries"].as_u64().unwrap() as usize, crate::config::params::PALW_T12_FULL_ACTIVATION_FENCES_V1.len());
    }

    #[test]
    fn fence_schedule_marks_armed_exactly_as_fork_activation_reads_it() {
        let mut params = palw_t12_shipped_params();
        crate::config::params::palw_t12_arm_full_activation_at_v1(&mut params, Some(16_000));
        let below = palw_fence_schedule_json_v1(&params, 15_999);
        let at = palw_fence_schedule_json_v1(&params, 16_000);
        let row = |doc: &Value, name: &str| doc["fences"].as_array().unwrap().iter().find(|f| f["name"] == name).unwrap().clone();
        let gate_c = "palw_independently_work_certified_v1";
        assert_eq!(row(&below, gate_c)["activationDaa"], 16_000);
        assert_eq!(row(&below, gate_c)["armed"], false);
        assert_eq!(row(&at, gate_c)["armed"], true);
        assert!(row(&at, gate_c)["lists"].as_array().unwrap().iter().any(|l| l == "fullActivation"));
        let full = at["lists"].as_array().unwrap().iter().find(|l| l["list"] == "fullActivation").unwrap().clone();
        assert_eq!(full["armedEntries"], full["entries"]);
    }

    #[test]
    fn no_shipped_build_adopts_a_typed_missing_value() {
        // The day one is adopted this test is updated with the reviewed change that adopts it — never silently.
        // Lane P3 (2026-10-22, user delegation of value decisions; awaiting independent verification): V7 and V8 —
        // docs/evidence/palw/studio-values-2026-10-22/values-p3-slash-clock.md.
        // Lane UX-B (2026-10-27): V4 paid ingress, V5 material intake and V1 court service profile are pinned estimates
        // (`palw_g14_values_v1`); only the funded-original schedule stays typed-missing.
        const DECIDED_P3: &[&str] = &[
            "D2 disclosure fee schedule",
            "D2 readiness draw cap",
            "TIR position witness budget",
            "TIR lookahead frontier bound",
            "court paid-ingress schedule",
            "material intake schedule",
            "court service profile",
        ];
        for value in palw_typed_missing_values_v1() {
            assert_eq!(
                value.adopted,
                DECIDED_P3.contains(&value.name),
                "{} adoption differs from the reviewed list; update the tooling register and its guide",
                value.name
            );
        }
        assert_eq!(palw_typed_missing_json_v1()["values"][0]["status"], "not adopted");
    }

    #[test]
    fn phase_text_matches_the_rpc_spelling() {
        assert_eq!(palw_claim_phase_text_v1(&PalwClaimPhaseV2::Final { final_daa: 9 }), ("final", 9, None));
        assert_eq!(palw_claim_phase_text_v1(&PalwClaimPhaseV2::Provisional).0, "provisional");
    }
}
