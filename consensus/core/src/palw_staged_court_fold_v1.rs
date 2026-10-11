//! Prospective claim contract/continuation writers. All failures precede claim acceptance writes.
use super::*;
use crate::palw_staged_court_state_v1::*;
impl PalwChainStateV2 {
    pub(super) fn staged_retirement_floor_v1(&self, claim: &Hash64, ordinary: u64) -> u64 {
        if self.claims.get(claim).is_some_and(|c| c.phase.is_terminal()) {
            self.staged_claim_contract_v1(claim).map(|c| ordinary.max(c.terms.material_until_daa)).unwrap_or(ordinary)
        } else {
            ordinary
        }
    }
    pub fn staged_claim_contract_v1(&self, claim: &Hash64) -> Option<&PalwStagedClaimContractV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::Claim(*claim)) {
            Some(PalwStagedCourtRowV1::Claim(c)) => Some(c),
            _ => None,
        }
    }
    pub fn staged_owned_constraint_v1(
        &self,
        claim: Hash64,
        owner: PalwBondKeyV2,
    ) -> Option<&crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1> {
        match self.staged_courts.get(&PalwStagedCourtKeyV1::Owner(claim, owner)) {
            Some(PalwStagedCourtRowV1::Constraint(c)) => Some(c),
            _ => None,
        }
    }
    pub fn staged_bond_exit_held_v1(&self, bond: &PalwBondKeyV2, now: u64) -> bool {
        now.checked_add(1).is_some_and(|after| {
            self.staged_holds
                .range((*bond, after, PalwStagedCourtKeyV1::Claim(ZERO_HASH64))..)
                .next()
                .is_some_and(|(held, _, _)| held == bond)
        })
    }
}
impl TransitionBuilder<'_> {
    /// Pure preflight: checked clocks are established before even an attempt's first write.
    pub(super) fn prepare_staged_claim_v1(
        &self,
        claim: &mut PalwClaimStateV2,
    ) -> Result<Option<PalwStagedClaimContractV1>, PalwStateV2Error> {
        let Some(config) = self.extras.staged_public_court else {
            return Ok(None);
        };
        let vocab = self.state.fp_work_profiles.get(&claim.class_id).map(|p| p.vocab_size).unwrap_or((1 << 22) - 1);
        // The authenticated class profile narrows vocabulary. The existing generated-position
        // format caps every other claim independently of a producer's asserted prompt/rendering size.
        let terms = config.claim_terms(claim.accepted_daa, 4095, vocab).map_err(PalwStateV2Error::CarriageInconsistent)?;
        claim.trace_retention_daa = claim.trace_retention_daa.max(terms.material_until_daa);
        Ok(Some(PalwStagedClaimContractV1 { terms, producer: claim.bond, panel: BTreeSet::new() }))
    }
    pub(super) fn write_staged_row_v1(&mut self, key: PalwStagedCourtKeyV1, new: Option<PalwStagedCourtRowV1>) {
        let old = match &new {
            Some(row) => self.state.staged_courts.insert(key, row.clone()),
            None => self.state.staged_courts.remove(&key),
        };
        update_indices(&mut self.state, key, old.as_ref(), new.as_ref());
        if old != new {
            self.entries.push(PalwDeltaEntryV2::StagedCourtSeed { key, old, new });
        }
    }
    pub(super) fn stamp_staged_claim_v1(&mut self, claim: Hash64, contract: Option<PalwStagedClaimContractV1>) {
        if let Some(c) = contract {
            self.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(claim), Some(PalwStagedCourtRowV1::Claim(c)));
        }
    }
    pub(super) fn record_staged_panel_v1(&mut self, claim: Hash64, panel: &PalwPanelStateV2) {
        if let Some(mut contract) = self.state.staged_claim_contract_v1(&claim).cloned() {
            contract.panel.extend(panel.seats.iter().map(|seat| seat.bond));
            self.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(claim), Some(PalwStagedCourtRowV1::Claim(contract)));
        }
    }
}

fn stage_refused(claim: Hash64, why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::ObjectiveOffenceRefused(claim, why.into())
}
fn gate<'a>(
    b: &'a TransitionBuilder<'_>,
    claim: &Hash64,
    owner: PalwBondKeyV2,
    reserved: u64,
    now: u64,
) -> Result<&'a PalwStagedClaimContractV1, PalwStateV2Error> {
    if b.extras.staged_public_court.is_none() {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    let c =
        b.state.staged_claim_contract_v1(claim).ok_or_else(|| stage_refused(*claim, "claim has no prospective staged contract"))?;
    if !b.state.legacy_dispute_reservation_v1(claim, &owner).is_some_and(|r| r.reserved_daa == reserved)
        || now >= c.terms.stream_until_daa
    {
        return Err(stage_refused(*claim, "staged owner is not the literal live reservation or its admitted clock ended"));
    }
    Ok(c)
}
pub(super) fn open(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    seed: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1,
) -> Result<(), PalwStateV2Error> {
    let contract = gate(b, &seed.claim, seed.reserver, seed.reserved_daa, ctx.daa_score)?.clone();
    let key = PalwStagedCourtKeyV1::Owner(seed.claim, seed.reserver);
    if b.state.staged_courts.contains_key(&key) {
        return Err(stage_refused(seed.claim, "owned staged seed is immutable"));
    }
    let allowance = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?
        .maximum_owned_rows_per_reserver;
    // Only this reserver's paid rows consume its allowance. No claim-wide first-writer cap.
    if b.state.staged_by_owner.range((seed.reserver, ZERO_HASH64)..)
        .take_while(|(owner, _)| *owner == seed.reserver).take(allowance as usize).count() >= allowance as usize {
        return Err(stage_refused(seed.claim, "reserver's own staged work allowance is occupied"));
    }
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &seed.claim)
        .ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
    if crate::palw_offence_attribution_v1::palw_binding_identity_fault_v1(
        &target,
        &seed.binding,
        b.identity_rules_v1(ctx.daa_score),
        false,
    )
    .map_err(|e| stage_refused(seed.claim, e.to_string()))?
    .is_some()
    {
        return Err(stage_refused(seed.claim, "staged seed binding answers another claim's job"));
    }
    let original = b
        .state
        .fp_public_jobs
        .get(&seed.claim)
        .ok_or_else(|| stage_refused(seed.claim, "staged selection seed requires accepted original public FP job"))?;
    let original: crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 =
        borsh::from_slice(original).map_err(|_| stage_refused(seed.claim, "accepted public FP job encoding invalid"))?;
    if original.job != seed.job {
        return Err(stage_refused(seed.claim, "staged seed changes accepted original FP job"));
    }
    if contract.panel.is_empty() {
        return Err(stage_refused(seed.claim, "staged contract has no actual bound Panel"));
    }
    // A root-authenticated profile may narrow a global fallback plan after seed admission.
    // Claim-wide clocks/holds remain the immutable conservative envelope stamped at acceptance.
    let mut terms = contract.terms;
    let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    terms.plan = crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1::derive(
        4095,
        seed.binding.shape_profile.vocab_size,
        config.prices,
    )
    .map_err(|e| stage_refused(seed.claim, e))?;
    if terms.plan.maximum_moves > contract.terms.plan.maximum_moves || terms.plan.inclusion_daa > contract.terms.plan.inclusion_daa {
        return Err(stage_refused(seed.claim, "authenticated seed exceeds immutable claim resource envelope"));
    }
    b.check_accuser_room(seed.reserver, u128::from(terms.plan.deposit_sompi), "staged public court", ctx.daa_score)?;
    // Rent is burned by this object's normative minimum carrier fee; only the deposit occupies
    // bond capital. The acceptance walk validates that fee before invoking this fold.
    let funded = contract
        .terms
        .plan
        .rent_sompi
        .checked_add(contract.terms.plan.deposit_sompi)
        .ok_or(PalwStateV2Error::Overflow("staged funding"))?;
    let exits = vec![contract.terms.bond_exit_after_daa; contract.panel.len()];
    let row = crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1::admit(
        seed,
        target.execution_root,
        terms,
        ctx.daa_score,
        funded,
        b.extras.terminal_evidence_max_bytes.unwrap_or(100_000),
        b.state.class_step_ladder_v1(
            &target.class_id,
            b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
        ),
        b.state.claim(&seed.claim).ok_or(PalwStateV2Error::MissingClaim(seed.claim))?.trace_retention_daa,
        contract.terms.bond_exit_after_daa,
        &exits,
    )
    .map_err(|e| stage_refused(seed.claim, e))?;
    b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::Constraint(Box::new(row))));
    Ok(())
}
pub(super) fn leaf(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    leaf: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamLeafV1,
) -> Result<(), PalwStateV2Error> {
    gate(b, &leaf.claim, leaf.reserver, leaf.reserved_daa, ctx.daa_score)?;
    let key = (leaf.claim, leaf.reserver);
    let row = b.state.staged_owned_constraint_v1(key.0, key.1).ok_or_else(|| stage_refused(key.0, "staged leaf lacks owned seed"))?;
    let old = row.checkpoint_v1();
    let row = match b.state.staged_courts.get_mut(&PalwStagedCourtKeyV1::Owner(key.0, key.1)) {
        Some(PalwStagedCourtRowV1::Constraint(row)) => row,
        _ => return Err(stage_refused(key.0, "owned staged seed disappeared")),
    };
    if let Err(error) = row.leaf(ctx.daa_score, leaf) {
        row.restore_checkpoint_v1(&old).map_err(|e| stage_refused(key.0, e))?;
        return Err(stage_refused(key.0, error));
    }
    let new = row.checkpoint_v1();
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key, old, new });
    Ok(())
}
pub(super) fn movement(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    gate(b, &movement.claim, movement.reserver, movement.reserved_daa, ctx.daa_score)?;
    let key = (movement.claim, movement.reserver);
    let row = b.state.staged_owned_constraint_v1(key.0, key.1).ok_or_else(|| stage_refused(key.0, "staged move lacks owned seed"))?;
    let old = row.checkpoint_v1();
    let row = match b.state.staged_courts.get_mut(&PalwStagedCourtKeyV1::Owner(key.0, key.1)) {
        Some(PalwStagedCourtRowV1::Constraint(row)) => row,
        _ => return Err(stage_refused(key.0, "owned staged seed disappeared")),
    };
    if let Err(error) = row.movement(ctx.daa_score, movement) {
        row.restore_checkpoint_v1(&old).map_err(|e| stage_refused(key.0, e))?;
        return Err(stage_refused(key.0, error));
    }
    let new = row.checkpoint_v1();
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove { key, old, new });
    Ok(())
}

/// Pure derived indexes; no additional rooted copy of a signer list or clock.
fn index_rows(key: PalwStagedCourtKeyV1, row: &PalwStagedCourtRowV1) -> (u64, Vec<(PalwBondKeyV2, u64, PalwStagedCourtKeyV1)>) {
    match row {
        PalwStagedCourtRowV1::Claim(c) => {
            let PalwStagedCourtKeyV1::Claim(_) = key else {
                return (0, Vec::new());
            };
            let rows = std::iter::once(c.producer)
                .chain(c.panel.iter().copied())
                .map(|bond| (bond, c.terms.bond_exit_after_daa, key))
                .collect();
            (c.terms.bond_exit_after_daa, rows)
        }
        PalwStagedCourtRowV1::Constraint(c) => (c.terms.material_until_daa, vec![(c.reserver, c.terms.material_until_daa, key)]),
    }
}
pub(super) fn update_indices(
    state: &mut PalwChainStateV2,
    key: PalwStagedCourtKeyV1,
    old: Option<&PalwStagedCourtRowV1>,
    new: Option<&PalwStagedCourtRowV1>,
) {
    if let Some(row) = old {
        let (due, holds) = index_rows(key, row);
        state.staged_due.remove(&(due, key));
        if let PalwStagedCourtRowV1::Constraint(c) = row { state.staged_by_owner.remove(&(c.reserver, c.claim)); }
        for hold in holds {
            state.staged_holds.remove(&hold);
        }
    }
    if let Some(row) = new {
        let (due, holds) = index_rows(key, row);
        state.staged_due.insert((due, key));
        if let PalwStagedCourtRowV1::Constraint(c) = row { state.staged_by_owner.insert((c.reserver, c.claim)); }
        state.staged_holds.extend(holds);
    }
}
pub(super) fn rebuild_indices(state: &mut PalwChainStateV2) {
    state.staged_holds.clear();
    state.staged_due.clear();
    state.staged_by_owner.clear();
    for (key, row) in &state.staged_courts {
        let (due, holds) = index_rows(*key, row);
        state.staged_due.insert((due, *key));
        if let PalwStagedCourtRowV1::Constraint(c) = row { state.staged_by_owner.insert((c.reserver, c.claim)); }
        state.staged_holds.extend(holds);
    }
}
pub(super) fn sweep(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    // The due index gives rooted-clock ordering;64 row removals bound every accepted block.
    let due: Vec<_> = b.state.staged_due.iter().take_while(|(at, _)| *at <= ctx.daa_score).take(64).map(|(_, key)| *key).collect();
    for key in due {
        b.write_staged_row_v1(key, None);
    }
}
