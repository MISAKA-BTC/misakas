//! V2 canonical reader inputs, selected by the native immutable declaration.
//! V1 still uses its historical unique-Q36-router rule. This API authenticates
//! no remote declaration and never turns data membership into claim approval.
use super::*;
use crate::palw_native_resource_v1::{NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_reader_router_v2::{PalwReaderRouterDeclarationV2, PalwReaderRouterErrorV2, selected_reader_router_v2};

type Inputs = Vec<Vec<(u64, PalwStepCoordinateV1)>>;
fn bad(s: &'static str) -> PalwReaderRouterErrorV2 {
    PalwReaderRouterErrorV2::Identity(s)
}

/// Build the exact input inventory before an evidence assembler allocates any
/// openings. The declaration must come from the owning native state/claim
/// stamp. The retained result owns enumeration workspace for its full lifetime.
/// A declaration supplied by a challenger is not admission authority.
pub fn canonical_reader_inputs_v2(
    profile: &PalwShapeProfileV3,
    context: &crate::palw_v2::PalwJobContextV2,
    coord: &PalwStepCoordinateV1,
    declaration: &PalwReaderRouterDeclarationV2,
) -> Result<NativeRetainedV1<Inputs>, PalwReaderRouterErrorV2> {
    let selected = selected_reader_router_v2(profile, declaration, coord.node_slot)?;
    let (node, _) = profile.resolve_node_slot(coord.node_slot).ok_or_else(|| bad("reader coordinate is unresolved"))?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or_else(|| bad("reader kernel is unavailable"))?;
    let first = coord
        .node_slot
        .checked_sub(intra_table_index(profile, coord.node_slot).ok_or_else(|| bad("reader table is absent"))? as u32)
        .ok_or_else(|| bad("reader table start underflows"))?;
    let mut q = TirNativeRequestsV1::new();
    // Routed readers consume actual rows at one position, never a recurrent
    // prefix or KV history. Refuse an unsupported sentinel before construction.
    // Whole source rows bound the actual narrower lane/strided tile inventory.
    let appendix = usize::from(qwen36_reads_routing_v1(node));
    q.vec_v1::<Vec<(u64, PalwStepCoordinateV1)>>(node.input_refs.len() as u64 + appendix as u64)?;
    q.vec_v1::<(u32, u32)>(1)?;
    q.vec_v1::<u64>(node.input_refs.len() as u64)?;
    let width = |n: &crate::palw_step::PalwStepNodeV1| match n.out_len {
        crate::palw_step::PalwStepOutLenV1::Fixed { elements } if n.tile_len > 0 => Ok(u64::from(elements)),
        _ => Err(bad("routed reader source is not a finite registered row")),
    };
    let route_width = width(selected.1)?;
    q.vec_v1::<(u64, u64)>(route_width / 2)?;
    for &r in &node.input_refs {
        if r >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
            return Err(bad("routed reader source is not an actual node"));
        }
        let slot = first.checked_add(u32::from(r)).ok_or_else(|| bad("reader source overflows"))?;
        if slot >= coord.node_slot {
            return Err(bad("reader source must precede output"));
        }
        let (source, _) = profile.resolve_node_slot(slot).ok_or_else(|| bad("reader source is absent"))?;
        let count = width(source)?.div_ceil(u64::from(source.tile_len));
        q.vec_v1::<(u64, PalwStepCoordinateV1)>(count)?;
        q.vec_v1::<(u64, u64)>(route_width / 2 + 1)?;
    }
    if appendix > 0 {
        q.vec_v1::<(u64, PalwStepCoordinateV1)>(route_width.div_ceil(u64::from(selected.1.tile_len)))?;
    }
    let lease = reserve_native_requests_v1(q.bytes_v1())?;
    let inputs = canonical_input_leaves_routing_v2(profile, context, coord.node_slot, coord, program, false, None, Some(selected))
        .ok_or_else(|| bad("reader's canonical leaf coordinates are unavailable"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(inputs, lease))
}
