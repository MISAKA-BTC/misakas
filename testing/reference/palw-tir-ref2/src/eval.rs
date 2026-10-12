//! The reference evaluation of a step, of a run (04b §9.1) and of a cone (§9.2).
//!
//! A step is a pure function from a run state to a new run state: effects are collected while the
//! occurrences are evaluated and applied to a copy only when every node succeeded (PALW-TIR-28).

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{Class, Res, TirError, err};
use crate::normal_form::block_window;
use crate::prims::eval_prim;
use crate::program::{Prim, Program, Ref, StateKind};
use crate::tensor::Tensor;
use crate::types::{DType, TensorType};

/// Param instances by `(param index, layer)`; the layer is `None` for a global param.
pub type Params = BTreeMap<(u16, Option<u32>), Tensor>;

/// Parameter bytes may be decoded on demand instead of retaining the entire model.
/// The independent evaluator still checks each tensor and evaluates all primitives itself.
pub trait ParamSource {
    fn tensor(&self, index: u16, layer: Option<u32>) -> Res<Option<Tensor>>;

    /// **Row-tiled access, if this source offers it** (RFC-0013 §5; [`crate::tiled`]). A source that returns one lets a `MatMul` or a
    /// `Gather` consume a param larger than a tile in row ranges, never whole. The default offers none, and evaluation is the whole-tensor
    /// evaluation it always was.
    fn tiles(&self) -> Option<&dyn crate::tiled::TileAccess> {
        None
    }
}

impl ParamSource for Params {
    fn tensor(&self, index: u16, layer: Option<u32>) -> Res<Option<Tensor>> {
        Ok(self.get(&(index, layer)).cloned())
    }
}

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

/// Where a node's leaves come from: a run state (a step) or a cone environment. `fixed` and `hist`
/// return `None` for an absent value; a step maps absence to the initial value first (§9.1
/// run-state completeness), a cone never does (§9.2).
struct Leaves<'a> {
    token: Option<u64>,
    pos: u64,
    carry_in: &'a BTreeMap<u8, Tensor>,
    fixed: &'a dyn Fn(u16) -> Option<Tensor>,
    hist: &'a dyn Fn(u16) -> Option<Vec<Tensor>>,
}

/// Checks a `Fixed` value against its declaration (§9.1(2)): dtype, shape, values in `[lo, hi]`.
fn check_fixed(p: &Program, j: u16, t: &Tensor) -> Res<()> {
    let s = &p.states[j as usize];
    check_tensor(t, &TensorType::fixed(s.dtype, &s.shape), 1, "Fixed state")?;
    if let StateKind::Fixed { lo, hi } = s.kind
        && t.data.iter().any(|&v| v < lo as i128 || v > hi as i128)
    {
        return err(Class::Operand, format!("Fixed state {j} outside [{lo}, {hi}]"));
    }
    Ok(())
}

/// Checks a history against its declaration: exactly `H − 1 = min(pos, window − 1)` rows
/// (`Position`), each of the state's dtype and row shape (`Operand`).
fn check_hist(p: &Program, j: u16, rows: &[Tensor], h: u64) -> Res<()> {
    if rows.len() as u64 + 1 != h {
        return err(Class::Position, format!("history of state {j} has {} rows, H − 1 = {}", rows.len(), h - 1));
    }
    let s = &p.states[j as usize];
    let ty = TensorType::fixed(s.dtype, &s.shape);
    for r in rows {
        check_tensor(r, &ty, 1, "history row")?;
    }
    Ok(())
}

fn const_tensor(p: &Program, j: u16) -> Res<Tensor> {
    let c = &p.consts[j as usize];
    Tensor::from_le_bytes(c.dtype, c.shape.iter().map(|&d| d as u64).collect(), &c.data)
}

/// An effect of a node: a `StateWrite` value or a `HistAppend` row, at a layer.
type Effect = (Prim, Option<u32>, Tensor);

/// **A param a node reads in tiles instead of whole** (RFC-0013 §5), or `None` when it is no larger than a tile (and is loaded whole, as ever). The
/// declaration is checked as the whole path checks it — present, the declared dtype and shape — and fails the same way; the elements are checked tile
/// by tile as they are read. `transposed` is for the param behind a fused `Transpose` `[1, 0]` (a rank-2 param only).
fn lazy_param_behind<'a>(
    p: &Program,
    tiles: &'a dyn crate::tiled::TileAccess,
    layer: Option<u32>,
    j: u16,
    transposed: bool,
) -> Res<Option<crate::tiled::LazyParam<'a>>> {
    let d = &p.params[j as usize];
    let Some((dtype, shape)) = tiles.decl(j, instance(d.per_layer, layer))? else {
        return err(Class::Missing, format!("param {j} ({}) at layer {layer:?} not supplied", d.name));
    };
    let declared: Vec<u64> = d.shape.iter().map(|&x| x as u64).collect();
    if dtype != d.dtype || shape != declared {
        return err(Class::Operand, "param: dtype or shape is not the declared one");
    }
    if (transposed && shape.len() != 2) || crate::tensor::count(&shape) <= tiles.tile_elems() {
        return Ok(None);
    }
    Ok(Some(crate::tiled::LazyParam { tiles, index: j, layer: instance(d.per_layer, layer), dtype, shape, transposed }))
}

/// Evaluates node `i` of occurrence `(b, layer)` from its operands (§3.2, §6, §9.1(2)); node operands
/// are read from `vals`, which must hold them.
#[allow(clippy::too_many_arguments)]
fn eval_one(
    p: &Program,
    params: &dyn ParamSource,
    b: usize,
    layer: Option<u32>,
    h: u64,
    leaves: &Leaves<'_>,
    i: usize,
    vals: &[Option<Tensor>],
    unbuilt: &BTreeSet<usize>,
) -> Res<(Tensor, Option<Effect>)> {
    let block = &p.blocks[b];
    let n = &block.nodes[i];
    let mut owned: Vec<Tensor> = Vec::with_capacity(n.inputs.len());
    let mut from_node: Vec<Option<usize>> = Vec::with_capacity(n.inputs.len());
    // A param this node reads in row tiles instead of whole (RFC-0013 §5): its position among the inputs and its declaration.
    let mut lazy: Option<(usize, crate::tiled::LazyParam<'_>)> = None;
    for (pos, r) in n.inputs.iter().enumerate() {
        match *r {
            Ref::Node(k) => {
                // RFC-0013 §5: a `Transpose` `[1, 0]` of a param the caller left unbuilt (`evaluate_nodes`) is read by this `MatMul` in row tiles,
                // through the transpose, from the param it transposes.
                if (k as usize) < i
                    && unbuilt.contains(&(k as usize))
                    && lazy.is_none()
                    && pos <= 1
                    && matches!(n.prim, Prim::MatMul)
                    && let Some(tiles) = params.tiles()
                    && let [Ref::Param(j)] = block.nodes[k as usize].inputs.as_slice()
                    && let Some(param) = lazy_param_behind(p, tiles, layer, *j, true)?
                {
                    lazy = Some((pos, param));
                    from_node.push(None);
                    owned.push(Tensor::zeros(DType::I8, vec![])); // placeholder, never read
                    continue;
                }
                if k as usize >= i || vals[k as usize].is_none() {
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
                // One param per node is read in tiles; a second (a `MatMul` of two params) is loaded whole like any other.
                if lazy.is_none()
                    && let Some(tiles) = params.tiles()
                    && crate::tiled::tiled_form_exists(&n.prim, pos, d.shape.len())
                    && let Some(param) = lazy_param_behind(p, tiles, layer, j, false)?
                {
                    lazy = Some((pos, param));
                    from_node.push(None);
                    owned.push(Tensor::zeros(DType::I8, vec![])); // placeholder, never read
                    continue;
                }
                let Some(t) = params.tensor(j, instance(d.per_layer, layer))? else {
                    return err(Class::Missing, format!("param {j} ({}) at layer {layer:?} not supplied", d.name));
                };
                check_tensor(&t, &TensorType::fixed(d.dtype, &d.shape), h, "param")?;
                from_node.push(None);
                owned.push(t);
            }
            Ref::Const(j) => {
                from_node.push(None);
                owned.push(const_tensor(p, j)?);
            }
            Ref::State(j) => {
                let Some(t) = (leaves.fixed)(j) else {
                    return err(Class::Missing, format!("Fixed state {j} not supplied"));
                };
                check_fixed(p, j, &t)?;
                from_node.push(None);
                owned.push(t);
            }
            Ref::Input(0) => {
                let Some(token) = leaves.token else {
                    return err(Class::Missing, "the token is read and absent");
                };
                if token >= p.token_bound as u64 {
                    return err(Class::Operand, format!("token {token} ≥ token_bound {}", p.token_bound));
                }
                from_node.push(None);
                owned.push(Tensor { dtype: DType::Idx, shape: vec![], data: vec![token as i128] });
            }
            Ref::Input(_) => {
                from_node.push(None);
                owned.push(Tensor { dtype: DType::Idx, shape: vec![], data: vec![leaves.pos as i128] });
            }
        }
    }
    let ins: Vec<&Tensor> = from_node
        .iter()
        .zip(owned.iter())
        .map(|(f, o)| match f {
            Some(k) => vals[*k].as_ref().unwrap(),
            None => o,
        })
        .collect();
    let prior = match n.prim {
        Prim::HistAppend { state } => {
            let Some(rows) = (leaves.hist)(state) else {
                return err(Class::Missing, format!("history of state {state} not supplied"));
            };
            // The rows visible at this position: exactly H − 1 of them.
            check_hist(p, state, &rows, h)?;
            Some(rows)
        }
        _ => None,
    };
    let v = match &lazy {
        Some((pos, param)) => crate::tiled::eval_prim_tiled(&n.prim, &ins, *pos, param, n.out.dtype, &n.out.extents(h))?,
        None => eval_prim(&n.prim, &ins, n.out.dtype, &n.out.extents(h), &p.states, prior.as_deref())?,
    };
    let effect = match n.prim {
        Prim::StateWrite { .. } => Some((n.prim.clone(), layer, v.clone())),
        Prim::HistAppend { .. } => Some((n.prim.clone(), layer, ins[0].clone())),
        _ => None,
    };
    Ok((v, effect))
}

/// Evaluates the nodes of occurrence `(b, layer)` selected by `todo` (in index order), with the
/// values already in `vals` taken as given. Returns every value computed or given, by node index.
#[allow(clippy::too_many_arguments)]
fn evaluate_nodes(
    p: &Program,
    params: &dyn ParamSource,
    b: usize,
    layer: Option<u32>,
    h: u64,
    leaves: &Leaves<'_>,
    todo: &[bool],
    mut vals: Vec<Option<Tensor>>,
    effects: &mut Vec<Effect>,
    fuse: bool,
) -> Res<Vec<Option<Tensor>>> {
    // RFC-0013 §5: with a tiling source, a `Transpose` `[1, 0]` of a param larger than a tile whose only reader is one `MatMul` is never built — the
    // `MatMul` reads the param in row tiles through it. Its value is unobservable (not committed, not a carry-out, not the logits).
    let fusable = if fuse && params.tiles().is_some() { crate::tiled::fusable_transposes(p, b) } else { BTreeSet::new() };
    let mut unbuilt: BTreeSet<usize> = BTreeSet::new();
    for i in 0..p.blocks[b].nodes.len() {
        if !todo[i] {
            continue;
        }
        if fusable.contains(&i)
            && let Some(tiles) = params.tiles()
            && let [Ref::Param(j)] = p.blocks[b].nodes[i].inputs.as_slice()
            && lazy_param_behind(p, tiles, layer, *j, true)?.is_some()
        {
            unbuilt.insert(i);
            continue;
        }
        let (v, e) = eval_one(p, params, b, layer, h, leaves, i, &vals, &unbuilt)?;
        if let Some(e) = e {
            effects.push(e);
        }
        vals[i] = Some(v);
    }
    Ok(vals)
}

/// Every rule a step breaks, as the set of their §9.3 classes — for checking "an input that breaks
/// several rules reports the class of one of them" (§9.3). The early checks of §9.1(1), then every
/// node whose operands can be computed: a node that fails adds its class and poisons its consumers
/// (and, through the carry, the next occurrence's readers), which are not evaluable. Empty iff the
/// step succeeds.
pub fn step_violations(p: &Program, params: &dyn ParamSource, st: &RunState, token: u64) -> BTreeSet<Class> {
    let mut out = BTreeSet::new();
    if st.pos >= p.history_bound as u64 {
        out.insert(Class::Position);
        return out;
    }
    if program_reads_token(p) && token >= p.token_bound as u64 {
        out.insert(Class::Operand);
    }
    let mut carry: BTreeMap<u8, Tensor> = BTreeMap::new();
    for (b, layer) in occurrences(p) {
        let bu = b as usize;
        let block = &p.blocks[bu];
        let h = match history_len(p, bu, st.pos) {
            Ok(h) => h,
            Err(e) => {
                out.insert(e.class);
                return out;
            }
        };
        let fixed = |j: u16| {
            let s = &p.states[j as usize];
            Some(match st.fixed.get(&(j, instance(s.per_layer, layer))) {
                Some(t) => t.clone(),
                None => Tensor::zeros(s.dtype, s.shape.iter().map(|&d| d as u64).collect()),
            })
        };
        let hist = |j: u16| Some(st.hist.get(&(j, instance(p.states[j as usize].per_layer, layer))).cloned().unwrap_or_default());
        let leaves = Leaves { token: Some(token), pos: st.pos, carry_in: &carry, fixed: &fixed, hist: &hist };
        let mut vals: Vec<Option<Tensor>> = vec![None; block.nodes.len()];
        let mut poisoned = vec![false; block.nodes.len()];
        for (i, n) in block.nodes.iter().enumerate() {
            let blocked = n.inputs.iter().any(|r| match *r {
                Ref::Node(k) => poisoned.get(k as usize).copied().unwrap_or(true),
                Ref::CarryIn(k) => !carry.contains_key(&k),
                _ => false,
            });
            if blocked {
                poisoned[i] = true;
                continue;
            }
            match eval_one(p, params, bu, layer, h, &leaves, i, &vals, &BTreeSet::new()) {
                Ok((v, _)) => vals[i] = Some(v),
                Err(e) => {
                    out.insert(e.class);
                    poisoned[i] = true;
                }
            }
        }
        carry = block
            .carry_out
            .iter()
            .enumerate()
            .filter_map(|(k, &c)| vals.get(c as usize).cloned().flatten().map(|v| (k as u8, v)))
            .collect();
    }
    out
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

/// One occurrence of a traced step: its carry-in and the value of every node.
#[derive(Clone, Debug)]
pub struct OccTrace {
    pub block: u8,
    pub layer: Option<u32>,
    pub slot_base: u64,
    pub carry_in: BTreeMap<u8, Tensor>,
    pub values: Vec<Tensor>,
}

/// §9.1: one step from `st` with `token`. Returns the step's result and the next run state.
pub fn step(p: &Program, params: &dyn ParamSource, st: &RunState, token: u64) -> Res<(StepOutput, RunState)> {
    step_inner(p, params, st, token, None)
}

/// [`step`], also returning every occurrence's carry-in and node values (for building cones).
pub fn step_traced(p: &Program, params: &dyn ParamSource, st: &RunState, token: u64) -> Res<(StepOutput, RunState, Vec<OccTrace>)> {
    let mut trace = Vec::new();
    let (o, next) = step_inner(p, params, st, token, Some(&mut trace))?;
    Ok((o, next, trace))
}

fn step_inner(
    p: &Program,
    params: &dyn ParamSource,
    st: &RunState,
    token: u64,
    mut trace: Option<&mut Vec<OccTrace>>,
) -> Res<(StepOutput, RunState)> {
    if st.pos >= p.history_bound as u64 {
        return err(Class::Position, format!("pos {} ≥ history_bound", st.pos));
    }
    if program_reads_token(p) && token >= p.token_bound as u64 {
        return err(Class::Operand, format!("token {token} ≥ token_bound {}", p.token_bound));
    }
    let mut commits = Vec::new();
    let mut carry: BTreeMap<u8, Tensor> = BTreeMap::new();
    let mut effects: Vec<Effect> = Vec::new();
    let mut slot_base: u64 = 0;
    let mut logits = None;
    for (b, layer) in occurrences(p) {
        let bu = b as usize;
        let Some(block) = p.blocks.get(bu) else {
            return err(Class::NormalForm, "schedule names no block");
        };
        let h = history_len(p, bu, st.pos)?;
        // Run-state completeness (§9.1): an absent Fixed instance is all zeros, an absent history
        // is empty; the values present are checked as they are read.
        let fixed = |j: u16| {
            let s = &p.states[j as usize];
            Some(match st.fixed.get(&(j, instance(s.per_layer, layer))) {
                Some(t) => t.clone(),
                None => Tensor::zeros(s.dtype, s.shape.iter().map(|&d| d as u64).collect()),
            })
        };
        let hist = |j: u16| Some(st.hist.get(&(j, instance(p.states[j as usize].per_layer, layer))).cloned().unwrap_or_default());
        let leaves = Leaves { token: Some(token), pos: st.pos, carry_in: &carry, fixed: &fixed, hist: &hist };
        let todo = vec![true; block.nodes.len()];
        let vals =
            evaluate_nodes(p, params, bu, layer, h, &leaves, &todo, vec![None; block.nodes.len()], &mut effects, trace.is_none())?;
        let vals: Vec<Tensor> = vals.into_iter().map(|v| v.unwrap_or_else(|| Tensor::zeros(DType::I8, vec![]))).collect();
        for (i, n) in block.nodes.iter().enumerate() {
            if n.commit {
                commits.push(Commit { slot: slot_base + i as u64, block: b, layer, node: i as u16, value: vals[i].clone() });
            }
        }
        if bu == p.schedule.post as usize {
            logits = vals.get(p.logits as usize).cloned();
        }
        let next_carry = block
            .carry_out
            .iter()
            .enumerate()
            .map(|(k, &c)| {
                vals.get(c as usize).cloned().map(|v| (k as u8, v)).ok_or_else(|| TirError::new(Class::NormalForm, "carry-out"))
            })
            .collect::<Res<BTreeMap<u8, Tensor>>>()?;
        if let Some(t) = trace.as_deref_mut() {
            t.push(OccTrace { block: b, layer, slot_base, carry_in: carry.clone(), values: vals });
        }
        carry = next_carry;
        slot_base += block.nodes.len() as u64;
    }
    let Some(logits) = logits else {
        return err(Class::NormalForm, "no logits");
    };
    // Effects, only now that every node of every occurrence succeeded. NF-19 (revision 2) leaves one
    // writer per state instance per step, so the order they are applied in is immaterial.
    let mut next = st.clone();
    for (prim, layer, v) in effects {
        match prim {
            Prim::StateWrite { state } => {
                let key = (state, instance(p.states[state as usize].per_layer, layer));
                next.fixed.insert(key, v);
            }
            Prim::HistAppend { state } => {
                let s = &p.states[state as usize];
                if let StateKind::Hist { window } = s.kind {
                    let key = (state, instance(s.per_layer, layer));
                    let rows = next.hist.entry(key).or_default();
                    rows.push(v);
                    // At most window − 1 rows stay visible to the next position.
                    let keep = (window as usize).saturating_sub(1);
                    if rows.len() > keep {
                        rows.drain(..rows.len() - keep);
                    }
                }
            }
            _ => {}
        }
    }
    next.pos = st.pos + 1;
    Ok((StepOutput { logits, commits }, next))
}

/// A run: steps at positions `0 … T−1` from the initial state (§9.1).
pub fn run(p: &Program, params: &dyn ParamSource, tokens: &[u64]) -> Res<Vec<StepOutput>> {
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
    /// The token, or none.
    pub token: Option<u64>,
    pub pos: u64,
    pub carry_in: BTreeMap<u8, Tensor>,
    /// `Fixed` values at the start of the position, by state (the instance at the cone's layer).
    pub fixed: BTreeMap<u16, Tensor>,
    /// For each `Hist` state, the prior rows, oldest first (possibly none).
    pub hist_prior: BTreeMap<u16, Vec<Tensor>>,
    /// Supplied node values of the occurrence.
    pub supplied: BTreeMap<u16, Tensor>,
}

/// §9.2 `eval_cone(block, layer, target, env)`, revision 2, in the text's order: the request
/// (`Malformed`), the environment's two malformations (`Malformed`), then before any node is
/// evaluated the position (`Position`) and the token (`Missing`/`Operand`), then every value the
/// closure reads against the §9.2 table (absent → `Missing`, never implied; ill-formed → `Operand`;
/// a history with the wrong number of rows → `Position`), and only then the evaluation.
pub fn eval_cone(p: &Program, params: &dyn ParamSource, block: u8, layer: Option<u32>, target: u16, env: &ConeEnv) -> Res<Tensor> {
    let bu = block as usize;
    // The request.
    let is_occurrence = bu < p.blocks.len()
        && match layer {
            None => block == p.schedule.pre || block == p.schedule.post,
            Some(l) => p.schedule.layers.get(l as usize) == Some(&block),
        };
    if !is_occurrence {
        return err(Class::Malformed, "(block, layer) is not an occurrence of the schedule");
    }
    let blk = &p.blocks[bu];
    let n = blk.nodes.len();
    if target as usize >= n {
        return err(Class::Malformed, "target is not a node of the block");
    }
    // The environment's malformations.
    if env.supplied.contains_key(&target) {
        return err(Class::Malformed, "the environment supplies the target itself");
    }
    if let Some((&k, _)) = env.supplied.iter().find(|(k, _)| **k as usize >= n) {
        return err(Class::Malformed, format!("supplied index {k} is not a node of the block"));
    }
    // The closure: backward from the target through Node refs, stopping at supplied nodes.
    let mut todo = vec![false; n];
    todo[target as usize] = true;
    for i in (0..=target as usize).rev() {
        if todo[i] {
            for r in &blk.nodes[i].inputs {
                if let Ref::Node(k) = *r
                    && !env.supplied.contains_key(&k)
                {
                    todo[k as usize] = true;
                }
            }
        }
    }
    let closure: Vec<usize> = (0..n).filter(|&i| todo[i]).collect();
    // Before any node is evaluated: the position, then the token if the closure reads it.
    if env.pos >= p.history_bound as u64 {
        return err(Class::Position, format!("pos {} ≥ history_bound", env.pos));
    }
    if closure.iter().any(|&i| blk.nodes[i].inputs.contains(&Ref::Input(0))) {
        match env.token {
            None => return err(Class::Missing, "the closure reads the token and the environment has none"),
            Some(t) if t >= p.token_bound as u64 => return err(Class::Operand, format!("token {t} ≥ token_bound {}", p.token_bound)),
            _ => {}
        }
    }
    // Every value the closure reads, checked before evaluation.
    let h = history_len(p, bu, env.pos)?;
    let mut vals: Vec<Option<Tensor>> = vec![None; n];
    for &i in &closure {
        let node = &blk.nodes[i];
        for r in &node.inputs {
            match *r {
                Ref::Node(k) => {
                    if let Some(v) = env.supplied.get(&k) {
                        check_tensor(v, &blk.nodes[k as usize].out, h, "supplied node")?;
                        vals[k as usize] = Some(v.clone());
                    }
                }
                Ref::CarryIn(k) => match env.carry_in.get(&k) {
                    None => return err(Class::Missing, format!("carry-in {k} not supplied")),
                    Some(t) => check_tensor(t, &blk.carry_in[k as usize], h, "carry-in")?,
                },
                Ref::State(j) => match env.fixed.get(&j) {
                    None => return err(Class::Missing, format!("Fixed state {j} not supplied (never implied)")),
                    Some(t) => check_fixed(p, j, t)?,
                },
                Ref::Param(j) => {
                    let d = &p.params[j as usize];
                    match params.tensor(j, instance(d.per_layer, layer))? {
                        None => return err(Class::Missing, format!("param {j} at layer {layer:?} not supplied")),
                        Some(t) => check_tensor(&t, &TensorType::fixed(d.dtype, &d.shape), h, "param")?,
                    }
                }
                Ref::Const(_) | Ref::Input(_) => {}
            }
        }
        if let Prim::HistAppend { state } = node.prim {
            match env.hist_prior.get(&state) {
                None => return err(Class::Missing, format!("history of state {state} not supplied (even when empty)")),
                Some(rows) => check_hist(p, state, rows, h)?,
            }
        }
    }
    let fixed = |j: u16| env.fixed.get(&j).cloned();
    let hist = |j: u16| env.hist_prior.get(&j).cloned();
    let leaves = Leaves { token: env.token, pos: env.pos, carry_in: &env.carry_in, fixed: &fixed, hist: &hist };
    let mut effects = Vec::new();
    let vals = evaluate_nodes(p, params, bu, layer, h, &leaves, &todo, vals, &mut effects, false)?;
    vals[target as usize].clone().ok_or_else(|| TirError::new(Class::Missing, "the target has no value"))
}
