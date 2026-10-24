//! Native journal lifecycle for aux146 checker duties. Separate from beacon
//! source fees; only signed funding supplies money, never the R ceiling.
use super::*;
use crate::palw_native_resource_v1::{
    NativeCoverageV1 as C, NativeDecodeErrorV1, reserve_native_requests_v1, typed::BTreeNodeEnvelopeV1,
};
use crate::palw_source_bootstrap_v1::{PalwSourceBootstrapEpochRowV1, PalwSourceBootstrapWorkRowV1};
use crate::palw_source_verifier_economics_v2::*;
fn bad(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}
fn native(why: NativeDecodeErrorV1<String>) -> PalwStateV2Error {
    crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, bad)
}
fn route<'a>(b: &'a TransitionBuilder<'_>) -> Result<&'a crate::palw_kernel_route_v1::PalwKernelRouteStateV1, PalwStateV2Error> {
    b.state.kernel_route().ok_or_else(|| bad("source economics route absent"))
}
fn write<T: borsh::BorshSerialize, const N: usize>(
    b: &mut TransitionBuilder<'_>,
    key: &[u8; N],
    value: Option<&T>,
) -> Result<(), PalwStateV2Error> {
    let n = value.map(borsh::object_length).transpose().map_err(|why| bad(why.to_string()))?.unwrap_or(0) as u64;
    let q = n.checked_mul(2).and_then(|v| v.checked_add(2 * N as u64)).ok_or(PalwStateV2Error::Overflow("source row buffers"))?;
    b.native_reservation.grow_v1(q).map_err(PalwStateV2Error::NativeResource)?;
    let absent = read_raw_absent(route(b)?, key)?;
    let mut k = Vec::new();
    k.try_reserve_exact(N).map_err(|why| PalwStateV2Error::NativeResource(C::unsupported::<T>("source key", &why.to_string())))?;
    k.extend_from_slice(key);
    let encoded = if let Some(value) = value {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(n as usize)
            .map_err(|why| PalwStateV2Error::NativeResource(C::unsupported::<T>("source row", &why.to_string())))?;
        borsh::to_writer(&mut bytes, value).map_err(|why| bad(why.to_string()))?;
        Some(bytes)
    } else {
        None
    };
    let pair = (SOURCE_VERIFIER_ECONOMICS_TABLE_V2, k);
    b.reserve_map_journal_v1(&pair, &encoded, absent, "source checker economics")?;
    b.write_kernel_row(pair.0, pair.1, encoded)
}
fn read_raw_absent<const N: usize>(
    r: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    key: &[u8; N],
) -> Result<bool, PalwStateV2Error> {
    crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(r, SOURCE_VERIFIER_ECONOMICS_TABLE_V2, key)
        .map(|v| v.is_none())
        .map_err(PalwStateV2Error::NativeResource)
}
fn put<T: borsh::BorshSerialize, const N: usize>(b: &mut TransitionBuilder<'_>, key: &[u8; N], v: &T) -> Result<(), PalwStateV2Error> {
    write(b, key, Some(v))
}
fn governor(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    epoch: &PalwSourceBootstrapEpochRowV1,
) -> Result<PalwSourceVerifierEconomicsEpochV2, PalwStateV2Error> {
    let expected =
        govern_v2(b.params, epoch, ctx.daa_score).map_err(native)?.ok_or_else(|| bad("source checker economics is dormant"))?;
    let actual = read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route(b)?, &[0])
        .map_err(native)?
        .ok_or_else(|| bad("source economic governor absent"))?;
    if !actual.same_contract(&expected)
        || actual.funded_works > actual.policy.maximum_funded_works
        || actual.liabilities > actual.policy.maximum_liabilities
    {
        return Err(bad("source economic governor/counters differ"));
    }
    Ok(*actual)
}
pub(super) fn prepare(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(epoch) = b.state.source_bootstrap_epoch_v1()? else { return Ok(()) };
    let Some(g) = govern_v2(b.params, &epoch, ctx.daa_score).map_err(native)? else { return Ok(()) };
    if let Some(old) = read_v2::<PalwSourceVerifierEconomicsEpochV2, 1>(route(b)?, &[0]).map_err(native)? {
        if !old.same_contract(&g) {
            return Err(bad("source economics cannot replace its whole-epoch contract"));
        }
        return Ok(());
    }
    if epoch.source_works != 0 {
        return Err(bad("source economic adoption after an opened work is forbidden"));
    }
    put(b, &[0], &g)
}
pub(super) fn legacy_approval_allowed(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    epoch: &PalwSourceBootstrapEpochRowV1,
) -> Result<(), PalwStateV2Error> {
    if govern_v2(b.params, epoch, ctx.daa_score).map_err(native)?.is_some() {
        return Err(bad("governed source checker requires its signed funded approval"));
    }
    Ok(())
}
pub(super) fn fund(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    actor: PalwBondKeyV2,
    epoch: &PalwSourceBootstrapEpochRowV1,
    terms: &PalwSourceVerifierFeeTermsV2,
) -> Result<(), PalwStateV2Error> {
    let mut g = governor(b, ctx, epoch)?;
    b.state.source_bootstrap_check_work_authority_v1(terms.work)?;
    let work = b.state.source_bootstrap_work_v1(terms.work)?.ok_or_else(|| bad("source funding work absent"))?;
    if terms.version != 2
        || terms.policy != g.policy.id()
        || terms.sponsor != actor
        || terms.fee_per_checker_sompi < g.policy.minimum_fee_per_checker_sompi
        || !work.approvals.is_empty()
        || work.settlement.is_some()
        || ctx.daa_score > work.work.deadline_daa
        || ctx.daa_score >= g.until_daa
    {
        return Err(bad("source funding changes its explicit pre-approval terms"));
    }
    if read_v2::<PalwSourceVerifierFundingV2, 65>(route(b)?, &key_v2(1, terms.work)).map_err(native)?.is_some() {
        return Err(bad("source work already funded"));
    }
    if g.funded_works >= g.policy.maximum_funded_works {
        return Err(bad("source funding count cap"));
    }
    let bytes = (epoch.cohort.len() as u64)
        .checked_mul(std::mem::size_of::<BTreeNodeEnvelopeV1<[u8; 64], ()>>() as u64)
        .ok_or_else(|| PalwStateV2Error::NativeResource(C::overflow::<PalwSourceVerifierFundingV2>("source distinct payees")))?;
    let _workspace = reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?;
    let distinct: BTreeSet<_> = epoch.cohort.iter().map(|s| s.bond).collect();
    let count = u16::try_from(distinct.len()).map_err(|_| bad("source payee count overflow"))?;
    let prepaid = terms
        .fee_per_checker_sompi
        .checked_mul(u64::from(count))
        .filter(|n| *n > 0)
        .ok_or_else(|| bad("source prepaid amount overflow"))?;
    if b.gate_room(&actor, ctx.daa_score, PalwRcoreGateV1::Work) < u128::from(prepaid) {
        return Err(bad("source funding lacks actual free collateral"));
    }
    if b.slash_bond(actor, u128::from(prepaid))? != prepaid {
        return Err(bad("source funding was not fully debited"));
    }
    let f = PalwSourceVerifierFundingV2 {
        version: 2,
        terms: *terms,
        epoch: g.epoch,
        funded_daa: ctx.daa_score,
        until_daa: g.until_daa,
        distinct_payees: count,
        prepaid,
        held: prepaid,
        queued: 0,
        refunded: 0,
        forfeited: 0,
        closed: false,
        refund_queue: None,
        refund_queued_block: None,
        refund_acknowledged_block: None,
        refund_acknowledged_daa: None,
    };
    put(b, &key_v2(1, terms.work), &f)?;
    put(b, &horizon_key(g.until_daa, terms.work), &1u8)?;
    g.funded_works += 1;
    put(b, &[0], &g)
}
fn hold_change(
    b: &mut TransitionBuilder<'_>,
    g: &PalwSourceVerifierEconomicsEpochV2,
    actor: PalwBondKeyV2,
    amount: u64,
    add: bool,
) -> Result<(), PalwStateV2Error> {
    let old = read_v2::<PalwSourceVerifierBondHoldV2, 69>(route(b)?, &actor_key_v2(3, &actor)).map_err(native)?;
    let mut h = old.as_deref().copied().unwrap_or(PalwSourceVerifierBondHoldV2 {
        version: 2,
        actor,
        policy: g.policy.id(),
        until_daa: g.until_daa,
        reserved: 0,
    });
    if h.version != 2 || h.actor != actor || h.policy != g.policy.id() || h.until_daa != g.until_daa || !add && old.is_none() {
        return Err(bad("source aggregate hold substituted or absent"));
    }
    h.reserved = if add { h.reserved.checked_add(u128::from(amount)) } else { h.reserved.checked_sub(u128::from(amount)) }
        .ok_or_else(|| bad("source hold accounting overflow"))?;
    // The existing primary exposure ledger is what every native collateral
    // gate/withdrawal reads. Do not add a fallible aux lookup to scalar gates,
    // and do not count this duty a second time in committed collateral.
    let total = b.state.reserved_exposure(&actor);
    let next = if add { total.checked_add(u128::from(amount)) } else { total.checked_sub(u128::from(amount)) }
        .ok_or_else(|| bad("source primary exposure accounting overflow"))?;
    b.write_exposure(actor, (next > 0).then_some(next))?;
    put(b, &actor_key_v2(3, &actor), &h)
}
pub(crate) fn income_context(g: &PalwSourceVerifierEconomicsEpochV2, funding: Hash64) -> Hash64 {
    identity_v2(b"MISAKA/PALW/SOURCE-VERIFIER-INCOME-CONTEXT/V2", &(g.policy.id(), g.epoch, funding))
}
pub(super) fn approve(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    actor: PalwBondKeyV2,
    epoch: &PalwSourceBootstrapEpochRowV1,
    work: &PalwSourceBootstrapWorkRowV1,
    funding: Hash64,
) -> Result<(), PalwStateV2Error> {
    let mut g = governor(b, ctx, epoch)?;
    let f = read_v2::<PalwSourceVerifierFundingV2, 65>(route(b)?, &key_v2(1, work.work.id()))
        .map_err(native)?
        .ok_or_else(|| bad("funded source approval lacks prepaid terms"))?;
    if f.version != 2
        || f.closed
        || f.terms.id() != funding
        || f.terms.policy != g.policy.id()
        || f.epoch != g.epoch
        || !f.conserves()
        || f.funded_daa > ctx.daa_score
        || f.until_daa != g.until_daa
        || ctx.daa_score > work.work.deadline_daa
    {
        return Err(bad("source approval changes exact funded contract"));
    }
    let key = liability_key_v2(&actor, work.work.id());
    if let Some(d) = read_v2::<PalwSourceVerifierLiabilityV2, 133>(route(b)?, &key).map_err(native)? {
        if d.funding != funding
            || d.exposure != g.policy.exposure_per_work_sompi
            || !matches!(d.status, PalwSourceVerifierDutyStatusV2::Active)
        {
            return Err(bad("repeated source actor changes original duty"));
        }
        return Ok(());
    }
    if b.bond_budget_mirror_at(ctx.daa_score).is_none() {
        return Err(bad("checker income needs its own active R budget, not R as funding"));
    }
    if g.liabilities >= g.policy.maximum_liabilities {
        return Err(bad("source liability count cap"));
    }
    if b.gate_room(&actor, ctx.daa_score, PalwRcoreGateV1::Work) < u128::from(g.policy.exposure_per_work_sompi) {
        return Err(bad("source checker collateral is committed elsewhere"));
    }
    let d = PalwSourceVerifierLiabilityV2 {
        version: 2,
        policy: g.policy.id(),
        work: work.work.id(),
        claim: work.work.claim,
        actor,
        funding,
        exposure: g.policy.exposure_per_work_sompi,
        fee: f.terms.fee_per_checker_sompi,
        accepted_daa: ctx.daa_score,
        until_daa: g.until_daa,
        status: PalwSourceVerifierDutyStatusV2::Active,
        entitlement: PalwSourceVerifierEntitlementV2::Held,
    };
    b.reserve_source_income_v1(ctx, income_context(&g, funding), d.id(), actor, g.until_daa, d.fee)?;
    hold_change(b, &g, actor, d.exposure, true)?;
    put(b, &key, &d)?;
    put(b, &work_actor_key(d.work, &actor), &d.id())?;
    put(b, &pair_key(7, d.claim, d.id()), &PalwSourceVerifierDutyRefV2 { actor, work: d.work })?;
    g.liabilities += 1;
    put(b, &[0], &g)
}
pub(super) fn court_events(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    events: &[misaka_palw_kernel::ledger::LedgerEventV1],
) -> Result<(), PalwStateV2Error> {
    let Some(epoch) = b.state.source_bootstrap_epoch_v1()? else { return Ok(()) };
    if govern_v2(b.params, &epoch, ctx.daa_score).map_err(native)?.is_none() {
        return Ok(());
    }
    let g = governor(b, ctx, &epoch)?;
    for event in events {
        let misaka_palw_kernel::ledger::LedgerEventV1::Convicted { claim, accuser, .. } = event else { continue };
        let prefix = key_v2(7, Hash64::from_bytes(*claim));
        let mut after = None;
        loop {
            let Some((index, reference)) =
                first_v2::<PalwSourceVerifierDutyRefV2, 129>(route(b)?, &prefix, after.as_ref()).map_err(native)?
            else {
                break;
            };
            after = Some(index);
            let mut d = *read_v2::<PalwSourceVerifierLiabilityV2, 133>(route(b)?, &liability_key_v2(&reference.actor, reference.work))
                .map_err(native)?
                .ok_or_else(|| bad("source court index lost duty"))?;
            if d.claim.as_bytes() != *claim || index != pair_key(7, d.claim, d.id()) {
                return Err(bad("source court index substitutes duty"));
            }
            if !matches!(d.status, PalwSourceVerifierDutyStatusV2::Active) || ctx.daa_score > d.until_daa {
                continue;
            }
            if read_v2::<PalwSourceVerifierCourtOutcomeV2, 65>(route(b)?, &key_v2(4, d.id())).map_err(native)?.is_some() {
                return Err(bad("source court outcome consumed twice"));
            }
            let mut f = *read_v2::<PalwSourceVerifierFundingV2, 65>(route(b)?, &key_v2(1, d.work))
                .map_err(native)?
                .ok_or_else(|| bad("source court funding absent"))?;
            let collected = b.slash_bond(d.actor, u128::from(d.exposure))?;
            hold_change(b, &g, d.actor, d.exposure, false)?;
            b.cancel_source_income_v1(ctx, income_context(&g, d.funding), d.id(), d.actor, d.accepted_daa)?;
            d.status = PalwSourceVerifierDutyStatusV2::Convicted { block: ctx.block, daa: ctx.daa_score, collected };
            d.entitlement = PalwSourceVerifierEntitlementV2::Forfeited { block: ctx.block };
            f.held = f.held.checked_sub(d.fee).ok_or_else(|| bad("source court fee underflow"))?;
            f.forfeited = f.forfeited.checked_add(d.fee).ok_or_else(|| bad("source court forfeiture overflow"))?;
            let receipt = PalwSourceVerifierCourtOutcomeV2 {
                version: 2,
                policy: d.policy,
                work: d.work,
                claim: d.claim,
                actor: d.actor,
                block: ctx.block,
                daa: ctx.daa_score,
                accuser: Hash64::from_bytes(*accuser),
                exposure: d.exposure,
                collected,
            };
            put(b, &key_v2(4, d.id()), &receipt)?;
            put(b, &liability_key_v2(&d.actor, d.work), &d)?;
            put(b, &key_v2(1, d.work), &f)?;
        }
    }
    Ok(())
}
fn queue_room(b: &TransitionBuilder<'_>, payload: Hash64, amount: u64) -> bool {
    let key = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
    b.state
        .pending_payouts
        .get(&key)
        .map_or(b.state.pending_payouts.len() < PALW_V2_MAX_PENDING_PAYOUTS, |p| p.amount.checked_add(amount).is_some())
}
fn queue_credit(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    f: &PalwSourceVerifierFundingV2,
    actor: PalwBondKeyV2,
    duty: Option<Hash64>,
    amount: u64,
) -> Result<PalwSourceVerifierQueueV2, PalwStateV2Error> {
    let payload = b.state.bond(&actor).ok_or(PalwStateV2Error::MissingBond(actor))?.payout_payload;
    let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
    let q = PalwSourceVerifierQueueV2 {
        version: 2,
        work: f.terms.work,
        actor,
        funding: f.terms.id(),
        duty,
        queue,
        payload,
        amount,
        queued_block: ctx.block,
        queued_daa: ctx.daa_score,
    };
    if !read_raw_absent(route(b)?, &pair_key(8, queue, q.id()))? {
        return Err(bad("source queue identity already consumed"));
    }
    b.add_kernel_payout(payload, amount)?;
    put(b, &pair_key(8, queue, q.id()), &q)?;
    Ok(q)
}
/// After native closing and source settlement, strictly after inclusive H.
/// Horizon and per-work indices make this proportional to due works/duties.
/// Queue saturation defers the unsettled part; it neither mints nor destroys it.
pub(super) fn sweep(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(epoch) = b.state.source_bootstrap_epoch_v1()? else { return Ok(()) };
    if govern_v2(b.params, &epoch, ctx.daa_score).map_err(native)?.is_none() {
        return Ok(());
    }
    let g = governor(b, ctx, &epoch)?;
    let Some((first, _)) = first_v2::<u8, 73>(route(b)?, &[5], None).map_err(native)? else { return Ok(()) };
    if ctx.daa_score <= u64::from_be_bytes(first[1..9].try_into().expect("8 bytes")) {
        return Ok(());
    }
    let ledger = route(b)?
        .ledger_native_projection_checked_v9()
        .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, bad))?;
    loop {
        let Some((horizon, marker)) = first_v2::<u8, 73>(route(b)?, &[5], None).map_err(native)? else { break };
        if *marker != 1 {
            return Err(bad("source horizon index marker"));
        }
        let until = u64::from_be_bytes(horizon[1..9].try_into().expect("8 bytes"));
        if ctx.daa_score <= until {
            break;
        }
        let work = Hash64::from_bytes(horizon[9..].try_into().expect("64 bytes"));
        let mut f = *read_v2::<PalwSourceVerifierFundingV2, 65>(route(b)?, &key_v2(1, work))
            .map_err(native)?
            .ok_or_else(|| bad("source horizon funding absent"))?;
        if f.closed || f.until_daa != until || f.terms.work != work || f.epoch != g.epoch || !f.conserves() {
            return Err(bad("source horizon substitutes funding"));
        }
        let source = b.state.source_bootstrap_work_v1(work)?.ok_or_else(|| bad("source funding work absent at release"))?;
        let eligible = source.settlement.is_some()
            && ledger.claims.get(&source.work.claim.as_bytes()).is_some_and(|r| !r.convicted)
            && matches!(crate::palw_kernel_settlement_v1::palw_kernel_row_settlement_v1(&ledger,&source.work.claim.as_bytes(),ctx.daa_score),Some(crate::palw_kernel_settlement_v1::PalwKernelSettlementV1::Settled{settled_from_daa,..}) if ctx.daa_score>=settled_from_daa);
        let mut cursor = None;
        let mut pending = false;
        loop {
            let Some((index, id)) = first_v2::<Hash64, 133>(route(b)?, &key_v2(6, work), cursor.as_ref()).map_err(native)? else {
                break;
            };
            cursor = Some(index);
            let actor: PalwBondKeyV2 = borsh::from_slice(&index[65..]).map_err(|why| bad(why.to_string()))?;
            let mut d = *read_v2::<PalwSourceVerifierLiabilityV2, 133>(route(b)?, &liability_key_v2(&actor, work))
                .map_err(native)?
                .ok_or_else(|| bad("source work index lost duty"))?;
            if d.id() != *id || d.work != work || d.actor != actor || d.funding != f.terms.id() {
                return Err(bad("source work index substitutes duty"));
            }
            if matches!(d.status, PalwSourceVerifierDutyStatusV2::Convicted { .. }) {
                continue;
            }
            if matches!(d.status, PalwSourceVerifierDutyStatusV2::Active) {
                hold_change(b, &g, actor, d.exposure, false)?;
                d.status = PalwSourceVerifierDutyStatusV2::Released { block: ctx.block, daa: ctx.daa_score };
                put(b, &liability_key_v2(&actor, work), &d)?;
            }
            if !matches!(d.entitlement, PalwSourceVerifierEntitlementV2::Held) {
                continue;
            }
            if !eligible {
                b.cancel_source_income_v1(ctx, income_context(&g, d.funding), d.id(), actor, d.accepted_daa)?;
                d.entitlement = PalwSourceVerifierEntitlementV2::Forfeited { block: ctx.block };
                put(b, &liability_key_v2(&actor, work), &d)?;
                continue;
            }
            let payload = b.state.bond(&actor).ok_or(PalwStateV2Error::MissingBond(actor))?.payout_payload;
            if !queue_room(b, payload, d.fee) {
                pending = true;
                continue;
            }
            let income = super::palw_bond_budget_fold_v1::source_income_id_v1(income_context(&g, d.funding), d.id(), &actor);
            b.bind_material_income_root_v1(ctx.daa_score, d.claim, income)?;
            b.settle_source_income_v1(ctx, income_context(&g, d.funding), d.id(), actor, d.accepted_daa, d.until_daa, d.fee)?;
            let q = queue_credit(b, ctx, &f, actor, Some(d.id()), d.fee)?;
            d.entitlement = PalwSourceVerifierEntitlementV2::Queued { block: q.queued_block, queue: q.queue };
            f.held = f.held.checked_sub(d.fee).ok_or_else(|| bad("source checker fee underflow"))?;
            f.queued = f.queued.checked_add(d.fee).ok_or_else(|| bad("source checker cumulative queue overflow"))?;
            put(b, &liability_key_v2(&actor, work), &d)?;
        }
        if pending {
            put(b, &key_v2(1, work), &f)?;
            break;
        }
        if f.held > 0 {
            let amount = f.held;
            let payload = b.state.bond(&f.terms.sponsor).ok_or(PalwStateV2Error::MissingBond(f.terms.sponsor))?.payout_payload;
            if !queue_room(b, payload, amount) {
                put(b, &key_v2(1, work), &f)?;
                break;
            }
            let q = queue_credit(b, ctx, &f, f.terms.sponsor, None, amount)?;
            f.held = 0;
            f.refunded = f.refunded.checked_add(amount).ok_or_else(|| bad("source principal refund overflow"))?;
            f.refund_queue = Some(q.queue);
            f.refund_queued_block = Some(ctx.block);
        }
        f.closed = true;
        if !f.conserves() {
            return Err(bad("source escrow conservation at release"));
        }
        put(b, &key_v2(1, work), &f)?;
        write::<u8, 73>(b, &horizon, None)?;
    }
    Ok(())
}
/// Runs immediately after the COMMON deterministic prefix drain, even when R
/// is absent (a funded work with no approvers still has principal to refund).
pub(super) fn acknowledge_consumed_prefix(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if b.params.selected_source_economics_v2.as_ref().is_none_or(|(at, _)| ctx.daa_score < *at) || b.state.kernel_route().is_none() {
        return Ok(());
    }
    if first_v2::<PalwSourceVerifierQueueV2, 129>(route(b)?, &[8], None).map_err(native)?.is_none() {
        return Ok(());
    }
    let count = b.entries.iter().filter(|e| matches!(e, PalwDeltaEntryV2::Payout { old: Some(_), new: None, .. })).count();
    if count == 0 {
        return Ok(());
    }
    let _workspace = reserve_native_requests_v1(
        (count as u64)
            .checked_mul(std::mem::size_of::<(Hash64, PalwPayoutV2)>() as u64)
            .ok_or_else(|| PalwStateV2Error::NativeResource(C::overflow::<PalwPayoutV2>("source consumed prefix")))?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let mut drained = Vec::new();
    drained
        .try_reserve_exact(count)
        .map_err(|why| PalwStateV2Error::NativeResource(C::unsupported::<PalwPayoutV2>("source consumed prefix", &why.to_string())))?;
    drained.extend(
        b.entries
            .iter()
            .filter_map(|e| if let PalwDeltaEntryV2::Payout { key, old: Some(p), new: None } = e { Some((*key, *p)) } else { None }),
    );
    for (queue, payout) in drained {
        let prefix = key_v2(8, queue);
        let mut cursor = None;
        let mut sum = 0u64;
        loop {
            let Some((key, q)) = first_v2::<PalwSourceVerifierQueueV2, 129>(route(b)?, &prefix, cursor.as_ref()).map_err(native)?
            else {
                break;
            };
            cursor = Some(key);
            if q.queue != queue
                || q.payload != payout.payload
                || q.amount == 0
                || q.queued_daa > ctx.daa_score
                || q.queued_block == ctx.block
                || key != pair_key(8, queue, q.id())
            {
                return Err(bad("source ACK substituted queue/payload/identity/clock"));
            }
            sum = sum.checked_add(q.amount).ok_or_else(|| bad("source queue ACK amount overflow"))?;
        }
        if sum > payout.amount {
            return Err(bad("source ACK exceeds actual consumed queue amount"));
        }
        loop {
            let Some((key, q)) = first_v2::<PalwSourceVerifierQueueV2, 129>(route(b)?, &prefix, None).map_err(native)? else { break };
            let mut f = *read_v2::<PalwSourceVerifierFundingV2, 65>(route(b)?, &key_v2(1, q.work))
                .map_err(native)?
                .ok_or_else(|| bad("source ACK funding absent"))?;
            if f.terms.id() != q.funding || q.queued_daa <= f.until_daa || !f.conserves() {
                return Err(bad("source ACK funding identity/conservation"));
            }
            if let Some(id) = q.duty {
                let mut d = *read_v2::<PalwSourceVerifierLiabilityV2, 133>(route(b)?, &liability_key_v2(&q.actor, q.work))
                    .map_err(native)?
                    .ok_or_else(|| bad("source ACK duty absent"))?;
                if d.id() != id
                    || d.funding != q.funding
                    || d.fee != q.amount
                    || !matches!(d.entitlement,PalwSourceVerifierEntitlementV2::Queued{block,queue:old} if block==q.queued_block&&old==queue)
                {
                    return Err(bad("source fee ACK is not its original queued entitlement"));
                }
                d.entitlement = PalwSourceVerifierEntitlementV2::QueueAcknowledged {
                    queued_block: q.queued_block,
                    queue,
                    acknowledged_block: ctx.block,
                    acknowledged_daa: ctx.daa_score,
                    amount: q.amount,
                };
                put(b, &liability_key_v2(&q.actor, q.work), &d)?;
            } else {
                if !f.closed
                    || f.terms.sponsor != q.actor
                    || f.refunded != q.amount
                    || f.refund_queue != Some(queue)
                    || f.refund_queued_block != Some(q.queued_block)
                    || f.refund_acknowledged_block.is_some()
                {
                    return Err(bad("source principal ACK is not its original refund"));
                }
                f.refund_acknowledged_block = Some(ctx.block);
                f.refund_acknowledged_daa = Some(ctx.daa_score);
                put(b, &key_v2(1, q.work), &f)?;
            }
            write::<PalwSourceVerifierQueueV2, 129>(b, &key, None)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "palw_state_v2/tests/source_verifier_economics_v2.rs"]
mod tests;
