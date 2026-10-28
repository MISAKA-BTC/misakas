//! **G14 r6: the receipt pause rows in the fold** (user decision 2026-10-22; [`crate::palw_receipt_pause_v2`]). A child module of
//! `palw_state_v2`: it reads the builder and writes only through the kernel route's journaled writer, so a reorg reverts the rows with
//! their block and the carriage / aux root carry them.
//!
//! **One writer, at one fixed place** — the end of the block, after every object, sweep and court close of the block — so the row
//! follows THE one predicate [`PalwChainStateV2::palw_accusation_pending_v1`] and no accusation path (DA session, court session,
//! legacy dispute reservation, court-service binding, owned TIR demand, `DefaultDisputed`) has to be hooked one by one:
//!
//! * a Gate C–governed, certified claim (not voided) that is accusation-pending and has no OPEN pause → the pause opens at this block's
//!   DAA (the block that started the accusation);
//! * an open pause whose claim is no longer pending → it closes at this block's DAA (a dismissal / acquittal: the span is credited, the
//!   remaining window resumes);
//! * a row whose claim is voided (a conviction — `revoked`, no window to resume) or gone (retired) → deleted.
//!
//! Dormant: a no-op unless Gate C is in force at this block (`gate_c_at`) and the kernel route exists (Gate C's certification rows live
//! in it, so without it no claim is certified and nothing is paused).
//!
//! Candidates are read from the indexes the pending predicate itself reads (bounded by their own caps), never from a walk of all claims.
use super::*;
use crate::palw_receipt_pause_v2::{
    PALW_RECEIPT_PAUSE_TABLE_V2, PalwReceiptPauseRowV2, palw_receipt_pause_applies_v2, palw_receipt_pause_claims_v2,
    palw_receipt_pause_key_v2, palw_receipt_pause_row_v2,
};

fn refused(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.to_string())
}

/// The claims that MAY be accusation-pending: the keys of every index `palw_accusation_pending_v1` reads (a superset; each is then
/// asked the predicate itself). `DefaultDisputed` claims are not `Final`, so never certified — not candidates.
fn pending_candidates_v2(state: &PalwChainStateV2) -> BTreeSet<Hash64> {
    let mut out = BTreeSet::new();
    out.extend(state.open_courts_by_claim.iter().filter(|(_, open)| **open > 0).map(|(claim, _)| *claim));
    out.extend(state.da_claims.iter().filter(|(_, record)| record.open_sessions() > 0).map(|(claim, _)| *claim));
    out.extend(state.legacy_disputes.iter().filter(|(_, record)| record.holds()).map(|(claim, _)| *claim));
    if let Some(service) = state.court_service_state_v1() {
        out.extend(service.claims.keys().copied());
    }
    out.extend(state.staged_tir_by_claim.keys().copied());
    out
}

/// Is `claim_id` a claim whose receipt window the pause governs right now (governed, certified, not voided)?
fn pausable_v2(params: &PalwStateParamsV2, state: &PalwChainStateV2, claim_id: &Hash64, claim: &PalwClaimStateV2) -> bool {
    palw_receipt_pause_applies_v2(params, claim)
        && matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
        && crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
            claim_id,
            claim,
            &crate::palw_rfc0018_v1::PalwGateCViewV1::of(params, state),
        )
}

fn write_row(builder: &mut TransitionBuilder<'_>, claim: &Hash64, row: Option<&PalwReceiptPauseRowV2>) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row_encoded_v1(PALW_RECEIPT_PAUSE_TABLE_V2, &palw_receipt_pause_key_v2(claim), row)
}

/// **The end-of-block reconcile** (module doc).
pub(super) fn reconcile(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !crate::palw_rfc0018_v1::palw_rfc0018_mirror_v1(builder.params).is_some_and(|mirror| mirror.gate_c_at(ctx.daa_score)) {
        return Ok(());
    }
    if builder.state.kernel_route.is_none() {
        return Ok(());
    }
    let now = ctx.daa_score;

    // 1. Rows this table already holds: close the dismissed, drop the convicted / gone.
    for claim_id in palw_receipt_pause_claims_v2(&builder.state) {
        let row = palw_receipt_pause_row_v2(&builder.state, &claim_id).map_err(refused)?.expect("listed rows exist");
        let Some(claim) = builder.state.claims.get(&claim_id).cloned() else {
            write_row(builder, &claim_id, None)?;
            continue;
        };
        if matches!(claim.phase, PalwClaimPhaseV2::Voided { .. }) {
            write_row(builder, &claim_id, None)?;
            continue;
        }
        let mut account = row.account();
        if account.open_since_daa.is_some() && !builder.state.palw_accusation_pending_v1(&claim_id) {
            account.dismiss(now);
            write_row(builder, &claim_id, Some(&row.with_account(&account)))?;
        }
    }

    // 2. Newly pending, certified, governed claims: open their pause at this block.
    for claim_id in pending_candidates_v2(&builder.state) {
        if !builder.state.palw_accusation_pending_v1(&claim_id) {
            continue;
        }
        let Some(claim) = builder.state.claims.get(&claim_id).cloned() else { continue };
        if !pausable_v2(builder.params, &builder.state, &claim_id, &claim) {
            continue;
        }
        let row = palw_receipt_pause_row_v2(&builder.state, &claim_id)
            .map_err(refused)?
            .unwrap_or_else(|| PalwReceiptPauseRowV2::new(&claim_id));
        let mut account = row.account();
        if account.open_since_daa.is_some() {
            continue;
        }
        account.open(now);
        write_row(builder, &claim_id, Some(&row.with_account(&account)))?;
    }
    Ok(())
}
