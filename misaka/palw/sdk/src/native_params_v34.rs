//! The SAME isolated owning snapshot factory as the daemon. Every input is
//! predeclared/hash-pinned; calibration receipts cannot become A6/A8 credit.
use kaspa_consensus_core::{
    config::params::Params,
    palw_native_calibration_v34::{NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34, NativeCalibrationSnapshotV34},
};
use misaka_palw_kernel::hash::Digest;
use sha2::{Digest as _, Sha256};
pub struct OwnedNativeParametersV34 {
    parameters: Params,
    snapshot: Digest,
    sha256: [u8; 32],
}
impl OwnedNativeParametersV34 {
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
    pub fn rfc02_qualified(&self) -> bool {
        self.parameters.native_final_is_qualified_v34()
    }
}
pub fn load_own_native_parameters_v34(
    base: &Params,
    path: &std::path::Path,
    pinned_root: Digest,
    pinned_sha256: [u8; 32],
) -> Result<OwnedNativeParametersV34, String> {
    let raw = crate::native_audit_io_v32::read_bounded_file_v32(path, NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34)?;
    let sha256: [u8; 32] = Sha256::digest(&raw).into();
    if sha256 != pinned_sha256 {
        return Err("own native34 profile differs from predeclared whole-file SHA256".into());
    }
    let snapshot = NativeCalibrationSnapshotV34::from_bytes(&raw)?;
    let parameters = snapshot.apply(base, pinned_root)?;
    if parameters.native_final_is_qualified_v34() || !parameters.has_isolated_native_calibration_v34() {
        return Err("isolated owning profile cannot grant qualified RFC02 receipts".into());
    }
    Ok(OwnedNativeParametersV34 { parameters, snapshot: pinned_root, sha256 })
}
