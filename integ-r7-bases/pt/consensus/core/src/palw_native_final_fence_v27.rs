//! Consumer plumbing for the separately versioned native Final contract.
//! The public verifier/resource profile is part of network identity; an
//! individual model or compiler is never an authorization registry.
use crate::{
    config::params::{ForkActivation, Params},
    palw_mode_v2::PalwModeV2Error,
};

impl crate::palw_native_final_v27::PalwNativeFinalFenceV27 {
    /// Identity/fence fixture only; an empty calibration grants no Final.
    pub fn unapproved_probe_at(activation: ForkActivation, network_domain: [u8; 64]) -> Self {
        Self {
            activation,
            policy: crate::palw_native_final_v27::NativeFinalPolicyV27 {
                version: 27,
                network_domain,
                purpose: crate::palw_native_final_v27::NativeFinalPurposeV34::Qualified,
                queue_policy: crate::palw_native_queue_v30::NativeQueuePolicyV30::probe(network_domain),
                finder_profit_margin_sompi: 1,
                producer_deterrence_margin_sompi: 1,
                calibrations: Vec::new(),
            },
        }
    }
}

impl Params {
    pub fn palw_native_final_active_at_v27(&self, daa: u64) -> bool {
        self.palw_native_final_v27.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
            && self.palw_probabilistic_constraints_active_at(daa)
            && self.palw_panel_free_v1.as_ref().is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
    }

    pub fn validate_palw_native_final_v27(&self) -> Result<(), PalwModeV2Error> {
        if self.has_isolated_native_calibration_v34() {
            return self.validate_isolated_native_calibration_v34().map_err(|_| {
                PalwModeV2Error::Invalid("native calibration34 owning isolated genesis or complete original policies are invalid")
            });
        }
        match &self.palw_native_final_v27 {
            Some(f) if f.activation != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_native_final_v27 cannot be armed before independent full-context Final/redemption, whole-claim detection, measured delivery/economics and A6/A8 completion evidence",
            )),
            _ => Ok(()),
        }
    }
}
