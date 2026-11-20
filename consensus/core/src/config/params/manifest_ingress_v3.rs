//! Unadopted native Manifest ingress contract. No shipped preset selects it.
//! Prices and limits are explicit private-network inputs, not measured defaults.
use super::{ForkActivation, Params};
use crate::config::genesis::PALW_T12_GENESIS;
use crate::{palw_court_service_v1::ServiceCost, palw_mode_v2::PalwModeV2Error};
use misaka_palw_positive_contract::ContractError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestIngressFenceV3 {
    pub activation: ForkActivation,
    pub maximum_source_blocks: u64,
    pub maximum_transactions: u64,
    pub maximum_requests: u64,
    pub maximum_request_bytes: u64,
    pub maximum_archive_heap: u64,
    pub maximum_archive_work: u64,
    pub replay_passes: u64,
    pub replay_visits: u64,
    pub byte_sompi: u64,
    pub work_sompi: u64,
    pub io_sompi: u64,
    pub control_sompi: u64,
    pub retained_byte_sompi: u64,
    pub archive_work_sompi: u64,
}
impl ManifestIngressFenceV3 {
    pub fn active_at(&self, daa: u64) -> bool {
        self.activation != ForkActivation::never() && self.activation.is_active(daa)
    }
    /// Stable declaration-order encoding used by identity and schedule hashing.
    pub fn dimensions_v3(&self) -> [u64; 14] {
        [
            self.maximum_source_blocks,
            self.maximum_transactions,
            self.maximum_requests,
            self.maximum_request_bytes,
            self.maximum_archive_heap,
            self.maximum_archive_work,
            self.replay_passes,
            self.replay_visits,
            self.byte_sompi,
            self.work_sompi,
            self.io_sompi,
            self.control_sompi,
            self.retained_byte_sompi,
            self.archive_work_sompi,
        ]
    }
    pub fn validate_v3(&self) -> Result<(), PalwModeV2Error> {
        if self.dimensions_v3().into_iter().any(|n| n == 0 || n == u64::MAX)
            || self.maximum_requests > self.maximum_transactions
            || self.replay_passes < 3
            || usize::try_from(self.maximum_requests).is_err()
            || usize::try_from(self.replay_passes).is_err()
        {
            return Err(PalwModeV2Error::Invalid("Manifest ingress requires finite complete bounds and prices"));
        }
        Ok(())
    }
    pub fn charge_v3(&self, cost: ServiceCost, retained: u64, archive_work: u64) -> Result<u64, ContractError> {
        [
            (cost.bytes, self.byte_sompi),
            (cost.work, self.work_sompi),
            (cost.io, self.io_sompi),
            (cost.controls, self.control_sompi),
            (retained, self.retained_byte_sompi),
            (archive_work, self.archive_work_sompi),
        ]
        .into_iter()
        .try_fold(0u64, |sum, (quantity, rate)| {
            quantity.checked_mul(rate).and_then(|charge| sum.checked_add(charge)).ok_or(ContractError::Range)
        })
        .and_then(|charge| if charge == 0 { Err(ContractError::Range) } else { Ok(charge) })
    }
}
impl Params {
    pub fn palw_manifest_ingress_v3_at(&self, daa: u64) -> Option<ManifestIngressFenceV3> {
        self.palw_manifest_ingress_v3.filter(|policy| policy.active_at(daa))
    }
    pub fn validate_palw_manifest_ingress_v3(&self) -> Result<(), PalwModeV2Error> {
        let Some(policy) = self.palw_manifest_ingress_v3.filter(|p| p.activation != ForkActivation::never()) else { return Ok(()) };
        policy.validate_v3()?;
        // This draft can only accompany an explicitly salted T12 lineage.
        // No ordinary public startup flag selects it.
        if self.net != crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12)
            || self.genesis.hash == PALW_T12_GENESIS.hash
        {
            return Err(PalwModeV2Error::Invalid("Manifest ingress draft requires isolated salted T12 genesis"));
        }
        let at = policy.activation.daa_score();
        if !self.protected_native_inputs_active_v3(at) {
            return Err(PalwModeV2Error::Invalid("Manifest ingress requires native court, Batch, tokenizer and protected retention"));
        }
        Ok(())
    }
}
