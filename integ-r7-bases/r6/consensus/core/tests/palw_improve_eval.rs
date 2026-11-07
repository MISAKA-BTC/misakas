//! **RFC-0004 §7.2 (A6): an evaluation job's derived context, end to end at the library level.**
//!
//! A toy IR class (a version-1 program with per-layer params, as Phase F registers one) is the
//! subject:
//!
//! * the chain derives the evaluation pipeline from the job, the class and the policy's scoring
//!   parameters — the subject's program lifted unchanged over its own layout: an ExactMatch job's
//!   text stage alone (the key is hidden until every subject's outputs are final, so the fold scores
//!   the generation at the key's reveal), a RefLogLik job's `Decode` stage then the scoring library's
//!   stages — and it validates;
//! * **the params are the class's own**: the evaluation pipeline's inventory root is the class's Phase
//!   F `artifact_root`, so the court reads the subject's weights from openings under the root the
//!   class registered;
//! * an ExactMatch job (Generate) and a RefLogLik job (TeacherForced) run, bind their execution root
//!   over the stage roots, the ids and the score, and every leaf of every stage is acquitted by the
//!   generative court; the ExactMatch job's answer, kept at acceptance, scores at the key's reveal as
//!   the library's stage would;
//! * a judged part is the judge class's teacher-forced RefLogLik pass over the template's fill, the verdict
//!   sequence as its reference (RFC-0004 §7.3 as decided 2026-09-30): its context is the RefLogLik one at the
//!   judge's scale, its score the pass's, and the parts' margin is the judged score; a job whose kind, mode
//!   and stage disagree is refused.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_gen_artifact_v1::*;
use kaspa_consensus_core::palw_gen_court_v1::*;
use kaspa_consensus_core::palw_gen_worker_v1::*;
use kaspa_consensus_core::palw_improve_eval_v1::*;
use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1};
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::TripRule;
use misaka_palw_tir::pipeline::{PipelineJob, PipelineParams, stage_job_facts};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::scoring::{exact_match_reference_v1, ref_loglik_join_v1, ref_loglik_reference_v1};
use misaka_palw_tir::{DType, Ref, Tensor, TensorType, TirProgramV1};
use std::borrow::Cow;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };

/// A toy IR class's program: an `i8` embedding, two layers of a per-layer `i8 [4, 4]` matrix and
/// `i64` multiplier, and an `i16` head over 16 ids — Phase F's shape (tiled logits).
fn subject_program() -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("embed.table", DType::I8, &[16, 4], false);
    let w = pb.param("blk.w", DType::I8, &[4, 4], true);
    let m = pb.param("blk.m", DType::I64, &[4], true);
    let head = pb.param("head.w", DType::I16, &[16, 4], false);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(table, Ref::Input(0), 0, 0);
        let row = b.cast(row, DType::I32);
        b.finish(&[row])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let acc = b.matmul(w, x, DType::I64);
        let acc = b.reshape_fixed(acc, &[4]);
        let y = b.mul(acc, m, DType::I128);
        let y = b.shr(y, 20, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I128);
        let y = b.clamp(y, -30_000, 30_000, DType::I32);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.reshape_fixed(l, &[16]);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let mut p = pb.finish(pre, vec![layer, layer], post, logits);
    p.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    p
}

/// Deterministic params for every instance (per-layer params at both layers).
fn subject_params(p: &TirProgramV1) -> MapParams {
    let mut out = MapParams::default();
    for (j, inst) in kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let d = &p.params[j];
        for l in inst {
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            let data: Vec<i128> = (0..n)
                .map(|i| {
                    let v = ((i * 37 + j * 11 + l.map_or(0, |l| l as usize) * 5) % 200) as i128 - 100;
                    if d.dtype == DType::I64 { v.abs() * 9_000 + 1 } else { v }
                })
                .collect();
            out.tensors.insert((j as u16, l), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
        }
    }
    out
}

struct Src<'a>(&'a MapParams);
impl PalwTirTensorSourceV1 for Src<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
    }
}

/// The evaluation pipeline's params: the subject's for program 0; the scoring stages have none.
struct Params(Vec<MapParams>);
impl PipelineParams for Params {
    fn params(&self, program: u16) -> &dyn ParamSource {
        &self.0[program as usize]
    }
}

fn layout(p: &TirProgramV1) -> PalwTirLayoutV1 {
    let commits = p.blocks.iter().map(|b| b.nodes.iter().filter(|n| n.commit).count()).sum::<usize>();
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 12,
        checkpoint_interval: 1,
        h_tile: 16,
        commit_tiles: vec![4; commits],
        state_tiles: vec![4; p.states.len()],
    }
}

fn job(kind: PalwScoringKindV1, mode: PalwEvalModeV1) -> PalwEvalJobV1 {
    PalwEvalJobV1 {
        line_id: Hash64::from_bytes([0x11; 64]),
        epoch: 3,
        item: 7,
        subject: PalwEvalSubjectV1::Candidate(Hash64::from_bytes([0x22; 64])),
        kind,
        part: 0,
        mode,
    }
}

/// Every leaf of every stage of `e`, acquitted by the generative court under the evaluation
/// inventory (the subject's own) and the job's facts.
fn every_leaf_acquitted(ctx: &PalwEvalContextV1, params: &Params, e: &PalwGenExecutionV1, job: &PipelineJob) -> usize {
    let inventory = PalwGenInventoryIndexV1::new_named(&ctx.programs, PalwGenInventoryNamingV1::Evaluation).expect("an index");
    let (root, count) = palw_gen_inventory_root_named_v1(&ctx.programs, params, PalwGenInventoryNamingV1::Evaluation).unwrap();
    assert_eq!(root, ctx.artifact_root, "the evaluation's params are the subject class's own");
    let openings = palw_gen_open_leaves_named_v1(&ctx.programs, params, PalwGenInventoryNamingV1::Evaluation, 0..count).unwrap();
    let court_job = PipelineJob { generated: e.claim.generated.clone(), ..job.clone() };
    let facts = stage_job_facts(&ctx.pipeline, &ctx.programs, &court_job).unwrap();
    let case = PalwGenCourtCaseV1 {
        space: &e.space,
        pipeline: &ctx.pipeline,
        programs: &ctx.programs,
        artifact_root: ctx.artifact_root,
        inventory: &inventory,
        facts: &facts,
        images: &[],
        draw: PalwGenDrawV1 { seed: ctx.seed, item_index: 0 },
        claim: &e.claim,
    };
    let mut judged = 0;
    for s in 0..ctx.pipeline.stages.len() {
        for i in 0..e.space.stages[s].leaves().len() {
            let mut operands = Vec::new();
            for st in 0..=s {
                let n = if st == s { i } else { e.space.stages[st].leaves().len() };
                operands.extend((0..n).map(|k| e.open(st as u8, k as u64).unwrap()));
            }
            let close = PalwGenCloseV1 {
                disputed: e.open(s as u8, i as u64).unwrap(),
                operands,
                image_tiles: vec![],
                params: openings.clone(),
            };
            assert_eq!(palw_gen_adjudicate_leaf_v1(&case, &close, &LIMITS), Ok(PalwGenVerdictV1::Acquitted), "stage {s} leaf {i}");
            judged += 1;
        }
    }
    judged
}

#[test]
fn an_exact_match_job_derives_runs_binds_and_is_adjudicable() {
    let program = subject_program();
    let params_v1 = subject_params(&program);
    let layout = layout(&program);
    let (artifact_root, _) = palw_tir_inventory_root_v1(&program, &Src(&params_v1)).unwrap();
    let subject =
        PalwEvalSubjectClassV1 { class_id: Hash64::from_bytes([0x22; 64]), artifact_root, program: &program, layout: &layout };
    let seed = palw_improve_eval_seed_v1(&Hash64::from_bytes([0x33; 64]), 7);
    let j = job(PalwScoringKindV1::ExactMatch, PalwEvalModeV1::Generate { seed, max_new: 4, stop_ids: vec![] });
    let ctx = palw_improve_eval_context_v1(&j, &subject, PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 }).unwrap();
    assert_eq!(ctx.job_id, j.id());
    assert_eq!(ctx.pipeline.stages.len(), 1, "the generation alone");
    assert_eq!(ctx.pipeline.stages[0].trip, TripRule::TextStream, "as the pipeline's text stage");
    assert!(ctx.scalars.is_empty());
    assert_eq!(ctx.layouts[0], layout, "the subject runs over its own layout");
    let params = Params(vec![params_v1.clone()]);
    // The executor generates (FP Job V4's greedy decode); the key is not in the job.
    let prompt = vec![3u32, 5, 1];
    let probe = PipelineJob { prompt: prompt.clone(), scalars: ctx.scalars.clone(), ..PipelineJob::default() };
    let decode = ctx.decode.clone().expect("a generating job decodes");
    let e = palw_gen_execute_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &probe, &decode, ctx.seed).unwrap();
    assert_eq!(e.claim.generated.len(), 4);
    // The binding: the execution root over the stage roots and the ids; no score is committed.
    let stage = PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 };
    let binding = PalwEvalBindingV1::of(&j, subject.class_id, &layout, &e.claim, e.space.leaf_count(), &prompt, stage, vec![], vec![]);
    assert_eq!(binding.execution_root(), binding.committed_execution_root);
    assert!(binding.score_value().is_err(), "an ExactMatch claim commits no score");
    let roots = binding.claim_roots();
    assert_eq!(roots.trace_root, e.claim.step_root, "the claim's trace root is the step root");
    // The binding alone derives the step space (the prompt by its length), and the closed-form count.
    let space = palw_improve_eval_step_space_of_binding_v1(&ctx, &binding).unwrap();
    assert_eq!(space.leaf_count(), e.space.leaf_count());
    assert_eq!(palw_improve_eval_step_leaves_v1(&ctx, &prompt, &e.claim.generated, &[]), Ok(e.space.leaf_count() as u128));
    let other_length = PalwEvalBindingV1 { prompt_tokens: 4, ..binding.clone() };
    assert_ne!(other_length.execution_root(), binding.committed_execution_root, "the prompt's length is in the root");
    assert_eq!(roots.output_root, palw_improve_eval_generated_root_v1(&e.claim.generated));
    let mut other = binding.clone();
    other.generated[0] ^= 1;
    assert_ne!(other.execution_root(), binding.committed_execution_root, "the ids are in the root");
    // At acceptance the row keeps the answer; at the key's reveal it scores as the library's stage.
    let answer = palw_improve_answer_of_v1(&e.claim.generated, -1, -1);
    for key in [e.claim.generated.clone(), vec![e.claim.generated[0]], vec![]] {
        let fold = answer == Some(palw_improve_answer_span_hash_v1(&key));
        assert_eq!(fold, exact_match_reference_v1(&e.claim.generated, &key, -1, -1));
        let stage = misaka_palw_tir::scoring::exact_match_v1(misaka_palw_tir::scoring::ExactMatchShapeV1 {
            gen_len: 4,
            key_len: 4,
            token_bound: program.token_bound,
        })
        .unwrap();
        assert_eq!(run_exact_match_stage(&stage, &e.claim.generated, &key, -1, -1), fold as i64, "the library's stage agrees");
    }
    // Every leaf, the subject's included, is adjudicable from the class's own params.
    let judged = every_leaf_acquitted(&ctx, &params, &e, &probe);
    assert!(judged > 20, "{judged}");
}

/// The library's ExactMatch stage run on its own, the key padded to its length.
fn run_exact_match_stage(
    stage: &misaka_palw_tir::program_v2::TirProgramV2,
    generated: &[u32],
    key: &[u32],
    open: i32,
    close: i32,
) -> i64 {
    use misaka_palw_tir::pipeline::{Binding, StageDecl, TIR_PIPELINE_VERSION_V1, TirPipelineV1, TokenPad, TokenRule, TokenSource};
    let rule = |source, to_len| TokenRule { prefix: vec![], source, suffix: vec![], pad: Some(TokenPad { id: 0, to_len }) };
    let pipeline = TirPipelineV1 {
        version: TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl {
            name: "score".into(),
            program: 0,
            trip: TripRule::Fixed { n: 1 },
            max_trip: 1,
            tokens: None,
            bind: vec![
                Binding::JobTokens { rule: rule(TokenSource::Prompt, 4) },
                Binding::JobTokenCount { rule: rule(TokenSource::Prompt, 4) },
                Binding::JobTokens { rule: rule(TokenSource::Key, 4) },
                Binding::JobTokenCount { rule: rule(TokenSource::Key, 4) },
                Binding::JobScalar { index: 0 },
                Binding::JobScalar { index: 1 },
            ],
        }],
        output_stage: 0,
    };
    let job = PipelineJob {
        prompt: generated.to_vec(),
        key: key.to_vec(),
        scalars: vec![open as i64, close as i64],
        ..PipelineJob::default()
    };
    struct NoRandom;
    impl misaka_palw_tir::pipeline::RandomSource for NoRandom {
        fn random(&self, _: u16, _: misaka_palw_tir::program_v2::RandomDist, _: u32, _: &[u32]) -> Option<Tensor> {
            None
        }
    }
    let params = Params(vec![MapParams::default()]);
    let run = misaka_palw_tir::pipeline::run_pipeline(&pipeline, std::slice::from_ref(stage), &params, &NoRandom, &job)
        .expect("the stage runs");
    run.output.data[0] as i64
}

#[test]
fn a_ref_loglik_job_is_teacher_forced_over_the_reference() {
    let program = subject_program();
    let params_v1 = subject_params(&program);
    let layout = layout(&program);
    let (artifact_root, _) = palw_tir_inventory_root_v1(&program, &Src(&params_v1)).unwrap();
    let subject =
        PalwEvalSubjectClassV1 { class_id: Hash64::from_bytes([0x22; 64]), artifact_root, program: &program, layout: &layout };
    let j = job(PalwScoringKindV1::RefLogLik, PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([0x44; 64]) });
    let ctx = palw_improve_eval_context_v1(&j, &subject, PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 }).unwrap();
    assert!(ctx.decode.is_none(), "teacher-forced: nothing is selected");
    assert_eq!(ctx.pipeline.stages.len(), 3);
    let params = Params(vec![params_v1, MapParams::default(), MapParams::default()]);
    let (prompt, reference) = (vec![3u32, 5], vec![7u32, 2, 9, 9]);
    let pj =
        PipelineJob { prompt: prompt.clone(), generated: reference.clone(), scalars: ctx.scalars.clone(), ..PipelineJob::default() };
    let e = palw_gen_replay_committed_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &pj, ctx.seed).unwrap();
    let rows: Vec<Vec<i32>> =
        e.run.stages[0].steps[prompt.len() - 1..].iter().map(|s| s.output.data.iter().map(|x| *x as i32).collect()).collect();
    let want = ref_loglik_reference_v1(&rows, &reference, 1 << 12);
    let score: Vec<i32> = e.run.output.data.iter().map(|v| *v as i32).collect();
    assert_eq!(ref_loglik_join_v1(score[0], score[1]), want);
    let stage = PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 };
    let binding = PalwEvalBindingV1::of(&j, subject.class_id, &layout, &e.claim, e.space.leaf_count(), &prompt, stage, vec![], score);
    assert_eq!(binding.score_value(), Ok(want));
    assert_eq!(binding.generated, reference, "teacher-forced: the ids are the reference");
    let judged = every_leaf_acquitted(&ctx, &params, &e, &pj);
    assert!(judged > 20, "{judged}");
}

#[test]
fn a_judged_part_is_the_judge_s_teacher_forced_pass_over_the_filled_prompt() {
    let program = subject_program();
    let params_v1 = subject_params(&program);
    let layout = layout(&program);
    let (artifact_root, _) = palw_tir_inventory_root_v1(&program, &Src(&params_v1)).unwrap();
    // The judge class: here the toy class itself plays it.
    let judge = PalwEvalSubjectClassV1 { class_id: Hash64::from_bytes([0x55; 64]), artifact_root, program: &program, layout: &layout };
    let scale: i32 = 1 << 12;
    let (yes, no) = (vec![7u32], vec![2u32, 9]);
    // The template and the prompt a part runs over: the item's prompt and the generation it reads.
    let template = PalwJudgeTemplateV1 { segments: vec![vec![1], vec![2], vec![3]] };
    let (item_prompt, generation) = (vec![5u32, 6], vec![8u32, 4, 4]);
    let prompt = palw_improve_judge_fill_v1(&template, &item_prompt, &[&generation]).unwrap();
    assert_eq!(prompt, vec![1, 5, 6, 2, 8, 4, 4, 3]);
    let stage = PalwEvalStageParamsV1::Judge { lo: -(1 << 30), hi: 1 << 30, logit_scale_q24: scale };
    let judged = PalwEvalModeV1::Judged { judge: judge.class_id };
    let mut lls = Vec::new();
    for (part, verdict) in [(0u8, &yes), (1u8, &no)] {
        let j = PalwEvalJobV1 { part, ..job(PalwScoringKindV1::Judge, judged.clone()) };
        let ctx = palw_improve_eval_context_v1(&j, &judge, stage).unwrap();
        assert!(ctx.decode.is_none(), "a judge is run teacher-forced: nothing is selected, nothing sampled");
        assert_eq!(ctx.pipeline.stages.len(), 3, "the RefLogLik pipeline: the judge's decode stage, the log-probs, the sum");
        assert_eq!(ctx.scalars, vec![scale as i64], "at the judge's logit scale");
        assert_eq!(ctx.subject_class, judge.class_id, "the executed class is the judge's");
        assert_ne!(j.id(), PalwEvalJobV1 { part: 1 - part, ..j.clone() }.id(), "each part has its own id");
        let params = Params(vec![params_v1.clone(), MapParams::default(), MapParams::default()]);
        let pj =
            PipelineJob { prompt: prompt.clone(), generated: verdict.clone(), scalars: ctx.scalars.clone(), ..PipelineJob::default() };
        let e = palw_gen_replay_committed_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &pj, ctx.seed).unwrap();
        let rows: Vec<Vec<i32>> =
            e.run.stages[0].steps[prompt.len() - 1..].iter().map(|s| s.output.data.iter().map(|x| *x as i32).collect()).collect();
        let want = ref_loglik_reference_v1(&rows, verdict, scale as i64);
        let score: Vec<i32> = e.run.output.data.iter().map(|v| *v as i32).collect();
        assert_eq!(ref_loglik_join_v1(score[0], score[1]), want, "the part's score is the verdict's log-likelihood");
        let binding = PalwEvalBindingV1::of(
            &j,
            judge.class_id,
            &layout,
            &e.claim,
            e.space.leaf_count(),
            &prompt,
            stage,
            vec![generation.clone()],
            score,
        );
        assert_eq!(binding.score_value(), Ok(want));
        assert_eq!(binding.generated, *verdict, "the ids are the verdict sequence");
        assert_eq!(palw_improve_eval_step_leaves_v1(&ctx, &prompt, verdict, &[]), Ok(e.space.leaf_count() as u128));
        let acquitted = every_leaf_acquitted(&ctx, &params, &e, &pj);
        assert!(acquitted > 20, "{acquitted}");
        lls.push(want);
    }
    // The judged score is the margin of the parts, clamped to the stage's range.
    assert_eq!(palw_improve_judged_score_v1(&stage, &lls), Some((lls[0] - lls[1]).clamp(-(1 << 30), 1 << 30)));
}

#[test]
fn disagreements_are_refused() {
    let program = subject_program();
    let layout = layout(&program);
    let subject =
        PalwEvalSubjectClassV1 { class_id: Hash64::default(), artifact_root: Hash64::default(), program: &program, layout: &layout };
    let generate = PalwEvalModeV1::Generate { seed: Hash64::default(), max_new: 4, stop_ids: vec![] };
    let judged = PalwEvalModeV1::Judged { judge: Hash64::from_bytes([5; 64]) };
    assert_eq!(
        palw_improve_eval_context_v1(
            &job(PalwScoringKindV1::ExactMatch, generate.clone()),
            &subject,
            PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 }
        )
        .err(),
        Some(PalwEvalErrorV1::KindMismatch)
    );
    assert_eq!(
        palw_improve_eval_context_v1(
            &job(PalwScoringKindV1::ExactMatch, judged.clone()),
            &subject,
            PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 }
        )
        .err(),
        Some(PalwEvalErrorV1::KindMismatch),
        "an exact match generates"
    );
    assert_eq!(
        palw_improve_eval_context_v1(
            &job(PalwScoringKindV1::Judge, generate),
            &subject,
            PalwEvalStageParamsV1::Judge { lo: -1, hi: 1, logit_scale_q24: 1 }
        )
        .err(),
        Some(PalwEvalErrorV1::KindMismatch),
        "a judge does not generate"
    );
    assert!(
        matches!(
            palw_improve_eval_context_v1(
                &job(PalwScoringKindV1::Judge, judged),
                &subject,
                PalwEvalStageParamsV1::Judge { lo: -1, hi: 1, logit_scale_q24: 0 }
            )
            .err(),
            Some(PalwEvalErrorV1::Scoring(_))
        ),
        "a judge's logit scale is positive"
    );
}

/// ref2's H7 shape as a Phase F class (`tests/palw_tir_h7.rs`): scores `Qᵀ · K` over a 16-row window
/// and a `TopK` of them — a commit point whose cone reduces over the history.
fn attention_program() -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 7], false);
    let head = pb.param("head", DType::I8, &[8, 7], false);
    let ks = pb.hist_state("k", DType::I32, &[3], 16, true);
    let qs = pb.hist_state("q", DType::I32, &[4], 16, true);
    let carry = vec![TensorType::fixed(DType::I32, &[7])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let rk = b.slice(Ref::CarryIn(0), 0, 0, 3);
        let rk = b.clamp(rk, -127, 127, DType::I32);
        let k = b.hist_append(ks, rk);
        let rq = b.slice(Ref::CarryIn(0), 0, 3, 4);
        let rq = b.clamp(rq, -127, 127, DType::I32);
        let q = b.hist_append(qs, rq);
        let qt = b.transpose(q, &[1, 0]);
        let s = b.matmul(qt, k, DType::I64);
        let s = b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.topk(s, 0, 4);
        let y = b.cast(Ref::CarryIn(0), DType::I32);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[7, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let mut program = pb.finish(pre, vec![layer], post, logits);
    program.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    program
}

/// **The dissected points follow the subject** — a Phase F class whose attention reduces over the
/// history keeps its dissected commit points in the evaluation pipeline, at the subject stage: the
/// lifted program's view is the class's program, so a court answers the same leaves with F7's phase.
#[test]
fn the_subject_s_dissected_points_are_its_class_s() {
    let program = attention_program();
    let class_points = kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&program);
    assert!(!class_points.is_empty(), "the attention's TopK reduces over the history");
    let layout = layout(&program);
    let subject =
        PalwEvalSubjectClassV1 { class_id: Hash64::default(), artifact_root: Hash64::default(), program: &program, layout: &layout };
    let seed = Hash64::from_bytes([1; 64]);
    let j = job(PalwScoringKindV1::ExactMatch, PalwEvalModeV1::Generate { seed, max_new: 4, stop_ids: vec![] });
    let ctx = palw_improve_eval_context_v1(&j, &subject, PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 }).unwrap();
    let at_subject: Vec<(u8, u16)> = ctx.dissected.iter().filter(|(s, _, _)| *s == 0).map(|(_, b, n)| (*b, *n)).collect();
    assert_eq!(at_subject, class_points, "the class's own points, at the subject stage");
    assert!(ctx.dissected.iter().all(|(s, _, _)| *s == 0), "only the subject stage reads history");
    assert_eq!(ctx.programs[0].v1_view().encode(), program.encode(), "the lifted program's view is the class's program");
}
