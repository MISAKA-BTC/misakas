//! Journaled enrollment, replacement, court and finality release. No operation
//! cancels an earned monetary right or edits an old absolute provider lease.
use super::*;
use crate::palw_monetary_material_v1::*;

fn refused(s: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::ExecV2Refused(s.into())
}
fn policy(builder: &TransitionBuilder<'_>, now: u64) -> Result<PalwMonetaryMaterialPolicyV1, PalwStateV2Error> {
    let p =
        builder.params.monetary_material_at_v1(now).ok_or_else(|| refused("monetary material enrollment gate is dormant"))?.clone();
    p.validate().map_err(refused)?;
    let w = builder.params.bond_budget().ok_or_else(|| refused("monetary material needs the monetary ledger"))?.policy.window_daa;
    if w != p.window_daa {
        return Err(refused("signed material window differs from the physical monetary window"));
    }
    Ok(p)
}
fn write(builder: &mut TransitionBuilder<'_>, root: Hash64, row: Option<&PalwMonetaryMaterialRowV1>) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row_encoded_v1(PALW_MONETARY_MATERIAL_TABLE_V1, root.as_byte_slice(), row)
}
fn active(builder: &TransitionBuilder<'_>, key: &PalwBondKeyV2) -> Result<PalwBondStateV2, PalwStateV2Error> {
    builder
        .state
        .bond(key)
        .filter(|b| matches!(b.status, PalwBondStatusV2::Active))
        .cloned()
        .ok_or_else(|| refused("monetary covenant signer is not an active bond"))
}

/// Verify all terms/capacity/capital before the first write. The native acceptance
/// layer independently verifies every ML-DSA signature on this exact preimage.
fn check_enrollment(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    e: &PalwMonetaryMaterialEnrollmentV1,
    prior: Option<&PalwMonetaryMaterialRowV1>,
) -> Result<(), PalwStateV2Error> {
    let p = policy(builder, ctx.daa_score)?;
    let t = &e.terms;
    if t.version != 1
        || t.root == Hash64::default()
        || t.nonce == Hash64::default()
        || t.policy != p.id()
        || t.window_daa != p.window_daa
        || !t.payment_relative_until_finality
        || (prior.is_none()
            && (t.enrollment_until_daa < ctx.daa_score
                || ctx.daa_score.checked_add(p.max_enrollment_daa).is_none_or(|limit| t.enrollment_until_daa > limit)))
        || t.max_material_bytes == 0
        || t.max_material_bytes > p.max_bytes_per_contract
        || t.dependencies.len() > PALW_MONETARY_MATERIAL_MAX_DEPENDENCIES_V1
        || t.dependencies.windows(2).any(|pair| pair[0] >= pair[1])
        || t.dependencies.contains(&t.root)
        || e.offers.len() < 2
        || e.offers.len() > PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
        || e.offers.windows(2).any(|pair| pair[0].provider >= pair[1].provider)
    {
        return Err(refused("monetary material terms are substituted, unsigned-risk, unordered or outside capacity"));
    }
    if let Some(session) = &t.native_session {
        if session.version != 3
            || session.job_policy != crate::palw_attempt_material_v3::native_job_policy_v3()
            || session.one_claim_cap != 1
            || session.admit_from_daa > session.admit_until_daa
            || session.admit_until_daa > t.enrollment_until_daa
            || session.max_units == 0
            || session.max_units as usize > crate::palw_material_capacity_v1::PALW_MATERIAL_MAX_UNITS_V1
            || t.material_manifest.is_some()
            || t.exec_plan_root.is_some()
            || !t.dependencies.is_empty()
        {
            return Err(refused("prospective native session policy/capacity is substituted"));
        }
        // The canonical session of an executor is the smallest-root eligible row naming it (`select_native_session_v3`), and
        // acceptance verifies only the sponsor's and the providers' signatures. A session is therefore the EXECUTOR's own
        // consent: its sponsor must be the executor, whose signature covers the whole payload. Otherwise any bond with two
        // bonded providers could enrol a smaller root naming another executor, withhold the Ready signatures and block that
        // executor's native mining (audit fix-cd). Dormant fence (`palw_monetary_material_v1` is armed nowhere), so no
        // accepted enrollment is reinterpreted.
        if t.sponsor != session.executor {
            return Err(refused("a prospective native session must be sponsored (consented) by the executor it names"));
        }
        let class = builder.state.class(&session.class_id).ok_or_else(|| refused("prospective native class absent"))?;
        let executor = active(builder, &session.executor)?;
        if class.artifact_root != session.artifact_root || executor.operator_id != session.operator {
            return Err(refused("prospective class/operator binding differs"));
        }
    }
    let route = builder.state.kernel_route();
    if let Some(manifest) = &t.material_manifest {
        if manifest.root != t.root {
            return Err(refused("signed material manifest belongs to another obligation"));
        }
        manifest.validate(t.max_material_bytes).map_err(|e| refused(e.to_string()))?;
        if manifest.scopes.iter().any(|s| s.codec == crate::palw_material_capacity_v1::PalwMaterialCodecV1::TypedKernelV4)
            && builder.params.material_release_recovery_at_v2(ctx.daa_score).is_none()
        {
            return Err(refused("typed complete material codec is below its signed recovery fence"));
        }
        if t.exec_plan_root.is_none()
            && manifest.scopes.iter().any(|scope| scope.claim != t.root && !t.dependencies.contains(&scope.claim))
        {
            return Err(refused("material manifest scope was not signed as a dependency"));
        }
    }
    for dependency in &t.dependencies {
        if route.and_then(|r| r.kernel_claim_row_v1(dependency)).is_none() {
            return Err(refused("monetary material dependency has no accepted claim"));
        }
    }
    let existing = builder.state.monetary_material_rows_v1();
    if prior.is_none() && existing.len() >= p.max_contracts as usize {
        return Err(refused("monetary material contract table is full"));
    }
    if let Some(old) = prior
        && (old.terms != *t
            || old.providers.iter().filter(|p| !p.charged || !p.fee_paid).count() + e.offers.len()
                > PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
            || e.offers.iter().any(|o| {
                old.providers.iter().any(|previous| previous.provider == o.provider && (!previous.charged || !previous.fee_paid))
            }))
    {
        return Err(refused("replacement changes the accepted obligation or reuses an old provider"));
    }
    let sponsor = active(builder, &t.sponsor)?;
    let mut operators = BTreeSet::new();
    let mut fees = 0u64;
    for offer in &e.offers {
        let provider = active(builder, &offer.provider)?;
        if offer.collateral_sompi < p.minimum_collateral_sompi
            || offer.prepaid_fee_sompi < p.minimum_fee_sompi
            || provider.operator_id == sponsor.operator_id
            || t.native_session.as_ref().is_some_and(|session| session.operator == provider.operator_id)
            || !operators.insert(provider.operator_id)
            || t.dependencies.iter().any(|id| {
                route
                    .and_then(|route| route.kernel_claim_row_v1(id).and_then(|r| route.bond_key_of(&r.producer)))
                    .and_then(|b| builder.state.bond(&b))
                    .is_some_and(|b| b.operator_id == provider.operator_id)
            })
        {
            return Err(refused("covenant needs two independent funded non-producer providers"));
        }
        let held: Vec<_> =
            existing.iter().filter(|r| r.providers.iter().any(|v| v.provider == offer.provider && !v.charged)).collect();
        let bytes = held
            .iter()
            .try_fold(t.max_material_bytes, |sum, r| sum.checked_add(r.terms.max_material_bytes))
            .ok_or_else(|| refused("provider material capacity overflows"))?;
        if held.len() >= p.max_contracts_per_provider as usize
            || bytes > p.max_bytes_per_provider
            || u128::from(provider.collateral).saturating_sub(builder.committed_at(&offer.provider, ctx.daa_score))
                < u128::from(offer.collateral_sompi)
        {
            return Err(refused("provider has no unused bytes/contracts/collateral capacity"));
        }
        fees = fees.checked_add(offer.prepaid_fee_sompi).ok_or_else(|| refused("material prepaid fee overflows"))?;
    }
    if u128::from(sponsor.collateral).saturating_sub(builder.committed_at(&t.sponsor, ctx.daa_score)) < u128::from(fees) {
        return Err(refused("material service must be prepaid from sponsor-owned free collateral"));
    }
    sponsor.slashed.checked_add(fees).ok_or_else(|| refused("material escrow cumulative debit overflows"))?;
    let mirror = builder.params.bond_budget().ok_or_else(|| refused("material fee budget policy is missing"))?;
    let mut monetary = builder.state.bond_budget.clone().ok_or_else(|| refused("material fee monetary engine is missing"))?;
    for offer in &e.offers {
        let id = super::palw_bond_budget_fold_v1::source_income_id_v1(e.id(), t.root, &offer.provider);
        let capital = builder.state.bond(&offer.provider).unwrap().collateral;
        let mut journal = Vec::new();
        monetary
            .reserve_source_income_v1(
                &mirror.policy,
                capital,
                id,
                offer.provider,
                ctx.daa_score,
                ctx.daa_score,
                offer.prepaid_fee_sompi,
                &mut journal,
            )
            .map_err(|e| refused(e.to_string()))?;
        monetary.bind_material_income_v1(t.root, id, &mut journal).map_err(|e| refused(e.to_string()))?;
    }
    Ok(())
}
fn fund(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    e: &PalwMonetaryMaterialEnrollmentV1,
) -> Result<(), PalwStateV2Error> {
    let fees: u64 = e.offers.iter().map(|o| o.prepaid_fee_sompi).sum();
    // RFC-0008 once-only settlement (dormant): a consent's sponsor prepaid funding is debited once across every consumer (an
    // enrollment and any replacement path that would fund the same consent). A second funding is refused; nothing moves.
    {
        use crate::palw_settlement_once_v1::{
            PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
        };
        let sponsor = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&e.terms.sponsor);
        let once = builder.settle_once_liable_v1(
            ctx.daa_score,
            PalwSettlementObligationV1::of(PalwSettlementLegV1::MaterialFunding, e.id().as_bytes()),
            PalwSettlementConsumerV1::MonetaryMaterial,
            fees,
            PalwSettlementDebitV1::Prepaid { sponsor, amount: fees },
            // RFC08-2: a debit INTO the books from the sponsor; it pays no liable party.
            super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::NoLiableClaim("a sponsor's prepaid funding debit"),
        )?;
        if !once.pays() {
            return Err(refused("material consent funding was already settled"));
        }
    }
    // Same native backing path as a funded source escrow: debit once, then
    // settle the provider's separately reserved R through a physical coinbase.
    if builder.slash_bond(e.terms.sponsor, u128::from(fees))? != fees {
        return Err(refused("material escrow was not fully prepaid"));
    }
    for offer in &e.offers {
        builder.reserve_source_income_v1(ctx, e.id(), e.terms.root, offer.provider, ctx.daa_score, offer.prepaid_fee_sompi)?;
        let id = super::palw_bond_budget_fold_v1::source_income_id_v1(e.id(), e.terms.root, &offer.provider);
        builder
            .bond_budget_op(super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |b, _, j| {
                b.bind_material_income_v1(e.terms.root, id, j)
            })?
            .ok_or_else(|| refused("material fee provenance engine is missing"))?
            .map_err(|e| refused(e.to_string()))?;
    }
    Ok(())
}
fn providers(e: &PalwMonetaryMaterialEnrollmentV1, now: u64) -> Vec<PalwMonetaryMaterialProviderV1> {
    e.offers
        .iter()
        .map(|o| PalwMonetaryMaterialProviderV1 {
            provider: o.provider,
            collateral_sompi: o.collateral_sompi,
            prepaid_fee_sompi: o.prepaid_fee_sompi,
            charged: false,
            fee_income: super::palw_bond_budget_fold_v1::source_income_id_v1(e.id(), e.terms.root, &o.provider),
            fee_context: e.id(),
            fee_accepted_daa: now,
            fee_queued: false,
            fee_paid: false,
        })
        .collect()
}
fn append_service_fee_history(
    row: &mut PalwMonetaryMaterialRowV1,
    e: &PalwMonetaryMaterialEnrollmentV1,
) -> Result<(), PalwStateV2Error> {
    for offer in &e.offers {
        let income = super::palw_bond_budget_fold_v1::source_income_id_v1(e.id(), e.terms.root, &offer.provider);
        row.service_fee_count = row.service_fee_count.checked_add(1).ok_or_else(|| refused("service fee history count overflow"))?;
        row.service_fee_total_sompi = row
            .service_fee_total_sompi
            .checked_add(u128::from(offer.prepaid_fee_sompi))
            .ok_or_else(|| refused("cumulative service fee backing overflows"))?;
        row.service_fee_history_root =
            monetary_service_fee_history_next_v1(row.service_fee_history_root, income, offer.prepaid_fee_sompi);
    }
    Ok(())
}
pub(super) fn enroll(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    e: &PalwMonetaryMaterialEnrollmentV1,
) -> Result<(), PalwStateV2Error> {
    if builder.state.material_recovery_row_v2(e.terms.root).is_some() {
        return Err(refused("root still has a reserved recovery receipt"));
    }
    if builder.state.monetary_material_row_v1(e.terms.root).is_some() {
        return Err(refused("root is already enrolled; use signed replacement"));
    }
    check_enrollment(builder, ctx, e, None)?;
    let mut row = PalwMonetaryMaterialRowV1 {
        terms: e.terms.clone(),
        consent: e.id(),
        sequence: 0,
        accepted_daa: ctx.daa_score,
        root_seen: false,
        native_binding: None,
        providers: providers(e, ctx.daa_score),
        challenges: Vec::new(),
        last_tail_until: None,
        tail_block: None,
        tail_blue_score: None,
        last_payment: None,
        payment_history_root: Hash64::default(),
        service_fee_count: 0,
        service_fee_total_sompi: 0,
        service_fee_paid_sompi: 0,
        service_fee_history_root: Hash64::default(),
        queued_entitlements: BTreeMap::new(),
    };
    append_service_fee_history(&mut row, e)?;
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    write(builder, e.terms.root, Some(&row))?;
    fund(builder, ctx, e)?;
    Ok(())
}
pub(super) fn replace(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    r: &PalwMonetaryMaterialReplacementV1,
) -> Result<(), PalwStateV2Error> {
    if builder.state.material_recovery_row_v2(r.root).is_some() {
        return Err(refused("recovery contract requires a newly signed RecoveryV2 replacement"));
    }
    replace_with_recovery_v2(builder, ctx, r)
}
pub(super) fn replace_with_recovery_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    r: &PalwMonetaryMaterialReplacementV1,
) -> Result<(), PalwStateV2Error> {
    replace_generation_with_recovery_v4(builder, ctx, r, r.id())
}
pub(super) fn replace_generation_with_recovery_v4(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    r: &PalwMonetaryMaterialReplacementV1,
    consent: Hash64,
) -> Result<(), PalwStateV2Error> {
    let mut old = builder.state.monetary_material_row_v1(r.root).ok_or_else(|| refused("replacement has no enrolled root"))?;
    if r.root != r.enrollment.terms.root || r.previous != old.consent || old.sequence.checked_add(1) != Some(r.sequence) {
        return Err(refused("replacement replay or wrong consent generation"));
    }
    check_enrollment(builder, ctx, &r.enrollment, Some(&old))?;
    append_service_fee_history(&mut old, &r.enrollment)?;

    old.providers.retain(|p| !p.charged || !p.fee_paid);
    old.providers.extend(providers(&r.enrollment, ctx.daa_score));
    old.sequence = r.sequence;
    old.consent = consent;
    write(builder, r.root, Some(&old))?;
    fund(builder, ctx, &r.enrollment)?;
    Ok(())
}

pub(super) fn action(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    a: &PalwMonetaryMaterialActionV1,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    action_inner_v4(builder, ctx, a, signer, false)
}
pub(super) fn action_with_recovery_answer_v4(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    a: &PalwMonetaryMaterialActionV1,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    action_inner_v4(builder, ctx, a, signer, true)
}
fn action_inner_v4(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    a: &PalwMonetaryMaterialActionV1,
    signer: &PalwBondKeyV2,
    recovery_answer: bool,
) -> Result<(), PalwStateV2Error> {
    if let PalwMonetaryMaterialActionV1::RecoveryV2(recovery) = a {
        return super::palw_material_release_recovery_fold_v2::action(builder, ctx, recovery, signer);
    }
    let p = policy(builder, ctx.daa_score)?;
    let root = match a {
        PalwMonetaryMaterialActionV1::RecoveryV2(_) => unreachable!("dispatched above"),
        PalwMonetaryMaterialActionV1::SegmentedChallenge { root, .. }
        | PalwMonetaryMaterialActionV1::SegmentedAnswer { root, .. }
        | PalwMonetaryMaterialActionV1::HeldChallenge { root, .. }
        | PalwMonetaryMaterialActionV1::HeldAnswer { root, .. }
        | PalwMonetaryMaterialActionV1::Challenge { root, .. }
        | PalwMonetaryMaterialActionV1::ChallengeCurrentV4 { root, .. }
        | PalwMonetaryMaterialActionV1::Answer { root, .. }
        | PalwMonetaryMaterialActionV1::Release { root, .. } => *root,
    };
    let mut row = builder.state.monetary_material_row_v1(root).ok_or_else(|| refused("no enrolled monetary material root"))?;
    let challenge_checkpoint = if let PalwMonetaryMaterialActionV1::ChallengeCurrentV4 { consent, provider, .. } = a {
        if builder.params.material_release_recovery_at_v2(ctx.daa_score).is_none() {
            return Err(refused("current generation challenge own fence dormant"));
        }
        let current = crate::palw_kernel_material_query_v4::material_provider_generation_v4(&builder.state, root, *provider)
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
        if current != Some(*consent) {
            return Err(refused("current challenge substitutes provider generation"));
        }
        use crate::palw_material_challenge_checkpoint_v4::{
            PalwMaterialChallengeCheckpointV4, material_challenge_checkpoint_from_route_v4,
        };
        let route = builder.state.kernel_route().ok_or_else(|| refused("challenge checkpoint route absent"))?;
        let previous = material_challenge_checkpoint_from_route_v4(route, *signer)
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
        let previous_history = crate::palw_material_challenge_history_v4::material_challenge_history_from_route_v4(route, *signer)
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
        let checkpoint = PalwMaterialChallengeCheckpointV4::next(
            previous,
            Hash64::from_bytes(route.header.policy.network_domain),
            *signer,
            a,
            ctx.daa_score,
        )
        .map_err(refused)?;
        let history = crate::palw_material_challenge_history_v4::PalwMaterialChallengeHistoryV4::next(
            Hash64::from_bytes(route.header.policy.network_domain),
            *signer,
            previous.as_ref(),
            previous_history.as_ref(),
            &checkpoint,
        )
        .map_err(refused)?;
        Some((checkpoint, history))
    } else {
        None
    };
    if !recovery_answer
        && builder.state.material_recovery_row_v2(root).is_some()
        && matches!(
            a,
            PalwMonetaryMaterialActionV1::HeldAnswer { .. }
                | PalwMonetaryMaterialActionV1::Answer { .. }
                | PalwMonetaryMaterialActionV1::SegmentedAnswer { .. }
        )
    {
        return Err(refused("new recovery contract answer requires atomic V4 applied checkpoint"));
    }
    match a {
        PalwMonetaryMaterialActionV1::RecoveryV2(_) => unreachable!("dispatched above"),
        PalwMonetaryMaterialActionV1::SegmentedChallenge { provider, dependency, position, part, valid_until_daa, .. } => {
            let challenger = active(builder, signer)?;
            let source = active(builder, provider)?;
            let deadline =
                ctx.daa_score.checked_add(p.response_window_daa).ok_or_else(|| refused("segmented material deadline overflows"))?;
            if challenger.operator_id == source.operator_id
                || *valid_until_daa < ctx.daa_score
                || *valid_until_daa > deadline
                || !builder.state.monetary_material_scope_v1(&row).contains(dependency)
                || row.challenges.len() >= crate::palw_provider_court_v1::PALW_PROVIDER_MAX_OPEN_CHALLENGES_V1
                || row.challenges.iter().any(|c| {
                    c.provider == *provider && c.dependency == *dependency && c.position == *position && c.segment_part == Some(*part)
                })
                || !row.providers.iter().any(|p| p.provider == *provider && !p.charged)
            {
                return Err(refused("segmented material challenge is foreign, stale, replayed or outside capacity"));
            }
            let ledger = builder
                .state
                .kernel_route()
                .unwrap()
                .ledger_native_projection_checked_v9()
                .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
            let claim = ledger.claims.get(&dependency.as_bytes()).ok_or_else(|| refused("segmented material claim is missing"))?;
            let misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { evidence, .. } = &claim.body else {
                return Err(refused("material unit is not a segmented claim"));
            };
            let class = ledger.classes.get(&claim.class_binding_id).ok_or_else(|| refused("segmented material class is missing"))?;
            if *position >= evidence.positions
                || misaka_palw_kernel::seg_da::position_part_bytes_v1(&class.program, *position)
                    .map_err(refused)?
                    .get(*part as usize)
                    .is_none()
            {
                return Err(refused("segmented unit is outside its canonical position/part layout"));
            }
            if u128::from(challenger.collateral).saturating_sub(builder.committed_at(signer, ctx.daa_score))
                < u128::from(crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1)
            {
                return Err(refused("segmented material challenger is unfunded"));
            }
            super::palw_material_release_recovery_fold_v2::reserve_answer_event_v4(builder, root, provider)?;
            row.challenges.push(PalwMonetaryMaterialChallengeV1 {
                provider: *provider,
                dependency: *dependency,
                stage: 0,
                position: *position,
                challenger: *signer,
                deadline_daa: deadline,
                answered: false,
                native_missing: None,
                native_binding: None,
                segment_part: Some(*part),
            });
        }
        PalwMonetaryMaterialActionV1::SegmentedAnswer { provider, dependency, position, part, bytes, .. } => {
            if provider != signer {
                return Err(refused("segmented answer is not its provider"));
            }
            let c = row
                .challenges
                .iter_mut()
                .find(|c| {
                    c.provider == *provider && c.dependency == *dependency && c.position == *position && c.segment_part == Some(*part)
                })
                .ok_or_else(|| refused("segmented answer has no exact part challenge"))?;
            if c.answered || ctx.daa_score > c.deadline_daa {
                return Err(refused("segmented answer is late or replayed"));
            }
            if bytes.len() as u64 > misaka_palw_kernel::seg_da::SEG_PART_BYTES_V4 || bytes.len() as u64 > row.terms.max_material_bytes
            {
                return Err(refused("segmented material answer exceeds its canonical/signed byte cap"));
            }
            if !super::palw_kernel_route_fold_v1::charge_route_budget_v1(builder, ctx, 0, false)? {
                return Err(refused("segmented answer adjudication budget is spent"));
            }
            let ledger = builder
                .state
                .kernel_route()
                .unwrap()
                .ledger_native_projection_checked_v9()
                .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
            let claim = ledger.claims.get(&dependency.as_bytes()).ok_or_else(|| refused("segmented material claim is missing"))?;
            let misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { evidence, segment_roots, .. } = &claim.body else {
                return Err(refused("material unit is not a segmented claim"));
            };
            let class = ledger.classes.get(&claim.class_binding_id).ok_or_else(|| refused("segmented material class is missing"))?;
            if misaka_palw_kernel::seg_da::classify_part_v1(&class.program, segment_roots, evidence.positions, *position, bytes)
                .map_err(refused)?
                != *part
            {
                return Err(refused("segmented answer authenticates another part"));
            }
            builder.slash_bond(c.challenger, u128::from(crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1))?;
            c.answered = true;
        }
        PalwMonetaryMaterialActionV1::HeldChallenge { provider, dependency, missing, binding, valid_until_daa, .. } => {
            let challenger = active(builder, signer)?;
            let source = active(builder, provider)?;
            let deadline = ctx
                .daa_score
                .checked_add(p.response_window_daa)
                .ok_or_else(|| refused("native material response deadline overflows"))?;
            if !builder.state.monetary_material_scope_v1(&row).contains(dependency)
                || row.terms.material_manifest.as_ref().and_then(|m| m.scope(*dependency)).is_none()
                || challenger.operator_id == source.operator_id
                || *valid_until_daa < ctx.daa_score
                || *valid_until_daa > deadline
                || row.challenges.len() >= crate::palw_provider_court_v1::PALW_PROVIDER_MAX_OPEN_CHALLENGES_V1
                || row
                    .challenges
                    .iter()
                    .any(|c| c.provider == *provider && c.dependency == *dependency && c.native_missing == Some(*missing))
                || !row.providers.iter().any(|v| v.provider == *provider && !v.charged)
            {
                return Err(refused("native material challenge is foreign, stale, replayed or outside capacity"));
            }
            let claim = builder.state.claim(dependency).ok_or(PalwStateV2Error::MissingClaim(*dependency))?;
            if borsh::to_vec(binding.as_ref()).unwrap().len() as u64 > row.terms.max_material_bytes {
                return Err(refused("native material binding exceeds signed capacity"));
            }
            crate::palw_held_da_v1::palw_held_da_check_accusation_v1(
                &claim.execution_root,
                missing,
                binding,
                builder.extras.prompt_ids_form_v1(),
            )
            .map_err(|e| refused(e.to_string()))?;
            if u128::from(challenger.collateral).saturating_sub(builder.committed_at(signer, ctx.daa_score))
                < u128::from(crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1)
            {
                return Err(refused("native material challenger is unfunded"));
            }
            super::palw_material_release_recovery_fold_v2::reserve_answer_event_v4(builder, root, provider)?;
            row.challenges.push(PalwMonetaryMaterialChallengeV1 {
                provider: *provider,
                dependency: *dependency,
                stage: 255,
                position: 0,
                challenger: *signer,
                deadline_daa: deadline,
                answered: false,
                native_missing: Some(*missing),
                native_binding: Some(binding.clone()),
                segment_part: None,
            });
        }
        PalwMonetaryMaterialActionV1::HeldAnswer { provider, dependency, missing, disclosure, .. } => {
            if provider != signer {
                return Err(refused("native material answer is not its provider"));
            }
            let c = row
                .challenges
                .iter_mut()
                .find(|c| c.provider == *provider && c.dependency == *dependency && c.native_missing == Some(*missing))
                .ok_or_else(|| refused("native material answer has no challenge"))?;
            if c.answered || ctx.daa_score > c.deadline_daa {
                return Err(refused("native material answer is late or replayed"));
            }
            if borsh::to_vec(disclosure.as_ref()).unwrap().len() as u64 > row.terms.max_material_bytes {
                return Err(refused("native material disclosure exceeds signed capacity"));
            }
            if !super::palw_kernel_route_fold_v1::charge_route_budget_v1(builder, ctx, 0, false)? {
                return Err(refused("native material answer adjudication budget is spent"));
            }
            let claim = builder.state.claim(dependency).ok_or(PalwStateV2Error::MissingClaim(*dependency))?;
            if let crate::palw_held_da_v1::PalwHeldDisclosureV1::StepLeaf { evidence } = disclosure.as_ref() {
                builder.state.require_implicit_reader_slot_v1(*dependency, evidence.refutation.output_preimage.coord.node_slot)?;
            }
            crate::palw_held_da_v1::palw_held_da_check_disclosure_v1(
                &claim.execution_root,
                missing,
                c.native_binding.as_ref().ok_or_else(|| refused("native material challenge lacks its binding"))?,
                disclosure,
                builder.state.class_step_ladder_v1(
                    &claim.class_id,
                    builder.extras.held_context_ladder.ok_or(PalwStateV2Error::HeldContextDormant)?,
                ),
                builder.extras.prompt_ids_form_v1(),
            )
            .map_err(|e| refused(e.to_string()))?;
            // Binding equality is insufficient for a leaf-evidence answer:
            // authenticate the output, operands, artifact and prompt using the
            // existing one-move verifier. This availability service records no
            // new computation verdict after the old computation court closed.
            if let crate::palw_held_da_v1::PalwHeldDisclosureV1::StepLeaf { evidence } = disclosure.as_ref() {
                let artifact_root =
                    builder.state.class(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.artifact_root;
                evidence
                    .verdict_at_v2(
                        &crate::palw_shard_court_v1::PalwOneMoveClaimV2 {
                            execution_root: claim.execution_root,
                            class_id: claim.class_id,
                            artifact_root,
                        },
                        builder.state.class_step_ladder_v1(
                            &claim.class_id,
                            builder.extras.held_context_ladder.ok_or(PalwStateV2Error::HeldContextDormant)?,
                        ),
                        true,
                    )
                    .map_err(|e| refused(e.to_string()))?;
            }
            builder.slash_bond(c.challenger, u128::from(crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1))?;
            c.answered = true;
        }
        PalwMonetaryMaterialActionV1::Challenge { provider, dependency, stage, position, valid_until_daa, .. }
        | PalwMonetaryMaterialActionV1::ChallengeCurrentV4 { provider, dependency, stage, position, valid_until_daa, .. } => {
            let challenger = active(builder, signer)?;
            let source = active(builder, provider)?;
            let deadline =
                ctx.daa_score.checked_add(p.response_window_daa).ok_or_else(|| refused("material response deadline overflow"))?;
            if challenger.operator_id == source.operator_id
                || *valid_until_daa < ctx.daa_score
                || *valid_until_daa > deadline
                || !builder.state.monetary_material_scope_v1(&row).contains(dependency)
                || !row.providers.iter().any(|v| v.provider == *provider && !v.charged)
                || row
                    .challenges
                    .iter()
                    .any(|c| c.provider == *provider && c.dependency == *dependency && c.stage == *stage && c.position == *position)
                || row.challenges.len() >= crate::palw_provider_court_v1::PALW_PROVIDER_MAX_OPEN_CHALLENGES_V1
            {
                return Err(refused("material challenge is replayed, foreign, stale or outside court capacity"));
            }
            let bond = crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1;
            if u128::from(challenger.collateral).saturating_sub(builder.committed_at(signer, ctx.daa_score)) < u128::from(bond) {
                return Err(refused("material challenger is unfunded"));
            }
            let claim = builder
                .state
                .kernel_route()
                .unwrap()
                .kernel_claim_row_v1(dependency)
                .ok_or_else(|| refused("material dependency is missing"))?;
            let scope = row.terms.material_manifest.as_ref().and_then(|m| m.scope(*dependency));
            let typed = scope.is_some_and(|s| s.codec == crate::palw_material_capacity_v1::PalwMaterialCodecV1::TypedKernelV4);
            if typed != matches!(a, PalwMonetaryMaterialActionV1::ChallengeCurrentV4 { .. }) {
                return Err(refused("complete typed scope requires explicitly bound current-generation challenge"));
            }
            let required = if typed {
                let ledger = builder
                    .state
                    .kernel_route()
                    .unwrap()
                    .ledger_native_projection_checked_v9()
                    .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
                crate::palw_prospective_kernel_material_v4::typed_kernel_required_unit_v4(
                    &ledger,
                    &dependency.as_bytes(),
                    *stage,
                    *position,
                )
                .map_err(refused)?
            } else {
                claim.body.position(*stage, *position).is_some()
            };
            if !required {
                return Err(refused("material challenge names no committed position"));
            }
            super::palw_material_release_recovery_fold_v2::reserve_answer_event_v4(builder, root, provider)?;
            row.challenges.push(PalwMonetaryMaterialChallengeV1 {
                provider: *provider,
                dependency: *dependency,
                stage: *stage,
                position: *position,
                challenger: *signer,
                deadline_daa: deadline,
                answered: false,
                native_missing: None,
                native_binding: None,
                segment_part: None,
            });
        }
        PalwMonetaryMaterialActionV1::Answer { provider, dependency, stage, position, bytes, .. } => {
            if provider != signer {
                return Err(refused("material answer is not its provider"));
            }
            let c = row
                .challenges
                .iter_mut()
                .find(|c| c.provider == *provider && c.dependency == *dependency && c.stage == *stage && c.position == *position)
                .ok_or_else(|| refused("material answer has no challenge"))?;
            if c.answered || ctx.daa_score > c.deadline_daa {
                return Err(refused("material answer is late or replayed"));
            }
            if !super::palw_kernel_route_fold_v1::charge_route_budget_v1(builder, ctx, 0, false)? {
                return Err(refused("material answer adjudication budget spent"));
            }
            if bytes.len() as u64 > row.terms.max_material_bytes {
                return Err(refused("material answer exceeds its signed capacity"));
            }
            let ledger = builder
                .state
                .kernel_route()
                .unwrap()
                .ledger_native_projection_checked_v9()
                .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
            if row
                .terms
                .material_manifest
                .as_ref()
                .and_then(|m| m.scope(*dependency))
                .is_some_and(|s| s.codec == crate::palw_material_capacity_v1::PalwMaterialCodecV1::TypedKernelV4)
            {
                crate::palw_prospective_kernel_material_v4::typed_kernel_classify_unit_v4(
                    &ledger,
                    &dependency.as_bytes(),
                    *stage,
                    *position,
                    bytes,
                )
                .map_err(refused)?;
            } else {
                ledger.classify_served_position_v1(&dependency.as_bytes(), *stage, *position, bytes).map_err(refused)?;
            }
            builder.slash_bond(c.challenger, u128::from(crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1))?;
            c.answered = true;
        }
        PalwMonetaryMaterialActionV1::Release { consent, tail_block, .. } => {
            // The native acceptance layer verifies ancestry/finality, and the fold
            // refuses a substituted or still-live tail as its independent lock.
            if *consent != row.consent
                || row.tail_block != Some(*tail_block)
                || !row.challenges.is_empty()
                || raw_hold(&builder.state, builder.params, &row, ctx.daa_score).is_some()
            {
                return Err(refused("material covenant has unspent rights or a substituted tail"));
            }
            super::palw_material_release_recovery_fold_v2::released(builder, ctx, root, *consent, *tail_block)?;
            write(builder, root, None)?;
            return Ok(());
        }
    }
    if let Some((checkpoint, history)) = challenge_checkpoint {
        builder.write_material_challenge_checkpoint_v4(*signer, Some(checkpoint))?;
        builder.write_material_challenge_history_v4(*signer, Some(history))?;
    }
    write(builder, root, Some(&row))?;
    Ok(())
}
fn raw_hold(state: &PalwChainStateV2, params: &PalwStateParamsV2, row: &PalwMonetaryMaterialRowV1, now: u64) -> Option<u64> {
    state
        .monetary_material_scope_v1(row)
        .into_iter()
        .filter_map(|root| state.material_income_hold_without_covenant_v1(params, root))
        .filter(|until| *until > now)
        .max()
}
/// This runs after all material/monetary changes in a block. Sparse DAA can
/// increase a real tail but cannot manufacture payment or discharge finality.
pub(super) fn tick(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    tick_inner(builder, ctx, true)
}
pub(super) fn refresh(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    tick_inner(builder, ctx, false)
}
fn tick_inner(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, queue_fees: bool) -> Result<(), PalwStateV2Error> {
    if builder.params.monetary_material_at_v1(ctx.daa_score).is_none() {
        return Ok(());
    }
    capture_queue_entitlements(builder)?;
    for mut row in builder.state.monetary_material_rows_v1() {
        let before = row.clone();
        if queue_fees {
            for provider in &mut row.providers {
                if provider.fee_queued || ctx.daa_score <= provider.fee_accepted_daa {
                    continue;
                }
                let payload =
                    builder.state.bond(&provider.provider).ok_or(PalwStateV2Error::MissingBond(provider.provider))?.payout_payload;
                let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
                let non_market = builder
                    .state
                    .pending_payouts
                    .keys()
                    .filter(|k| k.as_byte_slice()[0] != PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX)
                    .count();
                let market = builder
                    .state
                    .pending_payouts
                    .keys()
                    .filter(|k| k.as_byte_slice()[0] == PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX)
                    .count()
                    .min(2);
                if !builder.state.pending_payouts.contains_key(&queue)
                    && (non_market + market >= PALW_V2_MAX_PAYOUTS_PER_BLOCK
                        || builder.state.pending_payouts.len() >= PALW_V2_MAX_PENDING_PAYOUTS)
                {
                    continue;
                }
                builder
                    .state
                    .pending_payouts
                    .get(&queue)
                    .map_or(0, |p| p.amount)
                    .checked_add(provider.prepaid_fee_sompi)
                    .ok_or_else(|| refused("material fee queue aggregate overflows"))?;
                // RFC-0008 once-only settlement (dormant): one provider's prepaid fee of one consent is paid once across every
                // consumer; a fee another consumer settled is marked queued here without moving value again.
                let once = {
                    use crate::palw_settlement_once_v1::{
                        PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
                    };
                    builder.settle_once_liable_v1(
                        ctx.daa_score,
                        PalwSettlementObligationV1::with_party(
                            PalwSettlementLegV1::MaterialProviderFee,
                            provider.fee_context.as_bytes(),
                            crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&provider.provider),
                            0,
                        ),
                        PalwSettlementConsumerV1::MonetaryMaterial,
                        provider.prepaid_fee_sompi,
                        PalwSettlementDebitV1::Held { amount: provider.prepaid_fee_sompi },
                        // RFC08-2: the provider stands behind an event-relative covenant (service until every right terminated and
                        // the last payment + W), which has no finite liability horizon or signed hard deadline an at-once fee could
                        // be checked against — refused past the fence, never defaulted.
                        super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
                            "a material provider's fee: its covenant's liability horizon is event-relative (no finite horizon or hard deadline)",
                        ),
                    )?
                };
                if !once.pays() {
                    provider.fee_queued = true;
                    continue;
                }
                builder.settle_source_income_v1(
                    ctx,
                    provider.fee_context,
                    row.terms.root,
                    provider.provider,
                    provider.fee_accepted_daa,
                    provider.fee_accepted_daa,
                    provider.prepaid_fee_sompi,
                )?;
                builder.add_kernel_payout(payload, provider.prepaid_fee_sompi)?;
                provider.fee_queued = true;
            }
        }
        row.root_seen |= row.native_binding.is_some()
            || builder.state.claims.contains_key(&row.terms.root)
            || builder.state.kernel_route().and_then(|r| r.kernel_claim_row_v1(&row.terms.root)).is_some();
        let failed: Vec<_> =
            row.challenges.iter().filter(|c| !c.answered && ctx.daa_score > c.deadline_daa).map(|c| c.provider).collect();
        for challenge in
            row.challenges.iter().filter(|c| !c.answered && (ctx.daa_score > c.deadline_daa || failed.contains(&c.provider)))
        {
            super::palw_material_release_recovery_fold_v2::release_answer_event_v4(builder, row.terms.root, &challenge.provider)?;
        }
        for provider in failed {
            if let Some(p) = row.providers.iter_mut().find(|p| p.provider == provider && !p.charged) {
                // RFC-0008 once-only settlement (dormant): one provider's default on one root is charged once across consumers.
                let kid = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&provider);
                let once = {
                    use crate::palw_settlement_once_v1::{
                        PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
                    };
                    builder.settle_once_liable_v1(
                        ctx.daa_score,
                        PalwSettlementObligationV1::with_party(
                            PalwSettlementLegV1::MaterialProviderDefault,
                            row.terms.root.as_bytes(),
                            kid,
                            0,
                        ),
                        PalwSettlementConsumerV1::MonetaryMaterial,
                        p.collateral_sompi,
                        PalwSettlementDebitV1::Collateral { bond: kid, amount: p.collateral_sompi },
                        // RFC08-2: a charge on the defaulting provider's collateral; it pays no liable party.
                        super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::NoLiableClaim("a provider's default charge"),
                    )?
                };
                if once.pays() {
                    builder.slash_bond(provider, u128::from(p.collateral_sompi))?;
                }
                p.charged = true;
            }
            row.challenges.retain(|c| c.provider != provider);
        }
        row.challenges.retain(|c| ctx.daa_score <= c.deadline_daa);
        match raw_hold(&builder.state, builder.params, &row, ctx.daa_score) {
            Some(u64::MAX) => {
                row.tail_block = None;
                row.tail_blue_score = None;
            }
            Some(until) => {
                row.last_tail_until = row.last_tail_until.max(Some(until));
                row.tail_block = None;
                row.tail_blue_score = None;
            }
            None => {
                if row.tail_block.is_none()
                    && (row.root_seen || ctx.daa_score > row.terms.enrollment_until_daa)
                    && builder
                        .state
                        .claims
                        .get(&row.native_binding.as_ref().map(|b| b.claim).unwrap_or(row.terms.root))
                        .is_none_or(|c| c.phase.is_terminal())
                    && builder.state.kernel_route().and_then(|r| r.kernel_claim_row_v1(&row.terms.root)).is_none_or(|c| {
                        matches!(
                            c.life.state,
                            misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. }
                                | misaka_palw_kernel::lifecycle::ClaimStateV1::Unavailable { .. }
                                | misaka_palw_kernel::lifecycle::ClaimStateV1::Convicted { .. }
                                | misaka_palw_kernel::lifecycle::ClaimStateV1::TimedOut { .. }
                        )
                    })
                {
                    row.tail_block = Some(ctx.block);
                    row.tail_blue_score = Some(ctx.blue_score);
                }
            }
        }
        if row != before {
            write(builder, row.terms.root, Some(&row))?;
        }
    }
    Ok(())
}

/// Bind receipt authority before the monetary engine forgets a drained income.
pub(super) fn observe_payment(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    income: Hash64,
    queue: Hash64,
) -> Result<(), PalwStateV2Error> {
    if builder.params.monetary_material_at_v1(ctx.daa_score).is_none() {
        return Ok(());
    }
    let Some(coinbase_queue_amount) = builder.entries.iter().find_map(|entry| match entry {
        PalwDeltaEntryV2::Payout { key, old: Some(payout), new: None } if *key == queue => Some(payout.amount),
        _ => None,
    }) else {
        return Ok(());
    };
    let observations: Vec<_> = builder
        .state
        .bond_budget
        .as_ref()
        .into_iter()
        .flat_map(|b| b.material_income.iter())
        .filter_map(|(material_receipt_id, m)| match m.payment {
            crate::palw_bond_budget_v1::PalwMaterialPaymentV1::Queued { income: Some(id), paid_daa: None, .. } if id == income => {
                m.root.map(|root| PalwMonetaryMaterialPaymentObservationV1 {
                    root,
                    income,
                    queue,
                    amount: m.amount,
                    coinbase_queue_amount,
                    paid_block: ctx.block,
                    paid_daa: ctx.daa_score,
                    material_receipt_id: *material_receipt_id,
                    entitlement: None,
                })
            }
            _ => None,
        })
        .collect();
    for observation in observations {
        for mut row in builder.state.monetary_material_rows_v1() {
            if !builder.state.monetary_material_scope_v1(&row).contains(&observation.root) {
                continue;
            }
            let mut observation = observation.clone();
            observation.entitlement = row.queued_entitlements.remove(&observation.material_receipt_id);
            row.payment_history_root = Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                b"MISAKA/PALW/MONETARY-MATERIAL-PAYMENT-HISTORY/V1",
                &(row.payment_history_root, &observation),
            ));
            if let Some(entitlement) = observation.entitlement
                && let Some(provider) = row.providers.iter_mut().find(|p| p.fee_income == entitlement)
                && !provider.fee_paid
                && observation.amount == provider.prepaid_fee_sompi
            {
                provider.fee_paid = true;
                row.service_fee_paid_sompi = row.service_fee_paid_sompi.saturating_add(u128::from(observation.amount));
            }
            row.last_payment = Some(observation.clone());
            write(builder, row.terms.root, Some(&row))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_bond_budget_v1::*;
    use crate::tx::{TransactionId, TransactionOutpoint};
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }
    fn ctx(daa: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: Hash64::from_u64_word(daa), daa_score: daa, blue_score: daa, subsidy: 0 }
    }
    fn fixture() -> (PalwChainStateV2, PalwStateParamsV2, PalwMonetaryMaterialEnrollmentV1) {
        let p = PalwMonetaryMaterialPolicyV1 {
            version: 1,
            max_contracts: 10,
            max_contracts_per_provider: 4,
            max_bytes_per_contract: 1_000,
            max_bytes_per_provider: 4_000,
            minimum_collateral_sompi: 100,
            minimum_fee_sompi: 7,
            window_daa: 436,
            response_window_daa: 10,
            max_enrollment_daa: 100,
            cost_report_root: h(5),
        };
        let mut monetary = PalwBondBudgetPolicyV1::unapproved_probe_v1();
        monetary.window_daa = 436;
        let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_bond_budget(Some(PalwBondBudgetMirrorV1 { from_daa: 1, policy: monetary.clone(), allocation: None }))
            .with_reporter_income_from(Some(1))
            .with_material_income_from(Some(1))
            .with_monetary_material(Some(PalwMonetaryMaterialMirrorV1 { from_daa: 1, policy: p.clone() }));
        let mut state = PalwChainStateV2::genesis();
        for n in 1..=5 {
            state.bonds.insert(
                bond(n),
                PalwBondStateV2 {
                    pubkey: vec![n as u8],
                    operator_id: h(n as u8),
                    collateral: 10_000,
                    slashed: 0,
                    status: PalwBondStatusV2::Active,
                    registered_daa: 0,
                    payout_payload: h(n as u8),
                    capable_classes: BTreeSet::new(),
                },
            );
        }
        let mut book = PalwBondBudgetStateV1::new(1, &monetary);
        book.activate_material_income_v3(&mut Vec::new());
        state.bond_budget = Some(book);
        state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h(1), h(2)),
            None,
            misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
        ));
        let e = PalwMonetaryMaterialEnrollmentV1 {
            terms: PalwMonetaryMaterialTermsV1 {
                version: 1,
                root: h(9),
                dependencies: vec![],
                exec_plan_root: None,
                sponsor: bond(1),
                nonce: h(8),
                policy: p.id(),
                max_material_bytes: 100,
                material_manifest: None,
                native_session: None,
                window_daa: 436,
                enrollment_until_daa: 100,
                payment_relative_until_finality: true,
            },
            offers: vec![2, 3]
                .into_iter()
                .map(|n| PalwMonetaryMaterialOfferV1 {
                    provider: bond(n),
                    collateral_sompi: 100,
                    prepaid_fee_sompi: 7,
                    signature: vec![1],
                })
                .collect(),
        };
        (state, params, e)
    }
    #[test]
    fn applied_challenge_checkpoint_writer_reserves_before_mutation_and_journals_bond_cleanup() {
        use crate::palw_material_challenge_checkpoint_v4::{
            PalwMaterialChallengeCheckpointV4 as Checkpoint, material_challenge_checkpoint_from_route_v4 as read,
        };
        use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1};
        let (state, params, _) = fixture();
        use crate::palw_material_release_recovery_v2::{PalwMaterialReleaseRecoveryMirrorV2, PalwMaterialReleaseRecoveryPolicyV2};
        let params = params.with_material_release_recovery_v2(Some(PalwMaterialReleaseRecoveryMirrorV2 {
            from_daa: 1,
            policy: PalwMaterialReleaseRecoveryPolicyV2 {
                version: 2,
                max_reserved_contracts: 1,
                max_generations_per_contract: 2,
                max_provider_streams: 4,
                max_reserved_history_cells: 16,
                measurement_root: h(77),
            },
        }));
        let extras = PalwTransitionExtrasV1::default();
        let action = PalwMonetaryMaterialActionV1::ChallengeCurrentV4 {
            root: h(9),
            consent: h(7),
            provider: bond(2),
            dependency: h(9),
            stage: 255,
            position: 0,
            valid_until_daa: 100,
        };
        // Storage/owner fixture only; no signature, typed unit or native
        // admission success is inferred from direct calls to this writer.
        let checkpoint = Checkpoint::next(None, h(1), bond(4), &action, 2).unwrap();
        let mut refused = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
        refused.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
        assert!(matches!(
            refused.write_material_challenge_checkpoint_v4(bond(4), Some(checkpoint)),
            Err(PalwStateV2Error::NativeResource(_))
        ));
        assert!(refused.entries.is_empty());
        assert!(read(refused.state.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        assert_eq!(checker.outstanding(), (0, 0));
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_material_challenge_checkpoint_v4(bond(4), Some(checkpoint)).unwrap();
        assert_eq!(read(b.state.kernel_route().unwrap(), bond(4)).unwrap(), Some(checkpoint));
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let delta = PalwStateDeltaV2 { point: ctx(2), entries: b.entries.clone() };
        let applied = apply_delta_v2(&state, &delta, &params).unwrap();
        assert_eq!(read(applied.kernel_route().unwrap(), bond(4)).unwrap(), Some(checkpoint));
        let restored = revert_delta_v2(&applied, &delta, &params).unwrap();
        assert!(read(restored.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        b.write_bond(bond(4), None).unwrap();
        assert!(b.state.bond(&bond(4)).is_none());
        assert!(read(b.state.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let all = PalwStateDeltaV2 { point: ctx(2), entries: b.entries.clone() };
        let applied = apply_delta_v2(&state, &all, &params).unwrap();
        assert!(applied.bond(&bond(4)).is_none());
        let restored = revert_delta_v2(&applied, &all, &params).unwrap();
        assert!(restored.bond(&bond(4)).is_some());
        assert!(read(restored.kernel_route().unwrap(), bond(4)).unwrap().is_none());
    }
    /// Direct writer/journal controls only: these do not execute signed native
    /// challenge admission, a complete typed-unit check, or a node restart.
    #[test]
    fn applied_challenge_history_pair_reserves_before_mutation_and_cold_journal_rewinds_both_families() {
        use crate::{
            palw_material_challenge_checkpoint_v4::{
                PalwMaterialChallengeCheckpointV4 as Checkpoint, material_challenge_checkpoint_from_route_v4 as read_checkpoint,
            },
            palw_material_challenge_history_v4::{
                PalwMaterialChallengeHistoryV4 as History, material_challenge_history_from_route_v4 as read_history,
            },
            palw_material_release_recovery_v2::{PalwMaterialReleaseRecoveryMirrorV2, PalwMaterialReleaseRecoveryPolicyV2},
            palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1, NativeTransitionReservationV1},
        };
        let (state, params, _) = fixture();
        let params = params.with_material_release_recovery_v2(Some(PalwMaterialReleaseRecoveryMirrorV2 {
            from_daa: 1,
            // Storage fixture only, not approved production policy values.
            policy: PalwMaterialReleaseRecoveryPolicyV2 {
                version: 2,
                max_reserved_contracts: 1,
                max_generations_per_contract: 2,
                max_provider_streams: 4,
                max_reserved_history_cells: 16,
                measurement_root: h(77),
            },
        }));
        let action = PalwMonetaryMaterialActionV1::ChallengeCurrentV4 {
            root: h(9),
            consent: h(7),
            provider: bond(2),
            dependency: h(9),
            stage: 255,
            position: 0,
            valid_until_daa: 100,
        };
        let checkpoint = Checkpoint::next(None, h(1), bond(4), &action, 2).unwrap();
        let history = History::next(h(1), bond(4), None, None, &checkpoint).unwrap();
        let extras = PalwTransitionExtrasV1::default();
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_material_challenge_checkpoint_v4(bond(4), Some(checkpoint)).unwrap();
        // Preserve compatibility with a pre-A3 native A2 checkpoint. The new
        // history cell is optional until the next explicitly paired write.
        assert!(read_history(b.state.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let legacy_state = b.state.clone();

        let mut refused = TransitionBuilder::new(&legacy_state, &params, false, false, false, false, &extras);
        let before_state = refused.state.clone();
        let before_entries = refused.entries.clone();
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 });
        refused.native_reservation = NativeTransitionReservationV1::for_checker_v1(&checker);
        assert!(matches!(
            refused.write_material_challenge_history_v4(bond(4), Some(history)),
            Err(PalwStateV2Error::NativeResource(_))
        ));
        assert_eq!(refused.state, before_state);
        assert_eq!(refused.entries, before_entries);
        assert_eq!(checker.outstanding(), (0, 0));
        assert!(read_history(refused.state.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        assert_eq!(read_checkpoint(refused.state.kernel_route().unwrap(), bond(4)).unwrap(), Some(checkpoint));

        b.write_material_challenge_history_v4(bond(4), Some(history)).unwrap();
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let delta = PalwStateDeltaV2 { point: ctx(2), entries: b.entries.clone() };
        let applied = apply_delta_v2(&state, &delta, &params).unwrap();
        assert_eq!(read_checkpoint(applied.kernel_route().unwrap(), bond(4)).unwrap(), Some(checkpoint));
        assert_eq!(read_history(applied.kernel_route().unwrap(), bond(4)).unwrap(), Some(history));
        let mut cold = applied.clone();
        // Rebuild the serialized route rather than relying on a warm native
        // projection. This is a codec/consistency fixture, not OS crash evidence.
        cold.kernel_route = Some(borsh::from_slice(&borsh::to_vec(applied.kernel_route().unwrap()).unwrap()).unwrap());
        cold.assert_material_release_recovery_consistency_v2(&params).unwrap();
        assert_eq!(read_history(cold.kernel_route().unwrap(), bond(4)).unwrap(), Some(history));
        let restored = revert_delta_v2(&cold, &delta, &params).unwrap();
        assert!(read_checkpoint(restored.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        assert!(read_history(restored.kernel_route().unwrap(), bond(4)).unwrap().is_none());

        b.write_bond(bond(4), None).unwrap();
        assert!(b.state.bond(&bond(4)).is_none());
        assert!(read_checkpoint(b.state.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        assert!(read_history(b.state.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let all = PalwStateDeltaV2 { point: ctx(2), entries: b.entries.clone() };
        let removed = apply_delta_v2(&state, &all, &params).unwrap();
        assert!(removed.bond(&bond(4)).is_none());
        assert!(read_checkpoint(removed.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        assert!(read_history(removed.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        let restored = revert_delta_v2(&removed, &all, &params).unwrap();
        assert!(restored.bond(&bond(4)).is_some());
        assert!(read_checkpoint(restored.kernel_route().unwrap(), bond(4)).unwrap().is_none());
        assert!(read_history(restored.kernel_route().unwrap(), bond(4)).unwrap().is_none());
    }
    /// Fold/storage fixture: native signature and finalized-chain authority are
    /// tested by the integration consumers, not manufactured by this unit test.
    #[test]
    fn recovery_v2_reserves_history_and_streams_before_writes_and_ack_checkpoint_survives_terminal_deletion() {
        use super::super::palw_material_release_recovery_fold_v2 as recovery;
        use crate::palw_material_release_recovery_v2::*;
        let (state, params, e) = fixture();
        let policy = PalwMaterialReleaseRecoveryPolicyV2 {
            version: 2,
            max_reserved_contracts: 1,
            max_generations_per_contract: 2,
            max_provider_streams: 4,
            max_reserved_history_cells: 16,
            measurement_root: h(77),
        };
        let params = params
            .with_material_release_recovery_v2(Some(PalwMaterialReleaseRecoveryMirrorV2 { from_daa: 1, policy: policy.clone() }));
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        enroll(&mut b, &ctx(2), &e).unwrap();
        let terms = PalwMaterialRecoveryTermsV2 {
            version: 2,
            root: e.terms.root,
            initial_consent: e.id(),
            sponsor: e.terms.sponsor,
            nonce: h(76),
            policy: policy.id(),
            max_generations: 2,
            all_generation_ack: true,
        };
        let action = PalwMaterialRecoveryActionV2::Enroll {
            terms: terms.clone(),
            providers: e.offers.iter().map(|o| PalwMaterialRecoverySignatureV2 { provider: o.provider, signature: vec![1] }).collect(),
        };
        recovery::action(&mut b, &ctx(2), &action, &bond(1)).unwrap();
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let checkpoint = b.state.material_provider_ack_checkpoint_v2(&bond(2)).unwrap();
        assert_eq!(checkpoint.reserved_acks, 1);
        // Reservation seam: a court admission must reserve its eventual event
        // increment before committing the challenge. No verdict is fabricated.
        recovery::reserve_answer_event_v4(&mut b, e.terms.root, &bond(2)).unwrap();
        assert_eq!(b.state.material_provider_ack_checkpoint_v2(&bond(2)).unwrap().reserved_answer_events, 1);
        recovery::release_answer_event_v4(&mut b, e.terms.root, &bond(2)).unwrap();
        assert_eq!(b.state.material_provider_ack_checkpoint_v2(&bond(2)).unwrap().reserved_answer_events, 0);
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let before = b.state.clone();
        let entries = b.entries.len();
        assert!(recovery::action(&mut b, &ctx(3), &action, &bond(1)).is_err());
        assert_eq!(b.state, before);
        assert_eq!(b.entries.len(), entries);
        let replacement =
            PalwMonetaryMaterialReplacementV1 { root: e.terms.root, previous: e.id(), sequence: 1, enrollment: e.clone() };
        assert!(replace(&mut b, &ctx(3), &replacement).is_err(), "old replacement cannot silently lose recovery consent");
        assert_eq!(b.state, before);
        // Exercise only the journal's Release->Terminal transformation. This
        // direct helper is NOT a native finality witness or payout simulation.
        recovery::released(&mut b, &ctx(1000), e.terms.root, e.id(), h(75)).unwrap();
        write(&mut b, e.terms.root, None).unwrap();
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let Some(PalwMaterialRecoveryRowV2::Terminal(receipt)) = b.state.material_recovery_row_v2(e.terms.root) else { panic!() };
        let id = receipt.id();
        for n in [2, 3] {
            let c = b.state.material_provider_ack_checkpoint_v2(&bond(n)).unwrap();
            let ack = PalwMaterialRecoveryActionV2::Acknowledge {
                root: e.terms.root,
                receipt: id,
                stream: c.stream,
                previous: c.last_receipt,
                sequence: c.sequence + 1,
            };
            let before = b.state.clone();
            let mut bad = ack.clone();
            if let PalwMaterialRecoveryActionV2::Acknowledge { sequence, .. } = &mut bad {
                *sequence += 1;
            }
            assert!(recovery::action(&mut b, &ctx(1001), &bad, &bond(n)).is_err());
            assert_eq!(b.state, before);
            recovery::action(&mut b, &ctx(1001), &ack, &bond(n)).unwrap();
            assert!(recovery::action(&mut b, &ctx(1001), &ack, &bond(n)).is_err());
        }
        assert!(b.state.material_recovery_row_v2(e.terms.root).is_none());
        b.state.assert_material_release_recovery_consistency_v2(&params).unwrap();
        let c = b.state.material_provider_ack_checkpoint_v2(&bond(2)).unwrap();
        assert_eq!((c.sequence, c.last_receipt, c.reserved_acks), (1, id, 0));
        let retire = PalwMaterialRecoveryActionV2::RetireProviderStream { provider: bond(2), stream: c.stream, sequence: c.sequence };
        recovery::action(&mut b, &ctx(1002), &retire, &bond(2)).unwrap();
        assert!(b.state.material_provider_ack_checkpoint_v2(&bond(2)).is_none());
        assert!(recovery::action(&mut b, &ctx(1003), &retire, &bond(2)).is_err());
    }
    /// **audit fix-cd (verify-3 note on C-02): a prospective native session is the executor's own consent.** The enrollment's
    /// acceptance verifies only the sponsor's and the providers' signatures, and `select_native_session_v3` takes the
    /// smallest-root eligible row for an executor. A third party who could enrol a session NAMING another executor (any root it
    /// likes) would therefore become that executor's canonical roster, and its own providers then withhold their Ready
    /// signatures: every non-floor native attempt of the victim (the in-node producer included) is refused by `verify_prior`.
    /// The sponsor's signature covers the whole payload, so requiring `sponsor == native_session.executor` makes that signature
    /// the executor's consent without a new wire field. Every in-tree producer of a native session (the recovery quote, the CLI
    /// spec, the e2e conformance) already sponsors as the executor.
    #[test]
    fn audit_fixcd_a_third_party_cannot_enrol_the_canonical_native_session_of_another_executor() {
        let (mut state, params, base) = fixture();
        let class_id = h(40);
        state.classes.insert(
            class_id,
            PalwClassStateV2 {
                artifact_root: h(41),
                slash_value_per_pwu: 1,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
                status: PalwClassStatusV2::Active,
                registered_daa: 0,
                registrant_bond: Some(bond(5)),
                fused_attention: false,
            },
        );
        let victim = bond(4);
        let session = |executor: PalwBondKeyV2| crate::palw_attempt_material_v3::PalwNativeMaterialSessionV3 {
            version: 3,
            class_id,
            artifact_root: h(41),
            executor,
            operator: h(4),
            job_policy: crate::palw_attempt_material_v3::native_job_policy_v3(),
            codec: crate::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete,
            max_units: 1,
            one_claim_cap: 1,
            admit_from_daa: 0,
            admit_until_daa: 100,
        };
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        // The victim's own session (it sponsors and names itself): accepted.
        let mut own = base.clone();
        own.terms.root = h(20);
        own.terms.nonce = h(21);
        own.terms.sponsor = victim;
        own.terms.native_session = Some(session(victim));
        enroll(&mut builder, &ctx(2), &own).unwrap();
        // A third party (bond 1, two bonded providers 2 and 3) names the victim with a smaller root.
        let mut capture = base.clone();
        capture.terms.root = h(10);
        capture.terms.native_session = Some(session(victim));
        let before = builder.state.clone();
        let entries = builder.entries.len();
        let refused = enroll(&mut builder, &ctx(2), &capture);
        let mut attempt = crate::palw_attempt_v2::tests::attempt();
        attempt.class_id = class_id;
        attempt.artifact_root = h(41);
        attempt.executor_bond = victim.0;
        attempt.operator_id = h(4);
        let canonical =
            crate::palw_attempt_material_v3::select_native_session_v3(builder.state.monetary_material_rows_v1(), &attempt, 2, |p| {
                builder.state.bond(p).map(|b| b.operator_id)
            })
            .map(|row| row.terms.root);
        assert!(
            refused.is_err() && canonical == Some(h(20)),
            "audit fix-cd: a third party's session naming the victim executor was {:?}; the victim's canonical session is now {:?} (own session is {:?})",
            refused,
            canonical,
            h(20),
        );
        assert_eq!(builder.state, before, "a refused capture writes nothing");
        assert_eq!(builder.entries.len(), entries);
    }
    #[test]
    fn monetary_material_consent_preflights_all_funding_and_preserves_earned_cash_on_recovery() {
        let (state, params, e) = fixture();
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let original = builder.state.clone();
        let mut bad = e.clone();
        bad.offers[1].prepaid_fee_sompi = 100_000;
        assert!(enroll(&mut builder, &ctx(2), &bad).is_err());
        assert_eq!(builder.state, original);
        assert!(builder.entries.is_empty());
        enroll(&mut builder, &ctx(2), &e).unwrap();
        assert_eq!(builder.state.bond(&bond(1)).unwrap().collateral, 9_986);
        assert_eq!(builder.state.bond(&bond(2)).unwrap().collateral, 10_000);
        assert_eq!(builder.state.monetary_material_reserved_v1(&bond(2)), 100);
        let first = builder.state.monetary_material_row_v1(h(9)).unwrap();
        let mut failed = first.clone();
        failed.providers.iter_mut().for_each(|p| p.charged = true);
        write(&mut builder, h(9), Some(&failed)).unwrap();
        let earned_before = builder.state.pending_payouts.clone();
        let mut renewed = e.clone();
        renewed.offers.iter_mut().zip([4, 5]).for_each(|(p, n)| p.provider = bond(n));
        let r = PalwMonetaryMaterialReplacementV1 { root: h(9), previous: first.consent, sequence: 1, enrollment: renewed };
        replace(&mut builder, &ctx(3), &r).unwrap();
        assert_eq!(builder.state.pending_payouts, earned_before);
        let renewed = builder.state.monetary_material_row_v1(h(9)).unwrap();
        assert_eq!(renewed.providers.len(), 4, "charged old generations remain until their physical fees are paid");
        assert_eq!(renewed.service_fee_count, 4);
        assert_eq!(renewed.service_fee_total_sompi, 28);
        assert!(!builder.state.monetary_material_recovery_needed_v1(h(9)));
        let recovered = builder.state.clone();
        assert!(replace(&mut builder, &ctx(4), &r).is_err());
        assert_eq!(builder.state, recovered);
    }
    #[test]
    fn monetary_material_unknown_clock_tail_and_release_are_root_exact_and_journaled() {
        let (state, params, e) = fixture();
        let extras = PalwTransitionExtrasV1 {
            kernel_route: Some(crate::palw_kernel_route_v1::PalwKernelRouteExtrasV1 {
                network_domain: h(1),
                ruleset_digest: h(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut builder = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        enroll(&mut builder, &ctx(2), &e).unwrap();
        assert_eq!(builder.state.monetary_material_hold_v1(h(9)), Some(u64::MAX));
        assert_eq!(builder.state.monetary_material_hold_v1(h(10)), None);
        tick(&mut builder, &ctx(100)).unwrap();
        assert_eq!(builder.state.monetary_material_release_anchor_v1(h(9)), None);
        tick(&mut builder, &ctx(1_000)).unwrap();
        // Model-level validated-queue seam. Native acceptance separately verifies
        // the physical coinbase; this test exercises its common receipt hook.
        for n in [2, 3] {
            let payload = builder.state.bond(&bond(n)).unwrap().payout_payload;
            let queue = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payload);
            let income = super::super::palw_bond_budget_fold_v1::native_queued_income_id_v1(queue, &bond(n));
            builder.write_payout(queue, None).unwrap();
            observe_payment(&mut builder, &ctx(1_000), income, queue).unwrap();
            builder
                .bond_budget_op(super::super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |b, _, j| {
                    b.confirm_native_income_payment_v1(&params.bond_budget().unwrap().policy, &income, 1_000, j)
                })
                .unwrap()
                .unwrap()
                .unwrap();
        }
        let observed = builder.state.monetary_material_row_v1(h(9)).unwrap().last_payment.unwrap();
        assert_eq!(observed.paid_daa, 1_000);
        assert_eq!(observed.entitlement, Some(super::super::palw_bond_budget_fold_v1::source_income_id_v1(e.id(), h(9), &bond(3))));
        tick(&mut builder, &ctx(1_435)).unwrap();
        assert_eq!(builder.state.monetary_material_release_anchor_v1(h(9)), None);
        builder
            .bond_budget_op(super::super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1::point(), |b, _, j| {
                b.release_material_income_v1(1_436, j)
            })
            .unwrap();
        tick(&mut builder, &ctx(1_436)).unwrap();
        let row = builder.state.monetary_material_row_v1(h(9)).unwrap();
        assert_eq!(row.tail_block, Some(ctx(1_436).block));
        assert_eq!(builder.state.monetary_material_hold_v1(h(9)), Some(u64::MAX));
        let pre_release = builder.state.clone();
        let bad = PalwMonetaryMaterialActionV1::Release { root: h(9), consent: row.consent, tail_block: h(22) };
        assert!(action(&mut builder, &ctx(1_001), &bad, &bond(1)).is_err());
        assert_eq!(builder.state, pre_release);
        // Fold-level witness seam: real native acceptance must independently
        // establish the exact carrying block's selected-chain finality first.
        action(
            &mut builder,
            &ctx(2_000),
            &PalwMonetaryMaterialActionV1::Release { root: h(9), consent: row.consent, tail_block: row.tail_block.unwrap() },
            &bond(1),
        )
        .unwrap();
        assert_eq!(builder.state.monetary_material_hold_v1(h(9)), None);
        let mut reverted = builder.state.clone();
        for entry in builder.entries.iter().rev() {
            apply_delta_entry(&mut reverted, entry, true).unwrap();
        }
        assert_eq!(reverted, state);
    }
}

/// Queue IDs coalesce recipients; only the material receipt preimage links a
/// queue portion to the original exact funded entitlement. Capture that link
/// from authenticated journal rows before the original claim is forgotten.
fn capture_queue_entitlements(builder: &mut TransitionBuilder<'_>) -> Result<(), PalwStateV2Error> {
    use crate::palw_bond_budget_v1::{PALW_BUDGET_TABLE_MATERIAL_INCOME_V1, PalwMaterialIncomeV1, PalwMaterialPaymentV1};
    let mut pending = BTreeSet::new();
    let mut queued = Vec::new();
    for entry in &builder.entries {
        if let PalwDeltaEntryV2::BondBudgetRow { table, key, old, new } = entry {
            if *table != PALW_BUDGET_TABLE_MATERIAL_INCOME_V1 {
                continue;
            }
            for bytes in [old, new].into_iter().flatten() {
                let m: PalwMaterialIncomeV1 =
                    borsh::from_slice(bytes).map_err(|_| refused("material queue journal does not decode"))?;
                if let (Some(root), PalwMaterialPaymentV1::Pending { income }) = (m.root, m.payment) {
                    pending.insert((root, m.accepted_daa, income));
                }
            }
            if let Some(bytes) = new {
                let m: PalwMaterialIncomeV1 =
                    borsh::from_slice(bytes).map_err(|_| refused("material queue journal does not decode"))?;
                if let (Some(root), PalwMaterialPaymentV1::Queued { income: Some(income), queued_daa, paid_daa: None, .. }) =
                    (m.root, m.payment)
                {
                    let id: Hash64 = borsh::from_slice(key).map_err(|_| refused("material queue key does not decode"))?;
                    if builder
                        .state
                        .bond_budget
                        .as_ref()
                        .and_then(|b| b.material_income.get(&id))
                        .is_some_and(|m| matches!(m.payment, PalwMaterialPaymentV1::Queued { paid_daa: None, .. }))
                    {
                        queued.push((id, root, m.accepted_daa, income, queued_daa));
                    }
                }
            }
        }
    }
    for (receipt, root, accepted, income, queued_daa) in queued {
        let source = pending.iter().filter(|(r, a, _)| *r == root && *a == accepted).find_map(|(_, _, source)| {
            (crate::palw_bond_budget_v1::material_queue_receipt_id_v1(*source, income, queued_daa) == receipt).then_some(*source)
        });
        let Some(source) = source else { continue };
        for mut row in builder.state.monetary_material_rows_v1() {
            if !builder.state.monetary_material_scope_v1(&row).contains(&root) {
                continue;
            }
            if row.queued_entitlements.get(&receipt).is_some_and(|old| *old != source) {
                return Err(refused("material queue entitlement substitution"));
            }
            if row.queued_entitlements.insert(receipt, source) != Some(source) {
                write(builder, row.terms.root, Some(&row))?;
            }
        }
    }
    Ok(())
}

pub(super) fn plan_native_ready(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    native_claim: &super::PalwClaimStateV2,
) -> Result<Option<PalwMonetaryMaterialRowV1>, PalwStateV2Error> {
    if builder.params.monetary_material_at_v1(ctx.daa_score).is_none() {
        return Ok(None);
    }
    let (session, binding) = builder
        .monetary_native_ready
        .get(&claim)
        .ok_or_else(|| refused("new native work has no authenticated prior-session Ready"))?;
    let mut row = builder.state.monetary_material_row_v1(*session).ok_or_else(|| refused("native ready session disappeared"))?;
    if row.native_binding.is_some() || !row.healthy(&builder.state) {
        return Err(refused("native ready session consumed or providers defaulted"));
    }
    let scope = binding.manifest.scope(claim).ok_or_else(|| refused("native Ready has no exact claim scope"))?;
    if scope.codec != builder.state.native_material_codec_for_claim_v2(claim, native_claim)?
        || scope.commitment != native_claim.execution_root
    {
        return Err(refused("native Ready codec/execution differs from its own prospective routing authority"));
    }
    row.native_binding = Some(binding.clone());
    row.root_seen = true;
    Ok(Some(row))
}
pub(super) fn bind_native_ready(builder: &mut TransitionBuilder<'_>, row: &PalwMonetaryMaterialRowV1) -> Result<(), PalwStateV2Error> {
    write(builder, row.terms.root, Some(row))?;
    Ok(())
}

/// Explicit V2 retrieval + exact accepted generation + atomic applied receipt.
/// A receipt is only a recovery cursor. It grants no computation credit.
pub(super) fn routed_answer_v2(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    a: &crate::palw_routed_material_v2::PalwRoutedMaterialAnswerV2,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native_resource_v1::{
        NativeDecodeErrorV1, TirNativeRequestsV1, native_value_requests_v1, reserve_native_requests_v1,
        retain_native_value_decode_classified_v1,
    };
    use crate::palw_routed_material_v2::PalwRoutedMaterialTargetV2;
    use crate::palw_step_refute::routing_v2::{PalwRoutingCourtErrorV2, check_routed_step_committed_half_v2};
    let PalwRoutedMaterialTargetV2::Monetary { root, consent } = a.target else {
        return Err(refused("routed monetary answer target differs"));
    };
    let p = policy(b, ctx.daa_score)?;
    let limits = b.extras.memory_routing_court.ok_or(PalwStateV2Error::ShardCourtDormant)?;
    let route = b.state.kernel_route().ok_or_else(|| refused("routed monetary route absent"))?;
    let bytes = route
        .aux
        .iter()
        .find_map(|((t, k), v)| {
            (*t == PALW_MONETARY_MATERIAL_TABLE_V1 && k.as_slice() == root.as_byte_slice()).then_some(v.as_slice())
        })
        .ok_or_else(|| refused("routed material contract absent"))?;
    let owned =
        retain_native_value_decode_classified_v1::<PalwMonetaryMaterialRowV1, std::io::Error>(bytes, || borsh::from_slice(bytes))
            .map_err(|e| match e {
                NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                NativeDecodeErrorV1::Decode(e) => refused(e.to_string()),
            })?;
    let native = b.state.reader_router_of_claim_v2(a.claim)?.ok_or_else(|| refused("routed material claim stamp absent"))?;
    let claim = b.state.claim(&a.claim).ok_or(PalwStateV2Error::MissingClaim(a.claim))?;
    let ladder =
        b.state.class_step_ladder_v1(&claim.class_id, b.extras.held_context_ladder.ok_or(PalwStateV2Error::HeldContextDormant)?);
    a.check_shape(ladder).map_err(refused)?;
    let encoded = borsh::object_length(a).map_err(|e| refused(e.to_string()))? as u64;
    if encoded > limits.max_close_bytes || encoded > owned.terms.max_material_bytes || encoded > p.max_bytes_per_contract {
        return Err(refused("routed material answer exceeds native/signed capacity"));
    }
    let current = crate::palw_kernel_material_query_v4::material_provider_generation_v4(&b.state, root, a.provider)
        .map_err(|e| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(e, refused))?;
    if current != Some(consent) || !owned.providers.iter().any(|v| v.provider == a.provider && !v.charged) {
        return Err(refused("routed material provider generation differs from own accepted state"));
    }
    let scope = owned
        .native_binding
        .as_ref()
        .map(|v| &v.manifest)
        .or(owned.terms.material_manifest.as_ref())
        .and_then(|m| m.scope(a.claim))
        .ok_or_else(|| refused("routed material exact scope absent"))?;
    if scope.codec != crate::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldCompleteV2
        || scope.commitment != claim.execution_root
    {
        return Err(refused("routed answer scope codec/execution differs"));
    }
    let missing = crate::palw_held_da_v1::PalwHeldMissingV1::StepLeaf { leaf: a.refutation.step.output_opening.leaf_index };
    let index = owned
        .challenges
        .iter()
        .position(|c| c.provider == a.provider && c.dependency == a.claim && c.native_missing == Some(missing))
        .ok_or_else(|| refused("routed material has no exact challenged unit"))?;
    let challenge = &owned.challenges[index];
    if challenge.answered
        || ctx.daa_score > challenge.deadline_daa
        || challenge.native_binding.as_deref() != Some(&a.refutation.step.binding)
    {
        return Err(refused("routed material challenge is answered/late or binding differs"));
    }
    let mut checkpoint = if b.state.material_recovery_row_v2(root).is_some() {
        if b.params.material_release_recovery_at_v2(ctx.daa_score).is_none() {
            return Err(refused("routed recovery fence dormant"));
        }
        let receipt = a.receipt.as_ref().ok_or_else(|| refused("routed recovery answer requires applied checkpoint"))?;
        let checkpoint =
            b.state.material_provider_ack_checkpoint_v2(&a.provider).ok_or_else(|| refused("routed provider stream absent"))?;
        if receipt.challenge_deadline_daa != challenge.deadline_daa
            || receipt.stream != checkpoint.stream
            || receipt.previous != checkpoint.last_receipt
            || checkpoint.sequence.checked_add(1) != Some(receipt.sequence)
            || checkpoint.reserved_answer_events == 0
        {
            return Err(refused("routed answer replay/substitution in receipt stream"));
        }
        Some(checkpoint)
    } else {
        if a.receipt.is_some() {
            return Err(refused("routed legacy contract cannot introduce a receipt stream"));
        }
        None
    };
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(native_value_requests_v1(&*owned).map_err(PalwStateV2Error::NativeResource)?)
        .map_err(PalwStateV2Error::NativeResource)?;
    // Existing native write encodes, journals and keeps an independent aux row.
    let row_bytes = borsh::object_length(&*owned).map_err(|e| refused(e.to_string()))? as u64;
    for _ in 0..4 {
        q.vec_v1::<u8>(row_bytes).map_err(PalwStateV2Error::NativeResource)?;
    }
    q.vec_v1::<u8>(3 * 69 + 4 * 256).map_err(PalwStateV2Error::NativeResource)?;
    let _mutation = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    check_routed_step_committed_half_v2(&a.refutation, claim.execution_root, &native.declaration, ladder).map_err(|e| match e {
        PalwRoutingCourtErrorV2::Resource(e) => PalwStateV2Error::NativeResource(e),
        other => refused(other.to_string()),
    })?;
    if !super::palw_kernel_route_fold_v1::charge_route_budget_v1(b, ctx, 0, false)? {
        return Err(refused("routed answer route budget exhausted"));
    }
    let mut row = (*owned).clone();
    b.slash_bond(row.challenges[index].challenger, u128::from(crate::palw_provider_court_v1::PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1))?;
    row.challenges[index].answered = true;
    if let Some(checkpoint) = &mut checkpoint {
        let receipt = a.receipt.as_ref().expect("checked above");
        checkpoint.sequence = receipt.sequence;
        let network = Hash64::from_bytes(b.state.kernel_route().expect("route checked").header.policy.network_domain);
        checkpoint.last_receipt = a.message(network);
        checkpoint.reserved_answer_events -= 1;
        b.write_kernel_row(
            crate::palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2,
            crate::palw_material_release_recovery_v2::material_provider_stream_key_v2(&a.provider),
            Some(borsh::to_vec(checkpoint).unwrap()),
        )?;
    }
    write(b, root, Some(&row))?;
    Ok(())
}
