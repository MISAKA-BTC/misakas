//! **RFC-0002 Phase F, step F8: an IR class's canonical work, from the program's structure alone**
//! (`docs/design/palw/tir/phase-f-integration.md` §2.9; ADR-0145 §4's vector).
//!
//! A legacy class's work vector ([`crate::palw_canonical_work_v1`]) walks a shape profile whose node
//! kinds name what each node is. An IR program has no names: its nodes are 25 primitives, and what
//! a node IS economically is a fact of the graph around it. This module classifies every node by
//! structure only and prices it with spec 04b §8's per-node costs, then sums the executed positions
//! in closed form. The result is the same [`PalwCanonicalWorkVectorV1`] the accounting sites read.
//!
//! **Classification** (in this precedence; within a block, by structure only):
//!
//! 1. `dense_matmul` — a `MatMul` with an operand that is a `Param` seen through structural
//!    primitives only (`Reshape`, `Transpose`, `Slice`, `Broadcast`, `Cast`, or a `Concat` of such),
//!    the LM head included. Priced at its MACs × the weight's byte width (a 16-bit weight is two
//!    bytes of traffic per MAC, `palw_weight_dtype_cost_v1`'s rule), and its weight bytes are the
//!    param operand's bytes, per evaluation.
//! 2. `routed_expert_matmul` — a `MatMul` with an operand that is a `Gather` of such a param view
//!    whose indices trace to a `TopK` (the router): only the gathered — active — experts' MACs, at
//!    the gathered rows' width, and their bytes.
//! 3. `normalization` (RFC OQ7, user decision) — a `ReduceSum` whose output reaches an `IntRsqrt`
//!    through elementwise and structural primitives, and those `IntRsqrt`s.
//! 4. `recurrence` — a node on a path from a `State` read to a `StateWrite` (the state update), and a
//!    `MatMul` with the state as an operand (a `State` read or a `StateWrite`'s output, seen through
//!    structural primitives — the read-out).
//! 5. `attention` — a node whose output has `H`, a reduction over `H` (`ReduceSum`/`ReduceMax` along
//!    an `H` axis, a `MatMul` contracting `H`) — split into `attention_prefill` (positions `a < P`)
//!    and `attention_decode` (`a ≥ P`) because the same arithmetic at batch one and at batch `P` is not
//!    the same job on any device.
//! 6. `other_verified_ops` — every other node.
//!
//! **Units.** Arithmetic dimensions are MAC-equivalents with ADR-0131's table
//! ([`crate::palw_economic_compute_v1::PALW_ECONOMIC_COST_TABLE_V1`]): a MAC is 1 (× the weight width
//! for a weight matmul), an elementwise op 1, a transcendental evaluation 4 — §8's three arithmetic
//! counts, weighted. The traffic dimensions are bytes: weight bytes of dense and routed matmuls, and
//! the history's bytes read (`B(out)` of each `HistAppend`) and written (`B(row)`).
//!
//! **Nothing a registrant lays out is read**: not `commit`, not a tile length, not `h_tile`, not the
//! checkpoint interval (PALW-TIR-16, PALW-WK-3). `tests/palw_tir_work.rs` re-commits programs and
//! requires an identical vector, and checks the closed form against a position-by-position walk.
//!
//! **Closed form.** §8's costs are affine in `H` (a tensor has at most one `H`, so an element count
//! and a contraction are linear in it), so a block's cost at position `a` is `c₀ + c₁ · min(a + 1, W)`
//! and a job sums in `O(blocks)`, whatever its context.

use crate::palw_canonical_work_v1::{PalwCanonicalExecutionFactsV1, PalwCanonicalWorkError, PalwCanonicalWorkVectorV1};
use crate::palw_economic_compute_v1::PALW_ECONOMIC_COST_TABLE_V1;
use misaka_palw_tir::admit::{CostV1, node_cost};
use misaka_palw_tir::program::{Block, Ref};
use misaka_palw_tir::types::Dim;
use misaka_palw_tir::{Prim, TirProgramV1};

/// The dimension a node's cost is charged to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwTirWorkKindV1 {
    DenseMatmul,
    RoutedExpertMatmul,
    Normalization,
    Recurrence,
    Attention,
    Other,
}

const KINDS: usize = 6;

impl PalwTirWorkKindV1 {
    fn index(self) -> usize {
        self as usize
    }
}

/// `c0 + c1 · H`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Affine {
    c0: u128,
    c1: u128,
}

impl Affine {
    fn add(self, o: Affine) -> Affine {
        Affine { c0: self.c0.saturating_add(o.c0), c1: self.c1.saturating_add(o.c1) }
    }

    fn sub(self, o: Affine) -> Affine {
        Affine { c0: self.c0.saturating_sub(o.c0), c1: self.c1.saturating_sub(o.c1) }
    }

    /// Coefficient-wise maximum: `A + B − max(A, B)` is the coefficient-wise minimum, an affine form
    /// never above `min(A(H), B(H))` at any `H`.
    fn max(self, o: Affine) -> Affine {
        Affine { c0: self.c0.max(o.c0), c1: self.c1.max(o.c1) }
    }
}

/// One block's work, per dimension and per traffic term, affine in `H`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BlockWorkV1 {
    kinds: [Affine; KINDS],
    weight_bytes: Affine,
    kv_read: Affine,
    kv_write: Affine,
}

impl BlockWorkV1 {
    fn zip(self, o: BlockWorkV1, f: impl Fn(Affine, Affine) -> Affine) -> BlockWorkV1 {
        let mut kinds = [Affine::default(); KINDS];
        for (k, slot) in kinds.iter_mut().enumerate() {
            *slot = f(self.kinds[k], o.kinds[k]);
        }
        BlockWorkV1 {
            kinds,
            weight_bytes: f(self.weight_bytes, o.weight_bytes),
            kv_read: f(self.kv_read, o.kv_read),
            kv_write: f(self.kv_write, o.kv_write),
        }
    }

    fn add(self, o: BlockWorkV1) -> BlockWorkV1 {
        self.zip(o, Affine::add)
    }

    fn sub(self, o: BlockWorkV1) -> BlockWorkV1 {
        self.zip(o, Affine::sub)
    }

    fn max(self, o: BlockWorkV1) -> BlockWorkV1 {
        self.zip(o, Affine::max)
    }
}

/// **The `Select` arm-only regions of a block** (the second IR fence's work credit): for each
/// `Select` `s`, in ascending node order, and each arm `k ∈ {1, 2}` (its value operands), the nodes
/// whose every use reaches the rest of the block only through `s`'s operand `k` — `E(s, k)`. A node is
/// in `E(s, k)` iff it is not a sink (a commit point, which a court checks whole, carry-outs and the
/// logits included; a `StateWrite`; a `HistAppend`), it has a use, and every use is `s` at operand `k`
/// or a node already in `E(s, k)`. So a node that `s`'s condition, its other arm, or anything outside
/// the arm reads is outside it, and a region holds no committed or state-writing node: exactly the
/// work a backend may skip where the condition does not choose the arm, and no court would notice.
pub fn palw_tir_select_arm_regions_v1(block: &Block) -> Vec<(usize, [Vec<usize>; 2])> {
    let n = block.nodes.len();
    let mut uses: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (i, node) in block.nodes.iter().enumerate() {
        for (slot, r) in node.inputs.iter().enumerate() {
            if let Ref::Node(j) = r {
                uses[*j as usize].push((i, slot));
            }
        }
    }
    let sink = |i: usize| {
        let node = &block.nodes[i];
        node.commit || matches!(node.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. })
    };
    let mut out = Vec::new();
    for (s, node) in block.nodes.iter().enumerate() {
        if !matches!(node.prim, Prim::Select) || node.inputs.len() != 3 {
            continue;
        }
        let mut arms: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
        for (a, slot) in [1usize, 2].into_iter().enumerate() {
            let mut member = vec![false; n];
            for m in (0..s).rev() {
                member[m] = !sink(m)
                    && !uses[m].is_empty()
                    && uses[m].iter().all(|(c, k)| (*c == s && *k == slot) || (*c < s && member[*c]));
            }
            arms[a] = (0..s).filter(|m| member[*m]).collect();
        }
        out.push((s, arms));
    }
    out
}

/// **An IR class's work, per block and affine in `H`** — derived once per program, summed per job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirWorkShapeV1 {
    blocks: Vec<BlockWorkV1>,
    windows: Vec<Option<u32>>,
    /// The block of every occurrence, `pre`, each layer, `post`.
    occurrences: Vec<u8>,
    /// Bytes of every param instance the artifact holds.
    param_bytes: u128,
}

/// Why a program has no work shape.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirWorkError {
    #[error("the program is not in normal form: {0}")]
    Program(String),
    #[error("block {block} node {node}: §8's cost is not affine in H")]
    NotAffine { block: usize, node: usize },
    #[error("an IR job runs at least one prompt position")]
    NoPrompt,
    #[error(transparent)]
    Work(#[from] PalwCanonicalWorkError),
}

/// A node's value is a param seen through structural primitives only: the param, or `None`.
fn param_view(block: &Block, r: Ref) -> Option<u16> {
    let mut r = r;
    loop {
        match r {
            Ref::Param(j) => return Some(j),
            Ref::Node(m) => {
                let n = &block.nodes[m as usize];
                match n.prim {
                    Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast | Prim::Cast => r = n.inputs[0],
                    Prim::Concat { .. } => {
                        let views: Vec<Option<u16>> = n.inputs.iter().map(|i| param_view(block, *i)).collect();
                        return views.first().copied().flatten().filter(|_| views.iter().all(Option::is_some));
                    }
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
}

/// Whether node `m`'s backward closure within the block holds a `TopK`.
fn traces_to_topk(block: &Block, r: Ref) -> bool {
    let Ref::Node(start) = r else { return false };
    let mut seen = vec![false; block.nodes.len()];
    let mut stack = vec![start as usize];
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        if matches!(block.nodes[i].prim, Prim::TopK { .. }) {
            return true;
        }
        stack.extend(block.nodes[i].inputs.iter().filter_map(|x| if let Ref::Node(j) = x { Some(*j as usize) } else { None }));
    }
    false
}

/// A node's value is a `Gather` of a param view by router indices, seen through structural
/// primitives: the gathered node.
fn routed_view(block: &Block, r: Ref) -> Option<u16> {
    let mut r = r;
    loop {
        let Ref::Node(m) = r else { return None };
        let n = &block.nodes[m as usize];
        match n.prim {
            Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast | Prim::Cast => r = n.inputs[0],
            Prim::Gather { .. } => {
                return (param_view(block, n.inputs[0]).is_some() && traces_to_topk(block, n.inputs[1])).then_some(m);
            }
            _ => return None,
        }
    }
}

/// A node's value is the state — a `State` read or a `StateWrite`'s output — seen through structural
/// primitives.
fn state_view(block: &Block, r: Ref) -> bool {
    let mut r = r;
    loop {
        match r {
            Ref::State(_) => return true,
            Ref::Node(m) => {
                let n = &block.nodes[m as usize];
                match n.prim {
                    Prim::StateWrite { .. } => return true,
                    Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Broadcast | Prim::Cast => r = n.inputs[0],
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
}

fn is_scalar_op(p: &Prim) -> bool {
    matches!(
        p,
        Prim::Reshape
            | Prim::Transpose { .. }
            | Prim::Slice { .. }
            | Prim::Concat { .. }
            | Prim::Broadcast
            | Prim::Cast
            | Prim::Add
            | Prim::Sub
            | Prim::Mul
            | Prim::Div { .. }
            | Prim::Clamp { .. }
            | Prim::Log2Floor
            | Prim::Compare { .. }
            | Prim::Select
    )
}

/// **Every node's dimension, by structure.**
pub fn palw_tir_work_kinds_v1(p: &TirProgramV1, block: usize) -> Vec<PalwTirWorkKindV1> {
    let b = &p.blocks[block];
    let n = b.nodes.len();
    let consumers = {
        let mut c: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, node) in b.nodes.iter().enumerate() {
            for r in &node.inputs {
                if let Ref::Node(j) = r {
                    c[*j as usize].push(i);
                }
            }
        }
        c
    };
    // Normalization: IntRsqrts, and ReduceSums that reach one through scalar ops.
    let mut normalization = vec![false; n];
    for (i, node) in b.nodes.iter().enumerate() {
        match node.prim {
            Prim::IntRsqrt => normalization[i] = true,
            Prim::ReduceSum { .. } => {
                let mut seen = vec![false; n];
                let mut stack = consumers[i].clone();
                while let Some(k) = stack.pop() {
                    if std::mem::replace(&mut seen[k], true) {
                        continue;
                    }
                    if matches!(b.nodes[k].prim, Prim::IntRsqrt) {
                        normalization[i] = true;
                        break;
                    }
                    if is_scalar_op(&b.nodes[k].prim) {
                        stack.extend(consumers[k].iter().copied());
                    }
                }
            }
            _ => {}
        }
    }
    // Recurrence: forward from a State read, backward from a StateWrite.
    let mut from_state = vec![false; n];
    for (i, node) in b.nodes.iter().enumerate() {
        let reads_state = node.inputs.iter().any(|r| matches!(r, Ref::State(_)));
        let from_node = node.inputs.iter().any(|r| matches!(r, Ref::Node(j) if from_state[*j as usize]));
        from_state[i] = reads_state || from_node;
    }
    let mut to_write = vec![false; n];
    for i in (0..n).rev() {
        to_write[i] = matches!(b.nodes[i].prim, Prim::StateWrite { .. }) || consumers[i].iter().any(|k| to_write[*k]);
    }
    let over_h = |i: usize| -> bool {
        let node = &b.nodes[i];
        if node.out.has_h() {
            return true;
        }
        let input_type = |r: Ref| -> Option<Vec<Dim>> {
            match r {
                Ref::Node(j) => Some(b.nodes[j as usize].out.shape.clone()),
                Ref::CarryIn(k) => b.carry_in.get(k as usize).map(|t| t.shape.clone()),
                _ => None,
            }
        };
        match node.prim {
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
                input_type(node.inputs[0]).is_some_and(|s| s.get(axis as usize).is_some_and(|d| d.is_h()))
            }
            Prim::MatMul => input_type(node.inputs[0]).is_some_and(|s| s.last().is_some_and(|d| d.is_h())),
            _ => false,
        }
    };
    (0..n)
        .map(|i| {
            let node = &b.nodes[i];
            if matches!(node.prim, Prim::MatMul) {
                if node.inputs.iter().any(|r| param_view(b, *r).is_some()) {
                    return PalwTirWorkKindV1::DenseMatmul;
                }
                if node.inputs.iter().any(|r| routed_view(b, *r).is_some()) {
                    return PalwTirWorkKindV1::RoutedExpertMatmul;
                }
            }
            if normalization[i] {
                return PalwTirWorkKindV1::Normalization;
            }
            if (from_state[i] && to_write[i]) || (matches!(node.prim, Prim::MatMul) && node.inputs.iter().any(|r| state_view(b, *r))) {
                return PalwTirWorkKindV1::Recurrence;
            }
            if over_h(i) {
                return PalwTirWorkKindV1::Attention;
            }
            PalwTirWorkKindV1::Other
        })
        .collect()
}

/// A node's MAC-equivalents at one `H`: MACs (× the weight width for a weight matmul) plus
/// elementwise ops plus four per transcendental evaluation.
fn mac_eq(c: &CostV1, weight_width: u128) -> u128 {
    let t = &PALW_ECONOMIC_COST_TABLE_V1;
    (c.macs as u128)
        .saturating_mul(weight_width.max(1))
        .saturating_mul(t.matmul_mac as u128)
        .saturating_add((c.elementwise as u128).saturating_mul(t.elementwise as u128))
        .saturating_add((c.transcendentals as u128).saturating_mul(t.transcendental as u128))
}

/// **One node's work at one `H`**: `[MAC-equivalents, weight bytes, history bytes read, history
/// bytes written]`, for a node of dimension `kind` (from [`palw_tir_work_kinds_v1`]).
pub fn palw_tir_node_work_v1(p: &TirProgramV1, block: usize, node: usize, kind: PalwTirWorkKindV1, h: u64) -> [u128; 4] {
    let b = &p.blocks[block];
    let n = &b.nodes[node];
    // The weight a weight matmul streams: the param view's (or the gathered rows') width and bytes,
    // per evaluation.
    let (width, weight_bytes) = (|| {
        if !matches!(kind, PalwTirWorkKindV1::DenseMatmul | PalwTirWorkKindV1::RoutedExpertMatmul) {
            return (1u128, 0u128);
        }
        for r in &n.inputs {
            if let Some(j) = param_view(b, *r) {
                let width = p.params[j as usize].dtype.width() as u128;
                return (width, (operand_type(p, block, *r).elements_at(h) as u128).saturating_mul(width));
            }
            if let Some(m) = routed_view(b, *r) {
                let t = &b.nodes[m as usize].out;
                let width = t.dtype.width() as u128;
                return (width, (t.elements_at(h) as u128).saturating_mul(width));
            }
        }
        (1, 0)
    })();
    let c = node_cost(p, block, node, h);
    let (kv_read, kv_write) =
        if matches!(n.prim, Prim::HistAppend { .. }) { (c.bytes_read as u128, c.bytes_written as u128) } else { (0, 0) };
    [mac_eq(&c, width), weight_bytes, kv_read, kv_write]
}

/// **An IR program's work shape**: every node classified and priced, affine in `H`, summed per block.
pub fn palw_tir_work_shape_v1(p: &TirProgramV1) -> Result<PalwTirWorkShapeV1, PalwTirWorkError> {
    palw_tir_work_shape_with_v1(p, false)
}

/// [`palw_tir_work_shape_v1`], with `min_select_arms` the second IR fence's credit
/// (`Params::palw_tir_fence2`): each `Select`'s arm-only regions ([`palw_tir_select_arm_regions_v1`])
/// are credited at the coefficient-wise minimum of the two arms — the least any execution does, as it
/// computes one arm per element — instead of both. Nested `Select`s are credited first (ascending
/// node order), so an outer arm's size is its credited size: with `w(n)` a node's work and
/// `A_s = Σ_{n ∈ E(s,1)} w(n) − Σ_{Select s' ∈ E(s,1)} δ(s')` (`B_s` likewise), the discount is
/// `δ(s) = max(A_s, B_s)` coefficient-wise and the block is credited `Σ_n w(n) − Σ_s δ(s)`.
pub fn palw_tir_work_shape_with_v1(p: &TirProgramV1, min_select_arms: bool) -> Result<PalwTirWorkShapeV1, PalwTirWorkError> {
    let info = misaka_palw_tir::validate::validate(p).map_err(|e| PalwTirWorkError::Program(e.to_string()))?;
    let mut blocks = Vec::with_capacity(p.blocks.len());
    for bi in 0..p.blocks.len() {
        let kinds = palw_tir_work_kinds_v1(p, bi);
        let windowed = info.blocks[bi].window.is_some();
        let mut w = BlockWorkV1::default();
        let mut per_node: Vec<BlockWorkV1> = Vec::with_capacity(kinds.len());
        for (ni, &kind) in kinds.iter().enumerate() {
            let sample = |h: u64| palw_tir_node_work_v1(p, bi, ni, kind, h);
            let affine: [Affine; 4] = if windowed {
                let (s1, s2, s3) = (sample(1), sample(2), sample(3));
                let mut out = [Affine::default(); 4];
                for k in 0..4 {
                    let c1 = s2[k].checked_sub(s1[k]).ok_or(PalwTirWorkError::NotAffine { block: bi, node: ni })?;
                    let c0 = s1[k].checked_sub(c1).ok_or(PalwTirWorkError::NotAffine { block: bi, node: ni })?;
                    if c0.saturating_add(c1.saturating_mul(3)) != s3[k] {
                        return Err(PalwTirWorkError::NotAffine { block: bi, node: ni });
                    }
                    out[k] = Affine { c0, c1 };
                }
                out
            } else {
                sample(1).map(|v| Affine { c0: v, c1: 0 })
            };
            let mut node = BlockWorkV1::default();
            node.kinds[kind.index()] = affine[0];
            node.weight_bytes = affine[1];
            node.kv_read = affine[2];
            node.kv_write = affine[3];
            w = w.add(node);
            per_node.push(node);
        }
        if min_select_arms {
            let block = &p.blocks[bi];
            let mut discount: Vec<Option<BlockWorkV1>> = vec![None; block.nodes.len()];
            for (s, arms) in palw_tir_select_arm_regions_v1(block) {
                let credited = |region: &[usize]| {
                    region.iter().fold(BlockWorkV1::default(), |acc, m| {
                        let inner = discount[*m].unwrap_or_default();
                        acc.add(per_node[*m]).sub(inner)
                    })
                };
                let (a, b) = (credited(&arms[0]), credited(&arms[1]));
                let d = a.max(b);
                discount[s] = Some(d);
                w = w.sub(d);
            }
        }
        blocks.push(w);
    }
    let occurrences = p.occurrences().into_iter().map(|(b, _)| b).collect();
    let instances = crate::palw_tir_artifact_v1::palw_tir_param_instances_v1(p);
    let param_bytes = instances
        .iter()
        .enumerate()
        .map(|(j, inst)| {
            (crate::palw_tir_artifact_v1::palw_tir_tensor_bytes_v1(p, j as u16) as u128).saturating_mul(inst.len() as u128)
        })
        .fold(0u128, u128::saturating_add);
    Ok(PalwTirWorkShapeV1 { blocks, windows: info.blocks.iter().map(|b| b.window).collect(), occurrences, param_bytes })
}

fn operand_type(p: &TirProgramV1, block: usize, r: Ref) -> misaka_palw_tir::TensorType {
    let b = &p.blocks[block];
    match r {
        Ref::Node(j) => b.nodes[j as usize].out.clone(),
        Ref::CarryIn(k) => b.carry_in[k as usize].clone(),
        Ref::Param(j) => misaka_palw_tir::TensorType::fixed(p.params[j as usize].dtype, &p.params[j as usize].shape),
        Ref::Const(j) => misaka_palw_tir::TensorType::fixed(p.consts[j as usize].dtype, &p.consts[j as usize].shape),
        Ref::State(j) => misaka_palw_tir::TensorType::fixed(p.states[j as usize].dtype, &p.states[j as usize].shape),
        Ref::Input(_) => misaka_palw_tir::TensorType::scalar(misaka_palw_tir::DType::Idx),
    }
}

/// `Σ_{a ∈ [lo, hi)} H(a)` with `H(a) = min(a + 1, W)` (1 without a window).
fn sum_h(window: Option<u32>, lo: u128, hi: u128) -> u128 {
    if hi <= lo {
        return 0;
    }
    let Some(w) = window else { return hi - lo };
    let w = w as u128;
    // f(n) = Σ_{a < n} min(a + 1, W)
    let f = |n: u128| -> u128 { if n <= w { n * (n + 1) / 2 } else { w * (w + 1) / 2 + (n - w) * w } };
    f(hi) - f(lo)
}

impl PalwTirWorkShapeV1 {
    /// Bytes of every param instance — the artifact the class's inventory commits.
    pub fn param_bytes(&self) -> u128 {
        self.param_bytes
    }

    /// **The work of one execution**: positions `reused … P − 1` of the prompt, then one per further
    /// generated token (`P … P + G − 2`); `post` (the logits) runs where they are consumed, `a ≥ P − 1`.
    pub fn work_v1(&self, facts: &PalwCanonicalExecutionFactsV1) -> Result<PalwCanonicalWorkVectorV1, PalwTirWorkError> {
        self.work_cell_v1(0..self.occurrences.len(), 0..u128::MAX, facts)
    }

    /// **RFC-0006 §6.1: the work of one CELL** — the occurrences `occ` (indices into the program's occurrences:
    /// `0` is `pre`, `1 + l` layer `l`, the last `post`) over the absolute positions `[from, to)` of one
    /// execution. [`Self::work_v1`] is the whole execution: every occurrence over every position, so the cells of
    /// a partition sum to it exactly.
    pub fn work_cell_v1(
        &self,
        occ: std::ops::Range<usize>,
        positions: std::ops::Range<u128>,
        facts: &PalwCanonicalExecutionFactsV1,
    ) -> Result<PalwCanonicalWorkVectorV1, PalwTirWorkError> {
        if facts.generated_tokens == 0 {
            return Err(PalwCanonicalWorkError::NoGeneratedToken.into());
        }
        if facts.reused_prefix_tokens > facts.prefill_tokens {
            return Err(PalwCanonicalWorkError::PrefixExceedsPrefill {
                reused: facts.reused_prefix_tokens,
                prefill: facts.prefill_tokens,
            }
            .into());
        }
        if facts.prefill_tokens == 0 {
            return Err(PalwTirWorkError::NoPrompt);
        }
        let (reused, prefill) = (facts.reused_prefix_tokens as u128, facts.prefill_tokens as u128);
        let end = prefill + facts.generated_tokens as u128 - 1;
        let post = self.occurrences.len() - 1;
        let mut v = PalwCanonicalWorkVectorV1::default();
        for (o, block) in self.occurrences.iter().enumerate().filter(|(o, _)| occ.contains(o)) {
            let w = &self.blocks[*block as usize];
            let window = self.windows[*block as usize];
            // The positions this occurrence runs, split at the prompt's end.
            // `post` runs at the last prompt position whatever was reused — that is where the first
            // token comes from (the legacy derivation counts it the same way).
            let (lo, hi) = if o == post { (prefill - 1, end) } else { (reused, end) };
            // The cell's own positions (the whole execution's are `0..u128::MAX`).
            let (lo, hi) = (lo.max(positions.start), hi.min(positions.end));
            let sum = |a: Affine, from: u128, to: u128| -> u128 {
                if to <= from {
                    return 0;
                }
                a.c0.saturating_mul(to - from).saturating_add(a.c1.saturating_mul(sum_h(window, from, to)))
            };
            let all = |a: Affine| sum(a, lo, hi);
            let k = |kind: PalwTirWorkKindV1| w.kinds[kind.index()];
            v.dense_matmul = v.dense_matmul.saturating_add(all(k(PalwTirWorkKindV1::DenseMatmul)));
            v.routed_expert_matmul = v.routed_expert_matmul.saturating_add(all(k(PalwTirWorkKindV1::RoutedExpertMatmul)));
            v.normalization = v.normalization.saturating_add(all(k(PalwTirWorkKindV1::Normalization)));
            v.recurrence = v.recurrence.saturating_add(all(k(PalwTirWorkKindV1::Recurrence)));
            v.other_verified_ops = v.other_verified_ops.saturating_add(all(k(PalwTirWorkKindV1::Other)));
            let split = prefill.clamp(lo, hi.max(lo));
            v.attention_prefill = v.attention_prefill.saturating_add(sum(k(PalwTirWorkKindV1::Attention), lo, split));
            v.attention_decode = v.attention_decode.saturating_add(sum(k(PalwTirWorkKindV1::Attention), split, hi));
            v.weight_traffic_bytes = v.weight_traffic_bytes.saturating_add(all(w.weight_bytes));
            v.kv_read_bytes = v.kv_read_bytes.saturating_add(all(w.kv_read));
            v.kv_write_bytes = v.kv_write_bytes.saturating_add(all(w.kv_write));
        }
        Ok(v)
    }
}

/// **An IR class's canonical work for one execution** — [`palw_tir_work_shape_v1`] then
/// [`PalwTirWorkShapeV1::work_v1`].
pub fn palw_tir_canonical_work_v1(
    p: &TirProgramV1,
    facts: &PalwCanonicalExecutionFactsV1,
) -> Result<PalwCanonicalWorkVectorV1, PalwTirWorkError> {
    palw_tir_work_shape_v1(p)?.work_v1(facts)
}

/// **The registry's work of an IR class** (Protocol Upgrade A, [`crate::palw_model_registry_v1`]):
/// the verification compute of the canonical job a seat replays, the compute of one prefill draw,
/// and the artifact's bytes — exact for an IR class, whose inventory is a function of the program's
/// declarations. The same walk the claim accounting reads, so the two never disagree about a class.
pub fn palw_tir_model_work_v1(
    p: &TirProgramV1,
    canonical: &crate::palw_v2::PalwJobContextV2,
) -> Result<crate::palw_model_registry_v1::PalwModelWorkV1, PalwTirWorkError> {
    palw_tir_model_work_v2(p, canonical, false)
}

/// [`palw_tir_model_work_v1`] with the second IR fence's `Select`-arm credit when `min_select_arms`
/// ([`palw_tir_work_shape_with_v1`]) — what a registration past `Params::palw_tir_fence2` records.
pub fn palw_tir_model_work_v2(
    p: &TirProgramV1,
    canonical: &crate::palw_v2::PalwJobContextV2,
    min_select_arms: bool,
) -> Result<crate::palw_model_registry_v1::PalwModelWorkV1, PalwTirWorkError> {
    let shape = palw_tir_work_shape_with_v1(p, min_select_arms)?;
    let job = PalwCanonicalExecutionFactsV1::uncached(canonical.declared_prefill_tokens, canonical.exact_decode_tokens);
    let verification = shape.work_v1(&job)?.provisional_scalar_v1();
    let draw = shape.work_v1(&PalwCanonicalExecutionFactsV1::of_attempt(canonical, true))?.provisional_scalar_v1();
    let bytes = shape.param_bytes().min(u64::MAX as u128) as u64;
    Ok(crate::palw_model_registry_v1::PalwModelWorkV1 {
        verification_ccu: verification,
        economic_ccu_per_claim: draw,
        artifact_bytes: bytes,
        working_set_bytes: bytes,
        ops_supported: true,
    })
}
