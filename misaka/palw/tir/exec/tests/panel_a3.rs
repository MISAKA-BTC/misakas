#![cfg(feature = "node")]
//! Independent small-table expectations for the A3 Panel adapter. The expected
//! sequence below is hand-derived, without the Miner adapter or TIR evaluator.
mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_positive_court_v2::{gate::ClaimFormV2, journal, *},
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::PalwTirCourtRulesV1,
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1,
        inventory::{TirHeldInventoryV1, TirParamsSourceV1},
        positive_court::{PanelCaptureV2, RoleLimitsV2},
    },
};

const PROMPT: &[u32] = &[0];
// Embedding row 0 selects 1; row 1 ties at 0/1 and selects 0; row 0 selects 1.
const EXPECTED: &[u32] = &[1, 0, 1];

fn policy() -> ExactPolicyV2 {
    ExactPolicyV2 {
        max_leaves: 1 << 12,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 26,
        max_elements_per_leaf: 1 << 16,
        max_terms_per_leaf: 1 << 18,
        max_total_elements: 1 << 32,
        max_total_terms: 1 << 36,
        max_commit_bytes: 1 << 20,
        deadline: 100,
    }
}

fn limits() -> RoleLimitsV2 {
    RoleLimitsV2 { max_prefill: 4, max_decode: 4, max_leaves: 1 << 12, max_capture_bytes: 1 << 26 }
}

fn rules() -> PalwTirCourtRulesV1 {
    let p = policy();
    PalwTirCourtRulesV1 {
        max_step_leaf_count: p.max_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: p.max_elements_per_leaf, max_terms: p.max_terms_per_leaf },
    }
}

fn with_fixture(
    wide: bool,
    corrupt_weights: bool,
    body: impl FnOnce(&TirClassRunnerV1<'_>, &TirHeldInventoryV1<'_>, &PalwTirClassV1, &MinerResultV2),
) {
    let mut builder = ProgramBuilder::new(4, 1 << 18);
    let embedding = builder.param("embedding", DType::I8, &[4, 4], false);
    let mut pre = builder.block("pre", vec![]);
    let x = pre.gather(embedding, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let mut post = builder.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let y = post.cast(Ref::CarryIn(0), if wide { DType::I64 } else { DType::I32 });
    let Ref::Node(logits) = post.commit(y) else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = node_common::tiled(builder.finish(pre, vec![], post, logits));
    if wide {
        program.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
    }
    let mut map = MapParams::default();
    map.tensors
        .insert((0, None), Tensor::new(DType::I8, vec![4, 4], vec![1, 9, 3, 2, 5, 5, 1, -2, -3, -2, -1, 0, 0, 7, 10, 2]).unwrap());
    let plan = TirPlan::compile(&program).unwrap();
    let honest_params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&honest_params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    if corrupt_weights {
        map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], vec![11; 16]).unwrap());
    }
    let execution_params = TirParams::from_map(&plan, &map).unwrap();
    let mut layout = node_common::layout(&program, 3, 2, 2, 16);
    if wide {
        layout.commit_tiles.fill(4);
    }
    let class = PalwTirClassV1 { version: 1, program: program.encode(), layout, tokenizer_id: Hash64::from_u64_word(18) };
    let id = class.class_id(&artifact);
    let mut job = node_common::job(&program, 1, 3, id, PROMPT, PalwPromptIdsFormV1::Flat);
    job.tokenizer_id = class.tokenizer_id;
    if wide {
        job.trace_scheme_id = kaspa_consensus_core::palw_logits_q24_v2::scheme_id();
    }
    let result = MinerResultV2 {
        version: 2,
        network: Hash64::from_u64_word(1),
        branch: Hash64::from_u64_word(2),
        ruleset: Hash64::from_u64_word(3),
        claim: Hash64::from_u64_word(4),
        output: palw_attempt_output_root_v1(&job, EXPECTED),
        job,
        artifact,
        sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
        service_policy: Hash64::from_u64_word(5),
        dependencies: Hash64::from_u64_word(6),
    };
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = if wide {
        TirClassRunnerV1::new_wide_q24_v2(&space, &plan, &execution_params, id)
    } else {
        TirClassRunnerV1::new(&space, &plan, &execution_params, id)
    }
    .unwrap();
    body(&runner, &inventory, &class, &result);
}

fn capture(runner: &TirClassRunnerV1<'_>, class: &PalwTirClassV1, result: &MinerResultV2) -> PanelCaptureV2 {
    PanelCaptureV2::replay(runner, class, result.artifact, &result.job, PROMPT, PalwPromptIdsFormV1::Flat, limits()).unwrap()
}

fn admit(result: &MinerResultV2, witness: &PanelWitnessV2, class: &PalwTirClassV1) -> ExactCaseV2 {
    let miner = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
    let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([82; 32]);
    let sign = |key, id: Hash64, context| {
        SignatureV2(libcrux_ml_dsa::ml_dsa_87::sign(key, id.as_byte_slice(), context, [0; 32]).unwrap().as_ref().to_vec())
    };
    ExactCaseV2::admit(
        result.clone(),
        &sign(&miner.signing_key, result.id(), RESULT_CONTEXT),
        miner.verification_key.as_ref(),
        witness.clone(),
        &sign(&panel.signing_key, witness.id(), WITNESS_CONTEXT),
        panel.verification_key.as_ref(),
        class,
        policy(),
        PalwPromptIdsFormV1::Flat,
        1,
        AdmissionScopeV2 {
            network: result.network,
            branch: result.branch,
            ruleset: result.ruleset,
            required_form: ClaimFormV2::MinerResultV2,
            registered_class_id: class.class_id(&result.artifact),
        },
    )
    .unwrap()
}

fn encoded(part: &ExactPartV2) -> (Vec<u8>, Option<Vec<u8>>) {
    (borsh::to_vec(&part.cone).unwrap(), part.trace.as_ref().map(|t| borsh::to_vec(t).unwrap()))
}

#[test]
fn hand_computed_tie_sequence_passes_cold_node_without_miner_trace_api() {
    for wide in [false, true] {
        with_fixture(wide, false, |runner, inventory, class, result| {
            let panel = capture(runner, class, result);
            assert_eq!(panel.generated(), EXPECTED);
            let witness = panel.witness(result, policy(), 0).unwrap();
            let mut node = admit(result, &witness, class);
            panel
                .stream_parts(runner, inventory, &rules(), policy().max_part_bytes, &mut |_, part| {
                    let (cone, trace) = encoded(&part);
                    node.submit_encoded(&cone, trace.as_deref(), 2).map_err(|e| format!("{e:?}"))
                })
                .unwrap();
            assert_eq!(node.finish(3), Ok(ClaimStatusV2::Passed));
        });
    }
}

#[test]
fn wrong_signed_output_is_refuted_only_after_complete_node_check() {
    for wide in [false, true] {
        with_fixture(wide, false, |runner, inventory, class, honest_result| {
            let mut result = honest_result.clone();
            result.output = palw_attempt_output_root_v1(&result.job, &[1, 1, 1]);
            let panel = capture(runner, class, &result);
            let witness = panel.witness(&result, policy(), 0).unwrap();
            let mut node = admit(&result, &witness, class);
            assert_eq!(node.finish(2), Err(ErrorV2::Incomplete));
            panel
                .stream_parts(runner, inventory, &rules(), policy().max_part_bytes, &mut |_, part| {
                    let (cone, trace) = encoded(&part);
                    node.submit_encoded(&cone, trace.as_deref(), 2).map_err(|e| format!("{e:?}"))
                })
                .unwrap();
            assert_eq!(node.finish(3), Ok(ClaimStatusV2::Refuted));
        });
    }
}

#[test]
fn substituted_weights_never_convict_the_honest_miner() {
    for wide in [false, true] {
        with_fixture(wide, true, |runner, inventory, class, result| {
            let panel = capture(runner, class, result);
            assert_ne!(panel.generated(), EXPECTED);
            let witness = panel.witness(result, policy(), 0).unwrap();
            let mut node = admit(result, &witness, class);
            let run = panel.stream_parts(runner, inventory, &rules(), policy().max_part_bytes, &mut |_, part| {
                let (cone, trace) = encoded(&part);
                node.submit_encoded(&cone, trace.as_deref(), 2).map_err(|e| format!("{e:?}"))
            });
            assert!(run.is_err());
            assert_eq!(node.statuses().0, ClaimStatusV2::Checking);
            assert!(node.finish(3).is_err());
        });
    }
}

#[test]
fn substituted_prompt_model_and_policy_cannot_reuse_recorded_capture() {
    for wide in [false, true] {
        with_fixture(wide, false, |runner, _, class, result| {
            assert!(
                PanelCaptureV2::replay(runner, class, result.artifact, &result.job, &[1], PalwPromptIdsFormV1::Flat, limits(),)
                    .is_err()
            );
            let panel = capture(runner, class, result);
            let witness = panel.witness(result, policy(), 0).unwrap();
            let bytes = panel.encode_recorded(limits().max_capture_bytes).unwrap();
            let restore = |bytes: &[u8], result: &MinerResultV2, witness: &PanelWitnessV2, p| {
                PanelCaptureV2::restore_recorded(bytes, runner, result, witness, p, &rules(), limits())
            };
            let mut wrong_model = result.clone();
            wrong_model.artifact = Hash64::from_u64_word(999);
            assert!(restore(&bytes, &wrong_model, &witness, policy()).is_err());
            let mut wrong_input = result.clone();
            wrong_input.job.prompt_token_ids_hash = Hash64::from_u64_word(999);
            assert!(restore(&bytes, &wrong_input, &witness, policy()).is_err());
            let mut wrong_policy = policy();
            wrong_policy.deadline += 1;
            assert!(restore(&bytes, result, &witness, wrong_policy).is_err());
            let mut damaged = bytes.clone();
            *damaged.last_mut().unwrap() ^= 1;
            assert!(restore(&damaged, result, &witness, policy()).is_err());
            // Capture magic, binding version and job version precede the first
            // vector length (network_id). Reject the length before allocating.
            let mut forged_length = bytes.clone();
            forged_length[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(restore(&forged_length, result, &witness, policy()).is_err());
            assert!(restore(&bytes[..bytes.len() - 1], result, &witness, policy()).is_err());
            assert!(panel.encode_recorded(bytes.len() as u64 - 1).is_err());
        });
    }
}

#[test]
fn panel_takeover_restores_same_witness_and_uses_replayed_node_cursor() {
    for wide in [false, true] {
        with_fixture(wide, false, |runner, inventory, class, result| {
            let panel = capture(runner, class, result);
            let witness = panel.witness(result, policy(), 0).unwrap();
            let saved = panel.encode_recorded(limits().max_capture_bytes).unwrap();
            let count = witness.binding.step_leaf_count;
            assert!(count > 1);
            let first_batch = count / 2;
            let mut node = admit(result, &witness, class);
            let mut journal_bytes = Vec::new();
            journal::write_header(&node, &mut journal_bytes).unwrap();
            panel
                .stream_parts_range(runner, inventory, &rules(), 0, first_batch, policy().max_part_bytes, &mut |_, part| {
                    let (cone, trace) = encoded(&part);
                    node.submit_encoded(&cone, trace.as_deref(), 2).map_err(|e| format!("{e:?}"))?;
                    journal::write_part(&mut journal_bytes, &cone, trace.as_deref(), policy().max_part_bytes)
                        .map_err(|e| format!("{e:?}"))
                })
                .unwrap();
            drop(panel);
            drop(node);
            let takeover = PanelCaptureV2::restore_recorded(&saved, runner, result, &witness, policy(), &rules(), limits()).unwrap();
            assert_eq!(takeover.witness(result, policy(), 0).unwrap(), witness);
            let mut restarted = admit(result, &witness, class);
            assert_eq!(journal::replay(&mut restarted, &mut journal_bytes.as_slice(), 3), Ok(ClaimStatusV2::Checking));
            assert_eq!(restarted.cursor(), first_batch);
            let mut emitted = Vec::new();
            let part_rules = rules();
            let prepared = takeover.prepare_parts(runner, inventory, &part_rules).unwrap();
            // A replacement Panel builds the dense evidence tree once and
            // reuses it across individually bounded submission batches.
            for next in first_batch..count {
                prepared
                    .stream_range(next, 1, policy().max_part_bytes, &mut |index, part| {
                        emitted.push(index);
                        let (cone, trace) = encoded(&part);
                        restarted.submit_encoded(&cone, trace.as_deref(), 4).map_err(|e| format!("{e:?}"))
                    })
                    .unwrap();
            }
            assert_eq!(emitted, (first_batch..count).collect::<Vec<_>>());
            assert_eq!(restarted.finish(5), Ok(ClaimStatusV2::Passed));
        });
    }
}

#[test]
fn ranged_submission_rejects_overflow_and_stops_on_transport_failure() {
    with_fixture(false, false, |runner, inventory, class, result| {
        let panel = capture(runner, class, result);
        let count = panel.binding().step_leaf_count;
        let mut calls = 0;
        let mut consume = |_, _| {
            calls += 1;
            Ok(())
        };
        assert!(panel.stream_parts_range(runner, inventory, &rules(), u64::MAX, 2, policy().max_part_bytes, &mut consume).is_err());
        assert!(panel.stream_parts_range(runner, inventory, &rules(), count, 1, policy().max_part_bytes, &mut consume).is_err());
        assert_eq!(calls, 0);
        let failure = panel.stream_parts_range(runner, inventory, &rules(), 0, count, policy().max_part_bytes, &mut |_, _| {
            calls += 1;
            Err("carrier unavailable".into())
        });
        assert_eq!(failure, Err("carrier unavailable".into()));
        assert_eq!(calls, 1);
    });
}

#[test]
fn same_program_with_another_layout_cannot_impersonate_the_admitted_runner() {
    for wide in [false, true] {
        with_fixture(wide, false, |runner, inventory, class, result| {
            let panel = capture(runner, class, result);
            let witness = panel.witness(result, policy(), 0).unwrap();
            let saved = panel.encode_recorded(limits().max_capture_bytes).unwrap();
            let mut changed_class = class.clone();
            changed_class.layout.checkpoint_interval += 1;
            let foreign_space = PalwTirStepSpaceV1::new(&changed_class).unwrap();
            // The general runner constructor takes a caller-supplied class id.
            // A3 must compare the actual space layout too.
            let foreign = if wide {
                TirClassRunnerV1::new_wide_q24_v2(&foreign_space, runner.plan, runner.params, runner.class_id)
            } else {
                TirClassRunnerV1::new(&foreign_space, runner.plan, runner.params, runner.class_id)
            }
            .unwrap();
            assert!(
                PanelCaptureV2::replay(&foreign, class, result.artifact, &result.job, PROMPT, PalwPromptIdsFormV1::Flat, limits(),)
                    .is_err()
            );
            assert!(PanelCaptureV2::restore_recorded(&saved, &foreign, result, &witness, policy(), &rules(), limits()).is_err());
            let mut emitted = 0;
            assert!(
                panel
                    .stream_parts_range(&foreign, inventory, &rules(), 0, 1, policy().max_part_bytes, &mut |_, _| {
                        emitted += 1;
                        Ok(())
                    })
                    .is_err()
            );
            assert_eq!(emitted, 0);
        });
    }
}
