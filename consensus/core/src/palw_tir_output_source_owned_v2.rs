//! Output conviction's retained native source, with the legacy frame/source
//! identity contract preserved. No new installer, policy or source authority.
use super::{Frame, PalwChainStateV2, PalwStateV2Error, refused};
use crate::{
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1},
    palw_tir_public_source_v1::PalwTirPublicSourceV1 as Source,
};
#[cfg(test)]
thread_local! {static OBSERVED:std::cell::Cell<(u64,u64)>=const{std::cell::Cell::new((0,0))};}
#[cfg(test)]
pub(super) fn reset_observed_v2() {
    OBSERVED.with(|c| c.set((0, 0)));
}
#[cfg(test)]
pub(super) fn observed_v2() -> (u64, u64) {
    OBSERVED.with(std::cell::Cell::get)
}

pub(super) fn source_v2(state: &PalwChainStateV2, frame: &Frame) -> Result<NativeRetainedV1<Source>, PalwStateV2Error> {
    // Same lookup as legacy source(b,frame), and same full frame check as
    // frame.source_v1. The argument is the actual native fold's resolved frame.
    let record = state.tir_class_v1(&frame.original_v1().job.class_id).ok_or_else(|| refused("TIR original class is absent"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, record.facts.class_id)
        .ok_or_else(|| refused("TIR original registered layout is absent"))?;
    frame.check_structure_v1(&frame.key_v1()).map_err(refused)?;
    let descriptor = frame.descriptor_v1();
    // The maximum-leaves argument is the exact descriptor value, preserving
    // source_identity_v1's equality check rather than accepting a caller cap.
    let maximum_leaves = descriptor.maximum_leaves;
    if record.facts.class_id != descriptor.class_id || record.layout_digest != descriptor.layout_digest {
        return Err(refused("original TIR frame substitutes native class, layout or leaf ceiling"));
    }
    #[cfg(test)]
    OBSERVED.with(|c| {
        let (a, b) = c.get();
        c.set((a + 1, b));
    });
    let source = Source::new_native_v1(
        record,
        layout,
        frame.header_v1(),
        frame.original_v1().execution_root,
        frame.original_v1().trace_root,
        maximum_leaves,
    )
    .map_err(|why| match why {
        NativeDecodeErrorV1::Resource(why) => PalwStateV2Error::NativeResource(why),
        NativeDecodeErrorV1::Decode(why) => refused(why),
    })?;
    if source.source_id_v1() != frame.plan_v1().source_id {
        return Err(refused("original TIR frame substitutes source identity"));
    }
    #[cfg(test)]
    OBSERVED.with(|c| {
        let (a, b) = c.get();
        c.set((a, b + 1));
    });
    Ok(source)
}
