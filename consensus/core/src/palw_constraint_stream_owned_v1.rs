//! Checked owned-work accounting for the prospective staged court. The chain fold must supply
//! the claim-stamped contract, actual registered owner capital, and authenticated owner moves.
//! Reorg restores this record/counters together; a local checkpoint is never a credential.
use crate::Hash64;
use crate::palw_constraint_stream_carriage_v1::{
    PalwConstraintStreamActionV1, PalwConstraintStreamLeafV1, PalwConstraintStreamMoveV1, PalwConstraintStreamSeedV1,
};
use crate::palw_constraint_stream_court_v1::{
    PalwConstraintStreamCheckpointV1, PalwConstraintStreamCourtV1, PalwConstraintStreamFaultV1,
};
use crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1;
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
pub type PalwConstraintStreamKeyV1 = (Hash64, PalwBondKeyV2);
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedConstraintStreamV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub terms: PalwRenderingClaimTermsV1,
    pub rent_paid: u64,
    pub deposit_locked: u64,
    pub moves_used: u64,
    pub rendering_bytes_used: u64,
    pub eog_bytes_used: u64,
    court: PalwConstraintStreamCourtV1,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedConstraintCheckpointV1 {
    court: PalwConstraintStreamCheckpointV1,
    moves_used: u64,
    rendering_bytes_used: u64,
    eog_bytes_used: u64,
}

impl PalwOwnedConstraintStreamV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_with_authenticated_flat_pin_v1(
        seed: &PalwConstraintStreamSeedV1,
        original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        root: Hash64,
        terms: PalwRenderingClaimTermsV1,
        now: u64,
        funding: u64,
        maximum: u64,
        material: u64,
        producer_exit: u64,
        panel_exits: &[u64],
        binding: &crate::palw_step_leg::PalwStepBindingV2,
        pin: &crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
    ) -> Result<Self, String> {
        let job = &original.job;
        seed.check(maximum)?;
        if seed.resolved_binding_v1(job.class_id, Some(&binding.shape_profile))? != *binding
            || seed.job.as_deref().is_some_and(|original| original != job)
            || now < terms.accepted_daa
            || now >= terms.open_until_daa
            || seed.reserved_daa > now
            || terms.plan.prefix_tokens < pin.position_v1()
            || terms.plan.vocab != binding.shape_profile.vocab_size
            || !matches!(
                seed.auxiliary,
                Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FlatConstraint { .. })
            )
        {
            return Err("flat semantic admission changes original signed job, profile or frozen allocation".into());
        }
        terms.check_holds(material, producer_exit, panel_exits)?;
        if funding < terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).ok_or("flat semantic funding overflow")? {
            return Err("flat semantic court has no full prepaid work budget".into());
        }
        let seed_id = Hash64::from_bytes(seed.signing_message().try_into().map_err(|_| "flat seed digest width")?);
        let court = PalwConstraintStreamCourtV1::new_from_flat_pin_with_original_stop_context_v1(
            seed.claim,
            seed.reserver,
            seed.reserved_daa,
            root,
            binding,
            job,
            pin,
            seed.witness,
            seed.eog_count,
            &seed.eog_opening,
            original,
            Some(seed_id),
        )?;
        let row = Self {
            claim: seed.claim,
            reserver: seed.reserver,
            reserved_daa: seed.reserved_daa,
            terms,
            rent_paid: terms.plan.rent_sompi,
            deposit_locked: terms.plan.deposit_sompi,
            moves_used: 1,
            rendering_bytes_used: 0,
            eog_bytes_used: 0,
            court,
        };
        row.check()?;
        Ok(row)
    }
    /// The greedy Float32 twin of [`Self::admit_with_authenticated_flat_pin_v1`]: the pin comes
    /// from the accepted Float32 source slot, never from the carrier.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit_with_authenticated_float_pin_v1(
        seed: &PalwConstraintStreamSeedV1,
        original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        root: Hash64,
        terms: PalwRenderingClaimTermsV1,
        now: u64,
        funding: u64,
        maximum: u64,
        material: u64,
        producer_exit: u64,
        panel_exits: &[u64],
        binding: &crate::palw_step_leg::PalwStepBindingV2,
        pin: &crate::palw_float_constraint_source_v1::PalwFloatConstraintPinV1,
    ) -> Result<Self, String> {
        let job = &original.job;
        seed.check(maximum)?;
        if seed.resolved_binding_v1(job.class_id, Some(&binding.shape_profile))? != *binding
            || seed.job.as_deref().is_some_and(|original| original != job)
            || now < terms.accepted_daa
            || now >= terms.open_until_daa
            || seed.reserved_daa > now
            || terms.plan.prefix_tokens < pin.position_v1()
            || terms.plan.vocab != binding.shape_profile.vocab_size
            || !matches!(
                seed.auxiliary,
                Some(crate::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FloatConstraint { .. })
            )
        {
            return Err("Float32 semantic admission changes original signed job, profile or frozen allocation".into());
        }
        terms.check_holds(material, producer_exit, panel_exits)?;
        if funding < terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).ok_or("Float32 semantic funding overflow")? {
            return Err("Float32 semantic court has no full prepaid work budget".into());
        }
        let seed_id = Hash64::from_bytes(seed.signing_message().try_into().map_err(|_| "Float32 seed digest width")?);
        let court = PalwConstraintStreamCourtV1::new_from_float_pin_with_original_stop_context_v1(
            seed.claim,
            seed.reserver,
            seed.reserved_daa,
            root,
            binding,
            job,
            pin,
            seed.witness,
            seed.eog_count,
            &seed.eog_opening,
            original,
            Some(seed_id),
        )?;
        let row = Self {
            claim: seed.claim,
            reserver: seed.reserver,
            reserved_daa: seed.reserved_daa,
            terms,
            rent_paid: terms.plan.rent_sompi,
            deposit_locked: terms.plan.deposit_sompi,
            moves_used: 1,
            rendering_bytes_used: 0,
            eog_bytes_used: 0,
            court,
        };
        row.check()?;
        Ok(row)
    }
    pub(crate) fn checkpoint_v1(&self) -> PalwOwnedConstraintCheckpointV1 {
        PalwOwnedConstraintCheckpointV1 {
            court: self.court.checkpoint_v1(),
            moves_used: self.moves_used,
            rendering_bytes_used: self.rendering_bytes_used,
            eog_bytes_used: self.eog_bytes_used,
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwOwnedConstraintCheckpointV1) -> Result<(), String> {
        if cp.moves_used == 0
            || cp.moves_used > self.terms.plan.maximum_moves
            || cp.rendering_bytes_used > self.terms.plan.maximum_stream_bytes
            || cp.eog_bytes_used > self.terms.plan.maximum_eog_bytes
        {
            return Err("owned journal checkpoint exceeds admitted immutable resource plan".into());
        }
        self.court.restore_checkpoint_v1(&cp.court)?;
        self.moves_used = cp.moves_used;
        self.rendering_bytes_used = cp.rendering_bytes_used;
        self.eog_bytes_used = cp.eog_bytes_used;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        seed: &PalwConstraintStreamSeedV1,
        expected_claim_root: Hash64,
        terms: PalwRenderingClaimTermsV1,
        now_daa: u64,
        available_owner_capital: u64,
        carrier_maximum: u64,
        ladder: u64,
        material_until: u64,
        producer_exit_after: u64,
        panel_exit_after: &[u64],
    ) -> Result<Self, String> {
        let job = seed.job.as_deref().ok_or("unrooted admission cannot resolve public job reference")?;
        Self::admit_with_rooted_job(
            seed,
            job,
            expected_claim_root,
            terms,
            now_daa,
            available_owner_capital,
            carrier_maximum,
            ladder,
            material_until,
            producer_exit_after,
            panel_exit_after,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn admit_with_rooted_job(
        seed: &PalwConstraintStreamSeedV1,
        actual_job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        expected_claim_root: Hash64,
        terms: PalwRenderingClaimTermsV1,
        now_daa: u64,
        available_owner_capital: u64,
        carrier_maximum: u64,
        ladder: u64,
        material_until: u64,
        producer_exit_after: u64,
        panel_exit_after: &[u64],
    ) -> Result<Self, String> {
        if seed.retained_profile.is_some() {
            return Err("rooted admission must resolve retained profile first".into());
        }
        Self::admit_with_resolved_binding_v1(
            seed,
            actual_job,
            expected_claim_root,
            terms,
            now_daa,
            available_owner_capital,
            carrier_maximum,
            ladder,
            material_until,
            producer_exit_after,
            panel_exit_after,
            &seed.binding,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn admit_with_resolved_binding_v1(
        seed: &PalwConstraintStreamSeedV1,
        actual_job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        expected_claim_root: Hash64,
        terms: PalwRenderingClaimTermsV1,
        now_daa: u64,
        available_owner_capital: u64,
        carrier_maximum: u64,
        ladder: u64,
        material_until: u64,
        producer_exit_after: u64,
        panel_exit_after: &[u64],
        binding: &crate::palw_step_leg::PalwStepBindingV2,
    ) -> Result<Self, String> {
        seed.check(carrier_maximum)?;
        if now_daa < terms.accepted_daa
            || now_daa > terms.open_until_daa
            || seed.reserved_daa > now_daa
            || terms.plan.prefix_tokens < seed.pin.position
            || terms.plan.vocab != seed.binding.shape_profile.vocab_size
        {
            return Err("prospective owned court is outside claim-stamped profile/opening window".into());
        }
        terms.check_holds(material_until, producer_exit_after, panel_exit_after)?;
        let required =
            terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).ok_or("stream rent plus deposit overflows exact capital")?;
        if available_owner_capital < required {
            return Err("owner lacks unencumbered capital for complete admitted staged work".into());
        }
        let court = seed.court_with_resolved_binding_v1(expected_claim_root, ladder, actual_job, binding)?;
        let row = Self {
            claim: seed.claim,
            reserver: seed.reserver,
            reserved_daa: seed.reserved_daa,
            terms,
            rent_paid: terms.plan.rent_sompi,
            deposit_locked: terms.plan.deposit_sompi,
            moves_used: 1,
            rendering_bytes_used: 0,
            eog_bytes_used: 0,
            court,
        };
        row.check()?;
        Ok(row)
    }
    pub fn key(&self) -> PalwConstraintStreamKeyV1 {
        (self.claim, self.reserver)
    }
    pub fn seed_id(&self) -> Hash64 {
        self.court.seed_id()
    }
    pub fn court(&self) -> &PalwConstraintStreamCourtV1 {
        &self.court
    }
    pub fn check(&self) -> Result<(), String> {
        if self.rent_paid != self.terms.plan.rent_sompi
            || self.deposit_locked != self.terms.plan.deposit_sompi
            || self.moves_used == 0
            || self.moves_used > self.terms.plan.maximum_moves
            || self.rendering_bytes_used > self.terms.plan.maximum_stream_bytes
            || self.eog_bytes_used > self.terms.plan.maximum_eog_bytes
            || self.terms.accepted_daa > self.terms.open_until_daa
            || self.terms.open_until_daa > self.terms.stream_until_daa
            || self.terms.stream_until_daa > self.terms.collect_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.bond_exit_after_daa < self.terms.material_until_daa
        {
            return Err("owned staged resource/retention counters are inconsistent".into());
        }
        Ok(())
    }
    fn next_move(&self, now_daa: u64) -> Result<u64, String> {
        self.check()?;
        if now_daa >= self.terms.stream_until_daa {
            return Err("prospective stream deadline is closed".into());
        }
        let moves = self.moves_used.checked_add(1).ok_or("stream move count overflows")?;
        if moves > self.terms.plan.maximum_moves {
            return Err("stream exceeds claim-admitted cumulative work".into());
        }
        Ok(moves)
    }
    pub fn leaf(&mut self, now_daa: u64, leaf: &PalwConstraintStreamLeafV1) -> Result<(), String> {
        let moves = self.next_move(now_daa)?;
        leaf.check()?;
        if leaf.claim != self.claim || leaf.reserver != self.reserver || leaf.reserved_daa != self.reserved_daa {
            return Err("leaf changes actual owner/claim/reservation identity".into());
        }
        self.court.begin_rendering(leaf.seed_id, leaf.id, leaf.present, leaf.length, &leaf.opening)?;
        self.moves_used = moves;
        Ok(())
    }
    pub fn movement(&mut self, now_daa: u64, movement: &PalwConstraintStreamMoveV1) -> Result<(), String> {
        let moves = self.next_move(now_daa)?;
        if movement.claim != self.claim || movement.reserver != self.reserver || movement.reserved_daa != self.reserved_daa {
            return Err("move changes actual owner/claim/reservation identity".into());
        }
        let mut rendering = self.rendering_bytes_used;
        let mut eog = self.eog_bytes_used;
        match &movement.action {
            PalwConstraintStreamActionV1::RenderingPart { bytes, .. } => {
                rendering = rendering.checked_add(bytes.len() as u64).ok_or("stream cumulative bytes overflow")?
            }
            PalwConstraintStreamActionV1::EogPart { ids, .. } => {
                eog = eog.checked_add(ids.len() as u64 * 4).ok_or("stream EOG cumulative bytes overflow")?
            }
            _ => (),
        }
        if rendering > self.terms.plan.maximum_stream_bytes || eog > self.terms.plan.maximum_eog_bytes {
            return Err("stream exceeds claim-admitted cumulative bytes".into());
        }
        movement.apply_to(&mut self.court)?;
        self.moves_used = moves;
        self.rendering_bytes_used = rendering;
        self.eog_bytes_used = eog;
        Ok(())
    }
    /// The fold stores this small immutable result after successful finish, before releasing the
    /// private continuation/deposit. Direct caller-supplied certificates cannot replace that row.
    pub fn certificate(&self) -> Result<Option<PalwConstraintStreamFaultV1>, String> {
        self.check()?;
        self.court.fault()
    }
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1;
    use crate::palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwRenderingResourcePricesV1};
    use crate::tx::{TransactionId, TransactionOutpoint};
    fn seed() -> PalwConstraintStreamSeedV1 {
        seed_for(true).0
    }
    pub(crate) fn seed_for(
        fault: bool,
    ) -> (PalwConstraintStreamSeedV1, crate::palw_fp_constraint_job_v1::PalwConstrainedDecodeAccusationV1) {
        let (binding, a) = public_constrained_case_for_tests_v1(fault);
        let copy = a.clone();
        (
            PalwConstraintStreamSeedV1 {
                version: 1,
                claim: Hash64::from_u64_word(1),
                reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(2), index: 0 }),
                reserved_daa: 100,
                binding,
                job: Some(Box::new(a.job)),
                pin: a.pin,
                witness: a.witness,
                eog_count: a.eog.ids.len() as u32,
                eog_opening: a.eog.opening,
                auxiliary: None,
                retained_profile: None,
                signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
            copy,
        )
    }
    pub(crate) fn terms(seed: &PalwConstraintStreamSeedV1) -> PalwRenderingClaimTermsV1 {
        let p = PalwRenderingResourcePlanV1::derive(
            seed.pin.position,
            seed.binding.shape_profile.vocab_size,
            PalwRenderingResourcePricesV1 {
                inclusion_margin_daa: 4,
                rent_per_stream_byte: 1,
                rent_per_parser_step: 1,
                rent_per_row_byte: 1,
                deposit_per_move: 1,
                reorg_replay_moves: 0,
            },
        )
        .unwrap();
        PalwRenderingClaimTermsV1::derive(100, p, 100, 100, 10).unwrap()
    }
    /// Uses admitted, ordered public moves; no test checkpoint manufactures a certificate.
    pub(crate) fn completed_row(fault: bool) -> (PalwOwnedConstraintStreamV1, Hash64) {
        completed_profile_row(fault, false)
    }
    fn completed_profile_row(fault: bool, reference: bool) -> (PalwOwnedConstraintStreamV1, Hash64) {
        let (mut seed, a) = seed_for(fault);
        let binding = seed.binding.clone();
        if reference {
            seed.retained_profile = Some(crate::palw_constraint_stream_carriage_v1::PalwRetainedCourtProfileV1 {
                class_id: binding.shape_profile.shape_profile_id(),
                original_bytes: borsh::to_vec(&binding.shape_profile).unwrap().len() as u64,
            });
            seed.binding.shape_profile =
                crate::palw_constraint_stream_carriage_v1::palw_retained_profile_skeleton_v1(&binding.shape_profile);
        }
        let terms = terms(&seed);
        let root = seed.binding.committed_execution_root;
        let mut row = PalwOwnedConstraintStreamV1::admit_with_resolved_binding_v1(
            &seed,
            seed.job.as_deref().unwrap(),
            root,
            terms,
            100,
            u64::MAX,
            2_880_000,
            1 << 25,
            terms.material_until_daa,
            terms.bond_exit_after_daa,
            &[terms.bond_exit_after_daa; 5],
            &binding,
        )
        .unwrap();
        if reference {
            assert_eq!(row.seed_id(), Hash64::from_bytes(seed.signing_message().try_into().unwrap()));
        }
        fn movement(row: &mut PalwOwnedConstraintStreamV1, action: PalwConstraintStreamActionV1) {
            let m = PalwConstraintStreamMoveV1 {
                version: 1,
                claim: row.claim,
                reserver: row.reserver,
                reserved_daa: row.reserved_daa,
                seed_id: row.seed_id(),
                phase: row.court.phase(),
                action,
                signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            };
            row.movement(100, &m).unwrap();
        }
        movement(&mut row, PalwConstraintStreamActionV1::EogPart { offset: 0, ids: a.eog.ids.clone() });
        movement(&mut row, PalwConstraintStreamActionV1::EogFinish);
        while row.court.phase() != crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::Complete {
            let id = row.court.next_rendering_id().unwrap();
            if row.court.is_eog(id) {
                movement(&mut row, PalwConstraintStreamActionV1::EogTokenFinish);
                continue;
            }
            let o = a.renderings.iter().find(|o| o.id == id).unwrap();
            row.leaf(
                100,
                &PalwConstraintStreamLeafV1 {
                    version: 1,
                    claim: row.claim,
                    reserver: row.reserver,
                    reserved_daa: row.reserved_daa,
                    seed_id: row.seed_id(),
                    id,
                    present: o.rendering.is_some(),
                    length: o.rendering.as_ref().map_or(0, |b| b.len()) as u32,
                    opening: o.opening.clone(),
                    signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
                },
            )
            .unwrap();
            if let Some(bytes) = &o.rendering {
                for (n, part) in bytes.chunks(65536).enumerate() {
                    movement(
                        &mut row,
                        PalwConstraintStreamActionV1::RenderingPart { offset: (n * 65536) as u32, bytes: part.to_vec() },
                    );
                }
            }
            movement(&mut row, PalwConstraintStreamActionV1::RenderingFinish);
        }
        (row, root)
    }
    #[test]
    fn compact_registered_profile_reaches_same_constraint_verdict_with_original_signed_identity() {
        for fault in [false, true] {
            let (full, root) = completed_row(fault);
            let (compact, compact_root) = completed_profile_row(fault, true);
            assert_eq!(compact_root, root);
            assert_eq!(compact.certificate().unwrap().is_some(), fault);
            assert_eq!(compact.certificate().unwrap().map(|c| c.position), full.certificate().unwrap().map(|c| c.position));
            assert_ne!(compact.seed_id(), full.seed_id());
        }
    }
    #[test]
    fn admission_requires_entire_capital_and_retention_contract_before_owned_work() {
        let seed = seed();
        let terms = terms(&seed);
        let root = seed.binding.committed_execution_root;
        let required = terms.plan.rent_sompi + terms.plan.deposit_sompi;
        let make = |capital, material| {
            PalwOwnedConstraintStreamV1::admit(
                &seed,
                root,
                terms,
                100,
                capital,
                2_880_000,
                1 << 25,
                material,
                terms.bond_exit_after_daa,
                &[terms.bond_exit_after_daa; 5],
            )
        };
        assert!(make(required - 1, terms.material_until_daa).is_err());
        assert!(make(required, terms.material_until_daa - 1).is_err());
        let row = make(required, terms.material_until_daa).unwrap();
        assert!(row.certificate().is_err());
        assert_eq!(borsh::from_slice::<PalwOwnedConstraintStreamV1>(&borsh::to_vec(&row).unwrap()).unwrap(), row);
        assert!(row.next_move(terms.stream_until_daa).is_err());
    }
    #[test]
    fn small_mutable_checkpoint_reverts_and_restores_without_grammar_or_prior_bytes() {
        let seed = seed();
        let terms = terms(&seed);
        let root = seed.binding.committed_execution_root;
        let mut row = PalwOwnedConstraintStreamV1::admit(
            &seed,
            root,
            terms,
            100,
            u64::MAX,
            2_880_000,
            1 << 25,
            terms.material_until_daa,
            terms.bond_exit_after_daa,
            &[terms.bond_exit_after_daa; 5],
        )
        .unwrap();
        let before = row.clone();
        let old = row.checkpoint_v1();
        let m = PalwConstraintStreamMoveV1 {
            version: 1,
            claim: row.claim,
            reserver: row.reserver,
            reserved_daa: row.reserved_daa,
            seed_id: row.seed_id(),
            phase: row.court.phase(),
            action: PalwConstraintStreamActionV1::EogPart { offset: 0, ids: vec![0] },
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        row.movement(101, &m).unwrap();
        let after = row.clone();
        let bytes = borsh::to_vec(&row.checkpoint_v1()).unwrap();
        assert!(bytes.len() < 40_000);
        let checkpoint: PalwOwnedConstraintCheckpointV1 = borsh::from_slice(&bytes).unwrap();
        row.restore_checkpoint_v1(&old).unwrap();
        assert_eq!(row, before);
        row.restore_checkpoint_v1(&checkpoint).unwrap();
        assert_eq!(row, after);
        let mut foreign_seed = seed.clone();
        foreign_seed.reserver.0.index += 1;
        let mut foreign = PalwOwnedConstraintStreamV1::admit(
            &foreign_seed,
            root,
            terms,
            100,
            u64::MAX,
            2_880_000,
            1 << 25,
            terms.material_until_daa,
            terms.bond_exit_after_daa,
            &[terms.bond_exit_after_daa; 5],
        )
        .unwrap();
        let foreign_before = foreign.clone();
        assert!(foreign.restore_checkpoint_v1(&checkpoint).is_err());
        assert_eq!(foreign, foreign_before);
    }

    #[test]
    fn foreign_owner_and_over_budget_moves_cannot_mutate_or_poison_continuation() {
        let seed = seed();
        let terms = terms(&seed);
        let root = seed.binding.committed_execution_root;
        let mut row = PalwOwnedConstraintStreamV1::admit(
            &seed,
            root,
            terms,
            100,
            u64::MAX,
            2_880_000,
            1 << 25,
            terms.material_until_daa,
            terms.bond_exit_after_daa,
            &[terms.bond_exit_after_daa; 5],
        )
        .unwrap();
        let mut m = PalwConstraintStreamMoveV1 {
            version: 1,
            claim: row.claim,
            reserver: row.reserver,
            reserved_daa: row.reserved_daa,
            seed_id: row.seed_id(),
            phase: row.court.phase(),
            action: PalwConstraintStreamActionV1::EogPart { offset: 0, ids: vec![0] },
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let before = row.clone();
        m.reserver.0.index += 1;
        assert!(row.movement(101, &m).is_err());
        assert_eq!(row, before);
        m.reserver = row.reserver;
        row.moves_used = row.terms.plan.maximum_moves;
        let before = row.clone();
        assert!(row.movement(101, &m).is_err());
        assert_eq!(row, before);
    }
}
