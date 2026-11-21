//! Held-model exercise for the private EXEC input reader. Registration consumes
//! the artifact/class identities. The old Studio claim never supplies a job,
//! authorization, reserved funds, ticket or result to this run.
use super::*;
use kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_root_v1;
use misaka_palw_remote::tb1_exposure::exec_input::{DerivedExecInput, ExecInputTerms};
use misaka_palw_tir_exec::node::TirArtifactV1;

const MODEL_ROOT: &str = "8e31580dc81f6548b9e440c9b6d45fe63bff1d42ed2dbb4b342bdb58fc4590f0b8210f10a2a23e6c352c866a4313004b547ec363997e5757d2bce31a6838eec4";
const CLASS_ID: &str = "7a2616b4b132946d21b1d396c0a82f69a144fb7f692394314efb35e3f8bff1f231b857f174e1165ad92c65ce742e1bcc57dfb11e1e5eab02b67b8cf87436c625";
const TOKENIZER: &str = "57c55fb228e8bb272c1d3e3a75eb8e7a238fe2e6a7ccc5e4cd6b874c1278fbc79342c2d466bc52d53000bb0de6de57cb196fc0a413c2751d45cfb69551615ac2";

pub(super) fn terms(anchor: Hash64) -> ExecInputTerms<'static> {
    let mut target = [0; 33];
    target[1] = 0x80; // Private strict 256-bit ticket threshold. This test issues no ticket.
    ExecInputTerms {
        qf1: None,
        registration_anchor: anchor,
        candidate_set: [18; 64],
        beneficiary: [19; 64],
        model_root: MODEL_ROOT.parse().unwrap(),
        class_id: CLASS_ID.parse().unwrap(),
        tokenizer: TOKENIZER.parse().unwrap(),
        job_tokens: &[42, 108, 17],
        vocab: 49152,
        max_positions: 8192,
        decode_tokens: 1,
        max_leaves: 400_000,
        nonce: [20; 32],
        policy: [17; 64],
        difficulty: [21; 64],
        target,
        accounting: [22; 64],
        open_daa: 0,
        deadline_exclusive_daa: 40,
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(super) struct Inference {
    generated: Vec<u32>,
    input: Hash64,
    execution: Hash64,
    trace: Hash64,
    output: Hash64,
    steps: u64,
}

pub(super) fn infer(native: &dyn kaspa_consensus_core::api::ConsensusApi, path: &Path, input: &DerivedExecInput<'_>) -> Inference {
    let artifact = TirArtifactV1::open(path).unwrap();
    let run = input.replay_tir(native, &artifact).unwrap();
    assert_eq!(run.generated.len(), input.terms().decode_tokens as usize);
    assert_eq!(run.leaf_count, run.leaf_hashes.len() as u64);
    Inference {
        generated: run.generated,
        input: prompt_token_ids_root_v1(input.tokens()).unwrap(),
        execution: run.execution_root,
        trace: run.trace_root,
        output: run.output_root,
        steps: run.leaf_count,
    }
}
