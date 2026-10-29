//! RFC0040 producer-only source-export conviction. Old verifier signatures never
//! acquire responsibility for an export statement that they did not approve.
use super::*;
use crate::palw_claim_local_v1::prefix_v1::{PalwPrefixExportIdentityV1, PalwPrefixExportRefutationV1};

pub(super) fn offence_id(accused: PalwBondKeyV2, source: &PalwPrefixExportIdentityV1) -> Hash64 {
    crate::palw_claim_local_v1::prefix_v1::prefix_export_offence_id_v1(&accused.0, source)
}

impl TransitionBuilder<'_> {
    pub(super) fn consume_prefix_export_refuted_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        evidence: &[u8],
        evidence_budget: crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_v1::PalwOffenceKindV1;
        let refusal = |message: &'static str| PalwStateV2Error::ObjectiveOffenceRefused(evidence_id, message.into());
        if !self.params.claim_local_settlement_active_at(ctx.daa_score) || !self.params.rcore_plus_active_at(ctx.daa_score) {
            return Err(PalwStateV2Error::ObjectiveOffenceDormant);
        }
        let maximum = match evidence_budget {
            crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::DirectCarrier =>
                crate::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES,
            crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::ReservedTerminal { court_maximum } =>
                court_maximum.min(crate::palw_terminal_carriage_v1::PALW_TERMINAL_MAX_BYTES_V1),
        };
        if evidence.len() as u64 > maximum
            || crate::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != evidence_id {
            return Err(refusal("source export evidence exceeds its paid envelope or changes its digest"));
        }
        let payload = crate::palw_claim_local_v1::decode_claim_local_value_v1::<PalwPrefixExportRefutationV1>(evidence)
            .map_err(|e| match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => refusal("malformed source export evidence"),
            })?;
        let mut reservation = self.state.check_prefix_export_refutation_v1(self.params, ctx, &accused, &payload)?;
        let key = offence_id(accused, &payload.source);
        if self.state.consumed_offences.contains_key(&key) { return Ok(()); }
        let claim_id = payload.source.source_claim;
        let conv = self.open_conviction_v1(&[accused], ctx.daa_score);
        let gains = self.claim_g_v1(&claim_id).map(|g| g.g()).unwrap_or(0);
        let live = self.claim_is_live_before_final_v1(&claim_id);
        let fp_first = self.post_final_fp_g_v1(&claim_id);
        // Distinct reason: kind3 CourtFraud cannot read this as old-seat blame.
        let mut nominal = self.act_on_convicted_claim_as_v1(ctx, claim_id, Some(&conv), PalwVoidReasonV2::PrefixExportFraud)?;
        if !live {
            nominal = nominal.saturating_add(self.post_final_producer_leg_v1(
                &conv, claim_id, accused, key, PalwOffenceKindV1::PrefixExportRefuted, gains, fp_first,
            )?);
        }
        self.close_conviction_v1(
            &conv, key, PalwOffenceKindV1::PrefixExportRefuted, accused, nominal, Hash64::default(), claim_id,
            &[accused], &[(accused, crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::CourtFraud)],
        )?;
        self.forfeit_minted_round_rights_of_claim(&claim_id)?;
        self.open_reporter_reward(ctx.daa_score, key, 0, PalwConvictionBasisV1::CheckedEvidence { evidence_id })?;
        reservation.convicted_daa = Some(ctx.daa_score);
        self.write_kernel_row_encoded_v1(
            crate::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1,
            &super::palw_claim_local_prefix_reservation_v1::row_key(reservation.budget_id), Some(&reservation),
        )?;
        Ok(())
    }
}
