//! Trusted, separately versioned conditions for original native Final.
//! A producer/claimant cannot supply this policy through a route. Shipping
//! presets remain unarmed; a hash of an illustrative report is not a measured
//! detection floor, source availability, delivery service or economic theorem.
use crate::config::params::ForkActivation;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeFinalClassBasisV27 {
    pub class: Digest,
    pub program: Digest,
    pub inventory: Digest,
    pub real_parameter: Digest,
    pub original_max_scope: Digest,
    pub original_max_resource: Digest,
    pub execution_policy: Digest,
    pub quota_policy: Digest,
    pub discovery_policy: Digest,
    pub source_protocol: Digest,
    pub verifier_profile: Digest,
    pub physical_service: Digest,
    /// Includes full cold reference, every sequential acquisition, proof/court,
    /// carrier fee, and restart allowance. A point filing alone is insufficient.
    pub complete_reference_cost_sompi: u64,
    pub maximum_producer_gain_sompi: u64,
    pub original_producer_reserve_sompi: u64,
    pub original_finder_award_sompi: u64,
    pub original_default_obligation_sompi: u64,
    pub maximum_reference_bytes: u64,
    pub maximum_court_bytes: u64,
    pub maximum_court_work: u64,
    pub maximum_court_scratch: u64,
}
impl NativeFinalClassBasisV27 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-whole-class-final-basis/v27", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeFinalCalibrationV27 {
    /// Public verifier/arithmetic/storage/source-acquisition profile, never a model registry.
    pub profile: Digest,
    pub maximum_reference_bytes: u64,
    pub maximum_court_bytes: u64,
    pub maximum_court_work: u64,
    pub maximum_court_scratch: u64,
    pub whole_claim_detection_numerator: u64,
    pub whole_claim_detection_denominator: u64,
    pub measured_cold_reference_daa: u64,
    pub measured_sequential_discovery_daa: u64,
    pub measured_prove_before_seal_daa: u64,
    pub measured_proof_delivery_daa: u64,
    pub restart_reorg_margin_daa: u64,
    /// Independent original model/input/source and cross-node evidence records.
    pub measurement_root: Digest,
    pub source_availability_root: Digest,
    pub whole_claim_detection_root: Digest,
    pub real_delivery_service_root: Digest,
    pub independent_native_final_root: Digest,
    pub original_source_ready: bool,
    pub whole_claim_detector_ready: bool,
    pub actual_delivery_ready: bool,
    pub independent_node_ready: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeFinalPurposeV34 {
    Qualified,
    /// Separately identified isolated devnet experiment. No production/A6/A8
    /// qualification is inferred from any receipt made under this purpose.
    IsolatedCalibration {
        version: u16,
        isolation: Digest,
        base_genesis: crate::Hash64,
        measurement_manifest: Digest,
    },
    /// LOCAL validation only (lead default 2026-10-15, pending user review): the same
    /// isolated devnet binding as `IsolatedCalibration`, but its Finals are evaluated under
    /// the Qualified conditions (independent node evidence and Final root required), so the
    /// validation phase can produce qualified receipts. Constructible only through the v34
    /// calibration snapshot on a distinct devnet genesis — never a public preset, t12 or mainnet,
    /// never a release height.
    LocalQualifiedValidation {
        version: u16,
        isolation: Digest,
        base_genesis: crate::Hash64,
        measurement_manifest: Digest,
    },
}
impl NativeFinalPurposeV34 {
    pub fn is_qualified(&self) -> bool {
        matches!(self, Self::Qualified | Self::LocalQualifiedValidation { .. })
    }
    pub fn is_local_qualified_validation(&self) -> bool {
        matches!(self, Self::LocalQualifiedValidation { .. })
    }
    /// `(version, isolation, base_genesis, measurement_manifest)` of either isolated purpose.
    pub fn isolation_binding(&self) -> Option<(u16, Digest, crate::Hash64, Digest)> {
        match self {
            Self::Qualified => None,
            Self::IsolatedCalibration { version, isolation, base_genesis, measurement_manifest }
            | Self::LocalQualifiedValidation { version, isolation, base_genesis, measurement_manifest } => {
                Some((*version, *isolation, *base_genesis, *measurement_manifest))
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeFinalPolicyV27 {
    pub version: u16,
    pub network_domain: Digest,
    pub purpose: NativeFinalPurposeV34,
    pub queue_policy: crate::palw_native_queue_v30::NativeQueuePolicyV30,
    pub finder_profit_margin_sompi: u64,
    pub producer_deterrence_margin_sompi: u64,
    pub calibrations: Vec<NativeFinalCalibrationV27>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwNativeFinalFenceV27 {
    pub activation: ForkActivation,
    pub policy: NativeFinalPolicyV27,
}
/// A checked numerical/environmental condition, not a chain inclusion or coin
/// grant. Native must additionally authenticate fresh consent/Fund/nonce, all
/// original paid rows, closure, actual collected collateral and exact queues.
pub struct CheckedNativeFinalConditionsV27 {
    policy: Digest,
    basis: Digest,
    path_daa: u64,
}
impl CheckedNativeFinalConditionsV27 {
    pub fn policy_root(&self) -> Digest {
        self.policy
    }
    pub fn basis_root(&self) -> Digest {
        self.basis
    }
    pub fn complete_path_daa(&self) -> u64 {
        self.path_daa
    }
}
impl NativeFinalPolicyV27 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/trusted-final-policy/v27", self)
    }
    pub fn check(&self, network: Digest) -> Result<(), String> {
        self.queue_policy.check(network)?;
        if let NativeFinalPurposeV34::IsolatedCalibration { version, isolation, measurement_manifest, .. } = &self.purpose {
            if *version != 34
                || *isolation == [0; 64]
                || *measurement_manifest == [0; 64]
                || self.calibrations.iter().any(|c| c.independent_node_ready || c.independent_native_final_root != [0; 64])
            {
                return Err("native calibration34 cannot contain a fabricated prior Final or a qualification flag".into());
            }
        }
        if let NativeFinalPurposeV34::LocalQualifiedValidation { version, isolation, measurement_manifest, .. } = &self.purpose {
            if *version != 34 || *isolation == [0; 64] || *measurement_manifest == [0; 64] {
                return Err("native local qualified validation34 requires its isolated devnet binding".into());
            }
        }
        if self.version != 27
            || self.network_domain != network
            || self.calibrations.is_empty()
            || self.calibrations.len() > 128
            || self.finder_profit_margin_sompi == 0
            || self.producer_deterrence_margin_sompi == 0
            || self.calibrations.windows(2).any(|v| v[0].profile >= v[1].profile)
        {
            return Err("native Final27 lacks canonical trusted calibration/profit conditions".into());
        }
        for c in &self.calibrations {
            if c.profile == [0; 64]
                || c.maximum_reference_bytes == 0
                || c.maximum_court_bytes == 0
                || c.maximum_court_work == 0
                || c.maximum_court_scratch == 0
                || c.whole_claim_detection_numerator == 0
                || c.whole_claim_detection_denominator == 0
                || c.whole_claim_detection_numerator > c.whole_claim_detection_denominator
            {
                return Err("native Final27 lacks a positive whole-claim detection floor".into());
            }
        }
        Ok(())
    }
    pub fn check_class(
        &self,
        network: Digest,
        b: &NativeFinalClassBasisV27,
        original_complete_pursuit_daa: u64,
        source_ready: bool,
        physical_service_ready: bool,
    ) -> Result<CheckedNativeFinalConditionsV27, String> {
        self.check(network)?;
        let c = self
            .calibrations
            .binary_search_by_key(&b.verifier_profile, |v| v.profile)
            .ok()
            .map(|i| &self.calibrations[i])
            .ok_or("native Final27 verifier profile has no conservative public calibration")?;
        if b.maximum_reference_bytes > c.maximum_reference_bytes
            || b.maximum_court_bytes > c.maximum_court_bytes
            || b.maximum_court_work > c.maximum_court_work
            || b.maximum_court_scratch > c.maximum_court_scratch
            || !c.original_source_ready
            || !c.whole_claim_detector_ready
            || !c.actual_delivery_ready
            || (self.purpose.is_qualified() && !c.independent_node_ready)
            || !source_ready
            || !physical_service_ready
            || [c.measurement_root, c.source_availability_root, c.whole_claim_detection_root, c.real_delivery_service_root]
                .contains(&[0; 64])
            || (self.purpose.is_qualified() && c.independent_native_final_root == [0; 64])
            || [
                b.class,
                b.program,
                b.inventory,
                b.real_parameter,
                b.original_max_scope,
                b.original_max_resource,
                b.execution_policy,
                b.quota_policy,
                b.discovery_policy,
                b.source_protocol,
                b.verifier_profile,
                b.physical_service,
            ]
            .contains(&[0; 64])
            || b.maximum_reference_bytes == 0
            || b.maximum_court_bytes == 0
            || b.maximum_court_work == 0
            || b.maximum_court_scratch == 0
        {
            return Err("native Final27 original source/detection/resources/service/node evidence is unready or rebound".into());
        }
        let finder_cost = b
            .complete_reference_cost_sompi
            .checked_add(self.finder_profit_margin_sompi)
            .ok_or("native Final27 complete finder cost overflow")?;
        if b.original_finder_award_sompi < finder_cost {
            return Err("native Final27 complete reference/acquisition/proof/court has no positive finder margin".into());
        }
        let gain = b
            .maximum_producer_gain_sompi
            .checked_add(b.original_default_obligation_sompi)
            .and_then(|n| n.checked_add(self.producer_deterrence_margin_sompi))
            .ok_or("native Final27 gain margin overflow")?;
        let expected_loss = (b.original_producer_reserve_sompi as u128) * (c.whole_claim_detection_numerator as u128);
        if expected_loss <= (gain as u128) * (c.whole_claim_detection_denominator as u128) {
            return Err("native Final27 whole-claim detection-adjusted collateral does not strictly dominate all gains".into());
        }
        if b.original_finder_award_sompi > b.original_producer_reserve_sompi {
            return Err("native Final27 finder amount is not covered by original collectible collateral".into());
        }
        let path = [
            c.measured_cold_reference_daa,
            c.measured_sequential_discovery_daa,
            c.measured_prove_before_seal_daa,
            c.measured_proof_delivery_daa,
            c.restart_reorg_margin_daa,
        ]
        .iter()
        .try_fold(0u64, |a, v| a.checked_add(*v))
        .ok_or("native Final27 measured whole pursuit clock overflow")?;
        if path == 0 || path > original_complete_pursuit_daa {
            return Err("native Final27 measured complete pursuit exceeds original prepaid clock".into());
        }
        Ok(CheckedNativeFinalConditionsV27 { policy: self.root(), basis: b.root(), path_daa: path })
    }
}
