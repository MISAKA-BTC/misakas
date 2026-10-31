#![cfg(feature = "node")]

mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::PalwTirCourtRulesV1,
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1},
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::node::positive_check::{CheckError, CheckLimits, CheckOutcome, CoveragePlan, ExactChecker};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1},
};

#[test]
fn plan_derivation_refuses_inherited_and_malformed_contexts_before_reservation() {
    let (_, mut program, _) = node_common::h_program();
    program = node_common::tiled(program);
    for block in &mut program.blocks {
        for node in &mut block.nodes {
            node.commit = true;
        }
    }
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: node_common::layout(&program, 3, 1, 1, 64),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let artifact = Hash64::from_u64_word(19);
    let form = PalwPromptIdsFormV1::Flat;
    let mut ctx = node_common::job(&program, 1, 1, class.class_id(&artifact), &[1], form);
    ctx.tokenizer_id = class.tokenizer_id;
    let limits = CheckLimits {
        max_leaves: 1024,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 30,
        max_elements_per_leaf: 64,
        max_terms_per_leaf: 64,
        max_total_elements: 1 << 16,
        max_total_terms: 1 << 16,
    };
    let plan = CoveragePlan::derive(&class, artifact, &ctx, form, limits).unwrap();
    let zero = Hash64::default();
    let binding = PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: ctx.clone(),
        class: class.clone(),
        artifact_root: artifact,
        full_logits_trace_root: zero,
        step_leaf_count: plan.leaf_count(),
        step_merkle_root: zero,
        committed_execution_root: palw_tir_execution_root_v1(
            &ctx.context_hash(),
            &zero,
            &ctx.shape_profile_id,
            plan.leaf_count(),
            &zero,
        ),
    };
    plan.validate_witness(&binding, &[0]).unwrap();
    assert_eq!(plan.validate_witness(&binding, &[0; 4096]), Err(CheckError::PanelEvidence));
    for mutation in 0..3 {
        let mut bad = binding.clone();
        match mutation {
            0 => bad.class.program.extend([0; 4096]),
            1 => bad.class.layout.commit_tiles.extend([4; 4096]),
            _ => bad.job_context.network_id.extend([0; 4096]),
        }
        assert_eq!(plan.validate_witness(&bad, &[0]), Err(CheckError::Binding));
    }
    for mutation in 0..4 {
        let mut bad = ctx.clone();
        let expected = match mutation {
            0 => {
                bad.version = 3;
                CheckError::UnsupportedProfile
            }
            1 => {
                bad.network_id.clear();
                CheckError::Binding
            }
            2 => {
                bad.network_id = vec![0; 65];
                CheckError::Binding
            }
            _ => {
                bad.max_context_tokens = 1;
                CheckError::Binding
            }
        };
        assert_eq!(CoveragePlan::derive(&class, artifact, &bad, form, limits).unwrap_err(), expected);
    }
}

#[test]
fn honest_material_over_the_evaluation_allowance_ends_unverified_without_panel_blame() {
    let mut builder = ProgramBuilder::new(4, 1 << 18);
    let embedding = builder.param("embedding", DType::I8, &[4, 4], false);
    let mut pre = builder.block("pre", vec![]);
    let row = pre.gather(embedding, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[row]);
    let mut post = builder.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let row = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(row);
    let Ref::Node(logits) = row else { unreachable!() };
    let post = post.finish(&[]);
    let program = node_common::tiled(builder.finish(pre, vec![], post, logits));
    let mut map = MapParams::default();
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], (0..16).collect()).unwrap());
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: node_common::layout(&program, 3, 1, 1, 64),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let class_id = class.class_id(&artifact);
    let prompt = [1];
    let form = PalwPromptIdsFormV1::Flat;
    let mut ctx = node_common::job(&program, 1, 1, class_id, &prompt, form);
    ctx.tokenizer_id = class.tokenizer_id;
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
    let mut leaves = Vec::new();
    let run = runner.run(&ctx, &prompt, 64, true, &mut |leaf| leaves.push(leaf.preimage.clone())).unwrap();
    assert_eq!(run.generated, vec![3]);
    let binding = PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: ctx.clone(),
        class: class.clone(),
        artifact_root: artifact,
        full_logits_trace_root: run.trace_root,
        step_leaf_count: run.leaf_count,
        step_merkle_root: run.step_merkle_root,
        committed_execution_root: run.execution_root,
    };
    let rules = PalwTirCourtRulesV1 { max_step_leaf_count: 64, prompt_form: form, limits: DemandLimits::UNLIMITED };
    let store =
        TirEvidenceV1::dense(&runner, &binding, &prompt, &run.logits_rows, &run.generated, &leaves, &inventory, form, 64).unwrap();
    let cone = borsh::to_vec(&store.cone_refutation(0, &rules).unwrap()).unwrap();
    let limits = CheckLimits {
        max_leaves: 64,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 26,
        max_elements_per_leaf: 1,
        max_terms_per_leaf: 1,
        max_total_elements: 64,
        max_total_terms: 64,
    };
    let coverage = CoveragePlan::derive(&class, artifact, &ctx, form, limits).unwrap();
    let mut checker = ExactChecker::begin_greedy(
        coverage,
        binding,
        run.generated.clone(),
        palw_attempt_output_root_v1(&ctx, &run.generated),
        PalwDecodeSamplingV2::GREEDY,
    )
    .unwrap();
    assert_eq!(checker.submit_encoded(&cone, None), Err(CheckError::Unverified));
    let progress = checker.progress();
    assert_eq!(progress.next_leaf, 0);
    assert_eq!(progress.outcome, CheckOutcome::EndedUnverified);
    assert_eq!(progress.charged_elements, 1);
    assert_eq!(progress.charged_terms, 1);
    assert_eq!(progress.charged_bytes, cone.len() as u64);
    assert_eq!(checker.finish(), Err(CheckError::Closed));
    assert_eq!(checker.submit_encoded(&cone, None), Err(CheckError::Closed));
}
