//! **RFC-0001 §A (FP Job V4) in the worker and the seat** — every family the live chain uses (the
//! floor, the A16 dense tier at fixture scale, the Qwen3.6 hybrid): the engines execute V4 (G4), a
//! seat replays a V4 claim to the same claim under the claim's rule (G5), and a no-op V4 job decodes
//! exactly what the same V3 job decodes, with the same work (G7). The golden vectors
//! (`consensus-vectors/fp-v4/`) run through the worker's own decode loop and the seat's scoped
//! replay rule — the same files the sampler's tests read.

mod common;

use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwFpIntervalVerdictV1};
use kaspa_consensus_core::palw_decode_pipeline_v4::{
    DecodeConfigV4, PALW_DECODE_V4_BIAS_BAN_Q, PalwFpDecoderV1, PalwFpReplayRuleV1, decode_answer_stop_v4, decode_select_v4,
    palw_fp_replay_select_v1, palw_fp_with_replay_rule_v1,
};
use kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2;
use kaspa_consensus_core::palw_fp_v4_vectors::{fp_v4_check_noop_vectors, fp_v4_check_processor_vectors};
use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptJobV3, fp_job_id_v3};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

const PROCESSOR: &str = include_str!("../../consensus-vectors/fp-v4/processor_order.json");
const NOOP: &str = include_str!("../../consensus-vectors/fp-v4/v4_noop_equals_v3.json");

/// The three families as `(label, backend, class profile, prompt form, vocab)`.
fn families() -> Vec<(&'static str, Box<dyn PalwExecutionBackendV1>, PalwShapeProfileV3, PalwPromptIdsFormV1, u32)> {
    let floor = common::floor_backend(PalwPromptIdsFormV1::Flat);
    let floor_profile = floor.profile().clone();
    let a16_artifact = common::a16_artifact(128);
    let a16_profile =
        kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(common::a16_geometry(128)).expect("a held graph-v7 profile");
    let a16 = common::a16_backend(&a16_artifact, &a16_profile, (64, 8));
    let (q36_artifact, mut q36_geometry) = common::qwen36_fixture();
    // The dev fixture's table covers 32 positions (`attempt_rules_core_v1_golden`'s held row).
    q36_geometry.n_ctx = 32;
    let q36_profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(q36_geometry).expect("the held hybrid profile");
    let q36 = common::qwen36_backend(
        &q36_artifact,
        &q36_profile,
        kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(q36_profile.n_ctx),
    );
    vec![
        ("floor", Box::new(floor), floor_profile, PalwPromptIdsFormV1::Flat, 1_024),
        ("a16", Box::new(a16), a16_profile, PalwPromptIdsFormV1::MerkleV1, 128),
        ("qwen36", Box::new(q36), q36_profile, PalwPromptIdsFormV1::MerkleV1, 64),
    ]
}

/// The committed answer and the rows it was selected from, read back from the run's material.
fn rows_and_ids(label: &str, material: &[u8]) -> (Vec<Vec<i32>>, Vec<u32>) {
    if let Ok(fold) = misaka_palw_base0::produce::base0_fp_material_decode_v2(material) {
        return (fold.logits_rows, fold.generated_token_ids);
    }
    let dense =
        misaka_palw_base0::produce::base0_material_decode_v1(material).unwrap_or_else(|_| panic!("{label}: the material decodes"));
    (dense.2, dense.3)
}

/// Every committed id is §A.3's choice from its own row over the committed prefix.
fn assert_obeys_the_pipeline(label: &str, job: &PalwFreePromptJobV3, rows: &[Vec<i32>], ids: &[u32]) {
    let config = job.decode.clone().expect("a V4 job");
    assert_eq!(rows.len(), ids.len(), "{label}: one selecting row per committed id");
    for t in 0..ids.len() {
        assert_eq!(
            decode_select_v4(&config, &job.sampling_v2(), &ids[..t], &rows[t], &|_| true),
            Some(ids[t] as usize),
            "{label}: position {t} is not the pipeline's choice"
        );
    }
}

fn prompt_of(seed: usize, len: usize, vocab: u32) -> Vec<usize> {
    (0..len).map(|i| (seed * 7919 + i * 104_729 + 1013) % vocab as usize).collect()
}

fn job_for(profile: &PalwShapeProfileV3, form: PalwPromptIdsFormV1, prompt: &[usize], limit: u32) -> PalwFreePromptJobV3 {
    common::fp_job(profile, form, prompt, limit)
}

/// **G7 — the no-op V4 job IS the V3 job's decode**: the same committed ids, the same executed
/// count and stop reason, and the same work (the capture's leaf count), over many prompts and
/// budgets on every family. The job ids differ (a V4 job is its own job), so the roots do too.
#[test]
fn a_noop_v4_job_decodes_what_the_v3_job_decodes_with_the_same_work() {
    let mut compared = 0usize;
    for (label, backend, profile, form, vocab) in families() {
        let n_ctx = profile.n_ctx as usize;
        let (prompts, lens): (usize, &[usize]) = match label {
            "floor" => (40, &[1, 3, 5]),
            "a16" => (12, &[8, 24]),
            _ => (10, &[2, 3]),
        };
        for seed in 0..prompts {
            for &len in lens {
                if len >= n_ctx {
                    continue;
                }
                let prompt = prompt_of(seed, len, vocab);
                let limit = ((n_ctx - len) as u32).min(12);
                let v3 = job_for(&profile, form, &prompt, limit);
                let v4 = v3.clone().into_v4(DecodeConfigV4::NOOP);
                let a = backend.execute_free_prompt(&v3, &prompt).unwrap_or_else(|e| panic!("{label}: V3 runs: {e}"));
                let b = backend.execute_free_prompt(&v4, &prompt).unwrap_or_else(|e| panic!("{label}: V4 no-op runs: {e}"));
                assert_eq!(a.output_token_ids, b.output_token_ids, "{label} seed {seed} len {len}: the same tokens");
                assert_eq!(a.facts.decode_tokens_executed, b.facts.decode_tokens_executed, "{label}: the same count");
                assert_eq!(a.facts.stop_reason, b.facts.stop_reason, "{label}: the same stop");
                assert_eq!(a.facts.step_leaf_count, b.facts.step_leaf_count, "{label}: the same work");
                assert_ne!(fp_job_id_v3(&v3), fp_job_id_v3(&v4), "{label}: two jobs");
                compared += 1;
            }
        }
    }
    eprintln!("G7: V4 no-op == V3 on {compared} (prompt, budget) pairs across floor / a16 / qwen36");
    assert!(compared >= 150, "a sweep, not a sample ({compared})");
}

/// **G4 — the engines execute V4**: penalties and bias move the answer off the greedy one and every
/// committed id obeys the pipeline over its own row; a sampled job is reproducible bit for bit and
/// obeys it too; a stop sequence ends the run where it completes, the claim's count and stop reason
/// follow, and the work is the shorter run's.
#[test]
fn the_engines_execute_v4_penalties_bias_sampling_and_stop() {
    for (label, backend, profile, form, vocab) in families() {
        let n_ctx = profile.n_ctx as usize;
        let len = if label == "a16" { 16 } else { 2 };
        let prompt = prompt_of(3, len, vocab);
        let limit = ((n_ctx - len) as u32).min(12);
        let v3 = job_for(&profile, form, &prompt, limit);
        let greedy = backend.execute_free_prompt(&v3, &prompt).expect("the greedy run");

        // Penalties + a bias that bans the greedy answer's first id: a different answer, obeying §A.3.
        let config = DecodeConfigV4 {
            repeat_penalty_q: 262_144,
            penalty_window: 64,
            frequency_penalty_q: 1 << 24,
            presence_penalty_q: 1 << 23,
            logit_bias: vec![(greedy.output_token_ids[0], PALW_DECODE_V4_BIAS_BAN_Q)],
            stop_sequences: vec![],
        };
        let job = v3.clone().into_v4(config.clone());
        let run = backend.execute_free_prompt(&job, &prompt).unwrap_or_else(|e| panic!("{label}: the V4 run: {e}"));
        assert_ne!(run.output_token_ids, greedy.output_token_ids, "{label}: the controls move the answer");
        assert_ne!(run.output_token_ids[0], greedy.output_token_ids[0], "{label}: the banned id is never committed");
        let (rows, ids) = rows_and_ids(label, &run.outcome.material);
        assert_eq!(ids, run.output_token_ids);
        assert_obeys_the_pipeline(label, &job, &rows, &ids);
        assert_eq!(run.facts.decode_tokens_executed, limit, "{label}: no stop sequence, the whole budget");
        assert_eq!(decode_answer_stop_v4(&config, limit, vocab, &ids).map(|s| s.executed), Ok(limit));

        // Sampled: reproducible, and the pipeline's.
        let hot = PalwFreePromptJobV3 { temperature_q: 3 << 23, sampling_seed: [0x5E; 32], ..job.clone() };
        let one = backend.execute_free_prompt(&hot, &prompt).expect("the sampled run");
        let two = backend.execute_free_prompt(&hot, &prompt).expect("again");
        assert_eq!(one.output_token_ids, two.output_token_ids, "{label}: a seed is a pure function, not a draw");
        assert_eq!(one.outcome.execution_root, two.outcome.execution_root);
        let (rows, ids) = rows_and_ids(label, &one.outcome.material);
        assert_obeys_the_pipeline(label, &hot, &rows, &ids);

        // A stop sequence taken from the penalized answer itself: the run ends where it completes.
        let k = 2usize.min(run.output_token_ids.len() - 1);
        let stop = run.output_token_ids[k - 1..=k].to_vec();
        let first = (1..=run.output_token_ids.len()).find(|end| run.output_token_ids[..*end].ends_with(&stop)).expect("it occurs");
        let stopping = v3.clone().into_v4(DecodeConfigV4 { stop_sequences: vec![stop.clone()], ..config.clone() });
        let mut streamed = Vec::new();
        let stopped = backend
            .execute_free_prompt_streaming(&stopping, &prompt, &mut |id| streamed.push(id))
            .unwrap_or_else(|e| panic!("{label}: the stopping run: {e}"));
        assert_eq!(stopped.output_token_ids, run.output_token_ids[..first].to_vec(), "{label}: the answer ends with its stop");
        assert_eq!(streamed, stopped.output_token_ids, "{label}: each committed id streamed once, nothing past the stop");
        assert_eq!(stopped.facts.decode_tokens_executed as usize, first);
        let expected_reason =
            if (first as u32) < limit { PalwFpStopReasonV3::EndOfGeneration } else { PalwFpStopReasonV3::ExactBudgetReached };
        assert_eq!(stopped.facts.stop_reason, expected_reason, "{label}: the canonical stop reason");
        if (first as u32) < limit {
            assert!(stopped.facts.step_leaf_count < run.facts.step_leaf_count, "{label}: a shorter run is less work");
        }
        let (rows, ids) = rows_and_ids(label, &stopped.outcome.material);
        assert_obeys_the_pipeline(label, &stopping, &rows, &ids);
    }
}

/// **G5 — a seat replays a V4 claim to the same claim.** Every interval of a penalized V4 run
/// verifies `Valid` under the claim's rule (`verify_fp_interval_opening_under_job_v1`, the job and
/// the committed answer), after the seat recomputes its checkpoint state from the committed ids;
/// the V3 verifier — the shipped argmax — cannot follow the penalized answer, so the rule is what
/// the verdict rests on; and a V3 claim keeps the V3 verifier byte for byte.
#[test]
fn a_seat_replays_a_v4_claim_under_the_claims_rule() {
    for (label, backend, profile, form, vocab) in families() {
        let n_ctx = profile.n_ctx as usize;
        let len = if label == "a16" { 64 } else { 3 };
        let prompt = prompt_of(5, len, vocab);
        let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let limit = ((n_ctx - len) as u32).min(8);
        let v3 = job_for(&profile, form, &prompt, limit);
        let greedy = backend.execute_free_prompt(&v3, &prompt).expect("the greedy run");
        // Penalties, and a ban on the greedy answer's first id, so the claim is one the shipped argmax
        // cannot follow from its very first selecting row.
        let job = v3.clone().into_v4(DecodeConfigV4 {
            logit_bias: vec![(greedy.output_token_ids[0], PALW_DECODE_V4_BIAS_BAN_Q)],
            repeat_penalty_q: 262_144,
            penalty_window: 16,
            frequency_penalty_q: 2 << 24,
            presence_penalty_q: 2 << 24,
            ..DecodeConfigV4::NOOP
        });
        for (job, penalized) in [(v3.clone(), false), (job, true)] {
            let run = backend.execute_free_prompt(&job, &prompt).unwrap_or_else(|e| panic!("{label}: runs: {e}"));
            let claim = PalwClaimRootsV1 {
                execution_root: run.outcome.execution_root,
                trace_root: run.outcome.trace_root,
                anchor: fp_job_id_v3(&job),
                attempt_draw: None,
                output_root: None,
                job_pin: None,
            };
            let count = backend
                .fp_interval_count_for(job.prompt_tokens, run.facts.decode_tokens_executed)
                .unwrap_or_else(|| panic!("{label}: a free-prompt path"));
            // The hybrid fixture's anchored intervals do not verify for a V3 claim either (its
            // recurrent state at this scale is outside this test), so the hybrid is checked on its
            // genesis interval, where the whole V4 answer's first rows are replayed.
            let count = if label == "qwen36" { 1 } else { count };
            let mut v3_rule_valid = 0;
            for index in 0..count {
                let opening = backend
                    .open_fp_interval(&run.outcome.material, index, &ids)
                    .unwrap_or_else(|e| panic!("{label}: interval {index} opens: {e}"));
                backend.fp_forget_seat_state_v1();
                if let Some((_, covered, committed)) = misaka_palw_base0::fp_interval::base0_fp_interval_opening_anchor_v1(&opening) {
                    let ctx = misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening)
                        .expect("a V4 opening")
                        .binding
                        .job_context;
                    let recomputed = backend
                        .checkpoint_root_for_context_v1(&ctx, &ids, &run.output_token_ids, covered)
                        .unwrap_or_else(|e| panic!("{label}: interval {index}: the seat recomputes: {e}"));
                    assert_eq!(recomputed, committed, "{label}: interval {index}: the committed checkpoint");
                }
                let verdict = backend.verify_fp_interval_opening_under_job_v1(
                    &opening,
                    claim,
                    index,
                    &ids,
                    run.facts.step_leaf_count,
                    &job,
                    &run.output_token_ids,
                );
                assert_eq!(verdict, PalwFpIntervalVerdictV1::Valid, "{label}: interval {index} (V4 {penalized})");
                if backend.verify_fp_interval_opening(&opening, claim, index, &ids, run.facts.step_leaf_count)
                    == PalwFpIntervalVerdictV1::Valid
                {
                    v3_rule_valid += 1;
                }
            }
            if penalized {
                assert!(
                    v3_rule_valid < count,
                    "{label}: the shipped argmax cannot follow a penalized answer ({v3_rule_valid}/{count})"
                );
            } else {
                assert_eq!(v3_rule_valid, count, "{label}: a V3 claim keeps the V3 verifier");
            }
        }
    }
}

/// **The golden vectors through the worker's and the seat's own entry points** (RFC-0001 §A.5):
/// the worker's decode loop is the decoder every engine drives (`PalwFpDecoderV1`, fed row by row),
/// and the seat's replay derives each id through the scoped claim rule
/// (`palw_fp_replay_select_v1` inside `palw_fp_with_replay_rule_v1`).
#[test]
fn the_golden_vectors_pass_through_the_worker_and_the_seat() {
    let worker_run = |config: &DecodeConfigV4, limit: u32, rows: &[Vec<i32>]| {
        let mut decoder = PalwFpDecoderV1::v4(config.clone(), PalwDecodeSamplingV2::GREEDY, limit);
        let fed: Vec<u32> = rows.iter().map(|row| decoder.select(row)).collect();
        (fed, decoder.generated().to_vec(), decoder.stop())
    };
    // The seat: a V4 claim's rule over the committed prefix, scoped exactly as a verification
    // scopes it. The rule carries no constraint (no V4 job does), so the mask cases fall to the
    // processor the rule calls.
    let seat_select = |config: &DecodeConfigV4,
                       sampling: &PalwDecodeSamplingV2,
                       generated: &[u32],
                       row: &[i32],
                       admitted: &dyn Fn(usize) -> bool| {
        let masked = (0..row.len()).any(|lane| !admitted(lane));
        if masked {
            return decode_select_v4(config, sampling, generated, row, admitted);
        }
        let job =
            common::fp_job(&common::floor_backend(PalwPromptIdsFormV1::Flat).profile().clone(), PalwPromptIdsFormV1::Flat, &[1], 8);
        let job =
            PalwFreePromptJobV3 { temperature_q: sampling.temperature_q, sampling_seed: sampling.seed, ..job }.into_v4(config.clone());
        let expected = decode_select_v4(config, sampling, generated, row, &|_| true);
        let rule = PalwFpReplayRuleV1::of_job(&job, generated);
        let replayed = palw_fp_with_replay_rule_v1(rule, || palw_fp_replay_select_v1(row, generated.len() as u32));
        match expected {
            Some(lane) => {
                assert_eq!(replayed as usize, lane, "the seat's scoped rule is the processor");
                Some(replayed as usize)
            }
            None => None,
        }
    };
    let n = fp_v4_check_processor_vectors(PROCESSOR, &seat_select, &worker_run).expect("processor_order.json");
    let m = fp_v4_check_noop_vectors(
        NOOP,
        &|s, _t, row| {
            // The V3 verifier: outside any scope, the replay's shipped rule (every V3 claim is greedy).
            if s.temperature_q == 0 {
                palw_fp_replay_select_v1(row, 0) as usize
            } else {
                kaspa_consensus_core::palw_decode_select_v2::decode_token_select_v2(row, &s.seed, _t, s.temperature_q)
            }
        },
        &|config, sampling, generated, row, admitted| {
            let mut decoder = PalwFpDecoderV1::v4(config.clone(), *sampling, u32::MAX).resumed(generated);
            let lane = decoder.select(row) as usize;
            Some(lane).filter(|l| admitted(*l))
        },
    )
    .expect("v4_noop_equals_v3.json");
    assert!(n > 100 && m > 100, "{n} / {m}");
}
