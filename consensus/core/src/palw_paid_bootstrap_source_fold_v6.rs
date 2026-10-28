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
    b.write_kernel_row(key.0, key.1, value)?;
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
    put(b, &binding_key_v6(&a.enrollment.id()), &binding)
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
    Ok(())
}
