//! Physical TKR1 work is checked before UTXO, Round or native admission.
use super::*;
use kaspa_consensus_core::{
    errors::block::RuleError,
    palw_native_resource_v1::NativeCoverageV1,
    palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    palw_tokenizer_registration_carriage_v1::{
        TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1, TokenizerRegistrationCandidateWorkV1, tokenizer_registration_parent_quote_v1,
    },
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
};

impl VirtualStateProcessor {
    /// The complete physical stream includes bodies that may later lose their
    /// permit, fail UTXO validation or be refused by Round income admission.
    /// Return whether a candidate-scoped native fee/cache owner is necessary.
    pub(super) fn preflight_tokenizer_candidate_v1(
        &self,
        ctx: &UtxoProcessingContext<'_>,
        parent: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa: u64,
    ) -> Result<bool, RuleError> {
        let Some(policy) = self.palw_lifecycle_kind_fences.tokenizer_registration_v1_at(daa) else { return Ok(false) };
        let has_book = parent.staged_court_row_v1(&PalwStagedCourtKeyV1::TokenizerRegistrationBook).is_some();
        let mut meter = TokenizerRegistrationCandidateWorkV1::new(*policy, has_book)
            .map_err(RuleError::PalwTokenizerRegistrationServiceExceeded)?;
        let ordered = self.acceptance_ordered_mergeset_without_selected_parent(&ctx.ghostdag_data, daa);
        let mut present = false;
        for block in std::iter::once(ctx.selected_parent()).chain(ordered.iter().copied()) {
            let source_daa = self.headers_store.get_daa_score(block).map_err(|why| {
                RuleError::LocalNativeCapacityUnavailable(NativeCoverageV1::unsupported::<Self>(
                    "tokenizer source activation",
                    &format!("missing native source header {block}: {why}"),
                ))
            })?;
            // Historical opaque payloads never acquire TKR meaning just because
            // their body is accepted after activation.
            if self.palw_lifecycle_kind_fences.tokenizer_registration_v1_at(source_daa).is_none() {
                continue;
            }
            let transactions = self.block_transactions_store.get(block).map_err(|why| {
                RuleError::LocalNativeCapacityUnavailable(NativeCoverageV1::unsupported::<Self>(
                    "tokenizer physical work scan",
                    &format!("missing native block body {block}: {why}"),
                ))
            })?;
            for tx in transactions.iter() {
                present |= tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE
                    && tx.payload.starts_with(TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1);
                meter
                    .charge_v1(tx, |id| tokenizer_registration_parent_quote_v1(parent, id))
                    .map_err(RuleError::PalwTokenizerRegistrationServiceExceeded)?;
            }
        }
        Ok(present)
    }
}
