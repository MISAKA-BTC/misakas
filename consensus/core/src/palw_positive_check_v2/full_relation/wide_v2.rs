//! Explicit wide LOCAL full-relation prototype. Existing local manifest/domain/limits are reused;
//! class/context/program bind the profile. No native operation, authority or activation is added.
//! Panel claim mismatch rejects only its witness. Miner output is compared only after ALL exact
//! relations, trace bridges and greedy-prefix rows have completed from authenticated source data.
use super::*;
use crate::{
    palw_logits_q24_v2::WideLogitsEventV2,
    palw_positive_check_v2::expanded_plan::wide_v2::ExpandedPlanV2,
    palw_tir_court_v1::source_only::wide_v2::{EvaluationDiagnosticsV2, TirSourceOnlyMaterialV2, authenticate_source_only_v2},
    palw_tir_step_v1::wide_v2::verify_tir_binding_v2,
};
mod record_access_sealed {
    pub trait Sealed {}
}
/// Read-only sealed record view. Only signature-checked resident records and
/// the private single-claim streaming adapter implement this interface.
#[doc(hidden)]
pub trait WideRecordAccessV2: record_access_sealed::Sealed {
    fn binding(&self) -> &PalwTirStepBindingV1;
    fn generated(&self) -> &[u32];
    fn claim(&self, index: u64) -> Option<&[i128]>;
}
impl record_access_sealed::Sealed for PrototypePanelRecord {}
impl WideRecordAccessV2 for PrototypePanelRecord {
    fn binding(&self) -> &PalwTirStepBindingV1 {
        &self.binding
    }
    fn generated(&self) -> &[u32] {
        &self.generated
    }
    fn claim(&self, index: u64) -> Option<&[i128]> {
        self.claims.get(usize::try_from(index).ok()?).map(Vec::as_slice)
    }
}
/// Stream-backed private record/Node reader; no native authority or new codec.
pub mod stream;
pub struct StepTargetV2<'a> {
    pub opening: &'a PalwStepOpeningV1,
    pub preimage: &'a PalwStepTileLeafV1,
    pub trace: Option<&'a WideLogitsEventV2>,
}
pub struct FullRelationPreparationV2 {
    plan: ExpandedPlanV2,
    class: PalwTirClassV1,
    artifact: Hash64,
    job: PalwJobContextV2,
    result: MinerResultCommit,
    form: PalwPromptIdsFormV1,
    limits: FullRelationLimits,
    domains: PrototypeDomains,
    vocab: u64,
}
impl FullRelationPreparationV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn authenticate(
        rooted: &AdmissionContext,
        class: &PalwTirClassV1,
        artifact: Hash64,
        job: &PalwJobContextV2,
        form: PalwPromptIdsFormV1,
        limits: FullRelationLimits,
        domains: PrototypeDomains,
        result: MinerResultCommit,
        miner_key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<Self, FullRelationError> {
        rooted.check(ClaimForm::PositiveResultV2, &result).map_err(|_| FullRelationError::Binding)?;
        result.authenticate(miner_key, signature).map_err(|_| FullRelationError::Signature)?;
        // The planner checks format lengths before hashing foreign typed data.
        let plan = ExpandedPlanV2::derive(class, artifact, job, limits.plan).map_err(FullRelationError::Plan)?;
        if result.job != digest(job.job_id)
            || result.execution != digest(job.context_hash())
            || result.registered_class != digest(class.class_id(&artifact))
            || result.artifact != digest(artifact)
            || result.program != digest(class.graph_ir_root())
            || result.input != digest(job.prompt_token_ids_hash)
            || result.tokenizer != digest(job.tokenizer_id)
            || result.execution_rules != digest(job.cu_ruleset_id)
            || result.generation_seed != job.execution_seed
            || result.prefill != job.declared_prefill_tokens
            || result.decode != job.exact_decode_tokens
            || result.max_context != job.max_context_tokens
        {
            return Err(FullRelationError::Binding);
        }
        if result.temperature_q24 != 0 || result.selector != greedy_selector_id() {
            return Err(FullRelationError::UnsupportedProfile);
        }
        let space = plan.step_space();
        let output = &space.program().blocks[space.program().schedule.post as usize].nodes[space.program().logits as usize].out;
        // The explicit wide planner and source path pin the adopted strict
        // I64 final post profile; the old preparation remains I32-only.
        if output.dtype != DType::I64 {
            return Err(FullRelationError::UnsupportedProfile);
        }
        let vocab = output
            .shape
            .iter()
            .try_fold(1u64, |n, d| match d {
                Dim::Fixed(v) => n.checked_mul(u64::from(*v)),
                _ => None,
            })
            .filter(|n| *n > 0 && *n <= u64::from(u32::MAX))
            .ok_or(FullRelationError::UnsupportedProfile)?;
        let count = plan.summary().relations;
        cap_mul(count, limits.source.max_source_reads, limits.max_total_source_reads)?;
        cap_mul(count, mul(limits.source.max_material_bytes, 2)?, limits.max_total_material_bytes)?;
        cap_mul(mul(count, 3)?, plan.summary().geometry_visit_bound, limits.max_record_geometry_visits)?;
        if limits.source.max_source_reads == 0
            || limits.source.max_material_bytes == 0
            || limits.source.max_material_units == 0
            || limits.source.max_target_lanes == 0
            || limits.source.max_target_bytes == 0
            || limits.source.max_auth_storage_bytes == 0
            || limits.source.max_workspace_bytes == 0
            || limits.max_record_bytes == 0
            || limits.max_record_heap_bytes == 0
        {
            return Err(FullRelationError::Budget);
        }
        Ok(Self { plan, class: class.clone(), artifact, job: job.clone(), result, form, limits, domains, vocab })
    }
    pub fn plan(&self) -> &ExpandedPlanV2 {
        &self.plan
    }
    /// Independently authenticated Miner metadata; read-only, not native credit.
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    /// Original registration metadata. The job tokenizer remains a separate field.
    pub fn registered_class(&self) -> &PalwTirClassV1 {
        &self.class
    }
    /// Exact independently resolved job; no metadata substitution or rewriting.
    pub fn job(&self) -> &PalwJobContextV2 {
        &self.job
    }
    /// Original artifact identity, not a computation-success capability.
    pub fn artifact(&self) -> Hash64 {
        self.artifact
    }
    /// Immutable per-material Node limits. Producer/authentication-local caps
    /// may be stricter; session-wide work is never a replacement for these.
    pub fn source_limits(&self) -> TirSourceOnlyLimitsV1 {
        self.limits.source
    }
    pub fn prompt_form(&self) -> PalwPromptIdsFormV1 {
        self.form
    }
    /// Panel-owned additional record. Neither Miner Result nor original class,
    /// layout, step tree or trace is regenerated/resigned. Every claim is an
    /// exact original integer in i128 storage, within its ORIGINAL dtype.
    pub fn freeze_record(
        &self,
        binding: PalwTirStepBindingV1,
        generated: Vec<u32>,
        author: PrincipalId,
        claims: Vec<Vec<i128>>,
    ) -> Result<PrototypePanelRecord, FullRelationError> {
        self.validate_binding(&binding, &generated)?;
        if claims.len() as u64 != self.plan.summary().relations {
            return Err(FullRelationError::Binding);
        }
        let heap = record_heap(&binding, &generated, &claims)?;
        if heap > self.limits.max_record_heap_bytes {
            return Err(FullRelationError::Budget);
        }
        for (index, values) in claims.iter().enumerate() {
            let relation = self.plan.relation_at(index as u64).ok_or(FullRelationError::Binding)?;
            let (dtype, count) = match relation.kind {
                ExpandedRelationKind::Legacy(l) => (l.dtype, l.value_count),
                ExpandedRelationKind::RecomputeNode { dtype, lane_count, .. } => (dtype, lane_count),
            };
            if values.len() != count as usize || values.iter().any(|v| !dtype.contains(*v)) {
                return Err(FullRelationError::Binding);
            }
        }
        let mut record = PrototypePanelRecord {
            binding,
            generated,
            author,
            claims,
            policy: self.limits.words(),
            result: self.result.id(),
            form: self.form,
            domains: self.domains.clone(),
            commitment: Digest([0; 64]),
            heap,
        };
        record.commitment = record.hash(self.limits.max_record_bytes)?;
        Ok(record)
    }
    fn validate_binding(&self, binding: &PalwTirStepBindingV1, generated: &[u32]) -> Result<(), FullRelationError> {
        if binding.class != self.class
            || binding.job_context != self.job
            || binding.artifact_root != self.artifact
            || binding.step_leaf_count != self.plan.summary().legacy_relations
            || generated.len() != self.job.exact_decode_tokens as usize
            || generated.iter().any(|v| u64::from(*v) >= self.vocab)
        {
            return Err(FullRelationError::Binding);
        }
        verify_tir_binding_v2(binding, self.limits.plan.max_legacy_leaves).map_err(|_| FullRelationError::Binding)?;
        Ok(())
    }
    pub fn begin(
        self,
        record: PrototypePanelRecord,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<FullRelationCheckerV2, FullRelationError> {
        if record.result != self.result.id()
            || record.policy != self.limits.words()
            || record.domains != self.domains
            || record.form != self.form
        {
            return Err(FullRelationError::Binding);
        }
        self.validate_binding(&record.binding, &record.generated)?;
        if principal(key) != record.author || !verify_signature(key, record.commitment, signature, &self.domains.signature_context) {
            return Err(FullRelationError::Signature);
        }
        let geometry = mul(mul(self.plan.summary().relations, 2)?, self.plan.summary().geometry_visit_bound)?;
        let progress = FullRelationProgress { charged_geometry_visits: geometry, ..FullRelationProgress::default() };
        Ok(FullRelationCheckerV2 { prepared: self, record, progress, best: None, next_lane: 0 })
    }
}

/// Immutable local manifest, not a shared DTO or old execution-record root.
pub struct FullRelationCheckerV2<R: WideRecordAccessV2 = PrototypePanelRecord> {
    prepared: FullRelationPreparationV2,
    record: R,
    progress: FullRelationProgress,
    best: Option<(i64, u32)>,
    next_lane: u64,
}
impl<R: WideRecordAccessV2> FullRelationCheckerV2<R> {
    /// Borrow retained preparation for material construction. This grants no
    /// mutable progress access or additional native authority.
    pub fn preparation(&self) -> &FullRelationPreparationV2 {
        &self.prepared
    }
    pub fn progress(&self) -> FullRelationProgress {
        self.progress
    }
    /// Material is unauthenticated delivery. Membership/canonical failures do
    /// not reject the signed candidate. Only genuine completed recomputation
    /// versus its immutable signed claims can create PanelMismatch.
    pub fn submit(
        &mut self,
        material: &TirSourceOnlyMaterialV2<'_>,
        legacy: Option<StepTargetV2<'_>>,
    ) -> Result<(), FullRelationError> {
        self.submit_observed(material, legacy, None)
    }
    /// Same attempt and cumulative charges as submit. Diagnostics expose local
    /// source evaluation only; a Complete observation is not Panel acceptance.
    pub fn submit_with_diagnostics(
        &mut self,
        material: &TirSourceOnlyMaterialV2<'_>,
        legacy: Option<StepTargetV2<'_>>,
        diagnostics: &mut EvaluationDiagnosticsV2,
    ) -> Result<(), FullRelationError> {
        *diagnostics = EvaluationDiagnosticsV2::default();
        self.submit_observed(material, legacy, Some(diagnostics))
    }
    fn submit_observed(
        &mut self,
        material: &TirSourceOnlyMaterialV2<'_>,
        legacy: Option<StepTargetV2<'_>>,
        diagnostics: Option<&mut EvaluationDiagnosticsV2>,
    ) -> Result<(), FullRelationError> {
        if self.progress.outcome != CheckOutcome::Checking {
            return Err(FullRelationError::Closed);
        }
        if self.progress.next_relation == self.prepared.plan.summary().relations {
            return Err(FullRelationError::Incomplete);
        }
        let result = self.submit_inner(material, legacy, diagnostics);
        if let Err(e) = &result {
            self.progress.outcome = match e {
                FullRelationError::PanelMismatch => CheckOutcome::WitnessRejected,
                FullRelationError::Budget | FullRelationError::Unverified => CheckOutcome::EndedUnverified,
                _ => CheckOutcome::Checking,
            };
        }
        result
    }
    fn submit_inner(
        &mut self,
        material: &TirSourceOnlyMaterialV2<'_>,
        legacy: Option<StepTargetV2<'_>>,
        diagnostics: Option<&mut EvaluationDiagnosticsV2>,
    ) -> Result<(), FullRelationError> {
        let limits = self.prepared.limits;
        // All four reservations are calculated before any progress is changed.
        // There is no per-relation/failed-delivery budget reset.
        let bytes =
            reserve(self.progress.charged_material_bytes, mul(limits.source.max_material_bytes, 2)?, limits.max_total_material_bytes)?;
        let elements = reserve(self.progress.charged_elements, limits.plan.max_elements_per_relation, limits.plan.max_total_elements)?;
        let terms = reserve(self.progress.charged_terms, limits.plan.max_terms_per_relation, limits.plan.max_total_terms)?;
        let reads = reserve(self.progress.charged_source_reads, limits.source.max_source_reads, limits.max_total_source_reads)?;
        let geometry = reserve(
            self.progress.charged_geometry_visits,
            self.prepared.plan.summary().geometry_visit_bound,
            limits.max_record_geometry_visits,
        )?;
        self.progress.charged_material_bytes = bytes;
        self.progress.charged_elements = elements;
        self.progress.charged_terms = terms;
        self.progress.charged_source_reads = reads;
        self.progress.charged_geometry_visits = geometry;
        let relation = self.prepared.plan.relation_at(self.progress.next_relation).ok_or(FullRelationError::Incomplete)?;
        if relation.legacy_frontier != self.progress.legacy_frontier {
            return Err(FullRelationError::Unverified);
        }
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: limits.plan.max_legacy_leaves,
            prompt_form: self.prepared.form,
            limits: DemandLimits {
                max_elements: limits.plan.max_elements_per_relation,
                max_terms: limits.plan.max_terms_per_relation,
            },
        };
        if let Some(pin) = material.decode_tokens
            && pin.generated_token_ids.as_slice() != self.record.generated()
        {
            return Err(FullRelationError::Delivery);
        }
        let source =
            authenticate_source_only_v2(self.record.binding(), *material, self.progress.legacy_frontier, &rules, limits.source)
                .map_err(source_error)?;
        let evaluation = match &relation.kind {
            ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
                if legacy.is_some() {
                    return Err(FullRelationError::Delivery);
                }
                match diagnostics {
                    Some(d) => source.evaluate_node_with_diagnostics(*ctx, *node, *first_lane, *lane_count, d),
                    None => source.evaluate_node(*ctx, *node, *first_lane, *lane_count),
                }
                .map_err(source_error)?
            }
            ExpandedRelationKind::Legacy(leaf) => {
                let target = legacy.as_ref().ok_or(FullRelationError::Delivery)?;
                self.authenticate_legacy(leaf, target, limits.source.max_material_bytes)?;
                match diagnostics {
                    Some(d) => source.evaluate_legacy_with_diagnostics(leaf, d),
                    None => source.evaluate_legacy(leaf),
                }
                .map_err(source_error)?
            }
        };
        let values = evaluation.values();
        if values != self.record.claim(self.progress.next_relation).ok_or(FullRelationError::Delivery)? {
            return Err(FullRelationError::PanelMismatch);
        }
        // No coverage/selection mutation until membership, direct arithmetic,
        // canonical closure and every required legacy/logits check succeeded.
        if let ExpandedRelationKind::Legacy(leaf) = &relation.kind {
            let target = legacy.as_ref().ok_or(FullRelationError::Delivery)?;
            let width = if leaf.dtype == DType::I64 { 8 } else { 4 };
            for (value, bytes) in values.iter().zip(target.preimage.values_le.chunks_exact(width)) {
                let value_in_tree = if width == 8 {
                    i64::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?) as i128
                } else if leaf.dtype == DType::Idx {
                    u32::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?) as i128
                } else {
                    i32::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?) as i128
                };
                if *value != value_in_tree {
                    return Err(FullRelationError::PanelMismatch);
                }
            }
            self.check_selection(leaf, target)?;
            self.progress.legacy_frontier += 1;
        }
        self.progress.next_relation += 1;
        Ok(())
    }
    fn authenticate_legacy(
        &self,
        leaf: &crate::palw_tir_step_v1::PalwTirLeafV1,
        target: &StepTargetV2<'_>,
        cap: u64,
    ) -> Result<(), FullRelationError> {
        let mut counter = ByteCounter { bytes: 0, cap };
        (target.opening, target.preimage, target.trace).serialize(&mut counter).map_err(|_| FullRelationError::Budget)?;
        let binding = self.record.binding();
        let width = if leaf.dtype == DType::I64 { 8 } else { 4 };
        let version = if width == 8 { 2 } else { PALW_TIR_STEP_LEAF_VERSION_V1 };
        if self.prepared.plan.step_space().leaf_at(&binding.job_context, leaf.index) != Some(*leaf)
            || target.opening.leaf_index != leaf.index
            || target.preimage.version != version
            || target.preimage.coord != leaf.coord
            || target.preimage.value_count != leaf.value_count
            || target.preimage.values_le.len() as u64 != u64::from(leaf.value_count) * width
            || target.opening.leaf_hash
                != step_tile_leaf_hash_v1(
                    &binding.job_context.context_hash(),
                    &binding.class.class_id(&binding.artifact_root),
                    target.preimage,
                )
            || step_opening_root_capped_v1(binding.step_leaf_count, target.opening, self.prepared.limits.plan.max_legacy_leaves)
                .map_err(|_| FullRelationError::Delivery)?
                != binding.step_merkle_root
        {
            return Err(FullRelationError::Delivery);
        }
        Ok(())
    }
    fn check_selection(
        &mut self,
        leaf: &crate::palw_tir_step_v1::PalwTirLeafV1,
        target: &StepTargetV2<'_>,
    ) -> Result<(), FullRelationError> {
        let space = self.prepared.plan.step_space();
        let PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else {
            return if target.trace.is_none() { Ok(()) } else { Err(FullRelationError::Delivery) };
        };
        if occurrence as usize != space.occurrences().len() - 1 || node != space.program().logits {
            return if target.trace.is_none() { Ok(()) } else { Err(FullRelationError::Delivery) };
        }
        let row = leaf
            .position
            .checked_add(1)
            .and_then(|p| p.checked_sub(self.prepared.job.declared_prefill_tokens))
            .ok_or(FullRelationError::Unverified)?;
        if row != self.progress.checked_rows || first_element != self.next_lane {
            return Err(FullRelationError::Unverified);
        }
        let trace = target.trace.ok_or(FullRelationError::Delivery)?;
        if trace.generated.as_slice() != self.record.generated() || trace.row != row {
            return Err(FullRelationError::Delivery);
        }
        trace
            .authenticate(
                &self.prepared.job,
                &self.record.binding().full_logits_trace_root,
                self.prepared.vocab as usize,
                self.prepared.limits.plan.max_legacy_leaves,
            )
            .map_err(|_| FullRelationError::Delivery)?;
        let tile_width = crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64;
        if trace.tile_opening.leaf_index != first_element / tile_width {
            return Err(FullRelationError::Delivery);
        }
        let offset = (first_element % tile_width) as usize;
        let end = offset
            .checked_add(leaf.value_count as usize)
            .filter(|end| *end <= trace.lanes.len())
            .ok_or(FullRelationError::Delivery)?;
        for (bytes, value) in target.preimage.values_le.chunks_exact(8).zip(&trace.lanes[offset..end]) {
            if i64::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?) != *value {
                return Err(FullRelationError::Delivery);
            }
        }
        let mut best = self.best;
        let mut next = self.next_lane;
        for bytes in target.preimage.values_le.chunks_exact(8) {
            let value = i64::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?);
            if best.is_none_or(|(b, _)| value > b) {
                best = Some((value, next as u32));
            }
            next += 1;
        }
        if next > self.prepared.vocab {
            return Err(FullRelationError::Unverified);
        }
        if next == self.prepared.vocab {
            if best.map(|(_, token)| token) != self.record.generated().get(row as usize).copied() {
                return Err(FullRelationError::PanelMismatch);
            }
            self.progress.checked_rows += 1;
            self.best = None;
            self.next_lane = 0;
        } else {
            self.best = best;
            self.next_lane = next;
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<CheckOutcome, FullRelationError> {
        if self.progress.outcome != CheckOutcome::Checking {
            return Err(FullRelationError::Closed);
        }
        if self.progress.next_relation != self.prepared.plan.summary().relations
            || self.progress.legacy_frontier != self.prepared.plan.summary().legacy_relations
            || self.progress.checked_rows != self.prepared.job.exact_decode_tokens
            || self.next_lane != 0
        {
            return Err(FullRelationError::Incomplete);
        }
        self.progress.outcome =
            if digest(palw_attempt_output_root_v1(&self.prepared.job, self.record.generated())) == self.prepared.result.output {
                CheckOutcome::Matched
            } else {
                CheckOutcome::OutputMismatch
            };
        Ok(self.progress.outcome)
    }
}

#[cfg(test)]
mod tests;
