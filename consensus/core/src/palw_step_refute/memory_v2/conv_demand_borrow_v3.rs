//! Shared native chunk traversal and actual borrowed demand equality.
use super::*;

/// Only offset acquisition differs: the historical collector keeps its existing
/// caller-owned metadata scope; the new borrowed checker admits that metadata.
fn visit_conv_chunk_indices_v3(
    geometry: &PalwGdnStateGeometryV2<'_>,
    layer: u16,
    head: u32,
    offset: u64,
    mut visit: impl FnMut(u32) -> Result<(), PalwGdnCourtErrorV2>,
) -> Result<(), PalwGdnCourtErrorV2> {
    for block in 0..geometry.conv_kernel.div_ceil(geometry.conv_rows_per_chunk) {
        let local = geometry.conv_chunk_index(layer, head, block).ok_or(PalwStepRefuteError::Unadjudicable)?;
        let flat = offset
            .checked_add(local)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| NativeCoverageV1::overflow::<u32>("GDN borrowed conv chunk index"))?;
        visit(flat)?;
    }
    Ok(())
}

pub(super) fn collect_conv_chunks_v3(
    binding: &leg::PalwStepBindingV2,
    conv_slot: u32,
    head: u32,
    positions: u32,
) -> Result<Vec<u32>, PalwGdnCourtErrorV2> {
    let geometry = PalwGdnStateGeometryV2::of(&binding.shape_profile).map_err(evidence)?;
    let (layer, _) = conv_sources_v2(&binding.shape_profile, conv_slot, &geometry)?;
    let (offset, _, _) = gdn_recurrence_section_v2(&binding.shape_profile, positions)?;
    let mut out = Vec::new();
    visit_conv_chunk_indices_v3(&geometry, layer, head, offset, |flat| {
        out.push(flat);
        Ok(())
    })?;
    Ok(out)
}

/// Compare the actual borrowed units in native block order without constructing
/// a selected-index or replacement-demand Vec. Mismatch is never NoFaultFound.
pub fn gdn_check_conv_demands_borrowed_v3(
    binding: &leg::PalwStepBindingV2,
    conv_slot: u32,
    head: u32,
    index: u32,
    demands: &[crate::palw_held_da_v1::PalwHeldMissingV1],
) -> Result<(), PalwGdnCourtErrorV2> {
    let geometry = PalwGdnStateGeometryV2::of(&binding.shape_profile).map_err(evidence)?;
    let (layer, _) = conv_sources_v2(&binding.shape_profile, conv_slot, &geometry)?;
    let (_, positions) = checkpoint_site_v2(binding, index)?;
    let count = geometry.conv_kernel.div_ceil(geometry.conv_rows_per_chunk);
    if demands.len() as u64 != u64::from(count) {
        return Err(evidence("the answered demands are not the units this window reads"));
    }
    let metadata = super::super::capture::gdn_recurrence_section_retained_v3(&binding.shape_profile, positions)?;
    let mut ordinal = 0usize;
    visit_conv_chunk_indices_v3(&geometry, layer, head, metadata.offset, |flat| {
        if u64::from(flat) >= metadata.count {
            return Err(PalwStepRefuteError::Unadjudicable.into());
        }
        let expected = crate::palw_held_da_v1::PalwHeldMissingV1::StateChunk { checkpoint: index, chunk: flat };
        if demands.get(ordinal) != Some(&expected) {
            return Err(evidence("the answered demands are not the units this window reads"));
        }
        ordinal = ordinal.checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<usize>("GDN borrowed conv ordinal"))?;
        Ok(())
    })?;
    if ordinal != demands.len() {
        return Err(evidence("the answered demands are not the units this window reads"));
    }
    Ok(())
}
