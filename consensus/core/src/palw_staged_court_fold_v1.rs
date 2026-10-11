//! Prospective claim contract/continuation writers. All failures precede claim acceptance writes.
use super::*;
use crate::palw_staged_court_state_v1::*;
impl PalwChainStateV2 {
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
        self.staged_courts.values().any(|row| match row {
            PalwStagedCourtRowV1::Claim(c) => now < c.terms.bond_exit_after_daa && (c.producer == *bond || c.panel.contains(bond)),
            PalwStagedCourtRowV1::Constraint(c) => now < c.terms.stream_until_daa && c.reserver == *bond,
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
