//! **T18p-M (ADR-0152 v3.1 addendum §4-bis, Tests; the Phase 1–2 review's H-1): a seat's full
//! verdict routes give no `Valid` to a drill fault, and an honest run gets it.**
//!
//! The two routes a full-mask `Valid` comes from — the MATERIAL route (`verify_material`: SEAT-S1's
//! whole job, SEAT-0's tail, and under `CoreV1` the claim's output root,
//! `base0_material_answers_the_claim_v1`) and the REPLAY route (`execute_for_verdict`'s roots,
//! which SEAT-S2 extended with the output root; the comparison is kaspad's `replay_licenses_v1`,
//! restated here as "every root the claim commits reproduces") — for the floor, the held A16 v7 (a
//! fold), the per-call A16 v2 (dense) and the held Qwen3.6 v7, each under `CoreV1`:
//!
//! * an honest attempt and an honest free prompt: `Matches`, and the replay reproduces every root;
//! * the honest run under a ground `output_root` (`OutputRoot`): `Mismatch`, and the replay's
//!   output root is not the claim's;
//! * a relabel, a short prefill and (model classes) the `Legacy` context — each the wrong WHOLE job
//!   under the anchor's id: `Mismatch`, and the replay of the anchor's job reproduces none of it;
//! * the drill's injected step fault: the replay refuses it. (The material route re-derives the
//!   capture's own self-consistent roots and cannot see a step lie away from the head — which is why
//!   SEAT-R, kaspad's, makes the replay the full seat's only `Valid` exit.)
//!
//! **And two guards that fail if a rule leaves:** the replay's roots are destructured field by field
//! here, so a `PalwReplayRootsV1` without `output_root` does not compile; and a held class's attempt
//! IS a fold, whose roots a short prefill keeps self-consistent — the fold branch refuses it only
//! through the whole-job helper, so this file goes red if that branch loses it.

mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::{
    PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1, PalwReplayRootsV1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

const MATCHES: PalwMaterialVerdictV1 = PalwMaterialVerdictV1::Matches;
const MISMATCH: PalwMaterialVerdictV1 = PalwMaterialVerdictV1::Mismatch;

/// The chain's view of an attempt claim carrying `out`'s roots under `anchor`.
fn attempt_claim(anchor: Hash64, out: &PalwExecutionOutcomeV1) -> PalwClaimRootsV1 {
    PalwClaimRootsV1 {
        execution_root: out.execution_root,
        trace_root: out.trace_root,
        anchor,
        attempt_draw: Some(true),
        output_root: Some(out.output_root),
    }
}

/// **The replay route**: the seat re-runs the job the CHAIN asked for and every root the claim
/// commits must reproduce. Destructured field by field — the guard on `output_root`.
fn replay_licenses(backend: &dyn PalwExecutionBackendV1, job: &PalwJobContextV2, prompt: &[usize], claim: &PalwClaimRootsV1) -> bool {
    let PalwReplayRootsV1 { execution_root, trace_root, work_leaves: _, output_root } =
        backend.execute_for_verdict(job, prompt).expect("the seat's replay runs");
    assert!(output_root.is_some(), "every shipped family's replay names its output root (SEAT-S2)");
    execution_root == claim.execution_root && trace_root == claim.trace_root && output_root == claim.output_root
}

/// One family: the attempt drills at two anchors, then an honest and a ground free prompt.
fn duty(
    label: &str,
    backend: &dyn PalwExecutionBackendV1,
    legacy: Option<&dyn PalwExecutionBackendV1>,
    profile: &PalwShapeProfileV3,
    fp_prompt: &[usize],
) {
    assert_eq!(backend.attempt_rules_v1(), PalwAttemptRulesV1::CoreV1, "{label}: the seat runs the network's rule");
    let form = PalwPromptIdsFormV1::MerkleV1;
    for n in 0u64..2 {
        let anchor = Hash64::from_u64_word(0x7189_0000 ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let job = palw_attempt_job_v1(canonical, true);
        let honest = backend.execute(&job, &prompt).expect("the honest run");
        let claim = attempt_claim(anchor, &honest);
        assert_eq!(backend.verify_material(&honest.material, claim), MATCHES, "{label} #{n}: the honest material");
        assert!(replay_licenses(backend, &job, &prompt, &claim), "{label} #{n}: the honest replay");

        // OutputRoot: the honest run, a ground answer.
        let ground = PalwClaimRootsV1 { output_root: Some(Hash64::from_u64_word(0x6A0D ^ n)), ..claim };
        assert_eq!(backend.verify_material(&honest.material, ground), MISMATCH, "{label} #{n}: a ground output root");
        assert!(!replay_licenses(backend, &job, &prompt, &ground), "{label} #{n}: the replay's answer is not the ground one");

        // The wrong whole job under the anchor's id: a relabel, a short prefill, the Legacy context.
        let mut lies: Vec<(&str, PalwExecutionOutcomeV1)> = Vec::new();
        let other = Hash64::from_u64_word(0x4E1A_BE10 ^ n);
        let (their, their_prompt) = backend.job_for_anchor(other).expect("another anchor's job");
        let mut relabelled = palw_attempt_job_v1(their, true);
        relabelled.job_id = anchor;
        relabelled.execution_seed = anchor.as_byte_slice()[..32].try_into().unwrap();
        lies.push(("relabel", backend.execute(&relabelled, &their_prompt).expect("the relabel runs")));
        let short: Vec<usize> = prompt[..prompt.len() - 1].to_vec();
        let mut shortened = job.clone();
        shortened.declared_prefill_tokens -= 1;
        shortened.prompt_token_ids_hash =
            prompt_token_ids_commitment_v1(form, &short.iter().map(|t| *t as u32).collect::<Vec<_>>()).expect("commits");
        let short_run = backend.execute(&shortened, &short).expect("the short prefill runs");
        if kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(profile) {
            // The guard: a held class's attempt is a fold, and only the whole-job helper sees this.
            assert!(misaka_palw_base0::produce::base0_fp_material_decode_v2(&short_run.material).is_ok(), "{label}: a fold");
        }
        lies.push(("short prefill", short_run));
        if let Some(legacy) = legacy {
            let (old, old_prompt) = legacy.job_for_anchor(anchor).expect("the Legacy job");
            let old = palw_attempt_job_v1(old, true);
            assert_ne!(old.context_hash(), job.context_hash(), "{label}: an instance's fields enter the Legacy context");
            lies.push(("Legacy context", legacy.execute(&old, &old_prompt).expect("the Legacy job runs")));
        }
        for (what, lie) in &lies {
            let lie_claim = attempt_claim(anchor, lie);
            assert_eq!(backend.verify_material(&lie.material, lie_claim), MISMATCH, "{label} #{n}: {what} — the material route");
            assert!(!replay_licenses(backend, &job, &prompt, &lie_claim), "{label} #{n}: {what} — the replay route");
        }

        // The drill's step fault, in the middle of the step space: the replay refuses it.
        let capture = misaka_palw_base0::produce::base0_material_decode_any_v1(&honest.material).expect("decodes");
        let leaf = capture.binding().step_leaf_count / 2;
        if let Ok(lying) = backend.execute_with_injected_fault(&job, &prompt, leaf) {
            assert_ne!(lying.execution_root, honest.execution_root, "{label} #{n}: another execution");
            assert!(!replay_licenses(backend, &job, &prompt, &attempt_claim(anchor, &lying)), "{label} #{n}: the step lie");
        }
    }

    // The free-prompt lane: the claim's anchor is its job's id and its job is its own.
    let fp = fp_job(profile, form, fp_prompt, 2);
    let run = backend.execute_free_prompt(&fp, fp_prompt).expect("the free-prompt run").outcome;
    let ctx = misaka_palw_base0::produce::base0_material_decode_any_v1(&run.material).expect("decodes").binding().job_context.clone();
    let fp_claim = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
        output_root: Some(run.output_root),
    };
    assert_eq!(backend.verify_material(&run.material, fp_claim), MATCHES, "{label}: the honest free prompt");
    let fp_ground = PalwClaimRootsV1 { output_root: Some(Hash64::from_u64_word(0xF6A0)), ..fp_claim };
    assert_eq!(backend.verify_material(&run.material, fp_ground), MISMATCH, "{label}: a free prompt under a ground answer");
}

#[test]
fn the_floors_seat_gives_no_valid_to_a_drill_fault() {
    let backend = floor_backend(PalwPromptIdsFormV1::MerkleV1).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let profile = backend.profile().clone();
    let vocab = profile.vocab_size as usize;
    let prompt: Vec<usize> = (0..6).map(|i| (i * 7919 + 1013) % vocab).collect();
    // The floor's Legacy job IS its CoreV1 job: no Legacy lie to tell.
    duty("floor", &backend, None, &profile, &prompt);
}

#[test]
fn the_a16_seat_gives_no_valid_to_a_drill_fault_fold_and_dense() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    let artifact = a16_artifact(128);
    let prompt: Vec<usize> = (0..12).map(|i| (i * 7919 + 1013) % 128).collect();
    for (label, profile) in [
        ("A16 held v7 (fold)", qwen25_a16_profile_v7(a16_geometry(128)).expect("the held row")),
        ("A16 v2 (dense)", qwen25_a16_profile_v2(a16_geometry(128)).expect("the v2 row")),
    ] {
        let canonical = qwen25_a16_held_canonical_v1(profile.n_ctx);
        let backend = a16_backend(&artifact, &profile, canonical).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        let legacy = a16_backend(&artifact, &profile, canonical);
        duty(label, &backend, Some(&legacy), &profile, &prompt);
    }
}

#[test]
fn the_qwen36_seat_gives_no_valid_to_a_drill_fault() {
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v7};
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let held = qwen36_profile_v7(geometry).expect("the held graph-v7 projection");
    let canonical = qwen36_held_canonical_v1(held.n_ctx);
    let backend = qwen36_backend(&artifact, &held, canonical).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    let legacy = qwen36_backend(&artifact, &held, canonical);
    duty("Qwen3.6 held v7", &backend, Some(&legacy), &held, &[3, 1, 4]);
}

/// **Below `CoreV1` the material route judges as before**: a `Legacy` seat does not compare the
/// answer on the material route (its replay still does, SEAT-S2), so no seat of a `Legacy` network
/// moves.
#[test]
fn a_legacy_seat_does_not_bind_the_answer_on_the_material_route() {
    let backend = floor_backend(PalwPromptIdsFormV1::MerkleV1);
    assert_eq!(backend.attempt_rules_v1(), PalwAttemptRulesV1::Legacy);
    let anchor = Hash64::from_u64_word(0x7189_1E6A);
    let (canonical, prompt) = backend.job_for_anchor(anchor).unwrap();
    let job = palw_attempt_job_v1(canonical, true);
    let honest = backend.execute(&job, &prompt).unwrap();
    let ground = PalwClaimRootsV1 { output_root: Some(Hash64::from_u64_word(0x6A0D)), ..attempt_claim(anchor, &honest) };
    assert_eq!(backend.verify_material(&honest.material, ground), MATCHES, "Legacy: the material route as it was");
    assert!(!replay_licenses(&backend, &job, &prompt, &ground), "and the replay still compares the answer");
}
