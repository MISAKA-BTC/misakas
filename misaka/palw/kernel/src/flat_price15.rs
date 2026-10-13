//! Candidate cost namespace v15; no legacy scope, verifier, fence or reward
//! route selects it. Frozen v9 remains byte-for-byte unchanged. This prices
//! complete authenticated reductions, never the outsider's private prefix or
//! a one-input witness for a false-high maximum. Only a last-axis contiguous
//! flat reduction reuses the already conservative contiguous-row leaf bound.
//! The entire scalar dependency, original parser/work/scratch/file caps, and
//! maximum127-element merged tails remain charged. New ordinary checker/court
//! integration and explicit activation are required before this can admit jobs.
use crate::{
    element::{OperandSourceV1, PreparedV1, node_opening_bytes_v1, operand_sources, prompt_tile_bytes_v1},
    flat_court4::{
        self as common, FLAT_FAULT_MAX_BYTES_V4, FLAT_FAULT_MAX_LEAVES_V4, FLAT_FAULT_MAX_SCRATCH_V4, FLAT_FAULT_MAX_WORK_V4,
    },
    flat_point9::PointOperandKindV9,
    hash::{Digest, object_id},
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::{LayoutV3, TILE_V3},
    merkle4::FlatLayoutV4,
    merkle5::flat_material_bounds_v5,
    seg::PROMPT_TILE_IDS_V1,
    trace::SourceV1,
    verify::DismissalV1,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{DType, Prim, program::TirProgramV1};
pub fn candidate_point_price_id_v15() -> Digest {
    crate::hash::id(b"misaka-palw/kernel/candidate-point-price/v15",b"full-authenticated-reduction;contiguous-last-axis-flat64;frozen-tir25/v5/v3;original1024leaves/1MiB/2^30/32MiB;pricing-only-no-admission")
}
/// Full scalar last-axis dependency, including possible boundary overlap and
/// the merged final tail. This is a bound on authenticated leaves, never an
/// assertion that a private replay makes any unopened input true.
pub fn contiguous_reduction_leaf_bound_v15(shape: &[usize]) -> Option<u64> {
    let layout = FlatLayoutV4::of(&shape64(shape))?;
    let reads = u64::try_from(*shape.last()?).ok()?;
    Some(layout.leaves().min(reads).min(reads.div_ceil(64).checked_add(1)?))
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PointRelationPriceV15 {
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
impl PointRelationPriceV15 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/candidate-point-price/v15", self)
    }
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
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
fn relation_quote_v15(
    program: &TirProgramV1,
    max_positions: u32,
    pr: &PreparedV1<'_>,
    p: u32,
    s: u16,
    n: u16,
    program_bytes: u64,
) -> Result<PointRelationPriceV15, DismissalV1> {
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
        flat_material_bounds_v5(node.out.dtype, &shape64(&sh.out)).ok_or_else(|| bad("invalid flat point output"))?.leaf_hash_bytes
            + 4
            + 127 * node.out.dtype.width() as u64
    } else {
        legacy_leaf_bytes(node.out.dtype, &sh.out, AXIS_ROW)?
    };
    let sources = operand_sources(pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    let mut out = PointRelationPriceV15 {
        version: 15,
        court_id: candidate_point_price_id_v15(),
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
                if matches!(node.prim, Prim::MatMul) && i == 0
                    || matches!(node.prim, Prim::ReduceSum { axis } | Prim::ReduceMax { axis } if axis as usize + 1 == shape.len())
                {
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
    debug_assert_eq!(total_leaves, 1 + out.operand_leaves.iter().sum::<u64>());
    Ok(out)
}
/// Diagnostic cost quotation is separate from the bounded price API. Even a
/// true result here is only the ordinary relation's resource cap, never complete
/// scope admission, activated checker semantics, or eligibility for a Final.
pub fn price_within_frozen_court_caps_v15(price: &PointRelationPriceV15) -> bool {
    price.filing_bytes <= FLAT_FAULT_MAX_BYTES_V4
        && price.work <= FLAT_FAULT_MAX_WORK_V4
        && price.scratch_bytes <= FLAT_FAULT_MAX_SCRATCH_V4
        && price.operand_leaves.iter().sum::<u64>().saturating_add(1) <= FLAT_FAULT_MAX_LEAVES_V4 as u64
}
fn bounded_price(price: PointRelationPriceV15) -> Result<PointRelationPriceV15, DismissalV1> {
    if price_within_frozen_court_caps_v15(&price) {
        Ok(price)
    } else {
        Err(bad("ordinary point relation exceeds frozen court resource caps"))
    }
}
pub fn point_relation_price_v15(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
    p: u32,
    s: u16,
    n: u16,
) -> Result<PointRelationPriceV15, DismissalV1> {
    if max_positions == 0 || max_positions > program.history_bound || max_positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(bad("invalid point scope extent"));
    }
    let bytes = common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| bad("oversized point program"))?;
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    bounded_price(relation_quote_v15(program, max_positions, &pr, p, s, n, bytes)?)
}

/// One bounded metadata walk of every unrolled relation at the supplied
/// position. Kept separate from complete-class admission: the caller must
/// cover grammar boundaries and all special courts before a future activation.
pub fn all_point_relation_prices_v15(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
    p: u32,
) -> Result<Vec<(u16, u16, Result<PointRelationPriceV15, DismissalV1>)>, DismissalV1> {
    Ok(all_point_relation_quotes_v15(program, max_positions, encoder, p)?
        .into_iter()
        .map(|(s, n, q)| (s, n, q.and_then(bounded_price)))
        .collect())
}
/// Quotes retain the requested cost even when it exceeds frozen parser/court
/// limits. Only a diagnostic can consume this API; the bounded price API keeps
/// refusing those relations. No descriptor, checker, court, or fence selects it.
pub fn all_point_relation_quotes_v15(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
    p: u32,
) -> Result<Vec<(u16, u16, Result<PointRelationPriceV15, DismissalV1>)>, DismissalV1> {
    if max_positions == 0
        || max_positions > program.history_bound
        || max_positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD
        || p >= max_positions
    {
        return Err(bad("invalid candidate price15 extent"));
    }
    let bytes = common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| bad("oversized candidate price15 program"))?;
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    if pr.node_count as u128 * 23 > crate::flat_scope9::MAX_SCOPE_RELATION_VISITS_V9 {
        return Err(bad("candidate price15 metadata visit cap"));
    }
    let mut out = vec![];
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        for n in 0..program.blocks[*b as usize].nodes.len() {
            out.push((s as u16, n as u16, relation_quote_v15(program, max_positions, &pr, p, s as u16, n as u16, bytes)));
        }
    }
    Ok(out)
}
