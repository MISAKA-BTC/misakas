//! LOCAL TYPED PROTOTYPE. No shared profile, wire tag, domain, native receipt or
//! credit authority. Caller-local domains are fixed by the verifier, not a peer.
//! The complete bounded Panel manifest is retained; this is not an LLM memory
//! or performance claim. Values authenticated by its signature are comparison
//! targets only. Original inputs and prior legacy leaves supply computation.

use super::{
    CheckOutcome,
    expanded_plan::{ExpandedPlan, ExpandedPlanError, ExpandedPlanLimits, ExpandedRelationKind},
};
use crate::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_opening_root_capped_v1, step_tile_leaf_hash_v1},
    palw_step_refute::{PalwDecodeTokenPinV1, PalwStepRefuteError},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::{
        PalwTirCourtRulesV1, PalwTirLogitsConsistencyV1, PalwTirTraceEventDisclosureV1, PalwTirTraceLanesV1,
        check_tir_logits_consistency_v1,
        source_only::{TirSourceOnlyErrorV1, TirSourceOnlyLimitsV1, TirSourceOnlyMaterialV1, authenticate_source_only_v1},
    },
    palw_tir_step_v1::{PALW_TIR_STEP_LEAF_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1, verify_tir_binding_v1},
    palw_v2::PalwJobContextV2,
};
use borsh::BorshSerialize;
use misaka_palw_positive_contract::{
    identity::{Digest, PUBKEY_BYTES, PrincipalId, SIGNATURE_BYTES, principal, verify_signature},
    messages::{AdmissionContext, ClaimForm, MinerResultCommit},
    tir::greedy_selector_id,
};
use misaka_palw_tir::{DType, Dim, demand::DemandLimits};
use std::{
    io::{self, Write},
    mem::size_of,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FullRelationError {
    Binding,
    UnsupportedProfile,
    Budget,
    Signature,
    /// Unauthenticated/missing/noncanonical delivery; no candidate verdict.
    Delivery,
    /// Correctly attributed frozen Panel values differ from direct evaluation.
    PanelMismatch,
    Unverified,
    Incomplete,
    Closed,
    Plan(ExpandedPlanError),
}

/// There are deliberately no default or globally assigned domain strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrototypeDomains {
    hash_key: Vec<u8>,
    signature_context: Vec<u8>,
}
impl PrototypeDomains {
    /// Fixed independently by the local verifier. Never read these from a wire
    /// carrier, native ruleset, Panel record, or an old Witness field.
    pub fn local_only(hash_key: &[u8], signature_context: &[u8]) -> Result<Self, FullRelationError> {
        if hash_key.is_empty() || hash_key.len() > 64 || signature_context.is_empty() || signature_context.len() > 255 {
            return Err(FullRelationError::Binding);
        }
        Ok(Self { hash_key: hash_key.to_vec(), signature_context: signature_context.to_vec() })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FullRelationLimits {
    pub plan: ExpandedPlanLimits,
    pub source: TirSourceOnlyLimitsV1,
    /// Full retained manifest's canonical structural encoding; no public codec.
    pub max_record_bytes: u64,
    /// Record-owned backing allocations, not all analysis/evaluator/process RSS.
    pub max_record_heap_bytes: u64,
    pub max_total_material_bytes: u64,
    pub max_total_source_reads: u64,
    /// Conservative all-position geometry bound per lookup, three lookups per
    /// relation (freeze/rederive/diagnostics). No unmetered peer target list.
    pub max_record_geometry_visits: u64,
}
impl FullRelationLimits {
    fn words(self) -> Vec<u64> {
        let p = self.plan;
        let s = self.source;
        vec![
            u64::from(p.max_positions),
            p.max_geometry_visits,
            p.max_plan_bytes,
            p.max_legacy_leaves,
            p.max_relations,
            p.max_target_lanes,
            p.max_peak_target_bytes,
            u64::from(p.uncommitted_tile_lanes),
            p.max_elements_per_relation,
            p.max_terms_per_relation,
            p.max_total_elements,
            p.max_total_terms,
            s.max_source_reads,
            s.max_target_lanes,
            s.max_target_bytes,
            s.max_material_bytes,
            s.max_material_units,
            s.max_auth_storage_bytes,
            s.max_workspace_bytes,
            self.max_record_bytes,
            self.max_record_heap_bytes,
            self.max_total_material_bytes,
            self.max_total_source_reads,
            self.max_record_geometry_visits,
        ]
    }
}

/// Signed Miner inputs and independently rooted registration. No Deserialize,
/// native-admission capability, old-Witness acceptance, or reusable progress.
pub struct FullRelationPreparation {
    plan: ExpandedPlan,
    class: PalwTirClassV1,
    artifact: Hash64,
    job: PalwJobContextV2,
    result: MinerResultCommit,
    form: PalwPromptIdsFormV1,
    limits: FullRelationLimits,
    domains: PrototypeDomains,
    vocab: u64,
}
impl FullRelationPreparation {
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
        let plan = ExpandedPlan::derive(class, artifact, job, limits.plan).map_err(FullRelationError::Plan)?;
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
        let output = &space.program.blocks[space.program.schedule.post as usize].nodes[space.program.logits as usize].out;
        // Adopted RFC-0043 i64 Q24 is a separate profile; its APIs are not
        // integrated here. Never narrow, rewrite or accept it via this branch.
        if output.dtype != DType::I32 {
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
    pub fn plan(&self) -> &ExpandedPlan {
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
        verify_tir_binding_v1(binding, self.limits.plan.max_legacy_leaves).map_err(|_| FullRelationError::Binding)?;
        Ok(())
    }
    pub fn begin(
        self,
        record: PrototypePanelRecord,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
    ) -> Result<FullRelationChecker, FullRelationError> {
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
        Ok(FullRelationChecker { prepared: self, record, progress, best: None, next_lane: 0 })
    }
}

/// Immutable local manifest, not a shared DTO or old execution-record root.
/// No Deserialize, mutable claims, wire decoder, or native receipt conversion.
pub struct PrototypePanelRecord {
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    author: PrincipalId,
    claims: Vec<Vec<i128>>,
    policy: Vec<u64>,
    result: misaka_palw_positive_contract::identity::ResultId,
    form: PalwPromptIdsFormV1,
    domains: PrototypeDomains,
    commitment: Digest,
    heap: u64,
}
impl PrototypePanelRecord {
    pub fn signing_digest(&self) -> Digest {
        self.commitment
    }
    pub fn signature_context(&self) -> &[u8] {
        &self.domains.signature_context
    }
    pub fn retained_record_heap_bytes(&self) -> u64 {
        self.heap
    }
    fn hash(&self, cap: u64) -> Result<Digest, FullRelationError> {
        let mut writer =
            record_hash_prefix(self.result, self.author, &self.binding, &self.generated, &self.policy, self.form, &self.domains, cap)?;
        self.claims.serialize(&mut writer).map_err(|_| FullRelationError::Budget)?;
        Ok(Digest(writer.state.finalize().as_bytes().try_into().expect("64-byte hash")))
    }
}

/// Shared exact existing structural prefix. Splitting the tuple before its
/// final claims field preserves every old Borsh byte and hash/domain recipe.
#[allow(clippy::too_many_arguments)]
fn record_hash_prefix(
    result: misaka_palw_positive_contract::identity::ResultId,
    author: PrincipalId,
    binding: &PalwTirStepBindingV1,
    generated: &[u32],
    policy: &[u64],
    form: PalwPromptIdsFormV1,
    domains: &PrototypeDomains,
    cap: u64,
) -> Result<HashWriter, FullRelationError> {
    let mut writer =
        HashWriter { state: blake2b_simd::Params::new().hash_length(64).key(&domains.hash_key).to_state(), bytes: 0, cap };
    (result, author, binding, generated, policy, form, &domains.signature_context)
        .serialize(&mut writer)
        .map_err(|_| FullRelationError::Budget)?;
    Ok(writer)
}
pub struct LegacyTarget<'a> {
    pub opening: &'a PalwStepOpeningV1,
    pub preimage: &'a PalwStepTileLeafV1,
    pub trace: Option<&'a PalwTirTraceEventDisclosureV1>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullRelationProgress {
    pub next_relation: u64,
    pub legacy_frontier: u64,
    pub checked_rows: u32,
    /// Conservative work reservations are spent even on refused delivery.
    pub charged_material_bytes: u64,
    pub charged_elements: u64,
    pub charged_terms: u64,
    pub charged_source_reads: u64,
    pub charged_geometry_visits: u64,
    pub outcome: CheckOutcome,
}
impl Default for FullRelationProgress {
    fn default() -> Self {
        Self {
            next_relation: 0,
            legacy_frontier: 0,
            checked_rows: 0,
            charged_material_bytes: 0,
            charged_elements: 0,
            charged_terms: 0,
            charged_source_reads: 0,
            charged_geometry_visits: 0,
            outcome: CheckOutcome::Checking,
        }
    }
}

pub struct FullRelationChecker {
    prepared: FullRelationPreparation,
    record: PrototypePanelRecord,
    progress: FullRelationProgress,
    best: Option<(i32, u32)>,
    next_lane: u64,
}
impl FullRelationChecker {
    /// Borrow retained preparation for material construction. This grants no
    /// mutable progress access or additional native authority.
    pub fn preparation(&self) -> &FullRelationPreparation {
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
        material: &TirSourceOnlyMaterialV1<'_>,
        legacy: Option<LegacyTarget<'_>>,
    ) -> Result<(), FullRelationError> {
        if self.progress.outcome != CheckOutcome::Checking {
            return Err(FullRelationError::Closed);
        }
        if self.progress.next_relation == self.prepared.plan.summary().relations {
            return Err(FullRelationError::Incomplete);
        }
        let result = self.submit_inner(material, legacy);
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
        material: &TirSourceOnlyMaterialV1<'_>,
        legacy: Option<LegacyTarget<'_>>,
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
            && !matches!(pin,PalwDecodeTokenPinV1::TiledV1(p) if p.generated_token_ids==self.record.generated)
        {
            return Err(FullRelationError::Delivery);
        }
        let source =
            authenticate_source_only_v1(&self.record.binding, *material, self.progress.legacy_frontier, &rules, limits.source)
                .map_err(source_error)?;
        let evaluation = match &relation.kind {
            ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
                if legacy.is_some() {
                    return Err(FullRelationError::Delivery);
                }
                source.evaluate_node(*ctx, *node, *first_lane, *lane_count).map_err(source_error)?
            }
            ExpandedRelationKind::Legacy(leaf) => {
                let target = legacy.as_ref().ok_or(FullRelationError::Delivery)?;
                self.authenticate_legacy(leaf, target, limits.source.max_material_bytes)?;
                source.evaluate_legacy(leaf).map_err(source_error)?
            }
        };
        let values = evaluation.values();
        if values != self.record.claims[self.progress.next_relation as usize] {
            return Err(FullRelationError::PanelMismatch);
        }
        // No coverage/selection mutation until membership, direct arithmetic,
        // canonical closure and every required legacy/logits check succeeded.
        if let ExpandedRelationKind::Legacy(leaf) = &relation.kind {
            let target = legacy.as_ref().ok_or(FullRelationError::Delivery)?;
            for (value, bytes) in values.iter().zip(target.preimage.values_le.chunks_exact(4)) {
                let value_in_tree = if leaf.dtype == DType::Idx {
                    u32::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?) as i128
                } else {
                    i32::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?) as i128
                };
                if *value != value_in_tree {
                    return Err(FullRelationError::PanelMismatch);
                }
            }
            self.check_selection(leaf, target, &rules)?;
            self.progress.legacy_frontier += 1;
        }
        self.progress.next_relation += 1;
        Ok(())
    }
    fn authenticate_legacy(
        &self,
        leaf: &crate::palw_tir_step_v1::PalwTirLeafV1,
        target: &LegacyTarget<'_>,
        cap: u64,
    ) -> Result<(), FullRelationError> {
        let mut counter = ByteCounter { bytes: 0, cap };
        (target.opening, target.preimage, target.trace).serialize(&mut counter).map_err(|_| FullRelationError::Budget)?;
        let binding = &self.record.binding;
        if target.opening.leaf_index != leaf.index
            || target.preimage.version != PALW_TIR_STEP_LEAF_VERSION_V1
            || target.preimage.coord != leaf.coord
            || target.preimage.value_count != leaf.value_count
            || target.preimage.values_le.len() as u64 != u64::from(leaf.value_count) * 4
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
        target: &LegacyTarget<'_>,
        rules: &PalwTirCourtRulesV1,
    ) -> Result<(), FullRelationError> {
        let space = self.prepared.plan.step_space();
        let PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else {
            return if target.trace.is_none() { Ok(()) } else { Err(FullRelationError::Delivery) };
        };
        if occurrence as usize != space.occurrences().len() - 1 || node != space.program.logits {
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
        let Some(PalwTirTraceEventDisclosureV1::Tiled {
            binding,
            generated_token_ids,
            row_root,
            row_opening,
            tile_lanes,
            tile_opening,
        }) = target.trace
        else {
            return Err(FullRelationError::Delivery);
        };
        if binding.as_ref() != &self.record.binding || generated_token_ids != &self.record.generated {
            return Err(FullRelationError::Delivery);
        }
        let consistency = PalwTirLogitsConsistencyV1 {
            binding: self.record.binding.clone(),
            step_opening: target.opening.clone(),
            step_preimage: target.preimage.clone(),
            trace: PalwTirTraceLanesV1::Tiled {
                generated_token_ids: generated_token_ids.clone(),
                row_root: *row_root,
                row_opening: row_opening.clone(),
                tile_lanes: tile_lanes.clone(),
                tile_opening: tile_opening.clone(),
            },
        };
        if !matches!(check_tir_logits_consistency_v1(&consistency, rules), Err(PalwStepRefuteError::NoFaultFound)) {
            return Err(FullRelationError::Delivery);
        }
        let mut best = self.best;
        let mut next = self.next_lane;
        for bytes in target.preimage.values_le.chunks_exact(4) {
            let value = i32::from_le_bytes(bytes.try_into().map_err(|_| FullRelationError::Delivery)?);
            if best.is_none_or(|(b, _)| value > b) {
                best = Some((value, next as u32));
            }
            next += 1;
        }
        if next > self.prepared.vocab {
            return Err(FullRelationError::Unverified);
        }
        if next == self.prepared.vocab {
            if best.map(|(_, token)| token) != self.record.generated.get(row as usize).copied() {
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
            if digest(palw_attempt_output_root_v1(&self.prepared.job, &self.record.generated)) == self.prepared.result.output {
                CheckOutcome::Matched
            } else {
                CheckOutcome::OutputMismatch
            };
        Ok(self.progress.outcome)
    }
}

fn source_error(error: TirSourceOnlyErrorV1) -> FullRelationError {
    match error {
        TirSourceOnlyErrorV1::Budget => FullRelationError::Budget,
        TirSourceOnlyErrorV1::Evaluation(misaka_palw_tir::demand::DemandError::WorkLimit(_)) => FullRelationError::Budget,
        TirSourceOnlyErrorV1::Evaluation(misaka_palw_tir::demand::DemandError::Tir(error))
            if error.kind == misaka_palw_tir::TirErrorKind::Missing =>
        {
            FullRelationError::Delivery
        }
        TirSourceOnlyErrorV1::Evaluation(_) => FullRelationError::Unverified,
        _ => FullRelationError::Delivery,
    }
}
fn digest(value: Hash64) -> Digest {
    Digest(value.as_bytes())
}
fn mul(a: u64, b: u64) -> Result<u64, FullRelationError> {
    a.checked_mul(b).ok_or(FullRelationError::Budget)
}
fn cap_mul(a: u64, b: u64, cap: u64) -> Result<u64, FullRelationError> {
    mul(a, b).and_then(|v| if v <= cap { Ok(v) } else { Err(FullRelationError::Budget) })
}
fn reserve(old: u64, unit: u64, cap: u64) -> Result<u64, FullRelationError> {
    old.checked_add(unit).filter(|n| *n <= cap).ok_or(FullRelationError::Budget)
}
fn record_heap(binding: &PalwTirStepBindingV1, generated: &Vec<u32>, claims: &Vec<Vec<i128>>) -> Result<u64, FullRelationError> {
    let mut heap = size_of::<PrototypePanelRecord>() as u64;
    for (count, width) in [
        (binding.class.program.capacity(), 1),
        (binding.class.layout.commit_tiles.capacity(), 4),
        (binding.class.layout.state_tiles.capacity(), 4),
        (binding.job_context.network_id.capacity(), 1),
        (generated.capacity(), 4),
        (claims.capacity(), size_of::<Vec<i128>>()),
    ] {
        heap = reserve(heap, mul(count as u64, width as u64)?, u64::MAX)?;
    }
    for values in claims {
        heap = reserve(heap, mul(values.capacity() as u64, 16)?, u64::MAX)?;
    }
    // Local policy and two domains have fixed, pre-bounded allocation caps.
    reserve(heap, 24 * 8 + 64 + 255, u64::MAX)
}
struct ByteCounter {
    bytes: u64,
    cap: u64,
}
impl Write for ByteCounter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buf.len() as u64)
            .filter(|n| *n <= self.cap)
            .ok_or_else(|| io::Error::other("prototype byte cap"))?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct HashWriter {
    state: blake2b_simd::State,
    bytes: u64,
    cap: u64,
}
impl Write for HashWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buf.len() as u64)
            .filter(|n| *n <= self.cap)
            .ok_or_else(|| io::Error::other("prototype record cap"))?;
        self.state.update(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Explicit strict-wide local exact checker; V1 identities and acceptance stay unchanged.
pub mod wide_v2;
