//! Isolated scalar-column quotation after an explicitly new layout/program.
//! Frozen scope9 and candidate price15 remain unchanged. No checker, court,
//! descriptor, fence, native admission, or Final selects this namespace.
use crate::{
    element::PreparedV1,
    flat_point9::PointOperandKindV9,
    flat_price15::{PointRelationPriceV15, all_point_relation_quotes_v15},
    hash::{Digest, object_id},
    verify::DismissalV1,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{Prim, TirProgramV1};
pub fn candidate_point_price_id_v20() -> Digest {
    crate::hash::id(
        b"misaka-palw/kernel/candidate-point-price/v20",
        b"price15-full-reductions;MatMul-node-flat-second-lastdim1-contiguous-line;no-activation;frozen1024leaves/1MiB/2^30/32MiB",
    )
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PointRelationPriceV20 {
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
impl From<PointRelationPriceV15> for PointRelationPriceV20 {
    fn from(q: PointRelationPriceV15) -> Self {
        Self {
            version: 20,
            court_id: candidate_point_price_id_v20(),
            position: q.position,
            occurrence: q.occurrence,
            node: q.node,
            output_elements: q.output_elements,
            output_leaf_bytes: q.output_leaf_bytes,
            operand_kinds: q.operand_kinds,
            operand_leaves: q.operand_leaves,
            operand_reads: q.operand_reads,
            operand_leaf_bytes: q.operand_leaf_bytes,
            filing_bytes: q.filing_bytes,
            work: q.work,
            scratch_bytes: q.scratch_bytes,
        }
    }
}
impl PointRelationPriceV20 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/candidate-point-price/v20", self)
    }
    /// Only a local ordinary resource-cap quotation, not admission or semantics.
    pub fn within_frozen_court_caps(&self) -> bool {
        use crate::flat_court4::*;
        self.filing_bytes <= FLAT_FAULT_MAX_BYTES_V4
            && self.work <= FLAT_FAULT_MAX_WORK_V4
            && self.scratch_bytes <= FLAT_FAULT_MAX_SCRATCH_V4
            && self.operand_leaves.iter().sum::<u64>().saturating_add(1) <= FLAT_FAULT_MAX_LEAVES_V4 as u64
    }
}
/// With N=1, the MatMul second input [...,K,1] is a contiguous K-line at
/// each broadcast batch. Removing the singleton dimension preserves its flat
/// element order, total leaves, merged tail, and line-offset boundary envelope.
pub fn scalar_column_leaf_bound_v20(shape: &[usize]) -> Option<u64> {
    if shape.len() < 2 || shape.last() != Some(&1) {
        return None;
    }
    crate::flat_price15::contiguous_reduction_leaf_bound_v15(&shape[..shape.len() - 1])
}
pub fn all_point_relation_quotes_v20(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&crate::seg_encoder::EncoderBindingV1>,
    p: u32,
) -> Result<Vec<(u16, u16, Result<PointRelationPriceV20, DismissalV1>)>, DismissalV1> {
    let quotes = all_point_relation_quotes_v15(program, max_positions, encoder, p)?;
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    Ok(quotes
        .into_iter()
        .map(|(s, n, quote)| {
            let quote = quote.map(|q| {
                let mut q = PointRelationPriceV20::from(q);
                if matches!(pr.w.node(s, n).prim, Prim::MatMul) && q.operand_kinds.get(1) == Some(&PointOperandKindV9::Flat) {
                    let sh = pr.shapes(p, s, n);
                    if let Some(leaves) = scalar_column_leaf_bound_v20(&sh.inputs[1]) {
                        let new = q.operand_leaves[1].min(leaves);
                        let removed = q.operand_leaves[1] - new;
                        let removed_bytes = removed * q.operand_leaf_bytes[1];
                        q.operand_leaves[1] = new;
                        q.filing_bytes -= removed_bytes;
                        q.work -= removed * 4096 + removed_bytes * 128;
                        q.scratch_bytes -= removed * (127 * 16 + 5120) + removed_bytes;
                    }
                }
                q
            });
            (s, n, quote)
        })
        .collect())
}
