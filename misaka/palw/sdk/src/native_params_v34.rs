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
    /// A shipped preset as the owning ruleset (no snapshot). Only such a network
    /// can ever carry a qualified Final; today every preset leaves Final27 absent.
    pub fn shipped_preset(parameters: Params) -> Self {
        Self { parameters, snapshot: [0; 64], sha256: [0; 32] }
    }
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

/// Parse the validation phase's measured calibration records (JSON array). Every
/// digest is exact 128-hex; every number a u64; every readiness flag explicit.
/// Values are measurements to be bound into a LOCAL snapshot, never defaults.
pub fn parse_calibrations_json_v34(
    raw: &[u8],
) -> Result<Vec<kaspa_consensus_core::palw_native_final_v27::NativeFinalCalibrationV27>, String> {
    let doc: serde_json::Value = serde_json::from_slice(raw).map_err(|e| e.to_string())?;
    let items = doc.as_array().ok_or("calibrations JSON array required")?;
    if items.is_empty() || items.len() > 128 {
        return Err("1..=128 calibration records required".into());
    }
    let digest = |v: &serde_json::Value, k: &str| -> Result<Digest, String> {
        let h: kaspa_hashes::Hash64 = v
            .get(k)
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("{k} 128-hex required"))?
            .parse()
            .map_err(|e| format!("{k}: {e}"))?;
        Ok(h.as_bytes())
    };
    let num = |v: &serde_json::Value, k: &str| -> Result<u64, String> {
        v.get(k).and_then(|x| x.as_u64()).ok_or_else(|| format!("{k} u64 required"))
    };
    let flag = |v: &serde_json::Value, k: &str| -> Result<bool, String> {
        v.get(k).and_then(|x| x.as_bool()).ok_or_else(|| format!("{k} bool required"))
    };
    items
        .iter()
        .map(|v| {
            Ok(kaspa_consensus_core::palw_native_final_v27::NativeFinalCalibrationV27 {
                profile: digest(v, "profile")?,
                maximum_reference_bytes: num(v, "maximum_reference_bytes")?,
                maximum_court_bytes: num(v, "maximum_court_bytes")?,
                maximum_court_work: num(v, "maximum_court_work")?,
                maximum_court_scratch: num(v, "maximum_court_scratch")?,
                whole_claim_detection_numerator: num(v, "whole_claim_detection_numerator")?,
                whole_claim_detection_denominator: num(v, "whole_claim_detection_denominator")?,
                measured_cold_reference_daa: num(v, "measured_cold_reference_daa")?,
                measured_sequential_discovery_daa: num(v, "measured_sequential_discovery_daa")?,
                measured_prove_before_seal_daa: num(v, "measured_prove_before_seal_daa")?,
                measured_proof_delivery_daa: num(v, "measured_proof_delivery_daa")?,
                restart_reorg_margin_daa: num(v, "restart_reorg_margin_daa")?,
                measurement_root: digest(v, "measurement_root")?,
                source_availability_root: digest(v, "source_availability_root")?,
                whole_claim_detection_root: digest(v, "whole_claim_detection_root")?,
                real_delivery_service_root: digest(v, "real_delivery_service_root")?,
                independent_native_final_root: digest(v, "independent_native_final_root")?,
                original_source_ready: flag(v, "original_source_ready")?,
                whole_claim_detector_ready: flag(v, "whole_claim_detector_ready")?,
                actual_delivery_ready: flag(v, "actual_delivery_ready")?,
                independent_node_ready: flag(v, "independent_node_ready")?,
            })
        })
        .collect()
}
