//! Admission, `tir_admit_v1` (04b §10.3), with the §7 range analysis and the §8 costs — written from
//! the text alone. Readings taken where the text leaves a choice are marked `// reading:` and listed
//! in `docs/design/palw/tir/ref2-findings.md`.
//!
//! Structure (deliberately plain): intervals as exact `Wide` computations checked against the
//! output dtype; costs as saturating `u64`; cones by a worklist; the tile's box demand in one
//! descending pass; the replay closure as a fixpoint over states; the group rule's classes
//! (free / aligned / mixed) assigned in ascending node order, as §10.3 states it since the A1 fix.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::codec::decode_canonical;
use crate::error::{Class, Res, TirError};
use crate::normal_form::{block_window, ref_type};
use crate::prims::divide;
use crate::program::{Prim, Program, Ref, StateKind};
use crate::tensor::Tensor;
use crate::transcendental::{LN2_Q, ONE, floor_div, log2_floor};
use crate::types::{DType, Dim, TensorType, pow2};
use crate::wide::Wide;

// ------------------------------------------------------------------ inputs and outputs

/// The ceilings of §10.3 (names as the text lists them: the terminal tile's MACs,
/// transcendentals, opened bytes and committed operands; the position's MACs and
/// transcendentals; the state bytes; the step leaves per position; the longest checkpoint
/// interval; the cone work).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ceilings {
    pub max_tile_macs: u64,
    pub max_tile_transcendentals: u64,
    pub max_tile_opened_bytes: u64,
    pub max_tile_operands: u64,
    pub max_position_macs: u64,
    pub max_position_transcendentals: u64,
    pub max_state_bytes: u64,
    pub max_step_leaves: u64,
    pub max_checkpoint_interval: u32,
    pub max_cone_work: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmitInputs {
    pub tile_len: u32,
    pub h_chunk: u32,
    pub ceilings: Ceilings,
}

/// A §8 cost vector.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    pub macs: u64,
    pub elementwise: u64,
    pub transcendentals: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

impl Cost {
    pub fn add(&mut self, o: &Cost) {
        self.macs = self.macs.saturating_add(o.macs);
        self.elementwise = self.elementwise.saturating_add(o.elementwise);
        self.transcendentals = self.transcendentals.saturating_add(o.transcendentals);
        self.bytes_read = self.bytes_read.saturating_add(o.bytes_read);
        self.bytes_written = self.bytes_written.saturating_add(o.bytes_written);
    }

    /// `⌈c / g⌉` for every component (§10.3, one group's replay).
    pub fn per_group(&self, g: u64) -> Cost {
        let f = |c: u64| c.div_ceil(g);
        Cost {
            macs: f(self.macs),
            elementwise: f(self.elementwise),
            transcendentals: f(self.transcendentals),
            bytes_read: f(self.bytes_read),
            bytes_written: f(self.bytes_written),
        }
    }
}

/// A §7 interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub lo: i128,
    pub hi: i128,
}

/// A leaf of a cone (§10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Leaf {
    Commit(u16),
    CarryIn(u8),
    State(u16),
    History(u16),
    Param(u16),
    Const(u16),
    Input(u8),
}

impl Leaf {
    /// §10.3: "a committed leaf (commit point, carry-in, state, history)".
    pub fn committed(&self) -> bool {
        matches!(self, Leaf::Commit(_) | Leaf::CarryIn(_) | Leaf::State(_) | Leaf::History(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cone {
    pub block: u8,
    pub node: u16,
    pub nodes: Vec<u16>,
    pub leaves: Vec<Leaf>,
    /// The cone's whole cost: `Σ` of its nodes' §8 costs at `H = W` (informative, §10.3).
    pub whole: Cost,
    pub tiles: u64,
    pub tile: Cost,
    pub tile_opened_bytes: u64,
    pub operands: u64,
    pub h_reductions: Vec<u16>,
    pub chunk: Option<Cost>,
    pub chunk_opened_bytes: Option<u64>,
}

impl Cone {
    /// The terminal cost: one `H` chunk for a dissected cone, else one tile (§10.3).
    pub fn terminal(&self) -> &Cost {
        self.chunk.as_ref().unwrap_or(&self.tile)
    }
    pub fn terminal_opened_bytes(&self) -> u64 {
        self.chunk_opened_bytes.unwrap_or(self.tile_opened_bytes)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Position {
    pub cost: Cost,
    pub state_bytes: u64,
    pub peak_live_bytes: u64,
    pub commit_lanes: u64,
    pub step_leaves: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateCkpt {
    pub state: u16,
    pub closure: Vec<u16>,
    pub groups: u64,
    pub per_position: Cost,
    pub interval: u32,
}

#[derive(Clone, Debug)]
pub struct Admission {
    pub program: Program,
    pub intervals: Vec<Vec<Interval>>,
    pub node_costs: Vec<Vec<Cost>>,
    pub position: Position,
    pub cones: Vec<Cone>,
    pub states: Vec<StateCkpt>,
    pub checkpoint_interval: u32,
    pub cone_work: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmitError {
    Program(TirError),
    Exceeds { limit: &'static str, at: String, value: u64, cap: u64 },
    Inputs(&'static str),
}

// ------------------------------------------------------------------ helpers

fn width(d: DType) -> u64 {
    d.width() as u64
}

/// Element count of a type at `H = h`.
fn elems(t: &TensorType, h: u64) -> u64 {
    t.extents(h).iter().fold(1u64, |a, &e| a.saturating_mul(e))
}

/// The block's worst case `H`: its window, or 1 without one (§8).
fn worst_h(p: &Program, b: usize) -> u64 {
    match block_window(p, b) {
        Ok(Some(w)) => w as u64,
        _ => 1,
    }
}

fn input_types(p: &Program, b: usize, i: usize) -> Vec<TensorType> {
    p.blocks[b].nodes[i].inputs.iter().map(|r| ref_type(p, b, i, r).expect("normal form: refs exist")).collect()
}

fn roots_of(p: &Program, b: usize) -> Vec<bool> {
    let block = &p.blocks[b];
    let mut r: Vec<bool> =
        block.nodes.iter().map(|n| n.commit || matches!(n.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. })).collect();
    for &c in &block.carry_out {
        r[c as usize] = true;
    }
    if b == p.schedule.post as usize {
        r[p.logits as usize] = true;
    }
    r
}

fn occurrences(p: &Program) -> Vec<(usize, Option<usize>)> {
    let mut v = vec![(p.schedule.pre as usize, None)];
    for (l, &b) in p.schedule.layers.iter().enumerate() {
        v.push((b as usize, Some(l)));
    }
    v.push((p.schedule.post as usize, None));
    v
}

// ------------------------------------------------------------------ §7 ranges

fn iv_of(d: DType) -> Interval {
    Interval { lo: d.min(), hi: d.max() }
}

fn union(a: Interval, b: Interval) -> Interval {
    Interval { lo: a.lo.min(b.lo), hi: a.hi.max(b.hi) }
}

fn range_err(class: Class, b: usize, i: usize, what: &str) -> TirError {
    TirError::new(class, format!("§7: block {b} node {i}: {what}"))
}

/// The obligation "⊆ out dtype" on an interval of exact integers.
fn within(lo: Wide, hi: Wide, out: DType, b: usize, i: usize) -> Res<Interval> {
    if lo.within(out.min(), out.max()) && hi.within(out.min(), out.max()) {
        Ok(Interval { lo: lo.to_i128().unwrap(), hi: hi.to_i128().unwrap() })
    } else {
        Err(range_err(Class::Overflow, b, i, &format!("interval outside {}", out.name())))
    }
}

fn wmin(a: Wide, b: Wide) -> Wide {
    if a.cmp_wide(&b) == Ordering::Greater { b } else { a }
}
fn wmax(a: Wide, b: Wide) -> Wide {
    if a.cmp_wide(&b) == Ordering::Less { b } else { a }
}

/// min and max of a set of exact values.
fn span(vals: &[Wide]) -> (Wide, Wide) {
    let mut lo = vals[0];
    let mut hi = vals[0];
    for &v in &vals[1..] {
        lo = wmin(lo, v);
        hi = wmax(hi, v);
    }
    (lo, hi)
}

fn w(v: i128) -> Wide {
    Wide::from_i128(v)
}

fn const_iv(p: &Program, j: u16) -> Interval {
    let c = &p.consts[j as usize];
    let t = Tensor::from_le_bytes(c.dtype, c.shape.iter().map(|&d| d as u64).collect(), &c.data).expect("normal form: const");
    let lo = t.data.iter().copied().min().unwrap_or(0);
    let hi = t.data.iter().copied().max().unwrap_or(0);
    Interval { lo, hi }
}

/// §7: every node's interval, block by block; a broken obligation is a refusal (class: the §9.3
/// class of the evaluation rule the obligation stands for — reading).
/// §7: the interval of one node from its operands' intervals (`b`, `i` only label errors).
#[allow(clippy::too_many_arguments)]
pub fn transfer(
    prim: &Prim,
    ins: &[Interval],
    tys: &[TensorType],
    out_t: &TensorType,
    h: u64,
    states: &[crate::program::StateDecl],
    b: usize,
    i: usize,
) -> Res<Interval> {
    let out = out_t.dtype;
    let x = ins.first().copied().unwrap_or(Interval { lo: 0, hi: 0 });
    let iv = match prim {
        Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast => x,
        Prim::Concat { .. } => ins.iter().copied().reduce(union).unwrap(),
        Prim::Iota { axis, start, step } => {
            let extent = out_t.extents(h)[*axis as usize];
            let e0 = w(*start as i128);
            let en = w(*start as i128).checked_add(Wide::mul_i128(*step as i128, extent as i128 - 1)).unwrap();
            within(wmin(e0, en), wmax(e0, en), out, b, i)?
        }
        Prim::Gather { axis, .. } => {
            let Dim::Fixed(ext) = tys[0].shape[*axis as usize] else { unreachable!() };
            let idx = ins[1];
            if idx.lo < 0 || idx.hi > ext as i128 - 1 {
                return Err(range_err(Class::Index, b, i, "indices may leave the gathered axis"));
            }
            ins[0]
        }
        Prim::Cast => within(w(x.lo), w(x.hi), out, b, i)?,
        Prim::Add => {
            let (a, c) = (ins[0], ins[1]);
            within(w(a.lo).checked_add(w(c.lo)).unwrap(), w(a.hi).checked_add(w(c.hi)).unwrap(), out, b, i)?
        }
        Prim::Sub => {
            let (a, c) = (ins[0], ins[1]);
            within(w(a.lo).checked_sub(w(c.hi)).unwrap(), w(a.hi).checked_sub(w(c.lo)).unwrap(), out, b, i)?
        }
        Prim::Mul => {
            let (a, c) = (ins[0], ins[1]);
            let (lo, hi) = span(&[
                Wide::mul_i128(a.lo, c.lo),
                Wide::mul_i128(a.lo, c.hi),
                Wide::mul_i128(a.hi, c.lo),
                Wide::mul_i128(a.hi, c.hi),
            ]);
            within(lo, hi, out, b, i)?
        }
        Prim::MatMul => {
            let (a, c) = (ins[0], ins[1]);
            let (tlo, thi) = span(&[
                Wide::mul_i128(a.lo, c.lo),
                Wide::mul_i128(a.lo, c.hi),
                Wide::mul_i128(a.hi, c.lo),
                Wide::mul_i128(a.hi, c.hi),
            ]);
            // T fits i128: MatMul operands are at most i64.
            let k = *tys[0].extents(h).last().unwrap() as i128;
            let lo = Wide::mul_i128(k, wmin(tlo, Wide::ZERO).to_i128().unwrap());
            let hi = Wide::mul_i128(k, wmax(thi, Wide::ZERO).to_i128().unwrap());
            within(lo, hi, out, b, i)?
        }
        Prim::ReduceSum { axis } => {
            let nn = tys[0].extents(h)[*axis as usize] as i128;
            within(Wide::mul_i128(nn, x.lo.min(0)), Wide::mul_i128(nn, x.hi.max(0)), out, b, i)?
        }
        Prim::ReduceMax { .. } => x,
        Prim::Div { rule } => {
            let (xv, d) = (ins[0], ins[1]);
            if d.lo < 1 {
                return Err(range_err(Class::Divisor, b, i, "the divisor may be below 1"));
            }
            let (lo, hi) = span(&[
                divide(xv.lo, d.lo, *rule),
                divide(xv.lo, d.hi, *rule),
                divide(xv.hi, d.lo, *rule),
                divide(xv.hi, d.hi, *rule),
            ]);
            within(lo, hi, out, b, i)?
        }
        Prim::Clamp { lo, hi } => {
            let (l, u) = (*lo as i128, *hi as i128);
            Interval { lo: x.lo.clamp(l, u), hi: x.hi.clamp(l, u) }
        }
        Prim::Log2Floor => within(w(log2_floor(x.lo)), w(log2_floor(x.hi)), out, b, i)?,
        Prim::IntExp => {
            let hi = if x.hi <= -31 * LN2_Q { 0 } else { 16_781_800 };
            within(w(0), w(hi), out, b, i)?
        }
        Prim::IntRsqrt => {
            let hi = if x.hi <= 0 {
                0
            } else {
                let e = floor_div(log2_floor(x.lo.max(1)) - 24, 2);
                let ub = if e >= 0 { floor_div(ONE, pow2(e as u32)) } else { ONE * pow2((-e) as u32) };
                ub.min(68_719_472_640)
            };
            within(w(0), w(hi), out, b, i)?
        }
        Prim::IntLn => {
            let (lo, hi) = if x.hi <= 0 {
                (0, 0)
            } else {
                let s_lo = log2_floor(x.lo.max(1)) - 24;
                let s_hi = log2_floor(x.hi) - 24;
                let (mut lo, mut hi) = (s_lo * LN2_Q, (s_hi + 1) * LN2_Q - 1);
                if x.lo <= 0 {
                    lo = lo.min(0);
                    hi = hi.max(0);
                }
                (lo, hi)
            };
            within(w(lo), w(hi), out, b, i)?
        }
        Prim::Compare { .. } => Interval { lo: 0, hi: 1 },
        Prim::Select => {
            let u = union(ins[1], ins[2]);
            within(w(u.lo), w(u.hi), out, b, i)?
        }
        Prim::TopK { axis, .. } => {
            let Dim::Fixed(nn) = tys[0].shape[*axis as usize] else { unreachable!() };
            Interval { lo: 0, hi: nn as i128 - 1 }
        }
        Prim::StateWrite { state } => {
            let StateKind::Fixed { lo, hi } = states[*state as usize].kind else { unreachable!() };
            let (l, u) = (lo as i128, hi as i128);
            Interval { lo: x.lo.clamp(l, u), hi: x.hi.clamp(l, u) }
        }
        Prim::HistAppend { .. } => x,
    };
    Ok(iv)
}

pub fn ranges(p: &Program) -> Res<Vec<Vec<Interval>>> {
    let mut all = Vec::with_capacity(p.blocks.len());
    for (b, block) in p.blocks.iter().enumerate() {
        let h = worst_h(p, b);
        let mut ivs: Vec<Interval> = Vec::with_capacity(block.nodes.len());
        for (i, n) in block.nodes.iter().enumerate() {
            let ins: Vec<Interval> = n
                .inputs
                .iter()
                .map(|r| match *r {
                    Ref::Node(k) => ivs[k as usize],
                    Ref::CarryIn(k) => iv_of(block.carry_in[k as usize].dtype),
                    Ref::Param(j) => iv_of(p.params[j as usize].dtype),
                    Ref::Const(j) => const_iv(p, j),
                    Ref::State(j) => match p.states[j as usize].kind {
                        StateKind::Fixed { lo, hi } => Interval { lo: lo as i128, hi: hi as i128 },
                        StateKind::Hist { .. } => iv_of(p.states[j as usize].dtype),
                    },
                    Ref::Input(0) => Interval { lo: 0, hi: p.token_bound as i128 - 1 },
                    Ref::Input(_) => Interval { lo: 0, hi: p.history_bound as i128 - 1 },
                })
                .collect();
            let tys = input_types(p, b, i);
            let iv = transfer(&n.prim, &ins, &tys, &n.out, h, &p.states, b, i)?;
            ivs.push(iv);
        }
        all.push(ivs);
    }
    Ok(all)
}

// ------------------------------------------------------------------ §8 costs

/// §8: the cost of node `i` of block `b` at `H = h`.
pub fn node_cost(p: &Program, b: usize, i: usize, h: u64) -> Cost {
    let n = &p.blocks[b].nodes[i];
    let tys = input_types(p, b, i);
    let eo = elems(&n.out, h);
    let wo = width(n.out.dtype);
    let bin = |t: &TensorType| elems(t, h).saturating_mul(width(t.dtype));
    let sum_in: u64 = tys.iter().map(bin).fold(0u64, |a, v| a.saturating_add(v));
    let written = eo.saturating_mul(wo);
    let mut c = Cost { bytes_written: written, ..Cost::default() };
    match &n.prim {
        Prim::Iota { .. } => c.elementwise = eo,
        Prim::Gather { .. } => {
            c.elementwise = eo;
            c.bytes_read = eo.saturating_mul(width(tys[0].dtype)).saturating_add(bin(&tys[1]));
        }
        Prim::MatMul => {
            let k = *tys[0].extents(h).last().unwrap();
            c.macs = eo.saturating_mul(k);
            c.bytes_read = sum_in;
        }
        Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => {
            c.elementwise = elems(&tys[0], h);
            c.bytes_read = bin(&tys[0]);
        }
        Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
            c.transcendentals = eo;
            c.bytes_read = bin(&tys[0]);
        }
        Prim::TopK { k, .. } => {
            c.elementwise = elems(&tys[0], h).saturating_mul(*k as u64);
            c.bytes_read = bin(&tys[0]);
        }
        Prim::StateWrite { .. } => {
            c.elementwise = eo;
            c.bytes_read = bin(&tys[0]);
        }
        Prim::HistAppend { .. } => {
            c.elementwise = eo;
            c.bytes_read = written; // B(out)
            c.bytes_written = bin(&tys[0]); // B(row)
        }
        _ => {
            c.elementwise = eo;
            c.bytes_read = sum_in;
        }
    }
    c
}

// ------------------------------------------------------------------ §8 per position

fn position(p: &Program, node_costs: &[Vec<Cost>], tile_len: u64) -> Position {
    let mut pos = Position::default();
    for (b, _) in occurrences(p) {
        for (i, c) in node_costs[b].iter().enumerate() {
            pos.cost.add(c);
            let n = &p.blocks[b].nodes[i];
            if n.commit {
                let e = elems(&n.out, worst_h(p, b));
                pos.commit_lanes = pos.commit_lanes.saturating_add(e);
                pos.step_leaves = pos.step_leaves.saturating_add(e.div_ceil(tile_len));
            }
        }
    }
    // State bytes: a global state one instance if any block uses it, a per-layer state one per
    // layer whose block uses it.
    let uses = |b: usize, j: u16| {
        p.blocks[b].nodes.iter().any(|n| {
            n.inputs.contains(&Ref::State(j))
                || matches!(n.prim, Prim::StateWrite { state } | Prim::HistAppend { state } if state == j)
        })
    };
    for (j, s) in p.states.iter().enumerate() {
        let j = j as u16;
        let b_state = match s.kind {
            StateKind::Fixed { .. } => (s.shape.iter().fold(1u64, |a, &d| a.saturating_mul(d as u64))).saturating_mul(width(s.dtype)),
            StateKind::Hist { window } => (window as u64)
                .saturating_mul(s.shape.iter().fold(1u64, |a, &d| a.saturating_mul(d as u64)))
                .saturating_mul(width(s.dtype)),
        };
        let instances = if s.per_layer {
            p.schedule.layers.iter().filter(|&&lb| uses(lb as usize, j)).count() as u64
        } else if (0..p.blocks.len()).any(|b| uses(b, j)) {
            1
        } else {
            0
        };
        pos.state_bytes = pos.state_bytes.saturating_add(b_state.saturating_mul(instances));
    }
    // Peak live bytes.
    for b in 0..p.blocks.len() {
        let block = &p.blocks[b];
        let n = block.nodes.len();
        let h = worst_h(p, b);
        let roots = roots_of(p, b);
        let mut end: Vec<usize> = (0..n).collect();
        for (i, node) in block.nodes.iter().enumerate() {
            for r in &node.inputs {
                if let Ref::Node(k) = *r {
                    end[k as usize] = end[k as usize].max(i);
                }
            }
        }
        for (i, r) in roots.iter().enumerate() {
            if *r {
                end[i] = n - 1;
            }
        }
        for t in 0..n {
            let live: u64 = (0..=t)
                .filter(|&i| end[i] >= t)
                .map(|i| elems(&block.nodes[i].out, h).saturating_mul(width(block.nodes[i].out.dtype)))
                .fold(0u64, |a, v| a.saturating_add(v));
            pos.peak_live_bytes = pos.peak_live_bytes.max(live);
        }
    }
    pos
}

// ------------------------------------------------------------------ §10.2 cones

/// The cone of `root` in block `b`: its nodes (the root and every node reached backwards through
/// `Node` refs without passing a commit point) and its leaves.
pub fn cone(p: &Program, b: usize, root: usize) -> (Vec<u16>, Vec<Leaf>) {
    let block = &p.blocks[b];
    let mut nodes: BTreeSet<usize> = BTreeSet::from([root]);
    let mut leaves: BTreeSet<Leaf> = BTreeSet::new();
    let mut work = vec![root];
    while let Some(i) = work.pop() {
        let node = &block.nodes[i];
        if let Prim::HistAppend { state } = node.prim {
            leaves.insert(Leaf::History(state));
        }
        for r in &node.inputs {
            match *r {
                Ref::Node(k) => {
                    if block.nodes[k as usize].commit {
                        leaves.insert(Leaf::Commit(k));
                    } else if nodes.insert(k as usize) {
                        work.push(k as usize);
                    }
                }
                Ref::CarryIn(k) => {
                    leaves.insert(Leaf::CarryIn(k));
                }
                Ref::Param(j) => {
                    leaves.insert(Leaf::Param(j));
                }
                Ref::Const(j) => {
                    leaves.insert(Leaf::Const(j));
                }
                Ref::State(j) => {
                    leaves.insert(Leaf::State(j));
                }
                Ref::Input(j) => {
                    leaves.insert(Leaf::Input(j));
                }
            }
        }
    }
    (nodes.into_iter().map(|i| i as u16).collect(), leaves.into_iter().collect())
}

fn cone_work_of(p: &Program, b: usize, nodes: &[u16]) -> u64 {
    nodes.iter().map(|&i| 1 + p.blocks[b].nodes[i as usize].inputs.len() as u64).sum()
}

/// The element count and opened width of a leaf at `H = h` (a committed leaf at 4 bytes a lane).
fn leaf_count(p: &Program, b: usize, leaf: &Leaf, h: u64, hist_cap: &BTreeMap<u16, u64>) -> (u64, u64) {
    match *leaf {
        Leaf::Commit(k) => (elems(&p.blocks[b].nodes[k as usize].out, h), 4),
        Leaf::CarryIn(k) => (elems(&p.blocks[b].carry_in[k as usize], h), 4),
        Leaf::State(j) => (p.states[j as usize].shape.iter().fold(1u64, |a, &d| a.saturating_mul(d as u64)), 4),
        Leaf::History(j) => (hist_cap.get(&j).copied().unwrap_or(0), 4),
        Leaf::Param(j) => {
            let d = &p.params[j as usize];
            (d.shape.iter().fold(1u64, |a, &x| a.saturating_mul(x as u64)), width(d.dtype))
        }
        Leaf::Const(j) => {
            let c = &p.consts[j as usize];
            (c.shape.iter().fold(1u64, |a, &x| a.saturating_mul(x as u64)), width(c.dtype))
        }
        Leaf::Input(_) => (1, 4),
    }
}

fn leaf_of(r: &Ref) -> Option<Leaf> {
    Some(match *r {
        Ref::Node(_) => return None,
        Ref::CarryIn(k) => Leaf::CarryIn(k),
        Ref::Param(j) => Leaf::Param(j),
        Ref::Const(j) => Leaf::Const(j),
        Ref::State(j) => Leaf::State(j),
        Ref::Input(j) => Leaf::Input(j),
    })
}

/// §10.3 box demand of one tile of `root` at `H = h`: the demanded cost and the opened bytes.
pub fn box_demand(p: &Program, b: usize, root: usize, nodes: &[u16], h: u64, tile_len: u64) -> (Cost, u64) {
    let block = &p.blocks[b];
    let in_cone: BTreeSet<usize> = nodes.iter().map(|&i| i as usize).collect();
    let mut d: BTreeMap<usize, u64> = BTreeMap::new();
    d.insert(root, tile_len.min(elems(&block.nodes[root].out, h)));
    let mut leaf_dem: BTreeMap<Leaf, u64> = BTreeMap::new();
    let mut hist_cap: BTreeMap<u16, u64> = BTreeMap::new();
    let mut cost = Cost::default();
    for &i in in_cone.iter().rev() {
        let di = d.get(&i).copied().unwrap_or(0);
        if di == 0 {
            continue;
        }
        let node = &block.nodes[i];
        let tys = input_types(p, b, i);
        // Demand on each operand, capped at its element count.
        let dem: Vec<u64> = tys
            .iter()
            .map(|t| {
                let raw = match &node.prim {
                    Prim::MatMul => di.saturating_mul(*tys[0].extents(h).last().unwrap()),
                    Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => di.saturating_mul(t.extents(h)[*axis as usize]),
                    Prim::TopK { axis, k } => di.div_ceil(*k as u64).saturating_mul(t.extents(h)[*axis as usize]),
                    _ => di,
                };
                raw.min(elems(t, h))
            })
            .collect();
        for (r, &x) in node.inputs.iter().zip(dem.iter()) {
            match *r {
                Ref::Node(k) => {
                    let k = k as usize;
                    if in_cone.contains(&k) {
                        let e = elems(&block.nodes[k].out, h);
                        let v = d.entry(k).or_insert(0);
                        *v = (*v).saturating_add(x).min(e);
                    } else {
                        let v = leaf_dem.entry(Leaf::Commit(k as u16)).or_insert(0);
                        *v = (*v).saturating_add(x);
                    }
                }
                _ => {
                    let v = leaf_dem.entry(leaf_of(r).unwrap()).or_insert(0);
                    *v = (*v).saturating_add(x);
                }
            }
        }
        if let Prim::HistAppend { state } = node.prim {
            let prior = elems(&node.out, h) - elems(&tys[0], h);
            *leaf_dem.entry(Leaf::History(state)).or_insert(0) += di.min(prior);
            hist_cap.insert(state, prior);
        }
        // The node's demanded cost.
        let wo = width(node.out.dtype);
        let read: u64 =
            tys.iter().zip(dem.iter()).map(|(t, &x)| x.saturating_mul(width(t.dtype))).fold(0u64, |a, v| a.saturating_add(v));
        let mut c = Cost { bytes_read: read, bytes_written: di.saturating_mul(wo), ..Cost::default() };
        match &node.prim {
            Prim::MatMul => c.macs = di.saturating_mul(*tys[0].extents(h).last().unwrap()),
            Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => c.elementwise = dem[0],
            Prim::TopK { k, .. } => c.elementwise = dem[0].saturating_mul(*k as u64),
            Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => c.transcendentals = di,
            _ => c.elementwise = di,
        }
        cost.add(&c);
    }
    let mut opened: u64 = 0;
    for (l, &x) in &leaf_dem {
        let (count, wd) = leaf_count(p, b, l, h, &hist_cap);
        opened = opened.saturating_add(x.min(count).saturating_mul(wd));
    }
    (cost, opened)
}

/// The nodes of a cone that reduce over `H` (§10.3 dissection).
fn h_reductions(p: &Program, b: usize, nodes: &[u16]) -> Vec<u16> {
    nodes
        .iter()
        .copied()
        .filter(|&i| {
            let n = &p.blocks[b].nodes[i as usize];
            let tys = input_types(p, b, i as usize);
            match &n.prim {
                Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => tys[0].shape[*axis as usize] == Dim::H,
                Prim::MatMul => tys[0].shape.last() == Some(&Dim::H),
                _ => false,
            }
        })
        .collect()
}

fn build_cone(p: &Program, b: usize, root: usize, inputs: &AdmitInputs) -> Cone {
    let (nodes, leaves) = cone(p, b, root);
    let wh = worst_h(p, b);
    let tl = inputs.tile_len as u64;
    let mut whole = Cost::default();
    for &i in &nodes {
        whole.add(&node_cost(p, b, i as usize, wh));
    }
    let (tile, tile_opened) = box_demand(p, b, root, &nodes, wh, tl);
    let hr = h_reductions(p, b, &nodes);
    let (chunk, chunk_opened) = if hr.is_empty() {
        (None, None)
    } else {
        let hc = (inputs.h_chunk as u64).min(wh);
        let (c, o) = box_demand(p, b, root, &nodes, hc, tl);
        (Some(c), Some(o))
    };
    Cone {
        block: b as u8,
        node: root as u16,
        tiles: elems(&p.blocks[b].nodes[root].out, wh).div_ceil(tl),
        operands: leaves.iter().filter(|l| l.committed()).count() as u64,
        nodes,
        leaves,
        whole,
        tile,
        tile_opened_bytes: tile_opened,
        h_reductions: hr,
        chunk,
        chunk_opened_bytes: chunk_opened,
    }
}

// ------------------------------------------------------------------ §10.3 replay, groups, C_j

/// §10.3 "Groups": the class of a node of the union of the closure's update cones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Al {
    Free,
    Aligned,
    Mixed,
}

/// The replay of written `Fixed` state `j` in block `b`: its closure, its groups and one group's
/// cost of one position.
fn replay(p: &Program, b: usize, j: u16) -> (Vec<u16>, u64, Cost) {
    let block = &p.blocks[b];
    let writer = |m: u16| block.nodes.iter().position(|n| n.prim == Prim::StateWrite { state: m });
    let update_cone = |m: u16| writer(m).map(|s| cone(p, b, s).0);
    // The closure: j and every state read by an update cone of a member, in this block.
    let mut closure: BTreeSet<u16> = BTreeSet::from([j]);
    let mut work = vec![j];
    while let Some(m) = work.pop() {
        if let Some(nodes) = update_cone(m) {
            for &i in &nodes {
                for r in &block.nodes[i as usize].inputs {
                    if let Ref::State(k) = *r
                        && closure.insert(k)
                    {
                        work.push(k);
                    }
                }
            }
        }
    }
    // The closure's update cones: those of the members this block writes.
    let cones: Vec<Vec<u16>> = closure.iter().filter_map(|&m| update_cone(m)).collect();
    let h = worst_h(p, b);
    // G: every member (written or only read) has the same first dimension G > 1.
    let firsts: BTreeSet<Option<u32>> = closure.iter().map(|&m| p.states[m as usize].shape.first().copied()).collect();
    let g = match firsts.iter().next() {
        Some(Some(g)) if firsts.len() == 1 && *g > 1 => *g,
        _ => 0,
    };
    let classes = if g > 1 {
        let union: BTreeSet<u16> = cones.iter().flatten().copied().collect();
        Some(classify(p, b, &union, g, &closure))
    } else {
        None
    };
    let split = classes.as_ref().is_some_and(|c| c.values().all(|&a| a != Al::Mixed));
    if !split {
        // Unsplit: the whole sum over the cones, one cone at a time.
        let mut total = Cost::default();
        for c in &cones {
            for &i in c {
                total.add(&node_cost(p, b, i as usize, h));
            }
        }
        return (closure.into_iter().collect(), 1, total);
    }
    let classes = classes.unwrap();
    // One group pays ⌈aligned / G⌉ plus every free node whole, each sum over the cones one cone at a
    // time (a node two cones share counts in each).
    let (mut aligned, mut free) = (Cost::default(), Cost::default());
    for c in &cones {
        for &i in c {
            let cost = node_cost(p, b, i as usize, h);
            match classes[&i] {
                Al::Aligned => aligned.add(&cost),
                _ => free.add(&cost),
            }
        }
    }
    let mut per = aligned.per_group(g as u64);
    per.add(&free);
    (closure.into_iter().collect(), g as u64, per)
}

/// §10.3: every node of the union, in ascending index order, as free, aligned or mixed.
fn classify(p: &Program, b: usize, union: &BTreeSet<u16>, g: u32, closure: &BTreeSet<u16>) -> BTreeMap<u16, Al> {
    let block = &p.blocks[b];
    let mut class: BTreeMap<u16, Al> = BTreeMap::new();
    for &i in union {
        let node = &block.nodes[i as usize];
        let ops: Vec<(Al, TensorType)> = node
            .inputs
            .iter()
            .map(|r| {
                let t = ref_type(p, b, i as usize, r).expect("normal form");
                let a = match *r {
                    // A closure state is aligned.
                    Ref::State(k) if closure.contains(&k) => Al::Aligned,
                    // A commit point (another member's committed StateWrite included) is free.
                    Ref::Node(k) if block.nodes[k as usize].commit => Al::Free,
                    // Any other node: its class (reached only through the cone, so classified).
                    Ref::Node(k) => *class.get(&k).expect("an uncommitted operand of a cone node is in the cone"),
                    // Params, consts, inputs and carry-ins are free.
                    _ => Al::Free,
                };
                (a, t)
            })
            .collect();
        let out = &node.out;
        let a = if ops.iter().all(|(a, _)| *a == Al::Free) {
            Al::Free
        } else if ops.iter().any(|(a, _)| *a == Al::Mixed) || out.shape.first() != Some(&Dim::Fixed(g)) {
            Al::Mixed
        } else {
            // Aligned in place: free, or aligned with the output's rank.
            let in_place = |o: &(Al, TensorType)| o.0 == Al::Free || (o.0 == Al::Aligned && o.1.shape.len() == out.shape.len());
            let ok = match &node.prim {
                Prim::Add
                | Prim::Sub
                | Prim::Mul
                | Prim::Div { .. }
                | Prim::Cast
                | Prim::Clamp { .. }
                | Prim::Log2Floor
                | Prim::IntExp
                | Prim::IntRsqrt
                | Prim::IntLn
                | Prim::Select
                | Prim::Compare { .. }
                | Prim::Broadcast
                | Prim::StateWrite { .. } => ops.iter().all(in_place),
                Prim::Transpose { perm } => perm.first() == Some(&0) && in_place(&ops[0]),
                Prim::Slice { axis, .. } | Prim::Concat { axis } => *axis != 0 && ops.iter().all(in_place),
                Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => *axis != 0 && in_place(&ops[0]),
                Prim::Reshape => ops[0].1.shape.first() == Some(&Dim::Fixed(g)),
                Prim::Gather { axis, .. } => *axis != 0 && ops[1].0 == Al::Free && in_place(&ops[0]),
                Prim::MatMul => {
                    if out.shape.len() >= 3 {
                        in_place(&ops[0]) && in_place(&ops[1])
                    } else {
                        ops[1].0 == Al::Free && in_place(&ops[0])
                    }
                }
                // Always free (no operand, or a committed row): unreachable here.
                Prim::Iota { .. } | Prim::HistAppend { .. } => false,
            };
            if ok { Al::Aligned } else { Al::Mixed }
        };
        class.insert(i, a);
    }
    class
}

// ------------------------------------------------------------------ admission

fn exceeds(limit: &'static str, at: String, value: u64, cap: u64) -> AdmitError {
    AdmitError::Exceeds { limit, at, value, cap }
}

fn check_inputs(inputs: &AdmitInputs) -> Result<(), AdmitError> {
    if !(1..=(1 << 16)).contains(&inputs.tile_len) {
        return Err(AdmitError::Inputs("tile_len outside [1, 2^16]"));
    }
    if !(1..=(1 << 16)).contains(&inputs.h_chunk) || !inputs.h_chunk.is_power_of_two() {
        return Err(AdmitError::Inputs("h_chunk is not a power of two in [1, 2^16]"));
    }
    if inputs.ceilings.max_checkpoint_interval < 1 {
        return Err(AdmitError::Inputs("max_checkpoint_interval below 1"));
    }
    Ok(())
}

/// Every ceiling a program breaks, as `(limit, value)` — for comparing refusals, since "which of
/// several broken ceilings a refusal names is not normative". The cone work is reported at the
/// first cone (in this crate's order) that passes it. Programs must already be in normal form and
/// pass §7.
pub fn broken_ceilings(p: &Program, inputs: &AdmitInputs) -> Vec<(&'static str, u64)> {
    let c = &inputs.ceilings;
    let mut v = Vec::new();
    let node_costs: Vec<Vec<Cost>> =
        (0..p.blocks.len()).map(|b| (0..p.blocks[b].nodes.len()).map(|i| node_cost(p, b, i, worst_h(p, b))).collect()).collect();
    let pos = position(p, &node_costs, inputs.tile_len as u64);
    if pos.cost.macs > c.max_position_macs {
        v.push(("max_position_macs", pos.cost.macs));
    }
    if pos.cost.transcendentals > c.max_position_transcendentals {
        v.push(("max_position_transcendentals", pos.cost.transcendentals));
    }
    if pos.state_bytes > c.max_state_bytes {
        v.push(("max_state_bytes", pos.state_bytes));
    }
    if pos.step_leaves > c.max_step_leaves {
        v.push(("max_step_leaves", pos.step_leaves));
    }
    let mut work: u64 = 0;
    let mut work_broken = false;
    let mut count = |w: u64, v: &mut Vec<(&'static str, u64)>| {
        work = work.saturating_add(w);
        if !work_broken && work > c.max_cone_work {
            work_broken = true;
            v.push(("max_cone_work", work));
        }
    };
    for b in 0..p.blocks.len() {
        for i in 0..p.blocks[b].nodes.len() {
            if p.blocks[b].nodes[i].commit {
                let cn = build_cone(p, b, i, inputs);
                count(cone_work_of(p, b, &cn.nodes), &mut v);
                tile_ceilings(&cn, c, &mut v);
            }
        }
    }
    for b in 0..p.blocks.len() {
        for i in 0..p.blocks[b].nodes.len() {
            let n = &p.blocks[b].nodes[i];
            if matches!(n.prim, Prim::StateWrite { .. }) {
                let (nodes, _) = cone(p, b, i);
                count(cone_work_of(p, b, &nodes), &mut v);
            }
        }
    }
    for (j, s) in p.states.iter().enumerate() {
        if !matches!(s.kind, StateKind::Fixed { .. }) {
            continue;
        }
        for b in 0..p.blocks.len() {
            if p.blocks[b].nodes.iter().any(|n| n.prim == Prim::StateWrite { state: j as u16 }) {
                let (_, _, per) = replay(p, b, j as u16);
                if per.macs > c.max_tile_macs {
                    v.push(("max_tile_macs", per.macs));
                }
                if per.transcendentals > c.max_tile_transcendentals {
                    v.push(("max_tile_transcendentals", per.transcendentals));
                }
            }
        }
    }
    v
}

fn tile_ceilings(cn: &Cone, c: &Ceilings, v: &mut Vec<(&'static str, u64)>) {
    let t = cn.terminal();
    if t.macs > c.max_tile_macs {
        v.push(("max_tile_macs", t.macs));
    }
    if t.transcendentals > c.max_tile_transcendentals {
        v.push(("max_tile_transcendentals", t.transcendentals));
    }
    if cn.terminal_opened_bytes() > c.max_tile_opened_bytes {
        v.push(("max_tile_opened_bytes", cn.terminal_opened_bytes()));
    }
    if cn.operands > c.max_tile_operands {
        v.push(("max_tile_operands", cn.operands));
    }
}

/// `C_j` of one block's replay (§10.3); a zero component imposes no bound.
fn c_j(per: &Cost, c: &Ceilings) -> u64 {
    let mut v = c.max_checkpoint_interval as u64;
    if per.macs > 0 {
        v = v.min(c.max_tile_macs / per.macs);
    }
    if per.transcendentals > 0 {
        v = v.min(c.max_tile_transcendentals / per.transcendentals);
    }
    v
}

/// `tir_admit_v1`: the canonical bytes and the network inputs → the admission or a refusal.
pub fn admit(bytes: &[u8], inputs: &AdmitInputs) -> Result<Admission, AdmitError> {
    check_inputs(inputs)?;
    let p = decode_canonical(bytes).map_err(AdmitError::Program)?;
    admit_decoded(p, inputs)
}

/// Admission of a program already decoded (its normal form is checked again).
pub fn admit_program(p: &Program, inputs: &AdmitInputs) -> Result<Admission, AdmitError> {
    check_inputs(inputs)?;
    crate::normal_form::check(p).map_err(AdmitError::Program)?;
    admit_decoded(p.clone(), inputs)
}

fn admit_decoded(p: Program, inputs: &AdmitInputs) -> Result<Admission, AdmitError> {
    let c = inputs.ceilings;
    // 2. ranges
    let intervals = ranges(&p).map_err(AdmitError::Program)?;
    // 3. costs and the position
    let node_costs: Vec<Vec<Cost>> =
        (0..p.blocks.len()).map(|b| (0..p.blocks[b].nodes.len()).map(|i| node_cost(&p, b, i, worst_h(&p, b))).collect()).collect();
    let pos = position(&p, &node_costs, inputs.tile_len as u64);
    if pos.cost.macs > c.max_position_macs {
        return Err(exceeds("max_position_macs", "the position".into(), pos.cost.macs, c.max_position_macs));
    }
    if pos.cost.transcendentals > c.max_position_transcendentals {
        return Err(exceeds(
            "max_position_transcendentals",
            "the position".into(),
            pos.cost.transcendentals,
            c.max_position_transcendentals,
        ));
    }
    if pos.state_bytes > c.max_state_bytes {
        return Err(exceeds("max_state_bytes", "the position".into(), pos.state_bytes, c.max_state_bytes));
    }
    if pos.step_leaves > c.max_step_leaves {
        return Err(exceeds("max_step_leaves", "the position".into(), pos.step_leaves, c.max_step_leaves));
    }
    // 4. cones, with the cone work counted as they are built.
    let mut work: u64 = 0;
    let mut cones = Vec::new();
    for b in 0..p.blocks.len() {
        for i in 0..p.blocks[b].nodes.len() {
            if !p.blocks[b].nodes[i].commit {
                continue;
            }
            let (nodes, _) = cone(&p, b, i);
            work = work.saturating_add(cone_work_of(&p, b, &nodes));
            if work > c.max_cone_work {
                return Err(exceeds("max_cone_work", format!("block {b} node {i}"), work, c.max_cone_work));
            }
            let cn = build_cone(&p, b, i, inputs);
            let mut broken = Vec::new();
            tile_ceilings(&cn, &c, &mut broken);
            if let Some(&(limit, value)) = broken.first() {
                let cap = match limit {
                    "max_tile_macs" => c.max_tile_macs,
                    "max_tile_transcendentals" => c.max_tile_transcendentals,
                    "max_tile_opened_bytes" => c.max_tile_opened_bytes,
                    _ => c.max_tile_operands,
                };
                return Err(exceeds(limit, format!("block {b} node {i}"), value, cap));
            }
            cones.push(cn);
        }
    }
    // 5. every StateWrite's update cone against max_cone_work, by block then node — a committed
    // StateWrite's cone counts in both terms (§10.3) — then every written Fixed state by state index,
    // over the blocks that write it by block index, against its C_j.
    for b in 0..p.blocks.len() {
        for i in 0..p.blocks[b].nodes.len() {
            let n = &p.blocks[b].nodes[i];
            if matches!(n.prim, Prim::StateWrite { .. }) {
                let (nodes, _) = cone(&p, b, i);
                work = work.saturating_add(cone_work_of(&p, b, &nodes));
                if work > c.max_cone_work {
                    return Err(exceeds("max_cone_work", format!("block {b} node {i}"), work, c.max_cone_work));
                }
            }
        }
    }
    let mut states = Vec::new();
    for (j, s) in p.states.iter().enumerate() {
        if !matches!(s.kind, StateKind::Fixed { .. }) {
            continue;
        }
        let j = j as u16;
        let mut best: Option<StateCkpt> = None;
        for b in 0..p.blocks.len() {
            if !p.blocks[b].nodes.iter().any(|n| n.prim == Prim::StateWrite { state: j }) {
                continue;
            }
            let (closure, groups, per) = replay(&p, b, j);
            let cj = c_j(&per, &c);
            if cj == 0 {
                // §10.3: a C_j of 0 names the component past its cap — max_tile_macs if one group's
                // replay of one position has more MACs than it, otherwise max_tile_transcendentals
                // (max_checkpoint_interval ≥ 1 is an input rule).
                let at = format!("the replay of state {j} in block {b}");
                return Err(if per.macs > c.max_tile_macs {
                    exceeds("max_tile_macs", at, per.macs, c.max_tile_macs)
                } else {
                    exceeds("max_tile_transcendentals", at, per.transcendentals, c.max_tile_transcendentals)
                });
            }
            let ck = StateCkpt { state: j, closure, groups, per_position: per, interval: cj.min(u32::MAX as u64) as u32 };
            if best.as_ref().is_none_or(|x| ck.interval < x.interval) {
                best = Some(ck);
            }
        }
        // §10.3 (A7): a Fixed state no block writes has no C_j and does not enter C.
        if let Some(ck) = best {
            states.push(ck);
        }
    }
    let checkpoint_interval = states.iter().map(|s| s.interval).min().unwrap_or(c.max_checkpoint_interval);
    Ok(Admission { program: p, intervals, node_costs, position: pos, cones, states, checkpoint_interval, cone_work: work })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_per_group_rounds_up() {
        let c = Cost { macs: 10, elementwise: 9, transcendentals: 0, bytes_read: 1, bytes_written: 12 };
        assert_eq!(c.per_group(4), Cost { macs: 3, elementwise: 3, transcendentals: 0, bytes_read: 1, bytes_written: 3 });
    }
}
