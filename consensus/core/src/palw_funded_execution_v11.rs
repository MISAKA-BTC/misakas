//! Separate funded flat-execution fence and immutable network policy identity.
//! No preset activates this route. A declared policy is not an approval of its
//! economics, public-prosecution completeness or checkpoint-population coverage.
use crate::config::params::{ForkActivation, Params};
use crate::palw_flat_execution_v11::NativeExecutionPoliciesV11;
use crate::palw_mode_v2::PalwModeV2Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFundedExecutionFenceV11 {
    pub activation: ForkActivation,
    pub policies: NativeExecutionPoliciesV11,
}
impl Params {
    pub fn palw_funded_execution_active_at_v11(&self, daa: u64) -> bool {
        self.palw_funded_execution_v11.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_probabilistic_constraints_active_at(daa)
            && self.palw_panel_free_v1.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
    }
    pub fn validate_palw_funded_execution_v11(&self) -> Result<(), PalwModeV2Error> {
        match &self.palw_funded_execution_v11 {
            Some(f) if f.activation != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_funded_execution_v11 cannot be armed: funded public discovery/court/DA, economic and independent checkpoint Final gates remain unapproved",
            )),
            _ => Ok(()),
        }
    }
}

impl PalwFundedExecutionFenceV11 {
    /// Fixed mechanics fixture only. These are illustrative capacity/money
    /// constants, not measured prices, approved economics or a network preset.
    pub fn unapproved_probe_at(activation: ForkActivation, network_domain: [u8; 64]) -> Self {
        use misaka_palw_kernel::flat_execution11::{ExecutionHorizonsV11, ExecutionPolicyV11};
        use misaka_palw_kernel::flat_quota11::discovery13::DiscoveryPolicyV13;
        use misaka_palw_kernel::flat_quota11::{QuotaPolicyV11, QuotaQuantitiesV11, quota_ruleset_id_v11};
        let cap = QuotaQuantitiesV11([1 << 56; 21]);
        let quota = QuotaPolicyV11 {
            version: 11,
            enabled: true,
            network_domain,
            ruleset_digest: quota_ruleset_id_v11(),
            interval_daa: 1000,
            plan: cap.clone(),
            block: cap.clone(),
            actor: cap.clone(),
            network: cap,
            max_subjects: 128,
            max_actors: 128,
            future_attempts: [2, 2, 48, 2, 2, 1],
        };
        let discovery = DiscoveryPolicyV13 {
            version: 13,
            quota_policy_root: quota.root(),
            requests_per_session: 2,
            ordinary_lifetime_requests: 32,
        };
        let execution = ExecutionPolicyV11 {
            version: 11,
            enabled: true,
            network_domain,
            quota_policy_root: quota.root(),
            horizons: ExecutionHorizonsV11 {
                commit_window: 5,
                challenge_window: 4,
                seal_maturity: 3,
                seal_ttl: 100,
                da_answer: 3,
                proof_grace: 3,
                retention: 2,
            },
            reward_cap: 100,
            external_gain_cap: 20,
            default_obligation: 10,
            required_producer_reserve: 130,
            finder_permille: 490,
        };
        Self { activation, policies: NativeExecutionPoliciesV11 { execution, quota, discovery } }
    }
}
