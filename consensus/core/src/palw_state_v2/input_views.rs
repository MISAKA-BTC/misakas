//! Borrowed input boundaries over the existing resolved extras. These views do
//! not resolve forks, read a current tip, mutate extras, or alter legacy defaults.
//! Production resolution remains `palw_transition_extras_for(point)`; historical
//! callers keep the explicit extras/defaults supplied by their existing entry.
use super::*;

/// Accepted block facts, without access to rule flags or claim-origin heights.
#[derive(Clone, Copy)]
pub struct PalwBlockFactsV1<'a> {
    extras: &'a PalwTransitionExtrasV1,
}
impl PalwBlockFactsV1<'_> {
    pub fn round_fee_incomes(&self) -> &[crate::palw_bond_budget_v1::PalwRoundFeeIncomeV1] {
        &self.extras.round_fee_incomes
    }
    pub fn merged_reds(&self) -> &BTreeSet<BlockHash> {
        &self.extras.merged_reds
    }
    pub fn own_job_anchor(&self) -> Hash64 {
        self.extras.own_job_anchor
    }
}

/// Rules evaluated for one block. There is deliberately no claim pricing API:
/// a later block must not change the unit of a previously accepted claim.
#[derive(Clone, Copy)]
pub struct PalwRulesAtBlockV1<'a> {
    extras: &'a PalwTransitionExtrasV1,
    point: &'a PalwBlockContextV2,
}
impl PalwRulesAtBlockV1<'_> {
    pub fn round_lane(&self) -> Option<crate::palw_execution_lane_v1::PalwExecLaneFoldV1> {
        self.extras.round_lane
    }

    pub fn work_target_active(&self) -> bool {
        self.extras.work_target_active
    }
    pub fn single_lottery_active(&self) -> bool {
        self.extras.single_lottery_active
    }
    /// A fresh attempt at this block, rather than an existing claim.
    pub fn canonical_work_active(&self) -> bool {
        self.extras.canonical_work_daa.is_some_and(|height| self.point.daa_score >= height)
    }
    pub fn admission_independence_active(&self) -> bool {
        self.extras.admission_independence_at(self.point.daa_score)
    }
    pub fn fp_derived_work_active(&self) -> bool {
        self.extras.fp_derived_work_at(self.point.daa_score)
    }
    pub fn objective_offence_active(&self) -> bool {
        self.extras.objective_offence_at(self.point.daa_score)
    }
    pub fn seat_gate_possession_active(&self) -> bool {
        self.extras.seat_gate_possession_at(self.point.daa_score)
    }
}

/// Rules bound to a claim's recorded origin. No current block is accepted by
/// these operations; the established pricing and independence expressions stay
/// authoritative and retain source/class eligibility checks.
#[derive(Clone, Copy)]
pub struct PalwRulesForClaimV1<'a> {
    extras: &'a PalwTransitionExtrasV1,
    claim: &'a PalwClaimStateV2,
}
impl PalwRulesForClaimV1<'_> {
    pub fn prices_in_compute(&self, params: &PalwStateParamsV2) -> bool {
        PalwFpPricingV1::of(params, self.extras.canonical_work_daa).prices_in_compute(self.claim)
    }
    pub fn outsider_judged(&self, state: &PalwChainStateV2) -> bool {
        palw_claim_is_outsider_judged_v1(state, self.claim, self.extras.admission_independence_daa)
    }
    pub fn canonical_weight(&self, state: &PalwChainStateV2) -> Option<u128> {
        state.palw_claim_canonical_weight_v1(self.claim, self.extras.canonical_work_daa)
    }
}

impl PalwTransitionExtrasV1 {
    pub fn block_facts(&self) -> PalwBlockFactsV1<'_> {
        PalwBlockFactsV1 { extras: self }
    }
    pub fn rules_at<'a>(&'a self, point: &'a PalwBlockContextV2) -> PalwRulesAtBlockV1<'a> {
        PalwRulesAtBlockV1 { extras: self, point }
    }
    pub fn rules_for_claim<'a>(&'a self, claim: &'a PalwClaimStateV2) -> PalwRulesForClaimV1<'a> {
        PalwRulesForClaimV1 { extras: self, claim }
    }
}
