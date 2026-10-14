//! Explicit isolated devnet calibration. This constructor does not invent
//! measurements or turn a receipt into production/A6/A8 qualification. Node
//! and independent auditor use the same pinned snapshot and owning base.
use crate::{
    Hash64,
    config::{
        genesis::GenesisBlock,
        params::{ForkActivation, Params},
    },
    network::NetworkType,
    palw_flat_execution_v11::NativeExecutionPoliciesV11,
    palw_flat_stage_v28::NativeStagePoliciesV28,
    palw_native_final_v27::{NativeFinalPolicyV27, NativeFinalPurposeV34},
    palw_panel_free_v1::PalwPanelFreeFenceV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    hash::{Digest, object_id},
    opv::{CarrierCapsV1, OpvBudgetsV1, OpvEconomicsV1, OpvWindowV1},
};
const GENESIS_MARKER: &[u8] = b"misaka-palw/isolated-native-calibration-genesis/v34";
pub const NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34: usize = 256 << 10;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativePanelFreeSettingsV34 {
    pub denied_classes: Vec<Hash64>,
    pub min_effective_bits: u16,
    pub window: OpvWindowV1,
    pub budgets: OpvBudgetsV1,
    pub economics: OpvEconomicsV1,
    pub carrier: CarrierCapsV1,
}
impl NativePanelFreeSettingsV34 {
    pub fn fence(&self) -> Result<PalwPanelFreeFenceV1, String> {
        if self.denied_classes.len() > 128 || self.min_effective_bits < crate::palw_panel_free_v1::PALW_OPV_MIN_EFFECTIVE_BITS_V1 {
            return Err("native calibration34 bounded OPV deny list".into());
        }
        let f = PalwPanelFreeFenceV1 {
            activation: ForkActivation::always(),
            denied_classes: self.denied_classes.clone(),
            min_effective_bits: self.min_effective_bits,
            sampled_conformance_gates_reward: false,
            window: self.window,
            budgets: self.budgets,
            economics: self.economics,
            carrier: self.carrier,
        };
        f.validate_value().map_err(|e| format!("native calibration34 OPV terms: {e:?}"))?;
        Ok(f)
    }
}
/// The immutable base fingerprint includes every ordinary owning network
/// parameter. The snapshot contains every new policy value; overrides not
/// represented here cannot be silently applied by the reconstruction API.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeCalibrationSnapshotV34 {
    pub version: u16,
    pub isolation: Digest,
    pub base_params: kaspa_hashes::Hash,
    pub base_schedule: kaspa_hashes::Hash,
    pub funded: NativeExecutionPoliciesV11,
    pub stage: Option<NativeStagePoliciesV28>,
    pub final_policy: NativeFinalPolicyV27,
    pub panel: NativePanelFreeSettingsV34,
}
impl NativeCalibrationSnapshotV34 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/owning-calibration-snapshot/v34", self)
    }
    /// All dynamic vector prefixes are bounded BEFORE reserving their storage.
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        use std::io::Cursor;
        fn read<T: BorshDeserialize>(r: &mut Cursor<&[u8]>) -> Result<T, String> {
            T::deserialize_reader(r).map_err(|_| "native calibration34 truncated canonical fixed field".into())
        }
        if raw.len() > NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34 {
            return Err("native calibration34 snapshot byte cap".into());
        }
        let mut r = Cursor::new(raw);
        let version = read(&mut r)?;
        let isolation = read(&mut r)?;
        let base_params = read(&mut r)?;
        let base_schedule = read(&mut r)?;
        let funded = read(&mut r)?;
        let stage = match read::<u8>(&mut r)? {
            0 => None,
            1 => Some(read(&mut r)?),
            _ => return Err("native calibration34 stage option".into()),
        };
        let fv = read(&mut r)?;
        let network_domain = read(&mut r)?;
        let purpose = read(&mut r)?; // fixed-size enum; neither variant contains vectors
        let queue_policy = read(&mut r)?;
        let finder_profit_margin_sompi = read(&mut r)?;
        let producer_deterrence_margin_sompi = read(&mut r)?;
        let n = read::<u32>(&mut r)? as usize;
        if n == 0 || n > 128 || n > raw.len().saturating_sub(r.position() as usize) / 64 {
            return Err("native calibration34 bounded actual calibration count".into());
        }
        let mut calibrations = Vec::with_capacity(n);
        for _ in 0..n {
            calibrations.push(read(&mut r)?);
        }
        let final_policy = NativeFinalPolicyV27 {
            version: fv,
            network_domain,
            purpose,
            queue_policy,
            finder_profit_margin_sompi,
            producer_deterrence_margin_sompi,
            calibrations,
        };
        let n = read::<u32>(&mut r)? as usize;
        if n > 128 || n > raw.len().saturating_sub(r.position() as usize) / 64 {
            return Err("native calibration34 bounded deny count".into());
        }
        let mut denied_classes = Vec::with_capacity(n);
        for _ in 0..n {
            denied_classes.push(read(&mut r)?);
        }
        let panel = NativePanelFreeSettingsV34 {
            denied_classes,
            min_effective_bits: read(&mut r)?,
            window: read(&mut r)?,
            budgets: read(&mut r)?,
            economics: read(&mut r)?,
            carrier: read(&mut r)?,
        };
        let out = Self { version, isolation, base_params, base_schedule, funded, stage, final_policy, panel };
        if r.position() as usize != raw.len() || borsh::to_vec(&out).map_err(|e| e.to_string())? != raw {
            return Err("native calibration34 noncanonical/trailing snapshot bytes".into());
        }
        Ok(out)
    }
    pub fn apply(&self, base: &Params, pinned_snapshot: Digest) -> Result<Params, String> {
        if self.version != 34
            || self.isolation == [0; 64]
            || self.root() != pinned_snapshot
            || base.net.network_type != NetworkType::Devnet
            || base.consensus_params_id() != self.base_params
            || base.consensus_schedule_id() != self.base_schedule
            || base.palw_native_final_v27.is_some()
            || base.palw_funded_execution_v11.is_some()
            || base.palw_stage_execution_v14.is_some()
            || base.palw_stage_execution_v28.is_some()
            || base.palw_probabilistic_constraints_v1.is_some()
            || base.palw_panel_free_v1.is_some()
        {
            return Err("native calibration34 owning base/snapshot pin differs or base already overrides native rules".into());
        }
        let mut out = base.clone();
        out.genesis = isolated_native_genesis_v34(&base.genesis, self.isolation)?;
        out.palw_probabilistic_constraints_v1 = Some(ForkActivation::always());
        out.palw_panel_free_v1 = Some(self.panel.fence()?);
        out.palw_funded_execution_v11 = Some(crate::palw_funded_execution_v11::PalwFundedExecutionFenceV11 {
            activation: ForkActivation::always(),
            policies: self.funded.clone(),
        });
        out.palw_native_final_v27 = Some(crate::palw_native_final_v27::PalwNativeFinalFenceV27 {
            activation: ForkActivation::always(),
            policy: self.final_policy.clone(),
        });
        out.palw_stage_execution_v28 = self.stage.as_ref().map(|policies| crate::palw_flat_stage_v28::PalwStageExecutionFenceV28 {
            activation: ForkActivation::always(),
            policies: policies.clone(),
        });
        out.validate_isolated_native_calibration_v34()?;
        out.validate_palw_v2().map_err(|e| format!("native calibration34 ordinary owning ruleset invalid: {e:?}"))?;
        Ok(out)
    }
}
/// **The one owning factory** shared by the daemon (`--palw-native-calibration-snapshot`) and
/// every auditor/signer (`native_params_v34`). Canonical bounded decode, the explicit root pin,
/// the pinned owning devnet base, and the measurement-only purpose are checked here once, so the
/// node and the audit CLI cannot diverge on what a snapshot means. A qualified purpose is refused:
/// an isolated calibration receipt is a measurement Final, never A6/A8 pass evidence.
pub fn load_isolated_native_calibration_v34(base: &Params, raw: &[u8], pinned_root: Digest) -> Result<Params, String> {
    let snapshot = NativeCalibrationSnapshotV34::from_bytes(raw)?;
    let out = snapshot.apply(base, pinned_root)?;
    if out.native_final_is_qualified_v34() || !out.has_isolated_native_calibration_v34() {
        return Err("isolated owning profile cannot grant qualified RFC02 receipts".into());
    }
    Ok(out)
}
/// Uniform mixing refusal: a node's announced identity (its handshake fingerprint, fence schedule
/// id and genesis, as `getPalwNodeStatus` reports them) must equal the auditor's own reconstruction.
/// A different network, genesis, snapshot or any single owning policy value differs in at least one.
pub fn check_same_owning_ruleset_v34(
    own: &Params,
    reported_params_id: &str,
    reported_schedule_id: &str,
    reported_genesis: &str,
) -> Result<(), String> {
    if own.consensus_params_id().to_string() != reported_params_id
        || own.consensus_schedule_id().to_string() != reported_schedule_id
        || own.genesis.hash.to_string() != reported_genesis
    {
        return Err(format!(
            "node runs another owning ruleset: params {reported_params_id} schedule {reported_schedule_id} genesis {reported_genesis}, \
             own {} {} {}",
            own.consensus_params_id(),
            own.consensus_schedule_id(),
            own.genesis.hash
        ));
    }
    Ok(())
}
fn payload(base: &GenesisBlock, isolation: Digest) -> Result<Vec<u8>, String> {
    if isolation == [0; 64] || base.coinbase_payload.len() > 4096 {
        return Err("native calibration34 isolation/base payload cap".into());
    }
    let mut raw = base.coinbase_payload.to_vec();
    raw.extend_from_slice(GENESIS_MARKER);
    raw.extend_from_slice(&isolation);
    Ok(raw)
}
/// Public deterministic genesis derivation so the pre-measurement manifest can
/// bind its eventual network domain before any class/policy is assembled.
pub fn isolated_native_genesis_v34(base: &GenesisBlock, isolation: Digest) -> Result<GenesisBlock, String> {
    let raw = payload(base, isolation)?;
    let mut g = base.clone();
    // GenesisBlock owns a static payload by design. One bounded immutable
    // allocation per configuration construction; no protocol input calls this.
    g.coinbase_payload = Box::leak(raw.into_boxed_slice());
    g.hash_merkle_root = crate::merkle::calc_hash_merkle_root(g.build_genesis_transactions().iter());
    g.hash = crate::header::Header::from(&g).hash;
    Ok(g)
}
impl Params {
    pub fn native_final_is_qualified_v34(&self) -> bool {
        self.palw_native_final_v27.as_ref().is_some_and(|f| f.policy.purpose.is_qualified())
    }
    pub fn has_isolated_native_calibration_v34(&self) -> bool {
        self.palw_native_final_v27
            .as_ref()
            .is_some_and(|f| matches!(f.policy.purpose, NativeFinalPurposeV34::IsolatedCalibration { .. }))
    }
    pub fn validate_isolated_native_calibration_v34(&self) -> Result<(), String> {
        let f = self.palw_native_final_v27.as_ref().ok_or("native calibration34 Final policy absent")?;
        let NativeFinalPurposeV34::IsolatedCalibration { version, isolation, base_genesis, measurement_manifest } = &f.policy.purpose
        else {
            return Err("native calibration34 dedicated purpose absent".into());
        };
        let base = crate::config::params::devnet_shipped_params();
        let mut original = self.clone();
        original.genesis = base.genesis.clone();
        original.palw_native_final_v27 = base.palw_native_final_v27.clone();
        original.palw_funded_execution_v11 = base.palw_funded_execution_v11.clone();
        original.palw_stage_execution_v28 = base.palw_stage_execution_v28.clone();
        original.palw_panel_free_v1 = base.palw_panel_free_v1.clone();
        original.palw_probabilistic_constraints_v1 = base.palw_probabilistic_constraints_v1;
        if *version != 34
            || *isolation == [0; 64]
            || *measurement_manifest == [0; 64]
            || self.net != base.net
            || original.consensus_params_id() != base.consensus_params_id()
            || original.consensus_schedule_id() != base.consensus_schedule_id()
            || *base_genesis != base.genesis.hash
            || self.genesis.hash == base.genesis.hash
            || self.genesis.coinbase_payload != payload(&base.genesis, *isolation)?
            || self.genesis.utxo_commitment != base.genesis.utxo_commitment
            || crate::header::Header::from(&self.genesis).hash != self.genesis.hash
            || self.genesis.hash_merkle_root != crate::merkle::calc_hash_merkle_root(self.genesis.build_genesis_transactions().iter())
            || f.activation != ForkActivation::always()
            || self.palw_probabilistic_constraints_v1 != Some(ForkActivation::always())
            || self.palw_stage_execution_v14.is_some()
        {
            return Err("native calibration34 must be the distinct pinned devnet genesis and dedicated new-contract purpose".into());
        }
        let domain =
            crate::palw_attempt_v2::palw_network_domain_v2_for(self.net.to_string().as_bytes(), Some(self.genesis.hash)).as_bytes();
        f.policy.check(domain)?;
        let funded = self.palw_funded_execution_v11.as_ref().ok_or("native calibration34 funded policy absent")?;
        if funded.activation != ForkActivation::always() {
            return Err("native calibration34 original funded fence differs".into());
        }
        funded.policies.check(domain)?;
        let panel = self.palw_panel_free_v1.as_ref().ok_or("native calibration34 public verifier policy absent")?;
        if panel.activation != ForkActivation::always()
            || panel.denied_classes.len() > 128
            || panel.min_effective_bits < crate::palw_panel_free_v1::PALW_OPV_MIN_EFFECTIVE_BITS_V1
        {
            return Err("native calibration34 original public verifier fence differs".into());
        }
        panel.validate_value().map_err(|e| format!("native calibration34 public verifier terms: {e:?}"))?;
        if let Some(stage) = &self.palw_stage_execution_v28 {
            if stage.activation != ForkActivation::always()
                || stage.policies.final_policy != f.policy.root()
                || stage.policies.execution != funded.policies
            {
                return Err("native calibration34 stage/funded/Final policies differ".into());
            }
            stage.policies.check(domain)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_native_final_v27::NativeFinalCalibrationV27;
    /// Construction/identity probe only. Every source/delivery/detection flag
    /// is false; these records cannot pass check_class or issue any Final.
    fn snapshot(base: &Params) -> NativeCalibrationSnapshotV34 {
        let isolation = [7; 64];
        let genesis = isolated_native_genesis_v34(&base.genesis, isolation).unwrap();
        let domain =
            crate::palw_attempt_v2::palw_network_domain_v2_for(base.net.to_string().as_bytes(), Some(genesis.hash)).as_bytes();
        let funded =
            crate::palw_funded_execution_v11::PalwFundedExecutionFenceV11::unapproved_probe_at(ForkActivation::always(), domain)
                .policies;
        let mut final_policy =
            crate::palw_native_final_v27::PalwNativeFinalFenceV27::unapproved_probe_at(ForkActivation::always(), domain).policy;
        final_policy.purpose = NativeFinalPurposeV34::IsolatedCalibration {
            version: 34,
            isolation,
            base_genesis: base.genesis.hash,
            measurement_manifest: [9; 64],
        };
        final_policy.calibrations.push(NativeFinalCalibrationV27 {
            profile: [1; 64],
            maximum_reference_bytes: 1,
            maximum_court_bytes: 1,
            maximum_court_work: 1,
            maximum_court_scratch: 1,
            whole_claim_detection_numerator: 1,
            whole_claim_detection_denominator: 2,
            measured_cold_reference_daa: 0,
            measured_sequential_discovery_daa: 0,
            measured_prove_before_seal_daa: 0,
            measured_proof_delivery_daa: 0,
            restart_reorg_margin_daa: 0,
            measurement_root: [0; 64],
            source_availability_root: [0; 64],
            whole_claim_detection_root: [0; 64],
            real_delivery_service_root: [0; 64],
            independent_native_final_root: [0; 64],
            original_source_ready: false,
            whole_claim_detector_ready: false,
            actual_delivery_ready: false,
            independent_node_ready: false,
        });
        let panel = PalwPanelFreeFenceV1::at(ForkActivation::always());
        NativeCalibrationSnapshotV34 {
            version: 34,
            isolation,
            base_params: base.consensus_params_id(),
            base_schedule: base.consensus_schedule_id(),
            funded,
            stage: None,
            final_policy,
            panel: NativePanelFreeSettingsV34 {
                denied_classes: vec![],
                min_effective_bits: panel.min_effective_bits,
                window: panel.window,
                budgets: panel.budgets,
                economics: panel.economics,
                carrier: panel.carrier,
            },
        }
    }
    #[test]
    fn isolated_config_roundtrip_has_no_prior_final_or_population_qualification() {
        let base = crate::config::params::devnet_shipped_params();
        let wire = snapshot(&base);
        let bytes = borsh::to_vec(&wire).unwrap();
        let decoded = NativeCalibrationSnapshotV34::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, wire);
        let p = wire.apply(&base, wire.root()).unwrap();
        assert_ne!(p.genesis.hash, base.genesis.hash);
        assert_ne!(p.consensus_params_id(), base.consensus_params_id());
        assert!(!p.native_final_is_qualified_v34());
        let mut other = p.clone();
        other.crescendo_activation = ForkActivation::never();
        assert!(other.validate_palw_native_final_v27().is_err());
        let mut other = p.clone();
        other.net = crate::config::params::MAINNET_PARAMS.net;
        assert!(other.validate_palw_native_final_v27().is_err());
        assert!(wire.apply(&base, [0; 64]).is_err());
        let mut forged = wire.clone();
        forged.final_policy.calibrations[0].independent_native_final_root = [4; 64];
        assert!(forged.apply(&base, forged.root()).is_err());
        let mut forged = wire.clone();
        forged.panel.min_effective_bits = 1;
        assert!(forged.apply(&base, forged.root()).is_err());
        let mut qualified = wire.clone();
        qualified.final_policy.purpose = NativeFinalPurposeV34::Qualified;
        assert!(qualified.apply(&base, qualified.root()).is_err());
        // The shared loader is the same factory, and the mixing check refuses every other identity.
        let loaded = load_isolated_native_calibration_v34(&base, &bytes, wire.root()).unwrap();
        assert_eq!(loaded.consensus_params_id(), p.consensus_params_id());
        let (id, sched, gen) =
            (p.consensus_params_id().to_string(), p.consensus_schedule_id().to_string(), p.genesis.hash.to_string());
        assert!(check_same_owning_ruleset_v34(&loaded, &id, &sched, &gen).is_ok());
        assert!(check_same_owning_ruleset_v34(&loaded, &base.consensus_params_id().to_string(), &sched, &gen).is_err());
        assert!(check_same_owning_ruleset_v34(&loaded, &id, &sched, &base.genesis.hash.to_string()).is_err());
        let mut other_policy = wire.clone();
        other_policy.final_policy.finder_profit_margin_sompi += 1;
        if let Ok(q) = other_policy.apply(&base, other_policy.root()) {
            assert!(check_same_owning_ruleset_v34(&q, &id, &sched, &gen).is_err());
        }
    }
    #[test]
    fn truncated_or_oversized_snapshot_does_not_decode_or_arm() {
        let base = crate::config::params::devnet_shipped_params();
        let raw = borsh::to_vec(&snapshot(&base)).unwrap();
        for size in [0, 1, 65, raw.len() - 1] {
            assert!(NativeCalibrationSnapshotV34::from_bytes(&raw[..size]).is_err());
        }
        let mut trailing = raw.clone();
        trailing.push(0);
        assert!(NativeCalibrationSnapshotV34::from_bytes(&trailing).is_err());
        assert!(NativeCalibrationSnapshotV34::from_bytes(&vec![0; NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34 + 1]).is_err());
    }
}
