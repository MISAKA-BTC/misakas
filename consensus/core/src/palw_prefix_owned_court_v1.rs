//! Paid typed prefix court. Signature verification and rooted row admission belong to the
//! existing staged-carriage fold. This wrapper checks literal ownership, clocks, funding and
//! cumulative work on every transition; no supplied cache/state is accepted as a seed.
use crate::Hash64;
use crate::palw_offence_attribution_v1::PalwOffenceTargetV1;
use crate::palw_prefix_family_court_v1::{PalwPrefixCourtActionV1, PalwPrefixFamilyCourtV1};
use crate::palw_prefix_resources_v1::PalwPrefixResourcePlanV1;
use crate::palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwRenderingResourcePricesV1};
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedPrefixCourtV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub terms: PalwRenderingClaimTermsV1,
    pub rent_paid: u64,
    pub deposit_locked: u64,
    pub moves_used: u64,
    pub carrier_bytes_used: u64,
    seed_id: Hash64,
    execution_root: Hash64,
    maximum_carrier_bytes: u64,
    carrier_maximum: u64,
    court: PalwPrefixFamilyCourtV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixCourtFaultV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    pub execution_root: Hash64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedPrefixCheckpointV1 {
    court: PalwPrefixFamilyCourtV1,
    moves_used: u64,
    carrier_bytes_used: u64,
}
impl PalwOwnedPrefixCourtV1 {
    /// The outer adapter derives seed_id from the SIGNED literal staged seed and supplies
    /// actual claim/reservation rows, job, rule-owned ladder and retention/exit clocks.
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        seed_id: Hash64,
        actual_claim: Hash64,
        actual_reserver: PalwBondKeyV2,
        actual_reserved_daa: u64,
        binding: &crate::palw_step_leg::PalwStepBindingV2,
        actual_job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        target: &PalwOffenceTargetV1,
        claim_terms: PalwRenderingClaimTermsV1,
        prices: PalwRenderingResourcePricesV1,
        now: u64,
        available_capital: u64,
        carrier_maximum: u64,
        maximum_leaves: u64,
        seed_carrier_bytes: u64,
        material_until: u64,
        producer_exit_after: u64,
        panel_exit_after: &[u64],
    ) -> Result<Self, String> {
        if actual_claim != target.claim_id
            || now < claim_terms.accepted_daa
            || now > claim_terms.open_until_daa
            || actual_reserved_daa > now
            || seed_carrier_bytes > carrier_maximum
        {
            return Err("prefix seed changes actual claim/reservation or opening clock/carrier".into());
        }
        let semantic = target.registered_semantics.as_ref().ok_or("prefix claim has no immutable semantics")?;
        let relation = semantic.prefix_state_relation.ok_or("historic prefix has no qualified prospective family relation")?;
        let named = crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(actual_job).map_err(|e| e.to_string())?;
        let resources = PalwPrefixResourcePlanV1::derive(
            &binding.shape_profile,
            relation,
            named.prefix_tokens,
            binding.step_leaf_count,
            carrier_maximum,
            prices,
        )?;
        let plan = resources.envelope;
        if plan.maximum_moves > claim_terms.plan.maximum_moves
            || plan.maximum_stream_bytes > claim_terms.plan.maximum_stream_bytes
            || plan.inclusion_daa > claim_terms.plan.inclusion_daa
            || plan.maximum_parser_steps > claim_terms.plan.maximum_parser_steps
            || plan.maximum_blake_blocks > claim_terms.plan.maximum_blake_blocks
            || plan.row_bytes > claim_terms.plan.row_bytes
            || plan.resident_ram_bytes > claim_terms.plan.resident_ram_bytes
        {
            return Err("prefix seed exceeds immutable prospective claim work envelope".into());
        }
        claim_terms.check_holds(material_until, producer_exit_after, panel_exit_after)?;
        let funding = plan.rent_sompi.checked_add(plan.deposit_sompi).ok_or("prefix owner funding overflows")?;
        if available_capital < funding {
            return Err("prefix owner has not funded its complete admitted court".into());
        }
        let court = PalwPrefixFamilyCourtV1::new(
            seed_id,
            target,
            binding,
            actual_job,
            maximum_leaves,
            resources.maximum_head_working_set_bytes,
        )?;
        let mut terms = claim_terms;
        terms.plan = plan;
        let row = Self {
            claim: actual_claim,
            reserver: actual_reserver,
            reserved_daa: actual_reserved_daa,
            terms,
            rent_paid: plan.rent_sompi,
            deposit_locked: plan.deposit_sompi,
            moves_used: 1,
            carrier_bytes_used: seed_carrier_bytes,
            seed_id,
            execution_root: target.execution_root,
            maximum_carrier_bytes: resources.maximum_carrier_bytes,
            carrier_maximum,
            court,
        };
        row.check()?;
        Ok(row)
    }
    pub fn key(&self) -> (Hash64, PalwBondKeyV2, u64) {
        (self.claim, self.reserver, self.reserved_daa)
    }
    pub fn seed_id(&self) -> Hash64 {
        self.seed_id
    }
    pub fn check(&self) -> Result<(), String> {
        if self.moves_used == 0
            || self.moves_used > self.terms.plan.maximum_moves
            || self.carrier_bytes_used > self.maximum_carrier_bytes
            || self.rent_paid != self.terms.plan.rent_sompi
            || self.deposit_locked != self.terms.plan.deposit_sompi
            || self.execution_root != self.court.execution_root()
            || self.terms.accepted_daa > self.terms.open_until_daa
            || self.terms.open_until_daa > self.terms.stream_until_daa
            || self.terms.stream_until_daa > self.terms.collect_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.material_until_daa > self.terms.bond_exit_after_daa
        {
            return Err("owned prefix court exceeds rooted work/capital/clock contract".into());
        }
        self.court.check()
    }
    /// Adapter supplies the complete serialized signed carrier length and performs signature
    /// validation BEFORE this call. Identity and cumulative work are checked again here.
    pub fn movement(
        &mut self,
        now: u64,
        claim: Hash64,
        reserver: PalwBondKeyV2,
        reserved_daa: u64,
        seed_id: Hash64,
        action: &PalwPrefixCourtActionV1,
        carrier_bytes: u64,
    ) -> Result<(), String> {
        self.check()?;
        if (claim, reserver, reserved_daa) != self.key()
            || seed_id != self.seed_id
            || now >= self.terms.stream_until_daa
            || now < self.terms.accepted_daa
            || carrier_bytes > self.carrier_maximum
            || borsh::to_vec(action).map_err(|e| e.to_string())?.len() as u64 > carrier_bytes
        {
            return Err("prefix move changes literal owner/seed, clock or admitted bounded carrier".into());
        }
        let moves = self.moves_used.checked_add(1).ok_or("prefix move count overflow")?;
        let bytes = self.carrier_bytes_used.checked_add(carrier_bytes).ok_or("prefix carrier count overflow")?;
        if moves > self.terms.plan.maximum_moves || bytes > self.maximum_carrier_bytes {
            return Err("prefix cumulative moves/carriers exceed admitted work".into());
        }
        self.court.movement(seed_id, action)?;
        self.moves_used = moves;
        self.carrier_bytes_used = bytes;
        self.check()
    }
    pub fn certificate(&self) -> Result<Option<PalwPrefixCourtFaultV1>, String> {
        self.check()?;
        Ok(self.court.finish()?.then(|| PalwPrefixCourtFaultV1 {
            claim: self.claim,
            reserver: self.reserver,
            reserved_daa: self.reserved_daa,
            seed_id: self.seed_id,
            execution_root: self.execution_root,
        }))
    }
    pub(crate) fn checkpoint_v1(&self) -> PalwOwnedPrefixCheckpointV1 {
        PalwOwnedPrefixCheckpointV1 {
            court: self.court.clone(),
            moves_used: self.moves_used,
            carrier_bytes_used: self.carrier_bytes_used,
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwOwnedPrefixCheckpointV1) -> Result<(), String> {
        // Checkpoints are generated and paired by the rooted journal, never accepted as wire credentials.
        if !self.court.same_contract_v1(&cp.court)
            || cp.moves_used == 0
            || cp.moves_used > self.terms.plan.maximum_moves
            || cp.carrier_bytes_used > self.maximum_carrier_bytes
        {
            return Err("prefix checkpoint changes immutable seed or budget".into());
        }
        cp.court.check()?;
        self.court = cp.court.clone();
        self.moves_used = cp.moves_used;
        self.carrier_bytes_used = cp.carrier_bytes_used;
        self.check()
    }
}
