//! Bounded derivation of the immutable rendering facts for a prospective
//! tokenizer check. This is a read-only verifier, with no accepted state or
//! clock effect. EOG comes from the registered table, independently of JSON.

use crate::{
    Hash64,
    palw_fp_constraint_job_v1::{PALW_TOKEN_TABLE_MAX_LEAVES_V1, PalwTokenTableEogOpeningV1, table_leaf_hash_v1},
    palw_step_leg::PalwStepPrefixStreamV1,
    palw_tokenizer_rendering_v1::{
        PalwQwenRenderingMapV1, PalwTokenizerRenderingRuleV1, palw_tokenizer_raw_commitment_v1, palw_unique_json_v1,
        tokenizer_key_events_from_unique_json,
    },
};

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
}
