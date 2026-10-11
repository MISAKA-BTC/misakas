//! D7 held tokenizer authentication and a bounded direct semantic court. Oversized files
//! remain pending for the staged route; missing/wrong local model material is never guilt.
use std::io::Read;
use kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3;
use kaspa_consensus_core::palw_fp_constraint_job_v1::{PalwTokenTableV1, PalwTokenTableOpeningV1, PALW_TOKEN_TABLE_MAX_LEAVES_V1, table_leaf_hash_v1, table_eog_leaf_hash_v1};
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_tokenizer_table_fault_v1};
use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, step_opening_from_leaf_source_capped_v1, step_opening_root_capped_v1};
use kaspa_consensus_core::palw_tokenizer_rendering_v1::{PalwQwenRenderingMapV1, palw_tokenizer_raw_commitment_v1};

pub(super) struct PalwTokenizerCheckV1 {
    pub fault: Option<PalwPanelContradictionV1>,
    pub _reservation: crate::palw_memory_ledger::PalwMemoryReservationV1,
}

/// Run off the service loop. The memory ticket precedes file allocation and remains with
/// the result until its evidence is copied into the independently budgeted reporter queue.
pub(super) fn check_held_tokenizer_v1(
    path: &std::path::Path,
    target: &PalwOffenceTargetV1,
    binding: &PalwStepBindingV2,
    job: &PalwFreePromptJobV3,
    table: &PalwTokenTableV1,
    maximum_evidence: usize,
) -> Result<PalwTokenizerCheckV1, String> {
    let row = target.registered_semantics.as_ref().ok_or("claim has no immutable tokenizer statement")?;
    if row.rendering_rule.is_none() || !row.matches_context(&binding.job_context) {
        return Err("claim has no applicable prospective rendering rule".into());
    }
    let table_root = row.token_table_root.ok_or("claim has no immutable table root")?;
    if table.entries.len() != binding.shape_profile.vocab_size as usize || table.entries.is_empty() || table.tokenizer_id != row.tokenizer_id {
        return Err("held table does not match the registered vocabulary/tokenizer".into());
    }
    let file = std::fs::File::open(path).map_err(|e| format!("held tokenizer unavailable: {e}"))?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length > maximum_evidence as u64 || maximum_evidence as u64 > kaspa_consensus_core::palw_terminal_carriage_v1::PALW_TERMINAL_MAX_BYTES_V1 {
        return Err("held tokenizer needs the staged authenticated court".into());
    }
    let need = length.checked_mul(128).and_then(|n| n.checked_add(maximum_evidence as u64 * 2 + 16_384)).ok_or("tokenizer scratch bound overflow")?;
    let reserved = crate::palw_memory_ledger::host_ledger_v1().reserve(
        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role: "public-tokenizer-proof", class_id: target.class_id, job: target.execution_root },
        need,
    ).map_err(|e| format!("tokenizer check waits for memory: {e}"))?;
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(length + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 != length || palw_tokenizer_raw_commitment_v1(&bytes) != row.tokenizer_id {
        return Err("held tokenizer does not authenticate to the registered raw file".into());
    }
    let map = PalwQwenRenderingMapV1::parse(&bytes);
    let mismatch = match &map {
        Ok(map) => table.entries.iter().enumerate().position(|(id, entry)| map.rendering(id as u32).as_ref() != entry.as_ref()),
        Err(_) => Some(0),
    };
    drop(map);
    let id = mismatch.unwrap_or(0) as u32;
    let rendering_length = table.entries[id as usize].as_ref().map_or(0, Vec::len);
    if rendering_length.saturating_add(bytes.len()).saturating_add(borsh::to_vec(&(binding, job)).map_err(|e| e.to_string())?.len()).saturating_add(4096) > maximum_evidence {
        return Err("tokenizer mismatch rendering needs the staged authenticated court".into());
    }
    let count = table.entries.len() as u64 + 1;
    let opening = step_opening_from_leaf_source_capped_v1(count, u64::from(id), PALW_TOKEN_TABLE_MAX_LEAVES_V1, |j| {
        if j == table.entries.len() as u64 { table_eog_leaf_hash_v1(&table.eog_token_ids) }
        else { table_leaf_hash_v1(j as u32, table.entries[j as usize].as_deref()) }
    }).map_err(|e| e.to_string())?;
    if step_opening_root_capped_v1(count, &opening, PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e| e.to_string())? != table_root {
        return Err("held table bytes do not authenticate to the claim-stamped root".into());
    }
    let table_opening = PalwTokenTableOpeningV1 { id, rendering: table.entries[id as usize].clone(), opening };
    let fault = if palw_tokenizer_table_fault_v1(target, binding, job, &bytes, &table_opening).map_err(|e| e.to_string())? {
        Some(PalwPanelContradictionV1::TokenizerTableMismatch { binding: binding.clone(), job: Box::new(job.clone()), tokenizer_bytes: bytes, table_opening })
    } else { None };
    Ok(PalwTokenizerCheckV1 { fault, _reservation: reserved })
}

#[cfg(test)]
#[path = "palw_filer_tokenizer_tests.rs"]
mod tests;
