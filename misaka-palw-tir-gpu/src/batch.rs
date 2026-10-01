//! **Position-batched replay: a whole job, one occurrence at a time over all of its positions.**
//!
//! [`GpuExecutor`] steps a program one position at a time, as `TirExecutor` does: every node of
//! every layer is a dispatch per position — 948 a position at four 1.5B layers — and a step is
//! bound by dispatch cost, not by arithmetic (design `gpu-integer-backend.md` §6). A seat replaying
//! a committed job (a claim's prefill-and-decode run, a court's step leg, RFC-0006's cell over a
//! position segment) knows every token before it starts. [`BatchReplay`] uses that: it runs the
//! occurrences in schedule order and each occurrence ONCE over a chunk of positions — ADR-0117's
//! "a draw is one forward", `forward_prefill_planned` in `misaka-palw-base0` — so a projection is
//! one GEMM over the chunk's positions instead of a GEMV per position, and the dispatches of a job
//! fall from nodes × layers × positions to nodes × layers × chunks.
//!
//! **The batched form.** A value whose per-position shape is `S` is held for the chunk's `T`
//! positions as one tensor `[T] ++ S` — or, when it depends on no position at all (params, consts
//! and what is computed from them alone), once, as `S` ("shared"). A history axis `H`
//! (`H_p = min(p + 1, W)`, spec 04b §2.2) is padded to the chunk's largest, `H_max`; position `p`'s
//! elements past `H_p` are padding. Every kernel whose output has `H` masks the padding — stored 0,
//! never loaded, never failing (`wgsl::RAGGED_MASK`) — and every reduction and contraction over `H`
//! stops at `H_p`. Nothing else can read padding: by the type rules (spec 04b §5–§6, PALW-TIR-8) `H` never
//! broadcasts against a constant, is never sliced, concatenated, gathered or ranked along, and
//! leaves a value only through a reduction or a contraction over it. So each position's valid
//! elements are computed exactly as the per-position executor computes them, from the same operands,
//! under the same refined plan ([`TirPlan::refine`] with the actual param ranges).
//!
//! **Causal attention** needs no new primitive. A `HistAppend` over the chunk writes the chunk's
//! rows into the instance's row store (`[positions] ++ row`, row `p` appended at position `p`), and
//! its window `[H, ..row]` is, for every position at once, one strided view of that store — stride 0
//! along the positions while every position of the chunk still sees every row from the first
//! (`p0 + T ≤ W`), one row once every window is full (`p0 ≥ W − 1`) — or, for the one chunk where
//! the windows start to slide, a gather by host-built row indices. The scores `q·kᵀ` are then one batched `MatMul` whose `N` is the padded
//! `H`, masked; the softmax reduces over `H_p`; the values `p·v` contract over `H_p` (the ragged
//! `MatMul`, `wgsl::matmul_ragged`).
//!
//! **Projections** are folded into one product: `W[M, K]·x[K, 1]` at every position is
//! `W·X[K, T]`, one register-blocked GEMM, whose `[M, T]` result is viewed as `[T, M, 1]` — taken
//! only where the plan proves the sum cannot fail (`Acc::Fast64`/`Fast128`), because its elements
//! are not position-major; `x[1, K]·W[K, N]` folds into `X[T, K]·W` whatever the plan, since that
//! result is.
//!
//! **What stays per position.** An occurrence that writes a `Fixed` state (a recurrence: a GDN or
//! Mamba-2 scan, a counter) runs position by position — its next position reads what this one wrote
//! — still inside the chunk's recording, with every state value kept on the device in a row store
//! (`[positions + 1] ++ shape`, row `p` the value at the START of position `p`), so that an
//! occurrence that only READS a state (`post` reading a global state `pre` wrote) is batched again.
//! An occurrence whose batched form would pass rank 4 runs per position too. Chunked-parallel
//! recurrences are fused kernels under F-2, not this module's business.
//!
//! **Failures** are the CPU executor's: a replay stops at the first position that fails, reporting
//! there the first failing node in slot order and its class (spec 04b §9.3, §9.1's step checks
//! first). A batched node's status word holds its first failing element in `[T] ++ S` order —
//! position-major, so the element's quotient by the per-position count is its position, and within
//! the position the row-major order is the CPU's (padding never fails) — and a per-position node has
//! one status word per position. The replay's failure is the least `(position, slot)` over every
//! word and every host-side check; values reach the sink only for the positions before it, in the
//! CPU executor's order.
//!
//! **Fallback.** A node the device does not run ([`Unsupported`]: checked `i128` arithmetic, an
//! `Iota` past `i64`, a dimension past `u32`) runs on the CPU executor's kernel, position by
//! position on each position's valid elements — after a synchronisation that finds the first
//! failure so far, so that the CPU kernel only ever sees operands the CPU executor would have handed
//! it (positions at or past a failure are never handed to it). A program the batched form cannot
//! hold at all (a param past one binding, a rank-4 carry, a value past one binding at a single
//! position) is replayed by [`GpuExecutor`], position by position.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use misaka_palw_tir::program::{StateDecl, StateKind};
use misaka_palw_tir::{DType, Dim, Prim, Ref, TirError, TirErrorKind, TirResult};
use misaka_palw_tir_exec::elem::{Buf, Slice};
use misaka_palw_tir_exec::kernels::{Opd, Scratch, elementwise, matmul, misc, reduce};
use misaka_palw_tir_exec::layout::{Layout, MAX_RANK, numel, row_major};
use misaka_palw_tir_exec::plan::{Acc, BlockPlan, NodePlan, TirPlan};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirParams};

use crate::device::GpuDevice;
use crate::exec::GpuExecutor;
use crate::kernels::{DeviceFailure, Ragged, Recorder, Unsupported};
use crate::tensor::{DevTensor, Form, unpack};
use crate::wgsl::EwOp;

/// What a replay hands its caller.
pub trait ReplaySink {
    /// Also deliver every uncommitted node's value (as [`StepSink::every_node`]).
    fn every_node(&self) -> bool {
        false
    }
    /// A value of position `pos`: positions ascending, and within one position the order
    /// `TirExecutor` hands its sink (slot order).
    fn node(&mut self, pos: u32, v: &NodeValue<'_>);
    /// Position `pos` succeeded: its logits (after its values).
    fn logits(&mut self, pos: u32, shape: &[usize], data: Slice<'_>);
}

/// How a replay ended.
#[derive(Clone, Debug)]
pub struct ReplayOut {
    /// Positions that succeeded: all of them, or those before the failure.
    pub positions: u32,
    /// The failure at position `positions`, as the CPU executor reports it.
    pub failure: Option<TirError>,
}

/// What ran where, over the replayer's life.
#[derive(Clone, Debug, Default)]
pub struct BatchStats {
    pub replays: u64,
    pub positions: u64,
    pub chunks: u64,
    /// Occurrences run over a whole chunk at once / position by position (per chunk).
    pub batched_occurrences: u64,
    pub per_position_occurrences: u64,
    /// Node evaluations by a device kernel (a batched node counts once per chunk).
    pub device_nodes: u64,
    /// Node evaluations that were views.
    pub views: u64,
    /// Node evaluations run on the CPU executor's kernel, by reason.
    pub fallback: BTreeMap<String, u64>,
    pub dispatches: u64,
    /// Mid-chunk synchronisations (a CPU fallback first finds the earliest failure so far).
    pub syncs: u64,
    /// Host time recording dispatches.
    pub record_ns: u64,
    /// Time waiting for the device: per occurrence, and the chunk's status and lanes.
    pub wait_ns: u64,
    /// Host time reading values back and handing them to the sink.
    pub deliver_ns: u64,
    /// Per occurrence (schedule order): recording plus the device's work, over every chunk.
    pub occurrence_ns: Vec<u64>,
    /// Replays run by [`GpuExecutor`] instead, by reason.
    pub sequential: BTreeMap<String, u64>,
}

/// Does a node's value depend on the position?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Computed from params and consts alone, with no `H`: once per replay, the same at every
    /// position.
    Shared,
    Batched,
}

/// How an occurrence runs over a chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Every node once over the chunk's positions.
    Batched,
    /// Position by position (a `Fixed`-state recurrence, or a value the batched form cannot hold).
    PerPosition,
}

/// A value over a chunk.
#[derive(Clone, Debug)]
enum BV {
    Shared(DevTensor),
    /// `[T] ++ S(H_max)`.
    Batched(DevTensor),
}

impl BV {
    fn tensor(&self) -> &DevTensor {
        match self {
            BV::Shared(t) | BV::Batched(t) => t,
        }
    }
}

/// Everything a replay reads and never writes: the plan, the refined plans and how each
/// occurrence runs, the params and consts on the device.
struct Static<'a> {
    dev: &'a GpuDevice,
    plan: &'a TirPlan,
    occ_plans: Vec<BlockPlan>,
    /// Each node with its axis attributes moved past the position axis.
    shifted: Vec<Vec<NodePlan>>,
    kinds: Vec<Vec<Kind>>,
    modes: Vec<Mode>,
    dparams: BTreeMap<(u16, Option<u16>), DevTensor>,
    consts: Vec<DevTensor>,
}

/// **Replays a program over a job's tokens on a device, a chunk of positions at a time.**
pub struct BatchReplay<'a> {
    s: Static<'a>,
    params: &'a TirParams<'a>,
    /// Why the batched form cannot hold this program at all ([`GpuExecutor`] replays it then).
    whole: Option<String>,
    /// Positions per chunk: a chunk's values are on the device together, so this bounds memory
    /// (an LM head's `i64` products are `vocab × chunk × 8` bytes). Lowered automatically to what
    /// one binding and the status words can address.
    pub chunk: usize,
    pub stats: BatchStats,
}

fn missing(what: impl Into<String>) -> TirError {
    TirError::new(TirErrorKind::Missing, what)
}

fn hp_of(p: usize, w: Option<u32>) -> usize {
    w.map(|w| (p + 1).min(w as usize)).unwrap_or(1)
}

fn at_h(shape: &[Dim], h: usize) -> Vec<usize> {
    shape.iter().map(|d| d.at(h)).collect()
}

fn h_axis(shape: &[Dim]) -> Option<usize> {
    shape.iter().position(|d| d.is_h())
}

/// The node's attributes past a leading position axis.
fn shift_node(node: &NodePlan) -> NodePlan {
    let mut n = node.clone();
    n.prim = match node.prim {
        Prim::Iota { axis, start, step } => Prim::Iota { axis: axis + 1, start, step },
        Prim::Gather { axis, batch_dims } => Prim::Gather { axis: axis + 1, batch_dims: batch_dims + 1 },
        Prim::ReduceSum { axis } => Prim::ReduceSum { axis: axis + 1 },
        Prim::ReduceMax { axis } => Prim::ReduceMax { axis: axis + 1 },
        Prim::TopK { axis, k } => Prim::TopK { axis: axis + 1, k },
        Prim::Concat { axis } => Prim::Concat { axis: axis + 1 },
        ref other => other.clone(),
    };
    n
}

/// Shared nodes: no `H`, no effect, every input a param, a const or a shared node.
fn classify(bp: &BlockPlan) -> Vec<Kind> {
    let mut k: Vec<Kind> = Vec::with_capacity(bp.nodes.len());
    for node in &bp.nodes {
        let shared = !node.out.has_h()
            && !matches!(node.prim, Prim::HistAppend { .. } | Prim::StateWrite { .. })
            && node.inputs.iter().all(|r| match r {
                Ref::Param(_) | Ref::Const(_) => true,
                Ref::Node(j) => k[*j as usize] == Kind::Shared,
                _ => false,
            });
        k.push(if shared { Kind::Shared } else { Kind::Batched });
    }
    k
}

/// Batched unless the occurrence writes a `Fixed` state, or a batched value would pass rank 4, or
/// a `MatMul` has `H` among its batch dimensions (the ragged kernels mask `M`, `N` and `K` only).
fn mode_of(plan: &TirPlan, bp: &BlockPlan, kinds: &[Kind]) -> Mode {
    for (node, kind) in bp.nodes.iter().zip(kinds) {
        if matches!(node.prim, Prim::StateWrite { .. }) {
            return Mode::PerPosition;
        }
        let wide_state =
            node.inputs.iter().any(|r| matches!(r, Ref::State(j) if plan.program.states[*j as usize].shape.len() >= MAX_RANK));
        if wide_state {
            return Mode::PerPosition;
        }
        if *kind == Kind::Shared {
            continue;
        }
        if node.out.rank() >= MAX_RANK || node.in_types.iter().any(|t| t.rank() >= MAX_RANK) {
            return Mode::PerPosition;
        }
        if let Prim::MatMul = node.prim {
            let (a, b) = (&node.in_types[0].shape, &node.in_types[1].shape);
            if a[..a.len() - 2].iter().chain(&b[..b.len() - 2]).any(|d| d.is_h()) {
                return Mode::PerPosition;
            }
        }
    }
    Mode::Batched
}

/// The padding mask a node's kernel needs (`wgsl::RAGGED_MASK`), for an output of batched rank `rb`.
fn ragged_of(node: &NodePlan, rb: usize, w: Option<u32>, p0: usize) -> Option<Ragged> {
    let w = w?;
    let base = MAX_RANK - rb;
    let mut r = Ragged { h_axis: 0, pos_axis: base as u32, window: w, p0: p0 as u32, reduce_mode: 0, mm_bits: 0, pos_slot: 0 };
    let h_out = h_axis(&node.out.shape);
    match &node.prim {
        Prim::MatMul => {
            let (a, b) = (&node.in_types[0].shape, &node.in_types[1].shape);
            let bits = (a[a.len() - 2].is_h() as u8) | ((b[b.len() - 1].is_h() as u8) << 1) | ((a[a.len() - 1].is_h() as u8) << 2);
            if bits == 0 {
                return None;
            }
            r.mm_bits = bits;
            // The output's batch dims right-aligned to two; the position is the first of them.
            r.pos_slot = (MAX_RANK - rb) as u32;
        }
        Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
            if node.in_types[0].shape[*axis as usize].is_h() {
                r.reduce_mode = 1;
            } else {
                r.h_axis = (base + 1 + h_out?) as u32;
                r.reduce_mode = 2;
            }
        }
        Prim::Add
        | Prim::Sub
        | Prim::Mul
        | Prim::Div { .. }
        | Prim::Compare { .. }
        | Prim::Select
        | Prim::Cast
        | Prim::Clamp { .. }
        | Prim::Log2Floor
        | Prim::IntExp
        | Prim::IntRsqrt
        | Prim::IntLn
        | Prim::Gather { .. }
        | Prim::Iota { .. } => r.h_axis = (base + 1 + h_out?) as u32,
        // Copies (Reshape, Concat) and TopK never fail; their padding is never read.
        _ => return None,
    }
    Some(r)
}

/// A batched layout (`[T] ++ S`) with unit dims inserted after the position axis, to `rank`.
fn lift(l: &Layout, rank: usize) -> Layout {
    let r = l.rank as usize;
    let k = rank - r;
    let mut o = Layout { rank: rank as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: l.offset };
    o.shape[0] = l.shape[0];
    o.strides[0] = l.strides[0];
    for i in 1..r {
        o.shape[i + k] = l.shape[i];
        o.strides[i + k] = l.strides[i];
    }
    o
}

/// A shared layout as a batched one: a position axis of length `t` and stride 0.
fn as_batched(l: &Layout, t: usize) -> Layout {
    let r = l.rank as usize;
    let mut o = Layout { rank: (r + 1) as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: l.offset };
    o.shape[0] = t;
    for i in 0..r {
        o.shape[i + 1] = l.shape[i];
        o.strides[i + 1] = l.strides[i];
    }
    o
}

/// Is the per-position part (every axis after the first) of a batched layout contiguous?
fn inner_contiguous(l: &Layout) -> bool {
    let r = l.rank as usize;
    let mut inner = Layout { rank: (r - 1) as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: 0 };
    for i in 1..r {
        inner.shape[i - 1] = l.shape[i];
        inner.strides[i - 1] = l.strides[i];
    }
    inner.is_contiguous()
}

/// Position `p`'s valid elements of one padded block (row-major over `pad`, `H` at `h` padded).
fn valid(block: &[i128], pad: &[usize], h: Option<usize>, hp: usize) -> Vec<i128> {
    let Some(h) = h.filter(|h| pad[*h] != hp) else { return block.to_vec() };
    let inner: usize = pad[h + 1..].iter().product();
    let outer: usize = pad[..h].iter().product();
    let mut out = Vec::with_capacity(outer * hp * inner);
    for o in 0..outer {
        let at = o * pad[h] * inner;
        out.extend_from_slice(&block[at..at + hp * inner]);
    }
    out
}

/// The inverse of [`valid`]: `src` (exact at `hp`) into a padded block.
fn place(dst: &mut [i128], pad: &[usize], h: Option<usize>, hp: usize, src: &[i128]) {
    let Some(h) = h.filter(|h| pad[*h] != hp) else {
        dst.copy_from_slice(src);
        return;
    };
    let inner: usize = pad[h + 1..].iter().product();
    let outer: usize = pad[..h].iter().product();
    for o in 0..outer {
        let at = o * pad[h] * inner;
        dst[at..at + hp * inner].copy_from_slice(&src[o * hp * inner..(o + 1) * hp * inner]);
    }
}

/// A padded element index as the CPU numbers it at `hp`.
fn unpad_index(e: usize, pad: &[usize], h: Option<usize>, hp: usize) -> usize {
    let Some(h) = h else { return e };
    let inner: usize = pad[h + 1..].iter().product();
    let (o, rem) = (e / (pad[h] * inner), e % (pad[h] * inner));
    (o * hp + rem / inner) * inner + rem % inner
}

/// The CPU executor's kernel for one node, on contiguous host operands.
fn cpu_kernel(node: &NodePlan, opds: &[Opd<'_>], out_shape: &[usize], states: &[StateDecl]) -> TirResult<Buf> {
    let mut out = Buf::default();
    let mut scratch = Scratch::default();
    match &node.prim {
        Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
            elementwise::binary(node, &opds[0], &opds[1], out_shape, &mut out, &mut scratch)?
        }
        Prim::Select => elementwise::select(node, &opds[0], &opds[1], &opds[2], out_shape, &mut out, &mut scratch)?,
        Prim::Cast | Prim::Clamp { .. } | Prim::Log2Floor | Prim::IntExp | Prim::IntRsqrt | Prim::IntLn | Prim::StateWrite { .. } => {
            elementwise::unary(node, &opds[0], &mut out, &mut scratch, states)?
        }
        Prim::MatMul => matmul::matmul(node, &opds[0], &opds[1], out_shape, &mut out, &mut scratch)?,
        Prim::ReduceSum { .. } | Prim::ReduceMax { .. } => reduce::reduce(node, &opds[0], &mut out, &mut scratch)?,
        Prim::TopK { .. } => misc::topk(node, &opds[0], &mut out, &mut scratch)?,
        Prim::Gather { .. } => misc::gather(node, &opds[0], &opds[1], &mut out, &mut scratch)?,
        Prim::Concat { .. } => misc::concat(node, opds, out_shape, &mut out)?,
        Prim::Iota { .. } => misc::iota(node, out_shape, &mut out)?,
        Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast => {
            // Views on the CPU executor; here only when a layout cannot be addressed in u32.
            return Err(TirError::new(TirErrorKind::Shape, format!("{} has no CPU kernel here", node.prim.name())));
        }
        Prim::HistAppend { .. } => return Err(TirError::new(TirErrorKind::Shape, "HistAppend belongs to the replay")),
    }
    Ok(out)
}

/// Little-endian 32-bit words.
fn words(bytes: &[u8], n: usize) -> Vec<i32> {
    bytes.chunks_exact(4).take(n).map(|c| i32::from_le_bytes(c.try_into().expect("four bytes"))).collect()
}

/// A host buffer of `dtype` from committed lanes (sign-extended words; `idx` as unsigned).
fn words_buf(dtype: DType, w: &[i32]) -> Buf {
    match dtype {
        DType::I8 => Buf::I8(w.iter().map(|x| *x as i8).collect()),
        DType::I16 => Buf::I16(w.iter().map(|x| *x as i16).collect()),
        DType::I32 => Buf::I32(w.to_vec()),
        DType::Idx => Buf::Idx(w.iter().map(|x| *x as u32).collect()),
        DType::I64 => Buf::I64(w.iter().map(|x| *x as i64).collect()),
        DType::I128 => Buf::I128(w.iter().map(|x| *x as i128).collect()),
    }
}

/// [`valid`] for lanes, appended to `out`.
fn valid_into(block: &[i32], pad: &[usize], h: Option<usize>, hp: usize, out: &mut Vec<i32>) {
    let Some(h) = h.filter(|h| pad[*h] != hp) else {
        out.extend_from_slice(block);
        return;
    };
    let inner: usize = pad[h + 1..].iter().product();
    let outer: usize = pad[..h].iter().product();
    for o in 0..outer {
        let at = o * pad[h] * inner;
        out.extend_from_slice(&block[at..at + hp * inner]);
    }
}

/// A device value on the host: contiguous 32- and 64-bit values straight from their words, any
/// other layout through [`GpuDevice::download`].
fn read_buf(dev: &GpuDevice, t: &DevTensor) -> Buf {
    let n = t.numel();
    if t.layout.is_contiguous() {
        let off = t.layout.offset as u64;
        match t.form {
            Form::S32 | Form::U32 => return words_buf(t.dtype, &words(&dev.read_bytes(&t.buf, 4 * off, 4 * n as u64), n)),
            Form::I64 if t.dtype == DType::I64 => {
                let b = dev.read_bytes(&t.buf, 8 * off, 8 * n as u64);
                return Buf::I64(b.chunks_exact(8).take(n).map(|c| i64::from_le_bytes(c.try_into().expect("eight bytes"))).collect());
            }
            _ => {}
        }
    }
    dev.download(t)
}

/// The run state of a replay, on the device: every `Fixed` instance's value at the start of every
/// position, every `Hist` instance's rows.
struct Rings {
    /// `[positions + 1] ++ shape`, flat.
    fixed: Vec<Option<(DevTensor, Vec<usize>)>>,
    /// `[positions] ++ row`, flat.
    hist: Vec<Option<(DevTensor, Vec<usize>)>>,
    /// Rows of every history store.
    rows: usize,
}

impl Rings {
    /// Zero-initialised (wgpu zeroes every buffer): the initial state. `None` when a store is past
    /// one binding.
    fn new(dev: &GpuDevice, plan: &TirPlan, positions: usize) -> Option<Rings> {
        let max = dev.limits.max_storage_buffer_binding_size;
        let rows = positions.max(1);
        let (mut fixed, mut hist) = (Vec::new(), Vec::new());
        for inst in &plan.instances {
            let form = Form::computed(inst.dtype)?;
            let n = numel(&inst.shape);
            let total = match inst.kind {
                StateKind::Fixed { .. } => (rows + 1) * n,
                StateKind::Hist { .. } => rows * n,
            };
            if form.bytes(total) > max || total >= u32::MAX as usize {
                return None;
            }
            let t = DevTensor { buf: dev.alloc(form, total), form, dtype: inst.dtype, layout: Layout::contiguous(&[total]) };
            match inst.kind {
                StateKind::Fixed { .. } => {
                    fixed.push(Some((t, inst.shape.clone())));
                    hist.push(None);
                }
                StateKind::Hist { .. } => {
                    fixed.push(None);
                    hist.push(Some((t, inst.shape.clone())));
                }
            }
        }
        Some(Rings { fixed, hist, rows })
    }
}

/// How a region-0 status word decodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotMeta {
    Unused,
    /// A shared node: it fails at the replay's first position, if at all.
    Shared,
    /// A batched node: its element `e` is position `e / n` of the chunk.
    Batched,
}

/// A value the sink will be handed, position by position, once the chunk has succeeded.
struct Item {
    slot: u32,
    block: u8,
    layer: Option<u16>,
    node: u16,
    commit: bool,
    dtype: DType,
    shape: Vec<Dim>,
    window: Option<u32>,
    src: Src,
}

enum Src {
    /// Lanes `at ..`: the chunk's `[T] ++ pad`, position-major.
    Lanes { at: usize, pad: Vec<usize> },
    /// Lanes `at ..`: one value, the same at every position.
    SharedLanes { at: usize },
    /// Lanes per position, each exactly the position's shape.
    PerLanes { at: Vec<usize> },
    /// A device value, read back after the chunk (every-node mode).
    Value(BV, Vec<usize>),
    /// One device value per position.
    PerValues(Vec<DevTensor>),
}

/// One chunk's recording and what it will deliver.
struct Chunk<'r, 'a> {
    s: &'r Static<'a>,
    stats: &'r mut BatchStats,
    shared: &'r mut HashMap<(usize, usize), DevTensor>,
    rings: &'r Rings,
    rec: Recorder<'a>,
    p0: usize,
    t: usize,
    every: bool,
    slots: usize,
    per_pos_words: bool,
    tok: DevTensor,
    pos: DevTensor,
    toks: Vec<u32>,
    lanes: Arc<wgpu::Buffer>,
    lanes_at: usize,
    items: Vec<Item>,
    logits: Option<(BV, Vec<usize>, DType)>,
    meta: Vec<SlotMeta>,
    host_fail: Option<(usize, u32, TirError)>,
}

impl<'a> BatchReplay<'a> {
    /// A replayer for one program and one artifact: the plan refined by the actual param ranges
    /// (as [`GpuExecutor::new`]), each occurrence classified, every param instance uploaded.
    pub fn new(dev: &'a GpuDevice, plan: &'a TirPlan, params: &'a TirParams<'a>) -> TirResult<Self> {
        params.check_complete(plan)?;
        let occ_plans = plan.refine(&|j, l| params.range(j, l));
        let max = dev.limits.max_storage_buffer_binding_size as usize;
        let mut whole = None;
        let mut dparams = BTreeMap::new();
        for &(j, layer) in &plan.param_instances {
            let s = params.get(j, layer).ok_or_else(|| missing(format!("param {j} at {layer:?}")))?;
            let decl = &plan.program.params[j as usize];
            let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
            let form = Form::param(decl.dtype).expect("params are never i128");
            if form.bytes(s.len()) as usize <= max && s.len() < u32::MAX as usize {
                dparams.insert((j, layer), dev.upload(s, form, &shape));
            } else {
                whole = Some(format!("param {j} is past one binding"));
            }
        }
        let consts = plan
            .consts
            .iter()
            .zip(&plan.program.consts)
            .map(|(b, c)| {
                let shape: Vec<usize> = c.shape.iter().map(|d| *d as usize).collect();
                dev.upload(b.slice(), Form::computed(b.dtype()).expect("every dtype has a form"), &shape)
            })
            .collect();
        if plan.info.carry.iter().any(|t| t.rank() >= MAX_RANK) {
            whole = Some("a carry of rank 4".into());
        }
        let kinds: Vec<Vec<Kind>> = occ_plans.iter().map(classify).collect();
        let modes = occ_plans.iter().zip(&kinds).map(|(bp, k)| mode_of(plan, bp, k)).collect();
        let shifted = occ_plans.iter().map(|bp| bp.nodes.iter().map(shift_node).collect()).collect();
        Ok(BatchReplay {
            s: Static { dev, plan, occ_plans, shifted, kinds, modes, dparams, consts },
            params,
            whole,
            chunk: 256,
            stats: BatchStats::default(),
        })
    }

    /// How each occurrence runs (schedule order).
    pub fn modes(&self) -> &[Mode] {
        &self.s.modes
    }

    /// **Replay `tokens` from the initial state**: positions `0, 1, …` as `TirExecutor::step` would
    /// run them one after another, stopping at the first that fails. The sink receives every
    /// successful position's values and logits, in order.
    pub fn replay(&mut self, tokens: &[u32], sink: &mut dyn ReplaySink) -> ReplayOut {
        self.stats.replays += 1;
        let p = &self.s.plan.program;
        // The step-level checks (spec 04b §9.1), in the CPU executor's order, bound the replay.
        let mut limit = tokens.len();
        let mut stop = None;
        for (i, tok) in tokens.iter().enumerate() {
            if i as u64 >= p.history_bound as u64 {
                stop = Some(TirError::new(TirErrorKind::Position, format!("position {i} ≥ history_bound {}", p.history_bound)));
                limit = i;
                break;
            }
            if self.s.plan.reads_token && *tok >= p.token_bound {
                stop = Some(TirError::new(TirErrorKind::Operand, format!("token {tok} ≥ token_bound {}", p.token_bound)));
                limit = i;
                break;
            }
        }
        if let Some(why) = self.whole.clone() {
            return self.sequential(tokens, sink, &why);
        }
        let Some(chunk) = self.chunk_for(limit) else {
            return self.sequential(tokens, sink, "a value past one binding at a single position");
        };
        let Some(rings) = Rings::new(self.s.dev, self.s.plan, limit) else {
            return self.sequential(tokens, sink, "a state store past one binding");
        };
        let mut shared = HashMap::new();
        let mut p0 = 0usize;
        while p0 < limit {
            let t = chunk.min(limit - p0);
            match self.run_chunk(&rings, &mut shared, &tokens[p0..p0 + t], p0, sink) {
                Ok(None) => {}
                Ok(Some((pos, e))) => {
                    self.stats.positions += pos as u64;
                    return ReplayOut { positions: pos as u32, failure: Some(e) };
                }
                Err(e) => return ReplayOut { positions: p0 as u32, failure: Some(e) },
            }
            p0 += t;
        }
        self.stats.positions += limit as u64;
        ReplayOut { positions: limit as u32, failure: stop }
    }

    /// The largest chunk (at most [`Self::chunk`]) whose every batched value fits one binding and
    /// whose status words can name each element (`e < 2^30`); `None` if not even one position fits.
    fn chunk_for(&self, limit: usize) -> Option<usize> {
        let s = &self.s;
        let max = s.dev.limits.max_storage_buffer_binding_size as usize;
        let mut need_elems = 1usize;
        let mut need_bytes = 1usize;
        for (occ, bp) in s.occ_plans.iter().enumerate() {
            let h = bp.window.map(|w| limit.min(w as usize)).unwrap_or(1).max(1);
            for (ni, node) in bp.nodes.iter().enumerate() {
                let n = numel(&at_h(&node.out.shape, h)).max(1);
                let bytes = Form::computed(node.store).map(|f| f.bytes(n) as usize).unwrap_or(16 * n);
                if s.kinds[occ][ni] == Kind::Batched && s.modes[occ] == Mode::Batched {
                    need_elems = need_elems.max(n);
                    need_bytes = need_bytes.max(bytes);
                } else if bytes > max {
                    return None;
                }
            }
        }
        let by_status = ((1usize << 30) - 1) / need_elems;
        let by_bytes = max / need_bytes;
        let lanes_ok = |c: usize| s.lanes(0, c).max(s.lanes(limit.saturating_sub(c), c)) * 4 <= max;
        let mut c = self.chunk.max(1).min(by_status).min(by_bytes).min(limit.max(1));
        while c > 1 && !lanes_ok(c) {
            c /= 2;
        }
        (c >= 1 && need_elems < (1 << 30) && need_bytes <= max && lanes_ok(c)).then_some(c)
    }

    /// The replay by [`GpuExecutor`], position by position.
    fn sequential(&mut self, tokens: &[u32], sink: &mut dyn ReplaySink, why: &str) -> ReplayOut {
        *self.stats.sequential.entry(why.to_string()).or_default() += 1;
        let mut ex = match GpuExecutor::new(self.s.dev, self.s.plan, self.params) {
            Ok(e) => e,
            Err(e) => return ReplayOut { positions: 0, failure: Some(e) },
        };
        struct Fwd<'s> {
            sink: &'s mut dyn ReplaySink,
            pos: u32,
            every: bool,
        }
        impl StepSink for Fwd<'_> {
            fn every_node(&self) -> bool {
                self.every
            }
            fn node(&mut self, v: &NodeValue<'_>) {
                self.sink.node(self.pos, v);
            }
        }
        let every = sink.every_node();
        for (i, tok) in tokens.iter().enumerate() {
            let mut f = Fwd { sink: &mut *sink, pos: i as u32, every };
            if let Err(e) = ex.step(*tok, &mut f) {
                self.stats.positions += i as u64;
                return ReplayOut { positions: i as u32, failure: Some(e) };
            }
            let (shape, l) = ex.logits();
            sink.logits(i as u32, shape, l.slice());
        }
        self.stats.positions += tokens.len() as u64;
        ReplayOut { positions: tokens.len() as u32, failure: None }
    }

    /// One chunk: every occurrence over positions `p0 .. p0 + toks.len()`, then the status words,
    /// the failure (if any) and the values of the positions before it. Returns the failure.
    fn run_chunk(
        &mut self,
        rings: &Rings,
        shared: &mut HashMap<(usize, usize), DevTensor>,
        toks: &[u32],
        p0: usize,
        sink: &mut dyn ReplaySink,
    ) -> TirResult<Option<(usize, TirError)>> {
        let s = &self.s;
        let dev = s.dev;
        let plan = s.plan;
        let t = toks.len();
        let slots = plan.slots_per_position() as usize;
        let per_pos_words = s.modes.contains(&Mode::PerPosition);
        let n_words = if per_pos_words { slots * (1 + t) } else { slots };
        let every = sink.every_node();
        let lanes_n = if every { 0 } else { s.lanes(p0, t) };
        let positions: Vec<u32> = (p0..p0 + t).map(|p| p as u32).collect();
        let mut c = Chunk {
            s,
            stats: &mut self.stats,
            shared,
            rings,
            rec: Recorder::new(dev, n_words as u32),
            p0,
            t,
            every,
            slots,
            per_pos_words,
            tok: dev.upload(Slice::Idx(toks), Form::U32, &[t]),
            pos: dev.upload(Slice::Idx(&positions), Form::U32, &[t]),
            toks: toks.to_vec(),
            lanes: dev.alloc(Form::S32, lanes_n.max(1)),
            lanes_at: 0,
            items: Vec::new(),
            logits: None,
            meta: vec![SlotMeta::Unused; slots],
            host_fail: None,
        };
        c.stats.chunks += 1;
        let mut carry: Vec<BV> = Vec::new();
        for occ in 0..plan.occurrences.len() {
            let t_rec = Instant::now();
            carry = match s.modes[occ] {
                Mode::Batched => {
                    c.stats.batched_occurrences += 1;
                    c.occurrence_batched(occ, &carry)?
                }
                Mode::PerPosition => {
                    c.stats.per_position_occurrences += 1;
                    c.occurrence_per_position(occ, &carry)?
                }
            };
            c.stats.record_ns += t_rec.elapsed().as_nanos() as u64;
            // One occurrence a submission, waited for: the chunk's peak memory is about one
            // occurrence's values, not the whole program's.
            let t_wait = Instant::now();
            c.rec.flush();
            dev.wait();
            c.stats.wait_ns += t_wait.elapsed().as_nanos() as u64;
            if c.stats.occurrence_ns.len() <= occ {
                c.stats.occurrence_ns.resize(occ + 1, 0);
            }
            c.stats.occurrence_ns[occ] += t_rec.elapsed().as_nanos() as u64;
        }
        c.stats.dispatches += c.rec.dispatches as u64;
        let t_wait = Instant::now();
        let status = c.read_status();
        let fail = c.resolve(&status);
        let lanes: Vec<i32> =
            if c.lanes_at > 0 { words(&dev.read_bytes(&c.lanes, 0, 4 * c.lanes_at as u64), c.lanes_at) } else { Vec::new() };
        c.stats.wait_ns += t_wait.elapsed().as_nanos() as u64;
        let end = fail.as_ref().map(|f| f.0).unwrap_or(p0 + t).min(p0 + t);
        let t_del = Instant::now();
        c.deliver(sink, &lanes, end);
        c.stats.deliver_ns += t_del.elapsed().as_nanos() as u64;
        Ok(fail.map(|(p, _, e)| (p, e)))
    }
}

impl Static<'_> {
    /// The occurrence and node of a slot.
    fn node_of(&self, slot: usize) -> (usize, usize) {
        let occ = self.plan.slot_bases.partition_point(|b| *b as usize <= slot) - 1;
        (occ, slot - self.plan.slot_bases[occ] as usize)
    }

    /// Lanes a chunk stages for its commit points.
    fn lanes(&self, p0: usize, t: usize) -> usize {
        let mut total = 0usize;
        for (occ, bp) in self.occ_plans.iter().enumerate() {
            let hmax = bp.window.map(|w| (p0 + t).min(w as usize)).unwrap_or(1);
            for (ni, node) in bp.nodes.iter().enumerate() {
                if !node.commit {
                    continue;
                }
                let n_at = |h: usize| numel(&at_h(&node.out.shape, h));
                total += match (self.kinds[occ][ni], self.modes[occ]) {
                    (Kind::Shared, _) => n_at(1),
                    (Kind::Batched, Mode::Batched) => t * n_at(hmax),
                    (Kind::Batched, Mode::PerPosition) => (p0..p0 + t).map(|p| n_at(hp_of(p, bp.window))).sum(),
                };
            }
        }
        total
    }

    fn const_scalar(&self, j: u16) -> Option<i128> {
        let b = &self.plan.consts[j as usize];
        (b.len() == 1).then(|| b.to_i128s()[0])
    }
}

impl<'a> Chunk<'_, 'a> {
    fn hmax(&self, w: Option<u32>) -> usize {
        w.map(|w| (self.p0 + self.t).min(w as usize)).unwrap_or(1)
    }

    fn param(&self, j: u16, layer: Option<u16>) -> TirResult<DevTensor> {
        let l = if self.s.plan.program.params[j as usize].per_layer { layer } else { None };
        self.s.dparams.get(&(j, l)).cloned().ok_or_else(|| missing(format!("param {j} at {l:?}")))
    }

    fn zeros(&self, dtype: DType, shape: &[usize]) -> DevTensor {
        let form = Form::computed(dtype).expect("every dtype has a form");
        DevTensor { buf: self.s.dev.alloc(form, numel(shape)), form, dtype, layout: Layout::contiguous(shape) }
    }

    /// A batched tensor of `v` (a shared one with a stride-0 position axis).
    fn batched_of(&self, v: &BV) -> DevTensor {
        match v {
            BV::Batched(x) => x.clone(),
            BV::Shared(x) => x.view(as_batched(&x.layout, self.t)),
        }
    }

    fn fail_host(&mut self, pos: usize, slot: u32, e: TirError) {
        if self.host_fail.as_ref().is_none_or(|(p, s, _)| (pos, slot) < (*p, *s)) {
            self.host_fail = Some((pos, slot, e));
        }
    }

    fn read_status(&mut self) -> Vec<u32> {
        self.rec.flush();
        let n = self.rec.slots as usize;
        unpack(&self.s.dev.read_bytes(&self.rec.status, 0, 4 * n as u64), Form::U32, n).iter().map(|w| *w as u32).collect()
    }

    /// The first failure so far: `(position, slot, error)`, least first.
    fn resolve(&self, words: &[u32]) -> Option<(usize, u32, TirError)> {
        let s = self.s;
        let mut best = self.host_fail.clone();
        let mut consider = |p: usize, slot: u32, e: TirError| {
            if best.as_ref().is_none_or(|(bp, bs, _)| (p, slot) < (*bp, *bs)) {
                best = Some((p, slot, e));
            }
        };
        for (slot, word) in words.iter().enumerate().take(self.slots) {
            let Some(f) = DeviceFailure::decode(*word) else { continue };
            let (occ, ni) = s.node_of(slot);
            let bp = &s.occ_plans[occ];
            let node = &bp.nodes[ni];
            let name = node.prim.name();
            match self.meta[slot] {
                SlotMeta::Shared => consider(self.p0, slot as u32, f.to_error(&format!("{name} (device)"))),
                SlotMeta::Batched => {
                    let pad = at_h(&node.out.shape, self.hmax(bp.window));
                    let n = numel(&pad).max(1);
                    let e = f.element as usize;
                    let p = self.p0 + e / n;
                    let el = unpad_index(e % n, &pad, h_axis(&node.out.shape), hp_of(p, bp.window));
                    consider(
                        p,
                        slot as u32,
                        TirError::new(f.kind, format!("{name} (device, batched): position {p}, element {el} failed")),
                    );
                }
                SlotMeta::Unused => consider(self.p0, slot as u32, missing(format!("slot {slot}: a status word no node owns"))),
            }
        }
        if self.per_pos_words {
            for pr in 0..self.t {
                for slot in 0..self.slots {
                    if let Some(f) = DeviceFailure::decode(words[(1 + pr) * self.slots + slot]) {
                        let (occ, ni) = s.node_of(slot);
                        let name = s.occ_plans[occ].nodes[ni].prim.name();
                        consider(self.p0 + pr, slot as u32, f.to_error(&format!("{name} (device)")));
                    }
                }
            }
        }
        best
    }

    /// The first failing position so far (every recorded dispatch finished): a synchronisation.
    fn first_failure_pos(&mut self) -> usize {
        self.stats.syncs += 1;
        let words = self.read_status();
        self.resolve(&words).map(|f| f.0).unwrap_or(usize::MAX)
    }

    fn item(&self, occ: usize, ni: usize, src: Src) -> Item {
        let s = self.s;
        let (block, layer) = s.plan.occurrences[occ];
        let bp = &s.occ_plans[occ];
        let node = &bp.nodes[ni];
        Item {
            slot: s.plan.slot_bases[occ] + ni as u32,
            block,
            layer,
            node: ni as u16,
            commit: node.commit,
            dtype: node.out.dtype,
            shape: node.out.shape.clone(),
            window: bp.window,
            src,
        }
    }

    /// Copy `x` (layout `xl` over `geometry`) into the lanes at the current end.
    fn stage_lanes(&mut self, geometry: &[usize], x: &DevTensor, xl: Layout, slot: u32) -> TirResult<usize> {
        let at = self.lanes_at;
        let region = Layout::contiguous_at(geometry, at);
        let lanes = Arc::clone(&self.lanes);
        self.rec
            .ew(EwOp::Copy, geometry, (&lanes, Form::S32, &region), &[(x, xl)], &[], &[], None, false, slot)
            .map_err(|u| TirError::new(TirErrorKind::Shape, format!("staging a commit: {u:?}")))?;
        self.lanes_at += numel(geometry);
        Ok(at)
    }

    /// `x`, or a copy of it when it lives in `buf`: a dispatch may not read the buffer it writes (a
    /// `StateWrite` of a view of its own state, a row taken from its own history store).
    fn unalias(&mut self, x: &DevTensor, buf: &Arc<wgpu::Buffer>, status: u32) -> TirResult<DevTensor> {
        if !Arc::ptr_eq(&x.buf, buf) {
            return Ok(x.clone());
        }
        self.rec
            .materialize(x, Form::computed(x.dtype).expect("every dtype has a form"), status)
            .map_err(|u| TirError::new(TirErrorKind::Shape, format!("a copy out of a store: {u:?}")))
    }

    // ------------------------------------------------------------ an occurrence over the chunk

    fn occurrence_batched(&mut self, occ: usize, carry: &[BV]) -> TirResult<Vec<BV>> {
        let s = self.s;
        let plan = s.plan;
        let (_, layer) = plan.occurrences[occ];
        let bp = &s.occ_plans[occ];
        let base = plan.slot_bases[occ] as usize;
        let hmax = self.hmax(bp.window);
        let mut vals: Vec<BV> = Vec::with_capacity(bp.nodes.len());
        for (ni, node) in bp.nodes.iter().enumerate() {
            let slot = (base + ni) as u32;
            let v = if s.kinds[occ][ni] == Kind::Shared {
                let lookup = |j: usize| -> Option<DevTensor> {
                    match vals.get(j) {
                        Some(BV::Shared(t)) => Some(t.clone()),
                        _ => None,
                    }
                };
                BV::Shared(self.shared_node(occ, ni, &lookup, layer)?)
            } else {
                self.meta[slot as usize] = SlotMeta::Batched;
                self.batched_node(occ, ni, &vals, carry, layer, hmax)?
            };
            if node.commit || self.every {
                let pad = at_h(&node.out.shape, hmax);
                let src = if self.every {
                    Src::Value(v.clone(), pad)
                } else {
                    match &v {
                        BV::Shared(x) => Src::SharedLanes { at: self.stage_lanes(&pad, x, x.layout, slot)? },
                        BV::Batched(x) => {
                            let geo: Vec<usize> = std::iter::once(self.t).chain(pad.iter().copied()).collect();
                            Src::Lanes { at: self.stage_lanes(&geo, x, x.layout, slot)?, pad }
                        }
                    }
                };
                let it = self.item(occ, ni, src);
                self.items.push(it);
            }
            vals.push(v);
        }
        Ok(self.finish_occurrence(occ, &vals))
    }

    fn finish_occurrence(&mut self, occ: usize, vals: &[BV]) -> Vec<BV> {
        let plan = self.s.plan;
        let bp = &self.s.occ_plans[occ];
        if occ + 1 == plan.occurrences.len() {
            let li = plan.program.logits as usize;
            let node = &bp.nodes[li];
            self.logits = Some((vals[li].clone(), at_h(&node.out.shape, 1), node.out.dtype));
            Vec::new()
        } else {
            bp.carry_out.iter().map(|c| vals[*c as usize].clone()).collect()
        }
    }

    /// A shared node: computed once per replay (its status word is the first position's).
    fn shared_node(
        &mut self,
        occ: usize,
        ni: usize,
        lookup: &dyn Fn(usize) -> Option<DevTensor>,
        layer: Option<u16>,
    ) -> TirResult<DevTensor> {
        if let Some(t) = self.shared.get(&(occ, ni)) {
            return Ok(t.clone());
        }
        let s = self.s;
        let node = &s.occ_plans[occ].nodes[ni];
        let slot = s.plan.slot_bases[occ] + ni as u32;
        self.meta[slot as usize] = SlotMeta::Shared;
        let ins: Vec<DevTensor> = node
            .inputs
            .iter()
            .map(|r| match *r {
                Ref::Node(j) => lookup(j as usize).ok_or_else(|| missing("a shared node's input")),
                Ref::Param(j) => self.param(j, layer),
                Ref::Const(j) => Ok(s.consts[j as usize].clone()),
                _ => Err(missing("a shared node's input")),
            })
            .collect::<TirResult<_>>()?;
        let out = at_h(&node.out.shape, 1);
        let host = |r: &Ref| match *r {
            Ref::Const(j) => s.const_scalar(j),
            _ => None,
        };
        let v = self.plain(node, &ins, &out, slot, slot, self.p0, &host)?;
        self.shared.insert((occ, ni), v.clone());
        Ok(v)
    }

    fn ref_bv(&self, r: Ref, vals: &[BV], carry: &[BV], layer: Option<u16>) -> TirResult<BV> {
        let plan = self.s.plan;
        Ok(match r {
            Ref::Node(j) => vals.get(j as usize).cloned().ok_or_else(|| missing(format!("node {j}")))?,
            Ref::CarryIn(k) => carry.get(k as usize).cloned().ok_or_else(|| missing(format!("carry-in {k}")))?,
            Ref::Param(j) => BV::Shared(self.param(j, layer)?),
            Ref::Const(j) => BV::Shared(self.s.consts[j as usize].clone()),
            Ref::State(j) => {
                let inst = plan.instance(j, layer).ok_or_else(|| missing(format!("state {j} at {layer:?}")))?;
                let (ring, shape) = self.rings.fixed[inst as usize].as_ref().ok_or_else(|| missing("a state store"))?;
                let n = numel(shape);
                let geo: Vec<usize> = std::iter::once(self.t).chain(shape.iter().copied()).collect();
                BV::Batched(ring.view(Layout::contiguous_at(&geo, self.p0 * n)))
            }
            Ref::Input(k) => BV::Batched(if k == 0 { self.tok.clone() } else { self.pos.clone() }),
        })
    }

    /// **One node over the chunk's positions**: a view, a projection folded into one product, the
    /// history window, or a kernel over `[T] ++ S` with the padding masked.
    fn batched_node(&mut self, occ: usize, ni: usize, vals: &[BV], carry: &[BV], layer: Option<u16>, hmax: usize) -> TirResult<BV> {
        let s = self.s;
        let bp = &s.occ_plans[occ];
        let node = &bp.nodes[ni];
        let slot = s.plan.slot_bases[occ] + ni as u32;
        let t = self.t;
        let w = bp.window;
        let out_pp = at_h(&node.out.shape, hmax);
        let out_b: Vec<usize> = std::iter::once(t).chain(out_pp.iter().copied()).collect();
        let rb = out_b.len();
        let ins: Vec<BV> = node.inputs.iter().map(|r| self.ref_bv(*r, vals, carry, layer)).collect::<TirResult<_>>()?;
        match &node.prim {
            Prim::Reshape => {
                let x = self.batched_of(&ins[0]);
                if inner_contiguous(&x.layout) {
                    self.stats.views += 1;
                    let mut l = Layout::contiguous_at(&out_b, x.layout.offset);
                    l.strides[0] = x.layout.strides[0];
                    return Ok(BV::Batched(x.view(l)));
                }
            }
            Prim::Transpose { perm } => {
                let x = self.batched_of(&ins[0]);
                let p: Vec<u8> = std::iter::once(0).chain(perm.iter().map(|q| q + 1)).collect();
                self.stats.views += 1;
                return Ok(BV::Batched(x.view(x.layout.transposed(&p))));
            }
            Prim::Slice { axis, start } => {
                let x = self.batched_of(&ins[0]);
                let a = *axis as usize + 1;
                self.stats.views += 1;
                return Ok(BV::Batched(x.view(x.layout.sliced(a, *start as usize, out_b[a]))));
            }
            Prim::Broadcast => {
                let l = match &ins[0] {
                    BV::Batched(x) => lift(&x.layout, rb).broadcast_to(&out_b),
                    BV::Shared(x) => as_batched(&x.layout.broadcast_to(&out_pp), t),
                };
                self.stats.views += 1;
                return Ok(BV::Batched(ins[0].tensor().view(l)));
            }
            Prim::Gather { axis, batch_dims: 0 } if node.in_types[1].rank() == 0 && matches!(node.inputs[1], Ref::Const(_)) => {
                // One row along the axis by a const index: a view, as on the executor.
                let Ref::Const(j) = node.inputs[1] else { unreachable!() };
                if let Some(i) = s.const_scalar(j) {
                    let x = self.batched_of(&ins[0]);
                    let a = *axis as usize + 1;
                    if i < 0 || i >= x.layout.shape[a] as i128 {
                        let e = TirError::new(TirErrorKind::Index, format!("Gather: index {i} outside [0, {})", x.layout.shape[a]));
                        self.fail_host(self.p0, slot, e);
                        return Ok(BV::Batched(self.zeros(node.store, &out_b)));
                    }
                    let mut l = x.layout;
                    l.offset += i as usize * l.strides[a];
                    self.stats.views += 1;
                    return Ok(BV::Batched(x.view(l.without_axis(a))));
                }
            }
            Prim::Cast | Prim::Clamp { .. }
                if node.identity
                    && matches!(&ins[0], BV::Batched(x) if Some(x.form) == Form::computed(node.store) && x.dtype == node.store) =>
            {
                self.stats.views += 1;
                return Ok(ins[0].clone());
            }
            Prim::HistAppend { state } => return self.hist_append_batched(node, *state, layer, &ins[0], slot, w, hmax),
            Prim::StateWrite { .. } => return Err(TirError::new(TirErrorKind::Shape, "a batched occurrence writes no state")),
            _ => {}
        }
        if let Prim::MatMul = node.prim
            && let Some(v) = self.fold(node, &ins, slot)
        {
            return Ok(v);
        }
        let dev_ins: Vec<DevTensor> = match &node.prim {
            Prim::Gather { .. }
            | Prim::Concat { .. }
            | Prim::ReduceSum { .. }
            | Prim::ReduceMax { .. }
            | Prim::TopK { .. }
            | Prim::Reshape => ins.iter().map(|v| self.batched_of(v)).collect(),
            _ => ins
                .iter()
                .map(|v| match v {
                    BV::Batched(x) => x.view(lift(&x.layout, rb)),
                    BV::Shared(x) => x.clone(),
                })
                .collect(),
        };
        self.rec.ragged = ragged_of(node, rb, w, self.p0);
        let refs: Vec<&DevTensor> = dev_ins.iter().collect();
        let r = self.rec.node(&s.shifted[occ][ni], &refs, &out_b, slot);
        self.rec.ragged = None;
        match r {
            Ok(o) => {
                self.stats.device_nodes += 1;
                Ok(BV::Batched(o))
            }
            Err(u) => self.cpu_batched(node, &ins, slot, hmax, w, u),
        }
    }

    /// **A projection over the chunk as one product.** `W[M, K]·x[K, 1]` at every position is
    /// `W·X` with `X[K, T]` a view of the batched `x`: one GEMM, whose `[M, T]` result is viewed as
    /// `[T, M, 1]`. Its elements are not position-major, so this is taken only where the plan
    /// proves the sum cannot fail (`Fast`). `x[1, K]·W[K, N]` is `X[T, K]·W`, position-major
    /// whatever the plan.
    fn fold(&mut self, node: &NodePlan, ins: &[BV], slot: u32) -> Option<BV> {
        let (ta, tb) = (&node.in_types[0], &node.in_types[1]);
        if ta.rank() != 2 || tb.rank() != 2 || ta.has_h() || tb.has_h() {
            return None;
        }
        let t = self.t;
        match (&ins[0], &ins[1]) {
            (BV::Shared(a), BV::Batched(b)) if b.shape()[2] == 1 && matches!(node.acc, Acc::Fast64 | Acc::Fast128) => {
                let (m, k) = (a.shape()[0], a.shape()[1]);
                let bl = b.layout;
                let x =
                    b.view(Layout { rank: 2, shape: [k, t, 1, 1], strides: [bl.strides[1], bl.strides[0], 0, 0], offset: bl.offset });
                let o = self.rec.node(node, &[a, &x], &[m, t], slot).ok()?;
                self.stats.device_nodes += 1;
                Some(BV::Batched(o.view(Layout { rank: 3, shape: [t, m, 1, 1], strides: [1, t, 1, 0], offset: 0 })))
            }
            (BV::Batched(a), BV::Shared(b)) if a.shape()[1] == 1 => {
                let (k, n) = (b.shape()[0], b.shape()[1]);
                let al = a.layout;
                let x =
                    a.view(Layout { rank: 2, shape: [t, k, 1, 1], strides: [al.strides[0], al.strides[2], 0, 0], offset: al.offset });
                let o = self.rec.node(node, &[&x, b], &[t, n], slot).ok()?;
                self.stats.device_nodes += 1;
                Some(BV::Batched(o.view(Layout::contiguous(&[t, 1, n]))))
            }
            _ => None,
        }
    }

    /// **`HistAppend` over the chunk**: the rows into the store, and every position's window as
    /// one view of it (or one gather, in the chunk where the windows start to slide).
    #[allow(clippy::too_many_arguments)]
    fn hist_append_batched(
        &mut self,
        node: &NodePlan,
        state: u16,
        layer: Option<u16>,
        row: &BV,
        slot: u32,
        w: Option<u32>,
        hmax: usize,
    ) -> TirResult<BV> {
        let plan = self.s.plan;
        let inst = plan.instance(state, layer).ok_or_else(|| missing("history instance"))? as usize;
        let (ring, row_shape) = self.rings.hist[inst].clone().ok_or_else(|| missing("a history store"))?;
        let (t, p0) = (self.t, self.p0);
        let rn = numel(&row_shape);
        let geo: Vec<usize> = std::iter::once(t).chain(row_shape.iter().copied()).collect();
        let src = self.batched_of(row);
        let src = self.unalias(&src, &ring.buf, slot)?;
        let region = Layout::contiguous_at(&geo, p0 * rn);
        self.rec
            .ew(EwOp::Copy, &geo, (&ring.buf, ring.form, &region), &[(&src, src.layout)], &[], &[], None, false, slot)
            .map_err(|u| TirError::new(TirErrorKind::Shape, format!("history rows: {u:?}")))?;
        self.stats.device_nodes += 1;
        let w = w.ok_or_else(|| missing("a window"))? as usize;
        let rr = row_shape.len();
        let rs = row_major(&row_shape);
        let mut l = Layout { rank: (2 + rr) as u8, shape: [1; MAX_RANK], strides: [0; MAX_RANK], offset: 0 };
        l.shape[0] = t;
        l.shape[1] = hmax;
        l.strides[1] = rn;
        l.shape[2..2 + rr].copy_from_slice(&row_shape);
        l.strides[2..2 + rr].copy_from_slice(&rs[..rr]);
        if p0 + t <= w {
            // Every position sees every row from the first: stride 0 along the positions.
            return Ok(BV::Batched(ring.view(l)));
        }
        if p0 + 1 >= w {
            // Every window is full: position p's starts at row p + 1 − W.
            l.strides[0] = rn;
            l.offset = (p0 + 1 - w) * rn;
            return Ok(BV::Batched(ring.view(l)));
        }
        // The windows start to slide inside this chunk: the rows by index (padding repeats a row
        // of its own window, so every index is in range).
        let idx: Vec<u32> = (0..t)
            .flat_map(|pr| {
                let p = p0 + pr;
                let start = p + 1 - (p + 1).min(w);
                (0..hmax).map(move |h| (start + h).min(p) as u32)
            })
            .collect();
        let it = self.s.dev.upload(Slice::Idx(&idx), Form::U32, &[t, hmax]);
        let all: Vec<usize> = std::iter::once(self.rings.rows).chain(row_shape.iter().copied()).collect();
        let data = ring.view(Layout::contiguous(&all));
        let g = NodePlan::for_kernel(Prim::Gather { axis: 0, batch_dims: 0 }, node.out.dtype, Acc::Fast64);
        let out: Vec<usize> = [t, hmax].into_iter().chain(row_shape.iter().copied()).collect();
        let o = self
            .rec
            .gather(&g, &data, &it, 0, 0, &out, slot)
            .map_err(|u| TirError::new(TirErrorKind::Shape, format!("history window: {u:?}")))?;
        Ok(BV::Batched(o))
    }

    /// A node the device does not run, on the CPU executor's kernel position by position — only
    /// for the positions before the first failure so far (their operands are the CPU's).
    fn cpu_batched(&mut self, node: &NodePlan, ins: &[BV], slot: u32, hmax: usize, w: Option<u32>, why: Unsupported) -> TirResult<BV> {
        *self.stats.fallback.entry(format!("{}: {why:?}", node.prim.name())).or_default() += 1;
        let first = self.first_failure_pos();
        let s = self.s;
        let dev = s.dev;
        let t = self.t;
        let host: Vec<(bool, Vec<i128>, DType)> = ins
            .iter()
            .map(|v| match v {
                BV::Batched(x) => (true, dev.download(x).to_i128s(), x.dtype),
                BV::Shared(x) => (false, dev.download(x).to_i128s(), x.dtype),
            })
            .collect();
        let out_pad = at_h(&node.out.shape, hmax);
        let n_out = numel(&out_pad);
        let h_out = h_axis(&node.out.shape);
        let mut out = vec![0i128; t * n_out];
        let states = &s.plan.program.states;
        for pr in 0..t {
            let p = self.p0 + pr;
            if p >= first {
                break;
            }
            let hp = hp_of(p, w);
            let mut bufs = Vec::with_capacity(ins.len());
            let mut shapes = Vec::with_capacity(ins.len());
            for (k, (batched, data, dt)) in host.iter().enumerate() {
                let ty = &node.in_types[k].shape;
                let vals = if *batched {
                    let pad = at_h(ty, hmax);
                    let n = numel(&pad);
                    valid(&data[pr * n..(pr + 1) * n], &pad, h_axis(ty), hp)
                } else {
                    data.clone()
                };
                bufs.push(Buf::from_i128s(*dt, &vals));
                shapes.push(at_h(ty, hp));
            }
            let opds: Vec<Opd<'_>> =
                bufs.iter().zip(&shapes).map(|(b, sh)| Opd { data: b.slice(), layout: Layout::contiguous(sh) }).collect();
            match cpu_kernel(node, &opds, &at_h(&node.out.shape, hp), states) {
                Ok(b) => place(&mut out[pr * n_out..(pr + 1) * n_out], &out_pad, h_out, hp, &b.to_i128s()),
                Err(e) => {
                    self.fail_host(p, slot, e);
                    break;
                }
            }
        }
        let form = Form::computed(node.store).expect("every dtype has a form");
        let shape: Vec<usize> = std::iter::once(t).chain(out_pad).collect();
        Ok(BV::Batched(dev.upload(Buf::from_i128s(node.store, &out).slice(), form, &shape)))
    }

    // ------------------------------------------------------------ position by position

    /// **One node at one position's exact shapes** (a shared node, or a node of a per-position
    /// occurrence): a view where the executor makes one, the kernel, or the CPU kernel.
    #[allow(clippy::too_many_arguments)]
    fn plain(
        &mut self,
        node: &NodePlan,
        ins: &[DevTensor],
        out: &[usize],
        status: u32,
        slot: u32,
        pos: usize,
        host_scalar: &dyn Fn(&Ref) -> Option<i128>,
    ) -> TirResult<DevTensor> {
        match &node.prim {
            Prim::Reshape if ins[0].layout.is_contiguous() => {
                self.stats.views += 1;
                return Ok(ins[0].view(ins[0].layout.reshaped(out)));
            }
            Prim::Transpose { perm } => {
                self.stats.views += 1;
                return Ok(ins[0].view(ins[0].layout.transposed(perm)));
            }
            Prim::Slice { axis, start } => {
                self.stats.views += 1;
                let a = *axis as usize;
                return Ok(ins[0].view(ins[0].layout.sliced(a, *start as usize, out[a])));
            }
            Prim::Broadcast => {
                self.stats.views += 1;
                return Ok(ins[0].view(ins[0].layout.broadcast_to(out)));
            }
            Prim::Gather { axis, batch_dims: 0 } if ins[1].layout.rank == 0 && host_scalar(&node.inputs[1]).is_some() => {
                let i = host_scalar(&node.inputs[1]).expect("checked");
                let a = *axis as usize;
                let l = ins[0].layout;
                if i < 0 || i >= l.shape[a] as i128 {
                    self.fail_host(
                        pos,
                        slot,
                        TirError::new(TirErrorKind::Index, format!("Gather: index {i} outside [0, {})", l.shape[a])),
                    );
                    return Ok(self.zeros(node.store, out));
                }
                let mut v = l;
                v.offset += i as usize * l.strides[a];
                self.stats.views += 1;
                return Ok(ins[0].view(v.without_axis(a)));
            }
            Prim::Cast | Prim::Clamp { .. }
                if node.identity && Some(ins[0].form) == Form::computed(node.store) && ins[0].dtype == node.store =>
            {
                self.stats.views += 1;
                return Ok(ins[0].clone());
            }
            _ => {}
        }
        let refs: Vec<&DevTensor> = ins.iter().collect();
        match self.rec.node(node, &refs, out, status) {
            Ok(t) => {
                self.stats.device_nodes += 1;
                Ok(t)
            }
            Err(why) => {
                *self.stats.fallback.entry(format!("{}: {why:?}", node.prim.name())).or_default() += 1;
                if pos >= self.first_failure_pos() {
                    return Ok(self.zeros(node.store, out));
                }
                let dev = self.s.dev;
                let host: Vec<Buf> = ins.iter().map(|t| dev.download(t)).collect();
                let opds: Vec<Opd<'_>> =
                    host.iter().zip(ins).map(|(b, t)| Opd { data: b.slice(), layout: Layout::contiguous(t.shape()) }).collect();
                match cpu_kernel(node, &opds, out, &self.s.plan.program.states) {
                    Ok(b) => Ok(dev.upload(b.slice(), Form::computed(b.dtype()).expect("every dtype has a form"), out)),
                    Err(e) => {
                        self.fail_host(pos, slot, e);
                        Ok(self.zeros(node.store, out))
                    }
                }
            }
        }
    }

    fn ref_pp(&self, r: Ref, vals: &[Option<DevTensor>], carry: &[BV], layer: Option<u16>, pr: usize) -> TirResult<DevTensor> {
        let plan = self.s.plan;
        Ok(match r {
            Ref::Node(j) => vals.get(j as usize).cloned().flatten().ok_or_else(|| missing(format!("node {j}")))?,
            Ref::CarryIn(k) => match carry.get(k as usize).ok_or_else(|| missing(format!("carry-in {k}")))? {
                BV::Batched(x) => x.view(x.layout.sliced(0, pr, 1).without_axis(0)),
                BV::Shared(x) => x.clone(),
            },
            Ref::Param(j) => self.param(j, layer)?,
            Ref::Const(j) => self.s.consts[j as usize].clone(),
            Ref::State(j) => {
                let inst = plan.instance(j, layer).ok_or_else(|| missing(format!("state {j} at {layer:?}")))?;
                let (ring, shape) = self.rings.fixed[inst as usize].as_ref().ok_or_else(|| missing("a state store"))?;
                ring.view(Layout::contiguous_at(shape, (self.p0 + pr) * numel(shape)))
            }
            Ref::Input(k) => (if k == 0 { &self.tok } else { &self.pos }).view(Layout::contiguous_at(&[], pr)),
        })
    }

    /// **An occurrence position by position** (a `Fixed`-state recurrence): each position reads the
    /// state rows the previous one wrote; carry-outs (or the logits) are stacked into `[T] ++ S`.
    fn occurrence_per_position(&mut self, occ: usize, carry: &[BV]) -> TirResult<Vec<BV>> {
        let s = self.s;
        let plan = s.plan;
        let (_, layer) = plan.occurrences[occ];
        let bp = &s.occ_plans[occ];
        let base = plan.slot_bases[occ] as usize;
        let w = bp.window;
        let n = bp.nodes.len();
        let t = self.t;
        let last = occ + 1 == plan.occurrences.len();
        let outs: Vec<usize> =
            if last { vec![plan.program.logits as usize] } else { bp.carry_out.iter().map(|c| *c as usize).collect() };
        let stacks: Vec<DevTensor> = outs
            .iter()
            .map(|c| {
                let nd = &bp.nodes[*c];
                let shape: Vec<usize> = std::iter::once(t).chain(at_h(&nd.out.shape, 1)).collect();
                self.zeros(nd.store, &shape)
            })
            .collect();
        // This occurrence's items, in slot order, filled position by position.
        let first_item = self.items.len();
        let staged: Vec<usize> = (0..n).filter(|ni| bp.nodes[*ni].commit || self.every).collect();
        for ni in &staged {
            let src = match (s.kinds[occ][*ni], self.every) {
                (Kind::Batched, false) => Src::PerLanes { at: Vec::with_capacity(t) },
                (Kind::Batched, true) => Src::PerValues(Vec::with_capacity(t)),
                // Filled at the first position.
                (Kind::Shared, _) => Src::SharedLanes { at: usize::MAX },
            };
            let it = self.item(occ, *ni, src);
            self.items.push(it);
        }
        for pr in 0..t {
            let p = self.p0 + pr;
            let h = hp_of(p, w);
            let mut vals: Vec<Option<DevTensor>> = vec![None; n];
            for ni in 0..n {
                let node = &bp.nodes[ni];
                let slot = (base + ni) as u32;
                let status = ((1 + pr) * self.slots + base + ni) as u32;
                let out = at_h(&node.out.shape, h);
                let v = if s.kinds[occ][ni] == Kind::Shared {
                    let lookup = |j: usize| -> Option<DevTensor> { vals.get(j).cloned().flatten() };
                    self.shared_node(occ, ni, &lookup, layer)?
                } else {
                    let ins: Vec<DevTensor> =
                        node.inputs.iter().map(|r| self.ref_pp(*r, &vals, carry, layer, pr)).collect::<TirResult<_>>()?;
                    match node.prim {
                        Prim::StateWrite { state } => self.state_write_pp(node, state, layer, &ins[0], status, slot, p)?,
                        Prim::HistAppend { state } => {
                            let inst = plan.instance(state, layer).ok_or_else(|| missing("history instance"))? as usize;
                            let (ring, row_shape) = self.rings.hist[inst].clone().ok_or_else(|| missing("a history store"))?;
                            let rn = numel(&row_shape);
                            let region = Layout::contiguous_at(&row_shape, p * rn);
                            let x = &self.unalias(&ins[0], &ring.buf, status)?;
                            self.rec
                                .ew(
                                    EwOp::Copy,
                                    &row_shape,
                                    (&ring.buf, ring.form, &region),
                                    &[(x, x.layout)],
                                    &[],
                                    &[],
                                    None,
                                    false,
                                    status,
                                )
                                .map_err(|u| TirError::new(TirErrorKind::Shape, format!("history row: {u:?}")))?;
                            self.stats.device_nodes += 1;
                            ring.view(Layout::contiguous_at(&out, (p + 1 - h) * rn))
                        }
                        _ => {
                            let tok = self.toks[pr] as i128;
                            let host = |r: &Ref| match *r {
                                Ref::Const(j) => s.const_scalar(j),
                                Ref::Input(0) => Some(tok),
                                Ref::Input(_) => Some(p as i128),
                                _ => None,
                            };
                            self.plain(node, &ins, &out, status, slot, p, &host)?
                        }
                    }
                };
                if let Ok(k) = staged.binary_search(&ni) {
                    let i = first_item + k;
                    if s.kinds[occ][ni] == Kind::Shared {
                        // Staged once, at the chunk's first position.
                        if matches!(self.items[i].src, Src::SharedLanes { at: usize::MAX }) {
                            self.items[i].src = if self.every {
                                Src::Value(BV::Shared(v.clone()), out.clone())
                            } else {
                                Src::SharedLanes { at: self.stage_lanes(&out, &v, v.layout, slot)? }
                            };
                        }
                    } else if self.every {
                        if let Src::PerValues(list) = &mut self.items[i].src {
                            list.push(v.clone());
                        }
                    } else {
                        let a = self.stage_lanes(&out, &v, v.layout, slot)?;
                        if let Src::PerLanes { at } = &mut self.items[i].src {
                            at.push(a);
                        }
                    }
                }
                vals[ni] = Some(v);
            }
            for (k, c) in outs.iter().enumerate() {
                let v = vals[*c].clone().ok_or_else(|| missing("a carry-out"))?;
                let st = &stacks[k];
                let shape = v.shape().to_vec();
                let region = Layout::contiguous_at(&shape, pr * numel(&shape));
                self.rec
                    .ew(EwOp::Copy, &shape, (&st.buf, st.form, &region), &[(&v, v.layout)], &[], &[], None, false, (base + *c) as u32)
                    .map_err(|u| TirError::new(TirErrorKind::Shape, format!("a carry-out: {u:?}")))?;
            }
        }
        if last {
            let node = &bp.nodes[outs[0]];
            self.logits = Some((BV::Batched(stacks[0].clone()), at_h(&node.out.shape, 1), node.out.dtype));
            return Ok(Vec::new());
        }
        Ok(stacks.into_iter().map(BV::Batched).collect())
    }

    /// `StateWrite` at position `p`: the saturated value into the state's row `p + 1`.
    #[allow(clippy::too_many_arguments)]
    fn state_write_pp(
        &mut self,
        node: &NodePlan,
        state: u16,
        layer: Option<u16>,
        x: &DevTensor,
        status: u32,
        slot: u32,
        p: usize,
    ) -> TirResult<DevTensor> {
        let plan = self.s.plan;
        let inst = plan.instance(state, layer).ok_or_else(|| missing("state instance"))? as usize;
        let (ring, shape) = self.rings.fixed[inst].clone().ok_or_else(|| missing("a state store"))?;
        let n = numel(&shape);
        let dst = ring.view(Layout::contiguous_at(&shape, (p + 1) * n));
        let x = &self.unalias(x, &ring.buf, status)?;
        match self.rec.state_write(node, &plan.program.states[state as usize], x, &dst, status) {
            Ok(()) => {
                self.stats.device_nodes += 1;
                Ok(dst)
            }
            Err(why) => {
                *self.stats.fallback.entry(format!("{}: {why:?}", node.prim.name())).or_default() += 1;
                if p >= self.first_failure_pos() {
                    return Ok(dst);
                }
                let dev = self.s.dev;
                let xb = dev.download(x);
                let opd = Opd { data: xb.slice(), layout: Layout::contiguous(x.shape()) };
                match cpu_kernel(node, &[opd], &shape, &plan.program.states) {
                    Ok(b) => {
                        // Into the row through the recording, so that it lands in order.
                        let tmp = dev.upload(b.slice(), dst.form, &shape);
                        let lane = 4u64; // states are i8/i16/i32: S32 lanes
                        self.rec.encoder().copy_buffer_to_buffer(&tmp.buf, 0, &ring.buf, ((p + 1) * n) as u64 * lane, n as u64 * lane);
                    }
                    Err(e) => self.fail_host(p, slot, e),
                }
                Ok(dst)
            }
        }
    }

    // ------------------------------------------------------------ delivery

    /// Hand the sink every value of positions `p0 .. end`, in the CPU executor's order.
    fn deliver(&mut self, sink: &mut dyn ReplaySink, lanes: &[i32], end: usize) {
        if end <= self.p0 {
            return;
        }
        let dev = self.s.dev;
        let host: Vec<Option<Vec<i128>>> = self
            .items
            .iter()
            .map(|it| match &it.src {
                Src::Value(v, _) => Some(dev.download(v.tensor()).to_i128s()),
                _ => None,
            })
            .collect();
        let host_pp: Vec<Option<Vec<Vec<i128>>>> = self
            .items
            .iter()
            .map(|it| match &it.src {
                Src::PerValues(list) => Some(list.iter().take(end - self.p0).map(|t| dev.download(t).to_i128s()).collect()),
                _ => None,
            })
            .collect();
        let logits = self.logits.as_ref().map(|(v, shape, _)| (matches!(v, BV::Batched(_)), read_buf(dev, v.tensor()), shape.clone()));
        let mut scratch: Vec<i32> = Vec::new();
        for p in self.p0..end {
            let pr = p - self.p0;
            for (i, it) in self.items.iter().enumerate() {
                let hp = hp_of(p, it.window);
                let shape = at_h(&it.shape, hp);
                let h = h_axis(&it.shape);
                // Lanes: the committed words themselves (an `i32` value is handed without a copy).
                let words: Option<&[i32]> = match &it.src {
                    Src::Lanes { at, pad } => {
                        let n = numel(pad);
                        let block = &lanes[at + pr * n..at + (pr + 1) * n];
                        if h.is_some_and(|h| pad[h] != hp) {
                            scratch.clear();
                            valid_into(block, pad, h, hp, &mut scratch);
                            Some(&scratch)
                        } else {
                            Some(block)
                        }
                    }
                    Src::SharedLanes { at } => Some(&lanes[*at..at + numel(&shape)]),
                    Src::PerLanes { at } => Some(&lanes[at[pr]..at[pr] + numel(&shape)]),
                    _ => None,
                };
                let owned;
                let data = match (words, it.dtype) {
                    (Some(w), DType::I32) => Slice::I32(w),
                    (Some(w), dt) => {
                        owned = words_buf(dt, w);
                        owned.slice()
                    }
                    (None, dt) => {
                        let vals = match &it.src {
                            Src::Value(BV::Batched(_), pad) => {
                                let n = numel(pad);
                                let all = host[i].as_ref().expect("read back");
                                valid(&all[pr * n..(pr + 1) * n], pad, h, hp)
                            }
                            Src::Value(BV::Shared(_), _) => host[i].clone().expect("read back"),
                            _ => host_pp[i].as_ref().expect("read back")[pr].clone(),
                        };
                        owned = Buf::from_i128s(dt, &vals);
                        owned.slice()
                    }
                };
                sink.node(
                    p as u32,
                    &NodeValue {
                        slot: it.slot,
                        block: it.block,
                        layer: it.layer,
                        node: it.node,
                        commit: it.commit,
                        dtype: it.dtype,
                        shape: &shape,
                        data,
                    },
                );
            }
            if let Some((batched, buf, shape)) = &logits {
                let n = numel(shape);
                let d = if *batched { buf.slice().sub(pr * n, n) } else { buf.slice() };
                sink.logits(p as u32, shape, d);
            }
        }
    }
}
