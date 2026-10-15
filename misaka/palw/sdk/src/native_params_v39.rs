//! Same owning39 reconstruction as the node. All original34 bytes and new39
//! policy terms are separately versioned, root-pinned and SHA256-pinned.
use kaspa_consensus_core::{
    config::params::Params,
    palw_native_calibration_v39::{NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V39, load_isolated_native_calibration_v39},
};
use misaka_palw_kernel::hash::Digest;
use sha2::{Digest as _, Sha256};
pub struct OwnedNativeParametersV39 {
    parameters: Params,
    snapshot: Digest,
    sha256: [u8; 32],
}
impl OwnedNativeParametersV39 {
    pub fn parameters(&self) -> &Params {
        &self.parameters
    }
    pub fn into_parameters(self) -> Params {
        self.parameters
    }
    pub fn snapshot_root(&self) -> Digest {
        self.snapshot
    }
    pub fn snapshot_sha256(&self) -> [u8; 32] {
        self.sha256
    }
    pub fn shipped_preset(parameters: Params) -> Self {
        Self { parameters, snapshot: [0; 64], sha256: [0; 32] }
    }
    pub fn check_node_identity(&self, params: &str, schedule: &str, genesis: &str) -> Result<(), String> {
        kaspa_consensus_core::palw_native_calibration_v34::check_same_owning_ruleset_v34(&self.parameters, params, schedule, genesis)
    }
}
pub fn load_own_native_parameters_v39(
    base: &Params,
    path: &std::path::Path,
    pinned_root: Digest,
    pinned_sha256: [u8; 32],
) -> Result<OwnedNativeParametersV39, String> {
    let raw = crate::native_audit_io_v32::read_bounded_file_v32(path, NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V39)?;
    let sha256: [u8; 32] = Sha256::digest(&raw).into();
    if sha256 != pinned_sha256 {
        return Err("own native39 profile differs from predeclared whole-file SHA256".into());
    }
    let parameters = load_isolated_native_calibration_v39(base, &raw, pinned_root)?;
    Ok(OwnedNativeParametersV39 { parameters, snapshot: pinned_root, sha256 })
}
