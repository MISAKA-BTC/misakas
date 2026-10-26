use super::*;
use crate::palw_panel_lifetime_v2::{
    PALW_PANEL_LIFETIME_TABLE_V2, PANEL_AUTHORIZATION_PREFIX_V2, PalwPanelAuthorizationRowV2, PalwPanelConsentCaptureV2,
    PalwPanelLifetimeEnrollmentV2, PanelConsumerLifetimeTermsV2, PanelLifetimeAuthorizationV2, panel_authorization_key_v2,
    panel_capture_key_v2,
};

pub(super) fn authorize(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &PanelLifetimeAuthorizationV2,
) -> Result<(), PalwStateV2Error> {
    let refuse = |why: &str| PalwStateV2Error::KernelRouteRefused(why.into());
    let mirror = builder.params.panel_lifetime_v2_at(ctx.daa_score).ok_or_else(|| refuse("Panel lifetime fence absent"))?;
    terms.validate().map_err(refuse)?;
    mirror.policy.validate().map_err(refuse)?;
    let engine = builder.params.panel_v3().ok_or_else(|| refuse("compiled Panel engine policy absent"))?;
    let class_id = Hash64::from_bytes(terms.class);
    if builder.state.class(&class_id).is_none() {
        return Err(refuse("prospective Panel class is not registered"));
    }
    let court = builder.state.class_model_court_window_v1(&class_id).unwrap_or(builder.params.window_court());
    let tail = court
        .checked_mul(2)
        .and_then(|n| n.checked_add(builder.params.window_challenge()))
        .ok_or_else(|| refuse("Panel liability tail overflow"))?;
    if terms.network != engine.network.as_bytes()
        || terms.ruleset != engine.ruleset.as_bytes()
        || terms.producer != crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(signer)
        || terms.accept_by_daa < ctx.daa_score
        || ctx.daa_score.checked_add(mirror.policy.maximum_service_daa).is_none_or(|end| terms.service_until_daa > end)
        || terms.maximum_pause_daa > mirror.policy.maximum_pause_daa
        || terms.maximum_retries > mirror.policy.maximum_retries
        || terms.maximum_retries > u32::from(engine.policy.max_retries)
        || terms.service_until_daa.checked_add(tail).is_none_or(|end| terms.liability_until_daa < end)
    {
        return Err(refuse("prospective Panel authorization differs from its producer, class or finite policy"));
    }
    let key = panel_authorization_key_v2(&terms.id());
    let mut total = 0u32;
    let mut owned = 0u32;
    if let Some(route) = builder.state.kernel_route.as_ref() {
        if route.aux.contains_key(&(PALW_PANEL_LIFETIME_TABLE_V2, key.clone())) {
            return Err(refuse("Panel authorization already exists"));
        }
        for ((table, key), bytes) in &route.aux {
            if *table != PALW_PANEL_LIFETIME_TABLE_V2 || key.first() != Some(&PANEL_AUTHORIZATION_PREFIX_V2) || key.len() != 65 {
                continue;
            }
            let row: PalwPanelAuthorizationRowV2 =
                borsh::from_slice(bytes).map_err(|_| refuse("malformed Panel authorization inventory"))?;
            total = total.checked_add(1).ok_or_else(|| refuse("Panel lifetime count overflow"))?;
            if row.signer == *signer {
                owned = owned.checked_add(1).ok_or_else(|| refuse("Panel producer lifetime count overflow"))?;
            }
        }
    }
    if total >= mirror.policy.maximum_enrollments || owned >= mirror.policy.maximum_enrollments_per_producer {
        return Err(refuse("Panel prospective lifetime capacity is exhausted"));
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    let row = PalwPanelAuthorizationRowV2 {
        terms: *terms,
        signer: *signer,
        authorized_daa: ctx.daa_score,
        authorized_block: ctx.block,
        policy: mirror.policy.id(),
        consumed_claim: None,
    };
    builder.write_kernel_row(PALW_PANEL_LIFETIME_TABLE_V2, key, Some(borsh::to_vec(&row).unwrap()));
    Ok(())
}

/// Pure pre-write admission. A canonically ordered, unconsumed producer consent
/// must already exist; no exact claim/trace root is signed inside its own header.
pub(super) fn select_authorization(
    builder: &TransitionBuilder<'_>,
    claim: &PalwClaimStateV2,
) -> Result<Option<PalwPanelAuthorizationRowV2>, PalwStateV2Error> {
    let Some(mirror) = builder.params.panel_lifetime_v2_at(claim.accepted_daa) else { return Ok(None) };
    if !builder.params.panel_v3_rule_at(claim.accepted_daa) {
        return Ok(None);
    }
    if claim.work_id.is_none_or(|work| work == Hash64::default())
        || claim.phase.is_terminal()
        || builder.state.bond(&claim.bond).is_none()
    {
        return Err(PalwStateV2Error::KernelRouteRefused(
            "Panel consent cannot capture an unauthenticated immutable native claim".into(),
        ));
    }
    let route = builder
        .state
        .kernel_route
        .as_ref()
        .ok_or_else(|| PalwStateV2Error::KernelRouteRefused("Panel consent route missing".into()))?;
    let engine =
        builder.params.panel_v3().ok_or_else(|| PalwStateV2Error::KernelRouteRefused("Panel consent engine policy missing".into()))?;
    for (table, key) in route.aux.keys() {
        if *table != PALW_PANEL_LIFETIME_TABLE_V2 || key.len() != 65 || key.first() != Some(&PANEL_AUTHORIZATION_PREFIX_V2) {
            continue;
        }
        let id: [u8; 64] = key[1..].try_into().expect("authorization key is bounded");
        let auth = route.panel_authorization_v2(&id).map_err(PalwStateV2Error::KernelRouteRefused)?;
        let t = &auth.terms;
        if auth.policy == mirror.policy.id()
            && auth.signer == claim.bond
            && auth.consumed_claim.is_none()
            && t.network == engine.network.as_bytes()
            && t.ruleset == engine.ruleset.as_bytes()
            && t.class == claim.class_id.as_bytes()
            && claim.accepted_daa >= auth.authorized_daa
            && claim.accepted_daa >= t.accept_from_daa
            && claim.accepted_daa <= t.accept_by_daa
            && claim.pwu <= t.maximum_pwu
            && claim.reserved <= t.maximum_reserved_sompi
            && claim.escrowed_reward <= t.maximum_escrow_sompi
            && claim.rights_reserved <= t.maximum_rights_sompi
        {
            return Ok(Some(auth));
        }
    }
    Err(PalwStateV2Error::KernelRouteRefused("native Panel claim has no unconsumed prospective signed authorization".into()))
}

pub(super) fn capture(
    builder: &mut TransitionBuilder<'_>,
    id: Hash64,
    mut auth: PalwPanelAuthorizationRowV2,
) -> Result<(), PalwStateV2Error> {
    let admitted = crate::palw_permissionless_panel_v1::panel_admitted_claim_v1(&builder.state, id, 1)
        .map_err(|e| PalwStateV2Error::KernelRouteRefused(format!("Panel captured admission: {e}")))?;
    let claim = builder.state.claims.get(&id).ok_or(PalwStateV2Error::MissingClaim(id))?;
    let row = PalwPanelConsentCaptureV2 {
        authorization: auth.terms.id(),
        claim: id,
        accepted_daa: claim.accepted_daa,
        immutable_fields: admitted.immutable_fields,
    };
    let enrollment = PalwPanelLifetimeEnrollmentV2 {
        terms: PanelConsumerLifetimeTermsV2 {
            version: 2,
            authorization: row.authorization,
            network: auth.terms.network,
            ruleset: auth.terms.ruleset,
            claim: id.as_bytes(),
            class: claim.class_id.as_bytes(),
            work: admitted.work_id.as_bytes(),
            producer: auth.terms.producer,
            immutable_fields: admitted.immutable_fields.as_bytes(),
            accept_by_daa: auth.terms.accept_by_daa,
            service_until_daa: auth.terms.service_until_daa,
            liability_until_daa: auth.terms.liability_until_daa,
            maximum_retries: auth.terms.maximum_retries,
            maximum_pause_daa: auth.terms.maximum_pause_daa,
        },
        signer: auth.signer,
        enrolled_daa: claim.accepted_daa,
        enrolled_block: claim.accepted_block,
        policy: auth.policy,
        pause_started_daa: None,
        total_paused_daa: 0,
        authority_kind: 0,
    };
    auth.consumed_claim = Some(id);
    builder.write_kernel_row(PALW_PANEL_LIFETIME_TABLE_V2, panel_capture_key_v2(&id), Some(borsh::to_vec(&row).unwrap()));
    builder.write_kernel_row(
        PALW_PANEL_LIFETIME_TABLE_V2,
        panel_authorization_key_v2(&auth.terms.id()),
        Some(borsh::to_vec(&auth).unwrap()),
    );
    builder.write_kernel_row(
        PALW_PANEL_LIFETIME_TABLE_V2,
        borsh::to_vec(&id.as_bytes()).unwrap(),
        Some(borsh::to_vec(&enrollment).unwrap()),
    );
    Ok(())
}

/// Optional exact re-affirmation after native execution. It cannot change the
/// already captured deadline, counters, seed or monetary terms.
pub(super) fn enroll(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    terms: &PanelConsumerLifetimeTermsV2,
) -> Result<(), PalwStateV2Error> {
    let refuse = |why: &str| PalwStateV2Error::KernelRouteRefused(why.into());
    let mirror = builder.params.panel_lifetime_v2_at(ctx.daa_score).ok_or_else(|| refuse("Panel lifetime fence absent"))?;
    terms.validate().map_err(refuse)?;
    let id = Hash64::from_bytes(terms.claim);
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refuse("Panel lifetime route absent"))?;
    let mut row = route
        .panel_lifetime_enrollment_v2(&id)
        .map_err(|e| refuse(&e))?
        .ok_or_else(|| refuse("native Panel consent was not captured at admission"))?;
    let engine = builder.state.panel_v3.as_ref().ok_or_else(|| refuse("Panel engine absent"))?;
    if row.terms != *terms
        || row.signer != *signer
        || row.policy != mirror.policy.id()
        || ctx.daa_score > terms.service_until_daa
        || engine.claim(&id).is_none_or(|record| record.phase != misaka_palw_panel::ClaimPhaseV3::PendingSeal)
    {
        return Err(refuse("exact Panel attachment is substituted, retrospective or after pre-entropy sealing"));
    }
    if row.authority_kind == 1 {
        return Ok(());
    }
    row.authority_kind = 1;
    builder.write_kernel_row(PALW_PANEL_LIFETIME_TABLE_V2, borsh::to_vec(&terms.claim).unwrap(), Some(borsh::to_vec(&row).unwrap()));
    Ok(())
}

/// Both pre-object and post-object calls are intentional. A pause opened or
/// closed by this block starts/ends at this block's real DAA; calling again at
/// the same DAA cannot credit another interval. Sparse DAA charges the whole
/// elapsed interval. These counters are rooted and reverse with the journal.
pub(super) fn observe_pauses(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(mirror) = builder.params.panel_lifetime_v2_at(ctx.daa_score) else { return Ok(()) };
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let rows = route
        .aux
        .iter()
        .filter(|((table, key), _)| *table == PALW_PANEL_LIFETIME_TABLE_V2 && key.len() == 64)
        .map(|((_, key), bytes)| {
            borsh::from_slice::<PalwPanelLifetimeEnrollmentV2>(bytes)
                .map(|row| (key.clone(), row))
                .map_err(|e| PalwStateV2Error::KernelRouteRefused(format!("Panel pause row: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if rows.len() > mirror.policy.maximum_enrollments as usize {
        return Err(PalwStateV2Error::KernelRouteRefused("Panel lifetime inventory exceeds its authenticated bound".into()));
    }
    for (key, mut row) in rows {
        let claim = Hash64::from_bytes(row.terms.claim);
        let paused = builder.state.palw_accusation_pending_v1(&claim);
        let changed = match (row.pause_started_daa, paused) {
            (None, true) => {
                row.pause_started_daa = Some(ctx.daa_score);
                true
            }
            (Some(start), false) => {
                let duration = ctx
                    .daa_score
                    .checked_sub(start)
                    .ok_or_else(|| PalwStateV2Error::KernelRouteRefused("Panel pause counter has a future start".into()))?;
                row.total_paused_daa = row
                    .total_paused_daa
                    .checked_add(duration)
                    .ok_or_else(|| PalwStateV2Error::KernelRouteRefused("Panel pause lifetime counter overflow".into()))?;
                row.pause_started_daa = None;
                true
            }
            _ => false,
        };
        if changed {
            builder.write_kernel_row(PALW_PANEL_LIFETIME_TABLE_V2, key, Some(borsh::to_vec(&row).unwrap()));
        }
    }
    Ok(())
}
