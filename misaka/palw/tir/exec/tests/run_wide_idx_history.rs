#![cfg(feature = "node")]
//! High-bit Idx NODE and legal I32 state regression for mixed-width execution.
//! Idx Fixed/Hist states remain structurally forbidden. The paired new programs
//! share old signed-state bit patterns without widening the state profile.
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_logits_q24_v2,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_leg::{PalwStepTileLeafV1, step_tile_leaf_hash_v1},
    palw_step_refute::{tiled_logits_scheme_id_v1, tiled_logits_trace_root_v1},
    palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_step_v1::{
        PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepLegBuilderV1, PalwTirStepLegBuilderV2, PalwTirStepSpaceV1, PalwTirStepSpaceV2,
        palw_tir_leaf_preimage_v1, palw_tir_leaf_preimage_v2,
    },
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{
    DType, Interpreter, MapParams, Ref, RunState, Tensor, TirProgramV1, builder::ProgramBuilder, interp::CommitRecord,
};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirHeldInventoryV1, TirParamsSourceV1,
        run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
    },
};

const IDX: &[i128] = &[u32::MAX as i128, 1i128 << 31, (1i128 << 31) + 7];
const STATE: &[i128] = &[-1, i32::MIN as i128, i32::MIN as i128 + 7];
const LEAVES: u64 = 1 << 12;

fn declaration(wide: bool, window: u32) -> (TirProgramV1, MapParams) {
    let mut p = ProgramBuilder::new(2, 1 << 18);
    let index_row = p.param("index_row", DType::Idx, &[3], false);
    let state_row = p.param("signed_state_row", DType::I32, &[3], false);
    let logits_dtype = if wide { DType::I64 } else { DType::I32 };
    let output = p.param("output", logits_dtype, &[2], false);
    let fixed = p.fixed_state("last_signed_row", DType::I32, &[3], i64::from(i32::MIN), i64::from(i32::MAX), false);
    let history = p.hist_state("signed_history", DType::I32, &[3], window, false);
    let mut pre = p.block("pre", vec![]);
    let row = pre.reshape_fixed(index_row, &[3]);
    pre.commit(row);
    let signed = pre.reshape_fixed(state_row, &[3]);
    pre.commit(signed);
    pre.state_write(fixed, signed);
    pre.hist_append(history, signed);
    let pre = pre.finish(&[]);
    let mut post = p.block("post", vec![]);
    let logits = post.reshape_fixed(output, &[2]);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = if wide { palw_logits_q24_v2::scheme_id().as_bytes() } else { tiled_logits_scheme_id_v1().as_bytes() };
    misaka_palw_tir::validate::validate(&program).unwrap();
    let mut params = MapParams::default();
    params.tensors.insert((0, None), Tensor::new(DType::Idx, vec![3], IDX.to_vec()).unwrap());
    params.tensors.insert((1, None), Tensor::new(DType::I32, vec![3], STATE.to_vec()).unwrap());
    params.tensors.insert(
        (2, None),
        Tensor::new(logits_dtype, vec![2], if wide { vec![1i128 << 33, 3i128 << 32] } else { vec![2, 3] }).unwrap(),
    );
    (program, params)
}

fn reference(program: &TirProgramV1, params: &MapParams, window: u32) -> (Vec<Vec<CommitRecord>>, Vec<RunState>, Vec<Vec<i64>>) {
    let interpreter = Interpreter::new(program).unwrap();
    let mut state = RunState::default();
    let mut commits = Vec::new();
    let mut states = Vec::new();
    let mut rows = Vec::new();
    for position in 0..3 {
        let step = interpreter.step(params, &mut state, if position == 0 { 0 } else { 1 }).unwrap();
        assert_eq!(state.fixed[&(0, None)].dtype, DType::I32);
        assert_eq!(state.fixed[&(0, None)].data, STATE);
        let prior = &state.hist[&(1, None)];
        assert_eq!(prior.len(), (position + 1).min(window as usize - 1));
        assert!(prior.iter().all(|row| row.dtype == DType::I32 && row.data == STATE));
        assert_eq!(step.logits.data.iter().enumerate().max_by_key(|(_, value)| *value).unwrap().0, 1);
        rows.push(step.logits.data.iter().map(|value| i64::try_from(*value).unwrap()).collect());
        commits.push(step.commits);
        states.push(state.clone());
    }
    (commits, states, rows)
}

fn reference_values(leaf: &PalwTirLeafV1, commits: &[Vec<CommitRecord>], states: &[RunState]) -> Vec<i128> {
    match leaf.kind {
        PalwTirLeafKindV1::Commit { first_element, .. } => {
            let record = commits[leaf.position as usize].iter().find(|record| record.slot == leaf.coord.node_slot).unwrap();
            record.value.data[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
        }
        PalwTirLeafKindV1::State { state, layer, first_element, .. } => {
            let tensor = &states[leaf.position as usize].fixed[&(state, layer)];
            tensor.data[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
        }
        PalwTirLeafKindV1::HistTile { state, layer, first_lane, row_lanes, first_position, .. } => {
            assert_eq!((state, layer), (1, None));
            // Literal append oracle retains rows independently of the reference
            // history window, including window=1 which immediately evicts them.
            (first_position..=leaf.position)
                .flat_map(|_| STATE[first_lane as usize..first_lane as usize + row_lanes as usize].iter().copied())
                .collect()
        }
    }
}

fn run_profile(wide: bool, window: u32) -> Vec<PalwStepTileLeafV1> {
    let (program, map) = declaration(wide, window);
    let (reference_commits, reference_states, reference_rows) = reference(&program, &map, window);
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 16,
            checkpoint_interval: 1,
            h_tile: 2,
            commit_tiles: vec![4; 3],
            state_tiles: vec![4; 2],
        },
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let artifact = inventory.tree.root();
    let class_id = class.class_id(&artifact);
    let zero = Hash64::from_u64_word(0);
    let job = PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: Hash64::from_u64_word(42),
        job_nullifier: zero,
        assignment_id: zero,
        execution_seed: [3; 32],
        model_profile_id: zero,
        runtime_manifest_hash: zero,
        runtime_class_id: zero,
        shape_profile_id: class_id,
        trace_scheme_id: if wide { palw_logits_q24_v2::scheme_id() } else { tiled_logits_scheme_id_v1() },
        cu_ruleset_id: zero,
        tokenizer_id: zero,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &[0]).unwrap(),
        declared_prefill_tokens: 1,
        exact_decode_tokens: 3,
        max_context_tokens: 16,
    };
    let original = job.clone();
    let mut leaves = Vec::new();
    let mut fixed_tiles = 0;
    let mut history_tiles = 0;
    if wide {
        let space = PalwTirStepSpaceV2::new(&class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, &plan, &params, class_id).unwrap();
        let run = runner
            .run(
                &job,
                &[0],
                job.context_hash(),
                TirWideRunLimitsV2 {
                    max_positions: 16,
                    max_step_leaves: LEAVES,
                    max_row_lanes: 2,
                    max_work: 1 << 40,
                    max_requested_bytes: 1 << 34,
                },
                &mut |leaf| leaves.push(leaf.preimage.clone()),
            )
            .unwrap();
        assert_eq!(run.logits_rows(), reference_rows);
        assert_eq!(run.generated(), &[1, 1, 1]);
        assert_eq!(run.output_root(), palw_attempt_output_root_v1(&job, &[1, 1, 1]));
        assert_eq!(run.trace_root(), palw_logits_q24_v2::trace_root(&job, &reference_rows, &[1, 1, 1]).unwrap());
        let mut builder = PalwTirStepLegBuilderV2::new(&space, &job, class_id, LEAVES).unwrap();
        for (index, preimage) in leaves.iter().enumerate() {
            let leaf = space.leaf_at(&job, index as u64).unwrap();
            let values = reference_values(&leaf, &reference_commits, &reference_states);
            assert_eq!(*preimage, palw_tir_leaf_preimage_v2(&space, &job, &leaf, &values).unwrap());
            assert_eq!(builder.push(&values).unwrap(), leaf);
            assert_eq!(run.leaf_hashes()[index], step_tile_leaf_hash_v1(&job.context_hash(), &class_id, preimage));
            match leaf.kind {
                PalwTirLeafKindV1::State { .. } => fixed_tiles += 1,
                PalwTirLeafKindV1::HistTile { .. } => history_tiles += 1,
                _ => {}
            }
        }
        assert_eq!(builder.finish().unwrap(), (run.leaf_count(), run.step_merkle_root()));
    } else {
        let space = PalwTirStepSpaceV1::new(&class).unwrap();
        let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
        let run = runner.run(&job, &[0], LEAVES, true, &mut |leaf| leaves.push(leaf.preimage.clone())).unwrap();
        let rows: Vec<Vec<i32>> =
            reference_rows.iter().map(|row| row.iter().map(|value| i32::try_from(*value).unwrap()).collect()).collect();
        assert_eq!(run.logits_rows, rows);
        assert_eq!(run.generated, &[1, 1, 1]);
        assert_eq!(run.output_root, palw_attempt_output_root_v1(&job, &[1, 1, 1]));
        assert_eq!(run.trace_root, tiled_logits_trace_root_v1(&job, &rows, &[1, 1, 1]).unwrap());
        let mut builder = PalwTirStepLegBuilderV1::new(&space, &job, class_id, LEAVES).unwrap();
        for (index, preimage) in leaves.iter().enumerate() {
            let leaf = space.leaf_at(&job, index as u64).unwrap();
            let values = reference_values(&leaf, &reference_commits, &reference_states);
            assert_eq!(*preimage, palw_tir_leaf_preimage_v1(&leaf, &values).unwrap());
            assert_eq!(builder.push(&values).unwrap(), leaf);
            match leaf.kind {
                PalwTirLeafKindV1::State { .. } => fixed_tiles += 1,
                PalwTirLeafKindV1::HistTile { .. } => history_tiles += 1,
                _ => {}
            }
        }
        assert_eq!(builder.finish().unwrap(), (run.leaf_count, run.step_merkle_root));
    }
    assert_eq!(fixed_tiles, 3);
    assert_eq!(history_tiles, 1);
    assert_eq!(job, original);
    assert_eq!(job.tokenizer_id, zero);
    assert_ne!(job.tokenizer_id, class.tokenizer_id);
    leaves
}

#[test]
fn high_bit_idx_nodes_and_i32_state_keep_exact_legacy_words_in_wide_run() {
    for window in [1, 3] {
        let wide = run_profile(true, window);
        let narrow = run_profile(false, window);
        assert_eq!(wide.len(), narrow.len());
        let mut wide_outputs = 0;
        let mut high_bit_words = 0;
        for (wide, narrow) in wide.iter().zip(&narrow) {
            assert_eq!(wide.coord, narrow.coord);
            assert_eq!(wide.value_count, narrow.value_count);
            if wide.version == 2 {
                wide_outputs += 1;
                assert_eq!(narrow.version, 1);
                assert_eq!(wide.values_le.len(), narrow.values_le.len() * 2);
                assert_eq!(wide.values_le, [1i64 << 33, 3i64 << 32].into_iter().flat_map(i64::to_le_bytes).collect::<Vec<_>>());
            } else {
                assert_eq!(wide, narrow);
                for word in wide.values_le.chunks_exact(4) {
                    let value = u32::from_le_bytes(word.try_into().unwrap());
                    assert!(IDX.contains(&i128::from(value)));
                    assert!(value >= 1u32 << 31);
                    high_bit_words += 1;
                }
            }
        }
        assert_eq!(wide_outputs, 3);
        assert!(high_bit_words >= 6);
    }
}

#[test]
fn idx_fixed_and_history_states_remain_structurally_forbidden_in_both_profiles() {
    for wide in [false, true] {
        let (program, _) = declaration(wide, 3);
        for state in 0..program.states.len() {
            let mut illegal = program.clone();
            illegal.states[state].dtype = DType::Idx;
            let error = misaka_palw_tir::validate::validate(&illegal).unwrap_err();
            assert!(error.msg.contains("must be i8, i16 or i32"), "{error:?}");
            assert!(TirProgramV1::decode_canonical(&illegal.encode()).is_err());
            assert!(Interpreter::new(&illegal).is_err());
            assert!(TirPlan::compile(&illegal).is_err());
        }
    }
}
