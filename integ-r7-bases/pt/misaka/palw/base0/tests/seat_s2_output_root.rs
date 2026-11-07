//! **SEAT-S2 (core + base0 half): the replay reproduces the claim's `output_root`** (f1c_f1m spec
//! §4-bis.10, S-2).
//!
//! `PalwReplayRootsV1` carried the execution and trace roots only, so a seat licensing a claim by
//! replay (ADR-0084 Decision 7) compared the arithmetic and never the answer: a claim whose roots
//! are the honest run's and whose `output_root` names other tokens was licensed by every seat that
//! replayed it. Each family's `execute_for_verdict` now returns the output root its producer
//! commits — here, for every family and retention the chain uses, the replay's is the producer's
//! bit for bit, it is the family's own rule over the generated ids (`output_root_for_context_v1`),
//! and a claim whose output tokens were altered commits a root the replay does not reproduce. The
//! comparison itself is kaspad's (`replay_licenses_v1`).

mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::produce::base0_material_decode_any_v1;

/// The replay of `job` reproduces `committed` (the producer's `output_root`), it is the family's
/// rule over the material's generated ids, and ids altered in any one position do not reproduce it.
fn assert_replay_reproduces(
    label: &str,
    backend: &dyn PalwExecutionBackendV1,
    job: &PalwJobContextV2,
    prompt: &[usize],
    committed: Hash64,
    material: &[u8],
) {
    let replay = backend.execute_for_verdict(job, prompt).expect("the seat's replay runs");
    assert_eq!(replay.output_root, Some(committed), "{label}: the replay's output root is the producer's, bit for bit");
    let retention = base0_material_decode_any_v1(material).expect("the producer's retention decodes");
    let ids = retention.generated_token_ids().to_vec();
    assert!(!ids.is_empty(), "{label}: an answer");
    assert_eq!(backend.output_root_for_context_v1(job, &ids), Some(committed), "{label}: the family's rule over the generated ids");
    let vocab = retention.binding().shape_profile.vocab_size;
    for position in 0..ids.len() {
        let mut altered = ids.clone();
        altered[position] = (altered[position] + 1) % vocab;
        let forged = backend.output_root_for_context_v1(job, &altered).expect("a root");
        assert_ne!(replay.output_root, Some(forged), "{label}: a claim with token {position} altered is not the replay's answer");
    }
    let mut longer = ids.clone();
    longer.push(0);
    assert_ne!(replay.output_root, backend.output_root_for_context_v1(job, &longer), "{label}: an extra token is another answer");
}

fn attempts(label: &str, backend: &dyn PalwExecutionBackendV1) {
    for (anchor, draw) in [(0x52_0001u64, true), (0x52_0002, false)] {
        let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(anchor)).expect("the anchor implies a job");
        let job = palw_attempt_job_v1(canonical, draw);
        let out = backend.execute(&job, &prompt).expect("the producer's attempt runs");
        assert_replay_reproduces(&format!("{label} attempt draw={draw}"), backend, &job, &prompt, out.output_root, &out.material);
    }
}

fn free_prompt(
    label: &str,
    backend: &dyn PalwExecutionBackendV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    form: PalwPromptIdsFormV1,
    prompt: &[usize],
    decode: u32,
) {
    let job = fp_job(profile, form, prompt, decode);
    let run = backend.execute_free_prompt(&job, prompt).expect("the producer's free-prompt run");
    let ctx = base0_material_decode_any_v1(&run.outcome.material).expect("decodes").binding().job_context.clone();
    assert_replay_reproduces(&format!("{label} free prompt"), backend, &ctx, prompt, run.outcome.output_root, &run.outcome.material);
}

#[test]
fn the_floor_replay_reproduces_the_output_root() {
    let backend = floor_backend(PalwPromptIdsFormV1::MerkleV1);
    attempts("floor", &backend);
    let vocab = backend.profile().vocab_size as usize;
    let prompt: Vec<usize> = (0..6).map(|i| (i * 7919 + 1013) % vocab).collect();
    free_prompt("floor", &backend, backend.profile(), backend.prompt_ids_form(), &prompt, 4);
}

#[test]
fn the_a16_replay_reproduces_the_output_root_dense_and_fold() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    let artifact = a16_artifact(128);
    let held = qwen25_a16_profile_v7(a16_geometry(128)).expect("the held row");
    let per_call = qwen25_a16_profile_v2(a16_geometry(128)).expect("the v2 row");
    for (label, profile) in [("A16 held v7 (fold)", held), ("A16 v2 (dense)", per_call)] {
        let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx));
        attempts(label, &backend);
        let prompt: Vec<usize> = (0..12).map(|i| (i * 7919 + 1013) % 128).collect();
        free_prompt(label, &backend, &profile, backend.prompt_ids_form(), &prompt, 4);
    }
}

#[test]
fn the_qwen36_replay_reproduces_the_output_root() {
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_profile_v2, qwen36_profile_v7};
    let (artifact, geometry) = qwen36_fixture();
    for (label, profile) in
        [("Qwen3.6 v2", qwen36_profile_v2(geometry).expect("v2")), ("Qwen3.6 held v7", qwen36_profile_v7(geometry).expect("v7"))]
    {
        let backend = qwen36_backend(&artifact, &profile, (3, 4));
        attempts(label, &backend);
        free_prompt(label, &backend, &profile, backend.prompt_ids_form(), &[3, 1, 4], 4);
    }
}
