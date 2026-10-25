//! Explicit source-bootstrap mechanics profile. Every policy value comes from
//! the operator's finite file; no production/security/economic value is chosen.
use super::*;
use kaspa_consensus_core::{
    config::params::{ForkActivation, Params},
    palw_source_bootstrap_v1::{PalwSelectedSourceBootstrapFenceV1, PalwSelectedSourceBootstrapPolicyV1},
};

pub fn private_source_bootstrap_params_v1(
    salt: &kaspa_consensus_core::config::drill::PalwDrillSaltV1,
    policy_file: &Path,
) -> Result<Params, String> {
    attach_policy(private_complete_check_params_v1(salt)?, salt, policy_file)
}

/// One explicit Params instance carries both the exact paid-v4 base and the
/// operator's source epoch. Old paid-v4 and source-v1 factories stay unchanged.
pub fn private_source_bootstrap_paid_params_v1(
    salt: &kaspa_consensus_core::config::drill::PalwDrillSaltV1,
    policy_file: &Path,
) -> Result<Params, String> {
    attach_policy(private_paid_claim_params_v4(salt)?, salt, policy_file)
}

/// Explicit optional V6 downstream namespace; absence preserves source-paid-v1
/// bytes exactly. No default policy values or legacy source IDs are changed.
pub fn private_source_bootstrap_paid_params_with_consumer_v6(
    salt: &kaspa_consensus_core::config::drill::PalwDrillSaltV1,
    source_policy_file: &Path,
    consumer_policy_file: Option<&Path>,
    economics_file: Option<&Path>,
) -> Result<Params, String> {
    let mut params = private_source_bootstrap_paid_params_v1(salt, source_policy_file)?;
    if let Some(path) = consumer_policy_file {
        let (raw, _owner) = kernel_paid_producer_v4::read_bounded(path, 4096)?;
        let (activation, policy): (u64, misaka_palw_kernel::claim_paid_bootstrap_source_v6::PaidBootstrapSourcePolicyV6) =
            borsh::from_slice(&raw).map_err(|e| e.to_string())?;
        params.palw_paid_bootstrap_source_v6 =
            Some(kaspa_consensus_core::palw_paid_bootstrap_source_v6::PalwPaidBootstrapSourceFenceV6 {
                activation: ForkActivation::new(activation),
                policy,
            });
        params.sync_palw_paid_bootstrap_source_v6();
    }
    if let Some(path) = economics_file {
        if params.palw_paid_bootstrap_source_v6.is_none() {
            return Err("source economics requires the explicit V6 consumer policy".into());
        }
        let (raw, _owner) = kernel_paid_producer_v4::read_bounded(path, 4096)?;
        let (activation, policy): (u64, kaspa_consensus_core::palw_paid_bootstrap_economics_v1::PalwSourceEconomicsPolicyV1) =
            borsh::from_slice(&raw).map_err(|e| e.to_string())?;
        policy.validate()?;
        if activation == 0 {
            return Err("source economics needs an explicit nonzero schedule".into());
        }
        params.palw_source_economics_v1 = Some(kaspa_consensus_core::palw_paid_bootstrap_economics_v1::PalwSourceEconomicsFenceV1 {
            activation: ForkActivation::new(activation),
            policy,
        });
        params.sync_palw_source_economics_v1();
    }
    if consumer_policy_file.is_some() || economics_file.is_some() {
        validate_private_source_bootstrap_relations_v1(&params, salt)?;
    }
    Ok(params)
}

fn attach_policy(
    mut params: Params,
    salt: &kaspa_consensus_core::config::drill::PalwDrillSaltV1,
    policy_file: &Path,
) -> Result<Params, String> {
    // Fixed-size schema only: activation then policy, canonical Borsh; no
    // embedded graph, model, Vec or caller-supplied own snapshot is decoded.
    let (raw, _owner) = kernel_paid_producer_v4::read_bounded(policy_file, 4096)?;
    let (activation, policy): (u64, PalwSelectedSourceBootstrapPolicyV1) = borsh::from_slice(&raw).map_err(|e| e.to_string())?;
    params.palw_selected_source_bootstrap_v1 =
        Some(PalwSelectedSourceBootstrapFenceV1 { at: ForkActivation::new(activation), policy });
    params.sync_palw_selected_source_bootstrap_v1();
    validate_private_source_bootstrap_relations_v1(&params, salt)?;
    Ok(params)
}

pub(super) fn validate_private_source_bootstrap_relations_v1(
    params: &Params,
    salt: &kaspa_consensus_core::config::drill::PalwDrillSaltV1,
) -> Result<(), String> {
    let fence = params.palw_selected_source_bootstrap_v1.ok_or("private source policy absent")?;
    fence.policy.validate().map_err(str::to_owned)?;
    // At zero the policy would commit to an identity which contains its own
    // ruleset hash. A scheduled nonzero fence is normalized out of identity;
    // the full Params fingerprint still commits to every explicit policy bit.
    if fence.at.daa_score() == 0 || fence.at == ForkActivation::never() || fence.at.daa_score() > fence.policy.freeze_cutoff_daa {
        return Err("private source policy needs an explicit nonzero scheduled activation at/before freeze".into());
    }
    let baseline = if params.palw_claim_compact_v4.is_some() || params.palw_material_release_recovery_v2.is_some() {
        private_paid_claim_params_v4(salt)?
    } else {
        private_complete_check_params_v1(salt)?
    };
    let mut stripped = params.clone();
    stripped.palw_selected_source_bootstrap_v1 = None;
    stripped.sync_palw_selected_source_bootstrap_v1();
    stripped.palw_paid_bootstrap_source_v6 = None;
    stripped.sync_palw_paid_bootstrap_source_v6();
    stripped.palw_source_economics_v1 = None;
    stripped.sync_palw_source_economics_v1();
    if stripped.consensus_params_id() != baseline.consensus_params_id() {
        return Err("private source profile changes rules outside its explicit bootstrap policy".into());
    }
    let mut ruleset = [0; 64];
    ruleset[..32].copy_from_slice(&params.consensus_identity_id().as_bytes());
    let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    if fence.policy.chain_genesis != params.genesis.hash
        || fence.policy.network != network
        || fence.policy.ruleset != kaspa_consensus_core::Hash64::from_bytes(ruleset)
    {
        return Err("private source policy names another own chain/network/base ruleset".into());
    }
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("private source needs its own native ConsensusV2 state".into());
    };
    if let Some(economics) = params.palw_source_economics_v1 {
        economics.policy.validate()?;
        let public = kaspa_consensus_core::config::drill::palw_chain_params_v1(
            kaspa_consensus_core::config::drill::palw_drill_network_v1(),
            None,
        )?;
        if params.palw_paid_bootstrap_source_v6.is_none()
            || economics.activation == ForkActivation::never()
            || economics.activation.daa_score() == 0
            || params.genesis.hash == public.genesis.hash
        {
            return Err("source economics refuses public genesis and ordinary activation".into());
        }
    }
    if let Some(consumer) = params.palw_paid_bootstrap_source_v6 {
        consumer.policy.validate()?;
        // Both source and consumer policies normalize out of the scheduled
        // base identity. Genesis activation would introduce a hash cycle via
        // bootstrap_policy -> source.policy.ruleset -> complete Params.
        if params.palw_claim_compact_v4.is_none()
            || params.palw_material_release_recovery_v2.is_none()
            || consumer.activation == ForkActivation::never()
            || consumer.activation.daa_score() == 0
            || consumer.activation.daa_score() >= fence.policy.ends_at_daa
            || consumer.policy.bootstrap_policy != fence.policy.id().as_bytes()
            || consumer.policy.network != network.as_bytes()
            || consumer.policy.maximum_events != fence.policy.selection.maximum_source_works
            || !bundle.state.paid_bootstrap_source_v6_mirror_matches(consumer.activation.daa_score(), &consumer.policy)
        {
            return Err("private V6 consumer changes its explicit source-policy/network/schedule/native mirror contract".into());
        }
    } else {
        // An absent optional consumer must also have an absent native mirror;
        // the baseline fingerprint alone omits these runtime-only mirrors.
        params.validate_palw_paid_bootstrap_source_v6().map_err(|e| e.to_string())?;
    }
    if bundle.state.selected_source_bootstrap_v1_mirror_matches(fence.at.daa_score(), &fence.policy) == false {
        return Err("private source native mirror differs from explicit policy".into());
    }
    Ok(())
}
