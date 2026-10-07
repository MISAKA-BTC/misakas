//! **The K2-TIR-v1 reference verifier and its public fault proof** (RFC-0011 §15.2–§15.5, RFC-0007 §V.2–§V.7).
//!
//! [`verify_scope_v1`] checks one **scope** of a claim — the whole claim, a set of directory segments, or an audit sample of
//! positions — in node order (inputs before the node that reads them). It opens what each instance reads through [`MaterialV1`],
//! **authenticates every opening against the commitment its wiring names**, and checks it:
//!
//! * `MatMul` over a weight (a rank-2 operand computed from params and consts alone) — **batched across the scope's tokens**
//!   (RFC-0007 §V.3): every position's rows (weight on the right, `X (W r) = Y r`) or columns (weight on the left, the lowering's
//!   GEMV, `(rᵀ W) X = rᵀ Y`) are one stacked product checked by one fresh public vector per repetition; the weight is projected once
//!   per scope, not once per token, and its commitment at every position is compared with the one projected;
//! * any other `MatMul` (activation × activation: `Q·Kᵀ`, `P·V`, gathered experts) — Freivalds per instance and batch slice;
//! * every other family — exact recompute from the authenticated inputs;
//! * every opened output — its declared dtype, shape and range.
//!
//! On a mismatch the failing row is recomputed exactly and the first wrong scalar becomes a [`FaultKindV1::MatMulScalar`] proof.
//! An opening that is not the committed value, or is not served, is [`ScopeVerdictV1::Unavailable`] — the DA path (ADR-0173 D3),
//! never a pass and never an arithmetic conviction.
//!
//! [`verify_fault_proof_v1`] is the terminal court: any node holding only the public evidence object, the trace commitments, the
//! param commitments and the job's tokens re-authenticates the proof's openings and recomputes one instance (one scalar for a
//! `MatMul`). No producer state, no seat secret, no vote.

use std::cell::RefCell;
use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::program::{Ref, TirProgramV1};
use misaka_palw_tir::{DType, Prim, Tensor};

use crate::challenge::{ChallengeBindingV1, ChallengeLabelV1, ChallengeStreamV1};
use crate::descriptor::KernelDescriptorV1;
use crate::evidence::{EvidenceHeaderV1, VerificationEvidenceV1, check_evidence_v1};
use crate::family::CheckerIdV1;
use crate::field::Fp;
use crate::hash::{Digest, finish, keyed};
use crate::plan::{PlanRelationV1, VerificationPlanV1, derived_error_bits};
use crate::trace::{EvidenceV1, ParamCommitmentsV1, SourceV1, WiringV1, const_tensor, eval_node, tensor_commitment};

pub const SCOPE_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/scope/v1";

/// Where the verifier reads opened values from (a provider, a peer, the producer — whoever serves).
pub trait MaterialV1 {
    fn node_value(&self, position: u32, occurrence: u16, node: u16) -> Option<Tensor>;
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor>;
}

/// The material of an honest trace and its artifact (tests, drills, a producer serving itself).
pub struct TraceMaterialV1<'a> {
    pub trace: &'a crate::trace::TraceV1,
    pub params: &'a misaka_palw_tir::MapParams,
}

impl MaterialV1 for TraceMaterialV1<'_> {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.trace.value(p, s, n).cloned()
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.params.tensors.get(&(index, layer)).cloned()
    }
}

/// Everything public a claim's verification and its court read.
pub struct ClaimContextV1<'a> {
    pub descriptor: &'a KernelDescriptorV1,
    pub program: &'a TirProgramV1,
    pub plan: &'a VerificationPlanV1,
    /// The committed node values' commitments.
    pub trace: &'a EvidenceV1,
    /// The §15.3 evidence object the claim committed.
    pub evidence: &'a VerificationEvidenceV1,
    /// What the claim's class and network say the header must be.
    pub header: EvidenceHeaderV1,
    pub params: &'a ParamCommitmentsV1,
    pub tokens: &'a [u32],
    /// The challenge binding; its evidence root must be `evidence.root()`.
    pub binding: ChallengeBindingV1,
}

/// What a check covers (RFC-0007 §V.6 `scope_kind`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ScopeV1 {
    WholeClaim,
    /// Directory segments, by index.
    Segments(Vec<u32>),
    /// A diagnostic sample of positions: never coverage (RFC-0007 §V.7, RFC-0011 §15.1).
    AuditOnly(Vec<u32>),
}

impl ScopeV1 {
    pub fn kind_code(&self) -> u8 {
        match self {
            Self::WholeClaim => 0,
            Self::Segments(_) => 1,
            Self::AuditOnly(_) => 2,
        }
    }

    /// The positions the scope checks, sorted and unique; `Err` for a segment or position outside the claim.
    pub fn positions(&self, ev: &VerificationEvidenceV1) -> Result<Vec<u32>, String> {
        let mut out: Vec<u32> = match self {
            Self::WholeClaim => (0..ev.positions).collect(),
            Self::Segments(ix) => {
                let mut v = Vec::new();
                for i in ix {
                    let seg = ev.segment(*i).ok_or(format!("no segment {i}"))?;
                    v.extend(seg.first..seg.end);
                }
                v
            }
            Self::AuditOnly(ps) => {
                if ps.iter().any(|p| *p >= ev.positions) {
                    return Err("an audit position outside the claim".into());
                }
                ps.clone()
            }
        };
        out.sort_unstable();
        out.dedup();
        if out.is_empty() {
            return Err("an empty scope".into());
        }
        Ok(out)
    }

    /// The segment indices a scope covers in full (none for an audit sample).
    pub fn segments(&self, ev: &VerificationEvidenceV1) -> Vec<u32> {
        match self {
            Self::WholeClaim => (0..ev.segments.len() as u32).collect(),
            Self::Segments(ix) => {
                let mut ix: Vec<u32> = ix.iter().copied().filter(|i| (*i as usize) < ev.segments.len()).collect();
                ix.sort_unstable();
                ix.dedup();
                ix
            }
            Self::AuditOnly(_) => Vec::new(),
        }
    }

    /// The root a receipt attests: the scope, the segments' boundary roots it covers, and the evidence it is about.
    pub fn root(&self, ev: &VerificationEvidenceV1) -> Digest {
        let mut s = keyed(SCOPE_ROOT_DOMAIN_V1);
        s.update(&ev.root());
        s.update(&[self.kind_code()]);
        match self {
            Self::WholeClaim => {
                s.update(&ev.initial_state_root).update(&ev.final_state_root);
            }
            Self::Segments(_) => {
                for i in self.segments(ev) {
                    s.update(&i.to_le_bytes());
                    if let Some(seg) = ev.segment(i) {
                        s.update(&seg.entry_state_root).update(&seg.exit_state_root);
                    }
                }
            }
            Self::AuditOnly(ps) => {
                let mut ps = ps.clone();
                ps.sort_unstable();
                ps.dedup();
                for p in ps {
                    s.update(&p.to_le_bytes());
                }
            }
        }
        finish(s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FaultKindV1 {
    /// The committed output violates its declared dtype, shape or range.
    Malformed,
    /// Recomputing the instance from its inputs gives another value (or no valid value).
    Recompute,
    /// One scalar of a `MatMul`: `Y[slice, i, j] ≠ Σ_k X[slice, i, k] · W[slice, k, j]`.
    MatMulScalar { slice: u64, i: u64, j: u64 },
}

/// **A fault localized to one primitive instance**, with every opening the court needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelFaultProofV1 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub kind: FaultKindV1,
    pub output: Tensor,
    pub inputs: Vec<Tensor>,
    pub prior_rows: Vec<Tensor>,
}

/// What a scope's check cost (RFC-0007 §V.8 asks for it, measured).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheckCostV1 {
    /// Field multiplications the probabilistic checks performed.
    pub field_mults: u128,
    /// Bytes of opened values (node values and params, each param instance once).
    pub opened_bytes: u128,
    /// Of which params (the artifact): read once per scope whatever the number of tokens.
    pub param_bytes: u128,
    /// Exact-recompute elements read.
    pub exact_elements: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeVerdictV1 {
    /// Every relation of the scope passed. `probabilistic_checks`: independent vector-equality checks (per relation instance or
    /// per scope batch, before repetitions); `error_bits`: the suite's conditional bound for them.
    Pass { scope_root: Digest, positions: u32, probabilistic_checks: u128, error_bits: u16, cost: CheckCostV1 },
    /// A fault, localized, with its proof.
    Fault(Box<KernelFaultProofV1>),
    /// Something the scope reads was not served, or what was served is not what was committed. DA path.
    Unavailable { what: String },
    /// The evidence is not a claim of this program (object, shape, tokens, output, challenge binding).
    EvidenceMalformed { why: String },
    /// The verifier contradicted itself (a bug, never a verdict on the producer).
    Inconsistent { why: String },
}

/// A whole-claim verdict.
pub type ClaimVerdictV1 = ScopeVerdictV1;

/// Why a fault proof was dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DismissalV1 {
    /// An opening does not match its commitment, or the proof is about no instance of this claim.
    NotAuthentic(String),
    /// The instance recomputes to the committed value: there is no fault.
    NoFault,
}

/// What a conviction names (the instance; slashing is the lifecycle's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvictionV1 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub kind: FaultKindV1,
}

struct Ctx<'a> {
    c: &'a ClaimContextV1<'a>,
    w: WiringV1<'a>,
    param_cache: RefCell<BTreeMap<(u16, Option<u16>), Tensor>>,
    cost: RefCell<CheckCostV1>,
}

impl Ctx<'_> {
    /// Is `t` the value `src` names? Public sources are recomputed.
    fn authentic(&self, src: &SourceV1, t: &Tensor) -> Result<bool, String> {
        Ok(match src {
            SourceV1::Const(j) => const_tensor(self.c.program, *j).map(|c| c == *t).unwrap_or(false),
            SourceV1::Param { index, layer } => {
                *self.c.params.by_instance.get(&(*index, *layer)).ok_or(format!("no commitment for param {index} layer {layer:?}"))?
                    == tensor_commitment(t)
            }
            _ => self.w.source_commitment(self.c.trace, src).ok_or("a node outside the evidence")? == tensor_commitment(t),
        })
    }

    fn open(&self, m: &dyn MaterialV1, src: &SourceV1) -> Result<Tensor, ScopeVerdictV1> {
        if let SourceV1::Param { index, layer } = src
            && let Some(t) = self.param_cache.borrow().get(&(*index, *layer))
        {
            return Ok(t.clone());
        }
        let t = match src {
            SourceV1::Node { position, occurrence, node } => m.node_value(*position, *occurrence, *node),
            SourceV1::Param { index, layer } => m.param(*index, *layer),
            SourceV1::Const(j) => const_tensor(self.c.program, *j).ok(),
            SourceV1::Zeros { dtype, shape } => Some(Tensor::zeros(*dtype, shape)),
            SourceV1::Public(v) => Tensor::scalar(DType::Idx, *v as i128).ok(),
        }
        .ok_or_else(|| ScopeVerdictV1::Unavailable { what: format!("{src:?} was not served") })?;
        match self.authentic(src, &t) {
            Ok(true) => {
                if matches!(src, SourceV1::Node { .. } | SourceV1::Param { .. }) {
                    self.cost.borrow_mut().opened_bytes += (t.len() * t.dtype.width()) as u128;
                }
                if let SourceV1::Param { index, layer } = src {
                    self.cost.borrow_mut().param_bytes += (t.len() * t.dtype.width()) as u128;
                    self.param_cache.borrow_mut().insert((*index, *layer), t.clone());
                }
                Ok(t)
            }
            Ok(false) => Err(ScopeVerdictV1::Unavailable { what: format!("{src:?}: the served value is not the committed one") }),
            Err(why) => Err(ScopeVerdictV1::EvidenceMalformed { why }),
        }
    }

    fn well_typed(&self, s: u16, p: u32, n: u16, t: &Tensor) -> bool {
        let node = self.w.node(s, n);
        t.dtype == node.out.dtype
            && t.shape == node.out.resolve(self.w.h(s, p))
            && t.data.len() == t.shape.iter().product::<usize>()
            && t.data.iter().all(|v| t.dtype.contains(*v))
    }
}

/// The start of a batch slice's matrix inside an operand of shape `in_shape`, for an output whose
/// batch dims are `out_batch` (numpy broadcasting, right-aligned).
fn slice_offset(out_batch: &[usize], in_shape: &[usize], slice: usize) -> usize {
    let r = in_shape.len();
    let in_batch = &in_shape[..r - 2];
    let mut rem = slice;
    let mut mi = vec![0usize; out_batch.len()];
    for d in (0..out_batch.len()).rev() {
        mi[d] = rem % out_batch[d];
        rem /= out_batch[d];
    }
    let off = out_batch.len() - in_batch.len();
    let mut flat = 0usize;
    for (i, &dim) in in_batch.iter().enumerate() {
        flat = flat * dim + if dim == 1 { 0 } else { mi[off + i] };
    }
    flat * in_shape[r - 2] * in_shape[r - 1]
}

/// The exact value of `Y[slice, i, j]`.
fn matmul_scalar(x: &Tensor, wt: &Tensor, y_shape: &[usize], slice: usize, i: usize, j: usize) -> i128 {
    let out_batch = &y_shape[..y_shape.len() - 2];
    let (xo, wo) = (slice_offset(out_batch, &x.shape, slice), slice_offset(out_batch, &wt.shape, slice));
    let k = x.shape[x.shape.len() - 1];
    let n = wt.shape[wt.shape.len() - 1];
    (0..k).fold(0i128, |acc, kk| acc.wrapping_add(x.data[xo + i * k + kk].wrapping_mul(wt.data[wo + kk * n + j])))
}

/// The first wrong scalar of row `i` of slice `b`, exactly.
fn localize_row(x: &Tensor, wt: &Tensor, y: &Tensor, b: usize, i: usize) -> Option<(u64, u64, u64)> {
    let r = y.shape.len();
    let (m, nn) = (y.shape[r - 2], y.shape[r - 1]);
    (0..nn)
        .find(|&j| matmul_scalar(x, wt, &y.shape, b, i, j) != y.data[b * m * nn + i * nn + j])
        .map(|j| (b as u64, i as u64, j as u64))
}

/// `X (W r)` against `Y r` for one slice, row by row; the first row where they differ.
fn first_bad_row(x: &Tensor, y: &Tensor, b: usize, wr: &[Fp], r: &[Fp], cost: &mut CheckCostV1) -> Option<usize> {
    let yr = y.shape.len();
    let (m, nn) = (y.shape[yr - 2], y.shape[yr - 1]);
    let k = x.shape[x.shape.len() - 1];
    let out_batch = &y.shape[..yr - 2];
    let (xo, yo) = (slice_offset(out_batch, &x.shape, b), b * m * nn);
    cost.field_mults += (m * (k + nn)) as u128;
    (0..m).find(|&i| {
        let lhs = Fp::dot((0..k).map(|kk| Fp::from_i128(x.data[xo + i * k + kk])), wr.iter().copied());
        let rhs = Fp::dot((0..nn).map(|j| Fp::from_i128(y.data[yo + i * nn + j])), r.iter().copied());
        lhs != rhs
    })
}

/// `W r` for the `[k, nn]` matrix starting at `wo`.
fn project(wt: &Tensor, wo: usize, k: usize, nn: usize, r: &[Fp], cost: &mut CheckCostV1) -> Vec<Fp> {
    cost.field_mults += (k * nn) as u128;
    (0..k).map(|kk| Fp::dot((0..nn).map(|j| Fp::from_i128(wt.data[wo + kk * nn + j])), r.iter().copied())).collect()
}

/// `(rᵀ W) X` against `rᵀ Y` column by column (`W` `[m, k]`, `X` `[k, nn]`, `Y` `[m, nn]`); the first column where they differ.
fn first_bad_column(wt: &Tensor, x: &Tensor, y: &Tensor, rw: &[Fp], r: &[Fp], cost: &mut CheckCostV1) -> Option<usize> {
    let (m, k) = (wt.shape[0], wt.shape[1]);
    let nn = y.shape[1];
    cost.field_mults += (nn * (k + m)) as u128;
    (0..nn).find(|&j| {
        let lhs = Fp::dot((0..k).map(|kk| Fp::from_i128(x.data[kk * nn + j])), rw.iter().copied());
        let rhs = Fp::dot((0..m).map(|i| Fp::from_i128(y.data[i * nn + j])), r.iter().copied());
        lhs != rhs
    })
}

/// Freivalds per instance and batch slice (activation × activation products).
#[allow(clippy::too_many_arguments)]
fn freivalds_instance(
    seed: &Digest,
    (p, s, n): (u32, u16, u16),
    reps: u8,
    x: &Tensor,
    wt: &Tensor,
    y: &Tensor,
    checks: &mut u128,
    cost: &mut CheckCostV1,
) -> Result<Option<(u64, u64, u64)>, String> {
    let yr = y.shape.len();
    let nn = y.shape[yr - 1];
    let k = x.shape[x.shape.len() - 1];
    let out_batch = &y.shape[..yr - 2];
    let slices: usize = out_batch.iter().product();
    *checks += slices as u128;
    for rep in 0..reps {
        for b in 0..slices {
            let label = ChallengeLabelV1 { kind: 0, position: p, occurrence: s, node: n, repetition: rep, slice: b as u32 };
            let r = ChallengeStreamV1::new(*seed, label).vector(nn);
            let wr = project(wt, slice_offset(out_batch, &wt.shape, b), k, nn, &r, cost);
            if let Some(i) = first_bad_row(x, y, b, &wr, &r, cost) {
                return localize_row(x, wt, y, b, i)
                    .map(Some)
                    .ok_or_else(|| format!("Freivalds failed on row {i} of slice {b} but the row recomputes exactly"));
            }
        }
    }
    Ok(None)
}

/// Is node `j` of `block` position-independent — computed from params and consts alone (a weight after its casts and reshapes)?
fn is_static(program: &TirProgramV1, block: usize, j: u16, memo: &mut BTreeMap<u16, bool>) -> bool {
    if let Some(v) = memo.get(&j) {
        return *v;
    }
    let node = &program.blocks[block].nodes[j as usize];
    let v = !matches!(node.prim, Prim::StateWrite { .. } | Prim::HistAppend { .. } | Prim::Iota { .. })
        && node.inputs.iter().all(|r| match r {
            Ref::Param(_) | Ref::Const(_) => true,
            Ref::Node(i) => is_static(program, block, *i, memo),
            _ => false,
        });
    memo.insert(j, v);
    v
}

/// Which operand of a weight `MatMul` is the weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WeightSide {
    /// `Y = X · W`, `W` a static `[K, N]`: checked as `X (W r) = Y r`.
    Right,
    /// `Y = W · X`, `W` a static `[M, K]`, `X` and `Y` rank 2 (the weight-on-the-left GEMV the lowering emits): checked as
    /// `(rᵀ W) X = rᵀ Y`.
    Left,
}

fn static_rank2(program: &TirProgramV1, block: usize, r: Option<&Ref>) -> bool {
    match r {
        Some(Ref::Param(j)) => program.params[*j as usize].shape.len() == 2,
        Some(Ref::Node(j)) => {
            program.blocks[block].nodes[*j as usize].out.shape.len() == 2 && is_static(program, block, *j, &mut BTreeMap::new())
        }
        _ => false,
    }
}

/// A weight `MatMul`: one operand is a rank-2 value that does not depend on the position (a param, or a node computed from params
/// and consts alone), so every token of a scope multiplies the same matrix.
fn weight_side(program: &TirProgramV1, block: usize, node: &misaka_palw_tir::program::Node) -> Option<WeightSide> {
    if !matches!(node.prim, Prim::MatMul) {
        return None;
    }
    if static_rank2(program, block, node.inputs.get(1)) {
        return Some(WeightSide::Right);
    }
    if static_rank2(program, block, node.inputs.first())
        && node.out.shape.len() == 2
        && !static_rank2(program, block, node.inputs.get(1))
    {
        return Some(WeightSide::Left);
    }
    None
}

/// The scope-wide projections of the batched weight products: `(occurrence, node, repetition) → (r, W r, commitment of W)`.
type BatchProjections = BTreeMap<(u16, u16, u8), (Vec<Fp>, Vec<Fp>, Digest)>;

/// **Verify one scope of a claim.**
pub fn verify_scope_v1(c: &ClaimContextV1<'_>, material: &dyn MaterialV1, scope: &ScopeV1) -> ScopeVerdictV1 {
    let w = match WiringV1::new(c.program) {
        Ok(w) => w,
        Err(e) => return ScopeVerdictV1::EvidenceMalformed { why: format!("the program does not validate: {e}") },
    };
    let ctx = Ctx { c, w, param_cache: RefCell::new(BTreeMap::new()), cost: RefCell::new(CheckCostV1::default()) };
    if let Err(why) = shape_and_binding(&ctx) {
        return ScopeVerdictV1::EvidenceMalformed { why };
    }
    let positions = match scope.positions(c.evidence) {
        Ok(p) => p,
        Err(why) => return ScopeVerdictV1::EvidenceMalformed { why },
    };
    let rels: BTreeMap<(u8, u16), &PlanRelationV1> = c.plan.relations.iter().map(|r| ((r.block, r.node), r)).collect();
    let seed = c.binding.seed();
    let mut checks = 0u128;
    let mut batch: BatchProjections = BTreeMap::new();
    // The batch label is the scope's own: two scopes of one claim draw different vectors.
    let scope_label = positions[0];
    for &p in &positions {
        for s in 0..ctx.w.occurrences.len() as u16 {
            let block = ctx.w.occurrences[s as usize].0;
            for n in 0..c.program.blocks[block as usize].nodes.len() as u16 {
                let Some(rel) = rels.get(&(block, n)) else {
                    return ScopeVerdictV1::EvidenceMalformed { why: format!("block {block} node {n} has no relation in the plan") };
                };
                if let Err(v) = check_instance(&ctx, material, rel, &seed, (p, s, n), scope_label, &mut batch, &mut checks) {
                    return v;
                }
            }
        }
    }
    let cost = *ctx.cost.borrow();
    ScopeVerdictV1::Pass {
        scope_root: scope.root(c.evidence),
        positions: positions.len() as u32,
        probabilistic_checks: checks,
        error_bits: derived_error_bits(c.descriptor, checks),
        cost,
    }
}

/// **Verify a whole claim** (the [`ScopeV1::WholeClaim`] scope).
pub fn verify_claim_v1(c: &ClaimContextV1<'_>, material: &dyn MaterialV1) -> ScopeVerdictV1 {
    verify_scope_v1(c, material, &ScopeV1::WholeClaim)
}

fn shape_and_binding(ctx: &Ctx<'_>) -> Result<(), String> {
    let c = ctx.c;
    let b = &c.binding;
    if b.evidence_root != c.evidence.root()
        || b.plan_root != c.plan.root()
        || b.class_binding_id != c.header.class_binding_id
        || b.network_domain != c.header.network_domain
    {
        return Err("the challenge is not bound to this evidence, plan, class and network (bind first, then draw)".into());
    }
    if c.header.plan_root != c.plan.root() {
        return Err("the claim's header names another plan".into());
    }
    if c.tokens.is_empty() || c.tokens.len() as u64 > c.plan.max_positions as u64 {
        return Err(format!("{} positions against the plan's {}", c.tokens.len(), c.plan.max_positions));
    }
    for (p, pos) in c.trace.commitments.iter().enumerate() {
        if pos.len() != ctx.w.occurrences.len() {
            return Err(format!("position {p}: {} occurrences, the schedule has {}", pos.len(), ctx.w.occurrences.len()));
        }
        for (s, occ) in pos.iter().enumerate() {
            if occ.len() != c.program.blocks[ctx.w.occurrences[s].0 as usize].nodes.len() {
                return Err(format!("position {p} occurrence {s}: node count"));
            }
        }
    }
    check_evidence_v1(&ctx.w, c.trace, c.tokens, c.evidence, c.descriptor, &c.header)
}

#[allow(clippy::too_many_arguments)]
fn check_instance(
    ctx: &Ctx<'_>,
    m: &dyn MaterialV1,
    rel: &PlanRelationV1,
    seed: &Digest,
    (p, s, n): (u32, u16, u16),
    scope_label: u32,
    batch: &mut BatchProjections,
    checks: &mut u128,
) -> Result<(), ScopeVerdictV1> {
    let node = ctx.w.node(s, n);
    let tokens = ctx.c.tokens;
    let src = |i: usize| ctx.w.input_source(tokens, p, s, n, i).map_err(|e| ScopeVerdictV1::EvidenceMalformed { why: e.to_string() });
    let output = ctx.open(m, &SourceV1::Node { position: p, occurrence: s, node: n })?;
    let inputs = (0..node.inputs.len()).map(|i| ctx.open(m, &src(i)?)).collect::<Result<Vec<_>, _>>()?;
    let prior = ctx
        .w
        .hist_prior_sources(tokens, p, s, n)
        .map_err(|e| ScopeVerdictV1::EvidenceMalformed { why: e.to_string() })?
        .iter()
        .map(|src| ctx.open(m, src))
        .collect::<Result<Vec<_>, _>>()?;
    let fault = |kind| {
        ScopeVerdictV1::Fault(Box::new(KernelFaultProofV1 {
            position: p,
            occurrence: s,
            node: n,
            kind,
            output: output.clone(),
            inputs: inputs.clone(),
            prior_rows: prior.clone(),
        }))
    };
    if !ctx.well_typed(s, p, n, &output) {
        return Err(fault(FaultKindV1::Malformed));
    }
    // A weight product whose weight at this position is the one the scope projected (or the first one seen) is batched.
    let batch_side = weight_side(ctx.c.program, ctx.w.occurrences[s as usize].0 as usize, node)
        .filter(|_| rel.checker == CheckerIdV1::FreivaldsM127)
        .filter(|side| {
            let w = &inputs[if *side == WeightSide::Right { 1 } else { 0 }];
            batch.get(&(s, n, 0)).is_none_or(|(_, _, wc)| *wc == tensor_commitment(w))
        });
    let bad_row = match rel.checker {
        CheckerIdV1::FreivaldsM127 if batch_side.is_some() => {
            let side = batch_side.expect("guarded");
            // The same weight at every position of the scope (its commitment is compared, never assumed): one stacked check per
            // repetition, the weight projected once.
            let mut cost = ctx.cost.borrow_mut();
            let mut found = None;
            for rep in 0..rel.repetitions {
                let label = ChallengeLabelV1 { kind: 1, position: scope_label, occurrence: s, node: n, repetition: rep, slice: 0 };
                let fresh = !batch.contains_key(&(s, n, rep));
                if fresh && rep == 0 {
                    *checks += 1;
                }
                found = match side {
                    WeightSide::Right => {
                        let (x, wt, y) = (&inputs[0], &inputs[1], &output);
                        let (k, nn) = (wt.shape[0], wt.shape[1]);
                        let (r, wr, _) = batch.entry((s, n, rep)).or_insert_with(|| {
                            let r = ChallengeStreamV1::new(*seed, label).vector(nn);
                            let wr = project(wt, 0, k, nn, &r, &mut cost);
                            (r, wr, tensor_commitment(wt))
                        });
                        let slices: usize = y.shape[..y.shape.len() - 2].iter().product();
                        (0..slices).find_map(|b| first_bad_row(x, y, b, wr, r, &mut cost).map(|i| (b, i)))
                    }
                    WeightSide::Left => {
                        let (wt, x, y) = (&inputs[0], &inputs[1], &output);
                        let (m, k) = (wt.shape[0], wt.shape[1]);
                        let (r, rw, _) = batch.entry((s, n, rep)).or_insert_with(|| {
                            let r = ChallengeStreamV1::new(*seed, label).vector(m);
                            cost.field_mults += (m * k) as u128;
                            let rw = (0..k)
                                .map(|kk| Fp::dot((0..m).map(|i| Fp::from_i128(wt.data[i * k + kk])), r.iter().copied()))
                                .collect();
                            (r, rw, tensor_commitment(wt))
                        });
                        first_bad_column(wt, x, y, rw, r, &mut cost).map(|j| (usize::MAX, j))
                    }
                };
                if found.is_some() {
                    break;
                }
            }
            drop(cost);
            match found {
                None => return Ok(()),
                // A left-weight column: the wrong scalar is found by recomputing that column exactly.
                Some((usize::MAX, j)) => {
                    let (wt, x, y) = (&inputs[0], &inputs[1], &output);
                    let (m, k, nn) = (wt.shape[0], wt.shape[1], y.shape[1]);
                    let bad = (0..m).find(|&i| {
                        (0..k).fold(0i128, |a, kk| a.wrapping_add(wt.data[i * k + kk].wrapping_mul(x.data[kk * nn + j])))
                            != y.data[i * nn + j]
                    });
                    return match bad {
                        Some(i) => Err(fault(FaultKindV1::MatMulScalar { slice: 0, i: i as u64, j: j as u64 })),
                        None => Err(ScopeVerdictV1::Inconsistent {
                            why: format!("batched Freivalds failed at column {j} which recomputes exactly"),
                        }),
                    };
                }
                Some(bi) => Some(bi),
            }
        }
        CheckerIdV1::FreivaldsM127 if matches!(node.prim, Prim::MatMul) => {
            let mut cost = ctx.cost.borrow_mut();
            let r = freivalds_instance(seed, (p, s, n), rel.repetitions, &inputs[0], &inputs[1], &output, checks, &mut cost);
            drop(cost);
            return match r {
                Ok(None) => Ok(()),
                Ok(Some((slice, i, j))) => Err(fault(FaultKindV1::MatMulScalar { slice, i, j })),
                Err(why) => Err(ScopeVerdictV1::Inconsistent { why }),
            };
        }
        CheckerIdV1::FreivaldsM127 => {
            return Err(ScopeVerdictV1::Inconsistent { why: "a Freivalds relation on a node that is not a MatMul".into() });
        }
        CheckerIdV1::ExactRecompute | CheckerIdV1::StateContinuity => {
            ctx.cost.borrow_mut().exact_elements += (output.len() + inputs.iter().map(Tensor::len).sum::<usize>()) as u128;
            return match eval_node(ctx.c.program, node, &inputs, &prior, ctx.w.h(s, p)) {
                Ok(v) if v == output => Ok(()),
                _ => Err(fault(FaultKindV1::Recompute)),
            };
        }
    };
    match bad_row {
        None => Ok(()),
        Some((b, i)) => match localize_row(&inputs[0], &inputs[1], &output, b, i) {
            Some((slice, i, j)) => Err(fault(FaultKindV1::MatMulScalar { slice, i, j })),
            None => Err(ScopeVerdictV1::Inconsistent { why: format!("batched Freivalds failed at row {i} which recomputes exactly") }),
        },
    }
}

/// **The terminal court**: re-authenticate the proof's openings from public material and recompute.
pub fn verify_fault_proof_v1(c: &ClaimContextV1<'_>, proof: &KernelFaultProofV1) -> Result<ConvictionV1, DismissalV1> {
    let w = WiringV1::new(c.program).map_err(|e| DismissalV1::NotAuthentic(format!("the program does not validate: {e}")))?;
    let ctx = Ctx { c, w, param_cache: RefCell::new(BTreeMap::new()), cost: RefCell::new(CheckCostV1::default()) };
    shape_and_binding(&ctx).map_err(DismissalV1::NotAuthentic)?;
    let (p, s, n) = (proof.position, proof.occurrence, proof.node);
    if p as usize >= c.tokens.len()
        || s as usize >= ctx.w.occurrences.len()
        || n as usize >= c.program.blocks[ctx.w.occurrences[s as usize].0 as usize].nodes.len()
    {
        return Err(DismissalV1::NotAuthentic("the proof names no instance of this claim".into()));
    }
    let node = ctx.w.node(s, n);
    let na = DismissalV1::NotAuthentic;
    let check = |src: &SourceV1, t: &Tensor, what: &str| -> Result<(), DismissalV1> {
        match ctx.authentic(src, t) {
            Ok(true) => Ok(()),
            Ok(false) => Err(na(format!("{what} is not the committed value"))),
            Err(e) => Err(na(e)),
        }
    };
    check(&SourceV1::Node { position: p, occurrence: s, node: n }, &proof.output, "the output")?;
    if proof.inputs.len() != node.inputs.len() {
        return Err(na("the proof opens another number of inputs".into()));
    }
    for (i, t) in proof.inputs.iter().enumerate() {
        check(&ctx.w.input_source(c.tokens, p, s, n, i).map_err(|e| na(e.to_string()))?, t, &format!("input {i}"))?;
    }
    let prior = ctx.w.hist_prior_sources(c.tokens, p, s, n).map_err(|e| na(e.to_string()))?;
    if prior.len() != proof.prior_rows.len() {
        return Err(na("the proof opens another number of history rows".into()));
    }
    for (src, t) in prior.iter().zip(&proof.prior_rows) {
        check(src, t, "a history row")?;
    }
    let convicted = ConvictionV1 { position: p, occurrence: s, node: n, kind: proof.kind.clone() };
    if !ctx.well_typed(s, p, n, &proof.output) {
        // Whatever kind the filer named, an opened committed value outside its type convicts.
        return Ok(ConvictionV1 { kind: FaultKindV1::Malformed, ..convicted });
    }
    match proof.kind {
        FaultKindV1::Malformed => Err(DismissalV1::NoFault),
        FaultKindV1::Recompute => match eval_node(c.program, node, &proof.inputs, &proof.prior_rows, ctx.w.h(s, p)) {
            Ok(v) if v == proof.output => Err(DismissalV1::NoFault),
            _ => Ok(convicted),
        },
        FaultKindV1::MatMulScalar { slice, i, j } => {
            if !matches!(node.prim, Prim::MatMul) {
                return Err(na("a MatMul scalar fault on another primitive".into()));
            }
            let y = &proof.output;
            let r = y.shape.len();
            let (m, nn) = (y.shape[r - 2], y.shape[r - 1]);
            let slices: usize = y.shape[..r - 2].iter().product();
            if slice as usize >= slices || i as usize >= m || j as usize >= nn {
                return Err(na("the scalar is outside the product".into()));
            }
            let claimed = y.data[slice as usize * m * nn + i as usize * nn + j as usize];
            if matmul_scalar(&proof.inputs[0], &proof.inputs[1], &y.shape, slice as usize, i as usize, j as usize) == claimed {
                Err(DismissalV1::NoFault)
            } else {
                Ok(convicted)
            }
        }
    }
}
