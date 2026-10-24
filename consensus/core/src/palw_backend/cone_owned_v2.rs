//! nonwire sparse-cone outcomes and allocation owners.
//! These types authenticate no model computation, claim, duty or fee authority.
use super::PalwTirPublicConeNeedV1;
use crate::{
    palw_court_v2::PalwCourtVerdictProofV2,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1},
    palw_tir_court_v1::PalwTirBuilderMeterV1,
};

/// Classification comes from the actual allocator/reserver, never error text.
#[derive(Debug)]
pub enum PalwTirPublicConeFaultV2 {
    LocalResource(String),
    Unavailable(String),
}
impl std::fmt::Display for PalwTirPublicConeFaultV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalResource(why) => write!(f, "local TIR cone resource unavailable: {why}"),
            Self::Unavailable(why) => write!(f, "TIR cone unavailable: {why}"),
        }
    }
}
impl std::error::Error for PalwTirPublicConeFaultV2 {}

/// The host leases are MOVED from the evidence store before that store drops.
/// They remain necessary even when optional native logical quotas are disabled.
pub struct PalwTirPublicConeReadyV2 {
    proof: NativeRetainedV1<PalwCourtVerdictProofV2>,
    host_leases: Vec<Box<dyn Send + Sync>>,
    pub meter: PalwTirBuilderMeterV1,
}
impl PalwTirPublicConeReadyV2 {
    /// Caller must still hold the actual construction guards across this call.
    /// Reserving this typed retained object does not cover earlier construction.
    pub fn from_reserved_v2(
        proof: PalwCourtVerdictProofV2,
        host_leases: Vec<Box<dyn Send + Sync>>,
        meter: PalwTirBuilderMeterV1,
    ) -> Result<Self, NativeCoverageV1> {
        let proof = NativeRetainedV1::retain_existing_v1(proof)?;
        Ok(Self { proof, host_leases, meter })
    }
    pub fn proof_v2(&self) -> &PalwCourtVerdictProofV2 {
        &self.proof
    }
    /// Explicit owner transfer. The receiver must keep BOTH guard families
    /// through its evidence/queue lifetime; this does not serialize authority.
    pub fn into_parts_v2(
        self,
    ) -> (PalwCourtVerdictProofV2, Option<NativeDemandLeaseV1>, Vec<Box<dyn Send + Sync>>, PalwTirBuilderMeterV1) {
        let (proof, native) = self.proof.into_parts_v1();
        (proof, native, self.host_leases, self.meter)
    }
}
impl std::fmt::Debug for PalwTirPublicConeReadyV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PalwTirPublicConeReadyV2")
            .field("proof", &self.proof)
            .field("meter", &self.meter)
            .field("host_lease_count", &self.host_leases.len())
            .finish()
    }
}
#[derive(Debug)]
pub enum PalwTirPublicConeProbeOwnedV2 {
    Need(PalwTirPublicConeNeedV1),
    /// Configured construction/work cap or semantic construction refusal;
    /// never a failed local allocator/reserver callback.
    Refused {
        reason: String,
    },
    Ready(PalwTirPublicConeReadyV2),
}

impl From<String> for PalwTirPublicConeFaultV2 {
    fn from(why: String) -> Self {
        Self::Unavailable(why)
    }
}
impl From<&str> for PalwTirPublicConeFaultV2 {
    fn from(why: &str) -> Self {
        Self::Unavailable(why.into())
    }
}
