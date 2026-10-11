//! Public constrained selection proofs. Only the recorded target, authenticated public pin,
//! descriptor and this host's held public tokenizer table enter the decision.

use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
use kaspa_consensus_core::palw_offence_attribution_v1::{PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES, PalwOffenceTargetV1};
use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, PalwStepMerkleTreeV1, PalwStepOpeningV1};
use kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1;
use std::collections::BTreeSet;
use std::sync::Arc;

pub(super) fn flat_contradiction_v1(
    target: &PalwOffenceTargetV1,
    binding: &PalwStepBindingV2,
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    pin: &PalwBase0DecodeTokensV1,
    table: Arc<PalwTokenTableV1>,
) -> Result<Option<PalwPanelContradictionV1>, String> {
    kaspa_consensus_core::palw_offence_attribution_v1::palw_constrained_job_target_v1(target, binding, job)
        .map_err(|e| e.to_string())?;
    if table.vocab() != binding.shape_profile.vocab_size {
        return Err("the held token table is not the registered vocabulary wide".into());
    }
    let ceiling = PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES as usize;
    // Bound before cloning the potentially large job/pin into an offence. Multipart terminal
    // carriage is required when even these mandatory fields exceed the direct evidence ceiling.
    let base_bytes = borsh::object_length(job)
        .map_err(|e| e.to_string())?
        .saturating_add(borsh::object_length(pin).map_err(|e| e.to_string())?)
        .saturating_add(borsh::object_length(binding).map_err(|e| e.to_string())?);
    if base_bytes > ceiling {
        return Err("the constrained proof requires multipart terminal carriage".into());
    }
    kaspa_consensus_core::palw_step_refute::check_base0_decode_pin(binding, pin).map_err(|e| e.to_string())?;
    let mask = PalwConstraintMaskV1::for_job(job, table.clone())?;
    let sampling = job.sampling_v2();
    let mut state = mask.state_after(&[]).ok_or("the constraint has no initial state")?;
    for (position, (&committed, row)) in pin.generated_token_ids.iter().zip(&pin.logits_rows).enumerate() {
        let (selected, stop) = mask.select(Some(&state), row, position as u32, &sampling);
        if committed != selected {
            let admitted = mask.admit(&state, committed).is_some();
            let witness = (!admitted && !stop).then_some(selected);
            let beat_lane = admitted.then_some(selected);
            let mut ids: BTreeSet<u32> = pin.generated_token_ids[..=position].iter().copied().collect();
            ids.extend(witness);
            ids.extend(beat_lane);
            let leaves = table.leaf_hashes();
            let tree = PalwStepMerkleTreeV1::build_capped_v1(&leaves, PALW_TOKEN_TABLE_MAX_LEAVES_V1).map_err(|e| e.to_string())?;
            let opening = |id: u32| -> Result<PalwStepOpeningV1, String> {
                Ok(PalwStepOpeningV1 {
                    leaf_index: u64::from(id),
                    leaf_hash: *leaves.get(id as usize).ok_or("a lane is outside the table")?,
                    siblings: tree.path_v1(id as usize).map_err(|e| e.to_string())?,
                })
            };
            let mut eog_ids = table.eog_token_ids.clone();
            eog_ids.sort_unstable();
            let eog = PalwTokenTableEogOpeningV1 { ids: eog_ids, opening: opening(table.vocab())? };
            let mut bytes = base_bytes.saturating_add(borsh::object_length(&eog).map_err(|e| e.to_string())?);
            let mut renderings = Vec::new();
            for id in ids {
                let entry = table.entries.get(id as usize).ok_or("the committed token is past the vocabulary")?;
                let path = opening(id)?;
                bytes = bytes
                    .saturating_add(entry.as_ref().map_or(0, Vec::len))
                    .saturating_add(borsh::object_length(&path).map_err(|e| e.to_string())?)
                    .saturating_add(16);
                if bytes > ceiling {
                    return Err("the constrained proof requires multipart terminal carriage".into());
                }
                renderings.push(PalwTokenTableOpeningV1 { id, rendering: entry.clone(), opening: path });
            }
            let accusation = PalwConstrainedFlatDecodeAccusationV1 {
                pin: pin.clone(),
                position: position as u32,
                job: job.clone(),
                renderings,
                eog,
                witness,
                beat_lane,
            };
            kaspa_consensus_core::palw_step_refute::check_constrained_flat_decode_token_v1(
                binding,
                &accusation,
                binding.step_leaf_count,
            )
            .map_err(|e| e.to_string())?;
            return Ok(Some(PalwPanelContradictionV1::ConstrainedDecodeFlat {
                binding: binding.clone(),
                accusation: Box::new(accusation),
            }));
        }
        if stop {
            return Ok(None);
        }
        state = mask.admit(&state, committed).ok_or("the selected prefix is not admitted")?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use kaspa_consensus_core::palw_step_refute::{PalwTraceEventDisclosureV1, base0_logits_trace_root_v1};

    #[test]
    fn public_flat_constraint_proof_finds_later_forbidden_stop_and_beaten_tokens_and_refuses_forgery() {
        use kaspa_consensus_core::palw_decode_constraint_v1::*;
        use kaspa_consensus_core::palw_offence_attribution_v1::*;
        use kaspa_consensus_core::palw_offence_v1::PalwOffenceVerifyError as E;
        use kaspa_consensus_core::palw_state_v2::PalwChainStateV2;
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let (mut job, prompt) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        let constraint = PalwDecodeConstraintV1 {
            version: 1,
            compiler_id: Hash64::from_u64_word(0xC015),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![PalwConstraintNodeV1 {
                    accepting: true,
                    edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
                }],
            }],
        };
        let vocab = misaka_palw_base0::rc::palw_rc_base0_shape_v1().vocab;
        let mut table = PalwTokenTableV1 {
            entries: vec![Some(b"b".to_vec()); vocab],
            eog_token_ids: vec![(vocab - 1) as u32],
            tokenizer_id: job.tokenizer_id,
        };
        table.entries[0] = Some(b"a".to_vec());
        table.entries[1] = Some(b"a".to_vec());
        let table = Arc::new(table);
        let root = palw_token_tables_v1().register(table.as_ref().clone());
        job.version = PALW_FP_CONSTRAINT_VERSION;
        job.decode = None;
        job.tail = Some(kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 {
            constraint: constraint.to_bytes(),
            table_root: root,
        }));
        let mask = PalwConstraintMaskV1::for_job(&job, table.clone()).unwrap();
        let executed = palw_fp_with_constraint_scope_v1(Some(mask), || {
            backend.execute_free_prompt(&job, &prompt.iter().map(|id| *id as usize).collect::<Vec<_>>())
        })
        .unwrap();
        let PalwTraceEventDisclosureV1::Flat { binding: original, pin: original_pin } =
            backend.disclose_trace_event(&executed.outcome.material, 0, 0).unwrap()
        else {
            panic!("floor is flat")
        };
        assert!(original_pin.generated_token_ids.len() > 2);
        let rules = PalwIdentityRulesV1 {
            prompt_ids_form: backend.prompt_ids_form(),
            base_class_id: job.class_id,
            da_signer_liability: false,
        };
        for case in 0..4 {
            let mut binding = *original.clone();
            let mut pin = original_pin.clone();
            if case == 1 {
                pin.generated_token_ids[2] = 2;
            }
            if case == 2 {
                pin.generated_token_ids[2] = (vocab - 1) as u32;
            }
            if case == 3 {
                pin.logits_rows[2][0] = 10;
                pin.logits_rows[2][1] = 20;
                pin.generated_token_ids[2] = 0;
            }
            binding.full_logits_trace_root =
                base0_logits_trace_root_v1(&binding.job_context, &pin.logits_rows, &pin.generated_token_ids);
            binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
            kaspa_consensus_core::palw_step_leg::verify_binding_v1(&binding).unwrap();
            let target = PalwOffenceTargetV1 {
                claim_id: Hash64::from_u64_word(0xC015),
                class_id: job.class_id,
                artifact_root: Hash64::default(),
                executor_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(job.executor_bond),
                execution_root: binding.committed_execution_root,
                lane: Some(PalwClaimSourceKindV1::FreePrompt),
                segment_count: Some(4),
                phase: None,
                job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
                trace_root: binding.full_logits_trace_root,
                output_root: kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(
                    &binding.job_context,
                    &pin.generated_token_ids,
                ),
            };
            let proof = flat_contradiction_v1(&target, &binding, &job, &pin, table.clone()).unwrap();
            let answer = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(
                PalwTraceEventDisclosureV1::Flat { binding: Box::new(binding.clone()), pin: pin.clone() },
            ));
            let filing = super::super::palw_fraud_filer_public_filing_with_fp_v2(&target, rules, &answer, None, Some(&job)).unwrap();
            if case == 0 {
                assert!(proof.is_none());
                assert!(filing.is_none());
                continue;
            }
            let proof = proof.unwrap();
            let PalwPanelContradictionV1::ConstrainedDecodeFlat { accusation, .. } = &proof else { panic!("flat contradiction") };
            assert_eq!(accusation.position, 2);
            assert_eq!(borsh::to_vec(&proof).unwrap()[0], 15);
            let filing = filing.expect("actual public filer builder produces a reporter filing");
            let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = &filing.object else {
                panic!("offence")
            };
            assert!(evidence.len() as u64 <= PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
            assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_constrained_decode_v1(&filing.object));
            let decoded: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(evidence).unwrap();
            assert_eq!(decoded.contradiction, proof);
            let empty = PalwChainStateV2::genesis();
            assert!(matches!(
                palw_check_executor_refuted_at_v1(
                    &empty,
                    &target.executor_bond,
                    evidence,
                    true,
                    false,
                    rules,
                    PalwSessionRuleV1::RefusedUnderSession
                ),
                Err(E::ContradictionNotAdmitted(_))
            ));
            let mut wrong_pin = pin.clone();
            wrong_pin.logits_rows[0][0] ^= 1;
            assert!(flat_contradiction_v1(&target, &binding, &job, &wrong_pin, table.clone()).is_err());
            let mut wrong_job = job.clone();
            wrong_job.job_nonce[0] ^= 1;
            assert!(flat_contradiction_v1(&target, &binding, &wrong_job, &pin, table.clone()).is_err());
            let mut broken = accusation.as_ref().clone();
            broken.eog.opening.leaf_hash = Hash64::default();
            assert!(
                kaspa_consensus_core::palw_step_refute::check_constrained_flat_decode_token_v1(
                    &binding,
                    &broken,
                    binding.step_leaf_count
                )
                .is_err()
            );
        }
    }
}
