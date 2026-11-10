//! RFC-0051 transport fence. Shipped network presets leave it absent.

use super::{ForkActivation, Params};
use crate::{
    palw_mode_v2::PalwModeV2Error, palw_tokenizer_registration_carriage_v1::TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1,
    palw_tokenizer_registration_v1::TokenizerRegistrationPolicyV1,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenizerRegistrationFenceV1 {
    pub activation: ForkActivation,
    /// First DAA refusing new Open. Existing contracts keep this transport.
    pub open_cutoff: Option<ForkActivation>,
    pub policy: TokenizerRegistrationPolicyV1,
}

impl TokenizerRegistrationFenceV1 {
    pub fn active_at(&self, daa: u64) -> bool {
        self.activation != ForkActivation::never() && self.activation.is_active(daa)
    }

    pub fn open_cutoff_v1(&self) -> Option<ForkActivation> {
        self.open_cutoff.filter(|f| *f != ForkActivation::never())
    }

    fn validate_shape(&self) -> Result<(), PalwModeV2Error> {
        if self.open_cutoff_v1().is_some_and(|cutoff| cutoff.daa_score() <= self.activation.daa_score()) {
            return Err(PalwModeV2Error::Invalid("tokenizer registration Open cutoff must follow transport activation"));
        }
        let p = self.policy;
        let work = p.bookkeeping_work_v1().map_err(PalwModeV2Error::Invalid)?;
        let budget = p.effective_reservation_budget_v1().map_err(PalwModeV2Error::Invalid)?;
        if p.carrier_overhead_bytes < TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1.len() as u64
            || p.carrier_overhead_bytes >= p.max_carrier_bytes
            || p.metadata_bytes >= p.max_retained_bytes
            || p.metadata_bytes >= budget.io
            || [work.open, work.part, work.finish].into_iter().any(|w| w >= budget.work || w >= p.max_work)
            || p.max_session_daa < 4
        {
            return Err(PalwModeV2Error::Invalid("tokenizer registration policy cannot carry the minimum reservation"));
        }
        Ok(())
    }
}

impl Params {
    pub fn palw_tokenizer_registration_v1_fence(&self) -> Option<&TokenizerRegistrationFenceV1> {
        self.palw_tokenizer_registration_v1.as_ref().filter(|f| f.activation != ForkActivation::never())
    }

    pub fn palw_tokenizer_registration_v1_at(&self, daa: u64) -> Option<&TokenizerRegistrationFenceV1> {
        self.palw_tokenizer_registration_v1_fence().filter(|f| f.active_at(daa))
    }

    pub fn validate_palw_tokenizer_registration_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_tokenizer_registration_v1_fence() else {
            return Ok(());
        };
        fence.validate_shape()?;
        let at = fence.activation.daa_score();
        // Ordered TKR1 rehearsal requires the audit's pre-object prefix,
        // which preserves the selected-parent point between native carriers.
        // The other four fences are the native tokenizer intake prerequisites.
        if [
            self.palw_audit_2026_09_11_fence(),
            self.palw_class_semantics_v1_fence(),
            self.palw_staged_public_court_v1_fence(),
            self.palw_tir_container_identity_v1_fence(),
            self.palw_tir_public_conviction_v1_fence(),
        ]
        .into_iter()
        .any(|dependency| dependency.is_none_or(|f| !f.is_active(at)))
        {
            return Err(PalwModeV2Error::Invalid(
                "tokenizer registration activation precedes its lifecycle or native intake dependencies",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::params::TESTNET_PARAMS,
        palw_lifecycle_objects_v2::validate_palw_lifecycle_tx_in_context_v1,
        palw_tokenizer_registration_carriage_v1::encode_tokenizer_registration_payload_v1,
        palw_tokenizer_registration_v1::{TokenizerRegistrationOpenV1, TokenizerRegistrationWireV1},
    };

    #[test]
    fn schedule_fingerprint_and_header_gate_preserve_dormant_rules() {
        let baseline = TESTNET_PARAMS.clone();
        let (policy, body) = crate::palw_tokenizer_registration_v1::tests::fixture();
        let mut params = baseline.clone();
        let mut fence = TokenizerRegistrationFenceV1 { activation: ForkActivation::never(), open_cutoff: None, policy };
        params.palw_tokenizer_registration_v1 = Some(fence);
        assert_eq!(params.consensus_params_id(), baseline.consensus_params_id());
        assert_eq!(params.consensus_schedule_id(), baseline.consensus_schedule_id());
        assert!(params.palw_tokenizer_registration_v1_at(u64::MAX).is_none());

        fence.activation = ForkActivation::new(20);
        params.palw_tokenizer_registration_v1 = Some(fence);
        assert_ne!(params.consensus_schedule_id(), baseline.consensus_schedule_id());
        assert_ne!(params.consensus_params_id(), baseline.consensus_params_id());
        assert!(fence.validate_shape().is_ok());
        assert!(params.validate_palw_tokenizer_registration_v1().is_err()); // Missing prerequisite forks.
        let mut changed = params.clone();
        changed.palw_tokenizer_registration_v1.as_mut().unwrap().policy.rent_per_work += 1;
        assert_ne!(changed.consensus_schedule_id(), params.consensus_schedule_id());
        assert_ne!(changed.consensus_params_id(), params.consensus_params_id());

        let mut too_large = fence;
        too_large.policy.max_pending_total = u32::MAX;
        assert!(too_large.validate_shape().is_err());
        let mut cut = params.clone();
        cut.palw_tokenizer_registration_v1.as_mut().unwrap().open_cutoff = Some(ForkActivation::new(30));
        assert_ne!(cut.consensus_schedule_id(), params.consensus_schedule_id());
        assert_ne!(cut.consensus_params_id(), params.consensus_params_id());
        let mut invalid_cut = fence;
        invalid_cut.open_cutoff = Some(fence.activation);
        assert!(invalid_cut.validate_shape().is_err());
        let gates = cut.palw_lifecycle_kind_fences_v1();
        assert_eq!(gates.tokenizer_registration_open_cutoff_daa_v1_at(20), Some(30));
        assert!(gates.tokenizer_registration_v1_at(30).is_some());
        let payload = encode_tokenizer_registration_payload_v1(&TokenizerRegistrationWireV1::Open(TokenizerRegistrationOpenV1 {
            body,
            signature: [0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }))
        .unwrap();
        assert!(validate_palw_lifecycle_tx_in_context_v1(&payload, &gates, 20).is_ok());
        assert!(validate_palw_lifecycle_tx_in_context_v1(&payload, &gates, 30).is_ok());
        let truncated = &payload[..payload.len() - 1];
        assert!(validate_palw_lifecycle_tx_in_context_v1(truncated, &gates, 19).is_ok());
        assert!(validate_palw_lifecycle_tx_in_context_v1(truncated, &gates, 20).is_err());
    }
}
