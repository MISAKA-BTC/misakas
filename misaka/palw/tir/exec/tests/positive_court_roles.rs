#![cfg(feature = "node")]
mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_positive_court_v2::*,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::PalwTirCourtRulesV1,
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_tir::{DType, MapParams, Ref, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1,
        inventory::{TirHeldInventoryV1, TirParamsSourceV1},
        positive_court::*,
    },
};

fn wide_program() -> misaka_palw_tir::TirProgramV1 {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embedding = p.param("embedding", DType::I8, &[4, 4], false);
    let mut pre = p.block("pre", vec![]);
    let x = pre.gather(embedding, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let scale = p.konst(DType::I64, &[4], &[1 << 34, 1 << 33, 1 << 32, 1 << 31]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let x = post.cast(Ref::CarryIn(0), DType::I64);
    let y = post.mul(x, scale, DType::I64);
    post.commit(y);
    let Ref::Node(logits) = y else { unreachable!() };
    let post = post.finish(&[]);
    let mut p = p.finish(pre, vec![], post, logits);
    p.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
    p
}
fn wide_params() -> MapParams {
    let mut m = MapParams::default();
    m.tensors.insert((0, None), misaka_palw_tir::Tensor::new(DType::I8, vec![4, 4], (1..=16).collect()).unwrap());
    m
}

fn policy() -> ExactPolicyV2 {
    ExactPolicyV2 {
        max_leaves: 1 << 20,
        max_part_bytes: 1 << 24,
        max_part_heap: 1 << 26,
        max_total_bytes: 1 << 32,
        max_elements_per_leaf: 1 << 22,
        max_terms_per_leaf: 1 << 26,
        max_total_elements: 1 << 48,
        max_total_terms: 1 << 52,
        max_commit_bytes: 1 << 20,
        deadline: 100,
    }
}
fn limits() -> RoleLimitsV2 {
    RoleLimitsV2 { max_prefill: 32, max_decode: 32, max_leaves: 1 << 20, max_capture_bytes: 1 << 28 }
}

#[derive(Clone, Copy)]
enum Scenario {
    Honest,
    WrongMiner,
    BadPanel,
    BadPanelTrace,
    OmitLast,
}

fn run_case(wide: bool, scenario: Scenario) {
    let (program, map) = if wide {
        (wide_program(), wide_params())
    } else {
        let (_, p, m) = node_common::h_program();
        (node_common::tiled(p), m)
    };
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let mut layout = node_common::layout(&program, 3, 2, 2, 64);
    if wide {
        layout.commit_tiles.fill(4);
    }
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout,
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let class_id = class.class_id(&artifact);
    let prompt = if wide { vec![3] } else { vec![3, 8, 13] };
    let form = PalwPromptIdsFormV1::Flat;
    let mut ctx = node_common::job(&program, prompt.len() as u32, 3, class_id, &prompt, form);
    ctx.tokenizer_id = class.tokenizer_id;
    if wide {
        ctx.trace_scheme_id = kaspa_consensus_core::palw_logits_q24_v2::scheme_id();
    }
    let result = MinerResultV2 {
        version: 2,
        network: Hash64::from_u64_word(1),
        branch: Hash64::from_u64_word(2),
        ruleset: Hash64::from_u64_word(3),
        claim: Hash64::from_u64_word(4),
        job: ctx.clone(),
        artifact,
        output: Hash64::from_u64_word(0),
        sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
        service_policy: Hash64::from_u64_word(5),
        dependencies: Hash64::from_u64_word(6),
    };
    let (mut result, generated) = miner_result_v2(result, &class, &plan, &params, &prompt, form, limits()).unwrap();
    if matches!(scenario, Scenario::WrongMiner) {
        result.output = Hash64::from_u64_word(999);
    }
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = if wide {
        TirClassRunnerV1::new_wide_q24_v2(&space, &plan, &params, class_id)
    } else {
        TirClassRunnerV1::new(&space, &plan, &params, class_id)
    }
    .unwrap();
    // Warm Panel needs no Miner intermediate-data request or Miner capture.
    let panel = PanelCaptureV2::replay(&runner, &class, artifact, &ctx, &prompt, form, limits()).unwrap();
    assert_eq!(panel.generated(), generated);
    let p = policy();
    let witness = panel.witness(&result, p, 0).unwrap();
    let miner_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([71; 32]);
    let panel_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([72; 32]);
    let miner_sig = SignatureV2(
        libcrux_ml_dsa::ml_dsa_87::sign(&miner_key.signing_key, result.id().as_byte_slice(), RESULT_CONTEXT, [73; 32])
            .unwrap()
            .as_ref()
            .to_vec(),
    );
    let panel_sig = SignatureV2(
        libcrux_ml_dsa::ml_dsa_87::sign(&panel_key.signing_key, witness.id().as_byte_slice(), WITNESS_CONTEXT, [74; 32])
            .unwrap()
            .as_ref()
            .to_vec(),
    );
    // The Node has the registered program and authenticated material in parts.
    // No Node runner or params/model object is supplied to ExactCaseV2.
    let scope = AdmissionScopeV2 {
        network: result.network,
        branch: result.branch,
        ruleset: result.ruleset,
        required_form: gate::ClaimFormV2::MinerResultV2,
        registered_class_id: class_id,
    };
    let mut node = ExactCaseV2::admit(
        result.clone(),
        &miner_sig,
        miner_key.verification_key.as_ref(),
        witness,
        &panel_sig,
        panel_key.verification_key.as_ref(),
        &class,
        p,
        form,
        1,
        scope,
    )
    .unwrap();
    assert_eq!(node.finish(1), Err(ErrorV2::Incomplete));
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: p.max_leaves,
        prompt_form: form,
        limits: DemandLimits { max_elements: p.max_elements_per_leaf, max_terms: p.max_terms_per_leaf },
    };
    let last = panel.binding().step_leaf_count - 1;
    let stream = panel.stream_parts(&runner, &inventory, &rules, p.max_part_bytes, &mut |index, mut part| {
        if matches!(scenario, Scenario::OmitLast) && index == last {
            return Ok(());
        }
        if matches!(scenario, Scenario::BadPanel) && index == 0 {
            part.cone.output_preimage.values_le[0] ^= 1;
        }
        if wide && part.trace.is_some() {
            assert!(
                part.cone
                    .output_preimage
                    .values_le
                    .chunks_exact(8)
                    .any(|bytes| i64::from_le_bytes(bytes.try_into().unwrap()).unsigned_abs() > i32::MAX as u64)
            );
        }
        if matches!(scenario, Scenario::BadPanelTrace) {
            if let Some(event) = &mut part.trace {
                match event {
                    kaspa_consensus_core::palw_tir_court_v1::PalwTirTraceEventDisclosureV1::Tiled { tile_lanes, .. } => {
                        let last = tile_lanes.len() - 1;
                        tile_lanes[last] ^= 1;
                    }
                    kaspa_consensus_core::palw_tir_court_v1::PalwTirTraceEventDisclosureV1::WideQ24V2 { event, .. } => {
                        let last = event.lanes.len() - 1;
                        event.lanes[last] ^= 1;
                    }
                    _ => unreachable!(),
                }
            }
        }
        let cone = borsh::to_vec(&part.cone).unwrap();
        let trace = part.trace.as_ref().map(|t| borsh::to_vec(t).unwrap());
        node.submit_encoded(&cone, trace.as_deref(), 2).map_err(|e| format!("{index}: {e:?}"))
    });
    match scenario {
        Scenario::Honest => {
            stream.unwrap();
            assert_eq!(node.finish(3), Ok(ClaimStatusV2::Passed));
        }
        Scenario::WrongMiner => {
            stream.unwrap();
            assert_eq!(node.finish(3), Ok(ClaimStatusV2::Refuted));
        }
        Scenario::BadPanel | Scenario::BadPanelTrace => {
            assert!(stream.is_err());
            assert_eq!(node.statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
        }
        Scenario::OmitLast => {
            stream.unwrap();
            assert_eq!(node.finish(3), Err(ErrorV2::Incomplete));
            node.expire(100);
            assert_eq!(node.statuses(), (ClaimStatusV2::EndedUnverified, WitnessStatusV2::Incomplete));
        }
    }
}

#[test]
fn ordinary_miner_panel_replay_and_cold_node_pass() {
    run_case(false, Scenario::Honest);
}
#[test]
fn explicit_i64_q24_output_and_cold_node_pass() {
    run_case(true, Scenario::Honest);
}
#[test]
fn bad_panel_never_convicts_the_correct_miner() {
    run_case(false, Scenario::BadPanel);
    run_case(true, Scenario::BadPanel);
}
#[test]
fn altered_losing_logit_lane_rejects_panel_evidence() {
    run_case(false, Scenario::BadPanelTrace);
    run_case(true, Scenario::BadPanelTrace);
}
#[test]
fn only_checked_terminal_difference_refutes_miner() {
    run_case(false, Scenario::WrongMiner);
    run_case(true, Scenario::WrongMiner);
}
#[test]
fn missing_last_step_cannot_pass_or_gain_credit() {
    run_case(false, Scenario::OmitLast);
    run_case(true, Scenario::OmitLast);
}

#[test]
fn capture_limit_and_input_substitution_fail_before_replay() {
    let program = wide_program();
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &wide_params()).unwrap();
    let artifact = Hash64::from_u64_word(11);
    let mut layout = node_common::layout(&program, 3, 2, 2, 64);
    layout.commit_tiles.fill(4);
    let class = PalwTirClassV1 { version: 1, program: program.encode(), layout, tokenizer_id: Hash64::from_u64_word(18) };
    let id = class.class_id(&artifact);
    let mut ctx = node_common::job(&program, 1, 1, id, &[3], PalwPromptIdsFormV1::Flat);
    ctx.trace_scheme_id = kaspa_consensus_core::palw_logits_q24_v2::scheme_id();
    ctx.tokenizer_id = class.tokenizer_id;
    assert!(miner_infer_v2(&class, artifact, &plan, &params, &ctx, &[2], PalwPromptIdsFormV1::Flat, limits()).is_err());
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = TirClassRunnerV1::new_wide_q24_v2(&space, &plan, &params, id).unwrap();
    assert!(
        PanelCaptureV2::replay(
            &runner,
            &class,
            artifact,
            &ctx,
            &[3],
            PalwPromptIdsFormV1::Flat,
            RoleLimitsV2 { max_capture_bytes: 1, ..limits() }
        )
        .is_err()
    );
}
