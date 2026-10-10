//! Staged dependency-union court for the explicit flat trace profile.
//!
//! Large committable node values use flat-v4; small/wide intermediates and
//! registered parameters retain v3. Public output material is a HASH opening:
//! the court recomputes its entire 64..127-element span from authenticated
//! earlier operands and the frozen evaluator. No shipping descriptor or route
//! selects this profile. A consumer must bind this profile ID into its statement,
//! price program admission/aggregate load, and implement its DA and decode courts.
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::{self, Cursor, Read, Write};

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Prim, Tensor};

use crate::element::{
    MissingV1, OperandSourceV1, OperandsV1, PreparedV1, SegClaimContextV1, element_value_v1, node_opening_bytes_v1, operand_sources,
    prompt_tile_bytes_v1, token_at,
};
use crate::hash::{Digest, finish, keyed};
use crate::merkle::AXIS_ROW;
use crate::merkle3::{LayoutV3, LeafOpeningV3, tensor_commitment_v3};
use crate::merkle4::{FlatLayoutV4, FlatLeafHash, FlatLeafValue};
use crate::seg::{NodeOpeningV1, PROMPT_TILE_IDS_V1, PromptTileOpeningV1};
use crate::trace::SourceV1;
use crate::verify::DismissalV1;

pub const FLAT_FAULT_MAX_BYTES_V4: u64 = 1 << 20;
pub const FLAT_FAULT_MAX_WORK_V4: u64 = 1 << 30;
pub const FLAT_FAULT_MAX_SCRATCH_V4: u64 = 32 << 20;
pub const FLAT_FAULT_MAX_LEAVES_V4: usize = 1024;
/// Wire/profile version; this is not the frozen K2-TIR-v4 execution descriptor.
pub const FLAT_FAULT_VERSION_V4: u16 = 4;

pub fn flat_trace_profile_id_v4() -> Digest {
    profile_id::<4>()
}
pub(crate) fn profile_id<const V: u8>() -> Digest {
    let mut s = keyed(if V == 5 { b"misaka-palw/kernel/flat-trace-profile/v5" } else { b"misaka-palw/kernel/flat-trace-profile/v4" });
    if V == 5 {
        s.update(b"all-internal>=64:flat-v5/64-tail-merge-127;small-node:v3-whole;params:v3;semantics:tir-v1;operand-row-only;fault-set-v5;bounds:dependency-union-v1/1MiB/2^30/32MiB/1024");
    } else {
        s.update(b"committable>=64:flat-v4/64-tail-merge-127;other-node:v3;params:v3;semantics:tir-v1;operand-row-only;fault-v4;bounds:dependency-union-v1/1MiB/2^30/32MiB/1024");
    }
    finish(s)
}
pub type FlatSegContextV4<'a> = FlatSegContext<'a, 4>;
pub type FlatOperandV4 = FlatOperand<4>;
pub type FlatLeafFaultV4 = FlatLeafFault<4>;
/// Explicit opt-in context; an existing v4/v5 claim cannot implicitly select it.
pub struct FlatSegContext<'a, const V: u8> {
    pub profile: Digest,
    pub claim: SegClaimContextV1<'a>,
}

pub(crate) fn uses_flat<const V: u8>(dtype: DType, shape: &[usize]) -> bool {
    (V == 5 || (V == 4 && dtype.committable())) && LayoutV3::try_of(shape).is_some_and(|l| l.len >= 64)
}
/// Producer/model-holder construction under this profile, never a v3 replacement.
pub fn flat_profile_commitment_v4(t: &Tensor) -> Result<Digest, io::Error> {
    profile_commitment::<4>(t)
}
pub(crate) fn profile_commitment<const V: u8>(t: &Tensor) -> Result<Digest, io::Error> {
    if t.shape.len() > 4 || t.shape.contains(&0) {
        return Err(invalid());
    }
    if uses_flat::<V>(t.dtype, &t.shape) {
        crate::merkle4::flat_commitment_from_tensor::<V>(t)
    } else {
        if t.shape.len() > 4
            || LayoutV3::try_of(&t.shape).is_none_or(|l| l.len != t.data.len() as u64)
            || t.data.iter().any(|v| !t.dtype.contains(*v))
        {
            return Err(invalid());
        }
        Ok(tensor_commitment_v3(t))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize)]
pub struct FlatOperand<const V: u8> {
    pub node: Option<NodeOpeningV1>,
    /// Exactly one grammar is permitted by the source's declared type/profile.
    pub flat: Vec<FlatLeafValue<V>>,
    pub legacy: Vec<LeafOpeningV3>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatLeafFault<const V: u8> {
    pub version: u16,
    pub output: NodeOpeningV1,
    /// No output values are included in the filing.
    pub leaf: FlatLeafHash<V>,
    pub inputs: Vec<FlatOperand<V>>,
    pub token: Option<PromptTileOpeningV1>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatLeafConvictionV4 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub leaf: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatCourtBoundsV4 {
    pub filing_bytes: u64,
    pub work: u64,
    pub scratch_bytes: u64,
    pub operand_leaves: Vec<u64>,
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded flat-v4 fault")
}
fn na(message: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(message.into())
}
fn shape64(shape: &[usize]) -> Vec<u64> {
    shape.iter().map(|x| *x as u64).collect()
}
pub(crate) fn prepare<'a, const V: u8>(c: &FlatSegContext<'a, V>) -> Result<PreparedV1<'a>, DismissalV1> {
    let q = &c.claim;
    if (V != 4 && V != 5)
        || c.profile != profile_id::<V>()
        || q.positions == 0
        || q.positions > q.program.history_bound
        || q.positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD
        || q.prompt_len == 0
        || q.generated.len() > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD as usize
        || q.generated.iter().any(|t| *t >= q.program.token_bound)
    {
        return Err(na("wrong flat profile or job extent"));
    }
    // The program must fit its original byte cap before validated wiring allocates.
    size_with_limit(q.program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64).map_err(|_| na("oversized program"))?;
    if let Some(e) = &q.encoder {
        if crate::seg_encoder::encoder_binding_v1(q.program).ok().as_ref() != Some(e) || q.positions != 1 || q.prompt_len > e.l {
            return Err(na("wrong encoder binding"));
        }
    } else if q.prompt_len.checked_add(q.generated.len().saturating_sub(1) as u32) != Some(q.positions) {
        return Err(na("positions are not the job's"));
    }
    if q.segment_roots.len() != crate::seg::segment_count_v1(q.positions) as usize {
        return Err(na("wrong segment count"));
    }
    if let Some(ids) = q.inline_prompt {
        if ids.len() != q.prompt_len as usize
            || ids.iter().any(|t| *t >= q.program.token_bound)
            || crate::seg::prompt_root_of_ids_v1(ids) != q.prompt_root
        {
            return Err(na("inline prompt not bound to job"));
        }
    }
    PreparedV1::without_scope(q.program, q.encoder.as_ref()).map_err(DismissalV1::NotAuthentic)
}
fn valid_at<const V: u8>(c: &FlatSegContext<'_, V>, pr: &PreparedV1<'_>, p: u32, s: u16, n: u16) -> bool {
    p < c.claim.positions && pr.valid(s, n)
}
pub(crate) fn authenticate_node<const V: u8>(
    c: &FlatSegContext<'_, V>,
    pr: &PreparedV1<'_>,
    o: &NodeOpeningV1,
    at: (u32, u16, u16),
) -> bool {
    (o.position, o.occurrence, o.node) == at
        && valid_at(c, pr, at.0, at.1, at.2)
        && o.authenticates(c.claim.segment_roots, c.claim.positions, pr.index(at.1, at.2), pr.node_count)
}

/// Per-leaf bound derived without evaluating, allocating or hashing a tensor.
/// Counts unique dependency leaves rather than whole source tensors. Bounds are
/// conservative for strided relations; the prover deduplicates the actual union.
pub(crate) fn leaf_bounds<const V: u8>(
    c: &FlatSegContext<'_, V>,
    p: u32,
    s: u16,
    n: u16,
    index: u64,
) -> Result<FlatCourtBoundsV4, DismissalV1> {
    let pr = prepare(c)?;
    bounds(c, &pr, p, s, n, index)
}
pub(crate) fn bounds<const V: u8>(
    c: &FlatSegContext<'_, V>,
    pr: &PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
    index: u64,
) -> Result<FlatCourtBoundsV4, DismissalV1> {
    if !valid_at(c, pr, p, s, n) {
        return Err(na("no such relation"));
    }
    let node = pr.w.node(s, n);
    let sh = pr.shapes(p, s, n);
    if !uses_flat::<V>(node.out.dtype, &sh.out) {
        return Err(na("output does not use flat profile"));
    }
    let layout = FlatLayoutV4::of(&shape64(&sh.out)).ok_or_else(|| na("invalid output shape"))?;
    let (_, count) = layout.span(index).ok_or_else(|| na("no such output leaf"))?;
    let material = layout.bounds_for::<V>(node.out.dtype).ok_or_else(|| na("invalid output dtype"))?;
    relation_bounds(c, pr, p, s, n, count as u64, material.leaf_hash_bytes)
}
pub(crate) fn relation_bounds<const V: u8>(
    c: &FlatSegContext<'_, V>,
    pr: &PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
    count: u64,
    output_bytes: u64,
) -> Result<FlatCourtBoundsV4, DismissalV1> {
    if !valid_at(c, pr, p, s, n) {
        return Err(na("no such relation"));
    }
    let node = pr.w.node(s, n);
    let sh = pr.shapes(p, s, n);
    let sources = operand_sources(pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    let mut bytes = 2 + node_opening_bytes_v1(pr.node_count) + output_bytes + 4 + 1;
    let program_bytes =
        size_with_limit(c.claim.program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64).map_err(|_| na("oversized program"))?;
    // Canonical parsing/type validation and the unrolled source-writer scan are
    // separate from reference arithmetic, but both are part of this API's work.
    let mut work =
        (count as u64 * 4096).saturating_add(program_bytes.saturating_mul(1024)).saturating_add(pr.node_count.saturating_mul(256));
    let mut scratch = 8u64 << 20; // program/wiring metadata and fixed evaluator buffers
    let mut leaf_limits = Vec::with_capacity(sources.len());
    let mut total_leaves = 0;
    for (i, src) in sources.iter().enumerate() {
        bytes += 1 + 4 + 4; // node option and both grammar counts
        let shape = &sh.inputs[i];
        let dtype = sh.in_dtypes[i];
        let mut flat = false;
        let committed = match src {
            OperandSourceV1::Source(SourceV1::Node { .. }) => {
                flat = uses_flat::<V>(dtype, shape);
                bytes += node_opening_bytes_v1(pr.node_count);
                true
            }
            OperandSourceV1::Source(SourceV1::Param { .. }) => true,
            OperandSourceV1::Source(SourceV1::Const(j)) => {
                let d = &c.claim.program.consts[*j as usize];
                scratch = scratch.saturating_add(d.data.len() as u64 * 16);
                false
            }
            OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                if *k == 0 {
                    bytes = bytes.saturating_add(prompt_tile_bytes_v1(PROMPT_TILE_IDS_V1 as u64, 10));
                    scratch += PROMPT_TILE_IDS_V1 as u64 * 16;
                }
                false
            }
            OperandSourceV1::Token => {
                bytes = bytes.saturating_add(prompt_tile_bytes_v1(PROMPT_TILE_IDS_V1 as u64, 10));
                false
            }
            _ => false,
        };
        if !committed {
            leaf_limits.push(0);
            continue;
        }
        let line = match node.prim {
            Prim::MatMul => shape.last().copied().unwrap_or(1) as u64,
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } | Prim::TopK { axis, .. } => shape[axis as usize] as u64,
            _ => 1,
        };
        // The second MatMul operand's contraction is its penultimate dimension.
        let line = if matches!(node.prim, Prim::MatMul) && i == 1 { shape[shape.len() - 2] as u64 } else { line };
        let reads = (count as u64).saturating_mul(line);
        let l = LayoutV3::of(shape);
        let (all, max_value, leaf_bytes) = if flat {
            let f = FlatLayoutV4::of(&shape64(shape)).ok_or_else(|| na("invalid input shape"))?;
            let b = f.bounds_for::<V>(dtype).ok_or_else(|| na("invalid input dtype"))?;
            (f.leaves(), b.leaf_elements, b.leaf_value_bytes)
        } else {
            let values = l.row_len.min(crate::merkle3::TILE_V3);
            (
                l.leaves(AXIS_ROW),
                values,
                94 + 8 * shape.len() as u64 + values * dtype.width() as u64 + 64 * crate::merkle::depth(l.leaves(AXIS_ROW)),
            )
        };
        let mut leaves = reads.min(all);
        // Identity-position reads cover a contiguous flat interval, irrespective
        // of rank; equality of shapes in pointwise relations also preserves it.
        if flat
            && (matches!(node.prim, Prim::Reshape | Prim::Cast | Prim::StateWrite { .. })
                || (shape == &sh.out
                    && matches!(
                        node.prim,
                        Prim::Add
                            | Prim::Sub
                            | Prim::Mul
                            | Prim::Div { .. }
                            | Prim::Compare { .. }
                            | Prim::Select
                            | Prim::Clamp { .. }
                            | Prim::Log2Floor
                            | Prim::IntExp
                            | Prim::IntRsqrt
                            | Prim::IntLn
                    )))
        {
            leaves = leaves.min((count as u64).div_ceil(64) + 1);
        }
        leaf_limits.push(leaves);
        total_leaves += leaves;
        bytes = bytes.saturating_add(leaves.saturating_mul(leaf_bytes));
        scratch = scratch.saturating_add(leaves.saturating_mul(max_value * 16 + 5120));
        let sorting = if matches!(node.prim, Prim::TopK { .. }) { crate::merkle::depth(line) * 64 } else { 0 };
        work = work.saturating_add(reads.saturating_mul(64 + crate::merkle::depth(leaves.max(1)) * 16 + sorting));
        // Reference line/matmul/sort scratch is bounded before invoking it.
        scratch = scratch.saturating_add(line.saturating_mul(128));
    }
    work = work.saturating_add(bytes.saturating_mul(16));
    scratch = scratch.saturating_add(bytes); // input wire retained during decode
    if bytes > FLAT_FAULT_MAX_BYTES_V4
        || work > FLAT_FAULT_MAX_WORK_V4
        || scratch > FLAT_FAULT_MAX_SCRATCH_V4
        || total_leaves > FLAT_FAULT_MAX_LEAVES_V4 as u64
    {
        return Err(na("flat relation exceeds court resource caps"));
    }
    Ok(FlatCourtBoundsV4 { filing_bytes: bytes, work, scratch_bytes: scratch, operand_leaves: leaf_limits })
}

#[derive(Clone, Copy)]
enum LookupLayout {
    Flat(FlatLayoutV4),
    Legacy(LayoutV3),
}
enum Input<'a, const V: u8> {
    Proof(&'a FlatOperand<V>, DType, LookupLayout),
    Full(Tensor),
    Zeros,
    Scalar(i128),
}
pub(crate) struct Inputs<'a, const V: u8>(Vec<Input<'a, V>>);
impl<const V: u8> OperandsV1 for Inputs<'_, V> {
    fn get(&self, operand: usize, e: u64) -> Result<i128, MissingV1> {
        let missing = MissingV1(operand, e);
        match self.0.get(operand) {
            Some(Input::Proof(o, dtype, LookupLayout::Flat(l))) => {
                let index = l.leaf_of(e).ok_or(missing)?;
                let leaf = &o.flat[o.flat.binary_search_by_key(&index, |v| v.opening.index).map_err(|_| missing)?];
                let (first, _) = l.span(index).ok_or(missing)?;
                let at = (e - first) as usize * dtype.width();
                Ok(dtype.decode_le(leaf.bytes.get(at..at + dtype.width()).ok_or(missing)?))
            }
            Some(Input::Proof(o, _, LookupLayout::Legacy(l))) => {
                let (line, tile, offset) = l.row_leaf_of(e).ok_or(missing)?;
                let leaf = &o.legacy[o.legacy.binary_search_by_key(&(line, tile), |v| (v.line, v.tile)).map_err(|_| missing)?];
                leaf.values.get(offset as usize).copied().ok_or(missing)
            }
            Some(Input::Full(t)) => t.data.get(e as usize).copied().ok_or(missing),
            Some(Input::Zeros) => Ok(0),
            Some(Input::Scalar(v)) if e == 0 => Ok(*v),
            _ => Err(missing),
        }
    }
}

pub(crate) fn value_in_opening<const V: u8>(o: &FlatOperand<V>, dtype: DType, shape: &[usize], e: u64) -> Option<i128> {
    let l = if uses_flat::<V>(dtype, shape) {
        LookupLayout::Flat(FlatLayoutV4::of(&shape64(shape))?)
    } else {
        LookupLayout::Legacy(LayoutV3::try_of(shape)?)
    };
    Inputs(vec![Input::Proof(o, dtype, l)]).get(0, e).ok()
}

/// A terminal hash-leaf relation court; it never accepts producer output values.
/// Missing/borrowed/malformed/overbudget operands dismiss, not convict.
pub(crate) fn verify_leaf<const V: u8>(c: &FlatSegContext<'_, V>, f: &FlatLeafFault<V>) -> Result<FlatLeafConvictionV4, DismissalV1> {
    if f.version != V as u16 {
        return Err(na("unsupported flat fault version"));
    }
    structural(f).map_err(|_| na("malformed bounded flat fault"))?;
    let wire = size_with_limit(f, FLAT_FAULT_MAX_BYTES_V4).map_err(|_| na("flat filing exceeds byte cap or has no wire form"))?;
    let pr = prepare(c)?;
    verify_leaf_prepared(c, &pr, f, wire)
}
pub(crate) fn verify_leaf_prepared<const V: u8>(
    c: &FlatSegContext<'_, V>,
    pr: &PreparedV1<'_>,
    f: &FlatLeafFault<V>,
    wire: u64,
) -> Result<FlatLeafConvictionV4, DismissalV1> {
    let (p, s, n) = (f.output.position, f.output.occurrence, f.output.node);
    let b = bounds(c, &pr, p, s, n, f.leaf.index)?;
    if wire > b.filing_bytes || !authenticate_node(c, &pr, &f.output, (p, s, n)) {
        return Err(na("output not bound or filing over relation bound"));
    }
    let node = pr.w.node(s, n);
    let sh = pr.shapes(p, s, n);
    if f.leaf.dtype != node.out.dtype.tag() || f.leaf.shape != shape64(&sh.out) || !f.leaf.authenticates(&f.output.commitment) {
        return Err(na("output hash is not the declared committed value"));
    }
    let ops = authenticate_inputs(c, &pr, p, s, n, &f.inputs, &f.token, &b.operand_leaves)?;
    let (first, count) = f.leaf.layout().and_then(|l| l.span(f.leaf.index)).ok_or_else(|| na("invalid leaf span"))?;
    let mut bytes = Vec::with_capacity(count * node.out.dtype.width());
    let conviction = FlatLeafConvictionV4 { position: p, occurrence: s, node: n, leaf: f.leaf.index };
    for e in first..first + count as u64 {
        match element_value_v1(c.claim.program, node, &sh, e, &ops) {
            Err(_) => return Err(na("dependency not opened")),
            Ok(None) => return Ok(conviction),
            Ok(Some(v)) if node.out.dtype.contains(v) => node.out.dtype.encode_le(v, &mut bytes),
            Ok(Some(_)) => return Ok(conviction),
        }
    }
    if f.leaf.matches_bytes(&bytes) { Err(DismissalV1::NoFault) } else { Ok(conviction) }
}

pub(crate) fn authenticate_inputs<'f, const V: u8>(
    c: &FlatSegContext<'_, V>,
    pr: &PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
    openings: &'f [FlatOperand<V>],
    token: &Option<PromptTileOpeningV1>,
    limits: &[u64],
) -> Result<Inputs<'f, V>, DismissalV1> {
    let sh = pr.shapes(p, s, n);
    let sources = operand_sources(pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    if openings.len() != sources.len() || limits.len() != sources.len() {
        return Err(na("wrong operand count"));
    }
    let mut inputs = Vec::with_capacity(sources.len());
    for (i, (src, o)) in sources.iter().zip(openings).enumerate() {
        if o.flat.len() + o.legacy.len() > limits[i] as usize {
            return Err(na("operand exceeds dependency leaf bound"));
        }
        let mut root = None;
        let mut flat = false;
        match src {
            OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                let opening = o.node.as_ref().ok_or_else(|| na("missing operand node"))?;
                if !authenticate_node(c, &pr, opening, (*position, *occurrence, *node)) {
                    return Err(na("borrowed operand node"));
                }
                root = Some(opening.commitment);
                flat = uses_flat::<V>(sh.in_dtypes[i], &sh.inputs[i]);
            }
            OperandSourceV1::Source(SourceV1::Param { index, layer }) => {
                if o.node.is_some() {
                    return Err(na("param carries a node"));
                }
                root =
                    Some(*c.claim.params.by_instance.get(&(*index, *layer)).ok_or_else(|| na("no registered parameter commitment"))?);
            }
            _ => {}
        }
        if let Some(root) = root {
            let want = shape64(&sh.inputs[i]);
            if flat {
                if !o.legacy.is_empty()
                    || o.flat
                        .iter()
                        .any(|l| l.opening.dtype != sh.in_dtypes[i].tag() || l.opening.shape != want || !l.authenticates(&root))
                    || o.flat.windows(2).any(|w| w[0].opening.index >= w[1].opening.index)
                {
                    return Err(na("wrong flat operand leaves"));
                }
            } else if !o.flat.is_empty()
                || o.legacy
                    .iter()
                    .any(|l| l.dtype != sh.in_dtypes[i].tag() || l.shape != want || l.axis != AXIS_ROW || !l.authenticates(&root))
                || o.legacy.windows(2).any(|w| (w[0].line, w[0].tile) >= (w[1].line, w[1].tile))
            {
                return Err(na("wrong legacy operand leaves"));
            }
            let layout = if flat {
                LookupLayout::Flat(FlatLayoutV4::of(&want).ok_or_else(|| na("invalid operand shape"))?)
            } else {
                LookupLayout::Legacy(LayoutV3::of(&sh.inputs[i]))
            };
            inputs.push(Input::Proof(o, sh.in_dtypes[i], layout));
            continue;
        }
        if o.node.is_some() || !o.flat.is_empty() || !o.legacy.is_empty() {
            return Err(na("public operand carries proof material"));
        }
        inputs.push(match src {
            OperandSourceV1::Token => Input::Scalar(
                token_at(&c.claim, p, token.as_ref())
                    .filter(|t| *t < c.claim.program.token_bound)
                    .ok_or_else(|| na("missing bound token"))? as i128,
            ),
            OperandSourceV1::Position => Input::Scalar(p as i128),
            OperandSourceV1::Source(SourceV1::Const(j)) => {
                Input::Full(crate::trace::const_tensor(c.claim.program, *j).map_err(|_| na("invalid const"))?)
            }
            OperandSourceV1::Source(SourceV1::Zeros { .. }) => Input::Zeros,
            OperandSourceV1::Source(SourceV1::Public(v)) => Input::Scalar(*v as i128),
            OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                let e = c.claim.encoder.as_ref().ok_or_else(|| na("unbound stage input"))?;
                if *k == 1 {
                    Input::Full(e.count(c.claim.prompt_len).map_err(DismissalV1::NotAuthentic)?)
                } else if *k == 0 {
                    let ids = if let Some(ids) = c.claim.inline_prompt {
                        ids
                    } else {
                        let t = token
                            .as_ref()
                            .filter(|t| {
                                t.index == 0
                                    && t.ids.len() == c.claim.prompt_len as usize
                                    && t.authenticates(c.claim.prompt_len, &c.claim.prompt_root)
                            })
                            .ok_or_else(|| na("missing job ids"))?;
                        &t.ids
                    };
                    if ids.iter().any(|t| *t >= c.claim.program.token_bound) {
                        return Err(na("job token out of range"));
                    }
                    Input::Full(e.ids(ids).map_err(DismissalV1::NotAuthentic)?)
                } else {
                    return Err(na("unknown stage input"));
                }
            }
            _ => return Err(na("unbound operand")),
        });
    }
    Ok(Inputs(inputs))
}

struct SizeWriter {
    len: u64,
    limit: u64,
}
impl Write for SizeWriter {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.len = self.len.checked_add(b.len() as u64).ok_or_else(invalid)?;
        if self.len > self.limit { Err(invalid()) } else { Ok(b.len()) }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) fn size_with_limit<T: BorshSerialize>(v: &T, limit: u64) -> io::Result<u64> {
    let mut w = SizeWriter { len: 0, limit };
    v.serialize(&mut w)?;
    Ok(w.len)
}
pub(crate) fn structural<const V: u8>(f: &FlatLeafFault<V>) -> io::Result<()> {
    if f.version != V as u16 {
        return Err(invalid());
    }
    if f.leaf.shape.len() > 4 || f.leaf.siblings.len() > 64 {
        return Err(invalid());
    }
    parts_structure(Some(&f.output), &f.inputs, &f.token)
}
pub(crate) fn parts_structure<const V: u8>(
    output: Option<&NodeOpeningV1>,
    inputs: &[FlatOperand<V>],
    token: &Option<PromptTileOpeningV1>,
) -> io::Result<()> {
    let node_ok = |n: &NodeOpeningV1| n.node_siblings.len() <= 64 && n.position_siblings.len() <= 10;
    if inputs.len() > 9 || output.is_some_and(|n| !node_ok(n)) {
        return Err(invalid());
    }
    let mut leaves = 0usize;
    for o in inputs {
        leaves = leaves.checked_add(o.flat.len()).and_then(|n| n.checked_add(o.legacy.len())).ok_or_else(invalid)?;
        if leaves > FLAT_FAULT_MAX_LEAVES_V4 || o.node.as_ref().is_some_and(|n| !node_ok(n)) {
            return Err(invalid());
        }
        for l in &o.flat {
            if l.opening.shape.len() > 4 || l.opening.siblings.len() > 64 || l.bytes.len() > 127 * if V == 5 { 16 } else { 4 } {
                return Err(invalid());
            }
        }
        for l in &o.legacy {
            if l.shape.len() > 4 || l.siblings.len() > 64 || l.values.len() > 4096 {
                return Err(invalid());
            }
        }
    }
    if token.as_ref().is_some_and(|t| t.ids.len() > PROMPT_TILE_IDS_V1 || t.siblings.len() > 10) {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn vector<R: Read, T>(r: &mut R, max: usize, mut get: impl FnMut(&mut R) -> io::Result<T>) -> io::Result<Vec<T>> {
    let count = u32::deserialize_reader(r)? as usize;
    if count > max {
        return Err(invalid());
    }
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| invalid())?;
    for _ in 0..count {
        out.push(get(r)?);
    }
    Ok(out)
}
pub(crate) fn optional<R: Read, T>(r: &mut R, get: impl FnOnce(&mut R) -> io::Result<T>) -> io::Result<Option<T>> {
    match u8::deserialize_reader(r)? {
        0 => Ok(None),
        1 => get(r).map(Some),
        _ => Err(invalid()),
    }
}
pub(crate) fn read_node<R: Read>(r: &mut R) -> io::Result<NodeOpeningV1> {
    Ok(NodeOpeningV1 {
        position: u32::deserialize_reader(r)?,
        occurrence: u16::deserialize_reader(r)?,
        node: u16::deserialize_reader(r)?,
        commitment: Digest::deserialize_reader(r)?,
        node_siblings: vector(r, 64, Digest::deserialize_reader)?,
        position_root: Digest::deserialize_reader(r)?,
        position_siblings: vector(r, 10, Digest::deserialize_reader)?,
    })
}
pub(crate) fn read_legacy<R: Read>(r: &mut R) -> io::Result<LeafOpeningV3> {
    let tag = u8::deserialize_reader(r)?;
    let dtype = DType::ALL.into_iter().find(|d| d.tag() == tag).ok_or_else(invalid)?;
    let shape = vector(r, 4, u64::deserialize_reader)?;
    if shape.contains(&0) {
        return Err(invalid());
    }
    let shape_usize = shape.iter().map(|n| usize::try_from(*n).map_err(|_| invalid())).collect::<io::Result<Vec<_>>>()?;
    let layout = LayoutV3::try_of(&shape_usize).ok_or_else(invalid)?;
    let axis = u8::deserialize_reader(r)?;
    let line = u64::deserialize_reader(r)?;
    let tile = u64::deserialize_reader(r)?;
    if axis != AXIS_ROW {
        return Err(invalid());
    }
    let count = u32::deserialize_reader(r)? as usize;
    if count > 4096 || layout.leaf_element_count(axis, line, tile) != Some(count as u64) {
        return Err(invalid());
    }
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| invalid())?;
    let mut buf = [0; 16];
    for _ in 0..count {
        r.read_exact(&mut buf[..dtype.width()])?;
        values.push(dtype.decode_le(&buf[..dtype.width()]));
    }
    let siblings = vector(r, 64, Digest::deserialize_reader)?;
    let other_root = Digest::deserialize_reader(r)?;
    let out = LeafOpeningV3 { dtype: tag, shape, axis, line, tile, values, siblings, other_root };
    out.recomputed_commitment().ok_or_else(invalid)?;
    Ok(out)
}
impl<const V: u8> FlatLeafFault<V> {
    /// Bounded exact codec: every vector count is checked before allocation.
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > FLAT_FAULT_MAX_BYTES_V4 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != V as u16 {
            return Err(invalid());
        }
        let output = read_node(&mut r)?;
        let leaf = FlatLeafHash::<V>::deserialize_reader(&mut r)?;
        let mut total = 0;
        let inputs = vector(&mut r, 9, |r| {
            let node = optional(r, read_node)?;
            let flat = vector(r, FLAT_FAULT_MAX_LEAVES_V4 - total, FlatLeafValue::<V>::deserialize_reader)?;
            total += flat.len();
            let legacy = vector(r, FLAT_FAULT_MAX_LEAVES_V4 - total, read_legacy)?;
            total += legacy.len();
            Ok(FlatOperand { node, flat, legacy })
        })?;
        let token = optional(&mut r, |r| {
            Ok(PromptTileOpeningV1 {
                index: u32::deserialize_reader(r)?,
                ids: vector(r, PROMPT_TILE_IDS_V1, u32::deserialize_reader)?,
                siblings: vector(r, 10, Digest::deserialize_reader)?,
            })
        })?;
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        let out = Self { version, output, leaf, inputs, token };
        structural(&out)?;
        Ok(out)
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        structural(self)?;
        size_with_limit(self, FLAT_FAULT_MAX_BYTES_V4)?;
        borsh::to_vec(self)
    }
}

/// The model holder records only the union of leaves actually read by the same
/// evaluator as the court. Flat coordinates are `(index, 0)`; v3 coordinates
/// are `(row, tile)`. Public operands have no proof leaves. Supplied tensors
/// belong to the model holder, never to a producer response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatDependencyUnionV4 {
    pub leaves: Vec<Vec<(u64, u64)>>,
    pub reference_refuses: bool,
}
pub(crate) fn leaf_dependencies<const V: u8>(
    c: &FlatSegContext<'_, V>,
    p: u32,
    s: u16,
    n: u16,
    index: u64,
    own_inputs: &[Tensor],
) -> Result<FlatDependencyUnionV4, DismissalV1> {
    let pr = prepare(c)?;
    let b = bounds(c, &pr, p, s, n, index)?;
    let sh = pr.shapes(p, s, n);
    let (first, count) = FlatLayoutV4::of(&shape64(&sh.out)).and_then(|l| l.span(index)).ok_or_else(|| na("no output span"))?;
    record_span(c, &pr, p, s, n, first, count, own_inputs, b)
}
pub(crate) fn record_span<const V: u8>(
    c: &FlatSegContext<'_, V>,
    pr: &PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
    first: u64,
    count: usize,
    own_inputs: &[Tensor],
    b: FlatCourtBoundsV4,
) -> Result<FlatDependencyUnionV4, DismissalV1> {
    let sh = pr.shapes(p, s, n);
    let src = operand_sources(&pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    if own_inputs.len() != src.len()
        || own_inputs.iter().enumerate().any(|(i, t)| {
            t.dtype != sh.in_dtypes[i]
                || t.shape != sh.inputs[i]
                || LayoutV3::try_of(&t.shape).is_none_or(|l| l.len != t.data.len() as u64)
        })
    {
        return Err(na("model holder's operand metadata mismatch"));
    }
    struct Recording<'a> {
        tensors: &'a [Tensor],
        kinds: Vec<u8>,
        reads: RefCell<Vec<BTreeSet<(u64, u64)>>>,
        limits: Vec<u64>,
    }
    impl OperandsV1 for Recording<'_> {
        fn get(&self, i: usize, e: u64) -> Result<i128, MissingV1> {
            let missing = MissingV1(i, e);
            let t = self.tensors.get(i).ok_or(missing)?;
            let v = t.data.get(e as usize).copied().filter(|v| t.dtype.contains(*v)).ok_or(missing)?;
            let coordinate = match self.kinds[i] {
                1 => {
                    let (a, b, _) = LayoutV3::of(&t.shape).row_leaf_of(e).ok_or(missing)?;
                    Some((a, b))
                }
                2 => Some((FlatLayoutV4::of(&shape64(&t.shape)).and_then(|l| l.leaf_of(e)).ok_or(missing)?, 0)),
                _ => None,
            };
            if let Some(at) = coordinate {
                let mut reads = self.reads.borrow_mut();
                reads[i].insert(at);
                if reads[i].len() as u64 > self.limits[i] {
                    return Err(missing);
                }
            }
            Ok(v)
        }
    }
    let kinds = src
        .iter()
        .enumerate()
        .map(|(i, s)| match s {
            OperandSourceV1::Source(SourceV1::Node { .. }) => {
                if uses_flat::<V>(sh.in_dtypes[i], &sh.inputs[i]) {
                    2
                } else {
                    1
                }
            }
            OperandSourceV1::Source(SourceV1::Param { .. }) => 1,
            _ => 0,
        })
        .collect();
    let ops =
        Recording { tensors: own_inputs, kinds, reads: RefCell::new(vec![BTreeSet::new(); src.len()]), limits: b.operand_leaves };

    let mut reference_refuses = false;
    for e in first..first + count as u64 {
        match element_value_v1(c.claim.program, pr.w.node(s, n), &sh, e, &ops) {
            Err(_) => return Err(na("model holder missing dependency or over bound")),
            Ok(None) => {
                reference_refuses = true;
                break;
            }
            Ok(Some(_)) => {}
        }
    }
    Ok(FlatDependencyUnionV4 {
        leaves: ops.reads.into_inner().into_iter().map(|s| s.into_iter().collect()).collect(),
        reference_refuses,
    })
}

pub fn flat_leaf_court_bounds_v4(
    c: &FlatSegContextV4<'_>,
    p: u32,
    s: u16,
    n: u16,
    index: u64,
) -> Result<FlatCourtBoundsV4, DismissalV1> {
    leaf_bounds::<4>(c, p, s, n, index)
}
pub fn verify_flat_leaf_fault_v4(c: &FlatSegContextV4<'_>, f: &FlatLeafFaultV4) -> Result<FlatLeafConvictionV4, DismissalV1> {
    verify_leaf::<4>(c, f)
}
pub fn flat_leaf_dependencies_v4(
    c: &FlatSegContextV4<'_>,
    p: u32,
    s: u16,
    n: u16,
    index: u64,
    own_inputs: &[Tensor],
) -> Result<FlatDependencyUnionV4, DismissalV1> {
    leaf_dependencies::<4>(c, p, s, n, index, own_inputs)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::job::DecodeRuleV1;
    use crate::merkle4::{FlatLeafHashV4, FlatLeafValueV4, FlatTreeV4};
    use crate::seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1};
    use crate::trace::{ParamCommitmentsV1, TraceV1, trace_v1};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, Ref, TirProgramV1};
    use misaka_palw_tir::{Dim, MapParams, TensorType};

    fn hash(t: &Tensor, index: u64) -> FlatLeafHashV4 {
        FlatTreeV4::of(t).unwrap().opening(index).unwrap()
    }
    struct Trial {
        program: TirProgramV1,
        params: MapParams,
        own: TraceV1,
        public: SegmentedCommitmentsV1,
        pc: ParamCommitmentsV1,
        tokens: Vec<u32>,
        roots: Vec<Digest>,
        encoder: Option<crate::seg_encoder::EncoderBindingV1>,
    }
    impl Trial {
        fn of(program: TirProgramV1, params: MapParams, tokens: Vec<u32>, wrong: Option<(u32, u16, u16, u64)>) -> Self {
            let own = trace_v1(&program, &params, &tokens).unwrap();
            let mut posted = own.clone();
            if let Some((p, s, n, e)) = wrong {
                posted.values[p as usize][s as usize][n as usize].data[e as usize] += 1;
            }
            let commitments = posted
                .values
                .iter()
                .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v4(t).unwrap()).collect()).collect())
                .collect();
            let public = SegmentedCommitmentsV1::new(commitments);
            let roots = public.segment_roots();
            let pc = ParamCommitmentsV1::of_v3(&params);
            Self { program, params, own, public, pc, tokens, roots, encoder: None }
        }
        fn context(&self) -> FlatSegContextV4<'_> {
            FlatSegContextV4 {
                profile: flat_trace_profile_id_v4(),
                claim: SegClaimContextV1 {
                    program: &self.program,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: self.public.positions(),
                    prompt_len: self.tokens.len() as u32,
                    prompt_root: prompt_root_of_ids_v1(&self.tokens),
                    inline_prompt: Some(&self.tokens),
                    generated: &[],
                    decode: DecodeRuleV1::Greedy,
                    encoder: self.encoder,
                },
            }
        }
        // Model holder uses only its own operands and public node/hash openings.
        // The trial's copied producer tensor above only builds those commitments.
        fn filing(&self, p: u32, s: u16, n: u16, index: u64, public_hash: FlatLeafHashV4) -> FlatLeafFaultV4 {
            let c = self.context();
            let pr = prepare(&c).unwrap();
            let sources = operand_sources(&pr, p, s, n).unwrap();
            let own_inputs: Vec<Tensor> = sources
                .iter()
                .map(|src| match src {
                    OperandSourceV1::Token => Tensor::scalar(DType::Idx, self.tokens[p as usize] as i128).unwrap(),
                    OperandSourceV1::Position => Tensor::scalar(DType::Idx, p as i128).unwrap(),
                    OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                        self.own.value(*position, *occurrence, *node).unwrap().clone()
                    }
                    OperandSourceV1::Source(SourceV1::Param { index, layer }) => self.params.tensors[&(*index, *layer)].clone(),
                    OperandSourceV1::Source(SourceV1::Const(j)) => crate::trace::const_tensor(&self.program, *j).unwrap(),
                    OperandSourceV1::Source(SourceV1::Zeros { dtype, shape }) => Tensor::zeros(*dtype, shape),
                    OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                        let e = self.encoder.unwrap();
                        if *k == 0 { e.ids(&self.tokens).unwrap() } else { e.count(self.tokens.len() as u32).unwrap() }
                    }
                    _ => panic!("fixture source"),
                })
                .collect();
            let union = flat_leaf_dependencies_v4(&c, p, s, n, index, &own_inputs).unwrap();
            let inputs = sources
                .iter()
                .zip(&own_inputs)
                .zip(union.leaves)
                .map(|((src, t), reads)| {
                    let mut o = FlatOperandV4::default();
                    match src {
                        OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                            o.node = self.public.node_opening(*position, *occurrence, *node)
                        }
                        OperandSourceV1::Source(SourceV1::Param { .. }) => {}
                        _ => return o,
                    }
                    if o.node.is_some() && uses_flat::<4>(t.dtype, &t.shape) {
                        let tree = FlatTreeV4::of(t).unwrap();
                        o.flat = reads.into_iter().map(|(index, _)| FlatLeafValueV4::of(t, &tree, index).unwrap()).collect();
                    } else {
                        let trees = crate::merkle3::TreesV3::of(t);
                        o.legacy = reads
                            .into_iter()
                            .map(|(row, tile)| LeafOpeningV3::of_trees(t, &trees, AXIS_ROW, row, tile).unwrap())
                            .collect();
                    }
                    o
                })
                .collect();
            FlatLeafFaultV4 {
                version: FLAT_FAULT_VERSION_V4,
                output: self.public.node_opening(p, s, n).unwrap(),
                leaf: public_hash,
                inputs,
                token: None,
            }
        }
    }
    pub(crate) fn identity(n: u32) -> (TirProgramV1, MapParams) {
        let mut b = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let weight = b.param("unknown-party.weight", DType::I16, &[n], false);
        let mut pre = b.block("pre", vec![]);
        let x = pre.cast(weight, DType::I16);
        let pre = pre.finish(&[x]);
        let mut post = b.block("post", vec![TensorType::fixed(DType::I16, &[n])]);
        let y = post.cast(Ref::CarryIn(0), DType::I16);
        post.commit(y);
        let post = post.finish(&[]);
        let program = b.finish(pre, vec![], post, 0);
        let t = Tensor::new(DType::I16, vec![n as usize], (0..n).map(|i| (i % 127) as i128).collect()).unwrap();
        let mut params = MapParams::default();
        params.tensors.insert((0, None), t);
        (program, params)
    }
    #[test]
    fn flat_court_convicts_every_leaf_and_merged_tail_without_producer_output_values() {
        for n in [64, 65, 127, 128, 129, 191, 192, 257, 1025] {
            let (p, m) = identity(n);
            let layout = FlatLayoutV4::of(&[n as u64]).unwrap();
            for i in 0..layout.leaves() {
                let (first, count) = layout.span(i).unwrap();
                let e = first + count as u64 - 1;
                let trial = Trial::of(p.clone(), m.clone(), vec![1], Some((0, 1, 0, e)));
                let mut actual = trial.own.value(0, 1, 0).unwrap().clone();
                actual.data[e as usize] += 1;
                let f = trial.filing(0, 1, 0, i, hash(&actual, i));
                assert_eq!(f.inputs[0].flat.len(), 1, "duplicate reads must share one proof leaf");
                assert_eq!(
                    verify_flat_leaf_fault_v4(&trial.context(), &f),
                    Ok(FlatLeafConvictionV4 { position: 0, occurrence: 1, node: 0, leaf: i })
                );
                let wire = f.to_bytes().unwrap();
                let parsed = FlatLeafFaultV4::from_bytes(&wire).unwrap();
                assert_eq!(f, parsed);
                assert_eq!(verify_flat_leaf_fault_v4(&trial.context(), &parsed), verify_flat_leaf_fault_v4(&trial.context(), &f));
                assert!(wire.len() as u64 <= flat_leaf_court_bounds_v4(&trial.context(), 0, 1, 0, i).unwrap().filing_bytes);
            }
            let honest = Trial::of(p, m, vec![1], None);
            for i in 0..layout.leaves() {
                let f = honest.filing(0, 1, 0, i, hash(honest.own.value(0, 1, 0).unwrap(), i));
                assert_eq!(verify_flat_leaf_fault_v4(&honest.context(), &f), Err(DismissalV1::NoFault));
            }
        }
    }
    #[test]
    fn flat_court_authenticates_registered_weights_and_rejects_borrowed_source_or_profile() {
        let (p, m) = identity(129);
        let trial = Trial::of(p, m, vec![1, 2], Some((1, 0, 0, 128)));
        let mut actual = trial.own.value(1, 0, 0).unwrap().clone();
        actual.data[128] += 1;
        let f = trial.filing(1, 0, 0, 1, hash(&actual, 1));
        assert_eq!(f.inputs[0].legacy.len(), 1);
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_ok());
        let mut bad = f.clone();
        bad.inputs[0].legacy[0].values[0] += 1;
        assert!(matches!(verify_flat_leaf_fault_v4(&trial.context(), &bad), Err(DismissalV1::NotAuthentic(_))));
        let mut c = trial.context();
        c.profile[0] ^= 1;
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
        let mut bad = f.clone();
        bad.output = trial.public.node_opening(0, 0, 0).unwrap();
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &bad).is_err());
        let mut bad = f.clone();
        bad.leaf.siblings.push([0; 64]);
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &bad).is_err());
        let mut bad = f.clone();
        let copy = bad.inputs[0].legacy[0].clone();
        bad.inputs[0].legacy.push(copy);
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &bad).is_err());
        let mut bad = f.clone();
        bad.inputs[0].legacy.clear();
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &bad).is_err());
        let mut bad = f.clone();
        bad.version = 3;
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &bad).is_err());
        let mut c = trial.context();
        c.claim.inline_prompt = Some(&[2, 1]);
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
    }
    #[test]
    fn flat_court_wire_checks_every_count_truncation_and_surplus_before_allocation() {
        let (p, m) = identity(65);
        let trial = Trial::of(p, m, vec![1], None);
        let f = trial.filing(0, 1, 0, 0, hash(trial.own.value(0, 1, 0).unwrap(), 0));
        let wire = f.to_bytes().unwrap();
        assert_eq!(FlatLeafFaultV4::from_bytes(&wire).unwrap(), f);
        for cut in 0..wire.len() {
            assert!(FlatLeafFaultV4::from_bytes(&wire[..cut]).is_err(), "cut {cut}");
        }
        let mut b = wire.clone();
        b.push(0);
        assert!(FlatLeafFaultV4::from_bytes(&b).is_err());
        let mut b = wire.clone();
        b[0] = 3;
        assert!(FlatLeafFaultV4::from_bytes(&b).is_err());
        // Node siblings count directly follows version, coordinates and commitment.
        let mut b = wire.clone();
        b[74..78].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(FlatLeafFaultV4::from_bytes(&b).is_err());
        let mut bad = f.clone();
        let copy = bad.inputs[0].flat[0].clone();
        bad.inputs[0].flat.resize(1025, copy);
        assert!(bad.to_bytes().is_err());
        let mut bad = f.clone();
        bad.output.node_siblings.resize(65, [0; 64]);
        assert!(bad.to_bytes().is_err());
        let mut bad = f;
        bad.inputs[0].flat[0].bytes.resize(509, 0);
        assert!(bad.to_bytes().is_err());
    }
    #[test]
    fn flat_court_prices_large_declarations_without_allocating_tensors_and_refuses_large_lines() {
        let (p, m) = identity(1_048_577);
        let trial = Trial::of(p, m, vec![1], None);
        let b = flat_leaf_court_bounds_v4(&trial.context(), 0, 1, 0, 0).unwrap();
        assert!(b.filing_bytes < 8192);
        assert!(b.scratch_bytes < 12 << 20);
        assert!(b.operand_leaves[0] <= 3);
        let (p, m) = identity(65);
        let mut trial = Trial::of(p, m, vec![1], None);
        // Replace all fixed widths; program validation and bounds do not materialize them.
        let huge = 1 << 24;
        trial.program.params[0].shape = vec![huge];
        for block in &mut trial.program.blocks {
            for t in &mut block.carry_in {
                t.shape = vec![Dim::Fixed(huge)];
            }
            for node in &mut block.nodes {
                node.out.shape = vec![Dim::Fixed(huge)];
            }
        }
        let b = flat_leaf_court_bounds_v4(&trial.context(), 0, 1, 0, 0).unwrap();
        assert!(b.filing_bytes < 16 << 10);
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let w = pb.param("weight", DType::I16, &[64, 1 << 22], false);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.reduce_max(w, 1);
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I16, &[64, 1])]);
        let y = post.cast(Ref::CarryIn(0), DType::I16);
        post.commit(y);
        let post = post.finish(&[]);
        trial.program = pb.finish(pre, vec![], post, 0);
        assert!(flat_leaf_court_bounds_v4(&trial.context(), 0, 0, 0, 0).is_err());
    }
    pub(crate) fn primitive_fixture(tag: u8) -> (TirProgramV1, MapParams, u16) {
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let ashape = match tag {
            0 => vec![129],
            1 => vec![3, 43],
            2 => vec![3, 50],
            3 => vec![64],
            6 => vec![4, 43],
            11 => vec![3, 7],
            12 | 13 => vec![129, 7],
            22 => vec![3, 50],
            _ => vec![129],
        };
        let a = if tag == 4 || tag == 5 { None } else { Some(pb.param("source.a", DType::I16, &ashape, false)) };
        let bshape = match tag {
            3 => vec![65],
            6 => vec![3],
            11 => vec![7, 43],
            _ => vec![],
        };
        let b = if [3, 6, 8, 9, 10, 11, 14, 20, 21].contains(&tag) {
            Some(pb.param("source.b", if tag == 6 { DType::Idx } else { DType::I16 }, &bshape, false))
        } else {
            None
        };
        let mut pre = pb.block("pre", vec![]);
        let a = a.unwrap_or(Ref::Input(0));
        let b = b.unwrap_or(Ref::Input(0));
        let target = match tag {
            0 => pre.reshape_fixed(a, &[3, 43]),
            1 => pre.transpose(a, &[1, 0]),
            2 => pre.slice(a, 1, 3, 43),
            3 => pre.concat(&[a, b], 0),
            4 => {
                let x = pre.c(DType::I16, 7);
                pre.broadcast(x, &[Dim::Fixed(129)])
            }
            5 => pre.iota(DType::Idx, &[Dim::Fixed(129)], 0, 0, 1),
            6 => pre.gather(a, b, 0, 0),
            7 => pre.cast(a, DType::I32),
            8 => pre.add(a, b, DType::I32),
            9 => pre.sub(a, b, DType::I32),
            10 => pre.mul(a, b, DType::I32),
            11 => pre.matmul(a, b, DType::I32),
            12 => pre.reduce_sum(a, 1, DType::I32),
            13 => pre.reduce_max(a, 1),
            14 => pre.div(a, b, misaka_palw_tir::Rounding::Floor, DType::I32),
            15 => pre.clamp(a, 1, 5, DType::I16),
            16 => pre.log2_floor(a, DType::I32),
            17 => {
                let x = pre.clamp(a, 0, 0, DType::I32);
                pre.int_exp(x)
            }
            18 | 19 => {
                let one = misaka_palw_tir::arith::ONE as i64;
                let x = pre.clamp(a, one, one, DType::I32);
                pre.push(if tag == 18 { Prim::IntRsqrt } else { Prim::IntLn }, vec![x], TensorType::fixed(DType::I32, &[129]))
            }
            20 => pre.compare(a, b, misaka_palw_tir::prim::Cmp::Gt),
            21 => {
                let cmp = pre.compare(a, b, misaka_palw_tir::prim::Cmp::Gt);
                pre.select(cmp, a, b, DType::I16)
            }
            22 => pre.topk(a, 1, 43),
            _ => panic!("nonstate tag"),
        };
        let Ref::Node(n) = target else { unreachable!() };
        let ty = pre.ty(target);
        let pre = pre.finish(&[target]);
        let mut post = pb.block("post", vec![ty]);
        let y = post.reshape_fixed(Ref::CarryIn(0), &[129]);
        post.commit(y);
        let post = post.finish(&[]);
        let program = pb.finish(pre, vec![], post, 0);
        let mut params = MapParams::default();
        for (i, d) in program.params.iter().enumerate() {
            let shape: Vec<_> = d.shape.iter().map(|n| *n as usize).collect();
            let len: usize = shape.iter().product();
            let values = if d.dtype == DType::Idx { vec![0, 3, 2] } else { (0..len).map(|j| 1 + (j % 7) as i128).collect() };
            params.tensors.insert((i as u16, None), Tensor::new(d.dtype, shape, values).unwrap());
        }
        (program, params, n)
    }
    #[test]
    fn flat_court_all_nonstate_primitives_match_reference_and_convict_tail_falsification() {
        let mut covered = BTreeSet::new();
        for tag in 0..23 {
            let (p, m, n) = primitive_fixture(tag);
            assert_eq!(p.blocks[0].nodes[n as usize].prim.tag(), tag);
            covered.insert(tag);
            let honest = Trial::of(p.clone(), m.clone(), vec![1], None);
            let output = honest.own.value(0, 0, n).unwrap();
            let layout = FlatLayoutV4::of(&shape64(&output.shape)).unwrap();
            for index in 0..layout.leaves() {
                let f = honest.filing(0, 0, n, index, hash(output, index));
                assert_eq!(verify_flat_leaf_fault_v4(&honest.context(), &f), Err(DismissalV1::NoFault), "tag {tag} leaf {index}");
                let wire = f.to_bytes().unwrap();
                assert_eq!(FlatLeafFaultV4::from_bytes(&wire).unwrap(), f);
            }
            let index = layout.leaves() - 1;
            let (first, count) = layout.span(index).unwrap();
            let e = first + count as u64 - 1;
            let bad = Trial::of(p, m, vec![1], Some((0, 0, n, e)));
            let mut actual = output.clone();
            actual.data[e as usize] += 1;
            let f = bad.filing(0, 0, n, index, hash(&actual, index));
            assert!(verify_flat_leaf_fault_v4(&bad.context(), &f).is_ok(), "tag {tag}");
            assert!(
                f.to_bytes().unwrap().len() as u64 <= flat_leaf_court_bounds_v4(&bad.context(), 0, 0, n, index).unwrap().filing_bytes
            );
        }
        assert_eq!(covered.len(), 23);
    }
    pub(crate) fn state_fixture() -> (TirProgramV1, MapParams, u16, u16) {
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let state = pb.fixed_state("accumulator", DType::I16, &[129], -20, 20, false);
        let hist = pb.hist_state("history", DType::I16, &[129], 2, false);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.cast(Ref::Input(0), DType::I16);
        let row = pre.broadcast(x, &[Dim::Fixed(129)]);
        let sum = pre.add(row, Ref::State(state), DType::I32);
        let write = pre.state_write(state, sum);
        let window = pre.hist_append(hist, row);
        let Ref::Node(wn) = write else { unreachable!() };
        let Ref::Node(hn) = window else { unreachable!() };
        let pre = pre.finish(&[write]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I16, &[129])]);
        let y = post.cast(Ref::CarryIn(0), DType::I16);
        post.commit(y);
        let post = post.finish(&[]);
        (pb.finish(pre, vec![], post, 0), MapParams::default(), wn, hn)
    }
    #[test]
    fn flat_court_state_and_history_bind_previous_position_and_initialization() {
        let (p, m, wn, hn) = state_fixture();
        let tokens = vec![1, 2, 3, 1];
        let honest = Trial::of(p.clone(), m.clone(), tokens.clone(), None);
        for position in 0..4 {
            for node in [wn, hn] {
                let output = honest.own.value(position, 0, node).unwrap();
                let layout = FlatLayoutV4::of(&shape64(&output.shape)).unwrap();
                for index in 0..layout.leaves() {
                    let f = honest.filing(position, 0, node, index, hash(output, index));
                    assert_eq!(verify_flat_leaf_fault_v4(&honest.context(), &f), Err(DismissalV1::NoFault));
                }
                let index = layout.leaves() - 1;
                let (first, count) = layout.span(index).unwrap();
                let e = first + count as u64 - 1;
                let bad = Trial::of(p.clone(), m.clone(), tokens.clone(), Some((position, 0, node, e)));
                let mut actual = output.clone();
                actual.data[e as usize] += 1;
                let f = bad.filing(position, 0, node, index, hash(&actual, index));
                assert!(verify_flat_leaf_fault_v4(&bad.context(), &f).is_ok());
                if node == hn && position > 0 {
                    // First leaf reads old rows; replacing its authenticated window
                    // with another position/node cannot reset or skip the history.
                    let mut actual = output.clone();
                    actual.data[0] += 1;
                    let bad = Trial::of(p.clone(), m.clone(), tokens.clone(), Some((position, 0, node, 0)));
                    let mut f = bad.filing(position, 0, node, 0, hash(&actual, 0));
                    assert!(verify_flat_leaf_fault_v4(&bad.context(), &f).is_ok());
                    f.inputs[1].node = bad.public.node_opening(position, 0, wn);
                    assert!(verify_flat_leaf_fault_v4(&bad.context(), &f).is_err());
                }
            }
        }
        // Fixed state is read by the Add immediately before StateWrite.
        let add = wn - 1;
        let output = honest.own.value(2, 0, add).unwrap();
        let mut actual = output.clone();
        actual.data[0] += 1;
        let bad = Trial::of(p, m, tokens, Some((2, 0, add, 0)));
        let mut f = bad.filing(2, 0, add, 0, hash(&actual, 0));
        assert!(verify_flat_leaf_fault_v4(&bad.context(), &f).is_ok());
        assert_eq!(f.inputs[1].node.as_ref().unwrap().position, 1);
        f.inputs[1].node = bad.public.node_opening(0, 0, wn);
        assert!(verify_flat_leaf_fault_v4(&bad.context(), &f).is_err());
    }

    #[test]
    fn flat_court_binds_segment_boundary_and_authenticates_public_token_tiles() {
        let (p, m, wn, _) = state_fixture();
        let tokens = vec![1; 1025];
        let target = wn - 1;
        let trial = Trial::of(p, m, tokens, Some((1024, 0, target, 0)));
        let mut actual = trial.own.value(1024, 0, target).unwrap().clone();
        actual.data[0] += 1;
        let mut f = trial.filing(1024, 0, target, 0, hash(&actual, 0));
        assert_eq!(f.inputs[1].node.as_ref().unwrap().position, 1023);
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_ok());
        let mut c = trial.context();
        c.claim.segment_roots = &trial.roots[1..];
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
        f.inputs[1].node = trial.public.node_opening(1022, 0, wn);
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_err());
        // The scalar token is a small v3 source of the row Broadcast.
        let row = wn - 2;
        let mut actual = trial.own.value(1024, 0, row).unwrap().clone();
        actual.data[0] += 1;
        let mut posted = trial.own.clone();
        posted.values[1024][0][row as usize].data[0] += 1;
        let commitments = posted
            .values
            .iter()
            .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v4(t).unwrap()).collect()).collect())
            .collect();
        let mut trial = trial;
        trial.public = SegmentedCommitmentsV1::new(commitments);
        trial.roots = trial.public.segment_roots();
        let f = trial.filing(1024, 0, row, 0, hash(&actual, 0));
        assert_eq!(f.inputs[0].legacy.len(), 1);
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_ok());
        // A direct token Broadcast exercises public token proof binding.
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.broadcast(Ref::Input(0), &[Dim::Fixed(65)]);
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::Idx, &[65])]);
        let y = post.cast(Ref::CarryIn(0), DType::Idx);
        post.commit(y);
        let post = post.finish(&[]);
        let trial = Trial::of(pb.finish(pre, vec![], post, 0), MapParams::default(), vec![1, 2], Some((1, 0, 0, 64)));
        let mut actual = trial.own.value(1, 0, 0).unwrap().clone();
        actual.data[64] += 1;
        let mut f = trial.filing(1, 0, 0, 0, hash(&actual, 0));
        let mut c = trial.context();
        c.claim.inline_prompt = None;
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
        f.token = PromptTileOpeningV1::of(&trial.tokens, 0);
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_ok());
        f.token.as_mut().unwrap().ids[1] = 3;
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
    }
    #[test]
    fn flat_court_recomputes_batched_nonzero_axis_gather_and_ignores_duplicate_data_reads() {
        let mut pb = ProgramBuilder::new(5, HISTORY_BOUND_V1_SMALL);
        let a = pb.param("routed.weights", DType::I16, &[2, 5, 43], false);
        let idx = pb.param("routing", DType::Idx, &[2, 3], false);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.gather(a, idx, 1, 1);
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I16, &[2, 3, 43])]);
        let y = post.reshape_fixed(Ref::CarryIn(0), &[258]);
        post.commit(y);
        let post = post.finish(&[]);
        let p = pb.finish(pre, vec![], post, 0);
        let mut m = MapParams::default();
        m.tensors.insert((0, None), Tensor::new(DType::I16, vec![2, 5, 43], (0..430).map(|e| (e % 127) as i128).collect()).unwrap());
        m.tensors.insert((1, None), Tensor::new(DType::Idx, vec![2, 3], vec![4, 4, 0, 1, 1, 3]).unwrap());
        let trial = Trial::of(p, m, vec![1], Some((0, 0, 0, 257)));
        let mut actual = trial.own.value(0, 0, 0).unwrap().clone();
        actual.data[257] += 1;
        let f = trial.filing(0, 0, 0, 3, hash(&actual, 3));
        assert_eq!(f.inputs[0].legacy.len(), 2);
        assert_eq!(f.inputs[1].legacy.len(), 1);
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_ok());
        let mut bad = f.clone();
        bad.inputs[1].legacy[0].values[2] = 0;
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &bad).is_err());
    }

    #[test]
    fn flat_court_semantic_refusal_requires_authenticated_operands_and_wide_sources_remain_v3() {
        let (p, m, n) = primitive_fixture(14);
        let mut trial = Trial::of(p, m, vec![1], None);
        trial.params.tensors.get_mut(&(1, None)).unwrap().data[0] = 0;
        trial.pc = ParamCommitmentsV1::of_v3(&trial.params);
        let f = trial.filing(0, 0, n, 0, hash(trial.own.value(0, 0, n).unwrap(), 0));
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_ok(), "no defined result for division by zero");
        let mut missing = f.clone();
        missing.inputs[1].legacy.clear();
        assert!(matches!(verify_flat_leaf_fault_v4(&trial.context(), &missing), Err(DismissalV1::NotAuthentic(_))));
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let w = pb.param("wide.weight", DType::I64, &[129], false);
        let mut pre = pb.block("pre", vec![]);
        let wide = pre.cast(w, DType::I128);
        let x = pre.cast(wide, DType::I32);
        let Ref::Node(n) = x else { unreachable!() };
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I32, &[129])]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        let mut m = MapParams::default();
        m.tensors.insert((0, None), Tensor::new(DType::I64, vec![129], vec![7; 129]).unwrap());
        let trial = Trial::of(pb.finish(pre, vec![], post, 0), m, vec![1], Some((0, 0, n, 128)));
        let mut actual = trial.own.value(0, 0, n).unwrap().clone();
        actual.data[128] += 1;
        let f = trial.filing(0, 0, n, 1, hash(&actual, 1));
        assert_eq!(f.inputs[0].legacy[0].dtype, DType::I128.tag());
        assert!(f.inputs[0].flat.is_empty());
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_ok());
        assert_eq!(FlatLeafFaultV4::from_bytes(&f.to_bytes().unwrap()).unwrap(), f);
    }
    #[test]
    fn flat_court_encoder_ids_count_and_padding_are_job_bound() {
        let mut pb = ProgramBuilder::new(5, HISTORY_BOUND_V1_SMALL);
        let ids = pb.param(crate::seg_encoder::ENCODER_IDS_PARAM_V1, DType::Idx, &[65], false);
        let count = pb.param(crate::seg_encoder::ENCODER_COUNT_PARAM_V1, DType::Idx, &[], false);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.add(ids, count, DType::I32);
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I32, &[65])]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        let program = pb.finish(pre, vec![], post, 0);
        let params = MapParams::default();
        let tokens = vec![1, 4, 2];
        let encoder = crate::seg_encoder::encoder_binding_v1(&program).unwrap();
        let own = crate::seg_encoder::trace_encoder_v1(&program, &params, &encoder, &tokens).unwrap();
        let pc = ParamCommitmentsV1::default();
        let commitments = own
            .values
            .iter()
            .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v4(t).unwrap()).collect()).collect())
            .collect();
        let public = SegmentedCommitmentsV1::new(commitments);
        let roots = public.segment_roots();
        let mut trial = Trial { program, params, own, public, pc, tokens, roots, encoder: Some(encoder) };
        let f = trial.filing(0, 0, 0, 0, hash(trial.own.value(0, 0, 0).unwrap(), 0));
        assert!(f.inputs.iter().all(|o| o.flat.is_empty() && o.legacy.is_empty() && o.node.is_none()));
        assert_eq!(verify_flat_leaf_fault_v4(&trial.context(), &f), Err(DismissalV1::NoFault));
        let mut actual = trial.own.value(0, 0, 0).unwrap().clone();
        actual.data[64] += 1;
        trial.public.commitments[0][0][0] = flat_profile_commitment_v4(&actual).unwrap();
        trial.public = SegmentedCommitmentsV1::new(trial.public.commitments);
        trial.roots = trial.public.segment_roots();
        let mut f = trial.filing(0, 0, 0, 0, hash(&actual, 0));
        assert!(verify_flat_leaf_fault_v4(&trial.context(), &f).is_ok());
        let mut c = trial.context();
        c.claim.inline_prompt = None;
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
        f.token = PromptTileOpeningV1::of(&trial.tokens, 0);
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_ok());
        let mut c = trial.context();
        c.claim.prompt_len = 2;
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
        let mut c = trial.context();
        c.claim.encoder.as_mut().unwrap().l = 66;
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
        f.token.as_mut().unwrap().ids[0] = 4;
        assert!(verify_flat_leaf_fault_v4(&c, &f).is_err());
    }
    #[test]
    fn flat_court_all_hostile_nested_counts_and_tags_are_bounded_before_bodies() {
        let mut r = Cursor::new(u32::MAX.to_le_bytes());
        assert!(vector::<_, Digest>(&mut r, 64, |_| panic!("count was not refused")).is_err());
        let mut r = Cursor::new([2]);
        assert!(optional::<_, Digest>(&mut r, |_| panic!("unknown option parsed")).is_err());
        let shape_count = [DType::I8.tag(), 255, 255, 255, 255];
        assert!(read_legacy(&mut Cursor::new(shape_count)).is_err());
        let metadata = borsh::to_vec(&(DType::I8.tag(), vec![65u64], AXIS_ROW, 0u64, 0u64, u32::MAX)).unwrap();
        assert!(read_legacy(&mut Cursor::new(metadata)).is_err());
        let (p, m) = identity(65);
        let trial = Trial::of(p, m, vec![1], None);
        let f = trial.filing(0, 1, 0, 0, hash(trial.own.value(0, 1, 0).unwrap(), 0));
        let mut bad = f.clone();
        bad.inputs.resize(10, FlatOperandV4::default());
        assert!(FlatLeafFaultV4::from_bytes(&borsh::to_vec(&bad).unwrap()).is_err());
        let mut bad = f.clone();
        bad.token = Some(PromptTileOpeningV1 { index: 0, ids: vec![0; 4097], siblings: vec![] });
        assert!(FlatLeafFaultV4::from_bytes(&borsh::to_vec(&bad).unwrap()).is_err());
        let mut bad = f;
        bad.token = Some(PromptTileOpeningV1 { index: 0, ids: vec![1], siblings: vec![[0; 64]; 11] });
        assert!(FlatLeafFaultV4::from_bytes(&borsh::to_vec(&bad).unwrap()).is_err());
        let mut t = Tensor::new(DType::I8, vec![65], vec![1; 65]).unwrap();
        t.data[64] = 256;
        assert!(crate::merkle4::flat_commitment_from_tensor_v4(&t).is_err());
        t.data.pop();
        assert!(crate::merkle4::flat_commitment_from_tensor_v4(&t).is_err());
        assert!(crate::merkle4::flat_commitment_from_tensor_v4(&Tensor::zeros(DType::I128, &[65])).is_err());
    }
}
