#![allow(dead_code)]
#[path = "../node_common/mod.rs"]
mod node_common;

use kaspa_consensus_core::{
    Hash64, palw_prompt_ids_v1::PalwPromptIdsFormV1, palw_tir_class_v1::PalwTirClassV1, palw_tir_step_v1::PalwTirStepSpaceV1,
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{
    ConeEnv, DType, Interpreter, MapParams, Ref, Rounding, RunState, StateKind, Tensor, TensorType, TirProgramV1,
    builder::ProgramBuilder, interp::CommitRecord,
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{TirClassRunnerV1, TirHeldInventoryV1, TirParamsSourceV1},
};
use std::collections::BTreeMap;

pub const PROMPT: &[u32] = &[0, 1];
pub const EXPECTED_GENERATED: &[u32] = &[0, 1, 0];
pub const LEAF_CAP: u64 = 1 << 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpectedNode {
    pub position: u32,
    pub occurrence: u32,
    pub slot: u32,
    pub block: u8,
    pub layer: Option<u16>,
    pub node: u16,
    pub commit: bool,
    pub value: Tensor,
}
#[derive(Clone, Debug)]
pub struct ExpectedStep {
    pub position: u32,
    pub state: RunState,
    pub output: Option<(Tensor, u32)>,
}
pub struct Fixture {
    pub program: TirProgramV1,
    pub params: MapParams,
    pub class: PalwTirClassV1,
    pub artifact: Hash64,
    pub job: PalwJobContextV2,
    pub nodes: Vec<ExpectedNode>,
    pub steps: Vec<ExpectedStep>,
    pub encoded_program: Vec<u8>,
}

fn program(window: u32) -> (TirProgramV1, MapParams) {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embedding = p.param("embedding", DType::I8, &[4, 4], false);
    let layer_offset = p.param("layer_offset", DType::I32, &[4], true);
    let factor64 = p.scalar(DType::I64, 1i128 << 40);
    let factor128 = p.scalar(DType::I128, 1i128 << 40);
    let global_fixed = p.fixed_state("global_last", DType::I32, &[4], -32768, 32767, false);
    let global_hist = p.hist_state("global_rows", DType::I32, &[4], window, false);
    let layer_fixed = p.fixed_state("layer_last", DType::I32, &[4], -32768, 32767, true);
    let layer_hist = p.hist_state("layer_rows", DType::I32, &[4], window, true);
    let mut pre = p.block("pre", vec![]);
    let x = pre.gather(embedding, Ref::Input(0), 0, 0);
    let wide = pre.cast(x, DType::I64);
    let wide = pre.mul(wide, factor64, DType::I64);
    let wider = pre.cast(wide, DType::I128);
    let wider = pre.mul(wider, factor128, DType::I128);
    let wide = pre.div(wider, factor128, Rounding::Floor, DType::I64);
    let carry = pre.div(wide, factor64, Rounding::Floor, DType::I32);
    let rows = pre.hist_append(global_hist, carry);
    let sum = pre.reduce_sum(rows, 0, DType::I32);
    let sum = pre.reshape_fixed(sum, &[4]);
    let accumulated = pre.add(Ref::State(global_fixed), sum, DType::I32);
    pre.state_write(global_fixed, accumulated);
    let pre = pre.finish(&[carry]);
    let mut layer = p.block("repeated_layer", vec![TensorType::fixed(DType::I32, &[4])]);
    let carry = layer.add(Ref::CarryIn(0), layer_offset, DType::I32);
    let wide = layer.cast(carry, DType::I64);
    let wide = layer.mul(wide, factor64, DType::I64);
    let wider = layer.cast(wide, DType::I128);
    let wider = layer.mul(wider, factor128, DType::I128);
    let wide = layer.div(wider, factor128, Rounding::Floor, DType::I64);
    let carry = layer.div(wide, factor64, Rounding::Floor, DType::I32);
    let rows = layer.hist_append(layer_hist, carry);
    let sum = layer.reduce_sum(rows, 0, DType::I32);
    let sum = layer.reshape_fixed(sum, &[4]);
    let accumulated = layer.add(Ref::State(layer_fixed), sum, DType::I32);
    layer.state_write(layer_fixed, accumulated);
    let layer = layer.finish(&[carry]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I32, &[4])]);
    let logits = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(logits);
    let Ref::Node(logits_node) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let program = node_common::tiled(p.finish(pre, vec![layer, layer], post, logits_node));
    assert!(program.blocks.iter().flat_map(|b| &b.nodes).any(|n| !n.commit));
    assert!(
        program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| matches!(n.out.dtype, DType::I64 | DType::I128)).all(|n| !n.commit)
    );
    let mut params = MapParams::default();
    params
        .tensors
        .insert((0, None), Tensor::new(DType::I8, vec![4, 4], vec![1, 9, 3, 2, 5, 5, 1, -2, -3, -2, -1, 0, 0, 7, 10, 2]).unwrap());
    params.tensors.insert((1, Some(0)), Tensor::new(DType::I32, vec![4], vec![1, 0, 0, 0]).unwrap());
    params.tensors.insert((1, Some(1)), Tensor::new(DType::I32, vec![4], vec![0, 1, 0, 0]).unwrap());
    (program, params)
}

fn env(program: &TirProgramV1, before: &RunState, commits: &[CommitRecord], occ: usize, target: u16, token: u32) -> ConeEnv {
    let bases = program.occurrence_slot_bases();
    let occurrences = program.occurrences();
    let (block, layer) = occurrences[occ];
    let end = bases[occ] + program.blocks[block as usize].nodes.len() as u32;
    let supplied = commits
        .iter()
        .filter(|c| c.slot >= bases[occ] && c.slot < end && c.node != target)
        .map(|c| (c.node, c.value.clone()))
        .collect();
    let carry_in = if occ == 0 {
        BTreeMap::new()
    } else {
        let (previous, _) = occurrences[occ - 1];
        program.blocks[previous as usize]
            .carry_out
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let value = commits.iter().find(|c| c.slot == bases[occ - 1] + u32::from(*node)).unwrap().value.clone();
                (index as u8, value)
            })
            .collect()
    };
    let fixed = program
        .states
        .iter()
        .enumerate()
        .filter(|(_, s)| s.per_layer == layer.is_some() && matches!(s.kind, StateKind::Fixed { .. }))
        .map(|(state, s)| {
            let zero = || Tensor::zeros(s.dtype, &s.shape.iter().map(|n| *n as usize).collect::<Vec<_>>());
            (state as u16, before.fixed.get(&(state as u16, layer)).cloned().unwrap_or_else(zero))
        })
        .collect();
    let hist_prior = program
        .states
        .iter()
        .enumerate()
        .filter(|(_, s)| s.per_layer == layer.is_some() && matches!(s.kind, StateKind::Hist { .. }))
        .map(|(state, _)| {
            (state as u16, before.hist.get(&(state as u16, layer)).map(|r| r.iter().cloned().collect()).unwrap_or_default())
        })
        .collect();
    ConeEnv { token: Some(token), pos: before.pos, carry_in, fixed, hist_prior, supplied }
}
fn reference(program: &TirProgramV1, params: &MapParams) -> (Vec<ExpectedNode>, Vec<ExpectedStep>) {
    let reference = Interpreter::new(program).unwrap();
    let mut state = RunState::default();
    let occurrences = program.occurrences();
    let bases = program.occurrence_slot_bases();
    let mut nodes = Vec::new();
    let mut steps = Vec::new();
    let mut generated = Vec::new();
    for position in 0..4u32 {
        let token = if position < 2 { PROMPT[position as usize] } else { *generated.last().unwrap() };
        let before = state.clone();
        let step = reference.step(params, &mut state, token).unwrap();
        let run_post = position + 1 >= PROMPT.len() as u32;
        for (occurrence, (block, layer)) in occurrences.iter().copied().enumerate() {
            if !run_post && occurrence + 1 == occurrences.len() {
                continue;
            }
            for (node, decl) in program.blocks[block as usize].nodes.iter().enumerate() {
                let value = reference
                    .eval_cone(
                        block,
                        layer,
                        node as u16,
                        params,
                        &env(program, &before, &step.commits, occurrence, node as u16, token),
                    )
                    .unwrap();
                nodes.push(ExpectedNode {
                    position,
                    occurrence: occurrence as u32,
                    slot: bases[occurrence] + node as u32,
                    block,
                    layer,
                    node: node as u16,
                    commit: decl.commit,
                    value,
                });
            }
        }
        let output = if run_post {
            let selected = step.logits.data.iter().enumerate().reduce(|a, b| if b.1 > a.1 { b } else { a }).unwrap().0 as u32;
            generated.push(selected);
            Some((step.logits, selected))
        } else {
            None
        };
        steps.push(ExpectedStep { position, state: state.clone(), output });
    }
    assert_eq!(generated, EXPECTED_GENERATED);
    assert!(nodes.iter().any(|n| n.value.dtype == DType::I64 && n.value.data.iter().any(|v| v.unsigned_abs() > i32::MAX as u128)));
    assert!(nodes.iter().any(|n| n.value.dtype == DType::I128 && n.value.data.iter().any(|v| v.unsigned_abs() > i64::MAX as u128)));
    (nodes, steps)
}
pub fn with_fixture(body: impl FnOnce(&Fixture, &TirClassRunnerV1<'_>, &TirHeldInventoryV1<'_>)) {
    with_window_fixture(3, body)
}
pub fn with_window_fixture(window: u32, body: impl FnOnce(&Fixture, &TirClassRunnerV1<'_>, &TirHeldInventoryV1<'_>)) {
    let (program, params) = program(window);
    let plan = TirPlan::compile(&program).unwrap();
    let exec_params = TirParams::from_map(&plan, &params).unwrap();
    let source = TirParamsSourceV1(&exec_params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: node_common::layout(&program, 3, 2, window.min(2), 16),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let mut job = node_common::job(&program, 2, 3, class.class_id(&artifact), PROMPT, PalwPromptIdsFormV1::Flat);
    job.tokenizer_id = class.tokenizer_id;
    let (nodes, steps) = reference(&program, &params);
    let encoded_program = program.encode();
    let f = Fixture { program: program.clone(), params, class, artifact, job, nodes, steps, encoded_program };
    let space = PalwTirStepSpaceV1::new(&f.class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &exec_params, f.job.shape_profile_id).unwrap();
    body(&f, &runner, &inventory);
}
