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
