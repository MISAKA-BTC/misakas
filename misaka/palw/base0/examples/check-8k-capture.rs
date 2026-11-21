//! Read a retained Qwen2.5 A16 8K draw. Checks the signed Original and the
//! disclosed data against it. A selected verifier separately re-executes the job.
use kaspa_consensus_core::{
    Hash64,
    config::class_manifest_const_v1::{QWEN25_A16_8K_MANIFEST_V1, class_id_of_class, inventory_root_of_class},
    palw_attempt_v2::{PalwAttemptEnvelopeV2, attempt_id_v2},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, palw_prompt_ids_form_of_class_v1, prompt_token_ids_match_v1},
    palw_v2::output_commitment_v2,
};
use misaka_palw_base0::{
    produce::{Base0SeatFamilyV1, base0_fp_material_decode_v2, base0_fp_material_matches_claim_v2},
    qwen25_a16_backend::rendered_output_hash_v1,
};
use serde::Deserialize;
use std::{env, fs};

#[derive(Deserialize)]
struct WinningBlock {
    claim: Hash64,
    raw: RawBlock,
}
#[derive(Deserialize)]
struct RawBlock {
    header: Header,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    palw_commitment: Vec<u8>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [winning, capture] = args.as_slice() else { return Err("usage: check-8k-capture WINNING.json CAPTURE.bin".into()) };
    let record: WinningBlock = serde_json::from_reader(fs::File::open(winning)?)?;
    let envelope = PalwAttemptEnvelopeV2::decode_wire(&record.raw.header.palw_commitment)?;
    envelope.validate_shape_v2()?;
    envelope.validate_signature_v2(|k, m, s, c| kaspa_txscript::verify_mldsa87_with_context(k, m, s, c).unwrap_or(false))?;
    let a = &envelope.attempt;
    if a.class_id != class_id_of_class(QWEN25_A16_8K_MANIFEST_V1, 1)
        || a.artifact_root != inventory_root_of_class(QWEN25_A16_8K_MANIFEST_V1, 1)
        || attempt_id_v2(a) != record.claim
    {
        return Err("signed attempt differs from the recorded 8K class or claim".into());
    }
    let material = base0_fp_material_decode_v2(&fs::read(capture)?).map_err(|e| format!("material decode: {e:?}"))?;
    let ctx = &material.binding.job_context;
    let form = palw_prompt_ids_form_of_class_v1(PalwPromptIdsFormV1::Flat, &material.binding.shape_profile);
    let tokens = &material.generated_token_ids;
    let out = output_commitment_v2(&ctx.context_hash(), tokens, &rendered_output_hash_v1(tokens));
    if !base0_fp_material_matches_claim_v2(&material, a.execution_root, a.trace_root, Base0SeatFamilyV1::IntegerKv)
        .map_err(|e| format!("material check: {e:?}"))?
        || !prompt_token_ids_match_v1(form, &material.prompt_token_ids, &ctx.prompt_token_ids_hash)
        || out != a.output_root
        || material.prompt_token_ids.len() != ctx.declared_prefill_tokens as usize
        || tokens.len() != ctx.exact_decode_tokens as usize
    {
        return Err("retained input, output, trace or execution differs from the signed job".into());
    }
    println!(
        "{}",
        serde_json::json!({"claim":record.claim,"class":a.class_id,"signature":"valid","prompt_tokens":ctx.declared_prefill_tokens,
        "decode_tokens":ctx.exact_decode_tokens,"answer_ids":tokens,"step_leaves":material.binding.step_leaf_count,
        "checkpoint_leaves":material.checkpoint_leaves.len(),"trace_root":a.trace_root,"output_root":a.output_root,"execution_root":a.execution_root})
    );
    Ok(())
}
