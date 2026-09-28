//! A demand source model (04b §9.4) shared by the golden and differential tests: committed node
//! values by context, params, tokens and supplied `Fixed` values, with optional hostile answers per
//! question. It answers both implementations' source traits from the same data, so both see the
//! same answers, and each side records the questions it asked (its request set).

use std::collections::{BTreeMap, BTreeSet};

use misaka_palw_tir as first;
use misaka_palw_tir_ref2 as ref2;
use ref2::demand::{Ctx, DemandError, Limits, Question, Source, Supply, Target, Work};
use ref2::eval::{Params, RunState, initial_state, step_traced};
use ref2::{Prim, Program, Ref, StateKind};

use super::bridge::{catch_any, class_from};

/// A hostile answer to one question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Refuse,
    Value(i128),
    Replay,
}

/// Where a history row comes from: the committed node the row is.
#[derive(Clone, Copy, Debug)]
pub enum HistIn {
    /// The `HistAppend`'s input node, in the appending occurrence.
    Node(u16),
    /// Its carry-in: node `carry_out[k]` of the previous occurrence.
    Carry { occ: u32, node: u16 },
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub tokens: BTreeMap<u64, u64>,
    /// Committed node values by `(pos, occurrence, node)`.
    pub nodes: BTreeMap<(u64, u32, u16), Vec<i128>>,
    pub params: BTreeMap<(u16, Option<u32>), Vec<i128>>,
    /// Supplied `Fixed` values at the start of a position; every other `(p, j, l)` answers `Replay`.
    pub states: BTreeMap<(u64, u16, Option<u32>), Vec<i128>>,
    /// Per `Hist` instance, the appending occurrence and the row's committed node.
    pub hist_src: BTreeMap<(u16, Option<u32>), (u32, HistIn)>,
    pub faults: BTreeMap<Question, Fault>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Val(i128),
    Replay,
}

/// The occurrences of §3.3 as `(block, layer)`.
pub fn occurrences(p: &Program) -> Vec<(usize, Option<u32>)> {
    let mut occ = vec![(p.schedule.pre as usize, None)];
    for (l, &b) in p.schedule.layers.iter().enumerate() {
        occ.push((b as usize, Some(l as u32)));
    }
    occ.push((p.schedule.post as usize, None));
    occ
}

/// Every `Hist` instance's appending occurrence and row source (the golden vectors' rule).
pub fn hist_sources(p: &Program) -> BTreeMap<(u16, Option<u32>), (u32, HistIn)> {
    let mut out = BTreeMap::new();
    for (o, (b, layer)) in occurrences(p).into_iter().enumerate() {
        for n in &p.blocks[b].nodes {
            if let Prim::HistAppend { state } = n.prim {
                let s = &p.states[state as usize];
                let l = if s.per_layer { layer } else { None };
                let src = match n.inputs[0] {
                    Ref::Node(m) => HistIn::Node(m),
                    Ref::CarryIn(k) => {
                        let (pb, _) = occurrences(p)[o - 1];
                        HistIn::Carry { occ: o as u32 - 1, node: p.blocks[pb].carry_out[k as usize] }
                    }
                    _ => continue,
                };
                out.insert((state, l), (o as u32, src));
            }
        }
    }
    out
}

impl Model {
    pub fn answer(&self, q: &Question) -> Option<Answer> {
        if let Some(f) = self.faults.get(q) {
            return match f {
                Fault::Refuse => None,
                Fault::Value(v) => Some(Answer::Val(*v)),
                Fault::Replay => Some(Answer::Replay),
            };
        }
        let at = |v: &Vec<i128>, i: u64| v.get(i as usize).copied().map(Answer::Val);
        match *q {
            Question::Node { ctx, node, i } => at(self.nodes.get(&(ctx.pos, ctx.occ, node))?, i),
            Question::Param { param, layer, i } => at(self.params.get(&(param, layer))?, i),
            Question::State { pos, state, layer, i } => match self.states.get(&(pos, state, layer)) {
                Some(v) => at(v, i),
                None => Some(Answer::Replay),
            },
            Question::HistRow { pos, state, layer, row_pos, i } => {
                if row_pos >= pos {
                    return None;
                }
                let (occ, src) = *self.hist_src.get(&(state, layer))?;
                match src {
                    HistIn::Node(m) => at(self.nodes.get(&(row_pos, occ, m))?, i),
                    HistIn::Carry { occ, node } => at(self.nodes.get(&(row_pos, occ, node))?, i),
                }
            }
            Question::Token { pos } => self.tokens.get(&pos).map(|&t| Answer::Val(t as i128)),
        }
    }

    /// The model of an honest run of `tokens` from the initial state: every commit point of every
    /// position, and the `Fixed` values at the start of each position `p` for which `supply(p)`.
    pub fn from_run(p: &Program, params: &Params, tokens: &[u64], supply: &dyn Fn(u64) -> bool) -> Option<Model> {
        let mut m = Model { hist_src: hist_sources(p), ..Default::default() };
        for (&(j, l), t) in params {
            m.params.insert((j, l), t.data.clone());
        }
        let mut st: RunState = initial_state(p);
        for (pos, &tok) in tokens.iter().enumerate() {
            let pos = pos as u64;
            m.tokens.insert(pos, tok);
            if supply(pos) {
                for (&(j, l), t) in &st.fixed {
                    m.states.insert((pos, j, l), t.data.clone());
                }
            }
            let (_, next, trace) = step_traced(p, params, &st, tok).ok()?;
            for (o, occ) in trace.iter().enumerate() {
                for (n, node) in p.blocks[occ.block as usize].nodes.iter().enumerate() {
                    if node.commit {
                        m.nodes.insert((pos, o as u32, n as u16), occ.values[n].data.clone());
                    }
                }
            }
            st = next;
        }
        // The value at the start of the position after the last, for `state_after` of the last.
        let end = tokens.len() as u64;
        if supply(end) {
            for (&(j, l), t) in &st.fixed {
                m.states.insert((end, j, l), t.data.clone());
            }
        }
        Some(m)
    }
}

/// This implementation's view of a model.
pub struct Mine<'a>(pub &'a Model);

impl Source for Mine<'_> {
    fn node(&mut self, ctx: Ctx, node: u16, i: u64) -> Option<i128> {
        match self.0.answer(&Question::Node { ctx, node, i })? {
            Answer::Val(v) => Some(v),
            Answer::Replay => None,
        }
    }
    fn param(&mut self, param: u16, layer: Option<u32>, i: u64) -> Option<i128> {
        match self.0.answer(&Question::Param { param, layer, i })? {
            Answer::Val(v) => Some(v),
            Answer::Replay => None,
        }
    }
    fn state(&mut self, pos: u64, state: u16, layer: Option<u32>, i: u64) -> Option<Supply> {
        Some(match self.0.answer(&Question::State { pos, state, layer, i })? {
            Answer::Val(v) => Supply::Value(v),
            Answer::Replay => Supply::Replay,
        })
    }
    fn hist_row(&mut self, pos: u64, state: u16, layer: Option<u32>, row_pos: u64, i: u64) -> Option<i128> {
        match self.0.answer(&Question::HistRow { pos, state, layer, row_pos, i })? {
            Answer::Val(v) => Some(v),
            Answer::Replay => None,
        }
    }
    fn token(&mut self, pos: u64) -> Option<u64> {
        match self.0.answer(&Question::Token { pos })? {
            Answer::Val(v) if v >= 0 => Some(v as u64),
            _ => None,
        }
    }
}

/// The first implementation's view of the same model, recording what it asks.
pub struct Theirs<'a> {
    pub m: &'a Model,
    pub asked: BTreeSet<Question>,
    pub order: Vec<Question>,
}

thread_local! {
    /// The kind of the error the first implementation's source returns for a refusal (the text
    /// says a refusal fails the evaluation with class `Missing`, whatever the source reports).
    pub static REFUSAL_KIND: std::cell::Cell<Option<first::error::TirErrorKind>> = const { std::cell::Cell::new(None) };
}

fn refused() -> first::error::TirError {
    let kind = REFUSAL_KIND.with(|k| k.get()).unwrap_or(first::error::TirErrorKind::Missing);
    first::error::TirError::new(kind, "the model refuses")
}

fn ctx_from(c: first::demand::DemandContext) -> Ctx {
    Ctx { pos: c.pos as u64, occ: c.occurrence as u32 }
}

impl Theirs<'_> {
    fn note(&mut self, q: Question) {
        if self.asked.insert(q) {
            self.order.push(q);
        }
    }

    fn val(&mut self, q: Question) -> first::error::TirResult<i128> {
        self.note(q);
        match self.m.answer(&q) {
            Some(Answer::Val(v)) => Ok(v),
            _ => Err(refused()),
        }
    }
}

impl first::demand::DemandSource for Theirs<'_> {
    fn node(&mut self, ctx: first::demand::DemandContext, node: u16, index: usize) -> first::error::TirResult<i128> {
        self.val(Question::Node { ctx: ctx_from(ctx), node, i: index as u64 })
    }
    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> first::error::TirResult<i128> {
        self.val(Question::Param { param, layer: layer.map(|l| l as u32), i: index as u64 })
    }
    fn state(
        &mut self,
        pos: u32,
        state: u16,
        layer: Option<u16>,
        index: usize,
    ) -> first::error::TirResult<first::demand::StateSupply> {
        let q = Question::State { pos: pos as u64, state, layer: layer.map(|l| l as u32), i: index as u64 };
        self.note(q);
        match self.m.answer(&q) {
            Some(Answer::Val(v)) => Ok(first::demand::StateSupply::Value(v)),
            Some(Answer::Replay) => Ok(first::demand::StateSupply::Replay),
            None => Err(refused()),
        }
    }
    fn hist_row(
        &mut self,
        pos: u32,
        state: u16,
        layer: Option<u16>,
        row_pos: u32,
        index: usize,
    ) -> first::error::TirResult<i128> {
        self.val(Question::HistRow {
            pos: pos as u64,
            state,
            layer: layer.map(|l| l as u32),
            row_pos: row_pos as u64,
            i: index as u64,
        })
    }
    fn token(&mut self, pos: u32) -> first::error::TirResult<u32> {
        let q = Question::Token { pos: pos as u64 };
        self.note(q);
        match self.m.answer(&q) {
            Some(Answer::Val(v)) if (0..=u32::MAX as i128).contains(&v) => Ok(v as u32),
            _ => Err(refused()),
        }
    }
}

/// One evaluation's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DRes {
    Ok(Vec<i128>, Work),
    Err(DemandError),
    Panic(String),
}

/// One evaluation's result, request set, and the order the questions were first asked in.
#[derive(Clone, Debug)]
pub struct Run {
    pub res: DRes,
    pub asked: BTreeSet<Question>,
    pub order: Vec<Question>,
}

pub fn mine_demand(p: &Program, target: &Target, elements: &[u64], m: &Model, limits: Limits) -> Run {
    let mut src = Mine(m);
    let mut rec = ref2::demand::Recorder::new(&mut src);
    let r = catch_any(|| ref2::demand::eval_demanded(p, target, elements, &mut rec, limits));
    let (asked, order) = (rec.asked, rec.order);
    let res = match r {
        Ok(Ok((v, w))) => DRes::Ok(v, w),
        Ok(Err(e)) => DRes::Err(e),
        Err(msg) => DRes::Panic(msg),
    };
    Run { res, asked, order }
}

/// Every outcome the text allows (this implementation's exploration).
pub fn allowed(
    p: &Program,
    target: &Target,
    elements: &[u64],
    m: &Model,
    limits: Limits,
) -> Result<(Vec<i128>, Work), BTreeSet<DemandError>> {
    let mut src = Mine(m);
    ref2::demand::demand_outcomes(p, target, elements, &mut src, limits)
}

pub fn target_to(t: &Target) -> first::demand::DemandTarget {
    match *t {
        Target::Node { ctx, node } => first::demand::DemandTarget::Node {
            ctx: first::demand::DemandContext { pos: ctx.pos as u32, occurrence: ctx.occ as u16 },
            node,
        },
        Target::StateAfter { pos, state, layer } => {
            first::demand::DemandTarget::StateAfter { pos: pos as u32, state, layer: layer.map(|l| l as u16) }
        }
    }
}

/// The first implementation, as a black box, on the same request and the same answers.
pub fn first_demand(
    fp: &first::program::TirProgramV1,
    info: &first::validate::ProgramInfo,
    target: &Target,
    elements: &[u64],
    m: &Model,
    limits: Limits,
) -> Run {
    let els: Vec<usize> = elements.iter().map(|&e| e as usize).collect();
    let req = first::demand::DemandRequest { target: target_to(target), elements: &els };
    let lim = first::demand::DemandLimits { max_elements: limits.max_elements, max_terms: limits.max_terms };
    let mut src = Theirs { m, asked: BTreeSet::new(), order: Vec::new() };
    let r = catch_any(|| first::demand::eval_demanded(fp, info, &req, &mut src, &lim));
    let (asked, order) = (src.asked, src.order);
    let res = match r {
        Ok(Ok((v, w))) => DRes::Ok(v, Work { elements: w.elements, terms: w.terms }),
        Ok(Err(first::demand::DemandError::Tir(e))) => DRes::Err(DemandError::Class(class_from(e.kind))),
        Ok(Err(first::demand::DemandError::WorkLimit(_))) => DRes::Err(DemandError::WorkLimit),
        Err(msg) => DRes::Panic(msg),
    };
    Run { res, asked, order }
}

/// The request set grouped as the golden vectors print it: one line per question with every
/// argument but the index, and the indices as inclusive runs.
pub fn grouped(asked: &BTreeSet<Question>) -> Vec<String> {
    let mut out: Vec<(Question, Vec<u64>)> = Vec::new();
    for q in asked {
        let (key, i) = q.split();
        match out.last_mut() {
            Some((k, v)) if *k == key => v.extend(i),
            _ => out.push((key, i.into_iter().collect())),
        }
    }
    out.into_iter().map(|(k, v)| format!("{} {}", question_head(&k), runs(&v))).collect()
}

pub fn question_head(q: &Question) -> String {
    let l = |x: Option<u32>| x.map(|v| v.to_string()).unwrap_or_else(|| "null".into());
    match *q {
        Question::Node { ctx, node, .. } => format!("node(pos {}, occ {}, node {node})", ctx.pos, ctx.occ),
        Question::Param { param, layer, .. } => format!("param({param}, layer {})", l(layer)),
        Question::State { pos, state, layer, .. } => format!("state(pos {pos}, state {state}, layer {})", l(layer)),
        Question::HistRow { pos, state, layer, row_pos, .. } => {
            format!("hist_row(pos {pos}, state {state}, layer {}, row {row_pos})", l(layer))
        }
        Question::Token { pos } => format!("token(pos {pos})"),
    }
}

pub fn runs(v: &[u64]) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        parts.push(if i == j { v[i].to_string() } else { format!("{}-{}", v[i], v[j]) });
        i = j + 1;
    }
    parts.join(",")
}

/// A `Fixed` state's declared range.
pub fn fixed_range(p: &Program, j: u16) -> Option<(i128, i128)> {
    match p.states.get(j as usize)?.kind {
        StateKind::Fixed { lo, hi } => Some((lo as i128, hi as i128)),
        StateKind::Hist { .. } => None,
    }
}
