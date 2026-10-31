#![cfg(feature = "positive-panel")]
//! Source-only full typed capture on an ORIGINAL sparse TIR registration.
//! Native wide values are compared with the separate reference evaluator;
//! neither local capture equality nor old roots grants computation credit.
mod positive_typed_capture_common;
use misaka_palw_tir::{DType, Prim, Tensor};
use misaka_palw_tir_exec::{
    Buf, NodeValue, Slice,
    node::{
        positive_typed_capture::{
            TypedCaptureError, TypedCaptureLimits, TypedRecordKind, capture_typed_prototype, quote_typed_prototype,
        },
        run::TirRunObserverV1,
    },
};
use positive_typed_capture_common::*;

fn limits() -> TypedCaptureLimits {
    TypedCaptureLimits {
        max_positions: 16,
        max_geometry_visits: 1 << 20,
        max_records: 1 << 20,
        max_lanes: 1 << 22,
        max_payload_bytes: 1 << 26,
        max_peak_capture_heap_bytes: 1 << 28,
        max_local_storage_bytes: 1 << 28,
        max_legacy_leaves: LEAF_CAP,
    }
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
    bases: Vec<u32>,
}
impl TirRunObserverV1 for Observed {
    fn node(&mut self, position: u32, value: &NodeValue<'_>) -> Result<(), String> {
        let occurrence = self.bases.iter().rposition(|base| *base <= value.slot).unwrap() as u32;
        self.nodes.push(ExpectedNode {
            position,
            occurrence,
            slot: value.slot,
            block: value.block,
            layer: value.layer,
            node: value.node,
            commit: value.commit,
            value: value.to_tensor(),
        });
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
        self.outputs.push((position, Tensor { dtype, shape: shape.to_vec(), data: data.to_i128s() }, selected));
        Ok(())
    }
}
fn expected_states(f: &Fixture) -> Vec<StateObservation> {
    let mut states = Vec::new();
    for step in &f.steps {
        for (&(state, layer), value) in &step.state.fixed {
            states.push(StateObservation { position: step.position, state, layer, row: None, value: value.clone() });
        }
        for (&(state, layer), rows) in &step.state.hist {
            for (offset, value) in rows.iter().enumerate() {
                let row = step.position + 1 - rows.len() as u32 + offset as u32;
                states.push(StateObservation { position: step.position, state, layer, row: Some(row), value: value.clone() });
            }
        }
    }
    states
}
type ExpectedRecord = (TypedRecordKind, Tensor);
fn expected_records(f: &Fixture) -> Vec<ExpectedRecord> {
    let mut entries = Vec::new();
    for step in &f.steps {
        for value in f.nodes.iter().filter(|value| value.position == step.position) {
            entries.push((
                TypedRecordKind::Node {
                    position: value.position,
                    occurrence: value.occurrence as u16,
                    slot: value.slot,
                    block: value.block,
                    layer: value.layer,
                    node: value.node,
                    committed: value.commit,
                },
                value.value.clone(),
            ));
            if let Prim::HistAppend { state } = f.program.blocks[value.block as usize].nodes[value.node as usize].prim {
                let decl = &f.program.states[state as usize];
                let row_shape = decl.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
                let lanes = row_shape.iter().product::<usize>();
                let data = value.value.data[value.value.data.len() - lanes..].to_vec();
                entries.push((
                    TypedRecordKind::HistoryAppendRow { position: step.position, state, layer: value.layer },
                    Tensor { dtype: decl.dtype, shape: row_shape, data },
                ));
            }
        }
        for (&(state, layer), value) in &step.state.fixed {
            entries.push((TypedRecordKind::FixedAfter { position: step.position, state, layer }, value.clone()));
        }
        if let Some((output, selected)) = &step.output {
            entries.push((TypedRecordKind::LastOutput { position: step.position, selected: *selected }, output.clone()));
        }
    }
    entries
}

#[test]
fn borrowed_observation_matches_reference_wide_nodes_states_and_original_step_roots() {
    for window in [1, 3] {
        with_window_fixture(window, |f, runner, _| {
            let mut original_leaves = Vec::new();
            let original =
                runner.run(&f.job, PROMPT, LEAF_CAP, true, &mut |leaf| original_leaves.push(leaf.preimage.clone())).unwrap();
            let mut observer = Observed { bases: f.program.occurrence_slot_bases(), ..Default::default() };
            let mut observed_leaves = Vec::new();
            let observed = runner
                .run_observed(&f.job, PROMPT, LEAF_CAP, true, &mut |leaf| observed_leaves.push(leaf.preimage.clone()), &mut observer)
                .unwrap();
            assert_eq!(observed, original);
            assert_eq!(observed_leaves, original_leaves);
            assert_eq!(observer.nodes, f.nodes);
            assert_eq!(observer.states, expected_states(f));
            let outputs = f
                .steps
                .iter()
                .filter_map(|s| s.output.as_ref().map(|(tensor, selected)| (s.position, tensor.clone(), *selected)))
                .collect::<Vec<_>>();
            assert_eq!(observer.outputs, outputs);
            assert_eq!(observed.generated, EXPECTED_GENERATED);
            assert!(
                observer
                    .nodes
                    .iter()
                    .any(|n| n.value.dtype == DType::I64 && n.value.data.iter().any(|v| v.unsigned_abs() > i32::MAX as u128))
            );
            assert!(
                observer
                    .nodes
                    .iter()
                    .any(|n| n.value.dtype == DType::I128 && n.value.data.iter().any(|v| v.unsigned_abs() > i64::MAX as u128))
            );
            assert_eq!(runner.plan.program.encode(), f.encoded_program);
            assert_eq!(f.class.program, f.encoded_program);
            assert!(f.program.blocks.iter().flat_map(|b| &b.nodes).any(|n| !n.commit));
            if window == 1 {
                assert!(observer.states.iter().all(|s| s.row.is_none()));
            }
        });
    }
}

#[test]
fn typed_prototype_preserves_sparse_registration_and_all_native_payloads_including_window_one() {
    for window in [1, 3] {
        with_window_fixture(window, |f, runner, _| {
            let original = runner.run(&f.job, PROMPT, LEAF_CAP, true, &mut |_| {}).unwrap();
            let quote = quote_typed_prototype(runner, &f.class, f.artifact, &f.job, f.job.context_hash(), limits()).unwrap();
            let capture =
                capture_typed_prototype(runner, &f.class, f.artifact, &f.job, f.job.context_hash(), PROMPT, limits()).unwrap();
            let expected = expected_records(f);
            assert_eq!(capture.run(), &original);
            assert_eq!(capture.entries().len(), expected.len());
            for (entry, (kind, value)) in capture.entries().iter().zip(&expected) {
                assert_eq!(&entry.kind, kind);
                assert_eq!(entry.dtype, value.dtype);
                assert_eq!(entry.values.dtype(), value.dtype);
                assert_eq!(entry.shape, value.shape);
                assert_eq!(entry.values.to_i128s(), value.data);
            }
            let summary = capture.summary();
            eprintln!(
                "typed-capture window={window}: positions={} records={} nodes={} fixed={} appended_rows={} outputs={} lanes={} payload_bytes={} input_auth_heap_bytes={} peak_capture_heap_bytes={} local_storage_bytes={} geometry_visits={}",
                summary.positions,
                summary.records,
                summary.node_records,
                summary.fixed_records,
                summary.history_records,
                summary.output_records,
                summary.lanes,
                summary.payload_bytes,
                summary.input_auth_heap_bytes,
                summary.peak_capture_heap_bytes,
                summary.local_storage_bytes,
                summary.geometry_visits
            );
            assert_eq!(summary.positions, f.steps.len() as u32);
            assert_eq!(summary.records, expected.len() as u64);
            assert_eq!(summary.node_records, f.nodes.len() as u64);
            assert_eq!(summary.fixed_records, f.steps.iter().map(|s| s.state.fixed.len() as u64).sum::<u64>());
            assert_eq!(summary.history_records, 12); // 3 appended instances at each of 4 positions, even window 1.
            assert_eq!(summary.output_records, 3);
            assert_eq!(summary.lanes, expected.iter().map(|(_, v)| v.data.len() as u64).sum::<u64>());
            assert_eq!(summary.payload_bytes, expected.iter().map(|(_, v)| v.data.len() as u64 * v.dtype.width() as u64).sum::<u64>());
            assert_eq!(summary.records, quote.records);
            assert_eq!(summary.lanes, quote.lanes);
            assert_eq!(summary.payload_bytes, quote.payload_bytes);
            assert_eq!(summary.peak_capture_heap_bytes, quote.peak_capture_heap_bytes);
            assert_eq!(summary.local_storage_bytes, quote.local_storage_bytes);
            assert_eq!(summary.geometry_visits, quote.geometry_visits);
            capture.validate_local_copy(capture.origin(), capture.entries(), EXPECTED_GENERATED).unwrap();
            assert_eq!(runner.plan.program.encode(), f.encoded_program);
            assert_eq!(f.class.class_id(&f.artifact), f.job.shape_profile_id);
        });
    }
}

#[test]
fn copied_capture_omission_reorder_coordinate_and_type_truncation_are_rejected() {
    with_fixture(|f, runner, _| {
        let capture = capture_typed_prototype(runner, &f.class, f.artifact, &f.job, f.job.context_hash(), PROMPT, limits()).unwrap();
        let original = capture.entries();
        let wide = original
            .iter()
            .position(|r| r.dtype == DType::I64 && r.values.to_i128s().iter().any(|v| v.unsigned_abs() > i32::MAX as u128))
            .unwrap();
        let wider = original
            .iter()
            .position(|r| r.dtype == DType::I128 && r.values.to_i128s().iter().any(|v| v.unsigned_abs() > i64::MAX as u128))
            .unwrap();
        for attack in 0..8 {
            let mut altered = original.to_vec();
            match attack {
                0 => {
                    altered.remove(wide);
                }
                1 => altered.swap(0, 1),
                2 => {
                    if let TypedRecordKind::Node { occurrence, .. } = &mut altered[wide].kind {
                        *occurrence += 1;
                    } else {
                        panic!("wide record must be a node");
                    }
                }
                3 => {
                    let v = altered[wide].values.to_i128s().iter().map(|v| *v as i32).collect();
                    altered[wide].values = Buf::I32(v);
                }
                4 => {
                    let v = altered[wider].values.to_i128s().iter().map(|v| *v as i64).collect();
                    altered[wider].values = Buf::I64(v);
                    altered[wider].dtype = DType::I64;
                }
                5 => altered[wide].shape.insert(0, 1),
                6 => {
                    let mut data = altered[wide].values.to_i128s();
                    data[0] ^= 1;
                    altered[wide].values = Buf::from_i128s(altered[wide].dtype, &data);
                }
                _ => {
                    let record = altered.iter_mut().find(|r| matches!(r.kind, TypedRecordKind::LastOutput { .. })).unwrap();
                    if let TypedRecordKind::LastOutput { selected, .. } = &mut record.kind {
                        *selected = (*selected + 1) % 4;
                    }
                }
            }
            assert!(capture.validate_local_copy(capture.origin(), &altered, EXPECTED_GENERATED).is_err());
            capture.validate_local_copy(capture.origin(), original, EXPECTED_GENERATED).unwrap();
        }
        let mut wrong_generated = EXPECTED_GENERATED.to_vec();
        wrong_generated[0] = 3;
        assert!(capture.validate_local_copy(capture.origin(), original, &wrong_generated).is_err());
        let mut other_job = f.job.clone();
        other_job.job_id = kaspa_consensus_core::Hash64::from_u64_word(999);
        let other =
            capture_typed_prototype(runner, &f.class, f.artifact, &other_job, other_job.context_hash(), PROMPT, limits()).unwrap();
        assert!(capture.validate_local_copy(other.origin(), original, EXPECTED_GENERATED).is_err());
    });
}

#[test]
fn independent_quote_refuses_each_capture_dimension_below_required_bound() {
    with_fixture(|f, runner, _| {
        let quote = quote_typed_prototype(runner, &f.class, f.artifact, &f.job, f.job.context_hash(), limits()).unwrap();
        let original = runner.run(&f.job, PROMPT, LEAF_CAP, true, &mut |_| {}).unwrap();
        for dimension in 0..8 {
            let mut cap = limits();
            match dimension {
                0 => cap.max_positions = quote.positions - 1,
                1 => cap.max_geometry_visits = quote.geometry_visits - 1,
                2 => cap.max_records = quote.records - 1,
                3 => cap.max_lanes = quote.lanes - 1,
                4 => cap.max_payload_bytes = quote.payload_bytes - 1,
                5 => cap.max_peak_capture_heap_bytes = quote.peak_capture_heap_bytes - 1,
                6 => cap.max_local_storage_bytes = quote.local_storage_bytes - 1,
                _ => cap.max_legacy_leaves = original.leaf_count - 1,
            }
            assert_eq!(
                quote_typed_prototype(runner, &f.class, f.artifact, &f.job, f.job.context_hash(), cap).err(),
                Some(TypedCaptureError::Budget)
            );
            assert_eq!(
                capture_typed_prototype(runner, &f.class, f.artifact, &f.job, f.job.context_hash(), PROMPT, cap).err(),
                Some(TypedCaptureError::Budget)
            );
        }
        let mut cap = limits();
        cap.max_peak_capture_heap_bytes = quote.input_auth_heap_bytes - 1;
        let mut foreign = f.class.clone();
        foreign.program[0] ^= 1;
        // Input-authentication scratch is refused before even a mismatching
        // program is re-encoded/hashed, not after its identity comparison.
        assert_eq!(
            quote_typed_prototype(runner, &foreign, f.artifact, &f.job, f.job.context_hash(), cap).err(),
            Some(TypedCaptureError::Budget)
        );
    });
}

#[test]
fn observed_late_node_failure_rolls_back_pending_fixed_and_history_effects() {
    use misaka_palw_tir::{TirError, TirErrorKind, TirResult};
    use misaka_palw_tir_exec::{StepSink, TirExecutor};
    struct StopAt(u32);
    impl StepSink for StopAt {
        fn every_node(&self) -> bool {
            true
        }
        fn node(&mut self, _: &NodeValue<'_>) {}
        fn try_node(&mut self, value: &NodeValue<'_>) -> TirResult<()> {
            if value.slot == self.0 { Err(TirError::new(TirErrorKind::Operand, "late-source-capture-stop")) } else { Ok(()) }
        }
    }
    with_fixture(|_, runner, _| {
        let pre = runner.plan.program.schedule.pre as usize;
        let node = runner.plan.program.blocks[pre].nodes.iter().position(|n| matches!(n.prim, Prim::StateWrite { .. })).unwrap();
        let mut exec = TirExecutor::new(runner.plan, runner.params).unwrap();
        let before = exec.export_state();
        assert!(exec.step(PROMPT[0], &mut StopAt(runner.plan.slot_bases[0] + node as u32)).is_err());
        assert_eq!(exec.export_state(), before);
    });
}

#[test]
fn panel_source_replay_uses_ordinary_settings_without_inherited_selector_or_drill() {
    use misaka_palw_tir_exec::node::{TirBoundaryLieV1, TirClassRunnerV1};
    with_fixture(|f, runner, _| {
        let altered = TirClassRunnerV1::new(runner.space, runner.plan, runner.params, runner.class_id)
            .unwrap()
            .with_selector(std::sync::Arc::new(|_, _, _| 3))
            .with_boundary_lie(Some(TirBoundaryLieV1 { position: 0, boundary: 1 }));
        let original = runner.run(&f.job, PROMPT, LEAF_CAP, true, &mut |_| {}).unwrap();
        let capture = capture_typed_prototype(&altered, &f.class, f.artifact, &f.job, f.job.context_hash(), PROMPT, limits()).unwrap();
        assert_eq!(capture.run(), &original);
        assert_eq!(capture.run().generated, EXPECTED_GENERATED);
    });
}

#[test]
fn original_context_and_flat_prompt_binding_refuse_corrupted_source_metadata() {
    with_fixture(|f, runner, _| {
        for corruption in 0..5 {
            let mut job = f.job.clone();
            match corruption {
                0 => job.network_id = vec![0; kaspa_consensus_core::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES + 1],
                1 => job.tokenizer_id = kaspa_consensus_core::Hash64::from_u64_word(999),
                2 => job.version = 9,
                3 => job.trace_scheme_id = kaspa_consensus_core::Hash64::from_u64_word(999),
                _ => job.max_context_tokens = 4, // P+D=5; equality with last position count is insufficient.
            }
            assert!(quote_typed_prototype(runner, &f.class, f.artifact, &job, f.job.context_hash(), limits()).is_err());
            assert!(capture_typed_prototype(runner, &f.class, f.artifact, &job, f.job.context_hash(), PROMPT, limits()).is_err());
        }
        // Same number of IDs cannot silently replace the input whose Flat
        // commitment the unchanged context names.
        assert!(capture_typed_prototype(runner, &f.class, f.artifact, &f.job, f.job.context_hash(), &[1, 1], limits()).is_err());
        // Even when the commitment agrees, token_bound is an independent limit.
        let mut job = f.job.clone();
        let out_of_range = [f.program.token_bound, 1];
        job.prompt_token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            &out_of_range,
        )
        .unwrap();
        assert!(capture_typed_prototype(runner, &f.class, f.artifact, &job, job.context_hash(), &out_of_range, limits()).is_err());
        assert_eq!(f.class.program, f.encoded_program);
        assert_eq!(runner.plan.program.encode(), f.encoded_program);
    });
}

#[test]
fn canonical_zero_job_tokenizer_is_independent_of_registration_and_expected_context_stays_fixed() {
    use kaspa_consensus_core::Hash64;
    with_fixture(|f, runner, _| {
        let expected_job = f.job.clone();
        let expected_execution = expected_job.context_hash();
        let registered = f.class.clone();
        assert_eq!(expected_job.tokenizer_id, Hash64::default());
        assert_ne!(registered.tokenizer_id, expected_job.tokenizer_id);
        let ordinary = runner.run(&expected_job, PROMPT, LEAF_CAP, true, &mut |_| {}).unwrap();
        let capture =
            capture_typed_prototype(runner, &registered, f.artifact, &expected_job, expected_execution, PROMPT, limits()).unwrap();
        assert_eq!(capture.origin().execution, expected_execution);
        assert_eq!(capture.run(), &ordinary);
        assert_eq!(expected_job, f.job);
        assert_eq!(registered, f.class);

        for field in 0..6 {
            let mut delivered = expected_job.clone();
            match field {
                0 => delivered.tokenizer_id = registered.tokenizer_id,
                1 => delivered.prompt_token_ids_hash = Hash64::from_u64_word(901),
                2 => delivered.execution_seed[0] ^= 1,
                3 => delivered.shape_profile_id = Hash64::from_u64_word(902),
                4 => delivered.job_id = Hash64::from_u64_word(903),
                _ => delivered.max_context_tokens += 1,
            }
            assert_eq!(
                quote_typed_prototype(runner, &registered, f.artifact, &delivered, expected_execution, limits()).err(),
                Some(TypedCaptureError::Binding)
            );
            assert_eq!(
                capture_typed_prototype(runner, &registered, f.artifact, &delivered, expected_execution, PROMPT, limits()).err(),
                Some(TypedCaptureError::Binding)
            );
        }
        // Even a coherent different input/seed/tokenizer packet is not the
        // independently expected native job. Do not derive its own expected pin.
        let mut delivered = expected_job.clone();
        delivered.tokenizer_id = registered.tokenizer_id;
        delivered.execution_seed[0] ^= 1;
        let foreign_prompt = [1, 1];
        delivered.prompt_token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            &foreign_prompt,
        )
        .unwrap();
        assert!(
            capture_typed_prototype(runner, &registered, f.artifact, &delivered, expected_execution, &foreign_prompt, limits())
                .is_err()
        );
        let mut false_registration = registered.clone();
        false_registration.tokenizer_id = expected_job.tokenizer_id;
        assert!(quote_typed_prototype(runner, &false_registration, f.artifact, &expected_job, expected_execution, limits()).is_err());
        capture.validate_local_copy(capture.origin(), capture.entries(), EXPECTED_GENERATED).unwrap();
        assert_eq!(f.job.context_hash(), expected_execution);
        assert_eq!(f.class, registered);
    });
}

#[test]
fn fallible_node_observer_stops_before_any_state_or_output_callback() {
    struct Stop {
        nodes: u64,
        states: u64,
        outputs: u64,
    }
    impl TirRunObserverV1 for Stop {
        fn node(&mut self, _: u32, _: &NodeValue<'_>) -> Result<(), String> {
            self.nodes += 1;
            Err("typed-observer-stop".into())
        }
        fn state(
            &mut self,
            _: u32,
            _: u16,
            _: Option<u16>,
            _: Option<u32>,
            _: DType,
            _: &[usize],
            _: Slice<'_>,
        ) -> Result<(), String> {
            self.states += 1;
            Ok(())
        }
        fn last_output(&mut self, _: u32, _: DType, _: &[usize], _: Slice<'_>, _: u32) -> Result<(), String> {
            self.outputs += 1;
            Ok(())
        }
    }
    with_fixture(|f, runner, _| {
        let mut stop = Stop { nodes: 0, states: 0, outputs: 0 };
        let mut leaves = 0;
        let result = runner.run_observed(
            &f.job,
            PROMPT,
            LEAF_CAP,
            true,
            &mut |_| {
                leaves += 1;
            },
            &mut stop,
        );
        assert!(result.unwrap_err().contains("typed-observer-stop"));
        assert_eq!((stop.nodes, stop.states, stop.outputs, leaves), (1, 0, 0, 0));
    });
}
