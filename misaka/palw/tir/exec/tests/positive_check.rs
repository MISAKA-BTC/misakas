#![cfg(feature = "node")]

mod node_common;
use misaka_palw_tir_exec::node::positive_check;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_leg::{step_merkle_root_v1, step_tile_leaf_hash_v1},
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::{
        PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirParamOpeningV1, PalwTirTraceEventDisclosureV1,
        tir_logits_event_disclosure_v1,
    },
    palw_tir_step_v1::{
        PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1,
    },
};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1},
};
use positive_check::*;

type Part = (Vec<u8>, Option<Vec<u8>>);
struct Fixture {
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    parts: Vec<Part>,
}
fn limits() -> CheckLimits {
    CheckLimits {
        max_leaves: 1 << 20,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 40,
        max_elements_per_leaf: 1 << 20,
        max_terms_per_leaf: 1 << 24,
        max_total_elements: 1 << 40,
        max_total_terms: 1 << 44,
    }
}
fn fixture(form: PalwPromptIdsFormV1, forge: bool) -> Fixture {
    let (_, p, map) = node_common::h_program();
    let mut p = node_common::tiled(p);
    let state = p.states.len() as u16;
    p.states.push(misaka_palw_tir::program::StateDecl {
        name: "last_embedding".into(),
        kind: misaka_palw_tir::program::StateKind::Fixed { lo: -128, hi: 127 },
        dtype: misaka_palw_tir::DType::I8,
        shape: vec![4],
        per_layer: false,
    });
    p.blocks[p.schedule.pre as usize].nodes.push(misaka_palw_tir::program::Node {
        prim: misaka_palw_tir::Prim::StateWrite { state },
        inputs: vec![misaka_palw_tir::Ref::Node(0)],
        out: misaka_palw_tir::TensorType::fixed(misaka_palw_tir::DType::I8, &[4]),
        commit: true,
    });
    // Existing TIR semantics, with complete node commitments for this initial
    // profile. Do not silently promote a sparse legacy trace to full coverage.
    for block in &mut p.blocks {
        for node in &mut block.nodes {
            node.commit = true;
        }
    }
    run_fixture(p, map, vec![3, 8, 13], 3, form, forge)
}

fn run_fixture(
    p: misaka_palw_tir::TirProgramV1,
    map: misaka_palw_tir::MapParams,
    prompt: Vec<u32>,
    decode: u32,
    form: PalwPromptIdsFormV1,
    forge: bool,
) -> Fixture {
    let plan = TirPlan::compile(&p).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&p, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: p.encode(),
        layout: node_common::layout(&p, 3, 2, 2, 64),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let class_id = class.class_id(&artifact);
    let mut ctx = node_common::job(&p, prompt.len() as u32, decode, class_id, &prompt, form);
    ctx.tokenizer_id = class.tokenizer_id;
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
    let mut leaves = Vec::new();
    let run = runner.run(&ctx, &prompt, limits().max_leaves, true, &mut |l| leaves.push(l.preimage.clone())).unwrap();
    let mut binding = PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: ctx,
        class,
        artifact_root: artifact,
        full_logits_trace_root: run.trace_root,
        step_leaf_count: run.leaf_count,
        step_merkle_root: run.step_merkle_root,
        committed_execution_root: run.execution_root,
    };
    if forge {
        // Recommit the lie, so every Merkle opening is VALID. The arithmetic,
        // rather than hash membership, must reject this Panel candidate.
        let i = leaves.len() / 2;
        leaves[i].values_le[0] ^= 1;
        let h = binding.job_context.context_hash();
        let hashes: Vec<_> = leaves.iter().map(|l| step_tile_leaf_hash_v1(&h, &class_id, l)).collect();
        binding.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
        binding.committed_execution_root = palw_tir_execution_root_v1(
            &h,
            &binding.full_logits_trace_root,
            &class_id,
            binding.step_leaf_count,
            &binding.step_merkle_root,
        );
    }
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: limits().max_leaves,
        prompt_form: form,
        limits: DemandLimits { max_elements: limits().max_elements_per_leaf, max_terms: limits().max_terms_per_leaf },
    };
    let store = TirEvidenceV1::dense(
        &runner,
        &binding,
        &prompt,
        &run.logits_rows,
        &run.generated,
        &leaves,
        &inventory,
        form,
        limits().max_leaves,
    )
    .unwrap();
    let parts = (0..binding.step_leaf_count)
        .map(|i| {
            let cone = store.cone_refutation(i, &rules).unwrap();
            let leaf = space.leaf_at(&binding.job_context, i).unwrap();
            let trace = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                    if occurrence as usize == space.occurrences().len() - 1 && node == p.logits =>
                {
                    Some(
                        tir_logits_event_disclosure_v1(
                            &binding,
                            &run.logits_rows,
                            &run.generated,
                            leaf.position + 1 - binding.job_context.declared_prefill_tokens,
                            (first_element / kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES as u64) as u8,
                        )
                        .unwrap(),
                    )
                }
                _ => None,
            };
            (borsh::to_vec(&cone).unwrap(), trace.map(|t| borsh::to_vec(&t).unwrap()))
        })
        .collect();
    Fixture { binding, generated: run.generated, parts }
}
// A second fixture deliberately places the greedy winner in the ragged second
// vocabulary tile. A scalar input-dependent offset cannot change the winner.
fn multi_tile_fixture(form: PalwPromptIdsFormV1, input: Vec<u32>, model_offset: i128) -> Fixture {
    use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder};
    const VOCAB: usize = 4101;
    let mut p = ProgramBuilder::new(VOCAB as u32, 1 << 18);
    let embed = p.param("scalar_embed", DType::I8, &[VOCAB as u32, 1], false);
    let mut pre = p.block("pre", vec![]);
    let x = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let mut values: Vec<i128> = (0..VOCAB).map(|i| (i % 17) as i128 - 10).collect();
    values[4096] = 1000;
    values[4100] = 1000; // Equal maxima: the smaller lane 4096 must win.
    let logits_bias = p.konst(DType::I32, &[VOCAB as u32], &values);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[1])]);
    let offset = post.cast(Ref::CarryIn(0), DType::I32);
    let logits = post.add(logits_bias, offset, DType::I32);
    let Ref::Node(logits_node) = logits else { unreachable!() };
    post.commit(logits);
    let post = post.finish(&[]);
    let mut p = node_common::tiled(p.finish(pre, vec![], post, logits_node));
    for block in &mut p.blocks {
        for node in &mut block.nodes {
            node.commit = true;
        }
    }
    let mut params = MapParams::default();
    params.tensors.insert(
        (0, None),
        Tensor::new(DType::I8, vec![VOCAB, 1], (0..VOCAB).map(|i| (i % 3) as i128 + model_offset).collect()).unwrap(),
    );
    run_fixture(p, params, input, 2, form, false)
}

impl Fixture {
    fn plan(&self, form: PalwPromptIdsFormV1, limits: CheckLimits) -> Result<CoveragePlan, CheckError> {
        CoveragePlan::derive(&self.binding.class, self.binding.artifact_root, &self.binding.job_context, form, limits)
    }
    fn checker(&self, form: PalwPromptIdsFormV1) -> ExactChecker {
        ExactChecker::begin_greedy(
            self.plan(form, limits()).unwrap(),
            self.binding.clone(),
            self.generated.clone(),
            palw_attempt_output_root_v1(&self.binding.job_context, &self.generated),
            PalwDecodeSamplingV2::GREEDY,
        )
        .unwrap()
    }
}
fn submit(checker: &mut ExactChecker, part: &Part) -> Result<(), CheckError> {
    checker.submit_encoded(&part.0, part.1.as_deref())
}

#[test]
fn cold_node_checks_all_leaves_history_and_every_output_row() {
    for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
        let f = fixture(form, false);
        let plan = f.plan(form, limits()).unwrap();
        let cost = plan.cost();
        assert_eq!(cost.parts, f.parts.len() as u64);
        assert_eq!(cost.evaluation_terms, cost.parts * limits().max_terms_per_leaf);
        assert_eq!(cost.encoded_bytes, cost.parts * limits().max_part_bytes);
        assert_eq!(cost.decoded_part_bytes, limits().max_part_heap);
        assert!(plan.leaf(plan.leaf_count()).is_none());
        assert!((0..plan.leaf_count()).any(|i| matches!(plan.leaf(i).unwrap().kind, PalwTirLeafKindV1::HistTile { .. })));
        assert!((0..plan.leaf_count()).any(|i| matches!(plan.leaf(i).unwrap().kind, PalwTirLeafKindV1::State { .. })));
        let mut check = f.checker(form);
        // Independent expected tokens for this fixture's sums and greedy ties.
        // Each row of embed is ((i*37+11)%256)-128. Three sliding-window
        // layer sums clamp to i8. The resulting logits tie at lanes 0 and 1.
        assert_eq!(f.generated, vec![0, 0, 0]);
        assert_eq!(check.finish(), Err(CheckError::Incomplete));
        let parts = f.parts;
        drop(f.binding); // Node receives encoded parts, no held parameter source.
        for (i, p) in parts.iter().enumerate() {
            submit(&mut check, p).unwrap();
            assert_eq!(check.progress().next_leaf, i as u64 + 1);
        }
        assert_eq!(check.finish(), Ok(CheckOutcome::Matched));
        assert_eq!(check.progress().checked_rows, 3);
        assert_eq!(check.finish(), Err(CheckError::Closed));
    }
}

#[test]
fn omitted_final_leaf_duplicate_and_reordering_cannot_complete() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    let mut c = f.checker(PalwPromptIdsFormV1::Flat);
    for p in &f.parts[..f.parts.len() - 1] {
        submit(&mut c, p).unwrap();
    }
    assert_eq!(c.finish(), Err(CheckError::Incomplete));
    for index in [0, 2] {
        let mut c = f.checker(PalwPromptIdsFormV1::Flat);
        submit(&mut c, &f.parts[0]).unwrap();
        assert_eq!(submit(&mut c, &f.parts[index]), Err(CheckError::Order));
        assert_eq!(c.progress().next_leaf, 1);
        assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
        assert_eq!(c.finish(), Err(CheckError::Closed));
    }
}

#[test]
fn merkle_authenticated_arithmetic_lie_rejects_only_the_panel_candidate() {
    let f = fixture(PalwPromptIdsFormV1::Flat, true);
    let mut c = f.checker(PalwPromptIdsFormV1::Flat);
    let error = f.parts.iter().find_map(|p| submit(&mut c, p).err());
    assert_eq!(error, Some(CheckError::PanelEvidence));
    assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
    assert_eq!(c.finish(), Err(CheckError::Closed));
}

#[test]
fn unauthenticated_model_material_cannot_replace_the_registered_inventory() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    let mut c = f.checker(PalwPromptIdsFormV1::Flat);
    let mut cone: PalwTirConeRefutationV1 = borsh::from_slice(&f.parts[0].0).unwrap();
    assert!(matches!(cone.params, PalwTirParamOpeningV1::Single(_)));
    cone.params = PalwTirParamOpeningV1::None;
    assert_eq!(c.submit_encoded(&borsh::to_vec(&cone).unwrap(), None), Err(CheckError::PanelEvidence));
    assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
}

#[test]
fn output_mismatch_is_reported_only_after_complete_arithmetic() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    let mut c = ExactChecker::begin_greedy(
        f.plan(PalwPromptIdsFormV1::Flat, limits()).unwrap(),
        f.binding.clone(),
        f.generated.clone(),
        Hash64::from_u64_word(999),
        PalwDecodeSamplingV2::GREEDY,
    )
    .unwrap();
    assert_eq!(c.finish(), Err(CheckError::Incomplete));
    for p in &f.parts {
        submit(&mut c, p).unwrap();
    }
    assert_eq!(c.finish(), Ok(CheckOutcome::OutputMismatch));
}

#[test]
fn wrong_model_input_count_and_empty_schedule_are_rejected() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    let mut class = f.binding.class.clone();
    class.tokenizer_id = Hash64::from_u64_word(999);
    assert_eq!(
        CoveragePlan::derive(&class, f.binding.artifact_root, &f.binding.job_context, PalwPromptIdsFormV1::Flat, limits())
            .unwrap_err(),
        CheckError::Binding
    );
    for change in 0..3 {
        let mut binding = f.binding.clone();
        match change {
            0 => binding.artifact_root = Hash64::from_u64_word(999),
            1 => binding.job_context.prompt_token_ids_hash = Hash64::from_u64_word(999),
            _ => binding.step_leaf_count = 0,
        }
        assert!(matches!(
            ExactChecker::begin_greedy(
                f.plan(PalwPromptIdsFormV1::Flat, limits()).unwrap(),
                binding,
                f.generated.clone(),
                Hash64::from_u64_word(999),
                PalwDecodeSamplingV2::GREEDY,
            ),
            Err(CheckError::Binding)
        ));
    }
}

#[test]
fn sparse_legacy_commitments_do_not_authorize_full_coverage() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    let mut class = f.binding.class.clone();
    let mut p = class.decode_program().unwrap();
    p.blocks[p.schedule.pre as usize].nodes.last_mut().unwrap().commit = false;
    class.program = p.encode();
    class.layout = node_common::layout(&p, 3, 2, 2, 64);
    let mut job = f.binding.job_context.clone();
    job.shape_profile_id = class.class_id(&f.binding.artifact_root);
    assert_eq!(
        CoveragePlan::derive(&class, f.binding.artifact_root, &job, PalwPromptIdsFormV1::Flat, limits()).unwrap_err(),
        CheckError::UnsupportedProfile
    );
}

#[test]
fn stochastic_selection_is_unsupported_instead_of_a_miner_refutation() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    assert!(matches!(
        ExactChecker::begin_greedy(
            f.plan(PalwPromptIdsFormV1::Flat, limits()).unwrap(),
            f.binding.clone(),
            f.generated.clone(),
            Hash64::from_u64_word(0),
            PalwDecodeSamplingV2 { seed: [7; 32], temperature_q: 1 << 24 }
        ),
        Err(CheckError::UnsupportedProfile)
    ));
}

#[test]
fn missing_logits_bridge_and_forged_losing_lane_are_rejected() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    let index = f.parts.iter().position(|p| p.1.is_some()).unwrap();
    for omit in [false, true] {
        let mut c = f.checker(PalwPromptIdsFormV1::Flat);
        for p in &f.parts[..index] {
            submit(&mut c, p).unwrap();
        }
        let (cone, trace) = &f.parts[index];
        let bad = if omit {
            None
        } else {
            let mut t: PalwTirTraceEventDisclosureV1 = borsh::from_slice(trace.as_ref().unwrap()).unwrap();
            if let PalwTirTraceEventDisclosureV1::Tiled { tile_lanes, .. } = &mut t {
                tile_lanes[3] ^= 1;
            }
            Some(borsh::to_vec(&t).unwrap())
        };
        assert!(c.submit_encoded(cone, bad.as_deref()).is_err());
        assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
    }
}

#[test]
fn budgets_overflow_heap_caps_and_malformed_vectors_fail_closed() {
    let f = fixture(PalwPromptIdsFormV1::Flat, false);
    for change in 0..4 {
        let mut l = limits();
        match change {
            0 => l.max_part_bytes = u64::MAX,
            1 => l.max_total_elements = 1,
            2 => l.max_total_terms = 1,
            _ => l.max_total_bytes = 1,
        }
        assert_eq!(f.plan(PalwPromptIdsFormV1::Flat, l).unwrap_err(), CheckError::Budget);
    }
    let mut l = limits();
    l.max_part_heap = 1;
    let mut c = ExactChecker::begin_greedy(
        f.plan(PalwPromptIdsFormV1::Flat, l).unwrap(),
        f.binding.clone(),
        f.generated.clone(),
        Hash64::from_u64_word(0),
        PalwDecodeSamplingV2::GREEDY,
    )
    .unwrap();
    assert_eq!(submit(&mut c, &f.parts[0]), Err(CheckError::Budget));
    assert_eq!(c.progress().charged_elements, l.max_elements_per_leaf);
    let mut c = f.checker(PalwPromptIdsFormV1::Flat);
    let mut bad = f.parts[0].0.clone();
    bad.extend([0xff; 4]);
    assert_eq!(c.submit_encoded(&bad, None), Err(CheckError::Wire));
    assert_eq!(c.progress().charged_bytes, bad.len() as u64);
    assert_eq!(submit(&mut c, &f.parts[0]), Err(CheckError::Closed));
    let cone: PalwTirConeRefutationV1 = borsh::from_slice(&f.parts[0].0).unwrap();
    let mut hostile = f.parts[0].0.clone();
    let program = &cone.binding.class.program;
    let start = hostile.windows(program.len()).position(|w| w == program).unwrap();
    hostile[start - 4..start].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut c = f.checker(PalwPromptIdsFormV1::Flat);
    assert_eq!(c.submit_encoded(&hostile, None), Err(CheckError::Wire));
    assert_eq!(c.progress().next_leaf, 0);
}

#[test]
fn ragged_second_logits_tile_is_required_and_global_ties_choose_the_first_lane() {
    for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
        let f = multi_tile_fixture(form, vec![1, 2], 0);
        // Independently constructed bias has two maxima, at 4096 and 4100;
        // the input contributes the same scalar to all 4101 lanes.
        assert_eq!(f.generated, vec![4096, 4096]);
        let trace_indices: Vec<_> = f.parts.iter().enumerate().filter_map(|(i, p)| p.1.as_ref().map(|_| i)).collect();
        assert_eq!(trace_indices.len(), 4);
        let first_second_tile = trace_indices[1];
        let mut c = f.checker(form);
        for p in &f.parts[..first_second_tile] {
            submit(&mut c, p).unwrap();
        }
        assert_eq!(c.progress().checked_rows, 0);
        assert_eq!(c.progress().next_lane, 4096);
        assert_eq!(c.finish(), Err(CheckError::Incomplete));
        for p in &f.parts[first_second_tile..f.parts.len() - 1] {
            submit(&mut c, p).unwrap();
        }
        assert_eq!(c.progress().checked_rows, 1);
        assert_eq!(c.progress().next_lane, 4096);
        assert_eq!(c.finish(), Err(CheckError::Incomplete));
        submit(&mut c, f.parts.last().unwrap()).unwrap();
        assert_eq!(c.finish(), Ok(CheckOutcome::Matched));
        assert_eq!(c.progress().checked_rows, 2);
    }
}

#[test]
fn later_logits_tile_cannot_reuse_another_tile_or_rows_trace_opening() {
    let form = PalwPromptIdsFormV1::Flat;
    let f = multi_tile_fixture(form, vec![1, 2], 0);
    let indices: Vec<_> = f.parts.iter().enumerate().filter_map(|(i, p)| p.1.as_ref().map(|_| i)).collect();
    for (target, wrong_trace) in [(indices[1], indices[0]), (indices[3], indices[1])] {
        let mut c = f.checker(form);
        for p in &f.parts[..target] {
            submit(&mut c, p).unwrap();
        }
        assert_eq!(c.submit_encoded(&f.parts[target].0, f.parts[wrong_trace].1.as_deref()), Err(CheckError::PanelEvidence));
        assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
        assert_eq!(c.finish(), Err(CheckError::Closed));
    }
}

#[test]
fn valid_alternative_model_and_input_openings_cannot_be_spliced_into_the_admitted_job() {
    let form = PalwPromptIdsFormV1::Flat;
    let f = multi_tile_fixture(form, vec![1, 2], 0);
    let alternative_model = multi_tile_fixture(form, vec![1, 2], 1);
    let alternative_input = multi_tile_fixture(form, vec![2, 1], 0);
    // These are full, internally consistent alternate executions and openings.
    // Their matching outputs do not authorize replacing the admitted identities.
    for alternative in [&alternative_model, &alternative_input] {
        assert_eq!(alternative.generated, f.generated);
        assert!(matches!(
            ExactChecker::begin_greedy(
                f.plan(form, limits()).unwrap(),
                alternative.binding.clone(),
                alternative.generated.clone(),
                palw_attempt_output_root_v1(&f.binding.job_context, &f.generated),
                PalwDecodeSamplingV2::GREEDY
            ),
            Err(CheckError::Binding)
        ));
        let mut c = f.checker(form);
        assert_eq!(submit(&mut c, &alternative.parts[0]), Err(CheckError::Binding));
    }
    let root_cone: PalwTirConeRefutationV1 = borsh::from_slice(&f.parts[0].0).unwrap();
    let model_cone: PalwTirConeRefutationV1 = borsh::from_slice(&alternative_model.parts[0].0).unwrap();
    let input_cone: PalwTirConeRefutationV1 = borsh::from_slice(&alternative_input.parts[0].0).unwrap();
    assert_ne!(root_cone.params, model_cone.params);
    assert_ne!(root_cone.prompt_token_ids, input_cone.prompt_token_ids);
    for replacement in 0..2 {
        let mut c = f.checker(form);
        let mut splice = root_cone.clone();
        if replacement == 0 {
            splice.params = model_cone.params.clone();
        } else {
            splice.prompt_token_ids = input_cone.prompt_token_ids.clone();
        }
        assert_eq!(c.submit_encoded(&borsh::to_vec(&splice).unwrap(), None), Err(CheckError::PanelEvidence));
        assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
    }
}
