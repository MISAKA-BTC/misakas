//! Paid bounded tokenizer court owned by one literal ordinary reserver. Immutable identity
//! and resource terms survive separate small continuation journal moves. This is not a wire
//! credential; admission must root the returned row and signature-check each owner's move.
use crate::palw_constraint_stream_carriage_v1::{
    PalwAuxiliaryPublicCourtSeedV1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1, PalwConstraintStreamSeedV1,
};
use crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1;
use crate::palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwRenderingResourcePricesV1, PalwTokenizerResourcePlanV1};
use crate::palw_tokenizer_table_stream_v1::PalwTokenizerTableStreamV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedTokenizerCourtV1 {
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
    court: PalwTokenizerTableStreamV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedTokenizerCheckpointV1 {
    court: PalwTokenizerTableStreamV1,
    moves_used: u64,
    carrier_bytes_used: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerCourtFaultV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    pub execution_root: Hash64,
}
impl PalwOwnedTokenizerCourtV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn admit(
        seed: &PalwConstraintStreamSeedV1,
        actual_job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        target: &crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
        claim_terms: PalwRenderingClaimTermsV1,
        prices: PalwRenderingResourcePricesV1,
        now: u64,
        available_capital: u64,
        carrier_maximum: u64,
        material_until: u64,
        producer_exit_after: u64,
        panel_exit_after: &[u64],
    ) -> Result<Self, String> {
        seed.check(carrier_maximum)?;
        if seed.job.as_deref().is_some_and(|j| j != actual_job) {
            return Err("tokenizer seed changes literal retained public job".into());
        }
        if seed.claim != target.claim_id
            || now < claim_terms.accepted_daa
            || now > claim_terms.open_until_daa
            || seed.reserved_daa > now
        {
            return Err("tokenizer court changes actual claim/opening identity".into());
        }
        let Some(PalwAuxiliaryPublicCourtSeedV1::Tokenizer {
            id,
            present,
            length,
            opening,
            raw_tokenizer_bytes,
            tokenizer_key_events,
            operand_tokenizer_bytes,
        }) = &seed.auxiliary
        else {
            return Err("owned tokenizer admission requires typed tokenizer seed".into());
        };
        let registered = target.registered_semantics.as_ref().ok_or("tokenizer claim has no immutable semantics")?;
        let (Some(raw), Some(bound)) = (registered.raw_tokenizer_bytes, registered.tokenizer_key_events) else {
            return Err("tokenizer claim has no normative exact preimage/work obligation".into());
        };
        if (*raw_tokenizer_bytes, *tokenizer_key_events) != (raw, bound) {
            return Err("tokenizer seed changes immutable raw length/work declaration".into());
        }
        let tokenizer_plan = PalwTokenizerResourcePlanV1::derive_with_key_bound(*operand_tokenizer_bytes, bound, prices)?;
        let plan = tokenizer_plan.court_envelope(seed.binding.shape_profile.vocab_size, prices)?;
        if plan.maximum_moves > claim_terms.plan.maximum_moves
            || plan.inclusion_daa > claim_terms.plan.inclusion_daa
            || plan.maximum_stream_bytes > claim_terms.plan.maximum_stream_bytes
        {
            return Err("tokenizer seed exceeds its immutable claim resource envelope".into());
        }
        let mut terms = claim_terms;
        terms.plan = plan;
        terms.check_holds(material_until, producer_exit_after, panel_exit_after)?;
        let funding = plan.rent_sompi.checked_add(plan.deposit_sompi).ok_or("tokenizer funding overflows")?;
        if available_capital < funding {
            return Err("tokenizer owner has not funded complete admitted work".into());
        }
        let seed_id = Hash64::from_bytes(seed.signing_message().try_into().map_err(|_| "staged seed digest has wrong width")?);
        let court = PalwTokenizerTableStreamV1::new_with_operand_length(
            seed_id,
            target,
            &seed.binding,
            actual_job,
            *id,
            *present,
            *length,
            opening,
            *operand_tokenizer_bytes,
        )?;
        // Includes all token raw/proof fragments, all original table chunks and signed carriers,
        // plus the maximum admitted seed. No table length assertion reduces the old global cap.
        let table = crate::palw_fp_constraint_job_v1::PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u64;
        let table_moves = table.div_ceil(65536).checked_add(2).ok_or("tokenizer table moves overflow")?;
        let maximum_carrier_bytes = tokenizer_plan
            .maximum_carrier_bytes
            .checked_add(table)
            .and_then(|v| v.checked_add(table_moves.checked_mul(5632)?))
            .and_then(|v| v.checked_add(carrier_maximum))
            .ok_or("tokenizer cumulative carrier bytes overflow")?;
        let row = Self {
            claim: seed.claim,
            reserver: seed.reserver,
            reserved_daa: seed.reserved_daa,
            terms,
            rent_paid: plan.rent_sompi,
            deposit_locked: plan.deposit_sompi,
            moves_used: 1,
            carrier_bytes_used: borsh::to_vec(seed).map_err(|e| e.to_string())?.len() as u64,
            seed_id,
            execution_root: target.execution_root,
            maximum_carrier_bytes,
            court,
        };
        row.check()?;
        Ok(row)
    }
    pub fn key(&self) -> (Hash64, PalwBondKeyV2) {
        (self.claim, self.reserver)
    }
    pub fn seed_id(&self) -> Hash64 {
        self.seed_id
    }
    pub fn court(&self) -> &PalwTokenizerTableStreamV1 {
        &self.court
    }
    pub fn check(&self) -> Result<(), String> {
        if self.rent_paid != self.terms.plan.rent_sompi
            || self.deposit_locked != self.terms.plan.deposit_sompi
            || self.moves_used == 0
            || self.moves_used > self.terms.plan.maximum_moves
            || self.carrier_bytes_used > self.maximum_carrier_bytes
            || self.terms.accepted_daa > self.terms.open_until_daa
            || self.terms.open_until_daa > self.terms.stream_until_daa
            || self.terms.stream_until_daa > self.terms.collect_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.material_until_daa > self.terms.bond_exit_after_daa
        {
            return Err("owned tokenizer court exceeds immutable resource/clock contract".into());
        }
        self.court.check()
    }
    pub fn movement(&mut self, now: u64, m: &PalwConstraintStreamMoveV1) -> Result<(), String> {
        self.check()?;
        m.check()?;
        if now >= self.terms.stream_until_daa
            || (m.claim, m.reserver, m.reserved_daa) != (self.claim, self.reserver, self.reserved_daa)
            || m.seed_id != self.seed_id
            || m.phase != PalwConstraintStreamPhaseV1::Tokenizer
        {
            return Err("tokenizer move changes literal owner/claim/seed or admitted clock".into());
        }
        let moves = self.moves_used.checked_add(1).ok_or("tokenizer move count overflows")?;
        let carrier = self
            .carrier_bytes_used
            .checked_add(borsh::to_vec(m).map_err(|e| e.to_string())?.len() as u64)
            .ok_or("tokenizer carrier accounting overflows")?;
        if moves > self.terms.plan.maximum_moves || carrier > self.maximum_carrier_bytes {
            return Err("tokenizer cumulative moves/carriers exceed admitted work".into());
        }
        match &m.action {
            PalwConstraintStreamActionV1::TokenizerPacket(packet) => self.court.packet(self.seed_id, packet)?,
            PalwConstraintStreamActionV1::TokenizerSeal => self.court.seal_tokenizer(self.seed_id)?,
            PalwConstraintStreamActionV1::TokenizerRenderingPart { offset, bytes } => {
                self.court.rendering_part(self.seed_id, *offset, bytes)?
            }
            _ => return Err("tokenizer owner cannot change to another court's action".into()),
        }
        self.moves_used = moves;
        self.carrier_bytes_used = carrier;
        self.check()
    }
    pub fn certificate(&self) -> Result<Option<PalwTokenizerCourtFaultV1>, String> {
        self.check()?;
        Ok(self.court.finish()?.then(|| PalwTokenizerCourtFaultV1 {
            claim: self.claim,
            reserver: self.reserver,
            reserved_daa: self.reserved_daa,
            seed_id: self.seed_id,
            execution_root: self.execution_root,
        }))
    }
    pub(crate) fn checkpoint_v1(&self) -> PalwOwnedTokenizerCheckpointV1 {
        PalwOwnedTokenizerCheckpointV1 {
            court: self.court.clone(),
            moves_used: self.moves_used,
            carrier_bytes_used: self.carrier_bytes_used,
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwOwnedTokenizerCheckpointV1) -> Result<(), String> {
        // The journal owns cp and pairs it with this immutable row; no caller-chosen CP enters.
        if !self.court.same_contract_v1(&cp.court)
            || cp.moves_used == 0
            || cp.moves_used > self.terms.plan.maximum_moves
            || cp.carrier_bytes_used > self.maximum_carrier_bytes
        {
            return Err("tokenizer checkpoint exceeds immutable owner budget".into());
        }
        cp.court.check()?;
        self.court = cp.court.clone();
        self.moves_used = cp.moves_used;
        self.carrier_bytes_used = cp.carrier_bytes_used;
        self.check()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_json_key_set_v1::PalwJsonKeyCompactProofV1;
    use crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1;
    use crate::palw_tokenizer_json_stream_v1::tests::Book;
    use crate::palw_tokenizer_table_stream_v1::PalwTokenizerStreamPacketV1;
    use crate::tx::{TransactionId, TransactionOutpoint};
    pub(crate) fn admitted_case(fault: bool, bound: u64) -> (PalwOwnedTokenizerCourtV1, Vec<u8>, Vec<u8>) {
        admitted_case_with_lengths(fault, bound, 0, 0)
    }
    fn admitted_case_with_lengths(
        fault: bool,
        bound: u64,
        signed_delta: i64,
        operand_delta: i64,
    ) -> (PalwOwnedTokenizerCourtV1, Vec<u8>, Vec<u8>) {
        let raw = br#"{"model":{"vocab":{"a":0,"b":1},"merges":[]}}"#;
        let (mut target, binding, job, raw, opening) =
            crate::palw_offence_attribution_v1::tests::qualified_rendering_case_for_tests_v1(raw, fault, bound);
        let signed_length = raw.len() as u64;
        let signed_length = signed_length.checked_add_signed(signed_delta).unwrap();
        let operand_length = (raw.len() as u64).checked_add_signed(operand_delta).unwrap();
        target.registered_semantics.as_mut().unwrap().raw_tokenizer_bytes = Some(signed_length);
        // The selection pin is irrelevant to a typed tokenizer court; use the actual existing
        // public fixture's pin rather than manufacturing another semantic binding.
        let (_, selection) = crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1(false);
        let bytes = opening.rendering.clone().unwrap();
        let seed = PalwConstraintStreamSeedV1 {
            version: 1,
            claim: target.claim_id,
            reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(818), index: 0 }),
            reserved_daa: 100,
            binding,
            job: None,
            pin: selection.pin,
            witness: None,
            eog_count: 0,
            eog_opening: selection.eog.opening,
            auxiliary: Some(PalwAuxiliaryPublicCourtSeedV1::Tokenizer {
                id: 0,
                present: true,
                length: bytes.len() as u32,
                opening: opening.opening,
                raw_tokenizer_bytes: signed_length,
                operand_tokenizer_bytes: operand_length,
                tokenizer_key_events: bound,
            }),
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let plan = PalwTokenizerResourcePlanV1::derive_with_key_bound(operand_length, bound, config.prices)
            .unwrap()
            .court_envelope(seed.binding.shape_profile.vocab_size, config.prices)
            .unwrap();
        let declared_plan = PalwTokenizerResourcePlanV1::derive_with_key_bound(signed_length, bound, config.prices)
            .unwrap()
            .court_envelope(seed.binding.shape_profile.vocab_size, config.prices)
            .unwrap();
        let original_terms = PalwRenderingClaimTermsV1::derive(100, declared_plan, 100, 100, 10).unwrap();
        let terms = original_terms.extend_paid_work_v1(100, plan).unwrap();
        assert_eq!((terms.accepted_daa, terms.open_until_daa), (original_terms.accepted_daa, original_terms.open_until_daa));
        let funding = plan.rent_sompi + plan.deposit_sompi;
        let admit = |seed: &PalwConstraintStreamSeedV1, capital, material, panels: &[u64]| {
            PalwOwnedTokenizerCourtV1::admit(
                seed,
                &job,
                &target,
                terms,
                config.prices,
                100,
                capital,
                100000,
                material,
                terms.bond_exit_after_daa,
                panels,
            )
        };
        assert!(admit(&seed, funding - 1, terms.material_until_daa, &[terms.bond_exit_after_daa; 5]).is_err());
        assert!(admit(&seed, funding, terms.material_until_daa - 1, &[terms.bond_exit_after_daa; 5]).is_err());
        assert!(admit(&seed, funding, terms.material_until_daa, &[]).is_err());
        let mut false_job = seed.clone();
        false_job.job = Some(Box::new(job.clone()));
        false_job.job.as_mut().unwrap().job_nonce[0] ^= 1;
        assert!(admit(&false_job, funding, terms.material_until_daa, &[terms.bond_exit_after_daa; 5]).is_err());
        let mut false_bound = seed.clone();
        let Some(PalwAuxiliaryPublicCourtSeedV1::Tokenizer { tokenizer_key_events, .. }) = &mut false_bound.auxiliary else {
            unreachable!()
        };
        *tokenizer_key_events += 1;
        assert!(admit(&false_bound, funding, terms.material_until_daa, &[terms.bond_exit_after_daa; 5]).is_err());
        let row = admit(&seed, funding, terms.material_until_daa, &[terms.bond_exit_after_daa; 5]).unwrap();
        assert!(row.certificate().is_err());
        (row, raw, bytes)
    }
    pub(crate) fn movement(row: &PalwOwnedTokenizerCourtV1, action: PalwConstraintStreamActionV1) -> PalwConstraintStreamMoveV1 {
        PalwConstraintStreamMoveV1 {
            version: 1,
            claim: row.claim,
            reserver: row.reserver,
            reserved_daa: row.reserved_daa,
            seed_id: row.seed_id(),
            phase: PalwConstraintStreamPhaseV1::Tokenizer,
            action,
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }
    pub(crate) fn complete(row: &mut PalwOwnedTokenizerCourtV1, raw: &[u8], table: &[u8]) -> Vec<PalwConstraintStreamMoveV1> {
        let mut moves = vec![];
        let mut book = Book::new();
        while row.court().tokenizer_offset() < raw.len() as u64 || row.court().pending_key().is_some() {
            let action = if let Some((namespace, key)) = row.court().pending_key() {
                let proof = PalwJsonKeyCompactProofV1::compress(&book.proof(namespace, key)).unwrap();
                book.insert(namespace, key);
                PalwConstraintStreamActionV1::TokenizerPacket(PalwTokenizerStreamPacketV1 {
                    offset: row.court().tokenizer_offset(),
                    bytes: vec![],
                    proofs: vec![proof],
                })
            } else {
                let offset = row.court().tokenizer_offset();
                PalwConstraintStreamActionV1::TokenizerPacket(PalwTokenizerStreamPacketV1 {
                    offset,
                    bytes: raw[offset as usize..offset as usize + 1].to_vec(),
                    proofs: vec![],
                })
            };
            let m = movement(row, action);
            row.movement(100, &m).unwrap();
            moves.push(m);
            assert!(row.certificate().is_err());
            let bytes = borsh::to_vec(row).unwrap();
            assert!(bytes.len() < 70000);
            *row = borsh::from_slice(&bytes).unwrap();
        }
        let m = movement(row, PalwConstraintStreamActionV1::TokenizerSeal);
        row.movement(100, &m).unwrap();
        moves.push(m);
        let m = movement(row, PalwConstraintStreamActionV1::TokenizerRenderingPart { offset: 0, bytes: table.to_vec() });
        row.movement(100, &m).unwrap();
        moves.push(m);
        moves
    }
    #[test]
    fn actual_operand_eof_convicts_false_signed_length_without_trusting_the_operand_length() {
        for (signed_delta, bound) in [(-1, 64), (1, 64), (-1, 0)] {
            let (mut row, raw, table) = admitted_case_with_lengths(false, bound, signed_delta, 0);
            complete(&mut row, &raw, &table);
            assert!(row.certificate().unwrap().is_some());
        }
        // A caller-chosen short operand cannot create a false offence: even a different
        // signed declaration still requires the ORIGINAL length-prefixed raw hash at EOF.
        let (mut row, raw, table) = admitted_case_with_lengths(false, 64, 0, -1);
        complete(&mut row, &raw[..raw.len() - 1], &table);
        assert!(row.certificate().is_err());
    }
    #[test]
    fn owned_public_tokenizer_contract_refuses_false_identity_and_completes_only_after_both_original_hashes() {
        for (fault, bound, expected) in [(false, 64, false), (true, 64, true), (false, 0, true)] {
            let (mut row, raw, table) = admitted_case(fault, bound);
            let before = row.clone();
            let (foreign, _, _) = admitted_case(!fault, bound);
            assert!(row.restore_checkpoint_v1(&foreign.checkpoint_v1()).is_err());
            assert_eq!(row, before);
            let mut m = movement(
                &row,
                PalwConstraintStreamActionV1::TokenizerPacket(PalwTokenizerStreamPacketV1 {
                    offset: 0,
                    bytes: vec![raw[0]],
                    proofs: vec![],
                }),
            );
            m.seed_id = Hash64::default();
            assert!(row.movement(100, &m).is_err());
            assert_eq!(row, before);
            m.seed_id = row.seed_id();
            assert!(row.movement(row.terms.stream_until_daa, &m).is_err());
            assert_eq!(row, before);
            let old = row.checkpoint_v1();
            row.movement(100, &m).unwrap();
            let new = row.checkpoint_v1();
            row.restore_checkpoint_v1(&old).unwrap();
            assert_eq!(row, before);
            row.restore_checkpoint_v1(&new).unwrap();
            assert_eq!(row.court().tokenizer_offset(), 1);
            row.restore_checkpoint_v1(&old).unwrap();
            complete(&mut row, &raw, &table);
            assert_eq!(row.certificate().unwrap().is_some(), expected);
            assert!(borsh::to_vec(&row.checkpoint_v1()).unwrap().len() < 70000);
        }
    }
}
