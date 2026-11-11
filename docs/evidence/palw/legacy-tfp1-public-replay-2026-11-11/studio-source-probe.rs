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
