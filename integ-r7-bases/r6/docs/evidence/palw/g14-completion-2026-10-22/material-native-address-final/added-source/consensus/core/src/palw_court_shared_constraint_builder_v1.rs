//! ADR-0199 A private Constraint material -> native primitive evidence candidate.
//! No rooted owned certificate, accepting entry point, signed move, paid ingress,
//! deadline extension, custody/retention receipt or complete-original authority.
use super::SharedTirBuilderStoreV1;
use crate::palw_constraint_stream_court_v1::canonical_replay as replay;
use crate::palw_constraint_stream_court_v1::{PalwConstraintStreamCourtV1 as Court, PalwConstraintStreamFaultV1};
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_fp_constraint_job_v1::{PalwTokenTableEogOpeningV1, PalwTokenTableOpeningV1};
use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
use crate::{
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2},
    Hash64,
};

pub(crate) const CONSTRAINT_INTENT_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/shared-constraint-terminal/v1/mldsa87";
/// Distinct from the numerical terminal and old owner-slot intent. No decoder.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) struct SignedConstraintTerminalIntentV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub policy_id: Hash64,
    pub semantics_id: Hash64,
    pub material_id: Hash64,
    pub position: u32,
    pub beat_lane: u32,
    pub witness: Option<u32>,
    pub filer: PalwBondKeyV2,
    pub signature: Vec<u8>,
}
impl SignedConstraintTerminalIntentV1 {
    pub(crate) fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 {
            return Err("constraint terminal intent version".into());
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/shared-constraint-terminal/v1",
            &(
                network,
                self.version,
                self.claim,
                self.scope_id,
                self.policy_id,
                self.semantics_id,
                self.material_id,
                self.position,
                self.beat_lane,
                self.witness,
                self.filer,
            ),
        )
    }
}
/// Complete borrowed items; None rendering bytes is authenticated absence, not
/// missing input. Their proofs are rechecked by the existing native primitive.
#[derive(Clone, Copy)]
pub(crate) struct ConstraintMaterialV1<'a> {
    pub eog: &'a PalwTokenTableEogOpeningV1,
    pub renderings: &'a [PalwTokenTableOpeningV1],
}
impl replay::Material for ConstraintMaterialV1<'_> {
    fn eog(&self) -> Option<&[u32]> {
        Some(&self.eog.ids)
    }
    fn rendering(&self, id: u32) -> Option<replay::RenderingMaterial<'_>> {
        let i = self.renderings.binary_search_by_key(&id, |o| o.id).ok()?;
        let o = &self.renderings[i];
        Some(replay::RenderingMaterial { opening: &o.opening, bytes: o.rendering.as_deref() })
    }
}
impl ConstraintMaterialV1<'_> {
    /// Streaming bounded identity, without first serializing an unbounded Vec.
    /// This is not authentication; build/recheck must verify the actual proofs.
    pub(crate) fn identity(&self, budget: replay::WorkBudget) -> Result<Hash64, String> {
        if self.renderings.windows(2).any(|w| w[0].id >= w[1].id) {
            return Err("constraint material rendering order/duplicate".into());
        }
        let bytes = borsh::object_length(&(self.eog, self.renderings)).map_err(|e| e.to_string())?;
        if u64::try_from(bytes).map_err(|_| "constraint material size overflow")? > budget.max_material_bytes {
            return Err("constraint material input budget".into());
        }
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/shared-constraint-material/v1",
            &(self.eog, self.renderings),
        )
    }
}
/// Opaque, borrowed local evidence. It cannot be deserialized or converted to
/// an old rooted certificate. Inspection first reconstructs and verifies from
/// current retained state; the builder's cached finding is never authority.
pub(crate) struct ConstraintTerminalEvidenceV1<'a> {
    intent: SignedConstraintTerminalIntentV1,
    material: ConstraintMaterialV1<'a>,
    identity: (Hash64, Hash64, Hash64, Hash64),
    finding: Option<PalwConstraintStreamFaultV1>,
}
impl ConstraintTerminalEvidenceV1<'_> {
    pub(crate) fn recheck(
        &self,
        store: &SharedTirBuilderStoreV1<'_>,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        budget: replay::WorkBudget,
    ) -> Result<Option<PalwConstraintStreamFaultV1>, String> {
        let fresh = store.replay_constraint_terminal(state, params, rules, &self.intent, self.material, budget)?;
        if fresh.identity() != self.identity || fresh.finding() != self.finding.as_ref() {
            return Err("constraint terminal evidence reconstruction mismatch".into());
        }
        Ok(fresh.finding().cloned())
    }
}
impl SharedTirBuilderStoreV1<'_> {
    /// Real material-builder entry on the existing authenticated source store.
    /// `rules` is the native caller's resolved rules, not an ingress DTO. The
    /// eventual coordinator must still authenticate its accepting point and
    /// reserve reconstruction, signature, clone/scratch, carriage and retention.
    /// WorkBudget covers primitive input/work only; it is never a paid permit.
    pub(crate) fn build_constraint_terminal<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedConstraintTerminalIntentV1,
        material: ConstraintMaterialV1<'a>,
        budget: replay::WorkBudget,
    ) -> Result<ConstraintTerminalEvidenceV1<'a>, String> {
        let result = self.replay_constraint_terminal(state, params, rules, intent, material, budget)?;
        let evidence = ConstraintTerminalEvidenceV1 {
            intent: intent.clone(),
            material,
            identity: result.identity(),
            finding: result.finding().cloned(),
        };
        // Independent reconstruction, not acceptance of a serialized result bit.
        evidence.recheck(self, state, params, rules, budget)?;
        Ok(evidence)
    }
    fn replay_constraint_terminal(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedConstraintTerminalIntentV1,
        material: ConstraintMaterialV1<'_>,
        budget: replay::WorkBudget,
    ) -> Result<replay::Candidate, String> {
        let c = crate::palw_court_shared_tir_state_v1::context(state, intent.claim)?;
        let source = c.source(state)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, intent.claim)
            .ok_or("constraint terminal original frame absent")?;
        if intent.scope_id != c.scope_id
            || self.source.source_id_v1() != source.source_id_v1()
            || !rules.tir_active
            || !rules.constraint_active
            || rules.at.daa_score < c.original_accepted_daa
            || rules.at.daa_score >= c.hard_daa
            || rules.step_ladder != frame.plan_v1().owner_work.rules.max_step_leaf_count
            || intent.policy_id != crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
        {
            return Err("constraint terminal original/scope/native policy/interval mismatch".into());
        }
        let semantics = state.claim_semantics_v1(&intent.claim).ok_or("constraint terminal registered semantics absent")?;
        let header = source.header_v1();
        let job = &frame.original_v1().job;
        if !semantics.matches_context(&header.job_context)
            || !semantics.well_formed_for(&job.class_id, &header.artifact_root)
            || semantics.bound_from_daa > c.original_accepted_daa
            || intent.semantics_id != semantics.commitment()
            || semantics.token_table_root.is_none()
            || semantics.token_table_root != crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
        {
            return Err("constraint terminal lacks matching immutable table semantics".into());
        }
        if intent.material_id != material.identity(budget)? {
            return Err("constraint terminal material identity mismatch".into());
        }
        let bond = state.bond(&intent.filer).ok_or("constraint terminal filer absent")?;
        if intent.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || !matches!(bond.status, crate::palw_state_v2::PalwBondStatusV2::Active)
            || bond.collateral < params.min_collateral_sompi()
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                &bond.pubkey,
                intent.signing_message(rules.network_domain)?.as_byte_slice(),
                &intent.signature,
                CONSTRAINT_INTENT_SIGNATURE_CONTEXT_V1,
            )
        {
            return Err("constraint terminal filer current authority/signature invalid".into());
        }
        let record = state.tir_class_v1(&job.class_id).ok_or("constraint terminal registered class absent")?;
        let view =
            crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, frame.descriptor_v1().maximum_leaves)?;
        let table = semantics.token_table_root.ok_or("constraint table absent")?;
        if !material.eog.verifies(&table, view.vocab_size) || material.renderings.iter().any(|o| !o.verifies(&table, view.vocab_size))
        {
            return Err("constraint terminal material opening authentication failed".into());
        }
        let pin =
            self.row_pin(intent.position, intent.beat_lane).ok_or("constraint terminal authenticated row/tile material missing")?;
        let view_identity = borsh::to_vec(header).map_err(|e| e.to_string())?;
        let eog_count = u32::try_from(material.eog.ids.len()).map_err(|_| "constraint EOG count overflow")?;
        let base = Court::new_from_tiled_view_v1(
            intent.claim,
            intent.filer,
            c.original_accepted_daa,
            header.committed_execution_root,
            &view,
            &view_identity,
            job,
            &pin,
            intent.witness,
            eog_count,
            &material.eog.opening,
            rules.step_ladder,
            Some(intent.signing_message(rules.network_domain)?),
        )?;
        replay::build(&base, &material, budget).map_err(|e| format!("constraint terminal replay: {e:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> replay::WorkBudget {
        replay::WorkBudget { max_material_bytes: 1 << 25, max_calls: 100_000 }
    }
    #[test]
    fn borrowed_material_replays_nonempty_native_constraint_fixture_and_rejects_corruption() {
        // Existing authenticated legacy primitive fixture, NOT TIR original admission.
        // The store's actual-state/TIR adapter still needs a constrained native frame test.
        for fault in [false, true] {
            let (binding, a) = crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1(fault);
            let owner =
                PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(2), index: 0 });
            let base = Court::new(
                Hash64::from_u64_word(1),
                owner,
                100,
                binding.committed_execution_root,
                &binding,
                &a.job,
                &a.pin,
                a.witness,
                a.eog.ids.len() as u32,
                &a.eog.opening,
                1 << 25,
            )
            .unwrap();
            let saved = borsh::to_vec(&base).unwrap();
            assert!(!a.renderings.is_empty());
            let mut renderings = a.renderings.clone();
            renderings.sort_by_key(|o| o.id);
            renderings.dedup_by_key(|o| o.id);
            let m = ConstraintMaterialV1 { eog: &a.eog, renderings: &renderings };
            let id = m.identity(budget()).unwrap();
            let result = replay::build(&base, &m, budget()).unwrap();
            assert_eq!(result.finding().is_some(), fault);
            assert!(result.work().material_bytes > 0 && result.work().calls > 0);
            assert_eq!(borsh::to_vec(&base).unwrap(), saved);
            let mut corrupt = renderings.clone();
            let mut changed = 0;
            for bytes in corrupt.iter_mut().filter_map(|o| o.rendering.as_mut()).filter(|b| !b.is_empty()) {
                bytes[0] ^= 1;
                changed += 1;
            }
            assert!(changed > 0);
            let bad = ConstraintMaterialV1 { eog: &a.eog, renderings: &corrupt };
            assert_ne!(bad.identity(budget()).unwrap(), id);
            assert!(matches!(replay::build(&base, &bad, budget()), Err(replay::Refusal::Native(_))));
            let missing = ConstraintMaterialV1 { eog: &a.eog, renderings: &[] };
            assert!(matches!(replay::build(&base, &missing, budget()), Err(replay::Refusal::NeedRendering(_))));
            let w = result.work();
            assert!(matches!(
                replay::build(&base, &m, replay::WorkBudget { max_calls: w.calls - 1, ..budget() }),
                Err(replay::Refusal::Budget)
            ));
            assert_eq!(borsh::to_vec(&base).unwrap(), saved);
            let mut duplicate = renderings.clone();
            duplicate.insert(0, renderings[0].clone());
            assert!(ConstraintMaterialV1 { eog: &a.eog, renderings: &duplicate }.identity(budget()).is_err());
        }
    }
    #[test]
    fn semantic_intent_binds_domain_scope_material_selection_and_filer() {
        let h = Hash64::from_u64_word;
        let intent = SignedConstraintTerminalIntentV1 {
            version: 1,
            claim: h(1),
            scope_id: h(2),
            policy_id: h(3),
            semantics_id: h(4),
            material_id: h(5),
            position: 0,
            beat_lane: 1,
            witness: None,
            filer: PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(2),
                index: 0,
            }),
            signature: vec![],
        };
        let id = intent.signing_message(h(7)).unwrap();
        assert_ne!(id, intent.signing_message(h(8)).unwrap());
        let mut changes = Vec::new();
        let mut x = intent.clone();
        x.scope_id = h(9);
        changes.push(x);
        let mut x = intent.clone();
        x.material_id = h(9);
        changes.push(x);
        let mut x = intent.clone();
        x.semantics_id = h(9);
        changes.push(x);
        let mut x = intent.clone();
        x.position = 1;
        changes.push(x);
        let mut x = intent.clone();
        x.witness = Some(1);
        changes.push(x);
        let mut x = intent.clone();
        x.filer.0.index = 1;
        changes.push(x);
        for x in changes {
            assert_ne!(id, x.signing_message(h(7)).unwrap());
        }
        let mut x = intent;
        x.version = 2;
        assert!(x.signing_message(h(7)).is_err());
        assert_ne!(CONSTRAINT_INTENT_SIGNATURE_CONTEXT_V1, super::super::SHARED_TERMINAL_INTENT_CONTEXT_V1);
    }
}
