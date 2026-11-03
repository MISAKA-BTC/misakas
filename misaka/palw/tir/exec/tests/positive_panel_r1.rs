#![cfg(feature = "positive-panel")]
//! A3/A1 joint fixture uses A0's actual r1 identities and private framing.
mod positive_panel_r1_common;
use kaspa_consensus_core::{
    Hash64, palw_attempt_rules_v1::palw_attempt_output_root_v1, palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_court_v1::PalwTirConeRefutationV1,
};
use misaka_palw_positive_contract::{
    identity::*,
    messages::*,
    ports::{PanelBuilder, require_next},
    tir,
    wire::{EVIDENCE_HEADER_BYTES, EvidenceFrame, EvidenceHeader},
};
use misaka_palw_tir_exec::node::{
    positive_check::{CheckError, CheckOutcome},
    positive_panel::{PanelAdmission, TirPanelBuilder},
};
use positive_panel_r1_common::*;

#[test]
fn a0_panel_builder_emits_canonical_r1_frames_checked_by_cold_a1_core() {
    for wrong_result in [false, true] {
        with_fixture(wrong_result, |f, runner, inventory| {
            f.result.authenticate(&f.key, &f.signature).unwrap();
            let admitted = f.admit().unwrap();
            let descriptor = admitted.plan();
            let builder = TirPanelBuilder::new(runner, inventory);
            let capture = builder.replay_and_capture(&admitted).unwrap();
            assert_eq!(capture.generated(), EXPECTED);
            let overridden =
                misaka_palw_tir_exec::node::TirClassRunnerV1::new(runner.space, runner.plan, runner.params, runner.class_id)
                    .unwrap()
                    .with_selector(std::sync::Arc::new(|_, _, _| 3));
            let overridden_builder = TirPanelBuilder::new(&overridden, inventory);
            assert_eq!(overridden_builder.replay_and_capture(&admitted).unwrap().generated(), EXPECTED);
            assert_eq!(
                descriptor.plan,
                coverage_plan_id(
                    &f.result,
                    CheckProfile::ExactStreamV1,
                    tir::exact_derivation_id(),
                    capture.binding().step_leaf_count
                )
                .unwrap()
            );
            assert_eq!(f.result.selector, tir::greedy_selector_id());
            let witness = builder.commit(&capture, &descriptor, witness_author(), 0).unwrap();
            assert_eq!(witness.provenance, tir::provenance_id(&f.result));
            assert_eq!(witness.execution_record, digest(capture.binding().committed_execution_root));
            assert_eq!(witness.candidate_output, digest(palw_attempt_output_root_v1(&f.job, EXPECTED)));
            authenticate_witness(&witness, &f.result);
            let mut node = f.checker(capture.binding().clone(), capture.generated().to_vec());
            let mut cursor = CoverageCursor { plan: descriptor.plan, next_relation: 0 };
            let prepared = builder.prepare_parts(&capture).unwrap();
            for index in 0..descriptor.mandatory_relations {
                let part = prepared.build_part(CoverageCursor { plan: descriptor.plan, next_relation: index }).unwrap();
                let frame = part.frame(&witness, &descriptor).unwrap();
                assert_eq!(frame.header().first_relation, index);
                assert_eq!(frame.header().relation_count, 1);
                let encoded = frame.encode(descriptor.max_part_bytes).unwrap();
                assert!(encoded.len() as u64 <= FRAME_CAP);
                check_frame(&mut node, &descriptor, &witness, &mut cursor, &encoded).unwrap();
            }
            assert_eq!(cursor.next_relation, descriptor.mandatory_relations);
            assert_eq!(node.finish(), Ok(if wrong_result { CheckOutcome::OutputMismatch } else { CheckOutcome::Matched }));
        });
    }
}

#[test]
fn rooted_registration_and_input_reject_self_consistent_candidate_substitution() {
    with_fixture(false, |f, runner, _| {
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
        for field in 0..6 {
            let mut candidate = f.result.clone();
            match field {
                0 => candidate.artifact = d(101),
                1 => candidate.registered_class = d(102),
                2 => candidate.input = d(103),
                3 => candidate.execution = d(104),
                4 => candidate.selector = d(105),
                _ => candidate.generation_seed[0] ^= 1,
            }
            let signature = *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &candidate.id().0.0, RESULT_SIGN_CONTEXT, [0; 32])
                .unwrap()
                .as_ref();
            candidate.authenticate(&f.key, &signature).unwrap(); // A real signature does not authenticate the rooted assignment.
            assert!(
                PanelAdmission::authenticate(
                    &candidate,
                    f.rooted,
                    &f.class,
                    &f.job,
                    PROMPT,
                    PalwPromptIdsFormV1::Flat,
                    &f.key,
                    &signature,
                    panel_limits()
                )
                .is_err()
            );
        }
        assert!(
            PanelAdmission::authenticate(
                &f.result,
                f.rooted,
                &f.class,
                &f.job,
                &[1],
                PalwPromptIdsFormV1::Flat,
                &f.key,
                &f.signature,
                panel_limits()
            )
            .is_err()
        );
        // Build a coherent foreign model packet using an actual alternate
        // inventory. The rooted registration remains fixed independently.
        let mut map = misaka_palw_tir::MapParams::default();
        map.tensors.insert((0, None), misaka_palw_tir::Tensor::new(misaka_palw_tir::DType::I8, vec![4, 4], vec![11; 16]).unwrap());
        let params = misaka_palw_tir_exec::TirParams::from_map(runner.plan, &map).unwrap();
        let source = misaka_palw_tir_exec::node::TirParamsSourceV1(&params);
        let inventory = misaka_palw_tir_exec::node::TirHeldInventoryV1::build(&runner.plan.program, &source).unwrap();
        let foreign_artifact = inventory.tree.root();
        let mut foreign_job = f.job.clone();
        foreign_job.shape_profile_id = f.class.class_id(&foreign_artifact);
        let mut foreign_result = f.result.clone();
        foreign_result.artifact = digest(foreign_artifact);
        foreign_result.registered_class = digest(foreign_job.shape_profile_id);
        foreign_result.execution = digest(foreign_job.context_hash());
        foreign_result.output = digest(palw_attempt_output_root_v1(&foreign_job, &[0, 0, 0]));
        let sig = *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &foreign_result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32])
            .unwrap()
            .as_ref();
        foreign_result.authenticate(&f.key, &sig).unwrap();
        assert!(
            PanelAdmission::authenticate(
                &foreign_result,
                f.rooted,
                &f.class,
                &foreign_job,
                PROMPT,
                PalwPromptIdsFormV1::Flat,
                &f.key,
                &sig,
                panel_limits()
            )
            .is_err()
        );
        // Likewise, all fields of a different-input packet agree with its own
        // execution, but it is not the input assigned by the rooted job.
        let mut foreign_job = f.job.clone();
        foreign_job.prompt_token_ids_hash =
            kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[1]).unwrap();
        let mut foreign_result = f.result.clone();
        foreign_result.input = digest(foreign_job.prompt_token_ids_hash);
        foreign_result.execution = digest(foreign_job.context_hash());
        foreign_result.output = digest(palw_attempt_output_root_v1(&foreign_job, &[0, 1, 0]));
        let sig = *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &foreign_result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32])
            .unwrap()
            .as_ref();
        foreign_result.authenticate(&f.key, &sig).unwrap();
        assert!(
            PanelAdmission::authenticate(
                &foreign_result,
                f.rooted,
                &f.class,
                &foreign_job,
                &[1],
                PalwPromptIdsFormV1::Flat,
                &f.key,
                &sig,
                panel_limits()
            )
            .is_err()
        );
    });
}

#[test]
fn shortened_plan_foreign_cursor_and_wrong_frame_identity_are_rejected() {
    with_fixture(false, |f, runner, inventory| {
        for capture_budget in [false, true] {
            let mut limits = panel_limits();
            if capture_budget {
                limits.max_capture_bytes = 3;
            } else {
                limits.max_total_frame_bytes = 1;
            }
            assert!(
                PanelAdmission::authenticate(
                    &f.result,
                    f.rooted,
                    &f.class,
                    &f.job,
                    PROMPT,
                    PalwPromptIdsFormV1::Flat,
                    &f.key,
                    &f.signature,
                    limits
                )
                .is_err()
            );
        }
        let admitted = f.admit().unwrap();
        let descriptor = admitted.plan();
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&admitted).unwrap();
        let mut shortened = descriptor;
        shortened.mandatory_relations -= 1;
        assert!(builder.commit(&capture, &shortened, witness_author(), 0).is_err());
        let foreign = CoverageCursor { plan: PlanId(d(199)), next_relation: 0 };
        assert!(builder.build_part(&capture, foreign).is_err());
        let witness = builder.commit(&capture, &descriptor, witness_author(), 0).unwrap();
        let part = builder.build_part(&capture, CoverageCursor { plan: descriptor.plan, next_relation: 0 }).unwrap();
        let frame = part.frame(&witness, &descriptor).unwrap();
        let mut header = frame.header();
        header.result = ResultId(d(200));
        let foreign_frame = EvidenceFrame::new(header, frame.payload(), descriptor.max_part_bytes).unwrap();
        assert!(
            require_next(&descriptor, CoverageCursor { plan: descriptor.plan, next_relation: 0 }, &witness, &foreign_frame).is_err()
        );
        assert!(part.frame(&witness, &shortened).is_err());
    });
}

#[test]
fn malicious_panel_part_and_missing_terminal_cannot_match_honest_miner() {
    with_fixture(false, |f, runner, inventory| {
        let admitted = f.admit().unwrap();
        let descriptor = admitted.plan();
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&admitted).unwrap();
        let witness = builder.commit(&capture, &descriptor, witness_author(), 0).unwrap();
        let prepared = builder.prepare_parts(&capture).unwrap();
        let mut missing = f.checker(capture.binding().clone(), capture.generated().to_vec());
        let mut cursor = CoverageCursor { plan: descriptor.plan, next_relation: 0 };
        for index in 0..descriptor.mandatory_relations - 1 {
            let part = prepared.build_part(CoverageCursor { plan: descriptor.plan, next_relation: index }).unwrap();
            check_frame(
                &mut missing,
                &descriptor,
                &witness,
                &mut cursor,
                &part.frame(&witness, &descriptor).unwrap().encode(FRAME_CAP).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(missing.finish(), Err(CheckError::Incomplete));
        let part = prepared.build_part(CoverageCursor { plan: descriptor.plan, next_relation: 0 }).unwrap();
        let frame = part.frame(&witness, &descriptor).unwrap();
        let raw = tir::decode_frame(&frame, FRAME_CAP).unwrap();
        let mut cone: PalwTirConeRefutationV1 = borsh::from_slice(raw.cone()).unwrap();
        cone.output_preimage.values_le[0] ^= 1;
        let bytes = borsh::to_vec(&cone).unwrap();
        let payload = tir::ExactPart::new(&bytes, raw.trace(), FRAME_CAP - EVIDENCE_HEADER_BYTES as u64).unwrap().encode();
        let header = EvidenceHeader { payload_bytes: payload.len() as u32, ..frame.header() };
        let tampered = EvidenceFrame::new(header, &payload, FRAME_CAP).unwrap().encode(FRAME_CAP).unwrap();
        let mut rejected = f.checker(capture.binding().clone(), capture.generated().to_vec());
        let mut cursor = CoverageCursor { plan: descriptor.plan, next_relation: 0 };
        assert!(check_frame(&mut rejected, &descriptor, &witness, &mut cursor, &tampered).is_err());
        assert_eq!(rejected.progress().outcome, CheckOutcome::WitnessRejected);
        assert_eq!(rejected.finish(), Err(CheckError::Closed));
    });
}

#[test]
fn recorded_capture_takeover_keeps_witness_and_resumes_from_rechecked_prefix() {
    with_fixture(false, |f, runner, inventory| {
        let admitted = f.admit().unwrap();
        let descriptor = admitted.plan();
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&admitted).unwrap();
        let witness = builder.commit(&capture, &descriptor, witness_author(), 0).unwrap();
        authenticate_witness(&witness, &f.result);
        let saved = capture.encode_recorded(panel_limits().max_capture_bytes).unwrap();
        assert!(capture.encode_recorded(saved.len() as u64 - 1).is_err());
        let mut broken = saved.clone();
        *broken.last_mut().unwrap() ^= 1;
        assert!(builder.restore_recorded(&broken, &admitted, &witness).is_err());
        assert!(builder.restore_recorded(&saved[..saved.len() - 1], &admitted, &witness).is_err());
        let restored = builder.restore_recorded(&saved, &admitted, &witness).unwrap();
        assert_eq!(builder.commit(&restored, &descriptor, witness.author, witness.attempt).unwrap(), witness);
        let prepared = builder.prepare_parts(&capture).unwrap();
        let mut prefix = Vec::new();
        let split = descriptor.mandatory_relations / 2;
        for index in 0..split {
            prefix.push(
                prepared
                    .build_part(CoverageCursor { plan: descriptor.plan, next_relation: index })
                    .unwrap()
                    .frame(&witness, &descriptor)
                    .unwrap()
                    .encode(FRAME_CAP)
                    .unwrap(),
            );
        }
        let mut restarted = f.checker(restored.binding().clone(), restored.generated().to_vec());
        let mut cursor = CoverageCursor { plan: descriptor.plan, next_relation: 0 };
        for bytes in &prefix {
            check_frame(&mut restarted, &descriptor, &witness, &mut cursor, bytes).unwrap();
        }
        assert_eq!(cursor.next_relation, split);
        let prepared = builder.prepare_parts(&restored).unwrap();
        for index in cursor.next_relation..descriptor.mandatory_relations {
            let bytes = prepared
                .build_part(CoverageCursor { plan: descriptor.plan, next_relation: index })
                .unwrap()
                .frame(&witness, &descriptor)
                .unwrap()
                .encode(FRAME_CAP)
                .unwrap();
            check_frame(&mut restarted, &descriptor, &witness, &mut cursor, &bytes).unwrap();
        }
        assert_eq!(restarted.finish(), Ok(CheckOutcome::Matched));
    });
}

#[test]
fn zero_job_tokenizer_and_nonzero_registered_tokenizer_preserve_actual_r1_context() {
    with_fixture(false, |f, runner, inventory| {
        let canonical_job = f.job.clone();
        let registered_class = f.class.clone();
        assert_eq!(canonical_job.tokenizer_id, Hash64::from_u64_word(0));
        assert_eq!(registered_class.tokenizer_id, Hash64::from_u64_word(18));
        assert_ne!(canonical_job.tokenizer_id, registered_class.tokenizer_id);
        assert_eq!(f.rooted.tokenizer, digest(canonical_job.tokenizer_id));
        assert_eq!(f.result.tokenizer, digest(canonical_job.tokenizer_id));
        assert_eq!(f.rooted.registered_class, digest(registered_class.class_id(&f.artifact)));
        assert_eq!(f.result.registered_class, digest(registered_class.class_id(&f.artifact)));
        let direct = runner.run(&canonical_job, PROMPT, check_limits().max_leaves, true, &mut |_| {}).unwrap();
        let admitted = f.admit().unwrap();
        let descriptor = admitted.plan();
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&admitted).unwrap();
        assert_eq!(capture.generated(), EXPECTED);
        assert_eq!(capture.generated(), direct.generated);
        assert_eq!(capture.binding().job_context, canonical_job);
        assert_eq!(capture.binding().class, registered_class);
        assert_eq!(capture.binding().artifact_root, f.artifact);
        assert_eq!(capture.binding().full_logits_trace_root, direct.trace_root);
        assert_eq!(capture.binding().step_leaf_count, direct.leaf_count);
        assert_eq!(capture.binding().step_merkle_root, direct.step_merkle_root);
        assert_eq!(capture.binding().committed_execution_root, direct.execution_root);
        assert_eq!(digest(direct.output_root), f.result.output);
        let witness = builder.commit(&capture, &descriptor, witness_author(), 0).unwrap();
        assert_eq!(witness.provenance, tir::provenance_id(&f.result));
        let mut checker = f.checker(capture.binding().clone(), capture.generated().to_vec());
        let mut cursor = CoverageCursor { plan: descriptor.plan, next_relation: 0 };
        let prepared = builder.prepare_parts(&capture).unwrap();
        for index in 0..descriptor.mandatory_relations {
            let part = prepared.build_part(CoverageCursor { plan: descriptor.plan, next_relation: index }).unwrap();
            let bytes = part.frame(&witness, &descriptor).unwrap().encode(FRAME_CAP).unwrap();
            check_frame(&mut checker, &descriptor, &witness, &mut cursor, &bytes).unwrap();
        }
        assert_eq!(checker.finish(), Ok(CheckOutcome::Matched));
        assert_eq!(f.job, canonical_job);
        assert_eq!(capture.binding().job_context, canonical_job);
        assert_eq!(f.class, registered_class);
    });
}

#[test]
fn tokenizer_fix_preserves_independent_rooted_class_and_job_rejection() {
    with_fixture(false, |f, _, _| {
        let canonical_job = f.job.clone();
        let original_result = f.result.clone();
        let rooted = f.rooted;
        let mut altered_class = f.class.clone();
        altered_class.tokenizer_id = Hash64::from_u64_word(99);
        assert_ne!(digest(altered_class.class_id(&f.artifact)), rooted.registered_class);
        assert!(
            PanelAdmission::authenticate(
                &original_result,
                rooted,
                &altered_class,
                &canonical_job,
                PROMPT,
                PalwPromptIdsFormV1::Flat,
                &f.key,
                &f.signature,
                panel_limits(),
            )
            .is_err()
        );
        for field in 0..5 {
            let mut altered_job = canonical_job.clone();
            let prompt: &[u32] = match field {
                0 => {
                    altered_job.tokenizer_id = f.class.tokenizer_id;
                    PROMPT
                }
                1 => {
                    altered_job.prompt_token_ids_hash =
                        kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[1])
                            .unwrap();
                    &[1]
                }
                2 => {
                    altered_job.execution_seed[0] ^= 1;
                    PROMPT
                }
                3 => {
                    altered_job.shape_profile_id = Hash64::from_u64_word(101);
                    PROMPT
                }
                _ => {
                    altered_job.max_context_tokens += 1;
                    PROMPT
                }
            };
            assert!(
                PanelAdmission::authenticate(
                    &original_result,
                    rooted,
                    &f.class,
                    &altered_job,
                    prompt,
                    PalwPromptIdsFormV1::Flat,
                    &f.key,
                    &f.signature,
                    panel_limits(),
                )
                .is_err(),
                "altered canonical job field {field} was accepted"
            );
        }
        // A genuine Miner signature cannot replace the actual job tokenizer
        // with the independently registered class tokenizer.
        let mut copied_registration_tokenizer = original_result.clone();
        copied_registration_tokenizer.tokenizer = digest(f.class.tokenizer_id);
        assert_ne!(copied_registration_tokenizer.tokenizer, rooted.tokenizer);
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &copied_registration_tokenizer.id().0.0, RESULT_SIGN_CONTEXT, [0; 32])
                .unwrap()
                .as_ref();
        copied_registration_tokenizer.authenticate(&f.key, &signature).unwrap();
        assert!(
            PanelAdmission::authenticate(
                &copied_registration_tokenizer,
                rooted,
                &f.class,
                &canonical_job,
                PROMPT,
                PalwPromptIdsFormV1::Flat,
                &f.key,
                &signature,
                panel_limits(),
            )
            .is_err()
        );
        assert_eq!(f.job, canonical_job);
        assert_eq!(f.result, original_result);
        assert_eq!(f.rooted, rooted);
    });
}
