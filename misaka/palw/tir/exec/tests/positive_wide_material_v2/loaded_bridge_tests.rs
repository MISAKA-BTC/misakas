//! UNRUN cold-File source regressions for the combined908 Loaded + Root R5 tree.
//! Uses the existing independently declared wide fixture and original valid
//! prompt/context BEFORE writing a candidate file. No context/hash repair.
use super::*;
use kaspa_consensus_core::{
    palw_native_resource_v1::{
        SourceProgramLocalLimitsV1, require_source_program_local_profile_v1,
    },
    palw_positive_check_v2::full_relation::wide_v2::stream::{
        StreamRecordLimitsV2, StreamSubmitV2,
    },
    palw_prompt_ids_v1::prompt_token_ids_match_v1,
    palw_step_leg::{step_merkle_root_capped_v1, step_tile_leaf_hash_v1},
    palw_tir_step_v1::{
        PalwTirLeafKindV1, PalwTirStepBindingV1, palw_tir_execution_root_v1,
        palw_tir_leaf_preimage_v2, verify_tir_binding_v2,
    },
};
use misaka_palw_tir_artifact::bounded_v1::PalwTirHeaderLimitsV1;
use misaka_palw_tir_exec::{
    node::owned_artifact_params_v2::{
        OwnedParamLimitsV2,
        loaded::{
            LoadedWideArtifactV2, LoadedWideInputsV2, LoadedWideLimitsV2,
            LoadedWideStartupBudgetV2, load_wide_artifact_v2,
        },
    },
    plan::TirPlanCompileLimitsV2,
};
use std::{
    fs::File,
    io::Write,
    sync::atomic::{AtomicUsize, Ordering},
};
const EXTERNAL: u64 = 1 << 28;
const ALLOWANCE: u64 = 1 << 29;
fn startup_limits() -> LoadedWideLimitsV2 {
    LoadedWideLimitsV2 {
        header: PalwTirHeaderLimitsV1 {
            max_file_bytes: 1 << 20,
            max_header_bytes: 1 << 16,
            max_program_bytes: 1 << 18,
            max_layout_bytes: 1 << 16,
            max_tensors: 64,
            max_meta_bytes: 1024,
            max_native_bytes: 1 << 16,
            max_copy_bytes: 1 << 16,
            max_work: 1 << 32,
            max_operation_bytes: 1 << 34,
            external_retained_bytes: 0,
        },
        source: SourceProgramLocalLimitsV1 {
            max_program_bytes: 1 << 18,
            max_scan_steps: 1 << 32,
            max_nf_items: 1 << 20,
            max_native_requests: 1 << 26,
        },
        compile: TirPlanCompileLimitsV2 {
            max_quote_visits: 1 << 24,
            max_retained_bytes: 1 << 28,
            max_scratch_bytes: 1 << 30,
            max_operation_bytes: 1 << 34,
            external_retained_bytes: 0,
        },
        params: OwnedParamLimitsV2 {
            max_tensor_bytes: 1 << 20,
            max_payload_bytes: 1 << 20,
            max_chunk_bytes: 32768,
            max_metadata_bytes: 1 << 20,
            max_native_bytes: 1 << 20,
            max_inventory_leaves: 1 << 16,
            max_work_per_attempt: 1 << 40,
            max_attempts: 1,
            max_total_read_bytes: 1 << 20,
            max_total_inventory_leaves: 1 << 16,
            max_total_work: 1 << 40,
            max_operation_bytes: 1 << 34,
            external_retained_bytes: 0,
        },
        max_new_owner_bytes: ALLOWANCE,
        max_operation_bytes: 1 << 34,
        max_attempts: 1,
        max_total_read_bytes: 1 << 24,
        max_total_work: 1 << 44,
        max_total_source_steps: 1 << 36,
        max_retained_claims: ALLOWANCE,
    }
}
#[derive(Default)]
struct Counters {
    loaded: AtomicUsize,
    capture: AtomicUsize,
    run: AtomicUsize,
    capture_calls: AtomicUsize,
    run_calls: AtomicUsize,
}
struct Guard(Arc<Counters>, u8);
impl Drop for Guard {
    fn drop(&mut self) {
        match self.1 {
            0 => &self.0.loaded,
            1 => &self.0.capture,
            _ => &self.0.run,
        }
        .fetch_sub(1, Ordering::SeqCst);
    }
}
fn ledger(c: Arc<Counters>) -> TirMemoryReserverV1 {
    Arc::new(move |role, _, _, bytes| {
        assert!(bytes > 0); // Logical requests, not physical RSS or allocator measurement.
        let kind = match role {
            "tir-loaded-wide-artifact-v2" => {
                c.loaded.fetch_add(1, Ordering::SeqCst);
                0
            }
            "positive-wide-panel-capture-v2" => {
                c.capture.fetch_add(1, Ordering::SeqCst);
                c.capture_calls.fetch_add(1, Ordering::SeqCst);
                1
            }
            "tir-wide-q24-v2-required" => {
                c.run.fetch_add(1, Ordering::SeqCst);
                c.run_calls.fetch_add(1, Ordering::SeqCst);
                2
            }
            _ => return Ok(Box::new(()) as Box<dyn Send + Sync>),
        };
        Ok(Box::new(Guard(c.clone(), kind)) as Box<dyn Send + Sync>)
    })
}
fn source_profile() -> bool {
    if require_source_program_local_profile_v1().is_err() {
        eprintln!("LOADED_PANEL_UNSUPPORTED_SOURCE_PROFILE: cold success path not executed");
        false
    } else {
        true
    }
}
fn existing_binding(f: &Fixture<'_>) -> PalwTirStepBindingV1 {
    // Hash already-held original fixture observations, without another model
    // execution. No placeholder, zero root or suppressed old binding field.
    let space = PalwTirStepSpaceV2::new(&f.class).unwrap();
    let count = space.leaf_count_capped(&f.job, LEAF_CAP).unwrap();
    let mut hashes = Vec::with_capacity(count as usize);
    for index in 0..count {
        let leaf = space.leaf_at(&f.job, index).unwrap();
        let values = match leaf.kind {
            PalwTirLeafKindV1::Commit {
                occurrence,
                node,
                first_element,
                ..
            } => {
                let value = &f
                    .nodes
                    .iter()
                    .find(|n| {
                        n.position == leaf.position && n.occurrence == occurrence && n.node == node
                    })
                    .unwrap()
                    .value;
                value.data
                    [first_element as usize..first_element as usize + leaf.value_count as usize]
                    .to_vec()
            }
            PalwTirLeafKindV1::State {
                state,
                layer,
                first_element,
                ..
            } => {
                let value = &f.steps[leaf.position as usize].state.fixed[&(state, layer)];
                value.data
                    [first_element as usize..first_element as usize + leaf.value_count as usize]
                    .to_vec()
            }
            PalwTirLeafKindV1::HistTile {
                first_lane,
                row_lanes,
                first_position,
                ..
            } => {
                // Literal appends remain known even for window1, whose after-step
                // history state retains zero prior rows.
                (first_position..=leaf.position)
                    .flat_map(|position| {
                        let token = run_wide_common::EXPECTED_TOKENS[position as usize] as usize;
                        let first = token * 4 + first_lane as usize;
                        run_wide_common::EMBEDDING[first..first + row_lanes as usize]
                            .iter()
                            .copied()
                    })
                    .collect()
            }
        };
        let preimage = palw_tir_leaf_preimage_v2(&space, &f.job, &leaf, &values).unwrap();
        hashes.push(step_tile_leaf_hash_v1(
            &f.job.context_hash(),
            &f.job.shape_profile_id,
            &preimage,
        ));
    }
    let trace =
        kaspa_consensus_core::palw_logits_q24_v2::trace_root(&f.job, &f.rows, EXPECTED).unwrap();
    let step = step_merkle_root_capped_v1(&hashes, LEAF_CAP).unwrap();
    PalwTirStepBindingV1 {
        version: 1,
        class: f.class.clone(),
        job_context: f.job.clone(),
        artifact_root: f.artifact,
        full_logits_trace_root: trace,
        step_leaf_count: count,
        step_merkle_root: step,
        committed_execution_root: palw_tir_execution_root_v1(
            &f.job.context_hash(),
            &trace,
            &f.job.shape_profile_id,
            count,
            &step,
        ),
    }
}
fn with_loaded(
    f: &Fixture<'_>,
    reserve: &TirMemoryReserverV1,
    test: impl FnOnce(&LoadedWideArtifactV2<'_>, &PalwTirStepBindingV1),
) {
    // Controlled test-only copies/provider buffers are pre-owned separately.
    // The actual production bridge performs no param re-encoding or plan copy.
    let _exports = reserve(
        "test-cold-metadata-and-provider",
        f.job.shape_profile_id,
        f.job.context_hash(),
        EXTERNAL,
    )
    .unwrap();
    assert!(prompt_token_ids_match_v1(
        PalwPromptIdsFormV1::Flat,
        PROMPT,
        &f.job.prompt_token_ids_hash
    ));
    assert_eq!(f.class.tokenizer_id, Hash64::from_u64_word(18));
    assert_eq!(f.job.tokenizer_id, Hash64::from_u64_word(0));
    let original_job = f.job.clone();
    let original_class = f.class.clone();
    // Existing-claim Panel/court path: trace/step/execution commitments already
    // exist. Ordinary first-Miner/trace-free startup is a separate absent API.
    let binding = existing_binding(f);
    let verified = verify_tir_binding_v2(&binding, LEAF_CAP).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original-wide.palwtir");
    misaka_palw_tir_artifact::write_container_v1(
        &path,
        f.program,
        borsh::to_vec(&f.class.layout).unwrap(),
        f.class.tokenizer_id.as_bytes(),
        "original-root-wide-fixture".into(),
        &mut |j, l| Ok(f.params.get(j, l).unwrap().to_le_bytes()),
    )
    .unwrap();
    let mut budget = LoadedWideStartupBudgetV2::new(startup_limits()).unwrap();
    let loaded = load_wide_artifact_v2(
        File::open(&path).unwrap(),
        LoadedWideInputsV2 {
            expected: &binding,
            verified: &verified,
            original_context: &f.job,
            external_retained_bytes: EXTERNAL,
            external_ticket: &(),
        },
        &mut budget,
        Some(reserve),
    )
    .unwrap();
    assert_eq!(loaded.original_context(), &original_job);
    assert_eq!(binding.class, original_class);
    test(&loaded, &binding);
    assert_eq!(f.job, original_job);
    assert_eq!(f.class, original_class);
}
fn record_limits() -> StreamRecordLimitsV2 {
    StreamRecordLimitsV2 {
        max_record_bytes: 1 << 28,
        max_claim_values: 1 << 14,
        max_read_bytes: 1 << 30,
        max_read_calls: 1 << 24,
        max_extra_heap_bytes: 1 << 36,
        metadata_auth_bytes: 1 << 29,
        signature_workspace_bytes: 1 << 20,
        // The entire configured live producer cap + independent test export
        // owner, not an assumption that aliases/binding/signatures disappeared.
        external_retained_bytes: bounds().max_live_heap_bytes + EXTERNAL,
    }
}

#[test]
fn cold_original_file_loaded_private_source_reaches_wide_tape_and_independent_node() {
    if !source_profile() {
        return;
    }
    for window in [1, 3] {
        let counters = Arc::new(Counters::default());
        let reserve = ledger(counters.clone());
        run_wide_common::with_fixture(window, |f, _| {
            with_loaded(f, &reserve, |loaded, binding| {
                assert_eq!(counters.loaded.load(Ordering::SeqCst), 1);
                let work = loaded.wide_panel_work(bounds(), &disk_reserve()).unwrap();
                let shared = work.clone();
                let capture = loaded
                    .capture_wide_panel_one_pass(
                        PROMPT,
                        run_bounds(),
                        &work,
                        tempfile::tempfile().unwrap(),
                        &reserve,
                    )
                    .unwrap();
                assert_eq!(capture.run().generated(), EXPECTED);
                assert_eq!(capture.binding().unwrap(), *binding);
                assert_eq!(counters.capture.load(Ordering::SeqCst), 1);
                assert_eq!(counters.run.load(Ordering::SeqCst), 1);
                assert_eq!(counters.run_calls.load(Ordering::SeqCst), 1);
                let spent = shared.meter();
                assert!(
                    loaded
                        .capture_wide_panel_one_pass(
                            PROMPT,
                            run_bounds(),
                            &shared,
                            tempfile::tempfile().unwrap(),
                            &reserve
                        )
                        .is_err()
                );
                assert_eq!(shared.meter(), spent);
                assert_eq!(counters.run_calls.load(Ordering::SeqCst), 1);
                let signed = SignedMiner::new(f, false);
                let result = signed.authenticate();
                let prep = signed.preparation(f);
                // Independently authenticated existing provider; it is NOT an
                // extraction of Loaded params or a trusted producer verdict.
                let src = TirParamsSourceV1(f.params);
                let inventory = TirHeldInventoryV1::build(f.program, &src).unwrap();
                let builder = WidePanelBuilderV2::prepare(
                    &capture,
                    &prep,
                    &result,
                    &signed.rooted,
                    &inventory,
                    provider(),
                    owners(),
                    &reserve,
                )
                .unwrap();
                let mut tape = builder.claims_tape(tempfile::tempfile().unwrap()).unwrap();
                let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([97; 32]);
                let record = tape
                    .freeze_record(
                        &prep,
                        capture.binding().unwrap(),
                        capture.run().generated().to_vec(),
                        principal(panel.verification_key.as_ref()),
                        record_limits(),
                        &reserve,
                    )
                    .unwrap();
                let sig = *libcrux_ml_dsa::ml_dsa_87::sign(
                    &panel.signing_key,
                    &record.signing_digest().0,
                    record.signature_context(),
                    [0; 32],
                )
                .unwrap()
                .as_ref();
                tape.rewind();
                let node_reserver = |bytes| {
                    reserve(
                        "test-cold-node",
                        f.job.shape_profile_id,
                        f.job.context_hash(),
                        bytes,
                    )
                    .map_err(|_| FullRelationError::Budget)
                };
                let mut node = signed
                    .preparation(f)
                    .begin_stream(
                        record.binding().clone(),
                        record.generated().to_vec(),
                        record.author(),
                        record.signing_digest(),
                        tape,
                        *panel.verification_key.as_ref(),
                        sig,
                        record_limits(),
                        &node_reserver,
                    )
                    .unwrap();
                let relations = prep.plan().summary().relations;
                let mut frontier = 0;
                for i in 0..relations {
                    let relation = prep.plan().relation_at(i).unwrap();
                    assert_eq!(relation.legacy_frontier, frontier);
                    let material = builder.material(i, frontier).unwrap();
                    let step = node
                        .submit(&material.as_borrowed(), material.target())
                        .unwrap();
                    assert_eq!(node.checked_relations(), i + 1);
                    assert_eq!(
                        step,
                        if i + 1 == relations {
                            StreamSubmitV2::NeedEof
                        } else {
                            StreamSubmitV2::Continue
                        }
                    );
                    if matches!(relation.kind, ExpandedRelationKind::Legacy(_)) {
                        frontier += 1;
                    }
                }
                assert_eq!(node.finish().unwrap(), CheckOutcome::Matched);
                assert_eq!(counters.run_calls.load(Ordering::SeqCst), 1);
                drop(node);
                drop(record);
                drop(builder);
                drop(capture);
                assert_eq!(counters.capture.load(Ordering::SeqCst), 0);
                assert_eq!(counters.run.load(Ordering::SeqCst), 0);
                assert_eq!(counters.loaded.load(Ordering::SeqCst), 1);
            })
        });
        assert_eq!(counters.loaded.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn loaded_floor_refusal_and_failed_tape_preserve_shared_prefix_and_one_use() {
    if !source_profile() {
        return;
    }
    let counters = Arc::new(Counters::default());
    let reserve = ledger(counters.clone());
    run_wide_common::with_fixture(1, |f, _| {
        with_loaded(f, &reserve, |loaded, _| {
            let mut low = bounds();
            low.external_retained_bytes = loaded.quote().operation_retained_bytes - 1;
            let work = WidePanelWorkV2::new(
                low,
                f.job.shape_profile_id,
                f.job.context_hash(),
                &disk_reserve(),
            )
            .unwrap();
            let before = work.meter();
            assert!(
                loaded
                    .capture_wide_panel_one_pass(
                        PROMPT,
                        run_bounds(),
                        &work,
                        tempfile::tempfile().unwrap(),
                        &reserve
                    )
                    .is_err()
            );
            assert_eq!(work.meter(), before);
            assert_eq!(counters.capture_calls.load(Ordering::SeqCst), 0);
            assert_eq!(counters.run_calls.load(Ordering::SeqCst), 0);
            let work = loaded.wide_panel_work(bounds(), &disk_reserve()).unwrap();
            let shared = work.clone();
            let before = shared.meter();
            let mut tape = tempfile::tempfile().unwrap();
            tape.write_all(b"not an empty capture tape").unwrap();
            assert!(
                loaded
                    .capture_wide_panel_one_pass(PROMPT, run_bounds(), &work, tape, &reserve)
                    .is_err()
            );
            let spent = shared.meter();
            assert!(spent.visits > before.visits);
            assert_eq!(counters.capture_calls.load(Ordering::SeqCst), 1);
            assert_eq!(counters.run_calls.load(Ordering::SeqCst), 0);
            assert_eq!(counters.capture.load(Ordering::SeqCst), 0);
            assert_eq!(counters.loaded.load(Ordering::SeqCst), 1);
            assert!(
                loaded
                    .capture_wide_panel_one_pass(
                        PROMPT,
                        run_bounds(),
                        &shared,
                        tempfile::tempfile().unwrap(),
                        &reserve
                    )
                    .is_err()
            );
            assert_eq!(shared.meter(), spent);
            assert_eq!(counters.capture_calls.load(Ordering::SeqCst), 1);
        })
    });
    assert_eq!(counters.loaded.load(Ordering::SeqCst), 0);
}
