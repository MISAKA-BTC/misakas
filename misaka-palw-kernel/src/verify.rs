//! **The K2-TIR-v1 reference verifier and its public fault proof.**
//!
//! [`verify_claim_v1`] walks every relation instance of a claim in node order (inputs before the node
//! that reads them), opens what the instance reads through [`MaterialV1`], **authenticates every
//! opening against the commitment its wiring names**, and checks it:
//!
//! * `MatMul` — Freivalds over GF(2^127 − 1): `X(W r) = Y r` for a post-commit public `r`, every batch
//!   slice, `t` repetitions. On a mismatch the failing row is recomputed exactly and the first wrong
//!   scalar becomes a [`FaultKindV1::MatMulScalar`] proof;
//! * every other family — exact recompute of the instance from its authenticated inputs;
//! * every opened output — its declared dtype, shape and range.
//!
//! An opening that does not match its commitment is [`ClaimVerdictV1::Unavailable`]: the material
//! served is not the material committed, which is the DA path (ADR-0173 D3) — **never** a pass and
//! never an arithmetic conviction.
//!
//! [`verify_fault_proof_v1`] is the terminal court: any node, holding only the public evidence,
//! the param commitments and the job's tokens, re-authenticates the proof's openings and recomputes
//! one instance (one scalar for a `MatMul`). It needs no producer state, no seat secret and no vote.

use std::collections::BTreeMap;

use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, Prim, Tensor};

use crate::challenge::{ChallengeBindingV1, ChallengeLabelV1, ChallengeStreamV1};
use crate::descriptor::KernelDescriptorV1;
use crate::family::CheckerIdV1;
use crate::field::Fp;
use crate::hash::Digest;
use crate::plan::{PlanRelationV1, VerificationPlanV1, derived_error_bits};
use crate::trace::{EvidenceV1, ParamCommitmentsV1, SourceV1, WiringV1, const_tensor, eval_node, tensor_commitment};

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
    pub evidence: &'a EvidenceV1,
    pub params: &'a ParamCommitmentsV1,
    pub tokens: &'a [u32],
    /// The output root the claim committed.
    pub claimed_output_root: Digest,
    /// The challenge binding (its evidence and plan roots must be this claim's).
    pub binding: ChallengeBindingV1,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimVerdictV1 {
    /// Every relation passed. `error_bits`: the suite's conditional bound for this claim's instance count.
    Pass { probabilistic_instances: u128, error_bits: u16 },
    /// A fault, localized, with its proof.
    Fault(Box<KernelFaultProofV1>),
    /// Something the claim reads was not served, or what was served is not what was committed. DA path.
    Unavailable { what: String },
    /// The evidence is not a claim of this program (shape, tokens, output root, challenge binding).
    EvidenceMalformed { why: String },
    /// The verifier contradicted itself (a bug, never a verdict on the producer).
    Inconsistent { why: String },
}

/// Why a fault proof was dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DismissalV1 {
    /// An opening does not match its commitment, or the proof is about no instance of this claim.
    NotAuthentic(String),
    /// The instance recomputes to the committed value: there is no fault.
    NoFault,
}

/// What a conviction names (the instance; slashing is the caller's lifecycle).
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
}

impl Ctx<'_> {
    fn commitment(&self, src: &SourceV1) -> Result<Option<Digest>, String> {
        Ok(match src {
            SourceV1::Node { position, occurrence, node } => {
                Some(*self.c.evidence.at(*position, *occurrence, *node).ok_or("a node outside the evidence")?)
            }
            SourceV1::Param { index, layer } => Some(
                *self.c.params.by_instance.get(&(*index, *layer)).ok_or(format!("no commitment for param {index} layer {layer:?}"))?,
            ),
            _ => None,
        })
    }

    /// Is `t` the value `src` names? Public sources are recomputed.
    fn authentic(&self, src: &SourceV1, t: &Tensor) -> Result<bool, String> {
        Ok(match src {
            SourceV1::Const(j) => const_tensor(self.c.program, *j).map(|c| c == *t).unwrap_or(false),
            SourceV1::Zeros { dtype, shape } => *t == Tensor::zeros(*dtype, shape),
            SourceV1::Public(v) => Tensor::scalar(DType::Idx, *v as i128).map(|c| c == *t).unwrap_or(false),
            _ => self.commitment(src)?.is_some_and(|d| d == tensor_commitment(t)),
        })
    }

    fn open(&self, m: &dyn MaterialV1, src: &SourceV1) -> Result<Tensor, ClaimVerdictV1> {
        let t = match src {
            SourceV1::Node { position, occurrence, node } => m.node_value(*position, *occurrence, *node),
            SourceV1::Param { index, layer } => m.param(*index, *layer),
            SourceV1::Const(j) => const_tensor(self.c.program, *j).ok(),
            SourceV1::Zeros { dtype, shape } => Some(Tensor::zeros(*dtype, shape)),
            SourceV1::Public(v) => Tensor::scalar(DType::Idx, *v as i128).ok(),
        }
        .ok_or_else(|| ClaimVerdictV1::Unavailable { what: format!("{src:?} was not served") })?;
        match self.authentic(src, &t) {
            Ok(true) => Ok(t),
            Ok(false) => Err(ClaimVerdictV1::Unavailable { what: format!("{src:?}: the served value is not the committed one") }),
            Err(why) => Err(ClaimVerdictV1::EvidenceMalformed { why }),
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

struct MatView<'t> {
    t: &'t Tensor,
}

impl MatView<'_> {
    fn dims(&self) -> (usize, usize) {
        let r = self.t.shape.len();
        (self.t.shape[r - 2], self.t.shape[r - 1])
    }
}

/// The exact value of `Y[slice, i, j]`.
fn matmul_scalar(x: &Tensor, wt: &Tensor, y_shape: &[usize], slice: usize, i: usize, j: usize) -> i128 {
    let out_batch = &y_shape[..y_shape.len() - 2];
    let (xo, wo) = (slice_offset(out_batch, &x.shape, slice), slice_offset(out_batch, &wt.shape, slice));
    let (_, k) = MatView { t: x }.dims();
    let (_, n) = MatView { t: wt }.dims();
    (0..k).fold(0i128, |acc, kk| acc.wrapping_add(x.data[xo + i * k + kk].wrapping_mul(wt.data[wo + kk * n + j])))
}

/// Freivalds over every slice and repetition; `Some((slice, i, j))` names the first wrong scalar.
fn freivalds(
    seed: &Digest,
    (p, s, n): (u32, u16, u16),
    reps: u8,
    x: &Tensor,
    wt: &Tensor,
    y: &Tensor,
    instances: &mut u128,
) -> Result<Option<(u64, u64, u64)>, String> {
    let yr = y.shape.len();
    let (m, nn) = (y.shape[yr - 2], y.shape[yr - 1]);
    let k = x.shape[x.shape.len() - 1];
    let out_batch = &y.shape[..yr - 2];
    let slices: usize = out_batch.iter().product();
    for rep in 0..reps {
        for b in 0..slices {
            *instances += 1;
            let label = ChallengeLabelV1 { position: p, occurrence: s, node: n, repetition: rep, slice: b as u32 };
            let r = ChallengeStreamV1::new(*seed, label).vector(nn);
            let (xo, wo, yo) = (slice_offset(out_batch, &x.shape, b), slice_offset(out_batch, &wt.shape, b), b * m * nn);
            let wr: Vec<Fp> =
                (0..k).map(|kk| Fp::dot((0..nn).map(|j| Fp::from_i128(wt.data[wo + kk * nn + j])), r.iter().copied())).collect();
            for i in 0..m {
                let lhs = Fp::dot((0..k).map(|kk| Fp::from_i128(x.data[xo + i * k + kk])), wr.iter().copied());
                let rhs = Fp::dot((0..nn).map(|j| Fp::from_i128(y.data[yo + i * nn + j])), r.iter().copied());
                if lhs != rhs {
                    // Localize: the row recomputed exactly.
                    for j in 0..nn {
                        if matmul_scalar(x, wt, &y.shape, b, i, j) != y.data[yo + i * nn + j] {
                            return Ok(Some((b as u64, i as u64, j as u64)));
                        }
                    }
                    return Err(format!("Freivalds failed on row {i} of slice {b} but the row recomputes exactly"));
                }
            }
        }
    }
    Ok(None)
}

/// **Verify a claim.**
pub fn verify_claim_v1(c: &ClaimContextV1<'_>, material: &dyn MaterialV1) -> ClaimVerdictV1 {
    let w = match WiringV1::new(c.program) {
        Ok(w) => w,
        Err(e) => return ClaimVerdictV1::EvidenceMalformed { why: format!("the program does not validate: {e}") },
    };
    let ctx = Ctx { c, w };
    if let Err(why) = shape_and_binding(&ctx) {
        return ClaimVerdictV1::EvidenceMalformed { why };
    }
    let rels: BTreeMap<(u8, u16), &PlanRelationV1> = c.plan.relations.iter().map(|r| ((r.block, r.node), r)).collect();
    let seed = c.binding.seed();
    let mut instances = 0u128;
    for p in 0..c.tokens.len() as u32 {
        for s in 0..ctx.w.occurrences.len() as u16 {
            let block = ctx.w.occurrences[s as usize].0;
            for n in 0..c.program.blocks[block as usize].nodes.len() as u16 {
                let Some(rel) = rels.get(&(block, n)) else {
                    return ClaimVerdictV1::EvidenceMalformed { why: format!("block {block} node {n} has no relation in the plan") };
                };
                match check_instance(&ctx, material, rel, &seed, (p, s, n), &mut instances) {
                    Ok(()) => {}
                    Err(v) => return v,
                }
            }
        }
    }
    ClaimVerdictV1::Pass { probabilistic_instances: instances, error_bits: derived_error_bits(c.descriptor, instances) }
}

fn shape_and_binding(ctx: &Ctx<'_>) -> Result<(), String> {
    let c = ctx.c;
    if c.binding.evidence_root != c.evidence.root() || c.binding.plan_root != c.plan.root() {
        return Err("the challenge is not bound to this evidence and plan (bind first, then draw)".into());
    }
    if c.tokens.is_empty() || c.tokens.len() as u64 > c.plan.max_positions as u64 || c.evidence.commitments.len() != c.tokens.len() {
        return Err(format!(
            "{} positions of evidence for {} tokens (plan max {})",
            c.evidence.commitments.len(),
            c.tokens.len(),
            c.plan.max_positions
        ));
    }
    for (p, pos) in c.evidence.commitments.iter().enumerate() {
        if pos.len() != ctx.w.occurrences.len() {
            return Err(format!("position {p}: {} occurrences, the schedule has {}", pos.len(), ctx.w.occurrences.len()));
        }
        for (s, occ) in pos.iter().enumerate() {
            if occ.len() != c.program.blocks[ctx.w.occurrences[s].0 as usize].nodes.len() {
                return Err(format!("position {p} occurrence {s}: node count"));
            }
        }
    }
    if c.evidence.output_root(c.program) != c.claimed_output_root {
        return Err("the evidence's logits are not the claim's output root".into());
    }
    Ok(())
}

fn check_instance(
    ctx: &Ctx<'_>,
    m: &dyn MaterialV1,
    rel: &PlanRelationV1,
    seed: &Digest,
    (p, s, n): (u32, u16, u16),
    instances: &mut u128,
) -> Result<(), ClaimVerdictV1> {
    let node = ctx.w.node(s, n);
    let tokens = ctx.c.tokens;
    let src = |i: usize| ctx.w.input_source(tokens, p, s, n, i).map_err(|e| ClaimVerdictV1::EvidenceMalformed { why: e.to_string() });
    let output = ctx.open(m, &SourceV1::Node { position: p, occurrence: s, node: n })?;
    let inputs = (0..node.inputs.len()).map(|i| ctx.open(m, &src(i)?)).collect::<Result<Vec<_>, _>>()?;
    let prior = ctx
        .w
        .hist_prior_sources(tokens, p, s, n)
        .map_err(|e| ClaimVerdictV1::EvidenceMalformed { why: e.to_string() })?
        .iter()
        .map(|src| ctx.open(m, src))
        .collect::<Result<Vec<_>, _>>()?;
    let fault = |kind| {
        ClaimVerdictV1::Fault(Box::new(KernelFaultProofV1 {
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
    match rel.checker {
        CheckerIdV1::FreivaldsM127 if matches!(node.prim, Prim::MatMul) => {
            match freivalds(seed, (p, s, n), rel.repetitions, &inputs[0], &inputs[1], &output, instances) {
                Ok(None) => Ok(()),
                Ok(Some((slice, i, j))) => Err(fault(FaultKindV1::MatMulScalar { slice, i, j })),
                Err(why) => Err(ClaimVerdictV1::Inconsistent { why }),
            }
        }
        CheckerIdV1::FreivaldsM127 => {
            Err(ClaimVerdictV1::Inconsistent { why: "a Freivalds relation on a node that is not a MatMul".into() })
        }
        CheckerIdV1::ExactRecompute | CheckerIdV1::StateContinuity => {
            match eval_node(ctx.c.program, node, &inputs, &prior, ctx.w.h(s, p)) {
                Ok(v) if v == output => Ok(()),
                _ => Err(fault(FaultKindV1::Recompute)),
            }
        }
    }
}

/// **The terminal court**: re-authenticate the proof's openings from public material and recompute.
pub fn verify_fault_proof_v1(c: &ClaimContextV1<'_>, proof: &KernelFaultProofV1) -> Result<ConvictionV1, DismissalV1> {
    let w = WiringV1::new(c.program).map_err(|e| DismissalV1::NotAuthentic(format!("the program does not validate: {e}")))?;
    let ctx = Ctx { c, w };
    shape_and_binding(&ctx).map_err(DismissalV1::NotAuthentic)?;
    let (p, s, n) = (proof.position, proof.occurrence, proof.node);
    if p as usize >= c.tokens.len()
        || s as usize >= ctx.w.occurrences.len()
        || n as usize >= c.program.blocks[ctx.w.occurrences[s as usize].0 as usize].nodes.len()
    {
        return Err(DismissalV1::NotAuthentic("the proof names no instance of this claim".into()));
    }
    let node = ctx.w.node(s, n);
    let na = |e: String| DismissalV1::NotAuthentic(e);
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
