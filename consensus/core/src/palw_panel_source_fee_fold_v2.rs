//! Native-owned Panel permit: freeze authority and fund in one journaled object.
use super::*;
use crate::palw_panel_source_fee_v2::{
    PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, PalwPanelSourceFeeAuthorityV2, panel_source_target_v2,
};
use misaka_palw_kernel::beacon_source_fee::{BeaconSourceFeeTargetV1, BeaconSourceFeeTermsV1};

pub(super) fn fund_panel_sources_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    epoch: u64,
    terms: &BeaconSourceFeeTermsV1,
) -> Result<(), PalwStateV2Error> {
    let refuse = |s: &str| PalwStateV2Error::KernelRouteRefused(s.to_string());
    let extras = builder.extras.kernel_route.as_ref().ok_or_else(|| refuse("Panel sponsor route missing"))?;
    let source =
        extras.beacon_source.filter(|f| f.activation.is_active(ctx.daa_score)).ok_or_else(|| refuse("Panel source fence missing"))?;
    let fee = extras
        .beacon_source_fee
        .filter(|f| f.activation.is_active(ctx.daa_score))
        .ok_or_else(|| refuse("Panel monetary fence missing"))?;
    let engine = builder.state.panel_v3.as_ref().ok_or_else(|| refuse("Panel engine missing"))?;
    let panel = engine.policy();
    let inputs = builder.extras.panel_v3.as_ref().ok_or_else(|| refuse("Panel branch policy missing"))?;
    if !matches!(inputs.beacon_source, PalwPanelV3BeaconSourceV1::Chain) {
        return Err(refuse("Panel paid source target needs actual chain source authority"));
    }
    let challenge = inputs
        .approved_beacons
        .iter()
        .find(|p| p.id() == panel.beacon_scheme.as_bytes() && p.is_paid_opt_in_source())
        .ok_or_else(|| refuse("Panel epoch has no implemented paid source policy"))?
        .clone();
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("Panel route missing"))?;
    let snapshot = route.panel_source_snapshot_v1(epoch).map_err(PalwStateV2Error::KernelRouteRefused)?;
    let request = misaka_palw_panel::BeaconRequestV1 {
        network: snapshot.network,
        ruleset: snapshot.ruleset,
        scheme: snapshot.scheme,
        epoch,
        release_daa: snapshot.release_daa,
        deadline_daa: snapshot.deadline_daa,
    };
    if request.network != engine.network()
        || request.ruleset != engine.ruleset()
        || request.scheme != panel.beacon_scheme
        || snapshot.committed_daa < source.activation.daa_score().max(fee.activation.daa_score())
    {
        return Err(refuse("Panel permit refers to another engine or a pre-fence epoch"));
    }
    let context = snapshot.context(&request, &challenge).map_err(PalwStateV2Error::KernelRouteRefused)?;
    let tail = builder
        .params
        .window_court()
        .checked_mul(2)
        .and_then(|n| n.checked_add(builder.params.window_challenge()))
        .ok_or_else(|| refuse("Panel liability overflow"))?;
    let target = panel_source_target_v2(&source.policy, context, &request, &panel, tail, &route.header.policy)
        .map_err(PalwStateV2Error::KernelRouteRefused)?;
    // A sponsor cannot extend a consumer's service consent. Conversely, this
    // historical funding codec cannot silently enlarge its signed source hold.
    // New absolute consumer contracts must fit the source contract actually
    // signed and funded for this exact frozen epoch, including retry/pause tail.
    if builder.params.panel_lifetime_v2_at(ctx.daa_score).is_some() {
        for (id, claim) in &builder.state.claims {
            if claim.work_id.is_none_or(|work| !snapshot.assigned_work_ids.contains(&work.as_bytes())) {
                continue;
            }
            if let Some(consent) = crate::palw_panel_lifetime_v2::panel_lifetime_claim_v2(&builder.state, builder.params, id, claim)
                .map_err(PalwStateV2Error::KernelRouteRefused)?
                && consent.terms.liability_until_daa > target.terms.hold_until
            {
                return Err(refuse("the funded source hold does not cover its signed absolute Panel consumer liability"));
            }
        }
    }
    let key = borsh::to_vec(&target.terms.context).expect("context serializes");
    let cap = fee.policy.max_funded_sources.min(target.terms.max_total);
    let minimum = fee.policy.minimum_fee_for(&target).map_err(PalwStateV2Error::KernelRouteRefused)?;
    if terms.sponsor != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer)
        || terms.source != target.terms
        || terms.fee_per_source < minimum
        || terms.max_sources < challenge.work_count_k
        || terms.max_sources > cap
        || ctx.daa_score < snapshot.committed_daa
        || ctx.daa_score >= target.terms.seal_start
        || route.aux.contains_key(&(PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, key.clone()))
        || route.beacon_source_target_v4(&target.terms.context).is_ok()
    {
        return Err(refuse("Panel sponsor permit is late, substituted, unfunded or already consumed"));
    }
    let cost = fee.policy.cost_bounds.ok_or_else(|| refuse("Panel paid source cost bounds missing"))?;
    let authority = PalwPanelSourceFeeAuthorityV2 {
        version: 2,
        request: request.into(),
        snapshot_root: misaka_palw_challenge::hash::object_id(b"MISAKA/PALW/PANEL-SOURCE-SNAPSHOT/V1", &snapshot),
        challenge_policy: challenge,
        panel_policy: panel,
        source_policy: source.policy,
        native_liability_tail_daa: tail,
        authorized_daa: ctx.daa_score,
        sponsor: *signer,
    };
    let fee_target =
        BeaconSourceFeeTargetV1 { target: target.clone(), sponsor: terms.sponsor, minimum_fee: minimum, max_sources: cap };
    let checkpoint = builder.try_checkpoint()?;
    for (table, bytes) in [
        (crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4, borsh::to_vec(&target).unwrap()),
        (crate::palw_beacon_source_fee_v1::PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5, borsh::to_vec(&fee_target).unwrap()),
        (crate::palw_beacon_source_fee_v1::PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5, borsh::to_vec(&cost).unwrap()),
        (PALW_PANEL_SOURCE_FEE_AUTHORITY_TABLE_V2, borsh::to_vec(&authority).unwrap()),
    ] {
        builder.write_kernel_row(table, key.clone(), Some(bytes))?;
    }
    let object = misaka_palw_kernel::route::KernelRouteObjectV1::FundBeaconSourceFee { terms: *terms };
    let result = super::palw_kernel_route_fold_v1::apply_kernel_route_object_v1(builder, ctx, &object.encode(), signer);
    let funded = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|route| route.ledger().ok())
        .and_then(|ledger| ledger.beacon_source_fees.get(&terms.source.context).cloned())
        .is_some_and(|f| f.terms == *terms);
    if result.is_err() || !funded {
        builder.restore(checkpoint);
        return result;
    }
    Ok(())
}
