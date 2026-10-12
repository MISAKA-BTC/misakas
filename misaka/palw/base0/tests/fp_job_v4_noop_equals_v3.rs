//! **RFC-0001 §A.4 G7 — the no-op FP Job V4 decodes exactly what the same V3 job decodes, with the
//! same work, on many inputs** — the compatibility gate a flag day waits for ("これが大量の入力で成り立た
//! ない限り flag day に進まない").
//!
//! The floor class and the A16 dense tier (the 8k row's family, at fixture scale), over many prompts
//! and budgets, at the greedy temperature and at sampled ones: the same committed ids, the same
//! executed count, the same stop reason, and the same work (the capture's leaf count). The two jobs
//! are different jobs — a V4 job has its own id — so their roots differ; what a claim is PAID for
//! and what a user is SHOWN do not. (A sampled V3 job is not admitted by the chain; the engine runs
//! it here only to prove Decision 11's key is the same under both versions.)

mod common;

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4;
use kaspa_consensus_core::palw_freeprompt_v3::{PalwFreePromptJobV3, fp_job_id_v3};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

fn sweep(
    label: &str,
    backend: &dyn PalwExecutionBackendV1,
    profile: &PalwShapeProfileV3,
    form: PalwPromptIdsFormV1,
    vocab: u32,
    prompts: usize,
    lens: &[usize],
) -> usize {
    let n_ctx = profile.n_ctx as usize;
    let mut compared = 0;
    for seed in 0..prompts {
        for &len in lens.iter().filter(|len| **len < n_ctx) {
            let prompt: Vec<usize> = (0..len).map(|i| (seed * 7919 + i * 104_729 + 31 * len + 1013) % vocab as usize).collect();
            let limit = ((n_ctx - len) as u32).min(16);
            for (temperature_q, sampling_seed) in [(0u32, [0u8; 32]), (1 << 24, [seed as u8; 32]), (5 << 22, [0xA5; 32])] {
                let v3 = PalwFreePromptJobV3 { temperature_q, sampling_seed, ..common::fp_job(profile, form, &prompt, limit) };
                let v4 = v3.clone().into_v4(DecodeConfigV4::NOOP);
                let a = backend.execute_free_prompt(&v3, &prompt).unwrap_or_else(|e| panic!("{label}: V3 runs: {e}"));
                let b = backend.execute_free_prompt(&v4, &prompt).unwrap_or_else(|e| panic!("{label}: V4 runs: {e}"));
                let at = format!("{label} seed {seed} len {len} T {temperature_q}");
                assert_eq!(a.output_token_ids, b.output_token_ids, "{at}: the same ids");
                assert_eq!(a.facts.decode_tokens_executed, b.facts.decode_tokens_executed, "{at}: the same count");
                assert_eq!(a.facts.stop_reason, b.facts.stop_reason, "{at}: the same stop reason");
                assert_eq!(a.facts.step_leaf_count, b.facts.step_leaf_count, "{at}: the same work");
                assert_ne!(fp_job_id_v3(&v3), fp_job_id_v3(&v4), "{at}: two jobs");
                compared += 1;
            }
        }
    }
    compared
}

#[test]
fn the_noop_v4_job_is_the_v3_job_on_the_floor() {
    let backend = common::floor_backend(PalwPromptIdsFormV1::Flat);
    let profile = backend.profile().clone();
    let n = sweep("floor", &backend, &profile, PalwPromptIdsFormV1::Flat, 1_024, 60, &[1, 2, 4, 6, 8]);
    eprintln!("G7 floor: V4 no-op == V3 on {n} (prompt, budget, temperature) runs");
    assert!(n >= 900, "{n}");
}

#[test]
fn the_noop_v4_job_is_the_v3_job_on_the_a16_dense_tier() {
    let artifact = common::a16_artifact(128);
    let profile =
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(common::a16_geometry(128)).expect("a held graph-v7 profile");
    let backend = common::a16_backend(&artifact, &profile, (64, 8));
    let n = sweep("a16", &backend, &profile, PalwPromptIdsFormV1::MerkleV1, 128, 24, &[4, 16, 48]);
    eprintln!("G7 a16: V4 no-op == V3 on {n} (prompt, budget, temperature) runs");
    assert!(n >= 200, "{n}");
}
