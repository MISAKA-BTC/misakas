//! Independent register/seat anchors for the real-checkpoint path: the class is
//! registered against a mature unrefuted original source, or a job is funded with
//! the producer's original consent — re-derived from one pinned header pair's
//! verified full aux opening, never from producer or node flags.
use crate::native_audit_v32::native_inner_state_of_pinned_headers_v32;
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1,
    palw_native_audit_proof_v32::{
        CheckedNativeLifecycleAnchorV32, NativeLifecycleStageV32, derive_native_lifecycle_anchor_v32, verify_native_aux_v32,
    },
    palw_state_proof_v1::PalwFactProofV1,
};

#[allow(clippy::too_many_arguments)]
pub fn derive_anchored_lifecycle_v32(
    child: &Header,
    pinned_child: Hash64,
    parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
    fork_opening: Option<&PalwForkChoiceOpeningV1>,
    auxiliary: &PalwFactProofV1,
    stage: NativeLifecycleStageV32,
    id: [u8; 64],
) -> Result<CheckedNativeLifecycleAnchorV32, String> {
    let inner =
        native_inner_state_of_pinned_headers_v32(child, pinned_child, parent, pinned_selected_parent, parameters, fork_opening)?;
    let verified = verify_native_aux_v32(auxiliary, inner)?;
    derive_native_lifecycle_anchor_v32(&verified, parameters, parent.daa_score, stage, id)
}
