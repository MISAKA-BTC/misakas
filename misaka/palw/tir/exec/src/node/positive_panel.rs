//! A3 Panel construction over A0's v2/r1 contract and existing G14 material.
//! A capture is local construction state. Only A1's independent checker can
//! establish a checked terminal; none of these APIs grants consensus credit.
use super::{
    TirCaptureV1, TirClassRunnerV1,
    backend::tir_authenticate_capture_metadata_v1,
    evidence::TirEvidenceV1,
    inventory::TirParamOpenerV1,
    positive_check::{CheckLimits, CoveragePlan},
};
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step_leg::{step_merkle_root_capped_v1, step_tile_leaf_hash_v1},
    palw_step_refute::PALW_LOGITS_TILE_LANES,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::{PalwTirCourtRulesV1, tir_logits_event_disclosure_v1},
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PALW_TIR_STEP_LEAF_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::{Digest, PUBKEY_BYTES, PrincipalId, SIGNATURE_BYTES},
    messages::{
        AdmissionContext, CheckProfile, ClaimForm, CoverageCursor, CoverageDescriptor, MinerResultCommit, PanelWitnessCommit,
        coverage_plan_id,
    },
    ports::PanelBuilder,
    record::{self, RecordHeader},
    tir,
    wire::{EVIDENCE_HEADER_BYTES, EvidenceFrame, EvidenceHeader},
};
use misaka_palw_tir::demand::DemandLimits;

fn digest(hash: Hash64) -> Digest {
    Digest(hash.as_bytes())
}
fn err<E: std::fmt::Debug>(error: E) -> String {
    format!("A3 Panel: {error:?}")
}

/// Local bounds. `check` meters raw cone+trace work/bytes; frame reservations
/// additionally include BOTH common wrappers at every mandatory relation.
#[derive(Clone, Copy, Debug)]
pub struct PanelLimits {
    pub check: CheckLimits,
    pub max_frame_bytes: u64,
    pub max_total_frame_bytes: u64,
    /// Retained arithmetic capture and owned prompt copies. Rooted program,
    /// coverage/compiled plans and model/interpreter workspace are separate
    /// local admission resources; this is not a complete process RSS bound.
    pub max_capture_bytes: u64,
}

/// Authenticated result tied to independently supplied rooted registration and
/// canonical job/input. No fields can be populated by a decoded success flag.
#[derive(Clone)]
pub struct PanelAdmission {
    result: MinerResultCommit,
    class: PalwTirClassV1,
    job: PalwJobContextV2,
    prompt: Vec<u32>,
    form: PalwPromptIdsFormV1,
    limits: PanelLimits,
    coverage: CoveragePlan,
    plan: CoverageDescriptor,
    reserved_frame_bytes: u64,
}
impl PanelAdmission {
    /// `rooted`, `class`, `job`, and prompt form must be resolved from native
    /// admission, separately from the packet whose signature is checked here.
    #[allow(clippy::too_many_arguments)]
    pub fn authenticate(
        result: &MinerResultCommit,
        rooted: AdmissionContext,
        class: &PalwTirClassV1,
        job: &PalwJobContextV2,
        prompt: &[u32],
        form: PalwPromptIdsFormV1,
        key: &[u8; PUBKEY_BYTES],
        signature: &[u8; SIGNATURE_BYTES],
        mut limits: PanelLimits,
    ) -> Result<Self, String> {
        rooted.check(ClaimForm::PositiveResultV2, result).map_err(err)?;
        result.authenticate(key, signature).map_err(err)?;
        if (prompt.len() as u64).checked_mul(4).filter(|bytes| *bytes <= limits.max_capture_bytes).is_none() {
            return Err("A3 Panel admitted prompt exceeds capture reservation".into());
        }
        // The initial profile has no inherited-prefix context. These scalar
        // boundary checks precede replay; A1/G14 independently validate every
        // admitted binding under the same registered TIR rules.
        if job.version != kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2
            || job.network_id.is_empty()
            || job.network_id.len() > kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES
            || job.exact_decode_tokens == 0
            || job
                .declared_prefill_tokens
                .checked_add(job.exact_decode_tokens)
                .filter(|total| *total <= job.max_context_tokens)
                .is_none()
        {
            return Err("A3 Panel unsupported or over-budget canonical job context".into());
        }
        if result.job != digest(job.job_id)
            || result.execution != digest(job.context_hash())
            || result.registered_class != digest(class.class_id(&Hash64::from_bytes(rooted.artifact.0)))
            || result.program != digest(class.graph_ir_root())
            || result.input != digest(job.prompt_token_ids_hash)
            || result.tokenizer != digest(job.tokenizer_id)
            || result.execution_rules != digest(job.cu_ruleset_id)
            || result.generation_seed != job.execution_seed
            || result.temperature_q24 != 0
            || result.selector != tir::greedy_selector_id()
            || result.prefill != job.declared_prefill_tokens
            || result.decode != job.exact_decode_tokens
            || result.max_context != job.max_context_tokens
            || prompt.len() as u64 != u64::from(job.declared_prefill_tokens)
            || !prompt_token_ids_match_v1(form, prompt, &job.prompt_token_ids_hash)
        {
            return Err("A3 Panel result differs from rooted TIR registration/job/input".into());
        }
        // Class metadata (including its tokenizer) is pinned by class_id above.
        // Job/Result keep the actual expected-context tokenizer, including zero.
        limits.check.max_part_bytes = tir::raw_part_budget(limits.max_frame_bytes).map_err(err)?;
        let artifact = Hash64::from_bytes(rooted.artifact.0);
        let coverage = CoveragePlan::derive(class, artifact, job, form, limits.check).map_err(err)?;
        let program = class.decode_program().map_err(err)?;
        if prompt.iter().any(|id| *id >= program.token_bound) {
            return Err("A3 Panel prompt token is outside registered program".into());
        }
        let leaves = coverage.leaf_count();
        let reserved_frame_bytes = leaves
            .checked_mul(limits.max_frame_bytes)
            .filter(|bytes| *bytes <= limits.max_total_frame_bytes)
            .ok_or("A3 Panel full-frame reservation exceeded")?;
        let plan = CoverageDescriptor {
            scope: result.scope,
            result: result.id(),
            plan: coverage_plan_id(result, CheckProfile::ExactStreamV1, tir::exact_derivation_id(), leaves).map_err(err)?,
            profile: CheckProfile::ExactStreamV1,
            mandatory_relations: leaves,
            reserved_elements: coverage.cost().evaluation_elements,
            reserved_terms: coverage.cost().evaluation_terms,
            max_part_bytes: limits.max_frame_bytes,
        };
        if limits.max_capture_bytes == 0 {
            return Err("A3 Panel capture reservation is empty".into());
        }
        Ok(Self {
            result: result.clone(),
            class: class.clone(),
            job: job.clone(),
            prompt: prompt.to_vec(),
            form,
            limits,
            coverage,
            plan,
            reserved_frame_bytes,
        })
    }
    pub fn result(&self) -> &MinerResultCommit {
        &self.result
    }
    pub fn plan(&self) -> CoverageDescriptor {
        self.plan
    }
    pub fn reserved_frame_bytes(&self) -> u64 {
        self.reserved_frame_bytes
    }
    fn rules(&self) -> PalwTirCourtRulesV1 {
        PalwTirCourtRulesV1 {
            max_step_leaf_count: self.limits.check.max_leaves,
            prompt_form: self.form,
            limits: DemandLimits {
                max_elements: self.limits.check.max_elements_per_leaf,
                max_terms: self.limits.check.max_terms_per_leaf,
            },
        }
    }
}

/// Private dense replay, reusable across signature/transport retries. The Miner
/// result remains separate and may disagree with this candidate's output.
pub struct PanelCapture {
    admission: PanelAdmission,
    capture: TirCaptureV1,
}
impl PanelCapture {
    pub fn binding(&self) -> &PalwTirStepBindingV1 {
        &self.capture.binding
    }
    pub fn generated(&self) -> &[u32] {
        &self.capture.generated
    }
    pub fn admission(&self) -> &PanelAdmission {
        &self.admission
    }
    /// Encode only the retained roots and unchanged generated IDs using A0's
    /// primitive record codec. This carries data identity for independent Node
    /// checking; it establishes neither arithmetic correctness nor credit.
    /// Dense local persistence remains the separate `encode_recorded` format.
    pub fn encode_execution_record(&self, max_record_bytes: u64) -> Result<Vec<u8>, ContractError> {
        let binding = &self.capture.binding;
        record::encode_record(
            RecordHeader {
                version: CONTRACT_VERSION,
                revision: CONTRACT_REVISION,
                result: self.admission.result.id(),
                artifact: digest(binding.artifact_root),
                full_logits_trace_root: digest(binding.full_logits_trace_root),
                step_leaf_count: binding.step_leaf_count,
                step_merkle_root: digest(binding.step_merkle_root),
                committed_execution_root: digest(binding.committed_execution_root),
            },
            &self.capture.generated,
            self.admission.result.decode,
            max_record_bytes,
        )
    }

    pub fn encode_recorded(&self, max_bytes: u64) -> Result<Vec<u8>, String> {
        let length = borsh::object_length(&self.capture).map_err(err)?.checked_add(8).ok_or("A3 capture length overflow")?;
        if length as u64 > max_bytes.min(self.admission.limits.max_capture_bytes) {
            return Err("A3 capture encoded bytes exceed reservation".into());
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).map_err(err)?;
        bytes.extend_from_slice(&TirCaptureV1::MAGIC);
        borsh::to_writer(&mut bytes, &self.capture).map_err(err)?;
        Ok(bytes)
    }
    fn commit(&self, plan: &CoverageDescriptor, author: PrincipalId, attempt: u32) -> Result<PanelWitnessCommit, String> {
        if plan != &self.admission.plan {
            return Err("A3 Panel cannot shorten or replace the independently derived plan".into());
        }
        Ok(PanelWitnessCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: self.admission.result.scope,
            case: self.admission.result.case_id(),
            result: self.admission.result.id(),
            author,
            attempt,
            profile: CheckProfile::ExactStreamV1,
            plan: plan.plan,
            execution_record: digest(self.capture.binding.committed_execution_root),
            provenance: tir::provenance_id(&self.admission.result),
            candidate_output: digest(palw_attempt_output_root_v1(&self.admission.job, &self.capture.generated)),
        })
    }
}

/// Registered TIR arithmetic and parameter opener. A fresh ordinary runner is
/// constructed for replay, so custom selection/drill options cannot be inherited.
/// The model source belongs to the local role adapter; G14 authenticates every
/// parameter opening independently when the Node checks the emitted parts.
pub struct TirPanelBuilder<'a> {
    runner: &'a TirClassRunnerV1<'a>,
    opener: &'a dyn TirParamOpenerV1,
}
impl<'a> TirPanelBuilder<'a> {
    pub fn new(runner: &'a TirClassRunnerV1<'a>, opener: &'a dyn TirParamOpenerV1) -> Self {
        Self { runner, opener }
    }
    fn check_runner(&self, admitted: &PanelAdmission) -> Result<(), String> {
        if self.runner.class_id != admitted.job.shape_profile_id
            || self.runner.plan.program.encode() != admitted.class.program
            || self.runner.space.program != self.runner.plan.program
            || self.runner.space.layout != admitted.class.layout
        {
            return Err("A3 Panel runner differs from rooted class/layout".into());
        }
        Ok(())
    }
    fn capture_reservation(&self, admitted: &PanelAdmission) -> Result<(), String> {
        self.check_runner(admitted)?;
        let shape = self.runner.space.job_shape(&admitted.job).map_err(err)?;
        let lanes = self.runner.space.maximum_leaf_lanes_v1(&shape) as u128;
        let vocab = self.runner.plan.program.blocks[self.runner.plan.program.schedule.post as usize].nodes
            [self.runner.plan.program.logits as usize]
            .out
            .elements_at(1) as u128;
        let bytes = (admitted.plan.mandatory_relations as u128) * (lanes * 4 + 512)
            + (admitted.job.exact_decode_tokens as u128) * (vocab * 4 + 64)
            // PanelCapture owns a prompt in its admission and in the existing
            // capture DTO; count both retained copies.
            + (admitted.prompt.len() as u128) * 8;
        if bytes > admitted.limits.max_capture_bytes as u128 {
            return Err("A3 Panel dense capture reservation exceeded".into());
        }
        Ok(())
    }
    /// The expected witness comes from authenticated role/case admission, never
    /// from the snapshot itself. Recheck all retained data identity without model
    /// replay. This produces construction state, never a trusted checker cursor.
    pub fn restore_recorded(
        &self,
        bytes: &[u8],
        admitted: &PanelAdmission,
        expected: &PanelWitnessCommit,
    ) -> Result<PanelCapture, String> {
        self.capture_reservation(admitted)?;
        let count = admitted.plan.mandatory_relations;
        let reservation = (bytes.len() as u128) * 3 + (16u128 << 20) + (count as u128) * 160 + (admitted.prompt.len() as u128) * 4;
        if reservation > admitted.limits.max_capture_bytes as u128 {
            return Err("A3 Panel restore reservation exceeded".into());
        }
        expected.validate_binding(&admitted.result).map_err(err)?;
        let capture = TirCaptureV1::decode(bytes)?;
        if capture.binding.class != admitted.class
            || capture.binding.job_context != admitted.job
            || capture.binding.artifact_root != Hash64::from_bytes(admitted.result.artifact.0)
            || capture.prompt != admitted.prompt
            || capture.leaves.len() as u64 != count
            || capture.binding.step_leaf_count != count
        {
            return Err("A3 Panel restored capture has another class/job/input/coverage".into());
        }
        tir_authenticate_capture_metadata_v1(&capture, self.runner.class_id, admitted.form, admitted.limits.check.max_leaves)?;
        let mut hashes = Vec::new();
        hashes.try_reserve_exact(capture.leaves.len()).map_err(err)?;
        let context = admitted.job.context_hash();
        for (index, preimage) in capture.leaves.iter().enumerate() {
            let leaf = admitted.coverage.leaf(index as u64).ok_or("A3 Panel restored leaf out of range")?;
            if preimage.version != PALW_TIR_STEP_LEAF_VERSION_V1
                || preimage.coord != leaf.coord
                || preimage.value_count != leaf.value_count
                || preimage.values_le.len() as u64 != u64::from(leaf.value_count) * 4
            {
                return Err("A3 Panel restored leaf has noncanonical shape/order".into());
            }
            hashes.push(step_tile_leaf_hash_v1(&context, &self.runner.class_id, preimage));
        }
        if step_merkle_root_capped_v1(&hashes, admitted.limits.check.max_leaves).map_err(err)? != capture.binding.step_merkle_root {
            return Err("A3 Panel restored leaves differ from retained execution root".into());
        }
        let panel = PanelCapture { admission: admitted.clone(), capture };
        if panel.commit(&admitted.plan, expected.author, expected.attempt)? != *expected {
            return Err("A3 Panel snapshot does not preserve authenticated candidate identity".into());
        }
        Ok(panel)
    }
    /// Build the dense Merkle evidence tree once and reuse it across batches.
    pub fn prepare_parts<'b>(&'b self, capture: &'b PanelCapture) -> Result<PreparedPanelParts<'b>, String>
    where
        'a: 'b,
    {
        self.check_runner(&capture.admission)?;
        let c = &capture.capture;
        let store = TirEvidenceV1::dense(
            self.runner,
            &c.binding,
            &c.prompt,
            &c.logits_rows,
            &c.generated,
            &c.leaves,
            self.opener,
            capture.admission.form,
            capture.admission.limits.check.max_leaves,
        )?;
        Ok(PreparedPanelParts { capture, runner: self.runner, store, rules: capture.admission.rules() })
    }
}
impl PanelBuilder for TirPanelBuilder<'_> {
    type AdmittedResult = PanelAdmission;
    type Capture = PanelCapture;
    type G14EvidencePart = PanelEvidencePart;
    type Error = String;
    fn replay_and_capture(&self, admitted: &PanelAdmission) -> Result<PanelCapture, String> {
        self.capture_reservation(admitted)?;
        let ordinary = TirClassRunnerV1::new(self.runner.space, self.runner.plan, self.runner.params, self.runner.class_id)?;
        let mut leaves = Vec::new();
        leaves.try_reserve_exact(usize::try_from(admitted.plan.mandatory_relations).map_err(err)?).map_err(err)?;
        let mut retain = |leaf: &super::run::TirLeafOutV1<'_>| leaves.push(leaf.preimage.clone());
        let run = ordinary.run(&admitted.job, &admitted.prompt, admitted.limits.check.max_leaves, true, &mut retain)?;
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: admitted.job.clone(),
            class: admitted.class.clone(),
            artifact_root: Hash64::from_bytes(admitted.result.artifact.0),
            full_logits_trace_root: run.trace_root,
            step_leaf_count: run.leaf_count,
            step_merkle_root: run.step_merkle_root,
            committed_execution_root: run.execution_root,
        };
        let capture =
            TirCaptureV1 { binding, prompt: admitted.prompt.clone(), logits_rows: run.logits_rows, generated: run.generated, leaves };
        Ok(PanelCapture { admission: admitted.clone(), capture })
    }
    fn commit(
        &self,
        capture: &PanelCapture,
        plan: &CoverageDescriptor,
        author: PrincipalId,
        attempt: u32,
    ) -> Result<PanelWitnessCommit, String> {
        self.check_runner(&capture.admission)?;
        capture.commit(plan, author, attempt)
    }
    /// One-shot convenience. Streaming callers should retain `prepare_parts`
    /// across batches to avoid rebuilding the whole step tree for every part.
    fn build_part(&self, capture: &PanelCapture, cursor: CoverageCursor) -> Result<PanelEvidencePart, String> {
        self.prepare_parts(capture)?.build_part(cursor)
    }
}

/// Panel construction working set; contains no Node success/progress authority.
pub struct PreparedPanelParts<'a> {
    capture: &'a PanelCapture,
    runner: &'a TirClassRunnerV1<'a>,
    store: TirEvidenceV1<'a>,
    rules: PalwTirCourtRulesV1,
}
impl PreparedPanelParts<'_> {
    pub fn build_part(&self, cursor: CoverageCursor) -> Result<PanelEvidencePart, String> {
        let admission = &self.capture.admission;
        if cursor.plan != admission.plan.plan || cursor.next_relation >= admission.plan.mandatory_relations {
            return Err("A3 Panel cursor is outside canonical plan".into());
        }
        let index = cursor.next_relation;
        let cone = self.store.cone_refutation(index, &self.rules).map_err(err)?;
        let leaf = admission.coverage.leaf(index).ok_or("A3 Panel canonical leaf absent")?;
        let trace = match leaf.kind {
            PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                if occurrence as usize == self.runner.space.occurrences().len() - 1 && node == self.runner.plan.program.logits =>
            {
                let row = leaf
                    .position
                    .checked_add(1)
                    .and_then(|n| n.checked_sub(admission.job.declared_prefill_tokens))
                    .ok_or("A3 Panel logits row outside job")?;
                let tile = u8::try_from(first_element / PALW_LOGITS_TILE_LANES as u64).map_err(err)?;
                Some(
                    tir_logits_event_disclosure_v1(
                        &self.capture.capture.binding,
                        &self.capture.capture.logits_rows,
                        &self.capture.capture.generated,
                        row,
                        tile,
                    )
                    .ok_or("A3 Panel trace event unavailable")?,
                )
            }
            _ => None,
        };
        let raw_bytes = borsh::object_length(&cone).map_err(err)? as u64
            + trace.as_ref().map(borsh::object_length).transpose().map_err(err)?.unwrap_or(0) as u64;
        if raw_bytes > admission.limits.check.max_part_bytes {
            return Err("A3 Panel part exceeds raw budget".into());
        }
        let cone = borsh::to_vec(&cone).map_err(err)?;
        let trace = trace.as_ref().map(borsh::to_vec).transpose().map_err(err)?;
        let payload_cap = admission.plan.max_part_bytes.checked_sub(EVIDENCE_HEADER_BYTES as u64).ok_or("A3 frame cap too small")?;
        let payload = tir::ExactPart::new(&cone, trace.as_deref(), payload_cap).map_err(err)?.encode();
        Ok(PanelEvidencePart {
            template: self.capture.commit(&admission.plan, PrincipalId(Digest([0; 64])), 0)?,
            plan: admission.plan,
            index,
            payload,
        })
    }
    pub fn stream_range(
        &self,
        first: u64,
        count: u64,
        consume: &mut dyn FnMut(PanelEvidencePart) -> Result<(), String>,
    ) -> Result<(), String> {
        let plan = self.capture.admission.plan;
        let end = first.checked_add(count).filter(|end| *end <= plan.mandatory_relations).ok_or("A3 Panel range outside plan")?;
        if count == 0 && first != plan.mandatory_relations {
            return Err("A3 Panel empty range before completion".into());
        }
        for next_relation in first..end {
            consume(self.build_part(CoverageCursor { plan: plan.plan, next_relation })?)?;
        }
        Ok(())
    }
}

/// Owned common inner payload. Framing requires a witness matching the exact
/// replay/provenance/output/plan; changing author or attempt requires its proper
/// signature at the outer admission layer, not a rewritten original signature.
pub struct PanelEvidencePart {
    template: PanelWitnessCommit,
    plan: CoverageDescriptor,
    index: u64,
    payload: Vec<u8>,
}
impl PanelEvidencePart {
    pub fn index(&self) -> u64 {
        self.index
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    pub fn frame<'a>(&'a self, witness: &PanelWitnessCommit, plan: &CoverageDescriptor) -> Result<EvidenceFrame<'a>, String> {
        witness.validate_shape().map_err(err)?;
        let mut expected = self.template.clone();
        expected.author = witness.author;
        expected.attempt = witness.attempt;
        if witness != &expected || plan != &self.plan {
            return Err("A3 Panel evidence belongs to another candidate/plan".into());
        }
        EvidenceFrame::new(
            EvidenceHeader {
                case: witness.case,
                result: witness.result,
                witness: witness.id(),
                plan: witness.plan,
                first_relation: self.index,
                relation_count: 1,
                payload_bytes: u32::try_from(self.payload.len()).map_err(err)?,
            },
            &self.payload,
            plan.max_part_bytes,
        )
        .map_err(err)
    }
}
