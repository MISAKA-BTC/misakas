//! Explicit ordinary scalar courts over unchanged flat-v5 node/v3 parameter commitments.
//! The new proof grammar authenticates one output value and the frozen TIR25 evaluator's
//! dependency line. MatMul's second registered parameter is opened along its v3 column.
//! This does not change v5/v7 admission, commitments, semantics, activation or Final rights.
use crate::{
    element::{
        MissingV1, OperandSourceV1, OperandsV1, PreparedV1, element_value_v1, node_opening_bytes_v1, operand_sources,
        prompt_tile_bytes_v1, token_at,
    },
    flat_court4::{
        self as common, FLAT_FAULT_MAX_BYTES_V4, FLAT_FAULT_MAX_LEAVES_V4, FLAT_FAULT_MAX_SCRATCH_V4, FLAT_FAULT_MAX_WORK_V4,
    },
    flat_court5::{FlatOperandV5, FlatSegContextV5},
    hash::{Digest, object_id},
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::{LayoutV3, LeafOpeningV3, TILE_V3},
    merkle4::FlatLayoutV4,
    merkle5::{FlatLeafValueV5, flat_material_bounds_v5},
    seg::{PROMPT_TILE_IDS_V1, PromptTileOpeningV1},
    trace::SourceV1,
    verify::DismissalV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{DType, Prim, Tensor, program::TirProgramV1};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::{self, Cursor, Read},
};
pub const POINT_FAULT_VERSION_V9: u16 = 9;
pub fn ordinary_point_court_id_v9() -> Digest {
    crate::hash::id(b"misaka-palw/kernel/ordinary-point-court/v9", b"frozen-tir25;one-authenticated-output-value;flat-v5-internal/v3-param;MatMul-param-second-column;other-row;bounded-codec;1MiB/2^30/32MiB/1024;no-TopK")
}
pub struct PointContextV9<'a> {
    pub court_id: Digest,
    pub base: FlatSegContextV5<'a>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub enum PointOperandKindV9 {
    Public,
    Flat,
    Row,
    Column,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PointRelationScopeV9 {
    pub version: u16,
    pub court_id: Digest,
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub output_elements: u64,
    pub output_leaf_bytes: u64,
    pub operand_kinds: Vec<PointOperandKindV9>,
    pub operand_leaves: Vec<u64>,
    pub operand_reads: Vec<u64>,
    pub operand_leaf_bytes: Vec<u64>,
    pub filing_bytes: u64,
    pub work: u64,
    pub scratch_bytes: u64,
}
impl PointRelationScopeV9 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/ordinary-point-scope/v9", self)
    }
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bounded point-v9 fault")
}
fn shape64(shape: &[usize]) -> Vec<u64> {
    shape.iter().map(|n| *n as u64).collect()
}
fn legacy_leaf_bytes(dtype: DType, shape: &[usize], axis: u8) -> Result<u64, DismissalV1> {
    let l = LayoutV3::try_of(shape).ok_or_else(|| bad("invalid point operand shape"))?;
    let line = if axis == AXIS_COL { l.col_len } else { l.row_len };
    Ok(94 + 8 * shape.len() as u64 + line.min(TILE_V3) * dtype.width() as u64 + 64 * crate::merkle3::depth_v3(l.leaves(axis)))
}
fn kind(src: &OperandSourceV1, node: &misaka_palw_tir::program::Node, i: usize, dtype: DType, shape: &[usize]) -> PointOperandKindV9 {
    match src {
        OperandSourceV1::Source(SourceV1::Node { .. }) if common::uses_flat::<5>(dtype, shape) => PointOperandKindV9::Flat,
        OperandSourceV1::Source(SourceV1::Param { .. }) if i == 1 && matches!(node.prim, Prim::MatMul) => PointOperandKindV9::Column,
        OperandSourceV1::Source(SourceV1::Node { .. } | SourceV1::Param { .. }) => PointOperandKindV9::Row,
        _ => PointOperandKindV9::Public,
    }
}
pub(crate) fn relation_priced_v9(
    program: &TirProgramV1,
    max_positions: u32,
    pr: &PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
    program_bytes: u64,
) -> Result<PointRelationScopeV9, DismissalV1> {
    if p >= max_positions || !pr.valid(s, n) {
        return Err(bad("no such ordinary point relation"));
    }
    let node = pr.w.node(s, n);
    if matches!(node.prim, Prim::TopK { .. }) {
        return Err(bad("TopK requires its separately bound v7 counterexample court"));
    }
    let sh = pr.shapes(p, s, n);
    let output_elements = LayoutV3::try_of(&sh.out).ok_or_else(|| bad("invalid point output shape"))?.len;
    let output_leaf_bytes = if common::uses_flat::<5>(node.out.dtype, &sh.out) {
        u64::try_from(
            flat_material_bounds_v5(node.out.dtype, &shape64(&sh.out))
                .ok_or_else(|| bad("invalid flat point output"))?
                .leaf_hash_bytes
                + 4
                + 127 * node.out.dtype.width() as u64,
        )
        .map_err(|_| bad("point output byte overflow"))?
    } else {
        legacy_leaf_bytes(node.out.dtype, &sh.out, AXIS_ROW)?
    };
    let sources = operand_sources(pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    let mut out = PointRelationScopeV9 {
        version: 9,
        court_id: ordinary_point_court_id_v9(),
        position: p,
        occurrence: s,
        node: n,
        output_elements,
        output_leaf_bytes,
        operand_kinds: vec![],
        operand_leaves: vec![],
        operand_reads: vec![],
        operand_leaf_bytes: vec![],
        filing_bytes: 2 + 8 + 4 + 1 + node_opening_bytes_v1(pr.node_count) + 4 + output_leaf_bytes + 4 + 4 + 1,
        work: 4096 + program_bytes.saturating_mul(1024) + pr.node_count.saturating_mul(256),
        scratch_bytes: 8 << 20,
    };
    // The output carries exactly one leaf and all authenticated inputs remain under the
    // frozen 1024-leaf combined parser cap. Public operand tensors are charged separately.
    let mut total_leaves = 1u64;
    for (i, src) in sources.iter().enumerate() {
        let shape = &sh.inputs[i];
        let dtype = sh.in_dtypes[i];
        let l = LayoutV3::try_of(shape).ok_or_else(|| bad("invalid point input shape"))?;
        let k = kind(src, node, i, dtype, shape);
        let reads = match node.prim {
            Prim::MatMul => {
                if i == 1 {
                    shape[shape.len() - 2] as u64
                } else {
                    *shape.last().unwrap() as u64
                }
            }
            Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => shape[axis as usize] as u64,
            _ => 1,
        };
        let axis = if k == PointOperandKindV9::Column { AXIS_COL } else { AXIS_ROW };
        let mut leaves = 0;
        let mut leaf_bytes = 0;
        let values;
        out.filing_bytes = out.filing_bytes.saturating_add(1 + 4 + 4);
        if k != PointOperandKindV9::Public {
            if matches!(src, OperandSourceV1::Source(SourceV1::Node { .. })) {
                out.filing_bytes = out.filing_bytes.saturating_add(node_opening_bytes_v1(pr.node_count));
            }
            if k == PointOperandKindV9::Flat {
                let f = FlatLayoutV4::of(&shape64(shape)).ok_or_else(|| bad("invalid point flat input"))?;
                let material = flat_material_bounds_v5(dtype, &shape64(shape)).ok_or_else(|| bad("invalid point flat material"))?;
                leaves = reads.min(f.leaves());
                // A MatMul first operand is one contiguous row. Other contractions may
                // be strided and retain the conservative one-leaf-per-read envelope.
                if matches!(node.prim, Prim::MatMul) && i == 0 {
                    leaves = leaves.min(reads.div_ceil(64) + 1);
                }
                leaf_bytes = material.leaf_hash_bytes + 4 + 127 * dtype.width() as u64;
                values = 127;
            } else {
                leaves = reads.min(l.leaves(axis));
                if matches!(node.prim, Prim::MatMul) && (i == 0 || k == PointOperandKindV9::Column) {
                    leaves = leaves.min(reads.div_ceil(TILE_V3));
                }
                leaf_bytes = legacy_leaf_bytes(dtype, shape, axis)?;
                values = if axis == AXIS_COL { l.col_len } else { l.row_len }.min(TILE_V3);
            }
            total_leaves = total_leaves.saturating_add(leaves);
            out.filing_bytes = out.filing_bytes.saturating_add(leaves.saturating_mul(leaf_bytes));
            out.scratch_bytes = out.scratch_bytes.saturating_add(leaves.saturating_mul(values * 16 + 5120));
        } else {
            match src {
                OperandSourceV1::Source(SourceV1::Const(j)) => {
                    out.scratch_bytes = out.scratch_bytes.saturating_add(program.consts[*j as usize].data.len() as u64 * 16)
                }
                OperandSourceV1::Token | OperandSourceV1::Source(SourceV1::Input { k: 0, .. }) => {
                    out.filing_bytes = out.filing_bytes.saturating_add(prompt_tile_bytes_v1(PROMPT_TILE_IDS_V1 as u64, 10));
                    out.scratch_bytes = out.scratch_bytes.saturating_add(PROMPT_TILE_IDS_V1 as u64 * 16);
                }
                _ => {}
            }
        }
        // Cover reduced reference tensors and parser/hash/lookup costs; no whole operand allocation.
        out.work = out.work.saturating_add(reads.saturating_mul(4096)).saturating_add(leaves.saturating_mul(4096));
        out.scratch_bytes = out.scratch_bytes.saturating_add(reads.saturating_mul(128));
        out.operand_kinds.push(k);
        out.operand_leaves.push(leaves);
        out.operand_reads.push(reads);
        out.operand_leaf_bytes.push(leaf_bytes);
    }
    out.work = out.work.saturating_add(out.filing_bytes.saturating_mul(128));
    out.scratch_bytes = out.scratch_bytes.saturating_add(out.filing_bytes);
    if out.filing_bytes > FLAT_FAULT_MAX_BYTES_V4
        || out.work > FLAT_FAULT_MAX_WORK_V4
        || out.scratch_bytes > FLAT_FAULT_MAX_SCRATCH_V4
        || total_leaves > FLAT_FAULT_MAX_LEAVES_V4 as u64
    {
        return Err(bad("ordinary point relation exceeds frozen court resource caps"));
    }
    Ok(out)
}
pub fn point_relation_scope_v9(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
    p: u32,
    s: u16,
    n: u16,
) -> Result<PointRelationScopeV9, DismissalV1> {
    if max_positions == 0 || max_positions > program.history_bound || max_positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(bad("invalid point scope extent"));
    }
    let bytes = common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| bad("oversized point program"))?;
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    relation_priced_v9(program, max_positions, &pr, p, s, n, bytes)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatPointFaultV9 {
    pub version: u16,
    pub element: u64,
    pub output: FlatOperandV5,
    pub inputs: Vec<FlatOperandV5>,
    pub token: Option<PromptTileOpeningV1>,
}
fn read_legacy<R: Read>(r: &mut R) -> io::Result<LeafOpeningV3> {
    let tag = u8::deserialize_reader(r)?;
    let dtype = DType::ALL.into_iter().find(|d| d.tag() == tag).ok_or_else(invalid)?;
    let shape = common::vector(r, 4, u64::deserialize_reader)?;
    if shape.contains(&0) {
        return Err(invalid());
    }
    let sh = shape.iter().map(|v| usize::try_from(*v).map_err(|_| invalid())).collect::<io::Result<Vec<_>>>()?;
    let layout = LayoutV3::try_of(&sh).ok_or_else(invalid)?;
    let axis = u8::deserialize_reader(r)?;
    let line = u64::deserialize_reader(r)?;
    let tile = u64::deserialize_reader(r)?;
    if axis != AXIS_ROW && axis != AXIS_COL {
        return Err(invalid());
    }
    let count = u32::deserialize_reader(r)? as usize;
    if count > TILE_V3 as usize || layout.leaf_element_count(axis, line, tile) != Some(count as u64) {
        return Err(invalid());
    }
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| invalid())?;
    let mut buf = [0; 16];
    for _ in 0..count {
        r.read_exact(&mut buf[..dtype.width()])?;
        values.push(dtype.decode_le(&buf[..dtype.width()]));
    }
    let siblings = common::vector(r, 64, Digest::deserialize_reader)?;
    let other_root = Digest::deserialize_reader(r)?;
    let leaf = LeafOpeningV3 { dtype: tag, shape, axis, line, tile, values, siblings, other_root };
    leaf.recomputed_commitment().ok_or_else(invalid)?;
    Ok(leaf)
}
fn read_operand<R: Read>(r: &mut R, total: &mut usize) -> io::Result<FlatOperandV5> {
    let node = common::optional(r, common::read_node)?;
    let flat = common::vector(r, FLAT_FAULT_MAX_LEAVES_V4 - *total, FlatLeafValueV5::deserialize_reader)?;
    *total += flat.len();
    let legacy = common::vector(r, FLAT_FAULT_MAX_LEAVES_V4 - *total, read_legacy)?;
    *total += legacy.len();
    Ok(FlatOperandV5 { node, flat, legacy })
}
impl FlatPointFaultV9 {
    fn structure(&self) -> io::Result<()> {
        if self.version != 9 || self.output.node.is_none() || self.output.flat.len() + self.output.legacy.len() != 1 {
            return Err(invalid());
        }
        common::parts_structure(None, std::slice::from_ref(&self.output), &None)?;
        common::parts_structure(None, &self.inputs, &self.token)?;
        if self.inputs.iter().map(|o| o.flat.len() + o.legacy.len()).sum::<usize>() >= FLAT_FAULT_MAX_LEAVES_V4 {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        self.structure()?;
        common::size_with_limit(self, FLAT_FAULT_MAX_BYTES_V4)?;
        borsh::to_vec(self)
    }
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() as u64 > FLAT_FAULT_MAX_BYTES_V4 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 9 {
            return Err(invalid());
        }
        let element = u64::deserialize_reader(&mut r)?;
        let mut total = 0;
        let output = read_operand(&mut r, &mut total)?;
        if output.flat.len() + output.legacy.len() != 1 {
            return Err(invalid());
        }
        let inputs = common::vector(&mut r, 9, |r| read_operand(r, &mut total))?;
        let token = common::optional(&mut r, |r| {
            Ok(PromptTileOpeningV1 {
                index: u32::deserialize_reader(r)?,
                ids: common::vector(r, PROMPT_TILE_IDS_V1, u32::deserialize_reader)?,
                siblings: common::vector(r, 10, Digest::deserialize_reader)?,
            })
        })?;
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        let f = Self { version, element, output, inputs, token };
        f.structure()?;
        Ok(f)
    }
}
enum Input<'a> {
    Proof(&'a FlatOperandV5, DType, Vec<usize>, PointOperandKindV9),
    Full(Tensor),
    Zero,
    Scalar(i128),
}
struct Inputs<'a>(Vec<Input<'a>>);
impl OperandsV1 for Inputs<'_> {
    fn get(&self, i: usize, e: u64) -> Result<i128, MissingV1> {
        let missing = MissingV1(i, e);
        match self.0.get(i) {
            Some(Input::Proof(o, dtype, shape, PointOperandKindV9::Flat)) => {
                let l = FlatLayoutV4::of(&shape64(shape)).ok_or(missing)?;
                let index = l.leaf_of(e).ok_or(missing)?;
                let leaf = &o.flat[o.flat.binary_search_by_key(&index, |v| v.opening.index).map_err(|_| missing)?];
                let (first, _) = l.span(index).ok_or(missing)?;
                let offset = (e - first) as usize * dtype.width();
                Ok(dtype.decode_le(leaf.bytes.get(offset..offset + dtype.width()).ok_or(missing)?))
            }
            Some(Input::Proof(o, _, shape, k)) => {
                let l = LayoutV3::try_of(shape).ok_or(missing)?;
                let (line, tile, at) =
                    if *k == PointOperandKindV9::Column { l.col_leaf_of(e) } else { l.row_leaf_of(e) }.ok_or(missing)?;
                let leaf = &o.legacy[o.legacy.binary_search_by_key(&(line, tile), |v| (v.line, v.tile)).map_err(|_| missing)?];
                leaf.values.get(at as usize).copied().ok_or(missing)
            }
            Some(Input::Full(t)) => t.data.get(e as usize).copied().ok_or(missing),
            Some(Input::Zero) => Ok(0),
            Some(Input::Scalar(v)) if e == 0 => Ok(*v),
            _ => Err(missing),
        }
    }
}
fn authenticate<'a>(
    o: &'a FlatOperandV5,
    dtype: DType,
    shape: &[usize],
    root: &Digest,
    k: PointOperandKindV9,
) -> Result<Input<'a>, DismissalV1> {
    let dims = shape64(shape);
    if k == PointOperandKindV9::Flat {
        if !o.legacy.is_empty()
            || o.flat.windows(2).any(|w| w[0].opening.index >= w[1].opening.index)
            || o.flat.iter().any(|l| l.opening.dtype != dtype.tag() || l.opening.shape != dims || !l.authenticates(root))
        {
            return Err(bad("unauthentic point flat leaves"));
        }
    } else {
        let axis = if k == PointOperandKindV9::Column { AXIS_COL } else { AXIS_ROW };
        if !o.flat.is_empty()
            || o.legacy.windows(2).any(|w| (w[0].line, w[0].tile) >= (w[1].line, w[1].tile))
            || o.legacy.iter().any(|l| l.dtype != dtype.tag() || l.shape != dims || l.axis != axis || !l.authenticates(root))
        {
            return Err(bad("unauthentic point legacy leaves or wrong derived axis"));
        }
    }
    Ok(Input::Proof(o, dtype, shape.to_vec(), k))
}
pub fn verify_flat_point_fault_v9(c: &PointContextV9<'_>, f: &FlatPointFaultV9) -> Result<(), DismissalV1> {
    f.structure().map_err(|_| bad("malformed point filing"))?;
    if c.court_id != ordinary_point_court_id_v9() {
        return Err(bad("wrong opt-in ordinary point court"));
    }
    let wire = common::size_with_limit(f, FLAT_FAULT_MAX_BYTES_V4).map_err(|_| bad("point filing byte cap"))?;
    let pr = common::prepare(&c.base)?;
    let output = f.output.node.as_ref().unwrap();
    let (p, s, n) = (output.position, output.occurrence, output.node);
    let scope = relation_priced_v9(
        c.base.claim.program,
        c.base.claim.positions,
        &pr,
        p,
        s,
        n,
        common::size_with_limit(c.base.claim.program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
            .map_err(|_| bad("oversized program"))?,
    )?;
    if f.element >= scope.output_elements || wire > scope.filing_bytes || !common::authenticate_node(&c.base, &pr, output, (p, s, n)) {
        return Err(bad("point output coordinate/root or filing bound"));
    }
    let node = pr.w.node(s, n);
    let sh = pr.shapes(p, s, n);
    let output_kind = if common::uses_flat::<5>(node.out.dtype, &sh.out) { PointOperandKindV9::Flat } else { PointOperandKindV9::Row };
    let got = Inputs(vec![authenticate(&f.output, node.out.dtype, &sh.out, &output.commitment, output_kind)?])
        .get(0, f.element)
        .map_err(|_| bad("missing committed output point"))?;
    let sources = operand_sources(&pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    if f.inputs.len() != sources.len() {
        return Err(bad("wrong point operand count"));
    }
    let mut inputs = Vec::with_capacity(sources.len());
    for (i, (src, o)) in sources.iter().zip(&f.inputs).enumerate() {
        if o.flat.len() + o.legacy.len() > scope.operand_leaves[i] as usize {
            return Err(bad("point operand exceeds derived leaf bound"));
        }
        let root = match src {
            OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                let opening = o.node.as_ref().ok_or_else(|| bad("missing point operand node"))?;
                if !common::authenticate_node(&c.base, &pr, opening, (*position, *occurrence, *node)) {
                    return Err(bad("borrowed point operand node"));
                }
                Some(opening.commitment)
            }
            OperandSourceV1::Source(SourceV1::Param { index, layer }) => {
                if o.node.is_some() {
                    return Err(bad("point parameter carries node"));
                }
                Some(
                    *c.base
                        .claim
                        .params
                        .by_instance
                        .get(&(*index, *layer))
                        .ok_or_else(|| bad("missing registered point parameter"))?,
                )
            }
            _ => None,
        };
        if let Some(root) = root {
            inputs.push(authenticate(o, sh.in_dtypes[i], &sh.inputs[i], &root, scope.operand_kinds[i])?);
            continue;
        }
        if o != &FlatOperandV5::default() {
            return Err(bad("public point operand carries proof"));
        }
        inputs.push(match src {
            OperandSourceV1::Token => Input::Scalar(
                token_at(&c.base.claim, p, f.token.as_ref())
                    .filter(|t| *t < c.base.claim.program.token_bound)
                    .ok_or_else(|| bad("missing job-bound point token"))? as i128,
            ),
            OperandSourceV1::Position => Input::Scalar(p as i128),
            OperandSourceV1::Source(SourceV1::Const(j)) => {
                Input::Full(crate::trace::const_tensor(c.base.claim.program, *j).map_err(|_| bad("invalid point const"))?)
            }
            OperandSourceV1::Source(SourceV1::Zeros { .. }) => Input::Zero,
            OperandSourceV1::Source(SourceV1::Public(v)) => Input::Scalar(*v as i128),
            OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                let encoder = c.base.claim.encoder.as_ref().ok_or_else(|| bad("unbound point stage input"))?;
                if *k == 1 {
                    Input::Full(encoder.count(c.base.claim.prompt_len).map_err(DismissalV1::NotAuthentic)?)
                } else if *k == 0 {
                    let ids = if let Some(ids) = c.base.claim.inline_prompt {
                        ids
                    } else {
                        &f.token
                            .as_ref()
                            .filter(|t| {
                                t.index == 0
                                    && t.ids.len() == c.base.claim.prompt_len as usize
                                    && t.authenticates(c.base.claim.prompt_len, &c.base.claim.prompt_root)
                            })
                            .ok_or_else(|| bad("missing point job ids"))?
                            .ids
                    };
                    if ids.iter().any(|t| *t >= c.base.claim.program.token_bound) {
                        return Err(bad("point job id outside vocabulary"));
                    }
                    Input::Full(encoder.ids(ids).map_err(DismissalV1::NotAuthentic)?)
                } else {
                    return Err(bad("unknown point stage input"));
                }
            }
            _ => return Err(bad("unbound point source")),
        });
    }
    match element_value_v1(c.base.claim.program, node, &sh, f.element, &Inputs(inputs)) {
        Err(_) => Err(bad("point dependency not opened")),
        Ok(Some(expected)) if expected == got => Err(DismissalV1::NoFault),
        Ok(_) => Ok(()),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointDependenciesV9 {
    pub kinds: Vec<PointOperandKindV9>,
    pub leaves: Vec<Vec<(u64, u64)>>,
    pub expected: Option<i128>,
}
/// Record exactly the same scalar evaluator's reads over the holder's own execution.
/// The terminal court still authenticates every opening independently.
pub fn point_dependencies_v9(
    c: &PointContextV9<'_>,
    p: u32,
    s: u16,
    n: u16,
    e: u64,
    own: &[Tensor],
) -> Result<PointDependenciesV9, DismissalV1> {
    if c.court_id != ordinary_point_court_id_v9() {
        return Err(bad("wrong point detector court"));
    }
    let pr = common::prepare(&c.base)?;
    let bytes = common::size_with_limit(c.base.claim.program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| bad("oversized point program"))?;
    let scope = relation_priced_v9(c.base.claim.program, c.base.claim.positions, &pr, p, s, n, bytes)?;
    let sh = pr.shapes(p, s, n);
    if e >= scope.output_elements
        || own.len() != sh.inputs.len()
        || own.iter().enumerate().any(|(i, t)| {
            t.dtype != sh.in_dtypes[i]
                || t.shape != sh.inputs[i]
                || LayoutV3::try_of(&t.shape).is_none_or(|l| l.len != t.data.len() as u64)
        })
    {
        return Err(bad("point detector's own metadata or coordinate mismatch"));
    }
    struct Recording<'a> {
        own: &'a [Tensor],
        scope: &'a PointRelationScopeV9,
        leaves: RefCell<Vec<BTreeSet<(u64, u64)>>>,
    }
    impl OperandsV1 for Recording<'_> {
        fn get(&self, i: usize, e: u64) -> Result<i128, MissingV1> {
            let missing = MissingV1(i, e);
            let t = self.own.get(i).ok_or(missing)?;
            let value = *t.data.get(e as usize).filter(|v| t.dtype.contains(**v)).ok_or(missing)?;
            let coordinate = match self.scope.operand_kinds[i] {
                PointOperandKindV9::Public => None,
                PointOperandKindV9::Flat => Some((FlatLayoutV4::of(&shape64(&t.shape)).and_then(|l| l.leaf_of(e)).ok_or(missing)?, 0)),
                k => {
                    let l = LayoutV3::try_of(&t.shape).ok_or(missing)?;
                    let (a, b, _) =
                        if k == PointOperandKindV9::Column { l.col_leaf_of(e) } else { l.row_leaf_of(e) }.ok_or(missing)?;
                    Some((a, b))
                }
            };
            if let Some(coordinate) = coordinate {
                let mut all = self.leaves.borrow_mut();
                all[i].insert(coordinate);
                if all[i].len() as u64 > self.scope.operand_leaves[i] {
                    return Err(missing);
                }
            }
            Ok(value)
        }
    }
    let recorder = Recording { own, scope: &scope, leaves: RefCell::new(vec![BTreeSet::new(); own.len()]) };
    let expected = element_value_v1(c.base.claim.program, pr.w.node(s, n), &sh, e, &recorder)
        .map_err(|_| bad("point detector missing dependency or over bound"))?;
    let leaves = recorder.leaves.into_inner().into_iter().map(|s| s.into_iter().collect()).collect();
    Ok(PointDependenciesV9 { kinds: scope.operand_kinds, leaves, expected })
}
