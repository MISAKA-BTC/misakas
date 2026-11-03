//! Bounded derivation of the immutable rendering facts for a prospective
//! tokenizer check. This is a read-only verifier, with no accepted state or
//! clock effect. EOG comes from the registered table, independently of JSON.

use crate::{
    Hash64,
    palw_class_semantics_v1::PalwClassSemanticsV1,
    palw_fp_constraint_job_v1::{
        PALW_TOKEN_TABLE_MAX_LEAVES_V1, PalwTokenTableEogOpeningV1, PalwTokenTableOpeningV1, palw_fp_constraint_table_root_v1,
        table_leaf_hash_v1,
    },
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1, PalwStagedCourtRowV1},
    palw_state_v2::PalwChainStateV2,
    palw_step_leg::PalwStepPrefixStreamV1,
    palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1,
    palw_tokenizer_rendering_v1::{
        PalwQwenRenderingMapV1, PalwTokenizerRenderingRuleV1, palw_tokenizer_raw_commitment_v1, palw_unique_json_v1,
        tokenizer_key_events_from_unique_json,
    },
};
use kaspa_core::memory_budget::{MemoryBudget, MemoryClass, global_memory_budget};
use std::sync::Arc;

/// The caller supplies the immutable tokenizer identity and registered table
/// context, independently of the submitted raw bytes and EOG opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTokenizerRenderingStatementV2 {
    pub tokenizer_id: Hash64,
    pub exact_raw_bytes: u64,
    pub table_root: Hash64,
    pub vocab: u32,
    pub rule: PalwTokenizerRenderingRuleV1,
}

/// Local work limits for this single pass. A native caller must also budget
/// the JSON parser's transient allocations and reserve its processing slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTokenizerRenderingBudgetV2 {
    pub raw_bytes: u64,
    pub vocab: u32,
    pub key_events: u64,
    pub eog_ids: u32,
    pub total_rendering_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PalwDerivedTokenizerRenderingV2 {
    pub statement: PalwTokenizerRenderingStatementV2,
    pub key_events: u64,
    pub maximum_rendering_bytes: u64,
    pub total_rendering_bytes: u64,
    pub eog_leaf: Hash64,
    pub lowest_eog_id: u32,
}

/// Rebuild all token leaves in ID order against the original promote-odd
/// tree. The authenticated last (EOG) opening lets the checker retain only
/// a Merkle frontier, without storing the table or allocating for sparse IDs.
pub fn derive_tokenizer_rendering_v2(
    statement: PalwTokenizerRenderingStatementV2,
    raw: &[u8],
    eog: &PalwTokenTableEogOpeningV1,
    budget: PalwTokenizerRenderingBudgetV2,
) -> Result<PalwDerivedTokenizerRenderingV2, String> {
    check_operands(statement, raw, eog, budget)?;
    let leaf_count = u64::from(statement.vocab) + 1;
    let lowest_eog_id = eog.lowest().ok_or("tokenizer table has no EOG")?;
    if eog.ids.iter().any(|id| *id >= statement.vocab) || !eog.verifies(&statement.table_root, statement.vocab) {
        return Err("tokenizer EOG does not open the registered table".into());
    }
    if palw_tokenizer_raw_commitment_v1(raw) != statement.tokenizer_id {
        return Err("tokenizer does not open the exact raw commitment".into());
    }

    let json = palw_unique_json_v1(raw)?;
    let key_events = tokenizer_key_events_from_unique_json(&json)?;
    if key_events > budget.key_events {
        return Err("tokenizer exceeds its key event limit".into());
    }
    let rendering = match statement.rule {
        PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1 => PalwQwenRenderingMapV1::from_unique_json(&json)?,
    };
    drop(json);
    if rendering.highest_id() >= statement.vocab {
        return Err("tokenizer names a token outside the registered vocabulary".into());
    }

    let mut tree = PalwStepPrefixStreamV1::new_capped_v1(leaf_count, &eog.opening, &[], PALW_TOKEN_TABLE_MAX_LEAVES_V1)
        .map_err(|err| err.to_string())?;
    let mut maximum_rendering_bytes = 0;
    let mut total_rendering_bytes = 0u64;
    for id in 0..statement.vocab {
        let entry = rendering.rendering(id);
        let len = entry.as_ref().map_or(0, |bytes| bytes.len() as u64);
        if len > u64::from(u32::MAX) {
            return Err("tokenizer rendering exceeds the original table format".into());
        }
        maximum_rendering_bytes = maximum_rendering_bytes.max(len);
        total_rendering_bytes = total_rendering_bytes
            .checked_add(len)
            .filter(|total| *total <= budget.total_rendering_bytes)
            .ok_or("tokenizer exceeds its total rendering byte limit")?;
        tree.push(table_leaf_hash_v1(id, entry.as_deref())).map_err(|err| err.to_string())?;
    }
    let (root, _) = tree.finish().map_err(|err| err.to_string())?;
    if root != statement.table_root {
        return Err("derived token renderings differ from the registered table".into());
    }
    Ok(PalwDerivedTokenizerRenderingV2 {
        statement,
        key_events,
        maximum_rendering_bytes,
        total_rendering_bytes,
        eog_leaf: eog.opening.leaf_hash,
        lowest_eog_id,
    })
}

fn check_operands(
    statement: PalwTokenizerRenderingStatementV2,
    raw: &[u8],
    eog: &PalwTokenTableEogOpeningV1,
    budget: PalwTokenizerRenderingBudgetV2,
) -> Result<(), String> {
    let leaf_count = u64::from(statement.vocab) + 1;
    if statement.exact_raw_bytes == 0
        || statement.exact_raw_bytes != raw.len() as u64
        || statement.exact_raw_bytes > budget.raw_bytes
        || statement.vocab == 0
        || statement.vocab > budget.vocab
        || leaf_count > PALW_TOKEN_TABLE_MAX_LEAVES_V1
        || eog.ids.len() > budget.eog_ids as usize
        || eog.ids.len() > statement.vocab as usize
    {
        return Err("tokenizer rendering operands exceed their exact length or work limits".into());
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
struct PreparedTokenizerIdentityV2 {
    semantics: Hash64,
    graph_ir_root: Hash64,
    layout_digest: Hash64,
    eof: PalwTokenizerArtifactInventoryCertificateV1,
    declared_key_events: u64,
    statement: PalwTokenizerRenderingStatementV2,
}

impl PreparedTokenizerIdentityV2 {
    fn resolve(
        state: &PalwChainStateV2,
        class_id: &Hash64,
        artifact_root: &Hash64,
        semantics: &PalwClassSemanticsV1,
        applicable_daa: u64,
    ) -> Option<Self> {
        let record = state.tir_class_v1(class_id)?;
        let eof = state.staged_tokenizer_inventory_v1(semantics.tokenizer_id)?;
        if !semantics.well_formed_for(class_id, artifact_root)
            || semantics.bound_from_daa > applicable_daa
            || record.facts.class_id != *class_id
            || record.tokenizer_id != semantics.tokenizer_id
            || eof.check_v1().is_err()
            || eof.artifact_hash != semantics.tokenizer_id
            || eof.completed_daa > applicable_daa
            || semantics.raw_tokenizer_bytes? != eof.actual_bytes
        {
            return None;
        }
        Some(Self {
            semantics: semantics.commitment(),
            graph_ir_root: record.graph_ir_root,
            layout_digest: record.layout_digest,
            eof: eof.clone(),
            declared_key_events: semantics.tokenizer_key_events?,
            statement: PalwTokenizerRenderingStatementV2 {
                tokenizer_id: record.tokenizer_id,
                exact_raw_bytes: eof.actual_bytes,
                table_root: semantics.token_table_root?,
                vocab: record.logits_vocab,
                rule: semantics.rendering_rule?,
            },
        })
    }
}

struct PreparedTokenizerRenderingV2 {
    identity: PreparedTokenizerIdentityV2,
    facts: PalwDerivedTokenizerRenderingV2,
}

/// One process-local entry per instance for prospective TIR checks. Empty on restart.
/// Callers supply their current candidate-scoped state at each use. A reorg
/// can reuse the entry only if that state still resolves the exact same
/// registration and accepted EOF. No raw data or full table is retained.
/// This cache has no consensus, admission, scheduling or fee authority.
#[derive(Default)]
pub struct PalwTokenizerRenderingCacheV2 {
    prepared: Option<PreparedTokenizerRenderingV2>,
}

impl PalwTokenizerRenderingCacheV2 {
    fn class_identity(parent: &PalwChainStateV2, class_id: &Hash64, candidate_daa: u64) -> Option<PreparedTokenizerIdentityV2> {
        PreparedTokenizerIdentityV2::resolve(
            parent,
            class_id,
            &parent.class(class_id)?.artifact_root,
            parent.class_semantics_v1(class_id)?,
            candidate_daa,
        )
    }

    /// Check the current parent before acquiring the original material.
    pub fn class_is_prepared_v2(&self, parent: &PalwChainStateV2, class_id: &Hash64, candidate_daa: u64) -> bool {
        self.prepared.as_ref().is_some_and(|entry| {
            Self::class_identity(parent, class_id, candidate_daa).is_some_and(|identity| entry.identity == identity)
        })
    }

    /// Prepare a class before a prospective claim. On a matching cache hit,
    /// `raw` and `eog` are unused. Otherwise they are locally bounded inputs,
    /// and the checker reserves the extra parser scratch before hashing or
    /// parsing. The caller owns the budgets for acquisition and scheduling.
    pub fn prepare_class_v2(
        &mut self,
        parent: &PalwChainStateV2,
        class_id: &Hash64,
        candidate_daa: u64,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
        budget: PalwTokenizerRenderingBudgetV2,
    ) -> Result<(), String> {
        let identity = Self::class_identity(parent, class_id, candidate_daa)
            .ok_or("parent state has no applicable tokenizer registration and exact EOF")?;
        self.prepare(identity, raw, eog, budget, global_memory_budget())
    }

    fn prepare(
        &mut self,
        identity: PreparedTokenizerIdentityV2,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
        mut budget: PalwTokenizerRenderingBudgetV2,
        memory: &Arc<MemoryBudget>,
    ) -> Result<(), String> {
        if self.prepared.as_ref().is_some_and(|entry| entry.identity == identity) {
            return Ok(());
        }
        let statement = identity.statement;
        budget.key_events = budget.key_events.min(identity.declared_key_events);
        check_operands(statement, raw, eog, budget)?;
        // Same conservative parser multiplier as the existing Node tokenizer
        // check. Includes separate EOG-sort scratch and bounded tree overhead.
        let scratch = statement
            .exact_raw_bytes
            .checked_mul(128)
            .and_then(|bytes| bytes.checked_add(4 * eog.ids.len() as u64 + 16_384))
            .ok_or("tokenizer scratch estimate overflow")?;
        let _lease = memory.try_reserve(MemoryClass::Native, scratch).map_err(|e| e.to_string())?;
        let facts = derive_tokenizer_rendering_v2(statement, raw, eog, budget)?;
        self.prepared = Some(PreparedTokenizerRenderingV2 { identity, facts });
        Ok(())
    }

    fn for_claim(&self, state: &PalwChainStateV2, claim_id: &Hash64) -> Option<&PreparedTokenizerRenderingV2> {
        let prepared = self.prepared.as_ref()?;
        let claim = state.claim(claim_id)?;
        let semantics = state.claim_semantics_v1(claim_id)?;
        let PalwStagedCourtRowV1::TirClaim(frame) = state.staged_court_row_v1(&PalwStagedCourtKeyV1::TirClaim(*claim_id))? else {
            return None;
        };
        let original = frame.original_v1();
        let header = frame.header_v1();
        if frame.plan_v1().original_claim != *claim_id
            || frame.plan_v1().terms.accepted_daa != claim.accepted_daa
            || frame.descriptor_v1().layout_digest != prepared.identity.layout_digest
            || original.execution_root != claim.execution_root
            || original.output_root != claim.output_root
            || !semantics.matches_context(&header.job_context)
            || palw_fp_constraint_table_root_v1(&original.job).is_some_and(|root| Some(root) != semantics.token_table_root)
        {
            return None;
        }
        let identity =
            PreparedTokenizerIdentityV2::resolve(state, &claim.class_id, &header.artifact_root, semantics, claim.accepted_daa)?;
        // The EOF row has a DAA but no intra-DAA ordering pin. Do not
        // retroactively qualify claims accepted at that same DAA.
        (identity.eof.completed_daa < claim.accepted_daa && prepared.identity == identity).then_some(prepared)
    }

    /// `None` means no applicable local result. The caller must derive
    /// `expected_id` from the authenticated claim trace and still check the
    /// claim's constraint and output bindings.
    pub fn verify_claim_token_v2(
        &self,
        state: &PalwChainStateV2,
        claim_id: &Hash64,
        expected_id: u32,
        opening: &PalwTokenTableOpeningV1,
    ) -> Option<bool> {
        Some(self.for_claim(state, claim_id)?.verifies_token(expected_id, opening))
    }

    /// Read the authenticated EOG fallback for this claim without processing
    /// the JSON or opening the EOG list again.
    pub fn lowest_eog_for_claim_v2(&self, state: &PalwChainStateV2, claim_id: &Hash64) -> Option<u32> {
        Some(self.for_claim(state, claim_id)?.facts.lowest_eog_id)
    }
}

impl PreparedTokenizerRenderingV2 {
    fn verifies_token(&self, expected_id: u32, opening: &PalwTokenTableOpeningV1) -> bool {
        let statement = self.facts.statement;
        opening.id == expected_id
            && opening.rendering.as_ref().map_or(0, |bytes| bytes.len() as u64) <= self.facts.maximum_rendering_bytes
            && opening.verifies(&statement.table_root, statement.vocab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_constraint_job_v1::PalwTokenTableV1;

    const RAW: &[u8] = br#"{"added_tokens":[{"id":2,"content":"long"}],"model":{"vocab":{"A":0,"b":2},"merges":[]}}"#;
    const BUDGET: PalwTokenizerRenderingBudgetV2 =
        PalwTokenizerRenderingBudgetV2 { raw_bytes: 1024, vocab: 8, key_events: 32, eog_ids: 2, total_rendering_bytes: 8 };

    fn fixture() -> (PalwTokenizerRenderingStatementV2, PalwTokenTableV1) {
        let table = PalwTokenTableV1 {
            entries: vec![Some(b"A".to_vec()), Some(vec![]), Some(b"long".to_vec()), None],
            eog_token_ids: vec![2],
            tokenizer_id: palw_tokenizer_raw_commitment_v1(RAW),
        };
        let statement = PalwTokenizerRenderingStatementV2 {
            tokenizer_id: table.tokenizer_id,
            exact_raw_bytes: RAW.len() as u64,
            table_root: table.root(),
            vocab: 4,
            rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        };
        (statement, table)
    }

    #[test]
    fn derives_full_table_bounds_with_override_hole_and_padding() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        let facts = derive_tokenizer_rendering_v2(statement, RAW, &eog, BUDGET).unwrap();
        assert_eq!(facts.statement, statement);
        assert_eq!(facts.key_events, 11);
        assert_eq!(facts.maximum_rendering_bytes, 4);
        assert_eq!(facts.total_rendering_bytes, 5);
        assert_eq!(facts.eog_leaf, eog.opening.leaf_hash);
        assert_eq!(facts.lowest_eog_id, 2);
    }

    #[test]
    fn rejects_wrong_table_even_with_its_own_valid_eog_opening() {
        let (mut statement, mut table) = fixture();
        table.entries[2] = Some(b"x".to_vec());
        let wrong = table.eog_opening().unwrap();
        assert!(derive_tokenizer_rendering_v2(statement, RAW, &wrong, BUDGET).is_err());
        statement.table_root = table.root();
        assert!(wrong.verifies(&statement.table_root, statement.vocab));
        assert!(derive_tokenizer_rendering_v2(statement, RAW, &wrong, BUDGET).is_err());
        let mut wrong_eog = wrong;
        wrong_eog.ids[0] = 1;
        assert!(derive_tokenizer_rendering_v2(statement, RAW, &wrong_eog, BUDGET).is_err());
    }

    #[test]
    fn refuses_authenticated_invalid_json_ids_and_premature_eof() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        for raw in [
            br#"{"model":{"vocab":{"a":0,"\u0061":1},"merges":[]}}"#.as_slice(),
            br#"{"model":{"vocab":{"a":0,"b":0},"merges":[]}}"#,
            br#"{"model":{"vocab":{},"merges":[]},"added_tokens":[{"id":0,"content":"a"},{"id":0,"content":"b"}]}"#,
            br#"{"model":{"vocab":{"a":4294967295},"merges":[]}}"#,
            &RAW[..RAW.len() - 1],
        ] {
            let input = PalwTokenizerRenderingStatementV2 {
                tokenizer_id: palw_tokenizer_raw_commitment_v1(raw),
                exact_raw_bytes: raw.len() as u64,
                ..statement
            };
            assert!(derive_tokenizer_rendering_v2(input, raw, &eog, BUDGET).is_err());
        }
        assert!(derive_tokenizer_rendering_v2(statement, &RAW[..RAW.len() - 1], &eog, BUDGET).is_err());
    }

    #[test]
    fn refuses_limits_below_derived_work() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        for budget in [
            PalwTokenizerRenderingBudgetV2 { raw_bytes: RAW.len() as u64 - 1, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { vocab: 3, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { key_events: 10, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { eog_ids: 0, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { total_rendering_bytes: 4, ..BUDGET },
        ] {
            assert!(derive_tokenizer_rendering_v2(statement, RAW, &eog, budget).is_err());
        }
    }

    #[test]
    fn cold_prepare_reserves_scratch_and_only_memoizes_success() {
        use crate::palw_state_v2::PalwBondKeyV2;

        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        let identity = || PreparedTokenizerIdentityV2 {
            semantics: statement.table_root,
            graph_ir_root: statement.table_root,
            layout_digest: statement.table_root,
            eof: PalwTokenizerArtifactInventoryCertificateV1 {
                artifact_hash: statement.tokenizer_id,
                actual_bytes: statement.exact_raw_bytes,
                inventory_id: statement.table_root,
                opener: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0)),
                opened_daa: 1,
                completed_daa: 2,
            },
            declared_key_events: BUDGET.key_events,
            statement,
        };
        let small = MemoryBudget::new(1, [1; 4]);
        let adequate = MemoryBudget::new(32_768, [32_768; 4]);
        let mut cache = PalwTokenizerRenderingCacheV2::default();
        assert!(cache.prepare(identity(), RAW, &eog, BUDGET, &small).is_err());
        assert!(cache.prepared.is_none());
        let mut wrong_raw = RAW.to_vec();
        wrong_raw[0] = b'x';
        assert!(cache.prepare(identity(), &wrong_raw, &eog, BUDGET, &adequate).is_err());
        assert!(cache.prepared.is_none());
        assert_eq!(adequate.snapshot().reserved_bytes, 0);
        cache.prepare(identity(), RAW, &eog, BUDGET, &adequate).unwrap();
        assert_eq!(adequate.snapshot().reserved_bytes, 0);
        cache.prepare(identity(), &[], &eog, BUDGET, &small).unwrap();
        let mut replacement = identity();
        replacement.declared_key_events = 1;
        assert!(cache.prepare(replacement, RAW, &eog, BUDGET, &adequate).is_err());
        assert_eq!(adequate.snapshot().reserved_bytes, 0);
        let verified = cache.prepared.unwrap();
        assert!(verified.verifies_token(0, &table.opening_of(0).unwrap()));
        assert!(!verified.verifies_token(1, &table.opening_of(0).unwrap()));
        let mut wrong = table.opening_of(3).unwrap();
        assert!(verified.verifies_token(3, &wrong));
        wrong.rendering = Some(vec![]);
        assert!(!verified.verifies_token(3, &wrong));
    }
}
