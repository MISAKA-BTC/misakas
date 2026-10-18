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
    // Fixed-size schema only: activation then policy, canonical Borsh; no
    // embedded graph, model, Vec or caller-supplied own snapshot is decoded.
    let (raw, _owner) = kernel_paid_producer_v4::read_bounded(policy_file, 4096)?;
    let (activation, policy): (u64, PalwSelectedSourceBootstrapPolicyV1) = borsh::from_slice(&raw).map_err(|e| e.to_string())?;
    let mut params = private_complete_check_params_v1(salt)?;
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
    let baseline = private_complete_check_params_v1(salt)?;
    let mut stripped = params.clone();
    stripped.palw_selected_source_bootstrap_v1 = None;
    stripped.sync_palw_selected_source_bootstrap_v1();
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
    if bundle.state.selected_source_bootstrap_v1_mirror_matches(fence.at.daa_score(), &fence.policy) == false {
        return Err("private source native mirror differs from explicit policy".into());
    }
    Ok(())
}
