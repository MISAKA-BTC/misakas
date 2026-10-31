#![cfg(feature = "node")]
//! Explicit private wide execution, compared with a separate reference evaluator.
//! Commitments authenticate data identity; these small local runs grant no credit.
mod run_wide_common;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_logits_q24_v2,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1},
    palw_step_leg::PalwStepTileLeafV1,
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_step_v1::{
        PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepBindingV1, PalwTirStepLegBuilderV2, PalwTirStepSpaceV1, PalwTirStepSpaceV2,
        palw_tir_execution_root_v1, palw_tir_leaf_preimage_v1, palw_tir_leaf_preimage_v2, palw_tir_leaf_values_v2,
        verify_tir_binding_v1, verify_tir_binding_v2,
    },
};
use misaka_palw_tir::{DType, Tensor};
use misaka_palw_tir_exec::{
    NodeValue, Slice,
    node::{
        TirClassRunnerV1,
        run::TirRunObserverV1,
        run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2, TirWideRunQuoteV2},
    },
};
use run_wide_common::*;
use std::collections::BTreeSet;

fn limits() -> TirWideRunLimitsV2 {
    TirWideRunLimitsV2 {
        max_positions: MAX_CONTEXT,
        max_step_leaves: LEAF_CAP,
        max_row_lanes: 16,
        max_work: 1 << 40,
        max_requested_bytes: 1 << 34,
    }
}

fn exact_limits(quote: &TirWideRunQuoteV2) -> TirWideRunLimitsV2 {
    TirWideRunLimitsV2 {
        max_positions: EXPECTED_TOKENS.len() as u32,
        max_step_leaves: quote.leaf_count,
        max_row_lanes: 4,
        max_work: quote.work,
        max_requested_bytes: quote.requested_bytes,
    }
}

fn leaf_values(f: &Fixture<'_>, leaf: &PalwTirLeafV1) -> Vec<i128> {
    match leaf.kind {
        PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
            let value =
                &f.nodes.iter().find(|n| n.position == leaf.position && n.occurrence == occurrence && n.node == node).unwrap().value;
            value.data[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
        }
        PalwTirLeafKindV1::State { state, layer, first_element, .. } => {
            let value = &f.steps[leaf.position as usize].state.fixed[&(state, layer)];
            value.data[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
        }
        PalwTirLeafKindV1::HistTile { first_lane, row_lanes, first_position, .. } => {
            // The literal append sequence remains available even for window one,
            // whose reference after-step state retains zero previous rows.
            (first_position..=leaf.position)
                .flat_map(|position| {
                    let token = EXPECTED_TOKENS[position as usize] as usize;
                    let first = token * 4 + first_lane as usize;
                    EMBEDDING[first..first + row_lanes as usize].iter().copied()
                })
                .collect()
        }
    }
}

fn hash(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    Hash64::from_bytes(state.finalize().as_bytes().try_into().unwrap())
}

fn manual_leaf_hash(context: Hash64, class: Hash64, leaf: &PalwStepTileLeafV1) -> Hash64 {
    let mut bytes = leaf.version.to_le_bytes().to_vec();
    bytes.extend_from_slice(context.as_byte_slice());
    bytes.extend_from_slice(class.as_byte_slice());
    for scalar in [leaf.coord.call_index, leaf.coord.node_slot, leaf.coord.position, leaf.coord.tile_index, leaf.value_count] {
        bytes.extend_from_slice(&scalar.to_le_bytes());
    }
    bytes.extend_from_slice(&(leaf.values_le.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&leaf.values_le);
    hash(b"misaka-palw/step-tile-leaf/v1", &[&bytes])
}

fn manual_tree(hashes: &[Hash64]) -> Hash64 {
    let mut level: Vec<_> = hashes
        .iter()
        .enumerate()
        .map(|(index, leaf)| hash(b"misaka-palw/step-merkle-leaf/v1", &[&(index as u64).to_le_bytes(), leaf.as_byte_slice()]))
        .collect();
    while level.len() > 1 {
        level = level
            .chunks(2)
            .map(|pair| match pair {
                [left, right] => hash(b"misaka-palw/step-merkle-node/v1", &[left.as_byte_slice(), right.as_byte_slice()]),
                [odd] => *odd,
                _ => unreachable!(),
            })
            .collect();
    }
    level[0]
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StateObservation {
    position: u32,
    state: u16,
    layer: Option<u16>,
    row: Option<u32>,
    value: Tensor,
}

#[derive(Default)]
struct Observed {
    nodes: Vec<ExpectedNode>,
    states: Vec<StateObservation>,
    outputs: Vec<(u32, Tensor, u32)>,
    wide_native_nodes: usize,
    wide_native_outputs: usize,
    stop_at: Option<(u32, u32)>,
    bases: Vec<u32>,
}

impl TirRunObserverV1 for Observed {
    fn node(&mut self, position: u32, value: &NodeValue<'_>) -> Result<(), String> {
        self.nodes.push(ExpectedNode {
            position,
            occurrence: self.bases.iter().rposition(|base| *base <= value.slot).unwrap() as u32,
            slot: value.slot,
            block: value.block,
            layer: value.layer,
            node: value.node,
            commit: value.commit,
            value: value.to_tensor(),
        });
        if value.dtype == DType::I64 && matches!(value.data, Slice::I64(_)) {
            self.wide_native_nodes += 1;
        }
        if self.stop_at == Some((position, value.slot)) {
            return Err("wide-observer-stop".into());
        }
        Ok(())
    }

    fn state(
        &mut self,
        position: u32,
        state: u16,
        layer: Option<u16>,
        row: Option<u32>,
        dtype: DType,
        shape: &[usize],
        data: Slice<'_>,
    ) -> Result<(), String> {
        self.states.push(StateObservation {
            position,
            state,
            layer,
            row,
            value: Tensor { dtype, shape: shape.to_vec(), data: data.to_i128s() },
        });
        Ok(())
    }

    fn last_output(&mut self, position: u32, dtype: DType, shape: &[usize], data: Slice<'_>, selected: u32) -> Result<(), String> {
        if dtype == DType::I64 && matches!(data, Slice::I64(_)) {
            self.wide_native_outputs += 1;
        }
        self.outputs.push((position, Tensor { dtype, shape: shape.to_vec(), data: data.to_i128s() }, selected));
        Ok(())
    }
}

fn expected_states(f: &Fixture<'_>) -> Vec<StateObservation> {
    let mut states = Vec::new();
    for step in &f.steps {
        for (&(state, layer), value) in &step.state.fixed {
            states.push(StateObservation { position: step.position, state, layer, row: None, value: value.clone() });
        }
        for (&(state, layer), rows) in &step.state.hist {
            for (offset, value) in rows.iter().enumerate() {
                states.push(StateObservation {
                    position: step.position,
                    state,
                    layer,
                    row: Some(step.position + 1 - rows.len() as u32 + offset as u32),
                    value: value.clone(),
                });
            }
        }
    }
    states
}

#[test]
fn explicit_wide_run_preserves_original_context_and_exact_extreme_rows_with_canonical_roots() {
    for window in [1, 3] {
        with_fixture(window, |wide, _| {
            let original_class = wide.class.clone();
            let original_job = wide.job.clone();
            let original_program = wide.program.encode();
            assert_eq!(wide.class.tokenizer_id, Hash64::from_u64_word(18));
            assert_eq!(wide.job.tokenizer_id, Hash64::from_bytes([0; 64]));
            assert_eq!(wide.class.layout.max_context, MAX_CONTEXT);
            assert_eq!(wide.job.max_context_tokens, MAX_CONTEXT);
            let space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
            let runner = TirClassRunnerWideQ24V2::new(&space, wide.plan, wide.params, wide.job.shape_profile_id).unwrap();
            let quote = runner.quote(&wide.job, limits()).unwrap();
            assert_eq!(quote.leaf_count, if window == 1 { 35 } else { 42 });
            assert!(quote.node_visits >= 40); // Conservative quote includes skipped post nodes.
            let mut leaves = Vec::new();
            let mut hashes = Vec::new();
            let run = runner
                .run(&wide.job, PROMPT, wide.job.context_hash(), exact_limits(&quote), &mut |leaf| {
                    assert_eq!(leaf.index, leaves.len() as u64);
                    assert_eq!(space.leaf_at(&wide.job, leaf.index).unwrap().position, leaf.position);
                    leaves.push(leaf.preimage.clone());
                    hashes.push(leaf.hash);
                })
                .unwrap();
            assert_eq!(run.logits_rows(), wide.rows);
            assert_eq!(run.generated(), EXPECTED_GENERATED);
            assert!(run.logits_rows().iter().flatten().any(|value| *value == i64::MIN));
            assert!(run.logits_rows().iter().flatten().any(|value| *value == i64::MAX));
            assert!(run.logits_rows().iter().flatten().any(|value| *value == 1082i64 << 24));
            assert!(run.logits_rows()[1].iter().all(|value| *value < 0));
            assert_eq!(run.logits_rows()[0][2], run.logits_rows()[0][3]);
            assert_eq!(run.generated()[0], 2); // First maximum, not the last equal one.
            assert_eq!(run.generated()[1], 0); // Negative row tie also chooses its first lane.
            let mut builder = PalwTirStepLegBuilderV2::new(&space, &wide.job, wide.job.shape_profile_id, quote.leaf_count).unwrap();
            let mut independent_hashes = Vec::new();
            for (index, preimage) in leaves.iter().enumerate() {
                let leaf = space.leaf_at(&wide.job, index as u64).unwrap();
                let values = leaf_values(wide, &leaf);
                assert_eq!(builder.push(&values).unwrap(), leaf);
                assert_eq!(*preimage, palw_tir_leaf_preimage_v2(&space, &wide.job, &leaf, &values).unwrap());
                assert_eq!(palw_tir_leaf_values_v2(&space, &wide.job, &leaf, preimage).unwrap(), values);
                let final_logits = matches!(leaf.kind, PalwTirLeafKindV1::Commit { block, node, .. }
                    if block == wide.program.schedule.post && node == wide.program.logits);
                let manual_bytes: Vec<u8> = if final_logits {
                    assert_eq!(preimage.version, 2);
                    values.iter().flat_map(|v| i64::try_from(*v).unwrap().to_le_bytes()).collect()
                } else {
                    assert_eq!(preimage.version, 1);
                    values.iter().flat_map(|v| i32::try_from(*v).unwrap().to_le_bytes()).collect()
                };
                assert_eq!(preimage.values_le, manual_bytes);
                independent_hashes.push(manual_leaf_hash(wide.job.context_hash(), wide.job.shape_profile_id, preimage));
            }
            let (count, canonical_root) = builder.finish().unwrap();
            assert_eq!(run.leaf_count(), count);
            assert_eq!(run.leaf_hashes(), hashes);
            assert_eq!(hashes, independent_hashes);
            assert_eq!(run.step_merkle_root(), manual_tree(&independent_hashes));
            assert_eq!(run.step_merkle_root(), canonical_root);
            assert_eq!(run.trace_root(), palw_logits_q24_v2::trace_root(&wide.job, &wide.rows, EXPECTED_GENERATED).unwrap());
            assert_eq!(run.output_root(), palw_attempt_output_root_v1(&wide.job, EXPECTED_GENERATED));
            assert_eq!(
                run.execution_root(),
                palw_tir_execution_root_v1(
                    &wide.job.context_hash(),
                    &run.trace_root(),
                    &wide.job.shape_profile_id,
                    count,
                    &canonical_root
                )
            );
            assert_eq!(run.reserved_bytes(), quote.requested_bytes);
            let binding = PalwTirStepBindingV1 {
                version: 1,
                job_context: wide.job.clone(),
                class: wide.class.clone(),
                artifact_root: wide.artifact,
                full_logits_trace_root: run.trace_root(),
                step_leaf_count: count,
                step_merkle_root: canonical_root,
                committed_execution_root: run.execution_root(),
            };
            verify_tir_binding_v2(&binding, count).unwrap();
            assert!(verify_tir_binding_v1(&binding, count).is_err());
            assert_eq!(wide.class, original_class);
            assert_eq!(wide.job, original_job);
            assert_eq!(wide.program.encode(), original_program);
        });
    }
}

#[test]
fn paired_legacy_nodes_fixed_and_history_keep_exact_four_byte_preimages() {
    for window in [1, 3] {
        with_fixture(window, |wide, narrow| {
            let wide_space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
            let narrow_space = PalwTirStepSpaceV1::new(&narrow.class).unwrap();
            let wide_runner = TirClassRunnerWideQ24V2::new(&wide_space, wide.plan, wide.params, wide.job.shape_profile_id).unwrap();
            let narrow_runner = TirClassRunnerV1::new(&narrow_space, narrow.plan, narrow.params, narrow.job.shape_profile_id).unwrap();
            let mut wide_leaves = Vec::new();
            let mut narrow_leaves = Vec::new();
            let wide_run = wide_runner
                .run(&wide.job, PROMPT, wide.job.context_hash(), limits(), &mut |leaf| wide_leaves.push(leaf.preimage.clone()))
                .unwrap();
            let narrow_run =
                narrow_runner.run(&narrow.job, PROMPT, LEAF_CAP, true, &mut |leaf| narrow_leaves.push(leaf.preimage.clone())).unwrap();
            assert_eq!(wide_run.generated(), narrow_run.generated);
            assert_eq!(narrow_run.generated, EXPECTED_GENERATED);
            assert_eq!(wide_leaves.len(), narrow_leaves.len());
            let (mut fixed, mut history, mut final_logits) = (0, 0, 0);
            for (index, (wide_preimage, narrow_preimage)) in wide_leaves.iter().zip(&narrow_leaves).enumerate() {
                let leaf = wide_space.leaf_at(&wide.job, index as u64).unwrap();
                let legacy_leaf = narrow_space.leaf_at(&narrow.job, index as u64).unwrap();
                assert_eq!(leaf.coord, legacy_leaf.coord);
                assert_eq!(leaf.value_count, legacy_leaf.value_count);
                match leaf.kind {
                    PalwTirLeafKindV1::Commit { block, node, .. }
                        if block == wide.program.schedule.post && node == wide.program.logits =>
                    {
                        final_logits += 1;
                        assert_eq!(wide_preimage.version, 2);
                        assert_eq!(narrow_preimage.version, 1);
                        assert_eq!(wide_preimage.values_le.len(), narrow_preimage.values_le.len() * 2);
                    }
                    kind => {
                        assert_eq!(wide_preimage, narrow_preimage);
                        assert_eq!(*wide_preimage, palw_tir_leaf_preimage_v1(&leaf, &leaf_values(wide, &leaf)).unwrap());
                        assert_eq!(wide_preimage.values_le.len(), leaf.value_count as usize * 4);
                        fixed += usize::from(matches!(kind, PalwTirLeafKindV1::State { .. }));
                        history += usize::from(matches!(kind, PalwTirLeafKindV1::HistTile { .. }));
                    }
                }
            }
            assert_eq!(fixed, 2);
            assert_eq!(history, if window == 1 { 7 } else { 3 });
            assert_eq!(final_logits, EXPECTED_GENERATED.len());
            // Context/class identities differ between these two declarations;
            // no old hash or registered identity is reinterpreted as wide.
            assert_ne!(wide.job.context_hash(), narrow.job.context_hash());
            assert_ne!(wide_run.step_merkle_root(), narrow_run.step_merkle_root);
            assert_ne!(wide_run.trace_root(), narrow_run.trace_root);
        });
    }
}

#[test]
fn borrowed_wide_observer_reports_each_executed_node_state_and_output_once() {
    for window in [1, 3] {
        with_fixture(window, |wide, _| {
            let space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
            let runner = TirClassRunnerWideQ24V2::new(&space, wide.plan, wide.params, wide.job.shape_profile_id).unwrap();
            let mut plain_leaves = Vec::new();
            let plain = runner
                .run(&wide.job, PROMPT, wide.job.context_hash(), limits(), &mut |leaf| plain_leaves.push(leaf.preimage.clone()))
                .unwrap();
            let mut observer = Observed { bases: wide.program.occurrence_slot_bases(), ..Default::default() };
            let mut observed_leaves = Vec::new();
            let observed = runner
                .run_observed(
                    &wide.job,
                    PROMPT,
                    wide.job.context_hash(),
                    limits(),
                    &mut |leaf| observed_leaves.push(leaf.preimage.clone()),
                    &mut observer,
                )
                .unwrap();
            assert_eq!(observed.logits_rows(), plain.logits_rows());
            assert_eq!(observed.generated(), plain.generated());
            assert_eq!(observed.leaf_hashes(), plain.leaf_hashes());
            assert_eq!(observed.step_merkle_root(), plain.step_merkle_root());
            assert_eq!(observed.trace_root(), plain.trace_root());
            assert_eq!(observed.execution_root(), plain.execution_root());
            assert_eq!(observed.output_root(), plain.output_root());
            assert_eq!(observed_leaves, plain_leaves);
            assert_eq!(observer.nodes, wide.nodes);
            assert_eq!(observer.nodes.len(), 40);
            let unique: BTreeSet<_> = observer.nodes.iter().map(|node| (node.position, node.occurrence, node.node)).collect();
            assert_eq!(unique.len(), observer.nodes.len());
            assert_eq!(observer.states, expected_states(wide));
            assert_eq!(observer.states.len(), if window == 1 { 7 } else { 20 });
            let outputs: Vec<_> = wide
                .steps
                .iter()
                .filter_map(|step| step.output.as_ref().map(|(value, selected)| (step.position, value.clone(), *selected)))
                .collect();
            assert_eq!(observer.outputs, outputs);
            assert_eq!(observer.wide_native_nodes, EXPECTED_GENERATED.len());
            assert_eq!(observer.wide_native_outputs, EXPECTED_GENERATED.len());
        });
    }
}

#[test]
fn observer_refusal_stops_before_its_leaf_and_any_later_state_or_output() {
    for window in [1, 3] {
        with_fixture(window, |wide, _| {
            let space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
            let runner = TirClassRunnerWideQ24V2::new(&space, wide.plan, wide.params, wide.job.shape_profile_id).unwrap();
            for stop in [(0, 0), (1, 5)] {
                let mut observer = Observed { bases: wide.program.occurrence_slot_bases(), stop_at: Some(stop), ..Default::default() };
                let mut leaves = Vec::new();
                let result = runner.run_observed(
                    &wide.job,
                    PROMPT,
                    wide.job.context_hash(),
                    limits(),
                    &mut |leaf| leaves.push(leaf.preimage.clone()),
                    &mut observer,
                );
                assert!(result.err().unwrap().contains("wide-observer-stop"));
                let expected_end = wide.nodes.iter().position(|n| (n.position, n.slot) == stop).unwrap() + 1;
                assert_eq!(observer.nodes, wide.nodes[..expected_end]);
                assert!(observer.states.iter().all(|state| state.position < stop.0));
                assert!(observer.outputs.is_empty());
                if stop.0 == 0 {
                    assert_eq!((observer.nodes.len(), observer.states.len(), leaves.len()), (1, 0, 0));
                } else {
                    assert_eq!(observer.nodes.len(), 10);
                    assert_eq!(observer.wide_native_nodes, 1);
                    let failed =
                        space.leaves_of_position(&wide.job, stop.0).into_iter().find(|leaf| leaf.coord.node_slot == stop.1).unwrap();
                    assert_eq!(leaves.len() as u64, failed.index);
                    assert_eq!(
                        observer.states,
                        expected_states(wide).into_iter().filter(|state| state.position == 0).collect::<Vec<_>>()
                    );
                }
            }
        });
    }
}

#[test]
fn exact_quote_limits_fit_and_each_one_below_refuses_before_callbacks() {
    with_fixture(3, |wide, _| {
        let space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, wide.plan, wide.params, wide.job.shape_profile_id).unwrap();
        let quote = runner.quote(&wide.job, limits()).unwrap();
        assert!(quote.executor_bytes > 0);
        assert!(quote.retained_bytes > 0);
        assert!(quote.scratch_bytes > 0);
        assert!(quote.work > 0);
        let run = runner.run(&wide.job, PROMPT, wide.job.context_hash(), exact_limits(&quote), &mut |_| {}).unwrap();
        assert_eq!(run.reserved_bytes(), quote.requested_bytes);
        for bound in 0..5 {
            let mut cap = exact_limits(&quote);
            match bound {
                0 => cap.max_positions -= 1,
                1 => cap.max_step_leaves -= 1,
                2 => cap.max_row_lanes -= 1,
                3 => cap.max_work -= 1,
                _ => cap.max_requested_bytes -= 1,
            }
            let mut observer = Observed { bases: wide.program.occurrence_slot_bases(), ..Default::default() };
            let mut leaves = 0;
            assert!(
                runner.run_observed(&wide.job, PROMPT, wide.job.context_hash(), cap, &mut |_| leaves += 1, &mut observer).is_err(),
                "accepted one-below bound {bound}"
            );
            assert_eq!(leaves, 0);
            assert!(observer.nodes.is_empty() && observer.states.is_empty() && observer.outputs.is_empty());
        }
    });
}

#[test]
fn malformed_mixed_profiles_and_unpinned_inputs_fail_closed_without_execution() {
    with_fixture(3, |wide, narrow| {
        assert!(PalwTirStepSpaceV1::new(&wide.class).is_err());
        assert!(PalwTirStepSpaceV2::new(&narrow.class).is_err());
        // The old space exposes its decoded program as a mutable DTO. A caller
        // corrupting that DTO must still hit the runner's own profile guard.
        let mut forged_legacy_space = PalwTirStepSpaceV1::new(&narrow.class).unwrap();
        forged_legacy_space.program = wide.program.clone();
        assert!(TirClassRunnerV1::new(&forged_legacy_space, wide.plan, wide.params, wide.job.shape_profile_id).is_err());
        let space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
        assert!(TirClassRunnerWideQ24V2::new(&space, narrow.plan, narrow.params, wide.job.shape_profile_id).is_err());
        let runner = TirClassRunnerWideQ24V2::new(&space, wide.plan, wide.params, wide.job.shape_profile_id).unwrap();
        for mutation in 0..7 {
            let mut program = wide.program.clone();
            let post = program.schedule.post as usize;
            let logits = program.logits as usize;
            match mutation {
                0 => program.logits_scheme_id = tiled_logits_scheme_id_v1().as_bytes(),
                1 => program.blocks[post].nodes[logits].commit = false,
                2 => {
                    program.params[1].dtype = DType::I128;
                    program.blocks[post].nodes[logits].out.dtype = DType::I128;
                }
                3 => program.blocks[post].nodes[0].out.dtype = DType::I64,
                4 => program.states[0].dtype = DType::I64,
                5 => {
                    let mut after_final = program.blocks[post].nodes[0].clone();
                    after_final.commit = false;
                    program.blocks[post].nodes.push(after_final);
                }
                _ => program.logits_scheme_id = [0; 64],
            }
            let mut class = wide.class.clone();
            class.program = program.encode();
            assert!(PalwTirStepSpaceV2::new(&class).is_err(), "accepted malformed profile {mutation}");
        }
        for mutation in 0..11 {
            let mut job = wide.job.clone();
            let mut prompt = PROMPT.to_vec();
            let mut expected_execution = job.context_hash();
            match mutation {
                0 => expected_execution = Hash64::from_u64_word(999),
                1 => {
                    prompt.swap(0, 1); // Same length, distinct committed prompt.
                }
                2 => {
                    prompt[1] = 4;
                    job.prompt_token_ids_hash = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &prompt).unwrap();
                    expected_execution = job.context_hash();
                }
                3 => job.shape_profile_id = Hash64::from_u64_word(998),
                4 => job.trace_scheme_id = tiled_logits_scheme_id_v1(),
                5 => job.version = 3,
                6 => job.max_context_tokens = 7,
                7 => job.network_id.clear(),
                8 => prompt.pop().map(|_| ()).unwrap(),
                9 => job.exact_decode_tokens = 0,
                _ => job.prompt_token_ids_hash = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &prompt).unwrap(),
            }
            if mutation >= 3 {
                // Let independent shape/profile checks see the malformed context;
                // do not mask them with a stale execution pin.
                expected_execution = job.context_hash();
            }
            let mut observer = Observed { bases: wide.program.occurrence_slot_bases(), ..Default::default() };
            let mut leaves = 0;
            assert!(
                runner.run_observed(&job, &prompt, expected_execution, limits(), &mut |_| leaves += 1, &mut observer).is_err(),
                "accepted malformed job/input {mutation}"
            );
            assert_eq!(leaves, 0);
            assert!(observer.nodes.is_empty() && observer.states.is_empty() && observer.outputs.is_empty());
        }
        assert_eq!(wide.job.tokenizer_id, Hash64::from_bytes([0; 64]));
        assert_eq!(wide.class.tokenizer_id, Hash64::from_u64_word(18));
        assert_eq!(wide.class.layout.max_context, MAX_CONTEXT);
    });
}

#[test]
fn legacy_v1_cpu_process_and_ipc_entry_points_refuse_wide_before_execution_or_serialization() {
    use misaka_palw_tir_exec::{
        CpuCellStepperV1, CpuDeviceV1, StepSink, TirCellStepperV1, TirDeviceV1, TirExecutor, TirParams, TirPlan, TirProcessDeviceV1,
        cellproc::{CellParamBlobV1, CellRequestV1, CellResponseV1},
        serve_cell_requests_v1,
    };
    use std::{
        io::{Cursor, Read, Write},
        ops::Range,
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    #[derive(Default)]
    struct CountSink(usize);
    impl StepSink for CountSink {
        fn every_node(&self) -> bool {
            true
        }
        fn node(&mut self, _: &NodeValue<'_>) {
            self.0 += 1;
        }
    }

    fn framed(payload: &[u8]) -> Vec<u8> {
        let mut bytes = (payload.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    struct AuditWriter(Arc<Mutex<Vec<u8>>>);
    impl Write for AuditWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct NeverOpened(Arc<AtomicUsize>);
    impl TirDeviceV1 for NeverOpened {
        fn name(&self) -> String {
            "wide-ipc-tripwire".into()
        }
        fn capacity_bytes(&self) -> Option<u64> {
            None
        }
        fn cell_stepper<'a>(
            &'a self,
            _: &'a TirPlan,
            _: &'a TirParams<'a>,
            _: Range<usize>,
        ) -> Result<Box<dyn TirCellStepperV1 + 'a>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err("server must refuse wide before reaching the device".into())
        }
    }

    with_fixture(3, |wide, narrow| {
        let cpu = CpuDeviceV1;
        let occurrences = wide.program.occurrences().len();
        let refusal = cpu.cell_stepper(wide.plan, wide.params, 0..occurrences).err().unwrap();
        assert!(refusal.contains("V1 CPU device refuses wide Q24"));

        // Even a manually constructed old wrapper cannot send an exact i64
        // execution through the infallible i32 cell interface.
        let mut raw = CpuCellStepperV1(TirExecutor::new_cell(wide.plan, wide.params, 0..occurrences).unwrap());
        let original_state = raw.0.export_state();
        let mut sink = CountSink::default();
        let refusal = raw.step_cell(PROMPT[0], 0..occurrences, &[], &mut sink).unwrap_err();
        assert!(refusal.to_string().contains("V1 cell stepper refuses wide Q24"));
        assert_eq!(sink.0, 0);
        assert_eq!(raw.0.pos(), 0);
        assert_eq!(raw.0.export_state(), original_state);
        let panic = catch_unwind(AssertUnwindSafe(|| raw.logits_lanes())).unwrap_err();
        let reason = panic.downcast_ref::<String>().map(String::as_str).or_else(|| panic.downcast_ref::<&str>().copied()).unwrap();
        assert!(reason.contains("V1 cell logits cannot read wide Q24"));
        assert_eq!(sink.0, 0);
        assert_eq!(raw.0.export_state(), original_state);

        // The real process-device client performs its existing Hello1 exchange,
        // then must reject locally without serializing an Open or any params.
        let ready = CellResponseV1::Ready { version: 1, name: "bounded-test-pipe".into(), capacity: None };
        let sent = Arc::new(Mutex::new(Vec::new()));
        let process =
            TirProcessDeviceV1::connect(Box::new(Cursor::new(framed(&ready.encode()))), Box::new(AuditWriter(sent.clone())), false)
                .unwrap();
        let hello = sent.lock().unwrap().clone();
        assert_eq!(hello, framed(&CellRequestV1::Hello { version: 1 }.encode()));
        let refusal = process.cell_stepper(wide.plan, wide.params, 0..occurrences).err().unwrap();
        assert!(refusal.contains("V1 process device refuses wide Q24 before serialization"));
        assert_eq!(*sent.lock().unwrap(), hello);
        assert!(!process.poisoned());

        // Exercise the actual server framing/dispatcher with Hello1 and a raw
        // Open. The deliberately invalid param would fail a loader; the explicit
        // wide diagnostic must win first, and the device must never be invoked.
        let mut requests = framed(&CellRequestV1::Hello { version: 1 }.encode());
        requests.extend(framed(
            &CellRequestV1::Open {
                program: wide.program.encode(),
                occ: (0, occurrences as u32),
                params: vec![CellParamBlobV1 { param: u16::MAX, layer: None, bytes: vec![0] }],
            }
            .encode(),
        ));
        let device_calls = Arc::new(AtomicUsize::new(0));
        let mut output = Vec::new();
        serve_cell_requests_v1(&mut Cursor::new(requests), &mut output, &NeverOpened(device_calls.clone())).unwrap();
        assert_eq!(device_calls.load(Ordering::SeqCst), 0);
        let mut cursor = Cursor::new(output.as_slice());
        let mut replies = Vec::new();
        while cursor.position() < output.len() as u64 {
            let mut length = [0u8; 8];
            cursor.read_exact(&mut length).unwrap();
            let length = usize::try_from(u64::from_le_bytes(length)).unwrap();
            assert!(length <= output.len());
            let mut body = vec![0; length];
            cursor.read_exact(&mut body).unwrap();
            replies.push(CellResponseV1::decode(&body).unwrap());
        }
        assert_eq!(replies.len(), 2);
        assert!(matches!(replies[0], CellResponseV1::Ready { version: 1, .. }));
        assert!(matches!(&replies[1], CellResponseV1::Refused(reason)
            if reason.contains("V1 cell IPC refuses wide Q24")));
        assert!(!replies.iter().any(|reply| matches!(reply, CellResponseV1::Opened | CellResponseV1::Stepped { .. })));

        // The same V1 CPU seam still executes its original narrow declaration.
        let mut legacy = cpu.cell_stepper(narrow.plan, narrow.params, 0..occurrences).unwrap();
        let mut legacy_sink = CountSink::default();
        legacy.step_cell(PROMPT[1], 0..occurrences, &[], &mut legacy_sink).unwrap();
        assert_eq!(legacy_sink.0, 6);
        let expected: Vec<i32> = narrow.rows[0].iter().map(|value| i32::try_from(*value).unwrap()).collect();
        assert_eq!(legacy.logits_lanes(), expected);
    });
}
