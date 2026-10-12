//! Explicit mixed-court graph admission: unchanged flat-v5 commitments, v5
//! courts for ordinary relations, and v7 point courts for committed TopK inputs.
//! Metadata-only, all positions/occurrences/tails. This is not a node activation,
//! checkpoint-fidelity, global network-resource or ExecutionFinal certificate.
use crate::{
    element::{OperandSourceV1, PreparedV1, operand_sources},
    flat_court4::{self as common, FlatCourtBoundsV4},
    flat_court5::{decode_bounds_program, flat_trace_profile_id_v5, malformed_bounds},
    hash::{Digest, object_id},
    merkle3::LayoutV3,
    seg_encoder::EncoderBindingV1,
    trace::SourceV1,
    verify::DismissalV1,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{Prim, program::TirProgramV1};

pub fn flat_court_set_id_v7() -> Digest {
    let mut h = crate::hash::keyed(b"misaka-palw/kernel/flat-court-set/v7");
    h.update(&flat_trace_profile_id_v5()).update(&crate::flat_topk7::topk_court_id_v7());
    h.update(b"ordinary-v5;committed-TopK-point-v7;all-relations;all-positions;decode;malformed;outer-v7");
    crate::hash::finish(h)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub enum FlatRelationCourtV7 {
    V5,
    TopKPointV7,
    /// Initial zero/public state uses v5; a committed state writer uses v7.
    /// Selection is derived from wiring, never chosen by the registrant/filer.
    TopKMixedV7,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatRelationScopeV7 {
    pub block: u8,
    pub node: u16,
    pub occurrences: u32,
    pub court: FlatRelationCourtV7,
    pub filing_bytes: u64,
    pub work: u64,
    pub scratch_bytes: u64,
    pub operand_leaves: Vec<u64>,
    /// Zero for ordinary v5 relations. TopK includes every row at maximum H.
    pub selection_work: u128,
    pub selection_aux_bytes: u128,
    pub selected_value_reads: u128,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatResourceScopeV7 {
    pub version: u16,
    pub profile: Digest,
    pub court_set: Digest,
    pub program: Digest,
    pub max_positions: u32,
    pub encoder: Option<(u16, u32)>,
    pub relations: Vec<FlatRelationScopeV7>,
    pub relation_instances_per_position: u64,
    pub relation_instances_per_claim: u128,
    pub worst_court_bytes: u64,
    pub worst_court_work: u64,
    pub worst_court_scratch_bytes: u64,
    pub worst_operand_leaves: u64,
    pub trace_value_bytes_per_position: u128,
    pub trace_value_bytes_per_claim: u128,
    /// Reference tensors use i128 containers regardless of storage dtype.
    pub reference_tensor_bytes_per_position: u128,
    pub node_commitments_bytes_per_claim: u128,
    pub flat_tree_bytes_per_position: u128,
    pub flat_hash_input_bytes_per_claim: u128,
    pub topk_selection_work_per_claim: u128,
    pub topk_selection_aux_bytes: u128,
    pub topk_selected_value_reads_per_claim: u128,
    pub topk_selected_reply_bytes_per_claim: u128,
}
impl FlatResourceScopeV7 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/flat-resource-scope/v7", self)
    }
    pub(crate) fn relation(&self, block: u8, node: u16) -> Option<&FlatRelationScopeV7> {
        self.relations.iter().find(|r| r.block == block && r.node == node)
    }
    pub(crate) fn uses_point(&self, pr: &PreparedV1<'_>, p: u32, s: u16, n: u16) -> Result<bool, DismissalV1> {
        let block = pr.w.occurrences[s as usize].0;
        match self.relation(block, n).map(|r| r.court) {
            Some(FlatRelationCourtV7::V5) => Ok(false),
            Some(FlatRelationCourtV7::TopKPointV7) => Ok(true),
            Some(FlatRelationCourtV7::TopKMixedV7) => point_relation(pr, p, s, n),
            None => Err(na("complete v7 scope omitted a relation")),
        }
    }
}
fn na(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
/// `inner` is a v5 raw court bound (without its 3-byte wrapper), or a complete
/// v7 TopK bound. The mixed wrapper adds three bytes in both cases.
pub(crate) fn capped(mut b: FlatCourtBoundsV4, inner_v5: bool) -> Result<FlatCourtBoundsV4, DismissalV1> {
    let extra = if inner_v5 { 6 } else { 3 };
    b.filing_bytes = b.filing_bytes.checked_add(extra).ok_or_else(|| na("mixed scope byte overflow"))?;
    b.work = b.work.checked_add(extra * 128).ok_or_else(|| na("mixed scope work overflow"))?;
    b.scratch_bytes = b.scratch_bytes.checked_add(extra * 16).ok_or_else(|| na("mixed scope scratch overflow"))?;
    if b.filing_bytes > common::FLAT_FAULT_MAX_BYTES_V4
        || b.work > common::FLAT_FAULT_MAX_WORK_V4
        || b.scratch_bytes > common::FLAT_FAULT_MAX_SCRATCH_V4
        || b.operand_leaves.iter().map(|v| *v as u128).sum::<u128>() > common::FLAT_FAULT_MAX_LEAVES_V4 as u128
    {
        return Err(na("complete flat-v7 court scope exceeds resource caps"));
    }
    Ok(b)
}
pub(crate) fn point_relation(pr: &PreparedV1<'_>, p: u32, s: u16, n: u16) -> Result<bool, DismissalV1> {
    if !matches!(pr.w.node(s, n).prim, Prim::TopK { .. }) {
        return Ok(false);
    }
    let sources = operand_sources(pr, p, s, n).map_err(DismissalV1::NotAuthentic)?;
    Ok(matches!(sources.as_slice(), [OperandSourceV1::Source(SourceV1::Node { .. } | SourceV1::Param { .. })]))
}
pub fn flat_resource_scope_v7(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatResourceScopeV7, DismissalV1> {
    if max_positions == 0 || max_positions > program.history_bound || max_positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(na("invalid flat-v7 scope extent"));
    }
    let program_bytes = common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| na("oversized flat-v7 program"))?;
    if let Some(e) = encoder
        && (max_positions != 1 || crate::seg_encoder::encoder_binding_v1(program).ok().as_ref() != Some(e))
    {
        return Err(na("wrong flat-v7 encoder binding"));
    }
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    let topk_price = crate::flat_topk7::TopKProgramPriceV7::of(program)?;
    let p = max_positions - 1;
    let mut out = FlatResourceScopeV7 {
        version: 7,
        profile: flat_trace_profile_id_v5(),
        court_set: flat_court_set_id_v7(),
        program: object_id(b"misaka-palw/kernel/flat-resource-program/v7", program),
        max_positions,
        encoder: encoder.map(|e| (e.first_input, e.l)),
        relations: vec![],
        relation_instances_per_position: pr.node_count,
        relation_instances_per_claim: pr.node_count as u128 * max_positions as u128,
        worst_court_bytes: 0,
        worst_court_work: 0,
        worst_court_scratch_bytes: 0,
        worst_operand_leaves: 0,
        trace_value_bytes_per_position: 0,
        trace_value_bytes_per_claim: 0,
        reference_tensor_bytes_per_position: 0,
        node_commitments_bytes_per_claim: pr.node_count as u128 * max_positions as u128 * 64,
        flat_tree_bytes_per_position: 0,
        flat_hash_input_bytes_per_claim: 0,
        topk_selection_work_per_claim: 0,
        topk_selection_aux_bytes: 0,
        topk_selected_value_reads_per_claim: 0,
        topk_selected_reply_bytes_per_claim: 0,
    };
    let mut counts = vec![0u32; program.blocks.len()];
    let mut representative = vec![None; program.blocks.len()];
    let mut occurrences = vec![Vec::new(); program.blocks.len()];
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        counts[*b as usize] += 1;
        representative[*b as usize].get_or_insert(s as u16);
        occurrences[*b as usize].push(s as u16);
    }
    for (b, s) in representative.into_iter().enumerate() {
        let Some(s) = s else { continue };
        for (n, node) in program.blocks[b].nodes.iter().enumerate() {
            let n = n as u16;
            let sh = pr.shapes(p, s, n);
            let len = LayoutV3::of(&sh.out).len;
            let shape: Vec<_> = sh.out.iter().map(|d| *d as u64).collect();
            let material = if len >= 64 { crate::merkle5::flat_material_bounds_v5(node.out.dtype, &shape) } else { None };
            let (court, bound, work, aux, reads) = if matches!(node.prim, Prim::TopK { .. }) {
                let mut combined: Option<FlatCourtBoundsV4> = None;
                let (mut point, mut legacy, mut work, mut aux, mut reads) = (false, false, 0, 0, 0);
                // State sources may change from initial zeros to a prior writer,
                // or between shared layer occurrences. Cover both ends of each
                // unrolled occurrence without allocating a position array.
                for &s in &occurrences[b] {
                    for q in [0, p] {
                        let bound = if point_relation(&pr, q, s, n)? {
                            point = true;
                            let t = crate::flat_topk7::relation_scope_priced_v7(&topk_price, &pr, max_positions, q, s, n, true)?;
                            if t.selection_aux_bytes > crate::flat_topk7::TOPK_SCAN_MAX_AUX_BYTES_V7 {
                                return Err(na("TopK detector auxiliary memory exceeds v7 cap"));
                            }
                            work = work.max(t.selection_work);
                            aux = aux.max(t.selection_aux_bytes);
                            reads = reads.max(t.rows as u128 * (t.k as u128 + 3));
                            capped(
                                FlatCourtBoundsV4 {
                                    filing_bytes: t.filing_bytes,
                                    work: t.court_work,
                                    scratch_bytes: t.court_scratch_bytes,
                                    operand_leaves: vec![3, 2],
                                },
                                false,
                            )?
                        } else {
                            legacy = true;
                            let sh = pr.shapes(q, s, n);
                            let len = LayoutV3::of(&sh.out).len;
                            let shape: Vec<_> = sh.out.iter().map(|d| *d as u64).collect();
                            let bytes = if len >= 64 {
                                crate::merkle5::flat_material_bounds_v5(node.out.dtype, &shape).map(|m| m.leaf_hash_bytes).unwrap_or(0)
                            } else {
                                0
                            };
                            capped(
                                common::relation_bounds_program::<5>(
                                    program,
                                    max_positions,
                                    &pr,
                                    q,
                                    s,
                                    n,
                                    if len >= 64 { 127 } else { len },
                                    bytes,
                                    true,
                                    program_bytes,
                                )?,
                                true,
                            )?
                        };
                        if let Some(all) = &mut combined {
                            all.filing_bytes = all.filing_bytes.max(bound.filing_bytes);
                            all.work = all.work.max(bound.work);
                            all.scratch_bytes = all.scratch_bytes.max(bound.scratch_bytes);
                            all.operand_leaves.resize(all.operand_leaves.len().max(bound.operand_leaves.len()), 0);
                            for (a, b) in all.operand_leaves.iter_mut().zip(bound.operand_leaves) {
                                *a = (*a).max(b);
                            }
                        } else {
                            combined = Some(bound);
                        }
                    }
                }
                (
                    match (point, legacy) {
                        (true, true) => FlatRelationCourtV7::TopKMixedV7,
                        (true, false) => FlatRelationCourtV7::TopKPointV7,
                        _ => FlatRelationCourtV7::V5,
                    },
                    combined.ok_or_else(|| na("unreachable TopK relation"))?,
                    work,
                    aux,
                    reads,
                )
            } else {
                (
                    FlatRelationCourtV7::V5,
                    capped(
                        common::relation_bounds_program::<5>(
                            program,
                            max_positions,
                            &pr,
                            p,
                            s,
                            n,
                            if len >= 64 { 127 } else { len },
                            material.map(|m| m.leaf_hash_bytes).unwrap_or(0),
                            true,
                            program_bytes,
                        )?,
                        true,
                    )?,
                    0,
                    0,
                    0,
                )
            };
            maxima(&mut out, &bound);
            let instances = counts[b] as u128 * max_positions as u128;
            out.topk_selection_work_per_claim += work * instances;
            out.topk_selection_aux_bytes = out.topk_selection_aux_bytes.max(aux);
            out.topk_selected_value_reads_per_claim += reads * instances;
            out.topk_selected_reply_bytes_per_claim += reads * crate::flat_detect7::TOPK_VALUE_MAX_BYTES_V7 as u128 * instances;
            out.relations.push(FlatRelationScopeV7 {
                block: b as u8,
                node: n,
                occurrences: counts[b],
                court,
                filing_bytes: bound.filing_bytes,
                work: bound.work,
                scratch_bytes: bound.scratch_bytes,
                operand_leaves: bound.operand_leaves,
                selection_work: work,
                selection_aux_bytes: aux,
                selected_value_reads: reads,
            });
            out.trace_value_bytes_per_position += len as u128 * node.out.dtype.width() as u128 * counts[b] as u128;
            out.reference_tensor_bytes_per_position += len as u128 * 16 * counts[b] as u128;
            if let Some(m) = material {
                out.flat_tree_bytes_per_position += m.retained_tree_bytes * counts[b] as u128;
                out.flat_hash_input_bytes_per_claim += m.hash_input_bytes * instances;
            }
        }
    }
    maxima(&mut out, &capped(decode_bounds_program(program, &pr, p, (pr.w.occurrences.len() - 1) as u16, program.logits)?, true)?);
    maxima(&mut out, &capped(malformed_bounds(program, &pr)?, true)?);
    out.trace_value_bytes_per_claim = out.trace_value_bytes_per_position * max_positions as u128;
    Ok(out)
}
fn maxima(out: &mut FlatResourceScopeV7, b: &FlatCourtBoundsV4) {
    out.worst_court_bytes = out.worst_court_bytes.max(b.filing_bytes);
    out.worst_court_work = out.worst_court_work.max(b.work);
    out.worst_court_scratch_bytes = out.worst_court_scratch_bytes.max(b.scratch_bytes);
    out.worst_operand_leaves = out.worst_operand_leaves.max(b.operand_leaves.iter().sum());
}
pub fn check_flat_resource_scope_v7(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
    declared: &FlatResourceScopeV7,
) -> Result<(), DismissalV1> {
    if declared.relations.len() > misaka_palw_tir::program::MAX_BLOCKS * misaka_palw_tir::program::MAX_NODES_PER_BLOCK
        || declared.relations.iter().any(|r| r.operand_leaves.len() > 9)
    {
        return Err(na("oversized flat-v7 resource declaration"));
    }
    if flat_resource_scope_v7(program, max_positions, encoder)? != *declared {
        return Err(na("flat-v7 resource declaration differs from independent derivation"));
    }
    Ok(())
}
