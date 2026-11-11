//! Public material identities derived from the native cold TFP1, independently
//! of its companion. These helpers neither admit a claim nor activate a profile.

use crate::{
    Hash64,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_freeprompt_v3::{PALW_FP_V3_VERSION, PALW_FP_V4_VERSION, PalwFreePromptCommitmentV3, fp_job_id_v3},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_ids_form_id_v1},
    palw_tir_class_v1::{
        PALW_TIR_GRAPH_IR_ROOT_DOMAIN_V1, PALW_TIR_LAYOUT_DOMAIN_V1, PalwTirClassV1, palw_tir_class_id_from_digests_v1,
    },
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    public_one_honest::{
        ManifestEntryV1, ManifestScopeRules, tir_cold_dependencies_for_execution_v1, tir_cold_initial_state_for_execution_v1,
        tir_ordinary_tfp1_rules_material_entry_v1,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicTfp1ColdMaterialV1 {
    pub initial_state: (ManifestEntryV1, [u8; 128]),
    pub dependencies: (ManifestEntryV1, [u8; 68]),
}

/// A cold V3/V4 job has no tail and consumes genesis state. Other versions
/// may carry external dependencies and must use their own explicit relation.
pub(super) fn require_cold_tfp1_v1(fp: &PalwFreePromptCommitmentV3, job: &PalwJobContextV2) -> Result<(), ContractError> {
    if !matches!(fp.job.version, PALW_FP_V3_VERSION | PALW_FP_V4_VERSION)
        || fp.job.tail.is_some()
        || job.inherited_prefix_v1().is_some()
    {
        return Err(ContractError::UnsupportedProfile);
    }
    Ok(())
}

fn class_program_v1(class: &PalwTirClassV1, job: &PalwJobContextV2, artifact: Hash64) -> Result<Hash64, ContractError> {
    let graph = class.graph_ir_root();
    let layout = stream_digest(PALW_TIR_LAYOUT_DOMAIN_V1, &class.layout).map_err(|_| ContractError::NonCanonical)?;
    if palw_tir_class_id_from_digests_v1(class.version, &graph, &layout, &artifact, &class.tokenizer_id) != job.shape_profile_id
        || class.tokenizer_id != job.tokenizer_id
    {
        return Err(ContractError::Scope);
    }
    Ok(graph)
}

/// Fixed cold material from native job/class identities. Hashing program and
/// layout still consumes work under the caller's native or worker budget.
/// No full program/layout serialization buffer is allocated here.
pub fn public_tfp1_cold_material_v1(
    fp: &PalwFreePromptCommitmentV3,
    source: &PalwTirPublicSourceV1,
    class: &PalwTirClassV1,
    initial_rules: ManifestScopeRules,
    dependency_rules: ManifestScopeRules,
) -> Result<PublicTfp1ColdMaterialV1, ContractError> {
    super::tfp1_original::NativeTfp1PublicOriginalFactsV1::derive_v1(fp, source)?;
    let header = source.header_v1();
    let job = &header.job_context;
    require_cold_tfp1_v1(fp, job)?;
    let graph = class_program_v1(class, job, header.artifact_root)?;
    let execution = Digest(job.context_hash().as_bytes());
    Ok(PublicTfp1ColdMaterialV1 {
        initial_state: tir_cold_initial_state_for_execution_v1(Digest(graph.as_bytes()), execution, initial_rules)?,
        dependencies: tir_cold_dependencies_for_execution_v1(execution, dependency_rules)?,
    })
}

/// Preimage of the existing ordinary execution rules, including the full
/// native FP job identity (selector and stop configuration) and separately
/// named prompt form. The form must come from native class/network rules.
/// A registered program and its decoded value must agree. Tiled logits and
/// the generic context trace scheme are distinct and both are committed.
#[allow(clippy::too_many_arguments)]
pub fn public_ordinary_tfp1_rules_material_v1(
    fp: &PalwFreePromptCommitmentV3,
    source: &PalwTirPublicSourceV1,
    class: &PalwTirClassV1,
    program: &misaka_palw_tir::TirProgramV1,
    prompt_form: PalwPromptIdsFormV1,
    rules: ManifestScopeRules,
) -> Result<(ManifestEntryV1, [u8; 512]), ContractError> {
    ordinary_tfp1_rules_material_v1(fp, source, class, Some(program), prompt_form, rules)
}

/// A native Source already validated the registered program in
/// `PalwTirStepSpaceV1::new`, including the exact primitive-set ID, and
/// authenticated the legacy tiled logits scheme. Recheck the supplied
/// class's program and layout identity against that Source.
pub(super) fn native_ordinary_tfp1_rules_material_v1(
    fp: &PalwFreePromptCommitmentV3,
    source: &PalwTirPublicSourceV1,
    class: &PalwTirClassV1,
    prompt_form: PalwPromptIdsFormV1,
    rules: ManifestScopeRules,
) -> Result<(ManifestEntryV1, [u8; 512]), ContractError> {
    ordinary_tfp1_rules_material_v1(fp, source, class, None, prompt_form, rules)
}

fn ordinary_tfp1_rules_material_v1(
    fp: &PalwFreePromptCommitmentV3,
    source: &PalwTirPublicSourceV1,
    class: &PalwTirClassV1,
    program: Option<&misaka_palw_tir::TirProgramV1>,
    prompt_form: PalwPromptIdsFormV1,
    rules: ManifestScopeRules,
) -> Result<(ManifestEntryV1, [u8; 512]), ContractError> {
    super::tfp1_original::NativeTfp1PublicOriginalFactsV1::derive_v1(fp, source)?;
    let header = source.header_v1();
    let job = &header.job_context;
    require_cold_tfp1_v1(fp, job)?;
    let prim_set = misaka_palw_tir::prim::PRIM_SET_ID_V1;
    let logits_scheme = crate::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes();
    if !source.supports_tiled_trace_v1()
        || program.is_some_and(|program| program.logits_scheme_id != logits_scheme || program.prim_set_id != prim_set)
        || job.trace_scheme_id != crate::palw_v2::trace_scheme_id_v2()
    {
        return Err(ContractError::UnsupportedProfile);
    }
    let graph = class_program_v1(class, job, header.artifact_root)?;
    if let Some(program) = program {
        if stream_digest(PALW_TIR_GRAPH_IR_ROOT_DOMAIN_V1, program).map_err(|_| ContractError::NonCanonical)? != graph {
            return Err(ContractError::Scope);
        }
    }
    let mut bytes = [0; 512];
    for (part, field) in bytes.chunks_exact_mut(64).zip([
        graph.as_bytes(),
        prim_set,
        logits_scheme,
        job.trace_scheme_id.as_bytes(),
        job.cu_ruleset_id.as_bytes(),
        fp_job_id_v3(&fp.job).as_bytes(),
        job.context_hash().as_bytes(),
        prompt_ids_form_id_v1(prompt_form).as_bytes(),
    ]) {
        part.copy_from_slice(&field);
    }
    Ok((tir_ordinary_tfp1_rules_material_entry_v1(Digest(job.cu_ruleset_id.as_bytes()), &bytes, rules)?, bytes))
}
