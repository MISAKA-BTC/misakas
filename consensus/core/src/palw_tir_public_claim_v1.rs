//! Versioned ORIGINAL TIR full-allocation context. This is not an old
//! ClaimBundle and does not turn a quote, signature or deserialized value into
//! paid native standing. The accepting native transition must charge the exact
//! full SUM and install its holds and this row atomically. There is no admission
//! or carrier activation in this module.
//!
//! The original commitment is retained here, including for generic arithmetic
//! without a semantic bundle. All hashes authenticate original material, never
//! correctness of model computation.
use crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3;
use crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_public_allocation_v1::{PalwTirPublicAllocationPlanV1, PalwTirPublicAllocationRulesV1};
use crate::palw_tir_public_custody_v1::{PalwTirPublicCustodyContractV1, PalwTirPublicOriginalCustodyV1};
use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1;
use crate::palw_tir_public_state_v1::PalwTirPublicSourceDescriptorV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeSet;

fn header_digest(header: &PalwPublicTirBindingHeaderV1) -> Result<Hash64, String> {
    let raw = borsh::to_vec(header).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/tir-source-header/v1")
            .hash(&raw)
            .as_bytes()
            .try_into()
            .expect("64"),
    ))
}

/// Rooted original context, not a portable proof of admission. Native callers
/// must use `prepare_native_v1` and still perform full payment/hold admission.
/// Snapshot decoding is structural only; it cannot manufacture native grants.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicClaimV1 {
    original: PalwFreePromptCommitmentV3,
    header: PalwPublicTirBindingHeaderV1,
    plan: PalwTirPublicAllocationPlanV1,
    descriptor: PalwTirPublicSourceDescriptorV1,
    custody: PalwTirPublicOriginalCustodyV1,
}

impl PalwTirPublicClaimV1 {
    /// Construct a native-validated CANDIDATE at the original accepting block
    /// clock. `geometry`, rules, record/layout, Panel and original object must
    /// come from the native accepting context, not an incoming wire assertion.
    /// `verify` authenticates the original producer pledge only. The original
    /// object's ordinary signature/key/credit checks and the exact full-SUM
    /// payment, capital and holds remain the accepting transition's duties.
    /// No pre-acceptance pending grants are imported here.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_native_v1<F>(
        original: PalwFreePromptCommitmentV3,
        header: PalwPublicTirBindingHeaderV1,
        plan: PalwTirPublicAllocationPlanV1,
        custody: PalwTirPublicCustodyContractV1,
        geometry: &crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1,
        record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
        inventory: Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
        rules: PalwTirPublicAllocationRulesV1,
        native_panel: &BTreeSet<PalwBondKeyV2>,
        original_object: &crate::palw_state_v2::PalwConsensusObjectV2,
        verify: F,
    ) -> Result<Self, String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        crate::palw_tir_public_allocation_v1::check_tir_public_allocation_native_v1(
            &plan, geometry, &original, &header, record, layout, semantics, inventory, rules,
        )?;
        let source = PalwTirPublicSourceV1::new_v1(
            record,
            layout,
            &header,
            original.execution_root,
            original.trace_root,
            rules.maximum_leaves,
        )?;
        if custody.drawn_panel_v1() != native_panel || custody.liable_signers_v1() != std::iter::once(geometry.producer).collect() {
            return Err("original TIR frame imports a foreign Panel or a not-yet-credited custody grant".into());
        }
        custody.pledge_v1().authenticate_original_object_v1(
            original_object,
            &source,
            plan.resource_plan_id,
            rules.maximum_custodians,
            verify,
        )?;
        let descriptor = PalwTirPublicSourceDescriptorV1 {
            class_id: record.facts.class_id,
            layout_digest: record.layout_digest,
            header_digest: header_digest(&header)?,
            source_id: source.source_id_v1(),
            maximum_leaves: rules.maximum_leaves,
            native_config: rules.config,
            native_custodian_upper: rules.maximum_custodians,
            work_rules: Some(plan.owner_work.rules),
        };
        let frame = Self { original, header, plan, descriptor, custody: PalwTirPublicOriginalCustodyV1::new_v1(custody)? };
        frame.check_structure_v1(&frame.key_v1())?;
        Ok(frame)
    }

    pub fn key_v1(&self) -> Key {
        Key::TirClaim(self.plan.original_claim)
    }
    pub fn preparation_key_v1(&self) -> Key {
        Key::TirPreparation(self.plan.original_claim, self.plan.allocation_id)
    }
    pub fn original_v1(&self) -> &PalwFreePromptCommitmentV3 {
        &self.original
    }
    pub fn header_v1(&self) -> &PalwPublicTirBindingHeaderV1 {
        &self.header
    }
    pub fn plan_v1(&self) -> &PalwTirPublicAllocationPlanV1 {
        &self.plan
    }
    pub fn descriptor_v1(&self) -> &PalwTirPublicSourceDescriptorV1 {
        &self.descriptor
    }
    pub fn custody_v1(&self) -> &PalwTirPublicCustodyContractV1 {
        self.custody.contract_v1()
    }
    pub fn original_custody_v1(&self) -> &PalwTirPublicOriginalCustodyV1 {
        &self.custody
    }
    /// **G14 R4: what one public conviction filing burns** — the frame's own native terminal envelope rent (the price of one
    /// bounded adjudication), at least one sompi so the filing is always a declared, burned payment. Derived from the frozen
    /// original plan, never from the filer's carrier; a refused filing burns it all the same (no free adjudication CPU).
    pub fn conviction_rent_v1(&self) -> u64 {
        self.plan.terminal.envelope.rent_sompi.max(1)
    }

    /// Structural consistency is deliberately weaker than native admission.
    /// In particular neither this check nor an allocation checksum establishes
    /// payment, native price selection, receipt credit or signature validity.
    pub fn check_structure_v1(&self, key: &Key) -> Result<(), String> {
        self.plan.check_v1()?;
        self.custody.check_v1()?;
        let p = self.custody_v1().pledge_v1();
        let d = &self.descriptor;
        let ctx = &self.header.job_context;
        if *key != self.key_v1()
            || crate::palw_freeprompt_v3::fp_claim_id_v3(&self.original) != self.plan.original_claim
            || p.claim != self.plan.original_claim
            || p.producer != PalwBondKeyV2(self.original.job.executor_bond)
            || p.execution_root != self.original.execution_root
            || p.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&self.original)
            || p.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(ctx)
            || p.source_id != self.plan.source_id
            || p.resource_plan_id != self.plan.resource_plan_id
            || p.maximum_custodians != self.plan.acquisition.maximum_custodians
            || self.custody_v1().terms_v1() != self.plan.terms
            || d.class_id != self.original.job.class_id
            || d.class_id != ctx.shape_profile_id
            || d.header_digest != header_digest(&self.header)?
            || d.source_id != self.plan.source_id
            || d.native_config != self.plan.acquisition.config
            || d.native_custodian_upper != p.maximum_custodians
            || d.work_rules != Some(self.plan.owner_work.rules)
            || d.maximum_leaves == 0
            || d.maximum_leaves != self.plan.owner_work.rules.max_step_leaf_count
            || self.header.committed_execution_root != self.original.execution_root
            || self.header.full_logits_trace_root != self.original.trace_root
            || self.header.step_leaf_count != self.original.work_leaves
            || ctx.exact_decode_tokens != self.original.decode_tokens_executed
            || ctx.tokenizer_id != self.original.job.tokenizer_id
            || ctx.prompt_token_ids_hash != self.original.job.prompt_token_ids_hash
            || ctx.declared_prefill_tokens != self.original.job.prompt_tokens
        {
            return Err("original TIR frame changes claim, source, complete plan, native geometry or original clocks".into());
        }
        for (_, credit) in self.custody.credits_v1() {
            let ready = &credit.ready;
            let descriptor = &ready.descriptor;
            if ready.accepted_daa < self.plan.terms.accepted_daa
                || ready.accepted_daa >= self.plan.preparation_until_daa
                || descriptor.retained_bytes > self.plan.preparation.maximum_prepared_bytes
                || descriptor.class_id != d.class_id
                || descriptor.artifact_root != self.header.artifact_root
                || descriptor.execution_root != self.original.execution_root
                || descriptor.trace_root != self.original.trace_root
                || descriptor.step_root != self.header.step_merkle_root
                || descriptor.step_leaf_count != self.header.step_leaf_count
                || descriptor.decode_count != ctx.exact_decode_tokens
                || descriptor.vocab != self.plan.acquisition.envelope.vocab
            {
                return Err("original TIR credit changes original Ready source, bound or preparation clock".into());
            }
        }
        if borsh::to_vec(self).map_err(|e| e.to_string())?.len() as u64 > self.plan.control_footprint.maximum_frame_bytes {
            return Err("original TIR frame exceeds its native prepaid mutable frame bound".into());
        }
        Ok(())
    }

    /// Source identity reconstruction from an already authenticated native row.
    /// `maximum_leaves` is the frozen native rule, never a carrier ceiling.
    pub fn source_v1(
        &self,
        record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        maximum_leaves: u64,
    ) -> Result<PalwTirPublicSourceV1, String> {
        self.check_structure_v1(&self.key_v1())?;
        if maximum_leaves != self.descriptor.maximum_leaves
            || record.facts.class_id != self.descriptor.class_id
            || record.layout_digest != self.descriptor.layout_digest
        {
            return Err("original TIR frame substitutes native class, layout or leaf ceiling".into());
        }
        let source = PalwTirPublicSourceV1::new_v1(
            record,
            layout,
            &self.header,
            self.original.execution_root,
            self.original.trace_root,
            maximum_leaves,
        )?;
        if source.source_id_v1() != self.plan.source_id {
            return Err("original TIR frame substitutes source identity".into());
        }
        Ok(source)
    }

    pub(crate) fn bind_original_panel_native_v1(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        now: u64,
    ) -> Result<(), String> {
        self.check_structure_v1(&self.key_v1())?;
        self.custody.bind_original_panel_native_v1(state, now)
    }
    /// The native fold must atomically commit the ordinary credit, S4, Ready,
    /// this appended exact custody record and their original-horizon holds.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn qualify_original_native_v1(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        source: &PalwTirPublicSourceV1,
        preparation: &crate::palw_tir_public_owned_v1::PalwTirPublicPreparationV1,
        receipt: &crate::palw_panel_v2::PalwSeatReceiptV2,
        grant: &crate::palw_tir_public_custody_v1::PalwTirPublicCustodyGrantV1,
        now: u64,
        native_network_domain: Hash64,
    ) -> Result<(), String> {
        self.check_structure_v1(&self.key_v1())?;
        self.custody.qualify_original_native_v1(state, source, preparation, receipt, grant, now, native_network_domain)
    }
}

/// **G14 R4: what one public bonded verifier reads to file against an ORIGINAL frame** — from the accepted state only (never
/// the producer's capture or a Panel's word). `liable` is the literal set the fold refuses as filers and charges on a verdict:
/// the producer and every credited exact Valid custodian; a drawn seat outside it files exactly as an outside bond does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirPublicFrameViewV1 {
    pub claim: Hash64,
    pub allocation_id: Hash64,
    pub source_id: Hash64,
    pub producer: PalwBondKeyV2,
    /// The producer's signed source pledge id a Valid seat's raw custody grant binds.
    pub pledge_id: Hash64,
    pub liable: BTreeSet<PalwBondKeyV2>,
    pub drawn_panel: BTreeSet<PalwBondKeyV2>,
    pub accepted_daa: u64,
    /// The d0 conviction door closes here (exclusive); the claim's `Final` floor.
    pub collect_until_daa: u64,
    pub material_until_daa: u64,
    /// Exact rent one filing declares and burns ([`PalwTirPublicClaimV1::conviction_rent_v1`]).
    pub conviction_rent_sompi: u64,
    /// The prepaid terminal witness ceiling a carried primitive must fit.
    pub maximum_witness_bytes: u64,
    /// The original selection rule is plain greedy (the only one the frame's selection door judges).
    pub greedy_selection: bool,
    pub header: PalwPublicTirBindingHeaderV1,
}
impl PalwTirPublicClaimV1 {
    pub fn view_v1(&self) -> PalwTirPublicFrameViewV1 {
        let terms = self.plan.terms;
        PalwTirPublicFrameViewV1 {
            claim: self.plan.original_claim,
            allocation_id: self.plan.allocation_id,
            source_id: self.plan.source_id,
            producer: self.custody_v1().pledge_v1().producer,
            pledge_id: self.custody_v1().pledge_v1().id_v1(),
            liable: self.custody_v1().liable_signers_v1(),
            drawn_panel: self.custody_v1().drawn_panel_v1().clone(),
            accepted_daa: terms.accepted_daa,
            collect_until_daa: terms.collect_until_daa,
            material_until_daa: terms.material_until_daa,
            conviction_rent_sompi: self.conviction_rent_v1(),
            maximum_witness_bytes: self.plan.terminal.maximum_witness_bytes,
            greedy_selection: self.original.job.decode.is_none() && self.original.job.tail.is_none(),
            header: self.header.clone(),
        }
    }
}
/// The accepted-state reader behind the consensus API's `palw_tir_public_frame_v1`.
pub fn palw_tir_public_frame_view_v1(state: &crate::palw_state_v2::PalwChainStateV2, claim: Hash64) -> Option<PalwTirPublicFrameViewV1> {
    palw_tir_public_claim_v1(state, claim).map(PalwTirPublicClaimV1::view_v1)
}

/// No fallback to ClaimBundle: callers must distinguish the new original
/// full-allocation protocol from the old context-only dormant path.
pub fn palw_tir_public_claim_v1(state: &crate::palw_state_v2::PalwChainStateV2, claim: Hash64) -> Option<&PalwTirPublicClaimV1> {
    match state.staged_court_row_v1(&Key::TirClaim(claim)) {
        Some(Row::TirClaim(frame)) => Some(frame),
        _ => None,
    }
}

/// Synthetic signature injection for source-only lifecycle/index fixtures.
/// This is a structurally/native-geometry checked candidate, NOT paid standing.
#[cfg(test)]
pub(crate) fn palw_tir_public_claim_fixture_v1() -> PalwTirPublicClaimV1 {
    tests::fixture_frame()
}

#[cfg(test)]
impl PalwTirPublicClaimV1 {
    /// Test-only: replace the original custody with one carrying a frozen Panel and exact credited Valid custody whose
    /// signatures are SYNTHETIC. The frame's structural check still runs; native receipt/grant/Ready admission is not exercised.
    pub(crate) fn with_custody_for_tests_v1(mut self, custody: PalwTirPublicOriginalCustodyV1) -> Result<Self, String> {
        self.custody = custody;
        self.check_structure_v1(&self.key_v1())?;
        Ok(self)
    }
    /// Test-only: a producer whose signed ORIGINAL commitment names `output_root` (structure is unchanged: the output root is
    /// not one of the roots the frame's geometry cross-checks — the output court is what judges it).
    pub(crate) fn with_original_output_root_for_tests_v1(mut self, output_root: Hash64) -> Self {
        self.original.output_root = output_root;
        self
    }
}

/// **G14 E2E test support: an ORIGINAL frame over the tiny TIR execution, honest or with leaf `forge.0` lane `forge.1` moved by
/// one, at the network's FP job context** — the execution is run IN the FP-derived context, so the step tree, trace and execution
/// roots the frame commits are the ones its own evidence store (`TinyExecution`) opens. `maximum_custodians` sizes the native upper
/// (producer + drawn seats). Synthetic producer signature (injected verifier); no native admission, payment or credit.
#[cfg(test)]
pub(crate) struct PalwTirPublicFrameFixtureV1 {
    pub frame: PalwTirPublicClaimV1,
    pub record: crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    pub layout: crate::palw_tir_class_v1::PalwTirLayoutV1,
    pub execution: crate::palw_tir_court_v1::test_support::TinyExecution,
    pub rules: PalwTirPublicAllocationRulesV1,
    pub object: crate::palw_state_v2::PalwConsensusObjectV2,
}

#[cfg(test)]
pub(crate) fn palw_tir_public_frame_fixture_v1(forge: Option<(usize, usize)>, maximum_custodians: u64) -> PalwTirPublicFrameFixtureV1 {
    tests::fixture_in(forge, maximum_custodians)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        frame: PalwTirPublicClaimV1,
        record: crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: crate::palw_tir_class_v1::PalwTirLayoutV1,
    }
    pub(super) fn fixture_frame() -> PalwTirPublicClaimV1 {
        fixture().frame
    }
    /// [`super::palw_tir_public_frame_fixture_v1`]: the context is derived first (from an honest run's class, prompt and counts,
    /// which a one-lane forge does not change), then the execution is run in it.
    pub(super) fn fixture_in(forge: Option<(usize, usize)>, maximum_custodians: u64) -> super::PalwTirPublicFrameFixtureV1 {
        use crate::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3};
        use crate::palw_freeprompt_v3::*;
        use crate::palw_tir_public_allocation_v1::*;
        let honest = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let class_id = honest.binding.class.class_id(&honest.binding.artifact_root);
        let (_, _, mut job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
        job.version = PALW_FP_V3_VERSION;
        job.decode = None;
        job.tail = None;
        job.class_id = class_id;
        job.tokenizer_id = honest.binding.class.tokenizer_id;
        job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&honest.prompt);
        job.prompt_tokens = honest.prompt.len() as u32;
        job.decode_token_limit = honest.generated.len() as u32;
        job.max_context_tokens = 64;
        let class = PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: class_id,
            cu_ruleset_id: Hash64::default(),
        };
        let facts = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: honest.hashes.len() as u64,
        };
        let context = crate::palw_fp_execution_v3::palw_fp_job_context_v3(&job, &class, &facts, b"testnet-12").unwrap();
        let x = crate::palw_tir_court_v1::test_support::tiny_execution_in(|_, _, _| context.clone(), forge);
        let binding = x.binding.clone();
        let original = PalwFreePromptCommitmentV3 {
            job,
            trace_root: binding.full_logits_trace_root,
            output_root: crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &x.generated),
            schedule_root: Hash64::default(),
            execution_root: binding.committed_execution_root,
            decode_tokens_executed: x.generated.len() as u32,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            work_leaves: binding.step_leaf_count,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 100,
        };
        let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id,
            producer: PalwBondKeyV2(original.job.executor_bond),
            execution_root: original.execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&original),
            accepted_daa: 100,
        };
        let header = PalwPublicTirBindingHeaderV1::of_binding_v1(&binding);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&binding.class, &binding.artifact_root).unwrap();
        let layout = binding.class.layout.clone();
        let rules = PalwTirPublicAllocationRulesV1 {
            maximum_leaves: 1 << 40,
            maximum_custodians,
            court: crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 40, 45, 2).unwrap(),
            dissection: true,
            prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
        };
        let PalwTirPublicAllocationQuoteV1::Complete(plan) =
            derive_tir_public_allocation_v1(&geometry, &original, &header, &record, &layout, None, None, rules).unwrap()
        else {
            panic!("fixture requires a real native generic arithmetic quote")
        };
        let pledge = crate::palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1 {
            version: 1,
            claim: plan.original_claim,
            producer: geometry.producer,
            execution_root: original.execution_root,
            job_identity: geometry.job_identity,
            source_id: plan.source_id,
            resource_plan_id: plan.resource_plan_id,
            maximum_custodians,
            accept_until_daa: 101,
            material_until_daa: plan.terms.material_until_daa,
            signature: vec![2; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let key = vec![1; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
        // Injected verifier: geometry/scope only, never MLDSA, payment, credit or admission coverage.
        let verify = |_: &[u8], _: &[u8], _: &[u8], domain: &[u8]| {
            domain == crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1
        };
        let custody = PalwTirPublicCustodyContractV1::new_v1(
            pledge,
            plan.terms,
            BTreeSet::new(),
            plan.source_id,
            plan.resource_plan_id,
            maximum_custodians,
            &key,
            verify,
        )
        .unwrap();
        let object = crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim: plan.original_claim,
            class_id,
            bond: geometry.producer,
            executor_pubkey: key,
            work_leaves: original.work_leaves,
            prompt_token_ids_hash: original.job.prompt_token_ids_hash,
            prompt_tokens: original.job.prompt_tokens,
            prompt_token_ids: x.prompt.clone(),
            decode_tokens_executed: original.decode_tokens_executed,
            trace_root: original.trace_root,
            output_root: original.output_root,
            execution_root: original.execution_root,
            trace_chunk_count: original.trace_chunk_count,
            trace_retention_daa: original.trace_retention_daa,
            consumed_prefix_state: PalwFpPrefixStateV1::genesis(class_id),
            job_pin: geometry.job_identity,
            public_job: None,
            eval: None,
        };
        let frame = PalwTirPublicClaimV1::prepare_native_v1(
            original,
            header,
            *plan,
            custody,
            &geometry,
            &record,
            &layout,
            None,
            None,
            rules,
            &BTreeSet::new(),
            &object,
            verify,
        )
        .unwrap();
        super::PalwTirPublicFrameFixtureV1 { frame, record, layout, execution: x, rules, object }
    }
    fn fixture() -> Fixture {
        use crate::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3};
        use crate::palw_freeprompt_v3::*;
        use crate::palw_tir_public_allocation_v1::*;
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut binding = x.binding.clone();
        let class_id = binding.class.class_id(&binding.artifact_root);
        let (_, _, mut job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
        job.version = PALW_FP_V3_VERSION;
        job.decode = None;
        job.tail = None;
        job.class_id = class_id;
        job.tokenizer_id = binding.class.tokenizer_id;
        job.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&x.prompt);
        job.prompt_tokens = x.prompt.len() as u32;
        job.decode_token_limit = x.generated.len() as u32;
        job.max_context_tokens = 64;
        let class = PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: class_id,
            cu_ruleset_id: Hash64::default(),
        };
        let facts = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: Hash64::default(),
            step_leaf_count: x.hashes.len() as u64,
        };
        binding.job_context = crate::palw_fp_execution_v3::palw_fp_job_context_v3(&job, &class, &facts, b"testnet-12").unwrap();
        let context = binding.job_context.context_hash();
        let hashes: Vec<_> =
            x.preimages.iter().map(|leaf| crate::palw_step_leg::step_tile_leaf_hash_v1(&context, &class_id, leaf)).collect();
        binding.step_merkle_root = crate::palw_step_leg::step_merkle_root_v1(&hashes).unwrap();
        binding.full_logits_trace_root =
            crate::palw_step_refute::tiled_logits_trace_root_v1(&binding.job_context, &x.rows, &x.generated).unwrap();
        binding.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &context,
            &binding.full_logits_trace_root,
            &class_id,
            binding.step_leaf_count,
            &binding.step_merkle_root,
        );
        let original = PalwFreePromptCommitmentV3 {
            job,
            trace_root: binding.full_logits_trace_root,
            output_root: crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &x.generated),
            schedule_root: Hash64::default(),
            execution_root: binding.committed_execution_root,
            decode_tokens_executed: x.generated.len() as u32,
            stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            work_leaves: binding.step_leaf_count,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 100,
        };
        let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id,
            producer: PalwBondKeyV2(original.job.executor_bond),
            execution_root: original.execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(&original),
            accepted_daa: 100,
        };
        let header = PalwPublicTirBindingHeaderV1::of_binding_v1(&binding);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&binding.class, &binding.artifact_root).unwrap();
        let layout = binding.class.layout;
        let rules = PalwTirPublicAllocationRulesV1 {
            maximum_leaves: 1 << 40,
            maximum_custodians: 1,
            court: crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 40, 45, 2).unwrap(),
            dissection: true,
            prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
        };
        let PalwTirPublicAllocationQuoteV1::Complete(plan) =
            derive_tir_public_allocation_v1(&geometry, &original, &header, &record, &layout, None, None, rules).unwrap()
        else {
            panic!("fixture requires a real native generic arithmetic quote")
        };
        let pledge = crate::palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1 {
            version: 1,
            claim: plan.original_claim,
            producer: geometry.producer,
            execution_root: original.execution_root,
            job_identity: geometry.job_identity,
            source_id: plan.source_id,
            resource_plan_id: plan.resource_plan_id,
            maximum_custodians: 1,
            accept_until_daa: 101,
            material_until_daa: plan.terms.material_until_daa,
            signature: vec![2; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let key = vec![1; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
        // Isolate native geometry/scope in this fixture. An injected signature
        // check is not real MLDSA, native payment, credit or admission coverage.
        let verify = |_: &[u8], _: &[u8], _: &[u8], domain: &[u8]| {
            domain == crate::palw_tir_public_custody_v1::PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1
        };
        let custody = PalwTirPublicCustodyContractV1::new_v1(
            pledge,
            plan.terms,
            BTreeSet::new(),
            plan.source_id,
            plan.resource_plan_id,
            1,
            &key,
            verify,
        )
        .unwrap();
        let object = crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim: plan.original_claim,
            class_id,
            bond: geometry.producer,
            executor_pubkey: key,
            work_leaves: original.work_leaves,
            prompt_token_ids_hash: original.job.prompt_token_ids_hash,
            prompt_tokens: original.job.prompt_tokens,
            prompt_token_ids: x.prompt,
            decode_tokens_executed: original.decode_tokens_executed,
            trace_root: original.trace_root,
            output_root: original.output_root,
            execution_root: original.execution_root,
            trace_chunk_count: original.trace_chunk_count,
            trace_retention_daa: original.trace_retention_daa,
            consumed_prefix_state: PalwFpPrefixStateV1::genesis(class_id),
            job_pin: geometry.job_identity,
            public_job: None,
            eval: None,
        };
        let frame = PalwTirPublicClaimV1::prepare_native_v1(
            original,
            header,
            *plan,
            custody,
            &geometry,
            &record,
            &layout,
            None,
            None,
            rules,
            &BTreeSet::new(),
            &object,
            verify,
        )
        .unwrap();
        Fixture { frame, record, layout }
    }
    #[test]
    fn generic_original_context_does_not_depend_on_old_bundle_or_retired_fp_row() {
        let f = fixture();
        assert!(f.frame.plan_v1().semantic.is_none());
        assert_eq!(f.frame.original_v1().execution_root, f.frame.header_v1().committed_execution_root);
        let source = f.frame.source_v1(&f.record, &f.layout, 1 << 40).unwrap();
        assert_eq!(source.source_id_v1(), f.frame.plan_v1().source_id);
        let decoded: PalwTirPublicClaimV1 = borsh::from_slice(&borsh::to_vec(&f.frame).unwrap()).unwrap();
        assert_eq!(decoded, f.frame);
        decoded.check_structure_v1(&decoded.key_v1()).unwrap();
        // Round-trip only establishes structure, not a paid native transition.
    }
    #[test]
    fn original_frame_rejects_cross_scope_clock_and_plan_substitution() {
        let f = fixture();
        for n in 0..5 {
            let mut bad = f.frame.clone();
            match n {
                0 => bad.original.output_root = Hash64::from_u64_word(991),
                1 => bad.header.job_context.exact_decode_tokens += 1,
                2 => bad.plan.terms.accepted_daa += 1,
                3 => bad.plan.terms.plan.deposit_sompi -= 1,
                _ => bad.descriptor.native_custodian_upper += 1,
            }
            assert!(bad.check_structure_v1(&f.frame.key_v1()).is_err());
        }
        assert!(f.frame.source_v1(&f.record, &f.layout, (1 << 40) - 1).is_err());
    }
    #[test]
    fn new_frame_appends_ordinals_and_never_becomes_an_old_semantic_court() {
        let f = fixture();
        assert_eq!(borsh::to_vec(&f.frame.key_v1()).unwrap()[0], 9);
        let row = Row::TirClaim(Box::new(f.frame.clone()).into());
        assert_eq!(borsh::to_vec(&row).unwrap()[0], 15);
        assert!(row.active_row_v1().is_none());
        assert!(row.owner_accounting_v1().is_none());
        assert!(row.checkpoint_v1().is_err());
        let h = Hash64::from_u64_word(99);
        assert_eq!(borsh::to_vec(&Key::Claim(h)).unwrap(), [vec![0], h.as_byte_slice().to_vec()].concat());
        assert_eq!(borsh::to_vec(&Key::TirPreparation(h, h)).unwrap()[0], 8);
        assert_eq!(borsh::to_vec(&Row::TirLayout(Box::new(f.layout))).unwrap()[0], 10);
    }
}

// Exhaustive read-only native input allocation inventory; no consensus encoding change.
#[path = "palw_native_resource_v1/private/palw_tir_public_claim_v1.rs"]
mod native_resource_inventory_v1;
