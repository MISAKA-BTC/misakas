//! The assembly row is owned by an existing DA obligation, never by a free digest or a global slot.
use super::*;
use crate::palw_da_rcore_v1::PalwDaUnitV1;
use crate::palw_legacy_answer_chunks_v2::*;

fn unit_is_owed(state: &PalwChainStateV2, key: &PalwLegacyAnswerKeyV2, fp_pins_active: bool) -> bool {
    let unit = PalwDaUnitV1::LegacyHeldV2(key.1);
    let Some(claim) = state.claims.get(&key.0) else { return false };
    let Some(record) = state.da_claims.get(&key.0) else { return false };
    state.da_sessions_of(&key.0).any(|(_, s)| s.units.contains(&unit))
        && !state.palw_da_unit_available_v1(
            &key.0,
            record,
            &unit,
            crate::palw_da_rcore_v1::palw_da_in_run_rows_v1(claim, fp_pins_active),
        )
}

impl PalwChainStateV2 {
    pub fn legacy_answer_pending_v2(&self, key: &PalwLegacyAnswerKeyV2) -> Option<&PalwPendingLegacyAnswerV2> {
        self.legacy_answers.get(key)
    }
    pub(super) fn assert_legacy_answers_v2(&self) -> Result<(), PalwStateV2Error> {
        for (key, row) in &self.legacy_answers {
            if *key != row.declaration.key() {
                return Err(PalwStateV2Error::CarriageInconsistent("legacy answer owner/key differs".into()));
            }
            row.check(u64::from(PALW_LEGACY_ANSWER_MAX_CHUNKS_V2) * u64::from(PALW_LEGACY_ANSWER_CHUNK_BYTES_V2))
                .map_err(PalwStateV2Error::CarriageInconsistent)?;
            // Held units do not depend on the FP event-row count used by the availability predicate.
            if !unit_is_owed(self, key, false) {
                return Err(PalwStateV2Error::CarriageInconsistent("legacy answer has no live DA obligation".into()));
            }
            if self
                .da_sessions_of(&key.0)
                .filter(|(_, s)| s.units.contains(&PalwDaUnitV1::LegacyHeldV2(key.1)))
                .any(|(_, s)| row.deadline_daa > s.deadline_daa)
            {
                return Err(PalwStateV2Error::CarriageInconsistent("legacy answer extends a DA obligation's deadline".into()));
            }
        }
        Ok(())
    }
}

impl TransitionBuilder<'_> {
    fn write_legacy_answer_v2(&mut self, key: PalwLegacyAnswerKeyV2, new: Option<PalwPendingLegacyAnswerV2>) {
        let old = match &new {
            Some(row) => self.state.legacy_answers.insert(key, row.clone()),
            None => self.state.legacy_answers.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::LegacyAnswer { key, old, new });
        }
    }
}

fn refused(claim: Hash64, why: String) -> PalwStateV2Error {
    PalwStateV2Error::LegacyHeldDaRefused { claim, why }
}

pub(super) fn declare(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    d: &PalwLegacyAnswerDeclarationV2,
) -> Result<(), PalwStateV2Error> {
    if !b.extras.legacy_held_da_v2_active {
        return Err(PalwStateV2Error::LegacyHeldDaDormant);
    }
    d.check(u64::from(PALW_LEGACY_ANSWER_MAX_CHUNKS_V2) * u64::from(PALW_LEGACY_ANSWER_CHUNK_BYTES_V2))
        .map_err(|e| refused(d.claim, e))?;
    let unit = PalwDaUnitV1::LegacyHeldV2(d.unit);
    da_answer_admitted_v1(b, ctx, d.claim, &unit, d.discloser)?;
    let deadline = b
        .state
        .da_sessions_of(&d.claim)
        .filter(|(_, s)| s.units.contains(&unit))
        .map(|(_, s)| s.deadline_daa)
        .min()
        .ok_or(PalwStateV2Error::DaUnitNotDemanded(d.claim))?;
    // This does not extend the DA clock or turn a partial answer into a refutation.
    if ctx.daa_score.saturating_add(PALW_COURT_CLOSE_INCLUSION_MARGIN.saturating_mul(d.chunk_digests.len() as u64)) >= deadline {
        return Err(refused(d.claim, "not enough of the existing DA window remains to carry the declared chunks".into()));
    }
    if b.state.legacy_answers.contains_key(&d.key()) {
        return Err(refused(d.claim, "this discloser already has an assembly for the demanded unit".into()));
    }
    b.write_legacy_answer_v2(
        d.key(),
        Some(PalwPendingLegacyAnswerV2 { declaration: d.clone(), deadline_daa: deadline, parts: BTreeMap::new() }),
    );
    Ok(())
}

pub(super) fn chunk(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    c: &PalwLegacyAnswerChunkV2,
) -> Result<(), PalwStateV2Error> {
    if !b.extras.legacy_held_da_v2_active {
        return Err(PalwStateV2Error::LegacyHeldDaDormant);
    }
    da_answer_admitted_v1(b, ctx, c.claim, &PalwDaUnitV1::LegacyHeldV2(c.unit), c.discloser)?;
    let mut row = b
        .state
        .legacy_answers
        .get(&c.key())
        .cloned()
        .ok_or_else(|| refused(c.claim, "legacy answer chunk has no signed declaration".into()))?;
    if ctx.daa_score >= row.deadline_daa {
        return Err(refused(c.claim, "legacy answer assembly expired".into()));
    }
    if let Some(answer) = row.preview(c).map_err(|e| refused(c.claim, e))? {
        // The processor checks the original answer signature on completion. The fold applies
        // exactly the ordinary hash/identity predicate and DA-4 transition, not a second verdict.
        apply_legacy_held_answer_v2(b, ctx, &answer)?;
        b.write_legacy_answer_v2(c.key(), None);
    } else {
        row.parts.insert(c.index, c.bytes.clone());
        b.write_legacy_answer_v2(c.key(), Some(row));
    }
    sweep(b, ctx);
    Ok(())
}

pub(super) fn sweep(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    let drop: Vec<_> = b
        .state
        .legacy_answers
        .iter()
        .filter(|(key, row)| ctx.daa_score >= row.deadline_daa || !unit_is_owed(&b.state, key, b.extras.fp_da_pins_active))
        .map(|(key, _)| *key)
        .collect();
    for key in drop {
        b.write_legacy_answer_v2(key, None);
    }
}
