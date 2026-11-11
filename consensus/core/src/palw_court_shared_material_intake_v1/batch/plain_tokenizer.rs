//! Rooted tokenizer branch of an ordinary TFP1. Registration pays for the
//! raw-to-table parser. This branch still needs the exact claim's input and
//! output operands, their authentication and booked case execution.

pub(crate) mod material;

use super::plain_output::PlainBatchOutputV2;
use crate::{
    Hash64,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_fp_constraint_job_v1::{PalwTokenTableEogOpeningV1, PalwTokenTableOpeningV1},
    palw_freeprompt_v3::{PALW_FP_COMMITMENT_TX_MAX_BYTES, PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeDemandLeaseV1, TirNativeRequestsV1, reserve_native_pending_requests_v1},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_ids_form_id_v1, prompt_ids_tile_count_v1, prompt_token_ids_match_v1},
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwTransitionExtrasV1},
    palw_tir_public_source_v1::{PalwTirPublicAuthenticatedIdsV1, PalwTirPublicSourceV1},
    palw_tokenizer_rendering_v2::{palw_rooted_claim_tokenizer_v1, palw_rooted_tokenizer_for_original_at_clocks_v3},
    palw_v2::{PALW_V2_MAX_NETWORK_ID_BYTES, PalwJobContextV2},
};

pub(super) fn reserve_original_hash_writers_v2(
    original: &PalwFreePromptCommitmentV3,
    context: &PalwJobContextV2,
) -> Result<Option<NativeDemandLeaseV1>, NativeDecodeErrorV1<String>> {
    use NativeDecodeErrorV1::{Decode, Resource};
    let original_bytes = borsh::object_length(original).map_err(|why| Decode(why.to_string()))?;
    if original_bytes > PALW_FP_COMMITMENT_TX_MAX_BYTES || context.network_id.len() > PALW_V2_MAX_NETWORK_ID_BYTES {
        return Err(Decode("ordinary Batch original identity exceeds native wire bounds".into()));
    }
    let context_bytes = borsh::object_length(context).map_err(|why| Decode(why.to_string()))?;
    // Claim and job hashes serialize sequentially, and the job fits within
    // the commitment preimage. The context's CanonicalWriter has the same
    // field widths as Borsh. The existing bound covers simultaneous old/new
    // buffers during growth of any one of these writers.
    let requests = PalwTirPublicSourceV1::native_identity_writer_requests_v3(original_bytes.max(context_bytes) as u64)?;
    reserve_native_pending_requests_v1(requests).map_err(Resource)
}

/// Bounds on native operands, before material and control framing. FullIds
/// is the same body booked by plain Output. The separate literal prompt and
/// replay resource limits are selected by the ordinary PublicOriginal profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlainBatchTokenizerBoundsV2 {
    pub full_ids_body_bytes: u64,
    pub input_ids_count: u32,
    pub input_ids_body_bytes: u64,
    pub rendering_openings: u32,
    pub maximum_rendering_opening_bytes: u64,
    pub maximum_rendering_bytes: u64,
    pub maximum_rendering_siblings: u64,
    pub eog_opening_bytes: u64,
    pub eog_ids: u32,
    pub eog_siblings: u64,
    pub eog_replay_calls: u64,
    /// EOG opening and a length-prefixed Vec of all ordered renderings.
    pub opening_material_bytes: u64,
    /// Leaf, index binding and path hashes for every rendering and EOG.
    pub maximum_opening_hashes: u64,
    pub eog_scratch_bytes: u64,
    pub input_ids_scratch_bytes: u64,
}

/// Versioned branch identity derives only from immutable native inputs.
/// Candidate clocks are checked on each derivation and can rebase between
/// unsigned quote and acceptance without changing the relative graph ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlainBatchTokenizerV2 {
    claim: Hash64,
    source: Hash64,
    registration: Hash64,
    branch: Hash64,
    acceptance_clocks: (u64, u64),
    prompt_form: PalwPromptIdsFormV1,
    prompt_ids_root: Hash64,
    vocab: u32,
    bounds: PlainBatchTokenizerBoundsV2,
}

impl PlainBatchTokenizerV2 {
    pub(crate) fn derive_v2(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        at: &PalwBlockContextV2,
        original: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        prompt_form: PalwPromptIdsFormV1,
    ) -> Result<Self, String> {
        Self::derive_at_clocks_v2(parent, extras, at.daa_score, at.blue_score, original, source, prompt_form).map_err(|error| {
            match error {
                NativeDecodeErrorV1::Decode(why) => why,
                NativeDecodeErrorV1::Resource(coverage) => coverage.to_string(),
            }
        })
    }

    pub(crate) fn derive_at_clocks_v2(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        candidate_daa: u64,
        candidate_blue_score: u64,
        original: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        prompt_form: PalwPromptIdsFormV1,
    ) -> Result<Self, NativeDecodeErrorV1<String>> {
        let _identity_scratch = reserve_original_hash_writers_v2(original, &source.header_v1().job_context)?;
        Self::derive_with_hash_writers_v2(parent, extras, candidate_daa, candidate_blue_score, original, source, prompt_form)
            .map_err(NativeDecodeErrorV1::Decode)
    }

    fn derive_with_hash_writers_v2(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        candidate_daa: u64,
        candidate_blue_score: u64,
        original: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        prompt_form: PalwPromptIdsFormV1,
    ) -> Result<Self, String> {
        let full_ids_body_bytes = PlainBatchOutputV2::full_ids_bound_v2(original, source)
            .map_err(|why| format!("ordinary tokenizer branch changes the original or Source: {why:?}"))?;
        let header = source.header_v1();
        let prompt_ids_root = original.job.prompt_token_ids_hash;
        if original.job.prompt_tokens == 0
            || original.job.prompt_tokens != header.job_context.declared_prefill_tokens
            || prompt_ids_root != header.job_context.prompt_token_ids_hash
        {
            return Err("ordinary tokenizer branch changes the original prompt operands".into());
        }
        let (row, openings) =
            palw_rooted_tokenizer_for_original_at_clocks_v3(parent, extras, candidate_daa, candidate_blue_score, original, header)
                .ok_or("ordinary tokenizer branch requires an earlier authenticated registration on the native parent")?;
        if openings.full_ids_body_bytes != Some(full_ids_body_bytes) {
            return Err("ordinary tokenizer branch differs from plain Output FullIds".into());
        }
        let facts = row.facts_v1();
        let vocab = facts.statement.vocab;
        let leaves = u64::from(vocab) + 1;
        let rendering_count = u64::from(original.decode_tokens_executed);
        let maximum_rendering_siblings = crate::palw_step_leg::step_leg_max_opening_siblings_v1(leaves) as u64;
        let eog_siblings = crate::palw_step_leg::step_range_sibling_count_v1(leaves, u64::from(vocab), 1);
        let overflow = || "ordinary tokenizer branch resource bound overflows".to_string();
        let opening_material_bytes = rendering_count
            .checked_mul(openings.maximum_rendering_opening_bytes)
            .and_then(|bytes| bytes.checked_add(4 + openings.eog_opening_bytes))
            .ok_or_else(overflow)?;
        let maximum_opening_hashes = rendering_count
            .checked_mul(2 + maximum_rendering_siblings)
            .and_then(|hashes| hashes.checked_add(2 + eog_siblings))
            .ok_or_else(overflow)?;
        let input_ids_body_bytes = 4 + 4 * u64::from(original.job.prompt_tokens);
        let mut input = TirNativeRequestsV1::new();
        match prompt_form {
            // The native canonical writer appends each ID to a growing buffer.
            PalwPromptIdsFormV1::Flat => input.vec_v1::<u8>(input_ids_body_bytes).map_err(|why| why.to_string())?,
            PalwPromptIdsFormV1::MerkleV1 => {
                let tiles = prompt_ids_tile_count_v1(u64::from(original.job.prompt_tokens))
                    .ok_or("ordinary tokenizer input exceeds the native prompt tree")?;
                // Raw tile leaves, hashed tree level and its next level can coexist.
                input.vec_v1::<Hash64>(tiles).map_err(|why| why.to_string())?;
                input.exact_v1::<Hash64>(tiles + tiles.div_ceil(2)).map_err(|why| why.to_string())?;
                input
                    .exact_v1::<u32>(u64::from(crate::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN))
                    .map_err(|why| why.to_string())?;
            }
        }
        let bounds = PlainBatchTokenizerBoundsV2 {
            full_ids_body_bytes,
            input_ids_count: original.job.prompt_tokens,
            // Canonical Borsh Vec<u32>, without a carrier, proof or transport header.
            input_ids_body_bytes,
            rendering_openings: original.decode_tokens_executed,
            maximum_rendering_opening_bytes: openings.maximum_rendering_opening_bytes,
            maximum_rendering_bytes: facts.maximum_rendering_bytes,
            maximum_rendering_siblings,
            eog_opening_bytes: openings.eog_opening_bytes,
            eog_ids: facts.eog_count,
            eog_siblings,
            eog_replay_calls: openings.eog_replay_calls,
            opening_material_bytes,
            maximum_opening_hashes,
            // The EOG leaf primitive clones its ids before sorting them.
            eog_scratch_bytes: u64::from(facts.eog_count) * 4,
            input_ids_scratch_bytes: input.bytes_v1(),
        };
        let claim = fp_claim_id_v3(original);
        let source = source.source_id_v1();
        let registration = openings.registration_id;
        let branch = stream_digest(
            b"misaka-palw/plain-batch-tokenizer-branch/v2",
            &(claim, source, registration, prompt_ids_form_id_v1(prompt_form)),
        )?;
        Ok(Self {
            claim,
            source,
            registration,
            branch,
            acceptance_clocks: (candidate_daa, candidate_blue_score),
            prompt_form,
            prompt_ids_root,
            vocab,
            bounds,
        })
    }

    pub(crate) fn claim_id_v2(&self) -> Hash64 {
        self.claim
    }

    pub(crate) fn source_id_v2(&self) -> Hash64 {
        self.source
    }

    pub(crate) fn registration_id_v2(&self) -> Hash64 {
        self.registration
    }

    pub(crate) fn branch_id_v2(&self) -> Hash64 {
        self.branch
    }

    pub(crate) fn matches_claim_v2(&self, claim: Hash64) -> bool {
        self.claim == claim
    }

    /// The Batch scheduler consumes DAA. Derive again from the authenticated
    /// native parent and both accepting clocks before publishing a booking.
    pub(crate) fn matches_acceptance_v2(&self, daa: u64) -> bool {
        self.acceptance_clocks.0 == daa
    }

    pub(crate) fn bounds_v2(&self) -> PlainBatchTokenizerBoundsV2 {
        self.bounds
    }

    /// Compare the replayed IDs to the native input commitment. The selected
    /// verifier must produce these IDs from the authenticated literal prompt
    /// and registered raw tokenizer. Checking the committed IDs alone cannot
    /// establish that the text was tokenized correctly.
    pub(crate) fn check_replayed_input_ids_v2(&self, ids: &[u32]) -> Result<(), NativeDecodeErrorV1<String>> {
        use NativeDecodeErrorV1::{Decode, Resource};
        let mismatch = || Decode("ordinary tokenizer input differs from the native prompt commitment".into());
        if ids.len() != self.bounds.input_ids_count as usize || ids.iter().any(|&id| id >= self.vocab) {
            return Err(mismatch());
        }
        let _scratch = reserve_native_pending_requests_v1(self.bounds.input_ids_scratch_bytes).map_err(Resource)?;
        if !prompt_token_ids_match_v1(self.prompt_form, ids, &self.prompt_ids_root) {
            return Err(mismatch());
        }
        Ok(())
    }

    /// Authenticate output renderings in the original generated-ID order,
    /// including repeated IDs, and authenticate the complete EOG opening.
    /// Successful checking yields no paid case, ruling or availability credit.
    pub(crate) fn verify_output_openings_v2(
        &self,
        state: &PalwChainStateV2,
        source: &PalwTirPublicSourceV1,
        ids: &PalwTirPublicAuthenticatedIdsV1,
        ordered_renderings: &[PalwTokenTableOpeningV1],
        eog: &PalwTokenTableEogOpeningV1,
    ) -> Result<(), NativeDecodeErrorV1<String>> {
        use NativeDecodeErrorV1::{Decode, Resource};
        let bounds = self.bounds;
        if source.source_id_v1() != self.source
            || ordered_renderings.len() != bounds.rendering_openings as usize
            || eog.ids.len() != bounds.eog_ids as usize
            || eog.opening.siblings.len() as u64 != bounds.eog_siblings
        {
            return Err(Decode("ordinary tokenizer output operand shape differs from the compiled branch".into()));
        }
        let verifier = palw_rooted_claim_tokenizer_v1(state, self.claim, source, ids)
            .map_err(Decode)?
            .ok_or_else(|| Decode("ordinary tokenizer output requires its prior accepted registration".into()))?;
        if verifier.opening_bounds_v1().registration_id != self.registration {
            return Err(Decode("ordinary tokenizer output substitutes its registration".into()));
        }
        for (position, opening) in ordered_renderings.iter().enumerate() {
            if opening.opening.siblings.len() as u64 > bounds.maximum_rendering_siblings
                || verifier.committed_rendering_at(position as u32, opening).is_none()
            {
                return Err(Decode("ordinary tokenizer output has a foreign or invalid rendering opening".into()));
            }
        }
        let _scratch = reserve_native_pending_requests_v1(bounds.eog_scratch_bytes).map_err(Resource)?;
        if verifier.verified_eog(eog).is_none() {
            return Err(Decode("ordinary tokenizer output has a foreign or invalid EOG opening".into()));
        }
        Ok(())
    }
}
