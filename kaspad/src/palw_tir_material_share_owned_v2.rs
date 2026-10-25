//! Non-wire material sharing: every worker clone retains the payload ticket.
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeCoverageV1, NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1,
};
use std::{ops::Deref, sync::Arc};

/// Field order drops the retained payload before its actual Arc allocation hold.
/// An Arc clone copies no authenticated IDs or response vectors.
pub(super) struct SharedMaterialV2<T> {
    payload: NativeRetainedV1<T>,
    _arc: Option<NativeDemandLeaseV1>,
}
impl<T> Deref for SharedMaterialV2<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.payload
    }
}
impl<T> SharedMaterialV2<T> {
    pub(super) fn arc_requests_v2() -> Result<u64, NativeCoverageV1> {
        // Same pinned Arc allocation envelope as NativeHeapV1 for Arc<T>:
        // inline value + two reference counters + its alignment padding.
        (std::mem::size_of::<Self>() as u64)
            .checked_add(std::mem::size_of::<[usize; 2]>() as u64)
            .and_then(|n| n.checked_add(std::mem::align_of::<Self>() as u64))
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("shared TIR material Arc"))
    }
    pub(super) fn share(payload: NativeRetainedV1<T>) -> Result<Arc<Self>, NativeCoverageV1> {
        let lease = reserve_native_requests_v1(Self::arc_requests_v2()?)?;
        Ok(Arc::new(Self { payload, _arc: lease }))
    }
}

pub(super) struct MaterialWorkerInputsV2 {
    pub(super) units: Vec<Arc<SharedMaterialV2<kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1>>>,
    pub(super) ids: Option<Arc<SharedMaterialV2<kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1>>>,
    _members: Option<NativeDemandLeaseV1>,
}
impl MaterialWorkerInputsV2 {
    pub(super) fn from_case(case: &super::OwnerCaseV1) -> Result<Self, NativeCoverageV1> {
        use kaspa_consensus_core::palw_routed_material_v2::PalwMaterialConstructionRequestsV2;
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<Arc<SharedMaterialV2<kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1>>>(
            case.units.len() as u64,
        )?;
        let lease = reserve_native_requests_v1(q.bytes_v1())?;
        let mut units = Vec::new();
        units.try_reserve_exact(case.units.len()).map_err(|_| {
            NativeCoverageV1::unsupported::<Self>("TIR material worker members", "allocator refused admitted worker members")
        })?;
        if units.capacity() != case.units.len() {
            return Err(NativeCoverageV1::unsupported::<Self>("TIR material worker members", "unexpected exact-reserve capacity"));
        }
        units.extend(case.units.iter().cloned());
        Ok(Self { units, ids: case.ids.clone(), _members: lease })
    }
}

#[cfg(test)]
#[path = "palw_tir_material_share_owned_tests_v2.rs"]
mod tests;
