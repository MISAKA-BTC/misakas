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
    /// Individually charged logical actions, including the seed.
    pub moves_used: u64,
    pub physical_carriers_used: u64,
    pub carrier_bytes_used: u64,
    seed_id: Hash64,
    execution_root: Hash64,
    maximum_carrier_bytes: u64,
    maximum_carriers: u64,
    maximum_packet_body_bytes: u64,
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
    court: crate::palw_prefix_family_court_v1::PalwPrefixFamilyCheckpointV1,
    moves_used: u64,
    physical_carriers_used: u64,
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
            physical_carriers_used: 1,
            carrier_bytes_used: seed_carrier_bytes,
            seed_id,
            execution_root: target.execution_root,
            maximum_carrier_bytes: resources.maximum_carrier_bytes,
            maximum_carriers: resources.maximum_carriers,
            maximum_packet_body_bytes: resources.maximum_packet_body_bytes,
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
    pub fn cursor_v1(&self) -> Result<crate::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1, String> {
        self.check()?;
        self.court.cursor_v1()
    }
    pub fn next_public_source_v1(&self) -> Result<Option<crate::palw_prefix_family_court_v1::PalwPrefixPublicSourceV1>, String> {
        self.check()?;
        self.court.next_public_source_v1()
    }
    /// Returns only hash-EOF-validated original-source actions, without committing or
    /// bypassing this owner's cumulative move, carrier, capital and clock accounting.
    pub fn authenticated_public_source_actions_v1(
        &self,
        opening: &crate::palw_step_leg::PalwStepOpeningV1,
        bytes: &[u8],
    ) -> Result<Vec<PalwPrefixCourtActionV1>, String> {
        self.check()?;
        self.court.authenticated_public_source_actions_v1(self.seed_id, opening, bytes)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.moves_used == 0
            || self.physical_carriers_used == 0
            || self.physical_carriers_used > self.maximum_carriers
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
        self.movement_batch_v1(now, claim, reserver, reserved_daa, seed_id, std::slice::from_ref(action), carrier_bytes)
    }
    /// One flat signed packet, with per-action logical work and one physical carrier debit.
    /// No recursive action type exists. The same original-leaf sequence runs on a copied
    /// baseline, so a late bad hash/offset/phase cannot commit an earlier prefix of a packet.
    #[allow(clippy::too_many_arguments)]
    pub fn movement_batch_v1(
        &mut self,
        now: u64,
        claim: Hash64,
        reserver: PalwBondKeyV2,
        reserved_daa: u64,
        seed_id: Hash64,
        actions: &[PalwPrefixCourtActionV1],
        whole_signed_len: u64,
    ) -> Result<(), String> {
        self.check()?;
        if actions.is_empty()
            || actions.len() > 64
            || (claim, reserver, reserved_daa) != self.key()
            || seed_id != self.seed_id
            || now >= self.terms.stream_until_daa
            || now < self.terms.accepted_daa
            || whole_signed_len > self.carrier_maximum
        {
            return Err("prefix packet changes literal owner/seed, clock or bounded carrier".into());
        }
        let body = borsh::to_vec(&actions).map_err(|e| e.to_string())?.len() as u64;
        if body > self.maximum_packet_body_bytes || body > whole_signed_len {
            return Err("prefix packet exceeds its measured flat body or whole signed envelope".into());
        }
        let moves = self.moves_used.checked_add(actions.len() as u64).ok_or("prefix logical action count overflow")?;
        let physical = self.physical_carriers_used.checked_add(1).ok_or("prefix physical carrier count overflow")?;
        let bytes = self.carrier_bytes_used.checked_add(whole_signed_len).ok_or("prefix signed carrier byte count overflow")?;
        if moves > self.terms.plan.maximum_moves || physical > self.maximum_carriers || bytes > self.maximum_carrier_bytes {
            return Err("prefix cumulative logical work/physical carriers exceed admitted budget".into());
        }
        let mut candidate = self.court.clone();
        for action in actions {
            candidate.movement(seed_id, action)?;
        }
        self.court = candidate;
        self.moves_used = moves;
        self.physical_carriers_used = physical;
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
            court: self.court.checkpoint_v1(),
            moves_used: self.moves_used,
            physical_carriers_used: self.physical_carriers_used,
            carrier_bytes_used: self.carrier_bytes_used,
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwOwnedPrefixCheckpointV1) -> Result<(), String> {
        // Checkpoints are generated and paired by the rooted journal, never accepted as wire credentials.
        if cp.moves_used == 0
            || cp.moves_used > self.terms.plan.maximum_moves
            || cp.physical_carriers_used == 0
            || cp.physical_carriers_used > self.maximum_carriers
            || cp.carrier_bytes_used > self.maximum_carrier_bytes
        {
            return Err("prefix checkpoint changes immutable seed or budget".into());
        }
        self.court.restore_checkpoint_v1(&cp.court)?;
        self.moves_used = cp.moves_used;
        self.physical_carriers_used = cp.physical_carriers_used;
        self.carrier_bytes_used = cp.carrier_bytes_used;
        self.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_packet_charges_each_action_once_and_late_bad_eof_rolls_back_all_actions() {
        for inherited in [false, true] {
            for fault in [false, true] {
                let (target, binding, job, tiles) =
                    crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(fault, inherited);
                let prices = PalwRenderingResourcePricesV1 {
                    inclusion_margin_daa: 4,
                    rent_per_stream_byte: 1,
                    rent_per_parser_step: 1,
                    rent_per_row_byte: 1,
                    deposit_per_move: 1,
                    reorg_replay_moves: 0,
                };
                let resources = PalwPrefixResourcePlanV1::derive(
                    &binding.shape_profile,
                    target.registered_semantics.as_ref().unwrap().prefix_state_relation.unwrap(),
                    2,
                    binding.step_leaf_count,
                    90_000,
                    prices,
                )
                .unwrap();
                let terms = PalwRenderingClaimTermsV1::derive(100, resources.envelope, 100, 100, 20).unwrap();
                let owner = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(0x1917), 0));
                let seed = Hash64::from_u64_word(0x16517);
                let mut row = PalwOwnedPrefixCourtV1::admit(
                    seed,
                    target.claim_id,
                    owner,
                    100,
                    &binding,
                    &job,
                    &target,
                    terms,
                    prices,
                    100,
                    u64::MAX,
                    90_000,
                    1 << 22,
                    8000,
                    terms.material_until_daa,
                    terms.bond_exit_after_daa,
                    &[terms.bond_exit_after_daa; 5],
                )
                .unwrap();
                let mut actions = Vec::new();
                let mut candidate = row.court.clone();
                while let Some(source) = candidate.next_public_source_v1().unwrap() {
                    let tile = tiles.iter().find(|t| t.opening.leaf_index == source.leaf_index).unwrap();
                    let next =
                        candidate.authenticated_public_source_actions_v1(seed, &tile.opening, &tile.preimage.values_le).unwrap();
                    for action in &next {
                        candidate.movement(seed, action).unwrap();
                    }
                    actions.extend(next);
                }
                assert!(actions.len() > 1 && actions.len() <= 64);
                let signed_length = borsh::to_vec(&actions).unwrap().len() as u64 + 8000;
                let baseline = borsh::to_vec(&row).unwrap();
                let mut corrupt = actions.clone();
                let bytes = corrupt
                    .iter_mut()
                    .rev()
                    .find_map(|action| match action {
                        PalwPrefixCourtActionV1::TilePart { bytes, .. } => Some(bytes),
                        _ => None,
                    })
                    .unwrap();
                bytes[0] ^= 1;
                assert!(row.movement_batch_v1(101, target.claim_id, owner, 100, seed, &corrupt, signed_length).is_err());
                assert_eq!(borsh::to_vec(&row).unwrap(), baseline, "a late bad original EOF commits neither state nor counters");
                assert!(row.movement_batch_v1(101, target.claim_id, owner, 100, seed, &[], signed_length).is_err());
                assert!(row.movement_batch_v1(101, target.claim_id, owner, 100, Hash64::default(), &actions, signed_length).is_err());
                row.movement_batch_v1(101, target.claim_id, owner, 100, seed, &actions, signed_length).unwrap();
                assert_eq!(row.moves_used, 1 + actions.len() as u64);
                assert_eq!(row.physical_carriers_used, 2);
                assert_eq!(row.carrier_bytes_used, 8000 + signed_length);
                row = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
                assert_eq!(row.certificate().unwrap().is_some(), fault);
            }
        }
    }
}
