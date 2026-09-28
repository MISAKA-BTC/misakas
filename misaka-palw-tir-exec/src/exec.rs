//! The executor: one position step over typed buffers, with the reference evaluator's semantics
//! (spec 04b §9.1) — occurrences in schedule order, nodes in index order, effects applied only
//! after the whole step succeeded — and the values it produces at every node.
//!
//! **State without copies.** A `Fixed` state instance is double-buffered: `StateWrite` writes the
//! pending buffer, every `Ref::State` of the step reads the current one (the value at the start
//! of the position), and success swaps the two. A `Hist` instance is an append-only buffer of rows
//! whose visible window is one contiguous run: `HistAppend` writes its row just past the window and
//! its output is a view of the rows; rows that fall out of the window are compacted away only when
//! the dead prefix outgrows the live window (amortised one row per step), never per step.

use misaka_palw_tir::program::{StateKind, TirProgramV1};
use misaka_palw_tir::{DType, Dim, Prim, Ref, RunState, Tensor, TirError, TirErrorKind, TirResult};

use crate::elem::{Buf, Elem, Slice};
use crate::kernels::{Opd, Scratch, elementwise, materialize, matmul, misc, reduce};
use crate::layout::{Layout, MAX_RANK, numel};
use crate::params::TirParams;
use crate::plan::{BlockPlan, NodePlan, TirPlan};
use crate::{with_buf_mut, with_slice};

/// Where a value's elements live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Src {
    Slot(u16),
    Param(u16),
    Const(u16),
    Fixed(u32),
    FixedNext(u32),
    Hist(u32),
    Carry(u8),
    Input,
}

/// A node's value: storage and the layout that names its elements.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Val {
    pub src: Src,
    pub layout: Layout,
}

impl Default for Val {
    fn default() -> Self {
        Val { src: Src::Input, layout: Layout::contiguous(&[]) }
    }
}

/// One node's value as the executor produced it (always contiguous here).
pub struct NodeValue<'v> {
    /// The node slot of spec 04b §3.3.
    pub slot: u32,
    pub block: u8,
    pub layer: Option<u16>,
    pub node: u16,
    pub commit: bool,
    pub dtype: DType,
    pub shape: &'v [usize],
    pub data: Slice<'v>,
}

impl NodeValue<'_> {
    /// The value as the reference evaluator's tensor (tests, tools).
    pub fn to_tensor(&self) -> Tensor {
        Tensor { dtype: self.dtype, shape: self.shape.to_vec(), data: self.data.to_i128s() }
    }
}

/// Receives a step's committed values in slot order — and, when it asks, every node's value.
///
/// A step that fails has delivered some values before the failure; they are not part of any
/// result and the receiver discards them.
pub trait StepSink {
    /// Also deliver the value of every uncommitted node.
    fn every_node(&self) -> bool {
        false
    }
    fn node(&mut self, v: &NodeValue<'_>);
}

/// A sink that keeps nothing.
pub struct NoSink;
impl StepSink for NoSink {
    fn node(&mut self, _: &NodeValue<'_>) {}
}

/// A `Hist` instance: rows `start .. start + rows` of `data` are the visible prior rows, oldest
/// first; this step's appended rows sit just past them until the step commits.
#[derive(Clone, Debug, Default)]
pub(crate) struct HistBuf {
    pub data: Buf,
    pub row: usize,
    pub row_shape: Vec<usize>,
    pub start: usize,
    pub rows: usize,
    pub window: u32,
    pub pending: usize,
}

/// Where an appended window's elements are.
pub(crate) enum HistOut {
    /// Contiguous in the history buffer.
    View(Layout),
    /// A second append of the same history in one step: the prior rows and this row are not
    /// adjacent; the executor copies them.
    Split { prior_offset: usize, prior_elems: usize, row_offset: usize },
}

impl HistBuf {
    fn new(dtype: DType, row_shape: &[usize], window: u32) -> Self {
        HistBuf {
            data: Buf::empty(dtype),
            row: numel(row_shape),
            row_shape: row_shape.to_vec(),
            start: 0,
            rows: 0,
            window,
            pending: 0,
        }
    }

    /// Append `row` tentatively at position `pos` and describe the window `[H, ..row]`.
    fn append(&mut self, row: &Opd<'_>, pos: u32) -> TirResult<HistOut> {
        let want = (pos as usize).min(self.window as usize - 1);
        if self.rows != want {
            return Err(TirError::new(TirErrorKind::Position, format!("history: {} prior rows, want {want}", self.rows)));
        }
        let live = self.rows + self.pending;
        if (self.start + live + 1) * self.row > self.data.len() {
            if self.start > live {
                // Compact: the dead prefix is larger than what is still visible.
                let (from, n) = (self.start * self.row, live * self.row);
                with_buf_mut!(&mut self.data, v => v.copy_within(from..from + n, 0));
                self.start = 0;
            }
            let need = (self.start + live + 1) * self.row;
            if need > self.data.len() {
                let grow = need.max(self.data.len() * 2);
                with_buf_mut!(&mut self.data, v => v.resize(grow, Default::default()));
            }
        }
        let at = (self.start + live) * self.row;
        with_buf_mut!(&mut self.data, v => copy_row(v, at, row));
        let out = if self.pending == 0 {
            let mut shape = vec![self.rows + 1];
            shape.extend_from_slice(&self.row_shape);
            HistOut::View(Layout::contiguous_at(&shape, self.start * self.row))
        } else {
            HistOut::Split { prior_offset: self.start * self.row, prior_elems: self.rows * self.row, row_offset: at }
        };
        self.pending += 1;
        Ok(out)
    }

    /// The step succeeded: the appended rows become history, keeping `window − 1` of them.
    fn commit(&mut self) {
        if self.window == 0 {
            // The placeholder of a Fixed instance.
            return;
        }
        let total = self.rows + self.pending;
        let keep = total.min(self.window as usize - 1);
        self.start += total - keep;
        self.rows = keep;
        self.pending = 0;
    }
}

fn copy_row<T: Elem>(dst: &mut [T], at: usize, row: &Opd<'_>) {
    // The row's declared dtype is the state's (type rule); its storage may be narrower (a view of
    // an operand of another width whose values provably fit), so it converts.
    with_slice!(row.data, src => {
        let mut i = at;
        row.layout.for_each_run(|s, len, st| {
            for t in 0..len {
                dst[i + t] = T::from_i128(src[s + t * st].to_i128());
            }
            i += len;
        });
    });
}

/// Resolve a declared shape at the running `H`.
#[inline]
pub(crate) fn resolve(dims: &[Dim], h: usize) -> ([usize; MAX_RANK], usize) {
    let mut s = [1usize; MAX_RANK];
    for (i, d) in dims.iter().enumerate() {
        s[i] = d.at(h);
    }
    (s, dims.len())
}

/// Read access to every value a node may name, for one occurrence.
pub(crate) struct Reader<'r> {
    pub plan: &'r TirPlan,
    pub params: &'r TirParams<'r>,
    pub layer: Option<u16>,
    pub fixed: &'r [Buf],
    pub fixed_next: &'r [Buf],
    pub hist: &'r [HistBuf],
    pub carry: &'r [Buf],
    pub slots: &'r [Buf],
    pub inputs: &'r [u32; 2],
}

impl<'r> Reader<'r> {
    pub fn data(&self, src: Src) -> TirResult<Slice<'r>> {
        Ok(match src {
            Src::Slot(i) => self.slots[i as usize].slice(),
            Src::Param(j) => self.params.get(j, self.layer).ok_or_else(|| {
                TirError::new(
                    TirErrorKind::Missing,
                    format!("param {} (layer {:?})", self.plan.program.params[j as usize].name, self.layer),
                )
            })?,
            Src::Const(j) => self.plan.consts[j as usize].slice(),
            Src::Fixed(k) => self.fixed[k as usize].slice(),
            Src::FixedNext(k) => self.fixed_next[k as usize].slice(),
            Src::Hist(k) => self.hist[k as usize].data.slice(),
            Src::Carry(k) => {
                self.carry.get(k as usize).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("carry-in {k}")))?.slice()
            }
            Src::Input => Slice::Idx(&self.inputs[..]),
        })
    }

    pub fn opd(&self, v: Val) -> TirResult<Opd<'r>> {
        Ok(Opd { data: self.data(v.src)?, layout: v.layout })
    }
}

/// The value a `Ref` names in an occurrence (before any node of it is evaluated for leaves).
pub(crate) fn ref_val(plan: &TirPlan, bp: &BlockPlan, r: Ref, vals: &[Val], layer: Option<u16>) -> TirResult<Val> {
    let p = &plan.program;
    Ok(match r {
        Ref::Node(j) => vals[j as usize],
        Ref::CarryIn(k) => {
            let (s, n) = resolve(&bp.carry_in[k as usize].shape, 1);
            Val { src: Src::Carry(k), layout: Layout::contiguous(&s[..n]) }
        }
        Ref::Param(j) => {
            let sh: Vec<usize> = p.params[j as usize].shape.iter().map(|d| *d as usize).collect();
            Val { src: Src::Param(j), layout: Layout::contiguous(&sh) }
        }
        Ref::Const(j) => {
            let sh: Vec<usize> = p.consts[j as usize].shape.iter().map(|d| *d as usize).collect();
            Val { src: Src::Const(j), layout: Layout::contiguous(&sh) }
        }
        Ref::State(j) => {
            let inst =
                plan.instance(j, layer).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("state {j} at {layer:?}")))?;
            Val { src: Src::Fixed(inst), layout: Layout::contiguous(&plan.instances[inst as usize].shape) }
        }
        Ref::Input(k) => Val { src: Src::Input, layout: Layout::contiguous_at(&[], k as usize) },
    })
}

/// Evaluate one computing node (everything but `StateWrite` and `HistAppend`). Returns the value
/// as a view (`Some`) or `None` when the result was written to `out`.
pub(crate) fn eval_compute(
    plan: &TirPlan,
    bp: &BlockPlan,
    node: &NodePlan,
    rd: &Reader<'_>,
    vals: &[Val],
    out_shape: &[usize],
    out: &mut Buf,
    scratch: &mut Scratch,
) -> TirResult<Option<Val>> {
    let layer = rd.layer;
    let v = |i: usize| ref_val(plan, bp, node.inputs[i], vals, layer);
    let o = |i: usize| rd.opd(v(i)?);
    match &node.prim {
        Prim::Reshape => {
            let x = v(0)?;
            if x.layout.is_contiguous() {
                return Ok(Some(Val { src: x.src, layout: x.layout.reshaped(out_shape) }));
            }
            materialize(&rd.opd(x)?, out);
            Ok(None)
        }
        Prim::Transpose { perm } => {
            let x = v(0)?;
            Ok(Some(Val { src: x.src, layout: x.layout.transposed(perm) }))
        }
        Prim::Slice { axis, start } => {
            let x = v(0)?;
            let a = *axis as usize;
            Ok(Some(Val { src: x.src, layout: x.layout.sliced(a, *start as usize, out_shape[a]) }))
        }
        Prim::Broadcast => {
            let x = v(0)?;
            Ok(Some(Val { src: x.src, layout: x.layout.broadcast_to(out_shape) }))
        }
        Prim::Gather { axis, batch_dims } => {
            let (a, b) = (*axis as usize, *batch_dims as usize);
            let d = v(0)?;
            let idx = o(1)?;
            if idx.layout.rank == 0 && b == 0 {
                // One row along `axis`: a view of the data (the embedding row, a table entry).
                let n = d.layout.shape[a];
                let i = misc::scalar_index(&idx, n)?;
                let mut l = d.layout;
                l.offset += i * l.strides[a];
                return Ok(Some(Val { src: d.src, layout: l.without_axis(a) }));
            }
            misc::gather(node, &rd.opd(d)?, &idx, out, scratch)?;
            Ok(None)
        }
        Prim::Concat { .. } => {
            let ins: Vec<Opd<'_>> = (0..node.inputs.len()).map(o).collect::<TirResult<_>>()?;
            misc::concat(node, &ins, out_shape, out)?;
            Ok(None)
        }
        Prim::Iota { .. } => {
            misc::iota(node, out_shape, out)?;
            Ok(None)
        }
        Prim::Cast | Prim::Clamp { .. } if node.identity && rd.data(v(0)?.src)?.dtype() == node.store => {
            // The value cannot change (the input interval lies inside the clamp, or the cast
            // cannot fail) and is already held in this node's storage type: the output is the
            // input.
            Ok(Some(v(0)?))
        }
        Prim::Select => {
            let c = o(0)?;
            if c.numel() == 1 && !node.check_out {
                // One condition for the whole output: it is one operand, whole, when its shape is
                // the output's and it is held in this node's storage type (the first-position lane
                // choosing a parameter set).
                let pick = if c.data.get(c.layout.offset) != 0 { v(1)? } else { v(2)? };
                if pick.layout.shape() == out_shape && rd.data(pick.src)?.dtype() == node.store {
                    return Ok(Some(pick));
                }
            }
            elementwise::select(node, &c, &o(1)?, &o(2)?, out_shape, out, scratch)?;
            Ok(None)
        }
        Prim::Cast | Prim::Clamp { .. } | Prim::Log2Floor | Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
            elementwise::unary(node, &o(0)?, out, scratch, &plan.program.states)?;
            Ok(None)
        }
        Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
            elementwise::binary(node, &o(0)?, &o(1)?, out_shape, out, scratch)?;
            Ok(None)
        }
        Prim::MatMul => {
            matmul::matmul(node, &o(0)?, &o(1)?, out_shape, out, scratch)?;
            Ok(None)
        }
        Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => {
            reduce::reduce(node, &o(0)?, out, scratch)?;
            Ok(None)
        }
        Prim::TopK { .. } => {
            misc::topk(node, &o(0)?, out, scratch)?;
            Ok(None)
        }
        Prim::StateWrite { .. } | Prim::HistAppend { .. } => {
            Err(TirError::new(TirErrorKind::Shape, "state primitives are the executor's"))
        }
    }
}

/// Mutable run state: position, `Fixed` instances (current and pending), `Hist` instances.
pub(crate) struct RunBufs {
    pub pos: u32,
    pub fixed: Vec<Buf>,
    pub fixed_next: Vec<Buf>,
    pub written: Vec<bool>,
    pub hist: Vec<HistBuf>,
}

impl RunBufs {
    pub fn initial(plan: &TirPlan) -> Self {
        let mut fixed = Vec::with_capacity(plan.instances.len());
        let mut hist = Vec::with_capacity(plan.instances.len());
        for inst in &plan.instances {
            match inst.kind {
                StateKind::Fixed { .. } => {
                    fixed.push(Buf::zeros(inst.dtype, numel(&inst.shape)));
                    hist.push(HistBuf::default());
                }
                StateKind::Hist { window } => {
                    fixed.push(Buf::empty(inst.dtype));
                    hist.push(HistBuf::new(inst.dtype, &inst.shape, window));
                }
            }
        }
        let n = plan.instances.len();
        RunBufs { pos: 0, fixed_next: fixed.clone(), fixed, written: vec![false; n], hist }
    }
}

/// Per-block reusable storage of one executor.
struct WorkBufs {
    slots: Vec<Vec<Buf>>,
    vals: Vec<Vec<Val>>,
    carry: Vec<Buf>,
    carry_next: Vec<Buf>,
    scratch: Scratch,
    commit: Buf,
    logits: Buf,
    logits_shape: Vec<usize>,
    inputs: [u32; 2],
    /// Per primitive tag: nanoseconds and node evaluations (when profiling).
    profile: Option<Box<[(u64, u64); 25]>>,
    /// Per `(block, node)`: nanoseconds (when profiling).
    node_profile: Vec<Vec<u64>>,
}

/// Runs one program over positions, holding its run state.
pub struct TirExecutor<'a> {
    plan: &'a TirPlan,
    params: &'a TirParams<'a>,
    /// Per occurrence: the block plan refined by the actual ranges of the bound params.
    occ_plans: Vec<BlockPlan>,
    run: RunBufs,
    work: WorkBufs,
}

impl<'a> TirExecutor<'a> {
    /// An executor at position 0 with the initial state. Every param instance the plan reads
    /// must be bound (else `Missing`, the error every step of the reference would report).
    pub fn new(plan: &'a TirPlan, params: &'a TirParams<'a>) -> TirResult<Self> {
        params.check_complete(plan)?;
        let nb = plan.blocks.len();
        let occ_plans = plan.refine(&|j, l| params.range(j, l));
        Ok(TirExecutor {
            plan,
            params,
            occ_plans,
            run: RunBufs::initial(plan),
            work: WorkBufs {
                slots: (0..nb).map(|b| vec![Buf::default(); plan.blocks[b].nodes.len()]).collect(),
                vals: (0..nb).map(|b| vec![Val::default(); plan.blocks[b].nodes.len()]).collect(),
                carry: Vec::new(),
                carry_next: Vec::new(),
                scratch: Scratch::default(),
                commit: Buf::default(),
                logits: Buf::default(),
                logits_shape: Vec::new(),
                inputs: [0, 0],
                profile: None,
                node_profile: (0..nb).map(|b| vec![0; plan.blocks[b].nodes.len()]).collect(),
            },
        })
    }

    pub fn plan(&self) -> &TirPlan {
        self.plan
    }

    /// The next position to compute.
    pub fn pos(&self) -> u32 {
        self.run.pos
    }

    /// Accumulate the time spent per primitive (for benchmarks; off by default).
    pub fn set_profile(&mut self, on: bool) {
        self.work.profile = on.then(|| Box::new([(0u64, 0u64); 25]));
    }

    /// `(nanoseconds, evaluations)` per primitive tag since profiling was switched on.
    pub fn profile(&self) -> Option<&[(u64, u64); 25]> {
        self.work.profile.as_deref()
    }

    /// Nanoseconds per `(block, node)` since profiling was switched on.
    pub fn node_profile(&self) -> &[Vec<u64>] {
        &self.work.node_profile
    }

    /// The last successful step's logits.
    pub fn logits(&self) -> (&[usize], Slice<'_>) {
        (&self.work.logits_shape, self.work.logits.slice())
    }

    /// One position: `token` at [`Self::pos`]. On success the state advances; on failure it is
    /// exactly as before (spec 04b §9.1(4)).
    pub fn step(&mut self, token: u32, sink: &mut dyn StepSink) -> TirResult<()> {
        let p = &self.plan.program;
        let pos = self.run.pos;
        if pos >= p.history_bound {
            return Err(TirError::new(TirErrorKind::Position, format!("position {pos} ≥ history_bound {}", p.history_bound)));
        }
        if self.plan.reads_token && token >= p.token_bound {
            return Err(TirError::new(TirErrorKind::Operand, format!("token {token} ≥ token_bound {}", p.token_bound)));
        }
        self.work.inputs = [token, pos];
        let r = self.step_inner(pos, sink);
        let run = &mut self.run;
        match r {
            Ok(()) => {
                for (k, w) in run.written.iter_mut().enumerate() {
                    if std::mem::replace(w, false) {
                        std::mem::swap(&mut run.fixed[k], &mut run.fixed_next[k]);
                    }
                }
                for h in run.hist.iter_mut() {
                    h.commit();
                }
                run.pos += 1;
                Ok(())
            }
            Err(e) => {
                run.written.iter_mut().for_each(|w| *w = false);
                run.hist.iter_mut().for_each(|h| h.pending = 0);
                Err(e)
            }
        }
    }

    fn step_inner(&mut self, pos: u32, sink: &mut dyn StepSink) -> TirResult<()> {
        let plan = self.plan;
        let params = self.params;
        let every = sink.every_node();
        let n_occ = plan.occurrences.len();
        let TirExecutor { run, work, occ_plans, .. } = self;
        work.carry.clear();
        for (occ, &(block, layer)) in plan.occurrences.iter().enumerate() {
            let bp = &occ_plans[occ];
            let h = bp.window.map(|w| (pos as usize + 1).min(w as usize)).unwrap_or(1);
            let n = bp.nodes.len();
            let mut slots = std::mem::take(&mut work.slots[block as usize]);
            let mut vals = std::mem::take(&mut work.vals[block as usize]);
            slots.resize_with(n, Buf::default);
            vals.resize(n, Val::default());
            let base = plan.slot_bases[occ];
            for ni in 0..n {
                let node = &bp.nodes[ni];
                let started = work.profile.is_some().then(std::time::Instant::now);
                let (shape, rank) = resolve(&node.out.shape, h);
                let out_shape = &shape[..rank];
                let val = match node.prim {
                    Prim::StateWrite { state } => {
                        let inst =
                            plan.instance(state, layer).ok_or_else(|| TirError::new(TirErrorKind::Missing, "state instance"))?;
                        let mut buf = std::mem::take(&mut run.fixed_next[inst as usize]);
                        let r = {
                            let rd = Reader {
                                plan,
                                params,
                                layer,
                                fixed: &run.fixed,
                                fixed_next: &run.fixed_next,
                                hist: &run.hist,
                                carry: &work.carry,
                                slots: &slots,
                                inputs: &work.inputs,
                            };
                            rd.opd(ref_val(plan, bp, node.inputs[0], &vals, layer)?)
                                .and_then(|x| elementwise::unary(node, &x, &mut buf, &mut work.scratch, &plan.program.states))
                        };
                        run.fixed_next[inst as usize] = buf;
                        r?;
                        run.written[inst as usize] = true;
                        Val { src: Src::FixedNext(inst), layout: Layout::contiguous(out_shape) }
                    }
                    Prim::HistAppend { state } => {
                        let inst =
                            plan.instance(state, layer).ok_or_else(|| TirError::new(TirErrorKind::Missing, "history instance"))?;
                        let mut hb = std::mem::take(&mut run.hist[inst as usize]);
                        let r = {
                            let rd = Reader {
                                plan,
                                params,
                                layer,
                                fixed: &run.fixed,
                                fixed_next: &run.fixed_next,
                                hist: &run.hist,
                                carry: &work.carry,
                                slots: &slots,
                                inputs: &work.inputs,
                            };
                            rd.opd(ref_val(plan, bp, node.inputs[0], &vals, layer)?).and_then(|row| hb.append(&row, pos))
                        };
                        let r = r.map(|out| match out {
                            HistOut::View(l) => Val { src: Src::Hist(inst), layout: l },
                            HistOut::Split { prior_offset, prior_elems, row_offset } => {
                                let row = hb.row;
                                with_slice!(hb.data.slice(), v => {
                                    let o = crate::kernels::out_vec(&mut slots[ni]);
                                    o.clear();
                                    o.extend_from_slice(&v[prior_offset..prior_offset + prior_elems]);
                                    o.extend_from_slice(&v[row_offset..row_offset + row]);
                                });
                                Val { src: Src::Slot(ni as u16), layout: Layout::contiguous(out_shape) }
                            }
                        });
                        run.hist[inst as usize] = hb;
                        r?
                    }
                    _ => {
                        let mut out = std::mem::take(&mut slots[ni]);
                        let r = {
                            let rd = Reader {
                                plan,
                                params,
                                layer,
                                fixed: &run.fixed,
                                fixed_next: &run.fixed_next,
                                hist: &run.hist,
                                carry: &work.carry,
                                slots: &slots,
                                inputs: &work.inputs,
                            };
                            eval_compute(plan, bp, node, &rd, &vals, out_shape, &mut out, &mut work.scratch)
                        };
                        slots[ni] = out;
                        match r? {
                            Some(view) => view,
                            None => Val { src: Src::Slot(ni as u16), layout: Layout::contiguous(out_shape) },
                        }
                    }
                };
                vals[ni] = val;
                if let (Some(t0), Some(prof)) = (started, work.profile.as_mut()) {
                    let ns = t0.elapsed().as_nanos() as u64;
                    let e = &mut prof[node.prim.tag() as usize];
                    e.0 += ns;
                    e.1 += 1;
                    work.node_profile[block as usize][ni] += ns;
                }
                if node.commit || every {
                    let rd = Reader {
                        plan,
                        params,
                        layer,
                        fixed: &run.fixed,
                        fixed_next: &run.fixed_next,
                        hist: &run.hist,
                        carry: &work.carry,
                        slots: &slots,
                        inputs: &work.inputs,
                    };
                    let opd = rd.opd(val)?;
                    let data = if val.layout.is_contiguous() {
                        opd.data.sub(val.layout.offset, val.layout.numel())
                    } else {
                        materialize(&opd, &mut work.commit);
                        work.commit.slice()
                    };
                    sink.node(&NodeValue {
                        slot: base + ni as u32,
                        block,
                        layer,
                        node: ni as u16,
                        commit: node.commit,
                        dtype: node.out.dtype,
                        shape: out_shape,
                        data,
                    });
                }
            }
            // Carry to the next occurrence, or the logits.
            {
                let rd = Reader {
                    plan,
                    params,
                    layer,
                    fixed: &run.fixed,
                    fixed_next: &run.fixed_next,
                    hist: &run.hist,
                    carry: &work.carry,
                    slots: &slots,
                    inputs: &work.inputs,
                };
                if occ + 1 == n_occ {
                    let lv = vals[plan.program.logits as usize];
                    materialize(&rd.opd(lv)?, &mut work.logits);
                    work.logits_shape.clear();
                    work.logits_shape.extend_from_slice(lv.layout.shape());
                } else {
                    work.carry_next.resize_with(bp.carry_out.len(), Buf::default);
                    for (k, c) in bp.carry_out.iter().enumerate() {
                        materialize(&rd.opd(vals[*c as usize])?, &mut work.carry_next[k]);
                    }
                }
            }
            if occ + 1 != n_occ {
                std::mem::swap(&mut work.carry, &mut work.carry_next);
            }
            work.slots[block as usize] = slots;
            work.vals[block as usize] = vals;
        }
        Ok(())
    }

    /// The run state in the reference evaluator's form (every instance present).
    pub fn export_state(&self) -> RunState {
        let mut st = RunState { pos: self.run.pos, ..Default::default() };
        for (k, inst) in self.plan.instances.iter().enumerate() {
            let key = (inst.state, inst.layer);
            match inst.kind {
                StateKind::Fixed { .. } => {
                    st.fixed.insert(key, Tensor { dtype: inst.dtype, shape: inst.shape.clone(), data: self.run.fixed[k].to_i128s() });
                }
                StateKind::Hist { .. } => {
                    let h = &self.run.hist[k];
                    let all = h.data.to_i128s();
                    let rows = (0..h.rows)
                        .map(|r| {
                            let at = (h.start + r) * h.row;
                            Tensor { dtype: inst.dtype, shape: inst.shape.clone(), data: all[at..at + h.row].to_vec() }
                        })
                        .collect();
                    st.hist.insert(key, rows);
                }
            }
        }
        st
    }

    /// Resume from a run state (a checkpoint): every value is checked against its declaration
    /// as the reference checks what it reads — dtype, shape, a `Fixed` value inside `[lo, hi]`,
    /// and a history of exactly `min(pos, window − 1)` rows. An absent instance is the initial
    /// one (zeros, no rows).
    pub fn import_state(&mut self, st: &RunState) -> TirResult<()> {
        let p: &TirProgramV1 = &self.plan.program;
        if st.pos >= p.history_bound {
            return Err(TirError::new(TirErrorKind::Position, "position ≥ history_bound"));
        }
        let mut run = RunBufs::initial(self.plan);
        run.pos = st.pos;
        let bad = |what: &str| TirError::new(TirErrorKind::Operand, what.to_string());
        for (k, inst) in self.plan.instances.iter().enumerate() {
            let key = (inst.state, inst.layer);
            match inst.kind {
                StateKind::Fixed { lo, hi } => {
                    if let Some(t) = st.fixed.get(&key) {
                        if t.dtype != inst.dtype || t.shape != inst.shape || t.data.len() != numel(&inst.shape) {
                            return Err(bad("a Fixed state of the wrong type"));
                        }
                        if t.data.iter().any(|v| *v < lo as i128 || *v > hi as i128) {
                            return Err(bad("a Fixed state value outside [lo, hi]"));
                        }
                        run.fixed[k] = Buf::from_i128s(inst.dtype, &t.data);
                    }
                }
                StateKind::Hist { window } => {
                    let rows = st.hist.get(&key).map(|r| r.len()).unwrap_or(0);
                    if rows != (st.pos as usize).min(window as usize - 1) {
                        return Err(TirError::new(TirErrorKind::Position, "a history of the wrong length"));
                    }
                    let h = &mut run.hist[k];
                    let mut all = Vec::with_capacity(rows * h.row);
                    for r in st.hist.get(&key).into_iter().flatten() {
                        if r.dtype != inst.dtype || r.shape != inst.shape || r.data.iter().any(|v| !inst.dtype.contains(*v)) {
                            return Err(bad("a history row of the wrong type"));
                        }
                        all.extend_from_slice(&r.data);
                    }
                    h.data = Buf::from_i128s(inst.dtype, &all);
                    h.rows = rows;
                }
            }
        }
        self.run = run;
        Ok(())
    }
}
