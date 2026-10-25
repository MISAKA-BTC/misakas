//! Additive actual held route; the legacy raw callback and wire stay unchanged.
use super::*;

pub struct PalwGdnTransitionBuiltV3 {
    refutation: PalwGdnTransitionRefutationV2,
    predecessor_owner: Option<PalwGdnPredecessorOwnerV3>,
}
impl PalwGdnTransitionBuiltV3 {
    /// Explicit transfer into the Base0 value-before-owner carrier. The actual
    /// consumer must retain the owner through court/output admission; no raw
    /// compatibility caller is implicitly claimed to fulfill this obligation.
    pub fn into_parts_v3(self) -> (PalwGdnTransitionRefutationV2, Option<PalwGdnPredecessorOwnerV3>) {
        (self.refutation, self.predecessor_owner)
    }
}

/// Native site/start logic invokes the callback only for the exact predecessor.
/// Typed callback errors propagate directly at the point of failure, before
/// later input construction can substitute another refusal or absence.
#[allow(clippy::too_many_arguments)]
pub fn gdn_transition_refutation_with_accused_retained_v3(
    capture: &super::super::capture::PalwGdnCaptureV2<'_>,
    node_slot: u32,
    head: u32,
    index: u32,
    accused: PalwGdnCheckpointWitnessV2,
    tile: &dyn Fn(PalwStepCoordinateV1) -> Option<leg::PalwStepTileLeafV1>,
    checkpoint: &dyn Fn(u32) -> Result<Option<(leg::PalwCheckpointLeafV2, Vec<Vec<u8>>)>, PalwGdnCourtErrorV2>,
) -> Result<PalwGdnTransitionBuiltV3, PalwGdnCourtErrorV2> {
    let owner = std::cell::RefCell::new(None);
    let witness = |at: u32, positions: u32, layer: u16, selection: Option<u32>| {
        let (leaf, chunks) = checkpoint(at)?.ok_or_else(|| evidence("the capture lacks a checkpoint"))?;
        let built = gdn_predecessor_witness_request_v3(capture, at, positions, layer, selection, &leaf, &chunks)?.build_v3()?;
        let (witness, ticket) = built.into_parts_v3();
        *owner.borrow_mut() = Some(ticket);
        Ok(witness)
    };
    let refutation = transition_refutation_with_witness_v3(capture, node_slot, head, index, Some(accused), tile, &witness)?;
    Ok(PalwGdnTransitionBuiltV3 { refutation, predecessor_owner: owner.into_inner() })
}
