//! Rooted prospective contract and owned continuation rows. Immutable contracts are stamped
//! at claim acceptance; mutable checkpoints never contain another copy of public job/grammar.
use crate::palw_constraint_stream_owned_v1::PalwOwnedConstraintStreamV1;
use crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PalwStagedCourtKeyV1 {
    Claim(Hash64),
    Owner(Hash64, PalwBondKeyV2),
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedClaimContractV1 {
    pub terms: PalwRenderingClaimTermsV1,
    pub producer: PalwBondKeyV2,
    /// All bound seats remain held even after their panel/claim rows retire.
    pub panel: std::collections::BTreeSet<PalwBondKeyV2>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwStagedCourtRowV1 {
    Claim(PalwStagedClaimContractV1),
    Constraint(Box<PalwOwnedConstraintStreamV1>),
}
impl PalwStagedCourtRowV1 {
    pub fn check(&self, key: &PalwStagedCourtKeyV1) -> Result<(), String> {
        match (key, self) {
            (PalwStagedCourtKeyV1::Claim(_), Self::Claim(c))
                if c.terms.accepted_daa < c.terms.open_until_daa
                    && c.terms.open_until_daa < c.terms.stream_until_daa
                    && c.terms.stream_until_daa < c.terms.collect_until_daa
                    && c.terms.collect_until_daa <= c.terms.material_until_daa
                    && c.terms.material_until_daa <= c.terms.bond_exit_after_daa =>
            {
                Ok(())
            }
            (PalwStagedCourtKeyV1::Owner(claim, bond), Self::Constraint(c)) if (*claim, *bond) == c.key() => c.check(),
            _ => Err("staged rooted row has inconsistent key/immutable contract".into()),
        }
    }
}

/// A reference to a completed ROOTED court, not a challenger-supplied parser checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedCertificateRefV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
}
impl PalwStagedCertificateRefV1 {
    pub fn resolve(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        expected_root: Hash64,
    ) -> Result<crate::palw_constraint_stream_court_v1::PalwConstraintStreamFaultV1, String> {
        state.staged_claim_contract_v1(&self.claim).ok_or("certificate claim has no prospective contract")?;
        let row = state.staged_owned_constraint_v1(self.claim, self.reserver).ok_or("certificate has no rooted owned court")?;
        if row.reserved_daa != self.reserved_daa || row.seed_id() != self.seed_id {
            return Err("certificate changes immutable owner/seed/reservation".into());
        }
        let fault = row.certificate()?.ok_or("rooted court completed without an objective fault")?;
        if fault.claim_id != self.claim
            || fault.reservation_owner != self.reserver
            || fault.reserved_daa != self.reserved_daa
            || fault.seed_id != self.seed_id
            || fault.execution_root != expected_root
        {
            return Err("rooted certificate does not convict the target claim's execution".into());
        }
        Ok(fault)
    }
}
pub fn palw_staged_reference_v1(kind: crate::palw_offence_v1::PalwOffenceKindV1, evidence: &[u8]) -> Option<PalwStagedCertificateRefV1> {
    use crate::palw_offence_attribution_v1::*;
    use crate::palw_offence_v1::{PalwOffenceKindV1 as K, PalwPanelContradictionV1 as C};
    let contradiction = match kind {
        K::ExecutorRefuted => borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).ok()?.contradiction,
        K::PanelFalseValidV2 => borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).ok()?.contradiction,
        _ => return None,
    };
    match contradiction { C::StagedCourt { reference } => Some(reference), _ => None }
}
pub fn palw_is_staged_evidence_v1(kind: crate::palw_offence_v1::PalwOffenceKindV1, evidence: &[u8]) -> bool {
    palw_staged_reference_v1(kind, evidence).is_some()
}
/// The accepting block's exact clock, never the parent tip or a caller's declared timestamp.
pub fn palw_staged_collection_open_v1(state: &crate::palw_state_v2::PalwChainStateV2, reference: &PalwStagedCertificateRefV1, accepting_daa: u64) -> bool {
    state.staged_claim_contract_v1(&reference.claim).is_some_and(|c|
        accepting_daa >= c.terms.accepted_daa && accepting_daa < c.terms.collect_until_daa)
}
