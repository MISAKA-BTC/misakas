//! **Element-demand evaluation: the elements of one tile, computed from the elements they read.**
//!
//! The court adjudicates ONE committed tile (RFC-0002 §6; `docs/design/palw/tir/phase-f-integration.md`
//! §2.7). Evaluating whole nodes, as [`crate::interp::Interpreter::eval_cone`] does, is the definition
//! but not a budget: the cone of a vocabulary tile is a `[1, D] × [D, V]` product, and the cone of one
//! gated-delta head sits beside every other head's state. This module evaluates exactly the demanded
//! output elements, PULLING each operand element it needs through the primitive's own index map:
//!
//! * elementwise primitives and `Broadcast` read the same (broadcast-mapped) index;
//! * `Reshape`, `Transpose`, `Slice`, `Concat` read the remapped index;
//! * `MatMul` reads one row of `a` and one column of `b`; `ReduceSum`/`ReduceMax`/`TopK` read the
//!   whole reduced axis of their row;
//! * `Gather` reads its index element first, then the one data element that index names;
//! * `Select` reads its condition, then only the operand it chooses;
//! * `HistAppend` reads a prior history row or its own row.
//!
//! Every element is defined independently of every other (spec 04b §6.1), so a pulled element is,
//! bit for bit, the element [`crate::interp::Interpreter::eval_cone`] computes; `tests/demand.rs`
//! checks exactly that on every program of `consensus-vectors/tir-v1/programs`, at every node, tile
//! and position.
//!
//! **Where a node runs.** An evaluation is not confined to one block occurrence. A node runs in a
//! [`DemandContext`] — a position and an occurrence of the schedule — and a cone crosses contexts in
//! exactly two ways, both fixed by the program:
//!
//! * a `CarryIn` is the previous occurrence's carry-out at the same position, a commit point
//!   (NF-21), so it is always a leaf;
//! * a `State` read at position `p` is the instance's value at the START of `p`. The source either
//!   supplies it ([`StateSupply::Value`] — a checkpoint leaf, or the initial zero at position 0) or
//!   answers [`StateSupply::Replay`]: then it is the instance's writer's output at `p − 1` (or, if
//!   nothing writes it, its value at the start of `p − 1`), which this module evaluates in that
//!   earlier context by the same rules. That is Fixed-state replay (design §2.6), demand-restricted
//!   for free: a disputed head replays only the elements of its own state that its cone reads.
//!
//! **Leaves.** In the target's own context every committed node other than the target is a leaf
//! (spec 04b §10.2's cone); in every other context every committed node is a leaf. A leaf's value
//! comes from the [`DemandSource`], never from evaluation.
//!
//! **No recursion.** The evaluation is an explicit stack of `(context, node, element)` frames: a
//! frame whose operands are not all known pushes the missing ones and is resumed when they are. Its
//! depth is data, not native stack, so neither a 512-node chain nor a replay over thousands of
//! positions can exhaust a verifier's thread. Values are memoised per `(context, node, element)`, so
//! each element is computed at most once and the work an evaluation is charged is a function of the
//! demanded set and the values alone, never of the order the stack happened to visit them in.
//!
//! **What the evaluator does not decide.** Where leaf values come from: a [`DemandSource`] supplies
//! them, and a court's source authenticates each value against a commitment and records the request,
//! so "the operand set a refutation must carry" is exactly the set this evaluation asked for. A value
//! is never invented: a request the source cannot serve is its error.
//!
//! **Errors.** As everywhere in this crate, success versus failure is normative and the class is
//! diagnostic (PALW-TIR-34), and the classes are spec 04b §9.3's: a request that names no context, no
//! node or no element of its target is `Malformed`; a value nobody serves is `Missing` (a `Fixed`
//! value is never implied — the source says where it comes from); a served value that is not of its
//! declaration is `Operand`. An element that is not demanded is not evaluated, so a failure in an
//! undemanded element of a whole-node evaluation is not seen here — which is moot where it matters:
//! after admission (PALW-TIR-9) and the court's committed-operand check (PALW-TIR-33) no element of
//! any cone can fail.
//!
//! **Work.** Every computed element and every reduction term (`MatMul` contraction length,
//! `ReduceSum`/`ReduceMax`/`TopK` axis length, each position a never-written state's value is carried
//! across) is counted when the element is first scanned, and the evaluation stops with
//! [`DemandError::WorkLimit`] the moment either count passes its limit — before the operands of the
//! element that passed it are fetched — so a hostile request cannot make a verifier spend more than
//! the court's ceiling.

use std::collections::BTreeMap;

use crate::arith::{div_round, int_exp, int_ln, int_rsqrt, log2_floor};
use crate::error::{TirError, TirErrorKind, TirResult};
use crate::prim::Prim;
use crate::program::{Block, INPUT_TOKEN, Ref, StateKind, TirProgramV1};
use crate::types::{DType, element_count, strides};
use crate::validate::ProgramInfo;

/// Where a node runs: a position and an occurrence of the schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DemandContext {
    pub pos: u32,
    /// An index into [`TirProgramV1::occurrences`]: `0` is `pre`, `1 + l` is layer `l`, the last is
    /// `post`.
    pub occurrence: u16,
}

/// What a source says about a `Fixed` state value at the start of a position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateSupply {
    /// The value itself (a checkpoint leaf's lane, or the initial zero at position 0).
    Value(i128),
    /// "Recompute it": the value the instance held after position `pos − 1` — its writer's output
    /// there, or, when nothing writes the instance, its value at the start of `pos − 1`.
    Replay,
}

/// Where the values an evaluation does not compute come from. Every method may refuse; the
/// evaluator never substitutes a value for a refusal.
pub trait DemandSource {
    /// Element `index` (row-major, at the context's `H`) of COMMITTED node `node` of `ctx` — a leaf.
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128>;
    /// Element `index` of param `param`'s instance at `layer` (`None` for a global param).
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128>;
    /// Element `index` of `Fixed` state instance `(state, layer)` at the START of position `pos`.
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply>;
    /// Element `index` of the row appended to `Hist` state instance `(state, layer)` at position
    /// `row_pos`, as read at position `pos` (`row_pos < pos`, inside the window). The row is the
    /// committed node [`hist_row_node_v1`] names; a court may serve it from a history tile instead.
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128>;
    /// The token of position `pos` (`Input(0)`).
    fn token(&mut self, pos: u32) -> TirResult<u32>;
}

/// The work an evaluation did: computed (non-leaf) elements, and reduction terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DemandWork {
    pub elements: u64,
    pub terms: u64,
}

/// The most work an evaluation may do before it stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DemandLimits {
    pub max_elements: u64,
    pub max_terms: u64,
}

impl DemandLimits {
    /// No limit — tests and tools; a verifier passes its court's ceiling.
    pub const UNLIMITED: Self = Self { max_elements: u64::MAX, max_terms: u64::MAX };

    /// The frames an evaluation within these limits can push: each computed element is scanned at
    /// most three times (a data-dependent primitive's two phases and its final scan), and a scan
    /// pushes at most two frames per reduction term plus one per input.
    fn max_pushes(&self) -> u64 {
        self.max_terms.saturating_mul(6).saturating_add(self.max_elements.saturating_mul(3 * crate::program::MAX_NODE_INPUTS as u64))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DemandError {
    #[error("{0}")]
    Tir(TirError),
    #[error("the demanded evaluation passed its work limit ({0:?})")]
    WorkLimit(DemandWork),
}

impl From<TirError> for DemandError {
    fn from(e: TirError) -> Self {
        DemandError::Tir(e)
    }
}

pub type DemandResult<T> = Result<T, DemandError>;

fn fail<T>(kind: TirErrorKind, msg: impl Into<String>) -> DemandResult<T> {
    Err(DemandError::Tir(TirError::new(kind, msg)))
}

/// What to evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemandTarget {
    /// Elements of node `node` of `ctx`, computed from its cone — even when the node is committed.
    Node { ctx: DemandContext, node: u16 },
    /// Elements of `Fixed` state instance `(state, layer)` AFTER position `pos` — what a checkpoint
    /// leaf at `pos` holds: the writer's output at `pos` (a leaf when the writer is committed), or,
    /// when nothing writes the instance, its value at the start of `pos`.
    StateAfter { pos: u32, state: u16, layer: Option<u16> },
}

/// A request: `elements` (row-major, at the target context's `H`) of `target`, returned in this
/// order. Duplicates are allowed and cost nothing.
#[derive(Clone, Copy, Debug)]
pub struct DemandRequest<'a> {
    pub target: DemandTarget,
    pub elements: &'a [usize],
}

/// `H` of `block` at `pos`: `min(pos + 1, window)`, or 1 for a block that appends to no history.
pub fn history_length_v1(info: &ProgramInfo, block: u8, pos: u32) -> Option<usize> {
    let b = info.blocks.get(block as usize)?;
    Some(b.window.map(|w| (pos as usize + 1).min(w as usize)).unwrap_or(1))
}

/// The leaf predicate of a commit point's cone (spec 04b §10.2): every OTHER committed node of the
/// block. (The evaluator applies it itself; exported for tools that build a cone's operand list.)
pub fn commit_cone_leaves_v1(block: &Block, target: u16) -> impl Fn(u16) -> bool + '_ {
    move |n: u16| n != target && block.nodes.get(n as usize).is_some_and(|node| node.commit)
}

/// **Is `(state, layer)` an instance a run holds?** A global state has its one instance (normal form
/// uses every declared state, in `pre` or `post`, NF-15); a per-layer state has one at each layer
/// whose scheduled block reads, writes or appends to it — the instances the step space checkpoints.
/// Any other is no instance: `state_after` naming it is refused (`Malformed`).
pub fn state_instance_is_held_v1(program: &TirProgramV1, state: u16, layer: Option<u16>) -> bool {
    let Some(occ) = state_occurrence_v1(program, state, layer) else { return false };
    if layer.is_none() {
        return true;
    }
    let Some(block) = occurrence_block(program, occ) else { return false };
    program.blocks[block as usize].nodes.iter().any(|n| {
        n.inputs.contains(&Ref::State(state))
            || matches!(n.prim, Prim::StateWrite { state: s } | Prim::HistAppend { state: s } if s == state)
    })
}

/// The occurrence that runs state instance `(state, layer)`: layer `l`'s (`1 + l`) for a per-layer
/// state, `pre`'s (`0`) for a global one — NF-15 keeps a global state in `pre` and `post`, and NF-19
/// keeps `post` from writing or appending.
pub fn state_occurrence_v1(program: &TirProgramV1, state: u16, layer: Option<u16>) -> Option<u16> {
    let s = program.states.get(state as usize)?;
    match (s.per_layer, layer) {
        (true, Some(l)) if (l as usize) < program.schedule.layers.len() => Some(l + 1),
        (false, None) => Some(0),
        _ => None,
    }
}

fn occurrence_block(program: &TirProgramV1, occurrence: u16) -> Option<u8> {
    let o = occurrence as usize;
    let layers = program.schedule.layers.len();
    if o == 0 {
        Some(program.schedule.pre)
    } else if o <= layers {
        Some(program.schedule.layers[o - 1])
    } else if o == layers + 1 {
        Some(program.schedule.post)
    } else {
        None
    }
}

/// **The node that writes `Fixed` instance `(state, layer)`** — `(occurrence, node)` — or `None`
/// when nothing writes it (its value is then the initial zero forever). NF-19 makes it unique.
pub fn state_writer_v1(program: &TirProgramV1, state: u16, layer: Option<u16>) -> Option<(u16, u16)> {
    let occ = state_occurrence_v1(program, state, layer)?;
    let block = &program.blocks[occurrence_block(program, occ)? as usize];
    block.nodes.iter().position(|n| matches!(n.prim, Prim::StateWrite { state: s } if s == state)).map(|n| (occ, n as u16))
}

/// **The committed node holding the row appended to `Hist` instance `(state, layer)` at `row_pos`**:
/// the `HistAppend`'s input — a commit point (PALW-TIR-14) — or, when the row is a carry-in, the
/// previous occurrence's carry-out. `None` if the instance appends nothing.
pub fn hist_row_node_v1(program: &TirProgramV1, state: u16, layer: Option<u16>, row_pos: u32) -> Option<(DemandContext, u16)> {
    let occ = state_occurrence_v1(program, state, layer)?;
    let block = &program.blocks[occurrence_block(program, occ)? as usize];
    let append = block.nodes.iter().find(|n| matches!(n.prim, Prim::HistAppend { state: s } if s == state))?;
    match *append.inputs.first()? {
        Ref::Node(i) => Some((DemandContext { pos: row_pos, occurrence: occ }, i)),
        Ref::CarryIn(k) => carry_in_node_v1(program, DemandContext { pos: row_pos, occurrence: occ }, k),
        _ => None,
    }
}

/// **The committed node a `CarryIn(k)` of `ctx` reads**: carry-out `k` of the previous occurrence at
/// the same position (a commit point, NF-21).
pub fn carry_in_node_v1(program: &TirProgramV1, ctx: DemandContext, k: u8) -> Option<(DemandContext, u16)> {
    let prev = ctx.occurrence.checked_sub(1)?;
    let block = &program.blocks[occurrence_block(program, prev)? as usize];
    let node = *block.carry_out.get(k as usize)?;
    Some((DemandContext { pos: ctx.pos, occurrence: prev }, node))
}

/// **Evaluate the demanded elements of `request.target`.** `program` must be the program `info` was
/// validated from (the caller validated it once; this function trusts `info` and nothing else).
pub fn eval_demanded(
    program: &TirProgramV1,
    info: &ProgramInfo,
    request: &DemandRequest<'_>,
    source: &mut dyn DemandSource,
    limits: &DemandLimits,
) -> DemandResult<(Vec<i128>, DemandWork)> {
    let target_node = match request.target {
        DemandTarget::Node { ctx, node } => Some((ctx, node)),
        DemandTarget::StateAfter { .. } => None,
    };
    let mut engine = Engine {
        program,
        info,
        target: target_node,
        contexts: BTreeMap::new(),
        ctxs: Vec::new(),
        state_memo: BTreeMap::new(),
        source,
        work: DemandWork::default(),
        limits: *limits,
        pushes: 0,
        max_pushes: limits.max_pushes(),
        pending: Vec::new(),
    };
    let mut out = Vec::with_capacity(request.elements.len());
    match request.target {
        DemandTarget::Node { ctx, node } => {
            let ci = engine.context(ctx)?;
            let block = &program.blocks[engine.ctxs[ci].block as usize];
            if node as usize >= block.nodes.len() {
                return fail(TirErrorKind::Malformed, "no such node");
            }
            let count = engine.ctxs[ci].counts[node as usize];
            if let Some(bad) = request.elements.iter().find(|e| **e >= count) {
                return fail(TirErrorKind::Malformed, format!("element {bad} is outside the target's {count}"));
            }
            for e in request.elements {
                engine.run(Frame { ctx: ci, node, index: *e, charged: false })?;
                out.push(engine.ctxs[ci].memo[node as usize][e]);
            }
        }
        DemandTarget::StateAfter { pos, state, layer } => {
            let s = program.states.get(state as usize).ok_or_else(|| TirError::new(TirErrorKind::Malformed, "no such state"))?;
            if !matches!(s.kind, StateKind::Fixed { .. }) || !state_instance_is_held_v1(program, state, layer) {
                return fail(TirErrorKind::Malformed, "not a Fixed state instance a run holds");
            }
            if pos >= program.history_bound {
                return fail(TirErrorKind::Position, "position ≥ history_bound");
            }
            let count: usize = s.shape.iter().map(|d| *d as usize).product();
            if let Some(bad) = request.elements.iter().find(|e| **e >= count) {
                return fail(TirErrorKind::Malformed, format!("element {bad} is outside the state's {count}"));
            }
            for e in request.elements {
                loop {
                    match engine.state_after(pos, state, layer, *e)? {
                        Fetch::Ready(v) => {
                            out.push(v);
                            break;
                        }
                        Fetch::Need(frame) => engine.run(frame)?,
                    }
                }
            }
        }
    }
    Ok((out, engine.work))
}

/// One element to compute. `charged` is set when its work has been counted.
#[derive(Clone, Copy, Debug)]
struct Frame {
    ctx: usize,
    node: u16,
    index: usize,
    charged: bool,
}

enum Fetch {
    Ready(i128),
    Need(Frame),
}

struct Ctx {
    key: DemandContext,
    block: u8,
    layer: Option<u16>,
    h: usize,
    shapes: Vec<Vec<usize>>,
    counts: Vec<usize>,
    memo: Vec<BTreeMap<usize, i128>>,
}

struct Engine<'p, 's> {
    program: &'p TirProgramV1,
    info: &'p ProgramInfo,
    target: Option<(DemandContext, u16)>,
    contexts: BTreeMap<DemandContext, usize>,
    ctxs: Vec<Ctx>,
    /// `Fixed` values at the start of a position, by `(pos, state, layer, element)`.
    state_memo: BTreeMap<(u32, u16, Option<u16>, usize), i128>,
    source: &'s mut dyn DemandSource,
    work: DemandWork,
    limits: DemandLimits,
    pushes: u64,
    max_pushes: u64,
    /// Frames the last scan found missing, in operand order.
    pending: Vec<Frame>,
}

fn unravel(mut i: usize, st: &[usize]) -> Vec<usize> {
    st.iter()
        .map(|s| {
            let q = i / s;
            i %= s;
            q
        })
        .collect()
}

fn ravel(ix: &[usize], st: &[usize]) -> usize {
    ix.iter().zip(st).map(|(a, b)| a * b).sum()
}

/// The element of `in_shape` that output multi-index `o` (of rank ≥ `in_shape.len()`) reads under
/// numpy broadcasting — `eval.rs`'s `broadcast_map`, one element at a time.
fn broadcast_index(o: &[usize], in_shape: &[usize]) -> usize {
    let ist = strides(in_shape);
    let off = o.len() - in_shape.len();
    (0..in_shape.len()).map(|k| if in_shape[k] == 1 { 0 } else { o[off + k] * ist[k] }).sum()
}

fn fit(v: i128, dtype: DType, what: &str) -> DemandResult<i128> {
    if dtype.contains(v) { Ok(v) } else { fail(TirErrorKind::Overflow, format!("{what}: {v} does not fit {}", dtype.name())) }
}

fn checked(v: Option<i128>, what: &str) -> DemandResult<i128> {
    v.ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Overflow, format!("{what}: past i128"))))
}

fn operand_err<T>(msg: impl Into<String>) -> DemandResult<T> {
    fail(TirErrorKind::Operand, msg)
}

/// **A source's refusal is `Missing`, whatever the source's own reason** (spec 04b §9.4): the
/// evaluation never reads a class off the source, so two sources that refuse the same question for
/// different reasons fail the evaluation the same way.
fn refused(e: TirError) -> DemandError {
    DemandError::Tir(TirError::new(TirErrorKind::Missing, format!("the source refused: {} ({:?})", e.msg, e.kind)))
}

impl Engine<'_, '_> {
    /// The index of context `key`, created on first use.
    fn context(&mut self, key: DemandContext) -> DemandResult<usize> {
        if let Some(i) = self.contexts.get(&key) {
            return Ok(*i);
        }
        if key.pos >= self.program.history_bound {
            return fail(TirErrorKind::Position, "position ≥ history_bound");
        }
        let block = occurrence_block(self.program, key.occurrence)
            .ok_or_else(|| TirError::new(TirErrorKind::Malformed, format!("no occurrence {}", key.occurrence)))?;
        let layers = self.program.schedule.layers.len();
        let layer = (key.occurrence >= 1 && (key.occurrence as usize) <= layers).then(|| key.occurrence - 1);
        let h = history_length_v1(self.info, block, key.pos)
            .ok_or_else(|| TirError::new(TirErrorKind::Malformed, "the program's info does not cover the block"))?;
        let b = &self.program.blocks[block as usize];
        let shapes: Vec<Vec<usize>> = b.nodes.iter().map(|n| n.out.resolve(h)).collect();
        let counts = shapes.iter().map(|s| element_count(s)).collect();
        self.ctxs.push(Ctx { key, block, layer, h, shapes, counts, memo: vec![BTreeMap::new(); b.nodes.len()] });
        self.contexts.insert(key, self.ctxs.len() - 1);
        Ok(self.ctxs.len() - 1)
    }

    fn tick(&mut self, elements: u64, terms: u64) -> DemandResult<()> {
        self.work.elements = self.work.elements.saturating_add(elements);
        self.work.terms = self.work.terms.saturating_add(terms);
        if self.work.elements > self.limits.max_elements || self.work.terms > self.limits.max_terms {
            return Err(DemandError::WorkLimit(self.work));
        }
        Ok(())
    }

    fn push(&mut self, frame: Frame) -> DemandResult<()> {
        self.pushes += 1;
        if self.pushes > self.max_pushes {
            return Err(DemandError::WorkLimit(self.work));
        }
        self.pending.push(frame);
        Ok(())
    }

    fn is_leaf(&self, ci: usize, node: u16) -> bool {
        let c = &self.ctxs[ci];
        let committed = self.program.blocks[c.block as usize].nodes.get(node as usize).is_some_and(|n| n.commit);
        committed && self.target != Some((c.key, node))
    }

    fn memo_get(&self, f: &Frame) -> Option<i128> {
        self.ctxs[f.ctx].memo[f.node as usize].get(&f.index).copied()
    }

    /// Compute `root` and everything it needs.
    fn run(&mut self, root: Frame) -> DemandResult<()> {
        let mut stack = vec![root];
        while let Some(top) = stack.last().copied() {
            if self.memo_get(&top).is_some() {
                stack.pop();
                continue;
            }
            let mut frame = top;
            self.pending.clear();
            match self.step(&mut frame)? {
                Some(v) => {
                    self.ctxs[frame.ctx].memo[frame.node as usize].insert(frame.index, v);
                    stack.pop();
                }
                None => {
                    if self.pending.is_empty() {
                        return fail(TirErrorKind::Malformed, "an element waits on nothing");
                    }
                    *stack.last_mut().expect("non-empty") = frame;
                    // The first missing operand on top, so frames resolve in operand order.
                    stack.extend(self.pending.drain(..).rev());
                }
            }
        }
        Ok(())
    }

    /// The concrete shape of an operand in context `ci`.
    fn operand_shape(&self, ci: usize, r: Ref) -> DemandResult<Vec<usize>> {
        let p = self.program;
        let c = &self.ctxs[ci];
        let bad = || DemandError::Tir(TirError::new(TirErrorKind::Operand, format!("{r:?} names nothing")));
        Ok(match r {
            Ref::Node(j) => c.shapes.get(j as usize).ok_or_else(bad)?.clone(),
            Ref::CarryIn(k) => p.blocks[c.block as usize].carry_in.get(k as usize).ok_or_else(bad)?.resolve(c.h),
            Ref::Param(j) => p.params.get(j as usize).ok_or_else(bad)?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Const(j) => p.consts.get(j as usize).ok_or_else(bad)?.shape.iter().map(|d| *d as usize).collect(),
            Ref::State(j) => p.states.get(j as usize).ok_or_else(bad)?.shape.iter().map(|d| *d as usize).collect(),
            Ref::Input(_) => Vec::new(),
        })
    }

    /// A committed node's element, from the source, checked against its declaration and memoised.
    fn leaf(&mut self, ci: usize, node: u16, index: usize) -> DemandResult<i128> {
        let count = *self.ctxs[ci]
            .counts
            .get(node as usize)
            .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Operand, "no such node")))?;
        if index >= count {
            return fail(TirErrorKind::Index, format!("node {node}: element {index} outside {count}"));
        }
        let f = Frame { ctx: ci, node, index, charged: false };
        if let Some(v) = self.memo_get(&f) {
            return Ok(v);
        }
        let key = self.ctxs[ci].key;
        let dtype = self.program.blocks[self.ctxs[ci].block as usize].nodes[node as usize].out.dtype;
        let v = self.source.node(key, node, index).map_err(refused)?;
        if !dtype.contains(v) {
            return operand_err(format!("supplied node {node}: {v} is not a {}", dtype.name()));
        }
        self.ctxs[ci].memo[node as usize].insert(index, v);
        Ok(v)
    }

    /// Element `index` of node `node` of context `ci`: known, a leaf, or a frame to compute.
    fn node_value(&mut self, ci: usize, node: u16, index: usize) -> DemandResult<Fetch> {
        let count = *self.ctxs[ci]
            .counts
            .get(node as usize)
            .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Operand, "no such node")))?;
        if index >= count {
            return fail(TirErrorKind::Index, format!("node {node}: element {index} outside {count}"));
        }
        let f = Frame { ctx: ci, node, index, charged: false };
        if let Some(v) = self.memo_get(&f) {
            return Ok(Fetch::Ready(v));
        }
        if self.is_leaf(ci, node) {
            return Ok(Fetch::Ready(self.leaf(ci, node, index)?));
        }
        Ok(Fetch::Need(f))
    }

    /// `Fixed` instance `(state, layer)` element `index` at the START of position `pos`.
    fn state_at_start(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> DemandResult<Fetch> {
        if let Some(v) = self.state_memo.get(&(pos, state, layer, index)) {
            return Ok(Fetch::Ready(*v));
        }
        let program = self.program;
        let s = &program.states[state as usize];
        if !matches!(s.kind, StateKind::Fixed { .. }) {
            return fail(TirErrorKind::Shape, "a State ref names a Hist state");
        }
        let mut at = pos;
        loop {
            match self.source.state(at, state, layer, index).map_err(refused)? {
                StateSupply::Value(v) => {
                    let v = self.check_state_value(state, v)?;
                    self.state_memo.insert((pos, state, layer, index), v);
                    return Ok(Fetch::Ready(v));
                }
                StateSupply::Replay => {
                    if at == 0 {
                        return fail(TirErrorKind::Missing, format!("state {}: nothing precedes position 0", s.name));
                    }
                    match state_writer_v1(program, state, layer) {
                        Some((occ, w)) => {
                            let ci = self.context(DemandContext { pos: at - 1, occurrence: occ })?;
                            return match self.node_value(ci, w, index)? {
                                Fetch::Ready(v) => {
                                    let v = self.check_state_value(state, v)?;
                                    self.state_memo.insert((pos, state, layer, index), v);
                                    Ok(Fetch::Ready(v))
                                }
                                need => Ok(need),
                            };
                        }
                        None => {
                            // Unwritten at `at − 1`: its value there is its value at the start of it.
                            // One term for each position the walk passes, whether or not it ever
                            // finds a value (§9.4, "Work").
                            self.tick(0, 1)?;
                            at -= 1;
                        }
                    }
                }
            }
        }
    }

    /// `Fixed` instance `(state, layer)` element `index` AFTER position `pos` — the writer's output
    /// there, which must lie in `[lo, hi]` as every `Fixed` value the evaluation reads does (a computed
    /// write is clamped into it; a committed one is checked), or, for an unwritten instance, its value
    /// at the start of `pos`.
    fn state_after(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> DemandResult<Fetch> {
        match state_writer_v1(self.program, state, layer) {
            Some((occ, w)) => {
                let ci = self.context(DemandContext { pos, occurrence: occ })?;
                match self.node_value(ci, w, index)? {
                    Fetch::Ready(v) => Ok(Fetch::Ready(self.check_state_value(state, v)?)),
                    need => Ok(need),
                }
            }
            None => self.state_at_start(pos, state, layer, index),
        }
    }

    /// A `Fixed` state's value: of the state's dtype and in `[lo, hi]`, else `Operand`.
    fn check_state_value(&self, state: u16, v: i128) -> DemandResult<i128> {
        let s = &self.program.states[state as usize];
        let StateKind::Fixed { lo, hi } = s.kind else {
            return fail(TirErrorKind::Shape, "a State ref names a Hist state");
        };
        if !s.dtype.contains(v) || v < lo as i128 || v > hi as i128 {
            return operand_err(format!("state {}: {v} is outside [{lo}, {hi}]", s.name));
        }
        Ok(v)
    }

    /// One element of an operand of context `ci`, from wherever it lives.
    fn fetch(&mut self, ci: usize, r: Ref, index: usize) -> DemandResult<Fetch> {
        let p = self.program;
        let key = self.ctxs[ci].key;
        match r {
            Ref::Node(j) => self.node_value(ci, j, index),
            Ref::CarryIn(k) => {
                let declared = p.blocks[self.ctxs[ci].block as usize]
                    .carry_in
                    .get(k as usize)
                    .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Operand, "no such carry-in")))?
                    .dtype;
                let (prev, node) = carry_in_node_v1(p, key, k)
                    .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Operand, "the carry-in names no carry-out")))?;
                let pi = self.context(prev)?;
                let v = self.leaf(pi, node, index)?;
                if !declared.contains(v) {
                    return operand_err(format!("carry-in {k}: {v} is not a {}", declared.name()));
                }
                Ok(Fetch::Ready(v))
            }
            Ref::Param(j) => {
                let d =
                    p.params.get(j as usize).ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Operand, "no such param")))?;
                let layer = if d.per_layer { self.ctxs[ci].layer } else { None };
                let v = self.source.param(j, layer, index).map_err(refused)?;
                if !d.dtype.contains(v) {
                    return operand_err(format!("param {}: {v} is not a {}", d.name, d.dtype.name()));
                }
                Ok(Fetch::Ready(v))
            }
            Ref::Const(j) => {
                let c =
                    p.consts.get(j as usize).ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Operand, "no such const")))?;
                let w = c.dtype.width();
                let bytes = index
                    .checked_mul(w)
                    .and_then(|at| c.data.get(at..at + w))
                    .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Index, "const element out of range")))?;
                Ok(Fetch::Ready(c.dtype.decode_le(bytes)))
            }
            Ref::State(j) => {
                let s =
                    p.states.get(j as usize).ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Operand, "no such state")))?;
                let layer = if s.per_layer { self.ctxs[ci].layer } else { None };
                self.state_at_start(key.pos, j, layer, index)
            }
            Ref::Input(j) => {
                if j == INPUT_TOKEN {
                    let t = self.source.token(key.pos).map_err(refused)?;
                    if t >= p.token_bound {
                        return operand_err(format!("token {t} ≥ token_bound {}", p.token_bound));
                    }
                    Ok(Fetch::Ready(t as i128))
                } else {
                    Ok(Fetch::Ready(key.pos as i128))
                }
            }
        }
    }

    /// Fetch every listed operand element; push a frame for each one that must be computed first.
    /// `None` when anything was pushed.
    fn fetch_all(&mut self, ci: usize, reqs: &[(Ref, usize)]) -> DemandResult<Option<Vec<i128>>> {
        let mut out = Vec::with_capacity(reqs.len());
        let mut missing = false;
        for (r, i) in reqs {
            match self.fetch(ci, *r, *i)? {
                Fetch::Ready(v) => out.push(v),
                Fetch::Need(f) => {
                    self.push(f)?;
                    missing = true;
                }
            }
        }
        Ok(if missing { None } else { Some(out) })
    }

    /// One operand element, or `None` after pushing the frame that computes it.
    fn fetch_one(&mut self, ci: usize, r: Ref, index: usize) -> DemandResult<Option<i128>> {
        match self.fetch(ci, r, index)? {
            Fetch::Ready(v) => Ok(Some(v)),
            Fetch::Need(f) => {
                self.push(f)?;
                Ok(None)
            }
        }
    }

    /// The reduction terms one element of `prim` costs.
    fn terms_of(&self, ci: usize, prim: &Prim, inputs: &[Ref]) -> DemandResult<u64> {
        Ok(match prim {
            Prim::MatMul => {
                let xs = self.operand_shape(ci, inputs[0])?;
                xs.last().copied().unwrap_or(0) as u64
            }
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => {
                let xs = self.operand_shape(ci, inputs[0])?;
                xs.get(*axis as usize).copied().unwrap_or(0) as u64
            }
            _ => 0,
        })
    }

    /// Compute `f`, or push what it needs and return `None`.
    fn step(&mut self, f: &mut Frame) -> DemandResult<Option<i128>> {
        let program = self.program;
        let ci = f.ctx;
        let block = &program.blocks[self.ctxs[ci].block as usize];
        let node = &block.nodes[f.node as usize];
        let prim = &node.prim;
        let inputs = &node.inputs[..];
        let name = prim.name();
        let (lo, hi) = prim.arity();
        if inputs.len() < lo || inputs.len() > hi {
            return fail(TirErrorKind::Shape, format!("{name}: {} inputs", inputs.len()));
        }
        if !f.charged {
            let terms = self.terms_of(ci, prim, inputs)?;
            self.tick(1, terms)?;
            f.charged = true;
        }
        let out_dtype = node.out.dtype;
        let out_shape = self.ctxs[ci].shapes[f.node as usize].clone();
        let ost = strides(&out_shape);
        let index = f.index;
        match prim {
            Prim::Reshape | Prim::Cast => {
                let Some(v) = self.fetch_one(ci, inputs[0], index)? else { return Ok(None) };
                fit(v, out_dtype, name).map(Some)
            }
            Prim::Clamp { lo, hi } => {
                let Some(v) = self.fetch_one(ci, inputs[0], index)? else { return Ok(None) };
                Ok(Some(v.clamp(*lo as i128, *hi as i128)))
            }
            Prim::Log2Floor | Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
                let Some(v) = self.fetch_one(ci, inputs[0], index)? else { return Ok(None) };
                let r = match prim {
                    Prim::Log2Floor => log2_floor(v),
                    Prim::IntExp => int_exp(v),
                    Prim::IntRsqrt => int_rsqrt(v),
                    _ => int_ln(v),
                };
                fit(r, out_dtype, name).map(Some)
            }
            Prim::StateWrite { state } => {
                let s = program
                    .states
                    .get(*state as usize)
                    .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Shape, "StateWrite names no state")))?;
                let StateKind::Fixed { lo, hi } = s.kind else {
                    return fail(TirErrorKind::Shape, "StateWrite on a Hist state");
                };
                let Some(v) = self.fetch_one(ci, inputs[0], index)? else { return Ok(None) };
                Ok(Some(v.clamp(lo as i128, hi as i128)))
            }
            Prim::Transpose { perm } => {
                let ishape = self.operand_shape(ci, inputs[0])?;
                let o = unravel(index, &ost);
                let mut j = vec![0usize; o.len()];
                for (k, p) in perm.iter().enumerate() {
                    j[*p as usize] = o[k];
                }
                self.fetch_one(ci, inputs[0], ravel(&j, &strides(&ishape)))
            }
            Prim::Slice { axis, start } => {
                let ishape = self.operand_shape(ci, inputs[0])?;
                let mut o = unravel(index, &ost);
                o[*axis as usize] += *start as usize;
                self.fetch_one(ci, inputs[0], ravel(&o, &strides(&ishape)))
            }
            Prim::Concat { axis } => {
                let a = *axis as usize;
                let mut o = unravel(index, &ost);
                for r in inputs {
                    let t = self.operand_shape(ci, *r)?;
                    if o[a] < t[a] {
                        return self.fetch_one(ci, *r, ravel(&o, &strides(&t)));
                    }
                    o[a] -= t[a];
                }
                fail(TirErrorKind::Shape, "Concat: the inputs do not cover the output")
            }
            Prim::Broadcast => {
                let ishape = self.operand_shape(ci, inputs[0])?;
                let o = unravel(index, &ost);
                self.fetch_one(ci, inputs[0], broadcast_index(&o, &ishape))
            }
            Prim::Iota { axis, start, step } => {
                let o = unravel(index, &ost);
                fit(*start as i128 + (*step as i128) * (o[*axis as usize] as i128), out_dtype, name).map(Some)
            }
            Prim::Gather { axis, batch_dims } => {
                let (a, bd) = (*axis as usize, *batch_dims as usize);
                let dshape = self.operand_shape(ci, inputs[0])?;
                let xshape = self.operand_shape(ci, inputs[1])?;
                let m = xshape.len() - bd;
                let o = unravel(index, &ost);
                let mut xi: Vec<usize> = o[..bd].to_vec();
                xi.extend_from_slice(&o[a..a + m]);
                let Some(v) = self.fetch_one(ci, inputs[1], ravel(&xi, &strides(&xshape)))? else { return Ok(None) };
                if v < 0 || v >= dshape[a] as i128 {
                    return fail(TirErrorKind::Index, format!("Gather: index {v} outside [0, {})", dshape[a]));
                }
                let mut di: Vec<usize> = o[..a].to_vec();
                di.push(v as usize);
                di.extend_from_slice(&o[a + m..]);
                self.fetch_one(ci, inputs[0], ravel(&di, &strides(&dshape)))
            }
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                let o = unravel(index, &ost);
                let ash = self.operand_shape(ci, inputs[0])?;
                let bsh = self.operand_shape(ci, inputs[1])?;
                let reqs = [(inputs[0], broadcast_index(&o, &ash)), (inputs[1], broadcast_index(&o, &bsh))];
                let Some(v) = self.fetch_all(ci, &reqs)? else { return Ok(None) };
                let (x, y) = (v[0], v[1]);
                match prim {
                    Prim::Add => fit(checked(x.checked_add(y), name)?, out_dtype, name).map(Some),
                    Prim::Sub => fit(checked(x.checked_sub(y), name)?, out_dtype, name).map(Some),
                    Prim::Mul => fit(checked(x.checked_mul(y), name)?, out_dtype, name).map(Some),
                    Prim::Div { rule } => {
                        let q = div_round(x, y, *rule)
                            .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Divisor, format!("Div: divisor {y} < 1"))))?;
                        fit(q, out_dtype, name).map(Some)
                    }
                    Prim::Compare { cmp } => Ok(Some(cmp.holds(x, y) as i128)),
                    _ => unreachable!("matched above"),
                }
            }
            Prim::Select => {
                let o = unravel(index, &ost);
                let csh = self.operand_shape(ci, inputs[0])?;
                let Some(c) = self.fetch_one(ci, inputs[0], broadcast_index(&o, &csh))? else { return Ok(None) };
                let chosen = if c != 0 { inputs[1] } else { inputs[2] };
                let sh = self.operand_shape(ci, chosen)?;
                let Some(v) = self.fetch_one(ci, chosen, broadcast_index(&o, &sh))? else { return Ok(None) };
                fit(v, out_dtype, name).map(Some)
            }
            Prim::MatMul => {
                let xs = self.operand_shape(ci, inputs[0])?;
                let ys = self.operand_shape(ci, inputs[1])?;
                let (xr, yr, or) = (xs.len(), ys.len(), out_shape.len());
                let (m, kk, nn) = (xs[xr - 2], xs[xr - 1], ys[yr - 1]);
                let o = unravel(index, &ost);
                let (r, c) = (o[or - 2], o[or - 1]);
                let batch = &o[..or - 2];
                let xb = broadcast_index(batch, &xs[..xr - 2]);
                let yb = broadcast_index(batch, &ys[..yr - 2]);
                let (xo, yo) = (xb * m * kk + r * kk, yb * kk * nn + c);
                let mut reqs = Vec::with_capacity(2 * kk);
                for t in 0..kk {
                    reqs.push((inputs[0], xo + t));
                    reqs.push((inputs[1], yo + t * nn));
                }
                let Some(v) = self.fetch_all(ci, &reqs)? else { return Ok(None) };
                let (mut pos, mut neg) = (0i128, 0i128);
                for pair in v.chunks_exact(2) {
                    let term = checked(pair[0].checked_mul(pair[1]), name)?;
                    order_free_add(&mut pos, &mut neg, term, out_dtype, name)?;
                }
                Ok(Some(pos + neg))
            }
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
                let a = *axis as usize;
                let xs = self.operand_shape(ci, inputs[0])?;
                let ist = strides(&xs);
                let o = unravel(index, &ost);
                let reqs: Vec<(Ref, usize)> = (0..xs[a])
                    .map(|t| {
                        let mut j = o.clone();
                        j[a] = t;
                        (inputs[0], ravel(&j, &ist))
                    })
                    .collect();
                let Some(v) = self.fetch_all(ci, &reqs)? else { return Ok(None) };
                if matches!(prim, Prim::ReduceSum { .. }) {
                    let (mut pos, mut neg) = (0i128, 0i128);
                    for t in v {
                        order_free_add(&mut pos, &mut neg, t, out_dtype, name)?;
                    }
                    Ok(Some(pos + neg))
                } else {
                    v.into_iter()
                        .max()
                        .map(Some)
                        .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Shape, "ReduceMax over an empty axis")))
                }
            }
            Prim::TopK { axis, k } => {
                let a = *axis as usize;
                let xs = self.operand_shape(ci, inputs[0])?;
                let ist = strides(&xs);
                let mut o = unravel(index, &ost);
                let slot = o[a];
                o[a] = 0;
                let reqs: Vec<(Ref, usize)> = (0..xs[a])
                    .map(|t| {
                        let mut j = o.clone();
                        j[a] = t;
                        (inputs[0], ravel(&j, &ist))
                    })
                    .collect();
                let Some(row) = self.fetch_all(ci, &reqs)? else { return Ok(None) };
                let k = *k as usize;
                if k > row.len() {
                    return fail(TirErrorKind::Shape, "TopK: k exceeds the axis");
                }
                let mut order: Vec<usize> = (0..row.len()).collect();
                order.sort_by(|p, q| row[*q].cmp(&row[*p]).then(p.cmp(q)));
                let mut chosen: Vec<usize> = order[..k].to_vec();
                chosen.sort_unstable();
                // Every slot of the row is now known; memoise them all, one selection per row.
                for (s, idx) in chosen.iter().enumerate() {
                    let mut j = o.clone();
                    j[a] = s;
                    self.ctxs[ci].memo[f.node as usize].insert(ravel(&j, &ost), *idx as i128);
                }
                chosen
                    .get(slot)
                    .map(|v| Some(*v as i128))
                    .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Index, "TopK slot")))
            }
            Prim::HistAppend { state } => {
                let s = program
                    .states
                    .get(*state as usize)
                    .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Shape, "HistAppend names no state")))?;
                let StateKind::Hist { .. } = s.kind else {
                    return fail(TirErrorKind::Shape, "HistAppend on a Fixed state");
                };
                let row_len: usize = s.shape.iter().map(|d| *d as usize).product();
                if row_len == 0 {
                    return fail(TirErrorKind::Shape, "HistAppend of an empty row");
                }
                let (t, within) = (index / row_len, index % row_len);
                let (h, pos) = (self.ctxs[ci].h, self.ctxs[ci].key.pos);
                if t + 1 == h {
                    self.fetch_one(ci, inputs[0], within)
                } else {
                    // Row `t` of the `H` rows is the one appended at `pos + 1 − H + t`.
                    let row_pos = (pos as usize + 1 - h + t) as u32;
                    let layer = if s.per_layer { self.ctxs[ci].layer } else { None };
                    let v = self.source.hist_row(pos, *state, layer, row_pos, within).map_err(refused)?;
                    if !s.dtype.contains(v) {
                        return operand_err(format!("history {}: {v} is not a {}", s.name, s.dtype.name()));
                    }
                    Ok(Some(v))
                }
            }
        }
    }
}

/// One term of an order-free sum (PALW-TIR-24): the positive and the negative partial sums are
/// the extremes over every order and grouping, and each must stay inside the declared type.
fn order_free_add(pos: &mut i128, neg: &mut i128, t: i128, dtype: DType, what: &str) -> DemandResult<()> {
    if t > 0 {
        *pos = checked(pos.checked_add(t), what)?;
        if *pos > dtype.max_value() {
            return fail(TirErrorKind::Overflow, format!("{what}: a partial sum reaches {pos} > {}", dtype.max_value()));
        }
    } else {
        *neg = checked(neg.checked_add(t), what)?;
        if *neg < dtype.min_value() {
            return fail(TirErrorKind::Overflow, format!("{what}: a partial sum reaches {neg} < {}", dtype.min_value()));
        }
    }
    Ok(())
}

/// A [`DemandSource`] over in-memory values — for tools and the differential tests. Every request is
/// recorded, so a caller can compare the set it had to supply with the set it did.
#[derive(Clone, Debug, Default)]
pub struct MapSource {
    /// Tokens by position.
    pub tokens: BTreeMap<u32, u32>,
    /// Committed node values by `(context, node)` (full tensors, row-major at the context's `H`).
    pub nodes: BTreeMap<(DemandContext, u16), Vec<i128>>,
    /// Param instances by `(param, layer)`.
    pub params: BTreeMap<(u16, Option<u16>), Vec<i128>>,
    /// `Fixed` values at the START of a position, by `(pos, state, layer)`. A position without an
    /// entry answers [`StateSupply::Replay`].
    pub states: BTreeMap<(u32, u16, Option<u16>), Vec<i128>>,
    /// `Hist` rows by `(state, layer, row position)`.
    pub hist_rows: BTreeMap<(u16, Option<u16>, u32), Vec<i128>>,
    /// Questions the source refuses, and the reason it gives (which the evaluation does not read:
    /// every refusal is `Missing`).
    pub withheld: BTreeMap<MapSourceKey, TirErrorKind>,
    /// Every request served, in order.
    pub requests: Vec<MapSourceRequest>,
}

/// A question a [`MapSource`] can be told to refuse: a request without its element index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MapSourceKey {
    Node { ctx: DemandContext, node: u16 },
    Param { param: u16, layer: Option<u16> },
    State { pos: u32, state: u16, layer: Option<u16> },
    HistRow { pos: u32, state: u16, layer: Option<u16>, row_pos: u32 },
    Token { pos: u32 },
}

/// One request a [`MapSource`] served.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MapSourceRequest {
    Node { ctx: DemandContext, node: u16, index: usize },
    Param { param: u16, layer: Option<u16>, index: usize },
    State { pos: u32, state: u16, layer: Option<u16>, index: usize },
    HistRow { pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize },
    Token { pos: u32 },
}

fn missing<T>(what: String) -> TirResult<T> {
    Err(TirError::new(TirErrorKind::Missing, what))
}

impl MapSource {
    fn withhold(&self, key: MapSourceKey) -> TirResult<()> {
        match self.withheld.get(&key) {
            Some(kind) => Err(TirError::new(*kind, format!("{key:?} is withheld"))),
            None => Ok(()),
        }
    }
}

impl DemandSource for MapSource {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        self.requests.push(MapSourceRequest::Node { ctx, node, index });
        self.withhold(MapSourceKey::Node { ctx, node })?;
        match self.nodes.get(&(ctx, node)).and_then(|v| v.get(index)) {
            Some(v) => Ok(*v),
            None => missing(format!("{ctx:?} node {node} element {index}")),
        }
    }
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        self.requests.push(MapSourceRequest::Param { param, layer, index });
        self.withhold(MapSourceKey::Param { param, layer })?;
        match self.params.get(&(param, layer)).and_then(|v| v.get(index)) {
            Some(v) => Ok(*v),
            None => missing(format!("param {param} layer {layer:?} element {index}")),
        }
    }
    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply> {
        self.requests.push(MapSourceRequest::State { pos, state, layer, index });
        self.withhold(MapSourceKey::State { pos, state, layer })?;
        match self.states.get(&(pos, state, layer)) {
            None => Ok(StateSupply::Replay),
            Some(v) => match v.get(index) {
                Some(v) => Ok(StateSupply::Value(*v)),
                None => missing(format!("state {state} layer {layer:?} at {pos} element {index}")),
            },
        }
    }
    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        self.requests.push(MapSourceRequest::HistRow { pos, state, layer, row_pos, index });
        self.withhold(MapSourceKey::HistRow { pos, state, layer, row_pos })?;
        match self.hist_rows.get(&(state, layer, row_pos)).and_then(|r| r.get(index)) {
            Some(v) => Ok(*v),
            None => missing(format!("history {state} layer {layer:?} row {row_pos} element {index}")),
        }
    }
    fn token(&mut self, pos: u32) -> TirResult<u32> {
        self.requests.push(MapSourceRequest::Token { pos });
        self.withhold(MapSourceKey::Token { pos })?;
        self.tokens.get(&pos).copied().ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("token at {pos}")))
    }
}
