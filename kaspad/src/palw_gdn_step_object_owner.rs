//! Retained signed step accusation; the existing refutation is an input owner
//! boundary. Quote opening copies, signature and object/Arc storage before build.
use kaspa_consensus_core::{
    palw_artifact::PalwArtifactOpeningV1,
    palw_gdn_court_v2::PalwGdnCourtAccusationV2,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_state_v2::PalwConsensusObjectV2,
    palw_step_refute::memory_v2::PalwGdnStepRefutationV2,
};
use std::{mem::size_of, ops::Deref};

pub struct PalwGdnStepObjectV1 {
    object: PalwConsensusObjectV2,
    _ticket: Option<NativeDemandLeaseV1>,
}
impl Deref for PalwGdnStepObjectV1 {
    type Target = PalwConsensusObjectV2;
    fn deref(&self) -> &Self::Target {
        &self.object
    }
}
impl std::fmt::Debug for PalwGdnStepObjectV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.object.fmt(f)
    }
}
impl PalwGdnStepObjectV1 {
    pub(super) fn request(refutation: &PalwGdnStepRefutationV2, openings: &[PalwArtifactOpeningV1]) -> Result<u64, NativeCoverageV1> {
        let component = "retained GDN step accusation";
        let fixed = size_of::<Self>()
            .checked_add(size_of::<[usize; 3]>())
            .and_then(|n| n.checked_add(size_of::<PalwGdnCourtAccusationV2>()))
            .and_then(|n| n.checked_add(openings.len().checked_mul(size_of::<PalwArtifactOpeningV1>())?))
            .and_then(|n| n.checked_add(kaspa_txscript::MLDSA87_SIG_LEN))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>(component))?;
        let mut bound = NativeCoverageV1::Bounded { bytes: fixed }.add::<Self>(refutation.native_heap_v1(component), component);
        for opening in openings {
            bound = bound.add::<Self>(opening.native_heap_v1(component), component);
        }
        let bound = bound.scale::<Self>(2, component);
        bound.bytes().ok_or(bound)
    }
    pub(super) fn reserve(
        refutation: &PalwGdnStepRefutationV2,
        openings: &[PalwArtifactOpeningV1],
    ) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
        reserve_native_requests_v1(Self::request(refutation, openings)?)
    }
    pub(super) fn finish(object: PalwConsensusObjectV2, ticket: Option<NativeDemandLeaseV1>) -> Self {
        Self { object, _ticket: ticket }
    }
    /// Independent deep copies pre-admit the actual source before Clone.
    pub fn try_clone_v1(&self) -> Result<NativeRetainedV1<PalwConsensusObjectV2>, NativeCoverageV1> {
        NativeRetainedV1::try_from_clone_v1(&self.object)
    }
    /// Keep method resolution from silently selecting the payload's deep Clone.
    pub fn clone(&self) -> Result<NativeRetainedV1<PalwConsensusObjectV2>, NativeCoverageV1> {
        self.try_clone_v1()
    }
}
