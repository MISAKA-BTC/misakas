//! **G14 r6 D4 (user decision 2026-10-22): post-Final duty clocks for text29 and funded v11 / Final27** — written, not compiled. A
//! child module of `palw_state_v2`.
//!
//! While a text29 / funded v11 Final's fee is HELD (RFC08 08-B / ADR-0184 §1, aux 144, inside its liability horizon H), a public
//! post-Final accusation may be filed (kernel route funded-execution action [`POST_FINAL_ACCUSE_ACTION_V1`], any Active bond that is
//! not the producer). Its acceptance records one rooted duty clock per responsible actor (for these families: the held producer),
//! built with RFC-0028's [`CourtDutyClockV1`] structure and derivation: `first_eligible = accepting DAA + 1`,
//! `response_deadline = accepting DAA + turn_deadline_daa` (the adopted per-turn deadline). The actor answers with action
//! [`POST_FINAL_ANSWER_ACTION_V1`] (signed by the actor itself) inside `[first_eligible, deadline]`; the answer's digest and DAA are
//! recorded. Every block, [`observe_native_post_final_duties_v1`] records whether the block left the court service capacity for one
//! service packet (the pinned `PALW_COURT_SERVICE_PROFILE_V1`, after the block's actual unreserved court spend) inside an open
//! eligible window — the native supplied-opportunity fact.
//!
//! [`decide_native_post_final_duty_v1`] then decides silence natively (no injected fact): window open or answered → undecided;
//! silent with a supplied opportunity → `PartyDefault` (refund requester + slash actor; the slash amount stays the typed-missing
//! `PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1`); silent without supply → `NoSlashHold`. Recovery of a no-slash hold: the sweep
//! re-opens one more response turn for the same accusation (a fresh clock, turn + 1) while it still closes before H; past H the
//! unchanged release sweep pays without slash. The rows live in aux 144 under their own prefix (journaled with the kernel route,
//! so reorg restores them) and are deleted when the held fee leaves (release, refund, conviction).
//!
//! These clocks are NOT staged-court `CourtDutyClock` rows: they are never stored under `PalwStagedCourtKeyV1::CourtDutyClock`, do
//! not enter the TIR per-claim index and are never checked by `CourtDutyClockV1::check` (which is TIR-acquisition specific). Their
//! `acquisition` field names the claim (`Key::Claim`), `demand_id` the accusation and `allocation_id` / `source_id` the job and the
//! accusation; the profile id is the native namespace [`native_post_final_duty_profile_id_v1`].
//!
//! Dormant: the actions are refused unless `palw_native_liability_horizon_v1` and the TIR public conviction fence are both in
//! force; supply is observed only with a pinned service profile (typed-missing today); the default needs the slash amount.
use super::*;
use crate::palw_court_duty_clock_v1::CourtDutyClockV1;
use crate::palw_court_service_binding_v1::CourtServiceAttributionV1;
use crate::palw_native_liability_hold_v1::{
    PALW_NATIVE_FAMILY_FUNDED_V11, PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, PalwNativeHeldFeeRowV1,
};
use borsh::{BorshDeserialize, BorshSerialize};

fn refused(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.to_string())
}

/// Funded-execution (tag `TAG_FUNDED_EXECUTION_V11`) action: a post-Final accusation of a held text29 / funded v11 Final.
/// Body: `family u8 ‖ job [u8;64] ‖ accusation [u8;64]`. (Central collision check: 47 is unused on base 3c6e4c09d.)
pub const POST_FINAL_ACCUSE_ACTION_V1: u8 = 47;
/// Funded-execution action: the accused actor's answer. Body: `family u8 ‖ job [u8;64] ‖ len u32 LE ‖ answer bytes`.
pub const POST_FINAL_ANSWER_ACTION_V1: u8 = 48;
/// Aux 144 prefix of a job's post-Final duty record (`0x02 ‖ family ‖ job`); the held fee is `0x01`.
pub const PALW_NATIVE_POST_FINAL_DUTY_PREFIX_V1: u8 = 0x02;
pub const PALW_NATIVE_POST_FINAL_DUTY_VERSION_V1: u8 = 1;
/// Upper bound of one answer body (bytes); only its digest is retained.
pub const PALW_NATIVE_POST_FINAL_ANSWER_MAX_BYTES_V1: usize = 64 * 1024;
/// At most this many duty records are observed / recovered by one block (key order).
pub const PALW_NATIVE_POST_FINAL_DUTIES_PER_BLOCK_V1: usize = 64;

/// One actor's duty in a post-Final accusation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePostFinalDutyV1 {
    pub clock: CourtDutyClockV1,
    /// The first DAA inside the clock's eligible window at which the court service capacity left room for one packet.
    pub supplied_daa: Option<u64>,
    /// The DAA and digest of the actor's answer, if it answered in time.
    pub answered: Option<(u64, [u8; 64])>,
}

/// A job's post-Final accusation and its duties (aux 144, `0x02 ‖ family ‖ job`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativePostFinalDutyRowV1 {
    pub version: u8,
    pub family: u8,
    pub job: [u8; 64],
    pub claim: [u8; 64],
    pub accusation: [u8; 64],
    pub accuser: PalwBondKeyV2,
    pub accused_daa: u64,
    pub duties: Vec<NativePostFinalDutyV1>,
}

pub fn palw_native_post_final_duty_key_v1(family: u8, job: &[u8; 64]) -> Vec<u8> {
    let mut key = Vec::with_capacity(66);
    key.push(PALW_NATIVE_POST_FINAL_DUTY_PREFIX_V1);
    key.push(family);
    key.extend_from_slice(job);
    key
}

/// The native duty namespace (stands where a TIR clock names its service profile).
pub fn native_post_final_duty_profile_id_v1(family: u8) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/post-final-duty-profile/v1", &family))
}

fn family_supported_v1(family: u8) -> bool {
    matches!(family, PALW_NATIVE_FAMILY_TEXT_V29 | PALW_NATIVE_FAMILY_FUNDED_V11)
}

/// The job's post-Final duty record, if any.
pub(super) fn native_post_final_duty_row_v1(state: &PalwChainStateV2, family: u8, job: &[u8; 64]) -> Option<PalwNativePostFinalDutyRowV1> {
    state
        .kernel_route
        .as_ref()?
        .aux_row::<PalwNativePostFinalDutyRowV1>(PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, &palw_native_post_final_duty_key_v1(family, job))
        .filter(|r| r.version == PALW_NATIVE_POST_FINAL_DUTY_VERSION_V1 && r.family == family && r.job == *job)
}

fn write_v1(b: &mut TransitionBuilder<'_>, family: u8, job: &[u8; 64], row: Option<&PalwNativePostFinalDutyRowV1>) -> Result<(), PalwStateV2Error> {
    b.write_kernel_row_encoded_v1(PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, &palw_native_post_final_duty_key_v1(family, job), row)
}

/// The fences the accusation / answer actions need (the default additionally needs F02's profile and the slash amount).
fn duty_fences_active_v1(b: &TransitionBuilder<'_>, now: u64) -> bool {
    b.params.native_liability_horizon_active_at(now) && b.extras.tir_public_conviction_active
}

/// RFC-0028 derivation of one actor's clock for `turn` accepted at `ctx`.
fn native_clock_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    held: &PalwNativeHeldFeeRowV1,
    accusation: [u8; 64],
    actor: PalwBondKeyV2,
    turn: u64,
) -> Result<CourtDutyClockV1, PalwStateV2Error> {
    let now = ctx.daa_score;
    let turn_deadline = b.params.turn_deadline_daa();
    let deadline = now.checked_add(turn_deadline).ok_or_else(|| refused("post-Final duty deadline overflow"))?;
    if turn_deadline == 0 || deadline >= held.horizon_daa {
        return Err(refused("post-Final duty would not close before the liability horizon"));
    }
    let claim = Hash64::from_bytes(held.claim);
    let duty_id = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
        b"misaka-palw/native/post-final-duty/v1",
        &(held.family, held.job, accusation, actor, turn),
    ));
    Ok(CourtDutyClockV1 {
        version: 1,
        profile_id: native_post_final_duty_profile_id_v1(held.family),
        duty_id,
        claim,
        allocation_id: Hash64::from_bytes(held.job),
        source_id: Hash64::from_bytes(accusation),
        acquisition: crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::Claim(claim),
        answerer: actor,
        demand_id: Hash64::from_bytes(accusation),
        turn,
        predecessor_block: ctx.block,
        predecessor_daa: now,
        first_eligible_daa: now.checked_add(1).ok_or_else(|| refused("post-Final duty eligibility overflow"))?,
        response_deadline_daa: deadline,
        original_hard_daa: held.horizon_daa,
        material_until_daa: held.horizon_daa,
    })
}

/// The responsible actors of a held Final (text29 / funded v11: the held producer, the only bond the default may slash).
fn responsible_actors_v1(held: &PalwNativeHeldFeeRowV1) -> Vec<PalwBondKeyV2> {
    vec![held.producer]
}

/// **Accept a post-Final accusation** of the held `(family, job)` by `accuser`: one duty clock per responsible actor. Refused below
/// the fences, outside the hold, for the producer itself, and while an earlier accusation of the job is still recorded (one at a
/// time; it is cleared when the held fee leaves).
pub(super) fn accuse_post_final_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    accuser: &PalwBondKeyV2,
    family: u8,
    job: &[u8; 64],
    accusation: [u8; 64],
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    if !duty_fences_active_v1(b, now) {
        return Err(refused("post-Final accusation is dormant"));
    }
    if !family_supported_v1(family) {
        return Err(refused("post-Final accusation is defined for text29 and funded v11 only"));
    }
    let held = super::palw_native_liability_hold_fold_v1::native_accusable_after_final_v1(&b.state, family, job, now)
        .ok_or_else(|| refused("no held native fee inside its liability horizon"))?;
    if *accuser == held.producer || !b.state.bonds.get(accuser).is_some_and(|v| matches!(v.status, PalwBondStatusV2::Active)) {
        return Err(refused("post-Final accuser must be an Active bond other than the producer"));
    }
    if native_post_final_duty_row_v1(&b.state, family, job).is_some() {
        return Err(refused("the held Final already carries a post-Final accusation"));
    }
    let mut duties = Vec::new();
    for actor in responsible_actors_v1(&held) {
        duties.push(NativePostFinalDutyV1 { clock: native_clock_v1(b, ctx, &held, accusation, actor, 0)?, supplied_daa: None, answered: None });
    }
    let row = PalwNativePostFinalDutyRowV1 {
        version: PALW_NATIVE_POST_FINAL_DUTY_VERSION_V1,
        family,
        job: *job,
        claim: held.claim,
        accusation,
        accuser: *accuser,
        accused_daa: now,
        duties,
    };
    write_v1(b, family, job, Some(&row))
}

/// **Record the actor's answer.** Only the actor of an open clock (`first_eligible ≤ now ≤ deadline`) may answer, once per clock.
pub(super) fn answer_post_final_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    family: u8,
    job: &[u8; 64],
    answer: &[u8],
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    if !duty_fences_active_v1(b, now) {
        return Err(refused("post-Final answer is dormant"));
    }
    if answer.is_empty() || answer.len() > PALW_NATIVE_POST_FINAL_ANSWER_MAX_BYTES_V1 {
        return Err(refused("post-Final answer outside its bound"));
    }
    super::palw_native_liability_hold_fold_v1::native_accusable_after_final_v1(&b.state, family, job, now)
        .ok_or_else(|| refused("no held native fee inside its liability horizon"))?;
    let mut row = native_post_final_duty_row_v1(&b.state, family, job).ok_or_else(|| refused("no post-Final accusation to answer"))?;
    let digest = misaka_palw_kernel::hash::object_id(b"misaka-palw/native/post-final-answer/v1", &(family, *job, answer));
    let duty = row
        .duties
        .iter_mut()
        .find(|d| d.clock.answerer == *signer && d.answered.is_none() && now >= d.clock.first_eligible_daa && now <= d.clock.response_deadline_daa)
        .ok_or_else(|| refused("signer has no open post-Final duty"))?;
    duty.answered = Some((now, digest));
    write_v1(b, family, job, Some(&row))
}

/// Delete the job's duty record (the held fee left: released, refunded or convicted).
pub(super) fn clear_native_post_final_duties_v1(b: &mut TransitionBuilder<'_>, held: &PalwNativeHeldFeeRowV1) -> Result<(), PalwStateV2Error> {
    if native_post_final_duty_row_v1(&b.state, held.family, &held.job).is_none() {
        return Ok(());
    }
    write_v1(b, held.family, &held.job, None)
}

/// Does `profile`'s court service capacity leave room for one packet after `spent` (this block's actual unreserved court cost)?
pub(super) fn native_service_slot_supplied_v1(
    profile: &crate::palw_court_service_binding_v1::CourtServiceProfileV1,
    spent: &crate::palw_court_capacity_v1::PalwCourtCostV1,
) -> bool {
    let (budget, packet) = (&profile.budget, &profile.packet_cost);
    spent.bytes.checked_add(packet.bytes).is_some_and(|n| n <= budget.bytes)
        && spent.work.checked_add(packet.work).is_some_and(|n| n <= budget.work)
        && packet.io <= budget.io
        && packet.controls <= budget.controls
}

/// Production per-block observation (pinned profile, typed-missing today → no-op).
pub(super) fn observe_native_post_final_duties_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !super::palw_court_service_lifecycle_fold_v1::court_service_f02_active_v1(b) {
        return Ok(());
    }
    let Some(profile) = crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1 else { return Ok(()) };
    observe_native_post_final_duties_with_v1(b, ctx, &profile)
}

fn duty_rows_v1(b: &TransitionBuilder<'_>) -> Vec<PalwNativePostFinalDutyRowV1> {
    let Some(route) = b.state.kernel_route.as_ref() else { return Vec::new() };
    let table = PALW_NATIVE_LIABILITY_HOLD_TABLE_V1;
    let range = (table, vec![PALW_NATIVE_POST_FINAL_DUTY_PREFIX_V1])..(table, vec![PALW_NATIVE_POST_FINAL_DUTY_PREFIX_V1 + 1]);
    route
        .aux
        .range(range)
        .filter_map(|((_, key), bytes)| {
            let row = borsh::from_slice::<PalwNativePostFinalDutyRowV1>(bytes).ok()?;
            (row.version == PALW_NATIVE_POST_FINAL_DUTY_VERSION_V1 && *key == palw_native_post_final_duty_key_v1(row.family, &row.job))
                .then_some(row)
        })
        .collect()
}

/// [`observe_native_post_final_duties_v1`] under an explicit profile (fixtures; a future pinned profile). Each open, unanswered,
/// not-yet-supplied clock whose eligible window contains this DAA records it as its supplied opportunity when the block left room
/// for one packet. The first such block at the DAA sets it; later blocks change nothing.
pub(super) fn observe_native_post_final_duties_with_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    profile: &crate::palw_court_service_binding_v1::CourtServiceProfileV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    if !duty_fences_active_v1(b, now) || !native_service_slot_supplied_v1(profile, &b.court_cost_spent) {
        return Ok(());
    }
    let mut touched = 0usize;
    for mut row in duty_rows_v1(b) {
        if touched >= PALW_NATIVE_POST_FINAL_DUTIES_PER_BLOCK_V1 {
            break;
        }
        let mut changed = false;
        for d in row.duties.iter_mut() {
            if d.supplied_daa.is_none() && d.answered.is_none() && d.clock.stage_started_before_unit(now) {
                d.supplied_daa = Some(now);
                changed = true;
            }
        }
        if changed {
            touched += 1;
            let (family, job) = (row.family, row.job);
            write_v1(b, family, &job, Some(&row))?;
        }
    }
    Ok(())
}

/// The production silence fact for one recorded native clock: `Ok(answered)`; `Err` when the clock is not a recorded native duty
/// of the row's job (undecided, never inferred).
pub(super) fn native_duty_answered_v1(
    b: &TransitionBuilder<'_>,
    held: &PalwNativeHeldFeeRowV1,
    clock: &CourtDutyClockV1,
) -> Result<bool, &'static str> {
    let row = native_post_final_duty_row_v1(&b.state, held.family, &held.job).ok_or("no post-Final accusation is recorded")?;
    row.duties.iter().find(|d| d.clock == *clock).map(|d| d.answered.is_some()).ok_or("clock is not a recorded post-Final duty")
}

/// **Decide natively (pure read).** `Err` = undecided. Silent + supplied → `PartyDefault`; silent + not supplied → `NoSlashHold`.
/// `answered` is the silence fact (production: [`native_duty_answered_v1`], from the recorded answers).
pub(super) fn decide_native_post_final_duty_v1(
    b: &TransitionBuilder<'_>,
    held: &PalwNativeHeldFeeRowV1,
    now: u64,
    answered: &dyn Fn(&TransitionBuilder<'_>, &PalwNativeHeldFeeRowV1, &CourtDutyClockV1) -> Result<bool, &'static str>,
) -> Result<CourtServiceAttributionV1, &'static str> {
    if now > held.horizon_daa {
        return Err("held fee is past its liability horizon; released without slash");
    }
    let row = native_post_final_duty_row_v1(&b.state, held.family, &held.job).ok_or("no post-Final accusation is recorded")?;
    if row.claim != held.claim {
        return Err("post-Final accusation names another claim");
    }
    let mine: Vec<&NativePostFinalDutyV1> = row.duties.iter().filter(|d| d.clock.answerer == held.producer).collect();
    if mine.is_empty() {
        return Err("the held claim's actor has no post-Final duty clock");
    }
    // Only the latest turn decides (an earlier no-supply turn was recovered by a later one).
    let turn = mine.iter().map(|d| d.clock.turn).max().unwrap_or(0);
    let mut supplied = true;
    for d in mine.iter().filter(|d| d.clock.turn == turn) {
        if d.clock.response_deadline_daa >= now {
            return Err("the actor's response window is still open");
        }
        if answered(b, held, &d.clock)? {
            return Err("the actor answered its duty; no default");
        }
        if !d.supplied_daa.is_some_and(|s| d.clock.stage_started_before_unit(s)) {
            supplied = false;
        }
    }
    if !supplied {
        return Ok(CourtServiceAttributionV1::NoSlashHold(
            "no court service opportunity was supplied inside the actor's post-Final window; no party default, hold and recover",
        ));
    }
    Ok(CourtServiceAttributionV1::PartyDefault)
}

/// **Recovery of a no-slash hold:** a silent, unsupplied latest turn whose window has closed is followed by one more turn (fresh
/// clocks at this block) while it still closes before H. Returns whether a turn was re-opened.
pub(super) fn reopen_unsupplied_turn_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    held: &PalwNativeHeldFeeRowV1,
) -> Result<bool, PalwStateV2Error> {
    let now = ctx.daa_score;
    if !matches!(
        decide_native_post_final_duty_v1(b, held, now, &native_duty_answered_v1),
        Ok(CourtServiceAttributionV1::NoSlashHold(_))
    ) {
        return Ok(false);
    }
    let Some(mut row) = native_post_final_duty_row_v1(&b.state, held.family, &held.job) else { return Ok(false) };
    let turn = row.duties.iter().map(|d| d.clock.turn).max().unwrap_or(0).checked_add(1).ok_or_else(|| refused("post-Final turn overflow"))?;
    let mut next = Vec::new();
    for actor in responsible_actors_v1(held) {
        match native_clock_v1(b, ctx, held, row.accusation, actor, turn) {
            Ok(clock) => next.push(NativePostFinalDutyV1 { clock, supplied_daa: None, answered: None }),
            // No room before H: stay held; the release sweep pays at H without slash.
            Err(_) => return Ok(false),
        }
    }
    row.duties.extend(next);
    write_v1(b, held.family, &held.job, Some(&row))?;
    Ok(true)
}

/// Every held row with a recorded native accusation (the recovery sweep's candidates), bounded, key order.
pub(super) fn held_rows_with_native_duties_v1(b: &TransitionBuilder<'_>, now: u64) -> Vec<PalwNativeHeldFeeRowV1> {
    duty_rows_v1(b)
        .into_iter()
        .filter_map(|r| super::palw_native_liability_hold_fold_v1::native_accusable_after_final_v1(&b.state, r.family, &r.job, now))
        .take(PALW_NATIVE_POST_FINAL_DUTIES_PER_BLOCK_V1)
        .collect()
}

/// Decode a post-Final accusation body: `family ‖ job ‖ accusation`.
pub(super) fn apply_accuse_action_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() != 1 + 64 + 64 {
        return Err(refused("post-Final accusation framing"));
    }
    let family = body[0];
    let job: [u8; 64] = body[1..65].try_into().unwrap();
    let accusation: [u8; 64] = body[65..129].try_into().unwrap();
    accuse_post_final_v1(b, ctx, signer, family, &job, accusation)
}

/// Decode a post-Final answer body: `family ‖ job ‖ len u32 LE ‖ answer`.
pub(super) fn apply_answer_action_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    body: &[u8],
) -> Result<(), PalwStateV2Error> {
    if body.len() < 1 + 64 + 4 {
        return Err(refused("post-Final answer framing"));
    }
    let family = body[0];
    let job: [u8; 64] = body[1..65].try_into().unwrap();
    let n = u32::from_le_bytes(body[65..69].try_into().unwrap()) as usize;
    if n != body.len() - 69 {
        return Err(refused("post-Final answer exact count"));
    }
    answer_post_final_v1(b, ctx, signer, family, &job, &body[69..])
}
