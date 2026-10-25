//! Additive held predecessor entry: real prefix and serializer, admitted root.
//! Prefix/cache/serializer/source chunks remain separate resource boundaries.
use super::*;
use kaspa_consensus_core::{
    palw_native_resource_v1::NativeDecodeErrorV1,
    palw_step_refute::memory_v2::{PalwGdnCourtErrorV2, gdn_state_root_request_v3},
};
type Error = NativeDecodeErrorV1<String>;
fn replay_error(error: Base0FpRecomputeError) -> Error {
    Error::Decode(error.to_string())
}
fn root_error(error: PalwGdnCourtErrorV2) -> Error {
    match error {
        PalwGdnCourtErrorV2::Resource(capacity) => Error::Resource(capacity),
        semantic => Error::Decode(semantic.to_string()),
    }
}

/// The same native covered->positions, input checks, teacher-forced walk and
/// serializer as the raw entry. Only the root's transient workspace is new.
#[allow(clippy::too_many_arguments)]
pub fn base0_fp_recompute_state_at_covered_root_retained_v3<K: Base0FpRecomputeKernelsV1 + ?Sized>(
    profile: &PalwShapeProfileV3,
    ctx: &PalwJobContextV2,
    prompt_token_ids: &[u32],
    output_token_ids: &[u32],
    covered: u32,
    kernels: &mut K,
    prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Base0FpSeatStateV1, Error> {
    let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, ctx, covered);
    let (prefill, _) = walk_inputs_at_position_v1(profile, ctx, prompt_token_ids, output_token_ids, positions, prompt_ids_form)
        .map_err(replay_error)?;
    walk_rows_v1(kernels, prompt_token_ids, output_token_ids, prefill, 0, positions as usize).map_err(replay_error)?;
    let chunks = kernels.state_chunks_snapshot_retained_v4(profile, positions)?;
    let state_chunks_root =
        gdn_state_root_request_v3(profile, positions, &chunks).map_err(root_error)?.root_v3().map_err(root_error)?;
    Ok(Base0FpSeatStateV1 { covered_decode_call: covered, positions, state_chunks_root, chunks })
}
