//! Actual transition demand equality without replacement index/demand Vecs.
use super::*;

/// Site, checkpoint, geometry, metadata and native index traversal retain the
/// collector's semantic order. Mismatch is reported only after native traversal
/// completes: it must not mask a later native index representation refusal.
pub fn gdn_check_transition_demands_borrowed_v3(
    binding: &leg::PalwStepBindingV2,
    node_slot: u32,
    head: u32,
    index: u32,
    demands: &[crate::palw_held_da_v1::PalwHeldMissingV1],
) -> Result<(), PalwGdnCourtErrorV2> {
    let profile = &binding.shape_profile;
    let (layer, selection, _) = transition_site_v2(profile, node_slot, head)?;
    let (_, positions) = checkpoint_site_v2(binding, index)?;
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    let metadata = super::super::capture::gdn_recurrence_section_retained_v3(profile, positions)?;
    let mut ordinal = 0usize;
    let mut mismatch = false;
    super::super::capture::visit_selected_delta_chunks_at_v3(&geometry, layer, selection, metadata.offset, |flat| {
        let expected = crate::palw_held_da_v1::PalwHeldMissingV1::StateChunk { checkpoint: index, chunk: flat };
        mismatch |= demands.get(ordinal) != Some(&expected);
        ordinal = ordinal.checked_add(1).ok_or_else(|| NativeCoverageV1::overflow::<usize>("GDN borrowed transition ordinal"))?;
        Ok(())
    })?;
    if mismatch || ordinal != demands.len() {
        return Err(evidence("the answered demands are not the units this transition reads"));
    }
    Ok(())
}
