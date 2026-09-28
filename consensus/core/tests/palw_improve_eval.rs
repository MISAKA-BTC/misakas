//! **RFC-0004 §7.2 (A6): an evaluation job's derived context, end to end at the library level.**
//!
//! A toy IR class (a version-1 program with per-layer params, as Phase F registers one) is the
//! subject:
//!
//! * the chain derives the evaluation pipeline from the job, the class and the policy's scoring
//!   parameters — the subject's program lifted unchanged into a `Decode` stage over its own layout,
//!   then the scoring library's stages — and it validates;
//! * **the params are the class's own**: the evaluation pipeline's inventory root is the class's Phase
//!   F `artifact_root`, so the court reads the subject's weights from openings under the root the
//!   class registered;
//! * an ExactMatch job (Generate) and a RefLogLik job (TeacherForced) run, bind their execution root
//!   over the stage roots, the ids and the score, and every leaf of every stage is acquitted by the
//!   generative court;
//! * judged kinds wait for the judge class kind; a job whose kind, mode and stage disagree is refused.

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
    let ctx =
        palw_improve_eval_context_v1(&j, &subject, PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1, key_cap: 4 }).unwrap();
    assert_eq!(ctx.job_id, j.id());
    assert_eq!(ctx.pipeline.stages.len(), 2);
    assert_eq!(ctx.layouts[0], layout, "the subject runs over its own layout");
    let params = Params(vec![params_v1.clone(), MapParams::default()]);
    // The executor generates (FP Job V4's greedy decode) and the exact match reads what it decoded.
    let prompt = vec![3u32, 5, 1];
    let probe = PipelineJob { prompt: prompt.clone(), scalars: ctx.scalars.clone(), ..PipelineJob::default() };
    let decode = ctx.decode.clone().expect("a generating job decodes");
    let e = palw_gen_execute_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &probe, &decode, ctx.seed).unwrap();
    assert_eq!(e.claim.generated.len(), 4);
    assert_eq!(e.run.output.data, vec![0], "no key: the empty key does not match four ids");
    // The key is the whole output: a pass, the same generation (the decode does not read the key).
    let keyed = PipelineJob { key: e.claim.generated.clone(), ..probe.clone() };
    let e = palw_gen_execute_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &keyed, &decode, ctx.seed).unwrap();
    assert_eq!(e.run.output.data, vec![exact_match_reference_v1(&e.claim.generated, &keyed.key, -1, -1) as i128]);
    assert_eq!(e.run.output.data, vec![1]);
    // The binding: the execution root over the stage roots, the ids and the score.
    let score: Vec<i32> = e.run.output.data.iter().map(|v| *v as i32).collect();
    let binding = PalwEvalBindingV1::of(&j, subject.class_id, &layout, &e.claim, e.space.leaf_count(), score);
    assert_eq!(binding.execution_root(), binding.committed_execution_root);
    assert_eq!(binding.score_value(), Ok(1));
    let mut other = binding.clone();
    other.score = vec![0];
    assert_ne!(other.execution_root(), binding.committed_execution_root, "the score is in the root");
    // Every leaf, the subject's included, is adjudicable from the class's own params.
    let judged = every_leaf_acquitted(&ctx, &params, &e, &keyed);
    assert!(judged > 20, "{judged}");
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
    let binding = PalwEvalBindingV1::of(&j, subject.class_id, &layout, &e.claim, e.space.leaf_count(), score);
    assert_eq!(binding.score_value(), Ok(want));
    let judged = every_leaf_acquitted(&ctx, &params, &e, &pj);
    assert!(judged > 20, "{judged}");
}

#[test]
fn judged_kinds_wait_and_disagreements_are_refused() {
    let program = subject_program();
    let layout = layout(&program);
    let subject =
        PalwEvalSubjectClassV1 { class_id: Hash64::default(), artifact_root: Hash64::default(), program: &program, layout: &layout };
    let generate = PalwEvalModeV1::Generate { seed: Hash64::default(), max_new: 4, stop_ids: vec![] };
    let judged = PalwEvalModeV1::Judged { judge: Hash64::from_bytes([5; 64]) };
    assert_eq!(
        palw_improve_eval_context_v1(
            &job(PalwScoringKindV1::Judge, judged.clone()),
            &subject,
            PalwEvalStageParamsV1::Judge { lo: -1, hi: 1 }
        )
        .err(),
        Some(PalwEvalErrorV1::JudgedNotYet)
    );
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
            &job(PalwScoringKindV1::ExactMatch, judged),
            &subject,
            PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1, key_cap: 4 }
        )
        .err(),
        Some(PalwEvalErrorV1::KindMismatch),
        "an exact match generates"
    );
}
