//! Local correspondence check for a paired ordinary TFP1. It has no signer,
//! replay approval constructor or consensus authority.

use std::{mem::size_of, path::Path};

use super::{
    MaterialProvider, PanelSourceErrorR1, PublicTraceAcquisitionV1, check_public_input_token_entry_v1, check_public_prompt_v1,
    check_public_replay_trace_budget_v1, check_public_replay_trace_tiles_v1, checked_public_trace_entry_v1,
    fetch_public_replay_material, fetch_public_replay_output_v1, fetch_public_trace_index_v1, open_public_tir_program_v1,
};
use kaspa_consensus_core::{
    Hash64,
    palw_backend::{PalwExecutionBackendV1, PalwTirOwnStepSourceNeedV1},
    palw_fp_execution_v3::{palw_fp_job_pin_of_context_v1, palw_fp_job_pin_v1},
    palw_freeprompt_v3::{PALW_FP_V3_VERSION, PALW_FP_V4_VERSION, PalwFpStopReasonV3, PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_positive_contract_v2::{
        PublicTfp1HfArtifactV1, check_public_original_execution_v1, public_ordinary_tfp1_rules_material_v1,
        public_tfp1_cold_material_v1,
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_court_v1::PublicOriginalTraceIndexV1,
    palw_tir_objective_v1::original_stop_rule_fault_v1,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    public_one_honest::{
        CheckedManifestV1, ManifestEntryV1, ManifestScopeRules, MaterialScope, OriginalCommit, OriginalId, SignedPublicOriginalV1,
        hf_artifact_material_codec_v1,
    },
};
use misaka_palw_tir_exec::node::TirBackendV1;

const TRACE_HASH_PAGE_LEN: u64 = 1024;

/// Values resolved together from an authenticated accepted parent. The
/// adapter checks correspondence; this borrowed DTO does not establish
/// acceptance or replace the native pairing/link reader.
#[derive(Clone, Copy)]
pub struct LegacyTfp1ReplayContextV1<'a> {
    pub claim: Hash64,
    pub original: OriginalId,
    pub companion_carrier: Digest,
    pub source_id: Hash64,
    pub source: &'a PalwTirPublicSourceV1,
    pub fp: &'a PalwFreePromptCommitmentV3,
    pub prompt_form: PalwPromptIdsFormV1,
    pub literal_prompt: &'a [u8],
    /// Required for the HF descriptor codec, absent for the historical
    /// inventory codec. Resolved against the caller's authenticated parent.
    pub hf_artifact: Option<&'a PublicTfp1HfArtifactV1>,
}

#[derive(Clone, Copy)]
pub struct LegacyTfp1ReplayLimitsV1 {
    /// Sum of the checked public replay bodies other than Trace.
    pub max_input_output_bytes: u64,
    pub trace: PublicTraceAcquisitionV1,
}

/// Check the published program, prompt, tokenizer, output, execution rules,
/// cold state/dependencies and complete Trace against one independent legacy
/// tiled TFP1 replay. The backend must have
/// been loaded under the caller's existing artifact admission and identify
/// the registered inventory. The caller owns bounded acquisition and worker
/// admission and provides a guard covering both quoted replay RAM and
/// temporary disk before the replay opens its disk tree.
///
/// Success here only records local correspondence. Complete native relation
/// planning, acquisition and drawn approval are separate steps. No signing
/// entry point accepts this result.
#[allow(clippy::too_many_arguments)]
pub fn check_legacy_tfp1_public_replay_v1<ReplayLease>(
    provider: &dyn MaterialProvider,
    context: LegacyTfp1ReplayContextV1<'_>,
    signed: &SignedPublicOriginalV1,
    manifest: &CheckedManifestV1,
    backend: &TirBackendV1,
    cache_directory: &Path,
    limits: LegacyTfp1ReplayLimitsV1,
    reserve_replay: impl FnOnce(PalwTirOwnStepSourceNeedV1) -> Result<ReplayLease, String>,
) -> Result<(), PanelSourceErrorR1> {
    check_legacy_tfp1_pairing_v1(context, signed, manifest, backend)?;
    let original = &signed.original;
    let fp = context.fp;
    let source = context.source;
    let header = source.header_v1();
    let scopes = [
        MaterialScope::Program,
        MaterialScope::Prompt,
        MaterialScope::Tokenizer,
        MaterialScope::Output,
        MaterialScope::ExecutionRules,
        MaterialScope::InitialState,
        MaterialScope::Dependencies,
    ];
    let descriptor_bytes = context.hf_artifact.map_or(0, |material| material.entry_v1().total_bytes);
    let total = scopes.into_iter().try_fold(descriptor_bytes, |total, scope| total.checked_add(manifest.entry(scope).total_bytes));
    if total.is_none_or(|total| total > limits.max_input_output_bytes) {
        return Err(PanelSourceErrorR1::Budget);
    }
    let trace = checked_public_trace_entry_v1(original, manifest)?;
    check_public_replay_trace_budget_v1(source.leaf_count_v1(), trace.total_bytes, limits.trace)?;

    if let Some(material) = context.hf_artifact {
        // The Artifact body is only a bounded reference, never full weights.
        // Exact canonical bytes authenticate every recipe/reference field.
        let bytes = material.descriptor_v1().encode_v1()?;
        check_replay_material_body_v1(provider, original, manifest, MaterialScope::Artifact, &material.entry_v1(), &bytes)?;
    }

    backend.artifact().check_weights_unmodified().map_err(PanelSourceErrorR1::Artifact)?;
    let (inventory_root, _) = backend.artifact().inventory_root().map_err(PanelSourceErrorR1::Artifact)?;
    if inventory_root != header.artifact_root {
        return Err(PanelSourceErrorR1::Binding);
    }

    let program =
        open_public_tir_program_v1(manifest, &fetch_public_replay_material(provider, original, manifest, MaterialScope::Program)?)?;
    if program != backend.space().program {
        return Err(PanelSourceErrorR1::Binding);
    }
    let scope = MaterialScope::ExecutionRules;
    let (rules_entry, rules_bytes) = public_ordinary_tfp1_rules_material_v1(
        fp,
        source,
        backend.class(),
        &program,
        context.prompt_form,
        material_rules_v1(manifest, scope),
    )?;
    check_replay_material_body_v1(provider, original, manifest, scope, &rules_entry, &rules_bytes)?;
    drop(program);
    let cold = public_tfp1_cold_material_v1(
        fp,
        source,
        backend.class(),
        material_rules_v1(manifest, MaterialScope::InitialState),
        material_rules_v1(manifest, MaterialScope::Dependencies),
    )?;
    for (scope, entry, bytes) in [
        (MaterialScope::InitialState, &cold.initial_state.0, cold.initial_state.1.as_slice()),
        (MaterialScope::Dependencies, &cold.dependencies.0, cold.dependencies.1.as_slice()),
    ] {
        check_replay_material_body_v1(provider, original, manifest, scope, entry, bytes)?;
    }
    let prompt = fetch_public_replay_material(provider, original, manifest, MaterialScope::Prompt)?;
    if prompt.as_slice() != context.literal_prompt {
        return Err(PanelSourceErrorR1::Binding);
    }
    let tokenizer = fetch_public_replay_material(provider, original, manifest, MaterialScope::Tokenizer)?;
    let ids = check_public_prompt_v1(original, manifest, context.prompt_form, &prompt, &tokenizer)?;
    drop((prompt, tokenizer));
    let output = fetch_public_replay_output_v1(provider, original, manifest)?;
    let canonical_stop = if fp.decode_tokens_executed == fp.job.decode_token_limit {
        PalwFpStopReasonV3::ExactBudgetReached
    } else {
        PalwFpStopReasonV3::EndOfGeneration
    };
    if fp.stop_reason != canonical_stop || output.iter().any(|id| *id >= source.vocab_v1()) {
        return Err(PanelSourceErrorR1::Binding);
    }
    if let Some(why) = original_stop_rule_fault_v1(&fp.job, source.vocab_v1(), &output) {
        return Err(PanelSourceErrorR1::ReplayMismatch(why.into()));
    }
    let published = fetch_public_trace_index_v1(provider, original)?;
    if published.full_logits_trace_root != header.full_logits_trace_root
        || published.step_leaf_count != header.step_leaf_count
        || published.step_merkle_root != header.step_merkle_root
    {
        return Err(PanelSourceErrorR1::Binding);
    }

    let mut need = backend.tir_own_step_source_need_v1(header, &fp.job, &ids).map_err(PanelSourceErrorR1::ReplayExecution)?;
    if need.leaf_count != source.leaf_count_v1() {
        return Err(PanelSourceErrorR1::Binding);
    }
    // Acquiring/parsing the provider bodies uses the caller's separate budget.
    // This lease includes the decoded vectors and both bounded hash pages
    // that can coexist with the replay's own working/disk source reservation.
    let retained_ids = ids
        .capacity()
        .checked_add(output.capacity())
        .and_then(|capacity| capacity.checked_mul(size_of::<u32>()))
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or(PanelSourceErrorR1::Budget)?;
    let hash_page_bytes = need.leaf_count.min(TRACE_HASH_PAGE_LEN) * 2 * size_of::<Hash64>() as u64;
    need.resident_ram_bytes = need
        .resident_ram_bytes
        .checked_add(retained_ids)
        .and_then(|bytes| bytes.checked_add(hash_page_bytes))
        .ok_or(PanelSourceErrorR1::Budget)?;
    let _lease = reserve_replay(need).map_err(PanelSourceErrorR1::ReplayExecution)?;
    let own =
        backend.open_tir_own_step_source_v1(header, &fp.job, &ids, cache_directory).map_err(PanelSourceErrorR1::ReplayExecution)?;
    let replay = own.info_v1();
    if replay.header != *header || replay.output_root != fp.output_root || replay.generated_ids != output {
        return Err(PanelSourceErrorR1::ReplayMismatch("independent replay differs from the accepted TFP1".into()));
    }

    let max_chunk = usize::try_from(u64::from(trace.chunk_bytes).min(source.widest_leaf_value_bytes_v1()))
        .map_err(|_| PanelSourceErrorR1::Budget)?;
    // Read from the bounded disk tree in ranges instead of copying its full
    // leaf vector. The native source allows at most 1024 raw hashes per read.
    let mut hashes = Vec::new();
    let mut first = 0;
    check_public_replay_trace_tiles_v1(provider, original, &published, trace.total_bytes, max_chunk, limits.trace, |index| {
        if hashes.is_empty() || index - first >= hashes.len() as u64 {
            first = index;
            let count = (published.step_leaf_count - index).min(TRACE_HASH_PAGE_LEN) as u32;
            hashes = own.leaf_hashes_v1(first, count).map_err(PanelSourceErrorR1::ReplayExecution)?;
        }
        hashes.get((index - first) as usize).copied().ok_or(PanelSourceErrorR1::Binding)
    })
}

fn material_rules_v1(manifest: &CheckedManifestV1, scope: MaterialScope) -> ManifestScopeRules {
    let entry = manifest.entry(scope);
    ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes }
}

/// Publish only the signed HF reference, not checkpoint weights. The caller
/// must reserve bounded I/O and retained storage through the claim horizon
/// before encoding/writing; retention must outlive this temporary I/O lease.
/// A successful local write is neither native acceptance nor HF availability.
#[allow(clippy::too_many_arguments)]
pub fn publish_legacy_tfp1_hf_artifact_v1<WriteLease>(
    context: LegacyTfp1ReplayContextV1<'_>,
    signed: &SignedPublicOriginalV1,
    manifest: &CheckedManifestV1,
    backend: &TirBackendV1,
    provider_root: &Path,
    max_bytes: u64,
    reserve_write: impl FnOnce(OriginalId, u64) -> Result<WriteLease, String>,
) -> Result<(), PanelSourceErrorR1> {
    check_legacy_tfp1_pairing_v1(context, signed, manifest, backend)?;
    let material = context.hf_artifact.ok_or(PanelSourceErrorR1::Binding)?;
    let size = material.entry_v1().total_bytes;
    if size > max_bytes {
        return Err(PanelSourceErrorR1::Budget);
    }
    let _lease = reserve_write(context.original, size).map_err(PanelSourceErrorR1::ReplayProvider)?;
    let bytes = material.descriptor_v1().encode_v1()?;
    let path = crate::public_material::public_original_replay_path(context.original, MaterialScope::Artifact)
        .ok_or(ContractError::UnsupportedProfile)?;
    crate::evidence::fs::write_provider_chunk(&provider_root.join(path), &bytes)
        .map_err(|error| PanelSourceErrorR1::ReplayProvider(error.to_string()))
}

fn check_replay_material_body_v1(
    provider: &dyn MaterialProvider,
    original: &OriginalCommit,
    manifest: &CheckedManifestV1,
    scope: MaterialScope,
    expected: &ManifestEntryV1,
    expected_bytes: &[u8],
) -> Result<(), PanelSourceErrorR1> {
    if manifest.entry(scope) != expected
        || fetch_public_replay_material(provider, original, manifest, scope)?.as_slice() != expected_bytes
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    Ok(())
}

/// Bind both local consumers to the same signed companion, FP and registered Source.
pub(super) fn check_legacy_tfp1_pairing_v1(
    context: LegacyTfp1ReplayContextV1<'_>,
    signed: &SignedPublicOriginalV1,
    manifest: &CheckedManifestV1,
    backend: &TirBackendV1,
) -> Result<(), PanelSourceErrorR1> {
    let original = &signed.original;
    let fp = context.fp;
    let source = context.source;
    let header = source.header_v1();
    let job = &header.job_context;
    if context.original != original.id()
        || SignedPublicOriginalV1::carrier_digest(original, &signed.key, &signed.signature) != context.companion_carrier
        || manifest.original() != context.original
        || context.source_id != source.source_id_v1()
        || context.claim != fp_claim_id_v3(fp)
        || fp.job.executor_pubkey.as_slice() != signed.key.as_slice()
        || fp.job.class_id != job.shape_profile_id
        || palw_fp_job_pin_v1(fp) != palw_fp_job_pin_of_context_v1(job)
        || fp.execution_root != header.committed_execution_root
        || fp.trace_root != header.full_logits_trace_root
        || fp.decode_tokens_executed != source.decode_count_v1()
        || fp.work_leaves != source.leaf_count_v1()
        || original.trace_root != Digest(header.committed_execution_root.as_bytes())
        || original.output != Digest(fp.output_root.as_bytes())
        || backend.class_id() != job.shape_profile_id
        || backend.artifact_root() != header.artifact_root
    {
        return Err(PanelSourceErrorR1::Binding);
    }
    // The FP context carries the generic trace-context scheme. The native
    // logits scheme is resolved from the registered TIR program and Source.
    if Hash64::from_bytes(backend.space().program.logits_scheme_id) != tiled_logits_scheme_id_v1()
        || !source.supports_tiled_trace_v1()
        || !matches!(fp.job.version, PALW_FP_V3_VERSION | PALW_FP_V4_VERSION)
        || fp.job.tail.is_some()
        || job.inherited_prefix_v1().is_some()
        || backend.artifact().composite_ref().is_some()
    {
        return Err(ContractError::UnsupportedProfile.into());
    }
    original.authenticate(&signed.key, &signed.signature)?;
    check_public_original_execution_v1(original, job, backend.class(), header.artifact_root)?;
    let artifact = manifest.entry(MaterialScope::Artifact);
    match (artifact.codec == hf_artifact_material_codec_v1(), context.hf_artifact) {
        (true, Some(material)) => {
            material.check_context_v1(fp, source, backend.class())?;
            if *artifact != material.entry_v1() {
                return Err(PanelSourceErrorR1::Binding);
            }
        }
        (false, None) => {}
        _ => return Err(PanelSourceErrorR1::Binding),
    }
    check_public_input_token_entry_v1(original, manifest, context.prompt_form)?;
    manifest.open_prompt_text(context.literal_prompt).map_err(|_| PanelSourceErrorR1::Binding)?;
    Ok(())
}
