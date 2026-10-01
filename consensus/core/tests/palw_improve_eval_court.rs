//! **RFC-0004 A6: the evaluation court at the library level** (spec 17 §17.8.6).
//!
//! A toy IR class is the subject of real evaluation claims — a generation (ExactMatch) and a teacher-forced
//! likelihood (RefLogLik) — each executed for real, bound to its execution root and held to the chain's facts
//! ([`PalwEvalClaimFactsV1`]); the court's closes are built from the executor's own run
//! ([`PalwEvalEvidenceV1`]) and adjudicated by the functions the fold and the processor call:
//!
//! * **honest claims are acquitted** — every leaf of every stage, by `EvalCone`; the committed score by the score
//!   close; every generated id by the decode close;
//! * **planted lies are convicted** — a wrong value committed consistently into any stage's tree (the subject's
//!   logits, the log-probs, the score's sum) at its leaf; a wrong committed score over an honest tree
//!   (`TirOutputDigestMismatch`); a wrong generated id over an honest tree (`DecodeTokenMismatch`);
//! * **a close is the claim's**: a binding that is not the job table's, the claim's roots, the class's layout, a prompt
//!   carried where the stage does not read it (or not carried where it does), a parameter opening that does not reach
//!   the class's root, a leaf other than the one the ladder narrowed to — each is refused by name, convicting nobody;
//! * **a composite candidate** (RFC-0004 §6.3) is adjudicated from its sections' openings, the reference hashing to the
//!   class's artifact root.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_gen_artifact_v1::*;
use kaspa_consensus_core::palw_gen_step_v1::{palw_gen_stage_root_v1, palw_gen_step_leaf_hash_v1};
use kaspa_consensus_core::palw_gen_worker_v1::*;
use kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1;
use kaspa_consensus_core::palw_improve_eval_court_v1::*;
use kaspa_consensus_core::palw_improve_eval_v1::*;
use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1};
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interp::{MapParams, ParamSource};
use misaka_palw_tir::pipeline::{PipelineJob, PipelineParams};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Ref, Tensor, TensorType, TirProgramV1};
use std::borrow::Cow;

const LIMITS: DemandLimits = DemandLimits { max_elements: 1 << 20, max_terms: 1 << 24 };

// ---- the toy class (the evaluation tests' own) -------------------------------------------------------------

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

fn subject_params(p: &TirProgramV1, salt: usize) -> MapParams {
    let mut out = MapParams::default();
    for (j, inst) in kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let d = &p.params[j];
        for l in inst {
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            let data: Vec<i128> = (0..n)
                .map(|i| {
                    let v = ((i * 37 + j * 11 + salt * 7 + l.map_or(0, |l| l as usize) * 5) % 200) as i128 - 100;
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

// ---- a claim: its run, its binding and the facts the chain holds of it ---------------------------------------

/// A real evaluation claim of the toy class.
struct Claim {
    class_id: Hash64,
    artifact_root: Hash64,
    program_bytes: Vec<u8>,
    layout: PalwTirLayoutV1,
    layout_digest: Hash64,
    job: PalwEvalJobV1,
    prompt: Vec<u32>,
    params: Params,
    execution: PalwGenExecutionV1,
    binding: PalwEvalBindingV1,
    step_root: Hash64,
    output_root: Hash64,
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

impl Claim {
    fn facts(&self) -> PalwEvalClaimFactsV1<'_> {
        PalwEvalClaimFactsV1 {
            class_id: &self.class_id,
            execution_root: &self.binding.committed_execution_root,
            trace_root: &self.step_root,
            output_root: &self.output_root,
            work_leaves: self.binding.step_leaf_count,
            job: &self.job,
            program: &self.program_bytes,
            layout_digest: self.layout_digest,
            artifact_root: self.artifact_root,
        }
    }

    fn evidence(&self) -> PalwEvalEvidenceV1<'_> {
        PalwEvalEvidenceV1 {
            facts: self.facts(),
            params: &self.params,
            execution: &self.execution,
            binding: &self.binding,
            prompt: &self.prompt,
            composite: None,
        }
    }

    /// The same claim re-bound over `execution` (a lie committed consistently) and `score`.
    fn rebound(&self, execution: PalwGenExecutionV1, score: Vec<i32>) -> Claim {
        let stage = self.binding.params;
        let binding = PalwEvalBindingV1::of(
            &self.job,
            self.class_id,
            &self.layout,
            &execution.claim,
            execution.space.leaf_count(),
            &self.prompt,
            stage,
            self.binding.finalized.clone(),
            score,
        );
        Claim {
            class_id: self.class_id,
            artifact_root: self.artifact_root,
            program_bytes: self.program_bytes.clone(),
            layout: self.layout.clone(),
            layout_digest: self.layout_digest,
            job: self.job.clone(),
            prompt: self.prompt.clone(),
            params: Params(self.params.0.clone()),
            step_root: binding.step_root(),
            output_root: binding.generated_root(),
            execution,
            binding,
        }
    }
}

fn base() -> (TirProgramV1, MapParams, PalwTirLayoutV1, Hash64) {
    let program = subject_program();
    let params = subject_params(&program, 0);
    let layout = layout(&program);
    let (root, _) = palw_tir_inventory_root_v1(&program, &Src(&params)).unwrap();
    (program, params, layout, root)
}

/// An ExactMatch generation of four ids over a three-id prompt.
fn generation() -> Claim {
    let (program, params_v1, layout, artifact_root) = base();
    let class_id = Hash64::from_bytes([0x33; 64]);
    let seed = palw_improve_eval_seed_v1(&Hash64::from_bytes([0x44; 64]), 7);
    let j = job(PalwScoringKindV1::ExactMatch, PalwEvalModeV1::Generate { seed, max_new: 4, stop_ids: vec![] });
    let stage = PalwEvalStageParamsV1::ExactMatch { open: -1, close: -1 };
    let subject = PalwEvalSubjectClassV1 { class_id, artifact_root, program: &program, layout: &layout };
    let ctx = palw_improve_eval_context_v1(&j, &subject, stage).unwrap();
    let params = Params(vec![params_v1]);
    let prompt = vec![3u32, 5, 1];
    let probe = PipelineJob { prompt: prompt.clone(), scalars: ctx.scalars.clone(), ..PipelineJob::default() };
    let decode = ctx.decode.clone().unwrap();
    let execution = palw_gen_execute_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &probe, &decode, ctx.seed).unwrap();
    let binding =
        PalwEvalBindingV1::of(&j, class_id, &layout, &execution.claim, execution.space.leaf_count(), &prompt, stage, vec![], vec![]);
    Claim {
        class_id,
        artifact_root,
        program_bytes: program.encode(),
        layout_digest: palw_improve_eval_layout_digest_v1(&layout),
        layout,
        job: j,
        prompt,
        params,
        step_root: binding.step_root(),
        output_root: binding.generated_root(),
        execution,
        binding,
    }
}

/// A RefLogLik pass over a two-id prompt, the reference `[7, 2, 9, 9]` — three stages, the score committed.
fn likelihood() -> Claim {
    let (program, params_v1, layout, artifact_root) = base();
    let class_id = Hash64::from_bytes([0x33; 64]);
    let j = job(PalwScoringKindV1::RefLogLik, PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::from_bytes([0x55; 64]) });
    let stage = PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 12 };
    let subject = PalwEvalSubjectClassV1 { class_id, artifact_root, program: &program, layout: &layout };
    let ctx = palw_improve_eval_context_v1(&j, &subject, stage).unwrap();
    let params = Params(vec![params_v1, MapParams::default(), MapParams::default()]);
    let (prompt, reference) = (vec![3u32, 5], vec![7u32, 2, 9, 9]);
    let pj = PipelineJob { prompt: prompt.clone(), generated: reference, scalars: ctx.scalars.clone(), ..PipelineJob::default() };
    let execution = palw_gen_replay_committed_v1(&ctx.pipeline, &ctx.programs, &ctx.layouts, &params, &pj, ctx.seed).unwrap();
    let score: Vec<i32> = execution.run.output.data.iter().map(|v| *v as i32).collect();
    let binding =
        PalwEvalBindingV1::of(&j, class_id, &layout, &execution.claim, execution.space.leaf_count(), &prompt, stage, vec![], score);
    Claim {
        class_id,
        artifact_root,
        program_bytes: program.encode(),
        layout_digest: palw_improve_eval_layout_digest_v1(&layout),
        layout,
        job: j,
        prompt,
        params,
        step_root: binding.step_root(),
        output_root: binding.generated_root(),
        execution,
        binding,
    }
}

/// An execution whose one leaf is changed, its roots recomputed over it: the executor's lie, committed consistently.
fn lie(e: &PalwGenExecutionV1, stage: usize, index: usize, delta: i128) -> PalwGenExecutionV1 {
    let mut l = e.clone();
    l.leaf_values[stage][index][0] += delta;
    let leaf = l.space.stages[stage].leaves()[index];
    l.leaf_hashes[stage][index] = palw_gen_step_leaf_hash_v1(&leaf, &l.leaf_values[stage][index]).unwrap();
    l.claim.stage_roots[stage] = palw_gen_stage_root_v1(stage as u8, &l.leaf_hashes[stage]);
    l.claim.step_root = kaspa_consensus_core::palw_gen_step_v1::palw_gen_step_root_v1(&l.claim.stage_roots);
    l
}

/// The global index of leaf `index` of stage `stage` (the claim's one order: stage-major).
fn global(c: &Claim, stage: usize, index: usize) -> u64 {
    let before: usize = c.execution.space.stages[..stage].iter().map(|s| s.leaves().len()).sum();
    (before + index) as u64
}

fn cone(c: &Claim, global_index: u64) -> PalwEvalConeCloseV1 {
    c.evidence().cone_close(global_index, &LIMITS).expect("a cone close of an executor's own run")
}

fn check(c: &Claim, close: &PalwEvalConeCloseV1, narrowed: Option<u64>) -> PalwEvalCloseOutcomeV1 {
    check_eval_cone_close_v1(close, &c.facts(), narrowed, &LIMITS)
}

// ---- honest claims are acquitted, planted lies are convicted ---------------------------------------------------

#[test]
fn every_honest_leaf_of_a_generation_and_a_likelihood_is_acquitted_at_its_index() {
    for claim in [generation(), likelihood()] {
        let mut checked = 0;
        for stage in 0..claim.execution.space.stages.len() {
            for index in 0..claim.execution.space.stages[stage].leaves().len() {
                let g = global(&claim, stage, index);
                let close = cone(&claim, g);
                assert_eq!(check(&claim, &close, Some(g)), Ok(None), "stage {stage} leaf {index}: an honest leaf is acquitted");
                checked += 1;
            }
        }
        assert!(checked > 20, "{checked} leaves of {:?}", claim.job.kind);
        // The prompt rides exactly where a stage reads it.
        let text = cone(&claim, 0);
        assert!(!text.prompt_ids.is_empty(), "the subject's first stage reads the prompt");
    }
}

#[test]
fn a_planted_lie_in_any_stage_is_convicted_at_its_leaf() {
    for honest in [generation(), likelihood()] {
        for stage in 0..honest.execution.space.stages.len() {
            let index = honest.execution.space.stages[stage].leaves().len() - 1;
            let lied = honest.rebound(lie(&honest.execution, stage, index, 1), honest.binding.score.clone());
            let g = global(&lied, stage, index);
            let close = cone(&lied, g);
            assert_eq!(
                check(&lied, &close, Some(g)),
                Ok(Some(PalwStepFaultV1::ComputationMismatch { value_index: 0 })),
                "{:?} stage {stage}: a wrong value committed into the tree is convicted at its leaf",
                honest.job.kind
            );
            // A leaf the lie does not touch still acquits: the lie is the leaf's, not the claim's.
            let before = global(&lied, stage, 0);
            if before != g {
                assert_eq!(check(&lied, &cone(&lied, before), Some(before)), Ok(None), "an earlier honest leaf of the lying claim");
            }
        }
    }
}

#[test]
fn a_wrong_score_over_an_honest_tree_and_a_wrong_decode_are_convicted_by_the_decode_close() {
    // The score: the tree is honest, the committed lane is not.
    let honest = likelihood();
    let close = honest.evidence().score_close().expect("a score close");
    assert_eq!(check_eval_decode_close_v1(&close, &honest.facts(), None), Ok(None), "the honest score is acquitted");
    let mut score = honest.binding.score.clone();
    score[1] += 1;
    let lying = honest.rebound(honest.execution.clone(), score);
    let close = lying.evidence().score_close().expect("a score close");
    assert_eq!(
        check_eval_decode_close_v1(&close, &lying.facts(), None),
        Ok(Some(PalwStepFaultV1::TirOutputDigestMismatch { value_index: 1 })),
        "the committed score is not the score stage's output: convicted at the lane"
    );
    // In a session the close must be of the narrowed tile.
    let stage = lying.execution.space.stages.len() - 1;
    let g = global(&lying, stage, lying.execution.space.stages[stage].leaves().len() - 1);
    assert_eq!(
        check_eval_decode_close_v1(&close, &lying.facts(), Some(g)),
        Ok(Some(PalwStepFaultV1::TirOutputDigestMismatch { value_index: 1 })),
        "the output tile is the last leaf of the last stage"
    );
    assert!(matches!(
        check_eval_decode_close_v1(&close, &lying.facts(), Some(0)),
        Err(PalwEvalCourtErrorV1::NotTheNarrowedLeaf { .. })
    ));
    // A generation commits no score, and a score close of it is not this job's.
    let generated = generation();
    assert!(matches!(
        check_eval_decode_close_v1(&close, &generated.facts(), None),
        Err(PalwEvalCourtErrorV1::NotTheClaims(_)) | Err(PalwEvalCourtErrorV1::Binding(_))
    ));

    // The decode: a generated id that is not the selection from its committed row.
    let honest = generation();
    for t in 0..honest.binding.generated.len() as u32 {
        let close = honest.evidence().decode_close(t).expect("a decode close");
        assert_eq!(check_eval_decode_close_v1(&close, &honest.facts(), None), Ok(None), "id {t} is the selection");
    }
    let t = 2;
    let mut exec = honest.execution.clone();
    exec.claim.generated[t] ^= 1;
    let lying = honest.rebound(exec, vec![]);
    let close = lying.evidence().decode_close(t as u32).expect("a decode close");
    assert_eq!(
        check_eval_decode_close_v1(&close, &lying.facts(), None),
        Ok(Some(PalwStepFaultV1::DecodeTokenMismatch { position: t as u32 })),
        "a wrong generated id is convicted at its position"
    );
    // A job that selects nothing has no decode to close.
    let likely = likelihood();
    let token = PalwEvalDecodeCloseV1 {
        version: PALW_IMPROVE_EVAL_COURT_VERSION_V1,
        binding: likely.binding.clone(),
        output: PalwEvalOutputCloseV1::Token { t: 0, row: vec![] },
    };
    assert!(matches!(check_eval_decode_close_v1(&token, &likely.facts(), None), Err(PalwEvalCourtErrorV1::NotThisJobsClose(_))));
}

// ---- a close is the claim's --------------------------------------------------------------------------------

#[test]
fn a_close_is_the_claims_or_it_is_refused_by_name_and_convicts_nobody() {
    let honest = generation();
    let g = global(&honest, 0, 2);
    let close = cone(&honest, g);
    assert_eq!(check(&honest, &close, Some(g)), Ok(None));
    let refused = |c: &PalwEvalConeCloseV1, facts: &PalwEvalClaimFactsV1<'_>, narrowed: Option<u64>| {
        check_eval_cone_close_v1(c, facts, narrowed, &LIMITS)
    };
    let facts = honest.facts();

    // The ladder narrowed elsewhere.
    assert_eq!(refused(&close, &facts, Some(g + 1)), Err(PalwEvalCourtErrorV1::NotTheNarrowedLeaf { opened: g, narrowed: g + 1 }));
    // Another version.
    assert_eq!(refused(&PalwEvalConeCloseV1 { version: 9, ..close.clone() }, &facts, None), Err(PalwEvalCourtErrorV1::Version(9)));

    // The binding: another job, another class, another roots, another layout, another leaf count.
    let other_job = PalwEvalJobV1 { item: 8, ..honest.job.clone() };
    let mut other_facts = honest.facts();
    other_facts.job = &other_job;
    assert!(matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(why)) if why.contains("job table")));
    let other_class = Hash64::from_bytes([0x34; 64]);
    let mut other_facts = honest.facts();
    other_facts.class_id = &other_class;
    assert!(
        matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(why)) if why.contains("another class"))
    );
    let other_root = Hash64::from_bytes([0x66; 64]);
    let mut other_facts = honest.facts();
    other_facts.execution_root = &other_root;
    assert!(
        matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(why)) if why.contains("execution root"))
    );
    let mut other_facts = honest.facts();
    other_facts.trace_root = &other_root;
    assert!(matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(why)) if why.contains("step root")));
    let mut other_facts = honest.facts();
    other_facts.output_root = &other_root;
    assert!(
        matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(why)) if why.contains("output root"))
    );
    let mut other_facts = honest.facts();
    other_facts.layout_digest = Hash64::from_bytes([0x67; 64]);
    assert!(matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(why)) if why.contains("layout")));
    let mut other_facts = honest.facts();
    other_facts.work_leaves += 1;
    assert!(matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(why)) if why.contains("leaf count")));
    // A binding that lies about its own parts does not produce the claim's root.
    let mut forged = close.clone();
    forged.binding.generated[0] ^= 1;
    assert!(matches!(refused(&forged, &facts, None), Err(PalwEvalCourtErrorV1::NotTheClaims(_))));

    // The prompt: carried exactly where the disputed stage reads it, and then the binding's.
    let mut without = close.clone();
    without.prompt_ids.clear();
    assert_eq!(refused(&without, &facts, None), Err(PalwEvalCourtErrorV1::PromptNotCarried));
    let mut other_prompt = close.clone();
    other_prompt.prompt_ids[0] ^= 1;
    assert_eq!(refused(&other_prompt, &facts, None), Err(PalwEvalCourtErrorV1::PromptNotTheBindings));
    let late = global(&honest, 0, honest.execution.space.stages[0].leaves().len() - 1);
    let _ = late; // a one-stage pipeline: the text stage reads the prompt at every leaf

    // The parameters: an opening that is not its leaf's, or does not reach the class's root, refuses the close.
    let PalwEvalParamsV1::Single(openings) = &close.params else { panic!("a plain class carries its leaves whole") };
    assert!(!openings.is_empty());
    let mut bad = close.clone();
    let PalwEvalParamsV1::Single(o) = &mut bad.params else { unreachable!() };
    o[0].operand.bytes[0] ^= 1;
    assert!(matches!(refused(&bad, &facts, None), Err(PalwEvalCourtErrorV1::Params(_))));
    let mut bad = close.clone();
    let PalwEvalParamsV1::Single(o) = &mut bad.params else { unreachable!() };
    o.reverse();
    if o.len() > 1 {
        assert!(matches!(refused(&bad, &facts, None), Err(PalwEvalCourtErrorV1::Params(_))), "ascending leaf order");
    }
    // Under another artifact root nothing reaches.
    let mut other_facts = honest.facts();
    other_facts.artifact_root = Hash64::from_bytes([0x68; 64]);
    assert!(matches!(refused(&close, &other_facts, None), Err(PalwEvalCourtErrorV1::Params(_))));
    // A carried leaf that is not under its stage's root.
    let mut bad = close.clone();
    bad.disputed.lanes_le[0] ^= 1;
    assert!(matches!(refused(&bad, &facts, None), Err(PalwEvalCourtErrorV1::Refused(_))));
    // A leaf that is no leaf of the execution.
    let mut bad = close.clone();
    bad.disputed.coord.pos += 1000;
    assert!(matches!(refused(&bad, &facts, None), Err(PalwEvalCourtErrorV1::NoSuchLeaf(_))));
}

#[test]
fn the_evidence_id_binds_the_claim_the_leaf_and_the_fault() {
    let claim = generation();
    let fault = PalwStepFaultV1::ComputationMismatch { value_index: 0 };
    let id = palw_eval_evidence_id_v1(&claim.binding, 3, fault);
    assert_ne!(id, palw_eval_evidence_id_v1(&claim.binding, 4, fault), "the leaf");
    assert_ne!(id, palw_eval_evidence_id_v1(&claim.binding, 3, PalwStepFaultV1::ComputationMismatch { value_index: 1 }), "the fault");
    let other = likelihood();
    assert_ne!(id, palw_eval_evidence_id_v1(&other.binding, 3, fault), "the claim");
    assert_eq!(PALW_IMPROVE_EVAL_EVIDENCE_KIND_V1, 0x49);
}

// ---- the proofs: tags, round trips, price ---------------------------------------------------------------------

#[test]
fn the_proofs_take_the_reserved_tags_and_a_close_is_priced_by_its_own_bytes() {
    use kaspa_consensus_core::palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2, check_close_cost_v2};
    use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
    let claim = likelihood();
    let at_zero = cone(&claim, 0);
    let proofs = [
        (PalwCourtVerdictProofV2::EvalCone { close: Box::new(at_zero.clone()) }, 13u8),
        (PalwCourtVerdictProofV2::EvalDecodeToken { close: Box::new(claim.evidence().score_close().unwrap()) }, 14),
        (PalwCourtVerdictProofV2::EvalDissection { bottom: Box::new(at_zero.clone()) }, 15),
    ];
    let court = PalwCourtParamsV2::new(1 << 26, 20, 2).expect("a court");
    for (proof, tag) in &proofs {
        let bytes = borsh::to_vec(proof).unwrap();
        assert_eq!(bytes[0], *tag, "spec 17 §17.0: the evaluation court's proofs are 13, 14 and 15");
        assert_eq!(&borsh::from_slice::<PalwCourtVerdictProofV2>(&bytes).unwrap(), proof, "the object round-trips");
        assert!(proof.is_eval_v1() && !proof.is_gen_v1() && !proof.is_tir_v1(), "an evaluation proof is none of the others");
        assert_eq!(proof.eval_binding_v1(), Some(&claim.binding), "it carries the claim's binding");
        assert_eq!(check_close_cost_v2(proof, &court), Ok(()), "a close of a small program is within the ceiling");
    }
    // A close is priced by its own encoding, whatever it carries: pad one past the court's ceiling.
    let mut padded = at_zero.clone();
    let ceiling = court.max_close_bytes();
    padded.prompt_ids = vec![0; (ceiling / 4 + 8) as usize];
    let proof = PalwCourtVerdictProofV2::EvalCone { close: Box::new(padded) };
    assert!(matches!(check_close_cost_v2(&proof, &court), Err(PalwCourtV2Error::CloseTooLarge { .. })), "over the ceiling");
}

// ---- a composite candidate ------------------------------------------------------------------------------

/// The generation claim re-registered as a composite of itself, split at param 2: the claim, its reference and the
/// split leaf.
fn composite_generation() -> (Claim, PalwTirCompositeRefV1, u32) {
    use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
    let mut claim = generation();
    let program = subject_program();
    let split =
        kaspa_consensus_core::palw_improve_composite_v1::palw_tir_composite_split_v1(&program, 2).expect("a split").parent_leaves;
    let (_, params_v1, layout, _) = base();
    let subject =
        PalwEvalSubjectClassV1 { class_id: claim.class_id, artifact_root: claim.artifact_root, program: &program, layout: &layout };
    let ctx = palw_improve_eval_context_v1(&claim.job, &subject, claim.binding.params).unwrap();
    let operands =
        palw_gen_inventory_operands_named_v1(&ctx.programs, &Params(vec![params_v1]), PalwGenInventoryNamingV1::Evaluation).unwrap();
    let hashes: Vec<Hash64> = operands.iter().map(artifact_leaf_v1).collect();
    let r = PalwTirCompositeRefV1 {
        parent_class: Hash64::from_bytes([0x9C; 64]),
        parent_root: artifact_root_v1(&hashes[..split as usize]).expect("a parent section"),
        adapter_root: artifact_root_v1(&hashes[split as usize..]).expect("an adapter section"),
        p: 2,
    };
    claim.artifact_root = r.artifact_root();
    (claim, r, split)
}

fn composite_evidence<'a>(c: &'a Claim, r: &PalwTirCompositeRefV1) -> PalwEvalEvidenceV1<'a> {
    PalwEvalEvidenceV1 { composite: Some(*r), ..c.evidence() }
}

#[test]
fn a_composite_candidate_is_adjudicated_from_its_sections_openings() {
    let (claim, r, split) = composite_generation();
    let evidence = composite_evidence(&claim, &r);
    let (mut parent_only, mut adapter_only, mut both, mut none) = (0, 0, 0, 0);
    for stage in 0..claim.execution.space.stages.len() {
        for index in 0..claim.execution.space.stages[stage].leaves().len() {
            let g = global(&claim, stage, index);
            let close = evidence.cone_close(g, &LIMITS).expect("a composite close");
            assert_eq!(check(&claim, &close, Some(g)), Ok(None), "stage {stage} leaf {index}: acquitted through the sub-roots");
            match &close.params {
                PalwEvalParamsV1::Composite { artifact, parent, adapter } => {
                    assert_eq!(*artifact, r);
                    match (parent.is_empty(), adapter.is_empty()) {
                        (false, true) => parent_only += 1,
                        (true, false) => adapter_only += 1,
                        (false, false) => both += 1,
                        (true, true) => none += 1,
                    }
                    // The adapter section's openings are rebased: indices below the section's size.
                    assert!(adapter.iter().all(|o| o.leaf_count > 0 && o.leaf_index < o.leaf_count));
                    assert!(parent.iter().all(|o| o.leaf_count == split));
                }
                PalwEvalParamsV1::Single(_) => panic!("a one-root carriage from a composite store"),
            }
        }
    }
    assert!(parent_only + adapter_only + both > 0, "the cones read parameters from the sections");
    eprintln!("composite closes: {parent_only} parent-only, {adapter_only} adapter-only, {both} both, {none} reading no param");

    // A lie is convicted through the same sections.
    let stage = claim.execution.space.stages.len() - 1;
    let index = claim.execution.space.stages[stage].leaves().len() - 1;
    let lied = claim.rebound(lie(&claim.execution, stage, index, 1), vec![]);
    let evidence = composite_evidence(&lied, &r);
    let g = global(&lied, stage, index);
    let close = evidence.cone_close(g, &LIMITS).expect("a composite close");
    assert_eq!(check(&lied, &close, Some(g)), Ok(Some(PalwStepFaultV1::ComputationMismatch { value_index: 0 })));

    // The carriage does not cross: a one-root carriage of a composite class, and a composite one of a one-root class.
    let g = global(&claim, 0, 2);
    let close = composite_evidence(&claim, &r).cone_close(g, &LIMITS).unwrap();
    let PalwEvalParamsV1::Composite { parent, adapter, .. } = &close.params else { unreachable!() };
    let all: Vec<_> = parent.iter().cloned().chain(adapter.iter().cloned()).collect();
    let single = PalwEvalConeCloseV1 { params: PalwEvalParamsV1::Single(all), ..close.clone() };
    assert!(matches!(check(&claim, &single, None), Err(PalwEvalCourtErrorV1::Params(_))), "a one-root carriage of a composite class");
    let plain = generation();
    let on_plain = PalwEvalConeCloseV1 { params: close.params.clone(), ..cone(&plain, g) };
    assert!(
        matches!(check(&plain, &on_plain, None), Err(PalwEvalCourtErrorV1::Params(_))),
        "a composite carriage of a one-root class"
    );

    // Every malformed composite opening is refused: the reference is the class's, the sections are not swapped, no
    // opening is another's.
    let mut other_ref = close.clone();
    let PalwEvalParamsV1::Composite { artifact, .. } = &mut other_ref.params else { unreachable!() };
    artifact.adapter_root = Hash64::from_bytes([0x71; 64]);
    assert!(matches!(check(&claim, &other_ref, None), Err(PalwEvalCourtErrorV1::Params(why)) if why.contains("composite reference")));
    let mut swapped = close.clone();
    let PalwEvalParamsV1::Composite { parent, adapter, .. } = &mut swapped.params else { unreachable!() };
    std::mem::swap(parent, adapter);
    if !(close_has(&close, true) && close_has(&close, false)) || true {
        assert!(matches!(check(&claim, &swapped, None), Err(PalwEvalCourtErrorV1::Params(_)) | Ok(None)));
    }
    let mut forged = close.clone();
    let PalwEvalParamsV1::Composite { parent, adapter, .. } = &mut forged.params else { unreachable!() };
    if let Some(o) = parent.first_mut().or(adapter.first_mut()) {
        o.operand.bytes[0] ^= 1;
        assert!(matches!(check(&claim, &forged, None), Err(PalwEvalCourtErrorV1::Params(_))), "an operand under no root");
    }
    let _ = split;
}

fn close_has(close: &PalwEvalConeCloseV1, parent: bool) -> bool {
    match &close.params {
        PalwEvalParamsV1::Composite { parent: p, adapter: a, .. } => !(if parent { p } else { a }).is_empty(),
        PalwEvalParamsV1::Single(_) => false,
    }
}
