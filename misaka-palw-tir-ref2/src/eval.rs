//! The reference evaluation of a step, of a run (04b §9.1) and of a cone (§9.2).
//!
//! A step is a pure function from a run state to a new run state: effects are collected while the
//! occurrences are evaluated and applied to a copy only when every node succeeded (PALW-TIR-28).

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{Class, Res, err};
use crate::normal_form::block_window;
use crate::prims::eval_prim;
use crate::program::{Prim, Program, Ref, StateKind};
use crate::tensor::Tensor;
use crate::types::{DType, TensorType};

/// Param instances by `(param index, layer)`; the layer is `None` for a global param.
pub type Params = BTreeMap<(u16, Option<u32>), Tensor>;

/// The run state of §9.1: position, `Fixed` values and `Hist` rows by `(state, layer)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunState {
    pub pos: u64,
    pub fixed: BTreeMap<(u16, Option<u32>), Tensor>,
    /// The rows visible to the next position, oldest first (at most `window − 1`).
    pub hist: BTreeMap<(u16, Option<u32>), Vec<Tensor>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub slot: u64,
    pub block: u8,
    pub layer: Option<u32>,
    pub node: u16,
    pub value: Tensor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepOutput {
    pub logits: Tensor,
    pub commits: Vec<Commit>,
}

/// The occurrences of a step (§3.3): `(pre, None)`, `(layers[l], l)`, `(post, None)`.
pub fn occurrences(p: &Program) -> Vec<(u8, Option<u32>)> {
    let mut v = vec![(p.schedule.pre, None)];
    for (l, &b) in p.schedule.layers.iter().enumerate() {
        v.push((b, Some(l as u32)));
    }
    v.push((p.schedule.post, None));
    v
}

/// The instance key of a param or state referenced at an occurrence's layer.
fn instance(per_layer: bool, layer: Option<u32>) -> Option<u32> {
    if per_layer { layer } else { None }
}

/// The states a block uses (read, written or appended).
fn states_used(p: &Program, b: usize) -> BTreeSet<u16> {
    let mut s = BTreeSet::new();
    for n in &p.blocks[b].nodes {
        for r in &n.inputs {
            if let Ref::State(j) = *r {
                s.insert(j);
            }
        }
        if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
            s.insert(state);
        }
    }
    s
}

/// §9.1: `pos = 0`, every `Fixed` instance all zeros, every `Hist` instance empty.
pub fn initial_state(p: &Program) -> RunState {
    let mut st = RunState { pos: 0, fixed: BTreeMap::new(), hist: BTreeMap::new() };
    for (b, layer) in occurrences(p) {
        for j in states_used(p, b as usize) {
            let s = &p.states[j as usize];
            let key = (j, instance(s.per_layer, layer));
            match s.kind {
                StateKind::Fixed { .. } => {
                    let shape = s.shape.iter().map(|&d| d as u64).collect();
                    st.fixed.entry(key).or_insert_with(|| Tensor::zeros(s.dtype, shape));
                }
                StateKind::Hist { .. } => {
                    st.hist.entry(key).or_default();
                }
            }
        }
    }
    st
}

fn check_tensor(t: &Tensor, ty: &TensorType, h: u64, what: &str) -> Res<()> {
    if t.dtype != ty.dtype || t.shape != ty.extents(h) {
        return err(Class::Operand, format!("{what}: dtype or shape is not the declared one"));
    }
    if crate::tensor::count(&t.shape) != t.data.len() as u64 || t.data.iter().any(|&v| !t.dtype.contains(v)) {
        return err(Class::Operand, format!("{what}: malformed tensor"));
    }
    Ok(())
}

/// Where a node's leaves come from: a run state (a step) or a cone environment.
struct Leaves<'a> {
    token: u64,
    pos: u64,
    carry_in: &'a BTreeMap<u8, Tensor>,
    fixed: &'a dyn Fn(u16) -> Option<&'a Tensor>,
    hist: &'a dyn Fn(u16) -> Option<&'a [Tensor]>,
}

fn const_tensor(p: &Program, j: u16) -> Res<Tensor> {
    let c = &p.consts[j as usize];
    Tensor::from_le_bytes(c.dtype, c.shape.iter().map(|&d| d as u64).collect(), &c.data)
}

/// Evaluates the nodes of occurrence `(b, layer)` selected by `todo` (in index order), with the
/// values of `known` nodes taken as given. Returns every value computed or given, by node index.
#[allow(clippy::too_many_arguments)]
fn evaluate_nodes(
    p: &Program,
    params: &Params,
    b: usize,
    layer: Option<u32>,
    h: u64,
    leaves: &Leaves<'_>,
    todo: &[bool],
    mut vals: Vec<Option<Tensor>>,
    effects: &mut Vec<(Prim, Option<u32>, Tensor)>,
) -> Res<Vec<Option<Tensor>>> {
    let block = &p.blocks[b];
    for (i, n) in block.nodes.iter().enumerate() {
        if !todo[i] {
            continue;
        }
        let mut owned: Vec<Tensor> = Vec::with_capacity(n.inputs.len());
        let mut from_node: Vec<Option<usize>> = Vec::with_capacity(n.inputs.len());
        for r in &n.inputs {
            match *r {
                Ref::Node(k) => {
                    if vals[k as usize].is_none() {
                        return err(Class::Missing, format!("node {k} has no value"));
                    }
                    from_node.push(Some(k as usize));
                    owned.push(Tensor::zeros(DType::I8, vec![])); // placeholder, never read
                }
                Ref::CarryIn(k) => {
                    let Some(t) = leaves.carry_in.get(&k) else {
                        return err(Class::Missing, format!("carry-in {k} not supplied"));
                    };
                    check_tensor(t, &block.carry_in[k as usize], h, "carry-in")?;
                    from_node.push(None);
                    owned.push(t.clone());
                }
                Ref::Param(j) => {
                    let d = &p.params[j as usize];
                    let Some(t) = params.get(&(j, instance(d.per_layer, layer))) else {
                        return err(Class::Missing, format!("param {j} ({}) at layer {layer:?} not supplied", d.name));
                    };
                    check_tensor(t, &TensorType::fixed(d.dtype, &d.shape), h, "param")?;
                    from_node.push(None);
                    owned.push(t.clone());
                }
                Ref::Const(j) => {
                    from_node.push(None);
                    owned.push(const_tensor(p, j)?);
                }
                Ref::State(j) => {
                    let s = &p.states[j as usize];
                    let Some(t) = (leaves.fixed)(j) else {
                        return err(Class::Missing, format!("Fixed state {j} not supplied"));
                    };
                    check_tensor(t, &TensorType::fixed(s.dtype, &s.shape), h, "Fixed state")?;
                    if let StateKind::Fixed { lo, hi } = s.kind {
                        if t.data.iter().any(|&v| v < lo as i128 || v > hi as i128) {
                            return err(Class::Operand, format!("Fixed state {j} outside [{lo}, {hi}]"));
                        }
                    }
                    from_node.push(None);
                    owned.push(t.clone());
                }
                Ref::Input(0) => {
                    if leaves.token >= p.token_bound as u64 {
                        return err(Class::Operand, format!("token {} ≥ token_bound {}", leaves.token, p.token_bound));
                    }
                    from_node.push(None);
                    owned.push(Tensor { dtype: DType::Idx, shape: vec![], data: vec![leaves.token as i128] });
                }
                Ref::Input(_) => {
                    from_node.push(None);
                    owned.push(Tensor { dtype: DType::Idx, shape: vec![], data: vec![leaves.pos as i128] });
                }
            }
        }
        let ins: Vec<&Tensor> =
            from_node.iter().zip(owned.iter()).map(|(f, o)| match f { Some(k) => vals[*k].as_ref().unwrap(), None => o }).collect();
        let prior = match n.prim {
            Prim::HistAppend { state } => {
                let Some(rows) = (leaves.hist)(state) else {
                    return err(Class::Missing, format!("history of state {state} not supplied"));
                };
                // The rows visible at this position: exactly H − 1 of them.
                if rows.len() as u64 + 1 != h {
                    return err(Class::Operand, format!("history of state {state} has {} rows, H − 1 = {}", rows.len(), h - 1));
                }
                Some(rows)
            }
            _ => None,
        };
        let v = eval_prim(&n.prim, &ins, n.out.dtype, &n.out.extents(h), &p.states, prior)?;
        match n.prim {
            Prim::StateWrite { .. } => effects.push((n.prim.clone(), layer, v.clone())),
            Prim::HistAppend { .. } => effects.push((n.prim.clone(), layer, ins[0].clone())),
            _ => {}
        }
        vals[i] = Some(v);
    }
    Ok(vals)
}

/// The history length `H = min(pos + 1, W)` of block `b` (1 when the block has no window; no
/// tensor of such a block contains `H`).
fn history_len(p: &Program, b: usize, pos: u64) -> Res<u64> {
    Ok(match block_window(p, b)? {
        Some(w) => (pos + 1).min(w as u64),
        None => 1,
    })
}

fn program_reads_token(p: &Program) -> bool {
    p.blocks.iter().any(|b| b.nodes.iter().any(|n| n.inputs.contains(&Ref::Input(0))))
}

/// §9.1: one step from `st` with `token`. Returns the step's result and the next run state.
pub fn step(p: &Program, params: &Params, st: &RunState, token: u64) -> Res<(StepOutput, RunState)> {
    if st.pos >= p.history_bound as u64 {
        return err(Class::Position, format!("pos {} ≥ history_bound", st.pos));
    }
    if program_reads_token(p) && token >= p.token_bound as u64 {
        return err(Class::Operand, format!("token {token} ≥ token_bound {}", p.token_bound));
    }
    let mut commits = Vec::new();
    let mut carry: BTreeMap<u8, Tensor> = BTreeMap::new();
    let mut effects: Vec<(Prim, Option<u32>, Tensor)> = Vec::new();
    let mut slot_base: u64 = 0;
    let mut logits = None;
    for (b, layer) in occurrences(p) {
        let bu = b as usize;
        let block = &p.blocks[bu];
        let h = history_len(p, bu, st.pos)?;
        let fixed = |j: u16| st.fixed.get(&(j, instance(p.states[j as usize].per_layer, layer)));
        let hist = |j: u16| st.hist.get(&(j, instance(p.states[j as usize].per_layer, layer))).map(|v| v.as_slice());
        let leaves = Leaves { token, pos: st.pos, carry_in: &carry, fixed: &fixed, hist: &hist };
        let todo = vec![true; block.nodes.len()];
        let vals = evaluate_nodes(p, params, bu, layer, h, &leaves, &todo, vec![None; block.nodes.len()], &mut effects)?;
        let vals: Vec<Tensor> = vals.into_iter().map(|v| v.unwrap()).collect();
        for (i, n) in block.nodes.iter().enumerate() {
            if n.commit {
                commits.push(Commit { slot: slot_base + i as u64, block: b, layer, node: i as u16, value: vals[i].clone() });
            }
        }
        if bu == p.schedule.post as usize {
            logits = Some(vals[p.logits as usize].clone());
        }
        carry = block.carry_out.iter().enumerate().map(|(k, &c)| (k as u8, vals[c as usize].clone())).collect();
        slot_base += block.nodes.len() as u64;
    }
    // Effects, only now that every node of every occurrence succeeded.
    let mut next = st.clone();
    for (prim, layer, v) in effects {
        match prim {
            Prim::StateWrite { state } => {
                let key = (state, instance(p.states[state as usize].per_layer, layer));
                next.fixed.insert(key, v);
            }
            Prim::HistAppend { state } => {
                let s = &p.states[state as usize];
                let StateKind::Hist { window } = s.kind else { unreachable!() };
                let key = (state, instance(s.per_layer, layer));
                let rows = next.hist.entry(key).or_default();
                rows.push(v);
                let keep = window as usize - 1;
                if rows.len() > keep {
                    rows.drain(..rows.len() - keep);
                }
            }
            _ => {}
        }
    }
    next.pos = st.pos + 1;
    Ok((StepOutput { logits: logits.unwrap(), commits }, next))
}

/// A run: steps at positions `0 … T−1` from the initial state (§9.1).
pub fn run(p: &Program, params: &Params, tokens: &[u64]) -> Res<Vec<StepOutput>> {
    let mut st = initial_state(p);
    let mut out = Vec::with_capacity(tokens.len());
    for &t in tokens {
        let (o, next) = step(p, params, &st, t)?;
        out.push(o);
        st = next;
    }
    Ok(out)
}

/// The environment of a cone evaluation (§9.2).
#[derive(Clone, Debug, Default)]
pub struct ConeEnv {
    pub token: u64,
    pub pos: u64,
    pub carry_in: BTreeMap<u8, Tensor>,
    /// `Fixed` values at the start of the position, by state (the instance at the cone's layer).
    pub fixed: BTreeMap<u16, Tensor>,
    /// For each `Hist` state, the prior rows, oldest first.
    pub hist_prior: BTreeMap<u16, Vec<Tensor>>,
    /// Supplied node values of the occurrence.
    pub supplied: BTreeMap<u16, Tensor>,
}

/// §9.2 `eval_cone(block, layer, target, env)`.
///
/// Readings taken where the text is silent (ref2-findings.md): the target is always recomputed
/// (F-CONE-TARGET); `pos` must be a position (`< history_bound`) and a token the closure reads must
/// be below `token_bound` (F-CONE-BOUNDS); every carry-in, state and history row the closure reads
/// is checked against its declaration (F-CONE-CHECKS); a supplied index that is not a node of the
/// block is refused.
pub fn eval_cone(p: &Program, params: &Params, block: u8, layer: Option<u32>, target: u16, env: &ConeEnv) -> Res<Tensor> {
    let bu = block as usize;
    let is_occurrence = match layer {
        None => block == p.schedule.pre || block == p.schedule.post,
        Some(l) => p.schedule.layers.get(l as usize) == Some(&block),
    };
    if bu >= p.blocks.len() || !is_occurrence {
        return err(Class::Operand, "(block, layer) is not an occurrence of the schedule");
    }
    let blk = &p.blocks[bu];
    let n = blk.nodes.len();
    if target as usize >= n {
        return err(Class::Operand, "target is not a node of the block");
    }
    if env.pos >= p.history_bound as u64 {
        return err(Class::Position, format!("pos {} ≥ history_bound", env.pos));
    }
    if let Some((&k, _)) = env.supplied.iter().find(|(k, _)| **k as usize >= n) {
        return err(Class::Operand, format!("supplied node {k} is not a node of the block"));
    }
    let h = history_len(p, bu, env.pos)?;
    // The backward closure of the target, stopping at supplied nodes (the target itself is always
    // recomputed).
    let mut todo = vec![false; n];
    let mut vals: Vec<Option<Tensor>> = vec![None; n];
    todo[target as usize] = true;
    for i in (0..=target as usize).rev() {
        if !todo[i] {
            continue;
        }
        for r in &blk.nodes[i].inputs {
            if let Ref::Node(k) = *r {
                let k = k as usize;
                if let Some(v) = env.supplied.get(&(k as u16)) {
                    if vals[k].is_none() {
                        check_tensor(v, &blk.nodes[k].out, h, "supplied node")?;
                        vals[k] = Some(v.clone());
                    }
                } else {
                    todo[k] = true;
                }
            }
        }
    }
    let fixed = |j: u16| env.fixed.get(&j);
    let hist = |j: u16| env.hist_prior.get(&j).map(|v| v.as_slice());
    let leaves = Leaves { token: env.token, pos: env.pos, carry_in: &env.carry_in, fixed: &fixed, hist: &hist };
    let mut effects = Vec::new();
    let vals = evaluate_nodes(p, params, bu, layer, h, &leaves, &todo, vals, &mut effects)?;
    Ok(vals[target as usize].clone().unwrap())
}
