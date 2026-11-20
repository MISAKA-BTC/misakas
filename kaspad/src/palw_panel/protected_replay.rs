//! Bind the independently completed FP replay to its accepted public inputs.
//! This module authenticates data identity; the caller supplies the actual
//! completed replay, and consensus independently authorizes the selected seat.
use super::*;
use kaspa_consensus_core::{
    palw_freeprompt_v3::{PalwFpMaterialV1, fp_claim_id_v3},
    palw_protected_original_readback_v1::ProtectedReplayBindingV1,
    palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3,
};
use misaka_palw_positive_contract::public_one_honest::{ManifestScopeRules, ManifestV1, MaterialScope};
use std::io::Read;

/// Constructed only after the exact accepted carrier's public prompt encodes
/// to the IDs of the completed replay. Kept with that immutable admission.
#[derive(Clone, Copy)]
pub(super) struct CheckedReplayInputsV1 {
    binding: ProtectedReplayBindingV1,
}
impl CheckedReplayInputsV1 {
    pub(super) fn binding(self) -> ProtectedReplayBindingV1 {
        self.binding
    }
}

/// Public inputs reconstructed from the exact accepted carrier. Possession of
/// these inputs is not a replay verdict. The caller must complete the full
/// replay and recheck the inclusion before staging an approval.
pub(super) struct AcceptedReplayInputsV1 {
    pub(super) material: PalwFpMaterialV1,
    binding: ProtectedReplayBindingV1,
    _lease: crate::palw_memory_ledger::PalwMemoryReservationV1,
}

impl AcceptedReplayInputsV1 {
    pub(super) fn binding(&self) -> ProtectedReplayBindingV1 {
        self.binding
    }

    pub(super) fn check_completed_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        replayed: &PalwFpMaterialV1,
    ) -> Result<CheckedReplayInputsV1, String> {
        self.check_completed_at_v1(
            replayed,
            session.palw_protected_original_readback_v1(self.binding.claim)?.map(|r| r.replay_binding_v1()),
        )
    }

    fn check_completed_at_v1(
        &self,
        replayed: &PalwFpMaterialV1,
        current: Option<ProtectedReplayBindingV1>,
    ) -> Result<CheckedReplayInputsV1, String> {
        if replayed != &self.material || current != Some(self.binding) {
            return Err("protected replay job or accepted origin changed".into());
        }
        Ok(CheckedReplayInputsV1 { binding: self.binding })
    }
}

/// Every protected seat takes the full replay, independently of legacy masks.
/// The hard deadline governs this work; receipts retain their notice boundary.
pub(super) fn seat_r_duty_v1(duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2, heavy: bool) -> Option<PalwSeatRDutyV1> {
    let binding = duty.protected_replay?;
    Some(PalwSeatRDutyV1 {
        deadline: binding.accepted_daa.checked_add(kaspa_consensus_core::palw_court_policy_v1::DISPUTE_WINDOW_DAA)?,
        role: PalwSeatRRoleV1::FullSeat,
        heavy,
    })
}

pub(super) fn receipt_may_be_filed_v1(binding: ProtectedReplayBindingV1, current_daa: u64, already_filed: bool) -> bool {
    !already_filed
        && current_daa >= binding.accepted_daa
        && binding
            .accepted_daa
            .checked_add(kaspa_consensus_core::palw_court_policy_v1::NOTICE_WINDOW_DAA)
            .is_some_and(|end| current_daa <= end)
}

impl PalwPanelService {
    pub(super) async fn load_protected_replay_inputs_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
        cached: Option<Arc<AcceptedReplayInputsV1>>,
    ) -> Result<Option<Arc<AcceptedReplayInputsV1>>, String> {
        let Some(read) = session.palw_protected_original_readback_v1(duty.claim_id)? else {
            return if duty.protected_replay.is_some() { Err("protected replay admission is unavailable".into()) } else { Ok(None) };
        };
        let binding = read.replay_binding_v1();
        if duty.protected_replay.is_some_and(|expected| expected != binding)
            || binding.accepting_block != duty.accepted_block
            || binding.execution_root != duty.execution_root
        {
            return Err("protected replay duty changed its accepted origin".into());
        }
        if cached.as_ref().is_some_and(|inputs| inputs.binding == binding) {
            return Ok(cached);
        }
        // Release a superseded job before admitting its replacement. A cached
        // job never bypasses the current native frame/admission-link readback.
        drop(cached);
        let body = session
            .with_consensus_api_v1(|api| api.get_block_body(binding.carrier_block))
            .map_err(|e| format!("protected replay carrier unavailable: {e}"))?;
        let tx_index = body
            .iter()
            .position(|tx| tx.id() == binding.carrier_transaction && tx.id_is_current())
            .ok_or("protected replay carrier transaction unavailable")?;
        // Keep the already admitted block owner, not an uncharged transaction copy.
        let tokenizer = self
            .backends()
            .held_tokenizer_path_or_chain_v1(duty.class_id, duty.artifact_root, |id| self.chain_carriage_v1(session, id))
            .ok_or("protected replay tokenizer unavailable")?;
        let form = self.class_prompt_ids_form(duty.class_id);
        let duty = duty.clone();
        let checked = tokio::task::spawn_blocking(move || {
            let tx = &body[tx_index];
            // Reserve parser/tokenizer expansion before copying or decoding its body.
            let file = std::fs::File::open(tokenizer).map_err(|e| e.to_string())?;
            let length = file.metadata().map_err(|e| e.to_string())?.len();
            if length == 0 || length > 32 << 20 {
                return Err("protected replay tokenizer exceeds local read bound".into());
            }
            let need = length
                .checked_mul(128)
                .and_then(|n| n.checked_add((tx.payload.len() as u64).checked_mul(64)?))
                .and_then(|n| n.checked_add(1 << 20))
                .ok_or("protected replay metadata memory overflow")?;
            let _lease = crate::palw_memory_ledger::host_ledger_v1()
                .reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "protected-replay-inputs",
                        class_id: duty.class_id,
                        job: duty.claim_id,
                    },
                    need,
                )
                .map_err(|e| e.to_string())?;
            let mut raw = Vec::new();
            raw.try_reserve_exact(length as usize).map_err(|e| e.to_string())?;
            file.take(length + 1).read_to_end(&mut raw).map_err(|e| e.to_string())?;
            if raw.len() as u64 != length {
                return Err("protected replay tokenizer changed during read".into());
            }
            let material = read_carrier_inputs_v1(tx, binding, &duty, form, &raw)?;
            // Keep the returned job, its replay-loop copy and FPM encoding's
            // clone/body/output under a lease after the parser lease ends.
            let material_bytes = borsh::object_length(&material).map_err(|e| e.to_string())? as u64;
            let spare_ids = material.prompt_token_ids.capacity().saturating_sub(material.prompt_token_ids.len()) as u64;
            let copy_need = material_bytes
                .checked_mul(5)
                .and_then(|n| n.checked_add(spare_ids.checked_mul(4)?))
                .and_then(|n| n.checked_add(16_384))
                .ok_or("protected replay job copy overflow")?;
            let lease = crate::palw_memory_ledger::host_ledger_v1()
                .reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "protected-replay-job",
                        class_id: duty.class_id,
                        job: duty.claim_id,
                    },
                    copy_need,
                )
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(AcceptedReplayInputsV1 { material, binding, _lease: lease })
        })
        .await
        .map_err(|e| format!("protected replay input worker: {e}"))??;
        // The queued work must not silently migrate to another inclusion while
        // the tokenizer is being read. Signing checks the binding again atomically.
        if session.palw_protected_original_readback_v1(binding.claim)?.map(|r| r.replay_binding_v1()) != Some(binding) {
            return Err("protected replay origin changed during input check".into());
        }
        Ok(Some(Arc::new(checked)))
    }
}

fn read_carrier_inputs_v1(
    tx: &Transaction,
    binding: ProtectedReplayBindingV1,
    duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    raw: &[u8],
) -> Result<PalwFpMaterialV1, String> {
    let bytes = kaspa_consensus_core::palw_tir_funded_original_envelope_v1::actual_funded_payload_v1(tx)?;
    let wire = PalwTirFundedOriginalEnvelopeV3::decode_v3(bytes)?;
    if !wire.is_protected_v3() {
        return Err("protected TFO3 wire version required".into());
    }
    let funded = kaspa_consensus_core::palw_tir_funded_original_envelope_v1::decode_funded_original_carrier_v3(bytes)?;
    let fp = funded.original_fp_v2()?;
    let fp = &fp.commitment;
    if tx.id() != binding.carrier_transaction
        || !tx.id_is_current()
        || fp_claim_id_v3(fp) != binding.claim
        || binding.claim != duty.claim_id
        || fp.job.class_id != duty.class_id
        || fp.job.executor_bond != duty.executor_bond.0
        || fp.execution_root != duty.execution_root
        || fp.trace_root != duty.trace_root
        || fp.output_root != duty.output_root
        || fp.work_leaves != duty.work_leaves
    {
        return Err("protected original differs from the assigned FP duty".into());
    }
    let signed = wire.companion_v3().map_err(|e| format!("protected companion: {e:?}"))?;
    let original = &signed.original;
    if original.id().0.0 != binding.original.as_bytes() {
        return Err("protected replay companion differs from native admission".into());
    }
    original.authenticate(&signed.key, &signed.signature).map_err(|e| format!("protected companion signature: {e:?}"))?;
    let body = wire.manifest_bytes_v2().ok_or("protected original manifest missing")?;
    let checked = checked_manifest_v1(original, body)?;
    // Native acceptance already authenticates every manifest subject, byte size,
    // rule/cold-state preimage and registered container pin against this original.
    // Derive the public Text/control relation here, without a served FPM or
    // producer capture. The independent full replay still checks all roots.
    let prompt_token_ids = read_prompt_material_v1(
        original,
        &checked,
        wire.prompt_bytes_v3(),
        wire.prompt_encoding_v5(),
        raw,
        fp.job.tokenizer_id,
        form,
    )?;
    kaspa_consensus_core::palw_freeprompt_v3::palw_fp_seat_prompt_admit_v1(&fp.job, Some(&prompt_token_ids), form)
        .map_err(|e| format!("protected replay prompt admission: {e:?}"))?;
    Ok(PalwFpMaterialV1 { job: fp.job.clone(), prompt_token_ids })
}

pub(super) fn checked_manifest_v1(
    original: &misaka_palw_positive_contract::public_one_honest::OriginalCommit,
    body: &[u8],
) -> Result<misaka_palw_positive_contract::public_one_honest::CheckedManifestV1, String> {
    let manifest: ManifestV1 = borsh::from_slice(body).map_err(|e| e.to_string())?;
    let limits = manifest.entries.map(|e| ManifestScopeRules { codec: e.codec, max_bytes: e.total_bytes, chunk_bytes: e.chunk_bytes });
    let total = manifest.entries.iter().try_fold(0u64, |n, e| n.checked_add(e.total_bytes)).ok_or("manifest byte overflow")?;
    original
        .authenticate_manifest(body, ManifestV1::WIRE_BYTES)
        .and_then(|m| m.decode(original, &limits, total))
        .map_err(|e| format!("protected replay manifest: {e:?}"))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn read_prompt_material_v1(
    original: &misaka_palw_positive_contract::public_one_honest::OriginalCommit,
    checked: &misaka_palw_positive_contract::public_one_honest::CheckedManifestV1,
    body: &[u8],
    expected_codec: misaka_palw_positive_contract::public_one_honest::PromptMaterialEncodingV1,
    raw: &[u8],
    tokenizer_id: Hash64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<Vec<u32>, String> {
    if checked.original() != original.id() {
        return Err("protected replay manifest names another original".into());
    }
    let entry = checked.entry(MaterialScope::Tokenizer);
    let tokenizer_rules = ManifestScopeRules { codec: entry.codec, max_bytes: entry.total_bytes, chunk_bytes: entry.chunk_bytes };
    let (encoding, prompt) = checked.open_prompt_material_v2(body).map_err(|e| format!("protected replay prompt: {e:?}"))?;
    if encoding != expected_codec {
        return Err("protected replay prompt codec differs from carrier".into());
    }
    let tokenizer_entry = kaspa_consensus_core::palw_positive_contract_v2::public_tokenizer_material_v1(raw, tokenizer_rules)
        .map_err(|e| format!("protected replay tokenizer: {e:?}"))?;
    if &tokenizer_entry != checked.entry(MaterialScope::Tokenizer) || tokenizer_entry.subject.0 != tokenizer_id.as_bytes() {
        return Err("protected replay tokenizer differs from admitted bytes".into());
    }
    let tokenizer = misaka_palw_base0::tokenizer::QwenTokenizer::from_json(raw).map_err(|e| e.to_string())?;
    let ids = encoding
        .encode_ids_v1(prompt, original.prefill, |text| tokenizer.encode_without_specials(text), |id| tokenizer.is_added_id(id))
        .map_err(|e| format!("protected replay tokenization: {e:?}"))?;
    if ids.len() != original.prefill as usize
        || !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
            form,
            &ids,
            &Hash64::from_bytes(original.input_tokens.0),
        )
    {
        return Err("public prompt differs from admitted input IDs".into());
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
    use misaka_palw_positive_contract::{
        identity::{ChainScope, Digest, PrincipalId},
        public_one_honest::*,
    };

    #[test]
    fn accepted_job_retention_rechecks_origin_and_releases_with_the_duty() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        use kaspa_consensus_core::palw_freeprompt_v3::*;
        // Tests retained-input ownership and completion binding, not a native
        // admission, a completed execution or an authenticated Panel approval.
        let hash = Hash64::from_bytes([7; 64]);
        let binding = ProtectedReplayBindingV1 {
            claim: hash,
            execution_root: hash,
            original: hash,
            carrier_transaction: hash,
            carrier_block: hash,
            accepting_block: hash,
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: 10,
            accepted_blue_score: 10,
        };
        let material = PalwFpMaterialV1 {
            job: PalwFreePromptJobV3 {
                version: PALW_FP_V3_VERSION,
                network_domain: hash,
                class_id: hash,
                executor_bond: TransactionOutpoint::new(hash, 0),
                executor_pubkey: vec![0; 32],
                operator_id: hash,
                anchor_block: hash,
                anchor_daa: 1,
                job_nonce: [0; 32],
                tokenizer_id: hash,
                prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[1]).unwrap(),
                prompt_tokens: 1,
                decode_token_limit: 1,
                max_context_tokens: 2,
                privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
                prompt_mode: PALW_FP_PROMPT_MODE_USER,
                sampling_seed: [0; 32],
                temperature_q: 0,
                decode: None,
                tail: None,
            },
            prompt_token_ids: vec![1],
        };
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, None, || None);
        let lease =
            ledger.reserve(PalwMemoryReservationKeyV1 { role: "protected-replay-job", class_id: hash, job: hash }, 16_384).unwrap();
        let inputs = Arc::new(AcceptedReplayInputsV1 { material, binding, _lease: lease });
        let mut replays = PalwSeatReplaysV1::default();
        replays.protected_inputs.insert(hash, inputs.clone());
        assert_eq!(ledger.reserved_bytes(), 16_384);
        replays.retain_live(|claim| *claim == hash);
        let retained = replays.protected_inputs.get(&hash).unwrap();
        assert!(Arc::ptr_eq(retained, &inputs));
        assert_eq!(retained.check_completed_at_v1(&inputs.material, Some(binding)).unwrap().binding(), binding);
        assert!(retained.check_completed_at_v1(&inputs.material, None).is_err());
        let mut moved = binding;
        moved.accepting_block = Hash64::from_bytes([8; 64]);
        assert!(retained.check_completed_at_v1(&inputs.material, Some(moved)).is_err());
        moved = binding;
        moved.carrier_transaction = Hash64::from_bytes([8; 64]);
        assert!(retained.check_completed_at_v1(&inputs.material, Some(moved)).is_err());
        let mut different = inputs.material.clone();
        different.prompt_token_ids[0] = 2;
        assert!(retained.check_completed_at_v1(&different, Some(binding)).is_err());
        different = inputs.material.clone();
        different.job.sampling_seed[0] = 1;
        assert!(retained.check_completed_at_v1(&different, Some(binding)).is_err());
        replays.retain_live(|_| false);
        assert!(replays.protected_inputs.is_empty());
        assert_eq!(ledger.reserved_bytes(), 16_384, "the current pass still owns its inputs");
        drop(inputs);
        assert_eq!(ledger.reserved_bytes(), 0, "leaving the duty returns the input reservation");
    }

    #[test]
    fn protected_replay_inputs_require_public_body_tokenizer_codec_and_replayed_ids() {
        // Only the prompt/tokenizer relation is under test; remaining manifest
        // slots are placeholders, never a claim of native admission or replay.
        let d = Digest([7; 64]);
        let encoding = PromptMaterialEncodingV1::SegmentsV1;
        let body = b"PPS1
s:2
t:1:a";
        let ids = [2, 1];
        let form = PalwPromptIdsFormV1::Flat;
        let raw = br#"{"model":{"type":"BPE","vocab":{"a":1,"b":0,"<|start|>":2},"merges":[]},"added_tokens":[{"id":2,"content":"<|start|>","special":true,"normalized":false}]}"#;
        let mut limits = [ManifestScopeRules { codec: d, max_bytes: 4096, chunk_bytes: 4096 }; MaterialScope::COUNT];
        limits[MaterialScope::Prompt as usize].codec = encoding.codec_v1();
        limits[MaterialScope::Tokenizer as usize].codec = tokenizer_json_material_codec_v1();
        let prompt = encoding.material_v1(d, std::str::from_utf8(body).unwrap(), limits[MaterialScope::Prompt as usize]).unwrap();
        let tokenizer = kaspa_consensus_core::palw_positive_contract_v2::public_tokenizer_material_v1(
            raw,
            limits[MaterialScope::Tokenizer as usize],
        )
        .unwrap();
        let input = Digest(prompt_token_ids_commitment_v1(form, &ids).unwrap().as_bytes());
        let mut original = OriginalCommit {
            scope: ChainScope { network: d, ruleset: d, branch: d },
            job: d,
            execution: d,
            miner: PrincipalId(d),
            registered_class: d,
            artifact: d,
            program: d,
            prompt: prompt.subject,
            input_tokens: input,
            tokenizer: tokenizer.subject,
            execution_rules: d,
            generation_seed: [0; 32],
            initial_state: d,
            dependencies: d,
            prefill: 2,
            decode: 1,
            max_context: 3,
            output: d,
            trace_root: d,
            manifest_root: d,
            service_contract: d,
        };
        let mut entries =
            [ManifestEntryV1 { subject: d, content_root: d, codec: d, total_bytes: 1, chunk_bytes: 4096 }; MaterialScope::COUNT];
        entries[MaterialScope::Prompt as usize] = prompt;
        entries[MaterialScope::Tokenizer as usize] = tokenizer;
        entries[MaterialScope::InputTokens as usize].subject = input;
        entries[MaterialScope::InputTokens as usize].content_root = input;
        let manifest = ManifestV1 { service_contract: d, entries };
        let bytes = original.bind_manifest(&manifest, &limits, 100_000, ManifestV1::WIRE_BYTES).unwrap();
        let checked =
            original.authenticate_manifest(&bytes, ManifestV1::WIRE_BYTES).unwrap().decode(&original, &limits, 100_000).unwrap();
        let check = |o: &_, body: &[u8], encoding, raw: &[u8], ids: &[u32]| {
            let restored = read_prompt_material_v1(o, &checked, body, encoding, raw, Hash64::from_bytes(tokenizer.subject.0), form)?;
            if restored != ids {
                return Err("restored public prompt differs from replayed IDs".to_string());
            }
            Ok::<_, String>(())
        };
        // This is the job-start input: no FPM or producer capture is supplied.
        assert_eq!(
            read_prompt_material_v1(&original, &checked, body, encoding, raw, Hash64::from_bytes(tokenizer.subject.0), form,).unwrap(),
            ids
        );
        check(&original, body, encoding, raw, &ids).unwrap();
        assert!(check(&original, body, PromptMaterialEncodingV1::LiteralUtf8, raw, &ids).is_err());
        assert!(
            check(
                &original,
                b"PPS1
s:2
t:1:b",
                encoding,
                raw,
                &ids
            )
            .is_err()
        );
        assert!(check(&original, body, encoding, b"{}", &ids).is_err());
        assert!(check(&original, body, encoding, raw, &[1, 2]).is_err());
        let mut another = original.clone();
        another.job = Digest([8; 64]);
        assert!(check(&another, body, encoding, raw, &ids).is_err());
    }
}
