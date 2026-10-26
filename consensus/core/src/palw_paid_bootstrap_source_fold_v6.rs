//! Mutations only for explicitly signed V6 admission/lock/receipt namespaces.
use super::*;
use crate::palw_claim_challenge_v2::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1};
use crate::palw_paid_bootstrap_source_v6::*;
use misaka_palw_kernel::hash::Digest;
fn bad(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}
fn native(why: NativeDecodeErrorV1<String>) -> PalwStateV2Error {
    crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, bad)
}
fn put<T: borsh::BorshSerialize>(b: &mut TransitionBuilder<'_>, key: &[u8; 65], value: &T) -> Result<(), PalwStateV2Error> {
    let n = borsh::object_length(value).map_err(|e| bad(e.to_string()))?;
    b.native_reservation
        .grow_v1((n as u64).checked_add(65).ok_or(PalwStateV2Error::Overflow("V6 encoded source row"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<T>("V6 source row encoding", "allocator refusal"))
    })?;
    borsh::to_writer(&mut bytes, value).map_err(|e| bad(e.to_string()))?;
    let absent = !b
        .state
        .kernel_route()
        .ok_or_else(|| bad("V6 route absent"))?
        .aux
        .keys()
        .any(|(t, k)| *t == PAID_BOOTSTRAP_SOURCE_TABLE_V6 && k.as_slice() == key);
    let key = (PAID_BOOTSTRAP_SOURCE_TABLE_V6, key.to_vec());
    let value = Some(bytes);
    b.reserve_map_journal_v1(&key, &value, absent, "V6 bootstrap source rows")?;
    b.write_kernel_row(key.0, key.1, value);
    Ok(())
}
pub(super) fn admit(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    a: &PalwPaidClaimAuthorityV2,
    requested: Digest,
) -> Result<(), PalwStateV2Error> {
    let (at, policy) = b.params.paid_bootstrap_source_v6.ok_or_else(|| bad("V6 bootstrap source capability absent"))?;
    if ctx.daa_score < at || policy.id() != requested {
        return Err(bad("V6 source policy is dormant or not the exact signed policy"));
    }
    let route = b.state.kernel_route().ok_or_else(|| bad("V6 route absent"))?;
    let count = route.aux.keys().filter(|(t, k)| *t == PAID_BOOTSTRAP_SOURCE_TABLE_V6 && k.len() == 65 && k[0] == 1).count();
    if count >= policy.maximum_consumers as usize || route.paid_bootstrap_binding_v6(&a.enrollment.id()).map_err(native)?.is_some() {
        return Err(bad("V6 source binding duplicated or finite epoch consumer capacity exhausted"));
    }
    let ledger = route.ledger_checked_v1().map_err(native)?;
    let binding = binding_for_authority_v6(route, &ledger, a, policy).map_err(native)?;
    drop(ledger);
    put(b, &binding_key_v6(&a.enrollment.id()), &binding)?;
    fund_producer_economics_v1(b, ctx, a)
}
pub(super) fn lock(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, enrollment: &Digest) -> Result<(), PalwStateV2Error> {
    let route = b.state.kernel_route().ok_or_else(|| bad("V6 route absent"))?;
    let a = route.paid_claim_enrollment_checked_v6(enrollment).map_err(native)?;
    if route.paid_bootstrap_lock_v6(enrollment).map_err(native)?.is_some()
        || route.aux.keys().any(|(t, k)| *t == PAID_CLAIM_LOCKS_V2 && k.as_slice() == enrollment)
    {
        return Err(bad("V6 source lock already consumed or mixed with legacy lock"));
    }
    if !a.enrollment.lifetime.receipt_open(ctx.daa_score) {
        return Err(bad("V6 source lock is outside signed checking lifetime"));
    }
    let binding = route.paid_bootstrap_binding_v6(enrollment).map_err(native)?.ok_or_else(|| bad("V6 binding absent"))?;
    let ledger = route.ledger_checked_v1().map_err(native)?;
    a.validate(&ledger).map_err(bad)?;
    let source = collect_for_authority_v6(route, &ledger, &a, &binding, ctx.daa_score).map_err(native)?;
    let seed = binding.seed(&source).map_err(bad)?;
    let (seats, quorum, assignment) = assignment_v6(&a, &binding, seed).map_err(native)?;
    let (seats, _seats_owner) = seats.into_parts_v1();
    let row = PalwPaidBootstrapLockV6 {
        version: 6,
        binding: binding.id(),
        source,
        lock: PalwPaidClaimLockV2 {
            enrollment: *enrollment,
            context: binding.id(),
            anchor: anchor_v6(&binding, &source).map_err(bad)?,
            seed,
            locked_daa: ctx.daa_score,
            assignment,
            seats,
            quorum,
            source_valid: true,
        },
    };
    drop(ledger);
    put(b, &lock_key_v6(enrollment), &row)
}
pub(super) fn refresh(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(route) = b.state.kernel_route() else { return Ok(()) };
    let ledger = route.ledger_checked_v1().map_err(native)?;
    let mut after: Option<Digest> = None;
    // Admission caps this namespace. Iterate in canonical key order without
    // staging an unbounded list of cloned authority/lock/material rows.
    loop {
        let next = b.state.kernel_route().and_then(|route| {
            route.aux.keys().find_map(|(t, k)| {
                if *t != PAID_BOOTSTRAP_SOURCE_TABLE_V6 || k.len() != 65 || k[0] != 2 {
                    return None;
                }
                let id: Digest = k[1..].try_into().expect("lock65");
                after.is_none_or(|previous| id > previous).then_some(id)
            })
        });
        let Some(id) = next else { break };
        after = Some(id);
        let route = b.state.kernel_route().unwrap();
        let row = route.paid_bootstrap_lock_v6(&id).map_err(native)?.ok_or_else(|| bad("V6 lock vanished"))?;
        if !row.lock.source_valid {
            continue;
        }
        let a = route.paid_claim_enrollment_checked_v6(&id).map_err(native)?;
        a.validate(&ledger).map_err(bad)?;
        if ledger.claims.get(&a.enrollment.base_claim).is_none_or(|claim| claim.life.state.is_terminal()) {
            continue;
        }
        let binding = route.paid_bootstrap_binding_v6(&id).map_err(native)?.ok_or_else(|| bad("V6 lock lost binding"))?;
        let current = collect_for_authority_v6(route, &ledger, &a, &binding, ctx.daa_score).map_err(native)?;
        if binding.seed(&current).ok() != Some(row.lock.seed) || current != row.source {
            let _clone = crate::palw_native_resource_v1::reserve_native_requests_v1(
                (row.lock.seats.len() * std::mem::size_of::<PalwBondKeyV2>()) as u64,
            )
            .map_err(PalwStateV2Error::NativeResource)?;
            let mut invalid = (*row).clone();
            invalid.lock.source_valid = false;
            put(b, &lock_key_v6(&id), &invalid)?;
        }
    }
    release_economics(b, ctx)
}

fn put_economics(b: &mut TransitionBuilder<'_>, key: &[u8], bytes: Vec<u8>) -> Result<(), PalwStateV2Error> {
    let table = crate::palw_paid_bootstrap_economics_v1::PALW_SOURCE_ECONOMICS_TABLE_V1;
    b.native_reservation
        .grow_v1((bytes.len() as u64).checked_add(key.len() as u64).ok_or(PalwStateV2Error::Overflow("source economics row"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    let absent = !b
        .state
        .kernel_route()
        .ok_or_else(|| bad("source economics route absent"))?
        .aux
        .keys()
        .any(|(t, k)| *t == table && k.as_slice() == key);
    let key = (table, key.to_vec());
    let value = Some(bytes);
    b.reserve_map_journal_v1(&key, &value, absent, "source economics rows")?;
    b.write_kernel_row(key.0, key.1, value);
    Ok(())
}

fn active_economics(
    b: &TransitionBuilder<'_>,
    daa: u64,
) -> Option<crate::palw_paid_bootstrap_economics_v1::PalwSourceEconomicsPolicyV1> {
    b.params.source_economics_v1.as_ref().and_then(|(at, policy)| (daa >= *at).then_some(*policy))
}

fn load_economics(
    b: &TransitionBuilder<'_>,
) -> Result<
    (std::collections::BTreeMap<Vec<u8>, Vec<u8>>, crate::palw_paid_bootstrap_economics_v1::PalwSourceEconomicsBookV1),
    PalwStateV2Error,
> {
    use crate::palw_paid_bootstrap_economics_v1::{PALW_SOURCE_ECONOMICS_TABLE_V1, PalwSourceEconomicsBookV1};
    let before = b
        .state
        .kernel_route()
        .map(|route| {
            route
                .aux
                .iter()
                .filter(|((table, _), _)| *table == PALW_SOURCE_ECONOMICS_TABLE_V1)
                .map(|((_, key), value)| (key.clone(), value.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let book =
        PalwSourceEconomicsBookV1::restore(before.iter().map(|(key, value)| (key.as_slice(), value.as_slice()))).map_err(bad)?;
    Ok((before.into_iter().collect(), book))
}

fn store_economics(
    b: &mut TransitionBuilder<'_>,
    previous: std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
    book: &crate::palw_paid_bootstrap_economics_v1::PalwSourceEconomicsBookV1,
) -> Result<(), PalwStateV2Error> {
    for (key, bytes) in book.encoded_rows().map_err(bad)? {
        if previous.get(key.as_slice()) != Some(&bytes) {
            put_economics(b, &key, bytes)?;
        }
    }
    Ok(())
}

fn court_from_claim(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    policy: &crate::palw_paid_bootstrap_economics_v1::PalwSourceEconomicsPolicyV1,
) -> Result<Option<crate::palw_paid_bootstrap_economics_v1::PalwSourceCourtRowV1>, PalwStateV2Error> {
    use crate::palw_kernel_settlement_v1::{PalwKernelSettlementV1, palw_kernel_claim_settlement_v1};
    use crate::palw_paid_bootstrap_economics_v1::{PALW_SOURCE_ECONOMICS_VERSION_V1, PalwSourceCourtRowV1};
    let Some(route) = b.state.kernel_route() else { return Ok(None) };
    let Ok(authority) = route.paid_claim_enrollment_checked_v6(&policy.enrollment) else { return Ok(None) };
    if authority.enrollment.producer != policy.payer {
        return Err(bad("source economics payer is not the enrolled producer"));
    }
    let ledger = route.ledger_checked_v1().map_err(native)?;
    let Some(claim) = ledger.claims.get(&authority.enrollment.base_claim) else { return Ok(None) };
    if claim.reserved != 0 && claim.reserved != policy.budget_reservation_sompi {
        return Err(bad("source economics budget reservation is not the claim reservation"));
    }
    if claim.liability_until.is_some_and(|until| until != policy.inclusive_until_daa) {
        return Err(bad("source economics horizon is not the claim liability horizon"));
    }
    let forfeited = ledger.opv.claims.get(&authority.enrollment.base_claim).is_some_and(|row| row.forfeited_after_final);
    let standing = palw_kernel_claim_settlement_v1(
        &claim.life.state,
        claim.convicted,
        claim.reserved,
        claim.liability_until,
        forfeited,
        ctx.daa_score,
    );
    Ok(Some(PalwSourceCourtRowV1 {
        version: PALW_SOURCE_ECONOMICS_VERSION_V1,
        enrollment: policy.enrollment,
        actor: policy.actor,
        inclusive_until_daa: policy.inclusive_until_daa,
        settled: matches!(standing, PalwKernelSettlementV1::Settled { .. }),
        convicted: matches!(standing, PalwKernelSettlementV1::Convicted),
    }))
}

pub(super) fn fund_producer_economics_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    authority: &crate::palw_claim_challenge_v2::PalwPaidClaimAuthorityV2,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_paid_bootstrap_economics_v1::{PALW_SOURCE_ECONOMICS_VERSION_V1, PalwSourceFundingRowV1};
    let Some(policy) = active_economics(b, ctx.daa_score) else { return Ok(()) };
    if policy.enrollment != authority.enrollment.id() {
        return Ok(());
    }
    if policy.payer != authority.enrollment.producer {
        return Err(bad("source economics payer is not the enrolled producer"));
    }
    policy.validate().map_err(bad)?;
    if let Some(route) = b.state.kernel_route()
        && let Ok(ledger) = route.ledger_checked_v1()
        && let Some(claim) = ledger.claims.get(&authority.enrollment.base_claim)
        && claim.reserved != 0
        && claim.reserved != policy.budget_reservation_sompi
    {
        return Err(bad("source economics budget reservation is not the claim reservation"));
    }
    let (previous, mut book) = load_economics(b)?;
    book.fund(PalwSourceFundingRowV1 {
        version: PALW_SOURCE_ECONOMICS_VERSION_V1,
        enrollment: policy.enrollment,
        actor: policy.actor,
        payer: policy.payer,
        prepaid_sompi: policy.prepaid_sompi,
        reserved_sompi: policy.reserved_sompi,
        r_cap_sompi: policy.r_cap_sompi,
        budget_reservation_sompi: policy.budget_reservation_sompi,
    })
    .map_err(bad)?;
    store_economics(b, previous, &book)
}

pub(super) fn observe_checker_economics_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &crate::palw_state_v2::PalwBondKeyV2,
    enrollment: &[u8; 64],
) -> Result<(), PalwStateV2Error> {
    let Some(policy) = active_economics(b, ctx.daa_score) else { return Ok(()) };
    if policy.enrollment != *enrollment || policy.actor != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer) {
        return Ok(());
    }
    apply_checker_economics(b, ctx, &policy, true)
}

fn apply_checker_economics(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    policy: &crate::palw_paid_bootstrap_economics_v1::PalwSourceEconomicsPolicyV1,
    from_checker: bool,
) -> Result<(), PalwStateV2Error> {
    let (previous, mut book) = load_economics(b)?;
    if previous.is_empty() {
        return if from_checker { Err(bad("source economics checker approval has no producer funding")) } else { Ok(()) };
    }
    let court_key = {
        let mut key = [0u8; 129];
        key[0] = crate::palw_paid_bootstrap_economics_v1::PALW_SOURCE_ECONOMICS_ACTION_COURT_V1;
        key[1..65].copy_from_slice(&policy.enrollment);
        key[65..].copy_from_slice(&policy.actor);
        key
    };
    if (from_checker || previous.keys().any(|key| key.as_slice() == court_key.as_slice()))
        && let Some(court) = court_from_claim(b, ctx, policy)?
    {
        book.observe_court(court).map_err(bad)?;
    }
    book.release_at(ctx.daa_score).map_err(bad)?;
    store_economics(b, previous, &book)
}

fn release_economics(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(policy) = active_economics(b, ctx.daa_score) else {
        let (previous, mut book) = load_economics(b)?;
        if previous.is_empty() {
            return Ok(());
        }
        book.release_at(ctx.daa_score).map_err(bad)?;
        return store_economics(b, previous, &book);
    };
    apply_checker_economics(b, ctx, &policy, false)
}
