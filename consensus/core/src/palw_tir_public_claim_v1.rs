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

#[cfg(test)]
thread_local! { static PREPARATION_SOURCE_STAGES_V2: std::cell::Cell<[u64;4]> = const {std::cell::Cell::new([0;4])}; }
#[cfg(test)]
pub(crate) fn preparation_source_stages_v2() -> [u64;4] { PREPARATION_SOURCE_STAGES_V2.with(std::cell::Cell::get) }
#[cfg(test)]
pub(crate) fn reset_preparation_source_stages_v2() { PREPARATION_SOURCE_STAGES_V2.with(|c|c.set([0;4])); }
#[cfg(test)]
pub(crate) fn note_preparation_source_stage_v2(index:usize) { PREPARATION_SOURCE_STAGES_V2.with(|c|{let mut v=c.get();v[index]+=1;c.set(v);}); }

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
        Self::finish_preparation_source_v2(
            original,
            header,
            plan,
            custody,
            geometry,
            record,
            rules,
            native_panel,
            original_object,
            &source,
            verify,
        )
    }

    /// Typed native Source admission only. Frame arguments are already owned;
    /// their construction and header-digest scratch are separate boundaries.
    /// Legacy prepare_native_v1 keeps its original raw constructor and String API.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_native_source_v2<F>(
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
    ) -> Result<Self, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        crate::palw_tir_public_allocation_v1::check_tir_public_allocation_native_v1(
            &plan, geometry, &original, &header, record, layout, semantics, inventory, rules,
        )
        .map_err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode)?;
        #[cfg(test)]
        note_preparation_source_stage_v2(2);
        let source = PalwTirPublicSourceV1::new_native_v1(
            record,
            layout,
            &header,
            original.execution_root,
            original.trace_root,
            rules.maximum_leaves,
        )?;
        #[cfg(test)]
        note_preparation_source_stage_v2(3);
        Self::finish_preparation_source_v2(
            original,
            header,
            plan,
            custody,
            geometry,
            record,
            rules,
            native_panel,
            original_object,
            &source,
            verify,
        )
        .map_err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_preparation_source_v2<F>(
        original: PalwFreePromptCommitmentV3,
        header: PalwPublicTirBindingHeaderV1,
        plan: PalwTirPublicAllocationPlanV1,
        custody: PalwTirPublicCustodyContractV1,
        geometry: &crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1,
        record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        rules: PalwTirPublicAllocationRulesV1,
        native_panel: &BTreeSet<PalwBondKeyV2>,
        original_object: &crate::palw_state_v2::PalwConsensusObjectV2,
        source: &PalwTirPublicSourceV1,
        verify: F,
    ) -> Result<Self, String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        if custody.drawn_panel_v1() != native_panel || custody.liable_signers_v1() != std::iter::once(geometry.producer).collect() {
            return Err("original TIR frame imports a foreign Panel or a not-yet-credited custody grant".into());
        }
        custody.pledge_v1().authenticate_original_object_v1(
            original_object,
            source,
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
            native_clock: rules.clock,
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
    /// **G14 R1/R7 owner capacity (Lead default 2026-10-15, PENDING USER REVIEW):** at most this many outside owner acquisitions
    /// per ORIGINAL frame over its life. Excess owners are refused at Open, before any custodian duty exists.
    pub const MAXIMUM_OWNERS_V1: u64 = PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1;
    /// The custodian load this frame can impose, derived from the plan's per-owner acquisition capacity and the owner cap: every
    /// liable custodian answers at most `owners × maximum_turns` demands of at most `maximum_unit_bytes` each. Nodes provision
    /// for this bound; the fold never lets a further owner raise it.
    pub fn custodian_load_bound_v1(&self) -> PalwTirPublicCustodianLoadV1 {
        let owners = PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1;
        let turns = owners.saturating_mul(self.plan.acquisition.maximum_turns);
        PalwTirPublicCustodianLoadV1 {
            owners,
            demands_per_custodian: turns,
            answer_bytes_per_custodian: turns.saturating_mul(self.plan.acquisition.maximum_unit_bytes),
        }
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
            || d.native_clock != self.plan.acquisition.clock
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
        self.source_identity_v1(record, layout, maximum_leaves)
    }

    /// **The rooted frame's source without re-checking the whole frame** (audit §10): the per-action path of an owner's acquisition
    /// (Demand, answers, debits), on a frame the fold checked in full when it wrote it (install and every custody write) and the
    /// state root authenticates since. Checks the class, layout, leaf ceiling and source identity it builds from.
    pub fn source_identity_v1(
        &self,
        record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
        layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
        maximum_leaves: u64,
    ) -> Result<PalwTirPublicSourceV1, String> {
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

/// **K_owner — outside owners per ORIGINAL TIR frame** (Lead default 2026-10-15, pending user review). Each owner pays its own
/// rent and deposit (never the producer's prepaid pool); the cap bounds what the frame's liable custodians must answer. Known
/// residual: a colluder can pre-empt the slots by paying K owners' rent and deposit; slot recycling for idle owners is a
/// follow-up decision (recorded in the remaining-work document).
pub const PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1: u64 = 8;

/// **Lane g14-court-capacity-r2: an admitted owner's tenure while others wait** — the larger of the idle grace and the frame's own
/// Open window (`open_until − preparation_until`) over `2 × K_owner`, so the owner queue turns over at least `2 × K_owner` times
/// inside the window: a verifier with `q` rows ahead of it is admitted within `⌈(q + 1) / K_owner⌉` tenures of its Open. Past
/// `palw_court_capacity_v1` only (the fold's `rotate_owner_queue_v1`; a node's owner worker reads the same number).
pub fn palw_tir_owner_tenure_v1(frame: &PalwTirPublicClaimV1, grace: u64) -> u64 {
    let plan = frame.plan_v1();
    palw_tir_owner_tenure_of_window_v1(plan.terms.open_until_daa.saturating_sub(plan.preparation_until_daa), grace)
}

/// [`palw_tir_owner_tenure_v1`] of an Open window — the ONE tenure rule: the fold's queue rotation reads it through the frame, and the
/// frame's own door (`palw_tir_frame_cost_v1::tir_public_owner_queue_wait_v1`) reads it before the frame exists.
pub fn palw_tir_owner_tenure_of_window_v1(open_window: u64, grace: u64) -> u64 {
    grace.max(open_window / (2 * PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1)).max(1)
}

/// The bound on what one liable custodian of a frame can be asked to answer (see [`PalwTirPublicClaimV1::custodian_load_bound_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirPublicCustodianLoadV1 {
    pub owners: u64,
    pub demands_per_custodian: u64,
    pub answer_bytes_per_custodian: u64,
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
    /// The ORIGINAL job's selection rule, which the frame's selection and stop doors judge (`None`: no frame selection door).
    pub selection: Option<crate::palw_tir_court_v1::PalwTirSelectionRuleV1>,
    /// Outside owners already opened on the frame, and the custodian load bound (K_owner).
    pub owners_opened: u64,
    pub custodian_load: PalwTirPublicCustodianLoadV1,
    pub header: PalwPublicTirBindingHeaderV1,
}
impl PalwTirPublicClaimV1 {
    pub fn view_v1(&self, owners_opened: u64) -> PalwTirPublicFrameViewV1 {
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
            selection: crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&self.original.job),
            owners_opened,
            custodian_load: self.custodian_load_bound_v1(),
            header: self.header.clone(),
        }
    }
}
/// The accepted-state reader behind the consensus API's `palw_tir_public_frame_v1`.
pub fn palw_tir_public_frame_view_v1(state: &crate::palw_state_v2::PalwChainStateV2, claim: Hash64) -> Option<PalwTirPublicFrameViewV1> {
    palw_tir_public_claim_v1(state, claim).map(|frame| frame.view_v1(state.staged_tir_owner_count_v1(&claim)))
}

/// No fallback to ClaimBundle: callers must distinguish the new original
/// full-allocation protocol from the old context-only dormant path.
pub fn palw_tir_public_claim_v1(state: &crate::palw_state_v2::PalwChainStateV2, claim: Hash64) -> Option<&PalwTirPublicClaimV1> {
    match state.staged_court_row_v1(&Key::TirClaim(claim)) {
        Some(Row::TirClaim(frame)) => Some(frame),
        _ => None,
    }
}

/// **G14 G-13 (TIR side): is `(seat, claim)`'s slashable lock reserved for an ORIGINAL frame's own conviction at `now`?** — the
/// claim carries a frame whose d0 door is still open and `seat` holds an exact credited Valid custody grant in it. While this holds,
/// a path that would drop the lock for another reason (an S2 licence redraw, a Panel-v3 shard redraw, a vertex equivocation's lock
/// forfeiture) leaves it in place, so the frame's conviction finds every credited seat's lock (`apply_conviction_v1` asserts it).
/// `false` for every claim without a frame — every claim below `palw_tir_public_conviction_v1` — so those paths are byte-identical.
pub fn palw_tir_public_frame_keeps_lock_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    seat: &PalwBondKeyV2,
    claim: Hash64,
    now: u64,
) -> bool {
    palw_tir_public_claim_v1(state, claim)
        .is_some_and(|frame| now < frame.plan_v1().terms.collect_until_daa && frame.original_custody_v1().credit_v1(seat).is_some())
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

// Guard-only injection: deliberately not a native admission/serialization fixture.
#[cfg(test)]
impl PalwTirPublicClaimV1 {
    pub(crate) fn with_service_semantic_guard_fixture_v1(mut self, semantic: crate::palw_staged_bundle_v1::PalwStagedBundlePlanV1, id: Hash64) -> Self {
        self.plan.semantic = Some(semantic);
        self.plan.semantic_resource_plan_id = Some(id);
        self
    }
}

#[cfg(test)]
pub(crate) fn palw_tir_public_frame_fixture_v1(forge: Option<(usize, usize)>, maximum_custodians: u64) -> PalwTirPublicFrameFixtureV1 {
    tests::fixture_in(forge, maximum_custodians)
}

/// Golden history-bearing corpus in its actual original FP context; verified-half
/// original signature/payment assumptions are identical to the other fixtures.
#[cfg(test)]
pub(crate) fn palw_tir_public_history_frame_fixture_v1() -> PalwTirPublicFrameFixtureV1 {
    tests::history_frame()
}
/// A distinct registered and honestly executed program: numerical Select condition
/// reads actual Input(0), not a synthetic site or a mutated admitted plan.
#[cfg(test)]
pub(crate) fn palw_tir_public_input_history_frame_fixture_v1(
    form:crate::palw_prompt_ids_v1::PalwPromptIdsFormV1)->PalwTirPublicFrameFixtureV1 {
    tests::history_frame_form_v1(true,form)
}
#[cfg(test)]
pub(crate) fn palw_tir_public_prefix_history_frame_fixture_v1(fault:bool)->PalwTirPublicFrameFixtureV1 {
    tests::history_frame_condition_v1(tests::HistoryConditionV1::Prefix{fault},
        crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
}
#[cfg(test)]
pub(crate) fn palw_tir_public_fixed_history_frame_fixture_v1(
    form:crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,committed_writer:bool)->PalwTirPublicFrameFixtureV1 {
    tests::history_frame_condition_v1(tests::HistoryConditionV1::Fixed{committed_writer},form)
}

#[cfg(test)]
pub(crate) fn palw_tir_public_constraint_stop_frame_fixture_v1(stop:crate::palw_freeprompt_v3::PalwFpStopReasonV3,fault:bool)
    ->(PalwTirPublicFrameFixtureV1,crate::palw_class_semantics_v1::PalwClassSemanticsV1,crate::palw_fp_constraint_job_v1::PalwTokenTableV1) {
    tests::constraint_frame_stop_mode_v1(Some((stop,fault)))
}

/// Existing 4096-wide test execution reconstructed in its actual original job context.
#[cfg(test)]
pub(crate) fn palw_tir_public_frame_fixture_wide_v1(forge:Option<(usize,usize)>,maximum_custodians:u64)->PalwTirPublicFrameFixtureV1 {
    tests::fixture_with_vocab(forge,maximum_custodians,|_|{},None,None,true)
}

/// Additional two-tile fixture; existing tiny/GQA/4096-wide fixtures are unchanged.
#[cfg(test)]
pub(crate) fn palw_tir_public_frame_fixture_two_tile_v1() -> PalwTirPublicFrameFixtureV1 {
    tests::two_tile_frame()
}

/// Native-derived constrained frame over the real tiny registered program.
/// Signature/payment setup remains the same explicit verified-half fixture.
#[cfg(test)]
pub(crate) fn palw_tir_public_constraint_frame_fixture_v1()->(PalwTirPublicFrameFixtureV1,
    crate::palw_class_semantics_v1::PalwClassSemanticsV1,crate::palw_fp_constraint_job_v1::PalwTokenTableV1) {
    tests::constraint_frame()
}

#[cfg(test)]
pub(crate) fn palw_tir_public_tokenizer_frame_fixture_v1()->(PalwTirPublicFrameFixtureV1,
    crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1,
    Vec<u8>,crate::palw_fp_constraint_job_v1::PalwTokenTableV1) {tests::tokenizer_frame()}

/// **G14 G-T2 test support:** [`palw_tir_public_frame_fixture_v1`] over a job `edit_job` shapes (a seeded V3 job, a V4 job with
/// penalties or stop sequences, ...), whose execution decodes by THAT job's own selection rule — except at decode position
/// `wrong_at`, where it commits the best lane the rule does not choose (a selection violation over honest arithmetic).
#[cfg(test)]
pub(crate) fn palw_tir_public_frame_fixture_ruled_v1(
    maximum_custodians: u64,
    edit_job: impl FnOnce(&mut crate::palw_freeprompt_v3::PalwFreePromptJobV3),
    wrong_at: Option<u32>,
) -> PalwTirPublicFrameFixtureV1 {
    tests::fixture_with(None, maximum_custodians, edit_job, wrong_at, None)
}

/// **G14 test support: an honest execution whose producer-signed ORIGINAL commitment names `output_root`** — a different output
/// root than the IDs its own committed trace authenticates. Built from the start (the claim id, pledge, plan and frame all over
/// that commitment), so the frame's structure check holds; only the output door can convict it.
#[cfg(test)]
pub(crate) fn palw_tir_public_frame_fixture_lying_output_v1(maximum_custodians: u64, output_root: Hash64) -> PalwTirPublicFrameFixtureV1 {
    tests::fixture_with(None, maximum_custodians, |_| {}, None, Some(output_root))
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
        fixture_with(forge, maximum_custodians, |_| {}, None, None)
    }
    /// [`fixture_in`] with the job edited by `edit_job` before its context is derived, decoded by the edited job's own rule
    /// (greedy when the job leaves the V3 greedy default), the rule violated at decode position `wrong_at`.
    pub(super) fn fixture_with(
        forge: Option<(usize, usize)>,
        maximum_custodians: u64,
        edit_job: impl FnOnce(&mut crate::palw_freeprompt_v3::PalwFreePromptJobV3),
        wrong_at: Option<u32>,
        output_root: Option<Hash64>,
    ) -> super::PalwTirPublicFrameFixtureV1 {
        fixture_with_vocab(forge,maximum_custodians,edit_job,wrong_at,output_root,false)
    }
    pub(super) fn fixture_with_vocab(
        forge:Option<(usize,usize)>,maximum_custodians:u64,
        edit_job:impl FnOnce(&mut crate::palw_freeprompt_v3::PalwFreePromptJobV3),wrong_at:Option<u32>,output_root:Option<Hash64>,wide:bool,
    )->super::PalwTirPublicFrameFixtureV1 {
        use crate::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3};
        use crate::palw_freeprompt_v3::*;
        let honest = if wide {crate::palw_tir_court_v1::test_support::tiny_execution_wide().0}
            else {crate::palw_tir_court_v1::test_support::tiny_execution(None)};
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
        edit_job(&mut job);
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
        let rule = crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&job).expect("a fixture job has a selection rule");
        let select = |position: u32, row: &[i32], before: &[u32]| {
            crate::palw_tir_court_v1::test_support::select_by_rule(&rule, row, before, wrong_at == Some(position))
        };
        let x = if wide {crate::palw_tir_court_v1::test_support::tiny_execution_selecting_wide_in(|_,_,_|context.clone(),forge,&select)}
            else {crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in(|_, _, _| context.clone(), forge, &select)};
        frame_from_execution(x, job, maximum_custodians, output_root)
    }

    pub(super) fn two_tile_frame() -> super::PalwTirPublicFrameFixtureV1 {
        use crate::palw_tir_court_v1::test_support::{tiny_execution,tiny_execution_selecting_in_vocab,select_by_rule};
        use crate::palw_freeprompt_v3::*;
        use crate::palw_fp_execution_v3::{PalwFpClassFactsV3,PalwFpRunFactsV3};
        // A new synthetic program, not a change to the fixed existing fixtures or native caps.
        let honest=tiny_execution_selecting_in_vocab(|class,id,prompt| {
            let mut c=tiny_execution(None).binding.job_context;c.shape_profile_id=id;c.tokenizer_id=class.tokenizer_id;
            c.prompt_token_ids_hash=crate::palw_v2::prompt_token_ids_hash_v2(prompt);c
        },None,&|_,row,_|crate::palw_step_refute::base0_decode_token_select_v1(row) as u32,4097);
        let (_,_,mut job,_)=crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false,false);
        job.version=PALW_FP_V3_VERSION;job.decode=None;job.tail=None;
        job.class_id=honest.binding.class.class_id(&honest.binding.artifact_root);
        job.tokenizer_id=honest.binding.class.tokenizer_id;job.prompt_token_ids_hash=crate::palw_v2::prompt_token_ids_hash_v2(&honest.prompt);
        job.prompt_tokens=honest.prompt.len() as u32;job.decode_token_limit=honest.generated.len() as u32;job.max_context_tokens=64;
        let class=PalwFpClassFactsV3 {model_profile_id:Hash64::default(),runtime_manifest_hash:Hash64::default(),
            runtime_class_id:Hash64::default(),shape_profile_id:job.class_id,cu_ruleset_id:Hash64::default()};
        let facts=PalwFpRunFactsV3 {decode_tokens_executed:job.decode_token_limit,stop_reason:PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root:Hash64::default(),activation_leg_root:Hash64::default(),checkpoint_leg_root:Hash64::default(),
            step_leg_root:Hash64::default(),step_leaf_count:honest.hashes.len() as u64};
        let ctx=crate::palw_fp_execution_v3::palw_fp_job_context_v3(&job,&class,&facts,b"testnet-12").unwrap();
        let rule=crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&job).unwrap();
        let x=tiny_execution_selecting_in_vocab(|_,_,_|ctx,None,&|_,row,before|select_by_rule(&rule,row,before,false),4097);
        frame_from_execution(x,job,1,None)
    }

    // Shared geometry construction: every tree is committed in this job's actual FP context.
    // Signature/payment remain the explicitly injected verified-half fixture below.
    fn frame_from_execution(
        x: crate::palw_tir_court_v1::test_support::TinyExecution,
        job: crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        maximum_custodians: u64,
        output_root: Option<Hash64>,
    ) -> super::PalwTirPublicFrameFixtureV1 {
        frame_from_execution_with_semantics(x,job,maximum_custodians,output_root,None)
    }
    fn frame_from_execution_with_semantics(
        x:crate::palw_tir_court_v1::test_support::TinyExecution,
        job:crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        maximum_custodians:u64,output_root:Option<Hash64>,
        semantics:Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    )->super::PalwTirPublicFrameFixtureV1 {
        frame_from_execution_with_material(x,job,maximum_custodians,output_root,semantics,None)
    }
    fn frame_from_execution_with_material(
        x:crate::palw_tir_court_v1::test_support::TinyExecution,
        job:crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        maximum_custodians:u64,output_root:Option<Hash64>,
        semantics:Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
        inventory:Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    )->super::PalwTirPublicFrameFixtureV1 {
        frame_from_execution_with_material_form_v1(x,job,maximum_custodians,output_root,
            semantics,inventory,crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
    }
    fn frame_from_execution_with_material_form_v1(
        x:crate::palw_tir_court_v1::test_support::TinyExecution,
        job:crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        maximum_custodians:u64,output_root:Option<Hash64>,
        semantics:Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
        inventory:Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
        prompt_form:crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    )->super::PalwTirPublicFrameFixtureV1 {
        frame_from_execution_with_material_form_stop_v1(x,job,maximum_custodians,output_root,semantics,inventory,prompt_form,
            crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached)
    }
    fn frame_from_execution_with_material_form_stop_v1(
        x:crate::palw_tir_court_v1::test_support::TinyExecution,
        job:crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        maximum_custodians:u64,output_root:Option<Hash64>,
        semantics:Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
        inventory:Option<&crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
        prompt_form:crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        stop_reason:crate::palw_freeprompt_v3::PalwFpStopReasonV3,
    )->super::PalwTirPublicFrameFixtureV1 {
        use crate::palw_freeprompt_v3::*;
        use crate::palw_tir_public_allocation_v1::*;
        let binding = x.binding.clone();
        let class_id = binding.class.class_id(&binding.artifact_root);
        let original = PalwFreePromptCommitmentV3 {
            job,
            trace_root: binding.full_logits_trace_root,
            output_root: output_root
                .unwrap_or_else(|| crate::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &x.generated)),
            schedule_root: Hash64::default(),
            execution_root: binding.committed_execution_root,
            decode_tokens_executed: x.generated.len() as u32,
            stop_reason,
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
            prompt_form,
            config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
            clock: crate::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1().clock,
        };
        let PalwTirPublicAllocationQuoteV1::Complete(plan) =
            derive_tir_public_allocation_v1(&geometry, &original, &header, &record, &layout, semantics, inventory, rules).unwrap()
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
            semantics,
            inventory,
            rules,
            &BTreeSet::new(),
            &object,
            verify,
        )
        .unwrap();
        super::PalwTirPublicFrameFixtureV1 { frame, record, layout, execution: x, rules, object }
    }
    pub(super) fn constraint_frame()->(super::PalwTirPublicFrameFixtureV1,
        crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        crate::palw_fp_constraint_job_v1::PalwTokenTableV1) {
        constraint_frame_stop_mode_v1(None)
    }
    pub(super) fn constraint_frame_stop_mode_v1(mode:Option<(crate::palw_freeprompt_v3::PalwFpStopReasonV3,bool)>)->(super::PalwTirPublicFrameFixtureV1,
        crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        crate::palw_fp_constraint_job_v1::PalwTokenTableV1) {
        use crate::palw_freeprompt_v3::*;
        use crate::palw_fp_execution_v3::{PalwFpClassFactsV3,PalwFpRunFactsV3};
        let honest=crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let class_id=honest.binding.class.class_id(&honest.binding.artifact_root);
        let (_,accusation)=crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1(false);
        let mut job=accusation.job;
        job.class_id=class_id;job.tokenizer_id=honest.binding.class.tokenizer_id;
        job.prompt_tokens=honest.prompt.len() as u32;
        job.prompt_token_ids_hash=crate::palw_v2::prompt_token_ids_hash_v2(&honest.prompt);
        job.decode_token_limit=honest.generated.len() as u32+u32::from(mode.is_some_and(|(s,_)|s==PalwFpStopReasonV3::EndOfGeneration));job.max_context_tokens=64;
        let stop_reason=mode.map_or(PalwFpStopReasonV3::ExactBudgetReached,|(s,_)|s);
        // All real eight-word registered lanes render an inadmissible first byte
        // under the existing bracket/digit constraint. Arithmetic remains real;
        // the semantic court must detect this deliberately bad selection.
        let table=crate::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries:vec![Some(b"x".to_vec());honest.rows[0].len()],
            eog_token_ids:vec![mode.map_or((honest.generated[0]+1) % honest.rows[0].len() as u32,|(stop,fault)|match (stop,fault) {
                (PalwFpStopReasonV3::ExactBudgetReached,true)=>0,
                (PalwFpStopReasonV3::EndOfGeneration,false)=>honest.generated.len() as u32-1,
                _=>honest.rows[0].len() as u32-1,
            })],tokenizer_id:job.tokenizer_id,
        };
        let Some(PalwFpJobTailV1::Constraint(tail))=job.tail.as_mut() else {panic!("constraint job")};
        tail.table_root=table.root();
        let class=PalwFpClassFactsV3 {model_profile_id:Hash64::default(),runtime_manifest_hash:Hash64::default(),
            runtime_class_id:Hash64::default(),shape_profile_id:class_id,cu_ruleset_id:Hash64::default()};
        let facts=PalwFpRunFactsV3 {decode_tokens_executed:honest.generated.len() as u32,
            stop_reason,full_logits_trace_root:Hash64::default(),
            activation_leg_root:Hash64::default(),checkpoint_leg_root:Hash64::default(),step_leg_root:Hash64::default(),
            step_leaf_count:honest.hashes.len() as u64};
        let context=crate::palw_fp_execution_v3::palw_fp_job_context_v3(&job,&class,&facts,b"testnet-12").unwrap();
        let rule=crate::palw_tir_court_v1::PalwTirSelectionRuleV1::Keyed {sampling:job.sampling_v2()};
        // Stop-mode fixture still executes the actual registered arithmetic and
        // commits its actual IDs; selection may be false under the grammar.
        // Only the ORIGINAL stop predicate is the test's honest/fault label.
        let select=|pos:u32,row:&[i32],before:&[u32]|if mode.is_some(){pos % row.len() as u32}
            else{crate::palw_tir_court_v1::test_support::select_by_rule(&rule,row,before,false)};
        let x=crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in(|_,_,_|context.clone(),None,&select);
        let semantics=crate::palw_class_semantics_v1::PalwClassSemanticsV1 {
            version:1,class_id,artifact_root:x.binding.artifact_root,bound_from_daa:0,
            model_profile_id:context.model_profile_id,runtime_manifest_hash:context.runtime_manifest_hash,
            runtime_class_id:context.runtime_class_id,tokenizer_id:context.tokenizer_id,cu_ruleset_id:context.cu_ruleset_id,
            token_table_root:Some(table.root()),rendering_rule:None,raw_tokenizer_bytes:None,
            tokenizer_key_events:None,prefix_state_relation:None,
        };
        if mode.is_some(){assert_eq!(x.generated,(0..honest.generated.len() as u32).collect::<Vec<_>>());}
        let frame=frame_from_execution_with_material_form_stop_v1(x,job,1,None,Some(&semantics),None,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,stop_reason);
        assert!(frame.frame.plan_v1().semantic.is_some());
        (frame,semantics,table)
    }

    pub(super) fn tokenizer_frame()->(super::PalwTirPublicFrameFixtureV1,
        crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1,
        Vec<u8>,crate::palw_fp_constraint_job_v1::PalwTokenTableV1) {
        use crate::palw_freeprompt_v3::*;
        use crate::palw_tokenizer_artifact_inventory_v1::*;
        let raw=br#"{"model":{"vocab":{"A":0,"B":1,"C":2,"D":3,"E":4,"F":5,"G":6,"H":7},"merges":[]},"added_tokens":[]}"#.to_vec();
        let tokenizer=crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw);
        let table=crate::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries:(b'A'..=b'H').map(|b|Some(vec![b])).collect(),eog_token_ids:vec![7],tokenizer_id:tokenizer};
        let (_,_,mut job,_)=crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false,false);
        job.version=PALW_FP_V3_VERSION;job.tail=None;job.decode=None;job.tokenizer_id=tokenizer;
        job.decode_token_limit=crate::palw_tir_court_v1::test_support::DECODE;job.max_context_tokens=64;
        let rule=crate::palw_tir_court_v1::PalwTirSelectionRuleV1::Keyed {sampling:job.sampling_v2()};
        let select=|_:u32,row:&[i32],before:&[u32]|crate::palw_tir_court_v1::test_support::select_by_rule(&rule,row,before,false);
        let x=crate::palw_tir_court_v1::test_support::tiny_execution_selecting_in_vocab_with_tokenizer(
            |class,id,prompt| {
                job.class_id=id;job.tokenizer_id=class.tokenizer_id;job.prompt_tokens=prompt.len() as u32;
                job.prompt_token_ids_hash=crate::palw_v2::prompt_token_ids_hash_v2(prompt);
                let facts=crate::palw_fp_execution_v3::PalwFpClassFactsV3 {model_profile_id:Hash64::default(),
                    runtime_manifest_hash:Hash64::default(),runtime_class_id:Hash64::default(),shape_profile_id:id,cu_ruleset_id:Hash64::default()};
                crate::palw_fp_execution_v3::palw_fp_job_context_v3(&job,&facts,
                    &crate::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(&job,job.decode_token_limit),b"testnet-12").unwrap()
            },None,&select,8,tokenizer);
        let opener=PalwBondKeyV2(job.executor_bond);
        let key=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91;32]);
        let mut offer=PalwTokenizerArtifactInventoryOfferV1 {version:1,artifact_hash:tokenizer,opener,
            maximum_bytes:raw.len() as u64,accept_until_daa:10,signature:vec![]};
        offer.signature=libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key,offer.id_v1().as_byte_slice(),
            PALW_ARTIFACT_INVENTORY_CONTEXT_V1,[92;32]).unwrap().as_ref().to_vec();
        offer.validate_signature_v1(key.verification_key.as_ref(),crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1).unwrap();
        let mut inventory=PalwTokenizerArtifactInventoryV1::new_v1(offer,1,
            crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1()).unwrap();
        inventory.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Part {offset:0,bytes:raw.clone()},2).unwrap();
        let eof=inventory.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish,3).unwrap().unwrap();
        let c=&x.binding.job_context;
        let semantics=crate::palw_class_semantics_v1::PalwClassSemanticsV1 {version:1,class_id:job.class_id,
            artifact_root:x.binding.artifact_root,bound_from_daa:0,model_profile_id:c.model_profile_id,
            runtime_manifest_hash:c.runtime_manifest_hash,runtime_class_id:c.runtime_class_id,tokenizer_id:tokenizer,
            cu_ruleset_id:c.cu_ruleset_id,token_table_root:Some(table.root()),
            rendering_rule:Some(crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1),
            raw_tokenizer_bytes:Some(eof.actual_bytes),tokenizer_key_events:Some(eof.key_event_upper_bound_v1()),prefix_state_relation:None};
        let fx=frame_from_execution_with_material(x,job,1,None,Some(&semantics),Some(&eof));
        assert!(fx.frame.plan_v1().semantic.as_ref().unwrap().stages.iter().any(|s|
            s.family==crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Tokenizer));
        (fx,semantics,eof,raw,table)
    }

    /// Actual golden GQA execution with history reductions, re-committed under the public job context.
    pub(super) fn history_frame() -> super::PalwTirPublicFrameFixtureV1 {
        history_frame_form_v1(false,crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat)
    }
    #[derive(Clone,Copy)]
    pub(super) enum HistoryConditionV1 { None,Input,Fixed{committed_writer:bool},Prefix{fault:bool} }
    pub(super) fn history_frame_form_v1(direct_input:bool,prompt_form:crate::palw_prompt_ids_v1::PalwPromptIdsFormV1)
        ->super::PalwTirPublicFrameFixtureV1 {
        history_frame_condition_v1(if direct_input {HistoryConditionV1::Input}else{HistoryConditionV1::None},prompt_form)
    }
    pub(super) fn history_frame_condition_v1(condition:HistoryConditionV1,prompt_form:crate::palw_prompt_ids_v1::PalwPromptIdsFormV1)
        ->super::PalwTirPublicFrameFixtureV1 {
        use crate::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3};
        use crate::palw_freeprompt_v3::*;
        use crate::palw_tir_corpus_fixture_v1::{fixture_with, programs};
        let (name, mut program, params, tokens) = programs().into_iter()
            .find(|(name, ..)| name == "dense-gqa-2layer").expect("golden GQA corpus");
        let fixed=if let HistoryConditionV1::Fixed{committed_writer}=condition {
            let state:u16=program.states.len().try_into().unwrap();
            program.states.push(misaka_palw_tir::program::StateDecl{name:"boundary_scalar".into(),
                kind:misaka_palw_tir::program::StateKind::Fixed{lo:0,hi:1},dtype:misaka_palw_tir::DType::I8,
                shape:vec![],per_layer:true});
            Some((state,committed_writer))
        } else {None};
        if matches!(condition,HistoryConditionV1::Input|HistoryConditionV1::Fixed{..}) {
            use misaka_palw_tir::{Ref,Prim};
            use misaka_palw_tir::types::Dim;
            let (bi,ni,input,out)=program.blocks.iter().enumerate().find_map(|(bi,b)| {
                b.nodes.iter().enumerate().find_map(|(ni,n)| {
                    if !matches!(n.prim,Prim::ReduceSum{..}|Prim::ReduceMax{..}) {return None;}
                    let Ref::Node(input)=*n.inputs.first()? else {return None;};
                    let out=b.nodes.get(usize::from(input))?.out.clone();
                    out.shape.contains(&Dim::H).then_some((bi,ni,input,out))
                })
            }).expect("GQA actual history reduction");
            let block=&mut program.blocks[bi];
            for node in &mut block.nodes {for input in &mut node.inputs {
                if let Ref::Node(index)=input {if usize::from(*index)>=ni {*index=index.checked_add(1).unwrap();}}
            }}
            for index in &mut block.carry_out {if usize::from(*index)>=ni {*index=index.checked_add(1).unwrap();}}
            if bi==usize::from(program.schedule.post)&&usize::from(program.logits)>=ni {program.logits+=1;}
            block.nodes.insert(ni,misaka_palw_tir::program::Node {
                prim:Prim::Select,
                inputs:vec![fixed.map_or(Ref::Input(0),|(state,_)|Ref::State(state)),Ref::Node(input),Ref::Node(input)],out,commit:false,
            });
            block.nodes[ni+1].inputs[0]=Ref::Node(ni.try_into().unwrap());
            // The program is canonical-encoded, admitted, interpreted and all
            // roots recomputed by fixture_with. No existing original is edited.
        }
        if let Some((state,committed_writer))=fixed {
            use misaka_palw_tir::{DType,Ref,Prim,TensorType};
            use misaka_palw_tir::program::{ConstDecl,Node};
            let zero=ConstDecl{dtype:DType::Idx,shape:vec![],data:vec![0;4]};
            let z=if let Some(index)=program.consts.iter().position(|c|c==&zero) {index}
                else {program.consts.push(zero);program.consts.len()-1};
            // Each actual scheduled layer writes its own scalar instance from
            // the actual committed carry vector. Rank-reducing gathers use a
            // real Idx0 constant, then a clamp guarantees Fixed [0,1].
            let layers:std::collections::BTreeSet<_>=program.schedule.layers.iter().copied().collect();
            for bi in layers {
                let block=&mut program.blocks[usize::from(bi)];
                let carry=block.carry_in.first().expect("GQA carry").clone();
                let mut input=Ref::CarryIn(0);
                for dim in 0..carry.shape.len() {
                    let out=TensorType{dtype:carry.dtype,shape:carry.shape[dim+1..].to_vec()};
                    let node:u16=block.nodes.len().try_into().unwrap();
                    block.nodes.push(Node{prim:Prim::Gather{axis:0,batch_dims:0},inputs:vec![input,Ref::Const(z.try_into().unwrap())],out,commit:false});
                    input=Ref::Node(node);
                }
                let clamp:u16=block.nodes.len().try_into().unwrap();
                block.nodes.push(Node{prim:Prim::Clamp{lo:0,hi:1},inputs:vec![input],out:TensorType::scalar(DType::I8),commit:false});
                block.nodes.push(Node{prim:Prim::StateWrite{state},inputs:vec![Ref::Node(clamp)],out:TensorType::scalar(DType::I8),commit:committed_writer});
            }
        }
        let mut f = fixture_with(name, program, params, tokens, 16, 9);
        assert!(f.greedy, "the V3 job uses greedy decoding");
        let (_, _, mut job, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
        let prefix_decode=job.decode.clone();
        job.version = PALW_FP_V3_VERSION;
        job.decode = None;
        job.tail = None;
        job.class_id = f.class_id;
        job.tokenizer_id = f.class.tokenizer_id;
        job.prompt_token_ids_hash = crate::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(prompt_form,&f.prompt).unwrap();
        job.prompt_tokens = f.prompt.len() as u32;
        job.decode_token_limit = f.generated.len() as u32;
        job.max_context_tokens = 64;
        if let HistoryConditionV1::Prefix{fault}=condition {
            let k=4;
            let before=f.honest();
            let digest=crate::palw_tir_prefix_state_v1::palw_tir_prefix_kv_digest_v1(&f.space,&f.ctx,k,
                |leaf|Ok(before.preimages[leaf.index as usize].values_le.clone())).unwrap();
            let mut root=crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&f.class_id,k,&digest);
            if fault {let mut bytes=*root.as_byte_slice();bytes[0]^=1;root=Hash64::from_bytes(bytes);}
            job.version=crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION;
            job.decode=prefix_decode;
            job.tail=Some(PalwFpJobTailV1::Prefix(PalwFpPrefixStateV1{class_id:f.class_id,prefix_tokens:k,state_root:root}));
        }
        let class = PalwFpClassFactsV3 {
            model_profile_id: Hash64::default(), runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(), shape_profile_id: f.class_id, cu_ruleset_id: Hash64::default(),
        };
        let facts = PalwFpRunFactsV3 {
            decode_tokens_executed: job.decode_token_limit, stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
            full_logits_trace_root: Hash64::default(), activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(), step_leg_root: Hash64::default(), step_leaf_count: f.leaves.len() as u64,
        };
        f.ctx = crate::palw_fp_execution_v3::palw_fp_job_context_v3(&job, &class, &facts, b"testnet-12").unwrap();
        let x = f.honest();
        let x = crate::palw_tir_court_v1::test_support::TinyExecution {
            binding: x.binding, preimages: x.preimages, hashes: x.hashes, ops: f.ops,
            prompt: f.prompt, rows: x.rows, generated: x.generated,
        };
        if matches!(condition,HistoryConditionV1::Prefix{..}) {
            let c=&x.binding.job_context;
            let semantics=crate::palw_class_semantics_v1::PalwClassSemanticsV1{version:1,class_id:f.class_id,
                artifact_root:x.binding.artifact_root,bound_from_daa:0,model_profile_id:c.model_profile_id,
                runtime_manifest_hash:c.runtime_manifest_hash,runtime_class_id:c.runtime_class_id,tokenizer_id:c.tokenizer_id,
                cu_ruleset_id:c.cu_ruleset_id,token_table_root:None,rendering_rule:None,raw_tokenizer_bytes:None,
                tokenizer_key_events:None,prefix_state_relation:None};
            frame_from_execution_with_material_form_v1(x,job,1,None,Some(&semantics),None,prompt_form)
        } else {frame_from_execution_with_material_form_v1(x,job,1,None,None,None,prompt_form)}
    }

    #[test]
    fn history_original_frame_opens_its_own_dissected_root() {
        use crate::palw_tir_dissect_v1::*;
        use crate::palw_tir_court_v1::*;
        let fx = history_frame();
        let binding = &fx.execution.binding;
        let space = crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&binding.class).unwrap();
        let intervals = misaka_palw_tir::interval::analyze_ranges(&space.program).unwrap();
        let (leaf, site) = (0..binding.step_leaf_count).filter_map(|i| {
            let leaf = space.leaf_at(&binding.job_context, i).unwrap();
            palw_tir_dissect_site_v1(&space, &intervals, &leaf).map(|site| (i, site))
        }).max_by_key(|(i, site)| (site.history_positions, site.reductions.len(), std::cmp::Reverse(*i))).unwrap();
        assert_eq!(site.history_positions, 24);
        assert!(site.reductions.len() >= 3);
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: fx.rules.maximum_leaves, prompt_form: fx.rules.prompt_form,
            limits: crate::palw_court_v2::palw_tir_court_limits_v1(&fx.rules.court),
        };
        let root = build_tir_root_claim_v1(binding, leaf, &fx.execution, &rules).unwrap();
        assert_eq!(check_tir_root_claim_v1(&root, leaf, &rules).unwrap(), site);
        assert_eq!(fx.frame.original_v1().execution_root, binding.committed_execution_root);
        assert_eq!(fx.frame.original_v1().trace_root, binding.full_logits_trace_root);
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
            clock: crate::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1().clock,
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
