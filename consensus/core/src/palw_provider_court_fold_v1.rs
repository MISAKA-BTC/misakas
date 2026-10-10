//! **The fold arms of the provider court (tags 150–153) and its closing tick** — child module of `palw_state_v2`, like the kernel route's
//! and the onboarding's folds. See [`crate::palw_provider_court_v1`] for the design and [`crate::palw_court_scope_v1`] for what a court
//! may demand. Since ADR-0177 the court's only subject is a kernel claim and its only unit a committed claim position; an object naming
//! the withdrawn `Artifact` subject is refused first, before any row, bond or class is read.
//! Rows live in the kernel route's aux tables 43–45, written through the route's one journaled writer; reservations are mirrored into
//! V2's committed-collateral ledger (`PalwChainStateV2::provider_court_reserved`).
//!
//! A structural refusal is an `Err`: the acceptance walk's rehearsal drops the object and the block stands. An answer's judgement spends
//! the route's per-block adjudication budget first (the budget the kernel's own objects spend). The closing tick never fails a block on a
//! court row: what it cannot do (a provider bond that is gone) it skips.

use super::palw_kernel_route_fold_v1::{apply_settlements, charge_route_budget_v1, ensure_route_header, flush, load_ledger};
use super::*;
use crate::palw_provider_court_v1::*;
use crate::palw_public_material_v1::{PublicUnitAnswerV1, PublicUnitV1};
use misaka_palw_kernel::ledger::{ClaimRowV1, LedgerEventV1};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

/// The court's fence at this block (its activation DAA), or the refusal that drops the object.
fn court_at(builder: &TransitionBuilder<'_>) -> Result<u64, PalwStateV2Error> {
    builder
        .extras
        .kernel_route
        .as_ref()
        .and_then(|k| k.provider_court)
        .ok_or_else(|| refused("palw_provider_court_v1 is not in force at this block"))
}

fn route_of<'a>(
    builder: &'a TransitionBuilder<'_>,
) -> Result<&'a crate::palw_kernel_route_v1::PalwKernelRouteStateV1, PalwStateV2Error> {
    builder.state.kernel_route.as_ref().ok_or_else(|| refused("the kernel route has no state"))
}

/// **ADR-0177 D1**: the withdrawn artifact subject is refused at every height past the fence (no row, no reservation, no charge).
fn claim_subject(subject: &ProviderSubjectV1) -> Result<Hash64, PalwStateV2Error> {
    match subject {
        ProviderSubjectV1::KernelClaim { claim } => Ok(*claim),
        ProviderSubjectV1::Artifact { .. } => Err(refused(
            "model availability is not a consensus matter (ADR-0177 D1): the provider court leases, challenges and charges claim material only",
        )),
    }
}

/// An Active bond's operator.
fn active_operator(builder: &TransitionBuilder<'_>, bond: &PalwBondKeyV2) -> Result<Hash64, PalwStateV2Error> {
    match builder.state.bonds.get(bond) {
        Some(b) if matches!(b.status, PalwBondStatusV2::Active) => Ok(b.operator_id),
        Some(_) => Err(refused("the signer is no Active bond")),
        None => Err(refused("the signer is no bond")),
    }
}

/// `bond`'s free collateral at `now`: what no V2 gate, kernel reservation, binding or lease already holds.
fn free_collateral(builder: &TransitionBuilder<'_>, bond: &PalwBondKeyV2, now: u64) -> u128 {
    let collateral = builder.state.bonds.get(bond).map(|b| b.collateral as u128).unwrap_or(0);
    collateral.saturating_sub(builder.committed_at(bond, now))
}

/// A kernel claim's stored row (decoded alone, no ledger rebuild).
fn claim_row(builder: &TransitionBuilder<'_>, claim: &Hash64) -> Result<ClaimRowV1, PalwStateV2Error> {
    route_of(builder)?.kernel_claim_row_v1(claim).ok_or_else(|| refused("no such kernel claim"))
}

fn write_subject(builder: &mut TransitionBuilder<'_>, subject: &ProviderSubjectV1, row: &ProviderSubjectRowV1) {
    builder.write_kernel_row(
        PALW_PROVIDER_COURT_TABLE_SUBJECTS_V1,
        subject.key(),
        Some(borsh::to_vec(row).expect("a subject row serializes")),
    );
}

fn write_lease(
    builder: &mut TransitionBuilder<'_>,
    subject: &ProviderSubjectV1,
    provider: &PalwBondKeyV2,
    row: Option<&ProviderLeaseRowV1>,
) {
    builder.write_kernel_row(
        PALW_PROVIDER_COURT_TABLE_LEASES_V1,
        palw_provider_lease_key_v1(subject, provider),
        row.map(|r| borsh::to_vec(r).expect("a lease row serializes")),
    );
}

fn write_challenge(
    builder: &mut TransitionBuilder<'_>,
    subject: &ProviderSubjectV1,
    provider: &PalwBondKeyV2,
    unit: &PublicUnitV1,
    row: Option<&ProviderChallengeRowV1>,
) {
    builder.write_kernel_row(
        PALW_PROVIDER_COURT_TABLE_CHALLENGES_V1,
        palw_provider_challenge_key_v1(subject, provider, unit),
        row.map(|r| borsh::to_vec(r).expect("a challenge row serializes")),
    );
}

/// **Tag 150: a lease.** See [`crate::palw_provider_court_v1`].
pub(super) fn apply_provider_lease_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    subject: &ProviderSubjectV1,
    reserved: u64,
    serve_until_daa: u64,
    provider: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    court_at(builder)?;
    let claim = claim_subject(subject)?;
    ensure_route_header(builder, ctx)?;
    let now = ctx.daa_score;
    let operator = active_operator(builder, provider)?;
    if reserved < PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1 {
        return Err(refused("a lease reserves at least the court's floor"));
    }
    // (Also what makes a replayed lease harmless: once its term is within one window of the chain, it can never land again.)
    if serve_until_daa < now.saturating_add(PALW_PROVIDER_RESPONSE_WINDOW_DAA_V1) {
        return Err(refused("a lease shorter than one challenge's response window promises nothing a challenge could test"));
    }
    let row = claim_row(builder, &claim)?;
    if !row.holds_job() {
        return Err(refused("the claim is already decided: its material obliges nobody any more"));
    }
    let producer =
        route_of(builder)?.bond_key_of(&row.producer).ok_or_else(|| refused("the claim's producer is a bond the route never saw"))?;
    if builder.state.bonds.get(&producer).map(|b| b.operator_id) == Some(operator) {
        return Err(refused("a claim's producer cannot be its own provider: its own lease moves no responsibility"));
    }
    let route = route_of(builder)?;
    if route.provider_lease_v1(subject, provider).is_some() {
        return Err(refused("one lease per (subject, provider)"));
    }
    if route.provider_subject_v1(subject).is_some_and(|row| row.charged.contains(provider)) {
        return Err(refused("a provider charged for this subject cannot lease it again"));
    }
    if free_collateral(builder, provider, now) < reserved as u128 {
        return Err(refused("the provider's free collateral does not cover the lease's reservation"));
    }
    write_lease(builder, subject, provider, Some(&ProviderLeaseRowV1 { reserved, filed_daa: now, serve_until_daa, charged: false }));
    Ok(())
}

/// The unit of a kernel-claim subject is a position the claim commits, on a claim not yet decided — and claim-specific, never model
/// bytes (the court scope's one predicate decides; every artifact unit is refused by it).
fn claim_unit_in_scope(builder: &TransitionBuilder<'_>, claim: &Hash64, unit: &PublicUnitV1) -> Result<(), PalwStateV2Error> {
    use crate::palw_court_scope_v1::{palw_court_demand_allowed_v1, palw_public_unit_kind_v1};
    palw_court_demand_allowed_v1(palw_public_unit_kind_v1(unit)).map_err(|e| refused(e.why()))?;
    let PublicUnitV1::ClaimPosition { stage, position } = *unit else {
        return Err(refused("a claim's unit is one of its committed positions"));
    };
    let row = claim_row(builder, claim)?;
    if !row.holds_job() {
        return Err(refused("the claim is already decided"));
    }
    row.body.position(stage, position).map(|_| ()).ok_or_else(|| refused("the claim commits no such position"))
}

/// **Tag 151: a challenge of one unit of one lease.** The challenger signs `valid_until_daa` (at most one response window ahead): with
/// the row kept until the deadline, a replayed challenge never re-opens a closed one.
pub(super) fn apply_provider_challenge_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    subject: &ProviderSubjectV1,
    provider: &PalwBondKeyV2,
    unit: &PublicUnitV1,
    valid_until_daa: u64,
    challenger: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    court_at(builder)?;
    let claim = claim_subject(subject)?;
    ensure_route_header(builder, ctx)?;
    let now = ctx.daa_score;
    if now > valid_until_daa {
        return Err(refused("the challenge expired before it was carried"));
    }
    let deadline = now.saturating_add(PALW_PROVIDER_RESPONSE_WINDOW_DAA_V1);
    if valid_until_daa > deadline {
        return Err(refused("a challenge is valid for at most one response window"));
    }
    let challenger_operator = active_operator(builder, challenger)?;
    if builder.state.bonds.get(provider).map(|b| b.operator_id) == Some(challenger_operator) {
        return Err(refused("a provider is challenged by another operator"));
    }
    let route = route_of(builder)?;
    let lease = route.provider_lease_v1(subject, provider).ok_or_else(|| refused("no such lease"))?;
    if !lease.live_at(now) {
        return Err(refused("the lease is not in force"));
    }
    if deadline > lease.serve_until_daa {
        return Err(refused("the lease ends before an answer would be due"));
    }
    claim_unit_in_scope(builder, &claim, unit)?;
    let route = route_of(builder)?;
    if route.provider_challenge_v1(subject, provider, unit).is_some() {
        return Err(refused("a challenge of this unit of this lease is already open (or answered and not yet past its deadline)"));
    }
    if route.provider_open_challenges_by_v1(challenger) >= PALW_PROVIDER_MAX_OPEN_CHALLENGES_V1 {
        return Err(refused("the challenger holds the most open challenges a bond may"));
    }
    if free_collateral(builder, challenger, now) < PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1 as u128 {
        return Err(refused("the challenger's free collateral does not cover the challenge bond"));
    }
    let row = ProviderChallengeRowV1 {
        challenger: *challenger,
        bond: PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1,
        filed_daa: now,
        deadline_daa: deadline,
        answered: false,
    };
    write_challenge(builder, subject, provider, unit, Some(&row));
    Ok(())
}

/// **Tag 152: the provider's answer.** Verified against the claim's commitments by the kernel's own classification; a valid one by the
/// deadline clears the challenge (the challenger's fee is burned, its bond returns; the row stays as a tombstone until the deadline). A
/// wrong one is refused: it neither clears nor defaults — the clock decides.
pub(super) fn apply_provider_answer_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    subject: &ProviderSubjectV1,
    provider: &PalwBondKeyV2,
    unit: &PublicUnitV1,
    answer: &PublicUnitAnswerV1,
) -> Result<(), PalwStateV2Error> {
    court_at(builder)?;
    let claim = claim_subject(subject)?;
    ensure_route_header(builder, ctx)?;
    let now = ctx.daa_score;
    let row =
        route_of(builder)?.provider_challenge_v1(subject, provider, unit).ok_or_else(|| refused("no open challenge of this unit"))?;
    if row.answered {
        return Err(refused("the challenge is already answered"));
    }
    if now > row.deadline_daa {
        return Err(refused("the answer is past the challenge's deadline"));
    }
    // An answer is a response, not a proof: like a kernel `Respond` it stops short of the runs reserved for proofs (F-C4R4-10).
    if !charge_route_budget_v1(builder, ctx, 0, false)? {
        return Err(refused("the block's adjudication budget is spent: answer in a later block before the deadline"));
    }
    let (PublicUnitV1::ClaimPosition { stage, position }, PublicUnitAnswerV1::ClaimPosition { bytes }) = (unit, answer) else {
        return Err(refused("a claim's unit is answered by its position's response"));
    };
    let ledger = route_of(builder)?.ledger().map_err(refused)?;
    ledger.classify_served_position_v1(&claim.as_bytes(), *stage, *position, bytes).map_err(refused)?;
    write_challenge(builder, subject, provider, unit, Some(&ProviderChallengeRowV1 { bond: 0, answered: true, ..row }));
    // A challenge the provider answered was not free court work: its fee is burned out of the challenger's (now released) bond.
    builder.slash_bond(row.challenger, PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1 as u128)?;
    Ok(())
}

/// **Tag 153: the producer moves a claim's DA responsibility to its leases.** See [`crate::palw_provider_court_v1`].
pub(super) fn apply_da_transfer_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: &Hash64,
    producer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let fence = court_at(builder)?;
    ensure_route_header(builder, ctx)?;
    let now = ctx.daa_score;
    let operator = active_operator(builder, producer)?;
    let route = route_of(builder)?;
    let ledger = route.ledger().map_err(refused)?;
    let row = ledger.claims.get(&claim.as_bytes()).ok_or_else(|| refused("no such kernel claim"))?;
    if route.bond_key_of(&row.producer) != Some(*producer) {
        return Err(refused("only the claim's producer moves its DA responsibility"));
    }
    if row.committed_daa < fence {
        return Err(refused(
            "a claim committed below palw_provider_court_v1 stays its producer's (old and new claims are separated by the fence)",
        ));
    }
    if !row.holds_job() || row.reserved == 0 {
        return Err(refused("the claim is decided (or its producer already defaulted): a failure is never re-assigned"));
    }
    if ledger.demands.keys().any(|(c, _, _)| *c == claim.as_bytes()) {
        return Err(refused("a demand on the claim is open: a failure in flight is never re-assigned"));
    }
    let subject = ProviderSubjectV1::KernelClaim { claim: *claim };
    let prior = route.provider_subject_v1(&subject).unwrap_or_default();
    if prior.transferred_daa.is_some() {
        return Err(refused("the claim's DA responsibility has already moved"));
    }
    let horizon = palw_kernel_claim_horizon_bound_v1(row, &ledger.policy, ledger.opv.policy.as_ref());
    let availability = builder.state.provider_availability_v1(&subject, now, horizon, Some(operator));
    if !availability.ready {
        return Err(refused(
            "two live leases of distinct operators (none the producer's) must serve the claim through its liability bound",
        ));
    }
    if availability.reserved < row.reserved {
        return Err(refused("the leases reserve less than the claim: moving the liability must never make withholding cheaper"));
    }
    write_subject(builder, &subject, &ProviderSubjectRowV1 { transferred_daa: Some(now), ..prior });
    Ok(())
}

/// **Charge one lease** — once per (subject, provider): its reservation is slashed (burned at release), the row marked, the provider's
/// other challenges on the subject settled moot (their bonds return, no fee). Returns what the slash took (0 for a lease already
/// charged, and for a provider bond that is gone: the tick never fails a block on it).
fn charge_lease(
    builder: &mut TransitionBuilder<'_>,
    subject: &ProviderSubjectV1,
    provider: &PalwBondKeyV2,
) -> Result<u64, PalwStateV2Error> {
    let route = route_of(builder)?;
    let Some(lease) = route.provider_lease_v1(subject, provider) else { return Ok(0) };
    if lease.charged {
        return Ok(0);
    }
    let mut row = route.provider_subject_v1(subject).unwrap_or_default();
    let moot: Vec<PublicUnitV1> = route
        .provider_challenges_v1()
        .into_iter()
        .filter(|(s, p, _, _)| s == subject && p == provider)
        .map(|(_, _, u, _)| u)
        .collect();
    write_lease(builder, subject, provider, Some(&ProviderLeaseRowV1 { reserved: 0, charged: true, ..lease }));
    let slashed = if lease.reserved > 0 && builder.state.bonds.contains_key(provider) {
        builder.slash_bond(*provider, lease.reserved as u128)?
    } else {
        0
    };
    if let Err(at) = row.charged.binary_search(provider) {
        row.charged.insert(at, *provider);
    }
    write_subject(builder, subject, &row);
    for unit in moot {
        write_challenge(builder, subject, provider, &unit, None);
    }
    Ok(slashed)
}

/// **Could the provider still have answered?** A challenge whose claim the chain no longer holds was unanswerable at its deadline
/// through nobody's fault: it settles moot (the challenger's bond back, no fee, no charge). (A withdrawn artifact subject never has a row.)
fn subject_answerable(builder: &TransitionBuilder<'_>, subject: &ProviderSubjectV1) -> bool {
    match subject {
        ProviderSubjectV1::KernelClaim { claim } => claim_row(builder, claim).is_ok(),
        ProviderSubjectV1::Artifact { .. } => false,
    }
}

/// **Is a kernel-claim subject's claim Final now?** After Final every charge of its material is burned whole (the kernel's post-Final
/// rule: a post-Final default pays nobody).
fn subject_post_final(builder: &TransitionBuilder<'_>, subject: &ProviderSubjectV1) -> bool {
    let ProviderSubjectV1::KernelClaim { claim } = subject else { return false };
    builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|k| k.kernel_claim_row_v1(claim))
        .is_some_and(|row| matches!(row.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. }))
}

/// **After a charge: no live lease left ⇒ the subject lapses**, recorded on its row; a TRANSFERRED claim lapses in the kernel — void,
/// never convicted (`KernelLedgerV1::provider_lapse`).
fn lapse_if_uncovered(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    subject: &ProviderSubjectV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    let route = route_of(builder)?;
    if route.provider_leases_of_v1(subject).iter().any(|(_, lease)| lease.live_at(now)) {
        return Ok(());
    }
    let row = route.provider_subject_v1(subject).unwrap_or_default();
    let transferred = row.transferred_daa.is_some();
    write_subject(builder, subject, &ProviderSubjectRowV1 { lapsed_daa: Some(now), ..row });
    if let ProviderSubjectV1::KernelClaim { claim } = subject
        && transferred
    {
        let mut ledger = load_ledger(builder, ctx)?;
        let before = ledger.to_rows();
        // Refused only for a claim that is not provider-liable, which a transferred one always is: nothing to do then.
        if let Ok(events) = ledger.provider_lapse(&claim.as_bytes()) {
            apply_settlements(builder, &events, false)?;
            flush(builder, &ledger, &before);
        }
    }
    Ok(())
}

/// **The court's closing step** (before the kernel's tick; nothing at all below the fence): every challenge past its deadline
/// unanswered charges its lease and returns its challenger's bond; before Final the challenger is paid the PALW reporter share of the
/// charge (49%, [`palw_provider_reporter_share_v1`]) and the rest is burned, after Final all of it is burned (module doc); an answered
/// one's tombstone goes; a subject left with no live lease lapses; leases past their term are released.
pub(super) fn tick_provider_court_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if court_at(builder).is_err() {
        return Ok(());
    }
    let now = ctx.daa_score;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let due: Vec<(ProviderSubjectV1, PalwBondKeyV2, PublicUnitV1, ProviderChallengeRowV1)> =
        route.provider_challenges_v1().into_iter().filter(|(_, _, _, row)| now > row.deadline_daa).collect();
    for (subject, provider, unit, row) in due {
        // An earlier charge of this provider in this loop settled its other challenges moot.
        if route_of(builder)?.provider_challenge_v1(&subject, &provider, &unit).is_none() {
            continue;
        }
        write_challenge(builder, &subject, &provider, &unit, None);
        if row.answered || !subject_answerable(builder, &subject) {
            continue;
        }
        let post_final = subject_post_final(builder, &subject);
        let slashed = charge_lease(builder, &subject, &provider)?;
        if slashed > 0 && !post_final {
            let reward = palw_provider_reporter_share_v1(slashed);
            if let Some(payout) = builder.state.bonds.get(&row.challenger).map(|b| b.payout_payload) {
                builder.add_kernel_payout(payout, reward)?;
            }
        }
        lapse_if_uncovered(builder, ctx, &subject)?;
    }
    let expired: Vec<(ProviderSubjectV1, PalwBondKeyV2)> = route_of(builder)?
        .provider_leases_v1()
        .into_iter()
        .filter(|(_, _, lease)| now > lease.serve_until_daa)
        .map(|(subject, provider, _)| (subject, provider))
        .collect();
    for (subject, provider) in expired {
        write_lease(builder, &subject, &provider, None);
    }
    Ok(())
}

/// **The kernel's `ProviderLiableDefault` receipts of this block** (after its tick): a demand on a transferred claim that nobody answered
/// is the failure of every lease live behind it — each is charged; before Final the demanders share the reporter share (49%) of up to
/// `default_penalty` of the charge, the rest burned (after Final it is burned whole, as the producer path does); the claim (already void
/// in the kernel) lapses. The coalition's floor: it loses Σ leases (≥ the claim's reservation ≥ the penalty) less at most 49% of the
/// penalty — never less than the producer path's ≥ 51% of its penalty.
pub(super) fn settle_provider_liable_defaults_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    events: &[LedgerEventV1],
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    for event in events {
        let LedgerEventV1::ProviderLiableDefault { claim, post_final, demanders, .. } = event else { continue };
        let subject = ProviderSubjectV1::KernelClaim { claim: Hash64::from_bytes(*claim) };
        let providers: Vec<PalwBondKeyV2> =
            route_of(builder)?.provider_leases_of_v1(&subject).into_iter().filter(|(_, l)| l.live_at(now)).map(|(p, _)| p).collect();
        let mut pool = 0u64;
        for provider in &providers {
            pool = pool.saturating_add(charge_lease(builder, &subject, provider)?);
        }
        if !post_final && !demanders.is_empty() {
            let penalty = route_of(builder)?.header.policy.default_penalty;
            let share = palw_provider_reporter_share_v1(pool.min(penalty)) / demanders.len() as u64;
            for kid in demanders {
                let payee =
                    route_of(builder)?.bond_key_of(kid).and_then(|key| builder.state.bonds.get(&key).map(|b| b.payout_payload));
                if let Some(payout) = payee {
                    builder.add_kernel_payout(payout, share)?;
                }
            }
        }
        lapse_if_uncovered(builder, ctx, &subject)?;
    }
    Ok(())
}

/// **The court scope's admission of a kernel `FileDemand`** (ADR-0177 D2, [`crate::palw_court_scope_v1`]; called by the kernel route's
/// fold past `palw_provider_court_v1` only). The demanded unit must be claim-specific — a typed snapshot slice or the registered `M0`
/// memory is model content, refused at any count — and the requester's OPERATOR may name at most
/// `PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1` distinct units of one claim (no other requester's allowance is touched, so no
/// producer, seat or Sybil can exhaust an honest prosecutor's: G14's starvation-freedom). `Ok(Some(row))`: admitted, with the claim's
/// subject row carrying the unit (the caller writes it once the kernel accepts the demand); `Ok(None)`: no unit the ledger knows (the
/// kernel refuses it on its own); `Err`: the object is dropped.
pub(super) fn court_scope_admit_demand_v1(
    builder: &TransitionBuilder<'_>,
    ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
    signer: &PalwBondKeyV2,
    claim: &misaka_palw_kernel::hash::Digest,
    stage: u8,
    position: u32,
) -> Result<Option<(ProviderSubjectV1, ProviderSubjectRowV1)>, PalwStateV2Error> {
    use crate::palw_court_scope_v1::{palw_court_demand_allowed_v1, palw_court_scope_record_v1, palw_kernel_demand_unit_v1};
    let Some(unit) = palw_kernel_demand_unit_v1(ledger, claim, stage, position) else { return Ok(None) };
    palw_court_demand_allowed_v1(unit).map_err(|e| refused(e.why()))?;
    let operator = active_operator(builder, signer)?;
    let subject = ProviderSubjectV1::KernelClaim { claim: Hash64::from_bytes(*claim) };
    let mut row = route_of(builder)?.provider_subject_v1(&subject).unwrap_or_default();
    palw_court_scope_record_v1(&mut row.requested, operator, (stage, position)).map_err(refused)?;
    Ok(Some((subject, row)))
}

/// Write the court scope's tally (the claim's subject row) once the kernel accepted the demand it admitted.
pub(super) fn write_court_scope_row_v1(builder: &mut TransitionBuilder<'_>, subject: &ProviderSubjectV1, row: &ProviderSubjectRowV1) {
    write_subject(builder, subject, row);
}
