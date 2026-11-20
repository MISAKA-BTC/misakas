//! Owner-side checks for withdrawing an answered reservation on a plain protected
//! FP claim. The selected Panel, native courts and receipt consumers run independently.
use super::*;
use kaspa_consensus_core::{
    api::ConsensusApi,
    palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1,
    palw_freeprompt_v3::PalwFpStopReasonV3,
    palw_protected_original_readback_v1::{ProtectedFilerReservationV1, protected_filer_reservation_v1},
    palw_tokenizer_registration_v1::{TokenizerRegistrationPolicyV1, TokenizerRegistrationWireV1},
    palw_tokenizer_rendering_v2::{PalwTokenizerRenderingBudgetV2, derive_tokenizer_rendering_v2},
};
use std::{io::Read, path::Path};

pub(super) fn owner_context(
    session: &kaspa_consensusmanager::ConsensusProxy,
    claim: Hash64,
    owner: PalwBondKeyV2,
) -> Result<Option<ProtectedFilerReservationV1>, String> {
    let mut current = None;
    session.palw_tir_public_read_v1(&mut |state| {
        current = protected_filer_reservation_v1(state, claim, owner)?;
        Ok(())
    })?;
    Ok(current)
}

/// Read the EOG operand from the selected registration's accepted Finish.
/// Its opening must authenticate to the exact table used by this claim.
fn registration_eog(
    api: &dyn ConsensusApi,
    context: &ProtectedFilerReservationV1,
    policy: &TokenizerRegistrationPolicyV1,
) -> Result<PalwTokenTableEogOpeningV1, String> {
    use kaspa_consensus_core::palw_tokenizer_registration_carriage_v1::{
        TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1, decode_tokenizer_registration_payload_v1,
    };
    let facts = &context.tokenizer;
    let accepted = api.get_block_acceptance_data(context.registration_block).map_err(|e| e.to_string())?;
    for source in accepted.iter().filter(|s| !s.accepted_transactions.is_empty()) {
        let body = api.get_block_body(source.block_hash).map_err(|e| e.to_string())?;
        for entry in &source.accepted_transactions {
            let tx = body.get(entry.index_within_block as usize).ok_or("missing accepted registration transaction")?;
            if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
                || !tx.payload.starts_with(TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1)
                || tx.payload.get(4) != Some(&2)
                || tx.id() != entry.transaction_id
                || !tx.id_is_current()
            {
                continue;
            }
            if let Some(TokenizerRegistrationWireV1::Finish(finish)) = decode_tokenizer_registration_payload_v1(&tx.payload, policy)?
                && finish.eog.opening.leaf_hash == facts.eog_leaf
                && finish.eog.verifies(&facts.statement.table_root, facts.statement.vocab)
            {
                return Ok(finish.eog);
            }
        }
    }
    Err("the registered tokenizer's accepted EOG opening is unavailable".into())
}

fn check_carrier(
    api: &dyn ConsensusApi,
    context: &ProtectedFilerReservationV1,
    tir: &FilerTirContextV1,
    ids: &[u32],
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    tokenizer: &Path,
    policy: &TokenizerRegistrationPolicyV1,
) -> Result<(), String> {
    use crate::palw_panel::protected_replay::{checked_manifest_v1, read_prompt_material_v1};
    use kaspa_consensus_core::{
        palw_freeprompt_v3::palw_fp_seat_prompt_admit_v1,
        palw_tir_funded_original_envelope_v1::{actual_funded_payload_v1, decode_funded_original_carrier_v3},
        palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3,
    };
    let origin = context.origin;
    let body = api.get_block_body(origin.carrier_block).map_err(|e| e.to_string())?;
    let tx = body
        .iter()
        .find(|tx| tx.id() == origin.carrier_transaction && tx.id_is_current())
        .ok_or("protected original transaction missing")?;
    let facts = &context.tokenizer;
    let length = facts.statement.exact_raw_bytes;
    let file = std::fs::File::open(tokenizer).map_err(|e| e.to_string())?;
    if length == 0 || length > 32 << 20 || file.metadata().map_err(|e| e.to_string())?.len() != length {
        return Err("protected tokenizer length differs from registration or exceeds the worker bound".into());
    }
    let memory = length
        .checked_mul(128)
        .and_then(|n| n.checked_add((tx.payload.len() as u64).checked_mul(64)?))
        .and_then(|n| n.checked_add(policy.max_carrier_bytes.checked_mul(8)?))
        .and_then(|n| n.checked_add(1 << 20))
        .ok_or("protected filer memory overflow")?;
    let _lease = crate::palw_memory_ledger::host_ledger_v1()
        .reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "protected-filer-semantics",
                class_id: tir.view.class_id,
                job: origin.claim,
            },
            memory,
        )
        .map_err(|e| e.to_string())?;
    let bytes = actual_funded_payload_v1(tx)?;
    let wire = PalwTirFundedOriginalEnvelopeV3::decode_v3(bytes)?;
    let funded = decode_funded_original_carrier_v3(bytes)?;
    let payload = funded.original_fp_v2()?;
    let fp = &payload.commitment;
    if !wire.is_protected_v3()
        || payload.claim_id() != origin.claim
        || funded.original_header_v2()? != *tir.header
        || fp.execution_root != origin.execution_root
        || ids.len() != fp.decode_tokens_executed as usize
        || fp.job.tokenizer_id != facts.statement.tokenizer_id
        || facts.statement.vocab != tir.view.vocab_size
        || ids.iter().any(|id| *id >= facts.statement.vocab)
    {
        return Err("public output or tokenizer differs from the admitted FP job".into());
    }
    let config = fp.job.decode.as_ref().ok_or("no V4 decoding pipeline")?;
    kaspa_consensus_core::palw_decode_pipeline_v4::decode_answer_stop_v4(
        config,
        fp.job.decode_token_limit,
        facts.statement.vocab,
        ids,
    )?;
    if (fp.stop_reason == PalwFpStopReasonV3::ExactBudgetReached) != (fp.decode_tokens_executed == fp.job.decode_token_limit) {
        return Err("noncanonical stop reason".into());
    }
    let signed = wire.companion_v3().map_err(|e| format!("protected companion: {e:?}"))?;
    let original = &signed.original;
    if original.id().0.0 != origin.original.as_bytes() || original.output.0 != fp.output_root.as_bytes() {
        return Err("protected companion or output differs from admission".into());
    }
    original.authenticate(&signed.key, &signed.signature).map_err(|e| format!("protected companion signature: {e:?}"))?;
    let manifest = checked_manifest_v1(original, wire.manifest_bytes_v2().ok_or("missing protected manifest")?)?;
    if !kaspa_consensus_core::palw_positive_contract_v2::public_output_tokens_match_v1(original, &manifest, ids) {
        return Err("public output does not open the accepted commitment".into());
    }
    let mut raw = Vec::new();
    raw.try_reserve_exact(length as usize).map_err(|e| e.to_string())?;
    file.take(length + 1).read_to_end(&mut raw).map_err(|e| e.to_string())?;
    if raw.len() as u64 != length {
        return Err("tokenizer length changed while reading".into());
    }
    let input = read_prompt_material_v1(
        original,
        &manifest,
        wire.prompt_bytes_v3(),
        wire.prompt_encoding_v5(),
        &raw,
        fp.job.tokenizer_id,
        form,
    )?;
    palw_fp_seat_prompt_admit_v1(&fp.job, Some(&input), form).map_err(|e| format!("public input: {e:?}"))?;
    let eog = registration_eog(api, context, policy)?;
    let budget = PalwTokenizerRenderingBudgetV2 {
        raw_bytes: length,
        vocab: facts.statement.vocab,
        key_events: facts.key_events,
        eog_ids: facts.eog_count,
        total_rendering_bytes: facts.total_rendering_bytes,
    };
    if derive_tokenizer_rendering_v2(facts.statement, &raw, &eog, budget)? != *facts {
        return Err("token renderings or EOG differ from the original registration".into());
    }
    Ok(())
}

impl PalwPanelService {
    pub(super) async fn protected_filer_withdrawal(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        case: &PalwFraudFilerCaseV1,
        answer: &PalwDaAnswerV1,
        owner: PalwBondKeyV2,
    ) -> Result<Option<ProtectedFilerReservationV1>, String> {
        let claim = case.candidate.claim_id;
        let Some(tir) = case.tir.clone() else { return Ok(None) };
        let Some(own) = case.tir_selection_ids.clone() else { return Ok(None) };
        if !case.arithmetic_matched
            || !matches!(case.verdict, PalwFraudFilerVerdictV1::SemanticPending)
            || case.constraint_stage_pending
            || case.prefix_stage_pending
            || tir.row_zero_ids(&PalwDaUnitV1::Event { row: 0, tile: 0 }, answer) != Some(own.ids.as_slice())
        {
            return Ok(None);
        }
        let Some(context) = owner_context(session, claim, owner)? else { return Ok(None) };
        if case.protected_replay_origin != Some(context.origin)
            || context.origin.accepting_block != case.candidate.job.accepted_block
            || context.origin.accepted_daa != case.candidate.accepted_daa
            || context.origin.execution_root != case.candidate.job.execution_root
        {
            return Err("protected replay admission changed".into());
        }
        let class = tir.view.class_id;
        let path = self
            .backends()
            .held_tokenizer_path_or_chain_v1(class, tir.header.artifact_root, |id| self.chain_carriage_v1(session, id))
            .ok_or("the verifier does not hold the admitted tokenizer")?;
        let policy = self
            .consensus_config
            .params
            .palw_tokenizer_registration_v1_at(context.origin.accepted_daa)
            .ok_or("tokenizer registration transport absent at original acceptance")?
            .policy;
        let form = self.class_prompt_ids_form(class);
        let checked = context.clone();
        session.clone().spawn_blocking(move |api| check_carrier(api, &checked, &tir, &own.ids, form, &path, &policy)).await?;
        if owner_context(session, claim, owner)? != Some(context.clone()) {
            return Err("protected reservation changed during verification".into());
        }
        Ok(Some(context))
    }
}
