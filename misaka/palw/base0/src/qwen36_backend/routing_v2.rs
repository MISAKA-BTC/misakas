//! Actual Q36 fold -> complete routed material. This is a producer adapter for
//! codec4; neither capture bytes nor any historical root/selector is renamed.
use super::*;
use kaspa_consensus_core::{
    palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1},
    palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E,
    palw_native_resource_v1::{
        NativeDecodeErrorV1, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1, retain_native_value_decode_classified_v1,
    },
    palw_routed_material_v2::{
        PalwHeldMaterialBundleV2, PalwHeldMaterialUnitV2, PalwMaterialConstructionRequestsV2, PalwMaterialNativeAuthorityV2,
        construct_material_unit_v2,
    },
    palw_step_leg::PalwStepTileLeafV1,
};

/// The same pinned 64-bit BTree allocation envelope used by Core's native
/// visitor, expressed as concrete source fields. Nested tile/hash allocations
/// are quoted separately. This helper is only for the historical V1 assembler
/// used by non-routed leaves; routed leaves use a Vec-only gather.
fn legacy_map<K, V>(q: &mut PalwMaterialConstructionRequestsV2, count: u64) -> Result<(), E> {
    if !cfg!(target_pointer_width = "64") {
        return Err(E::LocalResource(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
            std::collections::BTreeMap<K, V>,
        >("Q36 folded legacy input map", "pinned native BTree layout requires 64-bit target")));
    }
    q.vec_v1::<[K; 16]>(count).map_err(E::LocalResource)?;
    q.vec_v1::<[V; 16]>(count).map_err(E::LocalResource)?;
    q.vec_v1::<[usize; 22]>(count).map_err(E::LocalResource)
}

impl Qwen36Backend {
    /// SDK runtime context: call after an actual holding matched the chain's
    /// class/root. This local builder neither verifies a raw network root nor
    /// creates native routing authority.
    pub fn with_resolved_artifact_identity_v2(mut self, class: Hash64, root: Hash64) -> Result<Self, String> {
        if class != self.class_profile_id || root == Hash64::default() {
            return Err("Q36 resolved holding identity differs from its actual compiled class".into());
        }
        self.resolved_artifact_identity_v2 = Some((class, root));
        Ok(self)
    }
    pub(super) fn require_native_routing_job_v2(&self, job: &PalwJobContextV2) -> Result<(), E> {
        if self.native_routing_execution_v2.as_ref().is_some_and(|authority| authority.job() != job)
            || self.native_routing_claim_v2.as_ref().is_some_and(|authority| authority.job() != job)
        {
            return Err(E::Substitution);
        }
        Ok(())
    }
    pub(super) fn require_native_fp_identity_v2(&self, job: &PalwJobContextV2) -> Result<(), E> {
        if self.native_routing_execution_v2.is_some() {
            return Err(E::Substitution); // A block attempt owner is never FP authority.
        }
        if let Some(owner) = &self.native_routing_claim_v2 {
            if !owner.is_free_prompt() {
                return Err(E::Substitution);
            }
            let expected = owner.job();
            // Compare every fixed context field without allocating a clone.
            // Only the stop probe's count is allowed to vary here.
            if job.version != expected.version
                || job.network_id != expected.network_id
                || job.job_id != expected.job_id
                || job.job_nullifier != expected.job_nullifier
                || job.assignment_id != expected.assignment_id
                || job.execution_seed != expected.execution_seed
                || job.model_profile_id != expected.model_profile_id
                || job.runtime_manifest_hash != expected.runtime_manifest_hash
                || job.runtime_class_id != expected.runtime_class_id
                || job.shape_profile_id != expected.shape_profile_id
                || job.trace_scheme_id != expected.trace_scheme_id
                || job.cu_ruleset_id != expected.cu_ruleset_id
                || job.tokenizer_id != expected.tokenizer_id
                || job.prompt_token_ids_hash != expected.prompt_token_ids_hash
                || job.declared_prefill_tokens != expected.declared_prefill_tokens
                || job.max_context_tokens != expected.max_context_tokens
            {
                return Err(E::Substitution);
            }
        }
        Ok(())
    }
    pub(super) fn bind_native_routing_plan_v2(
        &mut self,
        authority: std::sync::Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingExecutionAuthorityV2>,
    ) -> Result<(), E> {
        let row = authority.row();
        let profile = self.profile.as_ref().ok_or(E::Incomplete)?;
        if self.resolved_artifact_identity_v2 != Some((row.declaration.class_id, row.declaration.artifact_root))
            || self.native_routing_claim_v2.is_some()
            || self.class_profile_id != row.declaration.class_id
            || authority.native_profile() != profile
            || authority.job().shape_profile_id != row.declaration.class_id
            || self.native_routing_execution_v2.as_ref().is_some_and(|old| !old.same_execution_v2(&authority))
        {
            return Err(E::Substitution);
        }
        let native = self.plan.as_ref().ok_or(E::Incomplete)?.bind_reader_router_v2(profile, &row.declaration, authority.network())?;
        self.plan = Some(native.plan);
        self.native_routing_plan_lease_v2 = native.lease;
        self.native_routing_execution_v2 = Some(authority);
        Ok(())
    }
    pub(super) fn bind_native_claim_plan_v2(
        &mut self,
        authority: std::sync::Arc<kaspa_consensus_core::palw_reader_router_v2::PalwNativeRoutingClaimAuthorityV2>,
    ) -> Result<(), E> {
        let row = authority.row();
        let profile = self.profile.as_ref().ok_or(E::Incomplete)?;
        if self.native_routing_execution_v2.is_some()
            || self.resolved_artifact_identity_v2 != Some((row.declaration.class_id, row.declaration.artifact_root))
            || self.class_profile_id != row.declaration.class_id
            || authority.native_profile() != profile
            || authority.job().shape_profile_id != row.declaration.class_id
            || self.native_routing_claim_v2.as_ref().is_some_and(|old| !old.same_claim_v2(&authority))
        {
            return Err(E::Substitution);
        }
        let native = self.plan.as_ref().ok_or(E::Incomplete)?.bind_reader_router_v2(profile, &row.declaration, authority.network())?;
        self.plan = Some(native.plan);
        self.native_routing_plan_lease_v2 = native.lease;
        self.native_routing_claim_v2 = Some(authority);
        Ok(())
    }
    pub(super) fn routed_material_bundle_from_fold_v2(
        &self,
        capture: &[u8],
        prompt: &[u32],
        roots: PalwClaimRootsV1,
        authority: &PalwMaterialNativeAuthorityV2,
        max_bytes: u64,
    ) -> Result<NativeRetainedV1<PalwHeldMaterialBundleV2>, E> {
        let body = capture.strip_prefix(&crate::produce::PALW_BASE0_FP_MATERIAL_MAGIC_V2).ok_or(E::Substitution)?;
        let material = retain_native_value_decode_classified_v1::<crate::produce::Base0FpMaterialV2, std::io::Error>(body, || {
            borsh::from_slice(body)
        })
        .map_err(|e| match e {
            NativeDecodeErrorV1::Resource(e) => E::LocalResource(e),
            NativeDecodeErrorV1::Decode(e) => E::Authentication(e.to_string()),
        })?;
        if material.version != crate::produce::PALW_BASE0_FP_MATERIAL_VERSION_V2 {
            return Err(E::Substitution);
        }
        let b = &material.binding;
        authority.record().validate(b)?;
        if authority.record().ladder != self.step_ladder_cap()
            || !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
                authority.record().form,
                prompt,
                &b.job_context.prompt_token_ids_hash,
            )
        {
            return Err(E::Substitution);
        }
        if self.resolved_artifact_identity_v2 != Some((authority.record().class_id, authority.record().artifact_root)) {
            return Err(E::Substitution);
        }
        let native_plan = self
            .plan
            .as_ref()
            .ok_or_else(|| E::Unsupported("Q36 fold has no actual graph execution plan".into()))?
            .bind_reader_router_v2(&b.shape_profile, &authority.record().routing.declaration, authority.record().network)?;
        let plan = &native_plan.plan;
        let interval = self.checkpoint_interval().ok_or(E::Incomplete)?;
        let source = material.native_heap_v1("Q36 V2 folded material source");
        let source_bytes = source.bytes().ok_or_else(|| E::LocalResource(source))?;
        let mut preparation = PalwMaterialConstructionRequestsV2::new();
        preparation.add_v1(source_bytes.checked_mul(4).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
        preparation.vec_v1::<(u128, u128, u128)>(u64::from(b.shape_profile.global_node_count())).map_err(E::LocalResource)?;
        preparation.vec_v1::<(u128, u128, u128)>(u64::from(b.shape_profile.global_node_count())).map_err(E::LocalResource)?;
        preparation
            .vec_v1::<u8>((borsh::object_length(&b.shape_profile).map_err(|_| E::Capacity)? as u64).max(1024))
            .map_err(E::LocalResource)?;
        let _preparation = reserve_native_requests_v1(preparation.bytes_v1()).map_err(E::LocalResource)?;
        crate::produce::base0_material_answers_the_claim_v1(self, b, &material.generated_token_ids, &roots)
            .map_err(|e| E::Authentication(format!("native V2 fold job: {e:?}")))?;
        if !crate::produce::base0_fp_material_matches_claim_v2(
            &material,
            roots.execution_root,
            roots.trace_root,
            crate::produce::Base0SeatFamilyV1::Qwen36,
        )
        .map_err(|e| E::Authentication(format!("native V2 fold roots: {e:?}")))?
        {
            return Err(E::Substitution);
        }
        let runtime = self
            .resource_profile_v1(Some(&b.job_context), kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::FullSeat)
            .ok_or_else(|| {
                E::LocalResource(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Qwen36Backend>(
                    "Q36 routed fold replay",
                    "actual native replay working set cannot be derived",
                ))
            })?;
        // Runtime/cache, decoded source, one gathered unit, and the cumulative
        // complete-bundle destination coexist. No memo survives this call.
        let mut replay = PalwMaterialConstructionRequestsV2::new();
        replay.add_v1(runtime.working_set_bytes()).map_err(E::LocalResource)?;
        let state_bytes = runtime.checkpoint_bytes.checked_add(runtime.gdn_state_bytes).ok_or(E::Capacity)?;
        // Serialization/root/path construction can coexist with the live
        // runtime cache and the independently copied destination chunk.
        replay.vec_v1::<u8>(state_bytes.checked_mul(4).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
        if b.shape_profile.state_chunk_map_id != Hash64::default() {
            let chunks = kaspa_consensus_core::palw_state_chunk_map::palw_state_chunk_count_at_v1(
                &b.shape_profile,
                u32::try_from(runtime.end_rows).map_err(|_| E::Capacity)?,
            )
            .ok_or_else(|| {
                E::LocalResource(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Qwen36Backend>(
                    "Q36 fold state serialization",
                    "actual state chunk geometry cannot be derived",
                ))
            })?;
            // A held map's last position may omit the periodic recurrence
            // section. Include its actual two kinds × layers × heads as well.
            let chunks = chunks
                .checked_add(
                    2u64.checked_mul(runtime.recurrence_layers)
                        .and_then(|n| n.checked_mul(u64::from(b.shape_profile.gdn_heads)))
                        .ok_or(E::Capacity)?,
                )
                .ok_or(E::Capacity)?;
            replay.vec_v1::<Vec<u8>>(chunks.checked_mul(4).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
            replay.vec_v1::<Hash64>(chunks.checked_mul(6).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
        }
        let _runtime = reserve_native_requests_v1(replay.bytes_v1()).map_err(E::LocalResource)?;
        let kernels = Qwen36IntervalKernels { artifact: &self.artifact, plan };
        // Own, transient recomputation. A provider's complete bundle must not
        // grow a long-lived seat memo behind a released construction owner.
        let anchor_state = |covered| {
            let mut kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::new(&self.artifact, plan);
            crate::fp_recompute::base0_fp_recompute_state_at_covered_v1(
                &b.shape_profile,
                &b.job_context,
                prompt,
                &material.generated_token_ids,
                covered,
                &mut kernels,
                authority.record().form,
            )
            .ok()
        };
        PalwHeldMaterialBundleV2::from_source(b, authority, max_bytes, |missing| {
            let mut q = PalwMaterialConstructionRequestsV2::new();
            q.add_v1(source_bytes.checked_mul(4).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
            q.vec_v1::<u32>(prompt.len() as u64).map_err(E::LocalResource)?;
            q.vec_v1::<usize>(prompt.len() as u64).map_err(E::LocalResource)?;
            if let PalwHeldMissingV1::StepLeaf { leaf } = missing {
                let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&b.shape_profile, &b.job_context, leaf)
                    .ok_or(E::Incomplete)?;
                if authority.record().routing.declaration.selected_router(coord.node_slot).is_some() {
                    return crate::fp_interval::routing_v2::routed_unit_from_fold_v2(
                        &material,
                        leaf,
                        prompt,
                        interval,
                        authority.record().ladder,
                        &kernels,
                        authority.record().form,
                        &authority.record().routing.declaration,
                    );
                }
                let (node, _) = b.shape_profile.resolve_node_slot(coord.node_slot).ok_or(E::Incomplete)?;
                // Legacy recurrent/KV inputs may span the prefix. Quote their
                // finite metadata before asking the historical enumerator for
                // the exact list; it does not select a router for this leaf.
                let upper = b.step_leaf_count.checked_mul(node.input_refs.len() as u64 + 2).ok_or(E::Capacity)?;
                let mut enumeration = PalwMaterialConstructionRequestsV2::new();
                enumeration.vec_v1::<(u64, kaspa_consensus_core::palw_step::PalwStepCoordinateV1)>(upper).map_err(E::LocalResource)?;
                enumeration
                    .vec_v1::<Vec<(u64, kaspa_consensus_core::palw_step::PalwStepCoordinateV1)>>(upper)
                    .map_err(E::LocalResource)?;
                enumeration.vec_v1::<(u32, u32)>(upper).map_err(E::LocalResource)?;
                let _enumeration = reserve_native_requests_v1(enumeration.bytes_v1()).map_err(E::LocalResource)?;
                let inputs =
                    kaspa_consensus_core::palw_step_refute::canonical_input_leaves_v1(&b.shape_profile, &b.job_context, &coord)
                        .ok_or(E::Incomplete)?;
                let count = inputs.iter().try_fold(1u64, |n, row| n.checked_add(row.len() as u64)).ok_or(E::Capacity)?;
                q.vec_v1::<(u64, kaspa_consensus_core::palw_step::PalwStepCoordinateV1)>(count).map_err(E::LocalResource)?;
                q.vec_v1::<Vec<(u64, kaspa_consensus_core::palw_step::PalwStepCoordinateV1)>>(inputs.len() as u64)
                    .map_err(E::LocalResource)?;
                q.vec_v1::<(u32, u32)>(inputs.len() as u64).map_err(E::LocalResource)?;
                let block = 1u64.checked_shl(material.step_tree.retain_level()).ok_or(E::Capacity)?.min(b.step_leaf_count);
                legacy_map::<u64, ()>(&mut q, count.checked_mul(2).ok_or(E::Capacity)?)?;
                legacy_map::<u64, PalwStepTileLeafV1>(&mut q, count)?;
                legacy_map::<u64, Vec<Hash64>>(&mut q, count)?;
                q.vec_v1::<Hash64>(count.checked_mul(block).and_then(|n| n.checked_add(8 * block)).ok_or(E::Capacity)?)
                    .map_err(E::LocalResource)?;
                q.vec_v1::<PalwStepTileLeafV1>(count.checked_mul(2).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
                q.vec_v1::<Vec<Hash64>>(count).map_err(E::LocalResource)?;
                q.vec_v1::<(u64, u64)>(count).map_err(E::LocalResource)?;
                q.vec_v1::<(u64, &[Hash64])>(2).map_err(E::LocalResource)?;
                q.vec_v1::<(u64, Vec<Vec<Hash64>>)>(2).map_err(E::LocalResource)?;
                q.vec_v1::<kaspa_consensus_core::palw_step_refute::PalwStepInputRowV1>(inputs.len() as u64)
                    .map_err(E::LocalResource)?;
                let depth = u64::from(64 - b.step_leaf_count.leading_zeros());
                q.vec_v1::<Vec<Hash64>>(2 * (u64::from(material.step_tree.retain_level()) + 1) + depth).map_err(E::LocalResource)?;
                q.vec_v1::<Hash64>(count.checked_mul(2 * depth).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
                for c in std::iter::once(&coord).chain(inputs.iter().flatten().map(|(_, c)| c)) {
                    let words = kaspa_consensus_core::palw_step_leg::palw_expected_tile_values_v1(&b.shape_profile, &b.job_context, c)
                        .ok_or(E::Incomplete)?;
                    q.vec_v1::<u8>(u64::from(words).checked_mul(12).ok_or(E::Capacity)?.max(256)).map_err(E::LocalResource)?;
                }
                return construct_material_unit_v2(q, || {
                    let r = crate::fp_interval::base0_fp_leaf_refutation_from_fold_v1(
                        &material,
                        leaf,
                        prompt,
                        interval,
                        authority.record().ladder,
                        &kernels,
                        &anchor_state,
                        authority.record().form,
                    )
                    .map_err(|e| E::Authentication(e.to_string()))?;
                    let (refutation, prompt_ids_opening) =
                        kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(authority.record().form, r)
                            .map_err(|e| E::Authentication(e.to_string()))?;
                    Ok(PalwHeldMaterialUnitV2::Legacy(PalwHeldDisclosureV1::StepLeaf {
                        evidence: Box::new(kaspa_consensus_core::palw_shard_court_v1::PalwLeafEvidenceV1 {
                            refutation,
                            artifact_openings: Vec::new(),
                            prompt_ids_opening,
                        }),
                    }))
                });
            }
            if let PalwHeldMissingV1::StepRange { count, .. } = missing {
                let block = 1u64.checked_shl(material.step_tree.retain_level()).ok_or(E::Capacity)?;
                q.vec_v1::<Hash64>(u64::from(count).checked_add(2 * block).and_then(|n| n.checked_mul(4)).ok_or(E::Capacity)?)
                    .map_err(E::LocalResource)?;
            }
            construct_material_unit_v2(q, || {
                let disclosure = match missing {
                    PalwHeldMissingV1::PromptIdsTile { tile } => PalwHeldDisclosureV1::PromptIdsTile {
                        opening: kaspa_consensus_core::palw_prompt_ids_v1::prompt_ids_opening_v1(
                            prompt,
                            tile.checked_mul(kaspa_consensus_core::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN).ok_or(E::Capacity)?,
                        )
                        .map_err(|e| E::Authentication(e.to_string()))?,
                    },
                    PalwHeldMissingV1::StateChunk { checkpoint, chunk } => {
                        let retention = crate::produce::Base0RetentionV1::Folded((*material).clone());
                        let (anchor, chunk) =
                            crate::fp_interval::base0_fp_held_state_chunk_answer_v1(&retention, checkpoint, chunk, &anchor_state)
                                .map_err(|e| E::Authentication(e.to_string()))?;
                        PalwHeldDisclosureV1::StateChunk { anchor, chunk }
                    }
                    PalwHeldMissingV1::StepRange { first, count } => {
                        let retention = crate::produce::Base0RetentionV1::Folded((*material).clone());
                        let opening = crate::fp_interval::base0_fp_held_step_range_answer_v1(
                            &retention,
                            first,
                            count,
                            prompt,
                            interval,
                            authority.record().ladder,
                            &kernels,
                            &anchor_state,
                            authority.record().form,
                        )
                        .map_err(|e| E::Authentication(e.to_string()))?;
                        PalwHeldDisclosureV1::StepRange { opening }
                    }
                    PalwHeldMissingV1::StepLeaf { .. } => return Err(E::Incomplete),
                };
                Ok(PalwHeldMaterialUnitV2::Legacy(disclosure))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_reader_router_v2::{PalwReaderRouterBindingV2, PalwReaderRouterDeclarationV2, needs_reader_router_v2},
        palw_step::{PalwStepCoordinateV1, canonical_step_leaf_index, kernel_semantics_id_v1},
        palw_step_refute::KDESC_Q36_ROUTER_TOPK,
    };

    // These source controls deliberately use an actual Q36 artifact, plan and
    // execution, not a stdout fixture standing in for a native accepted claim.
    // They supply no selected-verifier approval and remain unexecuted until the
    // consolidated validation phase.
    #[test]
    fn actual_q36_fold_gather_is_the_dense_selected_router_opening() {
        let (artifact, v5) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        let geometry = kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
            n_ctx: v5.n_ctx,
            ..crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4)
        };
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(geometry).unwrap();
        let artifact = std::sync::Arc::new(artifact);
        let backend =
            Qwen36Backend::from_registered_profile(artifact.clone(), b"misaka-palw-test".to_vec(), profile.clone(), (3, 4)).unwrap();
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x524f5554)).unwrap();
        let folded = backend.execute(&job, &prompt).unwrap();
        let material = crate::produce::base0_fp_material_decode_v2(&folded.material).unwrap();
        let decoded = retain_native_value_decode_classified_v1::<crate::produce::Base0FpMaterialV2, std::io::Error>(
            &folded.material[crate::produce::PALW_BASE0_FP_MATERIAL_MAGIC_V2.len()..],
            || borsh::from_slice(&folded.material[crate::produce::PALW_BASE0_FP_MATERIAL_MAGIC_V2.len()..]),
        )
        .unwrap();
        assert_eq!(*decoded, material);
        let plan = backend.plan.as_ref().unwrap();
        let dense = qwen36_execute_for_attempt_capped_v1(&artifact, &profile, plan, &job, &prompt, backend.step_ladder_cap()).unwrap();
        let mut bindings = Vec::new();
        for reader_slot in 0..profile.global_node_count() {
            let (reader, layer) = profile.resolve_node_slot(reader_slot).unwrap();
            if !needs_reader_router_v2(reader) {
                continue;
            }
            // Fixture construction only. Production never replaces the
            // immutable owner's declaration by this uniqueness search.
            let routers: Vec<_> = (0..reader_slot)
                .filter(|slot| {
                    let (n, l) = profile.resolve_node_slot(*slot).unwrap();
                    l == layer && n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)
                })
                .collect();
            assert_eq!(routers.len(), 1);
            bindings.push(PalwReaderRouterBindingV2 { reader_slot, router_slot: routers[0] });
        }
        let declaration = PalwReaderRouterDeclarationV2 {
            version: 2,
            class_id: profile.shape_profile_id(),
            artifact_root: Hash64::from_u64_word(9),
            bindings,
        };
        assert!(!declaration.bindings.is_empty(), "the actual fixture must exercise routed readers");
        declaration.check_profile(&profile).unwrap();
        let kernels = Qwen36IntervalKernels { artifact: &artifact, plan };
        let prompt: Vec<u32> = prompt.iter().map(|v| *v as u32).collect();
        for selected in declaration.bindings.iter().take(2) {
            let target = PalwStepCoordinateV1 { call_index: 0, node_slot: selected.reader_slot, position: 0, tile_index: 0 };
            let leaf = canonical_step_leaf_index(&profile, &job, &target).unwrap();
            let from_fold = crate::fp_interval::routing_v2::routed_unit_from_fold_v2(
                &material,
                leaf,
                &prompt,
                backend.checkpoint_interval().unwrap(),
                backend.step_ladder_cap(),
                &kernels,
                backend.prompt_ids_form,
                &declaration,
            )
            .unwrap();
            let PalwHeldMaterialUnitV2::Routed(from_fold) = &*from_fold else {
                panic!("explicit routed unit");
            };
            let from_dense = kaspa_consensus_core::palw_step_refute::routing_v2::routed_refutation_from_capture_v2(
                &dense.binding,
                target,
                &declaration,
                &dense.tiles.leaves,
                &dense.tiles.tiles,
                backend.step_ladder_cap(),
            )
            .unwrap();
            assert_eq!(from_fold, &*from_dense);
            assert!(
                crate::fp_interval::routing_v2::routed_unit_from_fold_v2(
                    &material,
                    leaf,
                    &prompt,
                    backend.checkpoint_interval().unwrap(),
                    material.binding.step_leaf_count - 1,
                    &kernels,
                    backend.prompt_ids_form,
                    &declaration
                )
                .is_err(),
                "the native maximum is an actual refusal"
            );
            let mut wrong = material.clone();
            wrong.binding.step_merkle_root = Hash64::from_u64_word(0xdead);
            assert!(
                crate::fp_interval::routing_v2::routed_unit_from_fold_v2(
                    &wrong,
                    leaf,
                    &prompt,
                    backend.checkpoint_interval().unwrap(),
                    backend.step_ladder_cap(),
                    &kernels,
                    backend.prompt_ids_form,
                    &declaration
                )
                .is_err(),
                "another retained root cannot become evidence"
            );
        }
    }
}
