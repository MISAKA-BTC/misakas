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
pub struct CourtServiceStateV2 {
    pub version: u16,
    pub profile_id: Hash64,
    pub source_profile_id: Hash64,
    pub book: ServiceReservations,
    pub claims: BTreeMap<Hash64, ServiceClaimBinding>,
}
impl CourtServiceStateV2 {
    pub fn check(&self) -> Result<(), String> {
        if self.version != 2 || self.profile_id != court_service_profile_id_v2(self.source_profile_id) {
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

/// A minimum signed-custody constraint for the proposed service binding, not
/// complete dependency admission. Reads accepted contracts; never extends them.
pub(crate) fn require_tir_service_custody_window_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    required_until: u64,
) -> Result<(), String> {
    let plan = frame.plan_v1();
    if frame.custody_v1().pledge_v1().material_until_daa < required_until {
        return Err("service hard deadline exceeds signed TIR custody".into());
    }
    if plan.semantic.is_some() {
        let bundle = state.staged_bundle_contract_v1(plan.original_claim)
            .ok_or("service semantic dependency contract is missing")?;
        let pledge = &bundle.history_custody.pledge;
        if pledge.claim != plan.original_claim
            || Some(pledge.resource_plan_id) != plan.semantic_resource_plan_id
            || pledge.execution_root != frame.original_v1().execution_root
            || pledge.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_v1(frame.original_v1())
            || pledge.producer != frame.custody_v1().pledge_v1().producer
        {
            return Err("service semantic custody substitutes its original scope".into());
        }
        if pledge.material_until_daa < required_until {
            return Err("service hard deadline exceeds signed semantic custody".into());
        }
    }
    Ok(())
}

impl CourtServiceStateV2 {
    /// Contextual validation is separate from the standalone row codec check.
    /// A recovery binding may not outlive or substitute its immutable frame.
    pub fn check_context(&self, state: &crate::palw_state_v2::PalwChainStateV2) -> Result<(), String> {
        self.check()?;
        for binding in self.claims.values() {
            let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, binding.claim)
                .ok_or("service recovery lost its original frame")?;
            let plan = frame.plan_v1();
            if (plan.allocation_id, plan.source_id, plan.terms.accepted_daa)
                != (binding.allocation_id, binding.source_id, binding.original_accepted_daa)
            {
                return Err("service recovery substitutes original frame".into());
            }
            require_tir_service_custody_window_v1(state, frame, binding.hard_deadline_daa)?;
            let claim = state.claim(&binding.claim).ok_or("service recovery lost its claim")?;
            if matches!(claim.phase, crate::palw_state_v2::PalwClaimPhaseV2::Final { .. }) {
                return Err("unresolved service claim cannot already be Final".into());
            }
        }
        Ok(())
    }
}
impl crate::palw_state_v2::PalwChainStateV2 {
    pub fn court_service_state_v1(&self) -> Option<&CourtServiceStateV2> {
        match self.staged_court_row_v1(&crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtService) {
            Some(crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtService(row)) => Some(row.as_ref()),
            _ => None,
        }
    }
    /// All bindings remain unresolved in this candidate. Quota alone cannot
    /// discharge a duty, and no unadopted release flag or wall-clock expiry can.
    pub fn court_service_holds_claim_v1(&self, claim: &Hash64) -> bool {
        self.court_service_state_v1().is_some_and(|service| service.claims.contains_key(claim))
    }
    pub fn court_service_holds_bond_v1(&self, bond: &crate::palw_state_v2::PalwBondKeyV2) -> bool {
        self.court_service_state_v1().is_some_and(|service| {
            service.claims.keys().any(|claim| {
                // Missing frames are invalid on write/load; fail closed if queried
                // on a not-yet-validated candidate rather than releasing collateral.
                crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(self, *claim)
                    .is_none_or(|frame| frame.custody_v1().liable_signers_v1().contains(bond))
            })
        })
    }
    pub fn court_service_holds_material_v1(&self, key: &crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1) -> bool {
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        let claim = match key {
            Key::TirClaim(claim) | Key::TirAcquisition(claim, ..) | Key::TirPreparation(claim, ..) | Key::TirWitness(claim, ..) => {
                Some(*claim)
            }
            Key::CourtDutyClock(_) => match self.staged_court_row_v1(key) {
                Some(Row::CourtDutyClock(clock)) => Some(clock.claim),
                _ => None,
            },
            Key::TirRawChunk(_) | Key::TirAcceptedBody(_) => {
                let acquisition = match self.staged_court_row_v1(key) {
                    Some(Row::TirRawChunk(row)) => Some(row.acquisition),
                    Some(Row::TirAcceptedBody(row)) => Some(row.acquisition),
                    _ => None,
                };
                match acquisition {
                    Some(Key::TirAcquisition(claim, ..)) => Some(claim),
                    _ => None,
                }
            }
            _ => None,
        };
        claim.is_some_and(|claim| self.court_service_holds_claim_v1(&claim))
    }
}

/// Recovery report only; not a wire permission, terminal effect or default verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimServiceRecovery {
    pub claim: Hash64,
    pub result: Result<usize, crate::palw_court_service_v1::ServiceRefusal>,
}
impl CourtServiceStateV2 {
    /// A canonical sweep, not a requester-selected queue. Failure is atomic per
    /// claim; it cannot evict another claim or keep a smaller recoverable claim
    /// from using residual quota. All unresolved holds remain in either case.
    pub fn repair_missing_supply(&mut self) -> Result<Vec<ClaimServiceRecovery>, String> {
        self.check()?;
        let mut order: Vec<_> =
            self.claims.values().map(|binding| (binding.hard_deadline_daa, binding.original_accepted_daa, binding.claim)).collect();
        order.sort();
        let mut report = Vec::new();
        for (hard, _, claim) in order {
            let result = self.book.repair_missing(&self.claims[&claim].packet_ids, hard);
            if result != Ok(0) {
                report.push(ClaimServiceRecovery { claim, result });
            }
        }
        self.check()?;
        Ok(report)
    }
}

/// New isolated candidate namespace. Not an adopted profile record.
pub fn court_service_profile_id_v2(source: Hash64) -> Hash64 {
    Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64)
        .key(b"misaka-palw/court-service/v2").hash(&borsh::to_vec(&source).expect("hash serialization"))
        .as_bytes().try_into().expect("64"))
}
