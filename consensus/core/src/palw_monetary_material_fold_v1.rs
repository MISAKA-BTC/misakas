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
fn write(builder: &mut TransitionBuilder<'_>, root: Hash64, row: Option<&PalwMonetaryMaterialRowV1>) {
    builder.write_kernel_row(PALW_MONETARY_MATERIAL_TABLE_V1, root.as_byte_slice().to_vec(), row.map(|r| borsh::to_vec(r).unwrap()));
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
    let route = builder.state.kernel_route();
    if let Some(manifest) = &t.material_manifest {
        if manifest.root != t.root {
            return Err(refused("signed material manifest belongs to another obligation"));
        }
        manifest.validate(t.max_material_bytes).map_err(|e| refused(e.to_string()))?;
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
    if let Some(old) = prior {
        if old.terms != *t
            || old.providers.iter().filter(|p| !p.charged || !p.fee_queued).count() + e.offers.len()
                > PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
            || e.offers.iter().any(|o| {
                old.providers.iter().any(|previous| previous.provider == o.provider && (!previous.charged || !previous.fee_queued))
            })
        {
            return Err(refused("replacement changes the accepted obligation or reuses an old provider"));
        }
    }
    let sponsor = active(builder, &t.sponsor)?;
    let mut operators = BTreeSet::new();
    let mut fees = 0u64;
    for offer in &e.offers {
        let provider = active(builder, &offer.provider)?;
        if offer.collateral_sompi < p.minimum_collateral_sompi
            || offer.prepaid_fee_sompi < p.minimum_fee_sompi
            || provider.operator_id == sponsor.operator_id
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
    // Same native backing path as a funded source escrow: debit once, then
    // settle the provider's separately reserved R through a physical coinbase.
    if builder.slash_bond(e.terms.sponsor, u128::from(fees))? != fees {
        return Err(refused("material escrow was not fully prepaid"));
    }
    for offer in &e.offers {
        builder.reserve_source_income_v1(ctx, e.id(), e.terms.root, offer.provider, ctx.daa_score, offer.prepaid_fee_sompi)?;
        let id = super::palw_bond_budget_fold_v1::source_income_id_v1(e.id(), e.terms.root, &offer.provider);
        builder
            .bond_budget_op(|b, _, j| b.bind_material_income_v1(e.terms.root, id, j))
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
        })
        .collect()
}
pub(super) fn enroll(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    e: &PalwMonetaryMaterialEnrollmentV1,
) -> Result<(), PalwStateV2Error> {
    if builder.state.monetary_material_row_v1(e.terms.root).is_some() {
        return Err(refused("root is already enrolled; use signed replacement"));
    }
    check_enrollment(builder, ctx, e, None)?;
    let row = PalwMonetaryMaterialRowV1 {
        terms: e.terms.clone(),
        consent: e.id(),
        sequence: 0,
        accepted_daa: ctx.daa_score,
        root_seen: false,
        providers: providers(e, ctx.daa_score),
        challenges: Vec::new(),
        last_tail_until: None,
        tail_block: None,
        tail_blue_score: None,
        last_payment: None,
        payment_history_root: Hash64::default(),
        queued_entitlements: BTreeMap::new(),
    };
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, ctx)?;
    write(builder, e.terms.root, Some(&row));
    fund(builder, ctx, e)?;
    Ok(())
}
pub(super) fn replace(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    r: &PalwMonetaryMaterialReplacementV1,
) -> Result<(), PalwStateV2Error> {
    let mut old = builder.state.monetary_material_row_v1(r.root).ok_or_else(|| refused("replacement has no enrolled root"))?;
    if r.root != r.enrollment.terms.root || r.previous != old.consent || old.sequence.checked_add(1) != Some(r.sequence) {
        return Err(refused("replacement replay or wrong consent generation"));
    }
    check_enrollment(builder, ctx, &r.enrollment, Some(&old))?;

    old.providers.retain(|p| !p.charged || !p.fee_queued);
    old.providers.extend(providers(&r.enrollment, ctx.daa_score));
    old.sequence = r.sequence;
    old.consent = r.id();
    write(builder, r.root, Some(&old));
    fund(builder, ctx, &r.enrollment)?;
    Ok(())
}

pub(super) fn action(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    a: &PalwMonetaryMaterialActionV1,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let p = policy(builder, ctx.daa_score)?;
    let root = match a {
        PalwMonetaryMaterialActionV1::SegmentedChallenge { root, .. }
        | PalwMonetaryMaterialActionV1::SegmentedAnswer { root, .. }
        | PalwMonetaryMaterialActionV1::HeldChallenge { root, .. }
        | PalwMonetaryMaterialActionV1::HeldAnswer { root, .. }
        | PalwMonetaryMaterialActionV1::Challenge { root, .. }
        | PalwMonetaryMaterialActionV1::Answer { root, .. }
        | PalwMonetaryMaterialActionV1::Release { root, .. } => *root,
    };
    let mut row = builder.state.monetary_material_row_v1(root).ok_or_else(|| refused("no enrolled monetary material root"))?;
    match a {
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
            let ledger = builder.state.kernel_route().unwrap().ledger_native_projection_v9().map_err(refused)?;
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
            let ledger = builder.state.kernel_route().unwrap().ledger_native_projection_v9().map_err(refused)?;
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
        PalwMonetaryMaterialActionV1::Challenge { provider, dependency, stage, position, valid_until_daa, .. } => {
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
            if claim.body.position(*stage, *position).is_none() {
                return Err(refused("material challenge names no committed position"));
            }
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
            builder
                .state
                .kernel_route()
                .unwrap()
                .ledger_native_projection_v9()
                .map_err(refused)?
                .classify_served_position_v1(&dependency.as_bytes(), *stage, *position, bytes)
                .map_err(refused)?;
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
            write(builder, root, None);
            return Ok(());
        }
    }
    write(builder, root, Some(&row));
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
        row.root_seen |= builder.state.claims.contains_key(&row.terms.root)
            || builder.state.kernel_route().and_then(|r| r.kernel_claim_row_v1(&row.terms.root)).is_some();
        let failed: Vec<_> =
            row.challenges.iter().filter(|c| !c.answered && ctx.daa_score > c.deadline_daa).map(|c| c.provider).collect();
        for provider in failed {
            if let Some(p) = row.providers.iter_mut().find(|p| p.provider == provider && !p.charged) {
                builder.slash_bond(provider, u128::from(p.collateral_sompi))?;
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
                    && builder.state.claims.get(&row.terms.root).is_none_or(|c| c.phase.is_terminal())
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
            write(builder, row.terms.root, Some(&row));
        }
    }
    Ok(())
}

/// Bind receipt authority before the monetary engine forgets a drained income.
pub(super) fn observe_payment(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, income: Hash64, queue: Hash64) {
    if builder.params.monetary_material_at_v1(ctx.daa_score).is_none() {
        return;
    }
    let Some(coinbase_queue_amount) = builder.entries.iter().find_map(|entry| match entry {
        PalwDeltaEntryV2::Payout { key, old: Some(payout), new: None } if *key == queue => Some(payout.amount),
        _ => None,
    }) else {
        return;
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
            row.last_payment = Some(observation.clone());
            write(builder, row.terms.root, Some(&row));
        }
    }
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
            .with_bond_budget(Some(PalwBondBudgetMirrorV1 { from_daa: 1, policy: monetary, allocation: None }))
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
        write(&mut builder, h(9), Some(&failed));
        let earned_before = builder.state.pending_payouts.clone();
        let mut renewed = e.clone();
        renewed.offers.iter_mut().zip([4, 5]).for_each(|(p, n)| p.provider = bond(n));
        let r = PalwMonetaryMaterialReplacementV1 { root: h(9), previous: first.consent, sequence: 1, enrollment: renewed };
        replace(&mut builder, &ctx(3), &r).unwrap();
        assert_eq!(builder.state.pending_payouts, earned_before);
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
            builder.write_payout(queue, None);
            observe_payment(&mut builder, &ctx(1_000), income, queue);
            builder
                .bond_budget_op(|b, _, j| b.confirm_native_income_payment_v1(&params.bond_budget().unwrap().policy, &income, 1_000, j))
                .unwrap()
                .unwrap();
        }
        let observed = builder.state.monetary_material_row_v1(h(9)).unwrap().last_payment.unwrap();
        assert_eq!(observed.paid_daa, 1_000);
        assert_eq!(observed.entitlement, Some(super::super::palw_bond_budget_fold_v1::source_income_id_v1(e.id(), h(9), &bond(3))));
        tick(&mut builder, &ctx(1_435)).unwrap();
        assert_eq!(builder.state.monetary_material_release_anchor_v1(h(9)), None);
        builder.bond_budget_op(|b, _, j| b.release_material_income_v1(1_436, j));
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
                write(builder, row.terms.root, Some(&row));
            }
        }
    }
    Ok(())
}
