//! The reference evaluator (RFC §4.2, spec 04b §8): one position step over Fixed and Hist
//! states, a multi-position run, and cone evaluation from supplied committed values — the court's
//! future entry point.
//!
//! One evaluator serves all three. A block occurrence is evaluated in node order; a node whose
//! value was SUPPLIED (a committed operand the court opened) is not recomputed but checked against
//! its declared type; a node nobody needs is not evaluated. A full step supplies nothing and needs
//! everything. State writes and history appends take effect only after the whole step succeeded,
//! so a failed step leaves the run state untouched.

use std::collections::{BTreeMap, VecDeque};

use crate::error::{TirError, TirErrorKind, TirResult, err};
use crate::eval::eval_prim;
use crate::prim::Prim;
use crate::program::{INPUT_POS, INPUT_TOKEN, Ref, StateKind, TirProgramV1};
use crate::tensor::Tensor;
use crate::types::{DType, TensorType};
use crate::validate::{ProgramInfo, validate};

/// Where params come from: the artifact, opened by name (and layer). The interpreter checks the
/// returned tensor against the declaration; a mismatch is an `Operand` error, never a panic.
pub trait ParamSource {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor>;
}

/// A `ParamSource` over an in-memory map (tests, tools).
#[derive(Clone, Debug, Default)]
pub struct MapParams {
    pub tensors: BTreeMap<(u16, Option<u16>), Tensor>,
}

impl ParamSource for MapParams {
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.tensors.get(&(index, layer)).cloned()
    }
}

/// One state instance: `(state index, layer)`; `layer` is `None` for a global state.
pub type StateKey = (u16, Option<u16>);

/// The run state carried between positions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunState {
    /// The next position to compute.
    pub pos: u32,
    /// `Fixed` state values; an absent instance is all zeros.
    pub fixed: BTreeMap<StateKey, Tensor>,
    /// `Hist` rows, oldest first, at most `window` of them.
    pub hist: BTreeMap<StateKey, VecDeque<Tensor>>,
}

/// One committed node value of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitRecord {
    /// The node's unrolled index within the position (RFC §6 `node_slot`).
    pub slot: u32,
    pub block: u8,
    pub layer: Option<u16>,
    pub node: u16,
    pub value: Tensor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepOutput {
    pub pos: u32,
    pub logits: Tensor,
    pub commits: Vec<CommitRecord>,
}

/// What a cone evaluation is given. Everything is optional; a node that needs a value nobody
/// supplied fails with [`TirErrorKind::Missing`].
#[derive(Clone, Debug, Default)]
pub struct ConeEnv {
    pub token: Option<u32>,
    pub pos: u32,
    pub carry_in: BTreeMap<u8, Tensor>,
    /// `Fixed` state values at the start of the position, by state index.
    pub fixed: BTreeMap<u16, Tensor>,
    /// For each `Hist` state: the rows BEFORE this position inside its window, oldest first —
    /// exactly `min(pos, window − 1)` rows.
    pub hist_prior: BTreeMap<u16, Vec<Tensor>>,
    /// Committed values of this block occurrence's nodes, by node index.
    pub supplied: BTreeMap<u16, Tensor>,
}

/// The interpreter for one validated program.
pub struct Interpreter<'p> {
    pub program: &'p TirProgramV1,
    pub info: ProgramInfo,
}

fn resolve_h(window: Option<u32>, pos: u32) -> usize {
    window.map(|w| (pos as usize + 1).min(w as usize)).unwrap_or(1)
}

fn check_value(t: &Tensor, want: &TensorType, h: usize, what: &str) -> TirResult<()> {
    if t.dtype != want.dtype || t.shape != want.resolve(h) {
        return err(
            TirErrorKind::Operand,
            format!("{what}: got {} {:?}, declared {} {:?}", t.dtype.name(), t.shape, want.dtype.name(), want.resolve(h)),
        );
    }
    if t.data.len() != t.shape.iter().product::<usize>() || t.data.iter().any(|v| !t.dtype.contains(*v)) {
        return err(TirErrorKind::Operand, format!("{what}: a value outside {}", t.dtype.name()));
    }
    Ok(())
}

/// The environment one block occurrence reads, whichever mode it runs in.
struct OccurrenceEnv<'a> {
    token: Option<u32>,
    pos: u32,
    layer: Option<u16>,
    carry_in: &'a BTreeMap<u8, Tensor>,
    fixed: &'a dyn Fn(u16) -> Option<Tensor>,
    hist_prior: &'a dyn Fn(u16) -> Option<Vec<Tensor>>,
    supplied: &'a BTreeMap<u16, Tensor>,
    /// A RUN STATE may omit an instance never written: absent is the initial value (all zeros, no
    /// rows) — spec 04b §9.1. A cone's environment may not: its state values and history rows are
    /// opened from commitments, and one that is absent is `Missing` (§9.2, ref2 F1/F3).
    absent_is_initial: bool,
}

impl<'p> Interpreter<'p> {
    pub fn new(program: &'p TirProgramV1) -> TirResult<Self> {
        let info = validate(program)?;
        Ok(Self { program, info })
    }

    fn param(&self, params: &dyn ParamSource, j: u16, layer: Option<u16>) -> TirResult<Tensor> {
        let d = &self.program.params[j as usize];
        let layer = if d.per_layer { layer } else { None };
        let t = params
            .param(j, layer)
            .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("param {} (layer {layer:?})", d.name)))?;
        let want = TensorType::fixed(d.dtype, &d.shape);
        check_value(&t, &want, 1, &format!("param {}", d.name))?;
        Ok(t)
    }

    fn constant(&self, j: u16) -> TirResult<Tensor> {
        let c = &self.program.consts[j as usize];
        let shape: Vec<usize> = c.shape.iter().map(|d| *d as usize).collect();
        Tensor::from_le_bytes(c.dtype, &shape, &c.data)
    }

    /// Evaluate the nodes of one block occurrence that `needed` marks, in node order.
    fn eval_occurrence(
        &self,
        block: u8,
        env: &OccurrenceEnv<'_>,
        params: &dyn ParamSource,
        needed: &[bool],
    ) -> TirResult<Vec<Option<Tensor>>> {
        let b = &self.program.blocks[block as usize];
        let window = self.info.blocks[block as usize].window;
        let h = resolve_h(window, env.pos);
        let mut values: Vec<Option<Tensor>> = vec![None; b.nodes.len()];
        for (ni, node) in b.nodes.iter().enumerate() {
            if !needed[ni] {
                continue;
            }
            if let Some(v) = env.supplied.get(&(ni as u16)) {
                check_value(v, &node.out, h, &format!("supplied node {ni}"))?;
                values[ni] = Some(v.clone());
                continue;
            }
            let mut owned: Vec<Tensor> = Vec::with_capacity(node.inputs.len());
            for r in &node.inputs {
                let t = match *r {
                    Ref::Node(j) => values[j as usize]
                        .clone()
                        .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("node {j} (needed by node {ni})")))?,
                    Ref::CarryIn(k) => {
                        let t = env
                            .carry_in
                            .get(&k)
                            .cloned()
                            .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("carry-in {k}")))?;
                        check_value(&t, &b.carry_in[k as usize], h, &format!("carry-in {k}"))?;
                        t
                    }
                    Ref::Param(j) => self.param(params, j, env.layer)?,
                    Ref::Const(j) => self.constant(j)?,
                    Ref::State(j) => {
                        let s = &self.program.states[j as usize];
                        let t = match (env.fixed)(j) {
                            Some(t) => t,
                            None if env.absent_is_initial => {
                                Tensor::zeros(s.dtype, &s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>())
                            }
                            None => {
                                return err(
                                    TirErrorKind::Missing,
                                    format!("state {} (a cone's Fixed value is supplied, never implied)", s.name),
                                );
                            }
                        };
                        check_value(&t, &TensorType::fixed(s.dtype, &s.shape), h, &format!("state {}", s.name))?;
                        if let StateKind::Fixed { lo, hi } = s.kind
                            && t.data.iter().any(|v| *v < lo as i128 || *v > hi as i128)
                        {
                            return err(TirErrorKind::Operand, format!("state {}: a value outside [{lo}, {hi}]", s.name));
                        }
                        t
                    }
                    Ref::Input(j) => {
                        let v = if j == INPUT_TOKEN {
                            let t = env.token.ok_or_else(|| TirError::new(TirErrorKind::Missing, "token"))?;
                            if t >= self.program.token_bound {
                                return err(TirErrorKind::Operand, format!("token {t} ≥ token_bound {}", self.program.token_bound));
                            }
                            t
                        } else {
                            debug_assert_eq!(j, INPUT_POS);
                            env.pos
                        };
                        Tensor::scalar(DType::Idx, v as i128)?
                    }
                };
                owned.push(t);
            }
            let out_shape = node.out.resolve(h);
            let value = if let Prim::HistAppend { state } = node.prim {
                let s = &self.program.states[state as usize];
                let StateKind::Hist { window: w } = s.kind else { return err(TirErrorKind::Shape, "HistAppend on a Fixed state") };
                let prior = match (env.hist_prior)(state) {
                    Some(rows) => rows,
                    None if env.absent_is_initial => Vec::new(),
                    None => {
                        return err(
                            TirErrorKind::Missing,
                            format!("history {} (a cone's prior rows are supplied, even when none)", s.name),
                        );
                    }
                };
                let want_rows = (env.pos as usize).min(w as usize - 1);
                if prior.len() != want_rows {
                    return err(TirErrorKind::Position, format!("history {}: {} prior rows, want {want_rows}", s.name, prior.len()));
                }
                let row_t = TensorType::fixed(s.dtype, &s.shape);
                let mut data = Vec::with_capacity((want_rows + 1) * owned[0].len());
                for r in &prior {
                    check_value(r, &row_t, h, &format!("history {} row", s.name))?;
                    data.extend_from_slice(&r.data);
                }
                data.extend_from_slice(&owned[0].data);
                Tensor { dtype: s.dtype, shape: out_shape, data }
            } else {
                let refs: Vec<&Tensor> = owned.iter().collect();
                eval_prim(&node.prim, &refs, node.out.dtype, &out_shape, &self.program.states)?
            };
            values[ni] = Some(value);
        }
        Ok(values)
    }

    /// Whether any of `nodes` reads `Input(0)`, the token.
    fn reads_token<'n>(&self, mut nodes: impl Iterator<Item = &'n crate::program::Node>) -> bool {
        nodes.any(|n| n.inputs.iter().any(|r| matches!(r, Ref::Input(INPUT_TOKEN))))
    }

    /// One position: `token` at `state.pos`. On success the state advances by one position.
    pub fn step(&self, params: &dyn ParamSource, state: &mut RunState, token: u32) -> TirResult<StepOutput> {
        let p = self.program;
        let pos = state.pos;
        if pos >= p.history_bound {
            return err(TirErrorKind::Position, format!("position {pos} ≥ history_bound {}", p.history_bound));
        }
        // The token is NEEDED when some node of the program reads it (§9.1(1)); it is checked
        // before any node runs, so the class of a bad token never depends on evaluation order.
        if self.reads_token(p.blocks.iter().flat_map(|b| b.nodes.iter())) && token >= p.token_bound {
            return err(TirErrorKind::Operand, format!("token {token} ≥ token_bound {}", p.token_bound));
        }
        let bases = p.occurrence_slot_bases();
        let mut carry: BTreeMap<u8, Tensor> = BTreeMap::new();
        let mut commits = Vec::new();
        let mut writes: Vec<(StateKey, Tensor)> = Vec::new();
        let mut appends: Vec<(StateKey, u32, Tensor)> = Vec::new();
        let mut logits = None;
        let empty = BTreeMap::new();
        for (occ, (block, layer)) in p.occurrences().into_iter().enumerate() {
            let b = &p.blocks[block as usize];
            let fixed = |j: u16| state.fixed.get(&(j, layer)).cloned();
            let hist = |j: u16| state.hist.get(&(j, layer)).map(|rows| rows.iter().cloned().collect::<Vec<_>>());
            let env = OccurrenceEnv {
                token: Some(token),
                pos,
                layer,
                carry_in: &carry,
                fixed: &fixed,
                hist_prior: &hist,
                supplied: &empty,
                absent_is_initial: true,
            };
            let values = self.eval_occurrence(block, &env, params, &vec![true; b.nodes.len()])?;
            for (ni, node) in b.nodes.iter().enumerate() {
                let v = values[ni].as_ref().expect("a full step evaluates every node");
                if node.commit {
                    commits.push(CommitRecord { slot: bases[occ] + ni as u32, block, layer, node: ni as u16, value: v.clone() });
                }
                match node.prim {
                    Prim::StateWrite { state: j } => writes.push(((j, layer), v.clone())),
                    Prim::HistAppend { state: j } => {
                        let StateKind::Hist { window } = p.states[j as usize].kind else { unreachable!("validated") };
                        let row = match node.inputs[0] {
                            Ref::Node(i) => values[i as usize].clone().expect("evaluated"),
                            Ref::CarryIn(k) => carry[&k].clone(),
                            _ => unreachable!("validated: the appended row is a node or a carry-in"),
                        };
                        appends.push(((j, layer), window, row));
                    }
                    _ => {}
                }
            }
            let is_post = occ == p.schedule.layers.len() + 1;
            if is_post {
                logits = values[p.logits as usize].clone();
            } else {
                carry =
                    b.carry_out.iter().enumerate().map(|(k, n)| (k as u8, values[*n as usize].clone().expect("evaluated"))).collect();
            }
        }
        for (key, v) in writes {
            state.fixed.insert(key, v);
        }
        for (key, window, row) in appends {
            let rows = state.hist.entry(key).or_default();
            rows.push_back(row);
            // Keep only what the next position's window can still see: `window − 1` prior rows.
            while rows.len() > (window as usize).saturating_sub(1) {
                rows.pop_front();
            }
        }
        state.pos += 1;
        Ok(StepOutput { pos, logits: logits.expect("the post block ran"), commits })
    }

    /// Positions `0..tokens.len()` from a fresh state.
    pub fn run(&self, params: &dyn ParamSource, tokens: &[u32]) -> TirResult<Vec<StepOutput>> {
        let mut state = RunState::default();
        tokens.iter().map(|t| self.step(params, &mut state, *t)).collect()
    }

    /// Evaluate node `target` of block `block` at `layer` from committed values — the court's
    /// entry point. Only the target's cone is evaluated: the backward closure of `target` that
    /// stops at every supplied node.
    pub fn eval_cone(&self, block: u8, layer: Option<u16>, target: u16, params: &dyn ParamSource, env: &ConeEnv) -> TirResult<Tensor> {
        let p = self.program;
        // The request names an occurrence of the schedule and a node of its block, or it is about
        // some other program (class `Malformed`, spec 04b §9.2).
        let b = p.blocks.get(block as usize).ok_or_else(|| TirError::new(TirErrorKind::Malformed, "no such block"))?;
        if target as usize >= b.nodes.len() {
            return err(TirErrorKind::Malformed, "no such node");
        }
        let info = &self.info.blocks[block as usize];
        match layer {
            Some(l) if !info.is_layer || l as usize >= p.schedule.layers.len() || p.schedule.layers[l as usize] != block => {
                return err(TirErrorKind::Malformed, "the block does not run at that layer");
            }
            None if info.is_layer => return err(TirErrorKind::Malformed, "a layer block needs its layer"),
            _ => {}
        }
        // A malformed environment is refused, never read around (§9.2, ref2 F2/F4): the target is
        // always RECOMPUTED — a court that supplied the disputed value would otherwise "recompute"
        // exactly the claim under dispute — and an entry that names no node of the block is a
        // request about some other program.
        if env.supplied.contains_key(&target) {
            return err(TirErrorKind::Malformed, format!("the environment supplies the target node {target} itself"));
        }
        if let Some(k) = env.supplied.keys().find(|k| **k as usize >= b.nodes.len()) {
            return err(TirErrorKind::Malformed, format!("the environment supplies node {k}, which the block does not have"));
        }
        // §9.1(1) applies to a cone.
        if env.pos >= p.history_bound {
            return err(TirErrorKind::Position, "position ≥ history_bound");
        }
        let mut needed = vec![false; b.nodes.len()];
        let mut stack = vec![target as usize];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut needed[i], true) || (i != target as usize && env.supplied.contains_key(&(i as u16))) {
                continue;
            }
            for r in &b.nodes[i].inputs {
                if let Ref::Node(j) = r {
                    stack.push(*j as usize);
                }
            }
        }
        // The token is needed when a node of the CLOSURE reads it (a supplied node's inputs are
        // not read): present, and inside `token_bound`, before any node runs.
        let closure = b
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| needed[*i] && (*i == target as usize || !env.supplied.contains_key(&(*i as u16))));
        if self.reads_token(closure.map(|(_, n)| n)) {
            match env.token {
                None => return err(TirErrorKind::Missing, "token"),
                Some(t) if t >= p.token_bound => {
                    return err(TirErrorKind::Operand, format!("token {t} ≥ token_bound {}", p.token_bound));
                }
                Some(_) => {}
            }
        }
        let fixed = |j: u16| env.fixed.get(&j).cloned();
        let hist = |j: u16| env.hist_prior.get(&j).cloned();
        let occ = OccurrenceEnv {
            token: env.token,
            pos: env.pos,
            layer,
            carry_in: &env.carry_in,
            fixed: &fixed,
            hist_prior: &hist,
            supplied: &env.supplied,
            absent_is_initial: false,
        };
        let mut values = self.eval_occurrence(block, &occ, params, &needed)?;
        values[target as usize].take().ok_or_else(|| TirError::new(TirErrorKind::Missing, "target not evaluated"))
    }
}
