//! V2 canonical reader inputs, selected by the native immutable declaration.
//! V1 still uses its historical unique-Q36-router rule. This API authenticates
//! no remote declaration and never turns data membership into claim approval.
use super::*;
use crate::palw_native_resource_v1::{NativeRetainedV1, TirNativeRequestsV1, reserve_native_requests_v1};
use crate::palw_reader_router_v2::{PalwReaderRouterDeclarationV2, PalwReaderRouterErrorV2, selected_reader_router_v2};

type Inputs = Vec<Vec<(u64, PalwStepCoordinateV1)>>;
fn bad(s: &'static str) -> PalwReaderRouterErrorV2 {
    PalwReaderRouterErrorV2::Identity(s)
}

/// Build the exact input inventory before an evidence assembler allocates any
/// openings. The declaration must come from the owning native state/claim
/// stamp. The retained result owns enumeration workspace for its full lifetime.
/// A declaration supplied by a challenger is not admission authority.
pub fn canonical_reader_inputs_v2(
    profile: &PalwShapeProfileV3,
    context: &crate::palw_v2::PalwJobContextV2,
    coord: &PalwStepCoordinateV1,
    declaration: &PalwReaderRouterDeclarationV2,
) -> Result<NativeRetainedV1<Inputs>, PalwReaderRouterErrorV2> {
    let selected = selected_reader_router_v2(profile, declaration, coord.node_slot)?;
    let (node, _) = profile.resolve_node_slot(coord.node_slot).ok_or_else(|| bad("reader coordinate is unresolved"))?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or_else(|| bad("reader kernel is unavailable"))?;
    let first = coord
        .node_slot
        .checked_sub(intra_table_index(profile, coord.node_slot).ok_or_else(|| bad("reader table is absent"))? as u32)
        .ok_or_else(|| bad("reader table start underflows"))?;
    let mut q = TirNativeRequestsV1::new();
    // Routed readers consume actual rows at one position, never a recurrent
    // prefix or KV history. Refuse an unsupported sentinel before construction.
    // Whole source rows bound the actual narrower lane/strided tile inventory.
    let appendix = usize::from(qwen36_reads_routing_v1(node));
    q.vec_v1::<Vec<(u64, PalwStepCoordinateV1)>>(node.input_refs.len() as u64 + appendix as u64)?;
    q.vec_v1::<(u32, u32)>(1)?;
    q.vec_v1::<u64>(node.input_refs.len() as u64)?;
    // The canonical coordinate/index closed form holds two finite typed
    // KV-term arrays (three u128 values per term); every table term is bounded
    // by the native profile's complete node inventory.
    q.vec_v1::<(u128, u128, u128)>(u64::from(profile.global_node_count()))?;
    q.vec_v1::<(u128, u128, u128)>(u64::from(profile.global_node_count()))?;

    let width = |n: &crate::palw_step::PalwStepNodeV1| match n.out_len {
        crate::palw_step::PalwStepOutLenV1::Fixed { elements } if n.tile_len > 0 => Ok(u64::from(elements)),
        _ => Err(bad("routed reader source is not a finite registered row")),
    };
    let route_width = width(selected.1)?;
    q.vec_v1::<(u64, u64)>(route_width / 2)?;
    for &r in &node.input_refs {
        if r >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
            return Err(bad("routed reader source is not an actual node"));
        }
        let slot = first.checked_add(u32::from(r)).ok_or_else(|| bad("reader source overflows"))?;
        if slot >= coord.node_slot {
            return Err(bad("reader source must precede output"));
        }
        let (source, _) = profile.resolve_node_slot(slot).ok_or_else(|| bad("reader source is absent"))?;
        let count = width(source)?.div_ceil(u64::from(source.tile_len));
        q.vec_v1::<(u64, PalwStepCoordinateV1)>(count)?;
        q.vec_v1::<(u64, u64)>(route_width / 2 + 1)?;
    }
    if appendix > 0 {
        q.vec_v1::<(u64, PalwStepCoordinateV1)>(route_width.div_ceil(u64::from(selected.1.tile_len)))?;
    }
    let lease = reserve_native_requests_v1(q.bytes_v1())?;
    let inputs = canonical_input_leaves_routing_v2(profile, context, coord.node_slot, coord, program, false, None, Some(selected))
        .ok_or_else(|| bad("reader's canonical leaf coordinates are unavailable"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(inputs, lease))
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRoutedStepRefutationV2 {
    pub version: u16,
    pub step: PalwExecutionStepRefutationV1,
}

#[derive(Debug, thiserror::Error)]
pub enum PalwRoutingCourtErrorV2 {
    #[error("local routing court capacity unavailable: {0:?}")]
    Resource(crate::palw_native_resource_v1::NativeCoverageV1),
    #[error("routing evidence: {0}")]
    Evidence(#[from] PalwStepRefuteError),
}
impl From<crate::palw_native_resource_v1::NativeCoverageV1> for PalwRoutingCourtErrorV2 {
    fn from(e: crate::palw_native_resource_v1::NativeCoverageV1) -> Self {
        Self::Resource(e)
    }
}
impl From<crate::palw_step_leg::PalwStepLegError> for PalwRoutingCourtErrorV2 {
    fn from(e: crate::palw_step_leg::PalwStepLegError) -> Self {
        Self::Evidence(e.into())
    }
}
impl From<PalwReaderRouterErrorV2> for PalwRoutingCourtErrorV2 {
    fn from(e: PalwReaderRouterErrorV2) -> Self {
        match e {
            PalwReaderRouterErrorV2::Resource(e) => Self::Resource(e),
            PalwReaderRouterErrorV2::Identity(s) => Self::Evidence(PalwStepRefuteError::InputSetNotCanonical(s)),
        }
    }
}
fn evidence(s: &'static str) -> PalwRoutingCourtErrorV2 {
    PalwStepRefuteError::InputSetNotCanonical(s).into()
}
#[path = "routing_v2/workspace.rs"]
mod workspace;

/// Direct deterministic arithmetic for the exact V2-stamped native reader.
/// The declaration is native state, never a field supplied by an accuser.
/// Matching replay cannot license or finalize the claim. The evidence only
/// authenticates committed data and permits a local objective verdict.
pub fn check_routed_step_refutation_v2(
    r: &PalwRoutedStepRefutationV2,
    claim_execution: Hash64,
    declaration: &PalwReaderRouterDeclarationV2,
    weights: &dyn PalwWeightOracleV1,
    max_leaves: u64,
) -> Result<PalwStepRefutationVerdictV1, PalwRoutingCourtErrorV2> {
    use crate::palw_step_leg as leg;
    if r.version != 2 || r.step.kv_checkpoint.is_some() || !r.step.prompt_token_ids.is_empty() || r.step.decode_tokens.is_some() {
        return Err(evidence("routed V2 evidence requires version2 and no unrelated history/token carriage"));
    }
    let b = &r.step.binding;
    if b.step_leaf_count == 0 || b.step_leaf_count > max_leaves || r.step.output_opening.leaf_index >= b.step_leaf_count {
        return Err(evidence("routed evidence output/step count is outside the native ladder"));
    }
    if b.committed_execution_root != claim_execution {
        return Err(evidence("routed evidence names another execution"));
    }
    let mut coordinate_q = TirNativeRequestsV1::new();
    coordinate_q.vec_v1::<(u128, u128, u128)>(u64::from(b.shape_profile.global_node_count()))?;
    coordinate_q.vec_v1::<(u128, u128, u128)>(u64::from(b.shape_profile.global_node_count()))?;
    let _coordinate = reserve_native_requests_v1(coordinate_q.bytes_v1())?;
    let coord = crate::palw_step::canonical_step_coordinates(&b.shape_profile, &b.job_context, r.step.output_opening.leaf_index)
        .ok_or(PalwStepRefuteError::Unadjudicable)?;
    let selected = selected_reader_router_v2(&b.shape_profile, declaration, coord.node_slot)?;
    let (node, layer) = b.shape_profile.resolve_node_slot(coord.node_slot).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let program = resolve_kernel(&node.kernel_semantics_id).ok_or(PalwStepRefuteError::Unadjudicable)?;
    // No unreviewed kernel or a generic multiply carrying a misleading name
    // inherits this routing contract and silently ignores the selected row.
    let supported = match program {
        KernelProgram::Qwen36(Qwen36Op::MatMulGrouped | Qwen36Op::MatMulGroupedWide) => {
            node.input_refs.len() == 1
                && [".ffn_gate_exps.routed", ".ffn_up_exps.routed", ".ffn_down_exps.routed"]
                    .iter()
                    .any(|suffix| node.weight_name.ends_with(suffix))
        }
        KernelProgram::Qwen36(Qwen36Op::MulWide) => node.input_refs.len() == 2 && node.weight_name.ends_with(".ffn_expert_gated.a16"),
        KernelProgram::Qwen36(Qwen36Op::MoeCombine) => node.input_refs.len() == 2,
        KernelProgram::Kimi(KimiOp::MoeCombine) => node.input_refs.len() == 3,
        _ => false,
    };
    if !supported {
        return Err(PalwStepRefuteError::Unadjudicable.into());
    }
    let required = canonical_reader_inputs_v2(&b.shape_profile, &b.job_context, &coord, declaration)?;
    let _workspace = workspace::reserve(&r.step, &required, node, selected.1)?;
    leg::verify_binding_v1(b)?;
    let inputs = memory_v2::verified_input_rows_v2(b, &r.step.inputs, &required, max_leaves).map_err(|e| match e {
        memory_v2::PalwGdnCourtErrorV2::Resource(e) => PalwRoutingCourtErrorV2::Resource(e),
        memory_v2::PalwGdnCourtErrorV2::Evidence(e) => PalwRoutingCourtErrorV2::Evidence(e),
    })?;
    let structural = leg::PalwStepRefutationV1 {
        binding: b.clone(),
        evidence: leg::PalwStepEvidenceV1::StepTile {
            opening: r.step.output_opening.clone(),
            preimage: r.step.output_preimage.clone(),
        },
    };
    match leg::check_step_refutation_capped_v1(&structural, max_leaves) {
        Ok(v) => return Ok(v),
        Err(leg::PalwStepLegError::NoFaultFound) => {}
        Err(e) => return Err(e.into()),
    }
    let oracle = workspace::ReservedOracle::new(weights);
    let kv_len = if coord.call_index == 0 {
        u64::from(coord.position) + 1
    } else {
        u64::from(b.job_context.declared_prefill_tokens) + u64::from(coord.call_index)
    };
    let answer = run_program_routing_v2(
        program,
        node,
        layer,
        &b.shape_profile,
        &inputs,
        &oracle,
        kv_len,
        (&coord, crate::palw_prompt_ids_v1::PalwPromptIdWindowV1::EMPTY, &[]),
        Some(selected.1),
    );
    if let Some(e) = oracle.take_refusal() {
        return Err(PalwRoutingCourtErrorV2::Resource(e));
    }
    let (row, offset) = answer?;
    let tile_start = u64::from(coord.tile_index).checked_mul(u64::from(node.tile_len)).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let start = usize::try_from(tile_start).ok().and_then(|s| s.checked_sub(offset)).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let committed = &r.step.output_preimage.values_le;
    let recomputed = row
        .get(start..start.checked_add(committed.len() / 4).ok_or(PalwStepRefuteError::Unadjudicable)?)
        .ok_or_else(|| evidence("routed recomputation is shorter than its canonical tile"))?;
    let mismatch = recomputed.iter().zip(committed.chunks_exact(4)).position(|(word, bytes)| word.to_le_bytes().as_slice() != bytes);
    match mismatch {
        None => Err(PalwStepRefuteError::NoFaultFound.into()),
        Some(index) => {
            let fault = PalwStepFaultV1::ComputationMismatch { value_index: index as u32 };
            Ok(PalwStepRefutationVerdictV1 {
                fault,
                evidence_id: leg::step_refutation_evidence_id(&b.committed_execution_root, 5, r.step.output_opening.leaf_index, fault),
            })
        }
    }
}

#[path = "routing_v2/capture.rs"]
mod capture;
pub use capture::*;

#[cfg(test)]
#[path = "routing_v2/tests.rs"]
mod tests;
