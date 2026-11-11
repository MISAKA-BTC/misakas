use borsh::BorshDeserialize;
use kaspa_consensus_core::{palw_freeprompt_v3::{PalwFpWorkerRequestV3, PalwFpWorkerResultV3, PalwFpWorkerInputV3, fp_worker_request_hash_v3, fp_claim_id_v3},palw_prompt_ids_v1::PalwPromptIdsFormV1};
use misaka_palw_base0::tokenizer::QwenTokenizer;
fn read_frame(path: &str) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize, bytes.len()-4);
    bytes
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let request_frame = read_frame(&args[1]);
    let result_frame = read_frame(&args[2]);
    let request = PalwFpWorkerRequestV3::try_from_slice(&request_frame[4..]).unwrap();
    let result = PalwFpWorkerResultV3::try_from_slice(&result_frame[4..]).unwrap();
    let request_hash = fp_worker_request_hash_v3(&request_frame[4..]);
    let flat = result.validate_against_request(&request, request_hash, PalwPromptIdsFormV1::Flat);
    let merkle = result.validate_against_request(&request, request_hash, PalwPromptIdsFormV1::MerkleV1);
    println!("literal_request_matches_worker_hash={} native_flat={flat:?} native_merkle={merkle:?}", request_hash == result.request_hash);
    assert!(flat.is_ok() || merkle.is_ok());
    println!("local_claim_id={}", fp_claim_id_v3(&result.to_commitment(result.job.anchor_daa + 40_000)));
    let PalwFpWorkerInputV3::Text(bytes) = &request.input else { panic!("not literal text"); };
    let text = std::str::from_utf8(bytes).unwrap();
    let raw = std::fs::read(&args[3]).unwrap();
    println!("request_hash={} prompt_utf8={text:?} worker_prompt_ids={:?} native_prompt_commitment={} job_tokenizer_id={} execution_root={} trace_root={} output_root={} tokenizer_raw_commitment={}",
        result.request_hash,result.prompt_token_ids,result.job.prompt_token_ids_hash,result.job.tokenizer_id,
        result.execution_root,result.trace_root,result.output_root,
        kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw));
    match QwenTokenizer::from_json(&raw).and_then(|tokenizer| tokenizer.encode_without_specials(text)) {
        Ok(ids) => { println!("public_encoder_ids={ids:?} same_ids={}", ids == result.prompt_token_ids); assert_eq!(ids, result.prompt_token_ids); }
        Err(error) => panic!("existing consumer raw parse or text encoder refused: {error:?}"),
    }
}
