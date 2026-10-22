//! Real-capture adapter for the additive recurrence court. Merkle paths bind
//! committed state bytes; the court still directly recomputes the local step.
use kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1;
use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_consensus_core::palw_step_refute::{
    PalwStepRefuteError,
    memory_v2::{PalwGdnCourtErrorV2, PalwGdnStepRefutationV2, gdn_refutation_from_leaves_v2, gdn_replay_start_v2},
};

/// Build from the executor's actual captured leaves, tiles and checkpoint leg.
/// A retaining capture serves its recorded chunks. A folded capture must be
/// given the historical predecessor chunks it re-derived; the current GDN
/// cache is not prefix-stable and cannot be silently substituted. The Core
/// builder authenticates all supplied chunks and returns a construction owner.
///
/// This API does not take ownership of the capture itself, alter V1 roots or
/// activate a dormant native fence. Provider ingress/worker publication must
/// retain both their input capture and the returned owner for their lifetimes.
pub fn base0_gdn_refutation_from_capture_v2(
    tiles: &crate::legs::Base0StepTilesV1,
    checkpoints: &crate::legs::Base0CheckpointsV1,
    binding: &PalwStepBindingV2,
    target: PalwStepCoordinateV1,
    historical_chunks: Option<&[Vec<u8>]>,
    max_leaves: u64,
) -> Result<NativeRetainedV1<PalwGdnStepRefutationV2>, PalwGdnCourtErrorV2> {
    let start = gdn_replay_start_v2(binding, &target)?;
    let missing = |message| PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::InputSetNotCanonical(message));
    let chunks = if let Some(index) = start.checkpoint_index {
        match (checkpoints.chunks.get(index as usize), historical_chunks) {
            (Some(kept), Some(served)) if kept.as_slice() != served => {
                return Err(missing("served predecessor differs from the executor's retained checkpoint"));
            }
            (Some(kept), _) => kept.as_slice(),
            (None, Some(served)) => served,
            (None, None) => return Err(missing("folded recurrence capture needs the actual historical predecessor chunks")),
        }
    } else {
        if historical_chunks.is_some_and(|chunks| !chunks.is_empty()) {
            return Err(missing("genesis recurrence capture cannot accept historical state"));
        }
        &[]
    };
    gdn_refutation_from_leaves_v2(
        binding,
        target,
        &tiles.leaves,
        &tiles.tiles,
        &checkpoints.leaves,
        &checkpoints.leaf_hashes,
        chunks,
        max_leaves,
    )
}
