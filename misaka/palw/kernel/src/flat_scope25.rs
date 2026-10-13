//! Opt-in scalar ordinary-court scope over unchanged flat-v5 commitments.
//! Independently prices every unrolled occurrence and all sub-64 grammar transitions,
//! plus maximum H. This isolated scope changes no v5/v7 plan, activation or Final route.
use crate::{
    element::PreparedV1,
    flat_court4::{self as common, FlatCourtBoundsV4},
    flat_court5::{decode_bounds_program, flat_trace_profile_id_v5, malformed_bounds},
    flat_point25::{PointRelationScopeV25, ordinary_point_court_id_v25, relation_priced_v25},
    hash::{Digest, object_id},
    merkle3::LayoutV3,
    seg_encoder::EncoderBindingV1,
    verify::DismissalV1,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{Prim, program::TirProgramV1};
pub const MAX_SCOPE_RELATION_VISITS_V25: u128 = 1 << 20;
pub fn flat_court_set_id_v25() -> Digest {
    let mut h = crate::hash::keyed(b"misaka-palw/kernel/flat-court-set/v25");
    h.update(&flat_trace_profile_id_v5()).update(&ordinary_point_court_id_v25()).update(&crate::flat_topk7::topk_court_id_v7());
    h.update(&crate::flat_detect25::TYPED_VALUE_MAX_BYTES_V25.to_le_bytes())
        .update(&crate::flat_detect25::TYPED_HEADER_MAX_BYTES_V25.to_le_bytes());
    h.update(b"all-ordinary-scalar-v25;committed-TopK-v7;initial-TopK-v5;decode/malformed-v5;ordinary-v25-explicit-caps;special-frozen-v5-v7;all-occurrences/sub64/maxH;metadata-visits<=2^20");
    crate::hash::finish(h)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatOccurrenceScopeV25 {
    pub occurrence: u16,
    pub block: u8,
    pub node: u16,
    pub ordinary_point: bool,
    pub filing_bytes: u64,
    pub work: u64,
    pub scratch_bytes: u64,
    pub operand_leaves: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatResourceScopeV25 {
    pub version: u16,
    pub profile: Digest,
    pub court_set: Digest,
    pub program: Digest,
    pub max_positions: u32,
    pub encoder: Option<(u16, u32)>,
    pub relations: Vec<FlatOccurrenceScopeV25>,
    pub metadata_relation_visits: u128,
    pub metadata_derivation_work: u128,
    pub worst_court_bytes: u64,
    pub worst_court_work: u64,
    pub worst_court_scratch_bytes: u64,
    pub trace_value_bytes_per_position: u128,
    pub reference_tensor_bytes_per_position: u128,
    pub trace_value_bytes_per_claim: u128,
    pub node_commitments_bytes_per_claim: u128,
}
impl FlatResourceScopeV25 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/flat-resource-scope/v25", self)
    }
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn as_bound(p: &PointRelationScopeV25) -> FlatCourtBoundsV4 {
    FlatCourtBoundsV4 {
        filing_bytes: p.filing_bytes,
        work: p.work,
        scratch_bytes: p.scratch_bytes,
        operand_leaves: p.operand_leaves.clone(),
    }
}
fn maxima(out: &mut FlatResourceScopeV25, b: &FlatCourtBoundsV4) {
    out.worst_court_bytes = out.worst_court_bytes.max(b.filing_bytes);
    out.worst_court_work = out.worst_court_work.max(b.work);
    out.worst_court_scratch_bytes = out.worst_court_scratch_bytes.max(b.scratch_bytes);
}
// Monotone H changes neither fixed dimensions nor dtype; after reaching 64,
// flat path depth/leaf counts only increase. Point-v25 prices every flat leaf at
// 127 values, so merged-tail oscillation never hides a more expensive earlier H.
fn grammar_boundary_positions(pr: &PreparedV1<'_>, maximum: u32, s: u16, n: u16) -> Vec<u32> {
    let mut positions = BTreeSet::from([0, maximum - 1]);
    if maximum == 1 {
        return positions.into_iter().collect();
    }
    positions.insert(1);
    let small = pr.shapes(1, s, n);
    let large = pr.shapes(maximum - 1, s, n);
    for (small, large) in std::iter::once((&small.out, &large.out)).chain(small.inputs.iter().zip(&large.inputs)) {
        if let Some(axis) = small.iter().zip(large).position(|(a, b)| a != b) {
            let fixed = small.iter().enumerate().filter(|(i, _)| *i != axis).map(|(_, n)| *n as u64).product::<u64>();
            let last_legacy_h = 63 / fixed;
            // A current-H tensor uses H=min(p+1,W); prior history uses H=min(p,W).
            // Both pairs are covered without reading or allocating a tensor.
            for p in [last_legacy_h.saturating_sub(1), last_legacy_h] {
                if p < maximum as u64 {
                    positions.insert(p as u32);
                }
            }
        }
    }
    positions.into_iter().collect()
}
/// Geometry-only quotation for the complete scope metadata pass. A native
/// consumer pays its preliminary bounded wiring walk before calling this, and
/// pays the returned relation visits before any terminal scope construction.
/// It reads no tensor values and uses exactly the scope's boundary probes.
pub fn flat_scope_metadata_work_v25(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<u64, DismissalV1> {
    if max_positions == 0 || max_positions > program.history_bound || max_positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(bad("invalid flat-v25 metadata extent"));
    }
    let bytes = common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| bad("oversized v25 metadata program"))?;
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    if pr.node_count as u128 * 23 > MAX_SCOPE_RELATION_VISITS_V25 {
        return Err(bad("v25 metadata relation-visit limit"));
    }
    let mut work = bytes as u128 * 1024 + pr.node_count as u128 * 1024;
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        for n in 0..program.blocks[*b as usize].nodes.len() {
            work += grammar_boundary_positions(&pr, max_positions, s as u16, n as u16).len() as u128 * 4096;
        }
    }
    u64::try_from(work).map_err(|_| bad("v25 metadata work overflow"))
}

/// Before allocation or per-occurrence source scans, cap the number of metadata
/// relation visits. Validated types have at most one H axis; source initialization
/// changes at p=1. Probe both sides of each 64-element grammar boundary and max H.
pub fn flat_resource_scope_v25(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatResourceScopeV25, DismissalV1> {
    if max_positions == 0 || max_positions > program.history_bound || max_positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(bad("invalid flat-v25 scope extent"));
    }
    if let Some(e) = encoder
        && (max_positions != 1 || crate::seg_encoder::encoder_binding_v1(program).ok().as_ref() != Some(e))
    {
        return Err(bad("wrong flat-v25 encoder binding"));
    }
    let program_bytes = common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| bad("oversized v25 program"))?;
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    // Three endpoints and at most two probes per input/output H grammar boundary.
    // Bound metadata storage/work before collecting occurrence/position receipts.
    if pr.node_count as u128 * 23 > MAX_SCOPE_RELATION_VISITS_V25 {
        return Err(bad("v25 metadata relation-visit limit"));
    }
    let topk_price = crate::flat_topk7::TopKProgramPriceV7::of(program)?;
    let mut out = FlatResourceScopeV25 {
        version: 25,
        profile: flat_trace_profile_id_v5(),
        court_set: flat_court_set_id_v25(),
        program: object_id(b"misaka-palw/kernel/flat-resource-program/v25", program),
        max_positions,
        encoder: encoder.map(|e| (e.first_input, e.l)),
        relations: vec![],
        metadata_relation_visits: 0,
        metadata_derivation_work: program_bytes as u128 * 1024 + pr.node_count as u128 * 1024,
        worst_court_bytes: 0,
        worst_court_work: 0,
        worst_court_scratch_bytes: 0,
        trace_value_bytes_per_position: 0,
        reference_tensor_bytes_per_position: 0,
        trace_value_bytes_per_claim: 0,
        node_commitments_bytes_per_claim: pr.node_count as u128 * max_positions as u128 * 64,
    };
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            let (s, n) = (s as u16, n as u16);
            let ordinary_point = !matches!(node.prim, Prim::TopK { .. });
            let mut relation = FlatOccurrenceScopeV25 {
                occurrence: s,
                block: *b,
                node: n,
                ordinary_point,
                filing_bytes: 0,
                work: 0,
                scratch_bytes: 0,
                operand_leaves: vec![],
            };
            let samples = grammar_boundary_positions(&pr, max_positions, s, n);
            out.metadata_relation_visits += samples.len() as u128;
            out.metadata_derivation_work += samples.len() as u128 * 4096;
            for &p in &samples {
                let bound = if ordinary_point {
                    capped_v25(as_bound(
                        &relation_priced_v25(program, max_positions, &pr, p, s, n, program_bytes)
                            .map_err(|e| bad(&format!("v25 ordinary ({p},{s},{n}) {:?}: {e:?}", node.prim)))?,
                    ))?
                } else if crate::flat_scope7::point_relation(&pr, p, s, n)? {
                    let t = crate::flat_topk7::relation_scope_priced_v7(&topk_price, &pr, max_positions, p, s, n, true)?;
                    if t.selection_aux_bytes > crate::flat_topk7::TOPK_SCAN_MAX_AUX_BYTES_V7 {
                        return Err(bad("TopK v25 detector auxiliary cap"));
                    }
                    extend_special_envelope_v25(crate::flat_scope7::capped(
                        FlatCourtBoundsV4 {
                            filing_bytes: t.filing_bytes,
                            work: t.court_work,
                            scratch_bytes: t.court_scratch_bytes,
                            operand_leaves: vec![3, 2],
                        },
                        false,
                    )?)?
                } else {
                    let sh = pr.shapes(p, s, n);
                    let len = LayoutV3::of(&sh.out).len;
                    let material = if len >= 64 {
                        crate::merkle5::flat_material_bounds_v5(node.out.dtype, &sh.out.iter().map(|n| *n as u64).collect::<Vec<_>>())
                    } else {
                        None
                    };
                    extend_special_envelope_v25(crate::flat_scope7::capped(
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
                    )?)?
                };
                relation.filing_bytes = relation.filing_bytes.max(bound.filing_bytes);
                relation.work = relation.work.max(bound.work);
                relation.scratch_bytes = relation.scratch_bytes.max(bound.scratch_bytes);
                relation.operand_leaves.resize(relation.operand_leaves.len().max(bound.operand_leaves.len()), 0);
                for (a, b) in relation.operand_leaves.iter_mut().zip(&bound.operand_leaves) {
                    *a = (*a).max(*b);
                }
                maxima(&mut out, &bound);
            }
            let sh = pr.shapes(max_positions - 1, s, n);
            let len = LayoutV3::of(&sh.out).len as u128;
            out.trace_value_bytes_per_position += len * node.out.dtype.width() as u128;
            out.reference_tensor_bytes_per_position += len * 16;
            out.relations.push(relation);
        }
    }
    maxima(
        &mut out,
        &extend_special_envelope_v25(crate::flat_scope7::capped(
            decode_bounds_program(program, &pr, max_positions - 1, (pr.w.occurrences.len() - 1) as u16, program.logits)?,
            true,
        )?)?,
    );
    maxima(&mut out, &extend_special_envelope_v25(crate::flat_scope7::capped(malformed_bounds(program, &pr)?, true)?)?);
    out.trace_value_bytes_per_claim = out.trace_value_bytes_per_position * max_positions as u128;
    Ok(out)
}
pub fn check_flat_resource_scope_v25(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
    declared: &FlatResourceScopeV25,
) -> Result<CheckedResourceScopeV25, DismissalV1> {
    if declared.relations.len() as u128 > MAX_SCOPE_RELATION_VISITS_V25
        || declared.relations.iter().any(|r| r.operand_leaves.len() > 9)
    {
        return Err(bad("oversized v25 declaration"));
    }
    let checked = derive_checked_flat_resource_scope_v25(program, max_positions, encoder)?;
    if checked.scope() != declared {
        return Err(bad("v25 scope differs from independent derivation"));
    }
    Ok(checked)
}

use std::collections::BTreeSet;

pub(crate) fn capped_v25(mut b: FlatCourtBoundsV4) -> Result<FlatCourtBoundsV4, DismissalV1> {
    use crate::flat_point25::*;
    b.filing_bytes = b.filing_bytes.checked_add(7).ok_or_else(|| bad("v25 envelope bytes overflow"))?;
    b.work = b.work.checked_add(7 * 128).ok_or_else(|| bad("v25 envelope work overflow"))?;
    b.scratch_bytes = b.scratch_bytes.checked_add(7 * 16).ok_or_else(|| bad("v25 envelope scratch overflow"))?;
    let leaves = b.operand_leaves.iter().try_fold(1u64, |a, n| a.checked_add(*n)).ok_or_else(|| bad("v25 leaf overflow"))?;
    if b.filing_bytes > POINT_FAULT_MAX_BYTES_V25
        || b.work > POINT_FAULT_MAX_WORK_V25
        || b.scratch_bytes > POINT_FAULT_MAX_SCRATCH_V25
        || leaves > POINT_FAULT_MAX_LEAVES_V25 as u64
    {
        return Err(bad("v25 court cap exceeded"));
    }
    Ok(b)
}
/// An independently derived scope, never constructed by deserializing a caller's claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedResourceScopeV25 {
    scope: FlatResourceScopeV25,
}
impl CheckedResourceScopeV25 {
    pub fn scope(&self) -> &FlatResourceScopeV25 {
        &self.scope
    }
    pub fn root(&self) -> Digest {
        self.scope.root()
    }
    pub fn court_set(&self) -> Digest {
        self.scope.court_set
    }
    pub fn max_positions(&self) -> u32 {
        self.scope.max_positions
    }
    /// Same vector used by native paged reservation and Final27, including actual
    /// complete filing length rather than the transport's maximum capacity.
    pub fn paid_court_vector(&self) -> (u64, u64, u64, u16) {
        (
            self.scope.worst_court_bytes,
            self.scope.worst_court_work,
            self.scope.worst_court_scratch_bytes,
            self.scope.worst_court_bytes.div_ceil(crate::flat_point25::POINT_FAULT_PART_BYTES_V25) as u16,
        )
    }
}
pub fn derive_checked_flat_resource_scope_v25(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<CheckedResourceScopeV25, DismissalV1> {
    Ok(CheckedResourceScopeV25 { scope: flat_resource_scope_v25(program, max_positions, encoder)? })
}

/// Distinct artifact layout for the full25 route. Its leading version and fixed
/// grammar cannot be interpreted as a legacy PalwTirLayoutV1/class11 layout.
/// Resource/source roots are independently checked by native class29 registration.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct FullCourtLayoutV25 {
    pub version: u16,
    pub original_context_max: u32,
    pub lowered_context_max: u32,
    pub trace_profile: Digest,
    pub court_set: Digest,
    pub maximum_scope_root: Digest,
    pub maximum_resource_root: Digest,
    pub program_root: Digest,
    pub parameter_root: Digest,
    pub inventory_root: Digest,
    pub tokenizer_root: Digest,
    pub original_source_pin_sha256: [u8; 32],
}
impl FullCourtLayoutV25 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/full-court-artifact-layout/v25", self)
    }
    pub fn check_scope(&self, scope: &CheckedResourceScopeV25, program: &TirProgramV1) -> Result<(), DismissalV1> {
        if self.version != 25
            || self.original_context_max == 0
            || self.lowered_context_max != self.original_context_max
            || scope.max_positions() != self.original_context_max
            || self.trace_profile != scope.scope().profile
            || self.court_set != scope.court_set()
            || self.maximum_scope_root != scope.root()
            || self.program_root != crate::public::program_root_v1(&program.encode())
            || [self.maximum_resource_root, self.parameter_root, self.inventory_root, self.tokenizer_root].contains(&[0; 64])
            || self.original_source_pin_sha256 == [0; 32]
        {
            return Err(bad("full25 artifact layout differs from original independently derived scope"));
        }
        Ok(())
    }
}

pub(crate) fn extend_special_envelope_v25(mut b: FlatCourtBoundsV4) -> Result<FlatCourtBoundsV4, DismissalV1> {
    b.filing_bytes = b.filing_bytes.checked_add(4).ok_or_else(|| bad("v25 special envelope bytes overflow"))?;
    b.work = b.work.checked_add(4 * 128).ok_or_else(|| bad("v25 special envelope work overflow"))?;
    b.scratch_bytes = b.scratch_bytes.checked_add(4 * 16).ok_or_else(|| bad("v25 special envelope scratch overflow"))?;
    Ok(b)
}
