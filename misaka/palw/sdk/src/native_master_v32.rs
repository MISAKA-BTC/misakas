//! Audit-time Master32 composite Final (lead default 2026-10-15, pending user
//! review). Not a consensus object: one pinned header pair, its verified full aux
//! opening, the owning ruleset and public tokens/outputs determine the verdict.
use crate::native_audit_v32::native_inner_state_of_pinned_headers_v32;
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_fork_choice_commitment_v1::PalwForkChoiceOpeningV1,
    palw_native_audit_proof_v32::{CheckedNativeMasterCompositeV32, derive_native_master_composite_v32, verify_native_aux_v32},
    palw_state_proof_v1::PalwFactProofV1,
};

#[allow(clippy::too_many_arguments)]
pub fn derive_anchored_master_composite_v32(
    child: &Header,
    pinned_child: Hash64,
    parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
    fork_opening: Option<&PalwForkChoiceOpeningV1>,
    auxiliary: &PalwFactProofV1,
    master: [u8; 64],
    jobs: &[[u8; 64]],
    values: &[Vec<misaka_palw_tir::Tensor>],
    generated: &[u32],
) -> Result<CheckedNativeMasterCompositeV32, String> {
    let inner =
        native_inner_state_of_pinned_headers_v32(child, pinned_child, parent, pinned_selected_parent, parameters, fork_opening)?;
    let verified = verify_native_aux_v32(auxiliary, inner)?;
    derive_native_master_composite_v32(&verified, parameters, parent.daa_score, master, jobs, values, generated)
}
