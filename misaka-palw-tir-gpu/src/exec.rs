//! **The device executor: a whole program, one position at a time, every value on the device.**
//!
//! [`GpuExecutor`] is `misaka_palw_tir_exec::TirExecutor` with its storage on a GPU: the same
//! occurrences in schedule order, the same nodes in index order, the same refined plans
//! (`TirPlan::refine` with the actual ranges of the params it holds), the same views, and the same
//! rule for effects — `Fixed` states double-buffered and swapped, `Hist` rows appended, `pos`
//! advanced — only after the whole step succeeded (spec 04b §9.1). Params and consts are uploaded
//! once; a step submits its dispatches in one recording and reads back once: the status words and
//! the committed lanes together.
//!
//! **Per-node fallback.** A node the device does not run ([`Unsupported`] — an `i128` working type
//! or stored value, a `Fast128`/`Pn128` sum) runs on the CPU executor's kernel under the same plan:
//! the recording is submitted, the status words read (a failure of an EARLIER node is the step's
//! error, exactly as the CPU would have stopped there), the operands downloaded, the kernel run, and
//! its result uploaded (or kept on the host when it is an `i128` the device cannot hold). The
//! fallback is the CPU executor itself, so it cannot disagree with it; it only costs a round trip.
//!
//! **Failures** are reported as the CPU executor reports them: the first failing node in slot order,
//! and within it the first failing element (the status words of [`crate::kernels`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::{DType, Prim, Ref, TirError, TirErrorKind, TirResult};
use misaka_palw_tir_exec::elem::Buf;
use misaka_palw_tir_exec::kernels::{Opd, Scratch, elementwise, matmul, misc, reduce};
use misaka_palw_tir_exec::layout::{Layout, numel};
use misaka_palw_tir_exec::plan::{BlockPlan, NodePlan, TirPlan};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirParams};

use crate::device::GpuDevice;
use crate::kernels::{DeviceFailure, Recorder, Unsupported};
use crate::tensor::{DevTensor, Form, unpack};
use crate::wgsl::EwOp;

/// A value during a step: on the device, or on the host (an `i128` the device cannot hold).
#[derive(Clone, Debug)]
enum Val {
    Dev(DevTensor),
    Host(Arc<Buf>, Layout),
}

impl Val {
    fn layout(&self) -> Layout {
        match self {
            Val::Dev(t) => t.layout,
            Val::Host(_, l) => *l,
        }
    }
    fn with_layout(&self, l: Layout) -> Val {
        match self {
            Val::Dev(t) => Val::Dev(t.view(l)),
            Val::Host(b, _) => Val::Host(Arc::clone(b), l),
        }
    }
}

/// A `Hist` instance on the device: rows `start .. start + rows` of `buf` are the visible prior
/// rows; this step's row sits just past them until the step succeeds.
struct HistDev {
    buf: Arc<wgpu::Buffer>,
    form: Form,
    dtype: DType,
    row: usize,
    row_shape: Vec<usize>,
    cap: usize,
    start: usize,
    rows: usize,
    window: u32,
    pending: bool,
}

/// What ran where, over the executor's life.
#[derive(Clone, Debug, Default)]
pub struct ExecStats {
    pub steps: u64,
    /// Node evaluations computed by a device kernel.
    pub device_nodes: u64,
    /// Node evaluations that were views (no kernel).
    pub views: u64,
    /// Node evaluations run by the CPU kernel, by reason.
    pub fallback: BTreeMap<String, u64>,
    pub dispatches: u64,
    /// Mid-step synchronisations (one per fallback node).
    pub syncs: u64,
    /// Host time recording dispatches (params, bind groups, outputs), in nanoseconds.
    pub record_ns: u64,
    /// Time from the step's submission to its status and lanes being read back.
    pub wait_ns: u64,
}

/// Runs one program over positions on a device, holding its run state there.
pub struct GpuExecutor<'a> {
    dev: &'a GpuDevice,
    plan: &'a TirPlan,
    occ_plans: Vec<BlockPlan>,
    dparams: BTreeMap<(u16, Option<u16>), Val>,
    consts: Vec<Val>,
    pos: u32,
    fixed: Vec<Option<DevTensor>>,
    fixed_next: Vec<Option<DevTensor>>,
    written: Vec<bool>,
    hist: Vec<Option<HistDev>>,
    logits: Buf,
    logits_shape: Vec<usize>,
    /// This step's token and position, as the host knows them (a gather by the token is a view).
    inputs_host: [u32; 2],
    pub stats: ExecStats,
}

/// A value the step will hand to the sink once it has succeeded.
struct Pending {
    slot: u32,
    block: u8,
    layer: Option<u16>,
    node: u16,
    commit: bool,
    dtype: DType,
    shape: Vec<usize>,
    val: Val,
    /// Offset in the step's staged lanes (committed device values in the fast path).
    lanes_at: Option<usize>,
}

fn missing(what: String) -> TirError {
    TirError::new(TirErrorKind::Missing, what)
}

impl<'a> GpuExecutor<'a> {
    /// An executor at position 0 with the initial state; every param instance the plan reads is
    /// uploaded (in its packed form) now.
    pub fn new(dev: &'a GpuDevice, plan: &'a TirPlan, params: &'a TirParams<'a>) -> TirResult<Self> {
        params.check_complete(plan)?;
        let occ_plans = plan.refine(&|j, l| params.range(j, l));
        let max = dev.limits.max_storage_buffer_binding_size as usize;
        let mut dparams = BTreeMap::new();
        for &(j, layer) in &plan.param_instances {
            let s = params.get(j, layer).ok_or_else(|| missing(format!("param {j} at {layer:?}")))?;
            let decl = &plan.program.params[j as usize];
            let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
            let form = Form::param(decl.dtype).expect("params are never i128");
            let v = if form.bytes(s.len()) as usize <= max && s.len() < u32::MAX as usize {
                Val::Dev(dev.upload(s, form, &shape))
            } else {
                Val::Host(Arc::new(s.to_buf()), Layout::contiguous(&shape))
            };
            dparams.insert((j, layer), v);
        }
        let consts = plan
            .consts
            .iter()
            .zip(&plan.program.consts)
            .map(|(b, c)| {
                let shape: Vec<usize> = c.shape.iter().map(|d| *d as usize).collect();
                match Form::computed(b.dtype()) {
                    Some(f) => Val::Dev(dev.upload(b.slice(), f, &shape)),
                    None => Val::Host(Arc::new(b.clone()), Layout::contiguous(&shape)),
                }
            })
            .collect();
        let n = plan.instances.len();
        let mut fixed = Vec::with_capacity(n);
        let mut hist = Vec::with_capacity(n);
        for inst in &plan.instances {
            let form = Form::computed(inst.dtype).expect("states are i8, i16 or i32");
            match inst.kind {
                StateKind::Fixed { .. } => {
                    // wgpu zero-initialises every buffer: the initial state.
                    let t = DevTensor {
                        buf: dev.alloc(form, numel(&inst.shape)),
                        form,
                        dtype: inst.dtype,
                        layout: Layout::contiguous(&inst.shape),
                    };
                    fixed.push(Some(t));
                    hist.push(None);
                }
                StateKind::Hist { window } => {
                    fixed.push(None);
                    let row = numel(&inst.shape);
                    hist.push(Some(HistDev {
                        buf: dev.alloc(form, 8 * row),
                        form,
                        dtype: inst.dtype,
                        row,
                        row_shape: inst.shape.clone(),
                        cap: 8,
                        start: 0,
                        rows: 0,
                        window,
                        pending: false,
                    }));
                }
            }
        }
        Ok(GpuExecutor {
            dev,
            plan,
            occ_plans,
            dparams,
            consts,
            pos: 0,
            fixed_next: vec![None; n],
            fixed,
            written: vec![false; n],
            hist,
            logits: Buf::default(),
            logits_shape: Vec::new(),
            inputs_host: [0, 0],
            stats: ExecStats::default(),
        })
    }

    pub fn pos(&self) -> u32 {
        self.pos
    }

    /// The last successful step's logits.
    pub fn logits(&self) -> (&[usize], &Buf) {
        (&self.logits_shape, &self.logits)
    }

    /// One position: `token` at [`Self::pos`]. On success the state advances; on failure it is
    /// exactly as before. The sink receives the committed values (every node's, when it asks) in
    /// slot order once the step has succeeded.
    pub fn step(&mut self, token: u32, sink: &mut dyn StepSink) -> TirResult<()> {
        let p = &self.plan.program;
        let pos = self.pos;
        if pos >= p.history_bound {
            return Err(TirError::new(TirErrorKind::Position, format!("position {pos} ≥ history_bound {}", p.history_bound)));
        }
        if self.plan.reads_token && token >= p.token_bound {
            return Err(TirError::new(TirErrorKind::Operand, format!("token {token} ≥ token_bound {}", p.token_bound)));
        }
        // The plans are read while the run state is written: hold them apart for the step.
        let occ_plans = std::mem::take(&mut self.occ_plans);
        let r = self.step_inner(token, pos, sink, &occ_plans);
        self.occ_plans = occ_plans;
        match r {
            Ok(()) => {
                for k in 0..self.written.len() {
                    if std::mem::replace(&mut self.written[k], false) {
                        std::mem::swap(&mut self.fixed[k], &mut self.fixed_next[k]);
                    }
                }
                for h in self.hist.iter_mut().flatten() {
                    if std::mem::replace(&mut h.pending, false) {
                        let total = h.rows + 1;
                        let keep = total.min(h.window as usize - 1);
                        h.start += total - keep;
                        h.rows = keep;
                    }
                }
                self.pos += 1;
                self.stats.steps += 1;
                Ok(())
            }
            Err(e) => {
                self.written.iter_mut().for_each(|w| *w = false);
                self.hist.iter_mut().flatten().for_each(|h| h.pending = false);
                Err(e)
            }
        }
    }

    fn step_inner(&mut self, token: u32, pos: u32, sink: &mut dyn StepSink, occ_plans: &[BlockPlan]) -> TirResult<()> {
        let plan = self.plan;
        let dev = self.dev;
        let every = sink.every_node();
        let t_record = std::time::Instant::now();
        let slots = plan.slots_per_position();
        let mut rec = Recorder::new(dev, slots);
        self.inputs_host = [token, pos];
        let inputs = dev.upload(misaka_palw_tir_exec::elem::Slice::Idx(&[token, pos]), Form::U32, &[2]);
        // The committed lanes of this step, staged on the device for one readback.
        let mut lanes_total = 0usize;
        for (occ, &(block, _)) in plan.occurrences.iter().enumerate() {
            let bp = &occ_plans[occ];
            let h = bp.window.map(|w| (pos as usize + 1).min(w as usize)).unwrap_or(1);
            for n in &plan.blocks[block as usize].nodes {
                if n.commit {
                    lanes_total += n.out.shape.iter().map(|d| d.at(h)).product::<usize>();
                }
            }
        }
        let staging = dev.alloc(Form::S32, lanes_total.max(1));
        let mut lanes_at = 0usize;
        let mut logits_lanes: Option<usize> = None;
        let mut pending: Vec<Pending> = Vec::new();
        let mut carry: Vec<Val> = Vec::new();
        let mut host_failure: Option<(u32, TirError)> = None;
        'occurrences: for (occ, &(block, layer)) in plan.occurrences.iter().enumerate() {
            let bp = &occ_plans[occ];
            let h = bp.window.map(|w| (pos as usize + 1).min(w as usize)).unwrap_or(1);
            let base = plan.slot_bases[occ];
            let mut vals: Vec<Val> = Vec::with_capacity(bp.nodes.len());
            for (ni, node) in bp.nodes.iter().enumerate() {
                let slot = base + ni as u32;
                let out_shape: Vec<usize> = node.out.shape.iter().map(|d| d.at(h)).collect();
                let ins: Vec<Val> =
                    node.inputs.iter().map(|r| self.ref_val(*r, &vals, &carry, layer, &inputs)).collect::<TirResult<_>>()?;
                let v = match self.eval_node(&mut rec, node, &ins, &out_shape, slot, layer) {
                    Ok(v) => v,
                    Err(e) => {
                        host_failure = Some((slot, e));
                        break 'occurrences;
                    }
                };
                if node.commit || every {
                    let mut at = None;
                    if node.commit
                        && !every
                        && let Val::Dev(t) = &v
                    {
                        let region = Layout::contiguous_at(&out_shape, lanes_at);
                        rec.ew(EwOp::Copy, &out_shape, (&staging, Form::S32, &region), &[(t, t.layout)], &[], &[], None, false, slot)
                            .map_err(|u| TirError::new(TirErrorKind::Shape, format!("staging a commit: {u:?}")))?;
                        at = Some(lanes_at);
                        if occ + 1 == plan.occurrences.len() && ni == plan.program.logits as usize {
                            logits_lanes = Some(lanes_at);
                        }
                    }
                    if node.commit {
                        lanes_at += numel(&out_shape);
                    }
                    pending.push(Pending {
                        slot,
                        block,
                        layer,
                        node: ni as u16,
                        commit: node.commit,
                        dtype: node.out.dtype,
                        shape: out_shape.clone(),
                        val: v.clone(),
                        lanes_at: at,
                    });
                }
                vals.push(v);
            }
            if occ + 1 == plan.occurrences.len() {
                let lv = vals[plan.program.logits as usize].clone();
                pending.push(Pending {
                    slot: u32::MAX,
                    block,
                    layer,
                    node: plan.program.logits,
                    commit: false,
                    dtype: bp.nodes[plan.program.logits as usize].out.dtype,
                    shape: lv.layout().shape().to_vec(),
                    val: lv,
                    lanes_at: logits_lanes,
                });
            } else {
                carry = bp.carry_out.iter().map(|c| vals[*c as usize].clone()).collect();
            }
        }
        self.stats.dispatches += rec.dispatches as u64;
        self.stats.record_ns += t_record.elapsed().as_nanos() as u64;
        let t_wait = std::time::Instant::now();
        let status = rec.finish();
        // The first failure in slot order: a device node's, or the host's (a CPU fallback or a
        // scalar index found out of range before the device saw it).
        let device_first = status.iter().enumerate().find_map(|(s, w)| DeviceFailure::decode(*w).map(|f| (s as u32, f)));
        match (device_first, host_failure) {
            (Some((s, f)), Some((hs, he))) => return Err(if s < hs { f.to_error("node") } else { he }),
            (Some((_, f)), None) => return Err(f.to_error("node")),
            (None, Some((_, he))) => return Err(he),
            (None, None) => {}
        }
        // One readback of every staged lane; the rest (host values, every-node mode) one by one.
        let lanes =
            if lanes_at > 0 { unpack(&dev.read_bytes(&staging, 0, 4 * lanes_at as u64), Form::S32, lanes_at) } else { Vec::new() };
        self.stats.wait_ns += t_wait.elapsed().as_nanos() as u64;
        for pv in pending {
            let buf = match (&pv.val, pv.lanes_at) {
                (_, Some(at)) => {
                    let n = numel(&pv.shape);
                    let vals: Vec<i128> =
                        lanes[at..at + n].iter().map(|v| if pv.dtype == DType::Idx { *v as u32 as i128 } else { *v }).collect();
                    Buf::from_i128s(pv.dtype, &vals)
                }
                (Val::Dev(t), None) => dev.download(t),
                (Val::Host(b, l), None) => host_gather(b, l),
            };
            if pv.slot == u32::MAX {
                self.logits_shape = pv.shape.clone();
                self.logits = buf;
                continue;
            }
            sink.node(&NodeValue {
                slot: pv.slot,
                block: pv.block,
                layer: pv.layer,
                node: pv.node,
                commit: pv.commit,
                dtype: pv.dtype,
                shape: &pv.shape,
                data: buf.slice(),
            });
        }
        Ok(())
    }

    /// The value a `Ref` names in the running occurrence.
    fn ref_val(&self, r: Ref, vals: &[Val], carry: &[Val], layer: Option<u16>, inputs: &DevTensor) -> TirResult<Val> {
        let p = &self.plan.program;
        Ok(match r {
            Ref::Node(j) => vals[j as usize].clone(),
            Ref::CarryIn(k) => carry.get(k as usize).cloned().ok_or_else(|| missing(format!("carry-in {k}")))?,
            Ref::Param(j) => {
                let l = if p.params[j as usize].per_layer { layer } else { None };
                self.dparams.get(&(j, l)).cloned().ok_or_else(|| missing(format!("param {j} at {l:?}")))?
            }
            Ref::Const(j) => self.consts[j as usize].clone(),
            Ref::State(j) => {
                let inst = self.plan.instance(j, layer).ok_or_else(|| missing(format!("state {j} at {layer:?}")))?;
                Val::Dev(self.fixed[inst as usize].clone().ok_or_else(|| missing(format!("state {j}")))?)
            }
            Ref::Input(k) => Val::Dev(inputs.view(Layout::contiguous_at(&[], k as usize))),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_node(
        &mut self,
        rec: &mut Recorder<'_>,
        node: &NodePlan,
        ins: &[Val],
        out_shape: &[usize],
        slot: u32,
        layer: Option<u16>,
    ) -> TirResult<Val> {
        // Views: no kernel, on whichever side the operand lives.
        match &node.prim {
            Prim::Reshape if ins[0].layout().is_contiguous() => {
                self.stats.views += 1;
                return Ok(ins[0].with_layout(ins[0].layout().reshaped(out_shape)));
            }
            Prim::Transpose { perm } => {
                self.stats.views += 1;
                return Ok(ins[0].with_layout(ins[0].layout().transposed(perm)));
            }
            Prim::Slice { axis, start } => {
                self.stats.views += 1;
                let a = *axis as usize;
                return Ok(ins[0].with_layout(ins[0].layout().sliced(a, *start as usize, out_shape[a])));
            }
            Prim::Broadcast => {
                self.stats.views += 1;
                return Ok(ins[0].with_layout(ins[0].layout().broadcast_to(out_shape)));
            }
            Prim::Gather { axis, batch_dims: 0 } if ins[1].layout().rank == 0 && self.host_scalar(&node.inputs[1]).is_some() => {
                // One row along the axis by an index the host knows (the token, a const): a view.
                let i = self.host_scalar(&node.inputs[1]).expect("checked");
                let a = *axis as usize;
                let l = ins[0].layout();
                if i < 0 || i >= l.shape[a] as i128 {
                    return Err(TirError::new(TirErrorKind::Index, format!("Gather: index {i} outside [0, {})", l.shape[a])));
                }
                let mut v = l;
                v.offset += i as usize * l.strides[a];
                self.stats.views += 1;
                return Ok(ins[0].with_layout(v.without_axis(a)));
            }
            Prim::Cast | Prim::Clamp { .. }
                if node.identity
                    && matches!(&ins[0], Val::Dev(t) if Some(t.form) == Form::computed(node.store) && t.dtype == node.store) =>
            {
                self.stats.views += 1;
                return Ok(ins[0].clone());
            }
            _ => {}
        }
        // The device, when every operand is there and the plan allows it.
        let dev_ins: Option<Vec<DevTensor>> = ins
            .iter()
            .map(|v| match v {
                Val::Dev(t) => Some(t.clone()),
                Val::Host(..) => None,
            })
            .collect();
        let tried: Result<Val, Unsupported> = match (&dev_ins, &node.prim) {
            (None, _) => Err(Unsupported::I128Operand),
            (Some(d), Prim::StateWrite { state }) => self.state_write(rec, node, *state, layer, &d[0], slot),
            (Some(d), Prim::HistAppend { state }) => {
                // A copy into the history; a history of the wrong length is the reference's
                // Position error, never a fallback.
                let v = self.hist_append(rec, *state, layer, &d[0], slot)?;
                self.stats.device_nodes += 1;
                return Ok(v);
            }
            (Some(d), _) => {
                let refs: Vec<&DevTensor> = d.iter().collect();
                rec.node(node, &refs, out_shape, slot).map(Val::Dev)
            }
        };
        match tried {
            Ok(v) => {
                self.stats.device_nodes += 1;
                Ok(v)
            }
            Err(u) => {
                *self.stats.fallback.entry(format!("{}: {u:?}", node.prim.name())).or_default() += 1;
                self.fallback(rec, node, ins, out_shape, slot, layer)
            }
        }
    }

    /// The value of a host-known scalar operand: the token, the position, or a scalar const.
    fn host_scalar(&self, r: &Ref) -> Option<i128> {
        match *r {
            Ref::Const(j) => {
                let b = &self.plan.consts[j as usize];
                (b.len() == 1).then(|| b.to_i128s()[0])
            }
            Ref::Input(k) => Some(self.inputs_host[k as usize & 1] as i128),
            _ => None,
        }
    }

    fn state_write(
        &mut self,
        rec: &mut Recorder<'_>,
        node: &NodePlan,
        state: u16,
        layer: Option<u16>,
        x: &DevTensor,
        slot: u32,
    ) -> Result<Val, Unsupported> {
        let inst = self.plan.instance(state, layer).ok_or(Unsupported::StatePrimitive)? as usize;
        let shape = self.plan.instances[inst].shape.clone();
        let dtype = self.plan.instances[inst].dtype;
        let form = Form::computed(dtype).ok_or(Unsupported::I128Store)?;
        let dst = match &self.fixed_next[inst] {
            Some(t) => t.clone(),
            None => {
                let t = DevTensor { buf: self.dev.alloc(form, numel(&shape)), form, dtype, layout: Layout::contiguous(&shape) };
                self.fixed_next[inst] = Some(t.clone());
                t
            }
        };
        rec.state_write(node, &self.plan.program.states[state as usize], x, &dst, slot)?;
        self.written[inst] = true;
        Ok(Val::Dev(dst))
    }

    fn hist_append(
        &mut self,
        rec: &mut Recorder<'_>,
        state: u16,
        layer: Option<u16>,
        row: &DevTensor,
        slot: u32,
    ) -> Result<Val, TirError> {
        let inst = self.plan.instance(state, layer).ok_or_else(|| missing("history instance".into()))? as usize;
        let pos = self.pos as usize;
        let dev = self.dev;
        let h = self.hist[inst].as_mut().ok_or_else(|| missing("history".into()))?;
        let want = pos.min(h.window as usize - 1);
        if h.rows != want || h.pending {
            return Err(TirError::new(TirErrorKind::Position, format!("history: {} prior rows, want {want}", h.rows)));
        }
        if h.start + h.rows + 1 > h.cap {
            // Grow (or compact): the live rows to the front of a fresh buffer.
            let cap = (2 * (h.rows + 1)).max(8);
            let buf = dev.alloc(h.form, cap * h.row);
            if h.rows > 0 {
                let w = 4u64; // lanes: states are i8/i16/i32
                rec.encoder().copy_buffer_to_buffer(&h.buf, (h.start * h.row) as u64 * w, &buf, 0, (h.rows * h.row) as u64 * w);
            }
            h.buf = buf;
            h.cap = cap;
            h.start = 0;
        }
        let at = (h.start + h.rows) * h.row;
        let region = Layout::contiguous_at(&h.row_shape, at);
        let row_shape = h.row_shape.clone();
        let (buf, form) = (Arc::clone(&h.buf), h.form);
        rec.ew(EwOp::Copy, &row_shape, (&buf, form, &region), &[(row, row.layout)], &[], &[], None, false, slot)
            .map_err(|u| TirError::new(TirErrorKind::Shape, format!("history row: {u:?}")))?;
        h.pending = true;
        let mut shape = vec![h.rows + 1];
        shape.extend_from_slice(&h.row_shape);
        Ok(Val::Dev(DevTensor { buf, form, dtype: h.dtype, layout: Layout::contiguous_at(&shape, h.start * h.row) }))
    }

    /// Run `node` on the CPU executor's kernel: synchronise (an earlier device failure wins), bring
    /// the operands to the host, run, and send the result back when the device can hold it.
    fn fallback(
        &mut self,
        rec: &mut Recorder<'_>,
        node: &NodePlan,
        ins: &[Val],
        out_shape: &[usize],
        slot: u32,
        layer: Option<u16>,
    ) -> TirResult<Val> {
        let dev = self.dev;
        rec.flush();
        self.stats.syncs += 1;
        let status = unpack(&dev.read_bytes(&rec.status, 0, 4 * rec.slots as u64), Form::U32, rec.slots as usize);
        for (s, w) in status.iter().enumerate().take(slot as usize) {
            if let Some(f) = DeviceFailure::decode(*w as u32) {
                return Err(f.to_error(&format!("slot {s}")));
            }
        }
        let host: Vec<Buf> = ins
            .iter()
            .map(|v| match v {
                Val::Dev(t) => dev.download(t),
                Val::Host(b, l) => host_gather(b, l),
            })
            .collect();
        let opds: Vec<Opd<'_>> =
            host.iter().zip(ins).map(|(b, v)| Opd { data: b.slice(), layout: Layout::contiguous(v.layout().shape()) }).collect();
        let mut out = Buf::default();
        let mut scratch = Scratch::default();
        let states = &self.plan.program.states;
        match &node.prim {
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                elementwise::binary(node, &opds[0], &opds[1], out_shape, &mut out, &mut scratch)?
            }
            Prim::Select => elementwise::select(node, &opds[0], &opds[1], &opds[2], out_shape, &mut out, &mut scratch)?,
            Prim::Cast
            | Prim::Clamp { .. }
            | Prim::Log2Floor
            | Prim::IntExp
            | Prim::IntRsqrt
            | Prim::IntLn
            | Prim::StateWrite { .. } => elementwise::unary(node, &opds[0], &mut out, &mut scratch, states)?,
            Prim::MatMul => matmul::matmul(node, &opds[0], &opds[1], out_shape, &mut out, &mut scratch)?,
            Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => reduce::reduce(node, &opds[0], &mut out, &mut scratch)?,
            Prim::TopK { .. } => misc::topk(node, &opds[0], &mut out, &mut scratch)?,
            Prim::Gather { .. } => misc::gather(node, &opds[0], &opds[1], &mut out, &mut scratch)?,
            Prim::Concat { .. } => misc::concat(node, &opds, out_shape, &mut out)?,
            Prim::Iota { .. } => misc::iota(node, out_shape, &mut out)?,
            Prim::Reshape => misaka_palw_tir_exec::kernels::materialize(&opds[0], &mut out),
            other => return Err(TirError::new(TirErrorKind::Shape, format!("{} has no CPU fallback here", other.name()))),
        }
        if let Prim::StateWrite { state } = node.prim {
            let inst = self.plan.instance(state, layer).ok_or_else(|| missing("state instance".into()))? as usize;
            let form = Form::computed(out.dtype()).expect("states are narrow");
            let t = dev.upload(out.slice(), form, out_shape);
            self.fixed_next[inst] = Some(t.clone());
            self.written[inst] = true;
            return Ok(Val::Dev(t));
        }
        Ok(match Form::computed(out.dtype()) {
            Some(f) => Val::Dev(dev.upload(out.slice(), f, out_shape)),
            None => Val::Host(Arc::new(out), Layout::contiguous(out_shape)),
        })
    }
}

/// The elements a host layout names, row-major, as a contiguous buffer of its dtype.
fn host_gather(b: &Buf, l: &Layout) -> Buf {
    let mut out = Buf::default();
    misaka_palw_tir_exec::kernels::materialize(&Opd { data: b.slice(), layout: *l }, &mut out);
    out
}
