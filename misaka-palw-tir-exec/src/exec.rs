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
use crate::fused::Region;
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

/// Evaluate one computing node (everything but `StateWrite` and `HistAppend`) — node `at.1` of
/// block `at.0`. Returns the value as a view (`Some`) or `None` when the result was written to
/// `out`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn eval_compute(
    plan: &TirPlan,
    bp: &BlockPlan,
    node: &NodePlan,
    at: (u8, u16),
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
            // A residency serves this param by rows (`crate::rows`): read the rows the index names,
            // never the param — the same elements, checked the same way.
            let site = plan.rows.gathers.get(at.0 as usize).and_then(|g| g.get(at.1 as usize)).and_then(|s| s.as_ref());
            if let Some(j) = crate::rows::served_view(rd, d.src, &d.layout, site) {
                let site = site.expect("served_view saw the site");
                crate::rows::gather_rows(plan, node, rd, at, site, j, &idx, out, scratch)?;
                return Ok(None);
            }
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
    /// Per `Hist` instance, when kept: the last `tail_rows` appended rows, oldest first, as lanes —
    /// kept whatever the window (a step leg's history tile covers `h_tile` rows, which may be more
    /// than a sliding window still shows).
    pub tails: Vec<std::collections::VecDeque<Vec<i32>>>,
    pub tail_rows: usize,
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
        RunBufs {
            pos: 0,
            fixed_next: fixed.clone(),
            fixed,
            written: vec![false; n],
            hist,
            tails: vec![Default::default(); n],
            tail_rows: 0,
        }
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
    /// Per occurrence, when fused kernels are on ([`Self::set_fused`]): the regions that run fused.
    fused: Option<Vec<Option<crate::fused::OccFused>>>,
    /// The kernel whose deliberately broken variant runs ([`Self::set_fused_fault`]); test only.
    fused_fault: Option<usize>,
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
            fused: None,
            fused_fault: None,
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

    // ------------------------------------------------------------------------------------------
    // RFC-0006: a CELL's executor — the occurrences of one layer shard, from committed carry-in rows
    // ------------------------------------------------------------------------------------------

    /// **An executor for the occurrences `occ` only** (RFC-0006 §1): every param instance THOSE occurrences read must be bound,
    /// and nothing else — a shard seat holds its own layers' weights and no other's. The refined plan is the one
    /// [`Self::new`] computes (the ranges of the instances that are bound; the dtype's range for the others, which no step of
    /// this executor reads).
    pub fn new_cell(plan: &'a TirPlan, params: &'a TirParams<'a>, occ: std::ops::Range<usize>) -> TirResult<Self> {
        for (o, &(block, layer)) in plan.occurrences.iter().enumerate() {
            if !occ.contains(&o) {
                continue;
            }
            for n in &plan.program.blocks[block as usize].nodes {
                for r in &n.inputs {
                    if let Ref::Param(j) = r {
                        let l = if plan.program.params[*j as usize].per_layer { layer } else { None };
                        if !params.has(*j, l) {
                            return Err(TirError::new(
                                TirErrorKind::Missing,
                                format!("param {} (layer {l:?}): the cell's occurrence {o} reads it", plan.program.params[*j as usize].name),
                            ));
                        }
                    }
                }
            }
        }
        let nb = plan.blocks.len();
        let occ_plans = plan.refine(&|j, l| params.range(j, l));
        Ok(TirExecutor {
            plan,
            params,
            occ_plans,
            fused: None,
            fused_fault: None,
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

    /// **One position of a cell**: occurrences `occ` of position [`Self::pos`], from `carry_in` (the carry-out lanes of occurrence
    /// `occ.start − 1`, one vector per carry, as committed; empty when `occ` starts at `pre`), `token` the position's input (a public
    /// id every shard knows: any block may read `Input(0)`). The committed values reach `sink` in slot order; on success the state advances one position and the LAST
    /// occurrence's carry-out is returned (empty when `occ` ends at `post`: its logits are [`Self::logits`]). On failure the run
    /// state is exactly as before.
    pub fn step_cell(
        &mut self,
        token: u32,
        occ: std::ops::Range<usize>,
        carry_in: &[Vec<i128>],
        sink: &mut dyn StepSink,
    ) -> TirResult<Vec<Vec<i128>>> {
        let n_occ = self.plan.occurrences.len();
        if occ.start >= occ.end || occ.end > n_occ {
            return Err(TirError::new(TirErrorKind::Operand, format!("a cell of occurrences {occ:?} of {n_occ}")));
        }
        let runs_post = occ.end == n_occ;
        let to = self.begin_step(token, runs_post)?;
        if occ.start > 0 {
            // The first occurrence's carry-ins, typed by its block (spec 04b §3.2: a carry-in is a fixed-shape tensor).
            let (block, _) = self.plan.occurrences[occ.start];
            let want = &self.plan.program.blocks[block as usize].carry_in;
            if carry_in.len() != want.len() {
                self.work.carry.clear();
                return Err(TirError::new(
                    TirErrorKind::Operand,
                    format!("{} carry-ins for an occurrence that takes {}", carry_in.len(), want.len()),
                ));
            }
            let bufs = want
                .iter()
                .zip(carry_in)
                .map(|(t, lanes)| {
                    let expected: usize = t.shape.iter().map(|d| d.at(1)).product::<usize>().max(1);
                    if lanes.len() != expected {
                        return Err(TirError::new(
                            TirErrorKind::Operand,
                            format!("a carry-in of {} lanes for a type of {expected}", lanes.len()),
                        ));
                    }
                    Ok(Buf::from_i128s(t.dtype, lanes))
                })
                .collect::<TirResult<Vec<_>>>()?;
            self.work.carry = bufs;
        }
        let r = self.run_occurrences(occ.start, to.min(occ.end), sink);
        let out = if r.is_ok() && !runs_post { self.work.carry.iter().map(|b| b.to_i128s()).collect() } else { Vec::new() };
        self.end_step(r)?;
        Ok(out)
    }

    /// **Put the executor at the state after position `pos − 1`, for the instances `keep` names** (a cell resuming from a segment
    /// boundary: its layers' checkpoint and history rows, committed; every other instance stays initial and is never read).
    /// Checked as [`Self::import_state`] checks, over the kept instances only.
    pub fn import_cell_state(&mut self, st: &RunState, keep: &dyn Fn(u16, Option<u16>) -> bool) -> TirResult<()> {
        let p: &TirProgramV1 = &self.plan.program;
        if st.pos >= p.history_bound {
            return Err(TirError::new(TirErrorKind::Position, "position ≥ history_bound"));
        }
        let mut run = RunBufs::initial(self.plan);
        run.pos = st.pos;
        run.tail_rows = self.run.tail_rows;
        let bad = |what: &str| TirError::new(TirErrorKind::Operand, what.to_string());
        for (k, inst) in self.plan.instances.iter().enumerate() {
            if !keep(inst.state, inst.layer) {
                continue;
            }
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

    /// **Run the fused kernels (RFC-0002 §7)** — every region of the program a kernel of this build
    /// matched ([`crate::fused::match_program`]) whose nodes cannot fail under this executor's
    /// refined plan ([`crate::fused::enable`]) — or none. Off by default. Byte-identical either way
    /// (the gate is `tests/fused_gate.rs`); a step whose sink asks for every node's value runs
    /// generic throughout.
    pub fn set_fused(&mut self, on: bool) {
        let params = self.params;
        self.fused = on.then(|| {
            let regions = self.plan.fused_regions();
            self.plan
                .occurrences
                .iter()
                .zip(&self.occ_plans)
                .map(|(&(block, layer), bp)| {
                    // A kernel reads its holes whole: a region that would read a row-served param
                    // runs on the generic kernels, whose gathers read rows.
                    let held = |r: &Region| {
                        r.holes.iter().all(|h| !matches!(h, Ref::Param(j) if params.serves_rows(*j, layer)))
                            && r.nodes.iter().all(|n| {
                                self.plan.program.blocks[block as usize].nodes[*n as usize]
                                    .inputs
                                    .iter()
                                    .all(|i| !matches!(i, Ref::Param(j) if params.serves_rows(*j, layer)))
                            })
                    };
                    let mine: Vec<Region> = regions[block as usize].iter().filter(|r| held(r)).cloned().collect();
                    crate::fused::enable(&mine, bp, &self.plan.program)
                })
                .collect()
        });
    }

    /// **Run kernel `kernel`'s deliberately broken variant** (an index into
    /// [`crate::fused::kernels_v1`]): it moves one output lane by one. The gate
    /// (`tests/fused_gate.rs`) proves it catches it; nothing else calls this.
    #[doc(hidden)]
    pub fn set_fused_fault(&mut self, kernel: Option<usize>) {
        self.fused_fault = kernel;
    }

    /// `(kernel name, regions)` that run fused per position, when fused kernels are on.
    pub fn fused_summary(&self) -> Vec<(&'static str, usize)> {
        let kernels = crate::fused::kernels_v1();
        let mut counts = vec![0usize; kernels.len()];
        for occ in self.fused.iter().flatten().flatten() {
            for r in &occ.regions {
                counts[r.kernel] += 1;
            }
        }
        kernels.iter().zip(counts).map(|(k, c)| (k.name(), c)).collect()
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

    /// Keep the last `rows` appended rows of every history, whatever its window
    /// ([`Self::hist_tail`]); 0 keeps none.
    pub fn set_hist_tail(&mut self, rows: usize) {
        self.run.tail_rows = rows;
        for t in self.run.tails.iter_mut() {
            t.clear();
        }
    }

    /// Replace the kept tail of the history instance of state `j` at `layer` (a resumed run's rows
    /// from before its first position), oldest first; at most the kept length is retained.
    pub fn set_hist_tail_rows(&mut self, j: u16, layer: Option<u16>, rows: &[Vec<i32>]) -> TirResult<()> {
        let k = self
            .plan
            .instance(j, layer)
            .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("no history instance {j} at {layer:?}")))?
            as usize;
        let row_len = self.run.hist[k].row;
        if rows.iter().any(|r| r.len() != row_len) {
            return Err(TirError::new(TirErrorKind::Operand, "a history row of the wrong length"));
        }
        let keep = self.run.tail_rows;
        let tail = &mut self.run.tails[k];
        tail.clear();
        tail.extend(rows.iter().skip(rows.len().saturating_sub(keep)).cloned());
        Ok(())
    }

    /// The kept tail of the history instance of state `j` at `layer`: the last appended rows,
    /// oldest first, as lanes.
    pub fn hist_tail(&self, j: u16, layer: Option<u16>) -> Option<&std::collections::VecDeque<Vec<i32>>> {
        self.run.tails.get(self.plan.instance(j, layer)? as usize)
    }

    /// The current value of the `Fixed` instance of state `j` at `layer` (after the last
    /// successful step).
    pub fn fixed_value(&self, j: u16, layer: Option<u16>) -> Option<Slice<'_>> {
        let k = self.plan.instance(j, layer)? as usize;
        matches!(self.plan.instances[k].kind, StateKind::Fixed { .. }).then(|| self.run.fixed[k].slice())
    }

    /// The last successful step's logits.
    pub fn logits(&self) -> (&[usize], Slice<'_>) {
        (&self.work.logits_shape, self.work.logits.slice())
    }

    /// One position: `token` at [`Self::pos`]. On success the state advances; on failure it is
    /// exactly as before (spec 04b §9.1(4)).
    pub fn step(&mut self, token: u32, sink: &mut dyn StepSink) -> TirResult<()> {
        self.step_opt(token, sink, true)
    }

    /// [`Self::step`], evaluating `post` only when `run_post` — a job's positions whose logits are
    /// not consumed skip it (the step space of Phase F, design §2.5). Exact: `post` writes no
    /// state (NF-19), so the run state after the position is the same either way; the logits are
    /// then not produced ([`Self::logits`] is empty).
    pub fn step_opt(&mut self, token: u32, sink: &mut dyn StepSink, run_post: bool) -> TirResult<()> {
        let to = self.begin_step(token, run_post)?;
        let r = self.run_occurrences(0, to, sink);
        self.end_step(r)
    }

    /// **One position whose carry at a shard boundary is a LIE the producer commits and computes on** (RFC-0006, the drill's
    /// consistent lie): occurrences `[0, boundary)` run honestly; the first carry-out of occurrence `boundary − 1` is changed by
    /// `lie` — in the carry the next occurrences read AND in the committed value the sink receives — and occurrences
    /// `[boundary, to)` run honestly FROM it. The result is the execution of a producer who lied once at the boundary and
    /// computed everything after correctly: the upstream cell finds it, the downstream cell verifies it (RFC §2.1). A drill
    /// fault injector, never a rule.
    pub fn step_with_boundary_lie(
        &mut self,
        token: u32,
        sink: &mut dyn StepSink,
        run_post: bool,
        boundary: usize,
        lie: &dyn Fn(&mut [i128]),
    ) -> TirResult<()> {
        let to = self.begin_step(token, run_post)?;
        if boundary == 0 || boundary >= to {
            return Err(TirError::new(TirErrorKind::Operand, format!("a boundary at occurrence {boundary} of a step of {to}")));
        }
        let (block, _) = self.plan.occurrences[boundary - 1];
        let Some(&node) = self.plan.program.blocks[block as usize].carry_out.first() else {
            return Err(TirError::new(TirErrorKind::Operand, "the occurrence before the boundary has no carry-out"));
        };
        let slot = self.plan.slot_bases[boundary - 1] + u32::from(node);
        struct LyingSink<'s> {
            inner: &'s mut dyn StepSink,
            slot: u32,
            lie: &'s dyn Fn(&mut [i128]),
        }
        impl StepSink for LyingSink<'_> {
            fn every_node(&self) -> bool {
                self.inner.every_node()
            }
            fn node(&mut self, v: &NodeValue<'_>) {
                if v.slot == self.slot {
                    let mut lanes = v.data.to_i128s();
                    (self.lie)(&mut lanes);
                    let buf = Buf::from_i128s(v.dtype, &lanes);
                    self.inner.node(&NodeValue { data: buf.slice(), ..*v });
                } else {
                    self.inner.node(v);
                }
            }
        }
        let r = (|| {
            self.run_occurrences(0, boundary, &mut LyingSink { inner: &mut *sink, slot, lie })?;
            let first = self.work.carry.first().ok_or_else(|| TirError::new(TirErrorKind::Missing, "no carry at the boundary"))?;
            let mut lanes = first.to_i128s();
            lie(&mut lanes);
            let lied = Buf::from_i128s(first.dtype(), &lanes);
            self.work.carry[0] = lied;
            self.run_occurrences(boundary, to, sink)
        })();
        self.end_step(r)
    }

    /// **A position's start**: its inputs checked (the position bound, the token bound — before
    /// anything runs) and set, the carry cleared, and the logits cleared when `post` will not run.
    /// Returns how many occurrences the position runs. [`Self::step_opt`] is this, every occurrence
    /// ([`Self::run_occurrences`]), then [`Self::end_step`]; a lockstep batch
    /// (`lockstep::TirLockstepV1`) interleaves members between the three.
    pub(crate) fn begin_step(&mut self, token: u32, run_post: bool) -> TirResult<usize> {
        let p = &self.plan.program;
        let pos = self.run.pos;
        if pos >= p.history_bound {
            return Err(TirError::new(TirErrorKind::Position, format!("position {pos} ≥ history_bound {}", p.history_bound)));
        }
        if self.plan.reads_token && token >= p.token_bound {
            return Err(TirError::new(TirErrorKind::Operand, format!("token {token} ≥ token_bound {}", p.token_bound)));
        }
        self.work.inputs = [token, pos];
        self.work.carry.clear();
        if !run_post {
            self.work.logits_shape.clear();
            self.work.logits = Buf::default();
        }
        let n = self.plan.occurrences.len();
        Ok(if run_post { n } else { n - 1 })
    }

    /// **A position's end**: on success its effects apply and the position advances; on failure the
    /// run state is exactly as before (spec 04b §9.1(4)).
    pub(crate) fn end_step(&mut self, r: TirResult<()>) -> TirResult<()> {
        let run = &mut self.run;
        match r {
            Ok(()) => {
                for (k, w) in run.written.iter_mut().enumerate() {
                    if std::mem::replace(w, false) {
                        std::mem::swap(&mut run.fixed[k], &mut run.fixed_next[k]);
                    }
                }
                if run.tail_rows > 0 {
                    for (h, tail) in run.hist.iter().zip(run.tails.iter_mut()) {
                        for k in 0..h.pending {
                            let at = (h.start + h.rows + k) * h.row;
                            let mut row = if tail.len() >= run.tail_rows { tail.pop_front().unwrap_or_default() } else { Vec::new() };
                            row.clear();
                            with_slice!(h.data.slice(), v => row.extend(v[at..at + h.row].iter().map(|x| x.to_i64() as i32)));
                            tail.push_back(row);
                        }
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

    /// Occurrences `from..to` of the position [`Self::begin_step`] began, in schedule order.
    pub(crate) fn run_occurrences(&mut self, from: usize, to: usize, sink: &mut dyn StepSink) -> TirResult<()> {
        let plan = self.plan;
        let params = self.params;
        let every = sink.every_node();
        let n_occ = plan.occurrences.len();
        let pos = self.run.pos;
        let TirExecutor { run, work, occ_plans, fused, fused_fault, .. } = self;
        let fused_fault = *fused_fault;
        for (occ, &(block, layer)) in plan.occurrences.iter().enumerate().take(to).skip(from) {
            let bp = &occ_plans[occ];
            let h = bp.window.map(|w| (pos as usize + 1).min(w as usize)).unwrap_or(1);
            let n = bp.nodes.len();
            let mut slots = std::mem::take(&mut work.slots[block as usize]);
            let mut vals = std::mem::take(&mut work.vals[block as usize]);
            slots.resize_with(n, Buf::default);
            vals.resize(n, Val::default());
            let base = plan.slot_bases[occ];
            // The fused regions of this occurrence — none when the sink wants every node's value.
            let fo = if every { None } else { fused.as_ref().and_then(|f| f[occ].as_ref()) };
            for ni in 0..n {
                let node = &bp.nodes[ni];
                let role = fo.map_or(crate::fused::Role::Generic, |f| f.roles[ni]);
                if role == crate::fused::Role::Skip {
                    // Computed by its region's kernel at the region's output; read by nothing else.
                    continue;
                }
                let started = work.profile.is_some().then(std::time::Instant::now);
                let (shape, rank) = resolve(&node.out.shape, h);
                let out_shape = &shape[..rank];
                let val = match (role, &node.prim) {
                    (crate::fused::Role::Run(ri), _) => {
                        let region = &fo.expect("a fused role implies its occurrence's regions").regions[ri as usize];
                        let kernel = crate::fused::kernels_v1()[region.kernel];
                        let missing = || TirError::new(TirErrorKind::Missing, "fused state instance");
                        let insts: Vec<u32> =
                            region.states.iter().map(|s| plan.instance(*s, layer).ok_or_else(missing)).collect::<TirResult<_>>()?;
                        let ranges: Vec<(i64, i64)> = region
                            .states
                            .iter()
                            .map(|s| match plan.program.states[*s as usize].kind {
                                StateKind::Fixed { lo, hi } => (lo, hi),
                                StateKind::Hist { .. } => (0, 0),
                            })
                            .collect();
                        let write_inst = region.writes.map(|s| plan.instance(s, layer).ok_or_else(missing)).transpose()?;
                        let mut next: Vec<Buf> =
                            write_inst.map(|k| std::mem::take(&mut run.fixed_next[k as usize])).into_iter().collect();
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
                            region
                                .holes
                                .iter()
                                .map(|h| rd.opd(ref_val(plan, bp, *h, &vals, layer)?))
                                .collect::<TirResult<Vec<_>>>()
                                .and_then(|holes| {
                                    let states = insts.iter().map(|k| rd.data(Src::Fixed(*k))).collect::<TirResult<Vec<_>>>()?;
                                    kernel.run(
                                        &region.bound,
                                        &mut crate::fused::FusedIo {
                                            holes: &holes,
                                            states: &states,
                                            state_ranges: &ranges,
                                            state_next: &mut next,
                                            out: &mut out,
                                            out_store: node.store,
                                            out_shape,
                                            fault: fused_fault == Some(region.kernel),
                                        },
                                    )
                                })
                        };
                        if let Some(k) = write_inst {
                            run.fixed_next[k as usize] = next.pop().unwrap_or_default();
                            if r.is_ok() {
                                run.written[k as usize] = true;
                            }
                        }
                        slots[ni] = out;
                        r?;
                        Val { src: Src::Slot(ni as u16), layout: Layout::contiguous(out_shape) }
                    }
                    (_, &Prim::StateWrite { state }) => {
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
                    (_, &Prim::HistAppend { state }) => {
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
                            eval_compute(plan, bp, node, (block, ni as u16), &rd, &vals, out_shape, &mut out, &mut work.scratch)
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
        run.tail_rows = self.run.tail_rows;
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
