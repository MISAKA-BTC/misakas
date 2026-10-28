//! Independent consumer fence for the original stage28 source/court/Final book.
pub use crate::palw_flat_stage_v28::PalwStageExecutionFenceV28;
use crate::{
    config::params::{ForkActivation, Params},
    palw_mode_v2::PalwModeV2Error,
};

impl PalwStageExecutionFenceV28 {
    pub fn unapproved_probe_at(activation: ForkActivation, network_domain: [u8; 64]) -> Self {
        let old = crate::palw_funded_execution_v11::PalwFundedExecutionFenceV11::unapproved_probe_at(activation, network_domain);
        Self {
            activation,
            policies: crate::palw_flat_stage_v28::NativeStagePoliciesV28 {
                version: 28,
                network_domain,
                execution: old.policies,
                final_policy: [0; 64],
                physical_service: [0; 64],
                source_protocol: crate::palw_flat_stage_v28::stage_source_protocol_root_v28(),
                checker: 25,
                maximum_horizon: 900,
                catalog_cap: 8,
                max_input_bytes: 1 << 20,
            },
        }
    }
}
impl Params {
    pub fn palw_stage_execution_active_at_v28(&self, daa: u64) -> bool {
        self.palw_stage_execution_v28.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_native_final_active_at_v27(daa)
    }
    pub fn validate_palw_stage_execution_v28(&self) -> Result<(), PalwModeV2Error> {
        if self.has_isolated_native_calibration_v34() {
            return self.validate_isolated_native_calibration_v34().map_err(|_| {
                PalwModeV2Error::Invalid("native calibration34 owning isolated genesis or complete original policies are invalid")
            });
        }
        match &self.palw_stage_execution_v28 {
            Some(f) if f.activation != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_stage_execution_v28 cannot be armed before independent source26/court25, full bounded lengths, native Final/redemption and A6/A8 completion evidence",
            )),
            _ => Ok(()),
        }
    }
}
