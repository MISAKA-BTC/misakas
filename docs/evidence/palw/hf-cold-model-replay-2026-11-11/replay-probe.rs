use borsh::BorshDeserialize;
use kaspa_consensus_core::{
    Hash64, palw_fp_execution_v3::{palw_fp_job_pin_of_context_v1, palw_fp_job_pin_v1},
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, PalwFpWorkerRequestV3, PalwFpWorkerResultV3, fp_claim_id_v3, fp_worker_request_hash_v3},
    palw_logits_q24_v2, palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_step_refute::{flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1},
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_exec::node::TirCaptureV1;
use std::{fs::File, io::Read, path::Path};

fn read(path: &str, cap: usize) -> Vec<u8> {
    let mut data = Vec::new();
    File::open(path).unwrap().take(cap as u64 + 1).read_to_end(&mut data).unwrap();
    assert!(data.len() <= cap);
    data
}

fn frame(path: &str, cap: usize) -> Vec<u8> {
    let data = read(path, cap);
    assert_eq!(u32::from_le_bytes(data[..4].try_into().unwrap()) as usize, data.len() - 4);
    data
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let container = PalwTirContainerV1::open_bounded(Path::new(&args[1]), 256 << 20, 4 << 20).unwrap();
    let captured = TirCaptureV1::decode(&read(&args[2], 16 << 20)).unwrap();
    let request_frame = frame(&args[3], 1 << 20);
    let result_frame = frame(&args[4], 1 << 20);
    let request = PalwFpWorkerRequestV3::try_from_slice(&request_frame[4..]).unwrap();
    let result = PalwFpWorkerResultV3::try_from_slice(&result_frame[4..]).unwrap();
    result.validate_against_request(&request, fp_worker_request_hash_v3(&request_frame[4..]), PalwPromptIdsFormV1::MerkleV1).unwrap();
    let original = PalwFreePromptCommitmentV3::try_from_slice(&read(&args[5], 1 << 20)).unwrap();
    assert_eq!(result.to_commitment(original.trace_retention_daa), original);
    let known_inventory_root: Hash64 = args[6].parse().unwrap();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: container.header.program,
        layout: PalwTirLayoutV1::try_from_slice(&container.header.layout).unwrap(),
        tokenizer_id: Hash64::from_bytes(container.header.tokenizer_id),
    };
    assert_eq!(class, captured.binding.class);
    assert_eq!(class.class_id(&known_inventory_root), original.job.class_id);
    assert_eq!(captured.binding.artifact_root, known_inventory_root);
    let header = PalwPublicTirBindingHeaderV1::of_binding_v1(&captured.binding);
    assert_eq!(palw_fp_job_pin_v1(&original), palw_fp_job_pin_of_context_v1(&header.job_context));
    assert_eq!(captured.prompt, result.prompt_token_ids);
    assert!(prompt_token_ids_match_v1(PalwPromptIdsFormV1::MerkleV1, &captured.prompt, &header.job_context.prompt_token_ids_hash));
    assert_eq!(captured.generated, result.output_token_ids);
    assert_eq!(header.step_leaf_count, original.work_leaves);
    let (record, program) = palw_tir_class_record_v1(&class, &known_inventory_root).unwrap();
    assert_eq!(program, container.program);
    let source = PalwTirPublicSourceV1::new_native_v1(
        &record, &class.layout, &header, original.execution_root, original.trace_root, 1 << 22,
    ).unwrap();
    use kaspa_consensus_core::palw_positive_contract_v2::{public_tfp1_cold_material_v1, public_ordinary_tfp1_rules_material_v1};
    use misaka_palw_positive_contract::public_one_honest::{ManifestScopeRules,
        tir_ordinary_tfp1_rules_material_codec_v1, tir_cold_initial_state_codec_v1, tir_cold_dependencies_codec_v1};
    let fixed = |codec, size| ManifestScopeRules { codec, max_bytes: size, chunk_bytes: size as u32 };
    let (rules, rules_body) = public_ordinary_tfp1_rules_material_v1(
        &original, &source, &class, &program, PalwPromptIdsFormV1::MerkleV1,
        fixed(tir_ordinary_tfp1_rules_material_codec_v1(), 512),
    ).unwrap();
    let cold = public_tfp1_cold_material_v1(&original, &source, &class,
        fixed(tir_cold_initial_state_codec_v1(), 128), fixed(tir_cold_dependencies_codec_v1(), 68)).unwrap();
    let out = Path::new(&args[7]);
    std::fs::create_dir_all(out).unwrap();
    for (name, entry, body) in [
        ("execution-rules", &rules, rules_body.as_slice()),
        ("initial-state", &cold.initial_state.0, cold.initial_state.1.as_slice()),
        ("dependencies", &cold.dependencies.0, cold.dependencies.1.as_slice()),
    ] {
        std::fs::write(out.join(format!("{name}.bin")), body).unwrap();
        println!("native_material={} codec={} subject={} content_root={} bytes={}", name,
            Hash64::from_bytes(entry.codec.0), Hash64::from_bytes(entry.subject.0), Hash64::from_bytes(entry.content_root.0), entry.total_bytes);
    }
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3;
    use misaka_palw_tir_exec::node::{TirArtifactV1, TirBackendV1};
    let artifact = std::sync::Arc::new(TirArtifactV1::open(Path::new(&args[1])).unwrap());
    let (fresh_root, _) = artifact.inventory_root().unwrap();
    assert_eq!(fresh_root, known_inventory_root);
    assert_eq!(artifact.class().unwrap(), class);
    let ctx = &header.job_context;
    let backend = TirBackendV1::new("HF fresh replay".into(), artifact, fresh_root, ctx.clone(), PalwPromptIdsFormV1::MerkleV1, 1 << 22)
        .unwrap().with_network_id_v1(&ctx.network_id).with_fp_class_facts_v1(PalwFpClassFactsV3 {
            model_profile_id: ctx.model_profile_id, runtime_manifest_hash: ctx.runtime_manifest_hash,
            runtime_class_id: ctx.runtime_class_id, shape_profile_id: ctx.shape_profile_id, cu_ruleset_id: ctx.cu_ruleset_id,
        });
    let need = backend.tir_own_step_source_need_v1(&header, &original.job, &result.prompt_token_ids).unwrap();
    println!("fresh_model_replay_need={need:?}");
    if std::env::var_os("MISAKA_HF_PROBE_QUOTE_ONLY").is_some() { return; }
    // Standalone probe allowance: 3 GiB on a 32-GiB host (10+ GiB free). No consensus/profile budget is changed.
    assert!(need.resident_ram_bytes <= 3 << 30 && need.disk_bytes <= 128 << 20);
    let began = std::time::Instant::now();
    let own = backend.open_tir_own_step_source_v1(&header, &original.job, &result.prompt_token_ids, out).unwrap();
    let replay = own.info_v1();
    assert_eq!(replay.header, header);
    assert_eq!(replay.output_root, original.output_root);
    assert_eq!(replay.generated_ids, result.output_token_ids);
    println!("fresh_hf_replay_matched=true execution_root={} output_root={} generated_ids={:?} elapsed_seconds={:.3}",
        replay.header.committed_execution_root, replay.output_root, replay.generated_ids, began.elapsed().as_secs_f64());
    println!("public_admission=false public_panel_approval=false weights_rehashed=true inference=true provider_remote_acquisition=false");

    let scheme = Hash64::from_bytes(program.logits_scheme_id);
    assert_eq!(scheme, request.trace_scheme_id);
    assert_eq!(scheme, tiled_logits_scheme_id_v1());
    assert!(source.supports_tiled_trace_v1());
    println!("claim_id={} class_id={} graph_ir_root={}", fp_claim_id_v3(&original), original.job.class_id, record.graph_ir_root);
    println!("known_inventory_root={} source_id={} source_leaves={}", known_inventory_root, source.source_id_v1(), source.leaf_count_v1());
    println!(
        "registered_program_logits_scheme={} header_job_context_trace_scheme={} worker_request_trace_scheme={} tiled_source={} flat={} wide_q24={}",
        scheme, header.job_context.trace_scheme_id, request.trace_scheme_id, source.supports_tiled_trace_v1(),
        scheme == flat_logits_scheme_id_v1(), scheme == palw_logits_q24_v2::scheme_id()
    );
    println!("artifact_file_bytes={} program_bytes={} context_hash={} step_root={} logits_root={} execution_root={}",
        container.file_len, record.program_bytes, header.job_context.context_hash(), header.step_merkle_root, header.full_logits_trace_root,
        header.committed_execution_root);
}
