//! **The kernels' host side: which node the device may run, under which proof, and how.**
//!
//! A kernel never decides what may be assumed: the CPU executor's refined [`NodePlan`] does. The
//! device runs a node only where that plan's proof covers the device's arithmetic:
//!
//! | node | runs on the device when | how |
//! | --- | --- | --- |
//! | `Add` `Sub` `Mul` `Div` `Compare` `Select` `Cast` `Clamp` `Log2Floor` the transcendentals | `work = I64` (every operand and exact result fits `i64`) | one element per invocation in wrapping `i64` — exact, because the true result fits; the dtype check exactly where the plan keeps it (`check_out`) |
//! | `MatMul` | `Acc::Fast64` / `Acc::Pn64` | terms in `i32` chunks when the operand intervals allow it, combined in `i64`; `Pn64` keeps positives and negatives apart and checks them |
//! | `ReduceSum` | `Acc::Fast64` / `Acc::Pn64` | as `MatMul` |
//! | `ReduceMax`, `TopK` | `work = I64` | the maximum; the rank of every element by (value desc, index asc) |
//! | `Gather`, `Iota`, `Concat`, the views | always (`Iota`: its exact interval inside `i64`) | index arithmetic; the index check where the plan keeps it |
//!
//! Everything else — an `i128` working type, an `i128` stored value, a `Fast128`/`Pn128` sum — is
//! [`Unsupported`], and the executor runs the CPU kernel for that node. No node is ever run on the
//! device under a weaker assumption than the CPU's.

use std::sync::Arc;

use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::program::{StateDecl, StateKind};
use misaka_palw_tir::{DType, Prim, TirError, TirErrorKind};
use misaka_palw_tir_exec::layout::{Layout, MAX_RANK, numel, row_major};
use misaka_palw_tir_exec::plan::{Acc, NodePlan, Work};
use wgpu::util::DeviceExt;

use crate::device::GpuDevice;
use crate::tensor::{DevTensor, Form};
use crate::wgsl::{self, EwKey, EwOp, GemmKey, GemvKey, MatMulKey, ReduceKey, SumMode};

/// Why a node does not run on the device. The executor runs the CPU kernel instead — always
/// correct, since it IS the CPU executor's kernel under the same plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unsupported {
    /// The plan computes the node in `i128`.
    I128Work,
    /// The node's value is stored as `i128`.
    I128Store,
    /// An exact sum the plan accumulates in `i128` (`Fast128`, `Pn128`).
    WideSum,
    /// An `Iota` whose exact values leave `i64`.
    IotaPastI64,
    /// An operand held as `i128` (never on the device).
    I128Operand,
    /// An element index past `u32` (a tensor of more than 2^32 elements in one buffer).
    PastU32,
    /// `StateWrite` and `HistAppend` belong to the executor (they write run state).
    StatePrimitive,
}

/// The class a status word reports (spec 04b §9.3).
fn class_of(code: u32) -> TirErrorKind {
    match code {
        2 => TirErrorKind::Divisor,
        3 => TirErrorKind::Index,
        _ => TirErrorKind::Overflow,
    }
}

/// A failure the device reported for one node: the first failing element in output order and its
/// class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceFailure {
    pub element: u32,
    pub kind: TirErrorKind,
}

impl DeviceFailure {
    /// Decode a status word (0 = the node succeeded).
    pub fn decode(word: u32) -> Option<Self> {
        (word != 0).then(|| {
            let key = !word;
            DeviceFailure { element: key >> 2, kind: class_of(key & 3) }
        })
    }
    pub fn to_error(self, prim: &str) -> TirError {
        TirError::new(self.kind, format!("{prim} (device): element {} failed", self.element))
    }
}

/// Parameter words of one dispatch.
#[derive(Default)]
pub struct Params(pub Vec<u32>);

impl Params {
    fn at(&mut self, i: usize) -> &mut u32 {
        if self.0.len() <= i {
            self.0.resize(i + 1, 0);
        }
        &mut self.0[i]
    }
    fn set(&mut self, i: usize, v: u32) {
        *self.at(i) = v;
    }
    fn set64(&mut self, i: usize, v: i64) {
        let u = v as u64;
        self.set(i, u as u32);
        self.set(i + 1, (u >> 32) as u32);
    }
}

/// Elements of a value as a `u32`, or [`Unsupported::PastU32`].
fn u32_of(v: usize) -> Result<u32, Unsupported> {
    u32::try_from(v).map_err(|_| Unsupported::PastU32)
}

/// `(base, four strides)` of `layout`, read at the multi-index of a `rank`-dim index space
/// right-aligned to rank 4 (the layout already has the index space's rank).
fn geom(layout: &Layout) -> Result<[u32; 5], Unsupported> {
    let r = layout.rank as usize;
    u32_of(layout.extent())?;
    let mut g = [0u32; 5];
    g[0] = u32_of(layout.offset)?;
    for d in 0..r {
        g[1 + (MAX_RANK - r) + d] = u32_of(layout.strides[d])?;
    }
    Ok(g)
}

/// The output dims right-aligned to rank 4.
fn dims4(shape: &[usize]) -> Result<[u32; 4], Unsupported> {
    let mut d = [1u32; 4];
    let r = shape.len();
    for (i, x) in shape.iter().enumerate() {
        d[MAX_RANK - r + i] = u32_of(*x)?;
    }
    Ok(d)
}

/// `[lo, hi]` of a dtype inside `i64` (an `idx` or narrower bound is exact; an `i64` one is all of it).
fn bounds64(d: DType) -> (i64, i64) {
    (d.min_value().max(i64::MIN as i128) as i64, d.max_value().min(i64::MAX as i128) as i64)
}

/// The largest `|a·b|` over two intervals, when both lie inside `i32`.
fn max_term_i32(a: Interval, b: Interval) -> Option<i128> {
    let i32iv = Interval::of(DType::I32);
    if !(i32iv.contains(a.lo) && i32iv.contains(a.hi) && i32iv.contains(b.lo) && i32iv.contains(b.hi)) {
        return None;
    }
    Some([a.lo * b.lo, a.lo * b.hi, a.hi * b.lo, a.hi * b.hi].iter().map(|v| v.abs()).max().unwrap_or(0))
}

/// Terms an `i32` partial sum may take without leaving `i32` (`None`: terms are `i64`).
pub fn i32_chunk(a: Interval, b: Interval) -> Option<u32> {
    let t = max_term_i32(a, b)?;
    if t > i32::MAX as i128 {
        return None;
    }
    Some((i32::MAX as i128 / t.max(1)).min(u32::MAX as i128) as u32)
}

/// Dispatches recorded against one status buffer (a step, or a single kernel call).
pub struct Recorder<'d> {
    pub dev: &'d GpuDevice,
    pub enc: wgpu::CommandEncoder,
    pub status: Arc<wgpu::Buffer>,
    pub slots: u32,
    pub dispatches: usize,
    /// The kernel of every dispatch, in order (`ew:Add`, `gemv`, `gemm`, `matmul`, …): what ran.
    pub log: Vec<String>,
}

impl<'d> Recorder<'d> {
    /// A recorder with `slots` status words, all 0 (no failure).
    pub fn new(dev: &'d GpuDevice, slots: u32) -> Self {
        let status = Arc::new(dev.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tir status"),
            size: (4 * slots.max(4) as u64).div_ceil(16) * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        let mut enc = dev.device.create_command_encoder(&Default::default());
        enc.clear_buffer(&status, 0, None);
        Recorder { dev, enc, status, slots, dispatches: 0, log: Vec::new() }
    }

    /// Record one dispatch of `src` over `groups` workgroups.
    pub fn dispatch(&mut self, src: &str, params: &Params, out: &wgpu::Buffer, ins: &[&wgpu::Buffer], groups: (u32, u32, u32)) {
        let pipeline = self.dev.pipeline(src, ins.len());
        let mut words = params.0.clone();
        if words.is_empty() {
            words.push(0);
        }
        let pbuf = self.dev.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("tir params"),
            contents: bytemuck::cast_slice(&words),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let layout = self.dev.layout(ins.len());
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: pbuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: out.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: self.status.as_entire_binding() },
        ];
        for (k, b) in ins.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: 3 + k as u32, resource: b.as_entire_binding() });
        }
        let bg = self.dev.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tir kernel"),
            layout: &layout,
            entries: &entries,
        });
        let mut pass = self.enc.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.dispatch_workgroups(groups.0, groups.1, groups.2);
        self.dispatches += 1;
    }

    /// Workgroups for `items` invocations of `per` each, split over two dimensions past 65,535.
    fn groups(&self, items: u32, per: u32) -> (u32, u32, u32) {
        let wg = items.div_ceil(per).max(1);
        let max = self.dev.limits.max_compute_workgroups_per_dimension.max(1);
        if wg <= max { (wg, 1, 1) } else { (max, wg.div_ceil(max), 1) }
    }

    /// Submit everything recorded and wait; return the status words.
    pub fn finish(self) -> Vec<u32> {
        let Recorder { dev, enc, status, slots, .. } = self;
        dev.queue.submit([enc.finish()]);
        let bytes = dev.read_bytes(&status, 0, 4 * slots.max(1) as u64);
        bytes.chunks_exact(4).take(slots as usize).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
    }

    /// Submit what is recorded so far without waiting, and start a new encoder (the status words
    /// carry over).
    pub fn flush(&mut self) {
        let enc = std::mem::replace(&mut self.enc, self.dev.device.create_command_encoder(&Default::default()));
        self.dev.queue.submit([enc.finish()]);
    }

    // ---------------------------------------------------------------- elementwise

    /// An elementwise kernel: `geometry` is the index space (the output's shape, or the copied
    /// input's), `out` the destination layout over that space, `ins` each operand's layout over
    /// that space (broadcast strides already applied), `consts` the op's 64-bit constants, `extra`
    /// words after them (Gather's axis, Iota's axis), `check` the dtype the result is checked
    /// against.
    #[allow(clippy::too_many_arguments)]
    pub fn ew(
        &mut self,
        op: EwOp,
        geometry: &[usize],
        out: (&wgpu::Buffer, Form, &Layout),
        ins: &[(&DevTensor, Layout)],
        consts: &[i64],
        extra: &[u32],
        check: Option<DType>,
        check_operand: bool,
        slot: u32,
    ) -> Result<(), Unsupported> {
        let n = u32_of(numel(geometry))?;
        let key = EwKey { op, ins: ins.iter().map(|(t, _)| t.form).collect(), out: out.1, check_out: check.is_some(), check_operand };
        let mut p = Params::default();
        p.set(0, n);
        p.set(1, slot);
        for (i, d) in dims4(geometry)?.iter().enumerate() {
            p.set(2 + i, *d);
        }
        for (i, g) in geom(out.2)?.iter().enumerate() {
            p.set(6 + i, *g);
        }
        for (k, (_, l)) in ins.iter().enumerate() {
            for (i, g) in geom(l)?.iter().enumerate() {
                p.set(11 + 5 * k + i, *g);
            }
        }
        let mut at = wgsl::ew_const_base(ins.len());
        for c in consts {
            p.set64(at, *c);
            at += 2;
        }
        for (i, w) in extra.iter().enumerate() {
            p.set(at + i, *w);
        }
        if !extra.is_empty() {
            at += 2;
        }
        if let Some(d) = check {
            let (lo, hi) = bounds64(d);
            p.set64(at, lo);
            p.set64(at + 2, hi);
        }
        let src = wgsl::ew_source(&key);
        self.log.push(format!("ew:{op:?}"));
        let bufs: Vec<&wgpu::Buffer> = ins.iter().map(|(t, _)| &*t.buf).collect();
        let groups = self.groups(n, 256);
        self.dispatch(&src, &p, out.0, &bufs, groups);
        Ok(())
    }

    /// A fresh contiguous output of `shape` for a node stored as `store`.
    pub fn output(&self, store: DType, out_dtype: DType, shape: &[usize]) -> Result<DevTensor, Unsupported> {
        let form = Form::computed(store).ok_or(Unsupported::I128Store)?;
        Ok(DevTensor { buf: self.dev.alloc(form, numel(shape)), form, dtype: out_dtype, layout: Layout::contiguous(shape) })
    }

    /// Materialise any layout of `x` as a contiguous tensor of `form` (a view the executor must
    /// hand on whole, a `Reshape` of a non-contiguous value, a strided `TopK` operand).
    pub fn materialize(&mut self, x: &DevTensor, form: Form, slot: u32) -> Result<DevTensor, Unsupported> {
        let shape = x.shape().to_vec();
        let out = DevTensor { buf: self.dev.alloc(form, numel(&shape)), form, dtype: x.dtype, layout: Layout::contiguous(&shape) };
        self.ew(EwOp::Copy, &shape, (&out.buf, form, &out.layout), &[(x, x.layout)], &[], &[], None, false, slot)?;
        Ok(out)
    }

    // ---------------------------------------------------------------- one node

    /// **Run one computing node** on device operands (in input order), into a fresh output — or
    /// say why the device does not run it. Structural primitives are materialised here (the
    /// executor treats them as views instead).
    pub fn node(&mut self, node: &NodePlan, ins: &[&DevTensor], out_shape: &[usize], slot: u32) -> Result<DevTensor, Unsupported> {
        if ins.iter().any(|t| t.dtype == DType::I128 && t.form != Form::I64) {
            return Err(Unsupported::I128Operand);
        }
        let out_dtype = node.out.dtype;
        let bc = |t: &DevTensor| t.layout.broadcast_to(out_shape);
        match &node.prim {
            Prim::Reshape => {
                let x = ins[0];
                let o = self.output(node.store, out_dtype, out_shape)?;
                // Copy over the INPUT's index space: element e of its row-major order is element e
                // of the contiguous output.
                let space = Layout::contiguous(x.shape());
                self.ew(EwOp::Copy, x.shape(), (&o.buf, o.form, &space), &[(x, x.layout)], &[], &[], None, false, slot)?;
                Ok(o)
            }
            Prim::Transpose { perm } => self.copy_view(node, ins[0], ins[0].layout.transposed(perm), out_shape, slot),
            Prim::Slice { axis, start } => {
                let l = ins[0].layout.sliced(*axis as usize, *start as usize, out_shape[*axis as usize]);
                self.copy_view(node, ins[0], l, out_shape, slot)
            }
            Prim::Broadcast => self.copy_view(node, ins[0], ins[0].layout.broadcast_to(out_shape), out_shape, slot),
            Prim::Concat { axis } => {
                let o = self.output(node.store, out_dtype, out_shape)?;
                let whole = Layout::contiguous(out_shape);
                let a = *axis as usize;
                let mut start = 0usize;
                for x in ins {
                    let e = x.shape()[a];
                    let region = whole.sliced(a, start, e);
                    self.ew(EwOp::Copy, x.shape(), (&o.buf, o.form, &region), &[(x, x.layout)], &[], &[], None, false, slot)?;
                    start += e;
                }
                Ok(o)
            }
            Prim::Iota { axis, start, step } => {
                let exact = node.facts.exact.ok_or(Unsupported::IotaPastI64)?;
                if !(Interval::of(DType::I64).contains(exact.lo) && Interval::of(DType::I64).contains(exact.hi)) {
                    return Err(Unsupported::IotaPastI64);
                }
                let o = self.output(node.store, out_dtype, out_shape)?;
                let ax = (MAX_RANK - out_shape.len() + *axis as usize) as u32;
                let check = node.check_out.then_some(out_dtype);
                let ol = o.layout;
                self.ew(EwOp::Iota, out_shape, (&o.buf, o.form, &ol), &[], &[*start, *step], &[ax, 0], check, false, slot)?;
                Ok(o)
            }
            Prim::Gather { axis, batch_dims } => {
                self.gather(node, ins[0], ins[1], *axis as usize, *batch_dims as usize, out_shape, slot)
            }
            Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                if node.work != Work::I64 || node.checked_arith {
                    return Err(Unsupported::I128Work);
                }
                let op = match &node.prim {
                    Prim::Add => EwOp::Add,
                    Prim::Sub => EwOp::Sub,
                    Prim::Mul => EwOp::Mul,
                    Prim::Div { rule } => EwOp::Div(rule_tag(*rule)),
                    Prim::Compare { cmp } => EwOp::Compare(cmp_tag(*cmp)),
                    _ => unreachable!(),
                };
                let o = self.output(node.store, out_dtype, out_shape)?;
                let check = node.check_out.then_some(out_dtype);
                let ol = o.layout;
                self.ew(
                    op,
                    out_shape,
                    (&o.buf, o.form, &ol),
                    &[(ins[0], bc(ins[0])), (ins[1], bc(ins[1]))],
                    &[],
                    &[],
                    check,
                    false,
                    slot,
                )?;
                Ok(o)
            }
            Prim::Select => {
                if node.work != Work::I64 {
                    return Err(Unsupported::I128Work);
                }
                let o = self.output(node.store, out_dtype, out_shape)?;
                let check = node.check_out.then_some(out_dtype);
                let ol = o.layout;
                let ops = [(ins[0], bc(ins[0])), (ins[1], bc(ins[1])), (ins[2], bc(ins[2]))];
                self.ew(EwOp::Select, out_shape, (&o.buf, o.form, &ol), &ops, &[], &[], check, false, slot)?;
                Ok(o)
            }
            Prim::Cast | Prim::Clamp { .. } | Prim::Log2Floor | Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => {
                if node.work != Work::I64 {
                    return Err(Unsupported::I128Work);
                }
                let (op, consts, check) = match &node.prim {
                    Prim::Cast => (EwOp::Copy, vec![], node.check_out),
                    // The CPU clamps with the bounds saturated into its working type.
                    Prim::Clamp { lo, hi } => (EwOp::Clamp, vec![*lo, *hi], false),
                    Prim::Log2Floor => (EwOp::Log2Floor, vec![], node.check_out),
                    Prim::IntExp => (EwOp::IntExp, vec![], node.check_out),
                    Prim::IntRsqrt => (EwOp::IntRsqrt, vec![], node.check_out),
                    Prim::IntLn => (EwOp::IntLn, vec![], node.check_out),
                    _ => unreachable!(),
                };
                let o = self.output(node.store, out_dtype, out_shape)?;
                let ol = o.layout;
                self.ew(
                    op,
                    out_shape,
                    (&o.buf, o.form, &ol),
                    &[(ins[0], bc(ins[0]))],
                    &consts,
                    &[],
                    check.then_some(out_dtype),
                    false,
                    slot,
                )?;
                Ok(o)
            }
            Prim::MatMul => self.matmul(node, ins[0], ins[1], out_shape, slot),
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => self.reduce(node, ins[0], *axis as usize, out_shape, slot),
            Prim::TopK { axis, k } => self.topk(node, ins[0], *axis as usize, *k, out_shape, slot),
            Prim::StateWrite { .. } | Prim::HistAppend { .. } => Err(Unsupported::StatePrimitive),
        }
    }

    /// `StateWrite`'s value: `x` saturated to the state's `[lo, hi]`, written into `dst` (the
    /// state's pending buffer, contiguous, of the state's computed form).
    pub fn state_write(
        &mut self,
        node: &NodePlan,
        state: &StateDecl,
        x: &DevTensor,
        dst: &DevTensor,
        slot: u32,
    ) -> Result<(), Unsupported> {
        if node.work != Work::I64 {
            return Err(Unsupported::I128Work);
        }
        let StateKind::Fixed { lo, hi } = state.kind else { return Err(Unsupported::StatePrimitive) };
        let shape = dst.shape().to_vec();
        let dl = dst.layout;
        self.ew(
            EwOp::Clamp,
            &shape,
            (&dst.buf, dst.form, &dl),
            &[(x, x.layout.broadcast_to(&shape))],
            &[lo, hi],
            &[],
            None,
            false,
            slot,
        )
    }

    fn copy_view(
        &mut self,
        node: &NodePlan,
        x: &DevTensor,
        view: Layout,
        out_shape: &[usize],
        slot: u32,
    ) -> Result<DevTensor, Unsupported> {
        let o = self.output(node.store, node.out.dtype, out_shape)?;
        let ol = o.layout;
        self.ew(EwOp::Copy, out_shape, (&o.buf, o.form, &ol), &[(x, view)], &[], &[], None, false, slot)?;
        Ok(o)
    }

    // ---------------------------------------------------------------- Gather

    #[allow(clippy::too_many_arguments)]
    fn gather(
        &mut self,
        node: &NodePlan,
        data: &DevTensor,
        idx: &DevTensor,
        a: usize,
        b: usize,
        out_shape: &[usize],
        slot: u32,
    ) -> Result<DevTensor, Unsupported> {
        let o = self.output(node.store, node.out.dtype, out_shape)?;
        let r = out_shape.len();
        let m = idx.shape().len() - b;
        // Per output dim: the data's and the indices' strides (spec 04b §6.2's index map).
        let mut dl = Layout { rank: r as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: data.layout.offset };
        let mut il = Layout { rank: r as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: idx.layout.offset };
        for (d, ext) in out_shape.iter().enumerate() {
            dl.shape[d] = *ext;
            il.shape[d] = *ext;
            if d < a {
                dl.strides[d] = data.layout.strides[d];
                if d < b {
                    il.strides[d] = idx.layout.strides[d];
                }
            } else if d < a + m {
                il.strides[d] = idx.layout.strides[b + (d - a)];
            } else {
                dl.strides[d] = data.layout.strides[d - m + 1];
            }
        }
        // The data's whole extent must be addressable: its last element through the axis too.
        u32_of(data.layout.extent())?;
        let extent = u32_of(data.layout.shape[a])?;
        let stride = u32_of(data.layout.strides[a])?;
        let ol = o.layout;
        self.ew(
            EwOp::Gather,
            out_shape,
            (&o.buf, o.form, &ol),
            &[(data, dl), (idx, il)],
            &[],
            &[extent, stride],
            None,
            node.check_operand,
            slot,
        )?;
        Ok(o)
    }

    // ---------------------------------------------------------------- MatMul

    fn matmul(
        &mut self,
        node: &NodePlan,
        a: &DevTensor,
        b: &DevTensor,
        out_shape: &[usize],
        slot: u32,
    ) -> Result<DevTensor, Unsupported> {
        let mode = match node.acc {
            Acc::Fast64 => SumMode::Fast,
            Acc::Pn64 => SumMode::PosNeg,
            Acc::Fast128 | Acc::Pn128 => return Err(Unsupported::WideSum),
        };
        let o = self.output(node.store, node.out.dtype, out_shape)?;
        let (ash, bsh) = (a.shape(), b.shape());
        let (ra, rb, ro) = (ash.len(), bsh.len(), out_shape.len());
        let (m, k, n) = (ash[ra - 2], ash[ra - 1], bsh[rb - 1]);
        let batch = &out_shape[..ro - 2];
        if batch.len() > 2 {
            return Err(Unsupported::PastU32);
        }
        let mut p = Params::default();
        let outputs = u32_of(numel(out_shape))?;
        p.set(0, outputs);
        p.set(1, slot);
        p.set(2, u32_of(m)?);
        p.set(3, u32_of(n)?);
        p.set(4, u32_of(k)?);
        p.set(5, u32_of(a.layout.strides[ra - 2])?);
        p.set(6, u32_of(a.layout.strides[ra - 1])?);
        p.set(7, u32_of(b.layout.strides[rb - 2])?);
        p.set(8, u32_of(b.layout.strides[rb - 1])?);
        // Batch dims (right-aligned to two) and each operand's batch strides (0 where it broadcasts).
        let mut nb = [1usize; 2];
        let (mut abs, mut bbs) = ([0usize; 2], [0usize; 2]);
        for (d, ext) in batch.iter().enumerate() {
            let slot2 = 2 - batch.len() + d;
            nb[slot2] = *ext;
            if let Some(kk) = (d + (ra - 2)).checked_sub(ro - 2)
                && ash[kk] != 1
            {
                abs[slot2] = a.layout.strides[kk];
            }
            if let Some(kk) = (d + (rb - 2)).checked_sub(ro - 2)
                && bsh[kk] != 1
            {
                bbs[slot2] = b.layout.strides[kk];
            }
        }
        p.set(9, u32_of(nb[0])?);
        p.set(10, u32_of(nb[1])?);
        p.set(11, u32_of(abs[0])?);
        p.set(12, u32_of(abs[1])?);
        p.set(13, u32_of(bbs[0])?);
        p.set(14, u32_of(bbs[1])?);
        u32_of(a.layout.extent())?;
        u32_of(b.layout.extent())?;
        p.set(17, u32_of(a.layout.offset)?);
        p.set(18, u32_of(b.layout.offset)?);
        let (lo, hi) = bounds64(node.out.dtype);
        p.set64(19, hi);
        p.set64(21, lo);
        p.set(23, 0);
        let chunk = i32_chunk(node.in_ivs[0], node.in_ivs[1]);
        let nbatch = nb[0] * nb[1];
        let bufs = [&*a.buf, &*b.buf];
        // The matrix–vector product over packed weight rows (a decode projection).
        let gemv_ok = mode == SumMode::Fast
            && a.form == Form::P8
            && n == 1
            && nbatch == 1
            && k >= 4
            && k % 4 == 0
            && a.layout.strides[ra - 1] == 1
            && a.layout.strides[ra - 2].is_multiple_of(4)
            && a.layout.offset.is_multiple_of(4);
        if gemv_ok {
            let words = chunk.map(|c| c / 4).filter(|w| *w >= 1);
            p.set(16, words.unwrap_or(1));
            let key = GemvKey { b: b.form, out: o.form, chunked: words.is_some() };
            self.log.push(format!("gemv{}", if key.chunked { ":i32" } else { ":i64" }));
            let groups = self.groups(u32_of(m)?, 4);
            self.dispatch(&wgsl::gemv_source(&key), &p, &o.buf, &bufs, groups);
            return Ok(o);
        }
        // Many output columns (a batch of positions): tiled through workgroup memory.
        if mode == SumMode::Fast && n >= 8 && m * n >= 4096 && nbatch <= 65_535 {
            let tiles = chunk.map(|c| c / 16).filter(|t| *t >= 1);
            p.set(16, tiles.unwrap_or(1));
            let key = GemmKey { a: a.form, b: b.form, out: o.form, chunked: tiles.is_some() };
            self.log.push(format!("gemm{}", if key.chunked { ":i32" } else { ":i64" }));
            let groups = (u32_of(n.div_ceil(64))?, u32_of(m.div_ceil(64))?, u32_of(nbatch)?);
            self.dispatch(&wgsl::gemm_source(&key), &p, &o.buf, &bufs, groups);
            return Ok(o);
        }
        let chunked = chunk.filter(|c| *c >= 2);
        p.set(16, chunked.unwrap_or(1));
        let key = MatMulKey { a: a.form, b: b.form, out: o.form, mode, chunked: chunked.is_some() };
        self.log.push(format!("matmul:{mode:?}{}", if key.chunked { ":i32" } else { ":i64" }));
        let groups = self.groups(outputs, 256);
        self.dispatch(&wgsl::matmul_source(&key), &p, &o.buf, &bufs, groups);
        Ok(o)
    }

    // ---------------------------------------------------------------- reductions

    fn reduce(
        &mut self,
        node: &NodePlan,
        x: &DevTensor,
        axis: usize,
        out_shape: &[usize],
        slot: u32,
    ) -> Result<DevTensor, Unsupported> {
        let (max, mode) = match (&node.prim, node.acc) {
            (Prim::ReduceMax { .. }, _) => {
                if node.work != Work::I64 {
                    return Err(Unsupported::I128Work);
                }
                (true, SumMode::Fast)
            }
            (_, Acc::Fast64) => (false, SumMode::Fast),
            (_, Acc::Pn64) => (false, SumMode::PosNeg),
            _ => return Err(Unsupported::WideSum),
        };
        let o = self.output(node.store, node.out.dtype, out_shape)?;
        let n_out = u32_of(numel(out_shape))?;
        let ext = x.shape()[axis];
        let cooperative = ext >= 512;
        let key = ReduceKey { max, mode, x: x.form, out: o.form, cooperative };
        self.log.push(format!("{}:{mode:?}{}", if max { "reduce_max" } else { "reduce_sum" }, if cooperative { ":wg" } else { "" }));
        let mut p = Params::default();
        p.set(0, n_out);
        p.set(1, slot);
        for (i, d) in dims4(out_shape)?.iter().enumerate() {
            p.set(2 + i, *d);
        }
        for (i, g) in geom(&o.layout)?.iter().enumerate() {
            p.set(6 + i, *g);
        }
        for (i, g) in geom(&x.layout)?.iter().enumerate() {
            p.set(11 + i, *g);
        }
        p.set(16, u32_of(ext)?);
        p.set(17, u32_of(x.layout.strides[axis])?);
        let (lo, hi) = bounds64(node.out.dtype);
        p.set64(18, hi);
        p.set64(20, lo);
        let groups = if cooperative { self.groups(n_out, 1) } else { self.groups(n_out, 256) };
        self.dispatch(&wgsl::reduce_source(&key), &p, &o.buf, &[&*x.buf], groups);
        Ok(o)
    }

    // ---------------------------------------------------------------- TopK

    fn topk(
        &mut self,
        node: &NodePlan,
        x: &DevTensor,
        axis: usize,
        k: u32,
        out_shape: &[usize],
        slot: u32,
    ) -> Result<DevTensor, Unsupported> {
        if node.work != Work::I64 {
            return Err(Unsupported::I128Work);
        }
        let x = if x.layout.is_contiguous() { x.clone() } else { self.materialize(x, x.form, slot)? };
        let sh = x.shape().to_vec();
        let (outer, n, inner): (usize, usize, usize) = (sh[..axis].iter().product(), sh[axis], sh[axis + 1..].iter().product());
        let flags = self.dev.alloc(Form::U32, numel(&sh));
        let mut p = Params::default();
        p.set(0, u32_of(numel(&sh))?);
        p.set(1, slot);
        p.set(2, u32_of(n)?);
        p.set(3, u32_of(inner)?);
        p.set(4, k);
        p.set(5, u32_of(x.layout.offset)?);
        let groups = self.groups(u32_of(numel(&sh))?, 256);
        self.log.push("topk".to_string());
        self.dispatch(&wgsl::topk_rank_source(x.form), &p, &flags, &[&*x.buf], groups);
        let o = self.output(DType::Idx, DType::Idx, out_shape)?;
        p.set(0, u32_of(outer * inner)?);
        let flags_t = DevTensor { buf: flags, form: Form::U32, dtype: DType::Idx, layout: Layout::contiguous(&sh) };
        let groups = self.groups(u32_of(outer * inner)?, 64);
        self.dispatch(&wgsl::topk_emit_source(), &p, &o.buf, &[&*flags_t.buf], groups);
        Ok(o)
    }
}

pub fn rule_tag(r: misaka_palw_tir::Rounding) -> u8 {
    match r {
        misaka_palw_tir::Rounding::Floor => 0,
        misaka_palw_tir::Rounding::HalfUp => 1,
        misaka_palw_tir::Rounding::HalfAwayFromZero => 2,
    }
}

pub fn cmp_tag(c: misaka_palw_tir::Cmp) -> u8 {
    use misaka_palw_tir::Cmp;
    match c {
        Cmp::Eq => 0,
        Cmp::Ne => 1,
        Cmp::Lt => 2,
        Cmp::Le => 3,
        Cmp::Gt => 4,
        Cmp::Ge => 5,
    }
}

/// Row-major strides, for callers building contiguous layouts.
pub fn contiguous_strides(shape: &[usize]) -> [usize; MAX_RANK] {
    row_major(shape)
}
