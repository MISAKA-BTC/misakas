//! Private ADR-0199 A Tokenizer material builder, separate from numeric and
//! Constraint intents. No owned certificate, paid ingress, public material
//! retrieval, old move equivalence or complete-original authority follows.
use super::SharedTirBuilderStoreV1;
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_fp_constraint_job_v1::PalwTokenTableOpeningV1;
use crate::palw_json_key_set_v1::PalwJsonKeyCompactProofV1;
use crate::palw_tokenizer_table_stream_v1::{canonical_replay as replay, PalwTokenizerTableStreamV1 as Court};
use crate::{
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2},
    Hash64,
};
pub(crate) const TOKENIZER_INTENT_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/shared-tokenizer-terminal/v1/mldsa87";
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) struct SignedTokenizerTerminalIntentV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub policy_id: Hash64,
    pub semantics_id: Hash64,
    pub inventory_id: Hash64,
    pub material_id: Hash64,
    pub token_id: u32,
    pub filer: PalwBondKeyV2,
    pub signature: Vec<u8>,
}
impl SignedTokenizerTerminalIntentV1 {
    pub(crate) fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 {
            return Err("tokenizer terminal intent version".into());
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/shared-tokenizer-terminal/v1",
            &(
                network,
                self.version,
                self.claim,
                self.scope_id,
                self.policy_id,
                self.semantics_id,
                self.inventory_id,
                self.material_id,
                self.token_id,
                self.filer,
            ),
        )
    }
}
/// Borrowed complete operands, not a delivery/custody credential. Raw/proof
/// acquisition is absent from the current seven-family shared source universe.
#[derive(Clone, Copy)]
pub(crate) struct TokenizerMaterialV1<'a> {
    pub raw: &'a [u8],
    pub proofs: &'a [PalwJsonKeyCompactProofV1],
    pub rendering: &'a PalwTokenTableOpeningV1,
}
impl TokenizerMaterialV1<'_> {
    pub(crate) fn identity(&self, budget: replay::Budget) -> Result<Hash64, String> {
        if self.proofs.len() as u64 > budget.max_proofs
            || self.proofs.iter().any(|p| p.siblings.len() > crate::palw_json_key_set_v1::PALW_JSON_KEY_SET_DEPTH_V1)
        {
            return Err("tokenizer material proof budget".into());
        }
        let bytes = borsh::object_length(&(self.raw, self.proofs, self.rendering)).map_err(|e| e.to_string())?;
        if bytes as u64 > budget.max_material_bytes {
            return Err("tokenizer material byte budget".into());
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/shared-tokenizer-material/v1",
            &(self.raw, self.proofs, self.rendering),
        )
    }
}
/// Cached finding is not authority: recheck reconstructs the initial native
/// contract from actual retained original/semantics/EOF and verifies all bytes.
pub(crate) struct TokenizerTerminalEvidenceV1<'a> {
    intent: SignedTokenizerTerminalIntentV1,
    material: TokenizerMaterialV1<'a>,
    result: (bool, Hash64, u64),
}
impl TokenizerTerminalEvidenceV1<'_> {
    pub(crate) fn recheck(
        &self,
        store: &SharedTirBuilderStoreV1<'_>,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        budget: replay::Budget,
    ) -> Result<bool, String> {
        let fresh = store.replay_tokenizer_terminal(state, params, rules, &self.intent, self.material, budget)?;
        if (fresh.violation(), fresh.key_root(), fresh.key_events()) != self.result {
            return Err("tokenizer terminal reconstruction mismatch".into());
        }
        Ok(fresh.violation())
    }
}
impl SharedTirBuilderStoreV1<'_> {
    /// Private read-only builder. Native caller resolves rules/accepting point
    /// and admits source reconstruction, hashing, signature, clone/scratch and
    /// carriage separately. This work budget never grants a paid permit.
    pub(crate) fn build_tokenizer_terminal<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedTokenizerTerminalIntentV1,
        material: TokenizerMaterialV1<'a>,
        budget: replay::Budget,
    ) -> Result<TokenizerTerminalEvidenceV1<'a>, String> {
        let found = self.replay_tokenizer_terminal(state, params, rules, intent, material, budget)?;
        let evidence = TokenizerTerminalEvidenceV1 {
            intent: intent.clone(),
            material,
            result: (found.violation(), found.key_root(), found.key_events()),
        };
        evidence.recheck(self, state, params, rules, budget)?;
        Ok(evidence)
    }
    fn replay_tokenizer_terminal(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedTokenizerTerminalIntentV1,
        material: TokenizerMaterialV1<'_>,
        budget: replay::Budget,
    ) -> Result<replay::Finding, String> {
        let c = crate::palw_court_shared_tir_state_v1::context(state, intent.claim)?;
        let source = c.source(state)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, intent.claim)
            .ok_or("tokenizer terminal original frame missing")?;
        if intent.scope_id != c.scope_id
            || self.source.source_id_v1() != source.source_id_v1()
            || !rules.tir_active
            || rules.at.daa_score < c.original_accepted_daa
            || rules.at.daa_score >= c.hard_daa
            || rules.step_ladder != frame.plan_v1().owner_work.rules.max_step_leaf_count
            || intent.policy_id != crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
        {
            return Err("tokenizer terminal original/scope/policy/interval mismatch".into());
        }
        let header = source.header_v1();
        let job = &frame.original_v1().job;
        let record = state.tir_class_v1(&job.class_id).ok_or("tokenizer terminal registered class missing")?;
        let view =
            crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, frame.descriptor_v1().maximum_leaves)?;
        let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(state, &intent.claim)
            .ok_or("tokenizer terminal actual offence target missing")?;
        let semantics = crate::palw_tokenizer_table_stream_v1::palw_tokenizer_statement_view_v1(&target, &view, job)?;
        if intent.semantics_id != semantics.commitment()
            || semantics.bound_from_daa > c.original_accepted_daa
            || !semantics.well_formed_for(&job.class_id, &header.artifact_root)
        {
            return Err("tokenizer terminal immutable semantics mismatch".into());
        }
        let eof =
            state.staged_tokenizer_inventory_v1(semantics.tokenizer_id).ok_or("tokenizer terminal accepted EOF inventory missing")?;
        eof.check_v1()?;
        if eof.artifact_hash != semantics.tokenizer_id
            || eof.inventory_id != intent.inventory_id
            || eof.completed_daa > c.original_accepted_daa
            || eof.actual_bytes != material.raw.len() as u64
        {
            return Err("tokenizer terminal EOF scope/length/time mismatch".into());
        }
        if intent.material_id != material.identity(budget)? || material.rendering.id != intent.token_id {
            return Err("tokenizer terminal material/target identity mismatch".into());
        }
        let bond = state.bond(&intent.filer).ok_or("tokenizer terminal filer missing")?;
        if intent.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || !matches!(bond.status, crate::palw_state_v2::PalwBondStatusV2::Active)
            || bond.collateral < params.min_collateral_sompi()
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                &bond.pubkey,
                intent.signing_message(rules.network_domain)?.as_byte_slice(),
                &intent.signature,
                TOKENIZER_INTENT_SIGNATURE_CONTEXT_V1,
            )
        {
            return Err("tokenizer terminal filer current authority/signature invalid".into());
        }
        let length = u32::try_from(material.rendering.rendering.as_ref().map_or(0, Vec::len))
            .map_err(|_| "tokenizer rendering length overflow")?;
        let base = Court::new_with_operand_length_view_v1(
            intent.signing_message(rules.network_domain)?,
            &target,
            &view,
            job,
            intent.token_id,
            material.rendering.rendering.is_some(),
            length,
            &material.rendering.opening,
            eof.actual_bytes,
        )?;
        replay::build(
            &base,
            replay::Material { raw: material.raw, proofs: material.proofs, rendering: material.rendering.rendering.as_deref() },
            budget,
        )
        .map_err(|e| format!("tokenizer terminal replay: {e:?}"))
    }
}
