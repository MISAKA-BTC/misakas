//! **Demand evaluation of version-2 programs** (spec 04b §15.4) — the court's evaluator for a stage
//! of a pipeline.
//!
//! [`eval_demanded_v2`] is [`crate::demand::eval_demanded`], unchanged, over the program's view,
//! with two things the view cannot say supplied by version 2:
//!
//! * **inputs.** Input `k` is the view's param `|params| + k`, read at a position. The source answers
//!   a sixth question, [`DemandSourceV2::input`]`(pos, k, index)`: a random input's element is
//!   computed by the court from the claim's job (RFC-0003's `R`), never opened and never taken from a
//!   challenger; an external input's element is an earlier stage's committed element, or a job value
//!   (the pipeline's binding says which, §15.6).
//! * **`post` writers.** A state `post` writes is written by occurrence `L + 1`'s committed write
//!   (NF-29): its value at the start of `p` is that leaf at `p − 1`, never a replay of `post`.
//!
//! Everything else — the leaves, the index maps, the work and its limits, the refusals — is §9.4's.
//!
//! **An input's answer is held to its declared dtype AND interval** (PALW-TIR-42), by the evaluator,
//! at the point a param's is held to its dtype: a value outside the interval fails the evaluation
//! `Operand`, by name. A source answers inputs within their declared intervals by construction (the
//! court derives random ones, an upstream committed value was checked against its proven interval —
//! PALW-TIR-33 — and that interval lies inside the input's, NF-P7, and a job value was checked at
//! acceptance); the evaluator does not rely on it, because the bounds admission proved over the
//! cones — a value that overflows, a work estimate — are bounds over the declared intervals.

use crate::demand::{
    DemandContext, DemandInputsV1, DemandLimits, DemandRangeRequest, DemandRequest, DemandResult, DemandSource, DemandWork,
    StateSupply, eval_demanded_ext, eval_demanded_range_ext,
};
use crate::error::{TirError, TirErrorKind, TirResult};
use crate::program_v2::TirProgramV2;
use crate::validate_v2::ProgramInfoV2;

/// Where the values a version-2 evaluation does not compute come from: §9.4's five questions and
/// the input question. Every answer may be a refusal; the evaluation then fails `Missing`.
pub trait DemandSourceV2 {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128>;
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128>;
    /// Element `index` of input `input` at position `pos` (constant over positions unless the input
    /// is a per-step random one).
    fn input(&mut self, pos: u32, input: u16, index: usize) -> TirResult<i128>;
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply>;
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128>;
    fn token(&mut self, pos: u32) -> TirResult<u32>;
}

/// The view's source: declared params from the version-2 source, inputs through its input question.
struct ViewSource<'a> {
    inner: &'a mut dyn DemandSourceV2,
    first_input: u16,
}

impl DemandSource for ViewSource<'_> {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        self.inner.node(ctx, node, index)
    }
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        // The engine reads params through `param_at`. An input asked without its position is
        // ambiguous (a per-step input differs by position), so it is refused, never guessed.
        if param < self.first_input {
            self.inner.param(param, layer, index)
        } else {
            Err(TirError::new(TirErrorKind::Missing, "an input is read at a position"))
        }
    }
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply> {
        self.inner.state(pos, state, layer, index)
    }
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        self.inner.hist_row(pos, state, layer, row_pos, index)
    }
    fn token(&mut self, pos: u32) -> TirResult<u32> {
        self.inner.token(pos)
    }
    fn param_at(&mut self, pos: u32, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        if param < self.first_input {
            self.inner.param(param, layer, index)
        } else {
            self.inner.input(pos, param - self.first_input, index)
        }
    }
}

/// The declared interval of every input of `p`, in declaration order (`InputDecl::interval`).
pub fn input_intervals_v2(p: &TirProgramV2) -> Vec<(i128, i128)> {
    p.inputs.iter().map(|d| d.interval()).collect()
}

/// The `post` writers of a validated version-2 program: `(state, (occurrence L + 1, node))`.
pub fn post_writers_v2(p: &TirProgramV2, info: &ProgramInfoV2) -> Vec<(u16, (u16, u16))> {
    let post_occurrence = (p.schedule.layers.len() + 1) as u16;
    info.post_writes.iter().map(|(node, state)| (*state, (post_occurrence, *node))).collect()
}

/// **Evaluate the demanded elements of a version-2 program's target** (spec 04b §9.4 over the view,
/// §15.4). `info` must be `validate_v2(program)`'s.
pub fn eval_demanded_v2(
    program: &TirProgramV2,
    info: &ProgramInfoV2,
    request: &DemandRequest<'_>,
    source: &mut dyn DemandSourceV2,
    limits: &DemandLimits,
) -> DemandResult<(Vec<i128>, DemandWork)> {
    let writers = post_writers_v2(program, info);
    let intervals = input_intervals_v2(program);
    let inputs = DemandInputsV1 { first_input: info.first_input_param, intervals: &intervals };
    let mut view_source = ViewSource { inner: source, first_input: info.first_input_param };
    eval_demanded_ext(&info.view, &info.v1, request, &mut view_source, limits, &writers, inputs)
}

/// **A range evaluation of a version-2 program** (spec 04b §9.5 over the view): the history
/// dissection's arithmetic for a stage that appends to a history (a causal encoder).
pub fn eval_demanded_range_v2(
    program: &TirProgramV2,
    info: &ProgramInfoV2,
    request: &DemandRangeRequest<'_>,
    source: &mut dyn DemandSourceV2,
    limits: &DemandLimits,
) -> DemandResult<(Vec<i128>, DemandWork)> {
    let writers = post_writers_v2(program, info);
    let intervals = input_intervals_v2(program);
    let inputs = DemandInputsV1 { first_input: info.first_input_param, intervals: &intervals };
    let mut view_source = ViewSource { inner: source, first_input: info.first_input_param };
    eval_demanded_range_ext(&info.view, &info.v1, request, &mut view_source, limits, &writers, inputs)
}

/// A [`DemandSourceV2`] over in-memory values — for tools and the differential tests: version 1's
/// [`crate::demand::MapSource`] for the five questions, and inputs by `(input, position)`. Every input
/// request is recorded.
#[derive(Clone, Debug, Default)]
pub struct MapSourceV2 {
    pub base: crate::demand::MapSource,
    /// Inputs by `(input, pos)`; `(input, None)` answers at every position without its own entry.
    pub inputs: std::collections::BTreeMap<(u16, Option<u32>), Vec<i128>>,
    /// Inputs the source refuses, with the reason it gives (never read: every refusal is `Missing`).
    pub withheld_inputs: std::collections::BTreeMap<u16, TirErrorKind>,
    /// Every input request served, in order: `(pos, input, index)`.
    pub input_requests: Vec<(u32, u16, usize)>,
}

impl DemandSourceV2 for MapSourceV2 {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        self.base.node(ctx, node, index)
    }
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        self.base.param(param, layer, index)
    }
    fn input(&mut self, pos: u32, input: u16, index: usize) -> TirResult<i128> {
        self.input_requests.push((pos, input, index));
        if let Some(kind) = self.withheld_inputs.get(&input) {
            return Err(TirError::new(*kind, format!("input {input} is withheld")));
        }
        let v = self.inputs.get(&(input, Some(pos))).or_else(|| self.inputs.get(&(input, None)));
        v.and_then(|v| v.get(index))
            .copied()
            .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("input {input} at {pos} element {index}")))
    }
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply> {
        self.base.state(pos, state, layer, index)
    }
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        self.base.hist_row(pos, state, layer, row_pos, index)
    }
    fn token(&mut self, pos: u32) -> TirResult<u32> {
        self.base.token(pos)
    }
}

/// The occurrence index (§3.3) a step's commit record ran in.
pub fn commit_occurrence_v2(p: &TirProgramV2, block: u8, layer: Option<u16>) -> u16 {
    match layer {
        Some(l) => l + 1,
        None if block == p.schedule.pre => 0,
        None => (p.schedule.layers.len() + 1) as u16,
    }
}

impl MapSourceV2 {
    /// A source over a run's committed values: every commit point of every position, the params,
    /// the inputs as the run read them, the tokens, every history row (the committed node a row
    /// is), and the `Fixed` values at the start of position 0 (the initial zeros); every other
    /// position's state answers `Replay`.
    pub fn from_run(
        p: &TirProgramV2,
        info: &ProgramInfoV2,
        params: &crate::interp::MapParams,
        inputs: &dyn crate::interp_v2::InputProvider,
        tokens: &[u32],
        steps: &[crate::interp_v2::StepOutputV2],
    ) -> Self {
        use crate::program::StateKind;
        let mut s = MapSourceV2::default();
        for (pos, step) in steps.iter().enumerate() {
            s.base.tokens.insert(pos as u32, tokens[pos]);
            for c in &step.commits {
                let ctx = DemandContext { pos: pos as u32, occurrence: commit_occurrence_v2(p, c.block, c.layer) };
                s.base.nodes.insert((ctx, c.node), c.value.data.clone());
            }
            for k in 0..p.inputs.len() as u16 {
                if let Some(t) = inputs.input(k, pos as u32) {
                    s.inputs.insert((k, Some(pos as u32)), t.data);
                }
            }
        }
        for ((j, l), t) in &params.tensors {
            s.base.params.insert((*j, *l), t.data.clone());
        }
        for (j, st) in p.states.iter().enumerate() {
            let layers: Vec<Option<u16>> =
                if st.per_layer { (0..p.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
            for layer in layers {
                match st.kind {
                    StateKind::Fixed { .. } => {
                        let n: usize = st.shape.iter().map(|d| *d as usize).product();
                        s.base.states.insert((0, j as u16, layer), vec![0; n]);
                    }
                    StateKind::Hist { .. } => {
                        for row_pos in 0..steps.len() as u32 {
                            if let Some((ctx, node)) = crate::demand::hist_row_node_v1(&info.view, j as u16, layer, row_pos)
                                && let Some(v) = s.base.nodes.get(&(ctx, node))
                            {
                                let v = v.clone();
                                s.base.hist_rows.insert((j as u16, layer, row_pos), v);
                            }
                        }
                    }
                }
            }
        }
        s
    }
}
