use super::*;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_decode_constraint_v1::*;
use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
use kaspa_consensus_core::palw_freeprompt_v3::*;
use kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1;

const RAW: &[u8] = br#"{"model":{"vocab":{"a":0,"b":1},"merges":[]}}"#;

// Use an actual floor disclosure for the profile/legs, then commit the prospective
// registered constraint descriptor. No producer capture or injected court verdict is read.
fn prospective_table(table_fault: bool) -> (PalwOffenceTargetV1, PalwStepBindingV2, PalwFreePromptJobV3, PalwTokenTableV1) {
    let backend = crate::palw_panel::seat_s_tests::floor_backend();
    let (mut job, ids) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
    let run = backend.execute_free_prompt(&job, &ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
    let event = backend.disclose_trace_event(&run.outcome.material, 0, 0).unwrap();
    let mut binding = event.binding().clone();
    job.tokenizer_id = palw_tokenizer_raw_commitment_v1(RAW);
    let mut table = PalwTokenTableV1 {
        entries: vec![None; binding.shape_profile.vocab_size as usize],
        eog_token_ids: vec![1],
        tokenizer_id: job.tokenizer_id,
    };
    table.entries[0] = Some(b"a".to_vec());
    table.entries[1] = Some(b"b".to_vec());
    if table_fault {
        let last = table.entries.len() - 1;
        table.entries[last] = Some(b"forged last rendering".to_vec());
    }
    let constraint = PalwDecodeConstraintV1 {
        version: 1,
        compiler_id: Hash64::from_u64_word(0xC018),
        start_frame: 0,
        frames: vec![PalwConstraintFrameV1 {
            start: 0,
            nodes: vec![PalwConstraintNodeV1 {
                accepting: true,
                edges: vec![PalwConstraintEdgeV1 { lo: 0, hi: 255, action: PalwConstraintActionV1::Goto(0) }],
            }],
        }],
    };
    job.version = PALW_FP_CONSTRAINT_VERSION;
    job.decode = None;
    job.tail =
        Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint: constraint.to_bytes(), table_root: table.root() }));
    binding.job_context = backend.fp_job_context_for_executed_v1(&job, binding.job_context.exact_decode_tokens).unwrap();
    binding.activation_leg_root = kaspa_consensus_core::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&binding.job_context);
    binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&binding);
    kaspa_consensus_core::palw_step_leg::verify_binding_v1(&binding).unwrap();
    let mut semantics = kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(
        job.class_id,
        Hash64::from_u64_word(0xA018),
        &binding.job_context,
        300,
    );
    semantics.token_table_root = Some(table.root());
    semantics.rendering_rule = Some(PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1);
    semantics.raw_tokenizer_bytes = Some(RAW.len() as u64);
    let target = PalwOffenceTargetV1 {
        claim_id: Hash64::from_u64_word(0xC018),
        class_id: job.class_id,
        artifact_root: semantics.artifact_root,
        executor_bond: PalwBondKeyV2(job.executor_bond),
        execution_root: binding.committed_execution_root,
        trace_root: binding.full_logits_trace_root,
        output_root: run.outcome.output_root,
        lane: Some(PalwClaimSourceKindV1::FreePrompt),
        segment_count: Some(1),
        phase: None,
        job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
        registered_semantics: Some(semantics),
    };
    (target, binding, job, table)
}

#[test]
fn held_raw_file_checks_all_renderings_and_preserves_a_normal_objective_proof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tokenizer.json");
    std::fs::write(&path, RAW).unwrap();
    for fault in [false, true] {
        let (target, binding, job, table) = prospective_table(fault);
        let checked = check_held_tokenizer_v1(&path, &target, &binding, &job, &table, 100_000).unwrap();
        assert_eq!(checked._reservation.bytes(), RAW.len() as u64 * 128 + 200_000 + 16_384);
        match checked.fault {
            None => assert!(!fault, "the whole table, including its last entry, must pass"),
            Some(ref contradiction) => {
                assert!(fault);
                let PalwPanelContradictionV1::TokenizerTableMismatch { tokenizer_bytes, table_opening, .. } = contradiction else {
                    panic!("the held rendering check has a specific objective contradiction");
                };
                assert_eq!(tokenizer_bytes, RAW);
                assert_eq!(table_opening.id, binding.shape_profile.vocab_size - 1);
                assert!(palw_tokenizer_table_fault_v1(&target, &binding, &job, tokenizer_bytes, table_opening).unwrap());
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    target.claim_id,
                    target.executor_bond,
                    contradiction.clone(),
                    None,
                    100_000,
                )
                .unwrap();
                assert!(matches!(
                    built.object,
                    kaspa_consensus_core::palw_v2::PalwConsensusObjectV2::ObjectiveOffence {
                        kind: kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted,
                        ..
                    }
                ));
            }
        }
    }
}

#[test]
fn missing_wrong_or_over_budget_own_material_is_pending_but_false_registered_length_is_guilt() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tokenizer.json");
    let (mut target, binding, job, table) = prospective_table(false);
    assert!(check_held_tokenizer_v1(&path, &target, &binding, &job, &table, 100_000).is_err());
    std::fs::write(&path, b"wrong local bytes").unwrap();
    assert!(check_held_tokenizer_v1(&path, &target, &binding, &job, &table, 100_000).is_err());
    std::fs::write(&path, RAW).unwrap();
    assert!(check_held_tokenizer_v1(&path, &target, &binding, &job, &table, RAW.len() - 1).is_err());
    let mut other_table = table.clone();
    other_table.entries[0] = Some(b"uncommitted".to_vec());
    assert!(check_held_tokenizer_v1(&path, &target, &binding, &job, &other_table, 100_000).is_err());
    target.registered_semantics.as_mut().unwrap().raw_tokenizer_bytes = Some(RAW.len() as u64 - 1);
    assert!(check_held_tokenizer_v1(&path, &target, &binding, &job, &table, 100_000).unwrap().fault.is_some());
}
