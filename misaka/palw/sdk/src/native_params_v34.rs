//! The SAME isolated owning snapshot factory as the daemon. Every input is
//! predeclared/hash-pinned; calibration receipts cannot become A6/A8 credit.
use kaspa_consensus_core::{
    config::params::Params,
    palw_native_calibration_v34::{
        NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34, check_same_owning_ruleset_v34, load_isolated_native_calibration_v34,
    },
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
    let parameters = load_isolated_native_calibration_v34(base, &raw, pinned_root)?;
    Ok(OwnedNativeParametersV34 { parameters, snapshot: pinned_root, sha256 })
}
impl OwnedNativeParametersV34 {
    /// Refuse a node whose announced identity (`getPalwNodeStatus`: handshake fingerprint, fence
    /// schedule id, genesis) differs from this auditor's own reconstruction — another network,
    /// another snapshot, or one changed policy value. Raw node flags are never trusted instead.
    pub fn check_node_identity(&self, params_id: &str, schedule_id: &str, genesis: &str) -> Result<(), String> {
        check_same_owning_ruleset_v34(&self.parameters, params_id, schedule_id, genesis)
    }
}
/// Parse an exact 64-hex SHA256 pin without slicing past a short/odd input.
pub fn parse_sha256_hex_v34(raw: &str) -> Result<[u8; 32], String> {
    if raw.len() != 64 || !raw.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err("own profile exact SHA256 (64 hex) required".into());
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&raw[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
