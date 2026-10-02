//! **Fused kernels (RFC-0002 §7, Phase G)** — `(pattern, implementation)` pairs the executor runs in
//! place of a subgraph's generic primitive kernels, byte-identically.
//!
//! * **The pattern is a `tir_library_v1` template** ([`FusedKernelV1::emit`]) with its structural
//!   constraints — primitives, attributes, shapes, the operand dtypes the implementation serves — and
//!   it matches by structural equality over the canonical program ([`pattern`], F-5): the template is
//!   re-emitted at the program's own operand types and compared node for node. A subgraph no pattern
//!   matches runs on the generic kernels.
//! * **A region is fused only where nothing can fail** ([`enable`]): under the plan an executor
//!   refined for the params it holds, no node of the region has a check left (no dtype check, no
//!   checked product, no operand check, every exact sum `Fast64`). So a fused kernel never has to
//!   reproduce a failure, and where one could fire the region runs generic and fails exactly where
//!   the reference fails.
//! * **What may move (F-2).** An implementation reorders, vectorises and threads only exact sums
//!   (the order-free rule, PALW-TIR-24) and computes every lossy site — each `Div`, `Clamp`, the
//!   state saturation — on exactly the value the template computes it on, per element.
//! * **Nothing a region computes is observed but its output** (F-1 at the commit points): an interior
//!   node is not a commit point, not a carry-out, not the logits and read by nothing outside the
//!   region, so skipping it changes no byte anyone sees. A step whose sink asks for every node's
//!   value (`StepSink::every_node`, the cone and court paths) runs generic throughout.
//! * **Outside the identity (F-3).** This is node software: nothing here reaches a consensus object,
//!   a class id or a fingerprint, and a kernel is added or removed by a node release.
//! * **The gate (F-4)** is `tests/fused_gate.rs`: per kernel, random and range-extreme operands over
//!   its domain against the reference evaluator, the independent second implementation
//!   (`misaka-palw-tir-ref2`) and the generic backend, and a deliberately broken variant
//!   (`TirExecutor::set_fused_fault`) that the gate must catch.
//! * **An integer GPU backend (F-6)** is one more [`FusedKernelV1::run`] over the same operands: every
//!   primitive is integer and every order-free sum order-independent, so it is exact under F-1 and
//!   is held by the same gate.

mod pattern;

pub mod gdn_step;
pub mod rowops;

use misaka_palw_tir::builder::BlockBuilder;
use misaka_palw_tir::program::{Node, StateDecl, StateKind, TirProgramV1};
use misaka_palw_tir::{DType, Prim, Ref, TensorType, TirResult};

use crate::elem::{Buf, Slice};
use crate::kernels::Opd;
use crate::plan::{Acc, BlockPlan};

/// A kernel's attributes, as matching bound them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bound {
    /// [`gdn_step::GdnStep`]: everything is read off the operands and the state.
    GdnStep,
    /// [`rowops::L2UnitQ15`].
    L2UnitQ15,
    /// [`rowops::RmsUnitQ24`].
    RmsUnitQ24,
}

/// One variant of a pattern: the attributes it is emitted under, the probe types its skeleton is
/// emitted at, and the scratch states the template reads or writes.
pub struct Variant {
    pub bound: Bound,
    pub probe: Vec<TensorType>,
    pub states: Vec<StateDecl>,
}

/// The operands and results of one fused run.
pub struct FusedIo<'a, 'r> {
    /// The holes, in the template's carry-in order.
    pub holes: &'a [Opd<'r>],
    /// The current value (the position's start) of each state the template reads, in its order.
    pub states: &'a [Slice<'r>],
    /// The range of each of those states (a `Fixed` state's `[lo, hi]`).
    pub state_ranges: &'a [(i64, i64)],
    /// The pending value of the state the region writes, which the kernel fills (the region's
    /// `StateWrite`); empty when it writes none.
    pub state_next: &'a mut [Buf],
    /// The output node's storage, filled with `out_numel` values stored as `out_store`.
    pub out: &'a mut Buf,
    pub out_store: DType,
    pub out_shape: &'a [usize],
    /// Move one output lane by one — the deliberately broken variant the gate must catch
    /// (`TirExecutor::set_fused_fault`). Never set outside a test.
    pub fault: bool,
}

/// A fused kernel: a pattern and its implementation.
pub trait FusedKernelV1: Sync {
    fn name(&self) -> &'static str;
    /// The pattern's variants (a skeleton each).
    fn variants(&self) -> Vec<Variant>;
    /// The attributes at the actual hole types, from the skeleton's and the matched output node's.
    fn derive(&self, variant: &Bound, holes: &[TensorType], output: &Node) -> Option<Bound>;
    /// The template, emitted with `holes` as its operands.
    fn emit(&self, b: &mut BlockBuilder<'_>, holes: &[Ref], bound: &Bound) -> Ref;
    /// The implementation's domain beyond fail-freedom: the operand dtypes and shapes it computes.
    fn domain(&self, bound: &Bound, holes: &[TensorType], out: &TensorType) -> bool;
    /// Compute the region: the output (and the state the region writes).
    fn run(&self, bound: &Bound, io: &mut FusedIo<'_, '_>) -> TirResult<()>;
}

/// The kernels of this build, in matching priority (a region is claimed by the first that matches).
///
/// **Not here, measured rather than assumed** (`tir-exec-bench --fused-kernels`, 3 threads):
///
/// * `a16_matmul`, the projection and its per-channel narrowing, is no faster fused than on the
///   generic kernels at any Qwen2.5-1.5B shape (0.92–1.03×). The dot product is the same NEON
///   kernel either way, and the narrowing is `O(rows)` beside `O(rows · k)`.
/// * `moe_combine_q36` at Qwen3.6-35B's `k = 8`, width 2048 is ~15 µs of vectorised passes on the
///   generic kernels against ~26 µs for a scalar fused lane loop. Under 1% of a token either way.
/// * A16's fused attention (`a16_attn_fused`) appears in no program this tree lowers or mirrors: the
///   mirror spells its softmax with the class's runtime `up`, and the lowerer writes library
///   `attention`.
///
/// None of the three has a fused form.
pub fn kernels_v1() -> &'static [&'static dyn FusedKernelV1] {
    &[&gdn_step::GdnStep, &rowops::L2UnitQ15, &rowops::RmsUnitQ24]
}

/// A matched region of one block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    /// Index into [`kernels_v1`].
    pub kernel: usize,
    pub bound: Bound,
    /// The template's output node, where the kernel runs.
    pub output: u16,
    /// Every program node of the region, output included, ascending.
    pub nodes: Vec<u16>,
    /// The operands, in the template's carry-in order.
    pub holes: Vec<Ref>,
    /// The program states the template reads or writes, in its order.
    pub states: Vec<u16>,
    /// The program state the region writes, if any.
    pub writes: Option<u16>,
}

fn ref_type(p: &TirProgramV1, bi: usize, r: Ref) -> Option<TensorType> {
    let b = &p.blocks[bi];
    Some(match r {
        Ref::Node(j) => b.nodes.get(j as usize)?.out.clone(),
        Ref::CarryIn(k) => b.carry_in.get(k as usize)?.clone(),
        Ref::Param(j) => {
            let d = p.params.get(j as usize)?;
            TensorType::fixed(d.dtype, &d.shape)
        }
        Ref::Const(j) => {
            let d = p.consts.get(j as usize)?;
            TensorType::fixed(d.dtype, &d.shape)
        }
        Ref::State(j) => {
            let d = p.states.get(j as usize)?;
            TensorType::fixed(d.dtype, &d.shape)
        }
        Ref::Input(_) => TensorType::scalar(DType::Idx),
    })
}

/// **Every region the kernels of this build match in `p`**, per block — structural, so it holds for
/// every occurrence of the block and every artifact; [`enable`] decides per occurrence which of them
/// run fused.
pub fn match_program(p: &TirProgramV1) -> Vec<Vec<Region>> {
    let kernels = kernels_v1();
    let skeletons: Vec<Vec<(Variant, pattern::Emitted)>> = kernels
        .iter()
        .map(|k| {
            k.variants()
                .into_iter()
                .filter_map(|v| {
                    let e = pattern::emit(*k, &v.probe, &v.states, &v.bound)?;
                    Some((v, e))
                })
                .collect()
        })
        .collect();
    (0..p.blocks.len()).map(|bi| match_block(p, bi, kernels, &skeletons)).collect()
}

fn match_block(
    p: &TirProgramV1,
    bi: usize,
    kernels: &[&dyn FusedKernelV1],
    skeletons: &[Vec<(Variant, pattern::Emitted)>],
) -> Vec<Region> {
    let block = &p.blocks[bi];
    let n = block.nodes.len();
    // Who reads each node, and which nodes someone outside the block sees.
    let mut consumers: Vec<Vec<u16>> = vec![Vec::new(); n];
    for (i, node) in block.nodes.iter().enumerate() {
        for r in &node.inputs {
            if let Ref::Node(j) = r {
                consumers[*j as usize].push(i as u16);
            }
        }
    }
    let mut observed = vec![false; n];
    for (i, node) in block.nodes.iter().enumerate() {
        observed[i] |= node.commit;
    }
    for c in &block.carry_out {
        observed[*c as usize] = true;
    }
    if bi == p.schedule.post as usize {
        observed[p.logits as usize] = true;
    }
    let mut taken = vec![false; n];
    let mut regions = Vec::new();
    for (ki, kernel) in kernels.iter().enumerate() {
        for anchor in (0..n as u16).rev() {
            if taken[anchor as usize] {
                continue;
            }
            for (variant, skeleton) in &skeletons[ki] {
                if skeleton.nodes[skeleton.output as usize].prim.tag() != block.nodes[anchor as usize].prim.tag() {
                    continue;
                }
                if let Some(region) = try_region(p, bi, ki, *kernel, variant, skeleton, anchor, &consumers, &observed, &taken) {
                    for node in &region.nodes {
                        taken[*node as usize] = true;
                    }
                    regions.push(region);
                    break;
                }
            }
        }
    }
    regions.sort_by_key(|r| r.output);
    regions
}

#[allow(clippy::too_many_arguments)]
fn try_region(
    p: &TirProgramV1,
    bi: usize,
    ki: usize,
    kernel: &dyn FusedKernelV1,
    variant: &Variant,
    skeleton: &pattern::Emitted,
    anchor: u16,
    consumers: &[Vec<u16>],
    observed: &[bool],
    taken: &[bool],
) -> Option<Region> {
    let block = &p.blocks[bi];
    let skeleton_match = pattern::unify(skeleton, variant.probe.len(), p, bi, anchor)?;
    let holes: Vec<Ref> = skeleton_match.holes.iter().map(|h| h.expect("unify binds every hole")).collect();
    let hole_types: Vec<TensorType> = holes.iter().map(|h| ref_type(p, bi, *h)).collect::<Option<_>>()?;
    // The program's own declarations of the states the template names, in the template's order.
    let states: Vec<u16> = (0..variant.states.len() as u16).map(|t| skeleton_match.states.get(&t).copied()).collect::<Option<_>>()?;
    let decls: Vec<StateDecl> = states.iter().map(|s| p.states.get(*s as usize).cloned()).collect::<Option<_>>()?;
    let bound = kernel.derive(&variant.bound, &hole_types, &block.nodes[anchor as usize])?;
    let exact = pattern::emit(kernel, &hole_types, &decls, &bound)?;
    let exact_match = pattern::unify(&exact, hole_types.len(), p, bi, anchor)?;
    if exact_match.holes.iter().map(|h| h.expect("bound")).collect::<Vec<_>>() != holes || !pattern::equal(&exact, &exact_match, p, bi)
    {
        return None;
    }
    let mut nodes: Vec<u16> = exact_match.map.values().copied().collect();
    nodes.sort_unstable();
    let inside = |j: u16| nodes.binary_search(&j).is_ok();
    for &node in &nodes {
        if taken[node as usize] {
            return None;
        }
        if node != anchor && (observed[node as usize] || consumers[node as usize].iter().any(|c| !inside(*c))) {
            return None;
        }
    }
    if holes.iter().any(|h| matches!(h, Ref::Node(j) if inside(*j))) {
        return None;
    }
    if !kernel.domain(&bound, &hole_types, &block.nodes[anchor as usize].out) {
        return None;
    }
    let writes = nodes.iter().find_map(|n| match block.nodes[*n as usize].prim {
        Prim::StateWrite { state } => Some(state),
        _ => None,
    });
    Some(Region { kernel: ki, bound, output: anchor, nodes, holes, states, writes })
}

/// What a node does in one fused occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Generic,
    /// Inside a region: computed by the region's kernel, never materialised.
    Skip,
    /// A region's output: the kernel runs here (the index into [`OccFused::regions`]).
    Run(u16),
}

/// The fused regions of one occurrence and each node's role.
#[derive(Clone, Debug)]
pub struct OccFused {
    pub roles: Vec<Role>,
    pub regions: Vec<Region>,
}

/// **The regions that run fused in one occurrence**: those whose every node is fail-free under the
/// occurrence's refined plan `bp` — no dtype check, no checked product, no operand check, and every
/// exact sum `Fast64` (the kernels accumulate in `i64`). `None` when none is.
pub fn enable(regions: &[Region], bp: &BlockPlan, program: &TirProgramV1) -> Option<OccFused> {
    let mut roles = vec![Role::Generic; bp.nodes.len()];
    let mut kept = Vec::new();
    for r in regions {
        let fail_free = r.nodes.iter().all(|&n| {
            let np = &bp.nodes[n as usize];
            !np.check_out
                && !np.checked_arith
                && !np.check_operand
                && (!matches!(np.prim, Prim::MatMul | Prim::ReduceSum { .. }) || np.acc == Acc::Fast64)
        });
        // The kernel writes a Fixed state only (a HistAppend has no fused form).
        let states_ok = r.states.iter().all(|s| matches!(program.states[*s as usize].kind, StateKind::Fixed { .. }));
        if !fail_free || !states_ok {
            continue;
        }
        let at = kept.len() as u16;
        for n in &r.nodes {
            roles[*n as usize] = Role::Skip;
        }
        roles[r.output as usize] = Role::Run(at);
        kept.push(r.clone());
    }
    (!kept.is_empty()).then_some(OccFused { roles, regions: kept })
}

/// The index of the kernel named `name` in [`kernels_v1`].
pub fn kernel_index(name: &str) -> Option<usize> {
    kernels_v1().iter().position(|k| k.name() == name)
}

// ---- shared shape checks -------------------------------------------------------------------------

/// A static shape, or `None` for one with `H`.
pub(crate) fn static_shape(t: &TensorType) -> Option<Vec<usize>> {
    t.shape.iter().map(|d| if let misaka_palw_tir::Dim::Fixed(n) = d { Some(*n as usize) } else { None }).collect()
}

// ---- shared arithmetic --------------------------------------------------------------------------

/// An operand's elements as `i128`, row-major.
pub(crate) fn values(op: &Opd<'_>) -> Vec<i128> {
    let mut out = Vec::with_capacity(op.numel());
    crate::with_slice!(op.data, s => op.layout.for_each_run(|at, len, st| {
        for t in 0..len {
            out.push(crate::elem::Elem::to_i128(s[at + t * st]));
        }
    }));
    out
}

/// `clamp(x, lo, hi)` in `i128`.
#[inline(always)]
pub(crate) fn clamp(x: i128, lo: i128, hi: i128) -> i128 {
    x.clamp(lo, hi)
}

/// **The A16 narrowing** `clamp_[lo,hi]( sat64( HAFZ(x·m / d) ) + z )` (`narrow_a16`), or without a
/// zero term `clamp_[lo,hi]( HAFZ(x·m / d) )` (`narrow`'s three-node form) — each lossy site on the
/// value the template computes it on.
#[inline(always)]
pub(crate) fn narrow(x: i128, m: i128, d: i128, z: Option<i128>, lo: i128, hi: i128) -> i128 {
    // The exact product and quotient in one machine word where they fit it (the same integers: the
    // rounding helpers agree at both widths), in `i128` otherwise.
    let q = match (i64::try_from(x), i64::try_from(m), i64::try_from(d)) {
        (Ok(x), Ok(m), Ok(d)) if x.checked_mul(m).is_some() => {
            crate::scalar::div_round_i64(x * m, d, misaka_palw_tir::Rounding::HalfAwayFromZero) as i128
        }
        _ => crate::scalar::div_round_i128(x * m, d, misaka_palw_tir::Rounding::HalfAwayFromZero),
    };
    match z {
        Some(z) => clamp(clamp(q, i64::MIN as i128, i64::MAX as i128) + z, lo, hi),
        None => clamp(q, lo, hi),
    }
}

/// `2^clamp(s, 0, 62)` — `pow2_of`: the gather from the pinned table by the clamped shift.
#[inline(always)]
pub(crate) fn pow2_of(s: i128) -> i128 {
    1i128 << s.clamp(0, 62)
}

/// Store `vals` into `out` as `store`, the output node's storage type (the plan proved they fit) —
/// after the deliberately broken variant's one-lane move when `fault`, kept inside `[lo, hi]`.
pub(crate) fn store(vals: &mut [i128], out: &mut Buf, store: DType, fault: bool, hi: i128) {
    if fault && let Some(v) = vals.first_mut() {
        *v = if *v < hi { *v + 1 } else { *v - 1 };
    }
    *out = Buf::from_i128s(store, vals);
}
