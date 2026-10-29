//! Authenticated ordinary fee consent. Old claim cohorts and paid-V2 contracts are preserved.
use super::*;
use crate::palw_verifier_fee_consent_v1::*;
use misaka_palw_kernel::verifier_pay::{VERIFIER_PAY_DRAW_V1, VerifierPayRowV1};
fn refused(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.to_string())
}

pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &VerifierFeeConsentV1,
) -> Result<(), PalwStateV2Error> {
    let checkpoint = builder.try_checkpoint()?;
    let result = apply_inner(builder, ctx, signer, terms);
    if result.is_err() {
        builder.restore(checkpoint);
    }
    result
}

fn apply_inner(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &VerifierFeeConsentV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    if !builder.params.settlement_once_active_at(now) {
        return Err(refused("ordinary verifier fee consent is not active"));
    }
    terms.validate().map_err(refused)?;
    if crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer) != terms.verifier {
        return Err(refused("verifier fee consent signer mismatch"));
    }
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    reserve_fee_scratch(builder, builder.state.kernel_route.as_ref().map_or(0, |r| r.aux.len()))?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("missing consent route"))?;
    if terms.network != route.header.policy.network_domain || terms.ruleset != route.header.policy.ruleset_digest {
        return Err(refused("verifier fee consent network/ruleset mismatch"));
    }
    if route
        .paid_claim_authority_for_base_checked_v1(&terms.claim)
        .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?
        .is_some()
    {
        return Err(refused("ordinary consent cannot replace paid claim terms"));
    }
    let ledger =
        route.ledger_checked_v1().map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?;
    let claim = ledger.claims.get(&terms.claim).ok_or_else(|| refused("missing fee consent claim"))?;
    if claim.convicted || matches!(claim.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Convicted { .. }) {
        return Err(refused("cannot attest Clean after an objective conviction"));
    }
    if !builder.params.settlement_once_active_at(claim.committed_daa) {
        return Err(refused("ordinary fee consent cannot alter an old claim cohort"));
    }
    let hard =
        super::palw_settlement_liability_fold_v1::kernel_claim_signed_hard_deadline_v1(builder.params, claim.committed_daa, None)
            .map_err(refused)?;
    if terms.liability_until != hard || now > hard {
        return Err(refused("consent must bind the original finite X3 deadline"));
    }
    let Some(VerifierPayRowV1::Draw { poster, fee, slots, deadline_daa, .. }) =
        ledger.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, terms.claim))
    else {
        return Err(refused("fee consent needs an authenticated draw"));
    };
    if *poster != terms.poster
        || *fee != terms.fee
        || now > *deadline_daa
        || slots.get(terms.slot as usize) != Some(&(terms.verifier, false))
    {
        return Err(refused("fee consent draw slot, payer, amount or deadline mismatch"));
    }
    let material = builder
        .state
        .monetary_material_row_v1(Hash64::from_bytes(terms.claim))
        .ok_or_else(|| refused("fee consent lacks retained material"))?;
    if !material.terms.payment_relative_until_finality || !material.healthy(&builder.state) {
        return Err(refused("fee consent material is not healthy"));
    }
    let key = fee_key(CONSENT, &terms.claim, &terms.verifier, terms.slot);
    if route.aux.contains_key(&(TABLE, key.clone())) {
        return Err(refused("fee slot already has immutable consent"));
    }
    let poster_bond = route.bond_key_of(&terms.poster).ok_or_else(|| refused("fee poster native bond absent"))?;
    if builder.state.bonds.get(&poster_bond).is_none_or(|b| b.collateral < terms.fee) {
        return Err(refused("consenting fee native escrow underfunded"));
    }
    drop(ledger);
    builder.admit_ordinary_verifier_fee_v1(terms, now)?;
    let mut ledger = super::palw_kernel_route_fold_v1::load_ledger(builder, ctx)?;
    let before = super::palw_kernel_route_fold_v1::loaded_rows(builder, &ledger)?;
    ledger.fund_consented_slot_reserved_v1(&terms.claim, &terms.verifier, terms.slot, terms.fee).map_err(refused)?;
    if terms.fee > 0 {
        require_drawn_income(builder, terms.claim, terms.verifier, terms.slot, terms.fee, now)?;
    }
    let debit = builder.slash_bond(poster_bond, u128::from(terms.fee))?;
    if debit != terms.fee {
        return Err(refused("consenting fee full debit invariant"));
    }
    builder.hold_verifier_fee_material_v1(now, terms.claim, terms.verifier, terms.slot, terms.fee)?;
    builder.write_kernel_row_encoded_v1(TABLE, &key, Some(terms))?;
    write_hold(builder, &HeldVerifierFeeV1 { terms: *terms, amount: terms.fee, convicted_daa: None })?;
    super::palw_kernel_route_fold_v1::flush(builder, &ledger, &before)
}

/// Prepare a hold only for a signed new-cohort Clean slot. Generic pay-on-fate events
/// without consent require the existing objective timely-fate rule; they never fabricate Clean.
pub(super) enum FeeDisposition {
    Legacy,
    Fate(u64),
    Held(HeldVerifierFeeV1),
}
fn reserve_fee_scratch(builder: &mut TransitionBuilder<'_>, count: usize) -> Result<(), PalwStateV2Error> {
    if builder.native_writers_tracking_v1() {
        let mut q = super::native_writers::NativeWriteQuoteV1::new("ordinary fee rows and keys");
        q.exact::<HeldVerifierFeeV1>(u64::try_from(count.max(256)).map_err(|_| refused("fee row count overflow"))?)?;
        q.exact::<u8>(
            u64::try_from(count.checked_mul(137).and_then(|n| n.checked_add(548)).ok_or_else(|| refused("fee scratch overflow"))?)
                .map_err(|_| refused("fee scratch size overflow"))?,
        )?;
        builder.admit_native_write_v1(q)?;
    }
    Ok(())
}

pub(super) fn prepare_hold(
    builder: &mut TransitionBuilder<'_>,
    claim: [u8; 64],
    verifier: [u8; 64],
    ordinal: u64,
    poster: [u8; 64],
    fee: u64,
    now: u64,
    strict: bool,
    events: &[misaka_palw_kernel::ledger::LedgerEventV1],
) -> Result<FeeDisposition, PalwStateV2Error> {
    if !builder.params.settlement_once_active_at(now) {
        return Ok(FeeDisposition::Legacy);
    }
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    reserve_fee_scratch(builder, builder.state.kernel_route.as_ref().map_or(0, |r| r.aux.len()))?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("missing fee route"))?;
    let ledger =
        route.ledger_checked_v1().map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?;
    let kernel = ledger.claims.get(&claim).ok_or_else(|| refused("missing fee claim"))?;
    if !builder.params.settlement_once_active_at(kernel.committed_daa) {
        return Ok(FeeDisposition::Legacy);
    }
    let Some(VerifierPayRowV1::Draw { slots, poster: drawn_poster, fee: drawn_fee, deadline_daa, .. }) =
        ledger.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, claim))
    else {
        return Err(refused("fee draw missing"));
    };
    let (slot, _) = slots
        .iter()
        .enumerate()
        .filter(|(_, (bond, paid))| *bond == verifier && !*paid)
        .nth(ordinal as usize)
        .ok_or_else(|| refused("fee event has no unpaid drawn slot"))?;
    if *drawn_poster != poster || fee > *drawn_fee {
        return Err(refused("fee event differs from rooted draw"));
    }
    let key = fee_key(CONSENT, &claim, &verifier, slot as u64);
    let Some(bytes) = route.aux.get(&(TABLE, key)) else {
        use misaka_palw_kernel::{ledger::LedgerEventV1 as E, lifecycle::ClaimStateV1 as S};
        let rooted_fate = match &kernel.life.state {
            S::Convicted { daa } | S::Unavailable { daa, producer_defaulted: true } => *daa <= *deadline_daa,
            _ => false,
        };
        let batch_fate = now <= *deadline_daa
            && events.iter().any(|e| matches!(e,E::Convicted{claim:c,..}|E::ProducerDefault{claim:c,..} if *c==claim));
        let actual = route
            .bond_key_of(&poster)
            .and_then(|b| builder.state.bonds.get(&b))
            .map(|b| b.collateral.min(*drawn_fee))
            .ok_or_else(|| refused("fate payer missing"))?;
        if fee != actual {
            return Err(refused("fate fee does not equal actual collectible escrow"));
        }
        if strict || !(rooted_fate || batch_fate) {
            return Err(refused("ordinary Clean attestation requires signed fee consent; no objective timely fate"));
        }
        return Ok(FeeDisposition::Fate(slot as u64));
    };
    let terms: VerifierFeeConsentV1 = borsh::from_slice(bytes).map_err(|_| refused("malformed verifier fee consent"))?;
    terms.validate().map_err(refused)?;
    if terms.claim != claim || terms.verifier != verifier || terms.slot != slot as u64 || terms.poster != poster || terms.fee != fee {
        return Err(refused("fee event differs from signed consent"));
    }
    if route.aux.contains_key(&(TABLE, fee_key(HELD, &claim, &verifier, slot as u64))) {
        return Err(refused("fee slot already held"));
    }
    let obligation = crate::palw_settlement_once_v1::PalwSettlementObligationV1::with_party(
        crate::palw_settlement_once_v1::PalwSettlementLegV1::VerifierCheckFee,
        claim,
        verifier,
        slot as u64,
    );
    let mut conviction_key = Vec::with_capacity(65);
    conviction_key.push(6);
    conviction_key.extend_from_slice(&claim);
    let convicted_daa = route
        .aux
        .get(&(TABLE, conviction_key))
        .map(|b| borsh::from_slice::<u64>(b).map_err(|_| refused("malformed fee conviction")))
        .transpose()?
        .or_else(|| match kernel.life.state {
            misaka_palw_kernel::lifecycle::ClaimStateV1::Convicted { daa } => Some(daa),
            _ => None,
        });
    if builder.settled_row_checked_v1(&obligation)?.is_some() {
        return Err(refused("fee slot already settled"));
    }
    Ok(FeeDisposition::Held(HeldVerifierFeeV1 { terms, amount: fee, convicted_daa }))
}

pub(super) fn write_hold(builder: &mut TransitionBuilder<'_>, row: &HeldVerifierFeeV1) -> Result<(), PalwStateV2Error> {
    if row.amount != row.terms.fee {
        return Err(refused("held fee must exactly equal signed and debited escrow"));
    }
    builder.write_kernel_row_encoded_v1(TABLE, &fee_key(HELD, &row.terms.claim, &row.terms.verifier, row.terms.slot), Some(row))
}

pub(super) fn convictions(
    builder: &mut TransitionBuilder<'_>,
    events: &[misaka_palw_kernel::ledger::LedgerEventV1],
    now: u64,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.settlement_once_active_at(now) {
        return Ok(());
    }
    for event in events {
        let misaka_palw_kernel::ledger::LedgerEventV1::Convicted { claim, .. } = event else { continue };
        super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
        reserve_fee_scratch(builder, builder.state.kernel_route.as_ref().map_or(0, |r| r.aux.len()))?;
        let mut conviction_key = Vec::with_capacity(65);
        conviction_key.push(6);
        conviction_key.extend_from_slice(claim);
        let relevant = builder.state.kernel_route.as_ref().is_some_and(|r| {
            r.aux.range((TABLE, vec![CONSENT])..(TABLE, vec![CONSENT + 1])).any(|((_, k), _)| k.len() == 137 && k[1..65] == claim[..])
        });
        if relevant && !builder.state.kernel_route.as_ref().is_some_and(|r| r.aux.contains_key(&(TABLE, conviction_key.clone()))) {
            builder.write_kernel_row_encoded_v1(TABLE, &conviction_key, Some(&now))?;
        }
        let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
        let mut rows = Vec::with_capacity(route.aux.len());
        for ((_, key), bytes) in route.aux.range((TABLE, vec![HELD])..(TABLE, vec![HELD + 1])) {
            let mut row: HeldVerifierFeeV1 = borsh::from_slice(bytes).map_err(|_| refused("malformed ordinary held fee"))?;
            if *key != fee_key(HELD, &row.terms.claim, &row.terms.verifier, row.terms.slot) {
                return Err(refused("ordinary fee key mismatch"));
            }
            if row.terms.claim == *claim && now <= row.terms.liability_until && row.convicted_daa.is_none() {
                row.convicted_daa = Some(now);
                rows.push(row);
            }
        }
        for row in rows {
            write_hold(builder, &row)?;
        }
    }
    Ok(())
}

pub(super) fn release(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    if !builder.params.settlement_once_active_at(now) {
        return Ok(());
    }
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    reserve_fee_scratch(builder, builder.state.kernel_route.as_ref().map_or(0, |r| r.aux.len()))?;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let mut due = Vec::with_capacity(256);
    for ((_, key), bytes) in route.aux.range((TABLE, vec![HELD])..(TABLE, vec![HELD + 1])) {
        let row: HeldVerifierFeeV1 = borsh::from_slice(bytes).map_err(|_| refused("malformed ordinary held fee"))?;
        if *key != fee_key(HELD, &row.terms.claim, &row.terms.verifier, row.terms.slot) {
            return Err(refused("ordinary fee key mismatch"));
        }
        if now > row.terms.liability_until {
            due.push(row);
            if due.len() == 256 {
                break;
            }
        }
    }
    for row in due {
        row.terms.validate().map_err(refused)?;
        let payee = row.terms.payee_after_deadline(now, row.convicted_daa).ok_or_else(|| refused("fee not mature"))?;
        let bond = builder
            .state
            .kernel_route
            .as_ref()
            .and_then(|r| r.bond_key_of(&payee))
            .ok_or_else(|| refused("fee payee bond missing"))?;
        let payload = builder.state.bonds.get(&bond).ok_or_else(|| refused("fee payout missing"))?.payout_payload;
        match builder.ordinary_verifier_fee_terms_v1(row.terms.claim, row.terms.verifier, row.terms.slot, now, row.amount) {
            Ok(None) => {}
            Ok(Some(terms)) => {
                crate::palw_settlement_liability_v1::palw_settlement_liability_sourced_v1(&terms)
                    .map_err(super::palw_settlement_liability_fold_v1::liability_refused_v1)?;
            }
            Err(why) => return Err(super::palw_settlement_liability_fold_v1::liability_refused_v1(why)),
        }
        use crate::palw_settlement_once_v1::*;
        let once = builder.settle_once_v1(
            now,
            PalwSettlementObligationV1::with_party(
                PalwSettlementLegV1::VerifierCheckFee,
                row.terms.claim,
                row.terms.verifier,
                row.terms.slot,
            ),
            PalwSettlementConsumerV1::KernelTick,
            row.amount,
            PalwSettlementDebitV1::Escrow { poster: row.terms.poster, amount: row.amount },
        )?;
        if once.pays() && row.amount > 0 {
            let verifier_bond = builder
                .state
                .kernel_route
                .as_ref()
                .and_then(|r| r.bond_key_of(&row.terms.verifier))
                .ok_or_else(|| refused("fee recipient reservation bond missing"))?;
            let context = verifier_fee_income_context_v1(row.terms.claim, row.terms.verifier, row.terms.slot);
            let subject = Hash64::from_bytes(row.terms.claim);
            if builder.params.bond_budget_active_at(now) {
                let id = super::palw_bond_budget_fold_v1::source_income_id_v1(context, subject, &verifier_bond);
                let accepted = builder
                    .state
                    .bond_budget
                    .as_ref()
                    .and_then(|b| b.claim_row(&id))
                    .ok_or_else(|| refused("fee recipient R reservation missing"))?
                    .accepted_daa;
                if payee == row.terms.verifier {
                    builder.settle_source_income_v1(
                        ctx,
                        context,
                        subject,
                        verifier_bond,
                        accepted,
                        row.terms.liability_until,
                        row.amount,
                    )?;
                } else {
                    builder.cancel_source_income_v1(ctx, context, subject, verifier_bond, accepted)?;
                }
            }
            builder.add_kernel_payout(payload, row.amount)?;
            builder.finish_verifier_fee_material_v1(
                now,
                row.terms.claim,
                row.terms.verifier,
                row.terms.slot,
                row.amount,
                Some(crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload)),
            )?;
        } else if row.amount > 0 {
            builder.finish_verifier_fee_material_v1(now, row.terms.claim, row.terms.verifier, row.terms.slot, 0, None)?;
        }
        builder.write_kernel_row_encoded_v1::<HeldVerifierFeeV1>(
            TABLE,
            &fee_key(HELD, &row.terms.claim, &row.terms.verifier, row.terms.slot),
            None,
        )?;
        builder.write_kernel_row_encoded_v1::<VerifierFeeConsentV1>(
            TABLE,
            &fee_key(CONSENT, &row.terms.claim, &row.terms.verifier, row.terms.slot),
            None,
        )?;
        let pending = builder.state.kernel_route.as_ref().is_some_and(|r| {
            r.aux
                .range((TABLE, vec![CONSENT])..(TABLE, vec![HELD + 1]))
                .any(|((_, key), _)| key.len() == 137 && key[1..65] == row.terms.claim[..])
        });
        if !pending {
            let mut key = Vec::with_capacity(65);
            key.push(6);
            key.extend_from_slice(&row.terms.claim);
            builder.write_kernel_row_encoded_v1::<u64>(TABLE, &key, None)?;
        }
    }
    Ok(())
}

/// Every new-cohort passing receipt from a drawn checker must have immutable fee consent
/// before it creates signer liability. This makes absence of consent insufficient to fake Clean.
pub(super) fn clean_receipt_gate(
    builder: &mut TransitionBuilder<'_>,
    claim: &[u8; 64],
    seat: &PalwBondKeyV2,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.settlement_once_active_at(now) {
        return Ok(());
    }
    reserve_fee_scratch(builder, 1)?;
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Err(refused("Clean receipt route absent")) };
    if route
        .paid_claim_authority_for_base_checked_v1(claim)
        .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?
        .is_some()
    {
        return Ok(());
    }
    let ledger =
        route.ledger_checked_v1().map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?;
    let Some(kernel) = ledger.claims.get(claim) else { return Err(refused("Clean receipt claim absent")) };
    if !builder.params.settlement_once_active_at(kernel.committed_daa) {
        return Ok(());
    }
    let hard =
        super::palw_settlement_liability_fold_v1::kernel_claim_signed_hard_deadline_v1(builder.params, kernel.committed_daa, None)
            .map_err(refused)?;
    if now > hard {
        return Err(refused("new ordinary Clean receipt is past its original signed X3 horizon"));
    }
    if kernel.convicted
        || matches!(
            kernel.life.state,
            misaka_palw_kernel::lifecycle::ClaimStateV1::Convicted { .. }
                | misaka_palw_kernel::lifecycle::ClaimStateV1::Unavailable { producer_defaulted: true, .. }
        )
    {
        return Err(refused("objective fault cannot be converted into a Clean receipt"));
    }
    let Some(VerifierPayRowV1::Draw { slots, .. }) = ledger.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, *claim)) else {
        if ledger.verifier_pay.contains_key(&(misaka_palw_kernel::verifier_pay::VERIFIER_PAY_FEE_ESCROW_V1, kernel.job_id)) {
            return Err(refused("Clean receipt precedes its authenticated fee draw"));
        }
        let verifier = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(seat);
        if let Some(bytes) = route.aux.get(&(
            crate::palw_verifier_watch_v1::TABLE,
            crate::palw_verifier_watch_v1::key(crate::palw_verifier_watch_v1::CONTEXT, claim).to_vec(),
        )) {
            let context = crate::palw_claim_local_v1::decode_claim_local_value_v1::<
                crate::palw_verifier_watch_v1::PalwWatcherClaimContextV1,
            >(bytes)
            .map_err(|e| match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => refused(e),
            })?;
            if !context.complete {
                return Err(refused("Clean receipt precedes completed authenticated watcher draw"));
            }
            for (slot, actor) in context.selected.as_deref().unwrap_or(&[]).iter().enumerate() {
                if *actor == verifier && !route.aux.contains_key(&(TABLE, fee_key(CONSENT, claim, &verifier, slot as u64))) {
                    return Err(refused("retired draw does not replace actual signed Clean consent"));
                }
            }
        }
        return Ok(());
    };
    let verifier = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(seat);
    for (slot, (bond, _)) in slots.iter().enumerate() {
        if *bond == verifier && !route.aux.contains_key(&(TABLE, fee_key(CONSENT, claim, &verifier, slot as u64))) {
            // A fate-payment marker authenticates money movement, never a Clean
            // attestation. After consent retirement no new receipt may reuse it.
            return Err(refused("drawn Clean receipt needs its actual signed fee consent"));
        }
    }
    Ok(())
}

/// Bind only the immutable native paid-checker lock, never caller supplied seats.
/// The enclosing paid object checkpoint rolls back the lock if any recipient R cannot be reserved.
pub(super) fn bind_paid_draw(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    enrollment: &[u8; 64],
) -> Result<(), PalwStateV2Error> {
    if builder.extras.kernel_route.as_ref().and_then(|e| e.verifier_pay).is_none_or(|at| ctx.daa_score < at) {
        return Ok(());
    }
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    reserve_fee_scratch(builder, 1)?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("paid draw route missing"))?;
    let authority = route.paid_claim_enrollment_v3(enrollment).map_err(refused)?;
    let lock = route
        .paid_claim_effective_lock_checked_v6(&authority)
        .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?
        .ok_or_else(|| refused("paid verifier fee requires actual source lock"))?;
    if !lock.source_valid || lock.locked_daa != ctx.daa_score {
        return Err(refused("paid draw source/acceptance mismatch"));
    }
    let claim = authority.enrollment.base_claim;
    let slots: Vec<_> = lock.seats.iter().map(crate::palw_kernel_route_v1::palw_kernel_bond_id_v1).collect();
    let bonds = lock.seats.clone();
    let horizon = authority.enrollment.lifetime.liability_until;
    drop(lock);
    for bond in &bonds {
        builder.note_kernel_bond(bond)?;
    }
    let mut ledger = super::palw_kernel_route_fold_v1::load_ledger(builder, ctx)?;
    if !ledger.verifier_pay.contains_key(&(
        misaka_palw_kernel::verifier_pay::VERIFIER_PAY_FEE_ESCROW_V1,
        ledger.claims.get(&claim).ok_or_else(|| refused("paid draw claim missing"))?.job_id,
    )) {
        return Ok(());
    }
    let before = super::palw_kernel_route_fold_v1::loaded_rows(builder, &ledger)?;
    let events = ledger.assign_drawn_reserved_v1(&claim, &slots).map_err(PalwStateV2Error::NativeResource)?.map_err(refused)?;
    let Some(VerifierPayRowV1::Draw { fee, .. }) = ledger.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, claim)) else {
        return Err(refused("paid draw produced no fee slots"));
    };
    let fee = *fee;
    if fee > 0 {
        for (slot, bond) in bonds.iter().enumerate() {
            builder.reserve_source_income_v1(
                ctx,
                verifier_fee_income_context_v1(claim, slots[slot], slot as u64),
                Hash64::from_bytes(claim),
                *bond,
                horizon,
                fee,
            )?;
        }
    }
    super::palw_kernel_route_fold_v1::apply_settlements(builder, &events, true, ctx.daa_score)?;
    super::palw_kernel_route_fold_v1::flush(builder, &ledger, &before)
}

pub(super) fn actual_slot(
    builder: &mut TransitionBuilder<'_>,
    claim: [u8; 64],
    verifier: [u8; 64],
    occurrence: u64,
    now: u64,
) -> Result<Option<u64>, PalwStateV2Error> {
    if !builder.params.settlement_once_active_at(now) {
        return Ok(None);
    }
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("fee slot route missing"))?;
    let ledger =
        route.ledger_checked_v1().map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?;
    let Some(VerifierPayRowV1::Draw { slots, .. }) = ledger.verifier_pay.get(&(VERIFIER_PAY_DRAW_V1, claim)) else { return Ok(None) };
    let slot = slots
        .iter()
        .enumerate()
        .filter(|(_, (bond, paid))| *bond == verifier && !*paid)
        .nth(occurrence as usize)
        .map(|(slot, _)| slot as u64)
        .ok_or_else(|| refused("fee slot not present in authenticated draw"))?;
    Ok(Some(slot))
}

/// Consent and fate consume the original immutable draw's R; neither may obtain
/// a replacement reservation after observing the outcome.
fn require_drawn_income(
    builder: &TransitionBuilder<'_>,
    claim: [u8; 64],
    verifier: [u8; 64],
    slot: u64,
    amount: u64,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    if amount == 0 || builder.bond_budget_mirror_at(now).is_none() {
        return Ok(());
    }
    let bond = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|r| r.bond_key_of(&verifier))
        .ok_or_else(|| refused("drawn verifier native bond absent"))?;
    let income = super::palw_bond_budget_fold_v1::source_income_id_v1(
        verifier_fee_income_context_v1(claim, verifier, slot),
        Hash64::from_bytes(claim),
        &bond,
    );
    let row = builder
        .state
        .bond_budget
        .as_ref()
        .and_then(|b| b.claim_row(&income))
        .ok_or_else(|| refused("verifier fee has no original drawn-slot R reservation"))?;
    if row.origin != crate::palw_bond_budget_v1::PalwBudgetOriginV1::SourceFee
        || !row.live
        || !row.in_window
        || row.open
        || row.reserved.reward_sompi < amount
        || row.consumed.reward_sompi != 0
    {
        return Err(refused("verifier fee differs from its unconsumed drawn-slot R"));
    }
    Ok(())
}

pub(super) fn queue_fate_fee(
    builder: &mut TransitionBuilder<'_>,
    claim: [u8; 64],
    verifier: [u8; 64],
    slot: u64,
    amount: u64,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    require_drawn_income(builder, claim, verifier, slot, amount, now)?;
    let bond = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|r| r.bond_key_of(&verifier))
        .ok_or_else(|| refused("fate recipient native bond absent"))?;
    let payload = builder.state.bonds.get(&bond).ok_or_else(|| refused("fate recipient absent"))?.payout_payload;
    let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
    let income = super::palw_bond_budget_fold_v1::source_income_id_v1(
        verifier_fee_income_context_v1(claim, verifier, slot),
        Hash64::from_bytes(claim),
        &bond,
    );
    if let Some(mirror) = builder.bond_budget_mirror_at(now) {
        let queued = super::palw_bond_budget_fold_v1::native_queued_income_id_v1(queue, &bond);
        builder
            .bond_budget_op(super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |book, _, j| {
                if amount == 0 {
                    book.cancel_source_income_v1(&income, j)
                } else {
                    book.queue_terminal_verifier_income_v1(&mirror.policy, &income, now, amount, queued, j)
                }
            })?
            .ok_or_else(|| refused("fate R budget missing"))?
            .map_err(refused)?;
    }
    if amount > 0 {
        builder.hold_verifier_fee_material_v1(now, claim, verifier, slot, amount)?;
        builder.add_kernel_payout(payload, amount)?;
        builder.finish_verifier_fee_material_v1(now, claim, verifier, slot, amount, Some(queue))?;
    }
    Ok(())
}

/// Every real ledger flush compares the authoritative unpaid slots before and
/// after the transition. A zero-cash fate or ordinary deadline refund emits no
/// CheckFeePaid event, so cancellation cannot depend on the reward-event path.
pub(super) fn reconcile_closed_draws(
    builder: &mut TransitionBuilder<'_>,
    after: &misaka_palw_kernel::ledger::KernelLedgerV1,
) -> Result<(), PalwStateV2Error> {
    let now = after.daa;
    if builder.bond_budget_mirror_at(now).is_none() {
        return Ok(());
    }
    super::palw_kernel_route_fold_v1::reserve_route_rows_copy_v1(builder)?;
    let count = builder.state.kernel_route.as_ref().map_or(0, |r| r.rows.len());
    reserve_fee_scratch(builder, count)?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("draw reconciliation route absent"))?;
    let before =
        route.ledger_checked_v1().map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?;
    let mut cancelled = Vec::new();
    for ((kind, claim), row) in &before.verifier_pay {
        let VerifierPayRowV1::Draw { fee, slots, .. } = row else { continue };
        if *kind != VERIFIER_PAY_DRAW_V1 || *fee == 0 {
            continue;
        }
        let remaining = match after.verifier_pay.get(&(*kind, *claim)) {
            Some(VerifierPayRowV1::Draw { slots, .. }) => Some(slots),
            _ => None,
        };
        for (slot, (verifier, paid)) in slots.iter().enumerate() {
            if *paid || remaining.is_some_and(|s| s.get(slot) == Some(&(*verifier, false))) {
                continue;
            }
            let slot = slot as u64;
            let ordinary = fee_key(HELD, claim, verifier, slot);
            let paid_trap = crate::palw_paid_claim_trap_v1::palw_paid_trap_held_fee_key_v1(claim, verifier, slot);
            if route.aux.contains_key(&(TABLE, ordinary)) || route.aux.contains_key(&(TABLE, paid_trap)) {
                continue;
            }
            let bond = route.bond_key_of(verifier).ok_or_else(|| refused("closed draw recipient absent"))?;
            let income = super::palw_bond_budget_fold_v1::source_income_id_v1(
                verifier_fee_income_context_v1(*claim, *verifier, slot),
                Hash64::from_bytes(*claim),
                &bond,
            );
            if builder.state.bond_budget.as_ref().and_then(|b| b.claim_row(&income)).is_some_and(|r| {
                r.origin == crate::palw_bond_budget_v1::PalwBudgetOriginV1::SourceFee && r.live && r.consumed.reward_sompi == 0
            }) {
                cancelled.push(income);
            }
        }
    }
    drop(before);
    for income in cancelled {
        builder
            .bond_budget_op(super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |book, _, j| {
                book.cancel_source_income_v1(&income, j)
            })?
            .ok_or_else(|| refused("draw cancellation budget absent"))?
            .map_err(refused)?;
    }
    Ok(())
}
