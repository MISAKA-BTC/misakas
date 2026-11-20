//! Publisher-side material for the ordinary protected TFP1 route. This checks
//! data identities, not execution correctness or Panel approval. A folded
//! capture needs reconstruction before it can supply this complete inventory.
use super::backend::{TirCaptureV1, tir_authenticate_capture_metadata_v1};
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_backend::{PalwTirPublicCaptureReaderV1, PalwTirPublicCaptureWorkV1},
    palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1,
    palw_positive_contract_v2::{public_ordinary_tfp1_rules_material_v1, public_tfp1_cold_material_v1},
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, prompt_ids_form_id_v1},
    palw_protected_original_quote_v1::ProtectedOriginalQuoteMaterialV1,
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_step_leg::{step_merkle_root_capped_v1, step_tile_leaf_hash_v1},
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_court_v1::PublicOriginalTraceIndexV1,
    palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicResponseV1, PalwTirPublicSourceV1},
    palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1,
};
use misaka_palw_positive_contract::{identity::Digest, public_one_honest::*};

/// Publishing the public prompt must not widen a Panel-only job's disclosure.
/// Call this before opening output files or reconstructing a streamed trace.
pub fn require_public_original_input_v1(fp: &PalwFreePromptCommitmentV3) -> Result<(), String> {
    if fp.job.privacy_mode != kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PRIVACY_PUBLIC_DA {
        return Err("protected public material requires a PublicDa job".into());
    }
    Ok(())
}

/// Small generated preimages beside the caller's retained capture, literal
/// prompt, raw tokenizer and descriptor. Program, input and output bytes are
/// the capture's original vectors; trace is index followed by leaf values.
pub struct ProtectedOriginalPublicationV1 {
    pub quote_material: ProtectedOriginalQuoteMaterialV1,
    pub header: PalwPublicTirBindingHeaderV1,
    pub execution_rules: [u8; 512],
    pub initial_state: [u8; 128],
    pub dependencies: [u8; 68],
    pub trace_index: PublicOriginalTraceIndexV1,
}

/// Local reader reconstruction limits, unrelated to native reservations or
/// fees. Opening authentication and output IO are separately bounded by the
/// Source's leaf geometry; these counters are the reader's quoted work only.
pub struct ProtectedTraceWriteLimitsV1 {
    pub maximum_resident_bytes: u64,
    pub maximum_slices: u64,
    pub maximum_work: PalwTirPublicCaptureWorkV1,
}

/// Obtained only after every canonical leaf range reached the original root
/// and its raw values were written. This is data identity, not Panel approval.
pub struct PublishedOriginalTraceV1 {
    source: Hash64,
    value_bytes: u64,
}

fn work_fits(work: PalwTirPublicCaptureWorkV1, cap: PalwTirPublicCaptureWorkV1) -> bool {
    work.io_bytes <= cap.io_bytes
        && work.hash_bytes <= cap.hash_bytes
        && work.tree_hashes <= cap.tree_hashes
        && work.replay_leaves <= cap.replay_leaves
        && work.replay_positions <= cap.replay_positions
        && work.replay_id_words <= cap.replay_id_words
        && work.arithmetic_work <= cap.arithmetic_work
        && work.traffic_bytes <= cap.traffic_bytes
}

/// Reuse the ordinary retained-capture reader: folded input is reconstructed
/// by bounded replay slices. Authenticate each returned range before writing;
/// never collect a dense trace in RAM. Err leaves an incomplete output only.
/// The caller checks the backend's RAM/disk quote before opening the reader.
pub fn write_protected_original_trace_v1(
    source: &PalwTirPublicSourceV1,
    reader: &mut dyn PalwTirPublicCaptureReaderV1,
    out: &mut impl std::io::Write,
    limits: &ProtectedTraceWriteLimitsV1,
) -> Result<PublishedOriginalTraceV1, String> {
    let header = source.header_v1();
    let expected_bytes = source.trace_value_bytes_v1().ok_or("trace geometry is unavailable")?;
    borsh::to_writer(
        &mut *out,
        &PublicOriginalTraceIndexV1 {
            full_logits_trace_root: header.full_logits_trace_root,
            step_leaf_count: header.step_leaf_count,
            step_merkle_root: header.step_merkle_root,
        },
    )
    .map_err(|e| e.to_string())?;
    let mut work = PalwTirPublicCaptureWorkV1::default();
    let (mut first, mut slices, mut value_bytes) = (0u64, 0u64, 0u64);
    while first < header.step_leaf_count {
        let count = (header.step_leaf_count - first).min(256) as u32;
        let request = PalwTirPublicRequestV1::StepRun { first, count };
        let answer = loop {
            let quote = reader.slice_quote_v1(&request)?;
            slices = slices.checked_add(1).ok_or("publisher slice count overflow")?;
            work = work.checked_add_v1(quote.work)?;
            if quote.source_id != source.source_id_v1()
                || quote.resident_ram_bytes > limits.maximum_resident_bytes
                || slices > limits.maximum_slices
                || !work_fits(work, limits.maximum_work)
            {
                return Err("protected trace reconstruction exceeds local budget or names another source".into());
            }
            if let Some(answer) = reader.poll_quoted_v1(&request, &quote)? {
                break answer;
            }
        };
        source.authenticate_v1(&request, &answer, None)?;
        let PalwTirPublicResponseV1::StepRun { preimages, .. } = answer else {
            return Err("unexpected trace range response".into());
        };
        for leaf in preimages {
            value_bytes = value_bytes
                .checked_add(leaf.values_le.len() as u64)
                .filter(|n| *n <= expected_bytes)
                .ok_or("trace exceeds native byte geometry")?;
            out.write_all(&leaf.values_le).map_err(|e| e.to_string())?;
        }
        first += u64::from(count);
    }
    if value_bytes != expected_bytes {
        return Err("trace omits native leaf values".into());
    }
    out.flush().map_err(|e| e.to_string())?;
    Ok(PublishedOriginalTraceV1 { source: source.source_id_v1(), value_bytes })
}

/// The caller bounds capture decoding and must separately check that its
/// tokenizer encodes the literal prompt to these input IDs. Native admission
/// independently checks registered container pins and the same material sizes.
#[allow(clippy::too_many_arguments)]
pub fn prepare_protected_original_material_v1(
    fp: &PalwFreePromptCommitmentV3,
    capture: &TirCaptureV1,
    prompt: &str,
    raw_tokenizer: &[u8],
    descriptor_bytes: &[u8],
    form: PalwPromptIdsFormV1,
    maximum_leaves: u64,
) -> Result<ProtectedOriginalPublicationV1, String> {
    prepare_material(
        fp,
        capture,
        PromptMaterialEncodingV1::LiteralUtf8,
        prompt,
        raw_tokenizer,
        descriptor_bytes,
        form,
        maximum_leaves,
        None,
    )
}

/// Same manifest assembly after streaming the complete trace. The private
/// receipt must name this exact Source. It grants no claim inclusion or
/// approval; all original/metadata relations are checked again below.
#[allow(clippy::too_many_arguments)]
pub fn prepare_protected_original_material_from_stream_v1(
    fp: &PalwFreePromptCommitmentV3,
    capture: &TirCaptureV1,
    prompt: &str,
    raw_tokenizer: &[u8],
    descriptor_bytes: &[u8],
    form: PalwPromptIdsFormV1,
    maximum_leaves: u64,
    trace: &PublishedOriginalTraceV1,
) -> Result<ProtectedOriginalPublicationV1, String> {
    prepare_material(
        fp,
        capture,
        PromptMaterialEncodingV1::LiteralUtf8,
        prompt,
        raw_tokenizer,
        descriptor_bytes,
        form,
        maximum_leaves,
        Some(trace),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_protected_original_material_with_prompt_v2(
    fp: &PalwFreePromptCommitmentV3,
    capture: &TirCaptureV1,
    encoding: PromptMaterialEncodingV1,
    prompt: &str,
    raw_tokenizer: &[u8],
    descriptor_bytes: &[u8],
    form: PalwPromptIdsFormV1,
    maximum_leaves: u64,
    trace: Option<&PublishedOriginalTraceV1>,
) -> Result<ProtectedOriginalPublicationV1, String> {
    prepare_material(fp, capture, encoding, prompt, raw_tokenizer, descriptor_bytes, form, maximum_leaves, trace)
}

#[allow(clippy::too_many_arguments)]
fn prepare_material(
    fp: &PalwFreePromptCommitmentV3,
    capture: &TirCaptureV1,
    prompt_encoding: PromptMaterialEncodingV1,
    prompt: &str,
    raw_tokenizer: &[u8],
    descriptor_bytes: &[u8],
    form: PalwPromptIdsFormV1,
    maximum_leaves: u64,
    trace: Option<&PublishedOriginalTraceV1>,
) -> Result<ProtectedOriginalPublicationV1, String> {
    require_public_original_input_v1(fp)?;
    let binding = &capture.binding;
    let class = &binding.class;
    if binding.step_leaf_count == 0
        || binding.step_leaf_count > maximum_leaves
        || (trace.is_none() && capture.leaves.len() as u64 != binding.step_leaf_count)
    {
        return Err("protected publisher needs the complete retained trace within its leaf budget".into());
    }
    tir_authenticate_capture_metadata_v1(capture, fp.job.class_id, form, maximum_leaves)?;
    if palw_tokenizer_raw_commitment_v1(raw_tokenizer) != class.tokenizer_id
        || palw_attempt_output_root_v1(&binding.job_context, &capture.generated) != fp.output_root
    {
        return Err("protected publisher tokenizer or output differs from the original".into());
    }
    let header = PalwPublicTirBindingHeaderV1::of_binding_v1(binding);
    let (registered, program) = palw_tir_class_record_v1(class, &binding.artifact_root).map_err(|e| e.to_string())?;
    let source = PalwTirPublicSourceV1::new_v1(&registered, &class.layout, &header, fp.execution_root, fp.trace_root, maximum_leaves)?;
    let trace_values = if let Some(trace) = trace {
        if trace.source != source.source_id_v1() {
            return Err("published trace belongs to another Source".into());
        }
        trace.value_bytes
    } else {
        let ctx = binding.job_context.context_hash();
        let hashes: Vec<Hash64> = capture.leaves.iter().map(|leaf| step_tile_leaf_hash_v1(&ctx, &fp.job.class_id, leaf)).collect();
        if step_merkle_root_capped_v1(&hashes, maximum_leaves).map_err(|e| e.to_string())? != binding.step_merkle_root {
            return Err("retained leaf values differ from the committed trace".into());
        }
        drop(hashes);
        capture
            .leaves
            .iter()
            .try_fold(0u64, |sum, leaf| sum.checked_add(leaf.values_le.len() as u64))
            .ok_or("trace byte count overflow")?
    };
    if source.trace_value_bytes_v1() != Some(trace_values) {
        return Err("retained trace size differs from native geometry".into());
    }
    let descriptor = HfArtifactDescriptorV1::decode_v1(descriptor_bytes).map_err(|e| format!("HF descriptor: {e:?}"))?;
    let pins = descriptor.native_pins_v1();
    if pins.class != Digest(fp.job.class_id.as_bytes())
        || pins.program != Digest(class.graph_ir_root().as_bytes())
        || pins.tokenizer != Digest(class.tokenizer_id.as_bytes())
        || pins.inventory != Digest(binding.artifact_root.as_bytes())
    {
        return Err("HF descriptor differs from the retained class".into());
    }
    let whole = |codec| ManifestScopeRules { codec, max_bytes: u64::from(u32::MAX), chunk_bytes: u32::MAX };
    let fixed = |codec, n: u32| ManifestScopeRules { codec, max_bytes: u64::from(n), chunk_bytes: n };
    let entry = |subject: Hash64, codec: Digest, size: usize, chunk_bytes: u32| -> Result<ManifestEntryV1, String> {
        if size == 0 || size as u64 > u64::from(u32::MAX) {
            return Err("material size exceeds publisher bound".into());
        }
        Ok(ManifestEntryV1 {
            subject: Digest(subject.as_bytes()),
            content_root: Digest(subject.as_bytes()),
            codec,
            total_bytes: size as u64,
            chunk_bytes,
        })
    };
    let cold = public_tfp1_cold_material_v1(
        fp,
        &source,
        class,
        fixed(tir_cold_initial_state_codec_v1(), 128),
        fixed(tir_cold_dependencies_codec_v1(), 68),
    )
    .map_err(|e| format!("cold material: {e:?}"))?;
    let rules = public_ordinary_tfp1_rules_material_v1(
        fp,
        &source,
        class,
        &program,
        form,
        fixed(tir_ordinary_tfp1_rules_material_codec_v1(), 512),
    )
    .map_err(|e| format!("execution rules: {e:?}"))?;
    let prompt_entry = prompt_encoding
        .material_v1(
            prompt_encoding.ruleset_v1(Digest(palw_g14_funded_original_schedule_estimate_v1().fee_profile_id().as_bytes())),
            prompt,
            whole(prompt_encoding.codec_v1()),
        )
        .map_err(|e| format!("literal prompt: {e:?}"))?;
    let input_chunk = if form == PalwPromptIdsFormV1::MerkleV1 { PALW_PROMPT_IDS_TILE_LEN * 4 } else { u32::MAX };
    let trace_bytes = trace_values
        .checked_add(PublicOriginalTraceIndexV1::WIRE_BYTES as u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("trace size overflow")?;
    let word_bytes = |n: usize| n.checked_mul(4).ok_or("token byte count overflow".to_string());
    let entries = [
        descriptor
            .manifest_entry_v1(fixed(hf_artifact_material_codec_v1(), HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u32))
            .map_err(|e| format!("artifact entry: {e:?}"))?,
        entry(class.graph_ir_root(), tir_program_material_codec_v1(), class.program.len(), u32::MAX)?,
        prompt_entry,
        entry(
            binding.job_context.prompt_token_ids_hash,
            Digest(prompt_ids_form_id_v1(form).as_bytes()),
            word_bytes(capture.prompt.len())?,
            input_chunk,
        )?,
        entry(class.tokenizer_id, tokenizer_json_material_codec_v1(), raw_tokenizer.len(), u32::MAX)?,
        rules.0,
        cold.initial_state.0,
        cold.dependencies.0,
        entry(fp.execution_root, tir_trace_material_codec_v1(), trace_bytes, u32::MAX)?,
        entry(fp.output_root, tir_output_material_codec_v1(), word_bytes(capture.generated.len())?, u32::MAX)?,
    ];
    let quote_material = ProtectedOriginalQuoteMaterialV1 {
        prompt_encoding,
        prompt: prompt.as_bytes().to_vec(),
        hf_descriptor: descriptor_bytes.to_vec(),
        entries,
    };
    quote_material.encode_v1()?;
    Ok(ProtectedOriginalPublicationV1 {
        quote_material,
        header,
        execution_rules: rules.1,
        initial_state: cold.initial_state.1,
        dependencies: cold.dependencies.1,
        trace_index: PublicOriginalTraceIndexV1 {
            full_logits_trace_root: binding.full_logits_trace_root,
            step_leaf_count: binding.step_leaf_count,
            step_merkle_root: binding.step_merkle_root,
        },
    })
}
