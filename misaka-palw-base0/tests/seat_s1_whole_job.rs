//! **SEAT-S1: a held attempt served as a fold answers the WHOLE job the block asked for**
//! (f1c_f1m spec §4-bis.10, S-1; ADR-0117).
//!
//! On this line a held class's attempt folds (`palw_attempt_capture_folds_v1`), and the fold branch
//! of `verify_material` checked only `job_id == anchor` and the profile id, where the dense branch
//! also required the material's context to be `palw_attempt_job_v1(job_for_anchor(anchor), draw)`.
//! So a producer could run a SMALLER job under the right id — a shortened prefill, the decode call
//! skipped, a context field of its own — fold it, and every seat holding the fold licensed it.
//!
//! Each forgery here is an honest execution of a job that is not the block's: its roots are its
//! own (`base0_fp_material_matches_claim_v2` reads `Ok(true)` — the whole of what the fold branch
//! checked after the id), and the seat's verdict is `Mismatch`. The honest held attempt keeps
//! `Matches`, and a free-prompt fold (`attempt_draw == None`) keeps its old rule.

mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::produce::{Base0SeatFamilyV1, base0_fp_material_decode_v2, base0_fp_material_matches_claim_v2};

/// A job the block did not ask for, and the prompt it runs on — each keeps `job_id == anchor`.
fn forgeries(
    job: &PalwJobContextV2,
    prompt: &[usize],
    form: PalwPromptIdsFormV1,
) -> Vec<(&'static str, PalwJobContextV2, Vec<usize>)> {
    let mut out = Vec::new();
    // The prefill shortened by one position, its prompt root re-committed so the context is a
    // consistent description of what ran.
    let short: Vec<usize> = prompt[..prompt.len() - 1].to_vec();
    let ids: Vec<u32> = short.iter().map(|t| *t as u32).collect();
    out.push((
        "shortened prefill",
        PalwJobContextV2 {
            declared_prefill_tokens: job.declared_prefill_tokens - 1,
            prompt_token_ids_hash: prompt_token_ids_commitment_v1(form, &ids).expect("the ids commit"),
            ..job.clone()
        },
        short,
    ));
    // The decode call skipped (a two-token job answered with one) or one added.
    let decode = if job.exact_decode_tokens > 1 { job.exact_decode_tokens - 1 } else { job.exact_decode_tokens + 1 };
    out.push(("decode count moved", PalwJobContextV2 { exact_decode_tokens: decode, ..job.clone() }, prompt.to_vec()));
    // A free context field the producer chose: the nullifier.
    out.push((
        "free context field",
        PalwJobContextV2 { job_nullifier: Hash64::from_u64_word(0xF1E1D), ..job.clone() },
        prompt.to_vec(),
    ));
    out
}

fn check_backend(label: &str, backend: &dyn PalwExecutionBackendV1, form: PalwPromptIdsFormV1, family: Base0SeatFamilyV1) {
    for (anchor, draw) in [(0x51_0001u64, true), (0x51_0002, false)] {
        let anchor = Hash64::from_u64_word(anchor);
        let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let job = palw_attempt_job_v1(canonical, draw);
        let roots_of = |out: &kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1| PalwClaimRootsV1 {
            execution_root: out.execution_root,
            trace_root: out.trace_root,
            anchor,
            attempt_draw: Some(draw),
        };

        // The honest held attempt: a fold, licensed.
        let honest = backend.execute(&job, &prompt).expect("the honest attempt runs");
        assert!(base0_fp_material_decode_v2(&honest.material).is_ok(), "{label}: a held attempt retains a fold");
        assert_eq!(
            backend.verify_material(&honest.material, roots_of(&honest)),
            PalwMaterialVerdictV1::Matches,
            "{label} draw={draw}: the honest held attempt is licensed"
        );

        for (name, forged_job, forged_prompt) in forgeries(&job, &prompt, form) {
            assert_eq!(forged_job.job_id, anchor, "{label} {name}: the forgery keeps the anchor's id");
            let out = backend.execute(&forged_job, &forged_prompt).expect("the forged job runs");
            let folded = base0_fp_material_decode_v2(&out.material).expect("a fold");
            // Everything the fold branch read after the id holds: the roots are the forgery's own.
            assert_eq!(
                base0_fp_material_matches_claim_v2(&folded, out.execution_root, out.trace_root, family),
                Ok(true),
                "{label} {name}: the forged fold is self-consistent — only the job refuses it"
            );
            assert_eq!(
                backend.verify_material(&out.material, roots_of(&out)),
                PalwMaterialVerdictV1::Mismatch,
                "{label} draw={draw} {name}: a fold of a job the block did not ask for is not licensed"
            );
            // The rule is the attempt lane's: without a draw (a free-prompt claim, whose job is its
            // own) the id alone decides, as before, and the self-consistent fold is licensed.
            let fp_roots = PalwClaimRootsV1 { attempt_draw: None, ..roots_of(&out) };
            assert_eq!(
                backend.verify_material(&out.material, fp_roots),
                PalwMaterialVerdictV1::Matches,
                "{label} {name}: without a draw the whole-job rule does not apply"
            );
        }
    }
}

#[test]
fn a16_held_fold_attempt_of_a_smaller_job_is_refused_and_the_honest_one_licensed() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    for vocab in [128u32, 8_292] {
        let artifact = a16_artifact(vocab);
        let held = qwen25_a16_profile_v7(a16_geometry(vocab)).expect("the held graph-v7 row projects");
        assert!(palw_attempt_capture_folds_v1(&held), "the held row's attempt folds");
        let backend = a16_backend(&artifact, &held, qwen25_a16_held_canonical_v1(held.n_ctx));
        check_backend(&format!("A16 held v7 V={vocab}"), &backend, backend.prompt_ids_form(), Base0SeatFamilyV1::IntegerKv);
    }
}

#[test]
fn qwen36_held_fold_attempt_of_a_smaller_job_is_refused_and_the_honest_one_licensed() {
    use kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7;
    let (artifact, geometry) = qwen36_fixture();
    let held = qwen36_profile_v7(geometry).expect("the held graph-v7 projection");
    assert!(palw_attempt_capture_folds_v1(&held), "the held hybrid's attempt folds");
    let backend = qwen36_backend(&artifact, &held, (3, 4));
    check_backend("Qwen3.6 held v7", &backend, backend.prompt_ids_form(), Base0SeatFamilyV1::Qwen36);
}

/// The dense branch and the floor answer by the same function: a dense attempt of a smaller job
/// under the right id is refused there too (the rule it always had), and the honest one licensed.
#[test]
fn the_dense_branches_share_the_rule() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2;
    let artifact = a16_artifact(128);
    let per_call = qwen25_a16_profile_v2(a16_geometry(128)).expect("the v2 row projects");
    let a16 = a16_backend(&artifact, &per_call, (15, 2));
    let floor = floor_backend(PalwPromptIdsFormV1::MerkleV1);
    for (label, backend) in [("A16 v2 dense", &a16 as &dyn PalwExecutionBackendV1), ("floor", &floor)] {
        let anchor = Hash64::from_u64_word(0x51_DE45);
        let (canonical, prompt) = backend.job_for_anchor(anchor).expect("a job");
        let job = palw_attempt_job_v1(canonical, false);
        let honest = backend.execute(&job, &prompt).expect("runs");
        let roots = PalwClaimRootsV1 {
            execution_root: honest.execution_root,
            trace_root: honest.trace_root,
            anchor,
            attempt_draw: Some(false),
        };
        assert_eq!(backend.verify_material(&honest.material, roots), PalwMaterialVerdictV1::Matches, "{label}: honest");
        let skipped = PalwJobContextV2 { exact_decode_tokens: job.exact_decode_tokens - 1, ..job.clone() };
        let out = backend.execute(&skipped, &prompt).expect("the smaller job runs");
        let roots = PalwClaimRootsV1 { execution_root: out.execution_root, trace_root: out.trace_root, ..roots };
        assert_eq!(backend.verify_material(&out.material, roots), PalwMaterialVerdictV1::Mismatch, "{label}: a skipped decode call");
    }
}
