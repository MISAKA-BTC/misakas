//! Explicit, offline publisher for an ordinary Studio/outbox commitment.
//! It creates no signatures, payments, approvals or consensus state.
use kaspa_consensus_core::{
    palw_backend::{PalwExecutionBackendV1, PalwOriginalCaptureReaderV1, PalwTirPublicCaptureWorkV1},
    palw_freeprompt_v3::{PalwFpWorkerRequestV3, PalwFpWorkerResultV3, PalwFreePromptCommitmentV3, fp_worker_request_hash_v3},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_protected_original_quote_v1::{PromptMaterialEncodingV1, ProtectedOriginalQuoteMaterialV1},
    palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    palw_tir_attempt_v1::PalwTirJobFactsV1,
};
use misaka_palw_tir_exec::node::{
    backend::TirCaptureV1,
    protected_original_material::{
        ProtectedTraceWriteLimitsV1, prepare_protected_original_material_with_prompt_v2, require_public_original_input_v1,
        write_protected_original_trace_v1,
    },
};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayBudget {
    maximum_resident_bytes: u64,
    maximum_disk_bytes: u64,
    maximum_slices: u64,
    io_bytes: u64,
    hash_bytes: u64,
    tree_hashes: u64,
    replay_leaves: u64,
    replay_positions: u64,
    replay_id_words: u64,
    arithmetic_work: u128,
    traffic_bytes: u128,
}
impl ReplayBudget {
    fn limits(&self) -> ProtectedTraceWriteLimitsV1 {
        ProtectedTraceWriteLimitsV1 {
            maximum_resident_bytes: self.maximum_resident_bytes,
            maximum_slices: self.maximum_slices,
            maximum_work: PalwTirPublicCaptureWorkV1 {
                io_bytes: self.io_bytes,
                hash_bytes: self.hash_bytes,
                tree_hashes: self.tree_hashes,
                replay_leaves: self.replay_leaves,
                replay_positions: self.replay_positions,
                replay_id_words: self.replay_id_words,
                arithmetic_work: self.arithmetic_work,
                traffic_bytes: self.traffic_bytes,
            },
        }
    }
}
struct OriginalBytes(Vec<u8>);
impl PalwOriginalCaptureReaderV1 for OriginalBytes {
    fn len_v1(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_v1(&self, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        let first = usize::try_from(offset).map_err(|_| "capture offset overflow")?;
        let end = first.checked_add(length as usize).ok_or("capture range overflow")?;
        self.0.get(first..end).map(<[u8]>::to_vec).ok_or("capture range outside source".into())
    }
}

fn read(path: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path).and_then(|f| f.take(limit + 1).read_to_end(&mut bytes)).map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{path} exceeds {limit} bytes"));
    }
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn check_literal_prompt(
    tokenizer: &misaka_palw_base0::tokenizer::QwenTokenizer,
    prompt: &str,
    retained_ids: &[u32],
) -> Result<(), String> {
    // Match the Panel's check_public_prompt_v1 and the worker's Text arm.
    // A literal spelling of a control marker must remain ordinary text.
    if tokenizer.encode_without_specials(prompt).map_err(|e| e.to_string())? != retained_ids {
        return Err("literal prompt does not encode to the retained input IDs".into());
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut flags = BTreeMap::new();
    while let Some(flag) = args.next() {
        if ![
            "--commitment",
            "--capture",
            "--prompt",
            "--worker-request",
            "--worker-result",
            "--tokenizer",
            "--hf-descriptor",
            "--out-directory",
            "--artifact",
            "--replay-budget",
            "--protected-planning",
        ]
        .contains(&flag.as_str())
        {
            return Err("usage: palw-protected-material --commitment <unsigned.borsh> --capture <material.bin> (--prompt <exact-utf8> | --worker-request <request.borsh> --worker-result <result.borsh>) --tokenizer <tokenizer.json> --hf-descriptor <HFA1> --out-directory <new-directory> [--artifact <PALWTIR1> --replay-budget <json> (required for folded capture)] [--protected-planning <complete-declaration-v3.json>]".into());
        }
        let value = args.next().ok_or_else(|| format!("missing value for {flag}"))?;
        if flags.insert(flag, value).is_some() {
            return Err("duplicate argument".into());
        }
    }
    let arg = |key: &str| flags.get(key).map(String::as_str).ok_or_else(|| format!("{key} is required"));
    // Read once before capture reconstruction. A malformed explicit policy is
    // an error, never a fallback to the pinned public publisher ruleset.
    let planning = flags
        .get("--protected-planning")
        .map(|path| {
            use kaspa_consensus_core::palw_positive_contract_v2::ProtectedPlanningDeclarationV3;
            ProtectedPlanningDeclarationV3::decode_json_v3(&read(path, ProtectedPlanningDeclarationV3::MAX_JSON_BYTES as u64)?)
        })
        .transpose()?;
    let fp: PalwFreePromptCommitmentV3 = borsh::from_slice(&read(arg("--commitment")?, 1 << 20)?).map_err(|e| e.to_string())?;
    require_public_original_input_v1(&fp)?;
    // Keep the exact bytes immutable while the folded reader reconstructs.
    // The input bound is separate from the streaming reader's work budget.
    let original = Arc::new(OriginalBytes(read(arg("--capture")?, 256 << 20)?));
    let capture = TirCaptureV1::decode(&original.0)?;
    let request_result = match (flags.get("--worker-request"), flags.get("--worker-result"), flags.get("--prompt")) {
        (Some(request), Some(result), None) => {
            let request_bytes = read(request, 1 << 20)?;
            let request: PalwFpWorkerRequestV3 = borsh::from_slice(&request_bytes).map_err(|e| e.to_string())?;
            let result: PalwFpWorkerResultV3 = borsh::from_slice(&read(result, 32 << 20)?).map_err(|e| e.to_string())?;
            Some((request, fp_worker_request_hash_v3(&request_bytes), result))
        }
        (None, None, Some(_)) => None,
        _ => return Err("choose exactly --prompt or both --worker-request and --worker-result".into()),
    };
    let (encoding, prompt_bytes) = if let Some((request, _, _)) = &request_result {
        ProtectedOriginalQuoteMaterialV1::prompt_of_worker_input_v2(&request.input, 1 << 20)?
    } else {
        (PromptMaterialEncodingV1::LiteralUtf8, read(arg("--prompt")?, 1 << 20)?)
    };
    let prompt = String::from_utf8(prompt_bytes).map_err(|e| e.to_string())?;
    let raw = read(arg("--tokenizer")?, 32 << 20)?;
    let descriptor = read(arg("--hf-descriptor")?, 4096)?;
    let tokenizer = misaka_palw_sdk::kernel_job_v1::kernel_tokenizer_v1(&raw, Some(fp.job.tokenizer_id))?;

    let binding = &capture.binding;
    let network = std::str::from_utf8(&binding.job_context.network_id).map_err(|e| e.to_string())?;
    let mut form = misaka_palw_base0::fp_worker::fp_worker_prompt_ids_form_v1(network)?;
    let program = binding.class.decode_program().map_err(|e| e.to_string())?;
    if PalwTirJobFactsV1::of(&binding.class, &program, fp.job.class_id).held {
        form = PalwPromptIdsFormV1::MerkleV1;
    }
    if let Some((request, request_hash, result)) = &request_result {
        result.validate_against_request(request, *request_hash, form).map_err(|e| e.to_string())?;
        if result.job != fp.job
            || result.prompt_token_ids != capture.prompt
            || result.output_token_ids != capture.generated
            || result.execution_root != fp.execution_root
            || result.trace_root != fp.trace_root
            || result.output_root != fp.output_root
            || result.schedule_root != fp.schedule_root
            || result.decode_tokens_executed != fp.decode_tokens_executed
            || result.step_leaf_count != fp.work_leaves
        {
            return Err("worker request/result and retained claim differ".into());
        }
    }
    if encoding == PromptMaterialEncodingV1::LiteralUtf8 {
        check_literal_prompt(&tokenizer.tokenizer, &prompt, &capture.prompt)?;
    } else {
        let ids = encoding
            .encode_ids_v1(
                &prompt,
                u32::try_from(capture.prompt.len()).map_err(|_| "prompt count overflow")?,
                |text| tokenizer.tokenizer.encode_without_specials(text),
                |id| tokenizer.tokenizer.is_added_id(id),
            )
            .map_err(|e| format!("segmented prompt tokenization: {e:?}"))?;
        if ids != capture.prompt {
            return Err("segmented prompt differs from retained input IDs".into());
        }
    }
    let directory = Path::new(arg("--out-directory")?);
    let court = misaka_palw_base0::fp_worker::fp_worker_court_params_v1(network)?;
    let maximum_leaves = court.max_step_leaf_count();
    let folded = capture.leaves.is_empty();
    let replay = if folded {
        let budget: ReplayBudget = serde_json::from_slice(&read(arg("--replay-budget")?, 8192)?).map_err(|e| e.to_string())?;
        let entry = misaka_palw_sdk::lineages::tir::TirLineageV1::open_entry(Path::new(arg("--artifact")?))?;
        let backend = misaka_palw_sdk::lineages::tir::TirLineageV1::backend(&entry, &court, form)?;
        let header = kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(binding);
        let need = backend.tir_public_capture_need_v1(&header, original.len_v1())?;
        let (record, _) =
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&binding.class, &binding.artifact_root)
                .map_err(|e| e.to_string())?;
        let source = kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_v1(
            &record,
            &binding.class.layout,
            &header,
            fp.execution_root,
            fp.trace_root,
            maximum_leaves,
        )?;
        let trace_bytes = source
            .trace_value_bytes_v1()
            .and_then(|n| n.checked_add(kaspa_consensus_core::palw_tir_court_v1::PublicOriginalTraceIndexV1::WIRE_BYTES as u64))
            .ok_or("trace byte count overflow")?;
        if trace_bytes > u64::from(u32::MAX)
            || need.resident_ram_bytes > budget.maximum_resident_bytes
            || need.disk_bytes.checked_add(trace_bytes).is_none_or(|n| n > budget.maximum_disk_bytes)
        {
            return Err("folded publisher exceeds its local RAM/disk budget or material bound".into());
        }
        Some((backend, header, source, budget))
    } else {
        None
    };
    let dense = if folded {
        None
    } else {
        Some(prepare_protected_original_material_with_prompt_v2(
            &fp,
            &capture,
            encoding,
            &prompt,
            &raw,
            &descriptor,
            form,
            maximum_leaves,
            None,
        )?)
    };
    std::fs::create_dir(directory).map_err(|e| format!("new output directory: {e}"))?;
    let mut publication = if let Some((backend, header, source, budget)) = replay {
        let mut reader = backend.open_tir_public_capture_v1(&header, original, directory)?;
        let mut trace = std::io::BufWriter::new(File::create(directory.join("trace.bin")).map_err(|e| e.to_string())?);
        let receipt = write_protected_original_trace_v1(&source, &mut *reader, &mut trace, &budget.limits())?;
        prepare_protected_original_material_with_prompt_v2(
            &fp,
            &capture,
            encoding,
            &prompt,
            &raw,
            &descriptor,
            form,
            maximum_leaves,
            Some(&receipt),
        )?
    } else {
        dense.ok_or("publisher has neither dense nor streamed material")?
    };
    if let Some(declaration) = &planning {
        publication.quote_material = publication.quote_material.with_planning_declaration_v3(declaration)?;
    }
    let names = [
        "artifact.bin",
        "program.bin",
        "prompt.bin",
        "input-tokens.bin",
        "tokenizer.bin",
        "execution-rules.bin",
        "initial-state.bin",
        "dependencies.bin",
        "trace.bin",
        "output.bin",
    ];
    let bodies: [(usize, &[u8]); 7] = [
        (0, &descriptor),
        (1, &binding.class.program),
        (2, prompt.as_bytes()),
        (4, &raw),
        (5, &publication.execution_rules),
        (6, &publication.initial_state),
        (7, &publication.dependencies),
    ];
    for (index, body) in bodies {
        std::fs::write(directory.join(names[index]), body).map_err(|e| e.to_string())?;
    }
    for (index, ids) in [(3, &capture.prompt), (9, &capture.generated)] {
        let mut out = std::io::BufWriter::new(File::create(directory.join(names[index])).map_err(|e| e.to_string())?);
        for id in ids {
            out.write_all(&id.to_le_bytes()).map_err(|e| e.to_string())?;
        }
        out.flush().map_err(|e| e.to_string())?;
    }
    if !folded {
        let mut trace = std::io::BufWriter::new(File::create(directory.join(names[8])).map_err(|e| e.to_string())?);
        borsh::to_writer(&mut trace, &publication.trace_index).map_err(|e| e.to_string())?;
        for leaf in &capture.leaves {
            trace.write_all(&leaf.values_le).map_err(|e| e.to_string())?;
        }
        trace.flush().map_err(|e| e.to_string())?;
    }
    for (name, entry) in names.iter().zip(publication.quote_material.entries) {
        if std::fs::metadata(directory.join(name)).map_err(|e| e.to_string())?.len() != entry.total_bytes {
            return Err(format!("{name}: written body differs from manifest size"));
        }
    }
    let header = PalwPublicExecutionBindingHeaderV1::Tir(publication.header);
    std::fs::write(directory.join("public-binding.hex"), hex(&borsh::to_vec(&header).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;
    // Written last: the rail never sees a completed quote artifact on failure.
    std::fs::write(directory.join("protected-material.hex"), hex(&publication.quote_material.encode_v1()?))
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"claimId": kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&fp).to_string(),
        "directory": directory, "planningDeclarationId": planning.as_ref().map(|d| d.fingerprint_v3().to_string()),
        "panelReplayApproved": false})
    );
    Ok(())
}
fn main() {
    if let Err(why) = run() {
        eprintln!("protected material: {why}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_publication_preserves_text_without_inserting_control_ids() {
        let mut vocab = serde_json::Map::new();
        for c in '!'..='~' {
            vocab.insert(c.to_string(), serde_json::json!(c as u32));
        }
        let marker = "<|im_start|>";
        vocab.insert(marker.into(), serde_json::json!(128));
        let raw = serde_json::to_vec(&serde_json::json!({
            "model": {"type": "BPE", "vocab": vocab, "merges": []},
            "added_tokens": [{"id": 128, "content": marker, "special": true, "normalized": false}]
        }))
        .unwrap();
        let tokenizer = misaka_palw_sdk::kernel_job_v1::kernel_tokenizer_v1(&raw, None).unwrap().tokenizer;
        let ordinary = tokenizer.encode_without_specials(marker).unwrap();
        assert_eq!(tokenizer.encode(marker).unwrap(), vec![128]);
        assert!(!ordinary.contains(&128));
        assert!(check_literal_prompt(&tokenizer, marker, &ordinary).is_ok());
        assert!(check_literal_prompt(&tokenizer, marker, &[128]).is_err());
        assert!(check_literal_prompt(&tokenizer, "changed", &ordinary).is_err());
        use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpPromptSegmentV1, PalwFpWorkerInputV3};
        let input = PalwFpWorkerInputV3::Segments(vec![
            PalwFpPromptSegmentV1::Special(128),
            PalwFpPromptSegmentV1::Text(marker.as_bytes().to_vec()),
        ]);
        let (encoding, body) = ProtectedOriginalQuoteMaterialV1::prompt_of_worker_input_v2(&input, 4096).unwrap();
        let text = std::str::from_utf8(&body).unwrap();
        let encode = |cap| encoding.encode_ids_v1(text, cap, |s| tokenizer.encode_without_specials(s), |id| tokenizer.is_added_id(id));
        assert_eq!(encode(100).unwrap(), [vec![128], ordinary.clone()].concat());
        assert!(encode(1).is_err());
        let invalid_control = "PPS1\ns:33\n";
        assert!(
            encoding
                .encode_ids_v1(invalid_control, 100, |s| tokenizer.encode_without_specials(s), |id| tokenizer.is_added_id(id))
                .is_err()
        );
    }
}
