//! Independently fingerprinted, unapproved private paid-claim mechanics factory.
//! No public Args/preset, registry approval, class, receipt, source Final, or
//! monetary material enrollment is created by this configuration constructor.
use kaspa_consensus_core::{
    config::{
        drill::PalwDrillSaltV1,
        params::{ForkActivation, Params},
    },
    palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1,
    palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1,
    palw_bond_budget_v1::{PalwPaidConsumerLifecycleFenceV1, PalwPaidConsumerLifecyclePolicyV1},
    palw_claim_challenge_v2::PalwClaimChallengeFenceV2,
    palw_monetary_material_v1::{PalwMonetaryMaterialFenceV1, PalwMonetaryMaterialPolicyV1},
    palw_panel_lifetime_v2::PalwPanelLifetimeFenceV2,
};
use kaspa_hashes::Hash64;

/// **Explicit mechanics-only paid consumer lifecycle for the private paid-v3 profile** (2026-10-15; unapproved, and refused
/// by the public validator whenever armed): finite service/dispute/material spans on the private clock — the values the
/// native-Final conformance slice runs — chosen to exercise the immutable accepted-claim terms, never a measured cost.
/// The old 1,000-DAA service could not contain even T12's minimum floor path
/// (600 bind + 600 receipt + 120 challenge + 1). Derive this private-only floor
/// from its unchanged rules; per-class longer receipt paths remain checked by
/// the real contract planner. This changes the private fingerprint, not a preset.
pub(super) fn private_paid_consumer_lifecycle_v3(
    activation: ForkActivation,
    params: &Params,
) -> Result<PalwPaidConsumerLifecycleFenceV1, String> {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("private consumer lifecycle requires V2 clocks".into());
    };
    let state = &bundle.state;
    let minimum = state
        .window_bind()
        .checked_add(state.window_receipt())
        .and_then(|v| v.checked_add(state.window_challenge_at(activation.daa_score())))
        .and_then(|v| v.checked_add(1))
        .ok_or("private consumer minimum service overflow")?;
    Ok(PalwPaidConsumerLifecycleFenceV1 {
        activation,
        policy: PalwPaidConsumerLifecyclePolicyV1 {
            version: 1,
            service_span_daa: 1_000.max(minimum),
            dispute_tail_daa: 3_000,
            material_tail_daa: 2_000,
        },
    })
}

pub fn private_paid_claim_params_v3(salt: &PalwDrillSaltV1) -> Result<Params, String> {
    let mut params = super::private_complete_check_params_v1(salt)?;
    let activation = ForkActivation::new(0);
    let mut budget = kaspa_consensus_core::palw_bond_budget_v1::PalwBondBudgetFenceV1::at(activation);
    // Explicit mechanics-only cap units. One KAS capital unit avoids the
    // all-one probe's known whole reward-block B-quantum refusal/overflow.
    budget.policy.capital_unit_sompi = kaspa_consensus_core::constants::SOMPI_PER_KASPA;
    budget.policy.block_units_per_unit = kaspa_consensus_core::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1;
    budget.policy.reward_per_unit_sompi = kaspa_consensus_core::constants::SOMPI_PER_KASPA;
    budget.policy.max_open_claims_per_bond = 1000;
    budget.policy.validate().map_err(str::to_owned)?;
    params.palw_bond_budget_v1 = Some(budget);
    params.palw_reporter_income_v2 = Some(activation);
    params.palw_material_income_v3 = Some(activation);
    params.palw_provider_court_v1 = Some(activation);
    let mut source = PalwBeaconSourceFenceV1::unapproved_probe_at(activation);
    let ledger = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    source.policy.source_final_slack_daa = ledger
        .challenge_window_daa
        .checked_add(ledger.court_deadline_daa)
        .and_then(|v| v.checked_add(ledger.proof_grace_daa))
        .ok_or("private source/court floor overflow")?;
    params.palw_beacon_source_opt_in_v1 = Some(source);
    let mut fee = PalwBeaconSourceFeeFenceV1::unapproved_probe_at(activation);
    // This is an explicit large private mechanics scope, not a measured economic
    // time/capital price. Every real target must still satisfy its signed H.
    fee.policy.cost_bounds.as_mut().ok_or("private fee scope missing")?.maximum_liability_daa = 1_000_000;
    params.palw_beacon_source_fee_v1 = Some(fee);
    let mut claim = PalwClaimChallengeFenceV2::unapproved_probe_at(activation);
    claim.policy.maximum_rounds = 4;
    params.palw_claim_challenge_v2 = Some(claim);
    params.palw_typed_roots_v1 = Some(activation);
    // RFC07 absolute Panel consumer lifetime and the immutable paid consumer lifecycle, armed from DAA 0 beside PAV3 so the
    // private chain exercises them (2026-10-15). Both are the structural probe values; neither is an approval. The probe's
    // spans (10,000 DAA service, 1,000 pause) are already finite on the private clock.
    params.palw_panel_lifetime_v2 = Some(PalwPanelLifetimeFenceV2::unapproved_probe_at(activation));
    params.palw_paid_consumer_lifecycle_v1 = Some(private_paid_consumer_lifecycle_v3(activation, &params)?);
    params.palw_monetary_material_v1 = Some(PalwMonetaryMaterialFenceV1 {
        activation,
        policy: PalwMonetaryMaterialPolicyV1 {
            version: 1,
            max_contracts: 1024,
            max_contracts_per_provider: 1024,
            max_bytes_per_contract: 32 * 1024 * 1024,
            max_bytes_per_provider: 256 * 1024 * 1024,
            minimum_collateral_sompi: 100,
            minimum_fee_sompi: 100,
            window_daa: 20,
            response_window_daa: 20,
            max_enrollment_daa: 10_000,
            cost_report_root: Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                b"MISAKA/PRIVATE/UNAPPROVED-MATERIAL-COST-SCOPE/V3",
                &(),
            )),
        },
    });
    // The consumer lifecycle mirror rides the budget sync; the Panel lifetime mirror rides the Panel sync.
    params.sync_palw_bond_budget_v1();
    params.sync_palw_claim_challenge_v2();
    params.sync_palw_permissionless_panel_v1();
    validate_private_paid_claim_relations_v3(&params)?;
    Ok(params)
}
pub(super) fn validate_private_paid_claim_relations_v3(params: &Params) -> Result<(), String> {
    let activation = ForkActivation::new(0);
    let source = params.palw_beacon_source_opt_in_v1.ok_or("private paid source missing")?;
    let fee = params.palw_beacon_source_fee_v1.ok_or("private paid fee missing")?;
    let claim = params.palw_claim_challenge_v2.ok_or("private signed claim capability missing")?;
    let material = params.palw_monetary_material_v1.as_ref().ok_or("private signed material capability missing")?;
    let lifetime = params.palw_panel_lifetime_v2.ok_or("private Panel lifetime missing")?;
    let consumer = params.palw_paid_consumer_lifecycle_v1.as_ref().ok_or("private paid consumer lifecycle missing")?;
    params.palw_bond_budget_v1.as_ref().ok_or("private native budget missing")?.policy.validate().map_err(str::to_owned)?;
    claim.policy.validate()?;
    material.policy.validate().map_err(str::to_owned)?;
    lifetime.policy.validate().map_err(str::to_owned)?;
    if lifetime != PalwPanelLifetimeFenceV2::unapproved_probe_at(activation)
        || *consumer != private_paid_consumer_lifecycle_v3(activation, params)?
        || !consumer.policy.valid()
    {
        return Err("private Panel lifetime / paid consumer lifecycle differ from their exact mechanics-only values".into());
    }
    // The mirrors the fold reads must be the fences' own (a stale mirror would arm another policy than the fingerprint names).
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("private paid probe has no V2 bundle".into());
    };
    // Since 63b23dc0f the Panel lifetime mirrors only where a Panel engine is mirrored (it governs Panel consumers only); this
    // profile arms no Panel engine, so its fence must leave no lifetime mirror — and one beside an engine must be the fence's own.
    let lifetime_mirror = bundle.state.panel_v3().map(|_| (0, lifetime.policy));
    if bundle.state.panel_lifetime_v2_at(0).map(|m| (m.from_daa, m.policy)) != lifetime_mirror
        || bundle.state.paid_consumer_lifecycle().map(|m| (m.from_daa, m.policy)) != Some((0, consumer.policy))
        || !bundle.state.paid_consumer_lifecycle_active_at(0)
    {
        return Err("private Panel lifetime / paid consumer lifecycle mirrors differ from their fences".into());
    }
    if params.skip_proof_of_work
        || source.activation != activation
        || fee.activation != activation
        || claim.activation != activation
        || material.activation != activation
        || params.palw_typed_roots_v1 != Some(activation)
        || claim.policy.lifetime_review_scope != [0; 64]
        || claim.policy.review_authority != [0; 64]
        || claim.policy.source_independence_evidence != [0; 64]
        || claim.policy.economic_evidence != [0; 64]
        || !params.palw_bond_budget_v1.as_ref().is_some_and(|f| f.activation.is_active(0))
        || !params.palw_reporter_income_v2.is_some_and(|f| f.is_active(0))
        || !params.palw_audit_2026_09_23.is_some_and(|f| f.is_active(0))
        || !params.palw_economic_safety.is_some_and(|f| f.is_active(0))
        || !params.palw_material_income_v3.is_some_and(|f| f.is_active(0))
        || !params.palw_provider_court_v1.is_some_and(|f| f.is_active(0))
        || !params.palw_probabilistic_constraints_v1.is_some_and(|f| f.is_active(0))
        || !params.palw_panel_free_v1.as_ref().is_some_and(|f| f.activation.is_active(0))
    {
        return Err("private paid probe changes its actual gates, mirror prerequisites, or claims security/economic approval".into());
    }
    let ledger = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    let floor = ledger
        .challenge_window_daa
        .checked_add(ledger.court_deadline_daa)
        .and_then(|v| v.checked_add(ledger.proof_grace_daa))
        .ok_or("private source clock overflow")?;
    if source.policy.deposit_sompi == 0
        || source.policy.max_total == 0
        || source.policy.max_per_producer == 0
        || source.policy.source_final_slack_daa < floor
        || fee.policy.minimum_fee_sompi == 0
        || fee.policy.max_funded_sources == 0
    {
        return Err("private source value/timing/capacity missing".into());
    }
    let costs = fee.policy.cost_bounds.ok_or("private positive cost proposal missing")?;
    costs.minimum_gross_fee(&misaka_palw_kernel::beacon_source::BeaconSourceTermsV1 {
        context: [1; 64],
        seal_start: 1,
        seal_end: 2,
        reveal_end: 3,
        final_by: 4,
        hold_until: 5,
        deposit: source.policy.deposit_sompi,
        max_per_producer: source.policy.max_per_producer,
        max_total: source.policy.max_total,
    })?;
    // Keep all other production startup/value/dependency validators. Only the
    // explicitly unapproved new release fences are made dormant in this copy;
    // their actual values/dependencies were validated above. The running Config
    // keeps their original values and fingerprints, not this validation copy.
    let mut checked = params.clone();
    checked.palw_beacon_source_opt_in_v1.as_mut().unwrap().activation = ForkActivation::never();
    checked.palw_beacon_source_fee_v1.as_mut().unwrap().activation = ForkActivation::never();
    checked.palw_claim_challenge_v2.as_mut().unwrap().activation = ForkActivation::never();
    checked.palw_monetary_material_v1.as_mut().unwrap().activation = ForkActivation::never();
    checked.palw_typed_roots_v1 = Some(ForkActivation::never());
    checked.palw_bond_budget_v1.as_mut().unwrap().activation = ForkActivation::never();
    checked.palw_reporter_income_v2 = Some(ForkActivation::never());
    checked.palw_material_income_v3 = Some(ForkActivation::never());
    checked.palw_provider_court_v1 = Some(ForkActivation::never());
    checked.palw_panel_lifetime_v2.as_mut().unwrap().activation = ForkActivation::never();
    checked.palw_paid_consumer_lifecycle_v1.as_mut().unwrap().activation = ForkActivation::never();
    // The v4 profile's release recovery rides the monetary covenant, which this copy makes dormant: dormant with it (its own
    // mechanics scope was checked by the v4 relations; production validation refuses recovery without the covenant first).
    if let Some(recovery) = checked.palw_material_release_recovery_v2.as_mut() {
        recovery.activation = ForkActivation::never();
    }
    checked.sync_palw_bond_budget_v1();
    checked.sync_palw_claim_challenge_v2();
    checked.sync_palw_permissionless_panel_v1();
    super::validate_private_complete_relations(&checked)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The paid-v3 profile arms the Panel lifetime and the paid consumer lifecycle from DAA 0 with their exact mechanics-only
    /// values and mirrors, the relation check refuses any other value or a stale mirror, and the public validator still refuses
    /// both whenever armed.
    #[test]
    fn paid_v3_arms_the_panel_lifetime_and_the_consumer_lifecycle_exactly_and_they_stay_publicly_refused() {
        let salt = PalwDrillSaltV1::from_bytes([0x15; 32]).unwrap();
        let params = private_paid_claim_params_v3(&salt).unwrap();
        let at = ForkActivation::new(0);
        assert_eq!(params.palw_panel_lifetime_v2, Some(PalwPanelLifetimeFenceV2::unapproved_probe_at(at)));
        assert_eq!(params.palw_paid_consumer_lifecycle_v1, Some(private_paid_consumer_lifecycle_v3(at, &params).unwrap()));
        assert!(params.palw_paid_consumer_lifecycle_active_at(0));
        let consumer = params.palw_paid_consumer_lifecycle_v1.as_ref().unwrap();
        assert_eq!(consumer.policy.service_span_daa, 1_321);
        assert_eq!(consumer.policy.dispute_tail_daa, 3_000);
        assert_eq!(consumer.policy.material_tail_daa, 2_000);
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("V2")
        };
        let minimum = bundle.state.window_bind() + bundle.state.window_receipt() + bundle.state.window_challenge_at(0) + 1;
        assert!(consumer.policy.terms(0).unwrap().admits_service(0, minimum));
        assert!(
            !PalwPaidConsumerLifecyclePolicyV1 { service_span_daa: 1_000, ..consumer.policy }
                .terms(0)
                .unwrap()
                .admits_service(0, minimum),
            "the previous profile could never include its first ordinary claim"
        );

        assert!(params.validate_palw_panel_lifetime_v2().is_err(), "armed lifetime stays publicly refused");
        assert!(params.validate_palw_paid_consumer_lifecycle_v1().is_err(), "armed lifecycle stays publicly refused");
        validate_private_paid_claim_relations_v3(&params).unwrap();
        let mut repriced = params.clone();
        repriced.palw_paid_consumer_lifecycle_v1.as_mut().unwrap().policy.service_span_daa += 1;
        repriced.sync_palw_bond_budget_v1();
        assert!(validate_private_paid_claim_relations_v3(&repriced).is_err(), "another consumer value");
        let mut stale = params.clone();
        stale.palw_panel_lifetime_v2.as_mut().unwrap().policy.maximum_retries += 1;
        assert!(validate_private_paid_claim_relations_v3(&stale).is_err(), "another lifetime value");
        stale.palw_panel_lifetime_v2 = params.palw_panel_lifetime_v2;
        stale.palw_paid_consumer_lifecycle_v1 = None;
        assert!(validate_private_paid_claim_relations_v3(&stale).is_err(), "a missing lifecycle");
        assert_ne!(
            params.consensus_params_id(),
            {
                let mut without = params.clone();
                without.palw_panel_lifetime_v2 = None;
                without.palw_paid_consumer_lifecycle_v1 = None;
                without.consensus_params_id()
            },
            "the private fingerprint names both fences"
        );
    }
}
