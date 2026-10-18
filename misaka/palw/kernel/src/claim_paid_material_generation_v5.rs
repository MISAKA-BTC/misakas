//! A separately signed material generation, never a new paid draw identity.
//! The original V2 enrollment and detached signature remain byte-identical.
use crate::hash::Digest;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PaidClaimMaterialGenerationV5 {
    pub version: u16,
    pub enrollment: Digest,
    pub root: Digest,
    pub initial_consent: Digest,
    pub recovery: Digest,
    pub current_consent: Digest,
    pub sequence: u64,
}
impl PaidClaimMaterialGenerationV5 {
    pub fn validate(&self, e: &crate::claim_paid_v2::PaidClaimEnrollmentV2) -> Result<(), String> {
        if self.version != 5
            || e.version != 2
            || self.enrollment != e.id()
            || self.root != e.base_claim
            || self.initial_consent != e.material_consent
            || self.sequence == 0
            || self.root == [0; 64]
            || self.initial_consent == [0; 64]
            || self.recovery == [0; 64]
            || self.current_consent == [0; 64]
            || self.current_consent == self.initial_consent
        {
            return Err("V5 material generation changes the original paid enrollment or is not a replacement".into());
        }
        Ok(())
    }
}
