//! Read-only operand costs before registration. These are the exact floors
//! reused by admission, not a capacity declaration or a complete native quote.
//! Signature/pass/settlement overhead and the whole causal graph remain owned
//! by the enclosing private planning contract. No accepted state is fabricated.
use super::{
    admission::{plain_selection::PlainSelectionBudgetV3, plain_tokenizer::PlainTokenizerResourceFloorV2},
    plain_output::PlainBatchOutputV2,
    plain_tokenizer::PlainBatchTokenizerBoundsV2,
    selection::budget::SelectionTerminalBoundsV2,
};
use crate::{
    Hash64,
    palw_class_semantics_v1::PalwClassSemanticsDeclarationV1,
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_admission_v1::PalwTirClassRecordV1,
    palw_tir_class_v1::PalwTirLayoutV1,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
    palw_tokenizer_rendering_v2::{PalwDerivedTokenizerRenderingV2, PalwRootedTokenizerOpeningBoundsV1},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ProtectedOperandFloorsV1 {
    pub claim_id: Hash64,
    pub source_id: Hash64,
    pub full_ids_body_bytes: u64,
    pub input_ids_body_bytes: u64,
    pub tokenizer_opening_material_bytes: u64,
    pub tokenizer_work: u64,
    pub tokenizer_io: u64,
    pub tokenizer_workspace_bytes: u64,
    pub selection_work: u64,
    pub selection_io: u64,
    pub selection_workspace_bytes: u64,
    pub selection_signed_control_bytes: u64,
    pub selection_archive_cause_bytes: u64,
    pub selection_proof_bytes: u64,
}

/// The caller derives `facts` from the exact tokenizer bytes using the native
/// parser. Identity/shape checks here prevent mixed inputs; no registration or
/// accepted claim follows from this result. In particular this API cannot
/// replace admission's authenticated-parent lookup or select a service budget.
pub fn measure_protected_operand_floors_v1(
    original: &PalwFreePromptCommitmentV3,
    source: &PalwTirPublicSourceV1,
    record: &PalwTirClassRecordV1,
    layout: &PalwTirLayoutV1,
    semantics: &PalwClassSemanticsDeclarationV1,
    facts: &PalwDerivedTokenizerRenderingV2,
    prompt_form: PalwPromptIdsFormV1,
) -> Result<ProtectedOperandFloorsV1, String> {
    semantics.check()?;
    let header = source.header_v1();
    let _identity_scratch =
        super::plain_tokenizer::reserve_original_hash_writers_v2(original, &header.job_context).map_err(|e| e.to_string())?;
    let statement = facts.statement;
    if semantics.class_id != original.job.class_id
        || semantics.artifact_root != header.artifact_root
        || semantics.model_profile_id != header.job_context.model_profile_id
        || semantics.runtime_manifest_hash != header.job_context.runtime_manifest_hash
        || semantics.runtime_class_id != header.job_context.runtime_class_id
        || semantics.cu_ruleset_id != header.job_context.cu_ruleset_id
        || semantics.tokenizer_id != original.job.tokenizer_id
        || semantics.tokenizer_id != statement.tokenizer_id
        || semantics.token_table_root != statement.table_root
        || semantics.rendering_rule != statement.rule
        || semantics.raw_tokenizer_bytes != statement.exact_raw_bytes
        || semantics.tokenizer_key_events < facts.key_events
        || record.logits_vocab != statement.vocab
        || statement.vocab == 0
        || facts.eog_count == 0
        || facts.eog_count > statement.vocab
        || facts.lowest_eog_id >= statement.vocab
        || facts.maximum_rendering_bytes > facts.total_rendering_bytes
        || facts.maximum_rendering_bytes > u64::from(u32::MAX)
        || original.job.prompt_tokens == 0
        || original.job.prompt_tokens != header.job_context.declared_prefill_tokens
        || original.job.prompt_token_ids_hash != header.job_context.prompt_token_ids_hash
    {
        return Err("protected operand measurement mixes class, tokenizer or original inputs".into());
    }
    let full_ids = PlainBatchOutputV2::full_ids_bound_v2(original, source).map_err(|e| format!("original operands: {e:?}"))?;
    let selection = SelectionTerminalBoundsV2::derive_v2(original, source, record, layout).map_err(|e| e.to_string())?;
    let (selection_work, selection_io, selection_workspace_bytes) =
        PlainSelectionBudgetV3::service_floors_v3(selection).map_err(|e| format!("Selection floor: {e:?}"))?;
    // The sizing helper does not consume registration identity. Deliberately
    // use no identity instead of manufacturing an accepted registration row.
    let openings = PalwRootedTokenizerOpeningBoundsV1::from_facts(Hash64::default(), facts);
    let bounds = PlainBatchTokenizerBoundsV2::from_operands_v2(original, prompt_form, full_ids, facts, openings)?;
    let tokenizer = PlainTokenizerResourceFloorV2::derive_v2(bounds).map_err(|e| format!("tokenizer floor: {e:?}"))?;
    Ok(ProtectedOperandFloorsV1 {
        claim_id: fp_claim_id_v3(original),
        source_id: source.source_id_v1(),
        full_ids_body_bytes: full_ids,
        input_ids_body_bytes: bounds.input_ids_body_bytes,
        tokenizer_opening_material_bytes: bounds.opening_material_bytes,
        tokenizer_work: tokenizer.work,
        tokenizer_io: tokenizer.io,
        tokenizer_workspace_bytes: tokenizer.workspace_bytes,
        selection_work,
        selection_io,
        selection_workspace_bytes,
        selection_signed_control_bytes: selection.signed_control_bytes,
        selection_archive_cause_bytes: selection.archive_cause_bytes,
        selection_proof_bytes: selection.proof_bytes,
    })
}

#[cfg(test)]
pub(crate) fn compare_registered_operands_v1(
    original: &PalwFreePromptCommitmentV3,
    source: &PalwTirPublicSourceV1,
    record: &PalwTirClassRecordV1,
    layout: &PalwTirLayoutV1,
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    facts: &PalwDerivedTokenizerRenderingV2,
    registered: &super::plain_tokenizer::PlainBatchTokenizerV2,
) {
    let declaration = PalwClassSemanticsDeclarationV1 {
        version: semantics.version,
        class_id: semantics.class_id,
        artifact_root: semantics.artifact_root,
        model_profile_id: semantics.model_profile_id,
        runtime_manifest_hash: semantics.runtime_manifest_hash,
        runtime_class_id: semantics.runtime_class_id,
        tokenizer_id: semantics.tokenizer_id,
        cu_ruleset_id: semantics.cu_ruleset_id,
        token_table_root: semantics.token_table_root.unwrap(),
        rendering_rule: semantics.rendering_rule.unwrap(),
        raw_tokenizer_bytes: semantics.raw_tokenizer_bytes.unwrap(),
        tokenizer_key_events: semantics.tokenizer_key_events.unwrap(),
        prefix_state_relation: semantics.prefix_state_relation,
    };
    let form = PalwPromptIdsFormV1::Flat;
    let measured = measure_protected_operand_floors_v1(original, source, record, layout, &declaration, facts, form).unwrap();
    let bounds = registered.bounds_v2();
    let native = PlainTokenizerResourceFloorV2::derive_v2(bounds).unwrap();
    assert_eq!(measured.claim_id, registered.claim_id_v2());
    assert_eq!(measured.source_id, registered.source_id_v2());
    assert_eq!(measured.full_ids_body_bytes, bounds.full_ids_body_bytes);
    assert_eq!(measured.input_ids_body_bytes, bounds.input_ids_body_bytes);
    assert_eq!(measured.tokenizer_opening_material_bytes, bounds.opening_material_bytes);
    assert_eq!(
        (measured.tokenizer_work, measured.tokenizer_io, measured.tokenizer_workspace_bytes),
        (native.work, native.io, native.workspace_bytes)
    );
    let selection = SelectionTerminalBoundsV2::derive_v2(original, source, record, layout).unwrap();
    assert_eq!(
        (measured.selection_work, measured.selection_io, measured.selection_workspace_bytes),
        PlainSelectionBudgetV3::service_floors_v3(selection).unwrap()
    );
    let mut foreign = facts.clone();
    foreign.statement.table_root = Hash64::default();
    assert!(measure_protected_operand_floors_v1(original, source, record, layout, &declaration, &foreign, form).is_err());
    foreign = facts.clone();
    foreign.maximum_rendering_bytes = u64::MAX;
    assert!(measure_protected_operand_floors_v1(original, source, record, layout, &declaration, &foreign, form).is_err());
    let mut foreign = original.clone();
    foreign.execution_root = Hash64::default();
    assert!(measure_protected_operand_floors_v1(&foreign, source, record, layout, &declaration, facts, form).is_err());
}
