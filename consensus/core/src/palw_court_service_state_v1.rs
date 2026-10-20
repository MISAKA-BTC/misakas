//! Proposed rooted adapter for RFC-0026 service accounting. No production
//! original-admission or block-supply path creates these rows yet. In particular
//! this adapter NEVER grants default permission from quota alone.
use crate::{
    Hash64,
    palw_court_service_v1::{ServiceAssessment, ServiceReservations},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ServiceClaimBinding {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub original_accepted_daa: u64,
    pub hard_deadline_daa: u64,
    pub packet_ids: BTreeSet<[u8; 64]>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtServiceState {
    pub version: u16,
    pub profile_id: Hash64,
    pub book: ServiceReservations,
    pub claims: BTreeMap<Hash64, ServiceClaimBinding>,
}
impl CourtServiceState {
    pub fn check(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported court service state version".into());
        }
        self.book.check().map_err(str::to_owned)?;
        let mut owned = BTreeSet::new();
        for (claim, binding) in &self.claims {
            if claim != &binding.claim
                || binding.packet_ids.is_empty()
                || binding.original_accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
                    != Some(binding.hard_deadline_daa)
            {
                return Err("invalid original service binding".into());
            }
            for id in &binding.packet_ids {
                let packet = self.book.packets.get(id).ok_or("binding names absent service packet")?;
                if !owned.insert(*id)
                    || packet.release_daa <= binding.original_accepted_daa
                    || packet.service_before > binding.hard_deadline_daa
                {
                    return Err("service packet crosses original scope or deadline".into());
                }
            }
        }
        if owned.len() != self.book.packets.len() {
            return Err("unbound service packet".into());
        }
        Ok(())
    }
    /// Only a denial guard: signatures, response eligibility and adopted
    /// inclusion assumptions are NOT replaced by a ServiceAssessment.
    pub fn default_hold(&self, frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1) -> Result<(), &'static str> {
        let plan = frame.plan_v1();
        let Some(binding) = self.claims.get(&plan.original_claim) else { return Ok(()) };
        if self.check().is_err() {
            return Err("court service state is inconsistent");
        }
        if binding.allocation_id != plan.allocation_id
            || binding.source_id != plan.source_id
            || binding.original_accepted_daa != plan.terms.accepted_daa
        {
            return Err("court service binding substitutes original scope");
        }
        for id in &binding.packet_ids {
            match self.book.assess(id) {
                ServiceAssessment::UnknownObligation => return Err("court service obligation is missing"),
                ServiceAssessment::Pending => return Err("court service opportunity is still pending"),
                ServiceAssessment::MissingSupply => return Err("court service supply is missing; no party default"),
                ServiceAssessment::OpportunityProvided => {}
            }
        }
        Err("court service inclusion assumptions and duty eligibility are not adopted; no party default")
    }
}
