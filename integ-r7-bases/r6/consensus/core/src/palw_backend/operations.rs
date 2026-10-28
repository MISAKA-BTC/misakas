//! Borrowed operation views over the existing backend registry seam.
//! Construction does not assert support, certify a model, resolve rules, or change
//! a default refusal: every operation delegates to the original backend method.
//! Backend effects remain its own; these views add no scheduling.
//! Configuration and drill hooks remain on the compatibility trait.
use super::*;

#[derive(Clone, Copy)]
pub struct PalwExecutionOpsV1<'a> {
    backend: &'a dyn PalwExecutionBackendV1,
}
impl<'a> PalwExecutionOpsV1<'a> {
    pub fn new(backend: &'a dyn PalwExecutionBackendV1) -> Self {
        Self { backend }
    }
    pub fn model_id(&self) -> &str {
        self.backend.model_id()
    }
    pub fn job_for_anchor(&self, anchor: Hash64) -> Result<(PalwJobContextV2, Vec<usize>), String> {
        self.backend.job_for_anchor(anchor)
    }
    pub fn attempt_rules_v1(&self) -> crate::palw_attempt_rules_v1::PalwAttemptRulesV1 {
        self.backend.attempt_rules_v1()
    }
    pub fn execute(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwExecutionOutcomeV1, String> {
        self.backend.execute(job, prompt)
    }
    pub fn execute_free_prompt(
        &self,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_tokens: &[usize],
    ) -> Result<PalwFpRunV1, String> {
        self.backend.execute_free_prompt(_job, _prompt_tokens)
    }
    pub fn answer_free_prompt_v1(
        &self,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_tokens: &[usize],
        _stop_ids: &[u32],
        _on_token: &mut dyn FnMut(u32),
    ) -> Result<PalwFpAnswerV1, String> {
        self.backend.answer_free_prompt_v1(_job, _prompt_tokens, _stop_ids, _on_token)
    }
    pub fn token_table_for_job_v1(
        &self,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
    ) -> Option<std::sync::Arc<crate::palw_fp_constraint_job_v1::PalwTokenTableV1>> {
        self.backend.token_table_for_job_v1(job)
    }
    pub fn prefix_state_v1(
        &self,
        _class_id: crate::Hash64,
        _tokenizer_id: crate::Hash64,
        _prefix_ids: &[u32],
    ) -> Result<crate::palw_freeprompt_v3::PalwFpPrefixStateV1, String> {
        self.backend.prefix_state_v1(_class_id, _tokenizer_id, _prefix_ids)
    }
    pub fn embed_prompt_v1(
        &self,
        _prompt_tokens: &[usize],
        _class_id: crate::Hash64,
        _tokenizer_id: crate::Hash64,
        _pool: crate::palw_embedding_pool_v1::PalwEmbeddingPoolV1,
    ) -> Result<PalwFpEmbeddingV1, String> {
        self.backend.embed_prompt_v1(_prompt_tokens, _class_id, _tokenizer_id, _pool)
    }
    pub fn answer_batch_free_prompt_v1(
        &self,
        jobs: &[PalwFpAnswerJobV1<'_>],
        stop_ids: &[u32],
        on_token: &mut dyn FnMut(usize, u32),
    ) -> Result<Vec<PalwFpAnswerV1>, String> {
        self.backend.answer_batch_free_prompt_v1(jobs, stop_ids, on_token)
    }
    pub fn execute_free_prompt_streaming(
        &self,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        prompt_tokens: &[usize],
        on_token: &mut dyn FnMut(u32),
    ) -> Result<PalwFpRunV1, String> {
        self.backend.execute_free_prompt_streaming(job, prompt_tokens, on_token)
    }
    pub fn fp_job_context_for_executed_v1(
        &self,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _decode_tokens_executed: u32,
    ) -> Option<PalwJobContextV2> {
        self.backend.fp_job_context_for_executed_v1(_job, _decode_tokens_executed)
    }
    pub fn fp_job_context_v1(&self, job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3) -> Option<PalwJobContextV2> {
        self.backend.fp_job_context_v1(job)
    }
    pub fn fp_context_work_leaves_v1(&self, _context: &PalwJobContextV2) -> Option<u64> {
        self.backend.fp_context_work_leaves_v1(_context)
    }
}

#[derive(Clone, Copy)]
pub struct PalwReplayOpsV1<'a> {
    backend: &'a dyn PalwExecutionBackendV1,
}
impl<'a> PalwReplayOpsV1<'a> {
    pub fn new(backend: &'a dyn PalwExecutionBackendV1) -> Self {
        Self { backend }
    }
    pub fn execute_for_verdict(&self, job: &PalwJobContextV2, prompt: &[usize]) -> Result<PalwReplayRootsV1, String> {
        self.backend.execute_for_verdict(job, prompt)
    }
    pub fn replay_segment_from_checkpoint_v1(
        &self,
        _job: &PalwJobContextV2,
        _prompt: &[usize],
        _opening: &[u8],
        _claim: crate::palw_segment_resume_v1::PalwSegmentClaimV1,
    ) -> Result<crate::palw_segment_resume_v1::PalwSegmentReplayV1, String> {
        self.backend.replay_segment_from_checkpoint_v1(_job, _prompt, _opening, _claim)
    }
    pub fn replay_layer_site_v3(
        &self,
        _capture: &[u8],
        _site: crate::palw_layer_sample_v3::PalwLayerSiteV3,
        _seat_count: u16,
    ) -> Result<bool, String> {
        self.backend.replay_layer_site_v3(_capture, _site, _seat_count)
    }
    pub fn replay_accused_segment_v1(
        &self,
        _capture: &[u8],
        _seat_count: u16,
        _segment_index: u16,
        _job: &PalwJobContextV2,
        _prompt: &[usize],
    ) -> Result<crate::palw_segment_resume_v1::PalwSegmentReplayV1, String> {
        self.backend.replay_accused_segment_v1(_capture, _seat_count, _segment_index, _job, _prompt)
    }
    pub fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        self.backend.verify_material(material, claim)
    }
    pub fn replay_tir_generated_ids_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_token_ids: &[u32],
    ) -> Result<PalwTirReplayV1, String> {
        self.backend.replay_tir_generated_ids_v1(_header, _job, _prompt_token_ids)
    }
    pub fn verify_prefix_state_v1(&self, job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3, prompt_ids: &[u32]) -> Result<(), String> {
        self.backend.verify_prefix_state_v1(job, prompt_ids)
    }
    pub fn verify_fp_interval_opening(
        &self,
        _opening: &[u8],
        _claim: PalwClaimRootsV1,
        _index: u32,
        _prompt_token_ids: &[u32],
        _work_leaves: u64,
    ) -> PalwFpIntervalVerdictV1 {
        self.backend.verify_fp_interval_opening(_opening, _claim, _index, _prompt_token_ids, _work_leaves)
    }
    pub fn verify_fp_interval_opening_under_job_v1(
        &self,
        opening: &[u8],
        claim: PalwClaimRootsV1,
        index: u32,
        prompt_token_ids: &[u32],
        work_leaves: u64,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        committed_output_ids: &[u32],
    ) -> PalwFpIntervalVerdictV1 {
        self.backend.verify_fp_interval_opening_under_job_v1(opening, claim, index, prompt_token_ids, work_leaves, job, committed_output_ids)
    }
    pub fn fp_accept_resume_v1(
        &self,
        _resume: &[u8],
        _context: &crate::palw_v2::PalwJobContextV2,
        _prompt_token_ids: &[u32],
        _covered: u32,
    ) -> Result<crate::Hash64, String> {
        self.backend.fp_accept_resume_v1(_resume, _context, _prompt_token_ids, _covered)
    }
    pub fn fp_opening_job_context_v1(&self, _opening: &[u8]) -> Option<PalwJobContextV2> {
        self.backend.fp_opening_job_context_v1(_opening)
    }
    pub fn token_table_for_job_v1(
        &self,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
    ) -> Option<std::sync::Arc<crate::palw_fp_constraint_job_v1::PalwTokenTableV1>> {
        self.backend.token_table_for_job_v1(job)
    }
}

#[derive(Clone, Copy)]
pub struct PalwMaterialOpsV1<'a> {
    backend: &'a dyn PalwExecutionBackendV1,
}
impl<'a> PalwMaterialOpsV1<'a> {
    pub fn new(backend: &'a dyn PalwExecutionBackendV1) -> Self {
        Self { backend }
    }
    pub fn open_segment_checkpoint_v1(&self, _capture: &[u8], _seat_count: u16, _segment_index: u16) -> Result<Vec<u8>, String> {
        self.backend.open_segment_checkpoint_v1(_capture, _seat_count, _segment_index)
    }
    pub fn disclose_trace_event(
        &self,
        _material: &[u8],
        _row: u32,
        _tile: u8,
    ) -> Result<crate::palw_step_refute::PalwTraceEventDisclosureV1, String> {
        self.backend.disclose_trace_event(_material, _row, _tile)
    }
    pub fn open_flat_public_source_v1(
        &self,
        _source: std::sync::Arc<dyn PalwOriginalCaptureReaderV1>,
        _binding: &crate::palw_step_leg::PalwStepBindingV2,
    ) -> Result<Box<dyn PalwFlatPublicSourceReaderV1>, String> {
        self.backend.open_flat_public_source_v1(_source, _binding)
    }
    pub fn tir_public_capture_need_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _source_length: u64,
    ) -> Result<PalwTirPublicCaptureNeedV1, String> {
        self.backend.tir_public_capture_need_v1(_header, _source_length)
    }
    pub fn open_tir_public_capture_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _source: std::sync::Arc<dyn PalwOriginalCaptureReaderV1>,
        _cache_directory: &std::path::Path,
    ) -> Result<Box<dyn PalwTirPublicCaptureReaderV1>, String> {
        self.backend.open_tir_public_capture_v1(_header, _source, _cache_directory)
    }
    pub fn tir_prepared_capture_need_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _source_length: u64,
    ) -> Result<PalwTirPublicPreparedNeedV1, String> {
        self.backend.tir_prepared_capture_need_v1(_header, _source_length)
    }
    pub fn reopen_tir_prepared_capture_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _descriptor: &PalwTirPublicPreparedDescriptorV1,
        _cache_directory: &std::path::Path,
        _native_material_until_daa: u64,
        _native_now_daa: u64,
    ) -> Result<Box<dyn PalwTirPublicCaptureReaderV1>, String> {
        self.backend.reopen_tir_prepared_capture_v1(_header, _descriptor, _cache_directory, _native_material_until_daa, _native_now_daa)
    }
    pub fn retire_tir_prepared_capture_v1(
        &self,
        _descriptor: &PalwTirPublicPreparedDescriptorV1,
        _cache_directory: &std::path::Path,
        _native_material_until_daa: u64,
        _native_irreversible_daa: u64,
    ) -> Result<(), String> {
        self.backend.retire_tir_prepared_capture_v1(_descriptor, _cache_directory, _native_material_until_daa, _native_irreversible_daa)
    }
    pub fn open_float_public_source_v1(
        &self,
        _source: std::sync::Arc<dyn PalwOriginalCaptureReaderV1>,
        _binding: &crate::palw_step_leg::PalwStepBindingV2,
    ) -> Result<Box<dyn PalwFloatPublicSourceReaderV1>, String> {
        self.backend.open_float_public_source_v1(_source, _binding)
    }
    pub fn write_float_public_source_v1(&self, _material: &[u8], _out: &mut dyn std::io::Write) -> Result<bool, String> {
        self.backend.write_float_public_source_v1(_material, _out)
    }
    pub fn write_flat_public_source_v1(&self, _material: &[u8], _out: &mut dyn std::io::Write) -> Result<bool, String> {
        self.backend.write_flat_public_source_v1(_material, _out)
    }
    pub fn public_execution_binding_header_v1(
        &self,
        _material: &[u8],
    ) -> Result<crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1, String> {
        self.backend.public_execution_binding_header_v1(_material)
    }
    pub fn disclose_selected_event_v1(
        &self,
        _material: &[u8],
        _row: u32,
        _tile: u32,
        _maximum_leaves: u64,
    ) -> Result<(crate::palw_step_leg::PalwStepBindingV2, crate::palw_selected_event_source_v1::PalwSelectedTiledEventV1), String> {
        self.backend.disclose_selected_event_v1(_material, _row, _tile, _maximum_leaves)
    }
    pub fn disclose_tir_selected_event_v1(
        &self,
        _material: &[u8],
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _row: u32,
        _tile: u32,
        _maximum_leaves: u64,
    ) -> Result<crate::palw_selected_event_source_v1::PalwSelectedTiledEventV1, String> {
        self.backend.disclose_tir_selected_event_v1(_material, _header, _row, _tile, _maximum_leaves)
    }
    pub fn tir_own_step_source_need_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_token_ids: &[u32],
    ) -> Result<PalwTirOwnStepSourceNeedV1, String> {
        self.backend.tir_own_step_source_need_v1(_header, _job, _prompt_token_ids)
    }
    pub fn open_tir_own_step_source_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_token_ids: &[u32],
        _cache_directory: &std::path::Path,
    ) -> Result<std::sync::Arc<dyn PalwTirOwnStepSourceV1>, String> {
        self.backend.open_tir_own_step_source_v1(_header, _job, _prompt_token_ids, _cache_directory)
    }
    pub fn disclose_tir_step_leaf_v1(
        &self,
        _material: &[u8],
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _leaf_index: u64,
        _maximum_leaves: u64,
    ) -> Result<(Vec<u8>, crate::palw_step_leg::PalwStepOpeningV1), String> {
        self.backend.disclose_tir_step_leaf_v1(_material, _header, _leaf_index, _maximum_leaves)
    }
    pub fn artifact_inventory_digest(&self) -> Result<crate::palw_artifact::PalwArtifactInventoryDigestV1, String> {
        self.backend.artifact_inventory_digest()
    }
    pub fn artifact_row_opening(&self, _index: u32) -> Result<crate::palw_artifact::PalwArtifactOpeningV1, String> {
        self.backend.artifact_row_opening(_index)
    }
    pub fn artifact_possession_tree_v1(&self) -> Option<Result<(crate::Hash64, u32), String>> {
        self.backend.artifact_possession_tree_v1()
    }
    pub fn artifact_readiness_material(
        &self,
        draw: &[u32],
    ) -> Result<(crate::Hash64, Vec<crate::Hash64>, Vec<(u32, crate::palw_artifact::PalwArtifactOperandV1)>), String> {
        self.backend.artifact_readiness_material(draw)
    }
    pub fn artifact_readiness_material_streamed_v1(
        &self,
        _draw: &[u32],
        _on_leaf: &mut dyn FnMut(crate::Hash64),
    ) -> Option<Result<(crate::Hash64, u32, Vec<(u32, crate::palw_artifact::PalwArtifactOperandV1)>), String>> {
        self.backend.artifact_readiness_material_streamed_v1(_draw, _on_leaf)
    }
    pub fn fp_interval_count(&self, _capture: &[u8]) -> Option<u32> {
        self.backend.fp_interval_count(_capture)
    }
    pub fn fp_interval_count_for(&self, _prompt_tokens: u32, _decode_tokens_executed: u32) -> Option<u32> {
        self.backend.fp_interval_count_for(_prompt_tokens, _decode_tokens_executed)
    }
    pub fn open_fp_interval(&self, _capture: &[u8], _index: u32, _prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        self.backend.open_fp_interval(_capture, _index, _prompt_token_ids)
    }
    pub fn fp_checkpoint_covered_bound_v1(&self, job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3) -> u32 {
        self.backend.fp_checkpoint_covered_bound_v1(job)
    }
    pub fn open_fp_resume_v1(&self, _capture: &[u8], _index: u32, _prompt_token_ids: &[u32]) -> Result<Vec<u8>, String> {
        self.backend.open_fp_resume_v1(_capture, _index, _prompt_token_ids)
    }
    pub fn fp_committed_output_ids(&self, _capture: &[u8]) -> Option<Vec<u32>> {
        self.backend.fp_committed_output_ids(_capture)
    }
    pub fn open_fp_interval_with_close(
        &self,
        capture: &[u8],
        index: u32,
        prompt_token_ids: &[u32],
        disputed: &[u64],
    ) -> Result<Vec<u8>, String> {
        self.backend.open_fp_interval_with_close(capture, index, prompt_token_ids, disputed)
    }
    pub fn open_fp_block_leaves(
        &self,
        _capture: &[u8],
        _interval_index: u32,
        _block_index: u64,
        _prompt_token_ids: &[u32],
    ) -> Result<Vec<u8>, String> {
        self.backend.open_fp_block_leaves(_capture, _interval_index, _block_index, _prompt_token_ids)
    }
    pub fn held_state_chunk_answer_v1(
        &self,
        _capture: &[u8],
        _prompt_token_ids: &[u32],
        _checkpoint: u32,
        _chunk: u32,
    ) -> Result<(crate::palw_attn_court_v1::PalwAttnCheckpointAnchorV1, crate::palw_attn_court_v1::PalwAttnChunkOpeningV1), String> {
        self.backend.held_state_chunk_answer_v1(_capture, _prompt_token_ids, _checkpoint, _chunk)
    }
    pub fn held_step_range_answer_v1(
        &self,
        _capture: &[u8],
        _prompt_token_ids: &[u32],
        _first: u64,
        _count: u32,
    ) -> Result<crate::palw_step_leg::PalwStepRangeOpeningV1, String> {
        self.backend.held_step_range_answer_v1(_capture, _prompt_token_ids, _first, _count)
    }
    pub fn fp_interval_of_leaf_v1(&self, _context: &PalwJobContextV2, _leaf: u64) -> Option<u32> {
        self.backend.fp_interval_of_leaf_v1(_context, _leaf)
    }
    pub fn fp_output_root_v1(
        &self,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _output_token_ids: &[u32],
    ) -> Option<crate::Hash64> {
        self.backend.fp_output_root_v1(_job, _output_token_ids)
    }
    pub fn checkpoint_root_for_context_v1(
        &self,
        _context: &PalwJobContextV2,
        _prompt_token_ids: &[u32],
        _output_token_ids: &[u32],
        _covered: u32,
    ) -> Result<crate::Hash64, String> {
        self.backend.checkpoint_root_for_context_v1(_context, _prompt_token_ids, _output_token_ids, _covered)
    }
    pub fn checkpoint_covered_bound_for_context_v1(&self, context: &PalwJobContextV2) -> u32 {
        self.backend.checkpoint_covered_bound_for_context_v1(context)
    }
    pub fn output_root_for_context_v1(&self, _context: &PalwJobContextV2, _output_token_ids: &[u32]) -> Option<crate::Hash64> {
        self.backend.output_root_for_context_v1(_context, _output_token_ids)
    }
}

#[derive(Clone, Copy)]
pub struct PalwCourtEvidenceOpsV1<'a> {
    backend: &'a dyn PalwExecutionBackendV1,
}
impl<'a> PalwCourtEvidenceOpsV1<'a> {
    pub fn new(backend: &'a dyn PalwExecutionBackendV1) -> Self {
        Self { backend }
    }
    pub fn tir_logits_step_leaf_index_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _row: u32,
        _lane: u64,
    ) -> Result<u64, String> {
        self.backend.tir_logits_step_leaf_index_v1(_header, _row, _lane)
    }
    pub fn build_tir_public_cone_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _prompt_token_ids: &[u32],
        _trace: Option<&crate::palw_step_refute::PalwTiledDecodeTokensV1>,
        _runs: &[crate::palw_tir_court_v1::PalwTirStepRunDisclosureV1],
        _leaf_index: u64,
        _rules: &crate::palw_tir_court_v1::PalwTirCourtRulesV1,
    ) -> Result<crate::palw_court_v2::PalwCourtVerdictProofV2, String> {
        self.backend.build_tir_public_cone_v1(_header, _prompt_token_ids, _trace, _runs, _leaf_index, _rules)
    }
    pub fn probe_tir_public_cone_v1(
        &self,
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _prompt_token_ids: &[u32],
        _trace: Option<&crate::palw_step_refute::PalwTiledDecodeTokensV1>,
        _runs: &[crate::palw_tir_court_v1::PalwTirStepRunDisclosureV1],
        _leaf_index: u64,
        _rules: &crate::palw_tir_court_v1::PalwTirCourtRulesV1,
        _limits: crate::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1,
        _copy_budget_bytes: u64,
    ) -> Result<PalwTirPublicConeProbeV1, String> {
        self.backend.probe_tir_public_cone_v1(_header, _prompt_token_ids, _trace, _runs, _leaf_index, _rules, _limits, _copy_budget_bytes)
    }
    pub fn build_tir_public_cone_bounded_v1(
        &self,
        header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        prompt_token_ids: &[u32],
        trace: Option<&crate::palw_step_refute::PalwTiledDecodeTokensV1>,
        runs: &[crate::palw_tir_court_v1::PalwTirStepRunDisclosureV1],
        leaf_index: u64,
        rules: &crate::palw_tir_court_v1::PalwTirCourtRulesV1,
        limits: crate::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1,
        copy_budget_bytes: u64,
    ) -> Result<(crate::palw_court_v2::PalwCourtVerdictProofV2, crate::palw_tir_court_v1::PalwTirBuilderMeterV1), String> {
        self.backend.build_tir_public_cone_bounded_v1(header, prompt_token_ids, trace, runs, leaf_index, rules, limits, copy_budget_bytes)
    }
    pub fn declare_tir_prefix_state_v1(
        &self,
        _material: &[u8],
        _header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
        _prefix_tokens: u32,
    ) -> Result<Hash64, String> {
        self.backend.declare_tir_prefix_state_v1(_material, _header, _prefix_tokens)
    }
    pub fn bisect_prefix_state(&self, _material: &[u8], _index: u64) -> Option<Hash64> {
        self.backend.bisect_prefix_state(_material, _index)
    }
    pub fn supports_court(&self) -> bool {
        self.backend.supports_court()
    }
    pub fn executor_conformance_job_v1(&self) -> Option<Box<dyn FnOnce() -> Result<String, String> + Send + 'static>> {
        self.backend.executor_conformance_job_v1()
    }
    pub fn attn_site_evidence(
        &self,
        _material: &[u8],
        _narrowed: u64,
        _carried_prompt: Option<&[u32]>,
        _accused_out_tile: Option<&crate::palw_attn_court_v1::PalwAttnRowOpeningV1>,
    ) -> Result<crate::palw_attn_responder_v1::PalwAttnSiteEvidenceV1, String> {
        self.backend.attn_site_evidence(_material, _narrowed, _carried_prompt, _accused_out_tile)
    }
    pub fn attn_site_evidence_from_filing(
        &self,
        _filing: &crate::palw_attn_responder_v1::PalwAttnAccusedFilingV1,
        _narrowed: u64,
        _carried_prompt: Option<&[u32]>,
    ) -> Result<crate::palw_attn_responder_v1::PalwAttnSiteEvidenceV1, String> {
        self.backend.attn_site_evidence_from_filing(_filing, _narrowed, _carried_prompt)
    }
    pub fn attn_site_evidence_held_v1(
        &self,
        _material: &[u8],
        _narrowed: u64,
        _carried_prompt: Option<&[u32]>,
        _filing: Option<&crate::palw_attn_responder_v1::PalwAttnHeldFilingV1>,
    ) -> Result<crate::palw_attn_responder_v1::PalwAttnHeldEvidenceV1, String> {
        self.backend.attn_site_evidence_held_v1(_material, _narrowed, _carried_prompt, _filing)
    }
    pub fn attn_held_cache_write_rows_v1(
        &self,
        _filing: &crate::palw_attn_responder_v1::PalwAttnHeldFilingV1,
        _narrowed: u64,
        _carried_prompt: Option<&[u32]>,
        _kind: crate::palw_state_chunk_map::PalwStateChunkKindV1,
        _attn_layer: u16,
        _history_position: u32,
    ) -> Result<Vec<crate::palw_attn_court_v1::PalwAttnRowOpeningV1>, String> {
        self.backend.attn_held_cache_write_rows_v1(_filing, _narrowed, _carried_prompt, _kind, _attn_layer, _history_position)
    }
    pub fn supports_dissection(&self) -> bool {
        self.backend.supports_dissection()
    }
    pub fn supports_dissection_with_memory_routing_v2(&self, _active: bool) -> bool {
        self.backend.supports_dissection_with_memory_routing_v2(_active)
    }
    pub fn has_fused_site(&self) -> bool {
        self.backend.has_fused_site()
    }
    pub fn refutation_for_index(
        &self,
        _material: &[u8],
        _index: u64,
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.backend.refutation_for_index(_material, _index)
    }
    pub fn refutation_for_free_prompt_index(
        &self,
        _material: &[u8],
        _index: u64,
        _prompt_token_ids: &[u32],
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.backend.refutation_for_free_prompt_index(_material, _index, _prompt_token_ids)
    }
    pub fn job_anchor_v1(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        class_id: Hash64,
        executor_bond: &crate::tx::TransactionOutpoint,
        nonce_bucket: u64,
    ) -> Option<Hash64> {
        self.backend.job_anchor_v1(network_domain, pre_pow_hash, class_id, executor_bond, nonce_bucket)
    }
    pub fn operand_openings_for(
        &self,
        _refutation: &crate::palw_step_refute::PalwExecutionStepRefutationV1,
    ) -> Result<Vec<crate::palw_artifact::PalwArtifactOpeningV1>, String> {
        self.backend.operand_openings_for(_refutation)
    }
    pub fn fp_recompute_checkpoint_root(
        &self,
        _job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        _prompt_token_ids: &[u32],
        _output_token_ids: &[u32],
        _covered: u32,
    ) -> Result<crate::Hash64, String> {
        self.backend.fp_recompute_checkpoint_root(_job, _prompt_token_ids, _output_token_ids, _covered)
    }
    pub fn fp_held_route_v1(&self, _window_receipt_daa: u64) -> Option<crate::palw_held_context_v1::PalwHeldSeatRouteV1> {
        self.backend.fp_held_route_v1(_window_receipt_daa)
    }
    pub fn fp_forget_seat_state_v1(&self) {
        self.backend.fp_forget_seat_state_v1()
    }
    pub fn fp_name_the_leaf_v1(
        &self,
        _opening: &[u8],
        _block_leaves: &[u8],
        _claim: PalwClaimRootsV1,
        _index: u32,
        _prompt_token_ids: &[u32],
        _generated_token_ids: &[u32],
        _work_leaves: u64,
    ) -> Result<Option<u64>, String> {
        self.backend.fp_name_the_leaf_v1(_opening, _block_leaves, _claim, _index, _prompt_token_ids, _generated_token_ids, _work_leaves)
    }
    pub fn fp_name_the_edge_leaf_v1(
        &self,
        _opening: &[u8],
        _served_edges: &[Vec<u8>],
        _claim: PalwClaimRootsV1,
        _index: u32,
        _prompt_token_ids: &[u32],
        _generated_token_ids: &[u32],
        _work_leaves: u64,
    ) -> Result<Option<u64>, String> {
        self.backend.fp_name_the_edge_leaf_v1(_opening, _served_edges, _claim, _index, _prompt_token_ids, _generated_token_ids, _work_leaves)
    }
    pub fn refutation_from_served_intervals(
        &self,
        _held: &[(u32, Vec<u8>)],
        _claim: PalwClaimRootsV1,
        _prompt_token_ids: &[u32],
        _generated_token_ids: &[u32],
        _work_leaves: u64,
        _leaf: u64,
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.backend.refutation_from_served_intervals(_held, _claim, _prompt_token_ids, _generated_token_ids, _work_leaves, _leaf)
    }
    pub fn fp_leaf_refutation_v1(
        &self,
        capture: &[u8],
        prompt_token_ids: &[u32],
        claim: PalwClaimRootsV1,
        work_leaves: u64,
        leaf: u64,
    ) -> Result<crate::palw_step_refute::PalwExecutionStepRefutationV1, String> {
        self.backend.fp_leaf_refutation_v1(capture, prompt_token_ids, claim, work_leaves, leaf)
    }
}

#[derive(Clone, Copy)]
pub struct PalwResourceOpsV1<'a> {
    backend: &'a dyn PalwExecutionBackendV1,
}
impl<'a> PalwResourceOpsV1<'a> {
    pub fn new(backend: &'a dyn PalwExecutionBackendV1) -> Self {
        Self { backend }
    }
    pub fn prefill_run_positions_v1(&self) -> Option<u32> {
        self.backend.prefill_run_positions_v1()
    }
    pub fn capture_shape(&self, _material: &[u8]) -> Option<PalwCaptureShapeV1> {
        self.backend.capture_shape(_material)
    }
    pub fn canonical_job_prefill_tokens(&self) -> Option<usize> {
        self.backend.canonical_job_prefill_tokens()
    }
    pub fn attempt_working_set_bytes(&self, _prefill_tokens: usize) -> Option<u64> {
        self.backend.attempt_working_set_bytes(_prefill_tokens)
    }
    pub fn runtime_profile_v1(&self) -> Option<crate::palw_resource_profile_v1::PalwRuntimeProfileV1> {
        self.backend.runtime_profile_v1()
    }
    pub fn artifact_derived_resident_bytes_v1(&self) -> u64 {
        self.backend.artifact_derived_resident_bytes_v1()
    }
    pub fn resource_profile_v1(
        &self,
        _job: Option<&PalwJobContextV2>,
        _role: crate::palw_resource_profile_v1::PalwResourceRoleV1,
    ) -> Option<crate::palw_resource_profile_v1::PalwResourceProfileV1> {
        self.backend.resource_profile_v1(_job, _role)
    }
    pub fn artifact_root_and_leaf_count(&self) -> Result<(crate::Hash64, u32), String> {
        self.backend.artifact_root_and_leaf_count()
    }
}
