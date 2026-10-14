//! Native, read-only geometry for the original producer pledge. No accepted
//! claim, reservation, paid owner or signature is synthesized by this quote.
use crate::palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1};
use crate::palw_staged_bundle_v1::{
    PalwStagedBundlePlanV1, PalwStagedFamilyV1, PalwTokenizerInventoryPlanV1, staged_context_resource_plan_v1,
};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};

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
    let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Legacy(header) = header else {
        return Err("TIR requires its own verified execution projection, not a legacy quote".into());
    };
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
        if profile.lane != crate::palw_step::PalwStepLaneV1::Int32 {
            return Err("qualified Float constraint needs its original FloatV2 event source and semantic projection; integer source is not authority".into());
        }
        families.push((PalwStagedFamilyV1::Constraint, PalwRenderingResourcePlanV1::derive(4095, vocab, config.prices)?));
    }
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
    let profile_bytes = borsh::object_length(profile).map_err(|e| e.to_string())? as u64;
    let mut staged = Vec::new();
    for (family, semantic) in families {
        let semantic = if family == PalwStagedFamilyV1::PublicPrefix { semantic.checked_sum_v1(recovery)? } else { semantic };
        let mut acquisition = staged_context_resource_plan_v1(profile_bytes, semantic, config)?;
        if family == PalwStagedFamilyV1::Constraint {
            let slots = u32::try_from(maximum_custodians).map_err(|_| "source native custody exceeds format")?;
            let source = if profile.logits_scheme_id == crate::palw_step_refute::tiled_logits_scheme_id_v1() {
                crate::palw_selected_event_source_v1::selected_event_resource_plan_v1(
                    vocab,
                    original.decode_tokens_executed,
                    profile_bytes,
                    slots,
                    config,
                )?
            } else if profile.logits_scheme_id == crate::palw_step_refute::flat_logits_scheme_id_v1() {
                crate::palw_flat_constraint_stream_v1::flat_constraint_source_resource_plan_v1(
                    original.decode_tokens_executed,
                    vocab,
                    profile_bytes,
                    slots,
                    binding.step_leaf_count,
                    config.prices,
                )?
            } else {
                return Err("constraint source must preserve its actual native logits scheme".into());
            };
            acquisition = acquisition.checked_sum_v1(source)?;
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
