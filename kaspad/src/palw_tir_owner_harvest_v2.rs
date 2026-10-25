//! Publish owned authenticated material before recording its accepted reference.
use super::{OwnerCaseV1, SharedMaterialV2};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1 as Fault, btree_node_envelope_bytes_v1, reserve_native_requests_v1,
    },
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_tir_public_state_v1::PalwTirPublicFinishedOwnedV2,
};
use std::sync::Arc;

/// Whole retained membership capacity, separate from authenticated payload and
/// Arc ownership. Old ticket survives until the new whole quote and allocation
/// succeed; clear/failed publication cannot release retained Vec capacity.
pub(super) fn membership_requests_v2(case: &OwnerCaseV1) -> Result<(usize, u64), NativeCoverageV1> {
    if usize::BITS != 64 {
        return Err(NativeCoverageV1::unsupported::<OwnerCaseV1>(
            "TIR harvested membership",
            "BTree node layout requires pinned64-bit target",
        ));
    }
    let units = case.units.len().checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<OwnerCaseV1>("TIR unit membership"))?;
    let capacity = case.units.capacity().max(units);
    let entries =
        case.harvested.len().checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<OwnerCaseV1>("TIR harvested membership"))?;
    let mut q = PalwMaterialConstructionRequestsV2::new();
    q.vec_v1::<Arc<SharedMaterialV2<kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1>>>(
        capacity as u64,
    )?;
    let tree = entries
        .checked_mul(btree_node_envelope_bytes_v1::<Hash64, ()>())
        .ok_or_else(|| NativeCoverageV1::overflow::<OwnerCaseV1>("TIR harvested tree"))?;
    q.add_v1(tree as u64)?;
    Ok((capacity, q.bytes_v1()))
}
fn reserve_membership(case: &mut OwnerCaseV1) -> Result<(), NativeCoverageV1> {
    let (capacity, bytes) = membership_requests_v2(case)?;
    let ticket = reserve_native_requests_v1(bytes)?;
    let mut replacement = Vec::new();
    replacement.try_reserve_exact(capacity).map_err(|_| {
        NativeCoverageV1::unsupported::<OwnerCaseV1>("TIR unit membership", "allocator refused admitted unit membership")
    })?;
    // Pinned Vec records the exact requested RawVec capacity. Reject an
    // unsupported allocator/layout result before publishing any replacement.
    if replacement.capacity() != capacity {
        return Err(NativeCoverageV1::unsupported::<OwnerCaseV1>("TIR unit membership", "unexpected exact-reserve capacity"));
    }
    replacement.extend(case.units.iter().cloned());
    case.units = replacement;
    case._harvest_membership = ticket;
    Ok(())
}
pub(super) fn harvest_accepted_v2(
    case: &mut OwnerCaseV1,
    id: Hash64,
    load: impl FnOnce() -> Result<Option<PalwTirPublicFinishedOwnedV2>, Fault<String>>,
) -> Result<bool, Fault<String>> {
    if case.harvested.contains(&id) {
        return Ok(false);
    }
    let Some(material) = load()? else { return Ok(false) };
    reserve_membership(case).map_err(Fault::Resource)?;
    match material {
        PalwTirPublicFinishedOwnedV2::Unit(unit) => {
            if !case.units.iter().any(|u| u.request_v1() == unit.request_v1()) {
                let shared = SharedMaterialV2::share(unit).map_err(Fault::Resource)?;
                case.units.push(shared);
            }
        }
        PalwTirPublicFinishedOwnedV2::Ids(ids) => {
            let shared = SharedMaterialV2::share(ids).map_err(Fault::Resource)?;
            case.ids = Some(shared);
        }
    }
    case.harvested.insert(id);
    Ok(true)
}
#[cfg(test)]
#[path = "palw_tir_owner_harvest_tests_v2.rs"]
pub(super) mod tests;
