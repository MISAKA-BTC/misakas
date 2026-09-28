//! Demand evaluation, `eval_demanded` (04b §9.4, PALW-TIR-30), written from the text alone.
//!
//! One element of one context at a time, depth first, with an explicit stack of frames (no native
//! recursion, so a long chain or a replay across many positions costs no native stack). A frame is
//! one computed element — for `TopK`, one row — and is charged when it is first scanned, before any
//! operand is fetched; it then reads its operand elements in the order of §9.4's index-map table,
//! pushing a frame for any computed element that has no value yet and reading again once that frame
//! is done. Every element, leaf or computed, is evaluated at most once per request.
//!
//! [`eval_demanded`] stops at the first failure. [`demand_outcomes`] evaluates everything the
//! evaluation can reach without stopping and returns the SET of outcomes the text allows — every
//! failing element's class, and `WorkLimit` when the work exceeds the limits — for comparing
//! failures with another implementation, whose class the text leaves free among those ("the class of
//! a failing element or `WorkLimit`").
//!
//! Readings the text leaves open are marked `reading:` and listed as the D-series in
//! `docs/design/palw/tir/ref2-findings.md`.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::error::Class;
use crate::normal_form::{block_window, ref_type};
use crate::prims::{OrderFree, cmp_holds, divide, fit, fit_i};
use crate::program::{Node, Prim, Program, Ref, StateKind};
use crate::tensor::{Tensor, count, ravel, unravel};
use crate::transcendental::{int_exp, int_ln, int_rsqrt, log2_floor};
use crate::types::{DType, Dim};
use crate::wide::Wide;

/// A context `(p, o)`: a position and an index into the occurrence list of §3.3 (`0` is `pre`,
/// `1 + l` is layer `l`, `L + 1` is `post`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ctx {
    pub pos: u64,
    pub occ: u32,
}

/// The request's target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// `node(p, o, n)`.
    Node { ctx: Ctx, node: u16 },
    /// `state_after(p, j, l)`: `Fixed` state `j`'s instance at layer `l` after position `p`.
    StateAfter { pos: u64, state: u16, layer: Option<u32> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_elements: u64,
    pub max_terms: u64,
}

impl Limits {
    pub const UNLIMITED: Limits = Limits { max_elements: u64::MAX, max_terms: u64::MAX };

    /// The frame cap §9.4 describes for the reference evaluator: `6 · max_terms + 24 · max_elements`
    /// (saturating). Not part of the boundary.
    pub fn frame_cap(&self) -> u64 {
        self.max_terms.saturating_mul(6).saturating_add(self.max_elements.saturating_mul(24))
    }
}

/// The work of §9.4: computed elements (a `TopK` row counts once) and terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub elements: u64,
    pub terms: u64,
}

impl Work {
    pub fn within(&self, l: &Limits) -> bool {
        self.elements <= l.max_elements && self.terms <= l.max_terms
    }
}

/// The answer to a `state` question: a value, or `Replay`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Supply {
    Value(i128),
    Replay,
}

/// The five questions of §9.4; `None` is a refusal. Nothing else is ever read.
pub trait Source {
    /// Element `i` of committed node `node` of context `ctx`.
    fn node(&mut self, ctx: Ctx, node: u16, i: u64) -> Option<i128>;
    /// Element `i` of param `param`'s instance at `layer`.
    fn param(&mut self, param: u16, layer: Option<u32>, i: u64) -> Option<i128>;
    /// Element `i` of `Fixed` instance `(state, layer)` at the START of `pos`.
    fn state(&mut self, pos: u64, state: u16, layer: Option<u32>, i: u64) -> Option<Supply>;
    /// Element `i` of the row appended to `Hist` instance `(state, layer)` at `row_pos`, as read at
    /// `pos`.
    fn hist_row(&mut self, pos: u64, state: u16, layer: Option<u32>, row_pos: u64, i: u64) -> Option<i128>;
    /// The token of position `pos`.
    fn token(&mut self, pos: u64) -> Option<u64>;
}

/// A question with its arguments. The derived order is the golden vectors' grouping order: by
/// question (`node`, `param`, `state`, `hist_row`, `token`), then by argument, a null layer first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Question {
    Node { ctx: Ctx, node: u16, i: u64 },
    Param { param: u16, layer: Option<u32>, i: u64 },
    State { pos: u64, state: u16, layer: Option<u32>, i: u64 },
    HistRow { pos: u64, state: u16, layer: Option<u32>, row_pos: u64, i: u64 },
    Token { pos: u64 },
}

impl Question {
    /// The question with its element index zeroed (the grouping key), and the index (none for a
    /// token).
    pub fn split(&self) -> (Question, Option<u64>) {
        match *self {
            Question::Node { ctx, node, i } => (Question::Node { ctx, node, i: 0 }, Some(i)),
            Question::Param { param, layer, i } => (Question::Param { param, layer, i: 0 }, Some(i)),
            Question::State { pos, state, layer, i } => (Question::State { pos, state, layer, i: 0 }, Some(i)),
            Question::HistRow { pos, state, layer, row_pos, i } => {
                (Question::HistRow { pos, state, layer, row_pos, i: 0 }, Some(i))
            }
            Question::Token { pos } => (Question::Token { pos }, None),
        }
    }
}

/// A source that records every question asked of it — the evaluation's request set — and the order
/// they were first asked in (which is not part of the result).
pub struct Recorder<'a> {
    pub inner: &'a mut dyn Source,
    pub asked: BTreeSet<Question>,
    pub order: Vec<Question>,
}

impl<'a> Recorder<'a> {
    pub fn new(inner: &'a mut dyn Source) -> Self {
        Recorder { inner, asked: BTreeSet::new(), order: Vec::new() }
    }

    fn note(&mut self, q: Question) {
        if self.asked.insert(q) {
            self.order.push(q);
        }
    }
}

impl Source for Recorder<'_> {
    fn node(&mut self, ctx: Ctx, node: u16, i: u64) -> Option<i128> {
        self.note(Question::Node { ctx, node, i });
        self.inner.node(ctx, node, i)
    }
    fn param(&mut self, param: u16, layer: Option<u32>, i: u64) -> Option<i128> {
        self.note(Question::Param { param, layer, i });
        self.inner.param(param, layer, i)
    }
    fn state(&mut self, pos: u64, state: u16, layer: Option<u32>, i: u64) -> Option<Supply> {
        self.note(Question::State { pos, state, layer, i });
        self.inner.state(pos, state, layer, i)
    }
    fn hist_row(&mut self, pos: u64, state: u16, layer: Option<u32>, row_pos: u64, i: u64) -> Option<i128> {
        self.note(Question::HistRow { pos, state, layer, row_pos, i });
        self.inner.hist_row(pos, state, layer, row_pos, i)
    }
    fn token(&mut self, pos: u64) -> Option<u64> {
        self.note(Question::Token { pos });
        self.inner.token(pos)
    }
}

/// A failed evaluation: the class of a failing element (§9.3), or `WorkLimit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DemandError {
    Class(Class),
    WorkLimit,
}

impl DemandError {
    pub fn name(&self) -> &'static str {
        match self {
            DemandError::Class(c) => c.name(),
            DemandError::WorkLimit => "WorkLimit",
        }
    }
}

// ------------------------------------------------------------------ the program's geometry

/// The occurrences of §3.3 and each block's window.
struct Geo {
    occ: Vec<(usize, Option<u32>)>,
    windows: Vec<Option<u32>>,
}

impl Geo {
    fn new(p: &Program) -> Geo {
        let mut occ = vec![(p.schedule.pre as usize, None)];
        for (l, &b) in p.schedule.layers.iter().enumerate() {
            occ.push((b as usize, Some(l as u32)));
        }
        occ.push((p.schedule.post as usize, None));
        let windows = (0..p.blocks.len()).map(|b| block_window(p, b).ok().flatten()).collect();
        Geo { occ, windows }
    }

    fn block(&self, c: Ctx) -> usize {
        self.occ[c.occ as usize].0
    }

    /// The context's layer: `o − 1` for a layer occurrence, none otherwise.
    fn layer(&self, c: Ctx) -> Option<u32> {
        self.occ[c.occ as usize].1
    }

    /// `H = min(p + 1, W)`, and 1 in a block that appends to no history.
    fn h(&self, c: Ctx) -> u64 {
        match self.windows[self.block(c)] {
            Some(w) => c.pos.saturating_add(1).min(w as u64),
            None => 1,
        }
    }
}

/// The writer of `Fixed` instance `(j, l)`: the `StateWrite` of `j` in the block of occurrence
/// `1 + l` (per-layer) or `0` (global), as `(occurrence, node)`; `None` if nothing writes it.
fn writer(p: &Program, g: &Geo, j: u16, l: Option<u32>) -> Option<(u32, u16)> {
    let occ = match l {
        Some(l) => l.checked_add(1)?,
        None => 0,
    };
    let (b, _) = *g.occ.get(occ as usize)?;
    p.blocks[b].nodes.iter().position(|n| n.prim == Prim::StateWrite { state: j }).map(|w| (occ, w as u16))
}

/// Whether block `b` reads (`Ref::State`), writes (`StateWrite`) or appends to (`HistAppend`) state `j`.
fn block_references(p: &Program, b: usize, j: u16) -> bool {
    p.blocks[b].nodes.iter().any(|n| {
        matches!(n.prim, Prim::StateWrite { state } | Prim::HistAppend { state } if state == j) || n.inputs.contains(&Ref::State(j))
    })
}

/// §9.5.1: node `i` of block `b` reduces over `H` — a `ReduceSum` or `ReduceMax` whose operand's
/// declared shape has `H` at its axis, or a `MatMul` whose first operand's declared shape has `H`
/// last; the operand a `Node` or a `CarryIn` (any other ref has no `H`).
pub fn reduces_over_h(p: &Program, b: usize, i: usize) -> bool {
    let block = &p.blocks[b];
    let node = &block.nodes[i];
    let operand = |r: &Ref| -> Option<&crate::types::TensorType> {
        match *r {
            Ref::Node(j) => block.nodes.get(j as usize).map(|n| &n.out),
            Ref::CarryIn(k) => block.carry_in.get(k as usize),
            _ => None,
        }
    };
    match node.prim {
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            operand(&node.inputs[0]).is_some_and(|t| t.shape.get(axis as usize) == Some(&Dim::H))
        }
        Prim::MatMul => operand(&node.inputs[0]).is_some_and(|t| t.shape.last() == Some(&Dim::H)),
        _ => false,
    }
}

/// The request's refusals (§9.4, "refused before anything is read"): every class that applies,
/// empty when none does.
pub fn request_refusals(p: &Program, target: &Target, elements: &[u64]) -> BTreeSet<Class> {
    let mut out = BTreeSet::new();
    let layers = p.schedule.layers.len() as u64;
    match *target {
        Target::Node { ctx, node } => {
            if ctx.pos >= p.history_bound as u64 {
                out.insert(Class::Position);
            }
            if ctx.occ as u64 >= layers + 2 {
                out.insert(Class::Malformed);
                return out;
            }
            let g = Geo::new(p);
            let Some(n) = p.blocks[g.block(ctx)].nodes.get(node as usize) else {
                out.insert(Class::Malformed);
                return out;
            };
            let cnt = count(&n.out.extents(g.h(ctx)));
            if elements.iter().any(|&e| e >= cnt) {
                out.insert(Class::Malformed);
            }
        }
        Target::StateAfter { pos, state, layer } => {
            if pos >= p.history_bound as u64 {
                out.insert(Class::Position);
            }
            match p.states.get(state as usize) {
                Some(s) if matches!(s.kind, StateKind::Fixed { .. }) => {
                    // An instance a run holds: a global state's one instance always (normal form
                    // uses every declared state); a per-layer one only at a layer whose scheduled
                    // block reads, writes or appends to the state.
                    let has = match (s.per_layer, layer) {
                        (false, None) => true,
                        (true, Some(l)) => (l as u64) < layers && block_references(p, p.schedule.layers[l as usize] as usize, state),
                        _ => false,
                    };
                    if !has {
                        out.insert(Class::Malformed);
                    }
                    let cnt = count(&s.shape.iter().map(|&d| d as u64).collect::<Vec<_>>());
                    if elements.iter().any(|&e| e >= cnt) {
                        out.insert(Class::Malformed);
                    }
                }
                _ => {
                    out.insert(Class::Malformed);
                }
            }
        }
    }
    out
}

// ------------------------------------------------------------------ the evaluator

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct NKey {
    ctx: Ctx,
    node: u16,
    e: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SKey {
    pos: u64,
    state: u16,
    layer: Option<u32>,
    i: u64,
}

/// A memoized element: its value, or failed (only kept going in exploration).
#[derive(Clone, Copy, Debug)]
enum Slot {
    Val(i128),
    Bad,
}

/// A memoized value at the start of a position, with the position `q` the source answered it at.
#[derive(Clone, Copy, Debug)]
enum SSlot {
    Val(i128, u64),
    Bad,
}

/// The result of one read.
enum Got {
    Val(i128),
    Bad,
    /// A computed element without a value yet: push its frame, then read again.
    Push(NKey),
}

#[derive(Clone, Copy, Debug)]
enum Read {
    /// Operand `input` at its flat index `index`.
    Op { input: usize, index: u64 },
    /// `hist_row(p, j, l, row_pos, w)`.
    Hist { row_pos: u64, w: u64 },
}

enum Next {
    Read(Read),
    Done,
    /// §6.2's `Gather` index check failed: the data element is not read.
    IndexFail,
}

/// Why an evaluation stopped.
enum Stop {
    Fail(Class),
    Limit,
}

/// One computed element (a `TopK` row: `key.e` is the row's first slot).
struct Frame {
    key: NKey,
    scanned: bool,
    /// The output multi-index.
    o: Vec<u64>,
    /// Every operand's extents at the context's `H`.
    shapes: Vec<Vec<u64>>,
    step: usize,
    vals: Vec<i128>,
    bad: bool,
}

struct Ev<'p, 's> {
    p: &'p Program,
    g: Geo,
    consts: Vec<Vec<i128>>,
    src: &'s mut dyn Source,
    limits: Limits,
    explore: bool,
    work: Work,
    failures: BTreeSet<Class>,
    pushed: u64,
    memo: HashMap<NKey, Slot>,
    answers: HashMap<SKey, Option<Supply>>,
    smemo: HashMap<SKey, SSlot>,
    /// The distinct `(p, j, l, i)` needed of instances nothing writes (charged once each).
    needs: HashSet<SKey>,
    /// The target of a `node` request, which is computed even when committed.
    root: Option<(Ctx, u16)>,
    /// §9.5.2: nodes of the target's block that are leaves in the target's context.
    supplied: BTreeSet<u16>,
    /// §9.5.2: the history range the target reduces over, `[from, to)`.
    range: Option<(u64, u64)>,
    stack: Vec<Frame>,
}

fn got(s: Slot) -> Got {
    match s {
        Slot::Val(v) => Got::Val(v),
        Slot::Bad => Got::Bad,
    }
}

impl<'p, 's> Ev<'p, 's> {
    fn new(p: &'p Program, src: &'s mut dyn Source, limits: Limits, explore: bool, target: &Target) -> Self {
        let consts = p
            .consts
            .iter()
            .map(|c| {
                Tensor::from_le_bytes(c.dtype, c.shape.iter().map(|&d| d as u64).collect(), &c.data)
                    .map(|t| t.data)
                    .unwrap_or_default()
            })
            .collect();
        let root = match *target {
            Target::Node { ctx, node } => Some((ctx, node)),
            Target::StateAfter { .. } => None,
        };
        Ev {
            p,
            g: Geo::new(p),
            consts,
            src,
            limits,
            explore,
            work: Work::default(),
            failures: BTreeSet::new(),
            pushed: 0,
            memo: HashMap::new(),
            answers: HashMap::new(),
            smemo: HashMap::new(),
            needs: HashSet::new(),
            root,
            supplied: BTreeSet::new(),
            range: None,
            stack: Vec::new(),
        }
    }

    /// Whether frame key `k` is the target evaluated over a range.
    fn ranged(&self, k: &NKey) -> Option<(u64, u64)> {
        if self.root == Some((k.ctx, k.node)) { self.range } else { None }
    }

    /// A failing check: stop, or (exploring) record the class and go on.
    fn flag(&mut self, c: Class) -> Result<(), Stop> {
        if self.explore {
            self.failures.insert(c);
            Ok(())
        } else {
            Err(Stop::Fail(c))
        }
    }

    fn fail(&mut self, c: Class) -> Result<Got, Stop> {
        self.flag(c)?;
        Ok(Got::Bad)
    }

    /// Adds to the work; stops as soon as a count exceeds its limit.
    fn charge(&mut self, elements: u64, terms: u64) -> Result<(), Stop> {
        self.work.elements = self.work.elements.saturating_add(elements);
        self.work.terms = self.work.terms.saturating_add(terms);
        if !self.explore && !self.work.within(&self.limits) {
            return Err(Stop::Limit);
        }
        Ok(())
    }

    fn node_of(&self, k: &NKey) -> &'p Node {
        let p: &'p Program = self.p;
        &p.blocks[self.g.block(k.ctx)].nodes[k.node as usize]
    }

    /// §9.4 "Leaves": a commit point, except the target of a `node` request in its context.
    fn is_leaf(&self, k: &NKey) -> bool {
        if self.root == Some((k.ctx, k.node)) {
            return false;
        }
        self.node_of(k).commit || (self.root.is_some_and(|(c, _)| c == k.ctx) && self.supplied.contains(&k.node))
    }

    /// Element `k.e` of node `k.node` in `k.ctx`: memoized, a leaf, or a frame to push.
    fn node_elem(&mut self, k: NKey) -> Result<Got, Stop> {
        if let Some(&s) = self.memo.get(&k) {
            return Ok(got(s));
        }
        if self.is_leaf(&k) {
            let dt = self.node_of(&k).out.dtype;
            return self.leaf(k, dt);
        }
        Ok(Got::Push(k))
    }

    /// A leaf: `source.node(c, n, i)`, a value of `dt` (else `Operand`).
    fn leaf(&mut self, k: NKey, dt: DType) -> Result<Got, Stop> {
        if let Some(&s) = self.memo.get(&k) {
            return Ok(got(s));
        }
        match self.src.node(k.ctx, k.node, k.e) {
            Some(v) if dt.contains(v) => {
                self.memo.insert(k, Slot::Val(v));
                Ok(Got::Val(v))
            }
            a => {
                self.memo.insert(k, Slot::Bad);
                self.fail(if a.is_none() { Class::Missing } else { Class::Operand })
            }
        }
    }

    fn state_answer(&mut self, k: SKey) -> Option<Supply> {
        if let Some(&a) = self.answers.get(&k) {
            return a;
        }
        let a = self.src.state(k.pos, k.state, k.layer, k.i);
        self.answers.insert(k, a);
        a
    }

    /// §9.4 "Fixed-state replay": the value of instance `(j, l)` at the start of `p`, element `i`.
    fn state_at(&mut self, k: SKey) -> Result<Got, Stop> {
        let p: &'p Program = self.p;
        let s = &p.states[k.state as usize];
        let StateKind::Fixed { lo, hi } = s.kind else { unreachable!("State(j) names a Fixed state (NF-14)") };
        let (lo, hi) = (lo as i128, hi as i128);
        let Some((wocc, wnode)) = writer(p, &self.g, k.state, k.layer) else {
            return self.carried(k);
        };
        if let Some(&ss) = self.smemo.get(&k) {
            return Ok(match ss {
                SSlot::Val(v, _) => Got::Val(v),
                SSlot::Bad => Got::Bad,
            });
        }
        match self.state_answer(k) {
            None => {
                self.smemo.insert(k, SSlot::Bad);
                self.fail(Class::Missing)
            }
            Some(Supply::Value(v)) => {
                if s.dtype.contains(v) && lo <= v && v <= hi {
                    self.smemo.insert(k, SSlot::Val(v, k.pos));
                    Ok(Got::Val(v))
                } else {
                    self.smemo.insert(k, SSlot::Bad);
                    self.fail(Class::Operand)
                }
            }
            Some(Supply::Replay) => {
                if k.pos == 0 {
                    // Nothing precedes position 0.
                    self.smemo.insert(k, SSlot::Bad);
                    return self.fail(Class::Missing);
                }
                let wk = NKey { ctx: Ctx { pos: k.pos - 1, occ: wocc }, node: wnode, e: k.i };
                match self.node_elem(wk)? {
                    Got::Val(v) if lo <= v && v <= hi => {
                        self.smemo.insert(k, SSlot::Val(v, k.pos));
                        Ok(Got::Val(v))
                    }
                    Got::Val(_) => {
                        self.smemo.insert(k, SSlot::Bad);
                        self.fail(Class::Operand)
                    }
                    Got::Bad => {
                        self.smemo.insert(k, SSlot::Bad);
                        Ok(Got::Bad)
                    }
                    Got::Push(x) => Ok(Got::Push(x)),
                }
            }
        }
    }

    /// An instance nothing writes: the value at the start of `p`, carried unchanged from the
    /// largest `q ≤ p` at which the source answers a value, asking at `p`, `p − 1`, … in turn. A
    /// distinct need is charged `p − q` terms, one per position it is carried across, as the walk
    /// passes it (so a source that never answers is stopped by `max_terms`).
    fn carried(&mut self, k: SKey) -> Result<Got, Stop> {
        let p: &'p Program = self.p;
        let s = &p.states[k.state as usize];
        let StateKind::Fixed { lo, hi } = s.kind else { unreachable!() };
        let (lo, hi) = (lo as i128, hi as i128);
        let first = self.needs.insert(k);
        let mut q = k.pos;
        let mut walked: Vec<u64> = Vec::new();
        let found: SSlot = loop {
            let kq = SKey { pos: q, ..k };
            if let Some(&ss) = self.smemo.get(&kq) {
                if let SSlot::Val(_, from) = ss
                    && first
                {
                    self.charge(0, q - from)?;
                }
                break ss;
            }
            match self.state_answer(kq) {
                None => {
                    self.flag(Class::Missing)?;
                    break SSlot::Bad;
                }
                Some(Supply::Value(v)) => {
                    walked.push(q);
                    if s.dtype.contains(v) && lo <= v && v <= hi {
                        break SSlot::Val(v, q);
                    }
                    self.flag(Class::Operand)?;
                    break SSlot::Bad;
                }
                Some(Supply::Replay) => {
                    walked.push(q);
                    if q == 0 {
                        self.flag(Class::Missing)?;
                        break SSlot::Bad;
                    }
                    if first {
                        self.charge(0, 1)?;
                    }
                    q -= 1;
                }
            }
        };
        for w in walked {
            self.smemo.insert(SKey { pos: w, ..k }, found);
        }
        Ok(match found {
            SSlot::Val(v, _) => Got::Val(v),
            SSlot::Bad => Got::Bad,
        })
    }

    /// One read of element `k` (§9.4 "Operands").
    fn read(&mut self, k: &NKey, r: Read) -> Result<Got, Stop> {
        let p: &'p Program = self.p;
        let b = self.g.block(k.ctx);
        let node = &p.blocks[b].nodes[k.node as usize];
        match r {
            Read::Hist { row_pos, w } => {
                let Prim::HistAppend { state } = node.prim else { unreachable!() };
                let s = &p.states[state as usize];
                let l = if s.per_layer { self.g.layer(k.ctx) } else { None };
                match self.src.hist_row(k.ctx.pos, state, l, row_pos, w) {
                    Some(v) if s.dtype.contains(v) => Ok(Got::Val(v)),
                    None => self.fail(Class::Missing),
                    Some(_) => self.fail(Class::Operand),
                }
            }
            Read::Op { input, index } => match node.inputs[input] {
                Ref::Node(m) => self.node_elem(NKey { ctx: k.ctx, node: m, e: index }),
                Ref::CarryIn(kk) => {
                    // Node carry_out[k] of the previous occurrence's block in (p, o − 1): a commit
                    // point (NF-21), hence a leaf, of the carry-in's declared dtype.
                    let prev = Ctx { pos: k.ctx.pos, occ: k.ctx.occ - 1 };
                    let c = p.blocks[self.g.block(prev)].carry_out[kk as usize];
                    let dt = p.blocks[b].carry_in[kk as usize].dtype;
                    self.leaf(NKey { ctx: prev, node: c, e: index }, dt)
                }
                Ref::Param(j) => {
                    let d = &p.params[j as usize];
                    let l = if d.per_layer { self.g.layer(k.ctx) } else { None };
                    match self.src.param(j, l, index) {
                        Some(v) if d.dtype.contains(v) => Ok(Got::Val(v)),
                        None => self.fail(Class::Missing),
                        Some(_) => self.fail(Class::Operand),
                    }
                }
                Ref::Const(j) => Ok(Got::Val(self.consts[j as usize][index as usize])),
                Ref::State(j) => {
                    let s = &p.states[j as usize];
                    let l = if s.per_layer { self.g.layer(k.ctx) } else { None };
                    self.state_at(SKey { pos: k.ctx.pos, state: j, layer: l, i: index })
                }
                Ref::Input(0) => match self.src.token(k.ctx.pos) {
                    Some(t) if t < p.token_bound as u64 => Ok(Got::Val(t as i128)),
                    None => self.fail(Class::Missing),
                    Some(_) => self.fail(Class::Operand),
                },
                Ref::Input(_) => Ok(Got::Val(k.ctx.pos as i128)),
            },
        }
    }

    /// A `TopK` element's frame is its row's: the same multi-index with slot 0 on the axis.
    fn frame_key(&self, k: NKey) -> NKey {
        let node = self.node_of(&k);
        if let Prim::TopK { axis, .. } = node.prim {
            let shape = node.out.extents(self.g.h(k.ctx));
            let mut o = vec![0u64; shape.len()];
            unravel(k.e, &shape, &mut o);
            o[axis as usize] = 0;
            return NKey { e: ravel(&o, &shape), ..k };
        }
        k
    }

    fn push(&mut self, k: NKey) -> Result<(), Stop> {
        let key = self.frame_key(k);
        self.pushed += 1;
        if !self.explore && self.pushed > self.limits.frame_cap() {
            return Err(Stop::Limit);
        }
        self.stack.push(Frame { key, scanned: false, o: Vec::new(), shapes: Vec::new(), step: 0, vals: Vec::new(), bad: false });
        Ok(())
    }

    /// The terms of one computed element (§9.4 "Work"): `K` for a `MatMul`, the reduced extent `n`
    /// for a `ReduceSum`, a `ReduceMax` or a `TopK` row, and 0 otherwise.
    fn terms_of(&self, f: &Frame) -> u64 {
        if let Some((from, to)) = self.ranged(&f.key) {
            // §9.5.2: `to − from` terms per computed element of the target.
            return to - from;
        }
        match self.node_of(&f.key).prim {
            Prim::MatMul => *f.shapes[0].last().unwrap(),
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => f.shapes[0][axis as usize],
            _ => 0,
        }
    }

    fn scan(&self, f: &mut Frame) {
        let p: &'p Program = self.p;
        let b = self.g.block(f.key.ctx);
        let node = &p.blocks[b].nodes[f.key.node as usize];
        let h = self.g.h(f.key.ctx);
        let out = node.out.extents(h);
        f.o = vec![0u64; out.len()];
        unravel(f.key.e, &out, &mut f.o);
        f.shapes = node
            .inputs
            .iter()
            .map(|r| ref_type(p, b, f.key.node as usize, r).expect("an existing ref (NF-14)").extents(h))
            .collect();
    }

    /// The next read of frame `f` (§9.4 "Index maps", in the table's order).
    fn next(&self, f: &Frame) -> Next {
        let node = self.node_of(&f.key);
        let (o, s) = (&f.o, f.step);
        let rd = |input: usize, index: u64| Next::Read(Read::Op { input, index });
        // bc(o) into an operand of shape `sh`: aligned on the right, an extent of 1 read at 0.
        let bc = |sh: &[u64]| -> u64 {
            let off = o.len() - sh.len();
            let idx: Vec<u64> = sh.iter().enumerate().map(|(d, &ext)| if ext == 1 { 0 } else { o[d + off] }).collect();
            ravel(&idx, sh)
        };
        match &node.prim {
            Prim::Reshape
            | Prim::Cast
            | Prim::Clamp { .. }
            | Prim::Log2Floor
            | Prim::IntExp
            | Prim::IntRsqrt
            | Prim::IntLn
            | Prim::StateWrite { .. } => {
                if s == 0 {
                    rd(0, f.key.e)
                } else {
                    Next::Done
                }
            }
            Prim::Transpose { perm } => {
                if s > 0 {
                    return Next::Done;
                }
                let mut j = vec![0u64; o.len()];
                for (k, &pk) in perm.iter().enumerate() {
                    j[pk as usize] = o[k];
                }
                rd(0, ravel(&j, &f.shapes[0]))
            }
            Prim::Slice { axis, start } => {
                if s > 0 {
                    return Next::Done;
                }
                let mut j = o.clone();
                j[*axis as usize] += *start as u64;
                rd(0, ravel(&j, &f.shapes[0]))
            }
            Prim::Concat { axis } => {
                if s > 0 {
                    return Next::Done;
                }
                let a = *axis as usize;
                let mut at = o[a];
                for (q, sh) in f.shapes.iter().enumerate() {
                    if at < sh[a] {
                        let mut j = o.clone();
                        j[a] = at;
                        return rd(q, ravel(&j, sh));
                    }
                    at -= sh[a];
                }
                unreachable!("Concat extents sum to the output's (NF-16)")
            }
            Prim::Broadcast => {
                if s == 0 {
                    rd(0, bc(&f.shapes[0]))
                } else {
                    Next::Done
                }
            }
            Prim::Iota { .. } => Next::Done,
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                if s < 2 {
                    rd(s, bc(&f.shapes[s]))
                } else {
                    Next::Done
                }
            }
            Prim::Select => match s {
                0 => rd(0, bc(&f.shapes[0])),
                1 if !f.bad => {
                    // ONLY the chosen operand.
                    let q = if f.vals[0] != 0 { 1 } else { 2 };
                    rd(q, bc(&f.shapes[q]))
                }
                _ => Next::Done,
            },
            Prim::Gather { axis, batch_dims } => {
                let (a, b) = (*axis as usize, *batch_dims as usize);
                let m = f.shapes[1].len() - b;
                match s {
                    0 => {
                        let mut ii = o[..b].to_vec();
                        ii.extend_from_slice(&o[a..a + m]);
                        rd(1, ravel(&ii, &f.shapes[1]))
                    }
                    1 if !f.bad => {
                        let v = f.vals[0];
                        if !(0 <= v && v < f.shapes[0][a] as i128) {
                            return Next::IndexFail;
                        }
                        let mut dj = o[..a].to_vec();
                        dj.push(v as u64);
                        dj.extend_from_slice(&o[a + m..]);
                        rd(0, ravel(&dj, &f.shapes[0]))
                    }
                    _ => Next::Done,
                }
            }
            Prim::MatMul => {
                let (sa, sb) = (&f.shapes[0], &f.shapes[1]);
                let (from, to) = self.ranged(&f.key).unwrap_or((0, sa[sa.len() - 1]));
                let kk = (to - from) as usize;
                if s >= 2 * kk {
                    return Next::Done;
                }
                let t = from + (s / 2) as u64;
                let r = o.len();
                let beta = &o[..r - 2];
                let (row, col) = (o[r - 2], o[r - 1]);
                // The batch prefix mapped into an operand by broadcasting, then its last two indices.
                let map = |sh: &[u64], last: [u64; 2]| -> u64 {
                    let bsh = &sh[..sh.len() - 2];
                    let off = beta.len() - bsh.len();
                    let mut idx: Vec<u64> =
                        bsh.iter().enumerate().map(|(d, &ext)| if ext == 1 { 0 } else { beta[d + off] }).collect();
                    idx.extend_from_slice(&last);
                    ravel(&idx, sh)
                };
                if s % 2 == 0 { rd(0, map(sa, [row, t])) } else { rd(1, map(sb, [t, col])) }
            }
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => {
                let a = *axis as usize;
                let (from, to) = self.ranged(&f.key).unwrap_or((0, f.shapes[0][a]));
                if s as u64 >= to - from {
                    return Next::Done;
                }
                let mut j = o.clone();
                j[a] = from + s as u64;
                rd(0, ravel(&j, &f.shapes[0]))
            }
            Prim::HistAppend { .. } => {
                if s > 0 {
                    return Next::Done;
                }
                let rr = count(&f.shapes[0]);
                let (t, w) = (f.key.e / rr, f.key.e % rr);
                let h = self.g.h(f.key.ctx);
                if t == h - 1 {
                    rd(0, w)
                } else {
                    Next::Read(Read::Hist { row_pos: f.key.ctx.pos + 1 - h + t, w })
                }
            }
        }
    }

    /// The value of a computed element from the values it read (§6, element by element).
    fn value(&self, f: &Frame) -> Result<i128, Class> {
        let node = self.node_of(&f.key);
        let out = node.out.dtype;
        let v = &f.vals;
        let c = |e: crate::error::TirError| e.class;
        match &node.prim {
            Prim::Reshape
            | Prim::Transpose { .. }
            | Prim::Slice { .. }
            | Prim::Concat { .. }
            | Prim::Broadcast
            | Prim::Cast
            | Prim::HistAppend { .. } => fit_i(v[0], out).map_err(c),
            Prim::Gather { .. } | Prim::Select => fit_i(v[1], out).map_err(c),
            Prim::Iota { axis, start, step } => {
                let x = Wide::from_i128(*start as i128)
                    .checked_add(Wide::mul_i128(*step as i128, f.o[*axis as usize] as i128))
                    .unwrap_or(Wide::ZERO);
                fit(x, out).map_err(c)
            }
            Prim::Add => fit(Wide::from_i128(v[0]).checked_add(Wide::from_i128(v[1])).unwrap_or(Wide::ZERO), out).map_err(c),
            Prim::Sub => fit(Wide::from_i128(v[0]).checked_sub(Wide::from_i128(v[1])).unwrap_or(Wide::ZERO), out).map_err(c),
            Prim::Mul => fit(Wide::mul_i128(v[0], v[1]), out).map_err(c),
            Prim::Div { rule } => {
                if v[1] < 1 {
                    return Err(Class::Divisor);
                }
                fit(divide(v[0], v[1], *rule), out).map_err(c)
            }
            Prim::Clamp { lo, hi } => fit_i(v[0].clamp(*lo as i128, (*hi as i128).max(*lo as i128)), out).map_err(c),
            Prim::StateWrite { state } => {
                let StateKind::Fixed { lo, hi } = self.p.states[*state as usize].kind else { unreachable!() };
                fit_i(v[0].clamp(lo as i128, (hi as i128).max(lo as i128)), out).map_err(c)
            }
            Prim::Log2Floor => fit_i(log2_floor(v[0]), out).map_err(c),
            Prim::IntExp => fit_i(int_exp(v[0]), out).map_err(c),
            Prim::IntRsqrt => fit_i(int_rsqrt(v[0]), out).map_err(c),
            Prim::IntLn => fit_i(int_ln(v[0]), out).map_err(c),
            Prim::Compare { cmp } => fit_i(cmp_holds(*cmp, v[0], v[1]) as i128, out).map_err(c),
            Prim::MatMul => {
                let mut acc = OrderFree::new();
                for ab in v.chunks(2) {
                    acc.push(Wide::mul_i128(ab[0], ab[1]));
                }
                acc.finish(out).map_err(c)
            }
            Prim::ReduceSum { .. } => {
                let mut acc = OrderFree::new();
                for &x in v {
                    acc.push(Wide::from_i128(x));
                }
                acc.finish(out).map_err(c)
            }
            Prim::ReduceMax { .. } => fit_i(*v.iter().max().expect("n ≥ 1"), out).map_err(c),
            Prim::TopK { .. } => unreachable!("a TopK row is finished by finish_topk"),
        }
    }

    /// A `TopK` row: the `k` largest (value descending, index ascending), kept in index order; every
    /// slot of the row at once.
    fn finish_topk(&mut self, f: &Frame, axis: usize, k: u64) -> Result<(), Stop> {
        let node = self.node_of(&f.key);
        let shape = node.out.extents(self.g.h(f.key.ctx));
        let mut row: Vec<(i128, u64)> = f.vals.iter().enumerate().map(|(t, &v)| (v, t as u64)).collect();
        row.sort_by(|p, q| q.0.cmp(&p.0).then(p.1.cmp(&q.1)));
        let mut kept: Vec<u64> = row[..k as usize].iter().map(|e| e.1).collect();
        kept.sort_unstable();
        let mut j = f.o.clone();
        for (slot, &idx) in kept.iter().enumerate() {
            j[axis] = slot as u64;
            let key = NKey { e: ravel(&j, &shape), ..f.key };
            match fit_i(idx as i128, node.out.dtype) {
                Ok(v) => {
                    self.memo.insert(key, Slot::Val(v));
                }
                Err(e) => {
                    self.memo.insert(key, Slot::Bad);
                    self.flag(e.class)?;
                }
            }
        }
        Ok(())
    }

    fn finish(&mut self, f: Frame) -> Result<(), Stop> {
        let node = self.node_of(&f.key);
        if let Prim::TopK { axis, k } = node.prim {
            if f.bad {
                let shape = node.out.extents(self.g.h(f.key.ctx));
                let mut j = f.o.clone();
                for slot in 0..k as u64 {
                    j[axis as usize] = slot;
                    self.memo.insert(NKey { e: ravel(&j, &shape), ..f.key }, Slot::Bad);
                }
                return Ok(());
            }
            return self.finish_topk(&f, axis as usize, k as u64);
        }
        if f.bad {
            self.memo.insert(f.key, Slot::Bad);
            return Ok(());
        }
        match self.value(&f) {
            Ok(v) => {
                self.memo.insert(f.key, Slot::Val(v));
            }
            Err(c) => {
                self.memo.insert(f.key, Slot::Bad);
                self.flag(c)?;
            }
        }
        Ok(())
    }

    /// Runs the frames on the stack to completion.
    fn run(&mut self) -> Result<(), Stop> {
        while let Some(mut f) = self.stack.pop() {
            if !f.scanned {
                self.scan(&mut f);
                // Charged when first scanned, before any operand is fetched.
                let t = self.terms_of(&f);
                self.charge(1, t)?;
                f.scanned = true;
            }
            let mut child = None;
            loop {
                match self.next(&f) {
                    Next::Done => break,
                    Next::IndexFail => {
                        self.flag(Class::Index)?;
                        f.bad = true;
                        break;
                    }
                    Next::Read(r) => match self.read(&f.key, r)? {
                        Got::Val(v) => {
                            f.vals.push(v);
                            f.step += 1;
                        }
                        Got::Bad => {
                            f.vals.push(0);
                            f.bad = true;
                            f.step += 1;
                        }
                        Got::Push(k) => {
                            child = Some(k);
                            break;
                        }
                    },
                }
            }
            if let Some(k) = child {
                self.stack.push(f);
                self.push(k)?;
                continue;
            }
            self.finish(f)?;
        }
        Ok(())
    }

    /// Element `e` of the target (`None`: it failed, when exploring).
    fn root(&mut self, target: &Target, e: u64) -> Result<Option<i128>, Stop> {
        let (g, key) = match *target {
            Target::Node { ctx, node } => {
                let k = NKey { ctx, node, e };
                (self.node_elem(k)?, Some(k))
            }
            Target::StateAfter { pos, state, layer } => match writer(self.p, &self.g, state, layer) {
                // The writer's element, a leaf if committed, which must lie in [lo, hi] (else
                // Operand) as every Fixed value the evaluation reads does (§9.4, since 827187f34).
                Some((wocc, wnode)) => {
                    let k = NKey { ctx: Ctx { pos, occ: wocc }, node: wnode, e };
                    let g = match self.node_elem(k)? {
                        Got::Val(v) => {
                            let StateKind::Fixed { lo, hi } = self.p.states[state as usize].kind else { unreachable!() };
                            if (lo as i128..=hi as i128).contains(&v) { Got::Val(v) } else { self.fail(Class::Operand)? }
                        }
                        other => other,
                    };
                    (g, Some(k))
                }
                None => (self.carried(SKey { pos, state, layer, i: e })?, None),
            },
        };
        Ok(match g {
            Got::Val(v) => Some(v),
            Got::Bad => None,
            Got::Push(k) => {
                self.push(k)?;
                self.run()?;
                match self.memo.get(&key.expect("only a node element is pushed")) {
                    Some(Slot::Val(v)) => Some(*v),
                    _ => None,
                }
            }
        })
    }
}

fn stop(s: Stop) -> DemandError {
    match s {
        Stop::Fail(c) => DemandError::Class(c),
        Stop::Limit => DemandError::WorkLimit,
    }
}

/// `eval_demanded(target, elements, source, limits)` of §9.4 for a program in normal form: the
/// values in `elements`' order and the work, or the first failure met.
pub fn eval_demanded(
    p: &Program,
    target: &Target,
    elements: &[u64],
    source: &mut dyn Source,
    limits: Limits,
) -> Result<(Vec<i128>, Work), DemandError> {
    if let Some(&c) = request_refusals(p, target, elements).iter().next() {
        return Err(DemandError::Class(c));
    }
    let mut ev = Ev::new(p, source, limits, false, target);
    let mut out = Vec::with_capacity(elements.len());
    for &e in elements {
        match ev.root(target, e) {
            Ok(Some(v)) => out.push(v),
            Ok(None) => unreachable!("a failure stops the evaluation"),
            Err(s) => return Err(stop(s)),
        }
    }
    Ok((out, ev.work))
}

/// Every outcome §9.4 allows for this request and these answers: `Ok` when the evaluation must
/// succeed (with its values and work), otherwise the set of failures it may report — the class of
/// every element that fails, reached by evaluating everything the evaluation can reach (a `Select`'s
/// unchosen operand and a failed `Gather`'s data never), plus `WorkLimit` when that work exceeds the
/// limits. The request refusals, when any applies, are the whole set.
pub fn demand_outcomes(
    p: &Program,
    target: &Target,
    elements: &[u64],
    source: &mut dyn Source,
    limits: Limits,
) -> Result<(Vec<i128>, Work), BTreeSet<DemandError>> {
    let refusals = request_refusals(p, target, elements);
    if !refusals.is_empty() {
        return Err(refusals.into_iter().map(DemandError::Class).collect());
    }
    let mut ev = Ev::new(p, source, limits, true, target);
    let mut out = Vec::with_capacity(elements.len());
    for &e in elements {
        match ev.root(target, e) {
            Ok(v) => out.push(v),
            Err(_) => unreachable!("exploring never stops"),
        }
    }
    let mut set: BTreeSet<DemandError> = ev.failures.iter().map(|&c| DemandError::Class(c)).collect();
    if !ev.work.within(&limits) {
        set.insert(DemandError::WorkLimit);
    }
    if set.is_empty() { Ok((out.into_iter().map(|v| v.expect("no failure")).collect(), ev.work)) } else { Err(set) }
}

/// §9.5.2's refusals of a range request before anything is read: §9.4's for `node(p, o, target)`,
/// then — reading: a supplied index that is the target or no node of the block is `Malformed`, as
/// §9.2's environment malformations are — and, with a range, the target must reduce over `H` and
/// `0 ≤ from < to ≤ H` (else `Malformed`).
pub fn range_refusals(p: &Program, ctx: Ctx, target: u16, elements: &[u64], supplied: &[u16], range: Option<(u64, u64)>) -> BTreeSet<Class> {
    let mut out = request_refusals(p, &Target::Node { ctx, node: target }, elements);
    if ctx.occ as usize >= p.schedule.layers.len() + 2 {
        return out;
    }
    let g = Geo::new(p);
    let b = g.block(ctx);
    let n = p.blocks[b].nodes.len();
    if (target as usize) >= n {
        return out;
    }
    if supplied.iter().any(|&s| s == target || s as usize >= n) {
        out.insert(Class::Malformed);
    }
    if let Some((from, to)) = range
        && (!reduces_over_h(p, b, target as usize) || from >= to || to > g.h(ctx))
    {
        out.insert(Class::Malformed);
    }
    out
}

/// §9.5.2 `eval_range(ctx, target, elements, supplied, range)`: §9.4's `eval_demanded` of
/// `node(p, o, target)` with the supplied nodes leaves in the target's context, and the target
/// reducing over the history indices `[from, to)` only.
#[allow(clippy::too_many_arguments)]
pub fn eval_range(
    p: &Program,
    ctx: Ctx,
    target: u16,
    elements: &[u64],
    supplied: &[u16],
    range: Option<(u64, u64)>,
    source: &mut dyn Source,
    limits: Limits,
) -> Result<(Vec<i128>, Work), DemandError> {
    if let Some(&c) = range_refusals(p, ctx, target, elements, supplied, range).iter().next() {
        return Err(DemandError::Class(c));
    }
    let t = Target::Node { ctx, node: target };
    let mut ev = Ev::new(p, source, limits, false, &t);
    ev.supplied = supplied.iter().copied().collect();
    ev.range = range;
    let mut out = Vec::with_capacity(elements.len());
    for &e in elements {
        match ev.root(&t, e) {
            Ok(Some(v)) => out.push(v),
            Ok(None) => unreachable!("a failure stops the evaluation"),
            Err(s) => return Err(stop(s)),
        }
    }
    Ok((out, ev.work))
}
