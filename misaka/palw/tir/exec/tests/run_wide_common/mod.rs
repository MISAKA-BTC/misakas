//! Independent paired local class declarations for explicit wide-run tests.
//! No native registration receipt, public activation, or computation credit is asserted.
#![allow(dead_code)]

use kaspa_consensus_core::{
    Hash64, palw_logits_q24_v2,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{
    ConeEnv, DType, Interpreter, MapParams, Ref, RunState, StateKind, Tensor, TensorType, TirProgramV1, builder::ProgramBuilder,
    interp::CommitRecord, program::LOGITS_I64_Q24_SCHEME_V2,
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{TirHeldInventoryV1, TirParamsSourceV1},
};
use std::collections::BTreeMap;

pub const PROMPT: &[u32] = &[0, 1];
pub const EXPECTED_GENERATED: &[u32] = &[2, 0, 1, 2, 0, 1];
pub const EXPECTED_TOKENS: &[u32] = &[0, 1, 2, 0, 1, 2, 0];
pub const MAX_CONTEXT: u32 = 16;
pub const LEAF_CAP: u64 = 1 << 16;
pub const EMBEDDING: &[i128] = &[1, 9, 3, 2, 5, 5, 1, -2, -3, -2, -1, 0, 0, 7, 10, 2];

pub fn logits_table(wide: bool) -> Vec<i128> {
    if wide {
        let lo = i64::MIN as i128;
        let hi = i64::MAX as i128;
        vec![lo, lo + 1, lo + 1, lo, lo, 1082i128 << 24, hi, hi, -1, -(1i128 << 40), -1, lo, 0, 0, 0, 0]
    } else {
        // Same ordering and lowest-index ties, independently chosen small values.
        vec![-3, -2, -2, -3, -3, 0, 2, 2, -1, -2, -1, -3, 0, 0, 0, 0]
    }
}

pub fn program(wide: bool, window: u32) -> (TirProgramV1, MapParams) {
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let logits = p.param("logits", if wide { DType::I64 } else { DType::I32 }, &[4, 4], false);
    let fixed = p.fixed_state("last_row", DType::I8, &[4], -128, 127, false);
    let hist = p.hist_state("rows", DType::I8, &[4], window, false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    pre.state_write(fixed, row);
    let rows = pre.hist_append(hist, row);
    let rows = pre.cast(rows, DType::I32);
    pre.commit(rows);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let narrow = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(narrow);
    // Only this actual final post node is a committed wide value. Reading a
    // table permits exact i64 endpoints without a potentially overflowing op.
    let last = post.gather(logits, Ref::Input(0), 0, 0);
    post.commit(last);
    let Ref::Node(last) = last else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, last);
    program.logits_scheme_id = if wide { LOGITS_I64_Q24_SCHEME_V2 } else { tiled_logits_scheme_id_v1().as_bytes() };
    misaka_palw_tir::validate::validate(&program).unwrap();
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let mut params = MapParams::default();
    params.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], EMBEDDING.to_vec()).unwrap());
    params.tensors.insert((1, None), Tensor::new(if wide { DType::I64 } else { DType::I32 }, vec![4, 4], logits_table(wide)).unwrap());
    (program, params)
}

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

pub struct Fixture<'a> {
    pub program: &'a TirProgramV1,
    pub plan: &'a TirPlan,
    pub params: &'a TirParams<'a>,
    pub class: PalwTirClassV1,
    pub artifact: Hash64,
    pub job: PalwJobContextV2,
    pub nodes: Vec<ExpectedNode>,
    pub steps: Vec<ExpectedStep>,
    pub rows: Vec<Vec<i64>>,
}

fn cone_env(
    program: &TirProgramV1,
    before: &RunState,
    commits: &[CommitRecord],
    occurrence: usize,
    target: u16,
    token: u32,
) -> ConeEnv {
    let occurrences = program.occurrences();
    let bases = program.occurrence_slot_bases();
    let (block, layer) = occurrences[occurrence];
    let end = bases[occurrence] + program.blocks[block as usize].nodes.len() as u32;
    let supplied = commits
        .iter()
        .filter(|c| c.slot >= bases[occurrence] && c.slot < end && c.node != target)
        .map(|c| (c.node, c.value.clone()))
        .collect();
    let carry_in = if occurrence == 0 {
        BTreeMap::new()
    } else {
        let (previous, _) = occurrences[occurrence - 1];
        program.blocks[previous as usize]
            .carry_out
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let value = commits.iter().find(|c| c.slot == bases[occurrence - 1] + u32::from(*node)).unwrap().value.clone();
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
            let value = before
                .fixed
                .get(&(state as u16, layer))
                .cloned()
                .unwrap_or_else(|| Tensor::zeros(s.dtype, &s.shape.iter().map(|n| *n as usize).collect::<Vec<_>>()));
            (state as u16, value)
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

fn reference(program: &TirProgramV1, params: &MapParams) -> (Vec<ExpectedNode>, Vec<ExpectedStep>, Vec<Vec<i64>>) {
    let reference = Interpreter::new(program).unwrap();
    let mut state = RunState::default();
    let occurrences = program.occurrences();
    let bases = program.occurrence_slot_bases();
    let mut nodes = Vec::new();
    let mut steps = Vec::new();
    let mut rows = Vec::new();
    let mut generated = Vec::new();
    for (position, token) in EXPECTED_TOKENS.iter().copied().enumerate() {
        let position = position as u32;
        if position < PROMPT.len() as u32 {
            assert_eq!(token, PROMPT[position as usize]);
        } else {
            assert_eq!(Some(&token), generated.last());
        }
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
                        &cone_env(program, &before, &step.commits, occurrence, node as u16, token),
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
        let output = run_post.then(|| {
            // Reference arithmetic is separate from the node runner. The
            // independently hand-derived sequence is asserted as well.
            let selected = step.logits.data.iter().enumerate().reduce(|a, b| if b.1 > a.1 { b } else { a }).unwrap().0 as u32;
            generated.push(selected);
            rows.push(step.logits.data.iter().map(|v| i64::try_from(*v).unwrap()).collect());
            (step.logits, selected)
        });
        steps.push(ExpectedStep { position, state: state.clone(), output });
    }
    assert_eq!(generated, EXPECTED_GENERATED);
    assert_eq!(nodes.len(), 40);
    (nodes, steps, rows)
}

fn declaration(program: &TirProgramV1, artifact: Hash64, window: u32) -> (PalwTirClassV1, PalwJobContextV2) {
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: MAX_CONTEXT,
            checkpoint_interval: 3,
            h_tile: if window == 1 { 1 } else { 2 },
            commit_tiles: vec![4; 4],
            state_tiles: vec![4; 2],
        },
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let zero = Hash64::from_bytes([0; 64]);
    let job = PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_u64_word(41),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [2; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: class.class_id(&artifact),
        trace_scheme_id: if program.wide_logits_q24() { palw_logits_q24_v2::scheme_id() } else { tiled_logits_scheme_id_v1() },
        cu_ruleset_id: zero,
        tokenizer_id: zero,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, PROMPT).unwrap(),
        declared_prefill_tokens: 2,
        exact_decode_tokens: 6,
        max_context_tokens: MAX_CONTEXT,
    };
    (class, job)
}

pub fn with_fixture(window: u32, body: impl FnOnce(&Fixture<'_>, &Fixture<'_>)) {
    let (wide_program, wide_map) = program(true, window);
    let (narrow_program, narrow_map) = program(false, window);
    let wide_plan = TirPlan::compile(&wide_program).unwrap();
    let narrow_plan = TirPlan::compile(&narrow_program).unwrap();
    let wide_params = TirParams::from_map(&wide_plan, &wide_map).unwrap();
    let narrow_params = TirParams::from_map(&narrow_plan, &narrow_map).unwrap();
    let wide_source = TirParamsSourceV1(&wide_params);
    let narrow_source = TirParamsSourceV1(&narrow_params);
    let wide_inventory = TirHeldInventoryV1::build(&wide_program, &wide_source).unwrap();
    let narrow_inventory = TirHeldInventoryV1::build(&narrow_program, &narrow_source).unwrap();
    let wide_artifact = wide_inventory.tree.root();
    let narrow_artifact = narrow_inventory.tree.root();
    let (wide_class, wide_job) = declaration(&wide_program, wide_artifact, window);
    let (narrow_class, narrow_job) = declaration(&narrow_program, narrow_artifact, window);
    let (wide_nodes, wide_steps, wide_rows) = reference(&wide_program, &wide_map);
    let (narrow_nodes, narrow_steps, narrow_rows) = reference(&narrow_program, &narrow_map);
    let wide = Fixture {
        program: &wide_program,
        plan: &wide_plan,
        params: &wide_params,
        class: wide_class,
        artifact: wide_artifact,
        job: wide_job,
        nodes: wide_nodes,
        steps: wide_steps,
        rows: wide_rows,
    };
    let narrow = Fixture {
        program: &narrow_program,
        plan: &narrow_plan,
        params: &narrow_params,
        class: narrow_class,
        artifact: narrow_artifact,
        job: narrow_job,
        nodes: narrow_nodes,
        steps: narrow_steps,
        rows: narrow_rows,
    };
    body(&wide, &narrow);
}
