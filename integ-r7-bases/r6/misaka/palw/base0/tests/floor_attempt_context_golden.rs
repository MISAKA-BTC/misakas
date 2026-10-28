//! **ADR-0152 v3.1 J-5 (the audit's SPEC §4.7, Tier B): the chain's floor rule IS the floor's.**
//!
//! F1 moved the floor's counter-mode prompt loop into consensus core
//! (`palw_attempt_rules_v1::palw_attempt_prompt_ids_v1`) so the chain can derive the job an anchor
//! names — the identity check's J5 — and `base0_rc_job_v1` now calls it. This pins the two readings
//! equal where it matters, against the producer and seat path as they run it: for sixteen anchors and
//! both prompt-id forms, the core `palw_floor_attempt_context_v1` at the floor's canonical job and the
//! prefill draw is byte for byte `palw_attempt_job_v1(job_for_anchor(anchor), true)` — the job a
//! producer runs and a seat's `verify_material` holds a capture to — and the prompt ids are the
//! producer's, id for id. Without the draw it is the whole canonical job, as the backend names it.

use kaspa_consensus_core::palw_attempt_rules_v1::{
    PALW_ATTEMPT_PROMPT_DOMAIN_V1, palw_attempt_canonical_v1, palw_attempt_prompt_ids_v1, palw_floor_attempt_context_v1,
};
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES;
use kaspa_hashes::Hash64;
use misaka_palw_base0::backend::Base0Backend;

/// The floor, resolved as a node resolves it, at `form`.
fn floor_backend(form: PalwPromptIdsFormV1) -> Base0Backend {
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    let court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 4, 2).expect("the shipped court");
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("the floor is registered");
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
    Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("the floor resolves"))
        .with_step_ladder_cap(court.max_step_leaf_count())
        .with_prompt_ids_form(form)
}

#[test]
fn the_core_floor_context_is_the_producers_for_sixteen_anchors() {
    assert_eq!(
        misaka_palw_base0::produce::PALW_BASE0_DOMAIN_JOB_PROMPT,
        PALW_ATTEMPT_PROMPT_DOMAIN_V1,
        "one domain, re-exported, never re-typed"
    );
    assert_eq!(PALW_ATTEMPT_PROMPT_DOMAIN_V1, b"misaka-palw/base0/rc-job-prompt/v1", "the bytes the floor always used");
    for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
        let backend = floor_backend(form);
        let profile = backend.profile();
        assert_eq!(
            palw_attempt_canonical_v1(profile, true),
            Some(PALW_RC_BASE0_CANONICAL),
            "the chain's canonical job is the floor's"
        );
        for n in 0u64..16 {
            let anchor = Hash64::from_u64_word(0xF1_0000_0000 ^ (n.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
            let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the floor implies a job");
            let (core, ids) = palw_floor_attempt_context_v1(profile, &anchor, PALW_RC_BASE0_CANONICAL, form, true)
                .expect("a canonical prompt commits");
            assert_eq!(core, palw_attempt_job_v1(canonical.clone(), true), "{form:?} anchor {n}: the drawn job, field for field");
            assert_eq!(core.context_hash(), palw_attempt_job_v1(canonical.clone(), true).context_hash());
            let producer_ids: Vec<u32> = prompt.iter().map(|id| *id as u32).collect();
            assert_eq!(ids, producer_ids, "{form:?} anchor {n}: the prompt, id for id");
            assert_eq!(
                palw_attempt_prompt_ids_v1(&anchor, u64::from(profile.vocab_size), PALW_RC_BASE0_CANONICAL.0),
                producer_ids,
                "the profile's vocabulary is the artifact's"
            );
            let (whole, _) = palw_floor_attempt_context_v1(profile, &anchor, PALW_RC_BASE0_CANONICAL, form, false).expect("commits");
            assert_eq!(whole, canonical, "{form:?} anchor {n}: without the draw, the whole canonical job");
        }
    }
}
