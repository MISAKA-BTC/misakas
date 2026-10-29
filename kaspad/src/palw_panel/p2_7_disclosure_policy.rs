//! **`palw_panel`'s `p2_7_disclosure_policy` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

//! **ADR-0152 X7 (Phase 2, P2-7): when the node answers a disclosure duty, what it answers from,
//! and how its queue keys an answer.** The duty list and the answer object are consensus-core's
//! (`palw_disclosure_duties_v1`, `palw_da_answer_object_v1`) and are carried through the gate and
//! the fold on real claims in `t46_false_valid_real_claim::m3_da_court`; this is the node's own
//! policy around them — the material loader and the unit builders run here on the floor's fixture
//! backend (kept → verified → re-made → nothing), and the tick's loop is pinned where it runs.
use super::*;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
use kaspa_consensus_core::palw_producer_v2::{PalwDisclosureDutyV1, PalwDisclosureRoleV1};

fn duty(role: PalwDisclosureRoleV1, signer_rank: u16, unit: PalwDaUnitV1, deadline_daa: u64, window: u64) -> PalwDisclosureDutyV1 {
    let bond = PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(9), 0));
    PalwDisclosureDutyV1 {
        claim_id: Hash64::from_u64_word(1),
        accepted_block: Hash64::from_u64_word(2),
        class_id: Hash64::from_u64_word(3),
        artifact_root: Hash64::from_u64_word(4),
        executor_bond: bond,
        discloser: bond,
        role,
        signer_rank,
        unit,
        deadline_daa,
        disclose_window_daa: window,
        in_run_rows: 1,
        trace_root: Hash64::default(),
        execution_root: Hash64::default(),
        free_prompt: false,
        job_identity: Hash64::default(),
    }
}

/// **The producer answers at once; a covering signer waits out the producer's half of the
/// window, staggered by its rank, and never later than a quarter-window before the deadline;
/// nobody answers past the deadline.** testnet-12's `W_disclose` of 1,200 DAA on a session opened
/// at 1,000: the producer from 1,000 to 2,200; the rank-0 signer from 1,600 (600 left), rank 1
/// from 1,675, rank 4 and every later rank from 1,900 (300 left); and a devnet-short window keeps
/// the same shape.
#[test]
fn the_producer_answers_at_once_and_a_covering_signer_after_its_turn() {
    let unit = PalwDaUnitV1::Event { row: 0, tile: 0 };
    let producer = duty(PalwDisclosureRoleV1::Producer, 0, unit, 2_200, 1_200);
    for at in [1_000, 1_599, 2_200] {
        assert!(palw_disclosure_due_v1(&producer, at), "the producer at {at}");
    }
    assert!(!palw_disclosure_due_v1(&producer, 2_201), "past the deadline the session has defaulted");
    for (rank, first) in [(0u16, 1_600u64), (1, 1_675), (2, 1_750), (3, 1_825), (4, 1_900), (5, 1_900), (40, 1_900)] {
        let signer = duty(PalwDisclosureRoleV1::CoveringSigner, rank, unit, 2_200, 1_200);
        assert!(!palw_disclosure_due_v1(&signer, first - 1), "rank {rank} waits until {first}");
        assert!(palw_disclosure_due_v1(&signer, first), "rank {rank} answers from {first}");
        assert!(palw_disclosure_due_v1(&signer, 2_200) && !palw_disclosure_due_v1(&signer, 2_201), "rank {rank}: to the deadline");
    }
    let short = duty(PalwDisclosureRoleV1::CoveringSigner, 0, unit, 40, 20);
    assert!(!palw_disclosure_due_v1(&short, 29) && palw_disclosure_due_v1(&short, 30), "half of a short window");
    let degenerate = duty(PalwDisclosureRoleV1::CoveringSigner, 3, unit, 5, 0);
    assert!(
        palw_disclosure_due_v1(&degenerate, 5) && !palw_disclosure_due_v1(&degenerate, 6),
        "a zero window still ends at its deadline"
    );
}

/// A floor free-prompt claim the fixture backend ran: the backend, the claim's facts, its job and
/// prompt, and the honest capture.
fn fp_claim() -> (
    misaka_palw_base0::backend::Base0Backend,
    PalwDaClaimFactsV1,
    kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    Vec<u32>,
    Vec<u8>,
) {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    let backend = super::seat_s_tests::floor_backend();
    let (job, ids) = super::seat_s_tests::floor_fp_job(&backend);
    let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
    let run = backend.execute_free_prompt(&job, &prompt).expect("the producer's run");
    let facts = PalwDaClaimFactsV1 {
        claim_id: Hash64::from_u64_word(0xC1),
        class_id: job.class_id,
        executor_bond: PalwBondKeyV2(job.executor_bond),
        execution_root: run.outcome.execution_root,
        trace_root: run.facts.full_logits_trace_root,
        work_leaves: run.facts.step_leaf_count,
        form: backend.prompt_ids_form(),
        lane: PalwDaLaneV1::FreePrompt { panel_da_admissible: false },
        // No recorded identity (a fixture claim): H-2's check is skipped, as for any caller
        // without the claim's record.
        job_pin: None,
    };
    (backend, facts, job, ids, run.outcome.material)
}

/// Answer `units` of `facts`'s claim from `kept` on the fixture backend: what was built, and what
/// was handed to `keep`.
fn answer(
    backend: &misaka_palw_base0::backend::Base0Backend,
    facts: &PalwDaClaimFactsV1,
    kept: Vec<Vec<u8>>,
    units: &[PalwDaUnitV1],
    flat: bool,
) -> (Result<PalwDaClaimAnswersV1, String>, Option<Vec<u8>>) {
    let mut kept_back = None;
    let built = palw_da_claim_answers_v1(backend, facts, kept, |bytes| kept_back = Some(bytes.to_vec()), units, 2, flat, None);
    (built, kept_back)
}

/// **The P2-7 review's HIGH: an R-core answer is built from a capture that reproduces the claim's
/// roots, or from one re-made by replaying the claim's job — never from a payload that merely
/// decodes as `FPC1`.** On the floor's fixture backend, one free-prompt claim:
///
/// 1. an honest `FPC1` kept: answered from as it is, nothing replayed or rewritten;
/// 2. a PLANTED `FPC1` — the claim's job, a garbage capture — that the decoder accepts: never
///    answered from; the job it carries is replayed, and the verified capture (byte for byte the
///    honest payload) is handed to `keep` to replace it; with an honest copy kept beside it, that
///    copy answers and nothing is replayed;
/// 3. the job alone (`FPM1`, what a full seat that licensed by replay keeps): re-made and kept as
///    the claim's `FPC1`;
/// 4. nothing, a stranger's job (the claim's class and producer, another nonce), another
///    producer's job: no answer and nothing kept — and a stranger's job before the claim's costs a
///    replay, never the answer.
///
/// Before the fix case 2 answered from the garbage (the loader's guard was the decoder alone), so
/// it is red without it.
/// **H-2 on the answering path (the PANEL ⋈ M2 Phase 3 merge):** a free-prompt answer is built
/// only from a capture whose context reproduces the claim's recorded job pin
/// (`PalwDisclosureDutyV1::fp_job_pin_v1` → `PalwDaClaimFactsV1::job_pin` → the roots'
/// `job_pin`). The honest capture of a claim whose record names ANOTHER job's pin is neither
/// answered from nor re-made from its job (the re-make reproduces the same context), and nothing
/// is kept — the fold's identity checks (J1) would refuse that answer. Red if `roots_v1` drops
/// the pin.
#[test]
fn a_free_prompt_answer_needs_the_claims_recorded_job_pin() {
    use kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1;
    let (backend, mut facts, job, ids, capture) = fp_claim();
    let honest = palw_fp_capture_encode_v1(&job, &ids, &capture);
    let event = [PalwDaUnitV1::Event { row: 0, tile: 0 }];
    let (built, _) = answer(&backend, &facts, vec![honest.clone()], &event, false);
    assert!(!built.expect("no pin recorded: answered").remade);
    facts.job_pin = Some(Hash64::from_u64_word(0x0B0B));
    let (built, kept) = answer(&backend, &facts, vec![honest], &event, false);
    assert!(built.is_err() && kept.is_none(), "another job's pin: no answer, nothing kept");
    facts.lane = PalwDaLaneV1::Attempt { anchor: Hash64::default(), attempt_draw: None, job: None };
    assert_eq!(facts.roots_v1(None).job_pin, None, "the attempt lane never carries a free-prompt pin");
}

#[test]
fn a_public_job_answer_serves_only_the_pinned_descriptor_from_retained_fp_material() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpCaptureV1, PalwFpMaterialV1};
    use kaspa_consensus_core::palw_legacy_held_da_v2::{PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2};
    let (backend, mut facts, job, ids, capture) = fp_claim();
    let binding = backend.disclose_trace_event(&capture, u32::MAX, 0).unwrap().binding().clone();
    facts.job_pin = Some(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context));
    let material = PalwDaCaptureV1::FreePrompt(PalwFpCaptureV1 {
        material: PalwFpMaterialV1 { job: job.clone(), prompt_token_ids: ids },
        capture,
    });
    let answer = palw_da_legacy_answer_v2(&backend, &facts, &material, &PalwLegacyHeldUnitV2::FpJob).unwrap();
    let PalwDaBuiltAnswerV1::LegacyHeldV2(opened) = answer else { panic!("a signed tag-158 descriptor is required") };
    assert_eq!(opened.0, binding);
    assert_eq!(opened.1, PalwLegacyHeldAnswerV2::FpJob(Box::new(job)));
    // The enum's sole payload is the public job; no prompt ids or capture tail can be served.
    facts.job_pin = Some(Hash64::from_u64_word(0xBAD));
    assert!(palw_da_legacy_answer_v2(&backend, &facts, &material, &PalwLegacyHeldUnitV2::FpJob).is_err());
    facts.lane = PalwDaLaneV1::Attempt { anchor: Hash64::default(), attempt_draw: None, job: None };
    assert!(palw_da_legacy_answer_v2(&backend, &facts, &material, &PalwLegacyHeldUnitV2::FpJob).is_err());
}

#[test]
fn a_da_answer_is_built_from_a_verified_or_re_made_capture_never_a_decodable_one() {
    use kaspa_consensus_core::palw_freeprompt_v3::{palw_fp_capture_decode_v1, palw_fp_capture_encode_v1, palw_fp_material_encode_v1};
    let (backend, facts, job, ids, capture) = fp_claim();
    let honest = palw_fp_capture_encode_v1(&job, &ids, &capture);
    let garbage = palw_fp_capture_encode_v1(&job, &ids, b"a capture nobody checked");
    assert!(palw_fp_capture_decode_v1(&garbage, facts.form).is_some(), "the planted payload decodes as the claim's FPC1");
    let job_only = palw_fp_material_encode_v1(&job, &ids);
    let event = [PalwDaUnitV1::Event { row: 0, tile: 0 }];
    // (1) Honest.
    let (built, kept) = answer(&backend, &facts, vec![honest.clone()], &event, false);
    let built = built.expect("answered from the honest copy");
    assert!(!built.remade && kept.is_none(), "nothing replayed, nothing rewritten");
    let honest_answer = built.answers[0].clone().expect("asked").expect("the event opens");
    // (2) Planted.
    let (built, kept) = answer(&backend, &facts, vec![garbage.clone()], &event, false);
    let built = built.expect("re-made from the job the planted payload carries");
    assert!(built.remade, "the garbage capture is never answered from");
    assert_eq!(kept.as_deref(), Some(honest.as_slice()), "the verified re-make replaces it: the honest payload, byte for byte");
    assert_eq!(built.answers[0], Some(Ok(honest_answer.clone())), "and answers as the honest copy does");
    let (built, kept) = answer(&backend, &facts, vec![garbage.clone(), honest.clone()], &event, false);
    assert!(!built.expect("the honest copy answers").remade && kept.is_none(), "own retention planted, the seat's copy honest");
    // (3) The job alone.
    let (built, kept) = answer(&backend, &facts, vec![job_only.clone()], &event, false);
    assert!(built.expect("re-made from the kept job").remade);
    assert_eq!(kept.as_deref(), Some(honest.as_slice()), "and kept as the claim's FPC1");
    // (4) Nothing to answer from.
    let (built, kept) = answer(&backend, &facts, Vec::new(), &event, false);
    assert!(built.is_err() && kept.is_none(), "a free-prompt job is not chain data");
    let mut stranger = job.clone();
    stranger.job_nonce = [0xA5; 32];
    let stranger_only = palw_fp_material_encode_v1(&stranger, &ids);
    let (built, kept) = answer(&backend, &facts, vec![stranger_only.clone()], &event, false);
    assert!(built.is_err() && kept.is_none(), "a stranger's job does not reproduce the claim's roots");
    let (built, kept) = answer(&backend, &facts, vec![stranger_only, job_only.clone()], &event, false);
    assert!(built.expect("the claim's own job, tried next").remade && kept.as_deref() == Some(honest.as_slice()));
    let mut other = job.clone();
    other.executor_bond =
        kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB1), 0);
    let (built, kept) = answer(&backend, &facts, vec![palw_fp_material_encode_v1(&other, &ids)], &event, false);
    assert!(built.is_err() && kept.is_none(), "another producer's job is not replayed");
}

#[test]
fn constraint_job_only_recovery_restores_the_mask_and_never_answers_from_missing_tables_or_wrong_roots() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_decode_constraint_v1::*;
    use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpJobTailV1, palw_fp_capture_encode_v1, palw_fp_material_encode_v1};
    let (backend, mut facts, mut job, ids, _) = fp_claim();
    let constraint = PalwDecodeConstraintV1 {
        version: 1,
        compiler_id: Hash64::from_u64_word(0xDA_C014),
        start_frame: 0,
        frames: vec![PalwConstraintFrameV1 {
            start: 0,
            nodes: vec![PalwConstraintNodeV1 {
                accepting: true,
                edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
            }],
        }],
    };
    let vocab = misaka_palw_base0::rc::palw_rc_base0_shape_v1().vocab;
    let mut table = PalwTokenTableV1 {
        entries: vec![Some(b"b".to_vec()); vocab],
        eog_token_ids: vec![(vocab - 1) as u32],
        tokenizer_id: job.tokenizer_id,
    };
    table.entries[0] = Some(b"a".to_vec());
    let root = palw_token_tables_v1().register(table);
    job.version = PALW_FP_CONSTRAINT_VERSION;
    job.decode = None;
    job.tail = Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint: constraint.to_bytes(), table_root: root }));
    let prompt: Vec<_> = ids.iter().map(|id| *id as usize).collect();
    assert!(backend.execute_free_prompt(&job, &prompt).is_err(), "the unscoped recovery would fail");
    let expected = palw_fp_with_constraint_scope_v1(
        palw_fp_constraint_mask_for_host_v1(&job, backend.token_table_for_job_v1(&job)).unwrap(),
        || backend.execute_free_prompt(&job, &prompt),
    )
    .unwrap();
    assert!(expected.output_token_ids.iter().all(|id| *id == 0));
    facts.execution_root = expected.outcome.execution_root;
    facts.trace_root = expected.facts.full_logits_trace_root;
    facts.work_leaves = expected.facts.step_leaf_count;
    let binding = backend.disclose_trace_event(&expected.outcome.material, u32::MAX, 0).unwrap().binding().clone();
    facts.job_pin = Some(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context));
    let honest = palw_fp_capture_encode_v1(&job, &ids, &expected.outcome.material);
    let event = [PalwDaUnitV1::Event { row: 0, tile: 0 }];
    let expected_answer = answer(&backend, &facts, vec![honest.clone()], &event, false).0.unwrap().answers;
    for kept in [palw_fp_material_encode_v1(&job, &ids), palw_fp_capture_encode_v1(&job, &ids, b"corrupt capture")] {
        let (built, retained) = answer(&backend, &facts, vec![kept], &event, false);
        let actual = built.expect("the descriptor and owned public table recover the lost capture");
        assert!(actual.remade);
        assert_eq!(actual.answers, expected_answer);
        assert_eq!(retained.as_deref(), Some(honest.as_slice()));
        assert!(palw_fp_constraint_scope_v1().is_none(), "recovery restores the caller's thread scope");
    }
    let mut wrong_tokenizer = job.clone();
    wrong_tokenizer.tokenizer_id = Hash64::from_u64_word(0x000B_ADDA_C014);
    let mut unavailable = job.clone();
    let Some(PalwFpJobTailV1::Constraint(tail)) = unavailable.tail.as_mut() else { unreachable!() };
    tail.table_root = Hash64::from_u64_word(0x000B_ADDA_C015);
    for refused in [wrong_tokenizer, unavailable] {
        let (built, retained) = answer(&backend, &facts, vec![palw_fp_material_encode_v1(&refused, &ids)], &event, false);
        assert!(built.is_err() && retained.is_none(), "another/missing table cannot become a DA answer");
        assert!(palw_fp_constraint_scope_v1().is_none());
    }
    facts.execution_root = Hash64::from_u64_word(0x000B_ADDA_C016);
    let (built, retained) = answer(&backend, &facts, vec![palw_fp_material_encode_v1(&job, &ids)], &event, false);
    assert!(built.is_err() && retained.is_none(), "correct recovery must still reproduce the recorded claim roots");
    assert!(palw_fp_constraint_scope_v1().is_none());
}

/// **Every due unit of a claim from one load, and one `Flat` answers the claim's run** (IMPL-16): a
/// `Flat` already queued answers an in-run event unit (`None`, no carrier), and one built here
/// answers the next in-run unit the same way; a held unit of a free-prompt claim is answered from
/// the same capture by the one held builder; an answer is the form the ONE object builder takes.
#[test]
fn a_claims_units_are_answered_from_one_load_and_one_flat_answers_its_run() {
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1;
    use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
    use kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1;
    let (backend, facts, job, ids, capture) = fp_claim();
    let honest = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1(&job, &ids, &capture);
    let (built, _) = answer(&backend, &facts, vec![honest.clone()], &[PalwDaUnitV1::Event { row: 0, tile: 0 }], true);
    assert_eq!(built.expect("loaded").answers, vec![None], "a queued Flat answers the in-run unit");
    let units = [
        PalwDaUnitV1::Event { row: 0, tile: 0 },
        PalwDaUnitV1::Event { row: 1, tile: 0 },
        PalwDaUnitV1::Held(PalwHeldMissingV1::PromptIdsTile { tile: 0 }),
    ];
    let built = answer(&backend, &facts, vec![honest], &units, false).0.expect("loaded once");
    assert_eq!(built.answers.len(), 3);
    let first = built.answers[0].clone().expect("asked").expect("opens");
    if matches!(first, PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(PalwTraceEventDisclosureV1::Flat { .. }))) {
        assert_eq!(built.answers[1], None, "the Flat built for row 0 answers row 1 of a two-row run");
    } else {
        assert!(built.answers[1].is_some(), "no Flat, so row 1 is answered on its own");
    }
    let held = built.answers[2].clone().expect("asked").expect("the prompt tile opens from the capture's job");
    let bond = facts.executor_bond;
    for (unit, answer) in [(units[0], first), (units[2], held)] {
        palw_da_built_answer_object_v1(
            &Hash64::from_u64_word(0xD0),
            facts.claim_id,
            unit,
            answer,
            bond,
            facts.form,
            u64::MAX,
            |_, _| Some(vec![1]),
        )
        .expect("the form the fold takes");
    }
}

/// An actual dense claim's retained capture answers all legacy unit forms through the production worker and carrier builder.
/// A planted capture is remade first; wrong-unit, unsigned and over-ceiling carriers never leave this node.
#[test]
fn legacy_held_due_units_use_tag158_from_verified_dense_material() {
    use kaspa_consensus_core::palw_freeprompt_v3::{palw_fp_capture_encode_v1, palw_fp_material_encode_v1};
    use kaspa_consensus_core::palw_legacy_held_da_v2::{PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2, PalwLegacyHeldUnitV2};
    let (backend, facts, job, ids, capture) = fp_claim();
    let retention = misaka_palw_base0::produce::base0_material_decode_any_v1(&capture).expect("this family's capture");
    assert!(matches!(retention, misaka_palw_base0::produce::Base0RetentionV1::Dense(_)), "the floor keeps dense tiles");
    let binding = retention.binding();
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let c_height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(u64::from(binding.checkpoint_count));
    let witness_leaf = (0..binding.step_leaf_count)
        .find(|leaf| !kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_ckw_leaf_is_model_copy_v2(binding, *leaf))
        .expect("a computation leaf, not a model-copy gather");
    let mut units = vec![
        PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk: 0 }),
        PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 }),
        PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::KernelWitness { leaf: witness_leaf }),
    ];
    if c_height >= 1 {
        units.push(PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::CheckpointNode { level: c_height, index: 0 }));
    }
    let honest = palw_fp_capture_encode_v1(&job, &ids, &capture);
    let refused = answer(
        &backend,
        &facts,
        vec![honest.clone()],
        &[PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::KernelWitness { leaf: 0 })],
        false,
    )
    .0
    .expect("the honest capture is loaded");
    assert!(matches!(&refused.answers[0], Some(Err(why)) if why.contains("registered model")), "model-copy CKW remains out of scope");
    let built = answer(&backend, &facts, vec![honest.clone()], &units, true).0.expect("one verified load");
    assert!(!built.remade);
    let garbage = palw_fp_capture_encode_v1(&job, &ids, b"planted material");
    let (remade, kept) = answer(&backend, &facts, vec![garbage], &units, true);
    let remade = remade.expect("the public job can remake the producer's retention");
    assert!(remade.remade);
    assert_eq!(kept.as_deref(), Some(honest.as_slice()));
    assert_eq!(remade.answers, built.answers);
    let domain = Hash64::from_u64_word(0xD0);
    for (unit, answer) in units.iter().zip(&built.answers) {
        let answer = answer.clone().expect("Flat never covers a legacy unit").expect("the committed unit opens");
        let object = palw_da_built_answer_object_v1(
            &domain,
            facts.claim_id,
            *unit,
            answer.clone(),
            facts.executor_bond,
            facts.form,
            u64::MAX,
            |_, context| {
                assert_eq!(context, PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2);
                Some(vec![1])
            },
        )
        .expect("the signed tag-158 object can ride");
        assert!(matches!(object, PalwConsensusObjectV2::LegacyHeldAnsweredV2 { .. }));
        assert!(
            palw_da_built_answer_object_v1(
                &domain,
                facts.claim_id,
                *unit,
                answer.clone(),
                facts.executor_bond,
                facts.form,
                0,
                |_, _| panic!("ceiling checked before signing")
            )
            .is_err()
        );
        assert!(
            palw_da_built_answer_object_v1(
                &domain,
                facts.claim_id,
                *unit,
                answer.clone(),
                facts.executor_bond,
                facts.form,
                u64::MAX,
                |_, _| None
            )
            .is_err()
        );
        assert!(
            palw_da_built_answer_object_v1(
                &domain,
                facts.claim_id,
                PalwDaUnitV1::Event { row: 0, tile: 0 },
                answer,
                facts.executor_bond,
                facts.form,
                u64::MAX,
                |_, _| panic!("wrong unit checked before signing")
            )
            .is_err()
        );
    }
    let mut wrong_claim = facts.clone();
    wrong_claim.execution_root = Hash64::from_u64_word(0xBAD);
    let (refused, kept) = answer(&backend, &wrong_claim, vec![honest, palw_fp_material_encode_v1(&job, &ids)], &units, false);
    assert!(refused.is_err() && kept.is_none(), "no copy or remake answers another claim's commitment");
}

/// **An attempt claim's material: kept and verified, or re-made from its block's job** — which is
/// chain data, so a garbage copy (or none) costs a replay and is replaced, and only a node whose
/// store lacks the block has nothing to answer from.
#[test]
fn an_attempt_claims_capture_is_kept_verified_or_re_made_from_its_block() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    let backend = super::seat_s_tests::floor_backend();
    let anchor = Hash64::from_u64_word(0x00C0_FFEE);
    let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
    let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, true);
    let run = backend.execute(&job, &prompt).expect("the producer's run");
    let mut facts = PalwDaClaimFactsV1 {
        claim_id: Hash64::from_u64_word(0xC2),
        class_id: backend.profile().shape_profile_id(),
        executor_bond: PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(9), 0)),
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        work_leaves: 0,
        form: backend.prompt_ids_form(),
        lane: PalwDaLaneV1::Attempt { anchor, attempt_draw: Some(true), job: Some((job, prompt)) },
        job_pin: None,
    };
    let event = [PalwDaUnitV1::Event { row: 0, tile: 0 }];
    let (built, kept) = answer(&backend, &facts, vec![run.material.clone()], &event, false);
    assert!(!built.expect("answered").remade && kept.is_none(), "the kept capture verifies");
    let (built, kept) = answer(&backend, &facts, vec![b"garbage".to_vec()], &event, false);
    assert!(built.expect("re-made from the block's job").remade);
    assert_eq!(kept.as_deref(), Some(run.material.as_slice()), "kept in the garbage's place");
    facts.lane = PalwDaLaneV1::Attempt { anchor, attempt_draw: Some(true), job: None };
    let (built, kept) = answer(&backend, &facts, vec![b"garbage".to_vec()], &event, false);
    assert!(built.is_err() && kept.is_none(), "no block in this store: nothing to re-make from");
}

/// **The tick runs R-core's loop, and it runs before the seat's duties** (the P2-7 review's
/// MEDIUM and LOW): the duty read, then the pins of what it must keep (before any seat duty below
/// can write `foreign/` and prune it), then each claim's answers built and queued on the carrier
/// path every court move rides. And a licence by replay keeps the job it licensed in its
/// job-only spelling, never a served `FPC1` nothing checked, and past the fence a seat that could
/// not keep it signs no `Valid`.
#[test]
fn the_tick_answers_da_sessions_and_keeps_what_it_answers_from() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let court = include_str!("court_progress.rs");
    let worker = include_str!("../palw_panel.rs");
    let read = court.find_code("let disclosure = session.palw_disclosure_duties_v1(vec![bond_key]);").expect("the duty read");
    let pinned = read
        + court[read..].find_code("self.foreign_pinned.lock().unwrap().extend(disclosure_retain.iter().copied());").expect("pinned");
    let built = pinned
        + court[pinned..]
            .find_code("self.rcore_da_answers_v1(session, network_domain, &duties, flat_queued.contains(&claim)).await")
            .expect("each claim's answers built");
    let queued = built + court[built..].find_code("court_outbox.push_pending((key.0, key.1, key.2, object));").expect("and queued");
    let seats = worker.find_code("let duties = session.palw_seat_duties_v2(vec![bond_key]);").expect("the seat's duties");
    assert!(read < pinned && pinned < built && built < queued, "read, pin, build, queue");
    let court_call = worker.find(".court_progress_tick_v1(").expect("court stage");
    assert!(court_call < seats, "court/DA progression precedes the seat duty read");
    let fp = &source[source.find_code("    async fn fp_seat_replay_pass_v1(").expect("the FP replay")..];
    let fp = &fp[..fp.find("\n    }\n").expect("its end")];
    assert!(
            fp.contains_code("PalwSeatReplayStepV1::Licensed,\n                            Some(palw_fp_material_encode_v1(&material.job, &material.prompt_token_ids)),)"),
            "a licence keeps the job it licensed"
        );
    assert!(!fp.contains_code("payloads.first()"), "never the first-arrived spelling");
    let licensed = &source[source.find_code("(PalwSeatReplayStepV1::Licensed, bytes) => {").expect("the licence arm")..];
    let valid = licensed.find_code("break 'verdict Some(PalwReceiptVerdictV2::Valid);").expect("its Valid");
    let abstain = licensed
            .find_code("if !self.licensed_job_kept_v1(&duty.claim_id, bytes, current_daa) {\n                                        break 'verdict None;")
            .expect("the kept-or-abstain gate");
    assert!(abstain < valid, "past the fence nothing is signed that this seat cannot answer for");
    let gate = &source[source.find_code("    fn licensed_job_kept_v1(").expect("the gate")..];
    let gate = &gate[..gate.find("\n    }\n").expect("its end")];
    assert!(
        gate.contains_code("bytes.is_none_or(|bytes| self.persist_foreign_material(claim, &bytes))")
            && gate.contains_code("if !kept && self.consensus_config.params.palw_rcore_plus_active_at(current_daa) {"),
        "the job is kept, or past the fence the licence is not signed"
    );
}

/// **The queue key names the claim and the unit**: the same unit keys the same entry (so a
/// queued or recently sent answer is not rebuilt), distinct units of one claim key distinct
/// entries (so one answer never hides another), and the key's claim half is the claim.
#[test]
fn the_queue_key_is_the_claim_and_the_unit() {
    use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
    let units = [
        PalwDaUnitV1::Event { row: 0, tile: 0 },
        PalwDaUnitV1::Event { row: 7, tile: 0 },
        PalwDaUnitV1::Event { row: 0, tile: 1 },
        PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 5 }),
        PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first: 5, count: 1 }),
        PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint: 1, chunk: 2 }),
        PalwDaUnitV1::Held(PalwHeldMissingV1::PromptIdsTile { tile: 0 }),
    ];
    let keys: Vec<(Hash64, u32, bool)> =
        units.iter().map(|unit| palw_disclosure_queue_key_v1(&duty(PalwDisclosureRoleV1::Producer, 0, *unit, 10, 10))).collect();
    let distinct: HashSet<u32> = keys.iter().map(|key| key.1).collect();
    assert_eq!(distinct.len(), units.len(), "one entry a unit");
    assert!(keys.iter().all(|key| key.0 == Hash64::from_u64_word(1) && key.2), "the claim, as the responder");
    let again = palw_disclosure_queue_key_v1(&duty(PalwDisclosureRoleV1::CoveringSigner, 2, units[3], 99, 10));
    assert_eq!(again, keys[3], "the unit, whoever answers it");
}
