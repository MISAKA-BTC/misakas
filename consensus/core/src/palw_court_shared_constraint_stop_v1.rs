//! Private constrained original-stop evidence. Separate from old token intent,
//! no public wire/tag, paid admission, Complete flag or original-tail change.
use super::{SharedTirBuilderStoreV1 as Store, constraint_terminal::ConstraintMaterialV1 as Material};
use crate::palw_constraint_stream_court_v1::{
    PalwConstraintStreamCourtV1 as Court, PalwConstraintStreamFaultV1 as Fault, canonical_replay as replay,
};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_freeprompt_v3::PalwFpStopReasonV3;
use crate::palw_tokenizer_rendering_v2::{PalwRootedClaimTokenizerV1, palw_rooted_claim_tokenizer_v1};
use crate::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2},
};
#[path = "palw_court_batch_stop_payload_v1.rs"]
pub(crate) mod batch_payload;

/// The signed fields and tokenizer context returned for a booked Stop.
pub struct BatchStopSigningV1 {
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub policy_id: Hash64,
    pub semantics_id: Hash64,
    pub position: u32,
    pub stop_reason: PalwFpStopReasonV3,
    pub filer: PalwBondKeyV2,
    pub table_root: Hash64,
    pub vocab: u32,
}

impl BatchStopSigningV1 {
    /// Authenticate the local EOG opening and sign the existing EOG-only wire.
    /// The accepting node verifies this carrier against its own retained original.
    pub fn build_v1(
        self,
        network: Hash64,
        eog_bytes: &[u8],
        sign: impl FnOnce(Hash64, &[u8]) -> Vec<u8>,
    ) -> Result<crate::palw_batch_court_v1::carriage::BatchCourtWireV1, String> {
        use crate::palw_batch_court_v1::carriage::{BatchCourtWireV1, BatchStopControlV1};
        use crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
        if self.position == 0 || eog_bytes.len() > batch_payload::MAX_BATCH_STOP_EOG_OPENING_BYTES_V1 {
            return Err("Batch Stop position or EOG opening length".into());
        }
        let mut intent = SignedConstraintStopIntentV1 {
            version: 1, claim: self.claim, scope_id: self.scope_id, policy_id: self.policy_id, semantics_id: self.semantics_id,
            material_id: Hash64::default(), position: self.position, stop_reason: self.stop_reason, filer: self.filer,
            signature: vec![0; MLDSA87_SIGNATURE_LEN],
        };
        let mut operands = borsh::to_vec(&intent).map_err(|why| why.to_string())?;
        operands.extend_from_slice(eog_bytes);
        let empty_renderings: &[crate::palw_fp_constraint_job_v1::PalwTokenTableOpeningV1] = &[];
        operands.extend_from_slice(&0u32.to_le_bytes());
        let decoded = batch_payload::BatchStopPayloadV1::decode_v1(&operands, eog_bytes.len() as u64)
            .map_err(|why| why.to_string())?;
        if !decoded.eog.verifies(&self.table_root, self.vocab) {
            return Err("Batch Stop EOG opening does not match the quoted token table".into());
        }
        let material = Material { eog: &decoded.eog, renderings: empty_renderings };
        intent.material_id = material.identity(replay::WorkBudget { max_material_bytes: eog_bytes.len() as u64 + 4, max_calls: 0 })?;
        intent.signature = sign(intent.signing_message(network)?, CONSTRAINT_STOP_SIGNATURE_CONTEXT_V1);
        if intent.signature.len() != MLDSA87_SIGNATURE_LEN {
            return Err("Batch Stop signer returned the wrong signature length".into());
        }
        let operands = borsh::to_vec(&(&intent, &decoded.eog, empty_renderings)).map_err(|why| why.to_string())?;
        Ok(BatchCourtWireV1::Stop(BatchStopControlV1 { operands }))
    }
}

/// From the accepted BCT1 operand bytes to a native finding. The caller
/// still owns the terminal state transition and any resulting economic effect.
pub(crate) fn verify_rooted_batch_stop_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    rules: &NativeCourtRulesV1,
    operands: &[u8],
) -> Result<Option<Fault>, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::palw_court_shared_material_intake_v1::batch::constraint_stop::check_batch_stop_slot_v1;
    use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    let (claim, filer) = batch_payload::batch_stop_identity_v1(operands).map_err(|why| E::Decode(why.to_string()))?;
    let bytes = operands.len() as u64 + crate::palw_batch_court_v1::carriage::BATCH_STOP_WIRE_OVERHEAD_V1;
    if bytes > rules.court.max_close_bytes() {
        return Err(E::Decode("Batch Stop exceeds the native close byte limit".into()));
    }
    let bounds = check_batch_stop_slot_v1(state, claim, filer, rules.at.daa_score, bytes).map_err(E::Decode)?;
    let context = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(E::Decode)?;
    let key = Key::CourtSharedIds(claim);
    let Some(Row::CourtSharedIds(shared_ids)) = state.staged_court_row_v1(&key) else {
        return Err(E::Decode("Batch Stop has no accepted FullIds".into()));
    };
    let source = context.source(state).map_err(E::Decode)?;
    let ids = source.retain_complete_ids_classified_v2(shared_ids.rows_root, &shared_ids.ids)?;
    let tokenizer = palw_rooted_claim_tokenizer_v1(state, claim, &source, &ids)
        .map_err(E::Decode)?
        .ok_or_else(|| E::Decode("Batch Stop has no verified tokenizer registration for the original".into()))?;
    if tokenizer.opening_bounds_v1() != bounds {
        return Err(E::Decode("Batch Stop operand bounds changed during source authentication".into()));
    }
    let payload = batch_payload::BatchStopPayloadV1::decode_v1(operands, bounds.eog_opening_bytes)?;
    let store = Store {
        source,
        units: crate::palw_court_shared_tir_builder_store_v1::UnitMaterialV1::Retained(&[]),
        params: super::ParameterMaterialV1::Borrowed(&[]),
        ids: Some(&ids),
        prompt: super::PromptMaterialV1::Missing,
    };
    let budget = replay::WorkBudget { max_material_bytes: bounds.eog_opening_bytes + 4, max_calls: bounds.eog_replay_calls };
    store.verify_batch_stop_payload_v1(state, params, rules, &payload, budget).map_err(E::Decode)
}

pub(crate) const CONSTRAINT_STOP_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/shared-constraint-stop/v1/mldsa87";
#[derive(Clone, borsh::BorshSerialize)]
pub(crate) struct SignedConstraintStopIntentV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub policy_id: Hash64,
    pub semantics_id: Hash64,
    pub material_id: Hash64,
    /// Only the actual end position D is accepted; this is not a token root.
    pub position: u32,
    pub stop_reason: PalwFpStopReasonV3,
    pub filer: PalwBondKeyV2,
    pub signature: Vec<u8>,
}
impl SignedConstraintStopIntentV1 {
    pub(crate) fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 {
            return Err("constraint stop intent version".into());
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/shared-constraint-stop-intent/v1",
            &(
                network,
                self.version,
                self.claim,
                self.scope_id,
                self.policy_id,
                self.semantics_id,
                self.material_id,
                self.position,
                self.stop_reason,
                self.filer,
            ),
        )
    }
}
pub(crate) struct ConstraintStopEvidenceV1<'a> {
    intent: SignedConstraintStopIntentV1,
    material: Material<'a>,
    finding: Option<Fault>,
    identity: (Hash64, Hash64, Hash64, Hash64),
}
impl ConstraintStopEvidenceV1<'_> {
    pub(crate) fn recheck(
        &self,
        store: &Store<'_>,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        budget: replay::WorkBudget,
    ) -> Result<Option<Fault>, String> {
        let result = store.replay_constraint_stop_v1(state, params, rules, &self.intent, self.material, budget)?;
        if result.identity() != self.identity || result.finding() != self.finding.as_ref() {
            return Err("constraint stop reconstruction mismatch".into());
        }
        Ok(result.finding().cloned())
    }
}
// The replay cursor supplies each expected ID from the verified original
// sequence. Preserve ordered begin/finish verification and budget charging.
pub(crate) struct RootedStopMaterialV1<'a> {
    material: Material<'a>,
    eog: &'a [u32],
}
impl<'a> RootedStopMaterialV1<'a> {
    pub(crate) fn new(material: Material<'a>, tokenizer: &PalwRootedClaimTokenizerV1<'_>) -> Result<Self, String> {
        let eog = tokenizer.verified_eog(material.eog).ok_or("constraint stop registered EOG opening mismatch")?;
        if material.renderings.iter().any(|opening| !tokenizer.verifies_rendering(opening.id, opening)) {
            return Err("constraint stop registered rendering opening mismatch".into());
        }
        Ok(Self { material, eog })
    }
}
impl replay::Material for RootedStopMaterialV1<'_> {
    fn eog(&self) -> Option<&[u32]> {
        Some(self.eog)
    }
    fn rendering(&self, id: u32) -> Option<replay::RenderingMaterial<'_>> {
        // Same exact-ID search used by the ordinary path. `id` is the
        // native replay's next ID; a material entry cannot choose it.
        self.material.rendering(id)
    }
}

impl Store<'_> {
    /// Verify one decoded Batch filing against this branch's authenticated
    /// FullIds. The resulting finding still needs the native terminal fold.
    pub(crate) fn verify_batch_stop_payload_v1(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        payload: &batch_payload::BatchStopPayloadV1,
        budget: replay::WorkBudget,
    ) -> Result<Option<Fault>, String> {
        let material = Material { eog: &payload.eog, renderings: &[] };
        let result = self.replay_constraint_stop_v1(state, params, rules, &payload.intent, material, budget)?;
        Ok(result.finding().cloned())
    }

    /// Native caller resolves actual rules and separately admits source, parser,
    /// body proof/hash, actor signature, Court clone and evidence ownership.
    pub(crate) fn build_constraint_stop_terminal_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedConstraintStopIntentV1,
        material: Material<'a>,
        budget: replay::WorkBudget,
    ) -> Result<ConstraintStopEvidenceV1<'a>, String> {
        let result = self.replay_constraint_stop_v1(state, params, rules, intent, material, budget)?;
        let evidence = ConstraintStopEvidenceV1 {
            intent: intent.clone(),
            material,
            finding: result.finding().cloned(),
            identity: result.identity(),
        };
        evidence.recheck(self, state, params, rules, budget)?;
        Ok(evidence)
    }
    fn replay_constraint_stop_v1(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedConstraintStopIntentV1,
        material: Material<'_>,
        budget: replay::WorkBudget,
    ) -> Result<replay::Candidate, String> {
        let c = crate::palw_court_shared_tir_state_v1::context(state, intent.claim)?;
        let source = c.source(state)?;
        let frame =
            crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, intent.claim).ok_or("constraint stop original absent")?;
        let original = frame.original_v1();
        let job = &original.job;
        if intent.scope_id != c.scope_id
            || source.source_id_v1() != self.source.source_id_v1()
            || !rules.tir_active
            || !rules.constraint_active
            || rules.at.daa_score < c.original_accepted_daa
            || rules.at.daa_score >= c.hard_daa
            || rules.step_ladder != frame.plan_v1().owner_work.rules.max_step_leaf_count
            || intent.policy_id != crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
            || intent.position != original.decode_tokens_executed
            || intent.stop_reason != original.stop_reason
        {
            return Err("constraint stop actual scope/policy/end position/native reason mismatch".into());
        }
        let semantics = state.claim_semantics_v1(&intent.claim).ok_or("constraint stop immutable semantics absent")?;
        let header = source.header_v1();
        if !semantics.matches_context(&header.job_context)
            || !semantics.well_formed_for(&job.class_id, &header.artifact_root)
            || semantics.bound_from_daa > c.original_accepted_daa
            || intent.semantics_id != semantics.commitment()
            || semantics.token_table_root.is_none()
            || semantics.token_table_root != crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
        {
            return Err("constraint stop immutable table semantics mismatch".into());
        }
        if intent.material_id != material.identity(budget)? {
            return Err("constraint stop material identity mismatch".into());
        }
        let bond = state.bond(&intent.filer).ok_or("constraint stop filer absent")?;
        if intent.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || !matches!(bond.status, PalwBondStatusV2::Active)
            || bond.collateral < params.min_collateral_sompi()
        {
            return Err("constraint stop current filer authority/signature invalid".into());
        }
        let signed_context = intent.signing_message(rules.network_domain)?;
        if !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
            &bond.pubkey,
            signed_context.as_byte_slice(),
            &intent.signature,
            CONSTRAINT_STOP_SIGNATURE_CONTEXT_V1,
        ) {
            return Err("constraint stop current filer authority/signature invalid".into());
        }
        let record = state.tir_class_v1(&job.class_id).ok_or("constraint stop actual registered class missing")?;
        let view =
            crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, frame.descriptor_v1().maximum_leaves)?;
        let table = semantics.token_table_root.ok_or("constraint stop table missing")?;
        let rooted = self.ids.map(|ids| palw_rooted_claim_tokenizer_v1(state, intent.claim, &source, ids)).transpose()?.flatten();
        let rooted_material = rooted.as_ref().map(|tokenizer| RootedStopMaterialV1::new(material, tokenizer)).transpose()?;
        if rooted_material.is_none()
            && (!material.eog.verifies(&table, view.vocab_size)
                || material.renderings.iter().any(|r| !r.verifies(&table, view.vocab_size)))
        {
            return Err("constraint stop actual material opening mismatch".into());
        }
        let ids = self.ids.ok_or("constraint stop source-authenticated FullIds missing")?;
        let base = Court::new_from_tiled_ids_with_original_stop_context_v1(
            intent.claim,
            intent.filer,
            c.original_accepted_daa,
            &view,
            &source,
            ids,
            original,
            u32::try_from(material.eog.ids.len()).map_err(|_| "constraint stop EOG count overflow")?,
            &material.eog.opening,
            signed_context,
        )?;
        let replayed = if material.renderings.is_empty() {
            let eog = rooted_material.as_ref().map_or(material.eog.ids.as_slice(), |rooted| rooted.eog);
            replay::build_eog_stop_v1(base, eog, budget)
        } else {
            match rooted_material {
                Some(material) => replay::build(&base, &material, budget),
                None => replay::build(&base, &material, budget),
            }
        };
        replayed.map_err(|e| format!("constraint stop replay: {e:?}"))
    }
}
