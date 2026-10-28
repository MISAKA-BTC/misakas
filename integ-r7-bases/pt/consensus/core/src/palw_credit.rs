//! Reserved parameter encoding for the retired PALW V1 credit overlay.
//!
//! ADR-0037 withdrew this overlay. `Params::validate_palw_v1` rejects `Some`; only
//! the existing Borsh layout and fingerprint input remain for compatibility.

use crate::palw_registry::PalwClassRegistrationV1;

#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCreditParamsV1 {
    pub registration: PalwClassRegistrationV1,
    pub s_eff_sompi: u64,
    pub unbonding_period_blocks: u64,
    pub activation_daa: u64,
    pub class_daa: crate::palw_class_daa::PalwClassDaaParamsV1,
}
