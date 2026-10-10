//! **K2-TIR-v3's media-pipeline family end to end** (Kernel design §K.3, RFC-0003 pipelines, RFC-0004 §7.2 evaluation pipelines) on
//! the IR's own v2 fixtures: a text-to-image pipeline (encoder rows → denoiser with per-step `R` noise and a `post` latent write →
//! decoder), a vision encoder over a canonical job image, a vision-language text stream, an encoder–decoder, and an exact-match
//! evaluation pipeline over a decode stage. Registration needs the media-pipeline family; the kernel's per-stage traces equal the
//! reference pipeline run; an honest claim passes; a lie inside a stage, in an edge, or in `R`'s draw is found and convicted by a
//! court reading only public commitments; withheld upstream material is `Unavailable`.

#[path = "../../misaka-palw-tir/tests/v2common/mod.rs"]
mod v2common;

use misaka_palw_kernel::KernelDescriptorV1;
use misaka_palw_kernel::descriptor::{
    KernelScheduleV1, KernelStatusV1, builtin_schedule_v1, k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v3_descriptor,
};
use misaka_palw_kernel::family::ConstraintFamilyV1;
use misaka_palw_kernel::outcome::RegistrationOutcomeV1;
use misaka_palw_kernel::pipeline::*;
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_kernel::verify::MaterialV1;
use misaka_palw_tir::Tensor;
use misaka_palw_tir::pipeline::*;
use misaka_palw_tir::program_v2::TirProgramV2;
use v2common::*;

fn armed(d: &KernelDescriptorV1) -> KernelScheduleV1 {
    KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 })
}

fn params_for(programs: &[TirProgramV2], seed: u64) -> ProgramParams {
    ProgramParams(programs.iter().enumerate().map(|(i, p)| materialize_v2(p, seed + i as u64)).collect())
}

const HEADER: PipelineHeaderV1 = PipelineHeaderV1 { network_domain: [9; 64], ruleset_digest: [3; 64], class_binding_id: [7; 64] };
const RANDOM_BINDING: [u8; 64] = [0x5E; 64];

/// A producer's pipeline claim: everything public, and its honest trace.
struct PClaim {
    p: TirPipelineV1,
    programs: Vec<TirProgramV2>,
    params: ProgramParams,
    pcs: Vec<ParamCommitmentsV1>,
    random: GenRandom,
    job: PipelineJob,
    plan: PipelinePlanV1,
    trace: PipelineTraceV1,
}

impl PClaim {
    fn new((p, programs): (TirPipelineV1, Vec<TirProgramV2>), job: PipelineJob, seed: u64) -> Self {
        let d = k2_tir_v3_descriptor();
        let params = params_for(&programs, seed);
        let random = GenRandom { seed: [4; 32], position: 0 };
        // A text pipeline's committed ids come from generating once (RFC-0001 §A's decoder, greedy here).
        let job = if stream_stage(&p).is_some() && job.generated.is_empty() {
            let (_, generated) = run_text_pipeline(&p, &programs, &params, &random, &job, &mut greedy(3)).unwrap();
            PipelineJob { generated, ..job }
        } else {
            job
        };
        let plan = pipeline_plan_v1(&d, &p, &programs).unwrap();
        let trace = trace_pipeline_v1(&p, &programs, &params, &random, &job).unwrap();
        let pcs = params.0.iter().map(ParamCommitmentsV1::of).collect();
        PClaim { p, programs, params, pcs, random, job, plan, trace }
    }

    fn evidence(&self, committed: &PipelineTraceV1) -> PipelineEvidenceV1 {
        build_pipeline_evidence_v1(
            HEADER,
            &k2_tir_v3_descriptor(),
            &self.p,
            &self.programs,
            &self.plan,
            &self.pcs,
            committed,
            &self.job,
            RANDOM_BINDING,
            2,
        )
        .unwrap()
    }

    /// Verify `committed` served by `serve` (per stage), and hand back the verdict and the public context's courts.
    fn check(&self, committed: &PipelineTraceV1, withhold_stage: Option<usize>) -> (PipelineVerdictV1, Courts) {
        self.check_ev(committed, self.evidence(committed), withhold_stage)
    }

    /// Verify against an evidence object the producer committed earlier.
    fn check_ev(
        &self,
        committed: &PipelineTraceV1,
        ev: PipelineEvidenceV1,
        withhold_stage: Option<usize>,
    ) -> (PipelineVerdictV1, Courts) {
        let d = k2_tir_v3_descriptor();
        let traces: Vec<_> = committed.stages.iter().map(|t| t.evidence()).collect();
        let mats: Vec<PipelineMaterialV1<'_>> = self
            .p
            .stages
            .iter()
            .enumerate()
            .map(|(si, st)| PipelineMaterialV1 { trace: &committed.stages[si], params: &self.params.0[st.program as usize] })
            .collect();
        struct Withheld;
        impl MaterialV1 for Withheld {
            fn node_value(&self, _: u32, _: u16, _: u16) -> Option<Tensor> {
                None
            }
            fn param(&self, _: u16, _: Option<u16>) -> Option<Tensor> {
                None
            }
        }
        let refs: Vec<&dyn MaterialV1> =
            mats.iter().enumerate().map(|(i, m)| if Some(i) == withhold_stage { &Withheld as &dyn MaterialV1 } else { m }).collect();
        let ctx = PipelineContextV1 {
            descriptor: &d,
            pipeline: &self.p,
            programs: &self.programs,
            plan: &self.plan,
            header: HEADER,
            params: &self.pcs,
            traces: &traces,
            evidence: &ev,
            job: &self.job,
            random: &self.random,
            random_binding: RANDOM_BINDING,
            claim_id: [8; 64],
            beacon: [0x42; 64],
        };
        let v = verify_pipeline_v1(&ctx, &refs);
        // The courts: a fresh party with only the public context.
        let stage_court = {
            let (d, ev, traces) = (d.clone(), ev.clone(), traces.clone());
            let (p, programs, plan, pcs, job) =
                (self.p.clone(), self.programs.clone(), self.plan.clone(), self.pcs.clone(), self.job.clone());
            let seed = self.random.seed;
            move |stage: u8, proof: &misaka_palw_kernel::KernelFaultProofV1| {
                let random = GenRandom { seed, position: 0 };
                let ctx = PipelineContextV1 {
                    descriptor: &d,
                    pipeline: &p,
                    programs: &programs,
                    plan: &plan,
                    header: HEADER,
                    params: &pcs,
                    traces: &traces,
                    evidence: &ev,
                    job: &job,
                    random: &random,
                    random_binding: RANDOM_BINDING,
                    claim_id: [8; 64],
                    beacon: [0x42; 64],
                };
                verify_stage_fault_v1(&ctx, stage, proof).is_ok()
            }
        };
        let edge_court = {
            let (d, ev, traces) = (d.clone(), ev.clone(), traces.clone());
            let (p, programs, plan, pcs, job) =
                (self.p.clone(), self.programs.clone(), self.plan.clone(), self.pcs.clone(), self.job.clone());
            let seed = self.random.seed;
            move |proof: &EdgeFaultProofV1| {
                let random = GenRandom { seed, position: 0 };
                let ctx = PipelineContextV1 {
                    descriptor: &d,
                    pipeline: &p,
                    programs: &programs,
                    plan: &plan,
                    header: HEADER,
                    params: &pcs,
                    traces: &traces,
                    evidence: &ev,
                    job: &job,
                    random: &random,
                    random_binding: RANDOM_BINDING,
                    claim_id: [8; 64],
                    beacon: [0x42; 64],
                };
                verify_edge_fault_v1(&ctx, proof)
            }
        };
        (v, Courts { stage: Box::new(stage_court), edge: Box::new(edge_court) })
    }
}

struct Courts {
    stage: Box<dyn Fn(u8, &misaka_palw_kernel::KernelFaultProofV1) -> bool>,
    edge: Box<dyn Fn(&EdgeFaultProofV1) -> Result<(u8, u16, u32), EdgeDismissalV1>>,
}

fn toy() -> PClaim {
    PClaim::new(toy_pipeline(), toy_job(), 100)
}

#[test]
fn a_pipeline_needs_the_media_pipeline_family_and_is_eligible_under_an_armed_v3() {
    let (p, programs) = toy_pipeline();
    for d in [k2_tir_v1_descriptor(), k2_tir_v2_descriptor()] {
        let o = pipeline_registration_outcome_v1(&armed(&d), &d, &p, &programs, 0);
        assert!(
            matches!(o, RegistrationOutcomeV1::KernelExtensionRequired { family: Some(ConstraintFamilyV1::MediaPipeline), .. }),
            "{o}"
        );
    }
    let v3 = k2_tir_v3_descriptor();
    assert_eq!(pipeline_registration_outcome_v1(&builtin_schedule_v1(), &v3, &p, &programs, 0).code(), "KERNEL_NOT_ACTIVE");
    for (name, (p, programs)) in [
        ("text-to-image", toy_pipeline()),
        ("vision", vision_pipeline()),
        ("vision-language", vlm_pipeline()),
        ("encoder-decoder", encdec_pipeline()),
        ("exact-match evaluation", eval_exact_match_pipeline()),
        ("reference log-likelihood evaluation", eval_ref_loglik_pipeline()),
    ] {
        let o = pipeline_registration_outcome_v1(&armed(&v3), &v3, &p, &programs, 0);
        let RegistrationOutcomeV1::EligibleAt { error_bits, .. } = o else { panic!("{name}: {o}") };
        assert!(error_bits >= 128, "{name}: 2^-{error_bits}");
    }
}

#[test]
fn a_pipeline_plan_cannot_drop_an_edge_or_a_stage_or_overclaim() {
    let (p, programs) = toy_pipeline();
    let d = k2_tir_v3_descriptor();
    let plan = pipeline_plan_v1(&d, &p, &programs).unwrap();
    let check = |pl: &PipelinePlanV1| check_pipeline_plan_v1(&armed(&d), &d, &p, &programs, pl, 0);
    check(&plan).unwrap();
    let mut no_edge = plan.clone();
    no_edge.edges.pop();
    assert_eq!(check(&no_edge).unwrap_err().code(), "INCOMPLETE_COVERAGE");
    let mut no_stage = plan.clone();
    no_stage.stages.pop();
    assert_eq!(check(&no_stage).unwrap_err().code(), "INCOMPLETE_COVERAGE");
    let mut over = plan.clone();
    over.declared_error_bits += 1;
    assert_eq!(check(&over).unwrap_err().code(), "PLAN_FORGED");
    let mut weak_stage = plan.clone();
    if let Some(r) = weak_stage.stages[1].relations.iter_mut().find(|r| r.repetitions > 0) {
        r.repetitions = 1;
        assert_eq!(check(&weak_stage).unwrap_err().code(), "PLAN_FORGED");
    }
}

#[test]
fn the_kernels_stage_traces_are_the_reference_pipeline_run() {
    for (name, c) in [
        ("text-to-image", toy()),
        ("vision", PClaim::new(vision_pipeline(), vision_job(), 300)),
        (
            "vision-language",
            PClaim::new(vlm_pipeline(), PipelineJob { prompt: vec![3, PLACEHOLDER, PLACEHOLDER, 5], ..vision_job() }, 400),
        ),
        ("encoder-decoder", PClaim::new(encdec_pipeline(), encdec_job(), 500)),
        (
            "exact-match evaluation",
            PClaim::new(
                eval_exact_match_pipeline(),
                PipelineJob { prompt: vec![3, 5, 7], scalars: vec![-1, -1], ..PipelineJob::default() },
                600,
            ),
        ),
    ] {
        let run = run_pipeline(&c.p, &c.programs, &c.params, &c.random, &c.job).unwrap();
        for (si, st) in c.p.stages.iter().enumerate() {
            let v = stage_view_v1(&c.programs[st.program as usize]);
            let ours: Vec<&Tensor> =
                c.trace.stages[si].values.iter().map(|pos| &pos[v.post_occurrence as usize][v.output_node as usize]).collect();
            let theirs = run.stages[si].rows();
            assert_eq!(ours.len(), theirs.len(), "{name} stage {si}");
            for (a, b) in ours.iter().zip(&theirs) {
                assert_eq!(*a, b, "{name} stage {si}: the kernel's trace is not the reference run");
            }
        }
        let (v, _) = c.check(&c.trace, None);
        let PipelineVerdictV1::Pass { error_bits, edges_checked, .. } = v else { panic!("{name}: {v:?}") };
        assert!(error_bits >= 128 && edges_checked > 0, "{name}");
    }
}

#[test]
fn a_lie_inside_a_stage_is_convicted_by_the_stage_court() {
    let c = toy();
    // The denoiser (stage 1): bump one committed value of its last position's post block.
    let mut lie = c.trace.clone();
    let v = stage_view_v1(&c.programs[1]);
    let last = lie.stages[1].values.len() - 1;
    let t = &mut lie.stages[1].values[last][v.post_occurrence as usize][0];
    t.data[0] += if t.dtype.contains(t.data[0] + 1) { 1 } else { -1 };
    let (verdict, courts) = c.check(&lie, None);
    let PipelineVerdictV1::StageFault { stage, proof } = verdict else { panic!("{verdict:?}") };
    assert_eq!(stage, 1);
    assert!((courts.stage)(stage, &proof));
    let (_, honest) = c.check(&c.trace, None);
    assert!(!(honest.stage)(stage, &proof), "the honest claim, accused of the same, is cleared");
}

#[test]
fn a_false_edge_or_a_false_draw_of_r_is_convicted_by_the_edge_court() {
    let c = toy();
    // The conditioning rows the denoiser read are not the encoder's committed rows.
    let mut lie = c.trace.clone();
    lie.stages[1].inputs[0][IN_COND as usize].data[0] += 1;
    let (verdict, courts) = c.check(&lie, None);
    let PipelineVerdictV1::EdgeFault(proof) = verdict else { panic!("{verdict:?}") };
    assert_eq!((proof.stage, proof.input), (1, IN_COND));
    assert_eq!((courts.edge)(&proof), Ok((1, IN_COND, 0)));
    // A per-step jitter that is not R's draw for that step.
    let mut lie = c.trace.clone();
    lie.stages[1].inputs[2][IN_JITTER as usize].data[3] ^= 1;
    let (verdict, courts) = c.check(&lie, None);
    let PipelineVerdictV1::EdgeFault(proof) = verdict else { panic!("{verdict:?}") };
    assert_eq!((proof.stage, proof.input, proof.position), (1, IN_JITTER, 2));
    assert!((courts.edge)(&proof).is_ok());
    // An honest edge accused is dismissed; a forged opening is not authentic.
    let (_, honest) = c.check(&c.trace, None);
    let accuse = EdgeFaultProofV1 { claimed: c.trace.stages[1].inputs[2][IN_JITTER as usize].clone(), ..*proof.clone() };
    assert_eq!((honest.edge)(&accuse), Err(EdgeDismissalV1::NoFault));
    assert!(matches!((honest.edge)(&proof), Err(EdgeDismissalV1::NotAuthentic(_))));
}

#[test]
fn a_lie_in_every_stage_input_of_every_reference_pipeline_is_an_edge_fault_the_edge_court_convicts() {
    use std::collections::BTreeSet;
    // The media-pipeline family's review row: every edge (job scalar / tokens / counts / image, an earlier stage's rows or final
    // value, R) of every reference pipeline, lied at its first and last position, is localized to exactly that edge and convicted
    // by the public edge court; the honest claim, accused of the same, is cleared.
    let mut kinds = BTreeSet::new();
    let pipelines = [
        ("text-to-image", toy()),
        ("vision", PClaim::new(vision_pipeline(), vision_job(), 300)),
        (
            "vision-language",
            PClaim::new(vlm_pipeline(), PipelineJob { prompt: vec![3, PLACEHOLDER, PLACEHOLDER, 5], ..vision_job() }, 400),
        ),
        ("encoder-decoder", PClaim::new(encdec_pipeline(), encdec_job(), 500)),
        (
            "exact-match evaluation",
            PClaim::new(
                eval_exact_match_pipeline(),
                PipelineJob { prompt: vec![3, 5, 7], scalars: vec![-1, -1], ..PipelineJob::default() },
                600,
            ),
        ),
    ];
    let mut checked = 0usize;
    for (name, c) in &pipelines {
        for e in &c.plan.edges {
            let positions = c.trace.stages[e.stage as usize].inputs.len();
            for p in [0, positions.saturating_sub(1)] {
                let Some(t) = c.trace.stages[e.stage as usize].inputs.get(p).and_then(|r| r.get(e.input as usize)) else { continue };
                if t.data.is_empty() {
                    continue;
                }
                let mut lie = c.trace.clone();
                let x = &mut lie.stages[e.stage as usize].inputs[p][e.input as usize];
                x.data[0] += if x.dtype.contains(x.data[0] + 1) { 1 } else { -1 };
                let (verdict, courts) = c.check(&lie, None);
                let PipelineVerdictV1::EdgeFault(proof) = verdict else { panic!("{name}: edge {e:?} at {p}: {verdict:?}") };
                assert_eq!((proof.stage, proof.input, proof.position as usize), (e.stage, e.input, p), "{name}: {e:?}");
                assert!((courts.edge)(&proof).is_ok(), "{name}: the public edge court dismissed a true lie at {e:?}");
                let (_, honest) = c.check(&c.trace, None);
                assert!((honest.edge)(&proof).is_err(), "{name}: the honest claim, accused of the same, is cleared");
                kinds.insert(e.kind);
                checked += 1;
            }
        }
    }
    assert!(checked >= 30, "{checked} lies");
    assert_eq!(
        kinds.iter().copied().collect::<Vec<_>>(),
        EDGE_KINDS_WITH_A_NEGATIVE_TEST,
        "binding tags: 0 JobScalar … 6 JobImage, 7 R"
    );
}

/// Binding kinds some reference pipeline has an edge of (the rest — if any — are a GAP, not a PASS).
const EDGE_KINDS_WITH_A_NEGATIVE_TEST: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

#[test]
fn a_false_image_edge_is_convicted_and_withheld_upstream_rows_are_unavailable() {
    let c = PClaim::new(vlm_pipeline(), PipelineJob { prompt: vec![3, PLACEHOLDER, PLACEHOLDER, 5], ..vision_job() }, 400);
    // The vision stage's committed image is not the job's canonical pixels.
    let mut lie = c.trace.clone();
    lie.stages[0].inputs[0][0].data[0] = (lie.stages[0].inputs[0][0].data[0] + 1) % 256;
    let (verdict, courts) = c.check(&lie, None);
    let PipelineVerdictV1::EdgeFault(proof) = verdict else { panic!("{verdict:?}") };
    assert_eq!(proof.stage, 0);
    assert!((courts.edge)(&proof).is_ok());
    // The text stage reads the vision rows; if the vision stage's values are withheld, the edge cannot be checked: DA, not fraud.
    let (verdict, _) = c.check(&c.trace, Some(0));
    assert!(matches!(verdict, PipelineVerdictV1::Unavailable { .. }), "{verdict:?}");
}

#[test]
fn a_pipeline_claim_is_bound_to_its_job_and_its_r_binding() {
    let mut c = toy();
    let ev = c.evidence(&c.trace);
    let (v, _) = c.check_ev(&c.trace, ev.clone(), None);
    assert!(matches!(v, PipelineVerdictV1::Pass { .. }), "{v:?}");
    // Presented as the claim of another job (different scalars) than the one its evidence committed: refused before any check.
    c.job.scalars[0] += 1;
    let (v, _) = c.check_ev(&c.trace, ev.clone(), None);
    assert!(matches!(&v, PipelineVerdictV1::EvidenceMalformed { why } if why.contains("job")), "{v:?}");
    c.job.scalars[0] -= 1;
    // Another R binding (another seed/item) than the committed one.
    let mut other = ev.clone();
    other.random_binding = [0; 64];
    let (v, _) = c.check_ev(&c.trace, other, None);
    assert!(matches!(v, PipelineVerdictV1::EvidenceMalformed { .. }), "{v:?}");
    // Another output than the output stage's committed one.
    let mut other = ev;
    other.output_root = [1; 64];
    let (v, _) = c.check_ev(&c.trace, other, None);
    assert!(matches!(v, PipelineVerdictV1::EvidenceMalformed { .. }), "{v:?}");
}

/// **RFC-0004 §0 end to end**: evaluation claims (an exact-match evaluation pipeline over a decode stage) are verified by small
/// checks on the kernel route, reach Final through the lifecycle, and only then enter the promotion as assurance-labelled results;
/// a forged score is convicted and never counts (a missing result is a loss); the sign test and the computational bound are
/// reported apart.
#[test]
fn rfc04_promotion_reads_only_final_small_check_evaluations_and_a_forged_score_never_counts() {
    use misaka_palw_kernel::assurance::AssuranceModeV1;
    use misaka_palw_kernel::improve::{
        EpochKernelPolicyV1, EvaluationCompositionV1, EvaluationResultV1, PromotionRuleV1, SubjectV1, promotion_decision_v1,
    };
    use misaka_palw_kernel::lifecycle::{ClaimEventV1, ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1};
    use misaka_palw_kernel::receipt::TallyStateV1;

    let v3 = k2_tir_v3_descriptor();
    let cand = [0xCA; 64];
    let base = PipelineJob { prompt: vec![3, 5, 7], scalars: vec![-1, -1], ..PipelineJob::default() };
    // One evaluation claim: the subject generates; `keyed` sets the item's key to what it generated (score 1), else to an id it did
    // not generate (score 0). `forge` commits score 1 whatever the run says.
    let evaluate = |seed: u64, keyed: bool, forge: bool| -> (ClaimStateV1, i64, u16) {
        let draft = PClaim::new(eval_exact_match_pipeline(), base.clone(), seed);
        let key = if keyed { draft.job.generated.clone() } else { vec![TOK - 1; draft.job.generated.len()] };
        let c = PClaim::new(eval_exact_match_pipeline(), PipelineJob { key, ..draft.job.clone() }, seed);
        let mut committed = c.trace.clone();
        let out = c.p.output_stage as usize;
        let v = stage_view_v1(&c.programs[c.p.stages[out].program as usize]);
        let last = committed.stages[out].values.len() - 1;
        let score = &mut committed.stages[out].values[last][v.post_occurrence as usize][v.output_node as usize];
        if forge {
            score.data[0] = 1;
        }
        let claimed = score.data[0] as i64;
        let mut life = ClaimLifecycleV1::new(LifecyclePolicyV1 { check_window_daa: 100, challenge_window_daa: 50 });
        life.apply(ClaimEventV1::BindChallenge { anchor_daa: 10 }).unwrap();
        life.apply(ClaimEventV1::StartChecking { daa: 12 }).unwrap();
        let (verdict, courts) = c.check(&committed, None);
        let bits = match verdict {
            PipelineVerdictV1::Pass { error_bits, .. } => {
                life.apply(ClaimEventV1::Tally { daa: 20, state: TallyStateV1::Covered }).unwrap();
                error_bits
            }
            PipelineVerdictV1::StageFault { stage, proof } => {
                life.apply(ClaimEventV1::DisputeFiled { daa: 20 }).unwrap();
                life.apply(ClaimEventV1::CourtVerdict { daa: 30, convicted: (courts.stage)(stage, &proof) }).unwrap();
                0
            }
            v => panic!("{v:?}"),
        };
        // Terminal states (a conviction) accept nothing more.
        let _ = life.apply(ClaimEventV1::RetentionMet);
        let _ = life.apply(ClaimEventV1::Tick { daa: 100 });
        (life.state.clone(), claimed, bits)
    };
    let items: Vec<u32> = (0..12).collect();
    let mut results = Vec::new();
    for item in &items {
        let id = |s: u8| {
            let mut x = [s; 64];
            x[0] = *item as u8;
            x
        };
        let seed = 600 + *item as u64;
        // Item 5's candidate truly misses and commits a forged pass.
        for (subject, keyed, forge, cid) in
            [(SubjectV1::Parent, false, false, id(1)), (SubjectV1::Candidate(cand), *item != 5, *item == 5, id(2))]
        {
            let (state, score, bits) = evaluate(seed, keyed, forge);
            match state {
                ClaimStateV1::Final { .. } => results.push(EvaluationResultV1 {
                    claim_id: cid,
                    item: *item,
                    subject,
                    score,
                    mode: AssuranceModeV1::Probabilistic { descriptor: v3.digest(), error_bits: bits },
                }),
                ClaimStateV1::Convicted { .. } => assert!(forge, "only the forged claim is convicted"),
                s => panic!("item {item}: {s:?}"),
            }
        }
    }
    assert_eq!(results.len(), 23, "the forged evaluation never reaches Final");
    let policy = EpochKernelPolicyV1 {
        line: [5; 64],
        epoch: 1,
        opened_daa: 0,
        permitted_descriptors: vec![v3.digest()],
        composition: EvaluationCompositionV1 {
            task_root: [1; 64],
            tokenizer_or_input_schema_root: [2; 64],
            task_output_schema: [3; 64],
            score_definition_root: [4; 64],
        },
        cross_kernel_pairs: vec![],
    };
    let rule = PromotionRuleV1 { n_min: 10, delta_num: 1, delta_den: 10, alpha_num: 1, alpha_den: 20, candidates: 1 };
    let d = promotion_decision_v1(&policy, &rule, cand, &items, &results).unwrap();
    assert_eq!((d.wins, d.losses), (11, 1), "the forged item is a loss, not a win");
    assert!(d.eligible, "{:?}", d.why_not);
    assert!(
        d.computational_error_bits.is_some_and(|b| b >= 100),
        "the union bound is reported apart: {:?}",
        d.computational_error_bits
    );
    // The same results under an epoch that did not pin v3 are refused (an invalid suite change), never regraded.
    let other = EpochKernelPolicyV1 { permitted_descriptors: vec![k2_tir_v1_descriptor().digest()], ..policy };
    assert!(promotion_decision_v1(&other, &rule, cand, &items, &results).is_err());
}
