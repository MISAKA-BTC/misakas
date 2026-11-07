//! Explicit owning39 wrapper around an UNCHANGED canonical calibration34
//! snapshot. This separate version/root never extends the frozen34 wire and
//! cannot activate a shipping preset or fabricate measured qualification.
use crate::{
    config::params::{ForkActivation, Params},
    palw_native_calibration_v34::{
        NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34, NativeCalibrationSnapshotV34, load_isolated_native_calibration_v34,
    },
    palw_native_execution_v39::{NativeExecutionPoliciesV39, PalwNativeExecutionFenceV39, source_protocol_root_v39},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};
pub const NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V39: usize = NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34 + (32 << 10);
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeCalibrationSnapshotV39 {
    pub version: u16,
    pub original_snapshot: Vec<u8>,
    pub original_root: Digest,
    pub owning: NativeExecutionPoliciesV39,
}
impl NativeCalibrationSnapshotV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/owning-calibration-snapshot/v39", self)
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() < 6 || raw.len() > NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V39 || raw[..2] != 39u16.to_le_bytes() {
            return Err("native39 snapshot exact version/byte cap".into());
        }
        let n = u32::from_le_bytes(raw[2..6].try_into().unwrap()) as usize;
        if n > NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34 || n > raw.len().saturating_sub(6 + 64) {
            return Err("native39 original snapshot length prefix cap".into());
        }
        let original_snapshot = raw[6..6 + n].to_vec();
        NativeCalibrationSnapshotV34::from_bytes(&original_snapshot)?;
        let original_root = raw[6 + n..6 + n + 64].try_into().unwrap();
        // This owning policy consists exclusively of fixed-size fields and
        // arrays; no caller-controlled collection prefix is decoded here.
        let owning = borsh::from_slice(&raw[6 + n + 64..]).map_err(|e| format!("native39 canonical fixed owning policy: {e}"))?;
        let out = Self { version: 39, original_snapshot, original_root, owning };
        if borsh::to_vec(&out).map_err(|e| e.to_string())? != raw {
            return Err("native39 snapshot noncanonical/trailing bytes".into());
        }
        Ok(out)
    }
    pub fn apply(&self, base: &Params, pin: Digest) -> Result<Params, String> {
        if self.version != 39 || self.root() != pin {
            return Err("native39 owning snapshot explicit pin differs".into());
        }
        let mut out = load_isolated_native_calibration_v34(base, &self.original_snapshot, self.original_root)?;
        let funded = out.palw_funded_execution_v11.as_ref().ok_or("native39 snapshot original funded policy absent")?;
        let final27 = out.palw_native_final_v27.as_ref().ok_or("native39 snapshot original Final policy absent")?;
        let mass = crate::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&out);
        let service = crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(
            &mass,
            &final27.policy.queue_policy,
        )?;
        let domain =
            crate::palw_attempt_v2::palw_network_domain_v2_for(out.net.to_string().as_bytes(), Some(out.genesis.hash)).as_bytes();
        self.owning.check(domain)?;
        if self.owning.execution != funded.policies
            || self.owning.final_policy != final27.policy.root()
            || self.owning.physical_service != service.root()
        {
            return Err("native39 snapshot changed original shared execution/Final/physical terms".into());
        }
        out.palw_native_execution_v39 =
            Some(PalwNativeExecutionFenceV39 { activation: ForkActivation::always(), policies: self.owning.clone() });
        out.validate_palw_v2().map_err(|e| format!("native39 isolated owning ruleset invalid: {e:?}"))?;
        Ok(out)
    }
    /// Wrap a predeclared original34 profile with an explicitly chosen bounded
    /// horizon/catalog. No readiness or measurement values are invented.
    pub fn wrap(
        base: &Params,
        original_snapshot: Vec<u8>,
        original_root: Digest,
        maximum_horizon: u64,
        catalog_cap: u8,
    ) -> Result<Self, String> {
        let original = load_isolated_native_calibration_v34(base, &original_snapshot, original_root)?;
        let final27 = original.palw_native_final_v27.as_ref().ok_or("native39 original Final policy absent")?;
        let mass = crate::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&original);
        let service = crate::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(
            &mass,
            &final27.policy.queue_policy,
        )?;
        let owning = NativeExecutionPoliciesV39 {
            version: 39,
            network_domain: final27.policy.network_domain,
            execution: original.palw_funded_execution_v11.as_ref().ok_or("native39 original funded policy absent")?.policies.clone(),
            final_policy: final27.policy.root(),
            physical_service: service.root(),
            source_protocol39: source_protocol_root_v39(),
            maximum_horizon,
            catalog_cap,
        };
        let out = Self { version: 39, original_snapshot, original_root, owning };
        out.apply(base, out.root())?;
        Ok(out)
    }
}
pub fn load_isolated_native_calibration_v39(base: &Params, raw: &[u8], pin: Digest) -> Result<Params, String> {
    NativeCalibrationSnapshotV39::from_bytes(raw)?.apply(base, pin)
}
