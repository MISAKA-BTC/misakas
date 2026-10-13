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
    palw_claim_challenge_v2::PalwClaimChallengeFenceV2,
    palw_monetary_material_v1::{PalwMonetaryMaterialFenceV1, PalwMonetaryMaterialPolicyV1},
};
use kaspa_hashes::Hash64;

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
    params.sync_palw_bond_budget_v1();
    params.sync_palw_claim_challenge_v2();
    validate_private_paid_claim_relations_v3(&params)?;
    Ok(params)
}
pub(super) fn validate_private_paid_claim_relations_v3(params: &Params) -> Result<(), String> {
    let activation = ForkActivation::new(0);
    let source = params.palw_beacon_source_opt_in_v1.ok_or("private paid source missing")?;
    let fee = params.palw_beacon_source_fee_v1.ok_or("private paid fee missing")?;
    let claim = params.palw_claim_challenge_v2.ok_or("private signed claim capability missing")?;
    let material = params.palw_monetary_material_v1.as_ref().ok_or("private signed material capability missing")?;
    params.palw_bond_budget_v1.as_ref().ok_or("private native budget missing")?.policy.validate().map_err(str::to_owned)?;
    claim.policy.validate()?;
    material.policy.validate().map_err(str::to_owned)?;
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
    checked.sync_palw_bond_budget_v1();
    checked.sync_palw_claim_challenge_v2();
    super::validate_private_complete_relations(&checked)
}
