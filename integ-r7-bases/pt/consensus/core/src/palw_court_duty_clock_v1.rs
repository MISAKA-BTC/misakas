//! RFC-0028 isolated candidate: retained native prerequisite history, never
//! permission to default. No production profile/admission installs the binding.
use crate::{Hash64, BlockHash};
use crate::palw_state_v2::{PalwBondKeyV2, PalwBlockContextV2, PalwChainStateV2};
use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1 as Key;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CourtDutyClockV1 {
    pub version: u16,
    pub profile_id: Hash64,
    pub duty_id: Hash64,
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub acquisition: crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    pub answerer: PalwBondKeyV2,
    pub demand_id: Hash64,
    pub turn: u64,
    pub predecessor_block: BlockHash,
    pub predecessor_daa: u64,
    pub first_eligible_daa: u64,
    pub response_deadline_daa: u64,
    pub original_hard_daa: u64,
    pub material_until_daa: u64,
}
impl CourtDutyClockV1 {
    pub(crate) fn from_native(
        duty: &crate::palw_court_tir_requests_v1::TirNativeResponseDutyV1,
        ctx: &PalwBlockContextV2,
        profile_id: Hash64,
        material_until_daa: u64,
    ) -> Result<Self, String> {
        let clock = Self {
            version: 1, profile_id, duty_id: duty.id, claim: duty.claim,
            allocation_id: duty.allocation_id, source_id: duty.source_id,
            acquisition: duty.key, answerer: duty.answerer, demand_id: duty.demand_id,
            turn: duty.turn, predecessor_block: ctx.block, predecessor_daa: ctx.daa_score,
            first_eligible_daa: ctx.daa_score.checked_add(1).ok_or("duty eligibility successor overflow")?,
            response_deadline_daa: duty.respond_until_daa, original_hard_daa: duty.original_hard_daa,
            material_until_daa,
        };
        clock.check(&Key::CourtDutyClock(duty.id))?;
        Ok(clock)
    }
    pub fn check(&self, key: &Key) -> Result<(), String> {
        if self.version != 1 || *key != Key::CourtDutyClock(self.duty_id)
            || !matches!(self.acquisition, Key::TirAcquisition(claim, ..) if claim == self.claim)
            || self.predecessor_daa.checked_add(1) != Some(self.first_eligible_daa)
            || self.predecessor_daa >= self.response_deadline_daa
        {
            return Err("invalid native duty clock shape/key/version".into());
        }
        // No requirement that an eligible future unit exists before deadline.
        // Such a requirement would reject an otherwise legal latest response.
        Ok(())
    }
    pub(crate) fn check_service_binding(&self, service: &crate::palw_court_service_state_v2::CourtServiceStateV2) -> Result<(), String> {
        let binding = service.claims.get(&self.claim).ok_or("duty clock lost original binding")?;
        if service.profile_id != self.profile_id
            || (self.allocation_id, self.source_id, self.original_hard_daa)
                != (binding.allocation_id, binding.source_id, binding.hard_deadline_daa) {
            return Err("duty clock substitutes service binding".into());
        }
        Ok(())
    }
    pub fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&Key::CourtDutyClock(self.duty_id))?;
        let service = state.court_service_state_v1().ok_or("duty clock lost service binding")?;
        self.check_service_binding(service)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, self.claim)
            .ok_or("duty clock lost original frame")?;
        let plan = frame.plan_v1();
        let Some(crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::TirAcquisition(row)) = state.staged_court_row_v1(&self.acquisition) else {
            return Err("duty clock lost actual acquisition".into());
        };
        row.check_structure_v1(&self.acquisition)?;
        let c = row.owned.contract_v1();
        if (c.claim, c.bundle_id, c.source_id) != (self.claim, self.allocation_id, self.source_id)
            || !row.owned.response_custodians_v1().contains(&self.answerer)
            || (self.allocation_id, self.source_id) != (plan.allocation_id, plan.source_id)
            || self.predecessor_daa < plan.terms.accepted_daa
            || self.material_until_daa != plan.terms.material_until_daa
        {
            return Err("duty clock substitutes original service scope".into());
        }
        Ok(())
    }
    /// Necessary timing check only. No packet mapping, actual supply or personal
    /// blame follows from this predicate. The full causal graph is still absent.
    pub fn stage_started_before_unit(&self, daa: u64) -> bool {
        daa >= self.first_eligible_daa && daa < self.response_deadline_daa && daa < self.original_hard_daa
    }
}
