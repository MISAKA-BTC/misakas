//! Rooted, journaled transport ownership. Only a live reserver can allocate its own row.
use super::*;
use crate::palw_terminal_carriage_v1::*;

fn refusal(id: Hash64, why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::ObjectiveOffenceRefused(id, why.into())
}
impl PalwChainStateV2 {
    pub fn terminal_pending_v1(&self, key: &PalwTerminalKeyV1) -> Option<&PalwPendingTerminalV1> {
        self.terminal_evidence.get(key)
    }
    pub(super) fn assert_terminal_evidence_v1(&self) -> Result<(), PalwStateV2Error> {
        for (key, row) in &self.terminal_evidence {
            if *key != row.declaration.key() {
                return Err(PalwStateV2Error::CarriageInconsistent("terminal owner differs from its key".into()));
            }
            row.check(PALW_TERMINAL_MAX_BYTES_V1).map_err(PalwStateV2Error::CarriageInconsistent)?;
            let record = self
                .legacy_dispute_v1(&key.0)
                .ok_or_else(|| PalwStateV2Error::CarriageInconsistent("terminal assembly lacks a reservation record".into()))?;
            if !record.live.get(&key.1).is_some_and(|r| r.reserved_daa == row.declaration.reserved_daa)
                || row.deadline_daa != record.hard_deadline_daa
            {
                return Err(PalwStateV2Error::CarriageInconsistent(
                    "terminal assembly extends or changes reservation ownership".into(),
                ));
            }
        }
        Ok(())
    }
}
impl TransitionBuilder<'_> {
    fn write_terminal_v1(&mut self, key: PalwTerminalKeyV1, new: Option<PalwPendingTerminalV1>) {
        let old = match &new {
            Some(r) => self.state.terminal_evidence.insert(key, r.clone()),
            None => self.state.terminal_evidence.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::TerminalEvidence { key, old, new });
        }
    }
}
fn budget(b: &TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<u64, PalwStateV2Error> {
    if !b.params.legacy_public_filer_active_at(ctx.daa_score) {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    b.extras.terminal_evidence_max_bytes.map(|n| n.min(PALW_TERMINAL_MAX_BYTES_V1)).ok_or(PalwStateV2Error::ObjectiveOffenceDormant)
}
pub(super) fn declare(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    d: &PalwTerminalDeclarationV1,
) -> Result<(), PalwStateV2Error> {
    d.check(budget(b, ctx)?).map_err(|e| refusal(d.evidence_id, e))?;
    let record =
        b.state.legacy_dispute_v1(&d.claim).ok_or_else(|| refusal(d.evidence_id, "terminal declaration has no live reservation"))?;
    if !record.live.get(&d.reserver).is_some_and(|r| r.reserved_daa == d.reserved_daa) {
        return Err(refusal(d.evidence_id, "terminal declaration belongs to another reservation"));
    }
    let deadline = record.hard_deadline_daa;
    d.check_window(ctx.daa_score, deadline).map_err(|e| refusal(d.evidence_id, e))?;
    if b.state.terminal_evidence.contains_key(&d.key()) {
        return Err(refusal(d.evidence_id, "reserver already owns an immutable terminal assembly"));
    }
    b.write_terminal_v1(
        d.key(),
        Some(PalwPendingTerminalV1 { declaration: d.clone(), deadline_daa: deadline, parts: BTreeMap::new() }),
    );
    Ok(())
}
pub(super) fn part(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, p: &PalwTerminalPartV1) -> Result<(), PalwStateV2Error> {
    let maximum = budget(b, ctx)?;
    let key = (p.claim, p.reserver);
    let row =
        b.state.terminal_evidence.get(&key).ok_or_else(|| refusal(p.evidence_id, "terminal part lacks its owned declaration"))?;
    if ctx.daa_score >= row.deadline_daa
        || !b
            .state
            .legacy_dispute_reservation_v1(&p.claim, &p.reserver)
            .is_some_and(|r| r.reserved_daa == row.declaration.reserved_daa)
    {
        return Err(refusal(p.evidence_id, "terminal reservation closed or expired"));
    }
    if let Some(evidence) = row.preview(p, maximum).map_err(|e| refusal(p.evidence_id, e))? {
        let d = row.declaration.clone();
        // Binding identity and the named claim are checked before the ordinary conviction consumer.
        let target_claim = match d.kind {
            crate::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted => {
                borsh::from_slice::<crate::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1>(&evidence).map(|e| e.claim_id)
            }
            crate::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2 => {
                borsh::from_slice::<crate::palw_offence_attribution_v1::PalwPanelFalseValidEvidenceV2>(&evidence).map(|e| e.claim_id)
            }
            crate::palw_offence_v1::PalwOffenceKindV1::TirIdentityMismatch => {
                borsh::from_slice::<crate::palw_offence_attribution_v1::PalwTirIdentityEvidenceV1>(&evidence).map(|e| e.claim_id)
            }
            _ => return Err(refusal(p.evidence_id, "terminal kind is not fileable")),
        }
        .map_err(|e| refusal(p.evidence_id, e.to_string()))?;
        if target_claim != d.claim {
            return Err(refusal(p.evidence_id, "terminal evidence names another claim"));
        }
        if !b.params.fp_decode_constraint_active_at(ctx.daa_score)
            && crate::palw_terminal_carriage_v1::palw_terminal_is_constraint_v1(d.kind, &evidence)
        {
            return Err(refusal(p.evidence_id, "constrained terminal proof below its decode-constraint fence"));
        }
        crate::palw_terminal_carriage_v1::palw_terminal_adjudicate_v1(
            &b.state,
            &d,
            &evidence,
            maximum,
            b.params.fp_decode_rules_at(ctx.daa_score),
            b.identity_rules_v1(ctx.daa_score),
            b.session_rule_v1(ctx.daa_score),
            None,
        )
        .map_err(|e| refusal(p.evidence_id, e))?;
        b.consume_objective_offence_with_budget_v1(
            ctx,
            d.kind,
            d.accused,
            d.evidence_id,
            &evidence,
            crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::ReservedTerminal { court_maximum: maximum },
        )?;
        b.write_terminal_v1(key, None);
        sweep(b, ctx);
    } else {
        let bytes = p.bytes.clone();
        b.state.terminal_evidence.get_mut(&key).expect("validated owner").parts.insert(p.index, bytes.clone());
        b.entries.push(PalwDeltaEntryV2::TerminalPart { key, index: p.index, old: None, new: Some(bytes) });
    }
    Ok(())
}
pub(super) fn sweep(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    let expired: Vec<_> = b
        .state
        .terminal_evidence
        .iter()
        .filter(|(key, row)| {
            ctx.daa_score >= row.deadline_daa
                || !b
                    .state
                    .legacy_dispute_reservation_v1(&key.0, &key.1)
                    .is_some_and(|r| r.reserved_daa == row.declaration.reserved_daa)
        })
        .map(|(k, _)| *k)
        .collect();
    for key in expired {
        b.write_terminal_v1(key, None);
    }
}
