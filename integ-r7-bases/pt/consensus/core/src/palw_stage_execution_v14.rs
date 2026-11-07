//! Separate dormant typed stage14 policy fence; text11 cannot authorize it.
use crate::config::params::{ForkActivation, Params};
use crate::palw_flat_stage_v14::{NativeStagePoliciesV14, PalwStageExecutionFenceV14};
use crate::palw_funded_execution_v11::PalwFundedExecutionFenceV11;
use crate::palw_mode_v2::PalwModeV2Error;
impl Params {
    pub fn palw_stage_execution_active_at_v14(&self, daa: u64) -> bool {
        self.palw_stage_execution_v14.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_probabilistic_constraints_active_at(daa)
            && self.palw_panel_free_v1.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
    }
    pub fn validate_palw_stage_execution_v14(&self) -> Result<(), PalwModeV2Error> {
        match &self.palw_stage_execution_v14 {
            Some(f) if f.activation != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_stage_execution_v14 cannot be armed: typed source/court/DA, funded tensor Final and checkpoint/task gates remain unapproved",
            )),
            _ => Ok(()),
        }
    }
}
impl PalwStageExecutionFenceV14 {
    /// Unapproved fixed mechanics fixture, not a measured economic policy.
    pub fn unapproved_probe_at(activation: ForkActivation, network_domain: [u8; 64]) -> Self {
        Self {
            activation,
            policies: NativeStagePoliciesV14 {
                version: 14,
                execution: PalwFundedExecutionFenceV11::unapproved_probe_at(activation, network_domain).policies,
                source_reservation: 130,
                source_maturity: 3,
                source_liability: 1000,
                catalog_cap: 8,
            },
        }
    }
}
