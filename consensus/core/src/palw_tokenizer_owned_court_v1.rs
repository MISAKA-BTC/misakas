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
        if seed.job.as_deref().is_some_and(|j|j!=actual_job) {
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
        let tokenizer_plan = PalwTokenizerResourcePlanV1::derive_with_key_bound(raw, bound, prices)?;
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
        let court = PalwTokenizerTableStreamV1::new(seed_id, target, &seed.binding, actual_job, *id, *present, *length, opening)?;
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
        if cp.moves_used == 0 || cp.moves_used > self.terms.plan.maximum_moves || cp.carrier_bytes_used > self.maximum_carrier_bytes {
            return Err("tokenizer checkpoint exceeds immutable owner budget".into());
        }
        cp.court.check()?;
        self.court = cp.court.clone();
        self.moves_used = cp.moves_used;
        self.carrier_bytes_used = cp.carrier_bytes_used;
        self.check()
    }
}
