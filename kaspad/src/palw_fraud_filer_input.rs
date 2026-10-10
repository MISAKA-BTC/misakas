//! Public free-prompt job/input bootstrap for the common filer. No producer capture is executed or opened here.
use super::*;
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_ANSWER_V1_MAGIC, PALW_FP_CAPTURE_V1_MAGIC, PALW_FP_MATERIAL_V1_MAGIC, PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA,
    PALW_FP_PROMPT_MODE_CANONICAL, PALW_FP_PROMPT_MODE_USER, PalwFpCommitmentTxPayloadV3, PalwFpMaterialV1, PalwFreePromptJobV3,
    fp_canonical_anchor_v1, palw_fp_prompt_ids_admit_v1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;

/// Read the claim's commitment only from transactions accepted by its recorded chain block. A matching claim hash also fixes
/// every job field and root; the recorded job pin must agree. Unavailable retained history is a retry, not a verdict.
pub(in crate::palw_panel) fn palw_fraud_filer_fp_payload_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    candidate: &PalwFraudFilerCandidateV1,
    domain: Hash64,
) -> Result<PalwFpCommitmentTxPayloadV3, String> {
    use kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1;
    let target = consensus.palw_fraud_filer_target_v1(candidate.claim_id).ok_or("the FP claim's recorded target is unavailable")?;
    if !candidate.job.free_prompt
        || target.claim_id != candidate.claim_id
        || target.lane != Some(PalwClaimSourceKindV1::FreePrompt)
        || target.execution_root != candidate.job.execution_root
        || target.trace_root != candidate.job.trace_root
        || target.output_root != candidate.job.output_root
        || candidate.job.artifact_root != Some(target.artifact_root)
        || target.class_id != candidate.job.class_id
        || target.executor_bond != candidate.producer
    {
        return Err("the FP target changed since candidate discovery".into());
    }
    let accepted =
        consensus.get_block_acceptance_data(candidate.job.accepted_block).map_err(|e| format!("FP commitment acceptance: {e}"))?;
    for merged in accepted.iter() {
        if merged.accepted_transactions.is_empty() {
            continue;
        }
        let block = consensus.get_block(merged.block_hash).map_err(|e| format!("FP commitment block: {e}"))?;
        for entry in &merged.accepted_transactions {
            let tx =
                block.transactions.get(entry.index_within_block as usize).ok_or("FP accepted transaction index is unavailable")?;
            if tx.id() != entry.transaction_id {
                return Err("FP accepted transaction id differs from the block's transaction".into());
            }
            if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT {
                continue;
            }
            let Ok(payload) = borsh::from_slice::<PalwFpCommitmentTxPayloadV3>(&tx.payload) else {
                continue;
            };
            if payload.claim_id() != candidate.claim_id {
                continue;
            }
            let c = &payload.commitment;
            if c.job.network_domain != domain
                || c.job.class_id != candidate.job.class_id
                || c.job.executor_bond != candidate.producer.0
                || c.execution_root != target.execution_root
                || c.trace_root != target.trace_root
                || c.output_root != target.output_root
                || c.work_leaves != candidate.job.work_leaves
                || target.job_identity == Hash64::default()
                || kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(c) != target.job_identity
            {
                return Err("the public FP commitment does not reproduce the recorded job and roots".into());
            }
            return Ok(payload);
        }
    }
    Err("the recorded FP commitment is not present in retained accepted transactions".into())
}

/// Read just the authenticated input prefix of a public FPM1/FPC1/FPA1. A capture/answer tail is checked for wire length but
/// never deserialized, copied, or believed; the verifier executes its own model instead.
fn public_material_input(
    bytes: &[u8],
    job_wire: &[u8],
    job: &PalwFreePromptJobV3,
    form: PalwPromptIdsFormV1,
) -> Option<PalwFpMaterialV1> {
    let (body, width) = if let Some(body) = bytes.strip_prefix(&PALW_FP_MATERIAL_V1_MAGIC) {
        (body, 0usize)
    } else if let Some(body) = bytes.strip_prefix(&PALW_FP_CAPTURE_V1_MAGIC) {
        (body, 1)
    } else if let Some(body) = bytes.strip_prefix(&PALW_FP_ANSWER_V1_MAGIC) {
        (body, 4)
    } else {
        return None;
    };
    // Authenticate the job's canonical wire BEFORE parsing any peer-controlled vector. No foreign job/tail is deserialized.
    let body = body.strip_prefix(job_wire)?;
    let count = u32::from_le_bytes(body.get(..4)?.try_into().ok()?);
    if count != job.prompt_tokens {
        return None;
    }
    let input_len = usize::try_from(count).ok()?.checked_mul(4)?;
    let ids_wire = body.get(4..4usize.checked_add(input_len)?)?;
    let tail = body.get(4usize.checked_add(input_len)?..)?;
    if width == 0 {
        if !tail.is_empty() {
            return None;
        }
    } else {
        let count = u32::from_le_bytes(tail.get(..4)?.try_into().ok()?);
        if count == 0 || tail.len() - 4 != usize::try_from(count).ok()?.checked_mul(width)? {
            return None;
        }
    }
    let ids: Vec<u32> = ids_wire.chunks_exact(4).map(|b| u32::from_le_bytes(b.try_into().expect("four bytes"))).collect();
    palw_fp_prompt_ids_admit_v1(job, &ids, form).ok()?;
    Some(PalwFpMaterialV1 { job: job.clone(), prompt_token_ids: ids })
}

#[derive(Debug)]
pub(in crate::palw_panel) struct PalwFraudFilerFpInputV1 {
    pub(in crate::palw_panel) job: PalwFreePromptJobV3,
    pub(in crate::palw_panel) prompt_ids: Vec<u32>,
}

/// PublicDA reads carried ids; canonical jobs derive ids from their own job anchor; otherwise only an authenticated public
/// input envelope of this exact job is usable. Missing PanelDA input stays pending until its disclosure/bootstrap route supplies it.
pub(in crate::palw_panel) fn palw_fraud_filer_fp_input_v1(
    backend: &dyn PalwExecutionBackendV1,
    payload: &PalwFpCommitmentTxPayloadV3,
    form: PalwPromptIdsFormV1,
    public_material: &[Vec<u8>],
) -> Result<PalwFraudFilerFpInputV1, String> {
    let job = &payload.commitment.job;
    if !matches!(job.prompt_mode, PALW_FP_PROMPT_MODE_USER | PALW_FP_PROMPT_MODE_CANONICAL)
        || !matches!(job.privacy_mode, PALW_FP_PRIVACY_PUBLIC_DA | PALW_FP_PRIVACY_PANEL_DA)
    {
        return Err("unsupported FP input mode".into());
    }
    let ids = if job.prompt_mode == PALW_FP_PROMPT_MODE_CANONICAL {
        let (ctx, ids) = backend.job_for_anchor(fp_canonical_anchor_v1(job))?;
        if ctx.declared_prefill_tokens != job.prompt_tokens {
            return Err("the canonical FP prompt count is not the job's".into());
        }
        ids.into_iter()
            .map(|id| u32::try_from(id).map_err(|_| "a canonical FP id is past u32".to_string()))
            .collect::<Result<Vec<_>, _>>()?
    } else if job.privacy_mode == PALW_FP_PRIVACY_PUBLIC_DA {
        payload.prompt_token_ids.clone()
    } else {
        let job_wire = borsh::to_vec(job).map_err(|e| format!("public FP job encoding: {e}"))?;
        public_material
            .iter()
            .find_map(|bytes| public_material_input(bytes, &job_wire, job, form).map(|m| m.prompt_token_ids))
            .ok_or("the FP job's authenticated public input is not available yet")?
    };
    palw_fp_prompt_ids_admit_v1(job, &ids, form).map_err(|e| format!("public FP input: {e}"))?;
    Ok(PalwFraudFilerFpInputV1 { job: job.clone(), prompt_ids: ids })
}

/// Public input acquisition before any replay. Its retained ids and hashing temporaries hold a host memory ticket.
#[derive(Clone)]
pub(in crate::palw_panel) struct PalwFraudFilerFpBootstrapV1 {
    pub(in crate::palw_panel) payload: Arc<PalwFpCommitmentTxPayloadV3>,
    pub(in crate::palw_panel) form: PalwPromptIdsFormV1,
    pub(in crate::palw_panel) ids: Vec<u32>,
    pub(in crate::palw_panel) chunk: u32,
    pub(in crate::palw_panel) _reservation: Option<Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>>,
}

impl PalwFraudFilerFpBootstrapV1 {
    pub(in crate::palw_panel) fn ready(&self) -> bool {
        self.ids.len() == self.payload.commitment.job.prompt_tokens as usize
    }
    pub(in crate::palw_panel) fn learn(
        &mut self,
        binding: &PalwStepBindingV2,
        unit: PalwLegacyHeldUnitV2,
        answer: &PalwLegacyHeldAnswerV2,
    ) -> Result<(), String> {
        let PalwLegacyHeldUnitV2::PromptIds { chunk } = unit else {
            return Err("bootstrap consumes only input chunks".into());
        };
        if chunk != self.chunk {
            return Err("bootstrap needs the next contiguous input chunk".into());
        }
        if binding.job_context.declared_prefill_tokens != self.payload.commitment.job.prompt_tokens
            || kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context)
                != kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&self.payload.commitment)
        {
            return Err("the input binding is not the recorded FP job".into());
        }
        kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
            &self.payload.commitment.execution_root,
            &unit,
            binding,
            answer,
            binding.step_leaf_count,
            self.form,
        )
        .map_err(|e| e.to_string())?;
        let PalwLegacyHeldAnswerV2::PromptIds { ids, .. } = answer else {
            return Err("the input answer has another kind".into());
        };
        let (first, _) = kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_prompt_chunk_bounds_v2(binding, chunk, self.form)
            .map_err(|e| e.to_string())?;
        if first != self.ids.len() as u64 {
            return Err("the input would skip or overlap a chunk".into());
        }
        self.ids.extend_from_slice(ids);
        self.chunk = self.chunk.checked_add(1).ok_or("the input chunk counter overflowed")?;
        if self.ready() {
            palw_fp_prompt_ids_admit_v1(&self.payload.commitment.job, &self.ids, self.form).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(in crate::palw_panel) mod tests {
    use super::*;
    use kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3;
    use kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1;

    pub(in crate::palw_panel) fn payload() -> PalwFpCommitmentTxPayloadV3 {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let (job, ids) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        let run = backend.execute_free_prompt(&job, &ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        let ctx = backend.fp_job_context_for_executed_v1(&job, run.facts.decode_tokens_executed).unwrap();
        PalwFpCommitmentTxPayloadV3 {
            version: job.version,
            commitment: palw_fp_commitment_from_context_v3(&job, &ctx, &run, 9999).unwrap(),
            prompt_token_ids: ids,
            signature: vec![],
        }
    }

    #[test]
    fn input_bootstrap_authenticates_the_job_and_worker_state_before_replay() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        use crate::palw_panel::palw_fraud_filer::{PalwFraudFilerCaseV1, PalwFraudFilerVerdictV1};
        use kaspa_consensus_core::palw_legacy_public_filer_v1::PalwLegacyProbeV1;
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let form = backend.prompt_ids_form();
        let (mut job, ids) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        job.privacy_mode = PALW_FP_PRIVACY_PANEL_DA;
        let run = backend.execute_free_prompt(&job, &ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        let binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&run.outcome.material).unwrap().binding().clone();
        let payload = Arc::new(PalwFpCommitmentTxPayloadV3 {
            version: job.version,
            commitment: palw_fp_commitment_from_context_v3(&job, &binding.job_context, &run, 9999).unwrap(),
            prompt_token_ids: vec![],
            signature: vec![],
        });
        let mut input = PalwFraudFilerFpBootstrapV1 { payload, form, ids: vec![], chunk: 0, _reservation: None };
        let unit = PalwLegacyHeldUnitV2::PromptIds { chunk: 0 };
        let answer = kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_prompt_answer_v2(&binding, &ids, 0, form).unwrap();
        assert!(!input.ready());
        assert!(input.learn(&binding, PalwLegacyHeldUnitV2::PromptIds { chunk: 1 }, &answer).is_err());
        let mut wrong_job = binding.clone();
        wrong_job.job_context.job_id = Hash64::default();
        assert!(input.learn(&wrong_job, unit, &answer).is_err());
        let mut corrupt = answer.clone();
        let PalwLegacyHeldAnswerV2::PromptIds { ids: part, .. } = &mut corrupt else { unreachable!() };
        part[0] ^= 1;
        assert!(input.learn(&binding, unit, &corrupt).is_err());
        assert!(input.ids.is_empty());
        assert_eq!(input.chunk, 0);
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(4096), || None);
        input._reservation = Some(Arc::new(
            ledger
                .reserve(PalwMemoryReservationKeyV1 { role: "input-test", class_id: job.class_id, job: Hash64::default() }, 2048)
                .unwrap(),
        ));
        let mut case = PalwFraudFilerCaseV1::new(crate::palw_panel::palw_fraud_filer::tests::candidate_for_input_test());
        case.fp_bootstrap = Some(input);
        case.binding = Some(binding.clone());
        case.input_retry_at = 100;
        let mut learned = case.clone();
        learned
            .learn(
                PalwLegacyProbeV1::HeldNode { unit },
                &PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((binding.clone(), answer.clone()))),
            )
            .unwrap();
        assert!(!case.fp_bootstrap.as_ref().unwrap().ready(), "the worker has a separate state copy");
        case.accept_learned_v1(learned);
        assert!(case.fp_bootstrap.as_ref().unwrap().ready(), "the actual worker handoff includes the input");
        assert_eq!(case.fp_bootstrap.as_ref().unwrap().ids, ids);
        assert_eq!(case.input_retry_at, 0);
        assert_eq!(case.runs, 0);
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
        assert!(case.fp_bootstrap.as_mut().unwrap().learn(&binding, unit, &answer).is_err(), "no duplicate chunk append");
        assert_eq!(ledger.reserved_bytes(), 2048);
        drop(case);
        assert_eq!(ledger.reserved_bytes(), 0, "the shared worker ticket is released with the case");
        let large_ids = vec![0u32; 30_000];
        let mut large_binding = binding.clone();
        large_binding.job_context.declared_prefill_tokens = large_ids.len() as u32;
        large_binding.job_context.max_context_tokens = large_ids.len() as u32 + 7;
        large_binding.job_context.prompt_token_ids_hash =
            prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &large_ids).unwrap();
        large_binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&large_binding);
        let large_answer = kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_prompt_answer_v2(
            &large_binding,
            &large_ids,
            0,
            PalwPromptIdsFormV1::Flat,
        )
        .unwrap();
        let large = PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((large_binding, large_answer)));
        assert!(
            crate::palw_panel::palw_da_built_answer_object_v1(
                &job.network_domain,
                Hash64::default(),
                PalwDaUnitV1::LegacyHeldV2(unit),
                large,
                kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(job.executor_bond),
                PalwPromptIdsFormV1::Flat,
                u64::MAX,
                |_, _| panic!("single-carrier ceiling refused before signing")
            )
            .unwrap_err()
            .contains("single carrier")
        );
        let built = PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((binding, answer)));
        let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(job.executor_bond);
        assert!(
            crate::palw_panel::palw_da_built_answer_object_v1(
                &job.network_domain,
                Hash64::default(),
                PalwDaUnitV1::LegacyHeldV2(unit),
                built.clone(),
                bond,
                form,
                u64::MAX,
                |_, _| Some(vec![1])
            )
            .is_ok()
        );
        assert!(
            crate::palw_panel::palw_da_built_answer_object_v1(
                &job.network_domain,
                Hash64::default(),
                PalwDaUnitV1::LegacyHeldV2(unit),
                built.clone(),
                bond,
                PalwPromptIdsFormV1::Flat,
                u64::MAX,
                |_, _| panic!("wrong form refused before signing")
            )
            .is_err()
        );
        assert!(
            crate::palw_panel::palw_da_built_answer_object_v1(
                &job.network_domain,
                Hash64::default(),
                PalwDaUnitV1::LegacyHeldV2(unit),
                built,
                bond,
                form,
                0,
                |_, _| panic!("size refused before signing")
            )
            .is_err()
        );
    }

    #[test]
    fn public_fp_input_replays_on_a_fresh_model_and_retains_its_own_pins() {
        let payload = payload();
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let form = backend.prompt_ids_form();
        let input = palw_fraud_filer_fp_input_v1(&backend, &payload, form, &[]).unwrap();
        let ceiling = backend.fp_job_context_v1(&input.job).unwrap();
        let run = palw_fraud_filer_execute_v1(Box::new(backend), ceiling, input.prompt_ids, form, Some(true), Some(input.job), None)
            .unwrap();
        let c = &payload.commitment;
        assert_eq!((run.execution_root, run.trace_root, run.output_root), (c.execution_root, c.trace_root, c.output_root));
        let replica = run.legacy.unwrap();
        assert_eq!(replica.roots.job_pin, Some(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(c)));
        assert_eq!(replica.roots.output_root, Some(c.output_root));
        assert_eq!(replica.roots.attempt_draw, None);
        assert!(!replica.capture.is_empty());
        // Even a producer claiming a shorter execution does not choose the verifier's replay context or own root.
        let mut short = payload.clone();
        short.commitment.decode_tokens_executed = 1;
        short.commitment.work_leaves = 1;
        short.commitment.execution_root = Hash64::from_u64_word(0xBAD);
        let fresh = crate::palw_panel::seat_s_tests::floor_backend();
        let input = palw_fraud_filer_fp_input_v1(&fresh, &short, form, &[]).unwrap();
        let ceiling = fresh.fp_job_context_v1(&input.job).unwrap();
        assert_eq!(ceiling.exact_decode_tokens, input.job.decode_token_limit);
        let own = palw_fraud_filer_execute_v1(Box::new(fresh), ceiling, input.prompt_ids, form, None, Some(input.job), None).unwrap();
        assert_eq!(own.execution_root, c.execution_root);
        assert_ne!(own.execution_root, short.commitment.execution_root);
        assert_eq!(own.legacy.as_ref().unwrap().roots.job_pin, replica.roots.job_pin);
        let mut tampered = payload.clone();
        tampered.prompt_token_ids[0] ^= 1;
        assert!(palw_fraud_filer_fp_input_v1(replica.backend.as_ref(), &tampered, form, &[]).is_err());
    }

    #[test]
    fn panel_input_requires_the_exact_job_and_authenticated_ids_without_reading_a_capture_tail() {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let form = backend.prompt_ids_form();
        let mut payload = payload();
        let ids = std::mem::take(&mut payload.prompt_token_ids);
        payload.commitment.job.privacy_mode = PALW_FP_PRIVACY_PANEL_DA;
        assert!(palw_fraud_filer_fp_input_v1(&backend, &payload, form, &[]).unwrap_err().contains("not available yet"));
        let material = PalwFpMaterialV1 { job: payload.commitment.job.clone(), prompt_token_ids: ids.clone() };
        let encode = |magic: [u8; 4], m: &PalwFpMaterialV1, width: usize| {
            let mut bytes = magic.to_vec();
            bytes.extend(borsh::to_vec(m).unwrap());
            if width != 0 {
                bytes.extend(3u32.to_le_bytes());
                bytes.extend(vec![0xFF; 3 * width]);
            }
            bytes
        };
        for (magic, width) in [(PALW_FP_MATERIAL_V1_MAGIC, 0), (PALW_FP_CAPTURE_V1_MAGIC, 1), (PALW_FP_ANSWER_V1_MAGIC, 4)] {
            let bytes = encode(magic, &material, width);
            assert_eq!(palw_fraud_filer_fp_input_v1(&backend, &payload, form, &[bytes.clone()]).unwrap().prompt_ids, ids);
            assert!(
                public_material_input(&bytes[..bytes.len() - 1], &borsh::to_vec(&material.job).unwrap(), &material.job, form)
                    .is_none(),
                "truncated wire is refused"
            );
            let mut extra = bytes;
            extra.push(0);
            assert!(
                public_material_input(&extra, &borsh::to_vec(&material.job).unwrap(), &material.job, form).is_none(),
                "trailing wire is refused"
            );
        }
        let mut hostile_length = PALW_FP_MATERIAL_V1_MAGIC.to_vec();
        hostile_length.extend(borsh::to_vec(&material.job).unwrap());
        hostile_length.extend(u32::MAX.to_le_bytes());
        assert!(public_material_input(&hostile_length, &borsh::to_vec(&material.job).unwrap(), &material.job, form).is_none());
        let mut wrong_job = material.clone();
        wrong_job.job.job_nonce[0] ^= 1;
        let mut wrong_ids = material.clone();
        wrong_ids.prompt_token_ids[0] ^= 1;
        assert!(
            palw_fraud_filer_fp_input_v1(
                &backend,
                &payload,
                form,
                &[encode(PALW_FP_MATERIAL_V1_MAGIC, &wrong_job, 0), encode(PALW_FP_MATERIAL_V1_MAGIC, &wrong_ids, 0),]
            )
            .is_err()
        );
        payload.commitment.job.privacy_mode = 255;
        assert!(palw_fraud_filer_fp_input_v1(&backend, &payload, form, &[]).is_err());
    }

    #[test]
    fn canonical_fp_input_is_derived_from_its_job_anchor_and_never_a_served_prompt() {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let form = backend.prompt_ids_form();
        let mut payload = payload();
        payload.prompt_token_ids.clear();
        let job = &mut payload.commitment.job;
        job.prompt_mode = PALW_FP_PROMPT_MODE_CANONICAL;
        let (ctx, ids) = backend.job_for_anchor(fp_canonical_anchor_v1(job)).unwrap();
        let ids: Vec<u32> = ids.into_iter().map(|id| id as u32).collect();
        job.prompt_tokens = ctx.declared_prefill_tokens;
        job.prompt_token_ids_hash = prompt_token_ids_commitment_v1(form, &ids).unwrap();
        assert_eq!(palw_fraud_filer_fp_input_v1(&backend, &payload, form, &[vec![0xFF; 10]]).unwrap().prompt_ids, ids);
        payload.commitment.job.prompt_token_ids_hash = Hash64::from_u64_word(999);
        assert!(palw_fraud_filer_fp_input_v1(&backend, &payload, form, &[]).is_err());
    }
}
