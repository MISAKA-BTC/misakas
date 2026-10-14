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
    pub fn staged_owned_row_v1(&self, claim: Hash64, owner: PalwBondKeyV2) -> Option<&PalwStagedCourtRowV1> {
        self.staged_courts.get(&PalwStagedCourtKeyV1::Owner(claim, owner))
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
        self.prepare_staged_claim_with_job_v1(claim, None)
    }
    pub(super) fn prepare_staged_claim_with_job_v1(
        &self,
        claim: &mut PalwClaimStateV2,
        public_job: Option<&crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>,
    ) -> Result<Option<PalwStagedClaimContractV1>, PalwStateV2Error> {
        let Some(config) = self.extras.staged_public_court else {
            return Ok(None);
        };
        let vocab = self.state.fp_work_profiles.get(&claim.class_id).map(|p| p.vocab_size).unwrap_or((1 << 22) - 1);
        // The authenticated class profile narrows vocabulary. The existing generated-position
        // format caps every other claim independently of a producer's asserted prompt/rendering size.
        let mut terms = config.claim_terms(claim.accepted_daa, 4095, vocab).map_err(PalwStateV2Error::CarriageInconsistent)?;
        if let Some(registered) = self.state.class_semantics.get(&claim.class_id)
            && let (Some(raw), Some(bound)) = (registered.raw_tokenizer_bytes, registered.tokenizer_key_events)
        {
            let tokenizer =
                crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(raw, bound, config.prices)
                    .and_then(|p| p.court_envelope(vocab, config.prices))
                    .map_err(PalwStateV2Error::CarriageInconsistent)?;
            terms = crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1::derive(
                claim.accepted_daa,
                terms.plan.cover(tokenizer),
                config.acquisition_daa,
                config.collection_daa,
                config.reorg_retention_daa,
            )
            .map_err(PalwStateV2Error::CarriageInconsistent)?;
        }
        if let Some(public_job) = public_job.filter(|j| j.job.is_prefix_state()) {
            let job = &public_job.job;
            if let (Some(profile), Some(relation)) = (
                self.state.fp_work_profiles.get(&claim.class_id),
                self.state.class_semantics.get(&claim.class_id).and_then(|s| s.prefix_state_relation),
            ) {
                let named = crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job)
                    .map_err(|e| PalwStateV2Error::CarriageInconsistent(e.to_string()))?;
                let ladder = self.state.class_step_ladder_v1(
                    &claim.class_id,
                    self.extras
                        .held_context_ladder
                        .or(self.extras.shard_court_ladder)
                        .ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
                );
                // Authenticated class + accepted job counts establish the complete prefix
                // envelope before any liability/material row can become spendable.
                let leaves = crate::palw_step::step_leaf_count_of_tokens_capped_v1(
                    profile,
                    job.prompt_tokens,
                    public_job.decode_tokens_executed,
                    ladder,
                )
                .map_err(|e| PalwStateV2Error::CarriageInconsistent(e.to_string()))?;
                let prefix = crate::palw_prefix_resources_v1::PalwPrefixResourcePlanV1::derive(
                    profile,
                    relation,
                    named.prefix_tokens,
                    leaves,
                    90_000,
                    config.prices,
                )
                .map_err(PalwStateV2Error::CarriageInconsistent)?;
                terms = crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1::derive(
                    claim.accepted_daa,
                    terms.plan.cover(prefix.envelope),
                    config.acquisition_daa,
                    config.collection_daa,
                    config.reorg_retention_daa,
                )
                .map_err(PalwStateV2Error::CarriageInconsistent)?;
            }
        }
        claim.trace_retention_daa = claim.trace_retention_daa.max(terms.material_until_daa);
        let prefix_custody = public_job.filter(|c| c.job.is_prefix_state()).and_then(|commit| {
            self.state.class_semantics.get(&claim.class_id).filter(|s| s.prefix_state_relation.is_some()).map(|registered| {
                crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1::new(
                    claim.execution_root,
                    crate::palw_fp_execution_v3::palw_fp_job_pin_v1(commit),
                    registered.cu_ruleset_id,
                    &terms,
                )
            })
        });
        Ok(Some(PalwStagedClaimContractV1 { terms, producer: claim.bond, panel: BTreeSet::new(), prefix_custody }))
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
            if let Some(custody) = &mut contract.prefix_custody {
                custody.expected_panel = panel.seats.iter().map(|seat| seat.bond).collect();
            }
            self.write_staged_row_v1(PalwStagedCourtKeyV1::Claim(claim), Some(PalwStagedCourtRowV1::Claim(contract)));
        }
    }
    /// Called only on actual signed Valid credit edges; ordinary membership/Sampled pay is
    /// insufficient. Qualification remains claim-rooted after duty/receipt pruning.
    pub(super) fn record_staged_prefix_receipts_v1(
        &mut self,
        claim: Hash64,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
        credited: &[PalwBondKeyV2],
        now: u64,
    ) {
        if self.extras.staged_public_court.is_none() {
            return;
        }
        let Some(mut contract) = self.state.staged_claim_contract_v1(&claim).cloned() else {
            return;
        };
        let Some(custody) = &mut contract.prefix_custody else {
            return;
        };
        let mut changed = false;
        for receipt in receipts.iter().filter(|r| {
            credited.contains(&r.seat_bond)
                || self.state.panel_duties.get(&claim).is_some_and(|duties| duties.seats.get(&r.seat_bond).is_some_and(|at| *at != 0))
        }) {
            changed |= custody.qualify(claim, receipt, now);
        }
        if changed {
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
    if b.state.legacy_dispute_reservation_v1(claim, &owner).is_none_or(|r| r.reserved_daa != reserved)
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
    if ctx.daa_score >= contract.terms.open_until_daa {
        return Err(stage_refused(seed.claim, "seed is after immutable prospective opening deadline"));
    }
    let key = PalwStagedCourtKeyV1::Owner(seed.claim, seed.reserver);
    if b.state.staged_courts.contains_key(&key) {
        return Err(stage_refused(seed.claim, "owned staged seed is immutable"));
    }
    let allowance = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?.maximum_owned_rows_per_reserver;
    // Only this reserver's paid rows consume its allowance. No claim-wide first-writer cap.
    if b.state
        .staged_by_owner
        .range((seed.reserver, ZERO_HASH64)..)
        .take_while(|(owner, _)| *owner == seed.reserver)
        .take(allowance as usize)
        .count()
        >= allowance as usize
    {
        return Err(stage_refused(seed.claim, "reserver's own staged work allowance is occupied"));
    }
    let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&b.state, &seed.claim)
        .ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
    let binding = seed
        .resolved_binding_v1(target.class_id, b.state.fp_work_profiles.get(&target.class_id).map(|p| p.as_ref()))
        .map_err(|e| stage_refused(seed.claim, e))?;
    if crate::palw_offence_attribution_v1::palw_binding_identity_fault_v1(&target, &binding, b.identity_rules_v1(ctx.daa_score), false)
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
    if seed.job.as_deref().is_some_and(|j| *j != original.job) {
        return Err(stage_refused(seed.claim, "staged seed changes accepted original FP job"));
    }
    if contract.panel.is_empty() {
        return Err(stage_refused(seed.claim, "staged contract has no actual bound Panel"));
    }
    if let Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::Prefix {
        maximum_leaves,
        prefix_tokens,
        relation,
    }) = &seed.auxiliary
    {
        let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
        let ladder = b.state.class_step_ladder_v1(
            &target.class_id,
            b.extras.held_context_ladder.or(b.extras.shard_court_ladder).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?,
        );
        let named =
            crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(&original.job).map_err(|e| stage_refused(seed.claim, e.to_string()))?;
        if *maximum_leaves != ladder
            || *prefix_tokens != named.prefix_tokens
            || target.registered_semantics.as_ref().and_then(|s| s.prefix_state_relation) != Some(*relation)
        {
            return Err(stage_refused(seed.claim, "prefix fee metadata changes actual job, immutable relation or rule ladder"));
        }
        let plan = crate::palw_prefix_resources_v1::PalwPrefixResourcePlanV1::derive(
            &binding.shape_profile,
            *relation,
            *prefix_tokens,
            binding.step_leaf_count,
            90_000,
            config.prices,
        )
        .map_err(|e| stage_refused(seed.claim, e))?
        .envelope;
        let extended_terms = contract.terms.extend_paid_work_v1(ctx.daa_score, plan).map_err(|e| stage_refused(seed.claim, e))?;
        let mut extended_claim = b.state.claim(&seed.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
        // X3 (REQ-X3-3, `palw_claim_hard_deadline_v1`): a claim accepted past the fence has its deadlines fixed at acceptance — a
        // paid stage that would move its retention (and with it the hard deadline: `hard.max(new)` below) is refused, before any
        // write. Below the fence the extension is the OLD rule, unchanged.
        if extended_terms.material_until_daa > extended_claim.trace_retention_daa
            && crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(b.params, &extended_claim)
        {
            return Err(stage_refused(seed.claim, "X3: a paid stage cannot extend a claim's deadlines fixed at acceptance"));
        }
        extended_claim.trace_retention_daa = extended_claim.trace_retention_daa.max(extended_terms.material_until_daa);
        let hard = crate::palw_legacy_public_filer_v1::palw_dispute_hard_deadline_v1(
            &extended_claim,
            palw_da_disclose_window_daa_v1(b.params),
        );
        let until = hard
            .checked_add(b.params.window_challenge_at(ctx.daa_score))
            .ok_or(PalwStateV2Error::Overflow("paid prefix DA extension"))?
            .max(extended_terms.bond_exit_after_daa);
        super::palw_exec_v2_rewards_v1::require_native_session_window(&b.state, b.params, &seed.claim, until)?;
        b.check_accuser_room(seed.reserver, u128::from(plan.deposit_sompi), "staged prefix court", ctx.daa_score)?;
        let funded = plan.rent_sompi.checked_add(plan.deposit_sompi).ok_or(PalwStateV2Error::Overflow("staged prefix funding"))?;
        let exits = vec![extended_terms.bond_exit_after_daa; contract.panel.len()];
        let encoded = borsh::to_vec(seed).map_err(|e| stage_refused(seed.claim, e.to_string()))?;
        let message = seed.signing_message();
        let seed_id = Hash64::from_bytes(message.try_into().map_err(|_| stage_refused(seed.claim, "prefix seed identity length"))?);
        let row = crate::palw_prefix_owned_court_v1::PalwOwnedPrefixCourtV1::admit(
            seed_id,
            seed.claim,
            seed.reserver,
            seed.reserved_daa,
            &binding,
            &original.job,
            &target,
            extended_terms,
            config.prices,
            ctx.daa_score,
            funded,
            90_000,
            ladder,
            encoded.len() as u64,
            extended_claim.trace_retention_daa,
            extended_terms.bond_exit_after_daa,
            &exits,
        )
        .map_err(|e| stage_refused(seed.claim, e))?;
        b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::Prefix(Box::new(row))));
        if extended_terms != contract.terms {
            b.write_staged_row_v1(
                PalwStagedCourtKeyV1::Claim(seed.claim),
                Some(PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 { terms: extended_terms, ..contract.clone() })),
            );
            b.write_claim(seed.claim, Some(extended_claim));
            if let Some(mut dispute) = b.state.legacy_disputes.get(&seed.claim).cloned() {
                dispute.hard_deadline_daa = dispute.hard_deadline_daa.max(hard);
                b.write_legacy_dispute(seed.claim, Some(dispute));
            }
            b.da_rekey_v1(seed.claim, until, ctx.daa_score)?;
            if let Some(mut liability) = b.state.panel_liabilities.get(&seed.claim).cloned()
                && liability.voided_daa.is_none()
                && liability.void_reason.is_none()
                && liability.expiry_daa < until
            {
                liability.expiry_daa = until;
                b.write_panel_liability(seed.claim, Some(liability));
            }
            for (lock_key, mut lock) in b.claim_locks_v1(&seed.claim) {
                if lock.expiry_daa < until {
                    lock.expiry_daa = until;
                    b.write_slashable_lock(lock_key, Some(lock));
                }
            }
        }
        return Ok(());
    }
    if matches!(&seed.auxiliary, Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::Tokenizer { .. })) {
        let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
        let semantics = target
            .registered_semantics
            .as_ref()
            .ok_or_else(|| stage_refused(seed.claim, "tokenizer court has no immutable claim semantics"))?;
        let (Some(_raw), Some(bound)) = (semantics.raw_tokenizer_bytes, semantics.tokenizer_key_events) else {
            return Err(stage_refused(seed.claim, "historical tokenizer has no prospective raw/work obligation"));
        };
        let Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::Tokenizer {
            operand_tokenizer_bytes, ..
        }) = &seed.auxiliary
        else {
            unreachable!()
        };
        let plan = crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(
            *operand_tokenizer_bytes,
            bound,
            config.prices,
        )
        .and_then(|p| p.court_envelope(binding.shape_profile.vocab_size, config.prices))
        .map_err(|e| stage_refused(seed.claim, e))?;
        let extended_terms = contract.terms.extend_paid_work_v1(ctx.daa_score, plan).map_err(|e| stage_refused(seed.claim, e))?;
        let mut extended_claim = b.state.claim(&seed.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(seed.claim))?;
        // X3 (REQ-X3-3, `palw_claim_hard_deadline_v1`): a claim accepted past the fence has its deadlines fixed at acceptance — a
        // paid stage that would move its retention (and with it the hard deadline: `hard.max(new)` below) is refused, before any
        // write. Below the fence the extension is the OLD rule, unchanged.
        if extended_terms.material_until_daa > extended_claim.trace_retention_daa
            && crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(b.params, &extended_claim)
        {
            return Err(stage_refused(seed.claim, "X3: a paid stage cannot extend a claim's deadlines fixed at acceptance"));
        }
        extended_claim.trace_retention_daa = extended_claim.trace_retention_daa.max(extended_terms.material_until_daa);
        let hard = crate::palw_legacy_public_filer_v1::palw_dispute_hard_deadline_v1(
            &extended_claim,
            palw_da_disclose_window_daa_v1(b.params),
        );
        let until = hard
            .checked_add(b.params.window_challenge_at(ctx.daa_score))
            .ok_or(PalwStateV2Error::Overflow("paid DA extension"))?
            .max(extended_terms.bond_exit_after_daa);
        // EXECv2 retains its independently frozen horizon. A prospective stage cannot silently
        // reinterpret that historical contract to buy a later native reward/DA window.
        super::palw_exec_v2_rewards_v1::require_native_session_window(&b.state, b.params, &seed.claim, until)?;
        b.check_accuser_room(seed.reserver, u128::from(plan.deposit_sompi), "staged tokenizer court", ctx.daa_score)?;
        let funded = plan.rent_sompi.checked_add(plan.deposit_sompi).ok_or(PalwStateV2Error::Overflow("staged tokenizer funding"))?;
        let exits = vec![extended_terms.bond_exit_after_daa; contract.panel.len()];
        let row = crate::palw_tokenizer_owned_court_v1::PalwOwnedTokenizerCourtV1::admit_with_resolved_binding_v1(
            seed,
            &original.job,
            &target,
            extended_terms,
            config.prices,
            ctx.daa_score,
            funded,
            b.extras.terminal_evidence_max_bytes.unwrap_or(100000),
            extended_claim.trace_retention_daa,
            extended_terms.bond_exit_after_daa,
            &exits,
            &binding,
        )
        .map_err(|e| stage_refused(seed.claim, e))?;
        // Fee validation already burned the full declared carrier rent; the paid row reserves
        // the ordinary owner deposit before accused/material clocks are enlarged. Every
        // fallible identity/funding/overflow check is above these journaled writes.
        b.write_staged_row_v1(key, Some(PalwStagedCourtRowV1::Tokenizer(Box::new(row))));
        if extended_terms != contract.terms {
            b.write_staged_row_v1(
                PalwStagedCourtKeyV1::Claim(seed.claim),
                Some(PalwStagedCourtRowV1::Claim(PalwStagedClaimContractV1 { terms: extended_terms, ..contract.clone() })),
            );
            b.write_claim(seed.claim, Some(extended_claim));
            if let Some(mut dispute) = b.state.legacy_disputes.get(&seed.claim).cloned() {
                dispute.hard_deadline_daa = dispute.hard_deadline_daa.max(hard);
                b.write_legacy_dispute(seed.claim, Some(dispute));
            }
            b.da_rekey_v1(seed.claim, until, ctx.daa_score)?;
            if let Some(mut liability) = b.state.panel_liabilities.get(&seed.claim).cloned()
                && liability.voided_daa.is_none()
                && liability.void_reason.is_none()
                && liability.expiry_daa < until
            {
                liability.expiry_daa = until;
                b.write_panel_liability(seed.claim, Some(liability));
            }
            // The initial prospective holds keep these locks economically attributed even if
            // an older ordinary lock expiry has passed. Extend the actual saved locked rows.
            for (lock_key, mut lock) in b.claim_locks_v1(&seed.claim) {
                if lock.expiry_daa < until {
                    lock.expiry_daa = until;
                    b.write_slashable_lock(lock_key, Some(lock));
                }
            }
        }
        return Ok(());
    }
    // A root-authenticated profile may narrow a global fallback plan after seed admission.
    // Claim-wide clocks/holds remain the immutable conservative envelope stamped at acceptance.
    let mut terms = contract.terms;
    let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    terms.plan =
        crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1::derive(4095, binding.shape_profile.vocab_size, config.prices)
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
    let row = crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1::admit_with_resolved_binding_v1(
        seed,
        &original.job,
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
        &binding,
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
    b.entries.push(PalwDeltaEntryV2::StagedCourtMove {
        key,
        old: PalwStagedCheckpointV1::Constraint(old),
        new: PalwStagedCheckpointV1::Constraint(new),
    });
    Ok(())
}
pub(super) fn movement(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    movement: &crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
) -> Result<(), PalwStateV2Error> {
    gate(b, &movement.claim, movement.reserver, movement.reserved_daa, ctx.daa_score)?;
    let key = (movement.claim, movement.reserver);
    let row = b
        .state
        .staged_courts
        .get_mut(&PalwStagedCourtKeyV1::Owner(key.0, key.1))
        .ok_or_else(|| stage_refused(key.0, "staged move lacks immutable owned seed"))?;
    let old = row.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
    let result = match row {
        PalwStagedCourtRowV1::Constraint(c) => c.movement(ctx.daa_score, movement),
        PalwStagedCourtRowV1::Tokenizer(c) => c.movement(ctx.daa_score, movement),
        PalwStagedCourtRowV1::Prefix(c) => {
            if movement.phase != crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::PrefixCourt {
                return Err(stage_refused(movement.claim, "prefix movement changes typed phase"));
            }
            let carrier_len = borsh::to_vec(movement).map_err(|e| stage_refused(movement.claim, e.to_string()))?.len() as u64;
            movement.check().map_err(|e| stage_refused(movement.claim, e))?;
            match &movement.action {
                crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::Prefix(action) => c.movement(
                    ctx.daa_score,
                    movement.claim,
                    movement.reserver,
                    movement.reserved_daa,
                    movement.seed_id,
                    action,
                    carrier_len,
                ),
                crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::PrefixBatch(actions) => c.movement_batch_v1(
                    ctx.daa_score,
                    movement.claim,
                    movement.reserver,
                    movement.reserved_daa,
                    movement.seed_id,
                    actions,
                    carrier_len,
                ),
                _ => Err("prefix movement changes owned court action".into()),
            }
        }
        _ => Err("claim contract alone cannot accept an owned move".into()),
    };
    if let Err(error) = result {
        row.restore_checkpoint_v1(&old).map_err(|e| stage_refused(key.0, e))?;
        return Err(stage_refused(key.0, error));
    }
    let new = row.checkpoint_v1().map_err(|e| stage_refused(key.0, e))?;
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
        PalwStagedCourtRowV1::Tokenizer(c) => (c.terms.material_until_daa, vec![(c.reserver, c.terms.material_until_daa, key)]),
        PalwStagedCourtRowV1::Prefix(c) => (c.terms.material_until_daa, vec![(c.reserver, c.terms.material_until_daa, key)]),
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
        if let Some((claim, owner, _, _)) = row.owner_accounting_v1() {
            state.staged_by_owner.remove(&(owner, claim));
        }
        for hold in holds {
            state.staged_holds.remove(&hold);
        }
    }
    if let Some(row) = new {
        let (due, holds) = index_rows(key, row);
        state.staged_due.insert((due, key));
        if let Some((claim, owner, _, _)) = row.owner_accounting_v1() {
            state.staged_by_owner.insert((owner, claim));
        }
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
        if let Some((claim, owner, _, _)) = row.owner_accounting_v1() {
            state.staged_by_owner.insert((owner, claim));
        }
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
