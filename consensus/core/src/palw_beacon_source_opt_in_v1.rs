//! Dedicated fence for the source reservation foundation (inner kernel tag 21).
//! None on all presets. Target adapters, source-only beacon policy and funded fee terms are not complete.
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;

impl Params {
    pub fn palw_beacon_source_opt_in_activation(&self) -> Option<u64> {
        self.palw_beacon_source_opt_in_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    pub fn validate_palw_beacon_source_opt_in_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_beacon_source_opt_in_activation() {
            Some(_) => Err(PalwModeV2Error::Invalid(
                "palw_beacon_source_opt_in_v1 cannot be armed: authenticated targets, source-only beacon and funded fees are incomplete",
            )),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};

    #[test]
    fn source_opt_in_has_its_own_dormant_fence_and_none_is_fingerprint_neutral() {
        let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
        for original in [MAINNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, SIMNET_PARAMS, t12] {
            assert!(original.palw_beacon_source_opt_in_v1.is_none());
            original.validate_palw_beacon_source_opt_in_v1().unwrap();
            let mut p = original.clone();
            let before = (p.consensus_params_id(), p.consensus_schedule_id());
            p.palw_beacon_source_opt_in_v1 = Some(ForkActivation::new(1));
            assert!(p.validate_palw_beacon_source_opt_in_v1().is_err());
            assert_ne!((p.consensus_params_id(), p.consensus_schedule_id()), before);
            p.palw_beacon_source_opt_in_v1 = Some(ForkActivation::never());
            p.validate_palw_beacon_source_opt_in_v1().unwrap();
            assert!(p.palw_beacon_source_opt_in_activation().is_none());
            assert_eq!(p.consensus_identity_id(), original.consensus_identity_id());
        }
    }
}
