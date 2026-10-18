//! Routed-reader evidence from the executor's real capture and owning native
//! claim stamp. These paths identify data and directly recompute a step; they
//! never supply selected-verifier approval or change a model tier.
use kaspa_consensus_core::{
    Hash64,
    palw_artifact::PalwArtifactOperandV1,
    palw_native_resource_v1::NativeRetainedV1,
    palw_state_v2::{PalwChainStateV2, PalwStateV2Error},
    palw_step::PalwStepCoordinateV1,
    palw_step_leg::PalwStepBindingV2,
    palw_step_refute::{
        PalwStepRefuteError,
        routing_v2::{PalwRoutedLeafEvidenceV2, PalwRoutingCourtErrorV2, routed_leaf_evidence_from_capture_v2},
    },
};

/// Only the immutable declaration authenticated in this native claim's stamp
/// chooses its reader's router. A provider file / challenger cannot replace it.
/// The returned owner must follow evidence through signing and publication;
/// caller retains the input capture and inventory for their own lifetimes.
pub fn base0_routed_leaf_evidence_from_capture_v2(
    native: &PalwChainStateV2,
    claim_id: Hash64,
    tiles: &crate::legs::Base0StepTilesV1,
    binding: &PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    inventory: &[PalwArtifactOperandV1],
    max_leaves: u64,
) -> Result<NativeRetainedV1<PalwRoutedLeafEvidenceV2>, PalwRoutingCourtErrorV2> {
    let missing = |s| PalwRoutingCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(s));
    let claim = native.claim(&claim_id).ok_or_else(|| missing("routed capture has no owning native claim"))?;
    if claim.execution_root != binding.committed_execution_root {
        return Err(missing("routed capture is not the native claim's execution"));
    }
    let row = native
        .reader_router_of_claim_v2(claim_id)
        .map_err(|e| match e {
            PalwStateV2Error::NativeResource(e) => PalwRoutingCourtErrorV2::Resource(e),
            _ => missing("routed capture's native consumer identity is unavailable"),
        })?
        .ok_or_else(|| missing("routed capture belongs to a V1 claim without the V2 consumer stamp"))?;
    routed_leaf_evidence_from_capture_v2(binding, target, &row.declaration, &tiles.leaves, &tiles.tiles, inventory, max_leaves)
}
