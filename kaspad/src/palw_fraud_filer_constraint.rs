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
        if committed != selected || (stop && position + 1 < pin.generated_token_ids.len()) {
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
            kaspa_consensus_core::palw_step_refute::check_constrained_flat_decode_token_at_v2(
                binding,
                &accusation,
                binding.step_leaf_count,
                true,
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

/// At most two public tiles of one challenged position. Own replay IDs only nominate a
/// candidate; public authentication and the court rule independently decide conviction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TiledPlanV1 {
    pub position: u32,
    pub committed: u32,
    pub witness: Option<u32>,
    pub beat_lane: Option<u32>,
}

impl TiledPlanV1 {
    pub fn units(self) -> Vec<kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1> {
        use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
        use kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES;
        let tile = self.beat_lane.map_or(0, |_| self.committed as usize / PALW_LOGITS_TILE_LANES) as u8;
        let mut units = vec![PalwDaUnitV1::Event { row: self.position, tile }];
        if let Some(beat) = self.beat_lane {
            let other = PalwDaUnitV1::Event { row: self.position, tile: (beat as usize / PALW_LOGITS_TILE_LANES) as u8 };
            if !units.contains(&other) {
                units.push(other);
            }
        }
        units
    }
}

pub(super) fn tiled_plan_v1(
    target: &PalwOffenceTargetV1,
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    initial: &kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
    own_ids: &[u32],
    table: Arc<PalwTokenTableV1>,
) -> Result<Option<TiledPlanV1>, String> {
    use kaspa_consensus_core::palw_step_refute::*;
    let PalwTraceEventDisclosureV1::Tiled { binding, generated_token_ids, .. } = initial else { return Ok(None) };
    kaspa_consensus_core::palw_offence_attribution_v1::palw_constrained_job_target_v1(target, binding, job)
        .map_err(|e| e.to_string())?;
    check_trace_event_disclosure_v1(target.trace_root, target.execution_root, 0, 0, initial, binding.step_leaf_count)
        .map_err(|e| e.to_string())?;
    if table.vocab() != binding.shape_profile.vocab_size {
        return Err("the held token table has another vocabulary width".into());
    }
    if table.vocab() as usize > 256 * PALW_LOGITS_TILE_LANES {
        return Err("the vocabulary exceeds the public event tile index space".into());
    }
    let mask = PalwConstraintMaskV1::for_job(job, table)?;
    let mut state = mask.state_after(&[]).ok_or("the constraint has no initial state")?;
    let mut same_prefix = true;
    for (position, &committed) in generated_token_ids.iter().enumerate() {
        if committed >= binding.shape_profile.vocab_size {
            return Err("the committed token is past the vocabulary".into());
        }
        if let Some(next) = mask.admit(&state, committed) {
            if same_prefix
                && let Some(&own) = own_ids.get(position)
                && own != committed
                && mask.admit(&state, own).is_some()
            {
                return Ok(Some(TiledPlanV1 { position: position as u32, committed, witness: None, beat_lane: Some(own) }));
            }
            same_prefix &= own_ids.get(position) == Some(&committed);
            state = next;
        } else {
            let witness = (0..mask.table.vocab()).find(|&id| mask.admit(&state, id).is_some());
            if Some(committed) == mask.table.lowest_eog_id() && witness.is_none() && position + 1 == generated_token_ids.len() {
                return Ok(None);
            }
            return Ok(Some(TiledPlanV1 { position: position as u32, committed, witness, beat_lane: None }));
        }
    }
    Ok(None)
}

pub(super) fn tiled_contradiction_v1(
    target: &PalwOffenceTargetV1,
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    plan: TiledPlanV1,
    events: &[kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1],
    table: Arc<PalwTokenTableV1>,
) -> Result<Option<PalwPanelContradictionV1>, String> {
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
    use kaspa_consensus_core::palw_step_refute::*;
    let units = plan.units();
    if events.len() != units.len() {
        return Err("the selected public tiles are incomplete".into());
    }
    let first = events.first().ok_or("no selected public row")?;
    let binding = first.binding();
    kaspa_consensus_core::palw_offence_attribution_v1::palw_constrained_job_target_v1(target, binding, job)
        .map_err(|e| e.to_string())?;
    if table.vocab() != binding.shape_profile.vocab_size {
        return Err("the held token table has another vocabulary width".into());
    }
    // Also check the host table's tokenizer/root before generating openings.
    let _mask = PalwConstraintMaskV1::for_job(job, table.clone())?;
    for (unit, event) in units.iter().zip(events) {
        let PalwDaUnitV1::Event { row, tile } = *unit else { unreachable!() };
        if event.binding() != binding {
            return Err("the selected tiles carry different bindings".into());
        }
        check_trace_event_disclosure_v1(target.trace_root, target.execution_root, row, tile, event, binding.step_leaf_count)
            .map_err(|e| e.to_string())?;
    }
    let PalwTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } = first else {
        return Err("the selected event is not tiled".into());
    };
    if generated_token_ids.get(plan.position as usize) != Some(&plan.committed) {
        return Err("the selected token changed".into());
    }
    let base = borsh::object_length(job)
        .map_err(|e| e.to_string())?
        .saturating_add(borsh::object_length(binding).map_err(|e| e.to_string())?)
        .saturating_add(events.iter().map(|e| borsh::object_length(e).unwrap_or(usize::MAX)).fold(0usize, usize::saturating_add));
    let ceiling = PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES as usize;
    if base > ceiling {
        return Err("the constrained proof requires multipart terminal carriage".into());
    }
    let (beat_lanes, beat_opening) = if plan.beat_lane.is_some() {
        let PalwTraceEventDisclosureV1::Tiled {
            generated_token_ids: other_ids, row_root: other_root, tile_lanes, tile_opening, ..
        } = events.last().unwrap()
        else {
            return Err("the beating event is not tiled".into());
        };
        if other_ids != generated_token_ids || other_root != row_root {
            return Err("the selected public tiles disagree on their row".into());
        }
        (tile_lanes.clone(), tile_opening.clone())
    } else {
        (Vec::new(), row_opening.clone())
    };
    let pin = PalwTiledDecodePinV1 {
        position: plan.position,
        generated_token_ids: generated_token_ids.clone(),
        row_root: *row_root,
        row_opening: row_opening.clone(),
        committed_tile_lanes: if plan.beat_lane.is_some() { tile_lanes.clone() } else { Vec::new() },
        committed_opening: if plan.beat_lane.is_some() { tile_opening.clone() } else { row_opening.clone() },
        beat_tile_lanes: beat_lanes,
        beat_opening,
        beat_lane: plan.beat_lane.unwrap_or(0),
    };
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
    let mut ids: BTreeSet<u32> = generated_token_ids[..=plan.position as usize].iter().copied().collect();
    ids.extend(plan.witness);
    ids.extend(plan.beat_lane);
    let mut bytes = base.saturating_add(borsh::object_length(&eog).map_err(|e| e.to_string())?);
    let mut renderings = Vec::new();
    for id in ids {
        let entry = table.entries.get(id as usize).ok_or("the token is past the vocabulary")?;
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
    let accusation = PalwConstrainedDecodeAccusationV1 { pin, job: job.clone(), renderings, eog, witness: plan.witness };
    match check_constrained_decode_token_at_v2(binding, &accusation, binding.step_leaf_count, true) {
        Ok(_) => Ok(Some(PalwPanelContradictionV1::ConstrainedDecode { binding: binding.clone(), accusation: Box::new(accusation) })),
        Err(PalwStepRefuteError::NoFaultFound) => Ok(None),
        Err(why) => Err(why.to_string()),
    }
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
        for case in 0..5 {
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
            if case == 4 {
                pin.generated_token_ids[2] = vocab as u32;
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
                registered_semantics: None,
                    &binding.job_context,
                    &pin.generated_token_ids,
                ),
            };
            let proof = if case == 4 { None } else {
                flat_contradiction_v1(&target, &binding, &job, &pin, table.clone()).unwrap()
            };
            let answer = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(
                PalwTraceEventDisclosureV1::Flat { binding: Box::new(binding.clone()), pin: pin.clone() },
            ));
            let filing = super::super::palw_fraud_filer_public_filing_with_fp_v2(&target, rules, &answer, None, Some(&job)).unwrap();
            if case == 0 {
                assert!(proof.is_none());
                assert!(filing.is_none());
                continue;
            }
            if case == 4 {
                let filing = filing.expect("out-of-vocab pin files directly without rendering an invalid ID");
                let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = &filing.object else {
                    panic!("offence")
                };
                let decoded: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(evidence).unwrap();
                assert!(matches!(decoded.contradiction, PalwPanelContradictionV1::ForgedOutput { position: 2, .. }));
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
    #[test]
    fn tiled_public_builder_finds_later_faults_across_tiles_and_authenticates_every_input() {
        use kaspa_consensus_core::palw_decode_constraint_v1::*;
        use kaspa_consensus_core::palw_step_refute::*;
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let (mut job, prompt) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        let run = backend.execute_free_prompt(&job, &prompt.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        let mut template = backend.disclose_trace_event(&run.outcome.material, 0, 0).unwrap().binding().clone();
        let vocab = 2 * PALW_LOGITS_TILE_LANES + 3;
        let mut table = PalwTokenTableV1 {
            entries: vec![Some(b"b".to_vec()); vocab],
            eog_token_ids: vec![(vocab - 1) as u32],
            tokenizer_id: job.tokenizer_id,
        };
        for id in [0, 1, PALW_LOGITS_TILE_LANES] {
            table.entries[id] = Some(b"a".to_vec());
        }
        let table = Arc::new(table);
        let constraint = PalwDecodeConstraintV1 {
            version: 1,
            compiler_id: Hash64::from_u64_word(0xC014),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![PalwConstraintNodeV1 {
                    accepting: true,
                    edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
                }],
            }],
        };
        template.shape_profile.logits_scheme_id = tiled_logits_scheme_id_v1();
        template.shape_profile.vocab_size = vocab as u32;
        job.version = PALW_FP_CONSTRAINT_VERSION;
        job.class_id = template.shape_profile.shape_profile_id();
        job.decode = None;
        job.tail = Some(kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 {
            constraint: constraint.to_bytes(),
            table_root: table.root(),
        }));
        let class = kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3 {
            model_profile_id: template.job_context.model_profile_id,
            runtime_manifest_hash: template.job_context.runtime_manifest_hash,
            runtime_class_id: template.job_context.runtime_class_id,
            shape_profile_id: job.class_id,
            cu_ruleset_id: template.job_context.cu_ruleset_id,
        };
        template.job_context = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_context_v3(
            &job,
            &class,
            &kaspa_consensus_core::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(&job, job.decode_token_limit),
            &template.job_context.network_id,
        )
        .unwrap();
        template.activation_leg_root = kaspa_consensus_core::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&template.job_context);
        let own = vec![0; job.decode_token_limit as usize];
        for fault in 0..7 {
            let mut ids = own.clone();
            let mut rows = vec![vec![0; vocab]; ids.len()];
            for row in &mut rows {
                row[0] = 10;
            }
            if fault == 1 {
                ids[2] = 2;
            }
            if fault == 2 {
                ids[2] = (vocab - 1) as u32;
            }
            let mut nominee = own.clone();
            if fault == 3 {
                rows[2][PALW_LOGITS_TILE_LANES] = 20;
                nominee[2] = PALW_LOGITS_TILE_LANES as u32;
            }
            if fault == 4 {
                nominee[2] = PALW_LOGITS_TILE_LANES as u32;
            }
            if fault == 5 {
                ids[2] = vocab as u32;
            }
            if fault == 6 {
                // An automaton accepting only the empty string has no admitted rendering.
                // Its first EOG is a real stop; the authenticated suffix is nevertheless fraud.
                let terminal = PalwDecodeConstraintV1 {
                    version: 1, compiler_id: Hash64::from_u64_word(0xC016), start_frame: 0,
                    frames: vec![PalwConstraintFrameV1 { start: 0,
                        nodes: vec![PalwConstraintNodeV1 { accepting: true, edges: vec![] }] }],
                };
                let mut stopped_job = job.clone();
                let Some(kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1::Constraint(tail)) = &mut stopped_job.tail else {
                    unreachable!()
                };
                tail.constraint = terminal.to_bytes();
                for suffix in [false, true] {
                    let stopped_ids = if suffix { vec![(vocab - 1) as u32, 0] } else { vec![(vocab - 1) as u32] };
                    let stopped_rows = &rows[..stopped_ids.len()];
                    let mut stopped_binding = template.clone();
                    stopped_binding.job_context = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_context_v3(
                        &stopped_job, &class,
                        &kaspa_consensus_core::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(
                            &stopped_job, stopped_ids.len() as u32), &template.job_context.network_id).unwrap();
                    stopped_binding.full_logits_trace_root = tiled_logits_trace_root_v1(
                        &stopped_binding.job_context, stopped_rows, &stopped_ids).unwrap();
                    stopped_binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&stopped_binding);
                    let stopped_target = PalwOffenceTargetV1 {
                        claim_id: Hash64::from_u64_word(0xC016), class_id: job.class_id, artifact_root: Hash64::default(),
                        executor_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(job.executor_bond),
                        execution_root: stopped_binding.committed_execution_root,
                        lane: Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt),
                        segment_count: Some(4), phase: None,
                        job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&stopped_binding.job_context),
                        trace_root: stopped_binding.full_logits_trace_root,
                        output_root: kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&stopped_binding.job_context, &stopped_ids),
                        registered_semantics: None,
                    };
                    let stopped_event = logits_event_disclosure_v1(&stopped_binding, stopped_rows, &stopped_ids, 0, 0).unwrap();
                    let planned = tiled_plan_v1(&stopped_target, &stopped_job, &stopped_event, &[], table.clone()).unwrap();
                    if suffix {
                        let plan = planned.expect("a post-stop suffix is directly localizable");
                        assert_eq!(plan.position, 0);
                        assert_eq!(plan.witness, None);
                        assert_eq!(plan.units().len(), 1);
                        let proof = tiled_contradiction_v1(&stopped_target, &stopped_job, plan, &[stopped_event], table.clone())
                            .unwrap().expect("public EOG and IDs suffice without a selection witness");
                        assert!(matches!(proof, PalwPanelContradictionV1::ConstrainedDecode { .. }));
                    } else { assert!(planned.is_none(), "an honest terminal EOG clears"); }
                }
                continue;
            }
            let mut binding = template.clone();
            binding.full_logits_trace_root = tiled_logits_trace_root_v1(&binding.job_context, &rows, &ids).unwrap();
            binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
            kaspa_consensus_core::palw_step_leg::verify_binding_v1(&binding).unwrap();
            let target = PalwOffenceTargetV1 {
                claim_id: Hash64::from_u64_word(0xC014),
                class_id: job.class_id,
                artifact_root: Hash64::default(),
                executor_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(job.executor_bond),
                execution_root: binding.committed_execution_root,
                lane: Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt),
                segment_count: Some(4),
                phase: None,
                job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
                trace_root: binding.full_logits_trace_root,
                output_root: kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &ids),
                registered_semantics: None,
            };
            let initial = logits_event_disclosure_v1(&binding, &rows, &ids, 0, 0).unwrap();
            if fault == 5 {
                let answer = crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(
                    kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(initial.clone()));
                let rules = kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
                    prompt_ids_form: backend.prompt_ids_form(), base_class_id: Hash64::default(), da_signer_liability: false,
                };
                let filing = super::super::palw_fraud_filer_public_filing_with_fp_v2(&target, rules, &answer, None, Some(&job))
                    .unwrap().expect("invalid tiled ID requires no selected tile or table opening");
                let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = filing.object else {
                    panic!("offence")
                };
                let decoded: kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1 =
                    borsh::from_slice(&evidence).unwrap();
                assert!(matches!(decoded.contradiction, PalwPanelContradictionV1::ForgedOutputTiled {
                    proof: kaspa_consensus_core::palw_offence_v1::PalwForgedOutputTiledProofV1::OutOfVocab { position: 2, .. }, ..
                }));
                continue;
            }
            let plan = tiled_plan_v1(&target, &job, &initial, &nominee, table.clone()).unwrap();
            if fault == 0 {
                assert!(plan.is_none());
                continue;
            }
            let plan = plan.unwrap();
            assert_eq!(plan.position, 2);
            let events: Vec<_> = plan
                .units()
                .iter()
                .map(|unit| {
                    let kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::Event { row, tile } = *unit else { unreachable!() };
                    logits_event_disclosure_v1(&binding, &rows, &ids, row, tile).unwrap()
                })
                .collect();
            assert_eq!(events.len(), if fault >= 3 { 2 } else { 1 });
            // The real case book accepts only these selected authenticated tiles, and
            // retains the original binding/localizer while learning a later row.
            use super::super::{PalwFraudFilerBookV1, PalwFraudFilerCaseV1, PalwFraudFilerWalkV1};
            use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
            let mut candidate = super::super::tests::candidate_for_input_test();
            candidate.claim_id = target.claim_id;
            candidate.job.execution_root = target.execution_root;
            candidate.job.trace_root = target.trace_root;
            let mut case = PalwFraudFilerCaseV1::new(candidate);
            case.verdict = super::super::PalwFraudFilerVerdictV1::Mismatch(Arc::new(super::super::PalwFraudFilerRunV1 {
                execution_root: Hash64::default(),
                trace_root: Hash64::default(),
                output_root: Hash64::default(),
                own_range: Box::new(|_, _| Err("this proof does not use own ranges".into())),
                legacy: None,
                fp_job: Some(Box::new(job.clone())),
                own_output_ids: nominee.clone(),
                _job_reservation: None,
                _reservation: None,
            }));
            case.binding = Some(binding.clone());
            case.constraint_plan = Some(plan);
            let mut book = PalwFraudFilerBookV1::default();
            book.cases.insert(target.claim_id, case);
            let answer = |event| crate::palw_panel::PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event));
            let mut offered: Vec<_> = plan
                .units()
                .into_iter()
                .zip(events.iter().cloned())
                .map(|(unit, event)| ((target.claim_id, unit), answer(event)))
                .collect();
            let unsolicited = PalwDaUnitV1::Event { row: 1, tile: 1 };
            offered.push(((target.claim_id, unsolicited), answer(logits_event_disclosure_v1(&binding, &rows, &ids, 1, 1).unwrap())));
            book.walked(
                PalwFraudFilerWalkV1 { floor: 0, anchor: Hash64::from_u64_word(1), anchor_daa: 1, next: None },
                false,
                offered,
            );
            assert_eq!(book.answers.len(), plan.units().len());
            assert!(!book.answers.contains_key(&(target.claim_id, unsolicited)), "unselected public material cannot grow the cache");
            for (unit, event) in plan.units().iter().zip(&events) {
                let PalwDaUnitV1::Event { row, tile } = *unit else { unreachable!() };
                let case = book.cases.get_mut(&target.claim_id).unwrap();
                let prior_bisect = case.bisect.clone();
                case.learn(
                    kaspa_consensus_core::palw_legacy_public_filer_v1::PalwLegacyProbeV1::Binding { row, tile },
                    &answer(event.clone()),
                )
                .unwrap();
                assert_eq!(case.binding.as_ref(), Some(&binding));
                assert_eq!(case.bisect, prior_bisect);
            }
            let proof = tiled_contradiction_v1(&target, &job, plan, &events, table.clone()).unwrap();
            if fault == 4 {
                assert!(proof.is_none(), "an own-replay nomination is not a public conviction");
                continue;
            }
            let proof = proof.unwrap();
            assert_eq!(borsh::to_vec(&proof).unwrap()[0], 14);
            let PalwPanelContradictionV1::ConstrainedDecode { accusation, .. } = &proof else { panic!("tiled proof") };
            assert_eq!(accusation.pin.position, 2);
            assert!(
                kaspa_consensus_core::palw_offence_attribution_v1::palw_constrained_decode_fault_v1(
                    &target,
                    &binding,
                    accusation,
                    binding.step_leaf_count
                )
                .is_ok()
            );
            let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_v1(
                target.claim_id,
                target.executor_bond,
                proof,
                None,
            )
            .unwrap();
            let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = built.object else {
                panic!("offence")
            };
            assert!(evidence.len() as u64 <= PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
            let mut bad = events.clone();
            let PalwTraceEventDisclosureV1::Tiled { tile_lanes, .. } = bad.last_mut().unwrap() else { unreachable!() };
            tile_lanes[0] ^= 1;
            assert!(tiled_contradiction_v1(&target, &job, plan, &bad, table.clone()).is_err());
            let mut foreign = job.clone();
            foreign.job_nonce[0] ^= 1;
            assert!(tiled_contradiction_v1(&target, &foreign, plan, &events, table.clone()).is_err());
            assert!(tiled_contradiction_v1(&target, &job, plan, &events[..0], table.clone()).is_err());
            let mut other = table.as_ref().clone();
            other.entries[0] = Some(b"x".to_vec());
            assert!(tiled_contradiction_v1(&target, &job, plan, &events, Arc::new(other)).is_err());
        }
    }
}
