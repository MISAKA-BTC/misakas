//! Native recurrence-layout metadata, admitted before its layer Vecs are built.
use super::*;
use std::mem::size_of;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RecurrenceKindV3 {
    Held,
    Hybrid,
    Composed,
    Solo,
}
/// Shared with the existing native dispatcher; map meanings do not move.
pub(super) fn recurrence_kind_v3(profile: &crate::palw_step::PalwShapeProfileV3) -> RecurrenceKindV3 {
    if maps::palw_profile_is_held_v4(profile) {
        RecurrenceKindV3::Held
    } else if profile.state_chunk_map_id == maps::hybrid_state_chunk_map_id_v3() {
        RecurrenceKindV3::Hybrid
    } else if maps::palw_map_is_hybrid_composed_v2(&profile.state_chunk_map_id) {
        RecurrenceKindV3::Composed
    } else {
        RecurrenceKindV3::Solo
    }
}
const PATH: &str = "GDN recurrence layout layer metadata";
fn layers_request(n: u64) -> Result<u64, PalwGdnCourtErrorV2> {
    if n == 0 {
        return Ok(0);
    }
    let cap = n.max(4).checked_next_power_of_two().ok_or_else(|| NativeCoverageV1::overflow::<u16>(PATH))?;
    let peak = if cap > 4 { cap.checked_add(cap / 2).ok_or_else(|| NativeCoverageV1::overflow::<u16>(PATH))? } else { cap };
    peak.checked_mul(size_of::<u16>() as u64).ok_or_else(|| NativeCoverageV1::overflow::<u16>(PATH).into())
}
/// Returned native layout values retire before their request. Hybrid/composed
/// temporary layouts retire inside the native dispatcher while this lease lives.
pub(crate) struct RetainedRecurrenceSectionV3 {
    _layout: Option<maps::PalwStateLayoutV4>,
    _lease: Option<NativeDemandLeaseV1>,
    pub(crate) offset: u64,
    pub(crate) count: u64,
}
pub(crate) fn gdn_recurrence_section_retained_v3(
    profile: &crate::palw_step::PalwShapeProfileV3,
    positions: u32,
) -> Result<RetainedRecurrenceSectionV3, PalwGdnCourtErrorV2> {
    // This borrowed native geometry check rejects unknown layouts before any
    // reservation or fallback and gives the authoritative recurrence layers.
    let geometry = PalwGdnStateGeometryV2::of(profile).map_err(evidence)?;
    let kind = recurrence_kind_v3(profile);
    let attention = if kind == RecurrenceKindV3::Solo {
        0
    } else {
        (0..profile.layer_count).filter(|layer| profile.layer_kind(*layer) == crate::palw_step::PalwLayerKindV1::Attention).count()
            as u64
    };
    let recurrence =
        if matches!(kind, RecurrenceKindV3::Held | RecurrenceKindV3::Hybrid) { geometry.layers().count() as u64 } else { 0 };
    let bytes =
        layers_request(attention)?.checked_add(layers_request(recurrence)?).ok_or_else(|| NativeCoverageV1::overflow::<u16>(PATH))?;
    let lease = reserve_native_requests_v1(bytes)?;
    let (offset, count, layout) = gdn_recurrence_section_v2(profile, positions)?;
    Ok(RetainedRecurrenceSectionV3 { _layout: layout, _lease: lease, offset, count })
}

#[cfg(test)]
#[path = "recurrence_layout_owner_tests_v3.rs"]
mod tests;
