//! Finite native consumer service is separate from the physical cash/material clock.
use super::*;

impl PalwChainStateV2 {
    /// Exact native claim ID. Terms survive DA pauses, redraws, restart and reorg;
    /// an old accepted claim never acquires a retroactive signed contract.
    pub fn paid_consumer_contract_v1(&self, claim: &Hash64) -> Option<&PalwPaidConsumerContractV1> {
        self.bond_budget.as_ref()?.consumer_contract_v1(*claim)
    }
    pub fn paid_consumer_terms_v1(&self, claim: &Hash64) -> Option<PalwPaidConsumerTermsV1> {
        self.paid_consumer_contract_v1(claim).map(|row| row.terms)
    }
    pub fn paid_consumer_service_fits_v1(&self, claim: &Hash64, now: u64, through: u64) -> bool {
        self.paid_consumer_contract_v1(claim)
            .is_none_or(|row| matches!(row.status, PalwPaidConsumerStatusV1::Live) && row.terms.admits_service(now, through))
    }
    pub fn require_paid_consumer_dispute_v1(&self, claim: &Hash64, opened: u64, through: u64) -> Result<(), PalwStateV2Error> {
        if let Some(row) = self.paid_consumer_contract_v1(claim)
            && !row.terms.admits_new_dispute(opened, through)
        {
            return Err(PalwStateV2Error::ExecV2Refused("new native dispute exceeds the immutable paid consumer contract".into()));
        }
        Ok(())
    }
    pub fn require_paid_consumer_backstop_v1(&self, claim: &Hash64, through: u64) -> Result<(), PalwStateV2Error> {
        if self.paid_consumer_terms_v1(claim).is_some_and(|terms| through > terms.backstop_until) {
            return Err(PalwStateV2Error::ExecV2Refused("native obligation exceeds the immutable paid consumer backstop".into()));
        }
        Ok(())
    }
}

impl TransitionBuilder<'_> {
    pub(in crate::palw_state_v2) fn plan_paid_consumer_v1(
        &self,
        id: Hash64,
        claim: &PalwClaimStateV2,
    ) -> Result<Option<PalwPaidConsumerContractV1>, PalwStateV2Error> {
        if !self.params.paid_consumer_lifecycle_active_at(claim.accepted_daa) {
            return Ok(None);
        }
        if self.state.paid_consumer_contract_v1(&id).is_some() {
            return Err(PalwStateV2Error::DuplicateClaim(id));
        }
        let policy = &self.params.paid_consumer_lifecycle().expect("active mirror").policy;
        let terms = policy.terms(claim.accepted_daa).map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
        if claim.trace_retention_daa < terms.material_until {
            return Err(PalwStateV2Error::ExecV2Refused(
                "paid consumer requires already authorized material through its immutable contract".into(),
            ));
        }
        let receipt = self.params.receipt_window_for_claim_v1(
            &self.state,
            &claim.class_id,
            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
            claim.accepted_daa,
        );
        let earliest_final = claim
            .accepted_daa
            .checked_add(self.params.window_bind())
            .and_then(|daa| daa.checked_add(receipt))
            .and_then(|daa| daa.checked_add(self.params.window_challenge_at(claim.accepted_daa)))
            .and_then(|daa| daa.checked_add(1))
            .ok_or(PalwStateV2Error::Overflow("paid consumer minimum service"))?;
        if !terms.admits_service(claim.accepted_daa, earliest_final) {
            return Err(PalwStateV2Error::ExecV2Refused("paid consumer service cannot fit this class's whole minimum path".into()));
        }
        Ok(Some(PalwPaidConsumerContractV1 { terms, status: PalwPaidConsumerStatusV1::Live }))
    }
    pub(in crate::palw_state_v2) fn commit_paid_consumer_v1(
        &mut self,
        id: Hash64,
        row: PalwPaidConsumerContractV1,
    ) -> Result<(), PalwStateV2Error> {
        self.bond_budget_op(|budget, _, j| budget.put_consumer_contract_v1(id, Some(row), j))
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("paid consumer engine missing".into()))?
            .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))
    }
    pub(in crate::palw_state_v2) fn mark_paid_consumer_terminal_v1(&mut self, id: Hash64, daa: u64) -> Result<(), PalwStateV2Error> {
        let Some(mut row) = self.state.paid_consumer_contract_v1(&id).copied() else { return Ok(()) };
        // A post-Final conviction never resets the first terminal clock or terms.
        if matches!(row.status, PalwPaidConsumerStatusV1::Terminal { .. }) {
            return Ok(());
        }
        row.status = PalwPaidConsumerStatusV1::Terminal { daa };
        self.commit_paid_consumer_v1(id, row)
    }
    pub(in crate::palw_state_v2) fn forget_paid_consumer_v1(&mut self, id: Hash64) -> Result<(), PalwStateV2Error> {
        if self.state.paid_consumer_contract_v1(&id).is_none() {
            return Ok(());
        }
        self.bond_budget_op(|budget, _, j| budget.put_consumer_contract_v1(id, None, j))
            .expect("contract has engine")
            .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))
    }
    pub(in crate::palw_state_v2) fn require_paid_consumer_bind_v1(
        &self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.state.paid_consumer_contract_v1(&id).is_none() {
            return Ok(());
        }
        let receipt = self.params.receipt_window_for_claim_v1(
            &self.state,
            &claim.class_id,
            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim),
            now,
        );
        let through = now
            .checked_add(receipt)
            .and_then(|daa| daa.checked_add(self.params.window_challenge_at(now)))
            .and_then(|daa| daa.checked_add(1))
            .ok_or(PalwStateV2Error::Overflow("paid consumer binding path"))?;
        if !self.state.paid_consumer_service_fits_v1(&id, now, through) {
            return Err(PalwStateV2Error::ExecV2Refused("Panel assignment cannot fit the immutable paid consumer service".into()));
        }
        Ok(())
    }
}

/// Expiry is non-fraud. Accepted accusations finish on their already frozen
/// backstop before the neutral void; funded monetary rights use their ordinary
/// cancellation/recovery paths. No physical payment deadline is manufactured.
pub(in crate::palw_state_v2) fn sweep_paid_consumers_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.paid_consumer_lifecycle_active_at(ctx.daa_score) {
        return Ok(());
    }
    let book = builder
        .state
        .bond_budget
        .as_ref()
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("finite consumer engine missing".into()))?;
    let due = book.due_consumer_contracts_v1(ctx.daa_score);
    for id in due {
        let mut row = builder.state.paid_consumer_contract_v1(&id).copied().expect("due index");
        row.status = PalwPaidConsumerStatusV1::Exhausted { daa: ctx.daa_score };
        builder.commit_paid_consumer_v1(id, row)?;
    }
    let exhausted: Vec<_> = builder.state.bond_budget.as_ref().expect("engine").exhausted_consumer_contracts_v1().copied().collect();
    for id in exhausted {
        let Some(claim) = builder.state.claims.get(&id).cloned() else {
            return Err(PalwStateV2Error::CarriageInconsistent("finite consumer has no native claim".into()));
        };
        if claim.phase.is_terminal() {
            builder.mark_paid_consumer_terminal_v1(id, ctx.daa_score)?;
            continue;
        }
        if builder.state.palw_accusation_pending_v1(&id) {
            continue;
        }
        builder.void_claim(id, &claim, ctx.daa_score, PalwVoidReasonV2::PanelUnavailable)?;
    }
    Ok(())
}
