//! Native signature and capital admission for prospective M*-49 watcher pools.
use super::*;
use crate::palw_verifier_watch_v1::*;
fn bad(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.to_string())
}

pub(super) fn apply(
    builder: &mut TransitionBuilder<'_>,
    point: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &VerifierWatchTermsV1,
) -> Result<(), PalwStateV2Error> {
    let checkpoint = builder.try_checkpoint()?;
    let result = admit(builder, point, signer, terms);
    if result.is_err() {
        builder.restore(checkpoint);
    }
    result
}
fn admit(
    builder: &mut TransitionBuilder<'_>,
    point: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &VerifierWatchTermsV1,
) -> Result<(), PalwStateV2Error> {
    let (from, policy) = builder
        .params
        .verifier_watch_v1
        .as_ref()
        .filter(|(from, _)| point.daa_score >= *from)
        .ok_or_else(|| bad("watcher requires explicit active pool/draw policy"))?;
    let _ = from;
    policy.validate_terms(terms, point.daa_score).map_err(bad)?;
    if terms.watcher != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer) {
        return Err(bad("watcher signature names another bond"));
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, point)?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| bad("watcher route missing"))?;
    if terms.network != route.header.policy.network_domain || terms.ruleset != route.header.policy.ruleset_digest {
        return Err(bad("watcher network/ruleset differs"));
    }
    if route.aux.contains_key(&(TABLE, key(WATCH, &terms.id()).to_vec())) {
        return Err(bad("watcher reservation replay"));
    }
    let ledger =
        route.ledger_checked_v1().map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, bad))?;
    if ledger.bounds_of(&terms.class).is_none() {
        return Err(bad("watcher target binding absent"));
    }
    let bond = builder.state.bonds.get(signer).ok_or_else(|| bad("watcher bond absent"))?;
    if !matches!(bond.status, PalwBondStatusV2::Active)
        || builder.gate_room(signer, point.daa_score, PalwRcoreGateV1::Work) < u128::from(terms.stake)
    {
        return Err(bad("watcher stake is not free native capital"));
    }
    drop(ledger);
    let id = Hash64::from_bytes(terms.id());
    let ask = crate::palw_bond_budget_v1::PalwBudgetAskV1 {
        block_units: 0,
        reward_sompi: 0,
        final_weight: 0,
        liability_sompi: u128::from(terms.stake),
    };
    let plan = builder
        .plan_bond_budget_v1(point, *signer, None, ask, crate::palw_bond_budget_v1::PalwBudgetOriginV1::FreePrompt, 0)?
        .ok_or_else(|| bad("watcher requires native budget reservation"))?;
    builder.commit_bond_budget_v1(point, id, plan)?;
    let row = PalwVerifierWatchRowV1 { terms: *terms, accepted_daa: point.daa_score, reservation: id };
    builder.write_kernel_row_encoded_v1(TABLE, &key(WATCH, &terms.id()), Some(&row))?;
    let mut expiry = [0u8; 73];
    expiry[0] = EXPIRY;
    expiry[1..9].copy_from_slice(&terms.lock_until_daa.to_be_bytes());
    expiry[9..].copy_from_slice(&terms.id());
    builder.write_kernel_row_encoded_v1(TABLE, &expiry, Some(&1u8))?;
    Ok(())
}

pub(super) fn sweep(builder: &mut TransitionBuilder<'_>, point: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !builder.params.verifier_watch_v1.as_ref().is_some_and(|(at, _)| point.daa_score >= *at) {
        return Ok(());
    }
    builder.state.assert_verifier_watch_consistency_v1(builder.params)?;
    let Some(route) = builder.state.kernel_route.as_ref() else {
        return Ok(());
    };
    let count = route
        .aux
        .keys()
        .filter(|(t, k)| {
            *t == TABLE && k.len() == 73 && k[0] == EXPIRY && u64::from_be_bytes(k[1..9].try_into().unwrap()) < point.daa_score
        })
        .count();
    let mut quote = native_writers::NativeWriteQuoteV1::new("watcher reservation expiry keys");
    quote.exact::<[u8; 73]>(count as u64)?;
    builder.admit_native_write_v1(quote)?;
    let keys: Vec<[u8; 73]> = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .keys()
        .filter(|(t, k)| {
            *t == TABLE && k.len() == 73 && k[0] == EXPIRY && u64::from_be_bytes(k[1..9].try_into().unwrap()) < point.daa_score
        })
        .map(|(_, k)| k.as_slice().try_into().unwrap())
        .collect();
    for expiry in keys {
        let id: [u8; 64] = expiry[9..].try_into().unwrap();
        builder
            .bond_budget_op(super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |budget, _, j| {
                budget.forget(&Hash64::from_bytes(id), j)
            })?
            .ok_or_else(|| bad("watcher expiry budget absent"))?;
        builder.write_kernel_row_encoded_v1::<PalwVerifierWatchRowV1>(TABLE, &key(WATCH, &id), None)?;
        builder.write_kernel_row_encoded_v1::<u8>(TABLE, &expiry, None)?;
    }
    Ok(())
}
