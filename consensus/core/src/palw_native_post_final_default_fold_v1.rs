//! **G14 lane r5-f02-wire (2026-10-22): RFC-0026 F02 default during the RFC08 / ADR-0184 §1 post-Final liability hold** for the
//! text29 and funded v11 families — written, not compiled. A child module of `palw_state_v2`.
//!
//! While a family's Final fee is HELD (aux 144, inside its liability horizon H), the producer stays liable. If its post-Final
//! service duty was opportunity-SUPPLIED (RFC-0026 / ADR-0192 A: every reserved packet `OpportunityProvided` on the rooted book and a
//! supplied unit inside the actor's eligible window) and the actor stayed SILENT past its response deadline, the claim defaults:
//! the held fee is refunded to the requester ([`super::palw_native_liability_hold_fold_v1::refund_native_fee_v1`]) and the actor is
//! slashed. If supply is MISSING it is a **no-slash hold**: nothing moves, the held fee (and with it the producer's budget
//! reservation, the evidence retained to H and the quota subject) stays held; the recovery sweep
//! ([`recover_native_post_final_defaults_v1`], every closing tick) re-evaluates once the canonical recovery sweep has moved the
//! slot and supply is recorded, and resumes the default; if H passes first the existing release sweep releases the fee WITHOUT a
//! slash (`release_held_native_fees_v1`, unchanged).
//!
//! Dormant two ways: `palw_native_liability_horizon_v1` (the hold) and `court_service_f02_active_v1` (the conviction fence and a
//! pinned service profile — typed-missing today). The slash rule [`PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1`] is decided (lane P3,
//! 2026-10-22, awaiting independent verification): the claim's collectible reserve, as a conviction of the same Final collects. "Silent" is a family fact: [`native_post_final_duty_answered_v1`] answers it from the native
//! post-Final duty record (G14 r6 D4, `palw_native_post_final_duty_v1`: accusation → clocks → answers → supplied opportunity); a
//! job with a recorded accusation is decided from that record alone, one without falls back to the TIR service binding below.
use super::*;
use crate::palw_court_duty_clock_v1::CourtDutyClockV1;
use crate::palw_native_liability_hold_v1::{
    PALW_NATIVE_FAMILY_FUNDED_V11, PALW_NATIVE_FAMILY_TEXT_V29, PALW_NATIVE_HELD_FEE_PREFIX_V1,
    PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1, PALW_NATIVE_LIABILITY_HOLD_TABLE_V1, PalwNativeHeldFeeRowV1,
    palw_native_held_fee_key_v1,
};

fn refused(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.to_string())
}

/// **How the slash of a post-Final default is sized.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwNativePostFinalDefaultSlashV1 {
    /// The claim's own reserve — exactly what a conviction of the same held Final collects (funded v11 `claim.reserved_original`,
    /// text29 `maximum.producer_reserve`), bounded by what the producer's bond still holds (G-14: never refused for less).
    ClaimReserve,
}

/// **The slash charged to a silent, opportunity-supplied actor on a post-Final default: the claim's reserve.** **Decided 2026-10-22
/// (lane P3, user delegation of value decisions), awaiting independent verification** —
/// `docs/evidence/palw/studio-values-2026-10-22/values-p3-slash-clock.md` V6. Consistent with the families' existing default rules
/// (the pre-Final DA default collects the same collectible reserve a conviction does, `palw_flat_execution_da_fold_v11`): a fixed
/// amount below the reserve would make silence cheaper than an answer that convicts, and one above it would reach collateral the
/// budget reserved for other claims. `None` would refuse a decided `PartyDefault` whole (no refund without the slash).
pub const PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1: Option<PalwNativePostFinalDefaultSlashV1> =
    Some(PalwNativePostFinalDefaultSlashV1::ClaimReserve);

/// **The production slash amount of a post-Final default of `row`** under `rule`: the family's collectible reserve.
pub(super) fn native_post_final_default_slash_v1(
    b: &TransitionBuilder<'_>,
    rule: PalwNativePostFinalDefaultSlashV1,
    row: &PalwNativeHeldFeeRowV1,
) -> Result<u64, PalwStateV2Error> {
    match rule {
        PalwNativePostFinalDefaultSlashV1::ClaimReserve => {
            let reserve = super::palw_flat_execution_fold_v11::native_held_final_reserve_v1(b, row.family, &row.job)?;
            Ok(crate::palw_g14_lane_closure_v1::palw_native_producer_collectible_v1(
                b.state.bond(&row.producer).map(|bond| bond.collateral),
                reserve,
            ))
        }
    }
}
/// At most this many post-Final defaults are applied by one closing tick (key order).
pub const PALW_NATIVE_POST_FINAL_DEFAULTS_PER_BLOCK_V1: usize = 64;

/// The outcome of one post-Final F02 evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NativePostFinalDefaultV1 {
    /// Supplied and silent: the fee was refunded to the requester and the actor slashed by this amount (actually collected).
    Defaulted { refunded: u64, slashed: u64 },
    /// Supply missing (or the actor had no supplied unit in its window): nothing moved; held until recovery or H.
    NoSlashHold(&'static str),
}

/// Families whose held Final may default here.
fn family_supported_v1(family: u8) -> bool {
    matches!(family, PALW_NATIVE_FAMILY_TEXT_V29 | PALW_NATIVE_FAMILY_FUNDED_V11)
}

/// **Was the post-Final duty behind `clock` answered?** A family fact, recorded natively since G14 r6 D4: a post-Final accusation
/// (`palw_native_post_final_duty_v1`) records the actor's clocks and answers. A clock that is not such a recorded native duty is
/// undecided (never a default by inference).
pub(super) fn native_post_final_duty_answered_v1(
    b: &TransitionBuilder<'_>,
    row: &PalwNativeHeldFeeRowV1,
    clock: &CourtDutyClockV1,
) -> Result<bool, &'static str> {
    match row.family {
        PALW_NATIVE_FAMILY_TEXT_V29 | PALW_NATIVE_FAMILY_FUNDED_V11 => {
            super::palw_native_post_final_duty_v1::native_duty_answered_v1(b, row, clock)
        }
        _ => Err("family has no post-Final service duty"),
    }
}

/// **Decide (pure read) whether the held Final of `row` defaults at `now`.** `Err` = undecided (pending, unknown, not silent yet,
/// answer unknown, premise off): nothing may move. `answered` is the family's silence fact (production:
/// [`native_post_final_duty_answered_v1`]).
pub(super) fn decide_native_post_final_default_v1(
    b: &TransitionBuilder<'_>,
    row: &PalwNativeHeldFeeRowV1,
    now: u64,
    answered: &dyn Fn(&TransitionBuilder<'_>, &PalwNativeHeldFeeRowV1, &CourtDutyClockV1) -> Result<bool, &'static str>,
) -> Result<crate::palw_court_service_binding_v1::CourtServiceAttributionV1, &'static str> {
    if !family_supported_v1(row.family) {
        return Err("post-Final default is defined for text29 and funded v11 only");
    }
    if now > row.horizon_daa {
        return Err("held fee is past its liability horizon; released without slash");
    }
    // G14 r6 D4: a recorded native post-Final accusation decides from its own clocks, answers and supplied opportunities.
    if super::palw_native_post_final_duty_v1::native_post_final_duty_row_v1(&b.state, row.family, &row.job).is_some() {
        return super::palw_native_post_final_duty_v1::decide_native_post_final_duty_v1(b, row, now, answered);
    }
    let claim = Hash64::from_bytes(row.claim);
    let service = b.state.court_service_state_v1().ok_or("court service book is absent")?;
    let clocks: Vec<&CourtDutyClockV1> =
        b.state.court_duty_clocks_for_claim_v1(claim).into_iter().filter(|c| c.answerer == row.producer).collect();
    if clocks.is_empty() {
        return Err("the held claim's actor has no native post-Final duty clock");
    }
    for clock in &clocks {
        if clock.response_deadline_daa >= now {
            return Err("the actor's response window is still open");
        }
        if answered(b, row, clock)? {
            return Err("the actor answered its duty; no default");
        }
    }
    let unpaid: BTreeSet<PalwBondKeyV2> = std::iter::once(row.producer).collect();
    crate::palw_court_service_binding_v1::attribute_service_default_v1(
        service,
        claim,
        &clocks,
        &unpaid,
        b.extras.court_service_inclusion_premise_v1.as_ref(),
    )
}

/// **Apply a decided post-Final default of `(family, job)`** under an explicit slash amount (fixtures; a future pinned amount).
/// Atomic: refund + slash or nothing. A `NoSlashHold` decision moves nothing and is reported, not refused, to the sweep; the object
/// path ([`apply_native_post_final_default_v1`]) refuses it with its typed reason.
pub(super) fn apply_native_post_final_default_with_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    family: u8,
    job: &[u8; 64],
    slash: u64,
    answered: &dyn Fn(&TransitionBuilder<'_>, &PalwNativeHeldFeeRowV1, &CourtDutyClockV1) -> Result<bool, &'static str>,
) -> Result<NativePostFinalDefaultV1, PalwStateV2Error> {
    let now = ctx.daa_score;
    // The pinned-profile half of F02 is the production entries' (`court_service_f02_active_v1`); fixtures pass their own profile.
    if !b.params.native_liability_horizon_active_at(now) || !b.extras.tir_public_conviction_active {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    let row = super::palw_native_liability_hold_fold_v1::native_accusable_after_final_v1(&b.state, family, job, now)
        .ok_or_else(|| refused("no held native fee inside its liability horizon"))?;
    match decide_native_post_final_default_v1(b, &row, now, answered).map_err(refused)? {
        crate::palw_court_service_binding_v1::CourtServiceAttributionV1::NoSlashHold(why) => {
            Ok(NativePostFinalDefaultV1::NoSlashHold(why))
        }
        crate::palw_court_service_binding_v1::CourtServiceAttributionV1::PartyDefault => {
            // The actor's exit must still be held by the very fee being refunded (its budget reservation): no fictitious collection.
            if b.state.bond(&row.producer).is_none() {
                return Err(PalwStateV2Error::MissingBond(row.producer));
            }
            let saved = b.try_checkpoint()?;
            let result = (|| {
                let refunded = super::palw_native_liability_hold_fold_v1::refund_native_fee_v1(b, family, job, now)?;
                if refunded != row {
                    return Err(refused("held native fee changed under its own default"));
                }
                let slashed = b.slash_bond(row.producer, u128::from(slash))?;
                Ok(NativePostFinalDefaultV1::Defaulted { refunded: refunded.amount, slashed })
            })();
            if result.is_err() {
                b.restore(saved);
            }
            result
        }
    }
}

/// **The wire entry (tag 178, `PalwSharedCourtOpV1::NativeHeldDefault`).** Production silence fact and the pinned slash amount
/// (typed-missing → refused). A `NoSlashHold` is refused with its reason: the object changes nothing; recovery is the sweep's.
pub(super) fn apply_native_post_final_default_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    family: u8,
    job: &[u8; 64],
) -> Result<(), PalwStateV2Error> {
    if !b.params.native_liability_horizon_active_at(ctx.daa_score)
        || !super::palw_court_service_lifecycle_fold_v1::court_service_f02_active_v1(b)
    {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    let rule = PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1
        .ok_or_else(|| refused("post-Final default slash amount is not adopted (user decision); no default"))?;
    let row = super::palw_native_liability_hold_fold_v1::native_accusable_after_final_v1(&b.state, family, job, ctx.daa_score)
        .ok_or_else(|| refused("no held native fee inside its liability horizon"))?;
    let slash = native_post_final_default_slash_v1(b, rule, &row)?;
    match apply_native_post_final_default_with_v1(b, ctx, family, job, slash, &native_post_final_duty_answered_v1)? {
        NativePostFinalDefaultV1::Defaulted { .. } => Ok(()),
        NativePostFinalDefaultV1::NoSlashHold(why) => Err(refused(why)),
    }
}

/// **The recovery sweep** (closing tick, before the release sweep). Held text29 / funded v11 fees inside H whose claim has a service
/// binding are re-evaluated; a claim that was held for missing supply and whose slot the canonical recovery sweep has since supplied
/// now decides `PartyDefault` and is defaulted here (at most [`PALW_NATIVE_POST_FINAL_DEFAULTS_PER_BLOCK_V1`] a block, key order).
/// Undecided and no-slash rows are left untouched (they stay held; at H the release sweep pays them without a slash). A no-op
/// unless the hold, F02 and the slash amount are all in force.
pub(super) fn recover_native_post_final_defaults_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if !super::palw_court_service_lifecycle_fold_v1::court_service_f02_active_v1(b) {
        return Ok(());
    }
    let Some(rule) = PALW_NATIVE_POST_FINAL_DEFAULT_SLASH_V1 else { return Ok(()) };
    recover_native_post_final_defaults_by_v1(
        b,
        ctx,
        &|b: &TransitionBuilder<'_>, row: &PalwNativeHeldFeeRowV1| native_post_final_default_slash_v1(b, rule, row),
        &native_post_final_duty_answered_v1,
    )
}

/// [`recover_native_post_final_defaults_v1`] under an explicit slash amount and silence fact (fixtures).
pub(super) fn recover_native_post_final_defaults_with_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    slash: u64,
    answered: &dyn Fn(&TransitionBuilder<'_>, &PalwNativeHeldFeeRowV1, &CourtDutyClockV1) -> Result<bool, &'static str>,
) -> Result<(), PalwStateV2Error> {
    recover_native_post_final_defaults_by_v1(b, ctx, &|_: &TransitionBuilder<'_>, _: &PalwNativeHeldFeeRowV1| Ok(slash), answered)
}

/// The slash of one held row (production: [`native_post_final_default_slash_v1`]; fixtures: a constant).
type SlashOfV1<'f> = dyn Fn(&TransitionBuilder<'_>, &PalwNativeHeldFeeRowV1) -> Result<u64, PalwStateV2Error> + 'f;

/// [`recover_native_post_final_defaults_v1`] with the slash read per held row. A row whose slash cannot be read stays held (fail
/// closed: no refund without its slash; released at H as every undecided row is).
fn recover_native_post_final_defaults_by_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    slash_of: &SlashOfV1<'_>,
    answered: &dyn Fn(&TransitionBuilder<'_>, &PalwNativeHeldFeeRowV1, &CourtDutyClockV1) -> Result<bool, &'static str>,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    if !b.params.native_liability_horizon_active_at(now) || !b.extras.tir_public_conviction_active {
        return Ok(());
    }
    // G14 r6 D4: held rows with a recorded native accusation. A decided default is applied below; a no-slash hold re-opens one more
    // response turn (recovery) while it still closes before H.
    let mut due: Vec<PalwNativeHeldFeeRowV1> = Vec::new();
    let mut reopen: Vec<PalwNativeHeldFeeRowV1> = Vec::new();
    for row in super::palw_native_post_final_duty_v1::held_rows_with_native_duties_v1(b, now) {
        if !family_supported_v1(row.family) {
            continue;
        }
        match decide_native_post_final_default_v1(b, &row, now, answered) {
            Ok(crate::palw_court_service_binding_v1::CourtServiceAttributionV1::PartyDefault) => due.push(row),
            Ok(crate::palw_court_service_binding_v1::CourtServiceAttributionV1::NoSlashHold(_)) => reopen.push(row),
            Err(_) => {}
        }
    }
    for row in reopen {
        super::palw_native_post_final_duty_v1::reopen_unsupplied_turn_v1(b, ctx, &row)?;
    }
    for row in due {
        let Ok(slash) = slash_of(b, &row) else { continue };
        let _ = apply_native_post_final_default_with_v1(b, ctx, row.family, &row.job, slash, answered);
    }
    let Some(service) = b.state.court_service_state_v1() else { return Ok(()) };
    if service.claims.is_empty() {
        return Ok(());
    }
    let Some(route) = b.state.kernel_route.as_ref() else { return Ok(()) };
    let table = PALW_NATIVE_LIABILITY_HOLD_TABLE_V1;
    let range = (table, vec![PALW_NATIVE_HELD_FEE_PREFIX_V1])..(table, vec![PALW_NATIVE_HELD_FEE_PREFIX_V1 + 1]);
    let mut due: Vec<PalwNativeHeldFeeRowV1> = Vec::new();
    for ((_, key), bytes) in route.aux.range(range) {
        if due.len() >= PALW_NATIVE_POST_FINAL_DEFAULTS_PER_BLOCK_V1 {
            break;
        }
        let Ok(row) = borsh::from_slice::<PalwNativeHeldFeeRowV1>(bytes) else { continue };
        if row.version != PALW_NATIVE_LIABILITY_HOLD_ROW_VERSION_V1
            || *key != palw_native_held_fee_key_v1(row.family, &row.job)
            || !family_supported_v1(row.family)
            || now > row.horizon_daa
            || !service.claims.contains_key(&Hash64::from_bytes(row.claim))
        {
            continue;
        }
        if matches!(
            decide_native_post_final_default_v1(b, &row, now, answered),
            Ok(crate::palw_court_service_binding_v1::CourtServiceAttributionV1::PartyDefault)
        ) {
            due.push(row);
        }
    }
    for row in due {
        // Each default is atomic on its own; a refusal (e.g. a refund queue collision) leaves that row held and the block standing.
        let Ok(slash) = slash_of(b, &row) else { continue };
        let _ = apply_native_post_final_default_with_v1(b, ctx, row.family, &row.job, slash, answered);
    }
    Ok(())
}
