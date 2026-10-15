//! Native, read-only geometry for the original producer pledge. No accepted
//! claim, reservation, paid owner or signature is synthesized by this quote.
use crate::palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1};
use crate::palw_staged_bundle_v1::{
    PalwStagedBundlePlanV1, PalwStagedFamilyV1, PalwTokenizerInventoryPlanV1, staged_context_resource_plan_v1,
};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};

/// Which staged route extensions are armed at the quoting/accepting block. Below each fence
/// the extension's claims are refused at admission (A-2 uniform: the same refusal every build
/// below the fence applies), so no claim is admitted that its route could not prosecute.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwStagedRouteGatesV1 {
    /// `palw_staged_tir_route_v1`: TIR required families (native-context seeds and answers).
    pub tir: bool,
    /// `palw_staged_float_constraint_v1`: greedy Float32 Constraint (rooted f32 row court).
    pub float32: bool,
}
impl PalwStagedRouteGatesV1 {
    pub fn of_extras_v1(extras: &crate::palw_state_v2::PalwTransitionExtrasV1) -> Self {
        Self { tir: extras.staged_tir_route_active, float32: extras.staged_float_constraint_active }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedClaimGeometryV1 {
    pub class_id: Hash64,
    pub producer: PalwBondKeyV2,
    pub execution_root: Hash64,
    pub job_identity: Hash64,
    pub accepted_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedPreclaimQuoteV1 {
    pub resource_plan_id: Hash64,
    pub maximum_records: u64,
    pub maximum_custodians: u64,
    pub material_span_daa: u64,
    pub plan: PalwStagedBundlePlanV1,
}
#[allow(clippy::too_many_arguments)]
pub fn staged_preclaim_quote_v1(
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    profile: &crate::palw_step::PalwShapeProfileV3,
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    maximum_custodians: u64,
    maximum_leaves: u64,
    config: PalwStagedPublicCourtConfigV1,
) -> Result<Option<PalwStagedPreclaimQuoteV1>, String> {
    staged_preclaim_quote_with_gates_v1(
        geometry,
        original,
        header,
        profile,
        semantics,
        inventory,
        maximum_custodians,
        maximum_leaves,
        config,
        PalwStagedRouteGatesV1::default(),
    )
}

/// [`staged_preclaim_quote_v1`] with the armed route extensions (greedy Float32 Constraint).
#[allow(clippy::too_many_arguments)]
pub fn staged_preclaim_quote_with_gates_v1(
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    profile: &crate::palw_step::PalwShapeProfileV3,
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    maximum_custodians: u64,
    maximum_leaves: u64,
    config: PalwStagedPublicCourtConfigV1,
    gates: PalwStagedRouteGatesV1,
) -> Result<Option<PalwStagedPreclaimQuoteV1>, String> {
    check_preclaim_identity_v1(geometry, original, header, maximum_custodians, config)?;
    let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Legacy(header) = header else {
        return Err("a TIR header is quoted by staged_preclaim_tir_quote_v1 against its registered record".into());
    };
    legacy_preclaim_quote_v1(geometry, original, header, profile, semantics, inventory, maximum_custodians, maximum_leaves, config, gates)
}

fn check_preclaim_identity_v1(
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    maximum_custodians: u64,
    config: PalwStagedPublicCourtConfigV1,
) -> Result<(), String> {
    config.check()?;
    header.check_wire_v1()?;
    if original.job.class_id != geometry.class_id
        || original.job.executor_bond != geometry.producer.0
        || original.execution_root != geometry.execution_root
        || crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original) != geometry.job_identity
        || header.execution_root_v1() != geometry.execution_root
        || header.job_context_v1().shape_profile_id != geometry.class_id
        || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(header.job_context_v1()) != geometry.job_identity
        || maximum_custodians == 0
    {
        return Err("preclaim quote substitutes original producer, class, execution, job or native custody bound".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn legacy_preclaim_quote_v1(
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicLegacyBindingHeaderV1,
    profile: &crate::palw_step::PalwShapeProfileV3,
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    maximum_custodians: u64,
    maximum_leaves: u64,
    config: PalwStagedPublicCourtConfigV1,
    gates: PalwStagedRouteGatesV1,
) -> Result<Option<PalwStagedPreclaimQuoteV1>, String> {
    let binding = header.resolve_v1(profile, geometry.execution_root, maximum_leaves)?;
    crate::palw_legacy_held_da_v2::palw_legacy_fp_job_check_v2(&binding, &original.job).map_err(|e| e.to_string())?;
    if !semantics.matches_context(&binding.job_context) {
        return Err("preclaim quote changes immutable CU/tokenizer/model statement".into());
    }
    let mut families = Vec::new();
    let vocab = profile.vocab_size;
    if original.job.is_prefix_state() {
        let relation = semantics.prefix_state_relation.ok_or("prefix job has no implemented immutable state relation")?;
        let named = crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(&original.job).map_err(|e| e.to_string())?;
        let base = crate::palw_prefix_resources_v1::PalwPrefixResourcePlanV1::derive(
            profile,
            relation,
            named.prefix_tokens,
            binding.step_leaf_count,
            90_000,
            config.prices,
        )?;
        let slots = usize::try_from(maximum_custodians.checked_add(1).ok_or("native prefix slots overflow")?)
            .map_err(|_| "platform prefix slots overflow")?;
        let public = crate::palw_prefix_public_source_v1::PalwPrefixPublicAcquisitionPlanV1::derive(
            base,
            slots,
            config.prices,
            borsh::object_length(profile).map_err(|e| e.to_string())? as u64,
        )?;
        families.push((PalwStagedFamilyV1::PublicPrefix, public.envelope));
    }
    if original.job.is_constraint() {
        if profile.lane == crate::palw_step::PalwStepLaneV1::Float32 {
            // The rooted f32 row court covers GREEDY Float32 only, above its own fence. A seeded
            // Float32 job has no canonical float sampler and stays refused at every height.
            if !gates.float32 {
                return Err("Float32 Constraint claims are refused below palw_staged_float_constraint_v1".into());
            }
            if !original.job.sampling_v2().is_greedy() {
                return Err("a seeded Float32 Constraint job has no canonical float sampler; refused at every height".into());
            }
        }
        families.push((PalwStagedFamilyV1::Constraint, PalwRenderingResourcePlanV1::derive(4095, vocab, config.prices)?));
    }
    let profile_bytes = borsh::object_length(profile).map_err(|e| e.to_string())? as u64;
    finish_preclaim_quote_v1(
        geometry,
        original,
        semantics,
        inventory,
        families,
        PalwPreclaimSourceGeometryV1 {
            vocab,
            logits_scheme_id: profile.logits_scheme_id,
            lane: profile.lane,
            profile_bytes,
            step_leaf_count: binding.step_leaf_count,
        },
        maximum_custodians,
        config,
    )
}

/// What the Constraint source quote reads of a claim's class, legacy or TIR.
#[derive(Clone, Copy, Debug)]
struct PalwPreclaimSourceGeometryV1 {
    vocab: u32,
    logits_scheme_id: Hash64,
    /// The class's registered lane: a `Float32` class's source is its v2 event tree, whatever
    /// its logits scheme field says.
    lane: crate::palw_step::PalwStepLaneV1,
    /// Bytes a source session retains to resolve the class (a legacy profile, a TIR program).
    profile_bytes: u64,
    step_leaf_count: u64,
}

/// **The TIR preclaim quote**: the same planner over a TIR claim's decode view, derived from the
/// native-admitted public header and the class's registered record (no layout, no binding on the
/// wire). Covered families: Constraint over the TIR class's tiled logits and the raw tokenizer.
/// A TIR prefix-state job and a flat-scheme TIR Constraint job have no TIR source/relation and are
/// refused by name.
#[allow(clippy::too_many_arguments)]
pub fn staged_preclaim_tir_quote_v1(
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    maximum_custodians: u64,
    maximum_leaves: u64,
    config: PalwStagedPublicCourtConfigV1,
) -> Result<Option<PalwStagedPreclaimQuoteV1>, String> {
    check_preclaim_identity_v1(geometry, original, header, maximum_custodians, config)?;
    let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(header) = header else {
        return Err("a legacy header is quoted against its registered shape profile".into());
    };
    let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, maximum_leaves)?;
    if view.job_context.exact_decode_tokens != original.decode_tokens_executed
        || view.job_context.tokenizer_id != original.job.tokenizer_id
        || !semantics.matches_context(&view.job_context)
    {
        return Err("TIR preclaim quote changes the original decode count, tokenizer or immutable CU statement".into());
    }
    let mut families = Vec::new();
    if original.job.is_prefix_state() {
        // The relation core exists (`palw_tir_prefix_state_v1`: aligned boundary, canonical digest,
        // authenticated stream); its source session, family row, fold, quote and node actors do not.
        return Err("a TIR prefix-state job's family court is not wired (relation core only); refused".into());
    }
    if original.job.is_constraint() {
        if view.lane == crate::palw_step::PalwStepLaneV1::Float32 {
            return Err("a Float32 TIR class has no native-context Float32 source session".into());
        }
        if !view.is_tiled_v1() {
            return Err("a flat-scheme TIR Constraint job has no TIR Flat source session".into());
        }
        families.push((PalwStagedFamilyV1::Constraint, PalwRenderingResourcePlanV1::derive(4095, view.vocab_size, config.prices)?));
    }
    finish_preclaim_quote_v1(
        geometry,
        original,
        semantics,
        inventory,
        families,
        PalwPreclaimSourceGeometryV1 {
            vocab: view.vocab_size,
            logits_scheme_id: view.logits_scheme_id,
            lane: view.lane,
            profile_bytes: u64::from(record.program_bytes),
            step_leaf_count: view.step_leaf_count,
        },
        maximum_custodians,
        config,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_preclaim_quote_v1(
    geometry: &PalwStagedClaimGeometryV1,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    mut families: Vec<(PalwStagedFamilyV1, PalwRenderingResourcePlanV1)>,
    source: PalwPreclaimSourceGeometryV1,
    maximum_custodians: u64,
    config: PalwStagedPublicCourtConfigV1,
) -> Result<Option<PalwStagedPreclaimQuoteV1>, String> {
    let vocab = source.vocab;
    let intake = match (semantics.raw_tokenizer_bytes, inventory) {
        (Some(_), Some(cert)) => {
            cert.check_v1()?;
            if cert.artifact_hash != semantics.tokenizer_id || cert.completed_daa > geometry.accepted_daa {
                return Err("preclaim resource inventory is foreign or newer than accepted snapshot".into());
            }
            families.push((
                PalwStagedFamilyV1::Tokenizer,
                crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(
                    cert.actual_bytes,
                    cert.key_event_upper_bound_v1(),
                    config.prices,
                )?
                .court_envelope(vocab, config.prices)?,
            ));
            Some(PalwTokenizerInventoryPlanV1::derive_v1(cert.actual_bytes, config)?)
        }
        (None, None) => None,
        _ => return Err("prospective raw work requires native accepted exact EOF inventory, not declared length".into()),
    };
    if families.is_empty() {
        return Ok(None);
    }
    families.sort_by_key(|(family, _)| *family);
    let maximum_records = families
        .iter()
        .try_fold(0u64, |n, (_, p)| n.checked_add(p.maximum_moves).ok_or("original signed history capacity overflow"))?;
    let recovery = crate::palw_history_custody_v1::history_recovery_resource_plan_with_wait_v1(
        maximum_records,
        usize::try_from(maximum_custodians).map_err(|_| "platform history custody overflow")?,
        config.acquisition_daa,
        config.prices,
    )?;
    let profile_bytes = source.profile_bytes;
    let mut staged = Vec::new();
    for (family, semantic) in families {
        let semantic = if family == PalwStagedFamilyV1::PublicPrefix { semantic.checked_sum_v1(recovery)? } else { semantic };
        let mut acquisition = staged_context_resource_plan_v1(profile_bytes, semantic, config)?;
        if family == PalwStagedFamilyV1::Constraint {
            let slots = u32::try_from(maximum_custodians).map_err(|_| "source native custody exceeds format")?;
            let acquisition_source = if source.lane == crate::palw_step::PalwStepLaneV1::Float32 {
                crate::palw_float_constraint_source_v1::float_constraint_source_resource_plan_v1(
                    original.decode_tokens_executed,
                    vocab,
                    profile_bytes,
                    slots,
                    config.prices,
                )?
            } else if source.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1() {
                crate::palw_selected_event_source_v1::selected_event_resource_plan_v1(
                    vocab,
                    original.decode_tokens_executed,
                    profile_bytes,
                    slots,
                    config,
                )?
            } else if source.logits_scheme_id == crate::palw_step_refute::flat_logits_scheme_id_v1() {
                crate::palw_flat_constraint_stream_v1::flat_constraint_source_resource_plan_v1(
                    original.decode_tokens_executed,
                    vocab,
                    profile_bytes,
                    slots,
                    source.step_leaf_count,
                    config.prices,
                )?
            } else {
                return Err("constraint source must preserve its actual native logits scheme".into());
            };
            acquisition = acquisition.checked_sum_v1(acquisition_source)?;
        }
        staged.push((family, semantic, Some(acquisition)));
    }
    let plan = PalwStagedBundlePlanV1::derive_with_acquisitions_v1(geometry.accepted_daa, staged, intake, config)?;
    let mut committed = Vec::new();
    for stage in &plan.stages {
        committed.push((
            stage.family,
            match stage.acquisition {
                Some(p) => stage.terms.plan.checked_sum_v1(p)?,
                None => stage.terms.plan,
            },
        ));
    }
    if let Some(intake) = &plan.inventory {
        committed[0].1 = committed[0].1.checked_sum_v1(intake.envelope)?;
    }
    let resource_plan_id = crate::palw_tokenizer_artifact_inventory_v1::palw_inventory_resource_plan_id_v1(
        semantics,
        inventory,
        &committed,
        maximum_custodians,
        config,
    )?;
    let material_span_daa =
        plan.terms.material_until_daa.checked_sub(geometry.accepted_daa).ok_or("preclaim material span underflow")?;
    Ok(Some(PalwStagedPreclaimQuoteV1 { resource_plan_id, maximum_records, maximum_custodians, material_span_daa, plan }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        PalwStagedClaimGeometryV1,
        crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
        crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
        crate::palw_step::PalwShapeProfileV3,
        crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    ) {
        let (target, binding, job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
        let original = crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3 {
            job,
            trace_root: target.trace_root,
            output_root: target.output_root,
            schedule_root: Hash64::default(),
            execution_root: target.execution_root,
            decode_tokens_executed: 2,
            stop_reason: crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached,
            work_leaves: binding.step_leaf_count,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 100,
        };
        let geometry = PalwStagedClaimGeometryV1 {
            class_id: target.class_id,
            producer: target.executor_bond,
            execution_root: target.execution_root,
            job_identity: target.job_identity,
            accepted_daa: 100,
        };
        let header = crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Legacy(
            crate::palw_public_binding_v1::PalwPublicLegacyBindingHeaderV1::of_binding_v1(&binding),
        );
        (geometry, original, header, binding.shape_profile, target.registered_semantics.unwrap())
    }
    #[test]
    fn producer_resource_commitment_is_clock_free_but_owner_windows_remain_fixed() {
        let (mut geometry, original, header, profile, semantics) = fixture();
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let first =
            staged_preclaim_quote_v1(&geometry, &original, &header, &profile, &semantics, None, 6, 1 << 32, config).unwrap().unwrap();
        geometry.accepted_daa += 9;
        let later =
            staged_preclaim_quote_v1(&geometry, &original, &header, &profile, &semantics, None, 6, 1 << 32, config).unwrap().unwrap();
        assert_eq!(first.resource_plan_id, later.resource_plan_id);
        assert_eq!(first.material_span_daa, later.material_span_daa);
        assert_eq!(first.plan.terms.material_until_daa + 9, later.plan.terms.material_until_daa);
        assert_ne!(first.plan.id_v1(Hash64::from_u64_word(23)), later.plan.id_v1(Hash64::from_u64_word(23)));
        let mut substituted = original.clone();
        substituted.job.executor_bond = crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(999), 0);
        assert!(staged_preclaim_quote_v1(&geometry, &substituted, &header, &profile, &semantics, None, 6, 1 << 32, config).is_err());
        let mut overflow = config;
        overflow.prices.rent_per_parser_step = u64::MAX;
        assert!(staged_preclaim_quote_v1(&geometry, &original, &header, &profile, &semantics, None, 6, 1 << 32, overflow).is_err());
    }
    #[test]
    fn false_declared_raw_size_cannot_substitute_for_native_accepted_eof_geometry() {
        let (geometry, original, header, profile, mut semantics) = fixture();
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        semantics.raw_tokenizer_bytes = Some(1);
        semantics.tokenizer_key_events = Some(1);
        // This is an accepted-state fixture, not a certificate import path or
        // evidence that a wire-provided structural certificate is authoritative.
        let mut certificate = crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1 {
            artifact_hash: semantics.tokenizer_id,
            actual_bytes: 128,
            inventory_id: Hash64::from_u64_word(90),
            opener: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(91), 0)),
            opened_daa: 90,
            completed_daa: 99,
        };
        assert!(staged_preclaim_quote_v1(&geometry, &original, &header, &profile, &semantics, None, 6, 1 << 32, config).is_err());
        let quote =
            staged_preclaim_quote_v1(&geometry, &original, &header, &profile, &semantics, Some(&certificate), 6, 1 << 32, config)
                .unwrap()
                .unwrap();
        assert_eq!(quote.plan.inventory.as_ref().unwrap().maximum_bytes, 128);
        assert_eq!(
            quote.plan.stages.iter().map(|s| s.family).collect::<Vec<_>>(),
            vec![PalwStagedFamilyV1::PublicPrefix, PalwStagedFamilyV1::Tokenizer]
        );
        certificate.completed_daa = 101;
        assert!(
            staged_preclaim_quote_v1(&geometry, &original, &header, &profile, &semantics, Some(&certificate), 6, 1 << 32, config)
                .is_err()
        );
        certificate.completed_daa = 99;
        certificate.artifact_hash = Hash64::from_u64_word(992);
        assert!(
            staged_preclaim_quote_v1(&geometry, &original, &header, &profile, &semantics, Some(&certificate), 6, 1 << 32, config)
                .is_err()
        );
    }
}
