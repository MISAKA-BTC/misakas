//! **ADR-0152 v3.1 F1-M (addendum §4-bis.1, Tier B): under `CoreV1` every family's producer runs
//! the chain's attempt rule, and the chain's identity checks pass on what it runs.**
//!
//! The floor, the held A16 graph-v7 fixture and a held Qwen3.6 graph-v7 fixture, each backend built
//! the way the SDK builds it and switched to `CoreV1` (what a node on testnet-12 does from its
//! params):
//!
//! * `job_for_anchor` IS `palw_attempt_job_for_anchor_v1` over the class's profile at the formula
//!   canonical — context and prompt, byte for byte — for sixteen anchors; its drawn job IS the
//!   `palw_attempt_context_v1` J5 derives;
//! * a real run of that job commits `palw_int_activation_leg_root_v1` (J6), the class's canonical
//!   checkpoint profile (J7) and the one rendered output root (`palw_attempt_output_root_v1`);
//! * the chain's identity rule finds nothing wrong with the honest run (J1–J7 on the producer's own
//!   binding) and names a relabel of another anchor's prompt (J5b) — and the seat's own rule
//!   (`verify_material`, SEAT-S1) agrees on both;
//! * nothing an artifact or an instance holds enters the context: two backends of one class on two
//!   different network ids derive the identical job under `CoreV1` (under `Legacy` they do not).
//!
//! And, context only, the real t12 rows (A16 at 8,192 and 2,097,152) and Qwen3.6 at 512: the formula
//! is the held canonical, the prompt is `p` ids of the core loop, and the 2M row's prompt is past
//! J5b's inline bound (its root is `PromptNotAnchored`'s to check).

mod common;

use common::*;
use kaspa_consensus_core::palw_attempt_rules_v1::{
    PALW_J5_INLINE_PROMPT_IDS_V1, PalwAttemptRulesV1, palw_attempt_canonical_v1, palw_attempt_context_v1,
    palw_attempt_job_for_anchor_v1, palw_attempt_output_root_v1, palw_canonical_checkpoint_profile_v1,
    palw_int_activation_leg_root_v1,
};
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PalwClaimSourceKindV1, PalwIdentityFaultV1, PalwIdentityRulesV1, PalwOffenceTargetV1, palw_binding_identity_fault_v1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

fn anchors() -> impl Iterator<Item = Hash64> {
    (0u64..16).map(|n| Hash64::from_u64_word(0xC0_4E00_0000 ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15)))
}

/// The chain's view of a claim carrying `outcome`'s roots, recorded under `anchor`.
fn target_of(
    profile: &PalwShapeProfileV3,
    outcome: &kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
    anchor: Hash64,
) -> PalwOffenceTargetV1 {
    PalwOffenceTargetV1 {
        claim_id: Hash64::from_u64_word(0xC1A1),
        class_id: profile.shape_profile_id(),
        artifact_root: Hash64::default(),
        executor_bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0)),
        execution_root: outcome.execution_root,
        lane: Some(PalwClaimSourceKindV1::Attempt),
        segment_count: Some(4),
        phase: None,
        job_identity: anchor,
        trace_root: outcome.trace_root,
        output_root: outcome.output_root,
    }
}

/// One family, sixteen anchors: the job, the run's legs and roots, the chain's identity rule and
/// the seat, all under `CoreV1`; the relabel refused by both.
fn check_family(
    label: &str,
    backend: &dyn PalwExecutionBackendV1,
    profile: &PalwShapeProfileV3,
    is_base: bool,
    form: PalwPromptIdsFormV1,
) {
    let canonical = palw_attempt_canonical_v1(profile, is_base).expect("a context wide enough for the formula");
    let rules = PalwIdentityRulesV1 {
        prompt_ids_form: form,
        base_class_id: if is_base { profile.shape_profile_id() } else { Hash64::from_u64_word(0xF100) },
    };
    for (n, anchor) in anchors().enumerate() {
        let (job, prompt) = backend.job_for_anchor(anchor).expect("the anchor implies a job");
        let (core, core_prompt) = palw_attempt_job_for_anchor_v1(profile, &anchor, canonical, form).expect("the prompt commits");
        assert_eq!(job, core, "{label} anchor {n}: job_for_anchor is CoreV1's, field for field");
        assert_eq!(prompt, core_prompt, "{label} anchor {n}: the core prompt");
        let drawn = palw_attempt_job_v1(job.clone(), true);
        assert_eq!(
            drawn,
            palw_attempt_context_v1(profile, &anchor, canonical, job.prompt_token_ids_hash),
            "{label}: the drawn job is J5's"
        );
        assert_eq!(
            (job.model_profile_id, job.runtime_class_id, job.tokenizer_id, job.job_nullifier),
            Default::default(),
            "{label}: no artifact field"
        );
        if n >= 3 {
            continue; // the runs below are the expensive half; three anchors per family carry them
        }
        let outcome = backend.execute(&drawn, &prompt).expect("the drawn job runs");
        let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&outcome.material).expect("the capture decodes");
        let binding = retention.binding().clone();
        assert_eq!(binding.committed_execution_root, outcome.execution_root);
        assert_eq!(binding.activation_leg_root, palw_int_activation_leg_root_v1(&drawn), "{label}: J6's statement");
        assert_eq!(binding.checkpoint_profile, palw_canonical_checkpoint_profile_v1(profile), "{label}: J7's profile");
        assert_eq!(
            outcome.output_root,
            palw_attempt_output_root_v1(&drawn, retention.generated_token_ids()),
            "{label}: the one rendered rule"
        );
        let target = target_of(profile, &outcome, anchor);
        assert_eq!(
            palw_binding_identity_fault_v1(&target, &binding, rules, true),
            Ok(None),
            "{label} anchor {n}: the chain finds nothing"
        );
        let claim = PalwClaimRootsV1 {
            execution_root: outcome.execution_root,
            trace_root: outcome.trace_root,
            anchor,
            attempt_draw: Some(true),
            output_root: Some(outcome.output_root),
            job_pin: None,
        };
        assert_eq!(backend.verify_material(&outcome.material, claim), PalwMaterialVerdictV1::Matches, "{label}: the seat licenses it");

        // The relabel: another anchor's prompt, run with THIS anchor written as its job and seed.
        let other = Hash64::from_u64_word(0x4E1A_BE10 ^ n as u64);
        let (their, their_prompt) = backend.job_for_anchor(other).expect("the other anchor's job");
        let mut relabelled = palw_attempt_job_v1(their, true);
        relabelled.job_id = anchor;
        relabelled.execution_seed = anchor.as_byte_slice()[..32].try_into().unwrap();
        let lie = backend.execute(&relabelled, &their_prompt).expect("the relabelled job runs");
        let lie_binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&lie.material).expect("decodes").binding().clone();
        let lie_target = target_of(profile, &lie, anchor);
        assert_eq!(
            palw_binding_identity_fault_v1(&lie_target, &lie_binding, rules, true),
            Ok(Some(if canonical.0 <= PALW_J5_INLINE_PROMPT_IDS_V1 {
                PalwIdentityFaultV1::PromptNotTheAnchors
            } else {
                unreachable!("fixtures are narrow")
            })),
            "{label} anchor {n}: J5b names the relabel"
        );
        let lie_claim = PalwClaimRootsV1 {
            execution_root: lie.execution_root,
            trace_root: lie.trace_root,
            anchor,
            attempt_draw: Some(true),
            output_root: Some(lie.output_root),
            job_pin: None,
        };
        assert_eq!(
            backend.verify_material(&lie.material, lie_claim),
            PalwMaterialVerdictV1::Mismatch,
            "{label}: the seat refuses it too"
        );
    }
}

/// The held A16 fixture (graph-v7, n_ctx 128 → the formula's (15, 2)), under `CoreV1`.
fn a16_core_v1(vocab: u32) -> (Qwen25A16Backend, PalwShapeProfileV3) {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    let artifact = a16_artifact(vocab);
    let held = qwen25_a16_profile_v7(a16_geometry(vocab)).expect("the held graph-v7 row projects");
    let backend =
        a16_backend(&artifact, &held, qwen25_a16_held_canonical_v1(held.n_ctx)).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    (backend, held)
}

use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;

#[test]
fn the_floor_runs_core_v1_byte_for_byte() {
    for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
        let mut legacy = floor_backend(form);
        let profile = legacy.profile().clone();
        // The floor's Legacy rule IS CoreV1: the setter moves nothing.
        let before: Vec<_> = anchors().map(|a| legacy.job_for_anchor(a).unwrap()).collect();
        legacy.set_attempt_rules_v1(PalwAttemptRulesV1::CoreV1);
        let after: Vec<_> = anchors().map(|a| legacy.job_for_anchor(a).unwrap()).collect();
        assert_eq!(before, after, "{form:?}: the floor has one rule");
        check_family("floor", &legacy, &profile, true, form);
    }
}

#[test]
fn the_held_a16_fixture_runs_core_v1() {
    for vocab in [128u32, 8_292] {
        let (backend, held) = a16_core_v1(vocab);
        check_family(&format!("A16 held v7 (V={vocab})"), &backend, &held, false, PalwPromptIdsFormV1::MerkleV1);
    }
}

#[test]
fn the_held_qwen36_fixture_runs_core_v1() {
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v7};
    let (artifact, mut geometry) = qwen36_fixture();
    // The dev fixture's table covers 32 positions; at the formula's width the held row is (3, 2).
    geometry.n_ctx = 32;
    let held = qwen36_profile_v7(geometry).expect("the held graph-v7 projection");
    let canonical = qwen36_held_canonical_v1(held.n_ctx);
    assert_eq!(canonical, (3, 2));
    let backend = qwen36_backend(&artifact, &held, canonical).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    check_family("Qwen3.6 held v7", &backend, &held, false, PalwPromptIdsFormV1::MerkleV1);
}

/// **No instance field enters the job** (addendum §1, "two files with the same inventory root give
/// different contexts today"): two backends of one class on different network ids derive different
/// Legacy jobs and the identical CoreV1 job.
#[test]
fn an_instances_own_fields_do_not_enter_the_core_v1_job() {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    let artifact = a16_artifact(128);
    let held = qwen25_a16_profile_v7(a16_geometry(128)).unwrap();
    let canonical = qwen25_a16_held_canonical_v1(held.n_ctx);
    let on = |net: &[u8]| {
        Qwen25A16Backend::new(artifact.clone(), net.to_vec(), held.clone(), canonical)
            .expect("serves")
            .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
    };
    let anchor = Hash64::from_u64_word(0x5A7E);
    let (a, b) = (on(b"testnet-12"), on(b"a-private-drill"));
    assert_ne!(a.job_for_anchor(anchor).unwrap(), b.job_for_anchor(anchor).unwrap(), "Legacy reads the instance's network id");
    let (a, b) = (a.with_attempt_rules(PalwAttemptRulesV1::CoreV1), b.with_attempt_rules(PalwAttemptRulesV1::CoreV1));
    assert_eq!(a.job_for_anchor(anchor).unwrap(), b.job_for_anchor(anchor).unwrap(), "CoreV1 reads the profile and the anchor alone");
    // A registration whose canonical is not the formula is refused under CoreV1, never produced.
    let off = Qwen25A16Backend::new(artifact.clone(), b"n".to_vec(), held.clone(), (canonical.0 - 1, 2))
        .unwrap()
        .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    assert!(off.job_for_anchor(anchor).is_err(), "a non-formula canonical names a job the chain would call another's");
}

/// **The real t12 rows, context only**: the formula is each row's held canonical; the 8k prompt
/// (1,023 ids) sits under J5b's inline bound and the 2M prompt (262,143 ids) is past it.
#[test]
fn the_real_rows_take_the_formula_context() {
    use kaspa_consensus_core::palw_context_ladder::{palw_a16_context_row_profile_v7, palw_qwen36_context_row_profile_v7};
    let rows = [
        ("A16@8192", palw_a16_context_row_profile_v7(8_192).unwrap(), (1_023u32, 2u32)),
        ("A16@2M", palw_a16_context_row_profile_v7(2_097_152).unwrap(), (262_143, 2)),
        ("Q36@512", palw_qwen36_context_row_profile_v7(512).unwrap(), (63, 2)),
    ];
    let anchor = Hash64::from_u64_word(0x7E12);
    for (label, profile, want) in rows {
        let canonical = palw_attempt_canonical_v1(&profile, false).expect("wide enough");
        assert_eq!(canonical, want, "{label}");
        if canonical.0 > 70_000 {
            assert!(canonical.0 > PALW_J5_INLINE_PROMPT_IDS_V1, "{label}: its prompt root is PromptNotAnchored's");
            continue; // deriving 2^18 ids here adds nothing the 8k row does not show
        }
        let (job, prompt) = palw_attempt_job_for_anchor_v1(&profile, &anchor, canonical, PalwPromptIdsFormV1::MerkleV1).unwrap();
        assert_eq!(prompt.len(), canonical.0 as usize, "{label}");
        assert!(prompt.iter().all(|id| (*id as u32) < profile.vocab_size), "{label}: ids in the vocabulary");
        assert_eq!((job.max_context_tokens, job.shape_profile_id), (profile.n_ctx, profile.shape_profile_id()), "{label}");
        assert!(canonical.0 <= PALW_J5_INLINE_PROMPT_IDS_V1, "{label}: J5b recomputes it inline");
    }
}

/// **Both lanes** (addendum §4-bis.2: J6 and J7 hold free prompts too). A free-prompt run of every
/// family the chain uses — the floor, the held A16 v7 (a fold), the per-call A16 v2 (dense) and the
/// held Qwen3.6 v7 — at one and at three decode calls commits J6's activation leg and J7's
/// checkpoint profile over its OWN context and its output root by the one rendered rule, and the
/// identity rule finds nothing on its binding: no leg check and no output rule can convict an honest
/// free-prompt producer.
#[test]
fn every_familys_free_prompt_run_is_held_to_the_same_legs() {
    use kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1;
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v2, qwen25_a16_profile_v7};
    use kaspa_consensus_core::palw_qwen36_profile::{qwen36_held_canonical_v1, qwen36_profile_v7};
    let form = PalwPromptIdsFormV1::MerkleV1;
    let check = |label: &str, backend: &dyn PalwExecutionBackendV1, profile: &PalwShapeProfileV3, is_base: bool, prompt: &[usize]| {
        let rules = PalwIdentityRulesV1 {
            prompt_ids_form: form,
            base_class_id: if is_base { profile.shape_profile_id() } else { Hash64::from_u64_word(0xF100) },
        };
        for decode in [1u32, 3] {
            let job = fp_job(profile, form, prompt, decode);
            let out = backend.execute_free_prompt(&job, prompt).expect("the free-prompt run").outcome;
            let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&out.material).expect("the capture decodes");
            let binding = retention.binding().clone();
            let ctx = &binding.job_context;
            assert_eq!(binding.activation_leg_root, palw_int_activation_leg_root_v1(ctx), "{label} D={decode}: J6's statement");
            assert_eq!(binding.checkpoint_profile, palw_canonical_checkpoint_profile_v1(profile), "{label} D={decode}: J7's profile");
            assert_eq!(
                out.output_root,
                palw_attempt_output_root_v1(ctx, retention.generated_token_ids()),
                "{label} D={decode}: the one rendered rule"
            );
            let target = PalwOffenceTargetV1 {
                lane: Some(PalwClaimSourceKindV1::FreePrompt),
                job_identity: palw_fp_job_pin_of_context_v1(ctx),
                ..target_of(profile, &out, Hash64::default())
            };
            assert_eq!(
                palw_binding_identity_fault_v1(&target, &binding, rules, true),
                Ok(None),
                "{label} D={decode}: nothing to find"
            );
        }
    };
    let floor = floor_backend(form);
    let floor_profile = floor.profile().clone();
    let vocab = floor_profile.vocab_size as usize;
    let prompt: Vec<usize> = (0..6).map(|i| (i * 7919 + 1013) % vocab).collect();
    check("floor", &floor, &floor_profile, true, &prompt);
    let artifact = a16_artifact(128);
    let prompt: Vec<usize> = (0..12).map(|i| (i * 7919 + 1013) % 128).collect();
    for (label, profile) in [
        ("A16 held v7 (fold)", qwen25_a16_profile_v7(a16_geometry(128)).expect("the held row")),
        ("A16 v2 (dense)", qwen25_a16_profile_v2(a16_geometry(128)).expect("the v2 row")),
    ] {
        let backend = a16_backend(&artifact, &profile, qwen25_a16_held_canonical_v1(profile.n_ctx))
            .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        check(label, &backend, &profile, false, &prompt);
    }
    let (artifact, mut geometry) = qwen36_fixture();
    geometry.n_ctx = 32;
    let held = qwen36_profile_v7(geometry).expect("the held graph-v7 projection");
    let backend =
        qwen36_backend(&artifact, &held, qwen36_held_canonical_v1(held.n_ctx)).with_attempt_rules(PalwAttemptRulesV1::CoreV1);
    check("Qwen3.6 held v7", &backend, &held, false, &[3, 1, 4]);
}
