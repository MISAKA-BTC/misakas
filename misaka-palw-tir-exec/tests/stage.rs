//! **An evaluation's subject stage on the executor is the reference's stage** (`misaka_palw_tir_exec::stage`,
//! RFC-0004 §7.2): a version-1 program lifted unchanged with no input — as `palw_improve_subject_program_v1`
//! lifts a class's — stepped by `TirStageStepperV1` against `InterpreterV2` over the lifted program, the
//! reference a pipeline runs. Every position's output and commit points and the `Fixed` states after it
//! (exactly the reference's map: the instances written, no other), success against failure and the
//! failure's class — over random programs (range-extreme weights, out-of-range tokens) and the corpus
//! models; then through the pipeline runner itself (`run_pipeline`, and `run_text_pipeline` generating),
//! the stepper offered by the params against none; and members stepped together by one hub on one thread
//! while their pipelines run on their own threads, failing members and early stops included, each
//! member's run the run it has alone.

#[path = "../../misaka-palw-tir/tests/common/mod.rs"]
mod tircommon;

mod common;

use std::cell::RefCell;
use std::collections::BTreeMap;

use common::progen::{GenCfg, R, gen_params, gen_program, pick};
use misaka_palw_tir::interp::StateKey;
use misaka_palw_tir::interp_v2::{InterpreterV2, MapInputs};
use misaka_palw_tir::pipeline::{
    PipelineJob, PipelineParams, PipelineRun, RandomSource, StageDecl, StageStepperV1, TextSelectV1, TirPipelineV1, TripRule,
    run_pipeline, run_text_pipeline,
};
use misaka_palw_tir::program_v2::{OutputDecl, RandomDist, TirProgramV2};
use misaka_palw_tir::{MapParams, ParamSource, RunState, Tensor, TirErrorKind, TirProgramV1, TirResult};
use misaka_palw_tir_exec::{TirLockstepHubV1, TirLockstepSeatV1, TirParams, TirPlan, TirStageStepperV1};
use rand::{Rng, SeedableRng};
use tircommon::models::{dense, gdn_program, mamba2_program, materialize, moe_program};

fn scale() -> u64 {
    std::env::var("TIR_EXEC_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

/// The program as an evaluation lifts its subject: unchanged, no input, its logits the output.
fn lifted(p: &TirProgramV1) -> TirProgramV2 {
    TirProgramV2::from_v1_lifting_params(p, &[], OutputDecl::Logits { node: p.logits, scheme_id: p.logits_scheme_id }).expect("lifts")
}

fn tokens(rng: &mut R, p: &TirProgramV1, n: usize) -> Vec<u32> {
    (0..n)
        .map(|_| {
            if rng.gen_bool(0.9) || p.token_bound == u32::MAX {
                rng.gen_range(0..p.token_bound.min(64))
            } else {
                pick(rng, &[p.token_bound, u32::MAX])
            }
        })
        .collect()
}

#[derive(Debug, Default)]
struct Tally {
    programs: u64,
    /// Refused by the version-2 rules and the executor alike.
    refused: u64,
    steps_ok: u64,
    steps_err: u64,
    commits: u64,
    fixed: u64,
    class_diffs: u64,
}

/// Step the lifted program on the reference and on the stage stepper, position by position.
fn stage_differential(p: &TirProgramV1, params: &MapParams, toks: &[u32], t: &mut Tally) -> Result<(), String> {
    t.programs += 1;
    let decl = lifted(p);
    let reference = InterpreterV2::new(&decl);
    let plan = TirPlan::compile(p);
    let (interp, plan) = match (reference, plan) {
        (Ok(i), Ok(p)) => (i, p),
        (Err(a), Err(b)) if a.kind == b.kind => {
            t.refused += 1;
            return Ok(());
        }
        (a, b) => return Err(format!("validation differs: reference {:?}, executor {:?}", a.err(), b.err())),
    };
    assert!(TirStageStepperV1::serves(&plan, &decl), "the executor of a program serves its lifted stage");
    let inputs = MapInputs::default();
    let mut state = RunState::default();
    let exec_params = TirParams::from_map(&plan, params);
    let Some(mut stepper) = exec_params.as_ref().ok().and_then(|ep| TirStageStepperV1::for_stage(&plan, ep, &decl)) else {
        // The executor refuses the params: so does every reference step.
        for &tok in toks {
            if let Ok(s) = interp.step(params, &inputs, &mut state, tok) {
                return Err(format!("the executor refused the params but the reference stepped position {}", s.pos));
            }
            t.steps_err += 1;
        }
        return Ok(());
    };
    for (i, &tok) in toks.iter().enumerate() {
        match (interp.step(params, &inputs, &mut state, tok), stepper.step(tok)) {
            (Ok(out), Ok((mine, fixed))) => {
                if out != mine {
                    return Err(format!("step {i} (pos {}): the stepper's output or commits differ", out.pos));
                }
                if fixed != state.fixed {
                    return Err(format!(
                        "step {i}: the Fixed states differ: reference {:?}, stepper {:?}",
                        state.fixed.keys().collect::<Vec<_>>(),
                        fixed.keys().collect::<Vec<_>>()
                    ));
                }
                t.steps_ok += 1;
                t.commits += out.commits.len() as u64;
                t.fixed += fixed.len() as u64;
            }
            (Err(a), Err(b)) => {
                t.steps_err += 1;
                t.class_diffs += u64::from(a.kind != b.kind);
            }
            (a, b) => return Err(format!("step {i}: reference {:?}, stepper {:?}", a.err(), b.err())),
        }
    }
    Ok(())
}

#[test]
fn the_stage_stepper_is_the_reference_on_random_programs_step_for_step() {
    let n = 2000 * scale();
    let mut t = Tally::default();
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0x57a6_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let steps = rng.gen_range(1..=8);
        let toks = tokens(&mut rng, &g.prog, steps);
        stage_differential(&g.prog, &g.params, &toks, &mut t).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    }
    eprintln!("random programs through the stage stepper: {t:?}");
    assert!(t.refused * 20 < n, "the generator's programs are in normal form: {t:?}");
    assert!(t.steps_ok > n && t.steps_err > n / 4, "steps succeed and fail: {t:?}");
    assert!(t.fixed > 0, "Fixed states are written and compared: {t:?}");
    assert_eq!(t.class_diffs, 0, "the same failure class as the reference: {t:?}");
}

#[test]
fn the_stage_stepper_is_the_reference_on_the_corpus_models() {
    let mut t = Tally::default();
    let (moe, moe_g) = moe_program();
    let (den, den_g) = dense(&[misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL, 3]);
    let (gdn, gdn_g) = gdn_program(true);
    let (mamba, mamba_g) = mamba2_program();
    for (name, p, gens) in [("moe", moe, moe_g), ("dense", den, den_g), ("gdn", gdn, gdn_g), ("mamba2", mamba, mamba_g)] {
        for seed in [3u64, 4, 5] {
            let params = materialize(&p, &gens, seed);
            let toks: Vec<u32> = (0..12).map(|i| (i * 7 + seed as u32) % p.token_bound.min(48)).collect();
            stage_differential(&p, &params, &toks, &mut t).unwrap_or_else(|e| panic!("{name} seed {seed}: {e}"));
        }
    }
    eprintln!("corpus models through the stage stepper: {t:?}");
    assert_eq!((t.refused, t.steps_err, t.class_diffs), (0, 0, 0), "{t:?}");
    assert!(t.steps_ok == 12 * 12 && t.commits > 0 && t.fixed > 0, "{t:?}");
}

// ---------------------------------------------------------------------------------------------
// Through the pipeline runner
// ---------------------------------------------------------------------------------------------

struct NoRandom;
impl RandomSource for NoRandom {
    fn random(&self, _: u16, _: RandomDist, _: u32, _: &[u32]) -> Option<Tensor> {
        None
    }
}

/// The reference: the program's params, no stepper.
struct Reference<'a>(&'a MapParams);
impl PipelineParams for Reference<'_> {
    fn params(&self, _: u16) -> &dyn ParamSource {
        self.0
    }
}

/// The executor: the same params, and a stepper over a plan and its bound params.
struct Executor<'a> {
    map: &'a MapParams,
    plan: &'a TirPlan,
    params: &'a TirParams<'a>,
}
impl PipelineParams for Executor<'_> {
    fn params(&self, _: u16) -> &dyn ParamSource {
        self.map
    }
    fn stepper(&self, _: u16, decl: &TirProgramV2) -> Option<Box<dyn StageStepperV1 + '_>> {
        TirStageStepperV1::for_stage(self.plan, self.params, decl).map(|s| Box::new(s) as Box<dyn StageStepperV1 + '_>)
    }
}

/// A lockstep member: the same params, its stage stepped by the hub through its seat (handed out once).
struct Seated {
    map: MapParams,
    seat: RefCell<Option<TirLockstepSeatV1>>,
}
impl PipelineParams for Seated {
    fn params(&self, _: u16) -> &dyn ParamSource {
        &self.map
    }
    fn stepper(&self, _: u16, _: &TirProgramV2) -> Option<Box<dyn StageStepperV1 + '_>> {
        self.seat.borrow_mut().take().map(|s| Box::new(s) as Box<dyn StageStepperV1 + '_>)
    }
}

/// A one-stage pipeline over the lifted program: the text stage (`TextStream`) when it reads the token,
/// else `Fixed { n }`.
fn one_stage(decl: &TirProgramV2, n: u32) -> TirPipelineV1 {
    let reads = decl.blocks.iter().any(|b| b.nodes.iter().any(|x| x.inputs.contains(&misaka_palw_tir::Ref::Input(0))));
    let (trip, max_trip) = if reads { (TripRule::TextStream, 64) } else { (TripRule::Fixed { n }, n) };
    TirPipelineV1 {
        version: misaka_palw_tir::pipeline::TIR_PIPELINE_VERSION_V1,
        stages: vec![StageDecl { name: "subject".into(), program: 0, trip, max_trip, tokens: None, bind: vec![] }],
        output_stage: 0,
    }
}

/// A deterministic answer from a logits row: the next id a decode might select, the third the last.
fn select_of(bound: u32) -> impl FnMut(u32, &Tensor) -> TextSelectV1 {
    let mut picked = 0;
    move |_, logits| {
        let id = (logits.data.iter().fold(0i128, |a, v| a.wrapping_mul(31).wrapping_add(*v)).rem_euclid(bound.min(64) as i128)) as u32;
        picked += 1;
        if picked >= 3 { TextSelectV1::Last(id) } else { TextSelectV1::Next(id) }
    }
}

type Outcome = TirResult<(PipelineRun, Vec<u32>)>;

fn same(a: &Outcome, b: &Outcome) -> Result<(), String> {
    match (a, b) {
        (Ok(x), Ok(y)) if x == y => Ok(()),
        (Ok(_), Ok(_)) => Err("the runs differ".into()),
        (Err(x), Err(y)) if x.kind == y.kind => Ok(()),
        (x, y) => Err(format!("reference {:?}, executor {:?}", x.as_ref().err(), y.as_ref().err())),
    }
}

/// `run_pipeline` (replaying `prompt`) and `run_text_pipeline` (generating from it) with the
/// reference's params and the executor's: the same runs, or the same refusals.
fn pipeline_differential(p: &TirProgramV1, params: &MapParams, prompt: &[u32]) -> Result<bool, String> {
    let decl = lifted(p);
    if InterpreterV2::new(&decl).is_err() {
        return Ok(false);
    }
    let plan = TirPlan::compile(p).map_err(|e| format!("compile: {e}"))?;
    let Ok(bound) = TirParams::from_map(&plan, params) else { return Ok(false) };
    let pipeline = one_stage(&decl, prompt.len() as u32);
    let programs = vec![decl];
    let job = PipelineJob { prompt: prompt.to_vec(), ..Default::default() };
    let exec = Executor { map: params, plan: &plan, params: &bound };
    let replay = |pp: &dyn PipelineParams| run_pipeline(&pipeline, &programs, pp, &NoRandom, &job).map(|r| (r, Vec::new()));
    same(&replay(&Reference(params)), &replay(&exec)).map_err(|e| format!("run_pipeline: {e}"))?;
    if matches!(pipeline.stages[0].trip, TripRule::TextStream) {
        let generate =
            |pp: &dyn PipelineParams| run_text_pipeline(&pipeline, &programs, pp, &NoRandom, &job, &mut select_of(p.token_bound));
        same(&generate(&Reference(params)), &generate(&exec)).map_err(|e| format!("run_text_pipeline: {e}"))?;
    }
    Ok(true)
}

#[test]
fn the_pipeline_runner_computes_the_same_runs_with_the_stepper_as_without() {
    let n = 600 * scale();
    let mut ran = 0;
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0x9195_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let len = rng.gen_range(1..=6);
        let prompt = tokens(&mut rng, &g.prog, len);
        ran += u64::from(pipeline_differential(&g.prog, &g.params, &prompt).unwrap_or_else(|e| panic!("seed {seed}: {e}")));
    }
    let (moe, gens) = moe_program();
    let params = materialize(&moe, &gens, 9);
    assert!(pipeline_differential(&moe, &params, &[3, 1, 4, 1, 5]).expect("the mixture"));
    eprintln!("{ran} of {n} random programs ran through both pipelines");
    assert!(ran * 2 > n, "{ran} of {n}");
}

// ---------------------------------------------------------------------------------------------
// Members in lockstep through one hub
// ---------------------------------------------------------------------------------------------

/// One member: its params and its prompt, generating or replaying.
struct Member {
    params: MapParams,
    prompt: Vec<u32>,
}

/// The members' pipelines on their own threads, their stage stepped by one hub on this thread.
fn through_the_hub(p: &TirProgramV1, members: &[Member], generate: bool) -> (Vec<Outcome>, misaka_palw_tir_exec::TirLockstepServedV1) {
    let decl = lifted(p);
    let plan = TirPlan::compile(p).expect("compiles");
    let bound: Vec<TirParams<'static>> = members.iter().map(|m| TirParams::from_map(&plan, &m.params).expect("binds")).collect();
    let steppers = bound.iter().map(|b| TirStageStepperV1::for_stage(&plan, b, &decl).expect("serves")).collect();
    let (hub, seats) = TirLockstepHubV1::new(steppers).expect("one schedule");
    let programs = vec![decl.clone()];
    std::thread::scope(|scope| {
        let handles: Vec<_> = members
            .iter()
            .zip(seats)
            .map(|(m, seat)| {
                let (programs, decl) = (&programs, &decl);
                scope.spawn(move || {
                    let pp = Seated { map: m.params.clone(), seat: RefCell::new(Some(seat)) };
                    let pipeline = one_stage(decl, m.prompt.len() as u32);
                    let job = PipelineJob { prompt: m.prompt.clone(), ..Default::default() };
                    if generate {
                        run_text_pipeline(&pipeline, programs, &pp, &NoRandom, &job, &mut select_of(p.token_bound))
                    } else {
                        run_pipeline(&pipeline, programs, &pp, &NoRandom, &job).map(|r| (r, Vec::new()))
                    }
                })
            })
            .collect();
        let served = hub.serve();
        (handles.into_iter().map(|h| h.join().expect("a member's thread")).collect(), served)
    })
}

/// The same member alone, on the reference.
fn alone(p: &TirProgramV1, m: &Member, generate: bool) -> Outcome {
    let decl = lifted(p);
    let pipeline = one_stage(&decl, m.prompt.len() as u32);
    let programs = vec![decl];
    let job = PipelineJob { prompt: m.prompt.clone(), ..Default::default() };
    if generate {
        run_text_pipeline(&pipeline, &programs, &Reference(&m.params), &NoRandom, &job, &mut select_of(p.token_bound))
    } else {
        run_pipeline(&pipeline, &programs, &Reference(&m.params), &NoRandom, &job).map(|r| (r, Vec::new()))
    }
}

/// **Members stepped by one hub run what each runs alone**: the corpus mixture's three "candidates"
/// (one program, three weights) generating from one prompt — their streams part where their logits
/// do, and each stops when its own selection says — and replaying prompts of different lengths, one of
/// which carries an id past the token bound (that member's run fails at that position, as alone, and
/// the others go on without waiting for it). Then random programs, three members each.
#[test]
fn members_stepped_by_one_hub_run_what_each_runs_alone() {
    let (moe, gens) = moe_program();
    let members: Vec<Member> =
        [21u64, 22, 23].iter().map(|s| Member { params: materialize(&moe, &gens, *s), prompt: vec![5, 1, 4, 2] }).collect();
    let (runs, served) = through_the_hub(&moe, &members, true);
    for (i, (run, m)) in runs.iter().zip(&members).enumerate() {
        same(&alone(&moe, m, true), run).unwrap_or_else(|e| panic!("member {i} generating: {e}"));
        assert!(run.is_ok(), "member {i}: {:?}", run.as_ref().err());
    }
    assert!(served.rounds >= 4 && served.member_steps >= 3 * 4, "{served:?}");
    // Replaying, three lengths, one past the token bound at its third position.
    let mut members = members;
    members[0].prompt = vec![1, 2, 3, 4, 5, 6, 7];
    members[1].prompt = vec![3, 3, moe.token_bound, 3];
    members[2].prompt = vec![9];
    let (runs, served) = through_the_hub(&moe, &members, false);
    for (i, (run, m)) in runs.iter().zip(&members).enumerate() {
        same(&alone(&moe, m, false), run).unwrap_or_else(|e| panic!("member {i} replaying: {e}"));
    }
    assert!(runs[1].as_ref().is_err_and(|e| e.kind == TirErrorKind::Operand), "{:?}", runs[1].as_ref().err());
    assert_eq!(served.rounds, 7, "the longest member's positions: {served:?}");
    assert_eq!(served.member_steps, 7 + 3 + 1, "every member's positions, the failed one's up to its failure: {served:?}");
    // Random programs: three weights each, generating when the program reads the token.
    let mut together = 0;
    for seed in 0..200 * scale() {
        let mut rng = R::seed_from_u64(0x4ab0_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let decl = lifted(&g.prog);
        if InterpreterV2::new(&decl).is_err() {
            continue;
        }
        let Ok(plan) = TirPlan::compile(&g.prog) else { continue };
        let members: Vec<Member> = (0..3)
            .map(|k| {
                let params = if k == 0 { g.params.clone() } else { gen_params(&mut rng, &g.prog) };
                let len = rng.gen_range(1..=5);
                Member { params, prompt: tokens(&mut rng, &g.prog, len) }
            })
            .collect();
        if members.iter().any(|m| TirParams::from_map(&plan, &m.params).is_err()) {
            continue;
        }
        let generate = one_stage(&decl, 1).stages[0].trip == TripRule::TextStream;
        let (runs, _) = through_the_hub(&g.prog, &members, generate);
        for (i, (run, m)) in runs.iter().zip(&members).enumerate() {
            same(&alone(&g.prog, m, generate), run).unwrap_or_else(|e| panic!("seed {seed} member {i}: {e}"));
        }
        together += 1;
    }
    eprintln!("{together} random programs stepped three members through one hub");
    assert!(together > 50, "{together}");
}

/// The `Fixed` states a stepper returns are exactly the instances the reference holds — none for a
/// program that writes none, every written one from the first position on.
#[test]
fn the_fixed_states_returned_are_the_instances_written() {
    let (den, gens) = gdn_program(true);
    let params = materialize(&den, &gens, 1);
    let decl = lifted(&den);
    let plan = TirPlan::compile(&den).unwrap();
    let bound = TirParams::from_map(&plan, &params).unwrap();
    let mut stepper = TirStageStepperV1::for_stage(&plan, &bound, &decl).unwrap();
    let interp = InterpreterV2::new(&decl).unwrap();
    let mut state = RunState::default();
    let written: Vec<StateKey> = den
        .occurrences()
        .into_iter()
        .flat_map(|(b, l)| {
            den.blocks[b as usize].nodes.iter().filter_map(move |n| match n.prim {
                misaka_palw_tir::Prim::StateWrite { state } => Some((state, l)),
                _ => None,
            })
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for tok in [1u32, 2, 3] {
        let (_, fixed): (_, BTreeMap<StateKey, Tensor>) = stepper.step(tok).unwrap();
        interp.step(&params, &MapInputs::default(), &mut state, tok).unwrap();
        assert_eq!(fixed.keys().copied().collect::<Vec<_>>(), written);
        assert_eq!(fixed, state.fixed);
    }
    assert!(!written.is_empty(), "the gated delta net writes its states");
    // A stage the executor does not compute: another program, or one with an input.
    let (moe, _) = moe_program();
    assert!(!TirStageStepperV1::serves(&plan, &lifted(&moe)));
    let with_input = TirProgramV2::from_v1_lifting_params(
        &den,
        &[(0, misaka_palw_tir::program_v2::InputSource::External { lo: -128, hi: 127 })],
        OutputDecl::Logits { node: den.logits, scheme_id: den.logits_scheme_id },
    )
    .unwrap();
    assert!(!TirStageStepperV1::serves(&plan, &with_input));
}
