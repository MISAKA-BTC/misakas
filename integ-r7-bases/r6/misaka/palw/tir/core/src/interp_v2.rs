//! **The reference evaluator for version-2 programs** (spec 04b §15.4).
//!
//! It runs the unchanged version-1 [`Interpreter`] over the program's version-1 view and adds three
//! things:
//!
//! * **inputs.** The caller supplies every input tensor through an [`InputProvider`] — external
//!   values from the job or an upstream stage, and random ones as RFC-0003's `R` computes them (this
//!   crate never hashes). Each is checked against its declaration before it is read: a missing one
//!   is `Missing`, one of the wrong dtype or shape, or with an element outside the input's interval,
//!   is `Operand`.
//! * **`post` writes.** In a `Rows`/`Final` program a `post` `StateWrite` is the view's committed
//!   `Clamp`; its value becomes the state's value for the next position — only after the whole step
//!   succeeded, like every other effect (PALW-TIR-28).
//! * **the output.** A step's output is the output node's value; a run of a `Rows` program yields
//!   one row per position, a `Final` program's result is the last position's.
//!
//! A step's commit points are the version-2 program's: the view's extra commit flags on the `post`
//! writes are not reported.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{TirError, TirErrorKind, TirResult, err};
use crate::interp::{CommitRecord, ConeEnv, Interpreter, ParamSource, RunState};
use crate::program::Ref;
use crate::program_v2::*;
use crate::tensor::Tensor;
use crate::types::TensorType;
use crate::validate_v2::{ProgramInfoV2, validate_v2};

/// Where input tensors come from: `input(k, pos)` is input `k`'s value at position `pos`. An
/// external or step-independent random input has the same value at every position; a per-step
/// random input is keyed by the position.
pub trait InputProvider {
    fn input(&self, k: u16, pos: u32) -> Option<Tensor>;
}

/// An [`InputProvider`] over maps (tests, tools): `constant[k]` at every position, unless
/// `at[(k, pos)]` says otherwise.
#[derive(Clone, Debug, Default)]
pub struct MapInputs {
    pub constant: BTreeMap<u16, Tensor>,
    pub at: BTreeMap<(u16, u32), Tensor>,
}

impl InputProvider for MapInputs {
    fn input(&self, k: u16, pos: u32) -> Option<Tensor> {
        self.at.get(&(k, pos)).or_else(|| self.constant.get(&k)).cloned()
    }
}

/// One position of a version-2 program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepOutputV2 {
    pub pos: u32,
    /// The output node's value at this position.
    pub output: Tensor,
    /// Every commit point of the version-2 program, in slot order.
    pub commits: Vec<CommitRecord>,
}

/// The view's params: the declared ones from the caller's source, then the inputs.
struct ViewParams<'a> {
    base: &'a dyn ParamSource,
    first_input: u16,
    inputs: &'a BTreeMap<u16, Tensor>,
}

impl ParamSource for ViewParams<'_> {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        if index < self.first_input { self.base.param(index, layer) } else { self.inputs.get(&(index - self.first_input)).cloned() }
    }
}

/// The interpreter for one validated version-2 program.
pub struct InterpreterV2<'p> {
    pub program: &'p TirProgramV2,
    pub info: ProgramInfoV2,
}

impl<'p> InterpreterV2<'p> {
    pub fn new(program: &'p TirProgramV2) -> TirResult<Self> {
        let info = validate_v2(program)?;
        Ok(Self { program, info })
    }

    fn v1(&self) -> Interpreter<'_> {
        Interpreter { program: &self.info.view, info: self.info.v1.clone() }
    }

    /// Fetch input `k` at `pos` and hold it to its declaration.
    fn fetch(&self, inputs: &dyn InputProvider, k: u16, pos: u32) -> TirResult<Tensor> {
        let d = &self.program.inputs[k as usize];
        let t = inputs.input(k, pos).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("input {}", d.name)))?;
        let want = TensorType::fixed(d.dtype, &d.shape);
        if t.dtype != d.dtype || t.shape != want.resolve(1) || t.data.len() != t.shape.iter().product::<usize>() {
            return err(
                TirErrorKind::Operand,
                format!("input {}: got {} {:?}, declared {} {:?}", d.name, t.dtype.name(), t.shape, d.dtype.name(), d.shape),
            );
        }
        let (lo, hi) = d.interval();
        if let Some(v) = t.data.iter().find(|v| **v < lo || **v > hi) {
            return err(TirErrorKind::Operand, format!("input {}: {v} is outside its interval [{lo}, {hi}]", d.name));
        }
        Ok(t)
    }

    /// One position: `token` at `state.pos`. On success the state advances by one position, and the
    /// `post` writes of a `Rows`/`Final` program become the written states' values.
    pub fn step(
        &self,
        params: &dyn ParamSource,
        inputs: &dyn InputProvider,
        state: &mut RunState,
        token: u32,
    ) -> TirResult<StepOutputV2> {
        let p = self.program;
        let pos = state.pos;
        if pos >= p.history_bound {
            return err(TirErrorKind::Position, format!("position {pos} ≥ history_bound {}", p.history_bound));
        }
        // A full step evaluates every node, and every input is read by some node (NF-27): all are needed.
        let mut values = BTreeMap::new();
        for k in 0..p.inputs.len() as u16 {
            values.insert(k, self.fetch(inputs, k, pos)?);
        }
        let view_params = ViewParams { base: params, first_input: self.info.first_input_param, inputs: &values };
        let out = self.v1().step(&view_params, state, token)?;
        let post = p.schedule.post;
        let post_occurrence = |c: &CommitRecord| c.block == post && c.layer.is_none();
        let mut writes = Vec::with_capacity(self.info.post_writes.len());
        for (node, s) in &self.info.post_writes {
            // The view commits every post write, so its value is always among the step's commits;
            // a miss would be an interpreter defect, reported rather than panicked on.
            let Some(written) = out.commits.iter().find(|c| post_occurrence(c) && c.node == *node) else {
                return err(TirErrorKind::Missing, format!("post write {node} was not evaluated (an interpreter defect)"));
            };
            writes.push(((*s, None), written.value.clone()));
        }
        for (key, v) in writes {
            state.fixed.insert(key, v);
        }
        let post_nodes = &p.blocks[post as usize].nodes;
        let commits = out.commits.into_iter().filter(|c| !post_occurrence(c) || post_nodes[c.node as usize].commit).collect();
        Ok(StepOutputV2 { pos, output: out.logits, commits })
    }

    /// Positions `0..tokens.len()` from a fresh state. A program that reads no token takes any
    /// token list of the run's length (the token is checked only where some node reads it).
    pub fn run(&self, params: &dyn ParamSource, inputs: &dyn InputProvider, tokens: &[u32]) -> TirResult<Vec<StepOutputV2>> {
        let mut state = RunState::default();
        tokens.iter().map(|t| self.step(params, inputs, &mut state, *t)).collect()
    }

    /// `T` positions of a program that reads no token.
    pub fn run_positions(&self, params: &dyn ParamSource, inputs: &dyn InputProvider, t: u32) -> TirResult<Vec<StepOutputV2>> {
        self.run(params, inputs, &vec![0; t as usize])
    }

    /// Evaluate node `target` of block `block` at `layer` from committed values (spec 04b §9.2 over
    /// the view). Only the inputs the target's closure reads are fetched and checked; a `post`
    /// `StateWrite` target evaluates to the value it writes.
    pub fn eval_cone(
        &self,
        block: u8,
        layer: Option<u16>,
        target: u16,
        params: &dyn ParamSource,
        inputs: &dyn InputProvider,
        env: &ConeEnv,
    ) -> TirResult<Tensor> {
        let p = self.program;
        let mut values = BTreeMap::new();
        // A request about no node is the version-1 evaluator's to refuse (`Malformed`); only a
        // well-formed one has a closure to read inputs for.
        if let Some(b) = p.blocks.get(block as usize)
            && (target as usize) < b.nodes.len()
            && !env.supplied.contains_key(&target)
        {
            let mut needed = BTreeSet::new();
            let mut seen = vec![false; b.nodes.len()];
            let mut stack = vec![target as usize];
            while let Some(i) = stack.pop() {
                if std::mem::replace(&mut seen[i], true) || (i != target as usize && env.supplied.contains_key(&(i as u16))) {
                    continue;
                }
                for r in &b.nodes[i].inputs {
                    match *r {
                        Ref::Node(j) if (j as usize) < b.nodes.len() => stack.push(j as usize),
                        Ref::Input(j) if j >= FIRST_INPUT_REF_V2 => {
                            needed.insert((j - FIRST_INPUT_REF_V2) as u16);
                        }
                        _ => {}
                    }
                }
            }
            for k in needed {
                values.insert(k, self.fetch(inputs, k, env.pos)?);
            }
        }
        let view_params = ViewParams { base: params, first_input: self.info.first_input_param, inputs: &values };
        self.v1().eval_cone(block, layer, target, &view_params, env)
    }
}
