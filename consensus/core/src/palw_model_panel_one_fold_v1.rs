//! Endogenous G14 assignments for every model claim. A carrier cannot select a
//! historical panel or a different seed for a post-migration claim.
use super::*;
use crate::palw_model_panel_one_v1::{derive_model_panel_one_v1, model_panel_one_seed_v1};
use crate::palw_panel_v2::{PalwPanelDrawPolicyV1, PalwPanelV2Error};
use std::ops::Bound::{Excluded, Unbounded};

pub(super) fn bind_due_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(from) = builder.params.model_panel_one_from_daa() else { return Ok(()) };
    if ctx.daa_score < from {
        return Ok(());
    }
    let lane = builder
        .extras
        .round_lane
        .as_ref()
        .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("G14 model assignment has no post-commit randomness lane".into()))?;
    let span_daa = lane.schedule_span_daa;
    let mut cursor = None;
    // No second collection of the pending claims. Every decision uses the
    // ordered state after the previous assignment reserved its real collateral.
    loop {
        let next = match cursor {
            None => builder.state.unresolved.iter().next().copied(),
            Some(last) => builder.state.unresolved.range((Excluded(last), Unbounded)).next().copied(),
        };
        let Some(key) = next else { break };
        cursor = Some(key);
        let id = key.1;
        let Some(claim) = builder.state.claims.get(&id) else { continue };
        if !builder.params.model_panel_one_governs_v1(claim.accepted_daa)
            || !matches!(claim.phase, PalwClaimPhaseV2::Provisional)
            || builder.state.rfc0016_claim_queued_v1(&id)
        {
            continue;
        }
        if builder.state.palw_v2_expiry_held_v1(builder.params, &id, claim)
            || ctx.daa_score > palw_provisional_bind_deadline_v1(&builder.state, builder.params, &id, claim)?
        {
            continue;
        }
        // An ORIGINAL custody contract freezes its first native panel. A
        // receipt timeout cannot retarget that paid contract to a fresh panel.
        // End this unavailable claim without treating silence as fraud; never
        // let a per-claim redraw invalidate every subsequent chain block.
        if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&builder.state, id)
            .is_some_and(|frame| frame.original_custody_v1().panel_v1().is_some())
        {
            let claim = claim.clone();
            builder.void_claim(id, &claim, ctx.daa_score, PalwVoidReasonV2::PanelUnavailable)?;
            continue;
        }
        let Some(seed) = model_panel_one_seed_v1(&builder.state, &id, claim.bind_base_daa(), span_daa) else { continue };
        let lock = palw_panel_valid_lock_v1(&builder.state, builder.params, builder.extras, &id, claim, ctx.daa_score, 1);
        let policy = PalwPanelDrawPolicyV1 {
            weighted: true,
            economy: None,
            readiness: None,
            independence: None,
            valid_lock: lock,
            stake: None,
        };
        let collateral_floor = crate::palw_panel_economy_v1::palw_panel_collateral_floor_at_v1(
            builder.params.min_collateral_sompi(),
            builder.params.legacy_public_filer_active_at(ctx.daa_score),
        );
        let result = derive_model_panel_one_v1(&builder.state, &id, seed, collateral_floor, Some(claim.bind_base_daa()), policy);
        let claim = claim.clone();
        let seats = match result {
            Ok(seats) => seats,
            Err(PalwPanelV2Error::InsufficientEligibleBonds { .. }) => {
                // An unavailable bonded population is a per-claim non-fraud
                // outcome. It never revives a model review or a five-seat draw.
                builder.void_claim(id, &claim, ctx.daa_score, PalwVoidReasonV2::NoCapablePanel)?;
                continue;
            }
            Err(why) => return Err(PalwStateV2Error::CarriageInconsistent(why.to_string())),
        };
        let admissible = builder
            .require_paid_consumer_bind_v1(id, &claim, ctx.daa_score)
            .and_then(|()| builder.require_panel_lock_eligible(id, &claim, &seats, ctx.daa_score));
        if let Err(why) = admissible {
            if matches!(why, PalwStateV2Error::NativeResource(_) | PalwStateV2Error::NativeContextUnavailable(_)) {
                return Err(why);
            }
            builder.void_claim(id, &claim, ctx.daa_score, PalwVoidReasonV2::NoCapablePanel)?;
            continue;
        }
        builder.write_panel(id, Some(PalwPanelStateV2 { anchor: seed, seats: seats.clone(), bound_daa: ctx.daa_score }))?;
        palw_tir_public_fold_v1::bind_panel_v1(builder, ctx, id)?;
        builder.reserve_seat_duties(id, &claim, &seats, ctx.daa_score)?;
        let mut bound = claim;
        bound.phase = PalwClaimPhaseV2::PanelBound { bound_daa: ctx.daa_score };
        let shape = crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(&bound);
        let window = builder.params.panel_bound_sweep_window_v1(
            &bound,
            ctx.daa_score,
            builder.params.receipt_window_for_claim_v1(&builder.state, &bound.class_id, shape, ctx.daa_score),
        );
        builder.write_claim(id, Some(bound))?;
        builder.note_breaker_v1(ctx.daa_score, crate::palw_capacity_s567_v1::PalwBreakerEventV1::Bound)?;
        builder.disarm_deadline(id);
        let deadline = ctx.daa_score.checked_add(window).ok_or(PalwStateV2Error::Overflow("G14 model receipt deadline"))?;
        builder.arm_deadline(deadline, id)?;
    }
    Ok(())
}
