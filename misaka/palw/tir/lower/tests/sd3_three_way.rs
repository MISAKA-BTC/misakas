//! **RFC-0003 CHECKPOINT 4: the pipeline three-way identity** — the reduced SD3 pipeline (13 stages: two CLIP text stages, the MMDiT
//! denoiser over the job's steps with a `post`-written latent, the VAE decoder as ten stages) run on
//!
//! * the reference evaluator (`misaka-palw-tir`: `InterpreterV2`, the version-2 stage interpreter the pipeline runner and the court use),
//! * the independent second implementation (`misaka-palw-tir-ref2`, written from the spec alone, fed the canonical bytes of each stage's
//!   version-1 view — it decodes them with its own codec and runs them with its own tensors), and
//! * the typed backend that ships on nodes (`misaka-palw-tir-exec`, over the same view),
//!
//! each composing the pipeline ITSELF: stage by stage, an edge read from the backend's OWN earlier stage's output (`StageRows` with its
//! drop and its zero pad, `StageFinal`), a job-bound input from the job's facts, a random input drawn per step, a `post`-written state
//! (NF-29) carried from one position to the next. At EVERY position of EVERY stage the output and every commit point (slot, block,
//! layer, node, value) must be equal across all three, byte for byte — and the reference's composition is held to the pipeline
//! runner's own (`run_pipeline`), so the harness is validated against what the court and the worker run.
//!
//! The version-2 features the two backends do not know — inputs, `post` writes — are what the view makes of them: an input is a param
//! past the declared ones (`first_input_param`), carrying its value at the position; a `post` `StateWrite` is a committed `Clamp` into
//! the state's range, whose value the harness writes into the state before the next position (`InterpreterV2::step` does exactly that).
//!
//! The fixture is the lowerer's (`diffusion::fixture::build_sd3_tiny`); a checkout without it skips with a line saying so.

use misaka_palw_tir::interp::{MapParams, RunState as RefState};
use misaka_palw_tir::interp_v2::{InputProvider, InterpreterV2};
use misaka_palw_tir::pipeline::{Binding, PipelineJob, RandomSource, StageJobFacts, TirPipelineV1, run_pipeline, stage_job_facts};
use misaka_palw_tir::program_v2::{InputSource, RandomDist, TirProgramV2};
use misaka_palw_tir::validate_v2::validate_v2;
use misaka_palw_tir::{DType, Tensor};
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::diffusion::fixture::*;
use std::collections::BTreeMap;

/// The noise `R` answers: the same words at every call (the denoiser draws once, at its first position).
struct Noise(Vec<i128>);

impl RandomSource for Noise {
    fn random(&self, _domain: u16, dist: RandomDist, _step: u32, shape: &[u32]) -> Option<Tensor> {
        assert_eq!(dist, RandomDist::Normal);
        Tensor::new(DType::I32, shape.iter().map(|d| *d as usize).collect(), self.0.clone()).ok()
    }
}

/// One committed node of a position: `(slot, block, layer, node, values)`.
type Commit = (u64, u8, Option<u32>, u16, Vec<i128>);

/// One position of one stage, as a backend ran it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pos {
    output: Vec<i128>,
    commits: Vec<Commit>,
}

/// How a stage reads input `k` at position `pos`: a value the composition fixed, or a draw per step.
enum Input {
    Fixed(Tensor),
    PerStep { domain: u16, dist: RandomDist, shape: Vec<u32> },
}

struct Provider<'a> {
    inputs: &'a BTreeMap<u16, Input>,
    random: &'a dyn RandomSource,
}

impl InputProvider for Provider<'_> {
    fn input(&self, k: u16, pos: u32) -> Option<Tensor> {
        match self.inputs.get(&k)? {
            Input::Fixed(t) => Some(t.clone()),
            Input::PerStep { domain, dist, shape } => self.random.random(*domain, *dist, pos, shape),
        }
    }
}

/// A backend: one stage, position by position, from a fresh state.
trait Backend {
    fn run_stage(&mut self, prog: &TirProgramV2, params: &MapParams, inputs: &Provider<'_>, tokens: &[u32]) -> Result<Vec<Pos>, String>;
}

fn commits_of(c: impl Iterator<Item = Commit>) -> Vec<Commit> {
    let mut v: Vec<Commit> = c.collect();
    v.sort_by_key(|c| c.0);
    v
}

// ---- the reference evaluator ----------------------------------------------------------------------------------------------------

struct Reference;

impl Backend for Reference {
    fn run_stage(&mut self, prog: &TirProgramV2, params: &MapParams, inputs: &Provider<'_>, tokens: &[u32]) -> Result<Vec<Pos>, String> {
        let interp = InterpreterV2::new(prog).map_err(|e| e.to_string())?;
        let mut state = RefState::default();
        let mut out = Vec::new();
        for (p, t) in tokens.iter().enumerate() {
            let o = interp.step(params, inputs, &mut state, *t).map_err(|e| format!("reference at {p}: {e}"))?;
            out.push(Pos {
                output: o.output.data.clone(),
                commits: commits_of(o.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone()))),
            });
        }
        Ok(out)
    }
}

// ---- the typed backend ----------------------------------------------------------------------------------------------------------

struct Collect(Vec<Commit>);
impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.push((v.slot as u64, v.block, v.layer.map(u32::from), v.node, v.data.to_i128s()));
        }
    }
}

struct Exec;

impl Backend for Exec {
    fn run_stage(&mut self, prog: &TirProgramV2, params: &MapParams, inputs: &Provider<'_>, tokens: &[u32]) -> Result<Vec<Pos>, String> {
        let info = validate_v2(prog).map_err(|e| e.to_string())?;
        let plan = TirPlan::compile(&info.view).map_err(|e| format!("exec plan: {e}"))?;
        let owned: Vec<((u16, Option<u16>), DType, Vec<u8>)> =
            params.tensors.iter().map(|(k, t)| (*k, t.dtype, t.to_le_bytes())).collect();
        let post = prog.schedule.post;
        let mut state = RefState::default();
        let mut out = Vec::new();
        for (p, t) in tokens.iter().enumerate() {
            // The view's params for this position: the declared ones, then every input at its value here.
            let values: Vec<(u16, Tensor)> = (0..prog.inputs.len() as u16)
                .map(|k| inputs.input(k, p as u32).map(|v| (k, v)).ok_or_else(|| format!("input {k} at {p}")))
                .collect::<Result<_, _>>()?;
            let input_bytes: Vec<(u16, DType, Vec<u8>)> = values.iter().map(|(k, v)| (*k, v.dtype, v.to_le_bytes())).collect();
            let mut xparams = TirParams::new(&plan);
            for ((j, layer), dtype, b) in &owned {
                xparams.insert(&plan, *j, *layer, ParamData::from_le_bytes(*dtype, b).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            }
            for (k, dtype, b) in &input_bytes {
                xparams
                    .insert(&plan, info.first_input_param + k, None, ParamData::from_le_bytes(*dtype, b).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            }
            let mut exec = TirExecutor::new(&plan, &xparams).map_err(|e| e.to_string())?;
            exec.import_state(&state).map_err(|e| format!("exec state at {p}: {e}"))?;
            let mut sink = Collect(Vec::new());
            exec.step(*t, &mut sink).map_err(|e| format!("exec at {p}: {e}"))?;
            let (_, logits) = exec.logits();
            let output = logits.to_i128s();
            state = exec.export_state();
            let commits = commits_of(sink.0.iter().cloned());
            // A state `post` writes holds, at the next position, the committed write (NF-29).
            for (node, s) in &info.post_writes {
                let written = commits
                    .iter()
                    .find(|c| c.1 == post && c.2.is_none() && c.3 == *node)
                    .ok_or_else(|| format!("exec: the post write {node} is not a commit"))?;
                let decl = &prog.states[*s as usize];
                let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
                state.fixed.insert((*s, None), Tensor::new(decl.dtype, shape, written.4.clone()).map_err(|e| e.to_string())?);
            }
            out.push(Pos { output, commits });
        }
        Ok(out)
    }
}

// ---- the independent second implementation --------------------------------------------------------------------------------------

fn ref2_dtype(d: DType) -> misaka_palw_tir_ref2::DType {
    use DType as A;
    use misaka_palw_tir_ref2::DType as B;
    match d {
        A::I8 => B::I8,
        A::I16 => B::I16,
        A::I32 => B::I32,
        A::I64 => B::I64,
        A::I128 => B::I128,
        A::Idx => B::Idx,
    }
}

struct Ref2;

impl Backend for Ref2 {
    fn run_stage(&mut self, prog: &TirProgramV2, params: &MapParams, inputs: &Provider<'_>, tokens: &[u32]) -> Result<Vec<Pos>, String> {
        let info = validate_v2(prog).map_err(|e| e.to_string())?;
        // Its own decoding of the canonical bytes of the view.
        let p2 = misaka_palw_tir_ref2::codec::decode_canonical(&info.view.encode()).map_err(|e| format!("ref2 refuses the view: {e:?}"))?;
        let to2 = |t: &Tensor| {
            misaka_palw_tir_ref2::Tensor::from_le_bytes(
                ref2_dtype(t.dtype),
                t.shape.iter().map(|x| *x as u64).collect(),
                &t.to_le_bytes(),
            )
            .map_err(|e| format!("ref2 tensor: {e:?}"))
        };
        let mut declared = misaka_palw_tir_ref2::eval::Params::new();
        for ((j, layer), t) in &params.tensors {
            declared.insert((*j, layer.map(u32::from)), to2(t)?);
        }
        let post = prog.schedule.post;
        let mut state = misaka_palw_tir_ref2::eval::initial_state(&p2);
        let mut out = Vec::new();
        for (p, t) in tokens.iter().enumerate() {
            let mut params2 = declared.clone();
            for k in 0..prog.inputs.len() as u16 {
                let v = inputs.input(k, p as u32).ok_or_else(|| format!("input {k} at {p}"))?;
                params2.insert((info.first_input_param + k, None), to2(&v)?);
            }
            let (o, next) =
                misaka_palw_tir_ref2::eval::step(&p2, &params2, &state, *t as u64).map_err(|e| format!("ref2 at {p}: {e:?}"))?;
            state = next;
            let commits = commits_of(o.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())));
            for (node, s) in &info.post_writes {
                let written = o
                    .commits
                    .iter()
                    .find(|c| c.block == post && c.layer.is_none() && c.node == *node)
                    .ok_or_else(|| format!("ref2: the post write {node} is not a commit"))?;
                state.fixed.insert((*s, None), written.value.clone());
            }
            out.push(Pos { output: o.logits.data.clone(), commits });
        }
        Ok(out)
    }
}

// ---- the composition (one implementation, run by each backend over its OWN earlier stages) --------------------------------------

/// A stage's inputs from its bindings, the job's facts and the backend's own earlier stages' outputs.
fn stage_inputs(
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
    s: usize,
    facts: &StageJobFacts,
    earlier: &[Vec<Tensor>],
    random: &dyn RandomSource,
) -> Result<BTreeMap<u16, Input>, String> {
    let st = &pipeline.stages[s];
    let prog = &programs[st.program as usize];
    let mut out = BTreeMap::new();
    let mut bindings = st.bind.iter();
    for (k, d) in prog.inputs.iter().enumerate() {
        let k = k as u16;
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        match d.source {
            InputSource::Random { domain, dist, per_step: true } => {
                out.insert(k, Input::PerStep { domain, dist, shape: d.shape.clone() });
            }
            // Drawn once, at position 0, and held (the denoiser's noise).
            InputSource::Random { domain, dist, per_step: false } => {
                let v = random.random(domain, dist, 0, &d.shape).ok_or("R answers no draw")?;
                out.insert(k, Input::Fixed(v));
            }
            InputSource::External { .. } => {
                let b = bindings.next().ok_or("a binding per external input")?;
                let value = match b {
                    Binding::StageRows { stage, drop, pad_to } => {
                        let rows = &earlier[*stage as usize];
                        let kept = rows.get(*drop as usize..).unwrap_or(&[]);
                        let per_row: usize = shape[1..].iter().product();
                        let mut data: Vec<i128> = kept.iter().flat_map(|r| r.data.iter().copied()).collect();
                        data.resize(*pad_to as usize * per_row, 0);
                        Tensor::new(d.dtype, shape, data).map_err(|e| e.to_string())?
                    }
                    Binding::StageFinal { stage } => earlier[*stage as usize].last().cloned().ok_or("an earlier stage ran no position")?,
                    // A scalar, a token run, a count: the job's own facts.
                    _ => facts.inputs.get(&k).cloned().ok_or_else(|| format!("stage {s}: the job fixes no input {k}"))?,
                };
                out.insert(k, Input::Fixed(value));
            }
        }
    }
    Ok(out)
}

/// Every position of every stage, as `backend` ran the pipeline.
fn run_pipeline_on(
    backend: &mut dyn Backend,
    pipe: &misaka_palw_tir_lower::diffusion::pipeline::Sd3Pipeline,
    job: &PipelineJob,
    random: &dyn RandomSource,
) -> Result<Vec<Vec<Pos>>, String> {
    let facts = stage_job_facts(&pipe.pipeline, &pipe.programs, job).map_err(|e| e.to_string())?;
    let mut earlier: Vec<Vec<Tensor>> = Vec::new();
    let mut all = Vec::new();
    for (s, st) in pipe.pipeline.stages.iter().enumerate() {
        let prog = &pipe.programs[st.program as usize];
        let inputs = stage_inputs(&pipe.pipeline, &pipe.programs, s, &facts[s], &earlier, random)?;
        let tokens = if facts[s].tokens.is_empty() { vec![0; facts[s].trip as usize] } else { facts[s].tokens.clone() };
        let provider = Provider { inputs: &inputs, random };
        let rows = backend.run_stage(prog, &pipe.params[st.program as usize], &provider, &tokens).map_err(|e| format!("stage {s} ({}): {e}", st.name))?;
        // The stage's output as a tensor: the output node's declared type.
        let node = &prog.blocks[prog.schedule.post as usize].nodes[prog.output.node() as usize];
        let shape: Vec<usize> = node.out.resolve(1);
        let dtype = node.out.dtype;
        earlier.push(
            rows.iter().map(|r| Tensor::new(dtype, shape.clone(), r.output.clone()).map_err(|e| e.to_string())).collect::<Result<_, _>>()?,
        );
        all.push(rows);
    }
    Ok(all)
}

#[test]
fn the_sd3_pipeline_is_bit_identical_on_the_reference_the_typed_backend_and_ref2() {
    let Some(dir) = fixture() else { return };
    let fx = build_sd3_tiny(&dir).expect("the fixture lowers");
    let (c, side) = (fx.cfg.in_channels, fx.cfg.sample_size);
    let pipe = &fx.pipeline;
    let (mut positions, mut commits_total) = (0usize, 0usize);
    // The fidelity test's four evaluation jobs: prompts of one to four ids, two and four steps (the widest).
    for (prompt, seed, steps) in [(vec![5u32, 9, 13], 1u64, 4u32), (vec![2, 7], 2, 2), (vec![20, 21, 22, 23], 3, 4), (vec![11], 4, 2)] {
        let si = COUNTS.iter().position(|c| *c == steps).unwrap();
        let words = noise_words(seed, c * side * side);
        let random = Noise(words.clone());
        let job = PipelineJob { prompt: prompt.clone(), steps, scalars: vec![si as i64], ..Default::default() };

        // The pipeline runner's own run: the harness's reference composition is held to it.
        let run = run_pipeline(&pipe.pipeline, &pipe.programs, pipe, &random, &job).expect("the pipeline runs");
        let reference = run_pipeline_on(&mut Reference, pipe, &job, &random).expect("the reference composition runs");
        for (s, st) in run.stages.iter().enumerate() {
            assert_eq!(reference[s].len(), st.steps.len(), "stage {s}: positions");
            for (p, step) in st.steps.iter().enumerate() {
                assert_eq!(reference[s][p].output, step.output.data, "stage {s} position {p}: the harness's reference is the runner's");
                let want = commits_of(step.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())));
                assert_eq!(reference[s][p].commits, want, "stage {s} position {p}: the harness's commits are the runner's");
            }
        }
        // The image the integer pipeline produced: the last VAE stage's output.
        assert_eq!(reference.last().unwrap().last().unwrap().output, run.output.data, "the output");

        // The typed backend and ref2, each composing the pipeline from its own outputs.
        let exec = run_pipeline_on(&mut Exec, pipe, &job, &random).expect("the typed backend runs the pipeline");
        let second = run_pipeline_on(&mut Ref2, pipe, &job, &random).expect("ref2 runs the pipeline");
        for (s, st) in pipe.pipeline.stages.iter().enumerate() {
            for p in 0..reference[s].len() {
                assert_eq!(reference[s][p].output, exec[s][p].output, "{} position {p}: reference vs the typed backend, output", st.name);
                assert_eq!(reference[s][p].output, second[s][p].output, "{} position {p}: reference vs ref2, output", st.name);
                assert_eq!(reference[s][p].commits, exec[s][p].commits, "{} position {p}: reference vs the typed backend, commits", st.name);
                assert_eq!(reference[s][p].commits, second[s][p].commits, "{} position {p}: reference vs ref2, commits", st.name);
                positions += 1;
                commits_total += reference[s][p].commits.len();
            }
        }
        eprintln!(
            "job (prompt {prompt:?}, {steps} steps): {} stages, {} positions, every output and commit equal on the reference, the typed backend and ref2",
            pipe.pipeline.stages.len(),
            reference.iter().map(Vec::len).sum::<usize>()
        );
    }
    eprintln!("{positions} stage positions and {commits_total} committed nodes bit-identical on all three");
    assert!(positions > 0);
}
