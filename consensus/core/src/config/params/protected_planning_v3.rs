//! Explicit private protected-planning selection. Public presets have no value.
//! This configuration does not waive native Court activation/readiness checks.
use super::{ForkActivation, Params};
use crate::config::genesis::PALW_T12_GENESIS;
use crate::{palw_mode_v2::PalwModeV2Error, palw_positive_contract_v2::ProtectedPlanningDeclarationV3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedPlanningFenceV3 {
    pub activation: ForkActivation,
    pub declaration: ProtectedPlanningDeclarationV3,
}

impl ProtectedPlanningFenceV3 {
    pub fn active_at(&self, daa: u64) -> bool {
        self.activation != ForkActivation::never() && self.activation.is_active(daa)
    }
}

impl Params {
    /// Effective owners of protected TFO3/PMR/Batch processing. The separately
    /// reserved Positive Court object-176 namespace does not own these carriers
    /// and cannot substitute for any of their native prerequisites.
    pub(super) fn protected_native_inputs_active_v3(&self, at: u64) -> bool {
        let active = |f: Option<ForkActivation>| f.is_some_and(|f| f != ForkActivation::never() && f.is_active(at));
        active(self.palw_tir_public_conviction_v1_fence())
            && active(self.palw_staged_tir_route_v1_fence())
            && self.palw_lifecycle_kind_fences_v1().batch_court_in_force_at(at)
            && self.palw_tokenizer_registration_v1_at(at).is_some()
            && active(self.palw_model_panel_one_retention_v1)
    }

    /// A malformed active declaration stays present and fails validation;
    /// it cannot silently fall back to pinned terms.
    pub fn palw_protected_planning_v3_at(&self, daa: u64) -> Option<ProtectedPlanningFenceV3> {
        self.palw_protected_planning_v3.filter(|f| f.active_at(daa))
    }

    pub fn validate_palw_protected_planning_v3(&self) -> Result<(), PalwModeV2Error> {
        let Some(fence) = self.palw_protected_planning_v3.filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        fence
            .declaration
            .validate_v3()
            .map_err(|_| PalwModeV2Error::Invalid("protected planning requires a complete finite declaration"))?;
        if self.net != crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12)
            || self.genesis.hash == PALW_T12_GENESIS.hash
        {
            return Err(PalwModeV2Error::Invalid("protected planning draft requires isolated salted T12 genesis"));
        }
        let at = fence.activation.daa_score();
        if !self.protected_native_inputs_active_v3(at) || self.palw_manifest_ingress_v3_at(at).is_none() {
            return Err(PalwModeV2Error::Invalid(
                "protected planning requires native TIR court, Batch, tokenizer, retention and Manifest ingress",
            ));
        }
        self.validate_palw_manifest_ingress_v3()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Schema-only input, never a calibrated profile or an admitted claim.
    fn declaration() -> ProtectedPlanningDeclarationV3 {
        ProtectedPlanningDeclarationV3::decode_json_v3(include_bytes!("../../../tests/fixtures/protected-planning-schema-v3.json"))
            .unwrap()
    }

    /// Isolated configuration-shape exercise only. No daemon startup, resource
    /// calibration, source readiness or successful claim is asserted here.
    fn native_params() -> Params {
        use super::super::{ManifestIngressFenceV3, TokenizerRegistrationFenceV1};
        let salt = crate::config::drill::PalwDrillSaltV1::from_hex(&"53".repeat(32)).unwrap();
        let mut params = super::super::palw_t12_drill_params_v1(&salt);
        let at = ForkActivation::new(100);
        params.palw_staged_public_court_v1 = Some(at);
        params.palw_staged_tir_route_v1 = Some(at);
        params.palw_objective_offence = Some(at);
        params.palw_rcore_plus = Some(at);
        params.palw_capacity_aggregate_liability = Some(crate::palw_aggregate_liability_v1::PalwCapacityLiabilityV1::t12_at_v1(at));
        params.palw_tir_public_conviction_v1 = Some(at);
        params.palw_audit_2026_09_11 = Some(at);
        params.palw_batch_court_v1 = Some(at);
        params.palw_tokenizer_registration_v1 = Some(TokenizerRegistrationFenceV1 {
            activation: at,
            open_cutoff: None,
            policy: crate::palw_tokenizer_registration_v1::tests::fixture().0,
        });
        params.palw_model_panel_one_retention_v1 = Some(at);
        params.palw_manifest_ingress_v3 = Some(ManifestIngressFenceV3 {
            activation: at,
            maximum_source_blocks: 1,
            maximum_transactions: 1,
            maximum_requests: 1,
            maximum_request_bytes: 1,
            maximum_archive_heap: 1,
            maximum_archive_work: 1,
            replay_passes: 3,
            replay_visits: 1,
            byte_sompi: 1,
            work_sompi: 1,
            io_sompi: 1,
            control_sompi: 1,
            retained_byte_sompi: 1,
            archive_work_sompi: 1,
        });
        params.palw_protected_planning_v3 = Some(ProtectedPlanningFenceV3 { activation: at, declaration: declaration() });
        params
    }

    #[test]
    fn native_carriers_require_their_effective_owners_not_reserved_object_176() {
        let params = native_params();
        assert!(params.palw_positive_court_v2.is_none());
        assert!(params.palw_court_family_supplier_v1.is_none());
        assert!(params.palw_settlement_once_v1.is_none());
        assert!(!params.palw_lifecycle_kind_fences_v1().positive_court_reserved_active_at_v2(100));
        assert!(!params.protected_native_inputs_active_v3(99));
        assert!(params.protected_native_inputs_active_v3(100));
        params.validate_palw_manifest_ingress_v3().unwrap();
        params.validate_palw_protected_planning_v3().unwrap();
        // An unrelated reserved Positive namespace neither becomes adopted nor
        // satisfies native court prerequisites through this private declaration.
        let mut unrelated = params.clone();
        unrelated.palw_positive_court_v2 = Some(super::super::PositiveCourtFenceV2::unapproved_probe_at(ForkActivation::new(100)));
        assert!(unrelated.validate_palw_positive_court_v2().is_err());
        unrelated.palw_staged_tir_route_v1 = None;
        assert!(!unrelated.protected_native_inputs_active_v3(100));
        assert!(unrelated.validate_palw_manifest_ingress_v3().is_err());
        let mut public = params;
        public.genesis = PALW_T12_GENESIS;
        assert!(public.validate_palw_manifest_ingress_v3().is_err());
        assert!(public.validate_palw_protected_planning_v3().is_err());
    }

    #[test]
    fn parameter_override_retains_protected_declarations_and_their_identities() {
        let empty_override = || serde_json::from_str::<super::super::OverrideParams>("{}").unwrap();
        let declared = native_params();
        // Normalize the legacy gates that overrides deliberately clear before
        // checking that a subsequent no-op override preserves these inputs.
        let mut original = declared.clone().override_params(empty_override());
        original.palw_manifest_ingress_v3 = declared.palw_manifest_ingress_v3;
        original.palw_protected_planning_v3 = declared.palw_protected_planning_v3;
        let carried = original.clone().override_params(empty_override());
        assert_eq!(carried.palw_manifest_ingress_v3, declared.palw_manifest_ingress_v3);
        assert_eq!(carried.palw_protected_planning_v3, declared.palw_protected_planning_v3);
        assert_eq!(carried.consensus_params_id(), original.consensus_params_id());
        assert_eq!(carried.consensus_schedule_id(), original.consensus_schedule_id());
        assert_eq!(carried.consensus_identity_id(), original.consensus_identity_id());
        assert_eq!(carried.palw_manifest_ingress_v3_at(99), None);
        assert_eq!(carried.palw_manifest_ingress_v3_at(100), declared.palw_manifest_ingress_v3);
        assert_eq!(carried.palw_protected_planning_v3_at(99), None);
        assert_eq!(carried.palw_protected_planning_v3_at(100), declared.palw_protected_planning_v3);
    }

    #[test]
    fn each_native_dependency_and_its_effective_clock_remain_required() {
        let original = native_params();
        for missing in 0..9 {
            let mut params = original.clone();
            match missing {
                0 => params.palw_staged_public_court_v1 = None,
                1 => params.palw_staged_tir_route_v1 = None,
                2 => params.palw_tir_public_conviction_v1 = None,
                3 => params.palw_capacity_aggregate_liability = None,
                4 => params.palw_batch_court_v1 = None,
                5 => params.palw_tokenizer_registration_v1 = None,
                6 => params.palw_model_panel_one_retention_v1 = None,
                7 => params.palw_objective_offence = Some(ForkActivation::new(101)),
                8 => params.palw_audit_2026_09_11 = Some(ForkActivation::new(101)),
                _ => unreachable!(),
            }
            assert!(!params.protected_native_inputs_active_v3(100), "missing {missing}");
            assert!(params.validate_palw_manifest_ingress_v3().is_err(), "missing {missing}");
            assert!(params.validate_palw_protected_planning_v3().is_err(), "missing {missing}");
        }
    }

    #[test]
    fn planning_does_not_accept_malformed_active_manifest_terms() {
        let mut params = native_params();
        params.palw_manifest_ingress_v3.as_mut().unwrap().replay_passes = 2;
        assert!(params.palw_manifest_ingress_v3_at(100).is_some());
        assert!(params.validate_palw_protected_planning_v3().is_err());
    }

    #[test]
    fn activation_selection_preserves_original_clock_and_invalid_active_input() {
        let mut params = super::super::TESTNET_PARAMS.clone();
        let fence = ProtectedPlanningFenceV3 { activation: ForkActivation::new(100), declaration: declaration() };
        params.palw_protected_planning_v3 = Some(fence);
        assert_eq!(params.palw_protected_planning_v3_at(99), None);
        assert_eq!(params.palw_protected_planning_v3_at(100), Some(fence));
        assert_eq!(params.palw_protected_planning_v3_at(101), Some(fence));
        // A later tip does not select this declaration for a pre-activation original.
        assert_eq!(params.palw_protected_planning_v3_at(99), None);
        let mut invalid = serde_json::to_value(fence).unwrap();
        invalid["declaration"]["version"] = serde_json::json!(4);
        let invalid: ProtectedPlanningFenceV3 = serde_json::from_value(invalid).unwrap();
        params.palw_protected_planning_v3 = Some(invalid);
        assert_eq!(params.palw_protected_planning_v3_at(100), Some(invalid));
        assert!(params.validate_palw_protected_planning_v3().is_err());
    }

    #[test]
    fn dormant_declaration_does_not_change_public_identities_and_fence_walk_only_moves_activation() {
        let original = super::super::TESTNET_PARAMS.clone();
        let mut dormant = original.clone();
        dormant.palw_protected_planning_v3 =
            Some(ProtectedPlanningFenceV3 { activation: ForkActivation::never(), declaration: declaration() });
        assert_eq!(original.consensus_params_id(), dormant.consensus_params_id());
        assert_eq!(original.consensus_schedule_id(), dormant.consensus_schedule_id());
        assert_eq!(original.consensus_identity_id(), dormant.consensus_identity_id());
        assert_eq!(dormant.palw_protected_planning_v3_at(u64::MAX), None);
        let mut active = original;
        active.palw_protected_planning_v3 =
            Some(ProtectedPlanningFenceV3 { activation: ForkActivation::new(100), declaration: declaration() });
        active.for_each_fence(&mut |daa| {
            if *daa == 100 {
                *daa = 101;
            }
        });
        let moved = active.palw_protected_planning_v3.unwrap();
        assert_eq!(moved.activation, ForkActivation::new(101));
        assert_eq!(moved.declaration, declaration());
    }
}
