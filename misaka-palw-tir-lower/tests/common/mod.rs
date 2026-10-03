//! Checks shared by the integration tests of the lowered programs: **three-way exactness** (the
//! reference evaluator, the independent second implementation and the typed backend agree on every
//! logit and every commit point) and **court coverage** (every node of every occurrence, evaluated
//! by the court's demand evaluator from only the committed leaves, equals the honest run).
#![allow(dead_code)]

use misaka_palw_tir::demand::{
    DemandContext, DemandLimits, DemandRangeRequest, DemandRequest, DemandTarget, MapSource, eval_demanded, eval_demanded_range, history_length_v1,
    reduces_over_h_v1, state_instance_is_held_v1,
};
use misaka_palw_tir::Prim;
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::{ConeEnv, Interpreter, MapParams, RunState, Tensor, TirProgramV1};
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::lower::{IntParams, IntTensor};
use std::collections::BTreeMap;

/// The lowering's integer tensor of an evaluator tensor (a stage's output becoming the next stage's input).
pub fn int_tensor(t: &Tensor) -> IntTensor {
    use misaka_palw_tir::DType as D;
    let shape = t.shape.clone();
    match t.dtype {
        D::I8 => IntTensor::i8(shape, t.data.iter().map(|v| *v as i8).collect()),
        D::I16 => IntTensor::i16(shape, t.data.iter().map(|v| *v as i16).collect()),
        D::I32 => IntTensor::i32(shape, t.data.iter().map(|v| *v as i32).collect()),
        D::I64 => IntTensor::i64(shape, t.data.iter().map(|v| *v as i64).collect()),
        D::Idx => IntTensor::idx(shape, t.data.iter().map(|v| *v as u32).collect()),
        D::I128 => panic!("an i128 tensor is not a param"),
    }
}

/// A lowered stage's params with REAL values for its input params. The lowering declares a stage's inputs as params
/// (`encoder::lifted_params` drops them for the version-2 program); the second implementation, the typed backend and
/// the court's demand evaluator all see that version-1 view, with the inputs as leaves.
pub fn with_inputs(program: &TirProgramV1, params: &IntParams, inputs: &[(&str, IntTensor)]) -> IntParams {
    let mut p = params.clone();
    for (name, t) in inputs {
        let j = program.params.iter().position(|d| d.name == *name).unwrap_or_else(|| panic!("the program has no param `{name}`"));
        p.tensors.insert((j as u16, None), t.clone());
    }
    p
}

/// One commit: `(slot, block, layer, node, values)`.
type Commit = (u64, u8, Option<u32>, u16, Vec<i128>);

struct Collect(Vec<Commit>);
impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.push((v.slot as u64, v.block, v.layer.map(u32::from), v.node, v.data.to_i128s()));
        }
    }
}

fn ref2_dtype(d: misaka_palw_tir::DType) -> misaka_palw_tir_ref2::DType {
    use misaka_palw_tir::DType as A;
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

/// Run `p` with `params` over `eval` on the reference evaluator, ref2 and the typed backend; every
/// position's logits and commits must be equal. Returns the positions run.
pub fn three_ways(p: &misaka_palw_tir::TirProgramV1, params: &IntParams, eval: &[Vec<usize>]) -> Result<usize, String> {
    let IntParams { tensors } = params;

    // ref2: its own decoding of the canonical bytes, its own tensors.
    let bytes = p.encode();
    let p2 = misaka_palw_tir_ref2::codec::decode_canonical(&bytes).map_err(|e| format!("ref2 refuses the program: {e:?}"))?;
    let mut params2 = misaka_palw_tir_ref2::eval::Params::new();
    for ((j, layer), t) in tensors {
        let d = &p2.params[*j as usize];
        let shape = d.shape.iter().map(|x| *x as u64).collect();
        let t2 =
            misaka_palw_tir_ref2::Tensor::from_le_bytes(d.dtype, shape, &t.le_bytes()).map_err(|e| format!("ref2 tensor: {e:?}"))?;
        assert_eq!(d.dtype, ref2_dtype(p.params[*j as usize].dtype));
        params2.insert((*j, layer.map(u32::from)), t2);
    }
    // exec: the plan and borrowed little-endian params.
    let owned: Vec<((u16, Option<u16>), Vec<u8>)> = tensors.iter().map(|(k, t)| (*k, t.le_bytes())).collect();
    let plan = TirPlan::compile(p).map_err(|e| format!("exec plan: {e}"))?;
    let mut xparams = TirParams::new(&plan);
    for ((j, layer), b) in &owned {
        let data = ParamData::from_le_bytes(p.params[*j as usize].dtype, b).map_err(|e| e.to_string())?;
        xparams.insert(&plan, *j, *layer, data).map_err(|e| e.to_string())?;
    }

    let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
    let mut positions = 0;
    for seq in eval {
        let mut st1 = misaka_palw_tir::RunState::default();
        let mut st2 = misaka_palw_tir_ref2::eval::initial_state(&p2);
        let mut exec = TirExecutor::new(&plan, &xparams).map_err(|e| e.to_string())?;
        for (pos, tok) in seq.iter().enumerate() {
            let o1 = interp.step(params, &mut st1, *tok as u32).map_err(|e| format!("reference at {pos}: {e}"))?;
            let (o2, next) =
                misaka_palw_tir_ref2::eval::step(&p2, &params2, &st2, *tok as u64).map_err(|e| format!("ref2 at {pos}: {e:?}"))?;
            st2 = next;
            let mut sink = Collect(Vec::new());
            exec.step(*tok as u32, &mut sink).map_err(|e| format!("exec at {pos}: {e}"))?;
            let (_, xl) = exec.logits();
            let l1 = &o1.logits.data;
            if *l1 != o2.logits.data || *l1 != xl.to_i128s() {
                return Err(format!("position {pos}: the logits differ"));
            }
            let c1: Vec<Commit> =
                o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            let c2: Vec<Commit> = o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
            let mut c3 = sink.0;
            c3.sort_by_key(|c| c.0);
            if c1 != c2 {
                return Err(format!("position {pos}: reference and ref2 commit differently ({} vs {} commits)", c1.len(), c2.len()));
            }
            if c1 != c3 {
                return Err(format!("position {pos}: reference and exec commit differently ({} vs {} commits)", c1.len(), c3.len()));
            }
            positions += 1;
        }
    }
    Ok(positions)
}


// ───────────────────────────── court coverage ─────────────────────────────

/// What a court-coverage run touched.
#[derive(Debug, Default)]
pub struct CourtReport {
    /// Nodes (occurrence, position) whose demanded elements equal the honest run's.
    pub nodes: usize,
    pub elements: u64,
    /// How many nodes of each primitive the court evaluator evaluated.
    pub primitives: BTreeMap<&'static str, usize>,
    /// Committed nodes whose committed value the demand evaluator reproduced from the leaves alone.
    pub commits: usize,
    /// Reductions over `H` the court dissects by ranges of positions: the partials over two halves
    /// of the history combine (sum, or max) to the whole, exactly.
    pub dissected: usize,
}

fn occurrence_of(program: &TirProgramV1, block: u8, layer: Option<u16>) -> u16 {
    match layer {
        Some(l) => l + 1,
        None if block == program.schedule.pre => 0,
        None => program.schedule.layers.len() as u16 + 1,
    }
}

fn instances(program: &TirProgramV1, hist: bool) -> Vec<(u16, Option<u16>)> {
    let mut out = Vec::new();
    for (j, s) in program.states.iter().enumerate() {
        if matches!(s.kind, StateKind::Hist { .. }) != hist {
            continue;
        }
        if s.per_layer {
            // A per-layer state is held at the layers whose block reads, writes or appends to it.
            out.extend(
                (0..program.schedule.layers.len() as u16)
                    .filter(|l| state_instance_is_held_v1(program, j as u16, Some(*l)))
                    .map(|l| (j as u16, Some(l))),
            );
        } else {
            out.push((j as u16, None));
        }
    }
    out
}

struct Run {
    before: Vec<RunState>,
    after: Vec<RunState>,
    commits: BTreeMap<(DemandContext, u16), Tensor>,
    tokens: Vec<u32>,
}

fn honest_run(program: &TirProgramV1, params: &MapParams, tokens: &[u32]) -> Result<Run, String> {
    let interp = Interpreter::new(program).map_err(|e| e.to_string())?;
    let mut state = RunState::default();
    let (mut before, mut after, mut commits) = (Vec::new(), Vec::new(), BTreeMap::new());
    for token in tokens {
        before.push(state.clone());
        let step = interp.step(params, &mut state, *token).map_err(|e| format!("an honest step: {e}"))?;
        for c in step.commits {
            let ctx = DemandContext { pos: step.pos, occurrence: occurrence_of(program, c.block, c.layer) };
            commits.insert((ctx, c.node), c.value);
        }
        after.push(state.clone());
    }
    Ok(Run { before, after, commits, tokens: tokens.to_vec() })
}

fn fixed_value(program: &TirProgramV1, rs: &RunState, j: u16, layer: Option<u16>) -> Vec<i128> {
    let s = &program.states[j as usize];
    rs.fixed.get(&(j, layer)).map(|t| t.data.clone()).unwrap_or_else(|| vec![0; s.shape.iter().map(|d| *d as usize).product()])
}

fn source(program: &TirProgramV1, params: &MapParams, r: &Run, supply: &dyn Fn(u32) -> bool) -> MapSource {
    let mut s = MapSource {
        tokens: r.tokens.iter().enumerate().map(|(p, t)| (p as u32, *t)).collect(),
        nodes: r.commits.iter().map(|(k, t)| (*k, t.data.clone())).collect(),
        params: params.tensors.iter().map(|(k, t)| (*k, t.data.clone())).collect(),
        ..Default::default()
    };
    for (p, rs) in r.before.iter().enumerate() {
        if supply(p as u32) {
            for (j, l) in instances(program, false) {
                s.states.insert((p as u32, j, l), fixed_value(program, rs, j, l));
            }
        }
    }
    for (p, rs) in r.after.iter().enumerate() {
        for (j, l) in instances(program, true) {
            if let Some(rows) = rs.hist.get(&(j, l))
                && let Some(last) = rows.back()
            {
                s.hist_rows.insert((j, l, p as u32), last.data.clone());
            }
        }
    }
    s
}

fn cone_env(program: &TirProgramV1, r: &Run, pos: u32, block: u8, layer: Option<u16>, target: u16) -> ConeEnv {
    let occ = occurrence_of(program, block, layer);
    let ctx = DemandContext { pos, occurrence: occ };
    let supplied: BTreeMap<u16, Tensor> =
        r.commits.iter().filter(|((c, n), _)| *c == ctx && *n != target).map(|((_, n), t)| (*n, t.clone())).collect();
    let mut carry_in: BTreeMap<u8, Tensor> = BTreeMap::new();
    if occ > 0 {
        let (pb, _) = program.occurrences()[occ as usize - 1];
        let prev = DemandContext { pos, occurrence: occ - 1 };
        for (k, n) in program.blocks[pb as usize].carry_out.iter().enumerate() {
            carry_in.insert(k as u8, r.commits[&(prev, *n)].clone());
        }
    }
    let before = &r.before[pos as usize];
    let is_layer = layer.is_some();
    let (mut fixed, mut hist_prior) = (BTreeMap::new(), BTreeMap::new());
    for (j, s) in program.states.iter().enumerate() {
        if s.per_layer != is_layer {
            continue;
        }
        let key = (j as u16, if s.per_layer { layer } else { None });
        let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
        match s.kind {
            StateKind::Fixed { .. } => {
                fixed.insert(j as u16, before.fixed.get(&key).cloned().unwrap_or_else(|| Tensor::zeros(s.dtype, &shape)));
            }
            StateKind::Hist { .. } => {
                hist_prior.insert(j as u16, before.hist.get(&key).map(|rows| rows.iter().cloned().collect()).unwrap_or_default());
            }
        }
    }
    ConeEnv { token: Some(r.tokens[pos as usize]), pos, carry_in, fixed, hist_prior, supplied }
}

/// **Court coverage**: for every node of every occurrence at `positions`, the court's demand
/// evaluator ([`eval_demanded`]) — which sees only the committed leaves, the params, the tokens, the
/// history rows and the `Fixed` states a checkpoint supplies (every `C`-th position for each `C` in
/// `checkpoints`, replayed in between) — returns the honest run's elements: the whole node, and its
/// first and last element alone. A committed node's committed value is among them. No node is
/// skipped, so every primitive the program uses is evaluated by the court.
pub fn court_coverage(program: &TirProgramV1, params: &IntParams, tokens: &[u32], positions: &[u32], checkpoints: &[u32]) -> Result<CourtReport, String> {
    let mp = MapParams { tensors: params.tensors.iter().map(|(k, t)| (*k, t.to_tir())).collect() };
    let interp = Interpreter::new(program).map_err(|e| e.to_string())?;
    let run = honest_run(program, &mp, tokens)?;
    let mut report = CourtReport::default();
    for &c in checkpoints {
        // The value at the start of position p ≡ 0 (mod C) is supplied; the others are replayed.
        let mut src = source(program, &mp, &run, &|p| p % c == 0);
        for &pos in positions {
            for (block, layer) in program.occurrences() {
                let ctx = DemandContext { pos, occurrence: occurrence_of(program, block, layer) };
                for target in 0..program.blocks[block as usize].nodes.len() as u16 {
                    let env = cone_env(program, &run, pos, block, layer, target);
                    let full = interp.eval_cone(block, layer, target, &mp, &env).map_err(|e| format!("pos {pos} block {block} node {target}: eval_cone {e}"))?;
                    if let Some(committed) = run.commits.get(&(ctx, target)) {
                        if &full != committed {
                            return Err(format!("pos {pos} block {block} node {target}: the cone does not reproduce the committed value"));
                        }
                        report.commits += 1;
                    }
                    let n = full.data.len();
                    let sets: Vec<Vec<usize>> = if n > 2 { vec![(0..n).collect(), vec![0], vec![n - 1]] } else { vec![(0..n).collect()] };
                    for elements in sets {
                        let request = DemandRequest { target: DemandTarget::Node { ctx, node: target }, elements: &elements };
                        let (values, _) = eval_demanded(program, &interp.info, &request, &mut src, &DemandLimits::UNLIMITED)
                            .map_err(|e| format!("C {c} pos {pos} block {block} node {target} ({}): eval_demanded {e}", program.blocks[block as usize].nodes[target as usize].prim.name()))?;
                        let expect: Vec<i128> = elements.iter().map(|e| full.data[*e]).collect();
                        if values != expect {
                            return Err(format!(
                                "C {c} pos {pos} block {block} layer {layer:?} node {target} ({}): the court's elements differ",
                                program.blocks[block as usize].nodes[target as usize].prim.name()
                            ));
                        }
                        report.elements += elements.len() as u64;
                    }
                    // The dissection court's arithmetic: a reduction over `H` evaluated over two halves of the
                    // history (its `range` form) combines to the whole.
                    let blk = &program.blocks[block as usize];
                    if reduces_over_h_v1(blk, target)
                        && let Some(h) = history_length_v1(&interp.info, block, pos)
                        && h >= 2
                    {
                        let mid = h / 2;
                        for e in [0usize, n - 1] {
                            let mut part = |range: (usize, usize)| {
                                let req = DemandRangeRequest { ctx, target, elements: std::slice::from_ref(&e), supplied: &[], range: Some(range) };
                                eval_demanded_range(program, &interp.info, &req, &mut src, &DemandLimits::UNLIMITED)
                                    .map(|(v, _)| v[0])
                                    .map_err(|er| format!("C {c} pos {pos} block {block} node {target}: range {range:?}: {er}"))
                            };
                            let (lo, hi) = (part((0, mid))?, part((mid, h))?);
                            let whole = match blk.nodes[target as usize].prim {
                                Prim::ReduceMax { .. } => lo.max(hi),
                                _ => lo + hi,
                            };
                            if whole != full.data[e] {
                                return Err(format!("C {c} pos {pos} block {block} node {target}: the partials over {mid} + {} positions do not combine to the whole", h - mid));
                            }
                        }
                        report.dissected += 1;
                    }
                    report.nodes += 1;
                    *report.primitives.entry(program.blocks[block as usize].nodes[target as usize].prim.name()).or_default() += 1;
                }
            }
        }
        // The `Fixed` states after the last position are the run's, replayed from the checkpoints.
        for q in 0..tokens.len() as u32 {
            for (j, l) in instances(program, false) {
                let want = fixed_value(program, &run.after[q as usize], j, l);
                let elements: Vec<usize> = (0..want.len()).collect();
                let request = DemandRequest { target: DemandTarget::StateAfter { pos: q, state: j, layer: l }, elements: &elements };
                let (values, _) = eval_demanded(program, &interp.info, &request, &mut src, &DemandLimits::UNLIMITED)
                    .map_err(|e| format!("C {c} state {j}@{l:?} after {q}: {e}"))?;
                if values != want {
                    return Err(format!("C {c} state {j}@{l:?} after position {q}: the replayed state differs"));
                }
            }
        }
    }
    Ok(report)
}
