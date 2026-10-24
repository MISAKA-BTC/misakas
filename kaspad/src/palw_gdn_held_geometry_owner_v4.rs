//! Borrowed Q36 candidate/demand geometry. These reservations cover only the
//! named arrays/layout lists, not earlier leg reconstruction or prefix work.
use super::*;
use kaspa_consensus_core::{
    palw_gdn_state_map_v2::{PalwGdnStateGeometryV2, PalwGdnStateLayoutV2},
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_requests_v1},
};
use std::mem::size_of;
const PATH: &str = "held GDN candidate/demand geometry";
fn overflow<T>() -> NativeCoverageV1 {
    NativeCoverageV1::overflow::<T>(PATH)
}
fn unsupported(why: &str) -> PalwGdnHeldErrorV2 {
    NativeCoverageV1::unsupported::<GeometryPlanV4>(PATH, why).into()
}
fn add(a: u64, b: u64) -> Result<u64, NativeCoverageV1> {
    a.checked_add(b).ok_or_else(overflow::<GeometryPlanV4>)
}
fn exact<T>(n: u64) -> Result<u64, NativeCoverageV1> {
    n.checked_mul(size_of::<T>() as u64).ok_or_else(overflow::<T>)
}
/// Actual pinned Vec push growth: minimum nonzero capacity, doubling, and the
/// simultaneous old/new buffers at a reallocation. This is a typed role count,
/// not a serialized-byte multiplier or physical allocator bound.
fn pushed<T>(n: u64) -> Result<u64, NativeCoverageV1> {
    if n == 0 {
        return Ok(0);
    }
    let minimum = if size_of::<T>() == 1 {
        8
    } else if size_of::<T>() <= 1024 {
        4
    } else {
        1
    };
    let capacity = n.max(minimum).checked_next_power_of_two().ok_or_else(overflow::<T>)?;
    let peak = if capacity > minimum { add(capacity, capacity / 2)? } else { capacity };
    exact::<T>(peak)
}
pub(super) struct GeometryOwnerV4 {
    _lease: Option<NativeDemandLeaseV1>,
}
impl std::fmt::Debug for GeometryOwnerV4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GeometryOwnerV4")
    }
}
fn reserve(bytes: u64) -> Result<GeometryOwnerV4, NativeCoverageV1> {
    Ok(GeometryOwnerV4 { _lease: reserve_native_requests_v1(bytes)? })
}
#[derive(Clone, Copy, Debug)]
pub(super) struct GeometryPlanV4 {
    pub(super) candidates: u64,
    pub(super) transition_heads: u32,
    pub(super) conv_heads: u32,
    pub(super) delta_demands: u64,
    pub(super) conv_demands: u64,
}
impl GeometryPlanV4 {
    pub(super) fn of(binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2) -> Result<Self, PalwGdnHeldErrorV2> {
        let profile = &binding.shape_profile;
        let geometry = PalwGdnStateGeometryV2::of(profile).map_err(unsupported)?;
        let transition_heads = geometry.heads;
        let conv_heads = if geometry.layout == PalwGdnStateLayoutV2::FullConvV1 { 1 } else { geometry.heads };
        let mut candidates = 0u64;
        let mut slot = 0u32;
        while let Some((node, layer)) = profile.resolve_node_slot(slot) {
            if let Some(layer) = layer {
                let heads = if node.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_GDN_STEP) {
                    transition_heads
                } else if node.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_SSM_CONV) {
                    conv_heads
                } else {
                    0
                };
                if heads != 0 {
                    if !geometry.layers().any(|known| known == layer) {
                        return Err(unsupported("Q36 recurrence node is not on a signed recurrence layer"));
                    }
                    candidates = add(candidates, u64::from(heads))?;
                }
            }
            slot = slot.checked_add(1).ok_or_else(overflow::<Self>)?;
        }
        Ok(Self {
            candidates,
            transition_heads,
            conv_heads,
            delta_demands: u64::from(geometry.v_dim.div_ceil(geometry.delta_rows_per_chunk)),
            conv_demands: u64::from(geometry.conv_kernel.div_ceil(geometry.conv_rows_per_chunk)),
        })
    }
    fn demand_count(
        &self,
        binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
        candidate: Base0GdnCheckpointCourtV2,
    ) -> Result<u64, PalwGdnHeldErrorV2> {
        let (slot, head, semantics, heads, count) = match candidate {
            Base0GdnCheckpointCourtV2::Transition { node_slot, head } => {
                (node_slot, head, KDESC_Q36_GDN_STEP, self.transition_heads, self.delta_demands)
            }
            Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, head } => {
                (conv_slot, head, KDESC_Q36_SSM_CONV, self.conv_heads, self.conv_demands)
            }
        };
        let (node, layer) =
            binding.shape_profile.resolve_node_slot(slot).ok_or_else(|| unsupported("candidate node is outside the profile"))?;
        if layer.is_none() || head >= heads || node.kernel_semantics_id != kernel_semantics_id_v1(semantics) {
            return Err(unsupported("candidate changes its registered Q36 kernel/head"));
        }
        Ok(count)
    }
}
pub(super) fn candidates(
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
) -> Result<(Vec<Base0GdnCheckpointCourtV2>, GeometryOwnerV4), PalwGdnHeldErrorV2> {
    let plan = GeometryPlanV4::of(binding)?;
    let count = usize::try_from(plan.candidates).map_err(|_| overflow::<GeometryPlanV4>())?;
    let owner = reserve(exact::<Base0GdnCheckpointCourtV2>(plan.candidates)?)?;
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| unsupported("candidate array allocation refused"))?;
    // Preserve native recurrence-first, then conv, global-slot/head order.
    for conv in [false, true] {
        let semantics = kernel_semantics_id_v1(if conv { KDESC_Q36_SSM_CONV } else { KDESC_Q36_GDN_STEP });
        let heads = if conv { plan.conv_heads } else { plan.transition_heads };
        let mut slot = 0u32;
        while let Some((node, layer)) = binding.shape_profile.resolve_node_slot(slot) {
            if layer.is_some() && node.kernel_semantics_id == semantics {
                for head in 0..heads {
                    values.push(if conv {
                        Base0GdnCheckpointCourtV2::ConvWindow { conv_slot: slot, head }
                    } else {
                        Base0GdnCheckpointCourtV2::Transition { node_slot: slot, head }
                    });
                }
            }
            slot = slot.checked_add(1).ok_or_else(overflow::<GeometryPlanV4>)?;
        }
    }
    if values.len() != count {
        return Err(unsupported("candidate inventory changed after its quote"));
    }
    Ok((values, owner))
}
pub(super) struct DemandAdmissionV4 {
    count: u64,
    persistent_capacity: u64,
    owner: GeometryOwnerV4,
    _workspace: GeometryOwnerV4,
}
impl DemandAdmissionV4 {
    pub(super) fn prepare(
        binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
        candidate: Base0GdnCheckpointCourtV2,
    ) -> Result<Self, PalwGdnHeldErrorV2> {
        let plan = GeometryPlanV4::of(binding)?;
        let count = plan.demand_count(binding, candidate)?;
        let persistent_capacity = pushed::<PalwHeldMissingV1>(count)?;
        // Existing pure demand helpers build one selected u32 list and, for
        // held/composed layouts, the actual attention/recurrence layer lists.
        let lists = pushed::<u16>(u64::from(binding.shape_profile.layer_count))?;
        let workspace = add(pushed::<u32>(count)?, add(lists, lists)?)?;
        Ok(Self { count, persistent_capacity, owner: reserve(persistent_capacity)?, _workspace: reserve(workspace)? })
    }
    pub(super) fn finish(self, demands: &[PalwHeldMissingV1], capacity: usize) -> Result<GeometryOwnerV4, PalwGdnHeldErrorV2> {
        if demands.len() as u64 != self.count || exact::<PalwHeldMissingV1>(capacity as u64)? > self.persistent_capacity {
            return Err(unsupported("native demand result exceeds its exact head/chunk quote"));
        }
        Ok(self.owner)
    }
}
