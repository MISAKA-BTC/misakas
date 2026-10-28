//! Canonical SDK preparation for the explicitly selected full25/first-EOS29 route.
//! This is the SAME native derivation as admission, not a parallel tariff guess.
//! Signatures, source availability, activation and chain receipts remain native checks.
use kaspa_consensus_core::{
    palw_native_final_v27::{CheckedNativeFinalConditionsV27, NativeFinalClassBasisV27},
    palw_native_text_stop_v29::{CheckedNativeTextMaximumV29, NativeTextMaximumV29, NativeTextRequestV29},
};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::TirProgramV1;

pub struct PreparedNativeTextV29 {
    request: NativeTextRequestV29,
    prompt: Vec<u32>,
    maximum: CheckedNativeTextMaximumV29,
}
impl PreparedNativeTextV29 {
    pub fn request(&self) -> &NativeTextRequestV29 {
        &self.request
    }
    pub fn prompt(&self) -> &[u32] {
        &self.prompt
    }
    pub fn maximum(&self) -> &CheckedNativeTextMaximumV29 {
        &self.maximum
    }
    /// Maximum reservation bytes remain unchanged when an actual EOS occurs early.
    pub fn maximum_contract_bytes(&self) -> Result<Vec<u8>, String> {
        borsh::to_vec(self.maximum.record()).map_err(|e| e.to_string())
    }
    pub fn request_bytes(&self) -> Result<Vec<u8>, String> {
        borsh::to_vec(&self.request).map_err(|e| e.to_string())
    }
}
pub fn prepare_native_text_v29(
    program: &TirProgramV1,
    parameters: &ParamCommitmentsV1,
    request: NativeTextRequestV29,
    prompt: &[u32],
    maximum: NativeTextMaximumV29,
    basis: &NativeFinalClassBasisV27,
    final_conditions: &CheckedNativeFinalConditionsV27,
) -> Result<PreparedNativeTextV29, String> {
    // Check ORIGINAL maxima and prompt before cloning caller-sized dynamic data.
    request.check_prompt(program, prompt)?;
    let maximum = CheckedNativeTextMaximumV29::derive(program, parameters, &request, prompt, maximum, basis, final_conditions)?;
    Ok(PreparedNativeTextV29 { request, prompt: prompt.to_vec(), maximum })
}
